// Runs the launch for real on a cluster and checks every rule with real transactions. Run after
// `setup.ts ... --open`, within a few seconds of opening (the first check is the anti-sniper fee).
//
//   npx tsx scripts/scenario.ts <rpc url> <deployer keypair> [--graduate]
//
// Test wallets are funded from the deployer. --graduate also buys Romulus's whole curve from many
// wallets and graduates it: that needs about 35 SOL, so it is meant for a local validator.

import { Connection, Keypair, PublicKey, SystemProgram, type TransactionInstruction } from "@solana/web3.js";
import { readFileSync } from "node:fs";
import {
  BRIDGED_SOL,
  HOLD_CAP,
  LAMPORTS_PER_SOL,
  MAX_BUY,
  TREASURY_DAILY_LIMIT,
  Tiber,
  buyerAddress,
  fundOf,
  holding,
  loadKeypair,
  twinAddress,
  twinsConfig,
} from "../src/tiber.ts";

const [url, deployerPath, ...flags] = process.argv.slice(2);
const connection = new Connection(url, "confirmed");
const admin = loadKeypair(deployerPath);
const t = new Tiber(connection, admin);
const cluster = url.includes("devnet") ? "devnet" : url.includes("mainnet") ? "mainnet" : "local";
const saved = JSON.parse(readFileSync(new URL(`../deployments/${cluster}.json`, import.meta.url), "utf8"));
const ROM = new PublicKey(saved.addresses.rom);
const REM = new PublicKey(saved.addresses.rem);
const SOL = LAMPORTS_PER_SOL;

