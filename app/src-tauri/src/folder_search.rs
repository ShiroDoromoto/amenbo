//! Looking through a whole folder for a word, rather than through the file that happens to be open
//! (`AMB-D-910`).
//!
//! **What this costs is the reading, and nothing else.** Measured over the 18 bound folders of a
//! real store (`AMB-T-4917`): of the 2.1 seconds one pass takes serially, 2,071 ms is the disk,
//! 118 ms is guessing an encoding for the one file in 234 that is not UTF-8, and 4 ms is the
//! matching. So none of the three is worth leaving out, and the one thing worth being careful about
//! is how many bytes are read at all — which is the walk's business rather than this module's
//! (`crate::folder_walk`). A cold first pass is 4–6 seconds where a warm one is 0.8.
//!
//! **Three things follow from that, and they are what this module is shaped by.**
//!
//! 1. **The answer is sent while it is being found.** The first one percent of the hits is ready in
//!    5–9 ms and the last of them 0.7–1.6 seconds later, so a face made to wait for the whole
//!    answer would stand still for a second over a search that was all but done immediately.
//! 2. **It can be called off.** A reader typing a second search has stopped caring about the first,
//!    and the first is holding ten threads and the disk.
//! 3. **There is a ceiling on the answer.** `the` over that same store matches 358,935 times, which
//!    is 78.9 MB of JSON — a number nobody reads and no window survives being handed.
//!
//! **A file is read once, not twice.** The head is read to decide whether the bytes are text, and
//! where they are the read carries on from the same handle: opening a second time cost 192 ms over
//! 24,042 files, which is 9% of the pass for nothing (`AMB-T-4917` measured all three shapes).

use std::io::Read as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use regex::{Regex, RegexBuilder};
use tauri::Emitter as _;

use crate::dto::{FolderSearchDoneDto, FolderSearchFileDto, FolderSearchFoundDto, FolderSearchLineDto, FolderSearchSpanDto};
use crate::error::CmdError;
use crate::folder_bytes::{HEAD, TEXT_CAP};
use crate::folder_fence::open_no_follow;

/// The event a batch of hits arrives on, and the one that says the search is over.
const FOUND_EVENT: &str = "folder-search-found";
const DONE_EVENT: &str = "folder-search-done";

/// The most matching lines a search hands back, over all of its files.
///
/// `the` over a real store's 18 folders matches 358,935 times and 78.9 MB (`AMB-T-4917`). At the
/// same 220 bytes a line this is about 4 MB, which is the largest answer worth carrying across the
/// seam — and a search that reaches it has already told the reader what they needed to know, which
/// is that the word they asked for is everywhere.
const HIT_CAP: usize = 20_000;

/// The most matching lines carried out of one file. A minified bundle is one file and can hold
/// thousands, and a reader scrolling past them is no closer to the file they were looking for.
const FILE_LINES_CAP: usize = 200;

/// The most matches marked on one line, for the same reason at the other scale: a minified bundle
/// is also one *line*.
const LINE_SPANS_CAP: usize = 100;

/// How much of a long line is carried, and how much of it comes before the first match on it.
///
/// A line is carried whole where it fits. Where it does not, what is carried is a window around the
/// first match rather than the front of the line: a bundle's one line is megabytes long, and its
/// first thousand bytes say nothing about a match forty thousand bytes in.
const LINE_WINDOW: usize = 1000;
const LINE_LEAD: usize = 200;

/// How many files with hits gather before a batch is sent, and how long a batch waits when they
/// come in more slowly than that.
const BATCH: usize = 64;
const FLUSH: Duration = Duration::from_millis(60);

/// How many threads read at once. Ten was measured as the point where more stops helping — twenty
/// came back with the same 715 ms (`AMB-T-4917`) — and the ceiling matters because the machine is
/// the reader's, with their own editor on it.
const THREADS: usize = 10;

