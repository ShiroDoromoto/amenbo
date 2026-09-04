//! What one file has to show — read off its bytes, and never off its name.
//!
//! A name says nothing reliable: the extension table this replaces could not answer for 19% of this
//! repository's files (`AMB-T-3547`). A NUL byte in the head is what separates text from everything
//! else, and a picture is recognised by the bytes it starts with — so a `.md` that is really a PNG
//! draws as one, and a text file with no extension at all still reads.
//!
//! **Every read stops somewhere.** The face reads and does not page, so text comes back cut at
//! `TEXT_CAP` and said to be cut, and a picture is judged on its bytes and on its pixels both
//! before a webview is asked to draw it (`carriable`).

use std::path::Path;

use crate::dto::{FolderFileDto, FolderImageDto, FolderLineEndingDto, FolderOversizeDto};
use crate::error::CmdError;
use crate::folder_fence::{gone, open_no_follow, rooted, under};

/// How much of a file is read to decide whether it is text (`AMB-T-3547`).
const HEAD: usize = 8000;

/// The most text a panel is handed. A file longer than this is drawn as far as this goes and said
/// to be cut — the face reads, it does not page.
///
/// The old quarter of a megabyte was a number for looking, not for working: four files in this
/// repository alone are over it, and a cut one written back drops its tail without saying so
/// (`AMB-D-783`). What is cut is what `truncated` is for — a face that lets a file be edited reads
/// it and refuses to save.
const TEXT_CAP: usize = 5 * 1024 * 1024;

/// The largest picture the panel draws, in bytes. Past it the reader is told there is a picture and
/// not made to wait for it.
///
/// This is the cap on what the **host** holds. The bytes no longer cross the command seam — the
/// webview fetches them from [`crate::fileproto`] — but that door still reads the file whole into
/// this process to answer a request with no range on it, so the number guards the same thing it
/// always did.
const IMAGE_CAP: u64 = 5 * 1024 * 1024;

/// The largest picture a webview is asked to draw, in pixels — the second cap, and not a
/// restatement of the first (`AMB-D-783`).
///
/// **The two guard different things and neither subsumes the other.** Bytes stand for what this
/// process holds; pixels stand for what the webview decodes, and the relation between them is the
/// compression ratio, which an author chooses. A 4.83 MB PNG of sixteen hundred megapixels passes
/// the byte cap and freezes the window for twenty-two seconds; a 14 MB JPEG of nine hundred
/// megapixels passes this one and is decoded almost for free (`AMB-T-3769` measured both).
///
/// A hundred megapixels is roughly ten thousand square. Of the 27,659 pictures on the machine this
/// was measured against, the largest was 64 megapixels — so nothing anybody actually has is refused
/// by it, and the worst case it still admits costs about 430 MB and under a second.
const PIXEL_CAP: u64 = 100_000_000;

/// How much of a JPEG is read before it is asked how large it is.
///
/// Every other form answers within thirty bytes, so [`HEAD`] is all they need. JPEG writes its
/// frame header behind whatever came first, and what commonly comes first is an EXIF thumbnail and
/// a colour profile: of the 12,545 JPEGs measured in `AMB-T-3769`, 8 KB answered for 78.9% and
/// 64 KB for 99.3%. It is one extra read of a file already known to be under the byte cap.
const JPEG_HEAD: usize = 64 * 1024;

/// The other refusal a name can earn: it is there, and it is a link.
///
/// [`gone`] answers for every rule that turns a caller away, and answering a link with it too made
/// the panel say the file could not be read — which is not what happened. The file is whole, the
/// link is deliberate, and the refusal is `AMB-D-782`'s: a person who linked their `CLAUDE.md` into
/// several projects meets this first, and "could not be read" sends them looking for damage that is
/// not there.
///
/// Only the read answers this way. The refusal is the same wherever a link is met, but this is the
/// one door with a reader in front of it to tell.
fn link() -> CmdError {
    CmdError::coded(
        "folder_link",
        "this name is a link, and a link is not followed here",
        serde_json::Value::Null,
    )
}

