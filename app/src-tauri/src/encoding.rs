//! What a file's bytes say, and how to say it back the same way (`AMB-D-773`).
//!
//! Reading every file as UTF-8 put a `?` wherever a byte could not be read — over ten thousand of
//! them in one Shift_JIS document — and writing that text back would have written the `?`s into the
//! file. So a file is read in the encoding it is actually in, and what that was travels with the
//! text so it can be written back the same way.
//!
//! **The order is BOM, then UTF-8, then a guess**, and it is that order because each step is cheaper
//! and surer than the next. A BOM is an answer the file gave about itself. `str::from_utf8` costs
//! 26 µs over 256 KiB and settles 99.845% of a real folder's files (`AMB-T-3746` walked 486,169 of
//! them). Only what is left — one file in 645 — is guessed at, and a guess over a whole 5 MiB file
//! is tens of milliseconds.
//!
//! **The guess reads the whole file, not its front.** Of the 753 files in that walk that were not
//! UTF-8, 130 of them — 17% — were guessed differently from their first 4 KiB than from all of
//! them: a licence header in ASCII says nothing about the Japanese below it, and the first
//! non-ASCII byte was past 4 KiB in 69 of them.
//!
//! **A guess cannot be doubted.** [`chardetng`] returns one of its candidates always, publishes no
//! confidence, and its wrong answers came back with no replacement characters at all — all 46
//! misreads in that measurement produced zero. What tells a reader the guess was wrong is the text
//! on the screen, so the encoding is named to them and re-opening in another is theirs to ask for.

use encoding_rs::{Encoding, EncoderResult};

/// The encodings a file may be **written back** in — the ones a reader can be promised will survive
/// the round trip, plus the two Japanese legacy ones a real folder is full of (`AMB-T-3746` found
/// 151 Shift_JIS and EUC-JP CSVs, which is what Excel writes in Japan).
///
/// Anything else — Big5, GBK, KOI8, the other windows-125x — is still *read*, in whatever it was
/// guessed to be, and marked not clean: the text is shown, and saving is not offered. A list that
/// refused to show them would leave a reader with nothing where a readable file is; one that offered
/// to save them would be promising a round trip nobody has measured.
///
/// **UTF-16 is written here rather than by `encoding_rs`**, which has no encoder for it at all:
/// asked for one it hands back a UTF-8 encoder instead ([`Encoding::output_encoding`]), so a file
/// saved through that path would quietly stop being UTF-16. Writing it is three lines
/// ([`write()`]) and there is nothing it can fail on — a Rust `str` cannot hold an unpaired
/// surrogate, so the "this character cannot be written" answer never arises for UTF-16.
const WRITABLE: [&Encoding; 7] = [
    encoding_rs::UTF_8,
    encoding_rs::SHIFT_JIS,
    encoding_rs::EUC_JP,
    encoding_rs::WINDOWS_1252,
    encoding_rs::ISO_2022_JP,
    encoding_rs::UTF_16LE,
    encoding_rs::UTF_16BE,
];

/// How a file's lines end, as far as its bytes say.
///
/// `Mixed` is a value of its own rather than the majority rounded up, because the two forms cannot
/// be told apart again once they are read: an editor hands back one kind of newline for both. A file
/// written back in whichever kind was commoner would come out changed on **every line that was the
/// other kind** — 1,106 files in a real folder are mixed, and each would turn into a whole-file
/// diff. What to do about one is the reader's to say (`AMB-D-773`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineEnding {
    /// Every newline is `\n` — or there is no newline at all, which writes back the same either way.
    Lf,
    /// Every newline is `\r\n`.
    Crlf,
    /// Both, in the same file.
    Mixed,
}

/// What one file's bytes turned out to be, and the text they turned into.
pub struct Read {
    /// The text itself.
    pub text: String,
    /// The encoding it was read in, spelled the way `encoding_rs` spells it (`UTF-8`, `Shift_JIS`).
    pub encoding: &'static Encoding,
    /// Whether the file began with a byte order mark. Writing does not put one back by itself, and
    /// dropping it would rewrite the head of 178 files in a real folder.
    pub bom: bool,
    /// How its lines end.
    pub line_ending: LineEnding,
    /// Whether these bytes and this text are the same thing said twice — whether writing the text
    /// back in this encoding would produce the bytes that were read. False for a file cut at the
    /// read cap, for one whose bytes did not all decode, and for one in an encoding nothing here
    /// promises to write. A file that is not clean is a file to read and not to save.
    pub clean: bool,
}

