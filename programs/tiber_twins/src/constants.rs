//! Constants of the twins program: the addresses it compares against (constants rather than
//! derived on chain; the unit tests check each against its derivation) and every number of the
//! Romulus and Remus tokenomics, fixed in the code so no one can change them.

use anchor_lang::prelude::Pubkey;
use tiber_hook::pool_flags;

// ------------------------------------------------------------------------------------ seeds

/// `["config"]`: the admin.
pub const CONFIG_SEED: &[u8] = b"config";
/// `["twin", mint]`: one twin. It owns the twin's treasury, graduation reserve and locked LP.
pub const TWIN_SEED: &[u8] = b"twin";
/// The tithe's `["buyer", mint]`: the tithe account that buys `mint` with its sister's buy tax and
/// burns what it buys. The sister's buy tax is paid into its bridged-SOL holding.
pub const BUYER_SEED: &[u8] = b"buyer";

// ------------------------------------------------------------------------------------ programs

/// The tithe program (`tiber_tithe`).
pub const TITHE_ID: Pubkey = Pubkey::from_str_const("5WNWnM3JTDgDYkA4Mh7quKQXtkdiwF3d7bgc9RuX4KF");
/// The bridge, which carries the Wall: the twins' token hook.
pub const WALL_ID: Pubkey = tiber_bridge::ID;
/// The token program's signer of the wall's callbacks.
pub const WALL_TOKEN_SIGNER: Pubkey = tiber_bridge::wall::TOKEN_HOOK_SIGNER;
/// This program's own authority, `["hook-authority"]`: it signs `create_pool` and `finalize_curve`
/// on the exchange (proving the twins program opens its own curves) and the wall's `prepare`.
pub const TWINS_AUTHORITY: Pubkey = tiber_bridge::wall::TWINS_AUTHORITY;
/// Its bump.
pub const TWINS_AUTHORITY_BUMP: u8 = 255;
/// The exchange's signer of every pool callback to this program: `["hook-authority", tiber_twins]`
/// under the exchange.
pub const DEX_HOOK_SIGNER: Pubkey =
    Pubkey::from_str_const("EMJChUF5dQMsaPgPjBdZm4Nquy4cWy8i6zkBrFAdrtGu");
/// The exchange's config.
pub const DEX_CONFIG: Pubkey = Pubkey::from_str_const("2zKWppHPuF8ZtWSe7g9R1AqqNdajz2WnnZiV4SEYEuUW");
/// The exchange's event authority.
pub const DEX_EVENT_AUTHORITY: Pubkey =
    Pubkey::from_str_const("3BZ2RtLoTACKnDTM6UcpufRuv26zjtAZ8t911z5gEboV");
/// The token program's event authority.
pub const TOKEN_EVENT_AUTHORITY: Pubkey =
    Pubkey::from_str_const("6wvuqfAqqPsvPm8pSm9SfXe5UrpDGZDg1UkY78nc82aM");
/// Bridged SOL, the quote of both curves.
pub const BRIDGED_SOL_MINT: Pubkey = tiber_swap::constants::BRIDGED_SOL_MINT;
/// The upgradeable loader.
pub const BPF_LOADER_UPGRADEABLE_ID: Pubkey =
    Pubkey::from_str_const("BPFLoaderUpgradeab1e11111111111111111111111");

// ------------------------------------------------------------------------------------ the tokens

