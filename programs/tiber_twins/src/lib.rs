//! `tiber_twins`: Romulus (ROM) and Remus (REM), the Twins of the Tiber.
//!
//! The admin sets the twins up in four steps, then nothing about their rules can change:
//!
//! 1. `create_twin`, once per twin: writes the twin's wall (its token hook: the 1.5% holding cap
//!    and the wallets-only rule), creates the mint with the wall as its hook, mints all
//!    1,000,000,000 tokens to the twin's account and drops the mint authority for good.
//! 2. `open_curve`, once per twin: opens the twin's bonding curve on the Tiber exchange, with this
//!    program as the pool's hook and 600,000,000 tokens in it. The other 400,000,000 stay with
//!    the twin: 200,000,000 for the pool at graduation and 200,000,000 for the treasury.
//! 3. `link_twins`: makes each twin the other's sister, so each one's buy tax goes to the tithe
//!    account that buys the other.
//! 4. `open_trading`: opens both curves at the same moment.
//!
//! From then on every swap on a twin's pool calls this program (`before_swap`, `after_swap`):
//! a buy pays 2% of its SOL to the sister's buyer at the tithe (more in the first 30 seconds, the
//! anti-sniper fee, falling from 80%), a sell burns 2% of the tokens sold, and no swap may buy more
//! than 1% of the supply. The tithe's own buys pay nothing and have no limit. Once a curve has
//! raised 30 SOL (or sold out) anyone may `graduate` it: the reserve tops the pool up so the price
//! is unchanged, the exchange turns the curve into an ordinary pool whose LP lands in the twin's
//! account, where nothing can move it, and the rest of the reserve is burned. The admin releases
//! the treasury with `release_treasury`: 50,000,000 from the start, the other 150,000,000 six
//! months after trading opens, and never more than 1,000,000 in one day.

#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::InstructionData;
use tiber_core::{curve_out, graduation_topup, launch_fill, trade_burn, Reserves};
use tiber_hook::{
    hook_accounts_address, set_answer, write_registry, AccountSource, Delta, ExtraAccount,
    HookAccountList, HookReturn, PoolHookArgs, Seed, HOOK_AUTHORITY_SEED,
};
use tiber_swap::instructions::CreatePoolArgs;
use tiber_token::client::{self as token_client, Hook};
use tiber_token::instructions::CreateMintArgs;
use tiber_token::state::AuthorityKind;

pub mod constants;
pub mod error;
pub mod rules;
pub mod state;

use constants::*;
use error::TwinsError;
use state::*;

declare_id!("e1UkVXrsWoQW97Yw8eCP1NJ3kcdqLYNgwQwidL7Lg89");

#[cfg(not(feature = "no-entrypoint"))]
solana_security_txt::security_txt! {
    name: "Tiber twins",
    project_url: "https://github.com/TiberFounder/tiber",
    contacts: "link:https://github.com/TiberFounder/tiber/security/advisories/new",
    policy: "https://github.com/TiberFounder/tiber/blob/main/SECURITY.md",
    source_code: "https://github.com/TiberFounder/tiber"
}

/// Arguments of `create_twin`: the token's metadata.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct CreateTwinArgs {
    pub name: String,
    pub symbol: String,
    pub uri: String,
}

#[program]
pub mod tiber_twins {
    use super::*;

    /// Creates the config; only this program's upgrade authority may.
    pub fn init_config(ctx: Context<InitConfig>, admin: Pubkey) -> Result<()> {
        let authority = upgrade_authority(&ctx.accounts.program_data)?;
        require!(
            authority == Some(ctx.accounts.authority.key()),
            TwinsError::NotUpgradeAuthority
        );
        let config = &mut ctx.accounts.config;
        config.bump = ctx.bumps.config;
        config.admin = admin;
        config.twins = 0;
        Ok(())
    }

