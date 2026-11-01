// The Tiber client for the browser: addresses, instruction builders and account readers for
// Romulus and Remus. The same builders as the scripts' client (client/src/tiber.ts), with the
// interface files bundled and no signing of its own: the connected wallet signs.

import { AnchorProvider, Program, type Idl } from "@anchor-lang/core";
import BN from "bn.js";
import {
  ComputeBudgetProgram,
  Connection,
  PublicKey,
  SystemProgram,
  Transaction,
  type TransactionInstruction,
  type AccountMeta,
} from "@solana/web3.js";
import tokenIdl from "./idl/tiber_token.json";
import swapIdl from "./idl/tiber_swap.json";
import bridgeIdl from "./idl/tiber_bridge.json";
import twinsIdl from "./idl/tiber_twins.json";
import titheIdl from "./idl/tiber_tithe.json";

// ------------------------------------------------------------------------------------ programs

export const TOKEN_ID = new PublicKey("CFW2Z5J3dXwRXJzpxxfZiTc27KXuggDyzfZU7L6B6a8e");
export const SWAP_ID = new PublicKey("7nUWnHYtqgLv2sevNxxVBVqZSKd5VZqLBTWbf7un3kSM");
export const BRIDGE_ID = new PublicKey("4WXBYj7efT8SYhRxacekrp2zgmqoPnyGQifE6obRGLeG");
export const TWINS_ID = new PublicKey("e1UkVXrsWoQW97Yw8eCP1NJ3kcdqLYNgwQwidL7Lg89");
export const TITHE_ID = new PublicKey("5WNWnM3JTDgDYkA4Mh7quKQXtkdiwF3d7bgc9RuX4KF");
export const LOADER_V3 = new PublicKey("BPFLoaderUpgradeab1e11111111111111111111111");
export const NATIVE_MINT = new PublicKey("So11111111111111111111111111111111111111112");

// ------------------------------------------------------------------------------------ the numbers

export const ONE = 1_000_000n; // one whole twin token in base units (6 decimals)
export const LAMPORTS_PER_SOL = 1_000_000_000n;
export const SUPPLY = 1_000_000_000n * ONE;
export const MAX_BUY = 10_000_000n * ONE;
export const HOLD_CAP = 15_000_000n * ONE;
export const TREASURY_DAILY_LIMIT = 1_000_000n * ONE;
export const GRADUATION_QUOTE = 30n * LAMPORTS_PER_SOL;

// ------------------------------------------------------------------------------------ addresses

const enc = (s: string) => Buffer.from(s);
const pda = (seeds: (Buffer | Uint8Array)[], program: PublicKey) =>
  PublicKey.findProgramAddressSync(seeds, program)[0];

export const eventAuthority = (program: PublicKey) => pda([enc("__event_authority")], program);
export const programData = (program: PublicKey) => pda([program.toBuffer()], LOADER_V3);
export const holding = (mint: PublicKey, owner: PublicKey) =>
  pda([enc("holding"), mint.toBuffer(), owner.toBuffer()], TOKEN_ID);
export const hookAccounts = (hookProgram: PublicKey, key: PublicKey) =>
  pda([enc("tiber-hook-accounts"), key.toBuffer()], hookProgram);
/** The PDA `caller` signs every callback to `hookProgram` with. */
export const hookSigner = (caller: PublicKey, hookProgram: PublicKey) =>
  pda([enc("hook-authority"), hookProgram.toBuffer()], caller);
export const hookAuthority = (program: PublicKey) => pda([enc("hook-authority")], program);

export const BRIDGED_SOL = pda([enc("wrapped"), NATIVE_MINT.toBuffer()], BRIDGE_ID);
export const bridgeConfig = () => pda([enc("config")], BRIDGE_ID);
export const solWrapper = () => pda([enc("wrapper"), NATIVE_MINT.toBuffer()], BRIDGE_ID);
export const solVault = () => pda([enc("sol-vault")], BRIDGE_ID);
export const dexConfig = () => pda([enc("config")], SWAP_ID);
export const twinsConfig = () => pda([enc("config")], TWINS_ID);
export const twinAddress = (mint: PublicKey) => pda([enc("twin"), mint.toBuffer()], TWINS_ID);
export const wallAddress = (mint: PublicKey) => pda([enc("wall"), mint.toBuffer()], BRIDGE_ID);
export const buyerAddress = (mint: PublicKey) => pda([enc("buyer"), mint.toBuffer()], TITHE_ID);
export const titheState = (mint: PublicKey) => pda([enc("state"), mint.toBuffer()], TITHE_ID);
/** A twin's pool: base the twin, quote bridged SOL, no LP fee, the twins program as hook. */
export const poolAddress = (mint: PublicKey) => {
  const fee = Buffer.alloc(2); // LP fee 0, little-endian u16
  return pda([enc("pool"), mint.toBuffer(), BRIDGED_SOL.toBuffer(), fee, TWINS_ID.toBuffer()], SWAP_ID);
};
export const lpMint = (pool: PublicKey) => pda([enc("lp"), pool.toBuffer()], SWAP_ID);
export const vault = (pool: PublicKey, mint: PublicKey) => holding(mint, pool);
/** Where a twin's buy tax goes: its sister's buyer's bridged-SOL holding. */
export const fundOf = (mint: PublicKey) => holding(BRIDGED_SOL, buyerAddress(mint));