/// The file one name answers for, or why it does not — the two refusals told apart.
///
/// Read off the name itself and not off what it leads to: a link is not a file to read here
/// (`AMB-D-782`). What is neither a link nor a file — a folder named where a file was asked for —
/// is nothing this door hands out, and says so the way a name that is not there does.
fn readable(file: &Path) -> Result<std::fs::Metadata, CmdError> {
    let meta = std::fs::symlink_metadata(file).map_err(|_| gone())?;
    if meta.is_symlink() {
        return Err(link());
    }
    if !meta.is_file() {
        return Err(gone());
    }
    Ok(meta)
}

/// The encodings a file may be reopened in, in the order to offer them.
///
/// It is asked for rather than written on the panel's own side because the list is one fact with
/// one owner: which encodings this can write back ([`crate::encoding::writable_names`]). A copy
/// kept over there would go on offering an encoding the day it stopped being written.
#[tauri::command]
pub fn folder_encodings() -> Vec<String> {
    crate::encoding::writable_names().into_iter().map(str::to_owned).collect()
}

/// What one file has to show: its text, or that it is a picture and of what type, or why the
/// picture is not drawn, or none of those.
///
/// The head is read once and answers both questions — whether there is a NUL in it, and what the
/// first bytes say the file is — so a name is never consulted about either. Only what that head
/// settles on is then read further: the text up to its cap, or a JPEG's front far enough to reach
/// the frame header. A file that is neither is never read past the head at all.
///
/// **A picture is never read whole here.** Its bytes reach the webview through
/// [`crate::fileproto`], which the caller can address because it named this file by the same
/// project, folder and path (`AMB-D-783`).
///
/// `encoding` is the reader putting the guess right. Left out, the bytes are guessed at as usual;
/// named, that encoding is what they are decoded as and nothing is guessed (`AMB-D-773`). A name
/// this cannot write back is refused rather than honoured — offering to open a file in an encoding
/// that could never be saved would be handing back a file to look at and not to keep — and a name
/// on a file that is not text is simply not reached, the encoding question never being asked of a
/// picture.
#[tauri::command]
pub fn folder_read(
    project_id: i64,
    root: String,
    path: Vec<String>,
    encoding: Option<String>,
) -> Result<FolderFileDto, CmdError> {
    let asked = match encoding.as_deref() {
        None => None,
        Some(name) => Some(crate::encoding::writable(name).ok_or_else(|| {
            CmdError::coded(
                "folder.encoding",
                format!("not an encoding this writes back: {name}"),
                serde_json::Value::Null,
            )
        })?),
    };
    let (roots, base) = rooted(project_id, &root)?;
    let (_owner, file) = under(&roots, base, &path).ok_or_else(gone)?;
    let meta = readable(&file)?;
    let size = meta.len();
    let head = read_head(&file, HEAD).map_err(|_| gone())?;

    // The one judgement, made on bytes: text is what has no NUL in its head. Which encoding that
    // text is in is a separate question and never this one's — a page of Shift_JIS is text to the
    // person who wrote it — and it is `crate::encoding`'s to answer.
    if !head.contains(&0) {
        let bytes = if size > HEAD as u64 {
            read_head(&file, TEXT_CAP).map_err(|_| gone())?
        } else {
            head
        };
        let truncated = (bytes.len() as u64) < size;
        // The reader's own language is the guess's only hint, and it is fetched here rather than
        // held because only a file that is not UTF-8 is ever guessed at — one in 645 of them.
        let read = match asked {
            Some(encoding) => crate::encoding::read_as(&bytes, truncated, encoding),
            None => crate::encoding::read(&bytes, truncated, language_tld()),
        };
        return Ok(FolderFileDto {
            truncated,
            text: Some(read.text),
            image: None,
            oversize: None,
            encoding: Some(read.encoding.name().to_string()),
            bom: read.bom,
            line_ending: line_ending(read.line_ending),
            clean: read.clean,
            // Over the bytes, not over the text: what the panel holds has been through a decoder
            // and a truncation, and what a save is weighed against is the file (`AMB-D-784`).
            digest: Some(digest(&bytes)),
        });
    }

    let Some(mime) = picture(&head) else {
        return Ok(FolderFileDto {
            text: None,
            truncated: false,
            image: None,
            oversize: None,
            encoding: None,
            bom: false,
            line_ending: FolderLineEndingDto::Lf,
            clean: false,
            digest: None,
        });
    };

    // A JPEG is the one form whose size is not already in hand (`JPEG_HEAD`), and reading further
    // is worth nothing where the read cannot succeed: a file over the byte cap is refused whatever
    // its size turns out to be.
    let front = match mime {
        "image/jpeg" if size <= IMAGE_CAP && size > HEAD as u64 => {
            read_head(&file, JPEG_HEAD).unwrap_or(head)
        }
        _ => head,
    };
    let pixels = measure(mime, &front);

    if carriable(size, pixels) {
        // The bytes are not read here: what the panel is handed is the type, and it asks
        // fileproto for the picture itself at the path it named this call with. They are passed
        // through the hasher on the way past, though — a mark is what the panel watches a file it
        // has open by, and a picture with none of it is one that stays on the screen after the
        // agent beside it has redrawn the thing (`AMB-D-797`).
        return Ok(FolderFileDto {
            text: None,
            truncated: false,
            image: Some(FolderImageDto { mime: mime.to_string() }),
            oversize: None,
            encoding: None,
            bom: false,
            line_ending: FolderLineEndingDto::Lf,
            clean: false,
            digest: Some(digest_whole(&file).map_err(|_| gone())?),
        });
    }
    Ok(FolderFileDto {
        text: None,
        truncated: false,
        image: None,
        encoding: None,
        bom: false,
        line_ending: FolderLineEndingDto::Lf,
        clean: false,
        digest: None,
        oversize: Some(FolderOversizeDto {
            bytes: size,
            width: pixels.map(|(width, _)| width),
            height: pixels.map(|(_, height)| height),
        }),
    })
}