    /// Step 1 (see the module docs).
    pub fn create_twin<'info>(
        ctx: Context<'info, CreateTwin<'info>>,
        args: CreateTwinArgs,
    ) -> Result<()> {
        process_create_twin(ctx, args)
    }

    /// Step 2.
    pub fn open_curve<'info>(ctx: Context<'info, OpenCurve<'info>>) -> Result<()> {
        process_open_curve(ctx)
    }

    /// Step 3.
    pub fn link_twins<'info>(ctx: Context<'info, LinkTwins<'info>>) -> Result<()> {
        process_link_twins(ctx)
    }

    /// Step 4.
    pub fn open_trading(ctx: Context<OpenTrading>) -> Result<()> {
        let now = Clock::get()?.unix_timestamp;
        let (a, b) = (&mut ctx.accounts.twin_a, &mut ctx.accounts.twin_b);
        require!(
            a.sister == b.mint && b.sister == a.mint,
            TwinsError::NotLinked
        );
        require!(a.curve_open && b.curve_open, TwinsError::NoCurve);
        require!(
            a.opened_at == 0 && b.opened_at == 0,
            TwinsError::AlreadyOpen
        );
        a.opened_at = now;
        b.opened_at = now;
        emit!(TradingOpened {
            twin_a: a.mint,
            twin_b: b.mint,
            ts: now
        });
        Ok(())
    }

    /// Graduates a curve that has raised its target (anyone may call it).
    pub fn graduate<'info>(ctx: Context<'info, Graduate<'info>>) -> Result<()> {
        process_graduate(ctx)
    }

    /// Releases `amount` of a twin's treasury to `destination` (a holding of the twin, which the
    /// wall still holds to the 1.5% cap and the wallets-only rule).
    pub fn release_treasury<'info>(
        ctx: Context<'info, ReleaseTreasury<'info>>,
        amount: u64,
    ) -> Result<()> {
        process_release_treasury(ctx, amount)
    }

    /// The exchange's callback when someone creates a pool with this program as its hook: always
    /// refused (only this program opens a twin's pool, and the exchange skips the callback then).
    pub fn before_initialize(
        _ctx: Context<PoolCallback>,
        _args: PoolHookArgs,
    ) -> Result<HookReturn> {
        err!(TwinsError::OnlyTwinsCreatePools)
    }

    /// The exchange's callback before every swap on a twin's pool.
    pub fn before_swap<'info>(
        ctx: Context<'info, SwapCallback<'info>>,
        args: PoolHookArgs,
    ) -> Result<()> {
        process_before_swap(ctx, args)
    }

    /// The exchange's callback after every swap on a twin's pool.
    pub fn after_swap<'info>(
        ctx: Context<'info, SwapCallback<'info>>,
        args: PoolHookArgs,
    ) -> Result<()> {
        process_after_swap(ctx, args)
    }
}

// ================================================================================== events

#[event]
pub struct TwinCreated {
    pub mint: Pubkey,
    pub twin: Pubkey,
    pub pool: Pubkey,
    pub buyer: Pubkey,
}

#[event]
pub struct TradingOpened {
    pub twin_a: Pubkey,
    pub twin_b: Pubkey,
    pub ts: i64,
}

#[event]
pub struct Graduated {
    pub mint: Pubkey,
    pub pool: Pubkey,
    pub topup: u64,
    pub burned: u64,
    pub lp_locked: u64,
    pub ts: i64,
}

#[event]
pub struct TreasuryReleased {
    pub mint: Pubkey,
    pub destination: Pubkey,
    pub amount: u64,
    pub released: u64,
    pub ts: i64,
}

// ================================================================================== helpers

/// The upgrade authority recorded in `program_data` (this program's ProgramData).
fn upgrade_authority(program_data: &AccountInfo) -> Result<Option<Pubkey>> {
    let (expected, _) =
        Pubkey::find_program_address(&[crate::ID.as_ref()], &BPF_LOADER_UPGRADEABLE_ID);
    require_keys_eq!(
        *program_data.key,
        expected,
        TwinsError::NotUpgradeAuthority
    );
    require_keys_eq!(
        *program_data.owner,
        BPF_LOADER_UPGRADEABLE_ID,
        TwinsError::NotUpgradeAuthority
    );
    let data = program_data.try_borrow_data()?;
    require!(
        data.len() >= 45 && data[..4] == [3, 0, 0, 0],
        TwinsError::NotUpgradeAuthority
    );
    if data[12] == 0 {
        return Ok(None);
    }
    Ok(Some(Pubkey::new_from_array(
        data[13..45].try_into().expect("32"),
    )))
}

/// A twin's pool: base the twin, quote bridged SOL, no LP fee, this program as the hook.
pub fn pool_address(mint: &Pubkey) -> Pubkey {
    tiber_swap::state::Pool::address(mint, &BRIDGED_SOL_MINT, LP_FEE_BPS, Some(crate::ID)).0
}

/// The tithe's buyer of `mint`.
pub fn buyer_address(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[BUYER_SEED, mint.as_ref()], &TITHE_ID).0
}

/// A twin's account.
pub fn twin_address(mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[TWIN_SEED, mint.as_ref()], &crate::ID)
}

/// The wall's account for `mint`.
pub fn wall_address(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[tiber_bridge::wall::WALL_SEED, mint.as_ref()], &WALL_ID).0
}

/// The wall as a twin mint's token hook.
fn wall_hook() -> Hook {
    Hook {
        program: WALL_ID,
        signer: WALL_TOKEN_SIGNER,
    }
}

/// The wall's extras for `mint`: its account.
fn wall_extras(wall: Pubkey) -> Vec<AccountMeta> {
    vec![AccountMeta::new_readonly(wall, false)]
}

const AUTHORITY_SEEDS: &[&[u8]] = &[HOOK_AUTHORITY_SEED, &[TWINS_AUTHORITY_BUMP]];

/// Invokes `ix` signed by the twin of `mint` (and this program's authority when `with_authority`).
fn invoke_as_twin<'info>(
    ix: &Instruction,
    infos: &[AccountInfo<'info>],
    mint: &Pubkey,
    twin_bump: u8,
    with_authority: bool,
) -> Result<()> {
    let bump = [twin_bump];
    let twin_seeds: &[&[u8]] = &[TWIN_SEED, mint.as_ref(), &bump];
    if with_authority {
        invoke_signed(ix, infos, &[twin_seeds, AUTHORITY_SEEDS])?;
    } else {
        invoke_signed(ix, infos, &[twin_seeds])?;
    }
    Ok(())
}

