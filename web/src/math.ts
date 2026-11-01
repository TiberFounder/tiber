// The numbers the programs use, mirrored so the site can show a trade before it is signed.
// Rounding follows the programs: the buy tax rounds up, the sell burn and every output round down.

export const ONE = 1_000_000n; // one token, 6 decimals
export const SOL = 1_000_000_000n;
export const SUPPLY = 1_000_000_000n * ONE;
export const MAX_BUY = 10_000_000n * ONE;
export const HOLD_CAP = 15_000_000n * ONE;
export const GRADUATION_QUOTE = 30n * SOL;
export const BUY_TAX_BPS = 200n;
export const SELL_BURN_BPS = 200n;
export const SNIPER_WINDOW = 30;
export const SNIPER_START_BPS = 8_000n;

export type Reserves = { base: bigint; quote: bigint; vBase: bigint; vQuote: bigint; curve: boolean };

/** The buy tax at `now` (unix seconds) for trading opened at `openedAt`: 80% falling to 2%. */
export function buyTaxBps(now: number, openedAt: number): bigint {
  if (openedAt === 0 || now >= openedAt + SNIPER_WINDOW) return BUY_TAX_BPS;
  const elapsed = BigInt(Math.max(0, now - openedAt));
  const span = SNIPER_START_BPS - BUY_TAX_BPS;
  const fallen = (span * elapsed) / BigInt(SNIPER_WINDOW);
  return SNIPER_START_BPS - (fallen > span ? span : fallen);
}

const ceilDiv = (a: bigint, b: bigint) => (a + b - 1n) / b;

/** Price of one whole token in lamports, from the pool's effective reserves. */
export function priceLamports(r: Reserves): number {
  const x = r.quote + r.vQuote;
  const y = r.base + r.vBase;
  return y === 0n ? 0 : (Number(x) / Number(y)) * Number(ONE);
}

export type BuyQuote = { tax: bigint; out: bigint; soldOut: boolean };

/** A buy of `lamports`: the tax to the sister, and the tokens out (capped at what the curve has left). */
export function quoteBuy(r: Reserves, lamports: bigint, taxBps: bigint): BuyQuote {
  let tax = ceilDiv(lamports * taxBps, 10_000n);
  if (tax >= lamports) tax = 0n;
  const net = lamports - tax;
  const x = r.quote + r.vQuote;
  const y = r.base + r.vBase;
  let out = (net * y) / (x + net);
  const soldOut = r.curve && out >= r.base;
  if (soldOut) out = r.base;
  return { tax, out, soldOut };
}

export type SellQuote = { burn: bigint; out: bigint };

/** A sell of `amount` tokens: 2% burned, the rest sold for SOL. */
/** The loyalty burn: 5% for tokens under a day old, 1% from 30 days, 2% between or when unknown. */
export const DAY = 86_400;
export function sellBurnBps(now: number, heldSince: number): bigint {
  if (heldSince <= 0 || heldSince > now) return SELL_BURN_BPS;
  const age = now - heldSince;
  if (age < DAY) return 500n;
  if (age >= 30 * DAY) return 100n;
  return SELL_BURN_BPS;
}

/** A citizen's rank in their city, by how long their tokens have been held. */
export const RANKS: [string, number][] = [
  ["Consul", 90 * DAY],
  ["Senator", 30 * DAY],
  ["Tribune", 7 * DAY],
  ["Citizen", 0],
];
export function rankOf(now: number, heldSince: number): { rank: string; next: string | null; nextAt: number | null } {
  const age = heldSince > 0 ? now - heldSince : 0;
  for (let i = 0; i < RANKS.length; i++) {
    if (age >= RANKS[i][1]) {
      const up = i > 0 ? RANKS[i - 1] : null;
      return { rank: RANKS[i][0], next: up ? up[0] : null, nextAt: up ? heldSince + up[1] : null };
    }
  }
  return { rank: "Citizen", next: "Tribune", nextAt: heldSince + 7 * DAY };
}

/** Reads the arrival time the Wall keeps in a holding's note (0: none). */
export function noteHeldSince(note: number[] | Uint8Array): number {
  if (!note || note[8] !== 1) return 0;
  let v = 0n;
  for (let i = 7; i >= 0; i--) v = (v << 8n) | BigInt(note[i]);
  return Number(v);
}

export function quoteSell(r: Reserves, amount: bigint, burnBps: bigint = SELL_BURN_BPS): SellQuote {
  let burn = (amount * burnBps) / 10_000n;
  if (burn >= amount) burn = 0n;
  const net = amount - burn;
  const x = r.quote + r.vQuote;
  const y = r.base + r.vBase;
  return { burn, out: (net * x) / (y + net) };
}

export function parseAmount(text: string, decimals: number): bigint | null {
  const t = text.trim();
  if (!/^\d*(\.\d*)?$/.test(t) || t === "" || t === ".") return null;
  const [whole, frac = ""] = t.split(".");
  if (frac.length > decimals) return null;
  return BigInt(whole || "0") * 10n ** BigInt(decimals) + BigInt((frac + "0".repeat(decimals)).slice(0, decimals) || "0");
}

export function fmt(units: bigint, decimals: number, digits = 2): string {
  const neg = units < 0n;
  const v = neg ? -units : units;
  const n = Number(v) / 10 ** decimals;
  const s = n.toLocaleString("en-US", { maximumFractionDigits: digits, minimumFractionDigits: 0 });
  return neg ? `-${s}` : s;
}
export const fmtSol = (l: bigint, d = 4) => fmt(l, 9, d);
export const fmtTok = (t: bigint, d = 0) => fmt(t, 6, d);
