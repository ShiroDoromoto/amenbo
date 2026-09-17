import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Vite dev server. `npm run dev` stays a plain browser launch (for iterating on the front end alone), while
// `npm run tauri dev` drives the same server (Tauri waits on devUrl=:5180, hence strictPort).
export default defineConfig({
  plugins: [react()],
  // Tauri assumes a fixed port (get it wrong and you get a white screen).
  server: { port: 5180, strictPort: true },
  clearScreen: false,
  // Vite knows nothing about `.bcmap` or `.pfb`, the forms pdf.js publishes its character maps and
  // its standard fonts in, and an extension it does not know is not an asset it will carry. Named
  // here they are — and the reader imports each one with `?inline`, so what ships is data inside a
  // module rather than a file to go and fetch (`src/files/pdfLoad.ts`).
  assetsInclude: ["**/*.bcmap", "**/*.pfb"],
  build: {
    // One entry per window (`AMB-T-3588`): the board is the root document, the talk window its own.
    // Naming them both is what keeps the second one out of the build — Vite takes `index.html` alone
    // by default, and a window whose page was never emitted opens on a 404.
    rollupOptions: { input: { main: "index.html", talk: "talk.html" } },
  },
});