// ================================================================================== init_config

#[derive(Accounts)]
pub struct InitConfig<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(init, payer = authority, space = 8 + Config::INIT_SPACE, seeds = [CONFIG_SEED], bump)]
    pub config: Account<'info, Config>,
    /// CHECK: this program's ProgramData (checked in the handler).
    pub program_data: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

// ================================================================================== create_twin

#[derive(Accounts)]
pub struct CreateTwin<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump, has_one = admin @ TwinsError::NotAdmin)]
    pub config: Account<'info, Config>,
    /// CHECK: the new mint (a fresh keypair; the token program creates it).
    #[account(mut, signer)]
    pub mint: UncheckedAccount<'info>,
    #[account(init, payer = admin, space = 8 + Twin::INIT_SPACE, seeds = [TWIN_SEED, mint.key().as_ref()], bump)]
    pub twin: Box<Account<'info, Twin>>,
    /// CHECK: the twin's holding of its token (the token program creates it).
    #[account(mut)]
    pub twin_holding: UncheckedAccount<'info>,
    /// CHECK: this program's authority (address-checked); signs the wall's `prepare`.
    #[account(address = TWINS_AUTHORITY @ TwinsError::WrongAccount)]
    pub twins_authority: UncheckedAccount<'info>,
    /// CHECK: the wall (address-checked).
    #[account(address = WALL_ID @ TwinsError::WrongProgram)]
    pub wall_program: UncheckedAccount<'info>,
    /// CHECK: the wall's account for the mint (the wall creates it).
    #[account(mut)]
    pub wall: UncheckedAccount<'info>,
    /// CHECK: the wall's registry for the mint (the wall creates it).
    #[account(mut)]
    pub wall_registry: UncheckedAccount<'info>,
    /// CHECK: the token program's signer of the wall's callbacks (address-checked).
    #[account(address = WALL_TOKEN_SIGNER @ TwinsError::WrongAccount)]
    pub wall_token_signer: UncheckedAccount<'info>,
    /// CHECK: the token program (address-checked).
    #[account(address = tiber_token::ID @ TwinsError::WrongProgram)]
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: its event authority (address-checked).
    #[account(address = TOKEN_EVENT_AUTHORITY @ TwinsError::WrongAccount)]
    pub token_event_authority: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

fn process_create_twin<'info>(
    ctx: Context<'info, CreateTwin<'info>>,
    args: CreateTwinArgs,
) -> Result<()> {
    require!(ctx.accounts.config.twins < MAX_TWINS, TwinsError::TooManyTwins);
    let mint = ctx.accounts.mint.key();
    let twin_key = ctx.accounts.twin.key();
    let twin_bump = ctx.bumps.twin;
    let pool = pool_address(&mint);
    let buyer = buyer_address(&mint);
    let wall = wall_address(&mint);
    require_keys_eq!(ctx.accounts.wall.key(), wall, TwinsError::WrongAccount);
    let holding = token_client::holding_address(&mint, &twin_key);
    require_keys_eq!(
        ctx.accounts.twin_holding.key(),
        holding,
        TwinsError::WrongAccount
    );
    let admin = ctx.accounts.admin.key();
    let infos = ctx.accounts.to_account_infos();

    // 1. The wall: the twin's own accounts are exempt (the twin, its pool, the tithe's buyer).
    let ix = Instruction {
        program_id: WALL_ID,
        accounts: vec![
            AccountMeta::new(admin, true),
            AccountMeta::new_readonly(TWINS_AUTHORITY, true),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(wall, false),
            AccountMeta::new(ctx.accounts.wall_registry.key(), false),
            AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
        ],
        data: tiber_bridge::instruction::WallPrepare {
            exempt: [twin_key, pool, buyer, Pubkey::default()],
        }
        .data(),
    };
    invoke_signed(&ix, &infos, &[AUTHORITY_SEEDS])?;

    // 2. The mint: all of the supply, the wall as its hook, the twin as its mint authority until
    //    the supply is minted, no freeze authority. The admin keeps only the metadata (name,
    //    symbol, logo link).
    let ix = token_client::create_mint(
        admin,
        mint,
        CreateMintArgs {
            decimals: DECIMALS,
            name: args.name,
            symbol: args.symbol,
            uri: args.uri,
            max_supply: SUPPLY,
            mint_authority: Some(twin_key),
            freeze_authority: None,
            hook_program: Some(WALL_ID),
            hook_flags: tiber_bridge::wall::FLAGS,
            hook_authority: None,
            metadata_authority: Some(admin),
        },
    );
    invoke_signed(&ix, &infos, &[])?;
    let ix = token_client::create_holding(admin, mint, twin_key);
    invoke_signed(&ix, &infos, &[])?;

    // 3. The supply to the twin, then no one can ever mint again.
    let ix = token_client::mint_to_with(
        twin_key,
        mint,
        holding,
        Some(wall_hook()),
        wall_extras(wall),
        SUPPLY,
    );
    invoke_as_twin(&ix, &infos, &mint, twin_bump, false)?;
    let ix = token_client::set_authority(twin_key, mint, AuthorityKind::Mint, None);
    invoke_as_twin(&ix, &infos, &mint, twin_bump, false)?;

    let twin = &mut ctx.accounts.twin;
    twin.bump = twin_bump;
    twin.mint = mint;
    twin.pool = pool;
    twin.buyer = buyer;
    twin.holding = holding;
    twin.lp_holding =
        token_client::holding_address(&tiber_swap::client::lp_mint_address(&pool), &twin_key);
    ctx.accounts.config.twins += 1;
    emit!(TwinCreated {
        mint,
        twin: twin_key,
        pool,
        buyer
    });
    Ok(())
}

