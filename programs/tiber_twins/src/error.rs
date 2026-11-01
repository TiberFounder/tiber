use anchor_lang::prelude::*;

use crate::state::TwinRule;

#[error_code]
pub enum TwinsError {
    #[msg("only this program's upgrade authority may create the config")]
    NotUpgradeAuthority,
    #[msg("only the admin may do this")]
    NotAdmin,
    #[msg("both twins already exist")]
    TooManyTwins,
    #[msg("the account is not the one this twin expects")]
    WrongAccount,
    #[msg("a program this instruction calls is not the expected one")]
    WrongProgram,
    #[msg("the twin's curve already exists")]
    CurveExists,
    #[msg("the twin's curve does not exist yet")]
    NoCurve,
    #[msg("the twins are already linked")]
    AlreadyLinked,
    #[msg("a twin cannot be its own sister")]
    SameTwin,
    #[msg("the twins are not linked yet")]
    NotLinked,
    #[msg("trading is already open")]
    AlreadyOpen,
    #[msg("trading is not open yet")]
    NotOpen,
    #[msg("the hook signer is not the exchange's signer for this program")]
    BadHookSigner,
    #[msg("only the twins program opens a twin's pool")]
    OnlyTwinsCreatePools,
    #[msg("the pool is not this twin's")]
    WrongPool,
    #[msg("one swap may buy at most 1% of the supply")]
    BuyTooLarge,
    #[msg("the curve has already graduated")]
    AlreadyGraduated,
    #[msg("the curve has not raised its target yet")]
    NotReady,
    #[msg("the amount is zero")]
    ZeroAmount,
    #[msg("those treasury tokens are still locked")]
    TreasuryLocked,
    #[msg("the treasury may release at most 1,000,000 tokens a day")]
    DailyLimit,
    #[msg("math overflow")]
    MathOverflow,
}

impl From<TwinRule> for TwinsError {
    fn from(rule: TwinRule) -> Self {
        match rule {
            TwinRule::ZeroAmount => TwinsError::ZeroAmount,
            TwinRule::NotOpen => TwinsError::NotOpen,
            TwinRule::Locked => TwinsError::TreasuryLocked,
            TwinRule::DailyLimit => TwinsError::DailyLimit,
        }
    }
}