const TWINS_AUTHORITY = hookAuthority(TWINS_ID);
const WALL_TOKEN_SIGNER = hookSigner(TOKEN_ID, BRIDGE_ID);
const DEX_HOOK_SIGNER = hookSigner(SWAP_ID, TWINS_ID);

// ------------------------------------------------------------------------------------ the client


/** Retries `f` when the RPC server answers 429 (too many requests): 1, 2, then 4 seconds later. */
export async function retryBusy<T>(f: () => Promise<T>): Promise<T> {
  for (let i = 0; ; i++) {
    try {
      return await f();
    } catch (e: any) {
      if (i >= 3 || !/429|Too many requests|rate limit/i.test(String(e?.message ?? e))) throw e;
      await new Promise((r) => setTimeout(r, 1000 * 2 ** i));
    }
  }
}

/** A wallet as Anchor needs it to build instructions (the site's wallet signs, not this). */
const readOnlyWallet = (key: PublicKey) => ({
  publicKey: key,
  signTransaction: async <T,>(tx: T) => tx,
  signAllTransactions: async <T,>(txs: T[]) => txs,
});

export class Tiber {
  readonly provider: AnchorProvider;
  readonly token: Program;
  readonly swap: Program;
  readonly bridge: Program;
  readonly twins: Program;
  readonly tithe: Program;

  constructor(readonly connection: Connection, who: PublicKey = PublicKey.default) {
    this.provider = new AnchorProvider(connection, readOnlyWallet(who) as any, { commitment: "confirmed" });
    this.token = new Program(tokenIdl as Idl, this.provider);
    this.swap = new Program(swapIdl as Idl, this.provider);
    this.bridge = new Program(bridgeIdl as Idl, this.provider);
    this.twins = new Program(twinsIdl as Idl, this.provider);
    this.tithe = new Program(titheIdl as Idl, this.provider);
  }

  /** A transaction of `ixs` with a compute limit, ready for the wallet to sign and send. */
  async transaction(payer: PublicKey, ixs: TransactionInstruction[], units = 400_000): Promise<Transaction> {
    const tx = new Transaction().add(ComputeBudgetProgram.setComputeUnitLimit({ units }), ...ixs);
    tx.feePayer = payer;
    tx.recentBlockhash = (await retryBusy(() => this.connection.getLatestBlockhash("confirmed"))).blockhash;
    return tx;
  }

  // ---------------------------------------------------------------------------- readers

  async balance(mint: PublicKey, owner: PublicKey): Promise<bigint> {
    const info = await this.connection.getAccountInfo(holding(mint, owner), "confirmed");
    if (!info) return 0n;
    return info.data.readBigUInt64LE(8 + 1 + 1 + 32 + 32);
  }
  async mint(mint: PublicKey): Promise<any> {
    return (this.token.account as any).mint.fetch(mint);
  }
  async pool(mint: PublicKey): Promise<any> {
    return (this.swap.account as any).pool.fetch(poolAddress(mint));
  }
  async twin(mint: PublicKey): Promise<any> {
    return (this.twins.account as any).twin.fetch(twinAddress(mint));
  }
  async titheRecord(mint: PublicKey): Promise<any> {
    return (this.tithe.account as any).titheState.fetch(titheState(mint));
  }

  // ---------------------------------------------------------------------------- setup (admin)

  /** The exchange's config: no protocol fee share on the twins' pools. */
  dexInitConfig(admin: PublicKey, treasury: PublicKey): Promise<TransactionInstruction> {
    return this.swap.methods
      .initConfig({
        admin,
        protocolFeeBps: 0,
        feeCollector: admin,
        treasury,
        poolCreationFeeLamports: new BN(0),
        paused: false,
        launchProtocolShareBps: 0,
      })
      .accountsStrict({
        authority: admin,
        config: dexConfig(),
        programData: programData(SWAP_ID),
        systemProgram: SystemProgram.programId,
        eventAuthority: eventAuthority(SWAP_ID),
        program: SWAP_ID,
      })
      .instruction();
  }

