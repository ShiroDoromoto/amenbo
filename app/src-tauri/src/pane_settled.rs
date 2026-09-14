//! Carrying back what a pane settled in a file it was handed a copy of, so the copy is not where the
//! answer stops (`AMB-D-878`).
//!
//! **One file is a copy rather than a link, and only on Windows** ([`crate::pane_home`]). Codex
//! replaces `config.toml` — a temporary file and a rename — and the link Windows can make is a hard
//! one, which the rename leaves behind. A copy survives the replacing and carries what the reader
//! chose into the pane; what it gives up is the other direction, and this is that direction put
//! back.
//!
//! **Only the keys the pane is meant to settle are carried, and nothing else.** A pane writes a
//! folder's `trust_level` into the same file, and an `mcp add` typed in one writes a server there —
//! neither is the reader's to inherit from a pane they opened for something else, and carrying the
//! whole file back would carry both. The named keys are copied over and the rest of the copy is
//! dropped with it when the pane goes.
//!
//! **The reader's own file is read afresh and two lines of it are changed.** It is the shape Codex
//! itself writes in (`AMB-T-4835` watched it): comments, ordering, `[mcp_servers.*]` and every other
//! key survive, and what is replaced is replaced atomically, so nobody reading the file sees half of
//! one. What it costs is the one case it cannot tell apart: a reader who edited the same key by hand
//! while a pane was open has that edit overwritten when the pane settles the key.
//!
//! **Nothing is carried where the reader has no file of their own.** macOS and Linux share this file
//! by a link, and a link is only made for a name the reader already has — so a pane opened by
//! somebody with no `config.toml` writes into its own home there, and this does the same by not
//! writing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// How long the thread waits before looking up to ask whether the pane it watches is still there.
/// Nothing arrives any later for it: it is how long the thread outlives the pane, not how long a
/// change takes to be seen.
const HEARTBEAT: Duration = Duration::from_millis(500);

/// How long to wait for quiet before reading the copy. One replacement fires several events (the
/// temporary file, the rename, the directory), and reading between them would read a file that is
/// about to be replaced again.
const SETTLE: Duration = Duration::from_millis(150);

/// What a wake-up carries, which is nothing: a watcher says no more than "something moved".
type Wake = ();

/// The panes being watched, each with the flag that stops its threads. Keyed by the pane's home,
/// which is what [`crate::pane_home::forget`] is handed when the pane goes.
fn watching() -> &'static Mutex<BTreeMap<PathBuf, Vec<Arc<AtomicBool>>>> {
    static WATCHING: OnceLock<Mutex<BTreeMap<PathBuf, Vec<Arc<AtomicBool>>>>> = OnceLock::new();
    WATCHING.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Watch what this pane settles in the files it was handed copies of, and carry the named keys back
/// to the reader's own.
///
/// **Called on every open**, straight after the copies are taken, because what it holds is the
/// values the copy was taken with: a thread left running from the last open would be comparing
/// against what the reader had then. An open that comes round again stops the old threads first.
///
/// `carried` is the catalog's, not this module's: which file a pane is given a copy of, and which of
/// its keys are the pane's to settle, are answered where the copying is declared.
pub fn watch(
    home: &Path,
    into: &Path,
    theirs: &Path,
    carried: &'static [(&'static str, &'static [&'static str])],
) {
    forget(home);
    for (name, keys) in carried {
        let copy = into.join(name);
        let own = theirs.join(name);
        // Read here rather than on the thread: what this holds is the values the copy was taken
        // with, and a pane that settles a key before the thread has started would otherwise have it
        // read back as what was copied and carried nowhere.
        let held = settled(&text(&copy), keys);
        let stop = Arc::new(AtomicBool::new(false));
        if let Ok(mut watching) = watching().lock() {
            watching.entry(home.to_path_buf()).or_default().push(Arc::clone(&stop));
        }
        std::thread::spawn(move || run(&copy, &own, keys, held, &stop));
    }
}

/// Stop watching this pane's home. Answering for a home nothing was started for is a no-op, which is
/// every pane on every operating system that shares the file by a link.
pub fn forget(home: &Path) {
    let Ok(mut watching) = watching().lock() else { return };
    for stop in watching.remove(home).unwrap_or_default() {
        stop.store(true, Ordering::Relaxed);
    }
}

