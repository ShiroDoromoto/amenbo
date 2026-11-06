// What every PDF is opened with. The rest of this module needs a canvas and a worker, which jsdom
// does not have — but the switch that keeps WebAssembly from running is decided before either, and
// nothing else would notice it going: the wasm guard reads files, and a PostScript function compiled
// to wasm at run time is never a file.
import { describe, expect, it } from "vitest";
import { READER_OPTIONS } from "./pdfLoad";

describe("what a PDF is opened with", () => {
  it("runs no WebAssembly", () => {
    expect(READER_OPTIONS.useWasm).toBe(false);
  });

  it("looks for the JavaScript decoders in the folder the build emits them into", () => {
    // `vite.config.ts` emits `openjpeg_nowasm_fallback.js` and `jbig2_nowasm_fallback.js` there.
    expect(READER_OPTIONS.wasmUrl).toBe("/pdfjs/");
  });

  it("hands over no wasm file when one is asked for", async () => {
    const carried = new READER_OPTIONS.BinaryDataFactory();
    await expect(carried.fetch({ kind: "wasmUrl", filename: "openjpeg.wasm" })).rejects.toThrow(
      "no wasmUrl file called openjpeg.wasm is shipped",
    );
  });
});
