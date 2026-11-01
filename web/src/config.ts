// Which network the site talks to, and the twins on it. Set at build time:
//   VITE_CLUSTER=devnet (default) | local, VITE_RPC to override the RPC address,
//   VITE_ROM / VITE_REM to override the mints (a local validator's come from its setup run).
import { PublicKey } from "@solana/web3.js";

export type Cluster = "devnet" | "local" | "mainnet";
export const CLUSTER = (import.meta.env.VITE_CLUSTER ?? "devnet") as Cluster;

const RPC: Record<Cluster, string> = {
  devnet: "https://api.devnet.solana.com",
  local: "http://127.0.0.1:8899",
  mainnet: "",
};
export const RPC_URL: string = import.meta.env.VITE_RPC ?? RPC[CLUSTER];

const MINTS: Record<Cluster, { rom: string; rem: string }> = {
  devnet: {
    rom: "H36M9zjYzeCT8bLL9oVX4ywGjdn5eHCNY9gDon2MsPSA",
    rem: "E1vyLF2SbrfD8wfrGKWRZPYzawFZotZqNVnAmiBdUH4J",
  },
  local: { rom: "", rem: "" },
  mainnet: { rom: "", rem: "" },
};
export const ROM = new PublicKey(import.meta.env.VITE_ROM ?? MINTS[CLUSTER].rom);
export const REM = new PublicKey(import.meta.env.VITE_REM ?? MINTS[CLUSTER].rem);

/** A test network: its SOL is free and worthless. */
export const IS_TEST = CLUSTER !== "mainnet";

export const explorer = (address: string) =>
  `https://explorer.solana.com/address/${address}${CLUSTER === "devnet" ? "?cluster=devnet" : CLUSTER === "local" ? "?cluster=custom&customUrl=http%3A%2F%2F127.0.0.1%3A8899" : ""}`;