/// Read `bytes` as text, in whatever encoding they are in.
///
/// `truncated` says the bytes stop short of the file rather than at its end, which changes two
/// answers: such a file is never clean, and a cut through the middle of a UTF-8 character is not
/// evidence that the file is something other than UTF-8 — the cut is ours. Left unhandled that one
/// byte is enough to lose the whole file to `windows-1252`, since a UTF-8 candidate that fails
/// anywhere is out of the running.
///
/// `tld` is the guess's only hint: the top-level domain of the reader's own language, which stands
/// in for which language's files are being looked at. `jp` took the three Japanese encodings from
/// 96% right to 100%, and costs only European files of 64 bytes or less (`AMB-T-3746`).
pub fn read(bytes: &[u8], truncated: bool, tld: Option<&[u8]>) -> Read {
    let line_ending = line_ending(bytes);

    // 1. What the file says about itself. Nothing else can answer for UTF-16, whose bytes the guess
    //    has no candidate for at all.
    if let Some((encoding, bom_len)) = Encoding::for_bom(bytes) {
        let (text, had_errors) = encoding.decode_without_bom_handling(&bytes[bom_len..]);
        return Read {
            clean: !had_errors && !truncated && WRITABLE.contains(&encoding),
            text: text.into_owned(),
            encoding,
            bom: true,
            line_ending,
        };
    }

    // 2. UTF-8, which nearly everything is, answered by the standard library rather than by a guess.
    match std::str::from_utf8(bytes) {
        Ok(text) => {
            return Read {
                text: text.to_owned(),
                encoding: encoding_rs::UTF_8,
                bom: false,
                line_ending,
                clean: !truncated,
            };
        }
        // A character cut in half at the very end, on bytes that stop short of the file: the cut is
        // the read cap's and not the file's. What is whole is UTF-8; the half character is dropped.
        Err(error) if truncated && error.error_len().is_none() => {
            let whole = &bytes[..error.valid_up_to()];
            return Read {
                text: String::from_utf8_lossy(whole).into_owned(),
                encoding: encoding_rs::UTF_8,
                bom: false,
                line_ending,
                clean: false,
            };
        }
        Err(_) => {}
    }

    // 3. The guess, over all of the bytes. ISO-2022-JP is allowed among the answers — the reason a
    //    browser denies it is that it can be used to smuggle script through a page, and nothing here
    //    runs what it reads. It is the one encoding guessed right every time (`AMB-T-3746`), and the
    //    one that would otherwise never be guessed at all, being 7-bit and so always valid UTF-8.
    let mut detector =
        chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Allow);
    detector.feed(bytes, !truncated);
    let encoding = detector.guess(tld, chardetng::Utf8Detection::Allow);
    let (text, had_errors) = encoding.decode_without_bom_handling(bytes);
    Read {
        clean: !had_errors && !truncated && WRITABLE.contains(&encoding),
        text: text.into_owned(),
        encoding,
        bom: false,
        line_ending,
    }
}

