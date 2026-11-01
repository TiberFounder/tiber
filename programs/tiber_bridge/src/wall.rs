//! The Wall: the token hook of Romulus (ROM) and Remus (REM), part of the bridge program so the
//! twins need one program fewer on chain (the bridge is never running while a twin moves, so it
//! can be the twins' token hook).
//!
//! In the lore the wall is the line no citizen crosses: no one may hold more than 1.5% of a city.
//! On chain, every transfer of a twin runs `before_transfer` here, and the transfer fails when:
//!
//! - the destination's owner is not an ordinary wallet (an address off the ed25519 curve, which
//!   is what every program-owned account and every other exchange's pool is), unless it is one of
//!   the twin's own accounts named at creation (the twins program's account for the mint, the
//!   twin's pool, and the tithe's buyer of the twin). This keeps every trade of a twin on its own
//!   pool, where the taxes are taken; or
//! - the destination would hold more than [`HOLD_CAP`] (1.5% of the supply, fixed at 15,000,000
//!   tokens), again unless it is one of those accounts.
//!
//! The rules are written once, by the twins program when it creates the mint (`wall_prepare`,
//! signed by the twins program's own authority), and nothing can change them afterwards: there is
//! no instruction that edits a wall.

use anchor_lang::prelude::*;
use tiber_hook::{
    hook_accounts_address, token_flags, write_registry, AccountSource, ExtraAccount,
    HookAccountList, HookReturn, Seed, TokenHookArgs, TokenOp,
};
#[cfg(test)]
use tiber_hook::HOOK_AUTHORITY_SEED;

/// `["wall", mint]`: a mint's rules.
pub const WALL_SEED: &[u8] = b"wall";
/// The token program's signer of every callback to this hook: `["hook-authority", tiber_bridge]`
/// under the token program (the unit tests check the derivation).
pub const TOKEN_HOOK_SIGNER: Pubkey =
    Pubkey::from_str_const("4PeArj6Z3uFjYmQ85LUPi9Rkcvy7ruoeJ9hgEbdEiZ3L");
/// The twins program, the only program that may write a wall.
pub const TWINS_ID: Pubkey = Pubkey::from_str_const("e1UkVXrsWoQW97Yw8eCP1NJ3kcdqLYNgwQwidL7Lg89");
/// The twins program's own authority, `["hook-authority"]` under it: the signer of `prepare`.
pub const TWINS_AUTHORITY: Pubkey =
    Pubkey::from_str_const("9BPBHJSWcoySEgovPzqxsbPC8vbWWtAS3NcL9eKosxcs");
/// The flags a twin's mint carries: this hook runs before every transfer, and keeps a note in each
/// holding (when its tokens arrived, for the loyalty burn and the citizen ranks).
pub const FLAGS: u16 = token_flags::BEFORE_TRANSFER | token_flags::WRITES_HOOK_DATA;
/// Version byte of the holding note (`hook_data[8]`).
pub const NOTE_VERSION: u8 = 1;
/// Decimals of a twin.
pub const DECIMALS: u8 = 6;
/// The most any one wallet may hold: 15,000,000 tokens, 1.5% of the 1,000,000,000 minted. Fixed,
/// so burns never lower it under someone already holding.
pub const HOLD_CAP: u64 = 15_000_000 * 1_000_000;
/// Most exempt owners a wall names.
pub const MAX_EXEMPT: usize = 4;


/// `wall_prepare`: writes the wall of `mint` (which need not exist yet: the twins program calls
/// this just before it creates the mint) and publishes the hook's extra-accounts registry.
pub fn process_prepare(ctx: Context<Prepare>, exempt: [Pubkey; MAX_EXEMPT]) -> Result<()> {
    let mint = ctx.accounts.mint.key();
    let wall = &mut ctx.accounts.wall;
    wall.bump = ctx.bumps.wall;
    wall.mint = mint;
    wall.hold_cap = HOLD_CAP;
    wall.exempt = exempt;
    let (expected, bump) = hook_accounts_address(&crate::ID, &mint);
    require_keys_eq!(ctx.accounts.registry.key(), expected, WallError::WrongRegistry);
    let list = HookAccountList::new(vec![ExtraAccount {
        writable: false,
        source: AccountSource::Pda {
            program: crate::ID,
            // `Seed::Account(1)`: the prefix's mint.
            seeds: vec![Seed::Literal(WALL_SEED.to_vec()), Seed::Account(1)],
        },
    }]);
    write_registry(
        &ctx.accounts.payer.to_account_info(),
        &ctx.accounts.registry.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        &crate::ID,
        &mint,
        bump,
        &list,
    )
}

