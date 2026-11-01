//! Ready-made setups: configured programs, bridged SOL, holdings.

use anchor_lang::prelude::Pubkey;
use tiber_bridge::client as bridge;
use tiber_core::policy;
use tiber_swap::client as swap;
use tiber_token::client as token;
use solana_keypair::Keypair;
use solana_signer::Signer;

use crate::env::{Env, Tx};

/// The configured world: every config created, SOL registered on the bridge.
pub struct World {
    /// The environment.
    pub env: Env,
    /// Bridged SOL's mint.
    pub sol: Pubkey,
}

impl World {
    /// Configures everything with the policy defaults.
    pub fn new() -> Self {
        let mut env = Env::new();
        let deployer = env.deployer.insecure_clone();
        let admin = deployer.pubkey();
        let treasury = env.treasury.pubkey();
        let sol = bridge::wrapped_mint_address(&tiber_bridge::constants::NATIVE_MINT);
        let ixs = [
            swap::init_config(
                admin,
                tiber_swap::instructions::ConfigArgs {
                    admin,
                    protocol_fee_bps: policy::PROTOCOL_FEE_BPS,
                    launch_protocol_share_bps: policy::LAUNCH_PROTOCOL_SHARE_BPS,
                    fee_collector: admin,
                    treasury,
                    pool_creation_fee_lamports: 0,
                    paused: false,
                },
            ),
            bridge::init_config(
                admin,
                tiber_bridge::instructions::ConfigArgs {
                    admin,
                    paused: false,
                },
            ),
            bridge::register_sol(admin),
        ];
        env.send(&ixs, &[&deployer]).ok();
        Self { env, sol }
    }

    /// A funded wallet holding `lamports` of bridged SOL (and as much SOL again for fees).
    pub fn wallet_with_sol(&mut self, lamports: u64) -> Keypair {
        let kp = self.env.funded(lamports * 2 + 1_000_000_000);
        self.wrap_sol(&kp, lamports).ok();
        kp
    }

    /// Wraps `lamports` of a wallet's SOL.
    pub fn wrap_sol(&mut self, wallet: &Keypair, lamports: u64) -> Tx {
        let ixs = [
            token::create_holding(wallet.pubkey(), self.sol, wallet.pubkey()),
            bridge::wrap_sol(wallet.pubkey(), lamports),
        ];
        self.env.send_paid_by(&ixs, wallet, &[])
    }

    /// Creates a hook-less BTS mint with `supply` minted to `owner`.
    pub fn mint_to_owner(
        &mut self,
        owner: &Keypair,
        decimals: u8,
        supply: u64,
        symbol: &str,
    ) -> Pubkey {
        let mint = Keypair::new();
        let args = tiber_token::instructions::CreateMintArgs {
            decimals,
            name: format!("Test {symbol}"),
            symbol: symbol.to_string(),
            uri: String::new(),
            max_supply: 0,
            mint_authority: Some(owner.pubkey()),
            freeze_authority: Some(owner.pubkey()),
            hook_program: None,
            hook_flags: 0,
            hook_authority: Some(owner.pubkey()),
            metadata_authority: Some(owner.pubkey()),
        };
        let ixs = [
            token::create_mint(owner.pubkey(), mint.pubkey(), args),
            token::create_holding(owner.pubkey(), mint.pubkey(), owner.pubkey()),
            token::mint_to(
                owner.pubkey(),
                mint.pubkey(),
                token::holding_address(&mint.pubkey(), &owner.pubkey()),
                None,
                vec![],
                supply,
            ),
        ];
        self.env.send_paid_by(&ixs, owner, &[&mint]).ok();
        mint.pubkey()
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}
