// Checks a pair made before the loyalty burn still trades on the upgraded programs (flat 2% burn).
import { Connection, Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import { readFileSync } from "node:fs";
import { BRIDGED_SOL, Tiber, loadKeypair } from "../src/tiber.ts";
const [url, deployerPath, file] = process.argv.slice(2);
const connection = new Connection(url, "confirmed");
const admin = loadKeypair(deployerPath);
const t = new Tiber(connection, admin);
const s = JSON.parse(readFileSync(file, "utf8"));
const ROM = new PublicKey(s.addresses.rom), REM = new PublicKey(s.addresses.rem);
const kp = Keypair.generate();
await t.send([SystemProgram.transfer({ fromPubkey: admin.publicKey, toPubkey: kp.publicKey, lamports: 300_000_000 })]);
await t.send([await t.createHolding(kp.publicKey, BRIDGED_SOL, kp.publicKey), await t.createHolding(kp.publicKey, ROM, kp.publicKey), await t.createHolding(kp.publicKey, REM, kp.publicKey), await t.wrapSol(kp.publicKey, 200_000_000n)], [], kp);
await t.send([await t.swapIx(kp.publicKey, ROM, REM, 1, 100_000_000n)], [], kp);
const got = await t.balance(ROM, kp.publicKey);
const before = (t as any).token ? BigInt(((await (t as any).token.account.mint.fetch(ROM)) as any).supply.toString()) : 0n;
await t.send([await t.swapIx(kp.publicKey, ROM, REM, 0, got / 2n)], [], kp);
const after = BigInt(((await (t as any).token.account.mint.fetch(ROM)) as any).supply.toString());
console.log(`old pair: bought ${got / 1_000_000n} ROM, sold half, burned ${(before - after) * 10000n / (got / 2n)} bps`);