/// The type the bytes say they are, for the pictures a webview can draw. Sniffed rather than looked
/// up by name, which is the same rule the text judgement above follows.
fn picture(head: &[u8]) -> Option<&'static str> {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    const GIF: &[u8] = b"GIF8";
    if head.starts_with(PNG) {
        return Some("image/png");
    }
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if head.starts_with(GIF) {
        return Some("image/gif");
    }
    // RIFF containers name their form in the four bytes after the length: WEBP is one of several.
    if head.starts_with(b"RIFF") && head.len() >= 12 && &head[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}

/// Whether a picture this large is one the panel draws — both caps, asked as one question
/// (`AMB-D-783`).
///
/// **A size nobody could read is not a refusal.** What cannot be measured is nearly always a JPEG
/// behind a thick profile, and a JPEG under the byte cap is cheap however many pixels it holds; the
/// forms that are not cheap — PNG, GIF, WebP — all answer within thirty bytes. So "unmeasured"
/// never stands in for "dangerous", and the byte cap is left to guard those alone.
fn carriable(bytes: u64, pixels: Option<(u32, u32)>) -> bool {
    bytes <= IMAGE_CAP
        && match pixels {
            Some((width, height)) => u64::from(width) * u64::from(height) <= PIXEL_CAP,
            None => true,
        }
}

/// How large a picture says it is, in pixels — or nothing at all, where the bytes in hand do not
/// say (`AMB-D-783`).
///
/// **This rides on a read that has already happened.** Every one of these forms writes its size
/// near the front, so the head the type was sniffed from is usually the same bytes the size is read
/// out of; the whole of it costs single-digit nanoseconds (`AMB-T-3769`). Nothing is decoded and no
/// image library is involved — a decoder is exactly the cost this measurement exists to avoid
/// paying.
///
/// The four forms are the four [`picture`] answers for, and it stays that way on purpose: a form
/// this cannot measure is a form the panel does not draw either.
fn measure(mime: &str, head: &[u8]) -> Option<(u32, u32)> {
    match mime {
        "image/png" => png_pixels(head),
        "image/gif" => gif_pixels(head),
        "image/webp" => webp_pixels(head),
        "image/jpeg" => jpeg_pixels(head),
        _ => None,
    }
}

/// PNG writes it in the IHDR chunk, which the format requires to be the first one — so it is two
/// words at a fixed offset, and the chunk's own name is checked rather than assumed.
fn png_pixels(head: &[u8]) -> Option<(u32, u32)> {
    if head.get(12..16)? != b"IHDR" {
        return None;
    }
    Some((be32(head, 16)?, be32(head, 20)?))
}