let passed = 0;
let failed = 0;
function check(name: string, ok: boolean, detail = "") {
  if (ok) passed++;
  else failed++;
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? `  (${detail})` : ""}`);
}
async function fails(ixs: TransactionInstruction[], signers: Keypair[], payer: Keypair, expect: string): Promise<[boolean, string]> {
  try {
    await t.send(ixs, signers, payer);
    return [false, "it went through"];
  } catch (e: any) {
    const logs: string[] = e?.transactionLogs ?? e?.logs ?? [];
    const text = `${e?.message ?? e} ${logs.join(" ")}`;
    const m = text.match(/Error Code: (\w+)/);
    return [text.includes(expect), m ? m[1] : text.slice(0, 160)];
  }
}

/** `read()` once two reads 2 seconds apart agree: the public devnet RPC answers from several
 * nodes, and under load one can be a transaction or two behind the one that just confirmed. */
async function settled<T>(read: () => Promise<T>): Promise<T> {
  let last = await read();
  for (let i = 0; i < 10; i++) {
    await new Promise((r) => setTimeout(r, 2000));
    const next = await read();
    if (next === last) return next;
    last = next;
  }
  return last;
}

/** A wallet with `sol` of SOL, `wrapped` of it bridged, and empty holdings of both twins. */
async function wallet(sol: bigint, wrapped: bigint): Promise<Keypair> {
  const kp = Keypair.generate();
  await t.send([SystemProgram.transfer({ fromPubkey: admin.publicKey, toPubkey: kp.publicKey, lamports: Number(sol) })]);
  const ixs = [
    await t.createHolding(kp.publicKey, BRIDGED_SOL, kp.publicKey),
    await t.createHolding(kp.publicKey, ROM, kp.publicKey),
    await t.createHolding(kp.publicKey, REM, kp.publicKey),
  ];
  if (wrapped > 0n) ixs.push(await t.wrapSol(kp.publicKey, wrapped));
  await t.send(ixs, [], kp);
  return kp;
}
const buy = async (w: Keypair, mint: PublicKey, sister: PublicKey, lamports: bigint) =>
  t.send([await t.swapIx(w.publicKey, mint, sister, 1, lamports)], [], w);
const sell = async (w: Keypair, mint: PublicKey, sister: PublicKey, amount: bigint) =>
  t.send([await t.swapIx(w.publicKey, mint, sister, 0, amount)], [], w);
const fundFor = (mint: PublicKey) => t.balance(BRIDGED_SOL, buyerAddress(mint));
const supply = async (mint: PublicKey) => BigInt((await t.mint(mint)).supply.toString());
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

console.log(`cluster ${cluster}`);
const opened = (await t.twin(ROM)).openedAt.toNumber();
if (opened === 0) throw new Error("trading is not open: run setup.ts with --open first");

// 1. The anti-sniper fee, if we are still inside the first 30 seconds.
const sniper = await wallet((SOL * 3n) / 10n, SOL / 10n);
const before = await fundFor(REM);
await buy(sniper, ROM, REM, SOL / 20n);
const paid = (await fundFor(REM)) - before;
const age = Math.floor(Date.now() / 1000) - opened;
const rate = Number((paid * 10_000n) / (SOL / 20n)) / 100;
if (age < 25) check("a buy in the first seconds pays the anti-sniper fee", rate > 2.5, `${rate}% at ${age}s after opening`);
else console.log(`skip  anti-sniper check (trading opened ${age}s ago; the fee is ${rate}% now)`);

// Wait out the window.
while (Math.floor(Date.now() / 1000) - opened < 31) await sleep(1000);

// 2. The 2% buy tax goes to the sister's buyer.
const alice = await wallet((SOL * 8n) / 10n, (SOL * 6n) / 10n);
let f0 = await fundFor(REM);
await buy(alice, ROM, REM, SOL / 10n);
check("a ROM buy pays exactly 2% toward REM", (await fundFor(REM)) - f0 === (SOL / 10n) * 2n / 100n);
f0 = await fundFor(ROM);
await buy(alice, REM, ROM, SOL / 10n);
check("a REM buy pays exactly 2% toward ROM", (await fundFor(ROM)) - f0 === (SOL / 10n) * 2n / 100n);

// 3. The 1% buy limit.
let [ok, why] = await fails([await t.swapIx(alice.publicKey, ROM, REM, 1, (SOL * 35n) / 100n)], [], alice, "BuyTooLarge");
check("one buy cannot take more than 1% of the supply", ok, why);

// 4. The 1.5% holding cap.
const whale = await wallet((SOL * 8n) / 10n, (SOL * 6n) / 10n);
let held = 0n;
for (let i = 0; i < 40; i++) {
  try {
    await buy(whale, ROM, REM, SOL / 20n);
  } catch (e: any) {
    const text = `${e?.message ?? e} ${(e?.transactionLogs ?? e?.logs ?? []).join(" ")}`;
    check("a wallet cannot buy past 1.5% of the supply", text.includes("OverTheWall"), text.match(/Error Code: (\w+)/)?.[1] ?? "");
    break;
  }
}
held = await settled(() => t.balance(ROM, whale.publicKey));
check("and holds no more than 15,000,000", held <= HOLD_CAP && held > (HOLD_CAP * 9n) / 10n, `${held / 1_000_000n} ROM`);
const aliceRom = await t.balance(ROM, alice.publicKey);
[ok, why] = await fails([await t.transferTwin(alice.publicKey, ROM, whale.publicKey, aliceRom)], [], alice, "OverTheWall");
check("nor can someone send it more", ok, why);

// 5. Only wallets may hold a twin.
const program = twinsConfig();
await t.send([await t.createHolding(alice.publicKey, ROM, program)], [], alice);
[ok, why] = await fails([await t.transferTwin(alice.publicKey, ROM, program, 1000n)], [], alice, "WalletsOnly");
check("a program account cannot receive a twin", ok, why);

// 6. The loyalty burn: a sell within a day of buying burns 5% (2% after a day, 1% after 30 days, proven in the program tests).
const holdingNote = async (owner: PublicKey, mint: PublicKey) => {
  const info = await connection.getAccountInfo(holding(mint, owner), "confirmed");
  const note: number[] = (t.token.coder.accounts.decode("holding", info!.data) as { hookData: number[] }).hookData;
  return { since: Number(Buffer.from(note.slice(0, 8)).readBigInt64LE()), version: note[8] };
};
const note = await holdingNote(alice.publicKey, ROM);
check("the Wall notes when a holder's tokens arrived", note.version === 1 && Math.abs(Date.now() / 1000 - note.since) < 600, `${Math.round(Date.now() / 1000 - note.since)}s ago`);
const s0 = await settled(() => supply(ROM));
const sold = aliceRom / 2n;
await sell(alice, ROM, REM, sold);
check("a sell within a day of buying burns exactly 5%", s0 - (await settled(() => supply(ROM))) === (sold * 5n) / 100n);

// 7. The tithe.
const fund = await fundFor(REM);
const remSupply = await settled(() => supply(REM));
const romFund = await fundFor(ROM);
// The tithe's record keeps a running total, so a pair tithed before (an earlier run) counts from
// what it held before this tithe.
const burnedBefore = await t.titheRecord(REM).then((r: any) => BigInt(r.burned.toString())).catch(() => 0n);
await t.send([await t.titheIx(admin.publicKey, REM, ROM)]);
const rec = await t.titheRecord(REM);
const burned = BigInt(rec.burned.toString()) - burnedBefore;
check("the tithe spends the fund buying REM", (await fundFor(REM)) < fund, `${Number(fund - (await fundFor(REM))) / 1e9} SOL spent`);
check("and burns every REM it bought", remSupply - (await settled(() => supply(REM))) === burned && burned > 0n, `${burned / 1_000_000n} REM burned`);
check("and pays no tax itself", (await fundFor(ROM)) === romFund);
check("and keeps nothing", (await t.balance(REM, buyerAddress(REM))) === 0n);
[ok, why] = await fails([await t.titheIx(admin.publicKey, ROM, REM), await t.titheIx(admin.publicKey, ROM, REM)], [], admin, "OncePerSlot");
check("two tithes cannot run in one transaction", ok, why);

// 8. The treasury.
const me = admin.publicKey;
if ((await connection.getAccountInfo(t.token.programId)) && (await t.balance(ROM, me)) === 0n) {
  try {
    await t.send([await t.createHolding(me, ROM, me)]);
  } catch {}
}
const released = BigInt((await t.twin(ROM)).treasuryReleased.toString());
if (released === 0n) {
  await t.send([await t.releaseTreasury(me, ROM, me, TREASURY_DAILY_LIMIT)]);
  check("the treasury releases 1,000,000 a day", (await t.balance(ROM, me)) === TREASURY_DAILY_LIMIT);
}
[ok, why] = await fails([await t.releaseTreasury(me, ROM, me, 1n)], [], admin, "DailyLimit");
check("and no more on the same day", ok, why);
const stranger = await wallet(SOL / 20n, 0n);
[ok, why] = await fails([await t.releaseTreasury(stranger.publicKey, ROM, stranger.publicKey, 1n)], [], stranger, "NotAdmin");
check("and only the admin can release it", ok, why);

// 9. Graduation (local only: about 35 SOL).
if (flags.includes("--graduate")) {
  let wallets = 0;
  for (;;) {
    const p = await t.pool(ROM);
    const base = BigInt(p.baseReserve.toString());
    if (base === 0n) break;
    const x = BigInt(p.quoteReserve.toString()) + BigInt(p.virtualQuote.toString());
    const y = base + BigInt(p.virtualBase.toString());
    const want = 8_000_000n * 1_000_000n;
    const cost = want >= y ? x : (x * want) / (y - want);
    const w = await wallet(cost + SOL / 2n, (cost * 100n) / 98n + 1n);
    await buy(w, ROM, REM, (cost * 100n) / 98n);
    wallets++;
    if (wallets > 150) throw new Error("the curve did not sell out");
  }
  const p = await t.pool(ROM);
  check("Romulus's curve sold out", BigInt(p.baseReserve.toString()) === 0n, `${wallets} wallets, ${Number(p.quoteReserve.toString()) / 1e9} SOL raised`);
  await t.send([await t.graduateIx(me, ROM)]);
  const after = await t.pool(ROM);
  const tw = await t.twin(ROM);
  check("and graduated into an ordinary pool", tw.graduated && !after.curve);
  const lp = await t.balance(after.lpMint, twinAddress(ROM));
  check("with its liquidity locked with the twin", lp > 0n, `${lp} LP`);
  const late = await wallet(2n * SOL, SOL);
  const g0 = await fundFor(REM);
  await buy(late, ROM, REM, SOL / 10n);
  check("trading after graduation still pays the 2% tax", (await fundFor(REM)) - g0 === (SOL / 10n) * 2n / 100n);
}

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);