// ================================================================================== open_curve

#[derive(Accounts)]
pub struct OpenCurve<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = admin @ TwinsError::NotAdmin)]
    pub config: Account<'info, Config>,
    #[account(mut, seeds = [TWIN_SEED, mint.key().as_ref()], bump = twin.bump)]
    pub twin: Box<Account<'info, Twin>>,
    /// CHECK: the twin's mint (bound by the twin's seeds).
    pub mint: UncheckedAccount<'info>,
    /// CHECK: bridged SOL (address-checked).
    #[account(address = BRIDGED_SOL_MINT @ TwinsError::WrongAccount)]
    pub quote_mint: UncheckedAccount<'info>,
    /// CHECK: the pool (address-checked; the exchange creates it).
    #[account(mut, address = twin.pool @ TwinsError::WrongPool)]
    pub pool: UncheckedAccount<'info>,
    /// CHECK: the pool's LP mint, vaults and the twin's holdings, which the exchange derives and
    /// checks (the twin's token holding is address-checked).
    #[account(mut)]
    pub lp_mint: UncheckedAccount<'info>,
    /// CHECK: see `lp_mint`.
    #[account(mut)]
    pub base_vault: UncheckedAccount<'info>,
    /// CHECK: see `lp_mint`.
    #[account(mut)]
    pub quote_vault: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(mut, address = twin.holding @ TwinsError::WrongAccount)]
    pub twin_holding: UncheckedAccount<'info>,
    /// CHECK: the twin's bridged-SOL holding (created here; it never holds anything).
    #[account(mut)]
    pub twin_quote: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(mut, address = twin.lp_holding @ TwinsError::WrongAccount)]
    pub twin_lp: UncheckedAccount<'info>,
    /// CHECK: the exchange's config (address-checked).
    #[account(mut, address = DEX_CONFIG @ TwinsError::WrongAccount)]
    pub dex_config: UncheckedAccount<'info>,
    /// CHECK: the exchange's treasury (the exchange checks it).
    #[account(mut)]
    pub dex_treasury: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = TWINS_AUTHORITY @ TwinsError::WrongAccount)]
    pub twins_authority: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = DEX_HOOK_SIGNER @ TwinsError::WrongAccount)]
    pub dex_hook_signer: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = WALL_ID @ TwinsError::WrongProgram)]
    pub wall_program: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = WALL_TOKEN_SIGNER @ TwinsError::WrongAccount)]
    pub wall_token_signer: UncheckedAccount<'info>,
    /// CHECK: the wall's account for the mint (address-checked in the handler).
    pub wall: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = tiber_token::ID @ TwinsError::WrongProgram)]
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = TOKEN_EVENT_AUTHORITY @ TwinsError::WrongAccount)]
    pub token_event_authority: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = tiber_swap::ID @ TwinsError::WrongProgram)]
    pub swap_program: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = DEX_EVENT_AUTHORITY @ TwinsError::WrongAccount)]
    pub dex_event_authority: UncheckedAccount<'info>,
    /// CHECK: this program, the pool's hook (address-checked).
    #[account(address = crate::ID @ TwinsError::WrongProgram)]
    pub twins_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

fn process_open_curve<'info>(ctx: Context<'info, OpenCurve<'info>>) -> Result<()> {
    require!(!ctx.accounts.twin.curve_open, TwinsError::CurveExists);
    let mint = ctx.accounts.mint.key();
    let wall = wall_address(&mint);
    require_keys_eq!(ctx.accounts.wall.key(), wall, TwinsError::WrongAccount);
    let twin_key = ctx.accounts.twin.key();
    let twin_bump = ctx.accounts.twin.bump;
    let admin = ctx.accounts.admin.key();
    let infos = ctx.accounts.to_account_infos();

    let ix = token_client::create_holding(admin, BRIDGED_SOL_MINT, twin_key);
    invoke_signed(&ix, &infos, &[])?;

    let ix = tiber_swap::client::create_pool(
        &tiber_swap::client::CreatePoolKeys {
            payer: admin,
            authority: twin_key,
            treasury: ctx.accounts.dex_treasury.key(),
            base_mint: mint,
            quote_mint: BRIDGED_SOL_MINT,
            hook_caller: Some(TWINS_AUTHORITY),
        },
        CreatePoolArgs {
            lp_fee_bps: LP_FEE_BPS,
            hook_program: crate::ID,
            hook_flags: POOL_FLAGS,
            virtual_base: VIRTUAL_BASE,
            virtual_quote: VIRTUAL_QUOTE,
            base_amount: CURVE_TOKENS,
            quote_amount: 0,
            base_hook_accounts: 3,
            quote_hook_accounts: 0,
            hook_data: vec![],
        },
        vec![
            AccountMeta::new_readonly(WALL_ID, false),
            AccountMeta::new_readonly(WALL_TOKEN_SIGNER, false),
            AccountMeta::new_readonly(wall, false),
        ],
    );
    invoke_as_twin(&ix, &infos, &mint, twin_bump, true)?;
    ctx.accounts.twin.curve_open = true;
    Ok(())
}

