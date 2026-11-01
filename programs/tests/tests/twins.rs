//! Romulus and Remus end to end: the twins program and the wall on the token program and the
//! exchange, as deployed.

use anchor_lang::prelude::Pubkey;
use anchor_lang::{InstructionData, ToAccountMetas};
use solana_keypair::Keypair;
use solana_signer::Signer;
use tiber_core::policy;
use tiber_program_tests::fixture::World;
use tiber_program_tests::hooks::SwapSpec;
use tiber_program_tests::Tx;
use tiber_swap::client as swap;
use tiber_swap::state::Pool;
use tiber_token::client as token;
use tiber_token::state::Mint;
use tiber_twins::constants::*;
use tiber_twins::error::TwinsError;
use tiber_twins::state::Twin;
use tiber_bridge::wall::WallError;
use anchor_lang::solana_program::instruction::Instruction;

const SOL: u64 = 1_000_000_000;

struct Twins {
    w: World,
    admin: Keypair,
    rom: Pubkey,
    rem: Pubkey,
}

fn program_data(program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[program.as_ref()], &BPF_LOADER_UPGRADEABLE_ID).0
}

fn config() -> Pubkey {
    Pubkey::find_program_address(&[CONFIG_SEED], &tiber_twins::ID).0
}

fn twin(mint: &Pubkey) -> Pubkey {
    tiber_twins::twin_address(mint).0
}

