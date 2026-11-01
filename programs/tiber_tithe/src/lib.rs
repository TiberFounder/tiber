//! `tiber_tithe`: the tithe of the Twins of the Tiber.
//!
//! Every buy of one twin pays 2% of its SOL (more in the first 30 seconds) into the brother fund
//! of the other: the bridged-SOL holding of this program's buyer of that twin, `["buyer", mint]`.
//! `tithe` spends the fund buying that twin on its own pool and burns every token it buys, in the
//! same instruction. Nobody can take the SOL: the buyer is a PDA of this program, and this program
//! has no instruction that moves the fund anywhere but into the twin's pool.
//!
//! Anyone may call `tithe`; it pays the caller nothing and takes nothing from them. Each call
//! spends at most 0.5% of the pool's SOL side (real plus virtual), so one tithe moves the price
//! by about 1% at most, and a mint is tithed at most once per slot, so one transaction cannot
//! chain several. Trading ahead of a tithe does not pay: a round trip costs the 2% buy tax and
//! the 2% sell burn, more than the tithe can move the price. The buyer's own trades pay no tax
//! and have no buy limit (the twins program and the wall exempt it).

#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use tiber_core::curve_out;
use tiber_swap::client::{self as swap_client, SwapKeys};
use tiber_swap::instructions::SwapArgs;
use tiber_token::client::{self as token_client, Hook};
use tiber_twins::constants::{
    BRIDGED_SOL_MINT, BUYER_SEED, DEX_CONFIG, DEX_EVENT_AUTHORITY, DEX_HOOK_SIGNER,
    TOKEN_EVENT_AUTHORITY, WALL_ID, WALL_TOKEN_SIGNER,
};
use tiber_twins::state::Twin;

declare_id!("5WNWnM3JTDgDYkA4Mh7quKQXtkdiwF3d7bgc9RuX4KF");

#[cfg(not(feature = "no-entrypoint"))]
solana_security_txt::security_txt! {
    name: "Tiber tithe",
    project_url: "https://github.com/TiberFounder/tiber",
    contacts: "link:https://github.com/TiberFounder/tiber/security/advisories/new",
    policy: "https://github.com/TiberFounder/tiber/blob/main/SECURITY.md",
    source_code: "https://github.com/TiberFounder/tiber"
}

/// `["state", mint]`: the tithe's record for one twin.
pub const STATE_SEED: &[u8] = b"state";
/// Each tithe spends at most this share of the pool's SOL side (real plus virtual): 0.5%.
pub const MAX_SPEND_BPS: u64 = 50;
const BPS: u64 = 10_000;

#[program]
pub mod tiber_tithe {
    use super::*;

    /// Sets up the tithe for a twin (anyone may, once): its record and the buyer's holding of the
    /// twin, where bought tokens land before they burn.
    pub fn init_buyer(ctx: Context<InitBuyer>) -> Result<()> {
        let mint = ctx.accounts.mint.key();
        let buyer = ctx.accounts.buyer.key();
        require_keys_eq!(
            ctx.accounts.buyer_holding.key(),
            token_client::holding_address(&mint, &buyer),
            TitheError::WrongAccount
        );
        let ix = token_client::create_holding(ctx.accounts.payer.key(), mint, buyer);
        invoke_signed(&ix, &ctx.accounts.to_account_infos(), &[])?;
        let state = &mut ctx.accounts.state;
        state.bump = ctx.bumps.state;
        state.buyer_bump = ctx.bumps.buyer;
        state.mint = mint;
        Ok(())
    }

    /// Spends part of the twin's brother fund buying the twin, and burns what it bought.
    pub fn tithe<'info>(ctx: Context<'info, Tithe<'info>>) -> Result<()> {
        process_tithe(ctx)
    }
}

/// How much of `fund` one tithe spends against `pool`: at most 0.5% of its SOL side, and on a
/// curve less than what would buy out every token left (halved until it fits).
pub fn spend_for(
    fund: u64,
    quote_reserve: u64,
    virtual_quote: u64,
    base_reserve: u64,
    virtual_base: u64,
    curve: bool,
) -> u64 {
    let side = u128::from(quote_reserve) + u128::from(virtual_quote);
    let cap = u64::try_from(side * u128::from(MAX_SPEND_BPS) / u128::from(BPS)).unwrap_or(0);
    let mut spend = fund.min(cap);
    if curve {
        while spend > 0
            && curve_out(spend, quote_reserve, virtual_quote, base_reserve, virtual_base)
                .is_none_or(|out| out >= u128::from(base_reserve))
        {
            spend /= 2;
        }
    }
    spend
}

#[account]
#[derive(InitSpace)]
pub struct TitheState {
    pub bump: u8,
    pub buyer_bump: u8,
    pub mint: Pubkey,
    /// The last slot this twin was tithed in.
    pub last_slot: u64,
    /// Running totals: SOL spent, tokens bought and burned, tithes made.
    pub spent: u64,
    pub burned: u64,
    pub tithes: u64,
}

#[event]
pub struct Tithed {
    pub mint: Pubkey,
    pub spent: u64,
    pub burned: u64,
    pub fund_left: u64,
    pub slot: u64,
}

