# Running Romulus and Remus on devnet

Devnet is Solana's free test network: its SOL has no value. Everything here was first run end to
end on a local Solana validator (21 of 21 checks passed, including a full curve graduating).

## What you need

1. A computer with a terminal. On Windows, use WSL (Ubuntu); on a Mac, the Terminal app.
2. Node.js 22: https://nodejs.org (the "LTS" installer).
3. The Solana command-line tools, version 4.3.0:
   `sh -c "$(curl -sSfL https://release.anza.xyz/v4.3.0/install)"`
4. This kit, unzipped. It holds the five built programs (`programs/`), their devnet keys
   (`keys/`, including `deployer.json`, the devnet admin), and this client.
5. About 11 devnet SOL on the deployer: around 9.5 to deploy and the rest for the test
   wallets. Get it free at https://faucet.solana.com for the address that
   `solana-keygen pubkey keys/deployer.json` prints (a few requests may be needed).

## Steps

Run these from the unzipped kit's folder.

1. Deploy the five programs (takes a few minutes; it can be re-run if interrupted):

   ```sh
   bash deploy-devnet.sh
   ```

2. Install the client, then set up both twins without opening trading:

   ```sh
   cd client
   npm install
   npx tsx scripts/setup.ts https://api.devnet.solana.com ../keys/deployer.json
   ```

3. Open trading and, straight away, run the checks:

   ```sh
   npx tsx scripts/setup.ts https://api.devnet.solana.com ../keys/deployer.json --open
   npx tsx scripts/scenario.ts https://api.devnet.solana.com ../keys/deployer.json
   ```

   Each line says PASS or FAIL. Graduation is left out on devnet, because buying out a whole
   curve takes about 35 SOL; it passed on the local validator.

The addresses of ROM, REM and their pools are written to `client/deployments/devnet.json`.