/// The thread behind one copied file: hold what it was copied with, and carry back what the pane
/// changes it to, until the flag says the pane has gone.
fn run(
    copy: &Path,
    own: &Path,
    keys: &'static [&'static str],
    mut held: BTreeMap<&'static str, String>,
    stop: &AtomicBool,
) {
    let Some(dir) = copy.parent() else { return };
    let (tx, rx) = std::sync::mpsc::channel::<Wake>();
    // `_watcher` has to be held: dropping it takes the watch down.
    let Some(_watcher) = crate::store_watch::spawn_store_watcher(dir, tx) else {
        log::warn!("what a pane settles in {} stays there: no watch", copy.display());
        return;
    };
    // `held` is what the copy was taken with. Anything the file says other than that, the pane put
    // there — and a write that lands between the copy and the watch being installed is read on the
    // first turn of the loop, because the first thing this does is look rather than wait.
    let mut look = true;
    while !stop.load(Ordering::Relaxed) {
        if !look {
            match woke(&rx, stop) {
                Woke::Nothing => continue,
                Woke::Gone => return,
                Woke::Moved => {}
            }
        }
        look = false;
        let now = settled(&text(copy), keys);
        if now == held || now.is_empty() {
            continue;
        }
        held = now.clone();
        if let Err(e) = carry(own, &now) {
            log::warn!("what a pane settled did not reach {}: {e}", own.display());
        }
    }
}

/// What one turn of the loop found.
enum Woke {
    /// Something moved in the directory the copy is in.
    Moved,
    /// The heartbeat came round with nothing in it.
    Nothing,
    /// The watcher is gone, so nothing will arrive again.
    Gone,
}

/// Wait for a wake-up, and let the burst a single replacement fires settle into one.
fn woke(rx: &Receiver<Wake>, stop: &AtomicBool) -> Woke {
    match rx.recv_timeout(HEARTBEAT) {
        Ok(()) => {
            while !stop.load(Ordering::Relaxed) && rx.recv_timeout(SETTLE).is_ok() {}
            Woke::Moved
        }
        Err(RecvTimeoutError::Timeout) => Woke::Nothing,
        Err(RecvTimeoutError::Disconnected) => Woke::Gone,
    }
}

/// What a file says, or nothing at all where it cannot be read — which is a file the pane has not
/// been given and a file that has gone alike, and neither is something to carry.
fn text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// The values `keys` are given in the table this file opens with, each as it is written — the text
/// after the `=`, quotes and all.
///
/// **The value is carried rather than read.** What is wanted is the reader's file saying what the
/// pane's says, and the two spellings of one TOML string are two spellings: reading one and writing
/// the other would rewrite a value nobody asked to have rewritten.
///
/// **The first table header ends it.** A key under `[projects."…"]` has a different meaning from the
/// same key at the top, and a value spelled across more than one line is not one of these: Codex
/// writes both of them on a line (`AMB-T-4835`).
fn settled(text: &str, keys: &'static [&'static str]) -> BTreeMap<&'static str, String> {
    let mut found = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim_start();
        if line.starts_with('[') {
            break;
        }
        let Some((name, value)) = line.split_once('=') else { continue };
        let Some(key) = keys.iter().find(|key| **key == name.trim()) else { continue };
        found.entry(*key).or_insert_with(|| value.trim().to_string());
    }
    found
}

