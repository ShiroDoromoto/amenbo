import { readFileSync } from "node:fs";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

/**
 * The two decoders pdf.js falls back to when it is told not to use WebAssembly, served from a
 * folder of our own under the names pdf.js asks for (`src/files/pdfLoad.ts`).
 *
 * pdf.js's worker reaches them with `import(wasmUrl + "openjpeg_nowasm_fallback.js")` (and the same
 * for `jbig2_nowasm_fallback.js`), so the names are fixed by the library. An import with `?url` would
 * give each a hashed name the worker cannot guess, so they are emitted here under their own. The four
 * `.wasm` files beside them in the package are not carried — the reader never runs them
 * (`guards/check-no-wasm.sh`).
 *
 * What they are built from is OpenJPEG, under BSD-2-Clause (`wasm/LICENSE_OPENJPEG` inside
 * pdfjs-dist), and PDFium's JBIG2 decoder, under BSD-3-Clause and Apache-2.0
 * (`wasm/LICENSE_JBIG2`). pdf.js's own wrapping is BSD-2-Clause and Apache-2.0
 * (`wasm/LICENSE_PDFJS_*`). All of them are on `deny.toml`'s allow-list.
 */
const PDF_FALLBACK_DIR = "pdfjs/";
const PDF_FALLBACKS = ["openjpeg_nowasm_fallback.js", "jbig2_nowasm_fallback.js"];
const pdfFallback = (name: string) =>
  readFileSync(new URL(`./node_modules/pdfjs-dist/wasm/${name}`, import.meta.url));

function pdfFallbacks(): Plugin {
  return {
    name: "pdfjs-nowasm-fallbacks",
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const name = PDF_FALLBACKS.find((one) => req.url?.split("?")[0] === `/${PDF_FALLBACK_DIR}${one}`);
        if (name === undefined) {
          next();
          return;
        }
        res.setHeader("Content-Type", "text/javascript");
        res.end(pdfFallback(name));
      });
    },
    generateBundle() {
      for (const name of PDF_FALLBACKS) {
        this.emitFile({ type: "asset", fileName: `${PDF_FALLBACK_DIR}${name}`, source: pdfFallback(name) });
      }
    },
  };
}

// Vite dev server. `npm run dev` stays a plain browser launch (for iterating on the front end alone), while
// `npm run tauri dev` drives the same server (Tauri waits on devUrl=:5180, hence strictPort).
export default defineConfig({
  plugins: [react(), pdfFallbacks()],
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