/// The searches running, one per window that asked.
///
/// **A window searches one thing at a time.** The reader typing a second search has stopped caring
/// about the first, so starting one here is what stops the one before it — the face does not have
/// to say so, and cannot forget to.
///
/// The key is the window's own label, and a flag stays in the list after its search has ended: the
/// next search in that window writes over it. So the list is as long as the number of windows that
/// have ever searched, which is the number of windows.
#[derive(Default)]
pub struct FolderSearches(Mutex<std::collections::HashMap<String, Running>>);

/// One window's search: the flag the threads read to learn they are not wanted, and which search it
/// is. The tag is the face's own count, so a message about a search that has already been replaced
/// stops nothing.
struct Running {
    stop: Arc<AtomicBool>,
    tag: u64,
}

impl FolderSearches {
    /// Take over this window's search: stop whatever it was running, and answer with the flag the
    /// new one is to read.
    fn begin(&self, label: &str, tag: u64) -> Arc<AtomicBool> {
        let stop = Arc::new(AtomicBool::new(false));
        let mut running = self.0.lock().expect("the search registry");
        if let Some(before) = running.insert(label.to_string(), Running { stop: Arc::clone(&stop), tag }) {
            before.stop.store(true, Ordering::Relaxed);
        }
        stop
    }

    /// Call off this window's search, where the one running is the one being spoken about.
    fn end(&self, label: &str, tag: u64) {
        let running = self.0.lock().expect("the search registry");
        let Some(live) = running.get(label) else { return };
        if live.tag == tag {
            live.stop.store(true, Ordering::Relaxed);
        }
    }
}

/// Look through one of a project's folders for a word, and send what is found as it is found.
///
/// The command answers as soon as the folder and the pattern are known to be good; everything after
/// that arrives on [`FOUND_EVENT`], and [`DONE_EVENT`] says there is no more coming. `tag` is the
/// face's own count of which search this is: it comes back on every event, so a batch from a search
/// the reader has already moved on from can be dropped rather than drawn.
///
/// `ignored` is the switch `AMB-D-910` puts on the screen. Left off, the folder's own `.gitignore`
/// is read and what it calls noise is not searched; turned on, the walk is the one the tree is drawn
/// from. The difference is not small — 24,042 files against 177,752, and 148 MB against 1,652 MB —
/// which is why the switch exists rather than one walk being picked for both.
///
/// `regex` says the query is a pattern rather than a word. Either way it is compiled to one, because
/// a literal compiles to a pattern whose whole body is a literal and the engine searches that with
/// the same `memchr` a bare literal search would have used.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn folder_search(
    app: tauri::AppHandle,
    window: tauri::Window,
    searches: tauri::State<'_, FolderSearches>,
    project_id: i64,
    root: String,
    query: String,
    regex: bool,
    case_sensitive: bool,
    whole_word: bool,
    ignored: bool,
    tag: u64,
) -> Result<(), CmdError> {
    let asked = root.clone();
    let dir = tauri::async_runtime::spawn_blocking(move || {
        crate::folder_fence::root_of(project_id, &asked)
    })
    .await
    .map_err(|e| -> CmdError { format!("looking through this folder did not start: {e}").into() })??;

    let matcher = pattern(&query, regex, case_sensitive, whole_word)?;
    let stop = searches.begin(window.label(), tag);
    let label = window.label().to_string();

    std::thread::spawn(move || {
        let sent = |files: Vec<FolderSearchFileDto>| {
            let _ = app.emit_to(
                label.as_str(),
                FOUND_EVENT,
                FolderSearchFoundDto { root: root.clone(), tag, files },
            );
        };
        let tally = hunt(&dir, ignored, &matcher, &stop, &sent);
        let _ = app.emit_to(
            label.as_str(),
            DONE_EVENT,
            FolderSearchDoneDto {
                root: root.clone(),
                tag,
                files: tally.files as u32,
                hits: tally.hits as u32,
                capped: tally.capped,
                stopped: tally.stopped,
            },
        );
    });
    Ok(())
}

