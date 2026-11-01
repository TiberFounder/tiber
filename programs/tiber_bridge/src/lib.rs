//! `tiber_bridge`: SPL Token, Token-2022 and native SOL in; Tiber Token Standard out, one
//! for one, and back. It also carries the Wall, the twins' token hook ([`wall`]).
//!
//! A wrapper (`["wrapper", underlying]`) holds the underlying in a vault (the wrapper PDA's
//! associated token account, or the `["sol-vault"]` PDA for SOL) and is the mint authority of the
//! wrapped BTS mint (`["wrapped", underlying]`). `wrap` mints what actually arrived in the vault, so
//! a Token-2022 transfer fee is honoured; `unwrap` burns and pays out. Mints with transfer hooks
//! are not supported in this version.

#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;

pub mod client;
pub mod constants;
pub mod error;
pub mod events;
pub mod instructions;
pub mod spl;
pub mod state;
pub mod wall;

pub use instructions::*;
pub use wall::*;

declare_id!("4WXBYj7efT8SYhRxacekrp2zgmqoPnyGQifE6obRGLeG");

#[cfg(not(feature = "no-entrypoint"))]
solana_security_txt::security_txt! {
    name: "Tiber bridge",
    project_url: "https://github.com/TiberFounder/tiber",
    contacts: "link:https://github.com/TiberFounder/tiber/security/advisories/new",
    policy: "https://github.com/TiberFounder/tiber/blob/main/SECURITY.md",
    source_code: "https://github.com/TiberFounder/tiber"
}

/// Instructions of the bridge.
#[program]
pub mod tiber_bridge {
    use super::*;

    /// Creates the config; signed by the program's upgrade authority, once.
    pub fn init_config(ctx: Context<InitConfig>, args: ConfigArgs) -> Result<()> {
        config::process_init_config(ctx, args)
    }

    /// Admin: changes the config.
    pub fn set_config(ctx: Context<SetConfig>, args: ConfigArgs) -> Result<()> {
        config::process_set_config(ctx, args)
    }


    /// Registers native SOL (the wSOL mint address stands for it). Permissionless, once.
    pub fn register_sol(ctx: Context<RegisterSol>) -> Result<()> {
        wrapper::process_register_sol(ctx)
    }



    /// Moves `lamports` into the SOL vault and mints as much wrapped SOL.
    pub fn wrap_sol(ctx: Context<WrapSol>, lamports: u64) -> Result<()> {
        wrap::process_wrap_sol(ctx, lamports)
    }

    /// Burns `amount` of wrapped SOL (`u64::MAX` for the whole holding) and pays the lamports out.
    pub fn unwrap_sol(ctx: Context<WrapSol>, amount: u64) -> Result<()> {
        wrap::process_unwrap_sol(ctx, amount, false)
    }

    /// Burns everything above `keep` of the user's wrapped SOL and pays the lamports out: what a
    /// trade just delivered, leaving what was held before it.
    pub fn unwrap_sol_above(ctx: Context<WrapSol>, keep: u64) -> Result<()> {
        wrap::process_unwrap_sol(ctx, keep, true)
    }

    /// The Wall: writes a twin's rules (only the twins program may, once per mint).
    pub fn wall_prepare(ctx: Context<Prepare>, exempt: [Pubkey; wall::MAX_EXEMPT]) -> Result<()> {
        wall::process_prepare(ctx, exempt)
    }

    /// The Wall: the token program's callback before every transfer of a twin.
    pub fn before_transfer(
        ctx: Context<BeforeTransfer>,
        args: tiber_hook::TokenHookArgs,
    ) -> Result<tiber_hook::HookReturn> {
        wall::process_before_transfer(ctx, args)
    }
}
