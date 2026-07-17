// How an attachment's type decides its preview. An attachment is unvetted bytes the user brought in,
// so what may be rendered inside the webview is settled by an allowlist: a type not listed here is
// not rendered. Types that can execute (SVG / HTML / XHTML / XML / JavaScript) are shown as source
// rather than rendered. This table and the serving side's allowlist (`served_content_type` in
// blobproto.rs) are a pair — widening only one of them renders nothing.

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