/// Call off the search a window is running. The face says so when its search panel goes away; a
/// second search calls this one off by itself ([`FolderSearches::begin`]).
#[tauri::command]
pub fn folder_search_stop(window: tauri::Window, searches: tauri::State<'_, FolderSearches>, tag: u64) {
    searches.end(window.label(), tag);
}

/// The query as something to match with.
///
/// A literal is escaped into a pattern rather than searched for as bytes, so that one engine answers
/// for all three of the switches the face offers. It costs nothing worth measuring: matching was
/// 4 ms of a 2.1-second pass as a plain literal and 910 ms at its very worst as a real pattern
/// (`AMB-T-4917`), against 2,071 ms of reading either way.
///
/// **A pattern the reader is still typing is refused, not ignored.** `(` is half of something, and a
/// search that answered it with nothing would read as "no such text in this folder".
fn pattern(query: &str, regex: bool, case_sensitive: bool, whole_word: bool) -> Result<Regex, CmdError> {
    if query.is_empty() {
        return Err(CmdError::coded(
            "folder.search",
            "there is nothing to look for",
            serde_json::Value::Null,
        ));
    }
    let body = if regex { query.to_string() } else { regex::escape(query) };
    let body = if whole_word { format!(r"\b(?:{body})\b") } else { body };
    RegexBuilder::new(&body)
        .case_insensitive(!case_sensitive)
        // A pattern is the reader's own text, and one they mistyped can ask for a program too large
        // to build. The refusal is the same one a pattern with a stray bracket earns.
        .size_limit(1 << 20)
        .build()
        .map_err(|e| {
            CmdError::coded(
                "folder.search_pattern",
                format!("this is not a pattern that can be searched with: {e}"),
                serde_json::Value::Null,
            )
        })
}

/// What a whole search came to, beyond the hits themselves.
struct Tally {
    /// How many files were read and looked through.
    files: usize,
    /// How many matching lines were sent.
    hits: usize,
    /// Whether the search stopped at [`HIT_CAP`] rather than at the end of the folder.
    capped: bool,
    /// Whether it was called off — by the reader, or by their next search.
    stopped: bool,
}

/// The walk, the read and the match, with what is found handed to `sent` in batches as it goes.
///
/// `sent` is taken rather than written here so the whole of this can be run against a folder in a
/// test, where there is no window to emit to.
fn hunt(
    root: &Path,
    ignored: bool,
    matcher: &Regex,
    stop: &AtomicBool,
    sent: &(dyn Fn(Vec<FolderSearchFileDto>) + Sync),
) -> Tally {
    // Asked once rather than per file: it reads the reader's config off the disk, and the walk
    // below opens tens of thousands of files.
    let tld = crate::folder_bytes::language_tld();
    let files = AtomicUsize::new(0);
    let hits = AtomicUsize::new(0);
    let batch: Mutex<(Vec<FolderSearchFileDto>, Instant)> = Mutex::new((Vec::new(), Instant::now()));

    let mut walk = if ignored {
        crate::folder_walk::shown(root)
    } else {
        crate::folder_walk::walker(root)
    };
    walk.threads(THREADS);

    walk.build_parallel().run(|| {
        Box::new(|entry| {
            if stop.load(Ordering::Relaxed) || hits.load(Ordering::Relaxed) >= HIT_CAP {
                return ignore::WalkState::Quit;
            }
            let Ok(entry) = entry else { return ignore::WalkState::Continue };
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                return ignore::WalkState::Continue;
            }
            let Ok(Some(bytes)) = text(entry.path()) else {
                return ignore::WalkState::Continue;
            };
            files.fetch_add(1, Ordering::Relaxed);
            // The same three steps `crate::encoding` reads an open file with, in the same order:
            // what the file says about itself, then UTF-8, then a guess over what is left.
            let truncated = bytes.len() >= TEXT_CAP;
            let read = crate::encoding::read(&bytes, truncated, tld);
            let Some(path) = named(root, entry.path()) else {
                return ignore::WalkState::Continue;
            };
            // The mark is taken over the bytes that were read, which is the same stretch of the
            // file `crate::folder_bytes::digest_of` takes one over — so a replacement can hold what
            // it reads against what the search read (`AMB-D-911`).
            let Some(file) = looked(&read.text, path, crate::folder_bytes::digest(&bytes), matcher)
            else {
                return ignore::WalkState::Continue;
            };

            let found = file.lines.len();
            if hits.fetch_add(found, Ordering::Relaxed) + found >= HIT_CAP {
                // The cap is read again at the top of the next entry, so what this one found is
                // still sent: a batch thrown away here would be hits nobody was told about.
                flush(&batch, Some(file), sent, true);
                return ignore::WalkState::Quit;
            }
            flush(&batch, Some(file), sent, false);
            ignore::WalkState::Continue
        })
    });

    flush(&batch, None, sent, true);
    let found = hits.load(Ordering::Relaxed);
    Tally {
        files: files.load(Ordering::Relaxed),
        // What was actually sent, which is a little over the ceiling rather than exactly it: the
        // file that reached it was sent whole, and the threads beside it had files of their own in
        // hand. A count rounded down to the ceiling would not add up against what the face drew.
        hits: found,
        capped: found >= HIT_CAP,
        stopped: stop.load(Ordering::Relaxed),
    }
}

