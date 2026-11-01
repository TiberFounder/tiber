// Everything the page shows, read from the chain every few seconds.
import { useCallback, useEffect, useRef, useState } from "react";
import type { Connection, PublicKey } from "@solana/web3.js";
import { EventParser } from "@anchor-lang/core";
import { BRIDGED_SOL, TOKEN_ID, Tiber, buyerAddress, poolAddress, titheState, twinAddress } from "./tiber";
import { noteHeldSince, type Reserves } from "./math";

export type TwinView = {
  mint: PublicKey;
  exists: boolean;
  openedAt: number;
  graduated: boolean;
  reserves: Reserves;
  supply: bigint;
  /** SOL waiting at the tithe to buy and burn this twin (paid by the sister's buys). */
  fund: bigint;
  /** Burned so far: by sells, by the tithe, and at graduation. */
  sellBurned: bigint;
  titheBurned: bigint;
  graduationBurned: bigint;
  titheSpent: bigint;
  /** Tithes made for this twin. */
  tithes: number;
  buyTaxPaid: bigint;
};

export type Wallet = { sol: bigint; wrapped: bigint; rom: bigint; rem: bigint; hasWrapped: boolean; hasRom: boolean; hasRem: boolean; romSince: number; remSince: number };

const big = (v: any): bigint => BigInt(v?.toString?.() ?? "0");

/** Every account the page shows, read in one request (the free devnet server limits requests). */
async function readAll(t: Tiber, rom: PublicKey, rem: PublicKey, who: PublicKey | null): Promise<{ twins: [TwinView, TwinView]; wallet: Wallet | null }> {
  const per = (m: PublicKey) => [twinAddress(m), poolAddress(m), m, holdingOf(BRIDGED_SOL, buyerAddress(m)), titheState(m)];
  const keys = [...per(rom), ...per(rem)];
  if (who) keys.push(holdingOf(BRIDGED_SOL, who), holdingOf(rom, who), holdingOf(rem, who));
  const [infos, sol] = await Promise.all([
    t.connection.getMultipleAccountsInfo(keys, "confirmed"),
    who ? t.connection.getBalance(who, "confirmed") : Promise.resolve(0),
  ]);
  const amount = (i: any) => (i ? (i.data as Buffer).readBigUInt64LE(74) : 0n);
  const twin = (mint: PublicKey, at: number): TwinView => {
    const [twinInfo, poolInfo, mintInfo, fundInfo, stateInfo] = infos.slice(at, at + 5);
    const empty: Reserves = { base: 0n, quote: 0n, vBase: 0n, vQuote: 0n, curve: true };
    if (!twinInfo || !poolInfo || !mintInfo) {
      return { mint, exists: false, openedAt: 0, graduated: false, reserves: empty, supply: 0n, fund: 0n, sellBurned: 0n, titheBurned: 0n, graduationBurned: 0n, titheSpent: 0n, buyTaxPaid: 0n, tithes: 0 };
    }
    const tw: any = t.twins.coder.accounts.decode("twin", twinInfo.data);
    const pool: any = t.swap.coder.accounts.decode("pool", poolInfo.data);
    const m: any = t.token.coder.accounts.decode("mint", mintInfo.data);
    const st: any = stateInfo ? t.tithe.coder.accounts.decode("titheState", stateInfo.data) : null;
    return {
      mint,
      exists: true,
      openedAt: Number(big(tw.openedAt)),
      graduated: tw.graduated,
      reserves: { base: big(pool.baseReserve), quote: big(pool.quoteReserve), vBase: big(pool.virtualBase), vQuote: big(pool.virtualQuote), curve: pool.curve },
      supply: big(m.supply),
      fund: amount(fundInfo),
      sellBurned: big(tw.sellBurned),
      titheBurned: st ? big(st.burned) : 0n,
      graduationBurned: big(tw.graduationBurned),
      titheSpent: st ? big(st.spent) : 0n,
      tithes: st ? Number(big(st.tithes)) : 0,
      buyTaxPaid: big(tw.buyTaxPaid),
    };
  };
  const since = (i: any) => {
    if (!i) return 0;
    try {
      return noteHeldSince((t.token.coder.accounts.decode("holding", i.data) as any).hookData);
    } catch {
      return 0;
    }
  };
  const wallet = who
    ? {
        sol: BigInt(sol), wrapped: amount(infos[10]), rom: amount(infos[11]), rem: amount(infos[12]),
        hasWrapped: !!infos[10], hasRom: !!infos[11], hasRem: !!infos[12],
        romSince: since(infos[11]), remSince: since(infos[12]),
      }
    : null;
  return { twins: [twin(rom, 0), twin(rem, 5)], wallet };
}