/// `before_transfer`: the token program's callback before every transfer of a twin.
pub fn process_before_transfer(ctx: Context<BeforeTransfer>, args: TokenHookArgs) -> Result<HookReturn> {
    let wall = &ctx.accounts.wall;
    require!(args.op == TokenOp::Transfer, WallError::Unsupported);
    require_keys_eq!(args.mint, wall.mint, WallError::WrongMint);
    check(wall, &args)?;
    // The note of a wallet receiving tokens: when its tokens arrived, on average. The twin's own
    // accounts (pool, treasury, tithe buyer) keep no note.
    if wall.is_exempt(&args.destination_owner) || !keeps_notes(&ctx.accounts.mint)? {
        return Ok(HookReturn::default());
    }
    let now = Clock::get()?.unix_timestamp;
    let since = held_since(
        read_held_since(&args.destination_hook_data),
        args.destination_balance,
        args.amount,
        now,
    );
    Ok(HookReturn {
        destination_hook_data: Some(write_held_since(&args.destination_hook_data, since)),
        ..HookReturn::default()
    })
}

/// Whether the mint lets this hook keep notes (`WRITES_HOOK_DATA`). A mint made before the notes
/// existed has the Wall's rules without it; its transfers still pass, with no note.
fn keeps_notes(mint: &AccountInfo) -> Result<bool> {
    require_keys_eq!(*mint.owner, tiber_token::ID, WallError::WrongMint);
    let data = mint.try_borrow_data()?;
    let mint = tiber_token::state::Mint::try_deserialize(&mut &data[..])?;
    Ok(mint.hook_writes_data())
}

/// When the tokens in a holding arrived, on average, read from its note (0: no note yet).
pub fn read_held_since(note: &[u8; 64]) -> i64 {
    if note[8] != NOTE_VERSION {
        return 0;
    }
    i64::from_le_bytes(note[..8].try_into().expect("8"))
}

/// `note` with its arrival time set to `since`.
pub fn write_held_since(note: &[u8; 64], since: i64) -> [u8; 64] {
    let mut out = *note;
    out[..8].copy_from_slice(&since.to_le_bytes());
    out[8] = NOTE_VERSION;
    out
}

/// The arrival time after `amount` more tokens arrive at `now` in a holding of `balance` that
/// arrived at `since` on average: the balance-weighted average, so topping up a long-held wallet
/// brings its age down in proportion (a few old tokens can't make a large new buy look old). An
/// empty or note-less holding starts at `now`.
pub fn held_since(since: i64, balance: u64, amount: u64, now: i64) -> i64 {
    if balance == 0 || since == 0 {
        return now;
    }
    let total = i128::from(balance) + i128::from(amount);
    let weighted = i128::from(balance) * i128::from(since.min(now)) + i128::from(amount) * i128::from(now);
    i64::try_from(weighted / total).unwrap_or(now)
}

/// The two rules, as a pure function of the wall and the token program's arguments.
pub fn check(wall: &Wall, args: &TokenHookArgs) -> Result<()> {
    let to = args.destination_owner;
    if wall.is_exempt(&to) {
        return Ok(());
    }
    require!(to.is_on_curve(), WallError::WalletsOnly);
    let after = u128::from(args.destination_balance) + u128::from(args.amount);
    require!(after <= u128::from(wall.hold_cap), WallError::OverTheWall);
    Ok(())
}

#[account]
#[derive(InitSpace)]
pub struct Wall {
    pub bump: u8,
    pub mint: Pubkey,
    pub hold_cap: u64,
    pub exempt: [Pubkey; MAX_EXEMPT],
}

impl Wall {
    /// Whether `owner` is one of the twin's own accounts.
    pub fn is_exempt(&self, owner: &Pubkey) -> bool {
        *owner != Pubkey::default() && self.exempt.contains(owner)
    }
}

#[derive(Accounts)]
pub struct Prepare<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: the twins program's authority (address- and signer-checked).
    #[account(signer, address = TWINS_AUTHORITY @ WallError::NotTwins)]
    pub twins_authority: UncheckedAccount<'info>,
    /// CHECK: the mint the wall is for (it may not exist yet).
    pub mint: UncheckedAccount<'info>,
    #[account(init, payer = payer, space = 8 + Wall::INIT_SPACE, seeds = [WALL_SEED, mint.key().as_ref()], bump)]
    pub wall: Account<'info, Wall>,
    /// CHECK: the hook's registry for the mint (address-checked in the handler).
    #[account(mut)]
    pub registry: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct BeforeTransfer<'info> {
    /// CHECK: the token program's signer for this hook (address- and signer-checked).
    #[account(signer, address = TOKEN_HOOK_SIGNER @ WallError::BadHookSigner)]
    pub hook_signer: UncheckedAccount<'info>,
    /// CHECK: the mint (bound to the wall by its seeds).
    pub mint: UncheckedAccount<'info>,
    /// CHECK: unused.
    pub source: UncheckedAccount<'info>,
    /// CHECK: unused.
    pub destination: UncheckedAccount<'info>,
    /// CHECK: unused.
    pub authority: UncheckedAccount<'info>,
    #[account(seeds = [WALL_SEED, mint.key().as_ref()], bump = wall.bump)]
    pub wall: Account<'info, Wall>,
}

