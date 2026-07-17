// Stream URLs for attachment blobs.
//
// A custom protocol on the Rust side (`amenboblob`, app/src-tauri/src/blobproto.rs) serves the
// bytes out of the blob store with Range support. Viewers hand the URL built here to the `src` of
// an `<img>/<audio>/<video>/<iframe>`, so large files stream in instead of being inlined as data
// URLs. The host form of the URL differs by platform (`scheme://localhost/…` on macOS/Linux,
// `http://scheme.localhost/…` on Windows/Android); this module absorbs that difference.

const SCHEME = "amenboblob";

/** `http://<scheme>.localhost` on Windows/Android, `<scheme>://localhost` everywhere else. */
function base(): string {
  const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
  const isWindowsLike = /Windows|Android/i.test(ua);
  return isWindowsLike ? `http://${SCHEME}.localhost` : `${SCHEME}://localhost`;
}

/**
 * Builds the stream URL for a blob. `mime` rides along in the query string as the `Content-Type`,
 * so the served type matches the one the viewer picked its renderer from.
 */
export function blobUrl(hash: string, mime: string | null | undefined): string {
  const q = mime ? `?mime=${encodeURIComponent(mime)}` : "";
  return `${base()}/${encodeURIComponent(hash)}${q}`;
}
