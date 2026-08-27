//! The custom protocol that hands the webview a file **by its path**, and the fence that keeps that from
//! meaning any path.
//!
//! [`crate::blobproto`] can only be asked for a hash, so it has no way to name a place: the worst a caller
//! can do there is name bytes that are already in the store. This door is the opposite shape — it takes a
//! path — and that makes it the one place in the app where the webview could reach into the filesystem at
//! large. It is therefore not a general file server, and the address is not a path on its own:
//!
//! ```text
//! amenbofile://localhost/<project>/<root>/<segment>/<segment>…?mime=<type>
//! ```
//!
//! **A project, one of the folders it is bound to, and then a path under that folder.** The fence is the
//! project's, the same one the door that lists names is built on ([`crate::folder`]) — not the folder of
//! the pane the picture happens to be shown beside. A project bound to two folders showed the pictures of
//! one of them and refused the other's, because a pane is opened in one folder and the tree beside it
//! draws them all (`AMB-D-782`).
//!
//! **What the fence is made of**, in the order it is applied:
//!
//! 1. The root has to be a folder the store says this project is bound to. A webview naming one is making
//!    a claim, and the registry is what answers it.
//! 2. Every segment must be a single ordinary name. `..`, `.`, an embedded separator, a drive letter and an
//!    absolute path are each rejected as a segment rather than resolved and then judged.
//! 3. The folders the segments name are resolved on the real filesystem, symbolic links and all, and what
//!    they add up to must still be inside a folder of this project's. A folder linking out of it is
//!    followed and then refused, which is the only order that catches one.
//! 4. It must be a regular file, and the last name is not followed either: a link there is opened as the
//!    link it is and refused ([`crate::folder::open_no_follow`]). A directory is not listed here — what a
//!    folder holds is a question for the store, not for a door that hands out bytes.
//!
//! What is served then goes through [`crate::webproto`] like every other answer: the type comes from the
//! allowlist, active types are demoted to source, and nothing may be sniffed back.

use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};

use tauri::http::{header, Request, Response, StatusCode};

use crate::webproto::{empty, hardened, parse_range, percent_decode, query_param, served_content_type};

/// Build the one response. Failures come back as a bare status (404/400/416/500) with an empty body: which
/// of the fence's rules turned a request away is not something a caller is told, because the difference
/// between "outside the folder" and "not there" is itself an answer about the filesystem.
pub fn serve(request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    match try_serve(request) {
        Ok(resp) => resp,
        Err(status) => empty(status),
    }
}