  bridgeInitConfig(admin: PublicKey): Promise<TransactionInstruction> {
    return this.bridge.methods
      .initConfig({ admin, paused: false })
      .accountsStrict({
        authority: admin,
        config: bridgeConfig(),
        programData: programData(BRIDGE_ID),
        systemProgram: SystemProgram.programId,
        eventAuthority: eventAuthority(BRIDGE_ID),
        program: BRIDGE_ID,
      })
      .instruction();
  }

  registerSol(payer: PublicKey): Promise<TransactionInstruction> {
    return this.bridge.methods
      .registerSol()
      .accountsStrict({
        payer,
        config: bridgeConfig(),
        wrapper: solWrapper(),
        wrappedMint: BRIDGED_SOL,
        solVault: solVault(),
        tokenProgram: TOKEN_ID,
        tokenEventAuthority: eventAuthority(TOKEN_ID),
        systemProgram: SystemProgram.programId,
        eventAuthority: eventAuthority(BRIDGE_ID),
        program: BRIDGE_ID,
      })
      .instruction();
  }

  twinsInitConfig(authority: PublicKey, admin: PublicKey): Promise<TransactionInstruction> {
    return this.twins.methods
      .initConfig(admin)
      .accountsStrict({
        authority,
        config: twinsConfig(),
        programData: programData(TWINS_ID),
        systemProgram: SystemProgram.programId,
      })
      .instruction();
  }

  createTwin(admin: PublicKey, mint: PublicKey, name: string, symbol: string, uri: string): Promise<TransactionInstruction> {
    const twin = twinAddress(mint);
    return this.twins.methods
      .createTwin({ name, symbol, uri })
      .accountsStrict({
        admin,
        config: twinsConfig(),
        mint,
        twin,
        twinHolding: holding(mint, twin),
        twinsAuthority: TWINS_AUTHORITY,
        wallProgram: BRIDGE_ID,
        wall: wallAddress(mint),
        wallRegistry: hookAccounts(BRIDGE_ID, mint),
        wallTokenSigner: WALL_TOKEN_SIGNER,
        tokenProgram: TOKEN_ID,
        tokenEventAuthority: eventAuthority(TOKEN_ID),
        systemProgram: SystemProgram.programId,
      })
      .instruction();
  }

  openCurve(admin: PublicKey, mint: PublicKey, dexTreasury: PublicKey): Promise<TransactionInstruction> {
    const twin = twinAddress(mint);
    const pool = poolAddress(mint);
    const lp = lpMint(pool);
    return this.twins.methods
      .openCurve()
      .accountsStrict({
        admin,
        config: twinsConfig(),
        twin,
        mint,
        quoteMint: BRIDGED_SOL,
        pool,
        lpMint: lp,
        baseVault: vault(pool, mint),
        quoteVault: vault(pool, BRIDGED_SOL),
        twinHolding: holding(mint, twin),
        twinQuote: holding(BRIDGED_SOL, twin),
        twinLp: holding(lp, twin),
        dexConfig: dexConfig(),
        dexTreasury,
        twinsAuthority: TWINS_AUTHORITY,
        dexHookSigner: DEX_HOOK_SIGNER,
        wallProgram: BRIDGE_ID,
        wallTokenSigner: WALL_TOKEN_SIGNER,
        wall: wallAddress(mint),
        tokenProgram: TOKEN_ID,
        tokenEventAuthority: eventAuthority(TOKEN_ID),
        swapProgram: SWAP_ID,
        dexEventAuthority: eventAuthority(SWAP_ID),
        twinsProgram: TWINS_ID,
        systemProgram: SystemProgram.programId,
      })
      .instruction();
  }

  linkTwins(admin: PublicKey, a: PublicKey, b: PublicKey): Promise<TransactionInstruction> {
    return this.twins.methods
      .linkTwins()
      .accountsStrict({
        admin,
        config: twinsConfig(),
        twinA: twinAddress(a),
        twinB: twinAddress(b),
        quoteMint: BRIDGED_SOL,
        fundA: fundOf(a),
        fundB: fundOf(b),
        buyerA: buyerAddress(a),
        buyerB: buyerAddress(b),
        registryA: hookAccounts(TWINS_ID, poolAddress(a)),
        registryB: hookAccounts(TWINS_ID, poolAddress(b)),
        tokenProgram: TOKEN_ID,
        tokenEventAuthority: eventAuthority(TOKEN_ID),
        systemProgram: SystemProgram.programId,
      })
      .instruction();
  }

