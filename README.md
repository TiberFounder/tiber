# Tiber programs

The on-chain programs behind Romulus (ROM) and Remus (REM), the Twins of the Tiber, on Solana.

| Program | Devnet address | What it does |
| --- | --- | --- |
| `tiber_token` | `CFW2Z5J3dXwRXJzpxxfZiTc27KXuggDyzfZU7L6B6a8e` | The token standard: mints, holdings, and hooks that run on every transfer, mint and burn |
| `tiber_swap` | `7nUWnHYtqgLv2sevNxxVBVqZSKd5VZqLBTWbf7un3kSM` | The exchange: bonding curves and constant-product pools with hooks |
| `tiber_bridge` | `4WXBYj7efT8SYhRxacekrp2zgmqoPnyGQifE6obRGLeG` | Moves SOL in and out of the token standard, one for one; also carries the Wall, the twins' token hook (the 1.5% holding cap, and only ordinary wallets may hold a twin) |
| `tax_hook` | `4ZvQ5M27GKv3jmH7ksR6j1r6bnYeeqqckthn1sAHFG9F` | An example token hook, kept for the tests |
| `tiber_twins` | `e1UkVXrsWoQW97Yw8eCP1NJ3kcdqLYNgwQwidL7Lg89` | Creates ROM and REM, runs their bonding curves (buy tax, sell burn, anti-sniper fee, 1% buy limit), graduates them with locked liquidity, releases the treasury on its schedule |
| `tiber_tithe` | `5WNWnM3JTDgDYkA4Mh7quKQXtkdiwF3d7bgc9RuX4KF` | Spends each brother fund buying the sister token in small steps anyone can trigger, and burns what it buys |

`hook_tester` is a test-only program and is never deployed.

## Build and test

```sh
bash scripts/solana/toolchain.sh install
export PATH="$(bash scripts/solana/toolchain.sh path):$PATH"
bash scripts/solana/programs.sh build
cargo test --workspace
```

## Mainnet launch checklist

Steps outside the code that the website's promises depend on:

1. Fresh keys for everything (program keypairs, deployer, admin), made and kept off any shared
   machine and not funded straight from an exchange account in anyone's name.
2. Before the twins' pools are created, the exchange config must have
   `launch_protocol_share_bps: 0` (a pool keeps the share it was created with for life: anything
   above 0 sends part of every buy tax to the fee collector instead of the tithe) and
   `paused: false`.
3. Right after deploying, move the upgrade authority of all five programs to a multisig (for
   example Squads) of fresh keys that needs at least two of them to agree. The website says so.
4. After the first weeks, once the twins have traded without trouble, remove upgrades for good
   (`solana program set-upgrade-authority <program> --final`, through the multisig) and give up
   each twin's metadata authority (`set_authority`, kind `Metadata`, to none).

## License

Apache License 2.0; see `LICENSE` and `NOTICE`.