/// Put `values` into the reader's own file, leaving the rest of it as it is.
///
/// The file is read here rather than held from earlier, so an edit the reader made while the pane
/// was open is still in what gets written back. What replaces it is written beside it and renamed
/// over it, which is one step for anybody reading: a reader whose Codex starts while this is going
/// on reads the old file or the new one and never a half-written one.
fn carry(own: &Path, values: &BTreeMap<&'static str, String>) -> std::io::Result<()> {
    let text = std::fs::read_to_string(own)?;
    let written = rewritten(&text, values);
    if written == text {
        return Ok(());
    }
    let tmp = beside(own);
    std::fs::write(&tmp, written)?;
    std::fs::rename(&tmp, own).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// A name for the half-written file, beside the one it is about to become. It carries this process
/// and a count of its own, because two panes settling at once are two of these.
fn beside(own: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mine = NEXT.fetch_add(1, Ordering::Relaxed);
    let name = own.file_name().unwrap_or_default().to_string_lossy().into_owned();
    own.with_file_name(format!("{name}.amenbo-{}-{mine}", std::process::id()))
}

/// The file's text with `values` written into the table it opens with — each key replaced where it
/// is already there, and put in before the first table header where it is not.
///
/// Everything else is the text that came in, down to the line endings: a file written on Windows is
/// read by Codex on Windows, and a rewrite that quietly changed every line of it would be this
/// module making a change nobody asked for in a file it was let into for two lines.
fn rewritten(text: &str, values: &BTreeMap<&'static str, String>) -> String {
    let end = if text.contains("\r\n") { "\r" } else { "" };
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let mut left = values.clone();
    // Where the table this file opens with ends. Everything from here down belongs to a table of its
    // own, where these keys mean something else.
    let header = lines.iter().position(|line| line.trim_start().starts_with('['));
    for i in 0..header.unwrap_or(lines.len()) {
        let body = lines[i].strip_suffix('\r').unwrap_or(&lines[i]).to_string();
        let Some((name, _)) = body.split_once('=') else { continue };
        let name = name.trim().to_string();
        let Some(value) = left.remove(name.as_str()) else { continue };
        let end = if lines[i].ends_with('\r') { "\r" } else { "" };
        lines[i] = format!("{name} = {value}{end}");
    }
    if !left.is_empty() {
        // A key the reader has none of goes in at the end of the opening table, which is the first
        // table header — or, in a file that has none, after the last line with anything on it.
        let at = header.unwrap_or_else(|| {
            lines.iter().rposition(|line| !line.trim().is_empty()).map_or(0, |last| last + 1)
        });
        for (key, value) in left.iter().rev() {
            lines.insert(at, format!("{key} = {value}{end}"));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What Codex settles on a `/model` press, which is the one row this is for (`AMB-T-4835`).
    const KEYS: &[&str] = &["model", "model_reasoning_effort"];

    /// A file of the shape a reader's own is: a comment, the keys, and a table of their own things
    /// under it.
    const THEIRS: &str = concat!(
        "# what I keep here\n",
        "model = \"gpt-5.5\"\n",
        "model_reasoning_effort = \"medium\"\n",
        "model_verbosity = \"high\"\n",
        "\n",
        "[mcp_servers.demo]\n",
        "command = \"demo\"\n",
        "model = \"not this one\"\n",
    );

    #[test]
    fn what_the_opening_table_says_is_read_and_the_tables_under_it_are_not() {
        let found = settled(THEIRS, KEYS);

        assert_eq!(found["model"], "\"gpt-5.5\"");
        assert_eq!(found["model_reasoning_effort"], "\"medium\"");
        // The same key under a table of its own is a different key, and a file with nothing in the
        // opening table says nothing here.
        assert!(settled("[mcp_servers.demo]\nmodel = \"theirs\"\n", KEYS).is_empty());
        // A commented-out line is not a value, however much it looks like one.
        assert!(settled("# model = \"gpt-5.5\"\n", KEYS).is_empty());
    }

    #[test]
    fn the_value_is_carried_as_it_was_written() {
        // Not read and written again: `'single'` and `"double"` are one TOML string in two
        // spellings, and rewriting one as the other is a change nobody asked for.
        let found = settled("model = 'gpt-5.6-luna'  # the one I chose\n", KEYS);

        assert_eq!(found["model"], "'gpt-5.6-luna'  # the one I chose");
    }

    #[test]
    fn only_the_named_lines_move_and_the_rest_of_the_file_is_what_it_was() {
        let settled = settled("model = \"gpt-5.6-luna\"\nmodel_reasoning_effort = \"high\"\n", KEYS);

        let written = rewritten(THEIRS, &settled);

        assert_eq!(
            written,
            concat!(
                "# what I keep here\n",
                "model = \"gpt-5.6-luna\"\n",
                "model_reasoning_effort = \"high\"\n",
                "model_verbosity = \"high\"\n",
                "\n",
                "[mcp_servers.demo]\n",
                "command = \"demo\"\n",
                "model = \"not this one\"\n",
            )
        );
    }

    #[test]
    fn a_key_the_reader_has_none_of_goes_in_above_the_first_table() {
        let settled = settled("model = \"gpt-5.6-luna\"\n", KEYS);

        let written = rewritten("# mine\n\n[mcp_servers.demo]\ncommand = \"demo\"\n", &settled);

        assert_eq!(
            written,
            "# mine\n\nmodel = \"gpt-5.6-luna\"\n[mcp_servers.demo]\ncommand = \"demo\"\n"
        );
        // And in a file with no table at all, after the last line that has anything on it.
        assert_eq!(rewritten("# mine\n\n", &settled), "# mine\nmodel = \"gpt-5.6-luna\"\n\n");
    }

    #[test]
    fn a_file_written_on_windows_stays_written_that_way() {
        let settled = settled("model = \"gpt-5.6-luna\"\nmodel_reasoning_effort = \"high\"\n", KEYS);

        let written = rewritten("# mine\r\nmodel = \"gpt-5.5\"\r\n", &settled);

        assert_eq!(
            written,
            "# mine\r\nmodel = \"gpt-5.6-luna\"\r\nmodel_reasoning_effort = \"high\"\r\n"
        );
    }

    #[test]
    fn nothing_is_carried_where_the_reader_has_no_file_of_their_own() {
        let dir = amenbo_scratch::scratch("settled-none");
        let settled = settled("model = \"gpt-5.6-luna\"\n", KEYS);

        assert!(carry(&dir.join("config.toml"), &settled).is_err());
        assert!(!dir.join("config.toml").exists());
    }

    #[test]
    fn what_is_written_is_put_in_place_whole() {
        let dir = amenbo_scratch::scratch("settled-carry");
        let own = dir.join("config.toml");
        std::fs::write(&own, THEIRS).unwrap();
        let settled = settled("model = \"gpt-5.6-luna\"\n", KEYS);

        carry(&own, &settled).unwrap();

        assert!(std::fs::read_to_string(&own).unwrap().contains("model = \"gpt-5.6-luna\""));
        // Nothing of the half-written file is left beside it.
        let left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, ["config.toml"]);
    }

    /// The whole of it, as a pane runs it: the copy is taken, the pane settles a key in the copy,
    /// and the reader's own file says it.
    ///
    /// It is run here rather than only on the operating system that copies, because what decides
    /// whether a pane copies is the catalog's ([`crate::pane_home`]) and what this module does with
    /// the answer is the same everywhere.
    #[test]
    fn what_the_pane_settles_reaches_the_readers_own_file() {
        let home = amenbo_scratch::scratch("settled-home");
        let theirs = amenbo_scratch::scratch("settled-theirs");
        std::fs::write(theirs.join("config.toml"), THEIRS).unwrap();
        std::fs::write(home.join("config.toml"), THEIRS).unwrap();
        const CARRIED: &[(&str, &[&str])] = &[("config.toml", &["model", "model_reasoning_effort"])];

        watch(&home, &home, &theirs, CARRIED);
        // The pane's own write, the way Codex makes it: a file beside it and a rename over.
        let next = THEIRS.replace("gpt-5.5", "gpt-5.6-luna");
        std::fs::write(home.join("config.toml.new"), &next).unwrap();
        std::fs::rename(home.join("config.toml.new"), home.join("config.toml")).unwrap();

        assert!(
            waits_for(&theirs.join("config.toml"), "model = \"gpt-5.6-luna\""),
            "{}",
            std::fs::read_to_string(theirs.join("config.toml")).unwrap()
        );
        // What the pane wrote under a table of its own is the pane's, and is not carried.
        let own = std::fs::read_to_string(theirs.join("config.toml")).unwrap();
        assert!(own.contains("model = \"not this one\""), "{own}");
        forget(&home);
    }

    /// A pane whose home is gone stops being watched, so nothing it left behind is carried into the
    /// reader's file afterwards.
    #[test]
    fn a_pane_that_has_gone_carries_nothing_more() {
        let home = amenbo_scratch::scratch("settled-gone-home");
        let theirs = amenbo_scratch::scratch("settled-gone-theirs");
        std::fs::write(theirs.join("config.toml"), THEIRS).unwrap();
        std::fs::write(home.join("config.toml"), THEIRS).unwrap();
        const CARRIED: &[(&str, &[&str])] = &[("config.toml", &["model"])];
        watch(&home, &home, &theirs, CARRIED);

        forget(&home);
        std::fs::write(home.join("config.toml"), THEIRS.replace("gpt-5.5", "gpt-5.6-luna")).unwrap();

        assert!(!waits_for(&theirs.join("config.toml"), "gpt-5.6-luna"));
    }

    /// Wait for a file to say something, for as long as a kernel watch can reasonably take, and
    /// answer whether it ever did.
    fn waits_for(path: &Path, says: &str) -> bool {
        let until = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < until {
            if std::fs::read_to_string(path).is_ok_and(|text| text.contains(says)) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }
}