/// Add a file to the batch and send the batch on when it is full, has waited long enough, or is the
/// last one.
///
/// The batch is taken out from under the lock before it is sent, so one thread's emit does not hold
/// up another thread's read.
fn flush(
    batch: &Mutex<(Vec<FolderSearchFileDto>, Instant)>,
    file: Option<FolderSearchFileDto>,
    sent: &(dyn Fn(Vec<FolderSearchFileDto>) + Sync),
    last: bool,
) {
    let ready = {
        let mut held = batch.lock().expect("the batch being gathered");
        if let Some(file) = file {
            held.0.push(file);
        }
        if held.0.is_empty() || !(last || held.0.len() >= BATCH || held.1.elapsed() >= FLUSH) {
            return;
        }
        held.1 = Instant::now();
        std::mem::take(&mut held.0)
    };
    sent(ready);
}

/// The file's path as the caller names one: the segments under the root, and nothing above it.
fn named(root: &Path, file: &Path) -> Option<Vec<String>> {
    Some(
        file.strip_prefix(root)
            .ok()?
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect(),
    )
}

/// One file's text, or nothing where it holds no text.
///
/// **Opened once.** The head decides whether these bytes are text — a NUL in it is what says they
/// are not, the same judgement `crate::folder_bytes` makes and for the same reason — and where they
/// are, the rest is read from the handle that is already open.
pub(crate) fn text(file: &Path) -> std::io::Result<Option<Vec<u8>>> {
    let mut open = open_no_follow(file)?;
    let mut bytes = Vec::with_capacity(HEAD);
    open.by_ref().take(HEAD as u64).read_to_end(&mut bytes)?;
    if bytes.contains(&0) {
        return Ok(None);
    }
    if bytes.len() == HEAD {
        open.take((TEXT_CAP - HEAD) as u64).read_to_end(&mut bytes)?;
    }
    Ok(Some(bytes))
}

