#!/usr/bin/env bash
# Deploys or upgrades the Tiber programs on a cluster, one at a time (so the temporary deploy buffer
# never needs more than the largest program's rent on top of what is already locked).
#
#   deploy.sh <local|devnet|mainnet> <deployer keypair> [program...]
#
# Build first (scripts/solana/programs.sh build). The program keypairs come from
# keys/<cluster>/<program>-keypair.json (local uses the devnet ones); KEYS_DIR overrides that
# (mainnet: generate fresh keypairs, update every declare_id! and Anchor.toml, rebuild, and keep
# the keys off this machine). A first deploy needs the program keypair to sign; an upgrade needs
# only the upgrade authority. BUFFER_DIR, when set, writes each deploy through
# <BUFFER_DIR>/buffer-<program>.json, so an interrupted deploy resumes with the same buffer and
# its rent can be recovered with `solana program close <buffer>`. SOLANA_DEPLOY_FLAGS is passed on
# to every `solana program deploy` (on mainnet, a priority fee:
# SOLANA_DEPLOY_FLAGS="--with-compute-unit-price 50000 --use-rpc").
set -euo pipefail

CLUSTER="${1:-}"
DEPLOYER="${2:-}"
[[ -n "$CLUSTER" && -n "$DEPLOYER" ]] || { sed -n '2,16p' "${BASH_SOURCE[0]}" >&2; exit 2; }
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
AGAVE_BIN="$HOME/.local/share/solana/install/releases/v4.3.0/solana-release/bin"
export PATH="$AGAVE_BIN:$PATH"
case "$CLUSTER" in
  local) URL="${SOLANA_LOCAL_RPC:-http://127.0.0.1:8899}"; KEYS_CLUSTER=devnet ;;
  devnet) URL="${SOLANA_DEVNET_RPC:-https://api.devnet.solana.com}"; KEYS_CLUSTER=devnet ;;
  mainnet) URL="${SOLANA_MAINNET_RPC:?set SOLANA_MAINNET_RPC to a keyed mainnet RPC}"; KEYS_CLUSTER=mainnet ;;
  *) echo "cluster must be local, devnet or mainnet" >&2; exit 2 ;;
esac
PROGRAMS=(tiber_token tiber_swap tiber_bridge tiber_twins tiber_tithe)
if (($# > 2)); then PROGRAMS=("${@:3}"); fi
KEYS="${KEYS_DIR:-$ROOT/keys/$KEYS_CLUSTER}"

echo "deployer $(solana-keygen pubkey "$DEPLOYER"), balance $(solana balance -u "$URL" -k "$DEPLOYER")"
for program in "${PROGRAMS[@]}"; do
  key="$KEYS/$program-keypair.json"
  so="$ROOT/target/deploy/$program.so"
  [[ -f "$key" ]] || { echo "missing $key" >&2; exit 1; }
  [[ -f "$so" ]] || { echo "missing $so (build first)" >&2; exit 1; }
  id="$(solana-keygen pubkey "$key")"
  size="$(stat -c %s "$so")"
  echo "==> $program $id ($size bytes)"
  buffer_flags=()
  if [[ -n "${BUFFER_DIR:-}" ]]; then
    buffer="$BUFFER_DIR/buffer-$program.json"
    [[ -f "$buffer" ]] || solana-keygen new --no-bip39-passphrase --silent -o "$buffer" >/dev/null
    buffer_flags=(--buffer "$buffer")
  fi
  if solana program show -u "$URL" "$id" >/dev/null 2>&1; then
    # An upgrade: the ProgramData must hold the new code (extend it first; anyone may).
    have="$(solana program show -u "$URL" "$id" --output json | python3 -c 'import json,sys; print(json.load(sys.stdin).get("dataLen", 0))')"
    if ((size > have)); then
      echo "the ProgramData holds $have bytes; extend it first: solana program extend -u <url> -k <payer> $id $((size - have))" >&2
      exit 1
    fi
    solana program deploy -u "$URL" -k "$DEPLOYER" --program-id "$id" --upgrade-authority "$DEPLOYER" --max-sign-attempts 30 "${buffer_flags[@]}" ${SOLANA_DEPLOY_FLAGS:-} "$so"
  else
    # --max-len: exactly the program's size, so no rent is locked for space it does not use.
    solana program deploy -u "$URL" -k "$DEPLOYER" --program-id "$key" --upgrade-authority "$DEPLOYER" --max-len "$size" --max-sign-attempts 30 "${buffer_flags[@]}" ${SOLANA_DEPLOY_FLAGS:-} "$so"
  fi
done
echo "done; balance $(solana balance -u "$URL" -k "$DEPLOYER")"