#[derive(Accounts)]
pub struct InitBuyer<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// The twin of the mint (owned by the twins program, at its `["twin", mint]`).
    #[account(
        seeds = [tiber_twins::constants::TWIN_SEED, mint.key().as_ref()],
        bump = twin.bump,
        seeds::program = tiber_twins::ID
    )]
    pub twin: Box<Account<'info, Twin>>,
    /// CHECK: the twin's mint (bound by the twin's seeds).
    pub mint: UncheckedAccount<'info>,
    /// CHECK: the buyer PDA (seeds-checked).
    #[account(seeds = [BUYER_SEED, mint.key().as_ref()], bump)]
    pub buyer: UncheckedAccount<'info>,
    /// CHECK: the buyer's holding of the twin (address-checked; the token program creates it).
    #[account(mut)]
    pub buyer_holding: UncheckedAccount<'info>,
    #[account(init, payer = payer, space = 8 + TitheState::INIT_SPACE, seeds = [STATE_SEED, mint.key().as_ref()], bump)]
    pub state: Account<'info, TitheState>,
    /// CHECK: address-checked.
    #[account(address = tiber_token::ID @ TitheError::WrongProgram)]
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = TOKEN_EVENT_AUTHORITY @ TitheError::WrongAccount)]
    pub token_event_authority: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Tithe<'info> {
    /// Whoever triggers the tithe: pays only the transaction fee.
    pub cranker: Signer<'info>,
    #[account(
        mut,
        seeds = [tiber_twins::constants::TWIN_SEED, mint.key().as_ref()],
        bump = twin.bump,
        seeds::program = tiber_twins::ID
    )]
    pub twin: Box<Account<'info, Twin>>,
    #[account(mut, seeds = [STATE_SEED, mint.key().as_ref()], bump = state.bump)]
    pub state: Box<Account<'info, TitheState>>,
    /// CHECK: the twin's mint (bound by the twin's seeds; written by the burn).
    #[account(mut)]
    pub mint: UncheckedAccount<'info>,
    /// CHECK: the buyer PDA (seeds-checked).
    #[account(mut, seeds = [BUYER_SEED, mint.key().as_ref()], bump = state.buyer_bump)]
    pub buyer: UncheckedAccount<'info>,
    /// CHECK: the buyer's bridged-SOL holding, the fund (address-checked in the handler).
    #[account(mut)]
    pub fund: UncheckedAccount<'info>,
    /// CHECK: the buyer's holding of the twin (address-checked in the handler).
    #[account(mut)]
    pub buyer_holding: UncheckedAccount<'info>,
    /// CHECK: bridged SOL (address-checked).
    #[account(address = BRIDGED_SOL_MINT @ TitheError::WrongAccount)]
    pub quote_mint: UncheckedAccount<'info>,
    /// CHECK: the twin's pool (address-checked).
    #[account(mut, address = twin.pool @ TitheError::WrongAccount)]
    pub pool: UncheckedAccount<'info>,
    /// CHECK: the pool's vaults (the exchange checks them).
    #[account(mut)]
    pub base_vault: UncheckedAccount<'info>,
    /// CHECK: see `base_vault`.
    #[account(mut)]
    pub quote_vault: UncheckedAccount<'info>,
    /// CHECK: the twin's sister fund, which the twins program's callbacks take (address-checked).
    #[account(mut, address = twin.sister_fund @ TitheError::WrongAccount)]
    pub sister_fund: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = DEX_CONFIG @ TitheError::WrongAccount)]
    pub dex_config: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = tiber_twins::ID @ TitheError::WrongProgram)]
    pub twins_program: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = DEX_HOOK_SIGNER @ TitheError::WrongAccount)]
    pub dex_hook_signer: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = WALL_ID @ TitheError::WrongProgram)]
    pub wall_program: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = WALL_TOKEN_SIGNER @ TitheError::WrongAccount)]
    pub wall_token_signer: UncheckedAccount<'info>,
    /// CHECK: the wall's account for the mint (address-checked in the handler).
    pub wall: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = tiber_token::ID @ TitheError::WrongProgram)]
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = TOKEN_EVENT_AUTHORITY @ TitheError::WrongAccount)]
    pub token_event_authority: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = tiber_swap::ID @ TitheError::WrongProgram)]
    pub swap_program: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = DEX_EVENT_AUTHORITY @ TitheError::WrongAccount)]
    pub dex_event_authority: UncheckedAccount<'info>,
}