/// Write `text` back in `encoding`, putting a byte order mark back if the file had one.
///
/// The error is the **first character that cannot be written**, by name. That is the whole reason
/// this is not `Encoding::encode`: that one writes what it cannot encode as an HTML numeric
/// reference, so a `✓` — a character that is in this project's own notes — reaches the file as
/// `&#10003;` and the reader is never told. Named, it can be said out loud instead, and the save
/// stopped while it still can be (`AMB-D-773`).
///
/// `encoding` has to be one [`writable`] answered with. Handed anything else — `replacement`, one
/// of the encodings only read — `encoding_rs` would quietly encode UTF-8 in its place.
///
/// **UTF-16 is the one written here rather than by `encoding_rs`**, for the reason [`WRITABLE`]
/// gives, and it is written by laying the text's own code units down in the order the encoding
/// names. Nothing about it can fail: `encode_utf16` is the same sequence the text is already held
/// as, and a `str` cannot hold a lone surrogate for the pairing to go wrong on.
pub fn write(text: &str, encoding: &'static Encoding, bom: bool) -> Result<Vec<u8>, char> {
    if let Some(big_endian) = utf16_endianness(encoding) {
        let mut out = Vec::with_capacity(2 * text.len() + 2);
        if bom {
            out.extend_from_slice(bom_of(encoding));
        }
        for unit in text.encode_utf16() {
            let pair = if big_endian { unit.to_be_bytes() } else { unit.to_le_bytes() };
            out.extend_from_slice(&pair);
        }
        return Ok(out);
    }

    debug_assert_eq!(encoding.output_encoding(), encoding, "an encoding nothing writes");
    let mut out = Vec::with_capacity(text.len() + 3);
    if bom {
        out.extend_from_slice(bom_of(encoding));
    }

    let mut encoder = encoding.new_encoder();
    let mut buffer = [0u8; 4096];
    let mut rest = text;
    loop {
        let (result, read, written) =
            encoder.encode_from_utf8_without_replacement(rest, &mut buffer, true);
        out.extend_from_slice(&buffer[..written]);
        rest = &rest[read..];
        match result {
            EncoderResult::InputEmpty => return Ok(out),
            EncoderResult::OutputFull => continue,
            EncoderResult::Unmappable(character) => return Err(character),
        }
    }
}

/// Turn the name that travelled with the text back into an encoding, and only where writing it back
/// is something this promises. A name from anywhere else is a name a caller made up.
pub fn writable(name: &str) -> Option<&'static Encoding> {
    let encoding = Encoding::for_label(name.as_bytes())?;
    WRITABLE.contains(&encoding).then_some(encoding)
}