/// Decimals of each twin.
pub const DECIMALS: u8 = tiber_bridge::wall::DECIMALS;
/// One whole token in base units.
pub const ONE: u64 = 1_000_000;
/// Each twin's supply, all minted at creation: 1,000,000,000.
pub const SUPPLY: u64 = 1_000_000_000 * ONE;
/// Sold on the bonding curve: 712,500,000 (71.25%).
pub const CURVE_TOKENS: u64 = 712_500_000 * ONE;
/// Kept back for the pool at graduation: 237,500,000 (23.75%). What the top-up does not use is
/// burned.
pub const RESERVE_TOKENS: u64 = 237_500_000 * ONE;
/// The treasury: 50,000,000 (5%).
pub const TREASURY_TOKENS: u64 = 50_000_000 * ONE;
/// Of the treasury, usable from the moment trading opens: 10,000,000 (1% of the supply).
pub const TREASURY_OPEN_TOKENS: u64 = 10_000_000 * ONE;
/// Of the treasury, locked until [`TREASURY_LOCK_SECS`] after trading opens: 40,000,000 (80% of it, 4% of the supply).
pub const TREASURY_LOCKED_TOKENS: u64 = TREASURY_TOKENS - TREASURY_OPEN_TOKENS;
/// Six months: 182.5 days.
pub const TREASURY_LOCK_SECS: i64 = 15_768_000;
/// The most the treasury may release in one day, ever: 1,000,000 (0.1% of the supply).
pub const TREASURY_DAILY_LIMIT: u64 = 1_000_000 * ONE;
/// A day.
pub const DAY_SECS: i64 = 86_400;

// ------------------------------------------------------------------------------------ the curve

/// The curve's virtual SOL: 15 SOL. With [`VIRTUAL_BASE`] it opens each twin at a 14.0 SOL
/// valuation and raises exactly [`GRADUATION_QUOTE`] selling [`CURVE_TOKENS`].
pub const VIRTUAL_QUOTE: u64 = 15_000_000_000;
/// The curve's virtual tokens: 356,250,000 (`C · R / (C − R)`, so the price is the same before
/// and after graduation drops the virtual reserves).
pub const VIRTUAL_BASE: u64 = 356_250_000 * ONE;
/// SOL raised at which the curve graduates: 30 SOL (`Vq · (C − R) / R`).
pub const GRADUATION_QUOTE: u64 = 30_000_000_000;
/// The pools' LP fee: none. No one takes a cut of a trade beyond the two taxes.
pub const LP_FEE_BPS: u16 = 0;

// ------------------------------------------------------------------------------------ the rules

/// The buy tax: 2% of the SOL in, paid to the sister's buyer at the tithe.
pub const BUY_TAX_BPS: u16 = 200;
/// The sell burn for tokens held a normal time: 2% of the tokens sold.
pub const SELL_BURN_BPS: u16 = 200;
/// The loyalty burn: tokens sold within a day of arriving burn 5%...
pub const QUICK_SELL_BURN_BPS: u16 = 500;
/// ...and tokens held 30 days or more burn only 1%.
pub const LOYAL_SELL_BURN_BPS: u16 = 100;
/// Under this age a sale is quick: one day.
pub const QUICK_SECS: i64 = 86_400;
/// From this age a holder is loyal: 30 days.
pub const LOYAL_SECS: i64 = 30 * 86_400;
/// How many twins the config allows: Romulus and Remus. A devnet build allows more, so a test
/// network can make fresh pairs.
#[cfg(not(feature = "devnet"))]
pub const MAX_TWINS: u8 = 2;
#[cfg(feature = "devnet")]
pub const MAX_TWINS: u8 = 16;
/// The anti-sniper window: the first 30 seconds after trading opens.
pub const SNIPER_WINDOW_SECS: i64 = 30;
/// The buy tax at the instant trading opens: 80%, falling evenly to [`BUY_TAX_BPS`] over the
/// window. Like the ordinary tax it goes to the sister's buyer.
pub const SNIPER_START_BPS: u16 = 8_000;
/// The most one swap may buy: 10,000,000 tokens (1% of the supply).
pub const MAX_BUY: u64 = 10_000_000 * ONE;

/// The pool callbacks this program answers.
pub const POOL_FLAGS: u16 = pool_flags::BEFORE_INITIALIZE
    | pool_flags::BEFORE_SWAP
    | pool_flags::AFTER_SWAP
    | pool_flags::BEFORE_SWAP_RETURNS_DELTA;
/// Index, in a pool callback's accounts, of the twin (the registry's first extra).
pub const TWIN_INDEX: u8 = 5;
/// Index of the sister buyer's bridged-SOL holding, which receives the buy tax.
pub const SISTER_FUND_INDEX: u8 = 6;
/// Index of the trader's holding of the twin (read for the loyalty burn).
pub const TRADER_HOLDING_INDEX: u8 = 7;
