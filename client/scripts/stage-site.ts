// Fills a local chain for trying the website: many buyers on each curve (so chronicle chapters
// open), and several keepers lighting the tithe. Local only.
import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { readFileSync } from "node:fs";
import { BRIDGED_SOL, Tiber, loadKeypair } from "../src/tiber.ts";
const [url, deployerPath, romSol, remSol] = process.argv.slice(2);
if (!url.includes("127.0.0.1")) throw new Error("local only");
const connection = new Connection(url, "confirmed");
const admin = loadKeypair(deployerPath);
const t = new Tiber(connection, admin);
const s = JSON.parse(readFileSync(new URL("../deployments/local.json", import.meta.url), "utf8")).addresses;
const ROM = new PublicKey(s.rom), REM = new PublicKey(s.rem);
const SOL = 1_000_000_000n;
async function wallet(): Promise<Keypair> {
  const kp = Keypair.generate();
  await t.send([SystemProgram.transfer({ fromPubkey: admin.publicKey, toPubkey: kp.publicKey, lamports: Number(2n * SOL) })]);
  await t.send([await t.createHolding(kp.publicKey, BRIDGED_SOL, kp.publicKey), await t.createHolding(kp.publicKey, ROM, kp.publicKey), await t.createHolding(kp.publicKey, REM, kp.publicKey), await t.wrapSol(kp.publicKey, 19n * SOL / 10n)], [], kp);
  return kp;
}
const raised = async (mint: PublicKey) => BigInt((await t.pool(mint)).quoteReserve.toString());
const keepers: Keypair[] = [];
for (const [mint, sister, goal] of [[ROM, REM, BigInt(romSol)], [REM, ROM, BigInt(remSol)]] as const) {
  while ((await raised(mint)) < goal * SOL) {
    const w = await wallet();
    keepers.push(w);
    // Two buys, each under the 1% limit and together under the 1.5% wall.
    for (let i = 0; i < 2; i++) {
      try { await t.send([await t.swapIx(w.publicKey, mint, sister, 1, 120_000_000n)], [], w); } catch { break; }
    }
  }
  console.log(`${mint.toBase58().slice(0, 4)} raised ${Number(await raised(mint)) / 1e9} SOL`);
}
// Keepers: a few wallets light the tithe a different number of times.
for (const [i, k] of keepers.slice(0, 4).entries()) {
  for (let n = 0; n <= 3 - i; n++) {
    for (const [mint, sister] of [[ROM, REM], [REM, ROM]] as const) {
      try { await t.send([await t.titheIx(k.publicKey, mint, sister)], [], k); } catch {}
      await new Promise((r) => setTimeout(r, 900));
    }
  }
}
console.log("staged");
