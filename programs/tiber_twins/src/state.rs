//! Accounts of the twins program.

use anchor_lang::prelude::*;

use crate::constants::*;

/// `["config"]`: who may create, link and open the twins and release their treasuries. Created
/// once, by this program's upgrade authority.
#[account]
#[derive(InitSpace)]
pub struct Config {
    pub bump: u8,
    pub admin: Pubkey,
    /// Twins created (at most two: Romulus and Remus).
    pub twins: u8,
}

/// `["twin", mint]`: one twin.
#[account]
#[derive(InitSpace)]
pub struct Twin {
    pub bump: u8,
    pub mint: Pubkey,
    /// The curve pool on the exchange, the only place the twin trades.
    pub pool: Pubkey,
    /// This twin's buyer at the tithe (`["buyer", mint]` under the tithe), which buys this twin
    /// with the sister's buy tax. Its trades on the pool pay no tax and have no buy limit.
    pub buyer: Pubkey,
    /// The twin's holding of its own token: the treasury and, until graduation, the reserve.
    pub holding: Pubkey,
    /// The twin's holding of the pool's LP: locked for good (no instruction moves it).
    pub lp_holding: Pubkey,
    /// The sister twin's mint (default until linked).
    pub sister: Pubkey,
    /// The sister's buyer's bridged-SOL holding, where this twin's buy tax goes (default until
    /// linked).
    pub sister_fund: Pubkey,
    /// Whether the curve pool exists.
    pub curve_open: bool,
    /// When trading opened (0 until it does). Both twins open in the same instruction.
    pub opened_at: i64,
    /// Whether the curve graduated.
    pub graduated: bool,
    pub graduated_at: i64,
    /// Tokens the graduation top-up moved into the pool, and tokens it burned.
    pub graduation_topup: u64,
    pub graduation_burned: u64,
    /// Treasury tokens released so far.
    pub treasury_released: u64,
    /// The day (since trading opened) of the last release, and what was released on it.
    pub release_day: i64,
    pub released_today: u64,
    /// Running totals, for display: SOL paid to the sister's buyer, tokens burned on sells.
    pub buy_tax_paid: u64,
    pub sell_burned: u64,
}

impl Twin {
    /// Treasury tokens unlocked at `now`: 50,000,000 from the moment trading opens, all
    /// 200,000,000 from six months after; nothing before trading opens.
    pub fn treasury_unlocked(&self, now: i64) -> u64 {
        if self.opened_at == 0 || now < self.opened_at {
            0
        } else if now >= self.opened_at.saturating_add(TREASURY_LOCK_SECS) {
            TREASURY_TOKENS
        } else {
            TREASURY_OPEN_TOKENS
        }
    }

    /// Checks a release of `amount` at `now` against the lock and the daily limit and records it.
    pub fn release(&mut self, amount: u64, now: i64) -> core::result::Result<(), TwinRule> {
        if amount == 0 {
            return Err(TwinRule::ZeroAmount);
        }
        if self.opened_at == 0 || now < self.opened_at {
            return Err(TwinRule::NotOpen);
        }
        let released = self
            .treasury_released
            .checked_add(amount)
            .ok_or(TwinRule::Locked)?;
        if released > self.treasury_unlocked(now) {
            return Err(TwinRule::Locked);
        }
        let day = (now - self.opened_at) / DAY_SECS;
        let today = if day == self.release_day {
            self.released_today
        } else {
            0
        };
        let today = today.checked_add(amount).ok_or(TwinRule::DailyLimit)?;
        if today > TREASURY_DAILY_LIMIT {
            return Err(TwinRule::DailyLimit);
        }
        self.treasury_released = released;
        self.release_day = day;
        self.released_today = today;
        Ok(())
    }
}

/// Why a treasury release is refused (host-testable; mapped to [`crate::error::TwinsError`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TwinRule {
    ZeroAmount,
    NotOpen,
    Locked,
    DailyLimit,
}