// ================================================================================== link_twins

#[derive(Accounts)]
pub struct LinkTwins<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = admin @ TwinsError::NotAdmin)]
    pub config: Account<'info, Config>,
    #[account(mut, constraint = twin_a.key() != twin_b.key() @ TwinsError::SameTwin)]
    pub twin_a: Box<Account<'info, Twin>>,
    #[account(mut)]
    pub twin_b: Box<Account<'info, Twin>>,
    /// CHECK: bridged SOL (address-checked).
    #[account(address = BRIDGED_SOL_MINT @ TwinsError::WrongAccount)]
    pub quote_mint: UncheckedAccount<'info>,
    /// CHECK: the bridged-SOL holding of twin A's buyer (created here): receives B's buy tax.
    #[account(mut)]
    pub fund_a: UncheckedAccount<'info>,
    /// CHECK: the bridged-SOL holding of twin B's buyer (created here): receives A's buy tax.
    #[account(mut)]
    pub fund_b: UncheckedAccount<'info>,
    /// CHECK: twin A's buyer (address-checked against the twin).
    #[account(address = twin_a.buyer @ TwinsError::WrongAccount)]
    pub buyer_a: UncheckedAccount<'info>,
    /// CHECK: twin B's buyer.
    #[account(address = twin_b.buyer @ TwinsError::WrongAccount)]
    pub buyer_b: UncheckedAccount<'info>,
    /// CHECK: this program's registry for A's pool (address-checked in the handler).
    #[account(mut)]
    pub registry_a: UncheckedAccount<'info>,
    /// CHECK: this program's registry for B's pool.
    #[account(mut)]
    pub registry_b: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = tiber_token::ID @ TwinsError::WrongProgram)]
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = TOKEN_EVENT_AUTHORITY @ TwinsError::WrongAccount)]
    pub token_event_authority: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

fn process_link_twins<'info>(ctx: Context<'info, LinkTwins<'info>>) -> Result<()> {
    let a = &ctx.accounts;
    require!(
        a.twin_a.sister == Pubkey::default() && a.twin_b.sister == Pubkey::default(),
        TwinsError::AlreadyLinked
    );
    let fund_a = token_client::holding_address(&BRIDGED_SOL_MINT, &a.twin_a.buyer);
    let fund_b = token_client::holding_address(&BRIDGED_SOL_MINT, &a.twin_b.buyer);
    require_keys_eq!(a.fund_a.key(), fund_a, TwinsError::WrongAccount);
    require_keys_eq!(a.fund_b.key(), fund_b, TwinsError::WrongAccount);
    let admin = a.admin.key();
    let infos = a.to_account_infos();
    for buyer in [a.twin_a.buyer, a.twin_b.buyer] {
        let ix = token_client::create_holding(admin, BRIDGED_SOL_MINT, buyer);
        invoke_signed(&ix, &infos, &[])?;
    }
    // Each pool's callbacks get the twin and the sister's fund: A's buy tax buys B, so it goes to
    // B's buyer.
    for (twin, fund, registry) in [
        (a.twin_a.key(), fund_b, &a.registry_a),
        (a.twin_b.key(), fund_a, &a.registry_b),
    ] {
        let pool = if twin == a.twin_a.key() {
            a.twin_a.pool
        } else {
            a.twin_b.pool
        };
        let (expected, bump) = hook_accounts_address(&crate::ID, &pool);
        require_keys_eq!(registry.key(), expected, TwinsError::WrongAccount);
        write_registry(
            &a.admin.to_account_info(),
            &registry.to_account_info(),
            &a.system_program.to_account_info(),
            &crate::ID,
            &pool,
            bump,
            &HookAccountList::new(vec![
                ExtraAccount {
                    writable: true,
                    source: AccountSource::Key(twin),
                },
                ExtraAccount {
                    writable: true,
                    source: AccountSource::Key(fund),
                },
                // The trader's holding of the twin: `["holding", base_mint, actor]` under the
                // token program, read for the loyalty burn.
                ExtraAccount {
                    writable: false,
                    source: AccountSource::Pda {
                        program: tiber_token::ID,
                        seeds: vec![
                            Seed::Literal(b"holding".to_vec()),
                            Seed::Account(2),
                            Seed::Account(4),
                        ],
                    },
                },
            ]),
        )?;
    }
    let (mint_a, mint_b) = (a.twin_a.mint, a.twin_b.mint);
    let twin_a = &mut ctx.accounts.twin_a;
    twin_a.sister = mint_b;
    twin_a.sister_fund = fund_b;
    let twin_b = &mut ctx.accounts.twin_b;
    twin_b.sister = mint_a;
    twin_b.sister_fund = fund_a;
    Ok(())
}

