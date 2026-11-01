import { StrictMode, useMemo } from "react";
import { createRoot } from "react-dom/client";
import { ConnectionProvider, WalletProvider } from "@solana/wallet-adapter-react";
import { WalletModalProvider } from "@solana/wallet-adapter-react-ui";
import { UnsafeBurnerWalletAdapter } from "@solana/wallet-adapter-unsafe-burner";
import "@solana/wallet-adapter-react-ui/styles.css";
import "./styles.css";
import App from "./App";
import { CLUSTER, RPC_URL } from "./config";

function Root() {
  // Phantom, Solflare, Backpack and other modern wallets announce themselves (Wallet Standard),
  // so none need listing. On a local test network a throwaway in-browser wallet is offered too.
  const wallets = useMemo(() => (CLUSTER === "local" ? [new UnsafeBurnerWalletAdapter()] : []), []);
  return (
    <ConnectionProvider endpoint={RPC_URL} config={{ commitment: "confirmed", disableRetryOnRateLimit: true }}>
      <WalletProvider wallets={wallets} autoConnect>
        <WalletModalProvider>
          <App />
        </WalletModalProvider>
      </WalletProvider>
    </ConnectionProvider>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Root />
  </StrictMode>,
);