/// GIF writes it in the screen descriptor, immediately behind the six-byte signature.
fn gif_pixels(head: &[u8]) -> Option<(u32, u32)> {
    Some((le16(head, 6)?, le16(head, 8)?))
}

/// WebP is a RIFF container whose first chunk says which of three forms this is, and each of the
/// three writes its size somewhere else, in its own way.
fn webp_pixels(head: &[u8]) -> Option<(u32, u32)> {
    match head.get(12..16)? {
        // Lossy: the VP8 key frame header, behind a three-byte frame tag and the three-byte start
        // code — which is checked, because without it the offsets below are being read out of
        // whatever else the file happens to be. Fourteen bits each; the top two are a scale.
        b"VP8 " => {
            if head.get(23..26)? != [0x9D, 0x01, 0x2A] {
                return None;
            }
            Some((le16(head, 26)? & 0x3FFF, le16(head, 28)? & 0x3FFF))
        }
        // Lossless: fourteen bits each, packed into one little-endian word behind a signature byte,
        // and each written one short of the real number.
        b"VP8L" => {
            if *head.get(20)? != 0x2F {
                return None;
            }
            let packed = le32(head, 21)?;
            Some(((packed & 0x3FFF) + 1, ((packed >> 14) & 0x3FFF) + 1))
        }
        // Extended: the canvas rather than a frame, behind four bytes of feature flags — three
        // bytes each, and again one short.
        b"VP8X" => Some((le24(head, 24)? + 1, le24(head, 27)? + 1)),
        _ => None,
    }
}

/// JPEG writes it in a start-of-frame segment, and the only way to that segment is to walk the ones
/// in front of it — which is why this form alone is handed more of the file ([`JPEG_HEAD`]).
///
/// The walk stops at the scan: past that marker the file is entropy-coded data, not segments, and a
/// frame header that has not appeared by then is not going to.
fn jpeg_pixels(head: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2;
    loop {
        // A marker is 0xFF and then the marker byte, and any number of 0xFF may pad the gap.
        if *head.get(at)? != 0xFF {
            return None;
        }
        while *head.get(at)? == 0xFF {
            at += 1;
        }
        let marker = *head.get(at)?;
        at += 1;
        // Restarts and the one-byte extension carry no length at all, so there is nothing to skip.
        if (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
            continue;
        }
        if marker == 0xDA {
            return None;
        }
        let length = be16(head, at)? as usize;
        // A length counts its own two bytes, so anything under that would walk backwards forever.
        if length < 2 {
            return None;
        }
        // Every start-of-frame writes the two sizes in the same place, behind the length and the
        // sample precision. The three markers excepted are in the range but are not frames: a
        // Huffman table, an arithmetic-coding table, and a reserved extension.
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            return Some((be16(head, at + 5)?, be16(head, at + 3)?));
        }
        at += length;
    }
}

/// The four ways these headers spell a number, each answering nothing where the bytes are not
/// there — which is how a size read out of a truncated head comes back unmeasured rather than
/// wrong.
fn be16(bytes: &[u8], at: usize) -> Option<u32> {
    let pair: [u8; 2] = bytes.get(at..at + 2)?.try_into().ok()?;
    Some(u32::from(u16::from_be_bytes(pair)))
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    let word: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
    Some(u32::from_be_bytes(word))
}

fn le16(bytes: &[u8], at: usize) -> Option<u32> {
    let pair: [u8; 2] = bytes.get(at..at + 2)?.try_into().ok()?;
    Some(u32::from(u16::from_le_bytes(pair)))
}

fn le24(bytes: &[u8], at: usize) -> Option<u32> {
    let three = bytes.get(at..at + 3)?;
    Some(u32::from(three[0]) | u32::from(three[1]) << 8 | u32::from(three[2]) << 16)
}

fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    let word: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(word))
}

/// The wire form of what the bytes said about their newlines.
fn line_ending(read: crate::encoding::LineEnding) -> FolderLineEndingDto {
    match read {
        crate::encoding::LineEnding::Lf => FolderLineEndingDto::Lf,
        crate::encoding::LineEnding::Crlf => FolderLineEndingDto::Crlf,
        crate::encoding::LineEnding::Mixed => FolderLineEndingDto::Mixed,
    }
}