// ================================================================================== open_trading

#[derive(Accounts)]
pub struct OpenTrading<'info> {
    pub admin: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = admin @ TwinsError::NotAdmin)]
    pub config: Account<'info, Config>,
    #[account(mut, constraint = twin_a.key() != twin_b.key() @ TwinsError::SameTwin)]
    pub twin_a: Box<Account<'info, Twin>>,
    #[account(mut)]
    pub twin_b: Box<Account<'info, Twin>>,
}

// ================================================================================== swap hooks

/// Accounts of `before_initialize`: the prefix only.
#[derive(Accounts)]
pub struct PoolCallback<'info> {
    /// CHECK: the exchange's signer for this program (address- and signer-checked).
    #[account(signer, address = DEX_HOOK_SIGNER @ TwinsError::BadHookSigner)]
    pub hook_signer: UncheckedAccount<'info>,
    /// CHECK: unused.
    pub pool: UncheckedAccount<'info>,
    /// CHECK: unused.
    pub base_mint: UncheckedAccount<'info>,
    /// CHECK: unused.
    pub quote_mint: UncheckedAccount<'info>,
    /// CHECK: unused.
    pub actor: UncheckedAccount<'info>,
}

/// Accounts of `before_swap` and `after_swap`: the prefix, then the registry's two extras.
#[derive(Accounts)]
pub struct SwapCallback<'info> {
    /// CHECK: the exchange's signer for this program (address- and signer-checked): only the
    /// exchange can sign for it, so the arguments are the exchange's.
    #[account(signer, address = DEX_HOOK_SIGNER @ TwinsError::BadHookSigner)]
    pub hook_signer: UncheckedAccount<'info>,
    /// CHECK: the pool (must be the twin's).
    pub pool: UncheckedAccount<'info>,
    /// CHECK: the base mint (must be the twin's).
    pub base_mint: UncheckedAccount<'info>,
    /// CHECK: unused.
    pub quote_mint: UncheckedAccount<'info>,
    /// CHECK: the trader.
    pub actor: UncheckedAccount<'info>,
    #[account(
        mut,
        constraint = twin.pool == pool.key() @ TwinsError::WrongPool,
        constraint = twin.mint == base_mint.key() @ TwinsError::WrongPool
    )]
    pub twin: Box<Account<'info, Twin>>,
    /// CHECK: the sister's buyer's bridged-SOL holding (address-checked); the exchange checks it
    /// is a writable holding of the quote before paying it.
    #[account(mut, address = twin.sister_fund @ TwinsError::NotLinked)]
    pub sister_fund: UncheckedAccount<'info>,
    /// CHECK: the trader's holding of the twin, read for the loyalty burn; trusted only when it is
    /// a holding of this twin owned by the trader (checked in the handler).
    pub trader_holding: UncheckedAccount<'info>,
}

/// When the trader's tokens arrived, from its holding's note (0 when the account is not the
/// trader's holding of the twin, or has no note).
fn trader_held_since(info: &AccountInfo, mint: &Pubkey, trader: &Pubkey) -> i64 {
    if *info.owner != tiber_token::ID {
        return 0;
    }
    let Ok(data) = info.try_borrow_data() else { return 0 };
    match tiber_token::state::Holding::try_deserialize(&mut &data[..]) {
        Ok(h) if h.mint == *mint && h.owner == *trader => tiber_bridge::wall::read_held_since(&h.hook_data),
        _ => 0,
    }
}

fn process_before_swap<'info>(
    ctx: Context<'info, SwapCallback<'info>>,
    args: PoolHookArgs,
) -> Result<()> {
    require_keys_eq!(args.pool, ctx.accounts.twin.pool, TwinsError::WrongPool);
    let now = Clock::get()?.unix_timestamp;
    let twin = &mut ctx.accounts.twin;
    require!(twin.opened_at > 0 && now >= twin.opened_at, TwinsError::NotOpen);
    // The tithe's buyer of this twin trades free.
    if args.actor == twin.buyer {
        return Ok(());
    }
    let buy = args.direction == 1;
    if !buy {
        let since = trader_held_since(&ctx.accounts.trader_holding, &twin.mint, &args.actor);
        let burn = trade_burn(args.amount_in, rules::sell_burn_bps(now, since));
        twin.sell_burned = twin.sell_burned.saturating_add(burn);
        set_answer(
            &HookReturn {
                burn,
                ..HookReturn::default()
            },
            None,
        );
        return Ok(());
    }
    let tax_bps = rules::buy_tax_bps(now, twin.opened_at);
    // A buy past the end of the curve: the smallest input that sells it out, on which the tax is
    // then taken; the exchange leaves the rest with the trader. Only for a swap the transaction
    // sends the exchange itself.
    let direct = anchor_lang::solana_program::instruction::get_stack_height()
        == anchor_lang::solana_program::instruction::TRANSACTION_LEVEL_STACK_HEIGHT + 1;
    let reaches_end = args.base_reserve > 0
        && curve_out(
            args.amount_in,
            args.quote_reserve,
            args.virtual_quote,
            args.base_reserve,
            args.virtual_base,
        )
        .is_some_and(|out| out >= u128::from(args.base_reserve));
    let fill = if !twin.graduated && direct && reaches_end {
        launch_fill(
            &Reserves {
                base_reserve: args.base_reserve,
                quote_reserve: args.quote_reserve,
                virtual_base: args.virtual_base,
                virtual_quote: args.virtual_quote,
            },
            args.amount_in,
            args.lp_fee_bps,
            0,
            &rules::rates(tax_bps),
            0,
            0,
            0,
        )
    } else {
        None
    };
    let amount_in = fill.unwrap_or(args.amount_in);
    let tax = rules::buy_tax(amount_in, tax_bps);
    let mut ret = HookReturn::default();
    if tax > 0 {
        ret.deltas.push(Delta {
            amount: tax,
            account: SISTER_FUND_INDEX,
        });
        twin.buy_tax_paid = twin.buy_tax_paid.saturating_add(tax);
    }
    set_answer(&ret, fill);
    Ok(())
}