/// The Wall's errors are the bridge's (one error list per program).
pub use crate::error::BridgeError as WallError;


#[cfg(test)]
mod tests {
    use super::*;

    fn args(to: Pubkey, balance: u64, amount: u64) -> TokenHookArgs {
        TokenHookArgs {
            op: TokenOp::Transfer,
            phase: tiber_hook::Phase::Before,
            mint: Pubkey::new_unique(),
            source: Pubkey::new_unique(),
            destination: Pubkey::new_unique(),
            source_owner: Pubkey::new_unique(),
            destination_owner: to,
            authority: Pubkey::new_unique(),
            authority_is_delegate: false,
            amount,
            delta: 0,
            source_balance: 0,
            destination_balance: balance,
            decimals: DECIMALS,
            supply: 1_000_000_000 * 1_000_000,
            source_hook_data: [0; 64],
            destination_hook_data: [0; 64],
        }
    }

    fn wallet() -> Pubkey {
        // A real ed25519 public key: on the curve.
        Pubkey::from_str_const("64TR6NsC7i7kCQ3YSmkeQiakLcgVDWXnw3YbZ1cCjFPL")
    }

    fn wall(exempt: Pubkey) -> Wall {
        Wall {
            bump: 255,
            mint: Pubkey::new_unique(),
            hold_cap: HOLD_CAP,
            exempt: [exempt, Pubkey::default(), Pubkey::default(), Pubkey::default()],
        }
    }

    #[test]
    fn the_addresses_match_their_derivations() {
        let signer = Pubkey::find_program_address(
            &[HOOK_AUTHORITY_SEED, crate::ID.as_ref()],
            &tiber_token::ID,
        )
        .0;
        assert_eq!(TOKEN_HOOK_SIGNER, signer);
        assert_eq!(
            TWINS_AUTHORITY,
            Pubkey::find_program_address(&[HOOK_AUTHORITY_SEED], &TWINS_ID).0
        );
    }

    #[test]
    fn a_wallet_may_fill_up_to_the_wall_and_no_further() {
        let w = wall(Pubkey::new_unique());
        assert!(check(&w, &args(wallet(), 0, HOLD_CAP)).is_ok());
        assert!(check(&w, &args(wallet(), HOLD_CAP - 1, 1)).is_ok());
        assert!(check(&w, &args(wallet(), HOLD_CAP, 1)).is_err());
        assert!(check(&w, &args(wallet(), 0, HOLD_CAP + 1)).is_err());
    }

    #[test]
    fn a_program_account_may_not_hold_a_twin_unless_exempt() {
        let pda = Pubkey::find_program_address(&[b"pool"], &Pubkey::new_unique()).0;
        assert!(!pda.is_on_curve());
        assert!(check(&wall(Pubkey::new_unique()), &args(pda, 0, 1)).is_err());
        // Exempt: no cap either.
        assert!(check(&wall(pda), &args(pda, 0, HOLD_CAP * 50)).is_ok());
    }

    #[test]
    fn the_arrival_time_is_a_balance_weighted_average() {
        let day = 86_400;
        // An empty wallet starts now.
        assert_eq!(held_since(0, 0, 100, 10 * day), 10 * day);
        // Doubling a 30-day-old position today makes it 15 days old.
        assert_eq!(held_since(day, 1_000, 1_000, 31 * day), 16 * day);
        // A tiny top-up barely moves it; a huge one makes it nearly new.
        assert!(held_since(day, 1_000_000, 1, 31 * day) <= day + 5);
        assert!(held_since(day, 1, 1_000_000, 31 * day) >= 31 * day - 5);
        // The note round-trips.
        let note = write_held_since(&[0; 64], 12_345);
        assert_eq!(read_held_since(&note), 12_345);
        assert_eq!(read_held_since(&[0; 64]), 0);
    }

    #[test]
    fn the_default_key_is_never_exempt() {
        let w = wall(Pubkey::default());
        assert!(!w.is_exempt(&Pubkey::default()));
    }
}
