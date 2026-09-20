//! The custom protocol that hands the webview a picture the skin on this device carries.
//!
//! A skin arrives as one zip and is kept as one zip (`AMB-D-936`), so a background is not a file on
//! the disk that an `<img>` could be pointed at. This door is what turns the filename the author
//! wrote into bytes the window can draw:
//!
//! ```text
//! amenboskin://localhost/<skin>/<file>?v=<stamp>
//! ```
//!
//! **The author never writes this address.** They write a filename beside `skin.yaml`, and
//! `app/src/core/skin.ts` builds the url around it — which is the whole reason `url()` is kept out
//! of a skin's values: a document that could spell an address could spell one that is not on this
//! machine.
//!
//! **What may be asked for is what the document lays.** The skin is read and checked the way it is
//! read to be worn, and a file the check did not keep as a background is not served — so the door
//! answers for four names at most, all of them the author's own. The bytes themselves come out of
//! the zip ([`amenbo_core::skin::Materials::read`]), which is also where the per-file ceiling is
//! applied.
//!
//! **The type is read off the bytes rather than asked for.** [`crate::blobproto`] and
//! [`crate::fileproto`] are handed a mime by the caller and demote it
//! ([`crate::webproto::served_content_type`]) because nobody has looked at what they serve. Here
//! amenbo has looked: [`amenbo_core::skin::Picture::of`] says what the opening bytes are, and
//! [`amenbo_core::skin::Picture::mime`] is that answer. It is the reason an svg may go out as an
//! svg here and may not there — it is drawn as a background image and never opened as a document,
//! and `hardened` still puts a sandbox csp and `nosniff` on the way out.
//!
//! **`?v=` is not read.** It carries what the skin's file was when the window was handed the
//! tables, so that a skin taken in again under the same name is drawn from its new picture rather
//! than the one the webview already has. The address is what changes; the door answers the same
//! either way.

use tauri::http::{header, Request, Response, StatusCode};

use crate::webproto::{empty, hardened, percent_decode};

/// How long the webview may keep a picture. The address carries the stamp of the file it came out
/// of, so a picture that changed is asked for under another name — which is what makes a year safe
/// to say. Without it the same four urls are fetched again for every element drawn in that place,
/// and there are tens of them per surface.
const KEEP_FOR: &str = "public, max-age=31536000, immutable";

/// Build the one response. Failures come back as a bare status with an empty body: which of "no
/// such skin", "the document lays no such picture" and "the zip will not open" it was is not
/// something a caller is told.
pub fn serve(request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    match try_serve(request) {
        Ok(resp) => resp,
        Err(status) => empty(status),
    }
}

fn try_serve(request: &Request<Vec<u8>>) -> Result<Response<Vec<u8>>, StatusCode> {
    // The path is `/<skin>/<file>` (the host differs by platform; the path does not).
    let (name, file) = addressed(request.uri().path()).ok_or(StatusCode::BAD_REQUEST)?;

    let paths = amenbo_core::config::Paths::resolve().map_err(|_| StatusCode::NOT_FOUND)?;
    let (document, materials) = amenbo_core::skin::Skin::installed(&paths, &name)
        .map_err(|_| StatusCode::NOT_FOUND)?
        .ok_or(StatusCode::NOT_FOUND)?;
    // The check is what rules on a background — the place, the words and the name — so asking it is
    // how the door stays shut to every file the document does not lay.
    let taken = document.check(&materials).map_err(|_| StatusCode::NOT_FOUND)?;
    if !taken.skin.backgrounds.values().any(|laid| laid.file == file) {
        return Err(StatusCode::NOT_FOUND);
    }

    let bytes = materials.read(&file).ok_or(StatusCode::NOT_FOUND)?;
    let picture =
        amenbo_core::skin::Picture::of(&bytes).ok_or(StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
    let len = bytes.len();
    hardened(StatusCode::OK)
        .header(header::CONTENT_TYPE, picture.mime())
        .header(header::CACHE_CONTROL, KEEP_FOR)
        .header(header::CONTENT_LENGTH, len.to_string())
        .body(bytes)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// The skin and the file one address names, decoded — `None` where it names anything else.
///
/// Two segments and no more. A name with a separator in it is not a name a skin or a file inside
/// one may have, so an address carrying a third segment is turned away here rather than trimmed
/// into one that reads.
fn addressed(path: &str) -> Option<(String, String)> {
    let (name, file) = path.trim_start_matches('/').split_once('/')?;
    if name.is_empty() || file.is_empty() || file.contains('/') {
        return None;
    }
    Some((percent_decode(name), percent_decode(file)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_segments_are_the_whole_address() {
        assert_eq!(addressed("/paper/wall.png"), Some(("paper".into(), "wall.png".into())));
        assert_eq!(addressed("/paper/a%20b.png"), Some(("paper".into(), "a b.png".into())));
    }

    /// Anything that is not a skin and a file is refused rather than read as far as it goes.
    #[test]
    fn anything_else_is_not_an_address() {
        for path in ["/", "/paper", "/paper/", "//wall.png", "/paper/under/wall.png"] {
            assert_eq!(addressed(path), None, "{path}");
        }
    }
}
