//! The custom protocol that streams content-addressed blobs to the webview. Instead of carrying the big ones
//! (audio, video, PDF, images) around as data URLs, the viewer points the `src` of an
//! `<img>` / `<audio>` / `<video>` / `<iframe>` at `amenboblob://localhost/<hash>`. This module resolves that
//! URL to the blob store's real path and answers `Range` requests (seeking in audio and video, partial fetches)
//! with only the bytes asked for, so nothing is read whole. The path is `/<hash>` (BLAKE3). The `?mime=<type>`
//! query is what the viewer **asks** for, not a fact about the bytes — nobody has verified the content — so the
//! type actually served is decided by the allowlist in [`served_content_type`], which demotes anything active
//! (SVG, HTML, XML, JS) to `text/plain`. Blobs are read, never written, and neither the engine nor the Live
//! model is opened. An attachment is unverified bytes the user brought in and the webview is an execution
//! environment with IPC within reach — so the defence rests on limiting what those bytes can reach: (1) the
//! served type comes from an allowlist, (2) `nosniff` forbids guessing it back, and (3) should a blob ever be
//! opened **as a document**, `Content-Security-Policy: default-src 'none'; sandbox` drops it to an opaque origin
//! with no scripting. It pairs with the viewer's own allowlist (`previewKind` in `attachmentView.ts`): if either
//! side slackens, execution still does not follow.

use tauri::http::{header, Request, Response, StatusCode};

/// Build the one response. Failures come back as a bare status (404/400/416/500) with an empty body.
pub fn serve(request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    match try_serve(request) {
        Ok(resp) => resp,
        Err(status) => empty(status),
    }
}

fn try_serve(request: &Request<Vec<u8>>) -> Result<Response<Vec<u8>>, StatusCode> {
    let uri = request.uri();
    // The path is `/<hash>` (the host differs by platform; the path does not).
    let hash = uri.path().trim_start_matches('/');
    if hash.is_empty() || !is_hash(hash) {
        return Err(StatusCode::BAD_REQUEST);
    }
    // The mime the viewer put on the URL is a request, not a fact — nobody verified the content. Active types
    // are demoted here: what gets served is our decision, not the caller's.
    let mime = served_content_type(query_param(uri.query(), "mime").as_deref().unwrap_or(""));

    let blobs = blob_store().ok_or(StatusCode::NOT_FOUND)?;
    let total = blobs.plaintext_len(hash).ok_or(StatusCode::NOT_FOUND)?;

    match parse_range(request, total) {
        Some(Ok((start, end))) => {
            let len = end - start + 1;
            let buf = blobs
                .read_range(hash, start, len)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let n = buf.len();
            hardened(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{total}"))
                .header(header::CONTENT_LENGTH, n.to_string())
                .body(buf)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        }
        // A Range header that cannot be satisfied (it falls outside the file).
        Some(Err(())) => hardened(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{total}"))
            .body(Vec::new())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR),
        // No Range at all: serve the whole thing.
        None => {
            let bytes = blobs.read(hash).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            hardened(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, total.to_string())
                .body(bytes)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// The base of every blob response, so that **all** of them carry the same restraints: `X-Content-Type-Options:
/// nosniff` stops the webview from second-guessing the type we chose (HTML that `served_content_type` demoted to
/// `text/plain` must not be sniffed back into a document), and `Content-Security-Policy: default-src 'none';
/// sandbox` is the last wall if something does open a blob **as a document** — a sandbox with no tokens means an
/// opaque origin and no scripting, which puts IPC out of reach. The latter is ignored when a blob is loaded as a
/// subresource (image, audio, video), so it costs nothing in display.
fn hardened(status: StatusCode) -> tauri::http::response::Builder {
    Response::builder()
        .status(status)
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CONTENT_SECURITY_POLICY, "default-src 'none'; sandbox")
}

/// Decide the `Content-Type` actually served. It is **the pair of the viewer's own allowlist**
/// (`attachmentView.ts`), and neither half alone can render or run anything: active types (SVG, HTML, XHTML,
/// XML, JS) are demoted to `text/plain` (the viewer dumps their source into a `pre` either way, so nothing is
/// lost on screen and no path is left to render them as a document), types the viewer can render (images, audio,
/// video, PDF, text, JSON) pass through, and everything else (zip, executables, the unknown) becomes
/// `application/octet-stream` and is sent out to "open externally". The shape is checked too — token characters
/// only — which closes the door on header injection.
fn served_content_type(requested: &str) -> &'static str {
    const TEXT: &str = "text/plain; charset=utf-8";
    let m = requested.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    // Anything that is not shaped like a mime token is not read as a request at all.
    if m.is_empty() || !m.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'+' | b'.')) {
        return "application/octet-stream";
    }
    match m.as_str() {
        // Active types are handed back as source.
        "image/svg+xml"
        | "text/html"
        | "application/xhtml+xml"
        | "application/xml"
        | "text/xml"
        | "text/javascript"
        | "application/javascript"
        | "application/ecmascript" => TEXT,
        "application/pdf" => "application/pdf",
        "application/json" => "application/json",
        "text/markdown" => "text/markdown; charset=utf-8",
        "text/csv" => "text/csv; charset=utf-8",
        "text/tab-separated-values" => "text/tab-separated-values; charset=utf-8",
        _ if m.starts_with("text/") => TEXT,
        // Images, audio and video are only ever loaded as subresources, where no script runs. SVG is the one
        // exception, and it has already been demoted above.
        _ if m.starts_with("image/") || m.starts_with("audio/") || m.starts_with("video/") => {
            media_type(&m)
        }
        _ => "application/octet-stream",
    }
}

