//! The trading rules as pure functions, so the host tests check exactly what runs on chain.

use tiber_core::{fee_amount, sniper_lp_fee, LaunchFeeRates};

use crate::constants::*;

/// The buy tax at `now` for trading that opened at `opened_at`: 80% at the opening second,
/// falling evenly to 2% over the first 30 seconds, 2% from then on.
pub fn buy_tax_bps(now: i64, opened_at: i64) -> u16 {
    sniper_lp_fee(
        now,
        opened_at,
        SNIPER_WINDOW_SECS,
        SNIPER_START_BPS,
        BUY_TAX_BPS,
    )
}

/// The buy tax on `amount_in` SOL at `bps`: rounded up, and none when it would take the whole
/// input (a dust buy).
pub fn buy_tax(amount_in: u64, bps: u16) -> u64 {
    match fee_amount(amount_in, bps) {
        Some(tax) if tax < amount_in => tax,
        _ => 0,
    }
}

/// The sell burn for tokens that arrived at `held_since` (0: unknown), sold at `now`: 5% within a
/// day, 1% from 30 days, 2% between (and when unknown).
pub fn sell_burn_bps(now: i64, held_since: i64) -> u16 {
    if held_since <= 0 || held_since > now {
        return SELL_BURN_BPS;
    }
    let age = now - held_since;
    if age < QUICK_SECS {
        QUICK_SELL_BURN_BPS
    } else if age >= LOYAL_SECS {
        LOYAL_SELL_BURN_BPS
    } else {
        SELL_BURN_BPS
    }
}

/// The rates the shared fill math expects: the buy tax plays the part of a launch's creator fee
/// (both are a cut of a buy's SOL before the curve).
pub fn rates(tax_bps: u16) -> LaunchFeeRates {
    LaunchFeeRates {
        creator_fee_bps: tax_bps,
        ..LaunchFeeRates::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Twin, TwinRule};
    use anchor_lang::prelude::Pubkey;
    use tiber_core::{curve_params, trade_burn};

    #[test]
    fn the_curve_numbers_match_the_plan() {
        let p = curve_params(CURVE_TOKENS + RESERVE_TOKENS, 7_500, VIRTUAL_QUOTE).unwrap();
        assert_eq!(p.curve_tokens, CURVE_TOKENS);
        assert_eq!(p.reserve_tokens, RESERVE_TOKENS);
        assert_eq!(p.virtual_base, VIRTUAL_BASE);
        assert_eq!(p.graduation_quote, GRADUATION_QUOTE);
        assert_eq!(CURVE_TOKENS + RESERVE_TOKENS + TREASURY_TOKENS, SUPPLY);
        assert_eq!(MAX_BUY, SUPPLY / 100);
        assert_eq!(tiber_bridge::wall::HOLD_CAP, SUPPLY * 15 / 1_000);
        assert_eq!(TREASURY_DAILY_LIMIT, SUPPLY / 1_000);
        assert_eq!(TREASURY_LOCKED_TOKENS * 5, TREASURY_TOKENS * 4);
    }

    #[test]
    fn the_buy_tax_falls_from_80_to_2_percent_over_30_seconds() {
        let t0 = 1_000_000;
        assert_eq!(buy_tax_bps(t0, t0), 8_000);
        assert_eq!(buy_tax_bps(t0 + 15, t0), 4_100);
        assert_eq!(buy_tax_bps(t0 + 29, t0), 460);
        assert_eq!(buy_tax_bps(t0 + 30, t0), 200);
        assert_eq!(buy_tax_bps(t0 + 86_400, t0), 200);
    }

    #[test]
    fn the_loyalty_burn_falls_with_age() {
        let t = 100 * 86_400;
        assert_eq!(sell_burn_bps(t, t - 60), 500);
        assert_eq!(sell_burn_bps(t, t - 86_399), 500);
        assert_eq!(sell_burn_bps(t, t - 86_400), 200);
        assert_eq!(sell_burn_bps(t, t - 30 * 86_400 + 1), 200);
        assert_eq!(sell_burn_bps(t, t - 30 * 86_400), 100);
        assert_eq!(sell_burn_bps(t, 0), 200);
        assert_eq!(sell_burn_bps(t, t + 5), 200);
    }

    #[test]
    fn taxes_are_two_percent() {
        assert_eq!(buy_tax(1_000_000_000, BUY_TAX_BPS), 20_000_000);
        assert_eq!(buy_tax(1, BUY_TAX_BPS), 0);
        assert_eq!(trade_burn(10_000 * ONE, SELL_BURN_BPS), 200 * ONE);
    }

    fn opened(at: i64) -> Twin {
        Twin {
            bump: 255,
            mint: Pubkey::new_unique(),
            pool: Pubkey::new_unique(),
            buyer: Pubkey::new_unique(),
            holding: Pubkey::new_unique(),
            lp_holding: Pubkey::new_unique(),
            sister: Pubkey::new_unique(),
            sister_fund: Pubkey::new_unique(),
            curve_open: true,
            opened_at: at,
            graduated: false,
            graduated_at: 0,
            graduation_topup: 0,
            graduation_burned: 0,
            treasury_released: 0,
            release_day: 0,
            released_today: 0,
            buy_tax_paid: 0,
            sell_burned: 0,
        }
    }

    #[test]
    fn the_treasury_releases_at_most_a_million_a_day() {
        let t0 = 1_000_000;
        let mut t = opened(t0);
        assert_eq!(t.release(TREASURY_DAILY_LIMIT, t0), Ok(()));
        assert_eq!(t.release(1, t0 + 100), Err(TwinRule::DailyLimit));
        assert_eq!(t.release(1, t0 + DAY_SECS - 1), Err(TwinRule::DailyLimit));
        assert_eq!(t.release(TREASURY_DAILY_LIMIT, t0 + DAY_SECS), Ok(()));
        assert_eq!(t.treasury_released, 2 * TREASURY_DAILY_LIMIT);
    }

    #[test]
    fn only_ten_million_before_six_months() {
        let t0 = 1_000_000;
        let mut t = opened(t0);
        // 10 days of the daily maximum.
        for day in 0..10 {
            assert_eq!(t.release(TREASURY_DAILY_LIMIT, t0 + day * DAY_SECS), Ok(()));
        }
        assert_eq!(t.treasury_released, TREASURY_OPEN_TOKENS);
        assert_eq!(t.release(1, t0 + 60 * DAY_SECS), Err(TwinRule::Locked));
        assert_eq!(
            t.release(1, t0 + TREASURY_LOCK_SECS - 1),
            Err(TwinRule::Locked)
        );
        assert_eq!(t.release(TREASURY_DAILY_LIMIT, t0 + TREASURY_LOCK_SECS), Ok(()));
        // And never past the 50 million.
        t.treasury_released = TREASURY_TOKENS;
        assert_eq!(
            t.release(1, t0 + TREASURY_LOCK_SECS + 10 * DAY_SECS),
            Err(TwinRule::Locked)
        );
    }

    #[test]
    fn nothing_before_trading_opens() {
        let mut t = opened(0);
        assert_eq!(t.release(1, 5), Err(TwinRule::NotOpen));
        let mut t = opened(100);
        assert_eq!(t.release(1, 50), Err(TwinRule::NotOpen));
        assert_eq!(t.release(0, 150), Err(TwinRule::ZeroAmount));
    }
}