/// The byte order mark to put back. Three encodings have one and the rest have none, and encoding
/// text never puts a mark back by itself.
///
/// **A UTF-16 file without one cannot be read at all**, so writing it back without one would be
/// writing a file this module could no longer open: the guess has no UTF-16 candidate, and the mark
/// is the whole of how such a file is recognised. That is not enforced here — what a file had is
/// what it gets back ([`Read::bom`]), and a UTF-16 file that reached the reader had a mark.
fn bom_of(encoding: &'static Encoding) -> &'static [u8] {
    if encoding == encoding_rs::UTF_8 {
        &[0xEF, 0xBB, 0xBF]
    } else if encoding == encoding_rs::UTF_16LE {
        &[0xFF, 0xFE]
    } else if encoding == encoding_rs::UTF_16BE {
        &[0xFE, 0xFF]
    } else {
        &[]
    }
}

/// Whether this encoding is one of the two UTF-16 forms, and if so which end its code units lead
/// with — the one question that decides how [`write()`] lays them down.
fn utf16_endianness(encoding: &'static Encoding) -> Option<bool> {
    if encoding == encoding_rs::UTF_16BE {
        Some(true)
    } else if encoding == encoding_rs::UTF_16LE {
        Some(false)
    } else {
        None
    }
}

/// Which newline the bytes use, read off the bytes rather than off the text: a `\r` is a `\r`
/// whatever encoding surrounds it in every encoding read here.
fn line_ending(bytes: &[u8]) -> LineEnding {
    let mut crlf = false;
    let mut lf = false;
    let mut previous = 0u8;
    for byte in bytes {
        if *byte == b'\n' {
            if previous == b'\r' {
                crlf = true;
            } else {
                lf = true;
            }
        }
        previous = *byte;
    }
    match (crlf, lf) {
        (true, true) => LineEnding::Mixed,
        (true, false) => LineEnding::Crlf,
        _ => LineEnding::Lf,
    }
}

/// The top-level domain that stands for a language code, as the guess's hint.
///
/// It is a domain because that is what [`chardetng`] takes — a browser's question, where the answer
/// came from the address a page was loaded from. What it really names is which language's files are
/// being looked at, and here that is the language the reader chose to be spoken to in.
///
/// A language with no legacy encoding of its own is left unhinted: the hint only shifts the odds
/// between candidates, and shifting them towards an encoding nobody's files are in would cost the
/// short European files it already costs while buying nothing.
pub fn tld_for(language: Option<&str>) -> Option<&'static [u8]> {
    let code = language?;
    let primary = code.split(['-', '_']).next().unwrap_or(code).to_ascii_lowercase();
    let region = code.split(['-', '_']).nth(1).unwrap_or_default().to_ascii_lowercase();
    Some(match (primary.as_str(), region.as_str()) {
        ("ja", _) => b"jp".as_slice(),
        ("ko", _) => b"kr",
        ("zh", "hant") => b"tw",
        ("zh", _) => b"cn",
        ("ru", _) => b"ru",
        ("uk", _) => b"ua",
        ("pl", _) => b"pl",
        ("tr", _) => b"tr",
        ("th", _) => b"th",
        ("vi", _) => b"vn",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one thing this module exists for: a Japanese document in a legacy encoding reads as what
    /// somebody wrote, not as a page of `?` (`AMB-D-773`).
    #[test]
    fn a_japanese_document_reads_and_writes_back_byte_for_byte() {
        let written = "日本語のなかに English が混ざった文書。\n二行目もある。\n";
        for encoding in [encoding_rs::SHIFT_JIS, encoding_rs::EUC_JP, encoding_rs::UTF_8] {
            let (bytes, _, had_errors) = encoding.encode(written);
            assert!(!had_errors, "{} can write this", encoding.name());

            let read = read(&bytes, false, Some(b"jp"));
            assert_eq!(read.encoding, encoding, "guessed {}", read.encoding.name());
            assert_eq!(read.text, written);
            assert!(read.clean);
            assert!(!read.text.contains('\u{FFFD}'), "nothing was replaced");

            assert_eq!(write(&read.text, read.encoding, read.bom), Ok(bytes.to_vec()));
        }
    }

    /// A byte order mark is the file's own answer, taken before anything is guessed — and put back,
    /// since writing does not do it by itself. Dropping it would rewrite the head of 178 files in a
    /// real folder.
    #[test]
    fn a_mark_is_read_and_written_back() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("日本語 hello".as_bytes());

        let read = read(&bytes, false, None);
        assert_eq!(read.encoding, encoding_rs::UTF_8);
        assert!(read.bom, "the file began with a mark");
        assert_eq!(read.text, "日本語 hello");
        assert!(read.clean);
        assert_eq!(write(&read.text, read.encoding, read.bom), Ok(bytes));
    }

    /// UTF-16 is what only the mark can recognise — the guess has no candidate for it — and it
    /// round-trips byte for byte, mark and all. The bytes are laid down here rather than by
    /// `encoding_rs`, which has no UTF-16 encoder and hands back a UTF-8 one when asked: a file
    /// saved through that path would quietly stop being UTF-16.
    #[test]
    fn a_utf_16_file_reads_and_writes_back_byte_for_byte() {
        // A character outside the basic plane, which is the one shape a UTF-16 round trip can get
        // wrong: it is two code units rather than one, and the pair has to keep its order.
        let written = "日本語 hello 😀";
        for (encoding, bom) in [
            (encoding_rs::UTF_16LE, [0xFF, 0xFE].as_slice()),
            (encoding_rs::UTF_16BE, &[0xFE, 0xFF]),
        ] {
            let mut bytes = bom.to_vec();
            for unit in written.encode_utf16() {
                let pair = if encoding == encoding_rs::UTF_16LE {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                };
                bytes.extend_from_slice(&pair);
            }

            let read = read(&bytes, false, None);
            assert_eq!(read.encoding, encoding);
            assert!(read.bom);
            assert_eq!(read.text, written);
            assert!(read.clean, "{} is written as well as read", encoding.name());
            assert_eq!(writable(encoding.name()), Some(encoding));
            assert_eq!(write(&read.text, read.encoding, read.bom), Ok(bytes));
        }
    }

    /// The two forms are the same code units in opposite orders, which is the whole of the
    /// difference and the one thing a hand-written encoder can get backwards. Read back, each says
    /// what it was written as.
    #[test]
    fn the_two_utf_16_forms_are_written_end_for_end() {
        let le = write("A", encoding_rs::UTF_16LE, false).expect("nothing can fail here");
        let be = write("A", encoding_rs::UTF_16BE, false).expect("nothing can fail here");
        assert_eq!(le, [0x41, 0x00]);
        assert_eq!(be, [0x00, 0x41]);

        // And the mark, which is the same two bytes the other way round for the same reason.
        assert_eq!(write("", encoding_rs::UTF_16LE, true), Ok(vec![0xFF, 0xFE]));
        assert_eq!(write("", encoding_rs::UTF_16BE, true), Ok(vec![0xFE, 0xFF]));
    }

    /// A UTF-16 file whose bytes do not all decode is read and not saved, the same as any other
    /// file that would not come back as it went in — a lone surrogate has no character to become,
    /// and the replacement it turns into is not what was in the file.
    #[test]
    fn a_utf_16_file_with_a_lone_surrogate_is_read_and_not_saved() {
        // `FF FE` the mark, `41 00` the letter A, `00 D8` a leading surrogate with nothing after it.
        let bytes = vec![0xFF, 0xFE, 0x41, 0x00, 0x00, 0xD8];

        let read = read(&bytes, false, None);
        assert_eq!(read.encoding, encoding_rs::UTF_16LE);
        assert!(read.text.contains('\u{FFFD}'), "the half a character was replaced");
        assert!(!read.clean, "these bytes and this text are not the same thing said twice");
    }

    /// A character this encoding cannot write is named rather than mangled. `Encoding::encode` would
    /// have written `&#10003;` into the file and said nothing a reader could see.
    #[test]
    fn a_character_that_cannot_be_written_comes_back_by_name() {
        assert_eq!(write("これは ✓ です", encoding_rs::SHIFT_JIS, false), Err('✓'));
        assert_eq!(write("これは 😀 です", encoding_rs::EUC_JP, false), Err('😀'));
        // And what can be written, is.
        assert!(write("これは書けます", encoding_rs::SHIFT_JIS, false).is_ok());
    }

    /// A file cut at the read cap can be cut through the middle of a character, and that one byte is
    /// enough to put UTF-8 out of the running — the guess then answers `windows-1252` and every
    /// Japanese character on the screen turns into two European ones. The cut is ours, so it is ours
    /// to allow for.
    #[test]
    fn a_character_cut_in_half_by_the_cap_is_still_utf_8() {
        let whole = "日本語のテキスト".as_bytes();
        let cut = &whole[..whole.len() - 1];

        let read = read(cut, true, Some(b"jp"));
        assert_eq!(read.encoding, encoding_rs::UTF_8);
        assert_eq!(read.text, "日本語のテキス");
        assert!(!read.clean, "a file that stops short is not one to write back");
    }

    /// Both kinds of newline in one file is an answer of its own. Rounded to the commoner one, every
    /// line of the other kind would come back changed.
    #[test]
    fn a_file_with_both_newlines_says_so() {
        assert_eq!(line_ending(b"one\ntwo\n"), LineEnding::Lf);
        assert_eq!(line_ending(b"one\r\ntwo\r\n"), LineEnding::Crlf);
        assert_eq!(line_ending(b"one\r\ntwo\n"), LineEnding::Mixed);
        // No newline at all writes back the same either way.
        assert_eq!(line_ending(b"one line"), LineEnding::Lf);
    }

    /// An encoding nothing here promises to write is still read — the text is what a reader came
    /// for — and it is not offered for saving, which is a promise nobody has measured.
    #[test]
    fn a_file_in_an_encoding_we_do_not_write_is_read_but_not_clean() {
        let (bytes, _, _) = encoding_rs::BIG5.encode("這是繁體中文的一段文字，用來測試編碼的判斷。");
        let read = read(&bytes, false, Some(b"tw"));
        assert!(!read.text.is_empty());
        assert!(!read.clean, "guessed {} — read, not written", read.encoding.name());
        assert!(writable(read.encoding.name()).is_none());
        // The ones it does promise come back by the same name they travelled under.
        assert_eq!(writable("Shift_JIS"), Some(encoding_rs::SHIFT_JIS));
        assert_eq!(writable("UTF-8"), Some(encoding_rs::UTF_8));
    }

    /// The hint is the reader's own language, and only where that language has files in an encoding
    /// of its own to be told apart.
    #[test]
    fn the_hint_follows_the_language_the_reader_chose() {
        assert_eq!(tld_for(Some("ja")), Some(b"jp".as_slice()));
        assert_eq!(tld_for(Some("ja-JP")), Some(b"jp".as_slice()));
        assert_eq!(tld_for(Some("zh-Hant")), Some(b"tw".as_slice()));
        assert_eq!(tld_for(Some("zh-Hans")), Some(b"cn".as_slice()));
        assert_eq!(tld_for(Some("en")), None);
        assert_eq!(tld_for(None), None);
    }
}