  openTrading(admin: PublicKey, a: PublicKey, b: PublicKey): Promise<TransactionInstruction> {
    return this.twins.methods
      .openTrading()
      .accountsStrict({ admin, config: twinsConfig(), twinA: twinAddress(a), twinB: twinAddress(b) })
      .instruction();
  }

  initBuyer(payer: PublicKey, mint: PublicKey): Promise<TransactionInstruction> {
    const buyer = buyerAddress(mint);
    return this.tithe.methods
      .initBuyer()
      .accountsStrict({
        payer,
        twin: twinAddress(mint),
        mint,
        buyer,
        buyerHolding: holding(mint, buyer),
        state: titheState(mint),
        tokenProgram: TOKEN_ID,
        tokenEventAuthority: eventAuthority(TOKEN_ID),
        systemProgram: SystemProgram.programId,
      })
      .instruction();
  }

  releaseTreasury(admin: PublicKey, mint: PublicKey, to: PublicKey, amount: bigint): Promise<TransactionInstruction> {
    const twin = twinAddress(mint);
    return this.twins.methods
      .releaseTreasury(new BN(amount.toString()))
      .accountsStrict({
        admin,
        config: twinsConfig(),
        twin,
        mint,
        twinHolding: holding(mint, twin),
        destination: holding(mint, to),
        wallProgram: BRIDGE_ID,
        wallTokenSigner: WALL_TOKEN_SIGNER,
        wall: wallAddress(mint),
        tokenProgram: TOKEN_ID,
        tokenEventAuthority: eventAuthority(TOKEN_ID),
      })
      .instruction();
  }

  // ---------------------------------------------------------------------------- anyone

  createHolding(payer: PublicKey, mint: PublicKey, owner: PublicKey): Promise<TransactionInstruction> {
    return this.token.methods
      .createHolding()
      .accountsStrict({
        payer,
        mint,
        owner,
        holding: holding(mint, owner),
        systemProgram: SystemProgram.programId,
        eventAuthority: eventAuthority(TOKEN_ID),
        program: TOKEN_ID,
      })
      .instruction();
  }

  private wrapAccounts(user: PublicKey) {
    return {
      user,
      config: bridgeConfig(),
      wrapper: solWrapper(),
      solVault: solVault(),
      wrappedMint: BRIDGED_SOL,
      userWrapped: holding(BRIDGED_SOL, user),
      tokenProgram: TOKEN_ID,
      tokenEventAuthority: eventAuthority(TOKEN_ID),
      systemProgram: SystemProgram.programId,
      eventAuthority: eventAuthority(BRIDGE_ID),
      program: BRIDGE_ID,
    };
  }
  wrapSol(user: PublicKey, lamports: bigint): Promise<TransactionInstruction> {
    return this.bridge.methods.wrapSol(new BN(lamports.toString())).accountsStrict(this.wrapAccounts(user)).instruction();
  }
  /** Unwraps everything (u64 max). */
  unwrapAllSol(user: PublicKey): Promise<TransactionInstruction> {
    return this.bridge.methods
      .unwrapSol(new BN("18446744073709551615"))
      .accountsStrict(this.wrapAccounts(user))
      .instruction();
  }

  /** The wall as a twin's token-hook slice: the bridge, the token program's signer for it, the wall. */
  private wallSlice(mint: PublicKey): AccountMeta[] {
    return [
      { pubkey: BRIDGE_ID, isSigner: false, isWritable: false },
      { pubkey: WALL_TOKEN_SIGNER, isSigner: false, isWritable: false },
      { pubkey: wallAddress(mint), isSigner: false, isWritable: false },
    ];
  }

