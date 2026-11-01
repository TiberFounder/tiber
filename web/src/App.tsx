import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { WalletMultiButton } from "@solana/wallet-adapter-react-ui";
import type { PublicKey, TransactionInstruction } from "@solana/web3.js";
import { CLUSTER, IS_TEST, REM, ROM, explorer } from "./config";
import {
  GRADUATION_QUOTE,
  HOLD_CAP,
  MAX_BUY,
  ONE,
  SOL,
  SUPPLY,
  buyTaxBps,
  fmtSol,
  fmtTok,
  parseAmount,
  priceLamports,
  quoteBuy,
  quoteSell,
  rankOf,
  sellBurnBps,
  DAY,
} from "./math";
import { useCensus, useChain, useKeepers, type Census, type Citizen, type Lit, type TwinView, type Wallet } from "./state";
import { CHAPTERS, FINALE, MILESTONES_SOL, type Chapter } from "./chronicle";
import { drawCard } from "./card";
import { EventParser } from "@anchor-lang/core";
import { BRIDGED_SOL, retryBusy, type Tiber } from "./tiber";
import romArt from "./art/rom.svg?raw";
import remArt from "./art/rem.svg?raw";
import wolfArt from "./art/wolf.svg?raw";
import flameArt from "./art/flame.svg?raw";
import bannerArt from "./art/banner.svg?raw";

const Art = ({ svg, className }: { svg: string; className?: string }) => (
  <span className={`art ${className ?? ""}`} dangerouslySetInnerHTML={{ __html: svg }} />
);

type Side = "rom" | "rem";
const NAMES: Record<Side, { name: string; ticker: string; hill: string }> = {
  rom: { name: "Romulus", ticker: "ROM", hill: "the Palatine" },
  rem: { name: "Remus", ticker: "REM", hill: "the Aventine" },
};

