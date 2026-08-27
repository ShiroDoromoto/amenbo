// Stream URLs for attachment blobs.
//
// A custom protocol on the Rust side (`amenboblob`, app/src-tauri/src/blobproto.rs) serves the
// bytes out of the blob store with Range support. Viewers hand the URL built here to the `src` of
// an `<img>/<audio>/<video>/<iframe>`, so large files stream in instead of being inlined as data
// URLs. Where that protocol lives differs by platform, which `customScheme.ts` absorbs.
import { schemeBase } from "./customScheme";

const SCHEME = "amenboblob";

/**
 * Builds the stream URL for a blob. `mime` rides along in the query string as the `Content-Type`,
 * so the served type matches the one the viewer picked its renderer from.
 */
export function blobUrl(hash: string, mime: string | null | undefined): string {
  const q = mime ? `?mime=${encodeURIComponent(mime)}` : "";
  return `${schemeBase(SCHEME)}/${encodeURIComponent(hash)}${q}`;
}
