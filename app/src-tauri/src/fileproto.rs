//! The custom protocol that hands the webview a file **by its path**, and the fence that keeps that from
//! meaning any path.
//!
//! [`crate::blobproto`] can only be asked for a hash, so it has no way to name a place: the worst a caller
//! can do there is name bytes that are already in the store. This door is the opposite shape — it takes a
//! path — and that makes it the one place in the app where the webview could reach into the filesystem at
//! large. It is therefore not a general file server, and the address is not a path on its own:
//!
//! ```text
//! amenbofile://localhost/<session>/<segment>/<segment>…?mime=<type>
//! ```
//!
//! **A session, and then a path under that session's folder.** The session is a terminal this app opened
//! ([`crate::pty`]), and its folder is the one the person chose when they opened it. Nothing else is
//! reachable: not a sibling of that folder, not the app's own data, not the home directory — and not the
//! folder of a session that has since closed, because a session that is gone answers nothing at all.
//!
//! **What the fence is made of**, in the order it is applied:
//!
//! 1. The session has to name a terminal this app has open, and that terminal has to have a folder.
//! 2. Every segment must be a single ordinary name. `..`, `.`, an embedded separator, a drive letter and an
//!    absolute path are each rejected as a segment rather than resolved and then judged.
//! 3. What the segments add up to is resolved on the real filesystem, symbolic links and all, and must still
//!    be inside the folder. A link pointing out of it is followed and then refused, which is the only order
//!    that catches one.
//! 4. It must be a regular file. A directory is not listed here — what a folder holds is a question for the
//!    store, not for a door that hands out bytes.
//!
//! What is served then goes through [`crate::webproto`] like every other answer: the type comes from the
//! allowlist, active types are demoted to source, and nothing may be sniffed back.

use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Component, Path, PathBuf};

use tauri::http::{header, Request, Response, StatusCode};

use crate::webproto::{empty, hardened, parse_range, percent_decode, query_param, served_content_type};

/// Build the one response. Failures come back as a bare status (404/400/416/500) with an empty body: which
/// of the fence's rules turned a request away is not something a caller is told, because the difference
/// between "outside the folder" and "not there" is itself an answer about the filesystem.
pub fn serve(app: &tauri::AppHandle, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    match try_serve(app, request) {
        Ok(resp) => resp,
        Err(status) => empty(status),
    }
}