fn process_after_swap<'info>(
    ctx: Context<'info, SwapCallback<'info>>,
    args: PoolHookArgs,
) -> Result<()> {
    require_keys_eq!(args.pool, ctx.accounts.twin.pool, TwinsError::WrongPool);
    if args.direction == 1 && args.actor != ctx.accounts.twin.buyer {
        require!(args.amount_out <= MAX_BUY, TwinsError::BuyTooLarge);
    }
    Ok(())
}

// ================================================================================== graduate

#[derive(Accounts)]
pub struct Graduate<'info> {
    pub cranker: Signer<'info>,
    #[account(
        mut,
        seeds = [TWIN_SEED, mint.key().as_ref()],
        bump = twin.bump,
        constraint = !twin.graduated @ TwinsError::AlreadyGraduated,
        constraint = twin.curve_open @ TwinsError::NoCurve
    )]
    pub twin: Box<Account<'info, Twin>>,
    /// CHECK: the twin's mint (bound by the twin's seeds; written by the burn).
    #[account(mut)]
    pub mint: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(mut, address = twin.pool @ TwinsError::WrongPool)]
    pub pool: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(mut, address = twin.holding @ TwinsError::WrongAccount)]
    pub twin_holding: UncheckedAccount<'info>,
    /// CHECK: checked against the pool in the handler.
    #[account(mut)]
    pub base_vault: UncheckedAccount<'info>,
    /// CHECK: checked against the pool in the handler.
    pub quote_vault: UncheckedAccount<'info>,
    /// CHECK: checked against the pool in the handler.
    #[account(mut)]
    pub lp_mint: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(mut, address = twin.lp_holding @ TwinsError::WrongAccount)]
    pub twin_lp: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = TWINS_AUTHORITY @ TwinsError::WrongAccount)]
    pub twins_authority: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = WALL_ID @ TwinsError::WrongProgram)]
    pub wall_program: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = WALL_TOKEN_SIGNER @ TwinsError::WrongAccount)]
    pub wall_token_signer: UncheckedAccount<'info>,
    /// CHECK: address-checked in the handler.
    pub wall: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = tiber_token::ID @ TwinsError::WrongProgram)]
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = TOKEN_EVENT_AUTHORITY @ TwinsError::WrongAccount)]
    pub token_event_authority: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = tiber_swap::ID @ TwinsError::WrongProgram)]
    pub swap_program: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = DEX_EVENT_AUTHORITY @ TwinsError::WrongAccount)]
    pub dex_event_authority: UncheckedAccount<'info>,
}

