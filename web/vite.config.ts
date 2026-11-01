import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { nodePolyfills } from "vite-plugin-node-polyfills";

// `base` is the path the site is served from (GitHub Pages serves it under /<repo>/).
export default defineConfig({
  base: process.env.VITE_BASE ?? "/",
  plugins: [react(), nodePolyfills({ include: ["buffer", "crypto", "stream", "util"], globals: { Buffer: true } })],
  build: { chunkSizeWarningLimit: 2000 },
});