export default function App() {
  const { connection } = useConnection();
  const wallet = useWallet();
  const { tiber, twins, wallet: mine, error, now, refresh } = useChain(connection, ROM, REM, wallet.publicKey);
  const census = useCensus(connection, ROM, REM);
  const [notice, setNotice] = useState<{ kind: "ok" | "err"; text: ReactNode } | null>(null);
  const [busy, setBusy] = useState(false);
  const [poke, setPoke] = useState(0);

  /** Signs and sends `ixs` with the connected wallet, then refreshes. */
  async function send(label: string, build: () => Promise<TransactionInstruction[]>, done: string): Promise<string | null> {
    if (!wallet.publicKey) return null;
    setBusy(true);
    setNotice(null);
    try {
      const tx = await tiber.transaction(wallet.publicKey, await build());
      const sig = await wallet.sendTransaction(tx, connection);
      setNotice({ kind: "ok", text: <>{label}: sent, waiting for the network…</> });
      const res = await retryBusy(() => connection.confirmTransaction(sig, "confirmed"));
      if (res.value.err) throw new Error(JSON.stringify(res.value.err));
      setNotice({
        kind: "ok",
        text: (
          <>
            {done}{" "}
            <a href={`https://explorer.solana.com/tx/${sig}${CLUSTER === "devnet" ? "?cluster=devnet" : ""}`} target="_blank" rel="noreferrer">
              View the transaction
            </a>
          </>
        ),
      });
      await refresh();
      return sig;
    } catch (e: any) {
      setNotice({ kind: "err", text: explain(e) });
      return null;
    } finally {
      setBusy(false);
    }
  }

  const [rom, rem] = twins ?? [null, null];
  // For the automated tests on a local network only: which test wallet is connected.
  if (CLUSTER === "local") (window as any).__tiberWallet = wallet.publicKey?.toBase58() ?? null;
  const ready = !!rom?.exists && !!rem?.exists;

  return (
    <>
      <header className="top">
        <a className="brand" href="#">
          <Art svg={wolfArt} className="brand-wolf" />
          <span>The Twins of the Tiber</span>
        </a>
        {IS_TEST && <span className="testnet">{CLUSTER === "local" ? "Local test network" : "Devnet"}: test SOL only, nothing here has value</span>}
        <WalletMultiButton />
      </header>

      <section className="hero">
        <div className="hero-words">
          <h1>Two hills. Two cities. One wolf.</h1>
          <p>Romulus (ROM) and Remus (REM) are twin tokens on Solana. Every buy of one buys and burns the other. Every sell burns itself.</p>
          <p className="choose">Choose your brother.</p>
        </div>
        <Art svg={bannerArt} className="hero-art" />
      </section>

      {error && <p className="banner-msg err">{error}</p>}
      {notice && (
        <p className={`banner-msg ${notice.kind}`} role="status">
          {notice.text}
        </p>
      )}

      <main className="hills" id="trade">
        {!twins && <p className="loading">Reading the chain…</p>}
        {twins && !ready && <p className="loading">The twins aren't set up on this network yet.</p>}
        {rom && rem && ready && (
          <>
            <TwinColumn side="rom" twin={rom} sister={rem} mine={mine} now={now} busy={busy} tiber={tiber} send={send} who={wallet.publicKey} />
            <River rom={rom} rem={rem} now={now} busy={busy} tiber={tiber} send={send} who={wallet.publicKey} onLit={() => setPoke((n) => n + 1)} />
            <TwinColumn side="rem" twin={rem} sister={rom} mine={mine} now={now} busy={busy} tiber={tiber} send={send} who={wallet.publicKey} />
          </>
        )}
      </main>

      {rom && rem && ready && <Scoreboard rom={rom} rem={rem} census={census} now={now} />}
      {rom && rem && ready && <Keepers tiber={tiber} rom={rom} rem={rem} now={now} poke={poke} me={wallet.publicKey?.toBase58() ?? null} />}

      <Lore />
      {rom && rem && ready && <Chronicle rom={rom} rem={rem} />}
      <Oath />

      <footer className="foot">
        <p>No presale, no insider buys. A 5% treasury, four-fifths of it locked for 6 months by code.</p>
        <p className="addresses">
          <a href={explorer(ROM.toBase58())} target="_blank" rel="noreferrer">ROM on the explorer</a>
          <a href={explorer(REM.toBase58())} target="_blank" rel="noreferrer">REM on the explorer</a>
          <a href="https://github.com/TiberFounder/tiber" target="_blank" rel="noreferrer">Source code</a>
        </p>
      </footer>
    </>
  );
}

type Send = (label: string, build: () => Promise<TransactionInstruction[]>, done: string) => Promise<string | null>;

function TwinColumn(p: {
  side: Side;
  twin: TwinView;
  sister: TwinView;
  mine: Wallet | null;
  now: number;
  busy: boolean;
  tiber: Tiber;
  send: Send;
  who: PublicKey | null;
}) {
  const { side, twin } = p;
  const n = NAMES[side];
  const r = twin.reserves;
  const price = priceLamports(r);
  const value = (price * Number(twin.supply)) / Number(ONE);
  const raised = r.curve ? r.quote : GRADUATION_QUOTE;
  const progress = Math.min(1, Number(raised) / Number(GRADUATION_QUOTE));
  const burned = SUPPLY - twin.supply;
  const held = p.mine ? (side === "rom" ? p.mine.rom : p.mine.rem) : null;

  return (
    <article className={`twin twin-${side}`} aria-labelledby={`${side}-name`}>
      <Art svg={side === "rom" ? romArt : remArt} className="coin" />
      <h2 id={`${side}-name`}>
        {n.name} <span className="ticker">{n.ticker}</span>
      </h2>
      <dl className="facts">
        <div>
          <dt>Price of 1,000,000 {n.ticker}</dt>
          <dd>{(price * 1e6 / 1e9).toPrecision(3)} SOL</dd>
        </div>
        <div>
          <dt>Whole twin valued at</dt>
          <dd>{(value / 1e9).toLocaleString("en-US", { maximumFractionDigits: 1 })} SOL</dd>
        </div>
        <div>
          <dt>Burned for ever</dt>
          <dd>{fmtTok(burned)} {n.ticker}</dd>
        </div>
        {held !== null && (
          <div>
            <dt>You hold</dt>
            <dd>{fmtTok(held)} {n.ticker}</dd>
          </div>
        )}
      </dl>

      {held !== null && held > 0n && p.mine && p.who && (
        <CitizenNote side={side} since={side === "rom" ? p.mine.romSince : p.mine.remSince} now={p.now} owner={p.who.toBase58()} cityBurned={SUPPLY - twin.supply} />
      )}

      <div className="curve" aria-label={`${n.name}'s curve`}>
        {twin.graduated ? (
          <p>Graduated: trading in its own pool, with the liquidity locked for good.</p>
        ) : (
          <>
            <div className="bar" role="progressbar" aria-valuemin={0} aria-valuemax={30} aria-valuenow={Number(fmtSol(raised, 2))}>
              <span style={{ width: `${progress * 100}%` }} />
            </div>
            <p>
              {fmtSol(raised, 2)} of 30 SOL raised on the curve. At 30 SOL it graduates into a pool whose liquidity no one can pull.
            </p>
          </>
        )}
      </div>

      <TradeBox {...p} />
    </article>
  );
}

function TradeBox(p: { side: Side; twin: TwinView; sister: TwinView; mine: Wallet | null; now: number; busy: boolean; tiber: Tiber; send: Send; who: PublicKey | null }) {
  const { side, twin, sister, mine, now, tiber, who } = p;
  const n = NAMES[side];
  const [mode, setMode] = useState<"buy" | "sell">("buy");
  const [text, setText] = useState("");
  const open = twin.openedAt > 0 && now >= twin.openedAt;
  const taxBps = buyTaxBps(now, twin.openedAt);
  const held = mine ? (side === "rom" ? mine.rom : mine.rem) : 0n;

  const amount = parseAmount(text, mode === "buy" ? 9 : 6);
  const check = useMemo(() => {
    if (!amount || amount === 0n) return null;
    if (mode === "buy") {
      const q = quoteBuy(twin.reserves, amount, taxBps);
      let problem: string | null = null;
      if (q.out > MAX_BUY) problem = `One buy can take at most 10,000,000 ${n.ticker} (1% of the supply). Try a smaller amount.`;
      else if (held + q.out > HOLD_CAP) problem = `No wallet may hold more than 15,000,000 ${n.ticker} (1.5%). You hold ${fmtTok(held)}, so you can buy about ${fmtTok(HOLD_CAP - held)} more.`;
      else if (mine && amount + 10_000_000n > mine.sol + mine.wrapped) problem = "You don't have that much SOL (keep a little for the network fee).";
      return {
        lines: [
          [`You get about`, `${fmtTok(q.out)} ${n.ticker}`],
          [`Tax to buy and burn ${NAMES[side === "rom" ? "rem" : "rom"].ticker}`, `${fmtSol(q.tax)} SOL (${Number(taxBps) / 100}%)`],
        ],
        problem,
        minOut: (q.out * 98n) / 100n,
        soldOut: q.soldOut,
      };
    }
    const since = mine ? (side === "rom" ? mine.romSince : mine.remSince) : 0;
    const burnBps = sellBurnBps(now, since);
    const q = quoteSell(twin.reserves, amount, burnBps);
    let problem: string | null = null;
    if (amount > held) problem = `You hold ${fmtTok(held)} ${n.ticker}.`;
    return {
      lines: [
        [`You get about`, `${fmtSol(q.out)} SOL`],
        [`Burned in Vesta's flame`, `${fmtTok(q.burn)} ${n.ticker} (${Number(burnBps) / 100}%${burnBps === 500n ? ", held under a day" : burnBps === 100n ? ", loyal 30 days" : ""})`],
      ],
      problem,
      minOut: (q.out * 98n) / 100n,
      soldOut: false,
    };
  }, [amount, mode, twin.reserves, taxBps, held, mine, n.ticker, side]);

  async function go() {
    if (!who || !amount || !check || check.problem || !mine) return;
    const mint = twin.mint;
    const sisterMint = sister.mint;
    if (mode === "buy") {
      await p.send(
        `Buy ${n.ticker}`,
        async () => {
          const ixs: TransactionInstruction[] = [];
          if (!mine.hasWrapped) ixs.push(await tiber.createHolding(who, BRIDGED_SOL, who));
          if (!(side === "rom" ? mine.hasRom : mine.hasRem)) ixs.push(await tiber.createHolding(who, mint, who));
          const need = amount > mine.wrapped ? amount - mine.wrapped : 0n;
          if (need > 0n) ixs.push(await tiber.wrapSol(who, need));
          // Anything a sold-out curve does not use stays wrapped and goes into the next buy.
          ixs.push(await tiber.swapIx(who, mint, sisterMint, 1, amount, check.minOut));
          return ixs;
        },
        `Bought ${n.ticker}.`,
      );
    } else {
      await p.send(
        `Sell ${n.ticker}`,
        async () => {
          const ixs: TransactionInstruction[] = [];
          if (!mine.hasWrapped) ixs.push(await tiber.createHolding(who, BRIDGED_SOL, who));
          ixs.push(await tiber.swapIx(who, mint, sisterMint, 0, amount, check.minOut));
          ixs.push(await tiber.unwrapSolAbove(who, mine.wrapped));
          return ixs;
        },
        `Sold ${n.ticker}.`,
      );
    }
    setText("");
  }

  return (
    <div className="trade">
      <div className="modes" role="tablist" aria-label={`Buy or sell ${n.ticker}`}>
        <button role="tab" aria-selected={mode === "buy"} onClick={() => { setMode("buy"); setText(""); }}>
          Buy
        </button>
        <button role="tab" aria-selected={mode === "sell"} onClick={() => { setMode("sell"); setText(""); }}>
          Sell
        </button>
      </div>
      <label className="amount">
        <span>{mode === "buy" ? "SOL to spend" : `${n.ticker} to sell`}</span>
        <input inputMode="decimal" placeholder={mode === "buy" ? "0.1" : "1000000"} value={text} onChange={(e) => setText(e.target.value)} />
        {mode === "sell" && held > 0n && (
          <button className="max" type="button" onClick={() => setText((Number(held) / 1e6).toString())}>
            All
          </button>
        )}
      </label>
      {check && (
        <dl className="quote">
          {check.lines.map(([a, b]) => (
            <div key={a}>
              <dt>{a}</dt>
              <dd>{b}</dd>
            </div>
          ))}
          {check.soldOut && <p>This buy takes the last of the curve; what it doesn't need stays with you.</p>}
        </dl>
      )}
      {check?.problem && <p className="problem">{check.problem}</p>}
      {mode === "buy" && open && taxBps > 200n && <p className="problem">Anti-sniper fee: the buy tax is {Number(taxBps) / 100}% for the first 30 seconds after opening, falling to 2%.</p>}
      {!open && <p className="problem">Trading hasn't opened yet.</p>}
      <button className="act" disabled={!who || !open || !check || !!check.problem || p.busy} onClick={go}>
        {!who ? "Connect a wallet to trade" : mode === "buy" ? `Buy ${n.ticker}` : `Sell ${n.ticker}`}
      </button>
    </div>
  );
}

const when = (t: number) => new Date(t * 1000).toLocaleDateString("en-US", { month: "short", day: "numeric" });
const days = (s: number) => Math.max(0, Math.floor(s / DAY));

/** Your standing in a city: rank, how long your tokens have been here, and what selling burns. */
function CitizenNote({ side, since, now, owner, cityBurned }: { side: Side; since: number; now: number; owner: string; cityBurned: bigint }) {
  const n = NAMES[side];
  const [card, setCard] = useState<{ url: string; blob: Blob } | null>(null);
  const [making, setMaking] = useState(false);
  useEffect(() => () => { if (card) URL.revokeObjectURL(card.url); }, [card]);
  if (since <= 0) return null;
  const { rank, next, nextAt } = rankOf(now, since);
  const bps = sellBurnBps(now, since);
  const age = now - since;
  const burnNext = age < DAY ? `on ${when(since + DAY)} it falls to 2%` : age < 30 * DAY ? `on ${when(since + 30 * DAY)} it falls to 1%` : "the lowest it goes";
  return (
    <div className="citizen">
      <p className="citizen-rank">
        {rank} of {n.name}
      </p>
      <p>
        {days(age) === 0 ? `Your ${n.ticker} arrived today.` : `Your ${n.ticker} has been here ${days(age)} ${days(age) === 1 ? "day" : "days"} on average.`} Selling now burns {Number(bps) / 100}%; {burnNext}.
        {next && nextAt ? ` You become ${next} on ${when(nextAt)}.` : ""}
      </p>
      <button
        className="act small card-make"
        disabled={making}
        onClick={async () => {
          setMaking(true);
          try {
            const blob = await drawCard({
              side,
              rank,
              days: days(age),
              burnPct: Number(bps) / 100,
              next,
              nextDays: nextAt ? Math.max(1, Math.ceil((nextAt - now) / DAY)) : null,
              owner,
              cityBurned: fmtTok(cityBurned),
              site: `${location.host}${location.pathname}`.replace(/\/$/, ""),
            });
            setCard({ url: URL.createObjectURL(blob), blob });
          } finally {
            setMaking(false);
          }
        }}
      >
        {making ? "Striking your card…" : "Make my citizen card"}
      </button>
      {card && <CardShare side={side} rank={rank} card={card} />}
    </div>
  );
}

/** The card, with ways to keep it and post it. */
function CardShare({ side, rank, card }: { side: Side; rank: string; card: { url: string; blob: Blob } }) {
  const n = NAMES[side];
  const file = `${rank.toLowerCase()}-of-${n.name.toLowerCase()}.png`;
  const text = `I'm a ${rank} of ${n.name} (${n.ticker}). Two hills, two cities, one wolf.`;
  const link = `${location.origin}${location.pathname}`;
  const canShareFile = typeof navigator.canShare === "function" && navigator.canShare({ files: [new File([card.blob], file, { type: "image/png" })] });
  return (
    <div className="card-share">
      <img src={card.url} alt={`Citizen card: ${rank} of ${n.name}`} />
      <div className="card-actions">
        <a className="act small" href={card.url} download={file}>Download the card</a>
        <a className="act small" href={`https://x.com/intent/post?text=${encodeURIComponent(text)}&url=${encodeURIComponent(link)}`} target="_blank" rel="noreferrer">
          Post on X
        </a>
        {canShareFile && (
          <button className="act small" onClick={() => navigator.share({ files: [new File([card.blob], file, { type: "image/png" })], text: `${text} ${link}` }).catch(() => {})}>
            More ways to share
          </button>
        )}
      </div>
      <p className="card-hint">For X: download the card, then attach it to the post that opens.</p>
    </div>
  );
}

/** The keepers of the flame: who has lit the tithe, from the recent history of both twins. */
function Keepers({ tiber, rom, rem, now, poke, me }: { tiber: Tiber; rom: TwinView; rem: TwinView; now: number; poke: number; me: string | null }) {
  const ref = useRef<HTMLElement>(null);
  const [seen, setSeen] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el || seen) return;
    const io = new IntersectionObserver((es) => { if (es.some((e) => e.isIntersecting)) setSeen(true); }, { rootMargin: "200px" });
    io.observe(el);
    return () => io.disconnect();
  }, [seen]);
  const lit = useKeepers(tiber, rom.mint, rem.mint, seen, poke);
  const romKey = rom.mint.toBase58();
  const ago = (t: number) => {
    const s = Math.max(0, now - t);
    return s < 90 ? "just now" : s < 5400 ? `${Math.round(s / 60)} min ago` : s < 2 * DAY ? `${Math.round(s / 3600)} h ago` : `${Math.round(s / DAY)} days ago`;
  };
  const short = (a: string) => (a === me ? "You" : `${a.slice(0, 4)}…${a.slice(-4)}`);
  const board = useMemo(() => {
    const by = new Map<string, { keeper: string; count: number; rom: bigint; rem: bigint }>();
    for (const l of lit ?? []) {
      const k = by.get(l.keeper) ?? { keeper: l.keeper, count: 0, rom: 0n, rem: 0n };
      k.count += 1;
      if (l.mint === romKey) k.rom += l.burned;
      else k.rem += l.burned;
      by.set(l.keeper, k);
    }
    return [...by.values()].sort((a, b) => b.count - a.count || Number(b.rom + b.rem - (a.rom + a.rem))).slice(0, 8);
  }, [lit, romKey]);
  const last = (mint: string) => lit?.find((l) => l.mint === mint);
  return (
    <section className="keepers" ref={ref} aria-labelledby="keepers-title">
      <h2 id="keepers-title">Keepers of the flame</h2>
      <p className="score-lead">Whoever lights the tithe burns the sister twin for everyone. These are the keepers who have lit it most, from the latest tithes of each city.</p>
      {!lit && <p className="loading">Reading the flame's history…</p>}
      {lit && (
        <>
          <div className="last-lit">
            {(["rom", "rem"] as Side[]).map((s) => {
              const l = last((s === "rom" ? rom : rem).mint.toBase58());
              return (
                <p key={s}>
                  {NAMES[s].ticker}'s flame: {l ? <>last lit by {short(l.keeper)}, {ago(l.time)}, burning {fmtTok(l.burned)} {NAMES[s].ticker}</> : "not lit yet"}
                </p>
              );
            })}
          </div>
          {board.length > 0 ? (
            <ol className="keeper-board">
              {board.map((k, i) => (
                <li key={k.keeper} className={k.keeper === me ? "me" : ""}>
                  <span className="keeper-n">{i + 1}</span>
                  <span className="keeper-who">{short(k.keeper)}</span>
                  <span>{k.count} {k.count === 1 ? "tithe" : "tithes"}</span>
                  <span className="keeper-burn">{k.rom > 0n && `${fmtTok(k.rom)} ROM`}{k.rom > 0n && k.rem > 0n && " · "}{k.rem > 0n && `${fmtTok(k.rem)} REM`}</span>
                </li>
              ))}
            </ol>
          ) : (
            <p className="loading">No one has lit the flame yet. Be the first.</p>
          )}
        </>
      )}
    </section>
  );
}

/** The Chronicle: each city's chapters open as its curve raises SOL. */
const SEEN_KEY = "tiber.chapters.v1";
function readSeen(): string[] {
  try { return JSON.parse(localStorage.getItem(SEEN_KEY) ?? "[]"); } catch { return []; }
}
function Chronicle({ rom, rem }: { rom: TwinView; rem: TwinView }) {
  const [seen, setSeen] = useState<string[]>(readSeen);
  const markSeen = (id: string) => {
    if (seen.includes(id)) return;
    const next = [...seen, id];
    setSeen(next);
    try { localStorage.setItem(SEEN_KEY, JSON.stringify(next)); } catch { /* a convenience only */ }
  };
  const raised = (t: TwinView) => (t.graduated ? GRADUATION_QUOTE : t.reserves.quote);
  const both = rom.graduated && rem.graduated;
  const open = (t: TwinView, i: number) => i === MILESTONES_SOL.length - 1 ? t.graduated : raised(t) >= BigInt(MILESTONES_SOL[i]) * SOL;
  const total = 2 * MILESTONES_SOL.length + 1;
  const opened = [rom, rem].reduce((s, t) => s + MILESTONES_SOL.filter((_, i) => open(t, i)).length, 0) + (both ? 1 : 0);
  const chapter = (c: Chapter, isOpen: boolean, lock: ReactNode) =>
    isOpen ? (
      <details key={c.id} className="chapter" onToggle={(e) => (e.currentTarget as HTMLDetailsElement).open && markSeen(c.id)}>
        <summary>
          {c.title}
          {!seen.includes(c.id) && <span className="new">New</span>}
        </summary>
        {c.paragraphs.map((p, i) => <p key={i}>{p}</p>)}
      </details>
    ) : (
      <div key={c.id} className="chapter locked">
        <p className="chapter-title">{c.title}</p>
        <p className="chapter-teaser">{c.teaser}</p>
        <p className="chapter-lock">{lock}</p>
      </div>
    );
  const city = (t: TwinView, side: Side) => {
    const n = NAMES[side];
    const r = raised(t);
    return (
      <div className={`chronicle-city city-${side}`}>
        <h3>{n.name}'s chronicle</h3>
        {CHAPTERS[side].map((c, i) => {
          const goal = BigInt(MILESTONES_SOL[i]) * SOL;
          const pct = Math.min(100, Number((r * 100n) / goal));
          const lock = i === MILESTONES_SOL.length - 1
            ? <>Opens when {n.name} graduates: {fmtSol(r, 2)} of 30 SOL raised</>
            : <>Opens at {MILESTONES_SOL[i]} SOL raised: {fmtSol(r, 2)} so far</>;
          return chapter(c, open(t, i), <><span className="lock-bar"><span style={{ width: `${pct}%` }} /></span>{lock}</>);
        })}
      </div>
    );
  };
  return (
    <section className="chronicle" aria-labelledby="chronicle-title">
      <h2 id="chronicle-title">The Chronicle</h2>
      <p className="score-lead">The story goes on as the cities grow. Each city's chapters open as its curve raises SOL. {opened} of {total} chapters are open.</p>
      <div className="cities">
        {city(rom, "rom")}
        {city(rem, "rem")}
      </div>
      <div className="finale">{chapter(FINALE, both, <>Opens when both cities have graduated.</>)}</div>
    </section>
  );
}

/** Which city burns brighter: the two twins' burns side by side, and each city's citizens. */
function Scoreboard({ rom, rem, census, now }: { rom: TwinView; rem: TwinView; census: Census | null; now: number }) {
  const burned = (t: TwinView) => SUPPLY - t.supply;
  const [a, b] = [burned(rom), burned(rem)];
  const total = a + b;
  const share = total === 0n ? 50 : Number((a * 10_000n) / total) / 100;
  const leader = a === b ? null : a > b ? "Romulus" : "Remus";
  const city = (t: TwinView, side: Side, people: Citizen[] | undefined) => {
    const n = NAMES[side];
    const senate = (people ?? []).filter((c) => now - c.since >= 30 * DAY).length;
    const elders = [...(people ?? [])].sort((x, y) => x.since - y.since).slice(0, 5);
    return (
      <div className={`city city-${side}`}>
        <h3>{n.name}</h3>
        <p className="city-burned">{fmtTok(burned(t))} {n.ticker} burned</p>
        <dl>
          <div><dt>By sellers</dt><dd>{fmtTok(t.sellBurned)}</dd></div>
          <div><dt>By the tithe</dt><dd>{fmtTok(t.titheBurned)} in {t.tithes} {t.tithes === 1 ? "tithe" : "tithes"}</dd></div>
          {t.graduationBurned > 0n && <div><dt>At graduation</dt><dd>{fmtTok(t.graduationBurned)}</dd></div>}
          <div><dt>Citizens</dt><dd>{people ? people.length : "counting…"}</dd></div>
          <div><dt>Senators and above</dt><dd>{people ? senate : "…"}</dd></div>
        </dl>
        {elders.length > 0 && (
          <>
            <p className="elders-title">Longest-standing citizens</p>
            <ol className="elders">
              {elders.map((c) => (
                <li key={c.owner}>
                  <span>{c.owner.slice(0, 4)}…{c.owner.slice(-4)}</span>
                  <span>{rankOf(now, c.since).rank}, {days(now - c.since)}d</span>
                </li>
              ))}
            </ol>
          </>
        )}
      </div>
    );
  };
  return (
    <section className="scoreboard" aria-labelledby="score-title">
      <h2 id="score-title">Which city burns brighter</h2>
      <p className="score-lead">{leader ? `${leader} leads: more of it has gone into Vesta's flame.` : "The cities are level."}</p>
      <div className="tug" role="img" aria-label={`Romulus ${share}% of all burns, Remus ${(100 - share).toFixed(2)}%`}>
        <span className="tug-rom" style={{ width: `${share}%` }} />
        <span className="tug-rem" style={{ width: `${100 - share}%` }} />
      </div>
      <div className="cities">
        {city(rom, "rom", census?.rom)}
        {city(rem, "rem", census?.rem)}
      </div>
    </section>
  );
}

function River(p: { rom: TwinView; rem: TwinView; now: number; busy: boolean; tiber: Tiber; send: Send; who: PublicKey | null; onLit: () => void }) {
  const { rom, rem, tiber, who } = p;
  const [flare, setFlare] = useState(false);
  const [offering, setOffering] = useState<string | null>(null);
  async function light(target: TwinView, payer: TwinView, side: Side) {
    const n = NAMES[side];
    setOffering(null);
    const sig = await p.send(`Light the flame for ${n.ticker}`, async () => [await tiber.titheIx(who!, target.mint, payer.mint)], `You lit Vesta's flame: the tithe bought and burned ${n.ticker}.`);
    if (!sig) return;
    setFlare(true);
    setTimeout(() => setFlare(false), 2600);
    p.onLit();
    try {
      const tx = await retryBusy(() => tiber.connection.getTransaction(sig, { maxSupportedTransactionVersion: 0, commitment: "confirmed" }));
      const parser = new EventParser(tiber.tithe.programId, tiber.tithe.coder);
      for (const ev of parser.parseLogs(tx?.meta?.logMessages ?? [])) {
        const d: any = ev.data;
        if (d?.burned) setOffering(`Your tithe burned ${fmtTok(BigInt(d.burned.toString()))} ${n.ticker}. You are now among the keepers of the flame.`);
      }
    } catch {
      /* the burn happened either way; the board will show it */
    }
  }
  const flows: [TwinView, TwinView, Side][] = [
    [rem, rom, "rem"],
    [rom, rem, "rom"],
  ];
  return (
    <aside className="river" aria-label="The tithe between the twins">
      <Art svg={flameArt} className={`flame ${flare ? "flare" : ""}`} />
      <h2>The tithe</h2>
      <p>Each buy of one twin pays 2% of its SOL into a fund that buys the other twin and burns it in Vesta's flame. Anyone may light the flame: it costs only the network fee, and the keepers who light it most are honored below.</p>
      {offering && <p className="offering" role="status">{offering}</p>}
      {flows.map(([target, payer, side]) => {
        const n = NAMES[side];
        const burned = target.titheBurned;
        const graduating = target.reserves.curve && target.reserves.base === 0n;
        return (
          <div className="flow" key={side}>
            <p className="flow-head">
              {NAMES[side === "rom" ? "rem" : "rom"].ticker} buys feed {n.ticker}
            </p>
            <p className="flow-fund">{fmtSol(target.fund)} SOL waiting</p>
            <p className="flow-sub">{fmtTok(burned)} {n.ticker} bought and burned so far</p>
            {graduating ? (
              <button className="act small" disabled={!who || p.busy} onClick={() => p.send(`Graduate ${n.ticker}`, async () => [await tiber.graduateIx(who!, target.mint)], `${n.name} graduated.`)}>
                Graduate {n.name}
              </button>
            ) : (
              <button
                className="act small"
                disabled={!who || p.busy || target.fund === 0n || target.openedAt === 0}
                onClick={() => light(target, payer, side)}
              >
                Light the flame for {n.ticker}
              </button>
            )}
          </div>
        );
      })}
    </aside>
  );
}

function Oath() {
  const rules: [string, string][] = [
    ["The tithe", "Every buy pays 2% of its SOL toward buying the sister twin, which is burned as soon as it is bought."],
    ["Vesta's flame", "Every sell burns part of the tokens sold: 5% if they were bought within the last day, 2% normally, and 1% once they have been held 30 days. Supply only ever shrinks."],
    ["Citizens", "Hold a twin and you are a Citizen of its city. After 7 days you are a Tribune, after 30 a Senator, after 90 a Consul. Tokens moved to a new wallet start their clock again; topping up moves it forward in proportion."],
    ["The wall", "No wallet may hold more than 15,000,000 of a twin (1.5%), and one buy can take at most 10,000,000 (1%). Only ordinary wallets can hold a twin, so every trade goes through the taxed pools."],
    ["The first thirty seconds", "The buy tax starts at 80% when trading opens and falls to 2% over 30 seconds, so bots gain nothing by being first. What they pay feeds the sister's tithe."],
    ["The treasury", "5% of each twin: 50,000,000. 10,000,000 can be used from launch; the other 40,000,000 are locked by code for six months after trading opens. At most 1,000,000 can leave the treasury on any day, ever. The other 95% goes to buyers: 71.25% on the curve and 23.75% into the pool at graduation."],
    ["The curve", "Each twin starts on a bonding curve. At 30 SOL raised it graduates into a pool, and its liquidity is locked: the programs have no instruction that moves it, for anyone."],
    ["Written in code", "No one can mint more, freeze a holder, or pause the twins' trading: the programs enforce these rules, not a person. The programs are experimental and stay upgradeable, so the founder's key can fix bugs and add to the rules later. Anyone can see on the explorer who holds that key and when a program last changed."],
  ];
  return (
    <section className="oath" aria-labelledby="oath-title">
      <h2 id="oath-title">The Oath of the Wolf</h2>
      <dl>
        {rules.map(([a, b]) => (
          <div key={a}>
            <dt>{a}</dt>
            <dd>{b}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

function Lore() {
  return (
    <section className="lore" aria-labelledby="lore-title">
      <h2 id="lore-title">The Twins of the Tiber</h2>
      <p>Before there was a Rome, there were two boys in a basket on the river.</p>
      <p>
        Their mother, Rhea Silvia, was a priestess of Vesta, keeper of the sacred flame. Their father was Mars, god of war. A jealous king, Amulius, feared what the twins would become, and ordered them drowned in the Tiber. But the river would not take them. It carried the basket gently ashore, beneath a fig tree, below the Palatine hill.
      </p>
      <p>There a she-wolf found them. She did not tear them apart. She fed them, one at each side, and the brothers learned their first law before they could speak: there is enough when you share it.</p>
      <p>
        They grew up shepherds, then warriors, then founders. When the time came to build a city, they argued over the hill. Remus watched from the Aventine and saw six vultures. Romulus watched from the Palatine and saw twelve. In the old telling, the quarrel ended in blood at the city wall.
      </p>
      <p className="turn">Not in this one.</p>
      <p>In this telling, the wolf returned. She stood on the half-built wall between them and gave them a choice: two cities that starve each other, or two cities that feed each other. The brothers swore the Oath of the Wolf.</p>
      <p>
        Whatever is spent in Romulus's city, a tithe goes to Remus. Whatever is spent in Remus's city, a tithe goes to Romulus. And whoever walks away from either city leaves an offering to their mother's flame, so that each city grows smaller in number and dearer in worth.
      </p>
      <p>The oath is written into the stone. No king, priest or founder can change it, not even the twins themselves.</p>
    </section>
  );
}

/** A plain-language reason for a failed transaction. */
function explain(e: any): string {
  const text = `${e?.message ?? e} ${(e?.logs ?? e?.transactionLogs ?? []).join(" ")}`;
  const code = text.match(/Error Code: (\w+)/)?.[1];
  const known: Record<string, string> = {
    OverTheWall: "That would put the wallet over 1.5% of the supply.",
    WalletsOnly: "Only ordinary wallets can hold a twin.",
    BuyTooLarge: "One buy can take at most 1% of the supply.",
    NotOpen: "Trading hasn't opened yet.",
    OncePerSlot: "The tithe just ran; try again in a moment.",
    NothingToSpend: "The tithe fund is empty right now.",
    Slippage: "The price moved before your trade landed. Try again.",
    DelegateOnHookedPool: "Only the wallet that owns the tokens can trade them here.",
  };
  if (code && known[code]) return known[code];
  if (/User rejected|rejected the request/i.test(text)) return "You cancelled it in your wallet.";
  if (/429|Too many requests|rate limit/i.test(text)) return "The free devnet server is busy and turned the request away. Nothing was sent; wait a few seconds and try again.";
  if (/insufficient|0x1\b/i.test(text)) return "Not enough SOL for that and the network fee.";
  return `It didn't go through: ${code ?? (e?.message ?? String(e)).slice(0, 160)}`;
}