/// The matching lines of one file, or nothing where it has none.
fn looked(
    text: &str,
    path: Vec<String>,
    digest: String,
    matcher: &Regex,
) -> Option<FolderSearchFileDto> {
    let mut lines = Vec::new();
    let mut more = false;
    for (n, line) in text.lines().enumerate() {
        if lines.len() >= FILE_LINES_CAP {
            more = true;
            break;
        }
        let mut spans = Vec::new();
        let mut first = None;
        for found in matcher.find_iter(line) {
            // A pattern that can match nothing — `x*` — matches at every position of every line,
            // which is an answer about the pattern rather than about the folder.
            if found.is_empty() {
                continue;
            }
            if first.is_none() {
                first = Some(found.start());
            }
            if spans.len() >= LINE_SPANS_CAP {
                more = true;
                break;
            }
            spans.push(FolderSearchSpanDto {
                at: utf16(&line[..found.start()]),
                length: utf16(found.as_str()),
            });
        }
        let Some(first) = first else { continue };
        if spans.is_empty() {
            continue;
        }
        let (text, from, cut) = window(line, first);
        lines.push(FolderSearchLineDto { line: n as u32 + 1, from, text, cut, spans });
    }
    if lines.is_empty() {
        return None;
    }
    Some(FolderSearchFileDto { path, digest, lines, more })
}

/// As much of a line as is worth carrying, and where in the line it starts.
///
/// A line that fits is carried whole. One that does not is carried as a window holding the first
/// match, because the front of a minified bundle's single line says nothing about a match deep
/// inside it. Both ends land on character boundaries.
fn window(line: &str, first: usize) -> (String, u32, bool) {
    if line.len() <= LINE_WINDOW {
        return (line.to_string(), 0, false);
    }
    let start = boundary(line, first.saturating_sub(LINE_LEAD), false);
    let end = boundary(line, (start + LINE_WINDOW).min(line.len()), true);
    (line[start..end].to_string(), utf16(&line[..start]), true)
}

/// The nearest character boundary at or before `at` — or at or after it, reading `up`.
fn boundary(line: &str, at: usize, up: bool) -> usize {
    let mut at = at.min(line.len());
    while !line.is_char_boundary(at) {
        if up {
            at += 1;
        } else {
            at -= 1;
        }
    }
    at
}