fn ix(accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
    Instruction {
        program_id: tiber_twins::ID,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

fn tithe_state(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[tiber_tithe::STATE_SEED, mint.as_ref()], &tiber_tithe::ID).0
}

fn wall_registry(mint: &Pubkey) -> Pubkey {
    tiber_hook::hook_accounts_address(&tiber_bridge::ID, mint).0
}

impl Twins {
    /// The exchange with no protocol share, the twins config, and both twins created, opened,
    /// linked; trading not open yet.
    fn new() -> Self {
        let mut w = World::new();
        let deployer = w.env.deployer.insecure_clone();
        let admin = deployer.pubkey();
        // The exchange takes no share of what the twins' hooks cut.
        let set = swap::set_config(
            admin,
            tiber_swap::instructions::ConfigArgs {
                admin,
                protocol_fee_bps: policy::PROTOCOL_FEE_BPS,
                launch_protocol_share_bps: 0,
                fee_collector: admin,
                treasury: w.env.treasury.pubkey(),
                pool_creation_fee_lamports: 0,
                paused: false,
            },
        );
        w.env.send(&[set], &[&deployer]).ok();
        let init = ix(
            tiber_twins::accounts::InitConfig {
                authority: admin,
                config: config(),
                program_data: program_data(&tiber_twins::ID),
                system_program: anchor_lang::system_program::ID,
            },
            tiber_twins::instruction::InitConfig { admin },
        );
        w.env.send(&[init], &[&deployer]).ok();
        let mut t = Twins {
            w,
            admin: deployer,
            rom: Pubkey::default(),
            rem: Pubkey::default(),
        };
        t.rom = t.create("Romulus", "ROM");
        t.rem = t.create("Remus", "REM");
        t.open_curve(t.rom).ok();
        t.open_curve(t.rem).ok();
        t.link().ok();
        t
    }

    fn create(&mut self, name: &str, symbol: &str) -> Pubkey {
        let mint = Keypair::new();
        let m = mint.pubkey();
        let admin = self.admin.pubkey();
        let ix = ix(
            tiber_twins::accounts::CreateTwin {
                admin,
                config: config(),
                mint: m,
                twin: twin(&m),
                twin_holding: token::holding_address(&m, &twin(&m)),
                twins_authority: TWINS_AUTHORITY,
                wall_program: tiber_bridge::ID,
                wall: tiber_twins::wall_address(&m),
                wall_registry: wall_registry(&m),
                wall_token_signer: WALL_TOKEN_SIGNER,
                token_program: tiber_token::ID,
                token_event_authority: TOKEN_EVENT_AUTHORITY,
                system_program: anchor_lang::system_program::ID,
            },
            tiber_twins::instruction::CreateTwin {
                args: tiber_twins::CreateTwinArgs {
                    name: name.into(),
                    symbol: symbol.into(),
                    uri: String::new(),
                },
            },
        );
        let admin_kp = self.admin.insecure_clone();
        self.w.env.send(&[ix], &[&admin_kp, &mint]).ok();
        m
    }

    fn open_curve(&mut self, mint: Pubkey) -> Tx {
        let t = twin(&mint);
        let pool = tiber_twins::pool_address(&mint);
        let lp_mint = swap::lp_mint_address(&pool);
        let ix = ix(
            tiber_twins::accounts::OpenCurve {
                admin: self.admin.pubkey(),
                config: config(),
                twin: t,
                mint,
                quote_mint: BRIDGED_SOL_MINT,
                pool,
                lp_mint,
                base_vault: swap::vault_address(&pool, &mint),
                quote_vault: swap::vault_address(&pool, &BRIDGED_SOL_MINT),
                twin_holding: token::holding_address(&mint, &t),
                twin_quote: token::holding_address(&BRIDGED_SOL_MINT, &t),
                twin_lp: token::holding_address(&lp_mint, &t),
                dex_config: DEX_CONFIG,
                dex_treasury: self.w.env.treasury.pubkey(),
                twins_authority: TWINS_AUTHORITY,
                dex_hook_signer: DEX_HOOK_SIGNER,
                wall_program: tiber_bridge::ID,
                wall_token_signer: WALL_TOKEN_SIGNER,
                wall: tiber_twins::wall_address(&mint),
                token_program: tiber_token::ID,
                token_event_authority: TOKEN_EVENT_AUTHORITY,
                swap_program: tiber_swap::ID,
                dex_event_authority: DEX_EVENT_AUTHORITY,
                twins_program: tiber_twins::ID,
                system_program: anchor_lang::system_program::ID,
            },
            tiber_twins::instruction::OpenCurve {},
        );
        let admin = self.admin.insecure_clone();
        self.w.env.send(&[ix], &[&admin])
    }

    fn link(&mut self) -> Tx {
        let (a, b) = (self.rom, self.rem);
        let (ba, bb) = (tiber_twins::buyer_address(&a), tiber_twins::buyer_address(&b));
        let ix = ix(
            tiber_twins::accounts::LinkTwins {
                admin: self.admin.pubkey(),
                config: config(),
                twin_a: twin(&a),
                twin_b: twin(&b),
                quote_mint: BRIDGED_SOL_MINT,
                fund_a: token::holding_address(&BRIDGED_SOL_MINT, &ba),
                fund_b: token::holding_address(&BRIDGED_SOL_MINT, &bb),
                buyer_a: ba,
                buyer_b: bb,
                registry_a: tiber_hook::hook_accounts_address(
                    &tiber_twins::ID,
                    &tiber_twins::pool_address(&a),
                )
                .0,
                registry_b: tiber_hook::hook_accounts_address(
                    &tiber_twins::ID,
                    &tiber_twins::pool_address(&b),
                )
                .0,
                token_program: tiber_token::ID,
                token_event_authority: TOKEN_EVENT_AUTHORITY,
                system_program: anchor_lang::system_program::ID,
            },
            tiber_twins::instruction::LinkTwins {},
        );
        let admin = self.admin.insecure_clone();
        self.w.env.send(&[ix], &[&admin])
    }

    fn open_trading(&mut self) -> Tx {
        let ix = ix(
            tiber_twins::accounts::OpenTrading {
                admin: self.admin.pubkey(),
                config: config(),
                twin_a: twin(&self.rom),
                twin_b: twin(&self.rem),
            },
            tiber_twins::instruction::OpenTrading {},
        );
        let admin = self.admin.insecure_clone();
        self.w.env.send(&[ix], &[&admin])
    }

    /// Trading open, and the anti-sniper window over.
    fn trading() -> Self {
        let mut t = Self::new();
        t.open_trading().ok();
        t.w.env.warp(SNIPER_WINDOW_SECS);
        t
    }

    fn twin(&self, mint: &Pubkey) -> Twin {
        self.w.env.read(&twin(mint))
    }

    fn pool(&self, mint: &Pubkey) -> Pool {
        self.w.env.read(&tiber_twins::pool_address(mint))
    }

    /// A wallet with `sol` of bridged SOL and an empty holding of both twins.
    fn trader(&mut self, sol: u64) -> Keypair {
        let kp = self.w.wallet_with_sol(sol);
        let ixs = [
            token::create_holding(kp.pubkey(), self.rom, kp.pubkey()),
            token::create_holding(kp.pubkey(), self.rem, kp.pubkey()),
        ];
        self.w.env.send_paid_by(&ixs, &kp, &[]).ok();
        kp
    }

    fn buy(&mut self, trader: &Keypair, mint: Pubkey, lamports: u64) -> Tx {
        let spec = SwapSpec::new(trader.pubkey(), tiber_twins::pool_address(&mint), 1, lamports);
        let ix = self.w.env.swap_ix(&spec);
        self.w.env.send_paid_by(&[ix], trader, &[])
    }

    fn sell(&mut self, trader: &Keypair, mint: Pubkey, amount: u64) -> Tx {
        let spec =
            SwapSpec::new(trader.pubkey(), tiber_twins::pool_address(&mint), 0, amount).burnable();
        let ix = self.w.env.swap_ix(&spec);
        self.w.env.send_paid_by(&[ix], trader, &[])
    }

    fn balance(&self, mint: &Pubkey, owner: &Pubkey) -> u64 {
        self.w.env.holding(mint, owner)
    }

    fn fund_of(&self, mint: &Pubkey) -> u64 {
        // A twin's buy tax goes to its sister's buyer.
        let sister = self.twin(mint).sister;
        self.balance(&BRIDGED_SOL_MINT, &tiber_twins::buyer_address(&sister))
    }

    fn release(&mut self, mint: Pubkey, to: &Pubkey, amount: u64) -> Tx {
        let t = twin(&mint);
        let ix = ix(
            tiber_twins::accounts::ReleaseTreasury {
                admin: self.admin.pubkey(),
                config: config(),
                twin: t,
                mint,
                twin_holding: token::holding_address(&mint, &t),
                destination: token::holding_address(&mint, to),
                wall_program: tiber_bridge::ID,
                wall_token_signer: WALL_TOKEN_SIGNER,
                wall: tiber_twins::wall_address(&mint),
                token_program: tiber_token::ID,
                token_event_authority: TOKEN_EVENT_AUTHORITY,
            },
            tiber_twins::instruction::ReleaseTreasury { amount },
        );
        let admin = self.admin.insecure_clone();
        self.w.env.send(&[ix], &[&admin])
    }


    fn init_buyers(&mut self) {
        for mint in [self.rom, self.rem] {
            let buyer = tiber_twins::buyer_address(&mint);
            let ix = Instruction {
                program_id: tiber_tithe::ID,
                accounts: tiber_tithe::accounts::InitBuyer {
                    payer: self.w.env.payer.pubkey(),
                    twin: twin(&mint),
                    mint,
                    buyer,
                    buyer_holding: token::holding_address(&mint, &buyer),
                    state: tithe_state(&mint),
                    token_program: tiber_token::ID,
                    token_event_authority: TOKEN_EVENT_AUTHORITY,
                    system_program: anchor_lang::system_program::ID,
                }
                .to_account_metas(None),
                data: tiber_tithe::instruction::InitBuyer {}.data(),
            };
            self.w.env.send(&[ix], &[]).ok();
        }
    }

    fn tithe_ix(&self, mint: Pubkey, fund: Option<Pubkey>) -> Instruction {
        let buyer = tiber_twins::buyer_address(&mint);
        let pool = tiber_twins::pool_address(&mint);
        Instruction {
            program_id: tiber_tithe::ID,
            accounts: tiber_tithe::accounts::Tithe {
                cranker: self.w.env.payer.pubkey(),
                twin: twin(&mint),
                state: tithe_state(&mint),
                mint,
                buyer,
                fund: fund.unwrap_or(token::holding_address(&BRIDGED_SOL_MINT, &buyer)),
                buyer_holding: token::holding_address(&mint, &buyer),
                quote_mint: BRIDGED_SOL_MINT,
                pool,
                base_vault: swap::vault_address(&pool, &mint),
                quote_vault: swap::vault_address(&pool, &BRIDGED_SOL_MINT),
                sister_fund: self.twin(&mint).sister_fund,
                dex_config: DEX_CONFIG,
                twins_program: tiber_twins::ID,
                dex_hook_signer: DEX_HOOK_SIGNER,
                wall_program: tiber_bridge::ID,
                wall_token_signer: WALL_TOKEN_SIGNER,
                wall: tiber_twins::wall_address(&mint),
                token_program: tiber_token::ID,
                token_event_authority: TOKEN_EVENT_AUTHORITY,
                swap_program: tiber_swap::ID,
                dex_event_authority: DEX_EVENT_AUTHORITY,
            }
            .to_account_metas(None),
            data: tiber_tithe::instruction::Tithe {}.data(),
        }
    }

    fn tithe(&mut self, mint: Pubkey) -> Tx {
        let ix = self.tithe_ix(mint, None);
        self.w.env.send(&[ix], &[])
    }

    /// The SOL waiting to buy and burn `mint` (paid by its sister's buys).
    fn fund_for(&self, mint: &Pubkey) -> u64 {
        self.balance(&BRIDGED_SOL_MINT, &tiber_twins::buyer_address(mint))
    }

    fn graduate(&mut self, mint: Pubkey) -> Tx {
        let t = twin(&mint);
        let pool = tiber_twins::pool_address(&mint);
        let lp_mint = swap::lp_mint_address(&pool);
        let ix = ix(
            tiber_twins::accounts::Graduate {
                cranker: self.w.env.payer.pubkey(),
                twin: t,
                mint,
                pool,
                twin_holding: token::holding_address(&mint, &t),
                base_vault: swap::vault_address(&pool, &mint),
                quote_vault: swap::vault_address(&pool, &BRIDGED_SOL_MINT),
                lp_mint,
                twin_lp: token::holding_address(&lp_mint, &t),
                twins_authority: TWINS_AUTHORITY,
                wall_program: tiber_bridge::ID,
                wall_token_signer: WALL_TOKEN_SIGNER,
                wall: tiber_twins::wall_address(&mint),
                token_program: tiber_token::ID,
                token_event_authority: TOKEN_EVENT_AUTHORITY,
                swap_program: tiber_swap::ID,
                dex_event_authority: DEX_EVENT_AUTHORITY,
            },
            tiber_twins::instruction::Graduate {},
        );
        self.w.env.send(&[ix], &[])
    }
}

fn code<E: Into<u32>>(e: E) -> u32 {
    e.into()
}

#[test]
fn both_twins_are_created_with_the_planned_supply_and_curve() {
    let t = Twins::new();
    for mint in [t.rom, t.rem] {
        let m: Mint = t.w.env.read(&mint);
        assert_eq!(m.supply, SUPPLY);
        assert_eq!(m.max_supply, SUPPLY);
        assert_eq!(m.mint_authority, None, "nobody can ever mint more");
        assert_eq!(m.freeze_authority, None, "nobody can freeze a holder");
        assert_eq!(m.hook_program, Some(tiber_bridge::ID));
        let tw = t.twin(&mint);
        assert!(tw.curve_open);
        assert_eq!(tw.opened_at, 0);
        // 712.5M in the curve, 287.5M (reserve + treasury) with the twin.
        let pool = t.pool(&mint);
        assert!(pool.curve);
        assert_eq!(pool.base_reserve, CURVE_TOKENS);
        assert_eq!(pool.virtual_base, VIRTUAL_BASE);
        assert_eq!(pool.virtual_quote, VIRTUAL_QUOTE);
        assert_eq!(pool.lp_fee_bps, 0);
        assert_eq!(pool.protocol_share_bps, 0);
        assert_eq!(
            t.balance(&mint, &twin(&mint)),
            RESERVE_TOKENS + TREASURY_TOKENS
        );
    }
    assert_eq!(t.twin(&t.rom).sister, t.rem);
    assert_eq!(t.twin(&t.rem).sister, t.rom);
}

#[test]
fn nothing_trades_before_trading_opens() {
    let mut t = Twins::new();
    let trader = t.trader(10 * SOL);
    t.buy(&trader, t.rom, SOL)
        .expect_code(code(TwinsError::NotOpen));
}

#[test]
fn a_buy_pays_two_percent_to_the_sisters_buyer() {
    let mut t = Twins::trading();
    let trader = t.trader(10 * SOL);
    let spend = SOL / 10;
    t.buy(&trader, t.rom, spend).ok();
    assert_eq!(t.fund_of(&t.rom), spend * 2 / 100);
    assert!(t.balance(&t.rom, &trader.pubkey()) > 0);
    assert_eq!(t.twin(&t.rom).buy_tax_paid, spend * 2 / 100);
    // The curve got the other 98%.
    assert_eq!(t.pool(&t.rom).quote_reserve, spend * 98 / 100);
    // And REM's buy tax goes the other way.
    t.buy(&trader, t.rem, spend).ok();
    assert_eq!(t.fund_of(&t.rem), spend * 2 / 100);
}

#[test]
fn the_first_seconds_pay_the_anti_sniper_fee() {
    let mut t = Twins::new();
    t.open_trading().ok();
    let sniper = t.trader(10 * SOL);
    t.buy(&sniper, t.rom, SOL / 10).ok();
    // 80% at the opening second.
    assert_eq!(t.fund_of(&t.rom), SOL / 10 * 80 / 100);
    t.w.env.warp(15);
    let other = t.trader(10 * SOL);
    let before = t.fund_of(&t.rem);
    t.buy(&other, t.rem, SOL / 10).ok();
    assert_eq!(t.fund_of(&t.rem) - before, SOL / 10 * 41 / 100);
}

#[test]
fn one_swap_may_buy_at_most_one_percent() {
    let mut t = Twins::trading();
    let whale = t.trader(100 * SOL);
    // About 0.17 SOL buys 1% at the start of the curve: 1 SOL buys far more.
    t.buy(&whale, t.rom, SOL)
        .expect_code(code(TwinsError::BuyTooLarge));
    t.buy(&whale, t.rom, SOL / 10).ok();
    assert!(t.balance(&t.rom, &whale.pubkey()) <= MAX_BUY);
}

#[test]
fn no_wallet_may_hold_more_than_one_and_a_half_percent() {
    let mut t = Twins::trading();
    let whale = t.trader(100 * SOL);
    let mut ok = 0;
    loop {
        let tx = t.buy(&whale, t.rom, SOL / 20);
        if tx.result.is_err() {
            tx.expect_code(code(WallError::OverTheWall));
            break;
        }
        ok += 1;
        assert!(ok < 100);
    }
    assert!(t.balance(&t.rom, &whale.pubkey()) <= tiber_bridge::wall::HOLD_CAP);
    assert!(t.balance(&t.rom, &whale.pubkey()) > tiber_bridge::wall::HOLD_CAP * 9 / 10);
    // Nor by a transfer from another wallet.
    let friend = t.trader(10 * SOL);
    t.buy(&friend, t.rom, SOL / 20).ok();
    let held = t.balance(&t.rom, &friend.pubkey());
    t.w.send_tokens(&friend, t.rom, &whale.pubkey(), held)
        .expect_code(code(WallError::OverTheWall));
}

/// The burn of selling `held` after holding it `age` seconds.
fn burn_after(age: i64) -> (u64, u64) {
    let mut t = Twins::trading();
    let trader = t.trader(10 * SOL);
    t.buy(&trader, t.rom, SOL / 10).ok();
    t.w.env.warp(age);
    let held = t.balance(&t.rom, &trader.pubkey());
    let supply_before = t.w.env.read::<Mint>(&t.rom).supply;
    let sol_before = t.balance(&BRIDGED_SOL_MINT, &trader.pubkey());
    t.sell(&trader, t.rom, held).ok();
    let burned = supply_before - t.w.env.read::<Mint>(&t.rom).supply;
    assert_eq!(t.twin(&t.rom).sell_burned, burned);
    assert!(t.balance(&BRIDGED_SOL_MINT, &trader.pubkey()) > sol_before);
    assert_eq!(t.balance(&t.rom, &trader.pubkey()), 0);
    (held, burned)
}

#[test]
fn a_quick_sell_burns_five_percent() {
    let (held, burned) = burn_after(60);
    assert_eq!(burned, held * 5 / 100);
}

#[test]
fn a_normal_sell_burns_two_percent() {
    let (held, burned) = burn_after(2 * 86_400);
    assert_eq!(burned, held * 2 / 100);
}

#[test]
fn a_loyal_sell_burns_one_percent() {
    let (held, burned) = burn_after(30 * 86_400);
    assert_eq!(burned, held / 100);
}

#[test]
fn tokens_moved_to_another_wallet_start_their_clock_again() {
    let mut t = Twins::trading();
    let old = t.trader(10 * SOL);
    t.buy(&old, t.rom, SOL / 10).ok();
    t.w.env.warp(40 * 86_400);
    // Loyal in the first wallet; sent to a fresh wallet, the tokens are new there.
    let fresh = t.trader(SOL);
    let held = t.balance(&t.rom, &old.pubkey());
    t.w.send_tokens(&old, t.rom, &fresh.pubkey(), held).ok();
    let since = tiber_bridge::wall::read_held_since(&t.w.env.hook_data(&t.rom, &fresh.pubkey()));
    assert_eq!(since, t.w.env.now);
    let supply = t.w.env.read::<Mint>(&t.rom).supply;
    t.sell(&fresh, t.rom, held).ok();
    assert_eq!(supply - t.w.env.read::<Mint>(&t.rom).supply, held * 5 / 100);
}

#[test]
fn topping_up_moves_the_clock_in_proportion() {
    let mut t = Twins::trading();
    let w = t.trader(10 * SOL);
    t.buy(&w, t.rom, SOL / 20).ok();
    let first = tiber_bridge::wall::read_held_since(&t.w.env.hook_data(&t.rom, &w.pubkey()));
    assert_eq!(first, t.w.env.now);
    let a = t.balance(&t.rom, &w.pubkey());
    t.w.env.warp(40 * 86_400);
    t.buy(&w, t.rom, SOL / 20).ok();
    let b = t.balance(&t.rom, &w.pubkey()) - a;
    let since = tiber_bridge::wall::read_held_since(&t.w.env.hook_data(&t.rom, &w.pubkey()));
    let expect = ((a as i128 * first as i128 + b as i128 * t.w.env.now as i128) / (a + b) as i128) as i64;
    assert_eq!(since, expect);
    // About half old, half new: 20-odd days, so a normal 2% sell.
    let supply = t.w.env.read::<Mint>(&t.rom).supply;
    let all = a + b;
    t.sell(&w, t.rom, all).ok();
    assert_eq!(supply - t.w.env.read::<Mint>(&t.rom).supply, all * 2 / 100);
}

#[test]
fn only_wallets_may_hold_a_twin() {
    let mut t = Twins::trading();
    let trader = t.trader(10 * SOL);
    t.buy(&trader, t.rom, SOL / 20).ok();
    // A program-owned address (here the twins config) cannot receive one.
    let pda = config();
    t.w.env
        .send_paid_by(
            &[token::create_holding(trader.pubkey(), t.rom, pda)],
            &trader,
            &[],
        )
        .ok();
    t.w.send_tokens(&trader, t.rom, &pda, 1_000)
        .expect_code(code(WallError::WalletsOnly));
}

#[test]
fn the_curve_sells_out_and_graduates_with_locked_liquidity() {
    let mut t = Twins::trading();
    let mint = t.rom;
    t.graduate(mint).expect_code(code(TwinsError::NotReady));
    // Many wallets buy, each staying under the wall, until the curve is sold out.
    let mut wallets = 0;
    while t.pool(&mint).base_reserve > 0 {
        let buyer = t.trader(20 * SOL);
        // About 8,000,000 tokens' worth at the current price, tax included.
        let p = t.pool(&mint);
        let want = 8_000_000 * ONE as u128;
        let x = (p.quote_reserve + p.virtual_quote) as u128;
        let y = (p.base_reserve + p.virtual_base) as u128;
        let cost = if want >= y { x } else { x * want / (y - want) };
        let tx = t.buy(&buyer, mint, (cost * 100 / 98) as u64 + 1);
        tx.ok();
        wallets += 1;
        assert!(wallets < 200, "the curve should sell out");
    }
    let pool = t.pool(&mint);
    assert!(pool.quote_reserve >= GRADUATION_QUOTE * 99 / 100);
    t.graduate(mint).ok();
    let pool = t.pool(&mint);
    assert!(!pool.curve);
    assert_eq!(pool.virtual_base, 0);
    let tw = t.twin(&mint);
    assert!(tw.graduated);
    // The LP is all with the twin, and nothing can move it.
    let lp = t.balance(&pool.lp_mint, &twin(&mint));
    assert!(lp > 0);
    assert_eq!(tw.graduation_topup + tw.graduation_burned, RESERVE_TOKENS);
    // What stays with the twin is exactly the treasury.
    assert_eq!(t.balance(&mint, &twin(&mint)), TREASURY_TOKENS);
    // Trading goes on in the pool, still taxed.
    let late = t.trader(10 * SOL);
    let before = t.fund_of(&mint);
    t.buy(&late, mint, SOL / 10).ok();
    assert_eq!(t.fund_of(&mint) - before, SOL / 10 * 2 / 100);
    t.graduate(mint).expect_fail();
    // The tithe buys and burns on the graduated pool too (paid by REM buys).
    t.init_buyers();
    t.buy(&late, t.rem, SOL / 10).ok();
    let supply = t.w.env.read::<Mint>(&mint).supply;
    t.tithe(mint).ok();
    assert!(t.w.env.read::<Mint>(&mint).supply < supply);
    assert_eq!(t.fund_for(&mint), 0);
}

#[test]
fn the_treasury_follows_its_schedule() {
    let mut t = Twins::new();
    let me = t.trader(SOL);
    t.release(t.rom, &me.pubkey(), ONE)
        .expect_code(code(TwinsError::NotOpen));
    t.open_trading().ok();
    t.release(t.rom, &me.pubkey(), TREASURY_DAILY_LIMIT).ok();
    assert_eq!(t.balance(&t.rom, &me.pubkey()), TREASURY_DAILY_LIMIT);
    t.release(t.rom, &me.pubkey(), 1)
        .expect_code(code(TwinsError::DailyLimit));
    t.w.env.warp(DAY_SECS);
    t.release(t.rom, &me.pubkey(), TREASURY_DAILY_LIMIT).ok();
    // A stranger cannot release anything.
    let stranger = Keypair::new();
    t.w.env.fund(stranger.pubkey(), SOL);
    let tx_ix = {
        let mint = t.rom;
        let tw = twin(&mint);
        ix(
            tiber_twins::accounts::ReleaseTreasury {
                admin: stranger.pubkey(),
                config: config(),
                twin: tw,
                mint,
                twin_holding: token::holding_address(&mint, &tw),
                destination: token::holding_address(&mint, &stranger.pubkey()),
                wall_program: tiber_bridge::ID,
                wall_token_signer: WALL_TOKEN_SIGNER,
                wall: tiber_twins::wall_address(&mint),
                token_program: tiber_token::ID,
                token_event_authority: TOKEN_EVENT_AUTHORITY,
            },
            tiber_twins::instruction::ReleaseTreasury { amount: 1 },
        )
    };
    t.w.env
        .send(&[tx_ix], &[&stranger])
        .expect_code(code(TwinsError::NotAdmin));
}

#[test]
fn nobody_else_can_open_a_pool_with_the_twins_as_hook() {
    let mut t = Twins::trading();
    let lp = t.w.env.funded(50 * SOL);
    let a = t.w.mint_to_owner(&lp, 6, 1_000_000 * ONE, "AAA");
    let keys = swap::CreatePoolKeys {
        payer: lp.pubkey(),
        authority: lp.pubkey(),
        treasury: t.w.env.treasury.pubkey(),
        base_mint: a,
        quote_mint: t.w.sol,
        hook_caller: None,
    };
    t.w.wrap_sol(&lp, SOL).ok();
    let args = tiber_swap::instructions::CreatePoolArgs {
        lp_fee_bps: 0,
        hook_program: tiber_twins::ID,
        hook_flags: POOL_FLAGS,
        virtual_base: 0,
        virtual_quote: 0,
        base_amount: 1_000 * ONE,
        quote_amount: SOL / 10,
        base_hook_accounts: 0,
        quote_hook_accounts: 0,
        hook_data: vec![],
    };
    t.w.env
        .send_paid_by(&[swap::create_pool(&keys, args, vec![])], &lp, &[])
        .expect_code(code(TwinsError::OnlyTwinsCreatePools));
}

#[test]
fn a_trade_fits_well_inside_the_compute_budget() {
    let mut t = Twins::trading();
    let trader = t.trader(10 * SOL);
    let buy = t.buy(&trader, t.rom, SOL / 10).cu();
    let held = t.balance(&t.rom, &trader.pubkey());
    let sell = t.sell(&trader, t.rom, held / 2).cu();
    println!("compute: buy {buy}, sell {sell}");
    // Solana allows 1,400,000 per transaction; the default without asking is 200,000 per
    // instruction. A wallet's swap should fit the default comfortably.
    assert!(buy < 200_000 && sell < 200_000, "buy {buy}, sell {sell}");
}

// ================================================================================== the tithe

#[test]
fn the_tithe_buys_the_sister_and_burns_it_paying_no_tax() {
    let mut t = Twins::trading();
    t.init_buyers();
    let trader = t.trader(10 * SOL);
    // ROM buys pay into the fund that buys REM.
    t.buy(&trader, t.rom, SOL / 10).ok();
    let fund = t.fund_for(&t.rem);
    assert_eq!(fund, SOL / 10 * 2 / 100);
    let rem_supply = t.w.env.read::<Mint>(&t.rem).supply;
    let rom_fund = t.fund_for(&t.rom);
    t.tithe(t.rem).ok();
    // The fund was spent whole (it is under the cap), and every REM bought burned.
    assert_eq!(t.fund_for(&t.rem), 0);
    let state: tiber_tithe::TitheState = t.w.env.read(&tithe_state(&t.rem));
    assert_eq!(state.spent, fund);
    assert!(state.burned > 0);
    assert_eq!(t.w.env.read::<Mint>(&t.rem).supply, rem_supply - state.burned);
    assert_eq!(
        t.balance(&t.rem, &tiber_twins::buyer_address(&t.rem)),
        0,
        "nothing bought is kept"
    );
    // The tithe's own buy paid no tax into ROM's fund.
    assert_eq!(t.fund_for(&t.rom), rom_fund);
    assert_eq!(t.twin(&t.rem).buy_tax_paid, 0);
}

#[test]
fn the_tithe_runs_once_per_slot_and_spends_at_most_half_a_percent() {
    let mut t = Twins::new();
    t.open_trading().ok();
    t.init_buyers();
    // A sniper's buy at the opening second fills REM's fund with 80% of its SOL.
    let sniper = t.trader(10 * SOL);
    t.buy(&sniper, t.rom, SOL / 10).ok();
    let fund = t.fund_for(&t.rem);
    assert_eq!(fund, SOL / 10 * 80 / 100);
    t.tithe(t.rem).ok();
    let side = VIRTUAL_QUOTE; // REM's curve had no trades yet.
    let spent = fund - t.fund_for(&t.rem);
    assert_eq!(spent, side / 200);
    // Again in the same slot: refused, so one transaction cannot chain tithes.
    let again = t.tithe_ix(t.rem, None);
    t.w.env
        .send(&[again], &[])
        .expect_code(code(tiber_tithe::TitheError::OncePerSlot));
    t.w.env.warp(1);
    t.tithe(t.rem).ok();
    assert!(t.fund_for(&t.rem) < fund - spent);
}

#[test]
fn the_tithe_cannot_be_pointed_at_another_fund() {
    let mut t = Twins::trading();
    t.init_buyers();
    let trader = t.trader(10 * SOL);
    t.buy(&trader, t.rom, SOL / 10).ok();
    // Spending someone's own SOL holding as the "fund" is refused.
    let ix = t.tithe_ix(t.rem, Some(token::holding_address(&BRIDGED_SOL_MINT, &trader.pubkey())));
    t.w.env
        .send(&[ix], &[])
        .expect_code(code(tiber_tithe::TitheError::WrongAccount));
    // With nothing in the fund there is nothing to do.
    t.tithe(t.rem).ok();
    t.w.env.warp(1);
    t.tithe(t.rem)
        .expect_code(code(tiber_tithe::TitheError::NothingToSpend));
}
