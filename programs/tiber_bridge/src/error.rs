//! Errors of the bridge.

use anchor_lang::prelude::*;

/// Custom errors (6000 + index).
#[error_code]
pub enum BridgeError {
    #[msg("the amount is zero")]
    ZeroAmount,
    #[msg("not the upgrade authority")]
    NotUpgradeAuthority,
    #[msg("not the admin")]
    NotAdmin,
    #[msg("the bridge is paused")]
    Paused,
    #[msg("the underlying is not an SPL Token or Token-2022 mint")]
    NotAMint,
    #[msg("the token program passed is not the mint's")]
    WrongTokenProgram,
    #[msg("the vault passed is not the wrapper's")]
    WrongVault,
    #[msg("the token account passed is for another mint")]
    WrongTokenAccount,
    #[msg("nothing arrived in the vault")]
    NothingReceived,
    #[msg("the vault does not hold enough")]
    InsufficientVault,
    #[msg("invalid metadata")]
    InvalidMetadata,
    #[msg("this wrapper is native SOL; use wrap_sol and unwrap_sol")]
    NativeWrapper,
    #[msg("this wrapper is not native SOL")]
    NotNativeWrapper,
    #[msg("math overflow")]
    MathOverflow,
    // The Wall (`crate::wall`).
    #[msg("only the twins program may write a wall")]
    NotTwins,
    #[msg("the registry is not this hook's for the mint")]
    WrongRegistry,
    #[msg("the hook signer is not the token program's signer for this hook")]
    BadHookSigner,
    #[msg("the arguments are for another mint")]
    WrongMint,
    #[msg("only transfers run this hook")]
    Unsupported,
    #[msg("only ordinary wallets may hold a twin")]
    WalletsOnly,
    #[msg("no wallet may hold more than 1.5% of a twin")]
    OverTheWall,
}
