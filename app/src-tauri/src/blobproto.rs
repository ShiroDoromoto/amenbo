//! The custom protocol that streams content-addressed blobs to the webview. Instead of carrying the big ones
//! (audio, video, PDF, images) around as data URLs, the viewer points the `src` of an
//! `<img>` / `<audio>` / `<video>` / `<iframe>` at `amenboblob://localhost/<hash>`. This module resolves that
//! URL to the blob store's real path and answers `Range` requests (seeking in audio and video, partial fetches)
//! with only the bytes asked for, so nothing is read whole. The path is `/<hash>` (BLAKE3), and being a hash is
//! the whole of the addressing: **there is no way to name a place from here.** The `?mime=<type>` query is what
//! the viewer **asks** for, not a fact about the bytes — nobody has verified the content — so what is served is
//! decided by [`crate::webproto`], which every custom-protocol answer goes through.
//!
//! Blobs are read, never written, and neither the engine nor the Live model is opened.

use tauri::http::{header, Request, Response, StatusCode};

use crate::webproto::{empty, hardened, parse_range, query_param, served_content_type};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_validation_blocks_traversal() {
        assert!(is_hash(&"a".repeat(64)));
        assert!(!is_hash("../../etc/passwd"));
        assert!(!is_hash("abc"));
        assert!(!is_hash(&"g".repeat(64)));
    }
}