/// How long a piece of text is in the units a webview counts a string in.
///
/// The face slices these strings to draw the match in them, and JavaScript slices by UTF-16 code
/// unit: a column counted in bytes or in characters would be off by however many of each is not one
/// of the other, which is every emoji and every Japanese character respectively.
fn utf16(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder with something to find in it, built the same way each test reads it.
    fn folder() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("notes")).expect("a folder");
        std::fs::create_dir_all(root.join("build")).expect("this project's own");
        std::fs::create_dir_all(root.join("node_modules")).expect("the machine's");
        std::fs::write(root.join(".gitignore"), "build/\n").expect("the ignore file");
        std::fs::write(root.join("notes/a.md"), "hello needle\nnothing\nNEEDLE again\n").expect("a file");
        std::fs::write(root.join("build/out.js"), "needle in the build\n").expect("a built file");
        std::fs::write(root.join("node_modules/x.js"), "needle in the machine's\n").expect("the machine's file");
        std::fs::write(root.join("picture.png"), b"\x89PNG\r\n\x1a\n\0needle").expect("not text");
        dir
    }

    /// Run a whole search and hand back every file it found, in one list.
    fn search(root: &Path, ignored: bool, matcher: &Regex) -> (Vec<FolderSearchFileDto>, Tally) {
        let found = Mutex::new(Vec::new());
        let stop = AtomicBool::new(false);
        let tally = hunt(root, ignored, matcher, &stop, &|files| {
            found.lock().expect("the found list").extend(files);
        });
        let mut files = found.into_inner().expect("the found list");
        files.sort_by(|a, b| a.path.cmp(&b.path));
        (files, tally)
    }

    fn at(files: &[FolderSearchFileDto], name: &str) -> Option<usize> {
        files.iter().position(|f| f.path.join("/") == name)
    }

    /// The switch `AMB-D-910` puts on the screen is the whole difference between the two walks: off,
    /// the folder's own ignore file is read; on, the walk is the one the tree is drawn from. The
    /// floor is under both, so what a build wrote is never searched either way.
    #[test]
    fn the_ignore_file_is_read_unless_the_reader_says_not_to() {
        let dir = folder();
        let root = dir.path();
        let matcher = pattern("needle", false, true, false).expect("a pattern");

        let (kept, _) = search(root, false, &matcher);
        assert!(at(&kept, "notes/a.md").is_some(), "{:?}", paths(&kept));
        assert!(at(&kept, "build/out.js").is_none(), "ignored: {:?}", paths(&kept));

        let (all, _) = search(root, true, &matcher);
        assert!(at(&all, "build/out.js").is_some(), "the switch reaches it: {:?}", paths(&all));
        for both in [&kept, &all] {
            assert!(
                at(both, "node_modules/x.js").is_none(),
                "the floor is pruned either way: {:?}",
                paths(both),
            );
        }
    }

    fn paths(files: &[FolderSearchFileDto]) -> Vec<String> {
        files.iter().map(|f| f.path.join("/")).collect()
    }

    /// Case is the reader's to ask about, and a line is answered with the number a reader counts
    /// from one — not from zero, which is what a file's first line is in no editor.
    #[test]
    fn case_is_asked_about_and_lines_are_counted_from_one() {
        let dir = folder();
        let root = dir.path();

        let loose = pattern("needle", false, false, false).expect("a pattern");
        let (files, _) = search(root, false, &loose);
        let found = &files[at(&files, "notes/a.md").expect("the file")];
        assert_eq!(
            found.lines.iter().map(|l| l.line).collect::<Vec<_>>(),
            vec![1, 3],
            "both spellings, and the blank line between them is line 2",
        );

        let exact = pattern("needle", false, true, false).expect("a pattern");
        let (files, _) = search(root, false, &exact);
        let found = &files[at(&files, "notes/a.md").expect("the file")];
        assert_eq!(found.lines.iter().map(|l| l.line).collect::<Vec<_>>(), vec![1]);
    }

    /// A word is a word and not a run of letters inside one, and a pattern is a pattern.
    #[test]
    fn a_word_and_a_pattern_are_each_asked_for_as_themselves() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        std::fs::write(root.join("a.txt"), "needle\nneedles\n").expect("a file");

        let word = pattern("needle", false, true, true).expect("a pattern");
        let (files, _) = search(root, false, &word);
        assert_eq!(files[0].lines.len(), 1, "`needles` is another word: {:?}", files[0].lines);

        let literal = pattern("needle.", false, true, false).expect("a pattern");
        let (files, _) = search(root, false, &literal);
        assert!(files.is_empty(), "a literal dot is a dot: {:?}", paths(&files));

        let re = pattern("needle.", true, true, false).expect("a pattern");
        let (files, _) = search(root, false, &re);
        assert_eq!(files[0].lines.len(), 1, "as a pattern it reaches `needles`");

        assert!(pattern("(", true, true, false).is_err(), "half a pattern is refused, not ignored");
        assert!(pattern("", false, true, false).is_err());
    }

    /// A file's bytes are text or they are not, and a NUL in the head is what says so — the same
    /// judgement the panel's own read makes, so a picture is never searched.
    #[test]
    fn what_has_a_nul_in_its_head_is_not_looked_through() {
        let dir = folder();
        let matcher = pattern("needle", false, true, false).expect("a pattern");
        let (files, tally) = search(dir.path(), true, &matcher);
        assert!(at(&files, "picture.png").is_none(), "{:?}", paths(&files));
        assert!(tally.files > 0);
    }

    /// A file that is not UTF-8 is read in the encoding it is actually in, which is the one in 234
    /// the guess is there for (`AMB-T-4917`). Left as bytes, a Japanese word would never match.
    #[test]
    fn a_file_in_another_encoding_is_read_in_it() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode("これはタスクの話\n");
        std::fs::write(root.join("sjis.txt"), &bytes[..]).expect("a file");

        let matcher = pattern("タスク", false, true, false).expect("a pattern");
        let (files, _) = search(root, false, &matcher);
        assert_eq!(files.len(), 1, "read as Shift_JIS, the word is there: {:?}", paths(&files));
        assert_eq!(files[0].lines[0].spans.len(), 1);
    }

    /// A column is counted the way the webview counts one, so the face can slice the line it is
    /// handed with the number it is handed.
    #[test]
    fn a_column_is_counted_in_what_a_webview_counts_in() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        std::fs::write(root.join("a.txt"), "日本語 needle\n🍣 needle\n").expect("a file");

        let matcher = pattern("needle", false, true, false).expect("a pattern");
        let (files, _) = search(root, false, &matcher);
        let lines = &files[0].lines;
        // Three Japanese characters and a space: three UTF-16 units each of one, one of the space.
        assert_eq!(lines[0].spans[0].at, 4);
        // One emoji is a surrogate pair — two units — and the space after it makes three.
        assert_eq!(lines[1].spans[0].at, 3);
        assert_eq!(lines[0].spans[0].length, 6);
    }

    /// A line too long to carry is carried as a window holding the first match, and the columns go
    /// on being counted from the start of the line rather than from the start of the window.
    #[test]
    fn a_long_line_is_carried_as_a_window_around_the_match() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        let line = format!("{}needle{}\n", "x".repeat(40_000), "y".repeat(40_000));
        std::fs::write(root.join("bundle.js"), line).expect("a file");

        let matcher = pattern("needle", false, true, false).expect("a pattern");
        let (files, _) = search(root, false, &matcher);
        let line = &files[0].lines[0];
        assert!(line.cut);
        assert!(line.text.len() <= LINE_WINDOW + 4, "{} carried", line.text.len());
        assert!(line.text.contains("needle"), "the window holds the match");
        assert_eq!(line.spans[0].at, 40_000, "counted from the start of the line");
        assert_eq!(line.from, (40_000 - LINE_LEAD) as u32);
    }

    /// The answer has a ceiling, and the search says so rather than stopping quietly.
    #[test]
    fn the_answer_stops_at_its_ceiling_and_says_it_did() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = dir.path();
        // More matching lines in one file than a whole search carries.
        std::fs::write(root.join("many.txt"), "needle\n".repeat(HIT_CAP + FILE_LINES_CAP))
            .expect("a file");

        let matcher = pattern("needle", false, true, false).expect("a pattern");
        let (files, tally) = search(root, false, &matcher);
        assert!(files[0].more, "one file's own ceiling is reached first");
        assert_eq!(files[0].lines.len(), FILE_LINES_CAP);
        assert!(!tally.capped, "one file of 200 is nowhere near the search's own ceiling");
    }

    /// A search nobody is waiting for any more stops, and says that is why it ended.
    #[test]
    fn a_search_that_is_called_off_stops() {
        let dir = folder();
        let stop = AtomicBool::new(true);
        let matcher = pattern("needle", false, true, false).expect("a pattern");
        let found = Mutex::new(Vec::new());
        let tally = hunt(dir.path(), false, &matcher, &stop, &|files| {
            found.lock().expect("the found list").extend(files);
        });
        assert!(tally.stopped);
        assert_eq!(tally.hits, 0, "nothing was read at all");
    }

    /// Starting a search in a window is what stops the one that window was already running, and a
    /// message about a search that has been replaced stops nothing.
    #[test]
    fn a_second_search_takes_over_from_the_first() {
        let searches = FolderSearches::default();
        let first = searches.begin("main", 1);
        let second = searches.begin("main", 2);
        assert!(first.load(Ordering::Relaxed), "the first was called off by the second");
        assert!(!second.load(Ordering::Relaxed));

        searches.end("main", 1);
        assert!(!second.load(Ordering::Relaxed), "that message was about the search before this one");
        searches.end("main", 2);
        assert!(second.load(Ordering::Relaxed));

        // Another window's search is its own.
        let other = searches.begin("talk", 1);
        assert!(!other.load(Ordering::Relaxed));
    }
}
