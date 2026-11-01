// Sets Romulus and Remus up on a cluster, after the five programs are deployed. Safe to run again:
// every step that already happened is skipped.
//
//   npx tsx scripts/setup.ts <rpc url> <deployer keypair> [--open]
//
// --open also opens trading (both curves at the same moment). Without it, everything is ready
// and trading stays closed. The twins' mint keypairs and addresses are kept in
// deployments/<cluster>.json.

import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import {
  BRIDGED_SOL,
  Tiber,
  bridgeConfig,
  dexConfig,
  loadKeypair,
  poolAddress,
  titheState,
  twinAddress,
  twinsConfig,
} from "../src/tiber.ts";

const [url, deployerPath, ...flags] = process.argv.slice(2);
if (!url || !deployerPath) {
  console.error("usage: setup.ts <rpc url> <deployer keypair> [--open]");
  process.exit(2);
}
const connection = new Connection(url, "confirmed");
const admin = loadKeypair(deployerPath);
const t = new Tiber(connection, admin);
const cluster = url.includes("devnet") ? "devnet" : url.includes("mainnet") ? "mainnet" : "local";
const file = new URL(`../deployments/${cluster}.json`, import.meta.url);

type Saved = { rom: number[]; rem: number[] };
function mints(): { rom: Keypair; rem: Keypair } {
  mkdirSync(new URL("../deployments/", import.meta.url), { recursive: true });
  let saved: Saved;
  if (existsSync(file)) {
    saved = JSON.parse(readFileSync(file, "utf8"));
  } else {
    saved = { rom: Array.from(Keypair.generate().secretKey), rem: Array.from(Keypair.generate().secretKey) };
  }
  const rom = Keypair.fromSecretKey(Uint8Array.from(saved.rom));
  const rem = Keypair.fromSecretKey(Uint8Array.from(saved.rem));
  writeFileSync(
    file,
    JSON.stringify(
      {
        ...saved,
        addresses: {
          rom: rom.publicKey.toBase58(),
          rem: rem.publicKey.toBase58(),
          romPool: poolAddress(rom.publicKey).toBase58(),
          remPool: poolAddress(rem.publicKey).toBase58(),
          bridgedSol: BRIDGED_SOL.toBase58(),
        },
      },
      null,
      2,
    ),
  );
  return { rom, rem };
}

const exists = async (k: PublicKey) => (await connection.getAccountInfo(k, "confirmed")) !== null;

async function step(name: string, done: () => Promise<boolean>, run: () => Promise<unknown>) {
  if (await done()) {
    console.log(`skip  ${name}`);
    return;
  }
  await run();
  console.log(`done  ${name}`);
}

const { rom, rem } = mints();
const me = admin.publicKey;
console.log(`cluster ${cluster}, admin ${me.toBase58()}, balance ${(await connection.getBalance(me)) / 1e9} SOL`);
console.log(`ROM ${rom.publicKey.toBase58()}\nREM ${rem.publicKey.toBase58()}`);

await step("exchange config", () => exists(dexConfig()), async () => t.send([await t.dexInitConfig(me, me)]));
await step("bridge config", () => exists(bridgeConfig()), async () => t.send([await t.bridgeInitConfig(me)]));
await step("bridged SOL", () => exists(BRIDGED_SOL), async () => t.send([await t.registerSol(me)]));
await step("twins config", () => exists(twinsConfig()), async () => t.send([await t.twinsInitConfig(me, me)]));
await step(
  "create Romulus",
  () => exists(twinAddress(rom.publicKey)),
  async () => t.send([await t.createTwin(me, rom.publicKey, "Romulus", "ROM", "")], [rom]),
);
await step(
  "create Remus",
  () => exists(twinAddress(rem.publicKey)),
  async () => t.send([await t.createTwin(me, rem.publicKey, "Remus", "REM", "")], [rem]),
);
for (const [name, m] of [["Romulus", rom], ["Remus", rem]] as const) {
  await step(`open the ${name} curve`, () => exists(poolAddress(m.publicKey)), async () => t.send([await t.openCurve(me, m.publicKey, me)]));
}
await step(
  "link the twins",
  async () => !(await t.twin(rom.publicKey)).sister.equals(PublicKey.default),
  async () => t.send([await t.linkTwins(me, rom.publicKey, rem.publicKey)]),
);
for (const [name, m] of [["Romulus", rom], ["Remus", rem]] as const) {
  await step(`tithe buyer of ${name}`, () => exists(titheState(m.publicKey)), async () => t.send([await t.initBuyer(me, m.publicKey)]));
}
if (flags.includes("--open")) {
  await step(
    "open trading",
    async () => (await t.twin(rom.publicKey)).openedAt.toNumber() > 0,
    async () => t.send([await t.openTrading(me, rom.publicKey, rem.publicKey)]),
  );
} else {
  console.log("trading stays closed (run again with --open to open it)");
}
console.log(`balance after ${(await connection.getBalance(me)) / 1e9} SOL`);