fn process_graduate<'info>(ctx: Context<'info, Graduate<'info>>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let mint = ctx.accounts.mint.key();
    let wall = wall_address(&mint);
    require_keys_eq!(ctx.accounts.wall.key(), wall, TwinsError::WrongAccount);
    let pool = tiber_swap::client::read_pool(&ctx.accounts.pool)?;
    require_keys_eq!(
        ctx.accounts.base_vault.key(),
        pool.base_vault,
        TwinsError::WrongAccount
    );
    require_keys_eq!(
        ctx.accounts.quote_vault.key(),
        pool.quote_vault,
        TwinsError::WrongAccount
    );
    require_keys_eq!(
        ctx.accounts.lp_mint.key(),
        pool.lp_mint,
        TwinsError::WrongAccount
    );
    require!(pool.curve, TwinsError::AlreadyGraduated);
    require!(
        pool.quote_reserve >= GRADUATION_QUOTE || pool.base_reserve == 0,
        TwinsError::NotReady
    );
    let twin_key = ctx.accounts.twin.key();
    let twin_bump = ctx.accounts.twin.bump;
    let infos = ctx.accounts.to_account_infos();

    // 1. The top-up, so the price is the same once the virtual reserves are gone.
    let topup = graduation_topup(
        pool.base_reserve,
        pool.quote_reserve,
        pool.virtual_base,
        pool.virtual_quote,
    )
    .ok_or(TwinsError::MathOverflow)?
    .min(RESERVE_TOKENS);
    if topup > 0 {
        let ix = token_client::transfer_with(
            twin_key,
            ctx.accounts.twin_holding.key(),
            pool.base_vault,
            mint,
            Some(wall_hook()),
            wall_extras(wall),
            topup,
        );
        invoke_as_twin(&ix, &infos, &mint, twin_bump, false)?;
    }

    // 2. The exchange turns the curve into an ordinary pool; the LP lands with the twin for good.
    let ix = tiber_swap::client::finalize_curve(
        TWINS_AUTHORITY,
        TWINS_AUTHORITY_BUMP,
        ctx.accounts.twin.pool,
        mint,
        BRIDGED_SOL_MINT,
        ctx.accounts.twin_lp.key(),
    );
    invoke_signed(&ix, &infos, &[AUTHORITY_SEEDS])?;

    // 3. The rest of the reserve burns.
    let burned = RESERVE_TOKENS - topup;
    if burned > 0 {
        let ix = token_client::burn_with(
            twin_key,
            ctx.accounts.twin_holding.key(),
            mint,
            Some(wall_hook()),
            wall_extras(wall),
            burned,
        );
        invoke_as_twin(&ix, &infos, &mint, twin_bump, false)?;
    }

    let lp_locked = token_client::read_holding(&ctx.accounts.twin_lp)?.amount;
    let twin = &mut ctx.accounts.twin;
    twin.graduated = true;
    twin.graduated_at = now;
    twin.graduation_topup = topup;
    twin.graduation_burned = burned;
    emit!(Graduated {
        mint,
        pool: twin.pool,
        topup,
        burned,
        lp_locked,
        ts: now
    });
    Ok(())
}

// ================================================================================== treasury

#[derive(Accounts)]
pub struct ReleaseTreasury<'info> {
    pub admin: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = admin @ TwinsError::NotAdmin)]
    pub config: Account<'info, Config>,
    #[account(mut, seeds = [TWIN_SEED, mint.key().as_ref()], bump = twin.bump)]
    pub twin: Box<Account<'info, Twin>>,
    /// CHECK: the twin's mint (bound by the twin's seeds).
    pub mint: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(mut, address = twin.holding @ TwinsError::WrongAccount)]
    pub twin_holding: UncheckedAccount<'info>,
    /// CHECK: a holding of the twin (the token program checks it; the wall checks its owner).
    #[account(mut)]
    pub destination: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = WALL_ID @ TwinsError::WrongProgram)]
    pub wall_program: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = WALL_TOKEN_SIGNER @ TwinsError::WrongAccount)]
    pub wall_token_signer: UncheckedAccount<'info>,
    /// CHECK: address-checked in the handler.
    pub wall: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = tiber_token::ID @ TwinsError::WrongProgram)]
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: address-checked.
    #[account(address = TOKEN_EVENT_AUTHORITY @ TwinsError::WrongAccount)]
    pub token_event_authority: UncheckedAccount<'info>,
}

fn process_release_treasury<'info>(
    ctx: Context<'info, ReleaseTreasury<'info>>,
    amount: u64,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let mint = ctx.accounts.mint.key();
    let wall = wall_address(&mint);
    require_keys_eq!(ctx.accounts.wall.key(), wall, TwinsError::WrongAccount);
    ctx.accounts
        .twin
        .release(amount, now)
        .map_err(TwinsError::from)?;
    let twin_key = ctx.accounts.twin.key();
    let twin_bump = ctx.accounts.twin.bump;
    let infos = ctx.accounts.to_account_infos();
    let ix = token_client::transfer_with(
        twin_key,
        ctx.accounts.twin_holding.key(),
        ctx.accounts.destination.key(),
        mint,
        Some(wall_hook()),
        wall_extras(wall),
        amount,
    );
    invoke_as_twin(&ix, &infos, &mint, twin_bump, false)?;
    emit!(TreasuryReleased {
        mint,
        destination: ctx.accounts.destination.key(),
        amount,
        released: ctx.accounts.twin.treasury_released,
        ts: now
    });
    Ok(())
}

#[cfg(test)]
mod address_tests {
    use super::*;

    #[test]
    fn the_constants_match_their_derivations() {
        assert_eq!(
            (TWINS_AUTHORITY, TWINS_AUTHORITY_BUMP),
            Pubkey::find_program_address(&[HOOK_AUTHORITY_SEED], &crate::ID)
        );
        assert_eq!(
            DEX_HOOK_SIGNER,
            Pubkey::find_program_address(&[HOOK_AUTHORITY_SEED, crate::ID.as_ref()], &tiber_swap::ID).0
        );
        assert_eq!(DEX_CONFIG, tiber_swap::client::config_address());
        assert_eq!(DEX_EVENT_AUTHORITY, tiber_swap::EVENT_AUTHORITY_AND_BUMP.0);
        assert_eq!(TOKEN_EVENT_AUTHORITY, tiber_token::EVENT_AUTHORITY_AND_BUMP.0);
        assert_eq!(tiber_bridge::wall::TWINS_ID, crate::ID);
        assert_eq!(tiber_swap::constants::LAUNCHPAD_ID, crate::ID);
    }
}