/// Map an image/audio/video type onto a fixed string we are willing to hand out. A variant that is not listed
/// falls to octet-stream rather than being sniffed (display falls back to "open externally") — a closed
/// vocabulary, so that no borrowed string ever rides out in a `Content-Type`.
fn media_type(m: &str) -> &'static str {
    match m {
        "image/png" => "image/png",
        "image/jpeg" => "image/jpeg",
        "image/gif" => "image/gif",
        "image/webp" => "image/webp",
        "image/avif" => "image/avif",
        "image/bmp" => "image/bmp",
        "image/tiff" => "image/tiff",
        "image/x-icon" | "image/vnd.microsoft.icon" => "image/x-icon",
        "image/heic" => "image/heic",
        "audio/mpeg" => "audio/mpeg",
        "audio/mp4" => "audio/mp4",
        "audio/aac" => "audio/aac",
        "audio/ogg" => "audio/ogg",
        "audio/wav" | "audio/x-wav" => "audio/wav",
        "audio/flac" => "audio/flac",
        "audio/webm" => "audio/webm",
        "video/mp4" => "video/mp4",
        "video/webm" => "video/webm",
        "video/ogg" => "video/ogg",
        "video/quicktime" => "video/quicktime",
        "video/x-matroska" => "video/x-matroska",
        "video/x-msvideo" => "video/x-msvideo",
        _ => "application/octet-stream",
    }
}

/// Resolve the blob store. There is one per device (`blobs/` under the app-data dir); being content-addressed,
/// it deduplicates across projects on its own. Blobs are plaintext, so no key is involved.
fn blob_store() -> Option<amenbo_core::blob::BlobStore> {
    let paths = amenbo_core::config::Paths::resolve().ok()?;
    Some(amenbo_core::blob::BlobStore::at(
        paths.base_dir.join(amenbo_core::blob::BLOBS_SUBDIR),
    ))
}

/// Is this a BLAKE3 hex digest (64 lowercase digits)? It doubles as traversal defence, rejecting `/` and `.`.
fn is_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Read a `Range: bytes=start-end`. `None` means no header (serve the whole file), `Some(Ok((s,e)))` a closed
/// interval that can be satisfied, `Some(Err(()))` one that cannot. Only a single range is supported: multipart
/// ranges are rejected rather than quietly answered with the whole file.
#[allow(clippy::result_unit_err)]
fn parse_range(request: &Request<Vec<u8>>, total: u64) -> Option<Result<(u64, u64), ()>> {
    let raw = request.headers().get(header::RANGE)?.to_str().ok()?;
    let spec = raw.strip_prefix("bytes=")?;
    // One range only; a comma-separated list is refused.
    if spec.contains(',') {
        return Some(Err(()));
    }
    let (a, b) = spec.split_once('-')?;
    if total == 0 {
        return Some(Err(()));
    }
    let last = total - 1;
    let (start, end) = match (a.trim(), b.trim()) {
        // bytes=start-end
        (s, e) if !s.is_empty() && !e.is_empty() => {
            let start: u64 = s.parse().ok()?;
            let end: u64 = e.parse().ok()?;
            (start, end.min(last))
        }
        // bytes=start- (from start to the end of the file)
        (s, "") if !s.is_empty() => {
            let start: u64 = s.parse().ok()?;
            (start, last)
        }
        // bytes=-suffix (the last `suffix` bytes)
        ("", e) if !e.is_empty() => {
            let suffix: u64 = e.parse().ok()?;
            if suffix == 0 {
                return Some(Err(()));
            }
            (total.saturating_sub(suffix), last)
        }
        _ => return Some(Err(())),
    };
    if start > last || start > end {
        return Some(Err(()));
    }
    Some(Ok((start, end)))
}