fn try_serve(
    app: &tauri::AppHandle,
    request: &Request<Vec<u8>>,
) -> Result<Response<Vec<u8>>, StatusCode> {
    let uri = request.uri();
    // The path is `/<session>/<segments…>` (the host differs by platform; the path does not).
    let (session, rest) = uri
        .path()
        .trim_start_matches('/')
        .split_once('/')
        .ok_or(StatusCode::BAD_REQUEST)?;
    let root = crate::pty::folder(app, session).ok_or(StatusCode::NOT_FOUND)?;
    let path = under(&root, rest).ok_or(StatusCode::NOT_FOUND)?;

    // The mime the viewer put on the URL is a request, not a fact — nobody has read these bytes. Active types
    // are demoted here: what gets served is our decision, not the caller's.
    let mime = served_content_type(query_param(uri.query(), "mime").as_deref().unwrap_or(""));

    let total = std::fs::metadata(&path)
        .map_err(|_| StatusCode::NOT_FOUND)?
        .len();

    match parse_range(request, total) {
        Some(Ok((start, end))) => {
            let buf = read_range(&path, start, end - start + 1)
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
            let bytes = std::fs::read(&path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let n = bytes.len();
            hardened(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, n.to_string())
                .body(bytes)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// The file `rest` names inside `root`, or nothing at all — the fence itself.
///
/// Nothing is resolved before it is judged and nothing is judged before it is resolved: the segments are
/// checked one at a time as text, and what they add up to is then checked again against the real filesystem.
/// A path that passes the first check can still leave the folder through a symbolic link, and a path that
/// would pass the second could still have been written as `..` — neither check subsumes the other.
fn under(root: &Path, rest: &str) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;

    let mut path = root.clone();
    let mut named = false;
    for segment in rest.split('/').filter(|s| !s.is_empty()) {
        let decoded = percent_decode(segment);
        // One ordinary name and nothing else. `..`, `.`, an embedded separator, a root and a drive letter all
        // come back as some other kind of component, or as more than one — none of which is a file name.
        let mut parts = Path::new(&decoded).components();
        match (parts.next(), parts.next()) {
            (Some(Component::Normal(name)), None) => path.push(name),
            _ => return None,
        }
        named = true;
    }
    // The folder itself is not a file, and an address with nothing after the session is not an address.
    if !named {
        return None;
    }

    // Now the filesystem's own answer, links followed. A link inside the folder that points out of it is only
    // caught here, which is why the text check above is not the end of it.
    let path = path.canonicalize().ok()?;
    if !path.starts_with(&root) || !path.is_file() {
        return None;
    }
    Some(path)
}

/// Read `len` bytes from `start`. Short files and ranges that run past the end come back shorter, which is
/// what `Content-Length` is then taken from.
fn read_range(path: &Path, start: u64, len: u64) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    file.take(len).read_to_end(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder with a file in it, and a sibling folder holding a secret that must stay out of reach.
    fn folders() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path().join("work");
        std::fs::create_dir_all(root.join("notes")).expect("the folder");
        std::fs::write(root.join("notes/a.md"), b"hello").expect("a file");
        std::fs::write(dir.path().join("secret.txt"), b"no").expect("the secret");
        (dir, root)
    }

    #[test]
    fn a_file_in_the_folder_is_reached() {
        let (dir, root) = folders();
        let found = under(&root, "notes/a.md").expect("the file is inside the folder");
        assert_eq!(std::fs::read(&found).unwrap(), b"hello");
        assert!(found.starts_with(root.canonicalize().unwrap()));
        drop(dir);
    }

    /// The point of the whole module. Every spelling of "somewhere else" is refused, including the ones that
    /// would land back inside after a detour — a rule that only rejected a leading `..` would let this through.
    #[test]
    fn nothing_outside_the_folder_can_be_named() {
        let (dir, root) = folders();
        for rest in [
            "../secret.txt",
            "notes/../../secret.txt",
            "notes/./../../secret.txt",
            "%2E%2E/secret.txt",
            "%2e%2e%2Fsecret.txt",
            "/etc/passwd",
            "notes/a.md/../../../secret.txt",
        ] {
            assert!(under(&root, rest).is_none(), "reached out with {rest:?}");
        }
        drop(dir);
    }

    /// A separator that arrives percent-encoded is not a way to smuggle a second segment past the check: a
    /// name is one name, and what decodes into a path is not one.
    #[test]
    fn an_encoded_separator_is_not_a_name() {
        let (dir, root) = folders();
        assert!(under(&root, "notes%2Fa.md").is_none());
        drop(dir);
    }

    /// A link is followed and *then* judged, which is the only order that catches one pointing out of the
    /// folder. Judging the text alone would pass it, since nothing in the spelling says where it goes.
    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_folder_is_refused() {
        let (dir, root) = folders();
        std::os::unix::fs::symlink(dir.path().join("secret.txt"), root.join("escape"))
            .expect("the link");
        assert!(under(&root, "escape").is_none());
        drop(dir);
    }

    /// The folder itself, and an address with nothing under it, are not files. A directory is not listed
    /// here — what a folder holds is a question for the store, not for a door that hands out bytes.
    #[test]
    fn a_folder_is_not_a_file() {
        let (dir, root) = folders();
        assert!(under(&root, "").is_none());
        assert!(under(&root, "notes").is_none());
        drop(dir);
    }

    /// A folder that is not there answers nothing, rather than resolving against whatever the path would
    /// mean if it were.
    #[test]
    fn a_folder_that_is_gone_reaches_nothing() {
        let dir = tempfile::tempdir().expect("a temp dir");
        assert!(under(&dir.path().join("never-made"), "a.md").is_none());
    }

    #[test]
    fn a_range_is_read_from_where_it_starts() {
        let (dir, root) = folders();
        let file = root.join("notes/a.md");
        assert_eq!(read_range(&file, 1, 3).unwrap(), b"ell");
        // Past the end comes back short rather than failing, which is what the length is then taken from.
        assert_eq!(read_range(&file, 3, 99).unwrap(), b"lo");
        drop(dir);
    }
}
