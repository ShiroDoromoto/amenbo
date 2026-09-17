// How an attachment's type decides its preview. An attachment is unvetted bytes the user brought in,
// so what may be rendered inside the webview is settled by an allowlist: a type not listed here is
// not rendered. Types that can execute (SVG / HTML / XHTML / XML / JavaScript) are shown as source
// rather than rendered. This table and the serving side's allowlist (`served_content_type` in
// webproto.rs) are a pair — widening only one of them renders nothing. That allowlist is the serving
// side of every custom protocol, not of attachments alone, so this table is what a file read out of a
// session's folder passes through too.

/**
 * The largest PDF an attachment is drawn from, in bytes.
 *
 * **It is the pair of `PDF_CAP` in `app/src-tauri/src/folder_bytes.rs`** (`AMB-D-907`), and it is
 * held here because nothing on the way refuses one for the reader: a file in a folder is measured by
 * the host before the panel is told about it, while an attachment is answered for by its hash alone
 * and the door hands out whatever is under it. What the cap guards is the same either way — the
 * bytes this window holds to draw a document it was handed whole.
 */
export const PDF_PREVIEW_CAP = 64 * 1024 * 1024;

/** Is this attachment small enough to draw? A size nobody recorded is taken as one that fits. */
export function pdfFitsThePane(sizeBytes: bigint | number | null | undefined): boolean {
  if (sizeBytes === null || sizeBytes === undefined) return true;
  return Number(sizeBytes) <= PDF_PREVIEW_CAP;
}

/** How a preview is drawn. `none` is not drawn in the webview at all; it goes out to "open externally". */
export type PreviewKind = "image" | "audio" | "video" | "pdf" | "markdown" | "csv" | "tsv" | "text" | "none";

/** Types that can execute script. They never reach a rendering surface (an img or iframe); they drop to a source view. */
const ACTIVE = new Set([
  "image/svg+xml",
  "text/html",
  "application/xhtml+xml",
  "application/xml",
  "text/xml",
  "text/javascript",
  "application/javascript",
  "application/ecmascript",
]);

/** Text types that may be drawn as they are — as `<pre>`, a table, or Markdown. */
const TEXT: Record<string, PreviewKind> = {
  "text/markdown": "markdown",
  "text/csv": "csv",
  "text/tab-separated-values": "tsv",
  "application/json": "text",
};

/** Map a `mime` to how it previews. Parameters (`; charset=…`) are dropped; unknown and dangerous fall to the safe side. */
export function previewKind(mime: string | null | undefined): PreviewKind {
  const m = (mime ?? "").split(";")[0].trim().toLowerCase();
  if (m === "") return "none";
  if (ACTIVE.has(m)) return "text"; // Never execute it; show the source and nothing else
  if (m === "application/pdf") return "pdf";
  if (m.startsWith("image/")) return "image";
  if (m.startsWith("audio/")) return "audio";
  if (m.startsWith("video/")) return "video";
  if (m in TEXT) return TEXT[m];
  if (m.startsWith("text/")) return "text";
  return "none";
}