/// The hint the encoding guess is given: the top-level domain standing for the language the reader
/// chose to be spoken to in (`crate::encoding::tld_for`).
///
/// `config.json` is a file of its own, read here rather than held, because the only caller is the
/// one file in 645 that is not UTF-8 — holding it would be caching a read that almost never happens
/// against a setting that can change under it.
fn language_tld() -> Option<&'static [u8]> {
    let language = amenbo_core::config::Paths::resolve()
        .ok()
        .and_then(|paths| amenbo_core::config::Config::load(&paths.config_file).language)?;
    crate::encoding::tld_for(Some(&language))
}

/// The mark that says which bytes a reader was handed: blake3 of them, in hex.
///
/// **A mark and not a time or a length**, because neither of those answers the question. A FAT32
/// volume — a USB stick, a card — records modification times to the nearest two seconds, and 38 of
/// 40 writes made 120 ms apart came back with the same one; a length says nothing at all about a
/// file edited in place to the same size (`AMB-T-3739` measured both). The mark costs 106 µs for
/// 256 KB, and is taken once when a file is read and once more when it is saved — never on a walk,
/// where marking every name costs 22 times what looking at every name costs.
///
/// **It stops at [`TEXT_CAP`]**, which is where [`folder_read`] stops reading, so a mark taken over
/// what was read and one taken over what was written are marks of the same stretch of the file. Only
/// a save can hand this more than the cap — a reader who pasted six megabytes into a file that was
/// under it — and hashing all of that would give the file a mark no read of it could ever match.
pub fn digest(bytes: &[u8]) -> String {
    blake3::hash(&bytes[..bytes.len().min(TEXT_CAP)]).to_hex().to_string()
}

/// The same mark for a file already open, over the bytes the panel's own read would have taken.
///
/// **The handle rather than the name**: what is weighed has to be the file this call goes on to
/// act on, and a name is only a name until it is opened ([`open_no_follow`] is where a link at the
/// end is refused).
///
/// It reads to [`TEXT_CAP`] and no further, which is what [`digest`] marks. Past that cut a file is
/// drawn read-only and there is no save to weigh; and a file that grew past the cut since it was
/// read is a different set of bytes at the cut anyway.
pub fn digest_of(file: &std::fs::File) -> std::io::Result<String> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    file.take(TEXT_CAP as u64).read_to_end(&mut bytes)?;
    Ok(digest(&bytes))
}

/// The same mark over a whole file, taken without ever holding it (`AMB-D-797`).
///
/// **This is the picture's mark.** A picture's bytes never reach [`folder_read`] — the webview
/// fetches them from [`crate::fileproto`] — so there is nothing in hand to give to [`digest`], and
/// a picture with no mark is one the panel cannot watch. The file is passed through the hasher a
/// chunk at a time instead, so what this costs is the read: a mark is wanted here and the bytes
/// are not, and a [`digest`] of them would have to hold the whole picture to say the same thing.
///
/// The caller has already refused anything over [`IMAGE_CAP`], and the cap is applied here as well
/// rather than trusted: the size was read off the metadata, and a file that grew between that read
/// and this one would otherwise be hashed however large it had become.
fn digest_whole(path: &Path) -> std::io::Result<String> {
    use std::io::Read as _;
    let mut file = open_no_follow(path)?.take(IMAGE_CAP);
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hasher.finalize().to_hex().to_string());
        }
        hasher.update(&buf[..n]);
    }
}

