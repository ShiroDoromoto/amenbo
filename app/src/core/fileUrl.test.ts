// The address the picture viewer hands an `<img>`. What is held here is that every part of it
// arrives at the host the way the fence there expects to read it: the host splits the path on `/`
// before it decodes anything, so a separator that was part of a name — and the folder's own
// absolute path is nothing but separators — must not reach it as one (`crate::fileproto`).
import { afterEach, describe, expect, it, vi } from "vitest";
import { fileUrl } from "./fileUrl";

const WINDOWS = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

afterEach(() => { vi.unstubAllGlobals(); });

describe("the address of a file under a project's folder", () => {
  it("hides the folder's own separators, so the root stays one part of the path", () => {
    const url = fileUrl(7, "/Users/alice/work", ["notes", "a.png"], "image/png");
    expect(url).toBe("amenbofile://localhost/7/%2FUsers%2Falice%2Fwork/notes/a.png?mime=image%2Fpng");
  });

  it("encodes each segment on its own, and leaves the separators between them alone", () => {
    const url = fileUrl(1, "/w", ["a b", "c/d.png"], "image/png");
    // The space and the separator inside a name are encoded; the one this module wrote is not.
    expect(url).toBe("amenbofile://localhost/1/%2Fw/a%20b/c%2Fd.png?mime=image%2Fpng");
  });

  it("asks for nothing in particular where the type was never sniffed", () => {
    expect(fileUrl(1, "/w", ["a.png"], null)).toBe("amenbofile://localhost/1/%2Fw/a.png");
    expect(fileUrl(1, "/w", ["a.png"], undefined)).toBe("amenbofile://localhost/1/%2Fw/a.png");
  });

  it("moves to the origin Windows serves a custom scheme at", () => {
    vi.stubGlobal("navigator", { userAgent: WINDOWS });
    expect(fileUrl(2, "C:\\work", ["a.png"], "image/png"))
      .toBe("http://amenbofile.localhost/2/C%3A%5Cwork/a.png?mime=image%2Fpng");
  });
});