fn process_tithe<'info>(ctx: Context<'info, Tithe<'info>>) -> Result<()> {
    let clock = Clock::get()?;
    let a = &ctx.accounts;
    let mint = a.mint.key();
    let buyer = a.buyer.key();
    require!(clock.slot > a.state.last_slot, TitheError::OncePerSlot);
    require!(a.twin.opened_at > 0, TitheError::NotOpen);
    require_keys_eq!(
        a.fund.key(),
        token_client::holding_address(&BRIDGED_SOL_MINT, &buyer),
        TitheError::WrongAccount
    );
    require_keys_eq!(
        a.buyer_holding.key(),
        token_client::holding_address(&mint, &buyer),
        TitheError::WrongAccount
    );
    let wall = tiber_twins::wall_address(&mint);
    require_keys_eq!(a.wall.key(), wall, TitheError::WrongAccount);

    let fund = token_client::read_holding(&a.fund)?.amount;
    let pool = swap_client::read_pool(&a.pool)?;
    require!(
        !pool.curve || pool.base_reserve > 0,
        TitheError::AwaitingGraduation
    );
    let spend = spend_for(
        fund,
        pool.quote_reserve,
        pool.virtual_quote,
        pool.base_reserve,
        pool.virtual_base,
        pool.curve,
    );
    require!(spend > 0, TitheError::NothingToSpend);

    let hook = Hook {
        program: WALL_ID,
        signer: WALL_TOKEN_SIGNER,
    };
    let ix = swap_client::swap(
        &SwapKeys {
            trader: buyer,
            pool: a.twin.pool,
            base_mint: mint,
            quote_mint: BRIDGED_SOL_MINT,
            trader_base: a.buyer_holding.key(),
            trader_quote: a.fund.key(),
            hook_program: Some(tiber_twins::ID),
            base_mint_writable: false,
            quote_mint_writable: false,
        },
        SwapArgs {
            direction: 1,
            amount_in: spend,
            min_amount_out: 1,
            in_hook_accounts: 0,
            out_hook_accounts: 3,
            hook_data: vec![],
        },
        vec![
            AccountMeta::new_readonly(WALL_ID, false),
            AccountMeta::new_readonly(WALL_TOKEN_SIGNER, false),
            AccountMeta::new_readonly(wall, false),
            AccountMeta::new(a.twin.key(), false),
            AccountMeta::new(a.sister_fund.key(), false),
            AccountMeta::new_readonly(a.buyer_holding.key(), false),
        ],
    );
    let bump = [a.state.buyer_bump];
    let seeds: &[&[u8]] = &[BUYER_SEED, mint.as_ref(), &bump];
    let infos = ctx.accounts.to_account_infos();
    invoke_signed(&ix, &infos, &[seeds])?;

    // Everything the buyer holds of the twin burns.
    let bought = token_client::read_holding(&ctx.accounts.buyer_holding)?.amount;
    if bought > 0 {
        let ix = token_client::burn_with(
            buyer,
            ctx.accounts.buyer_holding.key(),
            mint,
            Some(hook),
            vec![AccountMeta::new_readonly(wall, false)],
            bought,
        );
        invoke_signed(&ix, &infos, &[seeds])?;
    }
    let fund_left = token_client::read_holding(&ctx.accounts.fund)?.amount;
    let spent = fund.saturating_sub(fund_left);
    let state = &mut ctx.accounts.state;
    state.last_slot = clock.slot;
    state.spent = state.spent.saturating_add(spent);
    state.burned = state.burned.saturating_add(bought);
    state.tithes = state.tithes.saturating_add(1);
    emit!(Tithed {
        mint,
        spent,
        burned: bought,
        fund_left,
        slot: clock.slot
    });
    Ok(())
}

#[error_code]
pub enum TitheError {
    #[msg("the account is not the one the tithe expects")]
    WrongAccount,
    #[msg("a program the tithe calls is not the expected one")]
    WrongProgram,
    #[msg("this twin was already tithed in this slot")]
    OncePerSlot,
    #[msg("trading is not open yet")]
    NotOpen,
    #[msg("the curve is sold out and waiting to graduate")]
    AwaitingGraduation,
    #[msg("the fund has nothing to spend")]
    NothingToSpend,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOL: u64 = 1_000_000_000;
    const ONE: u64 = 1_000_000;

    #[test]
    fn a_tithe_spends_at_most_half_a_percent_of_the_sol_side() {
        // A fresh curve: 15 SOL virtual, nothing real.
        let s = spend_for(10 * SOL, 0, 15 * SOL, 600_000_000 * ONE, 300_000_000 * ONE, true);
        assert_eq!(s, 15 * SOL / 200);
        // A small fund is spent whole.
        assert_eq!(
            spend_for(SOL / 100, 0, 15 * SOL, 600_000_000 * ONE, 300_000_000 * ONE, true),
            SOL / 100
        );
        // After graduation, real reserves only.
        assert_eq!(spend_for(10 * SOL, 30 * SOL, 0, 200_000_000 * ONE, 0, false), 30 * SOL / 200);
        assert_eq!(spend_for(0, 30 * SOL, 0, 1, 0, false), 0);
    }

    #[test]
    fn a_tithe_never_buys_past_the_end_of_a_curve() {
        // Nearly sold out: a few tokens left.
        let left = 1_000 * ONE;
        let s = spend_for(10 * SOL, 29 * SOL, 15 * SOL, left, 300_000_000 * ONE, true);
        let out = curve_out(s, 29 * SOL, 15 * SOL, left, 300_000_000 * ONE).unwrap();
        assert!(out < u128::from(left));
        assert!(s > 0);
    }
}