  /** A buy (`direction` 1, `amountIn` bridged SOL) or sell (0, `amountIn` tokens) of a twin. */
  async swapIx(trader: PublicKey, mint: PublicKey, sister: PublicKey, direction: 0 | 1, amountIn: bigint, minOut = 0n): Promise<TransactionInstruction> {
    const pool = poolAddress(mint);
    const buy = direction === 1;
    const extras: AccountMeta[] = [
      ...this.wallSlice(mint), // the twin's side: the input on a sell, the output on a buy
      { pubkey: twinAddress(mint), isSigner: false, isWritable: true },
      { pubkey: fundOf(sister), isSigner: false, isWritable: true },
      // The trader's holding of the twin: the loyalty burn reads when its tokens arrived.
      { pubkey: holding(mint, trader), isSigner: false, isWritable: false },
    ];
    return this.swap.methods
      .swap({
        direction,
        amountIn: new BN(amountIn.toString()),
        minAmountOut: new BN(minOut.toString()),
        inHookAccounts: buy ? 0 : 3,
        outHookAccounts: buy ? 3 : 0,
        hookData: Buffer.alloc(0),
      })
      .accountsStrict({
        trader,
        config: dexConfig(),
        pool,
        baseMint: mint,
        quoteMint: BRIDGED_SOL,
        baseVault: vault(pool, mint),
        quoteVault: vault(pool, BRIDGED_SOL),
        traderBase: holding(mint, trader),
        traderQuote: holding(BRIDGED_SOL, trader),
        hookProgram: TWINS_ID,
        hookSigner: DEX_HOOK_SIGNER,
        tokenProgram: TOKEN_ID,
        tokenEventAuthority: eventAuthority(TOKEN_ID),
        eventAuthority: eventAuthority(SWAP_ID),
        program: SWAP_ID,
      })
      .remainingAccounts(extras)
      .preInstructions([])
      .instruction()
      .then((ix) => {
        // A sell burns 2% of the tokens: the mint must be writable.
        if (!buy) ix.keys[3].isWritable = true;
        return ix;
      });
  }

  /** Unwraps everything above `keep` of the user's bridged SOL: what a trade just delivered. */
  unwrapSolAbove(user: PublicKey, keep: bigint): Promise<TransactionInstruction> {
    return this.bridge.methods
      .unwrapSolAbove(new BN(keep.toString()))
      .accountsStrict(this.wrapAccounts(user))
      .instruction();
  }

  /** A transfer of a twin between wallets (the wall checks the destination). */
  transferTwin(from: PublicKey, mint: PublicKey, to: PublicKey, amount: bigint): Promise<TransactionInstruction> {
    return this.token.methods
      .transfer(new BN(amount.toString()))
      .accountsStrict({
        authority: from,
        source: holding(mint, from),
        destination: holding(mint, to),
        mint,
        hookProgram: BRIDGE_ID,
        hookSigner: WALL_TOKEN_SIGNER,
        eventAuthority: eventAuthority(TOKEN_ID),
        program: TOKEN_ID,
      })
      .remainingAccounts([{ pubkey: wallAddress(mint), isSigner: false, isWritable: false }])
      .instruction();
  }

  /** The tithe of `mint`: spends the fund its sister's buys filled, buying `mint`, and burns it. */
  async titheIx(cranker: PublicKey, mint: PublicKey, sister: PublicKey): Promise<TransactionInstruction> {
    const pool = poolAddress(mint);
    const buyer = buyerAddress(mint);
    return this.tithe.methods
      .tithe()
      .accountsStrict({
        cranker,
        twin: twinAddress(mint),
        state: titheState(mint),
        mint,
        buyer,
        fund: holding(BRIDGED_SOL, buyer),
        buyerHolding: holding(mint, buyer),
        quoteMint: BRIDGED_SOL,
        pool,
        baseVault: vault(pool, mint),
        quoteVault: vault(pool, BRIDGED_SOL),
        sisterFund: fundOf(sister),
        dexConfig: dexConfig(),
        twinsProgram: TWINS_ID,
        dexHookSigner: DEX_HOOK_SIGNER,
        wallProgram: BRIDGE_ID,
        wallTokenSigner: WALL_TOKEN_SIGNER,
        wall: wallAddress(mint),
        tokenProgram: TOKEN_ID,
        tokenEventAuthority: eventAuthority(TOKEN_ID),
        swapProgram: SWAP_ID,
        dexEventAuthority: eventAuthority(SWAP_ID),
      })
      .instruction();
  }

  graduateIx(cranker: PublicKey, mint: PublicKey): Promise<TransactionInstruction> {
    const twin = twinAddress(mint);
    const pool = poolAddress(mint);
    const lp = lpMint(pool);
    return this.twins.methods
      .graduate()
      .accountsStrict({
        cranker,
        twin,
        mint,
        pool,
        twinHolding: holding(mint, twin),
        baseVault: vault(pool, mint),
        quoteVault: vault(pool, BRIDGED_SOL),
        lpMint: lp,
        twinLp: holding(lp, twin),
        twinsAuthority: TWINS_AUTHORITY,
        wallProgram: BRIDGE_ID,
        wallTokenSigner: WALL_TOKEN_SIGNER,
        wall: wallAddress(mint),
        tokenProgram: TOKEN_ID,
        tokenEventAuthority: eventAuthority(TOKEN_ID),
        swapProgram: SWAP_ID,
        dexEventAuthority: eventAuthority(SWAP_ID),
      })
      .instruction();
  }
}