import { holding as holdingOf } from "./tiber";

export type Citizen = { owner: string; amount: bigint; since: number };
export type Census = { rom: Citizen[]; rem: Citizen[]; at: number };

/** Every holder of both twins, read every two minutes (one request per twin). */
async function readCensus(t: Tiber, rom: PublicKey, rem: PublicKey): Promise<Census> {
  const list = async (mint: PublicKey): Promise<Citizen[]> => {
    const accounts = await t.connection.getProgramAccounts(TOKEN_ID, {
      commitment: "confirmed",
      filters: [{ memcmp: { offset: 10, bytes: mint.toBase58() } }],
    });
    const out: Citizen[] = [];
    for (const a of accounts) {
      try {
        const h: any = t.token.coder.accounts.decode("holding", a.account.data);
        const amount = BigInt(h.amount.toString());
        const since = noteHeldSince(h.hookData);
        // Only wallets with a note are citizens; the pools, the treasury and the tithe keep none.
        if (amount > 0n && since > 0) out.push({ owner: h.owner.toBase58(), amount, since });
      } catch {
        // Not a holding.
      }
    }
    return out;
  };
  const [a, b] = [await list(rom), await list(rem)];
  return { rom: a, rem: b, at: Date.now() };
}

export function useCensus(connection: Connection, rom: PublicKey, rem: PublicKey) {
  const tiber = useRef(new Tiber(connection)).current;
  const [census, setCensus] = useState<Census | null>(null);
  useEffect(() => {
    let stop = false;
    let timer: ReturnType<typeof setTimeout>;
    const tick = async () => {
      try {
        if (!document.hidden || !census) setCensus(await readCensus(tiber, rom, rem));
      } catch {
        // Busy server: try again next round.
      }
      if (!stop) timer = setTimeout(tick, 120_000);
    };
    tick();
    return () => {
      stop = true;
      clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tiber, rom, rem]);
  return census;
}

export function useChain(connection: Connection, rom: PublicKey, rem: PublicKey, who: PublicKey | null) {
  const tiber = useRef(new Tiber(connection)).current;
  const [twins, setTwins] = useState<[TwinView, TwinView] | null>(null);
  const [wallet, setWallet] = useState<Wallet | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));

  const failures = useRef(0);
  const refresh = useCallback(async () => {
    try {
      const r = await readAll(tiber, rom, rem, who);
      setTwins(r.twins);
      setWallet(r.wallet);
      failures.current = 0;
      setError(null);
    } catch (e: any) {
      failures.current += 1;
      // One busy moment is normal on the free devnet server; say so only if it keeps happening.
      if (failures.current >= 3) setError("The free devnet server is busy right now, so the numbers may be a little behind. They'll catch up on their own.");
    }
  }, [tiber, rom, rem, who]);

  useEffect(() => {
    let stop = false;
    let timer: ReturnType<typeof setTimeout>;
    let first = true;
    const tick = async () => {
      // Always read once on arrival; after that, skip while the tab is in the background.
      if (first || !document.hidden) await refresh();
      first = false;
      // Every 15 seconds, backing off to a minute while the server is refusing.
      const wait = Math.min(60_000, 15_000 * 2 ** Math.min(failures.current, 2));
      if (!stop) timer = setTimeout(tick, wait);
    };
    tick();
    const clock = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000);
    // Coming back to the tab: catch up straight away.
    const onShow = () => { if (!document.hidden) refresh(); };
    document.addEventListener("visibilitychange", onShow);
    return () => {
      document.removeEventListener("visibilitychange", onShow);
      stop = true;
      clearTimeout(timer);
      clearInterval(clock);
    };
  }, [refresh]);

  return { tiber, twins, wallet, error, now, refresh };
}