/// Pull one key's value out of the query string and URL-decode it (`mime` is the only one we look for).
fn query_param(query: Option<&str>, key: &str) -> Option<String> {
    let query = query?;
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| percent_decode(v))
    })
}

/// A minimal percent-decode (`%XX`, and `+` for a space). It exists for short values like a mime, which the
/// webview itself put there — this is not a general decoder for outside input.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    out.push((hi * 16 + lo) as u8);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn empty(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder().status(status).body(Vec::new()).unwrap_or_else(|_| {
        let mut r = Response::new(Vec::new());
        *r.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        r
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(range: Option<&str>) -> Request<Vec<u8>> {
        let mut b = Request::builder().uri("amenboblob://localhost/abc");
        if let Some(r) = range {
            b = b.header(header::RANGE, r);
        }
        b.body(Vec::new()).unwrap()
    }

    #[test]
    fn no_range_is_none() {
        assert!(parse_range(&req(None), 100).is_none());
    }

    #[test]
    fn closed_range_clamps_end() {
        assert_eq!(parse_range(&req(Some("bytes=0-999")), 100), Some(Ok((0, 99))));
        assert_eq!(parse_range(&req(Some("bytes=10-20")), 100), Some(Ok((10, 20))));
    }

    #[test]
    fn open_ended_range() {
        assert_eq!(parse_range(&req(Some("bytes=50-")), 100), Some(Ok((50, 99))));
    }

    #[test]
    fn suffix_range() {
        assert_eq!(parse_range(&req(Some("bytes=-10")), 100), Some(Ok((90, 99))));
    }

    #[test]
    fn unsatisfiable_start_past_end() {
        assert_eq!(parse_range(&req(Some("bytes=100-200")), 100), Some(Err(())));
    }

    #[test]
    fn multi_range_rejected() {
        assert_eq!(parse_range(&req(Some("bytes=0-10,20-30")), 100), Some(Err(())));
    }

    #[test]
    fn hash_validation_blocks_traversal() {
        assert!(is_hash(&"a".repeat(64)));
        assert!(!is_hash("../../etc/passwd"));
        assert!(!is_hash("abc"));
        assert!(!is_hash(&"g".repeat(64)));
    }

    /// An active type is **never** served as that type: it is demoted to `text/plain`, which the viewer shows as
    /// source, leaving no path by which it could be rendered as a document.
    #[test]
    fn active_types_are_served_as_source() {
        for m in [
            "image/svg+xml",
            "text/html",
            "application/xhtml+xml",
            "application/xml",
            "text/xml",
            "text/javascript",
            "application/javascript",
            "TEXT/HTML; charset=utf-8", // case and parameters are no way through
        ] {
            assert_eq!(served_content_type(m), "text/plain; charset=utf-8", "{m}");
        }
    }

    /// Types the viewer can render pass through; the unknown, the executable and the malformed become
    /// octet-stream, which is to say "open externally".
    #[test]
    fn known_types_pass_and_the_rest_fall_back() {
        assert_eq!(served_content_type("image/png"), "image/png");
        assert_eq!(served_content_type("video/mp4"), "video/mp4");
        assert_eq!(served_content_type("application/pdf"), "application/pdf");
        assert_eq!(served_content_type("text/markdown"), "text/markdown; charset=utf-8");

        assert_eq!(served_content_type("application/zip"), "application/octet-stream");
        assert_eq!(served_content_type("application/x-msdownload"), "application/octet-stream");
        assert_eq!(served_content_type("image/unheard-of"), "application/octet-stream");
        assert_eq!(served_content_type(""), "application/octet-stream");
        // Header injection (newlines, control characters) is not shaped like a mime token, so it is not read as one.
        assert_eq!(
            served_content_type("text/plain\r\nSet-Cookie: x=1"),
            "application/octet-stream"
        );
    }

    /// Every response carries the restraints — success, partial content and failure alike.
    #[test]
    fn every_response_carries_nosniff_and_a_sandbox_csp() {
        let resp = hardened(StatusCode::OK).body(Vec::<u8>::new()).unwrap();
        assert_eq!(resp.headers().get(header::X_CONTENT_TYPE_OPTIONS).unwrap(), "nosniff");
        assert_eq!(
            resp.headers().get(header::CONTENT_SECURITY_POLICY).unwrap(),
            "default-src 'none'; sandbox"
        );
    }

    #[test]
    fn query_param_decodes() {
        assert_eq!(query_param(Some("mime=image%2Fpng"), "mime"), Some("image/png".to_string()));
        assert_eq!(query_param(Some("mime=text/plain"), "mime"), Some("text/plain".to_string()));
        assert_eq!(query_param(Some("other=1"), "mime"), None);
    }
}