fn try_serve(request: &Request<Vec<u8>>) -> Result<Response<Vec<u8>>, StatusCode> {
    let uri = request.uri();
    // The path is `/<project>/<root>/<segments…>` (the host differs by platform; the path does not).
    let (project, rest) = uri
        .path()
        .trim_start_matches('/')
        .split_once('/')
        .ok_or(StatusCode::BAD_REQUEST)?;
    let (root, rest) = rest.split_once('/').ok_or(StatusCode::BAD_REQUEST)?;
    let project_id: i64 = project.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    let (roots, base) = crate::folder::rooted(project_id, &percent_decode(root))
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let path = under(&roots, base, rest).ok_or(StatusCode::NOT_FOUND)?;

    // The mime the viewer put on the URL is a request, not a fact — nobody has read these bytes. Active types
    // are demoted here: what gets served is our decision, not the caller's.
    let mime = served_content_type(query_param(uri.query(), "mime").as_deref().unwrap_or(""));

    let total = std::fs::symlink_metadata(&path)
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
            let bytes = read_whole(&path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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

/// The file `rest` names inside `roots[base]`, or nothing at all.
///
/// The fence itself is shared with the door that lists what a folder holds ([`crate::folder::under`]):
/// the segments are checked one at a time as text, the folders they name are then checked again against
/// the real filesystem, and the answer belongs to the deepest bound folder holding it. What is added
/// here is what this door is for — the segments arrive percent-encoded, an address with nothing after
/// the root names no file, and a folder is not a file, whatever else it may be to a caller that lists
/// names.
///
/// **A link is not a file here either.** The name is asked about itself rather than about what it leads
/// to, which is the same rule the open below follows and the reason it can be asked twice without
/// disagreeing.
fn under(roots: &[PathBuf], base: usize, rest: &str) -> Option<PathBuf> {
    let segments: Vec<String> = rest
        .split('/')
        .filter(|s| !s.is_empty())
        .map(percent_decode)
        .collect();
    // The folder itself is not a file, and an address with nothing after the root is not an address.
    if segments.is_empty() {
        return None;
    }
    let (_owner, path) = crate::folder::under(roots, base, &segments)?;
    std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_file()).then_some(path)
}

/// The whole of a file, opened the way a range of it is.
fn read_whole(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    crate::folder::open_no_follow(path)?.read_to_end(&mut buf)?;
    Ok(buf)
}

/// Read `len` bytes from `start`. Short files and ranges that run past the end come back shorter, which is
/// what `Content-Length` is then taken from.
fn read_range(path: &Path, start: u64, len: u64) -> std::io::Result<Vec<u8>> {
    let mut file = crate::folder::open_no_follow(path)?;
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    file.take(len).read_to_end(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder with a file in it, and a sibling folder holding a secret that must stay out of reach.
    fn folders() -> (tempfile::TempDir, Vec<PathBuf>) {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path().join("work");
        std::fs::create_dir_all(root.join("notes")).expect("the folder");
        std::fs::write(root.join("notes/a.md"), b"hello").expect("a file");
        std::fs::write(dir.path().join("secret.txt"), b"no").expect("the secret");
        (dir, vec![amenbo_core::binding::canonical_dir(&root).expect("the folder is there")])
    }

    #[test]
    fn a_file_in_the_folder_is_reached() {
        let (dir, roots) = folders();
        let found = under(&roots, 0, "notes/a.md").expect("the file is inside the folder");
        assert_eq!(std::fs::read(&found).unwrap(), b"hello");
        assert!(found.starts_with(&roots[0]));
        drop(dir);
    }

    /// The point of the whole module. Every spelling of "somewhere else" is refused, including the ones that
    /// would land back inside after a detour — a rule that only rejected a leading `..` would let this through.
    #[test]
    fn nothing_outside_the_folder_can_be_named() {
        let (dir, roots) = folders();
        for rest in [
            "../secret.txt",
            "notes/../../secret.txt",
            "notes/./../../secret.txt",
            "%2E%2E/secret.txt",
            "%2e%2e%2Fsecret.txt",
            "/etc/passwd",
            "notes/a.md/../../../secret.txt",
        ] {
            assert!(under(&roots, 0, rest).is_none(), "reached out with {rest:?}");
        }
        drop(dir);
    }

    /// A separator that arrives percent-encoded is not a way to smuggle a second segment past the check: a
    /// name is one name, and what decodes into a path is not one.
    #[test]
    fn an_encoded_separator_is_not_a_name() {
        let (dir, roots) = folders();
        assert!(under(&roots, 0, "notes%2Fa.md").is_none());
        drop(dir);
    }

    /// A link is not a file here, whatever it points at. Asking the name about itself is what makes this
    /// answer agree with the open that follows it, which refuses a link in the same call (`AMB-D-782`).
    #[cfg(unix)]
    #[test]
    fn a_link_is_not_a_file_here() {
        let (dir, roots) = folders();
        std::os::unix::fs::symlink(dir.path().join("secret.txt"), roots[0].join("escape"))
            .expect("the link");
        assert!(under(&roots, 0, "escape").is_none());
        // Even one that stays inside the folder: the door hands out the bytes of a file, and a link is
        // not one.
        std::os::unix::fs::symlink(roots[0].join("notes/a.md"), roots[0].join("inside"))
            .expect("the link");
        assert!(under(&roots, 0, "inside").is_none());
        drop(dir);
    }

    /// A picture in a folder the pane beside it is not open in is still this project's. The fence is the
    /// project's folders, and a project bound to two of them showed one and refused the other while it was
    /// a pane's (`AMB-D-782`).
    #[test]
    fn every_folder_of_the_project_is_reachable() {
        let (dir, mut roots) = folders();
        let other = dir.path().join("designs");
        std::fs::create_dir_all(&other).expect("the folder");
        std::fs::write(other.join("logo.png"), b"\x89PNG").expect("a file");
        roots.push(amenbo_core::binding::canonical_dir(&other).expect("the folder is there"));

        let found = under(&roots, 1, "logo.png").expect("the second folder answers too");
        assert_eq!(std::fs::read(&found).unwrap(), b"\x89PNG");
        drop(dir);
    }

    /// The folder itself, and an address with nothing under it, are not files. A directory is not listed
    /// here — what a folder holds is a question for the store, not for a door that hands out bytes.
    #[test]
    fn a_folder_is_not_a_file() {
        let (dir, roots) = folders();
        assert!(under(&roots, 0, "").is_none());
        assert!(under(&roots, 0, "notes").is_none());
        drop(dir);
    }

    /// A folder that is not there answers nothing, rather than resolving against whatever the path would
    /// mean if it were.
    #[test]
    fn a_folder_that_is_gone_reaches_nothing() {
        let dir = tempfile::tempdir().expect("a temp dir");
        assert!(under(&[dir.path().join("never-made")], 0, "a.md").is_none());
    }

    #[test]
    fn a_range_is_read_from_where_it_starts() {
        let (dir, roots) = folders();
        let file = roots[0].join("notes/a.md");
        assert_eq!(read_range(&file, 1, 3).unwrap(), b"ell");
        // Past the end comes back short rather than failing, which is what the length is then taken from.
        assert_eq!(read_range(&file, 3, 99).unwrap(), b"lo");
        drop(dir);
    }
}