/** One tithe from the chain: who lit it, for which twin, what it burned, and when. */
export type Lit = { sig: string; keeper: string; mint: string; burned: bigint; spent: bigint; time: number };

const KEEPERS_KEY = "tiber.tithes.v1";
const KEEPERS_LOOKBACK = 40;

function loadLit(): Record<string, Lit> {
  try {
    const raw = JSON.parse(localStorage.getItem(KEEPERS_KEY) ?? "{}");
    const out: Record<string, Lit> = {};
    for (const [k, v] of Object.entries<any>(raw)) out[k] = { ...v, burned: BigInt(v.burned), spent: BigInt(v.spent) };
    return out;
  } catch {
    return {};
  }
}
function saveLit(all: Record<string, Lit>) {
  try {
    const keep = Object.values(all).sort((a, b) => b.time - a.time).slice(0, 400);
    localStorage.setItem(KEEPERS_KEY, JSON.stringify(Object.fromEntries(keep.map((l) => [l.sig, { ...l, burned: l.burned.toString(), spent: l.spent.toString() }]))));
  } catch {
    /* storage is a convenience only */
  }
}

/** The recent tithes of both twins (newest first), read from the chain's history of each twin's
 *  tithe record. Only tithes not already seen are fetched, so later visits cost little. */
async function readLit(t: Tiber, mints: PublicKey[], known: Record<string, Lit>): Promise<Record<string, Lit>> {
  const all = { ...known };
  const parser = new EventParser(t.tithe.programId, t.tithe.coder);
  for (const mint of mints) {
    const sigs = await t.connection.getSignaturesForAddress(titheState(mint), { limit: KEEPERS_LOOKBACK }, "confirmed");
    const fresh = sigs.filter((s) => !s.err && !all[s.signature]).map((s) => s.signature);
    if (fresh.length === 0) continue;
    const txs = await t.connection.getTransactions(fresh, { maxSupportedTransactionVersion: 0, commitment: "confirmed" });
    txs.forEach((tx, i) => {
      if (!tx?.meta?.logMessages) return;
      for (const ev of parser.parseLogs(tx.meta.logMessages)) {
        if (ev.name !== "tithed" && ev.name !== "Tithed") continue;
        const d: any = ev.data;
        all[fresh[i]] = {
          sig: fresh[i],
          keeper: tx.transaction.message.staticAccountKeys[0].toBase58(),
          mint: d.mint.toBase58(),
          burned: big(d.burned),
          spent: big(d.spent),
          time: tx.blockTime ?? 0,
        };
      }
    });
  }
  return all;
}

/** Recent tithes, read when `active` (the section is on screen) and every three minutes after. */
export function useKeepers(t: Tiber, rom: PublicKey, rem: PublicKey, active: boolean, poke: number) {
  const [lit, setLit] = useState<Lit[] | null>(null);
  const store = useRef<Record<string, Lit>>(loadLit());
  useEffect(() => {
    if (!active) return;
    let stop = false;
    let timer: ReturnType<typeof setTimeout>;
    const mints = [rom.toBase58(), rem.toBase58()];
    const show = () => setLit(Object.values(store.current).filter((l) => mints.includes(l.mint)).sort((a, b) => b.time - a.time));
    show();
    const tick = async () => {
      try {
        store.current = await readLit(t, [rom, rem], store.current);
        saveLit(store.current);
        if (!stop) show();
      } catch {
        /* a busy server: keep what we have and try again later */
      }
      if (!stop) timer = setTimeout(tick, 180_000);
    };
    tick();
    return () => {
      stop = true;
      clearTimeout(timer);
    };
  }, [t, rom, rem, active, poke]);
  return lit;
}
