import { describe, expect, it } from "vitest";
import { previewKind } from "./attachmentView";

describe("previewKind", () => {
  it("keeps executable types off the render surface and falls back to source view", () => {
    for (const m of [
      "image/svg+xml",
      "text/html",
      "application/xhtml+xml",
      "application/xml",
      "text/xml",
      "text/javascript",
      "application/javascript",
      "IMAGE/SVG+XML; charset=utf-8", // case and parameters must not let it slip through
    ]) {
      expect(previewKind(m), m).toBe("text");
    }
  });

  it("routes renderable types to their respective surfaces", () => {
    expect(previewKind("image/png")).toBe("image");
    expect(previewKind("audio/mpeg")).toBe("audio");
    expect(previewKind("video/mp4")).toBe("video");
    expect(previewKind("application/pdf")).toBe("pdf");
    expect(previewKind("text/markdown")).toBe("markdown");
    expect(previewKind("text/csv")).toBe("csv");
    expect(previewKind("text/tab-separated-values")).toBe("tsv");
    expect(previewKind("text/plain")).toBe("text");
    expect(previewKind("application/json")).toBe("text");
  });

  it("does not render unknown or unset types (defers to \"open externally\")", () => {
    expect(previewKind("application/zip")).toBe("none");
    expect(previewKind("application/x-msdownload")).toBe("none");
    expect(previewKind(null)).toBe("none");
    expect(previewKind("")).toBe("none");
  });
});