/// At most `cap` bytes from the front of a file. A short file comes back short; a long one comes
/// back cut, which is what `truncated` is then read from.
fn read_head(path: &Path, cap: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut buf = Vec::new();
    open_no_follow(path)?.take(cap as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use amenbo_core::binding::canonical_dir;
    #[cfg(unix)]
    use std::path::PathBuf;

    /// A folder with something in it, and a sibling holding a secret that must stay out of reach.
    #[cfg(unix)]
    fn folders() -> (tempfile::TempDir, Vec<PathBuf>) {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path().join("work");
        std::fs::create_dir_all(root.join("notes")).expect("the folder");
        std::fs::create_dir_all(root.join("node_modules")).expect("the machine's folder");
        std::fs::write(root.join("notes/a.md"), b"hello").expect("a file");
        std::fs::write(root.join("node_modules/x.js"), b"built").expect("the machine's file");
        std::fs::write(dir.path().join("secret.txt"), b"no").expect("the secret");
        (dir, vec![canonical_dir(&root).expect("the folder is there")])
    }

    /// The mark is what the panel knows an open file by, and it answers where the two cheaper
    /// questions do not: an edit that leaves the file the same length changes it, and a
    /// modification time on a FAT32 volume would not have moved at all (`AMB-D-784`).
    #[test]
    fn a_file_edited_to_the_same_length_has_a_different_mark() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("note.md");
        std::fs::write(&file, b"before").expect("a file");
        let was = digest_of(&open_no_follow(&file).expect("the file")).expect("the mark");

        std::fs::write(&file, b"BEFORE").expect("somebody else writing");
        let now = digest_of(&open_no_follow(&file).expect("the file")).expect("the mark");

        assert_eq!(std::fs::metadata(&file).expect("the file").len(), 6, "the same length");
        assert_ne!(was, now, "the same length, and not the same file");
        // And a file standing still is the same mark read twice, which is what silence rests on.
        assert_eq!(now, digest(b"BEFORE"));
    }

    /// A picture is marked over the whole of it, and that is the point of marking it at all: a
    /// diagram redrawn keeps its header and often its length, so everything the panel already reads
    /// of a picture — the first bytes, the size, the pixel count — says the same thing before and
    /// after (`AMB-D-797`).
    #[test]
    fn a_picture_is_marked_over_the_whole_of_it() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("chart.png");
        let mut was = png(1920, 1080);
        was.extend(vec![0xAA; 200_000]);
        std::fs::write(&file, &was).expect("a picture");
        let before = digest_whole(&file).expect("the mark");

        let mut now = png(1920, 1080);
        now.extend(vec![0xBB; 200_000]);
        std::fs::write(&file, &now).expect("the agent redrawing it");

        assert_eq!(was.len(), now.len(), "the same length, and the same first bytes");
        assert_ne!(digest_whole(&file).expect("the mark"), before);
        // And what is streamed past the hasher is the file itself: the mark is the one the bytes
        // in hand would have given, so a picture and a text file are known by the same kind of mark.
        assert_eq!(digest_whole(&file).expect("the mark"), digest(&now));
    }

    /// The last name is not resolved, so a link there passes the fence — and is refused where the
    /// file is opened, in the same call that opens it. That order is what leaves no window between
    /// asking and acting, and it is what lets a name that is not there yet be named at all.
    #[cfg(unix)]
    #[test]
    fn a_link_at_the_last_name_is_refused_at_the_open() {
        let (dir, roots) = folders();
        std::os::unix::fs::symlink(dir.path().join("secret.txt"), roots[0].join("escape"))
            .expect("the link");
        let (_, path) = under(&roots, 0, ["escape"]).expect("the fence lets the name through");
        let refused = open_no_follow(&path).expect_err("the open refuses it");
        assert_eq!(refused.raw_os_error(), Some(libc::ELOOP));
        // And the file it points at is readable through no door of this module.
        assert!(read_head(&path, TEXT_CAP).is_err());
        // A file that is really a file still opens.
        let (_, real) = under(&roots, 0, ["notes", "a.md"]).expect("a real file");
        assert_eq!(read_head(&real, TEXT_CAP).unwrap(), b"hello");
        drop(dir);
    }

    /// A link and a name that is not there are both refused, and the refusals are not the same one.
    /// The read is the door a person is standing at, and "could not be read" told somebody who
    /// linked a file in on purpose that their file was broken (`AMB-D-782`).
    #[cfg(unix)]
    #[test]
    fn a_link_is_refused_as_a_link_and_not_as_a_file_that_is_not_there() {
        let (dir, roots) = folders();
        std::os::unix::fs::symlink(dir.path().join("secret.txt"), roots[0].join("escape"))
            .expect("the link");

        let (_, link) = under(&roots, 0, ["escape"]).expect("the fence lets the name through");
        assert_eq!(readable(&link).expect_err("a link is refused").code, "folder_link");

        // What it points at is never asked about, so a link leading nowhere is refused as a link
        // and not as the file that is not on the other side of it.
        std::os::unix::fs::symlink(dir.path().join("nowhere.txt"), roots[0].join("dangling"))
            .expect("the link");
        let (_, dangling) = under(&roots, 0, ["dangling"]).expect("the fence lets the name through");
        assert_eq!(readable(&dangling).expect_err("a link is refused").code, "folder_link");

        let (_, missing) = under(&roots, 0, ["notes", "nope.md"]).expect("a name that is not there");
        assert_eq!(readable(&missing).expect_err("nothing is there").code, "not_found");

        let (_, real) = under(&roots, 0, ["notes", "a.md"]).expect("a real file");
        assert!(readable(&real).is_ok());
        drop(dir);
    }

    /// What a file is, is read off its bytes and never off its name — the point of the whole
    /// judgement (`AMB-T-3547`).
    #[test]
    fn text_and_binary_are_told_apart_by_a_nul_and_not_by_a_name() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let text = dir.path().join("no-extension-at-all");
        std::fs::write(&text, "日本語もテキスト").expect("a file");
        let binary = dir.path().join("looks-like.md");
        std::fs::write(&binary, [0x00, 0x01, 0x02]).expect("a file");

        let head = read_head(&text, HEAD).unwrap();
        assert!(!head.contains(&0));
        let head = read_head(&binary, HEAD).unwrap();
        assert!(head.contains(&0));
    }

    /// A NUL past the head is not looked for. What matters is that the judgement reads a bounded
    /// piece of the file, so a huge one costs the same as a small one — and the bound is the head,
    /// not the text cap, so raising the cap did not raise what a binary costs to recognise.
    #[test]
    fn only_the_head_is_judged() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("long.txt");
        let mut bytes = vec![b'a'; HEAD + 10];
        bytes[HEAD + 5] = 0;
        std::fs::write(&file, &bytes).expect("a file");
        let head = read_head(&file, HEAD).unwrap();
        assert_eq!(head.len(), HEAD);
        assert!(!head.contains(&0));
    }

    /// A picture is what its first bytes say it is. The name is not asked, so a PNG called `.md`
    /// draws and a text file called `.png` does not.
    #[test]
    fn a_picture_is_recognised_by_its_bytes() {
        assert_eq!(picture(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(picture(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(picture(b"GIF89a"), Some("image/gif"));
        assert_eq!(picture(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(picture(b"RIFF\0\0\0\0WAVEfmt "), None);
        assert_eq!(picture(b"# a heading"), None);
    }

    /// A picture with a bare header of the given form, long enough to be measured and nothing more.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes
    }

    fn webp(form: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(form);
        bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
        bytes.extend_from_slice(body);
        bytes
    }

    /// A JPEG whose frame header sits behind `ahead` bytes of something else — which is what an
    /// EXIF thumbnail and a colour profile are, and why this form is handed more of the file.
    fn jpeg(width: u16, height: u16, ahead: usize) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8];
        bytes.extend_from_slice(&[0xFF, 0xE1]);
        bytes.extend_from_slice(&((ahead + 2) as u16).to_be_bytes());
        bytes.extend(std::iter::repeat(0xAB).take(ahead));
        bytes.extend_from_slice(&[0xFF, 0xC0]);
        bytes.extend_from_slice(&17u16.to_be_bytes());
        bytes.push(8);
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes
    }

    /// How large a picture is, read off its front and never by decoding it — the measurement the
    /// pixel cap is applied to (`AMB-D-783`). All four forms the panel draws answer.
    #[test]
    fn a_picture_says_how_large_it_is_in_its_first_bytes() {
        assert_eq!(measure("image/png", &png(1920, 1080)), Some((1920, 1080)));
        assert_eq!(measure("image/gif", b"GIF89a\x40\x01\xf0\x00rest"), Some((320, 240)));
        assert_eq!(measure("image/jpeg", &jpeg(800, 600, 4)), Some((800, 600)));

        // Lossy: fourteen bits each behind the frame tag and the start code.
        let mut lossy = vec![0x00, 0x00, 0x00, 0x9D, 0x01, 0x2A];
        lossy.extend_from_slice(&300u16.to_le_bytes());
        lossy.extend_from_slice(&200u16.to_le_bytes());
        assert_eq!(measure("image/webp", &webp(b"VP8 ", &lossy)), Some((300, 200)));

        // Lossless: the same two numbers packed into one word, each written one short.
        let packed: u32 = (300 - 1) | ((200 - 1) << 14);
        let mut lossless = vec![0x2F];
        lossless.extend_from_slice(&packed.to_le_bytes());
        assert_eq!(measure("image/webp", &webp(b"VP8L", &lossless)), Some((300, 200)));

        // Extended: the canvas behind the feature flags, three bytes each and again one short.
        let mut extended = vec![0x00; 4];
        extended.extend_from_slice(&(300u32 - 1).to_le_bytes()[..3]);
        extended.extend_from_slice(&(200u32 - 1).to_le_bytes()[..3]);
        assert_eq!(measure("image/webp", &webp(b"VP8X", &extended)), Some((300, 200)));
    }

    /// The one form whose size is not already in hand: the walk has to step over whatever was
    /// written in front of the frame, and 8 KB of head is not enough to reach past a thumbnail
    /// (`AMB-T-3769` — 78.9% at 8 KB, 99.3% at 64 KB).
    #[test]
    fn a_jpeg_is_measured_from_behind_what_was_written_in_front_of_it() {
        let fat = jpeg(4000, 3000, 25_000);
        assert!(fat.len() > HEAD && fat.len() <= JPEG_HEAD);
        assert_eq!(measure("image/jpeg", &fat), Some((4000, 3000)));
        // The same file cut at the head the type was sniffed from says nothing — not a wrong number.
        assert_eq!(measure("image/jpeg", &fat[..HEAD]), None);
    }

    /// Bytes that do not say are answered with nothing at all, whatever they are — a truncated
    /// header, a chunk that is not IHDR, a WebP form nobody has seen. Nothing here guesses.
    #[test]
    fn a_front_that_does_not_say_is_not_guessed_at() {
        assert_eq!(measure("image/png", &png(10, 10)[..20]), None);
        assert_eq!(measure("image/png", b"\x89PNG\r\n\x1a\n\0\0\0\rIDATxxxxxxxx"), None);
        assert_eq!(measure("image/webp", &webp(b"VP9 ", &[0; 20])), None);
        // A lossy chunk whose start code is not there is not read at the offsets that follow it.
        assert_eq!(measure("image/webp", &webp(b"VP8 ", &[0; 20])), None);
        // The scan is where the segments stop; a frame that has not appeared by then never will.
        assert_eq!(measure("image/jpeg", &[0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x0C]), None);
        assert_eq!(measure("image/avif", &png(10, 10)), None);
    }

    /// Two caps, and each catches what the other passes (`AMB-D-783`). The bytes stand for what this
    /// process holds, the pixels for what the webview decodes, and their ratio is the author's to
    /// choose — so neither alone is a fence.
    #[test]
    fn both_caps_are_needed_because_each_passes_what_the_other_stops() {
        // A 4.83 MB PNG of sixteen hundred megapixels: under the byte cap, twenty-two seconds of
        // frozen window. Only the pixel cap stops it.
        assert!(!carriable(4_830_000, Some((40_000, 40_000))));
        // A 14 MB JPEG of nine hundred megapixels: decoded almost for free, but held whole in this
        // process. Only the byte cap stops it.
        assert!(!carriable(14_060_000, Some((30_000, 30_000))));
        // What people actually have goes through: the largest of 27,659 pictures measured was 64
        // megapixels at under 2 MB.
        assert!(carriable(1_980_000, Some((9_824, 6_552))));
    }

    /// A size nobody could read is let through on the bytes alone. The forms that are expensive to
    /// decode all answer within thirty bytes, so "unmeasured" never stands in for "dangerous".
    #[test]
    fn a_picture_that_would_not_say_its_size_is_still_judged_on_its_bytes() {
        assert!(carriable(IMAGE_CAP, None));
        assert!(!carriable(IMAGE_CAP + 1, None));
    }

    /// The cap is what a panel is handed, not what the file is: the size travels whole, and the cut
    /// is said out loud.
    #[test]
    fn a_long_file_comes_back_cut() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let file = dir.path().join("long.txt");
        std::fs::write(&file, "x".repeat(TEXT_CAP + 100)).expect("a file");
        let head = read_head(&file, TEXT_CAP).unwrap();
        assert_eq!(head.len(), TEXT_CAP);
        assert!((head.len() as u64) < std::fs::metadata(&file).unwrap().len());
    }

}
