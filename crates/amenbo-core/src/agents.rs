//! Safely injects Amenbo's generic guidance (Class A) into the user's own `CLAUDE.md` / `AGENTS.md`.
//! Amenbo owns **only what sits between the markers** `<!-- amenbo:begin (managed vN) -->` …
//! `<!-- amenbo:end -->`. The begin marker carries a **format version** (an unversioned `(managed)`
//! counts as version 1); detection, replacement and strip all go through
//! [prefix match + version extraction](find_begin_marker) rather than an exact string compare, so
//! older markers are still recognised. Everything outside the markers (the project's own Class P
//! content) is left alone. upsert is idempotent: no file → create it with just the block; markers
//! present → swap the block only; an existing file without markers → append (the user's content is
//! preserved). **Thin-pointer invariant**: the block stays a language directive plus a pointer at
//! `amenbo agent --json`, and never duplicates the command spec — the single source of truth is
//! `agent --json` inside the binary. The thinner the block, the less its content changes between
//! versions, which structurally minimises the collision between a binary update and the stale
//! blocks sitting in the user's filesystem (no rot, no commit-diff churn). Holding this invariant
//! is what makes the versioning and backward compatibility above worth having.

/// Format version of the begin marker. An unversioned `(managed)` counts as version 1; the newest
/// version this binary writes is 4 (`(managed v4)`). Bumping the version is how a binary update
/// whose block template changed can tell an existing folder's block is out of date: run Amenbo in
/// that folder and [`follow_stale_block`] brings it up to the current version by itself, while the
/// folders you have not been in are [detected](stale_bound_blocks) by `doctor` and
/// [fixed](resync_bound_blocks) by `sync-guide`.
pub const MANAGED_BLOCK_VERSION: u32 = 4;

/// Version-independent stable prefix of the begin marker (both `(managed)` and `(managed vN)`
/// start with it).
const BEGIN_MARKER_PREFIX: &str = "<!-- amenbo:begin (managed";
/// Close of the begin marker. The version token (` vN`, or nothing when unversioned) sits between
/// the prefix and this close.
const BEGIN_MARKER_SUFFIX: &str = ") -->";

/// The begin marker at the version **this binary writes** (paired with [`MANAGED_BLOCK_VERSION`]).
/// Detection goes through [`find_begin_marker`]'s prefix match rather than an exact compare, so an
/// unversioned `(managed)` is still recognised.
pub const BEGIN_MARKER: &str = "<!-- amenbo:begin (managed v4) -->";
pub const END_MARKER: &str = "<!-- amenbo:end -->";

/// Finds the first managed begin marker in `text`, **whatever version it carries**. Returns
/// `(offset of the marker, offset just past it, format version)`: `1` for an unversioned
/// `(managed)`, `N` for `(managed vN)` (and `1` if the token does not parse — stay conservative).
/// Matching on prefix + close catches both the old and the new shape; this is the core of the
/// backward compatibility.
fn find_begin_marker(text: &str) -> Option<(usize, usize, u32)> {
    let start = text.find(BEGIN_MARKER_PREFIX)?;
    let after_prefix = start + BEGIN_MARKER_PREFIX.len();
    let close = after_prefix + text[after_prefix..].find(BEGIN_MARKER_SUFFIX)?;
    let end = close + BEGIN_MARKER_SUFFIX.len();
    let token = text[after_prefix..close].trim(); // "" (unversioned) or "vN"
    let version = if token.is_empty() {
        1
    } else {
        token.strip_prefix('v').and_then(|n| n.parse::<u32>().ok()).unwrap_or(1)
    };
    Some((start, end, version))
}

/// The **format version** of the managed block in `text` (`None` when there is none): `1` for an
/// unversioned `(managed)`, `N` for `(managed vN)` (and `1` if it does not parse). The single entry
/// point for telling a stale block from a current one ([`MANAGED_BLOCK_VERSION`]): `doctor` uses it
/// to **detect without rewriting**, and [`follow_stale_block`] uses it to bring **just that folder**
/// up to the current version.
pub fn managed_block_version(text: &str) -> Option<u32> {
    find_begin_marker(text).map(|(_, _, v)| v)
}

/// The one instruction the managed block carries, on a single line: run `agent --json` before acting,
/// and declare the facet. `cmd` is the launch command name, as in [`managed_block_body`].
///
/// It is a function of its own because the block is no longer the only place it is said — a session-start
/// hook injects this same line ([`crate::harness`]), which is the whole of what such a hook is for. Two
/// copies of a sentence drift; this one has a test holding it to the block's wording, so rewording the
/// block moves what the hooks inject with it.
pub fn launch_instruction(cmd: &str) -> String {
    format!(
        "Before you act on any request in this directory, you MUST first run \
         `{cmd} agent --json` and follow it — it is the single source of truth for how to work in \
         this folder. Always pass `--actor ai`."
    )
}

/// Body of the managed block (without the markers): English-based, plus the directive naming the
/// language to talk to the human in. `cmd` is the launch command name
/// ([`crate::config::Paths::command_name`] — `amenbo` in production, `amenbo-dev` on the dev
/// channel) and is used **only where a command is invoked** — the product name, `.amenbo` and the
/// flags Amenbo takes are fixed and channel-independent. `language_label` is what
/// `config::language_label` returns (e.g. `Japanese`).
pub fn managed_block_body(language_label: &str, cmd: &str) -> String {
    format!(
        "# Amenbo — read this before you work in this directory\n\
         \n\
         The user set up Amenbo here. Before you act on any request in this directory,\n\
         you MUST first run `{cmd} agent --json` and follow it — it is the single source\n\
         of truth for how to work in this folder. Always pass `--actor ai`.\n\
         \n\
         **Communicate with the human, and write task titles, notes, and comments, in {language_label}.**\n\
         \n\
         (Managed block — do not edit between the markers; content outside them is preserved.)"
    )
}

/// Wraps a body in the markers (body between them, no trailing newline).
pub fn wrap(body: &str) -> String {
    format!("{BEGIN_MARKER}\n{body}\n{END_MARKER}")
}

/// Idempotently upserts the managed block into an existing file's content (`None` = the file does
/// not exist) and returns the whole new file. `body` is the block body, without the markers.
pub fn upsert_managed(existing: Option<&str>, body: &str) -> String {
    let block = wrap(body);
    let Some(text) = existing else {
        // No file → create it with just the block (one trailing newline).
        return format!("{block}\n");
    };
    match (find_begin_marker(text), text.find(END_MARKER)) {
        (Some((begin, _, _)), Some(end_start)) if end_start >= begin => {
            // Markers present → swap the block only; everything outside them is untouched.
            let end = end_start + END_MARKER.len();
            let mut out = String::with_capacity(text.len());
            out.push_str(&text[..begin]);
            out.push_str(&block);
            out.push_str(&text[end..]);
            out
        }
        _ => {
            // An existing file with no markers → append the block, one blank line apart, keeping
            // the user's content.
            format!("{}\n\n{block}\n", text.trim_end())
        }
    }
}

/// Whether `dir` already holds a `CLAUDE.md` or `AGENTS.md` carrying an Amenbo managed block.
/// The clobber guard for init/bind: an existing managed block means the folder may already be set
/// up for another project, so do not walk over it unasked.
pub fn dir_has_managed_block(dir: &std::path::Path) -> bool {
    ["CLAUDE.md", "AGENTS.md"].iter().any(|name| {
        std::fs::read_to_string(dir.join(name))
            .map(|t| find_begin_marker(&t).is_some())
            .unwrap_or(false)
    })
}

/// Pulls the **language label** out of an existing text's managed block, so a rewrite can keep it.
/// It reads the `... comments, in <Label>.**` line near the end of the block — a shape
/// [`managed_block_body`] owns, and a round-trip test keeps the extractor from drifting away from
/// it. `None` when there are no markers, or the language cannot be read.
pub fn extract_managed_language(text: &str) -> Option<String> {
    let (begin, _, _) = find_begin_marker(text)?;
    let end = text[begin..].find(END_MARKER).map(|e| begin + e)?;
    let block = &text[begin..end];
    const NEEDLE: &str = "comments, in ";
    let i = block.rfind(NEEDLE)? + NEEDLE.len();
    let rest = &block[i..];
    let j = rest.find(".**").or_else(|| rest.find('.'))?;
    let label = rest[..j].trim();
    (!label.is_empty()).then(|| label.to_string())
}

/// Idempotently upserts the managed block into **both** `AGENTS.md` and `CLAUDE.md` under `dir`,
/// owning only what sits between the markers and preserving the user's Class P content. `lang_code`
/// is the store's `config.language` (`None` = unset) and `cmd` the launch command name. Returns
/// only the files whose content actually changed (created or updated) — an unchanged file is not
/// written, so mtimes and commit diffs do not move for nothing. Every face that writes the block
/// goes through here: the CLI (init / bind / config set language), the GUI (bind_folder /
/// config_set_language), the resync ([`resync_bound_blocks`]), and the follow-on-startup path
/// ([`follow_stale_block`]). Do not grow a second write path. The language is settled **once per
/// folder**: an explicit setting (`Some`) wins; **when unset
/// (`None`), the language of whichever managed block the folder already has (AGENTS.md or
/// CLAUDE.md) is kept**; failing that, the English default. Settling it per folder — rather than
/// per file — is what makes a missing sibling **inherit the other file's language** so the two stay
/// in step: in a Japanese store where only CLAUDE.md carries a block, the AGENTS.md we regenerate
/// must not come out in English. That closes both the churn of a language-less store flattening an
/// existing Japanese block to English, and a language mismatch between the two files in one folder.
pub fn upsert_into_dir(dir: &std::path::Path, lang_code: Option<&str>, cmd: &str) -> Vec<&'static str> {
    // Settle the folder's language first (shared by both siblings); with None, take it from
    // whichever block the folder already has.
    let label = lang_code
        .map(crate::config::language_label)
        .or_else(|| {
            ["AGENTS.md", "CLAUDE.md"].iter().find_map(|name| {
                std::fs::read_to_string(dir.join(name)).ok().as_deref().and_then(extract_managed_language)
            })
        })
        .unwrap_or_else(|| "English".to_string());
    let body = managed_block_body(&label, cmd);
    let mut touched = Vec::new();
    for name in ["AGENTS.md", "CLAUDE.md"] {
        let path = dir.join(name);
        let existing = std::fs::read_to_string(&path).ok();
        let updated = upsert_managed(existing.as_deref(), &body);
        if existing.as_deref() != Some(updated.as_str()) && std::fs::write(&path, &updated).is_ok() {
            touched.push(name);
        }
    }
    touched
}

/// Whether `dir`'s managed block is **stale** (either `CLAUDE.md` or `AGENTS.md` carries a version
/// below the current [`MANAGED_BLOCK_VERSION`]). A file with no block is never called stale —
/// anything Amenbo does not own is out of scope.
fn has_stale_block(dir: &std::path::Path) -> bool {
    ["CLAUDE.md", "AGENTS.md"].iter().any(|name| {
        std::fs::read_to_string(dir.join(name))
            .ok()
            .and_then(|text| managed_block_version(&text))
            .is_some_and(|version| version < MANAGED_BLOCK_VERSION)
    })
}

/// **Follow on startup**: when the one resolved bound folder's managed block is stale, quietly
/// bring it up to the current version. Called from [`crate::binding::resolve_upward`] (the only
/// path that resolves a `.amenbo`), so stale guidance is gone the moment you run Amenbo in that
/// folder. Returns the files actually rewritten (empty when there was nothing to do). It touches
/// **that folder only**; the other folders are detected by [`stale_bound_blocks`]
/// (`doctor`) and fixed by [`resync_bound_blocks`] (`sync-guide` and the GUI) — there is no second
/// scan path. It writes **only when the block is stale**: at the current version [`has_stale_block`]
/// is false and [`upsert_into_dir`] is not even called, so mtimes and commit diffs do not move for
/// nothing. The language label is kept from the existing block (`lang_code = None`). **A file it
/// cannot write does not kill the command**: [`upsert_into_dir`] simply leaves it out of `touched`,
/// so Amenbo still runs on a read-only filesystem.
pub fn follow_stale_block(dir: &std::path::Path, cmd: &str) -> Vec<&'static str> {
    if !has_stale_block(dir) {
        return Vec::new();
    }
    upsert_into_dir(dir, None, cmd)
}

/// One stale managed block found in a bound folder (its version is below the current
/// [`MANAGED_BLOCK_VERSION`]). Shared detection data: `doctor` (CLI) and the GUI's resync entry
/// point both go through this one scan, so there is no second implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleBlock {
    /// Absolute path of the bound folder.
    pub dir: String,
    /// The file carrying the stale block (`"CLAUDE.md"` / `"AGENTS.md"`).
    pub file: &'static str,
    /// Format version of that file's managed block.
    pub version: u32,
}

/// Outcome of `resync_bound_blocks`. `scanned` counts the folders that still exist (the ones we
/// walked); `updated` lists the `(dir, file)` pairs whose managed block was actually rewritten
/// (only where the content changed).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResyncReport {
    /// How many folders existed and were scanned (moved or renamed folders are not counted).
    pub scanned: usize,
    /// The `(folder, file)` pairs actually rewritten to the current version.
    pub updated: Vec<(String, &'static str)>,
}

/// Scans the `CLAUDE.md` / `AGENTS.md` of every folder in `registry` and lists the managed blocks
/// whose version is below the current one ([`MANAGED_BLOCK_VERSION`]). It **never rewrites**
/// anything (no side effects), and quietly skips files that are gone, unreadable, or carry no
/// managed block. This is the one detection path shared by `doctor`'s `stale_managed_block` check
/// and the GUI's resync — there is no duplicate scan. It takes a `Registry` so it stays independent
/// of the persistence backend (the caller reads one with [`crate::store::Store::bindings`]).
pub fn stale_bound_blocks(registry: &crate::binding::Registry) -> Vec<StaleBlock> {
    let mut stale = Vec::new();
    for dir in registry.all_dirs() {
        for name in ["CLAUDE.md", "AGENTS.md"] {
            let path = std::path::Path::new(&dir).join(name);
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue; // Missing or unreadable → out of scope.
            };
            let Some(version) = managed_block_version(&text) else {
                continue; // A file with no managed block is not ours to touch.
            };
            if version < MANAGED_BLOCK_VERSION {
                stale.push(StaleBlock { dir: dir.clone(), file: name, version });
            }
        }
    }
    stale
}

/// Resyncs the managed block to the current version, across every folder in `registry`
/// (`dir = None`) or in just one folder (`Some`). Each folder goes through [`upsert_into_dir`]: a
/// file is written only when its content changed (low churn), and each folder's own language label
/// is kept rather than downgraded. `cmd` is the launch command name
/// ([`crate::config::Paths::command_name`] — `amenbo` in production, `amenbo-dev` on the dev
/// channel, task instances included: they read their own app-data but install no CLI of their own).
/// Moved or renamed folders are skipped. This is the one resync path shared by the CLI's
/// `sync-guide` and the GUI. It takes a `Registry` so it stays independent of the persistence
/// backend (the caller reads one from the root store).
pub fn resync_bound_blocks(registry: &crate::binding::Registry, dir: Option<&str>, cmd: &str) -> ResyncReport {
    let target_dirs: Vec<String> = match dir {
        Some(d) => vec![d.to_string()],
        None => registry.all_dirs(),
    };
    let mut report = ResyncReport::default();
    for d in &target_dirs {
        let path = std::path::Path::new(d);
        if !path.is_dir() {
            continue; // Moved or renamed away → out of scope (doctor surfaces it as stale).
        }
        report.scanned += 1;
        // lang_code = None: keep each folder's existing block language. A missing sibling inherits
        // the other file's language, and only a folder with no block at all is created in English
        // (upsert_into_dir settles the language per folder).
        for f in upsert_into_dir(path, None, cmd) {
            report.updated.push((d.clone(), f));
        }
    }
    report
}

/// Returns the text with the managed block removed — the inverse of upsert, for `unbind`. Only what
/// sits between the markers is dropped; the user's content outside them (Class P) is preserved.
/// `None` when there are no markers (nothing changes). When removing the block leaves nothing but
/// whitespace behind, it returns `Some(String::new())`, so the caller can delete the file outright.
pub fn strip_managed(text: &str) -> Option<String> {
    let (begin, _, _) = find_begin_marker(text)?;
    let end_start = text.find(END_MARKER)?;
    if end_start < begin {
        return None;
    }
    let end = end_start + END_MARKER.len();
    let before = text[..begin].trim_end();
    let after = text[end..].trim_start();
    let joined = match (before.is_empty(), after.is_empty()) {
        (true, true) => String::new(),
        (false, true) => before.to_string(),
        (true, false) => after.to_string(),
        // What sat either side of the block is rejoined one blank line apart, keeping the break.
        (false, false) => format!("{before}\n\n{after}"),
    };
    Some(if joined.is_empty() {
        String::new()
    } else {
        format!("{joined}\n")
    })
}

/// Removes Amenbo's managed block from `AGENTS.md` and `CLAUDE.md` under `dir` — the inverse of
/// [`upsert_into_dir`], for `unbind`. The user's content outside the markers is preserved, and a
/// file left empty once the block is gone is deleted: an AGENTS.md that bind/init had created with
/// just the block disappears here, putting the folder back the way it was before it was bound.
/// Returns only the files actually changed (updated or deleted); a file with no managed block is
/// left alone.
pub fn remove_from_dir(dir: &std::path::Path) -> Vec<&'static str> {
    let mut touched = Vec::new();
    for name in ["AGENTS.md", "CLAUDE.md"] {
        let path = dir.join(name);
        let Ok(existing) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(stripped) = strip_managed(&existing) else {
            continue; // A file with no managed block is not ours to touch.
        };
        let ok = if stripped.is_empty() {
            std::fs::remove_file(&path).is_ok()
        } else {
            std::fs::write(&path, &stripped).is_ok()
        };
        if ok {
            touched.push(name);
        }
    }
    touched
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> String {
        managed_block_body("Japanese", "amenbo")
    }

    /// The block wraps the instruction over three lines; a hook injects it as one. Comparing them with
    /// the whitespace flattened is what makes them one sentence with two renderings rather than two
    /// sentences that happen to agree today.
    #[test]
    fn the_block_and_the_hooks_say_the_same_thing() {
        fn flat(text: &str) -> String {
            text.split_whitespace().collect::<Vec<_>>().join(" ")
        }
        assert!(
            flat(&body()).contains(&flat(&launch_instruction("amenbo"))),
            "the block no longer carries the instruction the hooks inject:\n{}",
            body()
        );
    }

    #[test]
    fn creates_block_when_file_absent() {
        let out = upsert_managed(None, &body());
        assert!(out.starts_with(BEGIN_MARKER));
        assert!(out.trim_end().ends_with(END_MARKER));
        assert!(out.contains("in Japanese."));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn replaces_only_the_managed_block_and_preserves_user_content() {
        let original = format!(
            "# My project\n\nHand-written Class P instructions.\n\n{}\n\n## More user notes\nkeep me.\n",
            wrap(&managed_block_body("Japanese", "amenbo"))
        );
        // Re-upsert with a body in another language.
        let updated = upsert_managed(Some(&original), &managed_block_body("English", "amenbo"));
        // The user's content, outside the markers, is unchanged.
        assert!(updated.contains("# My project"));
        assert!(updated.contains("Hand-written Class P instructions."));
        assert!(updated.contains("## More user notes"));
        assert!(updated.contains("keep me."));
        // Only the language inside the block is swapped.
        assert!(updated.contains("in English."));
        assert!(!updated.contains("in Japanese."));
        // Exactly one pair of markers — no duplicates.
        assert_eq!(updated.matches(BEGIN_MARKER).count(), 1);
        assert_eq!(updated.matches(END_MARKER).count(), 1);
    }

    #[test]
    fn appends_block_to_existing_file_without_markers() {
        let original = "# Existing CLAUDE.md\n\nProject-specific prompt only.\n";
        let out = upsert_managed(Some(original), &body());
        // The existing content is preserved and the block is appended.
        assert!(out.contains("# Existing CLAUDE.md"));
        assert!(out.contains("Project-specific prompt only."));
        assert!(out.contains(BEGIN_MARKER));
        assert!(out.contains(END_MARKER));
        // A blank line separates the existing content from the block.
        assert!(out.contains("Project-specific prompt only.\n\n<!-- amenbo:begin"));
    }

    #[test]
    fn upsert_is_idempotent() {
        let once = upsert_managed(None, &body());
        let twice = upsert_managed(Some(&once), &body());
        assert_eq!(once, twice, "re-applying the same block changes nothing");
    }

    #[test]
    fn managed_block_version_reads_current_legacy_and_absent() {
        // A block at the current version (v2).
        let current = upsert_managed(None, &body());
        assert_eq!(managed_block_version(&current), Some(MANAGED_BLOCK_VERSION));
        // The old unversioned `(managed)` counts as version 1 (backward compatibility).
        let legacy = current.replace(BEGIN_MARKER, "<!-- amenbo:begin (managed) -->");
        assert_eq!(managed_block_version(&legacy), Some(1));
        // Text with no marker is None.
        assert_eq!(managed_block_version("# just a project file\n"), None);
    }

    #[test]
    fn upsert_into_dir_writes_both_files_and_is_idempotent() {
        let dir = amenbo_scratch::scratch("agents-test");
        // Put a hand-written (Class P) CLAUDE.md there: the block must be appended to it and the
        // content preserved.
        std::fs::write(dir.join("CLAUDE.md"), "# Project rules\n\nhand-written.\n").unwrap();

        let touched = upsert_into_dir(&dir, Some("ja"), "amenbo");
        assert!(touched.contains(&"AGENTS.md") && touched.contains(&"CLAUDE.md"), "both touched: {touched:?}");
        let claude = std::fs::read_to_string(dir.join("CLAUDE.md")).unwrap();
        assert!(claude.contains("hand-written."), "Class P preserved");
        assert!(claude.contains(BEGIN_MARKER));
        assert!(std::fs::read_to_string(dir.join("AGENTS.md")).unwrap().contains("in Japanese."));

        // Run again with the same language → nothing changes (touched is empty).
        let again = upsert_into_dir(&dir, Some("ja"), "amenbo");
        assert!(again.is_empty(), "no-op when unchanged: {again:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strip_managed_removes_block_and_preserves_user_content() {
        // The Class P content outside the markers survives; only the block is dropped (upsert's
        // inverse).
        let original = format!(
            "# My project\n\nHand-written Class P instructions.\n\n{}\n\n## More user notes\nkeep me.\n",
            wrap(&body())
        );
        let out = strip_managed(&original).expect("has a managed block");
        assert!(out.contains("# My project"));
        assert!(out.contains("Hand-written Class P instructions."));
        assert!(out.contains("## More user notes"));
        assert!(out.contains("keep me."));
        // The block is gone — neither the markers nor the body remain.
        assert!(!out.contains(BEGIN_MARKER));
        assert!(!out.contains(END_MARKER));
        assert!(!out.contains("read this before you work"));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn strip_managed_block_only_file_becomes_empty() {
        // A file bind/init created with just the block strips to empty — the caller's cue to delete
        // it.
        let only_block = upsert_managed(None, &body());
        assert_eq!(strip_managed(&only_block).as_deref(), Some(""));
        // No marker → None (nothing changes).
        assert_eq!(strip_managed("# hand-written only\n"), None);
    }

    #[test]
    fn remove_from_dir_strips_blocks_and_deletes_pure_block_files() {
        let dir = amenbo_scratch::scratch("agents-remove");
        // AGENTS.md holds nothing but the block (bind created it) → it is deleted. CLAUDE.md carries
        // Class P content → only the block is dropped.
        std::fs::write(dir.join("AGENTS.md"), upsert_managed(None, &body())).unwrap();
        std::fs::write(
            dir.join("CLAUDE.md"),
            upsert_managed(Some("# Project rules\n\nhand-written.\n"), &body()),
        )
        .unwrap();

        let touched = remove_from_dir(&dir);
        assert!(touched.contains(&"AGENTS.md") && touched.contains(&"CLAUDE.md"), "both touched: {touched:?}");
        assert!(!dir.join("AGENTS.md").exists(), "pure-block file is deleted");
        let claude = std::fs::read_to_string(dir.join("CLAUDE.md")).unwrap();
        assert!(claude.contains("hand-written."), "Class P preserved");
        assert!(!claude.contains(BEGIN_MARKER), "managed block removed");

        // Idempotent: nothing is left, so a second call touches nothing.
        assert!(remove_from_dir(&dir).is_empty(), "no-op on a folder with no managed block");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_managed_language_round_trips_the_body() {
        // The extractor is coupled to the shape managed_block_body writes; the round trip catches
        // the two drifting apart.
        // "Simplified Chinese" holds a space, so the extractor must not stop at the first word.
        for label in ["Japanese", "English", "Chinese", "Simplified Chinese"] {
            let wrapped = wrap(&managed_block_body(label, "amenbo"));
            assert_eq!(extract_managed_language(&wrapped).as_deref(), Some(label), "round-trips {label}");
        }
        // No markers → None.
        assert_eq!(extract_managed_language("no markers here"), None);
    }

    /// An old unversioned `(managed)` block, as an earlier binary would have written it — the
    /// baseline for the backward-compatibility tests.
    fn legacy_block() -> String {
        format!(
            "<!-- amenbo:begin (managed) -->\n{}\n<!-- amenbo:end -->",
            managed_block_body("Japanese", "amenbo")
        )
    }

    #[test]
    fn current_begin_marker_matches_declared_version() {
        // Catches the BEGIN_MARKER literal and MANAGED_BLOCK_VERSION drifting apart.
        let (start, end, version) = find_begin_marker(BEGIN_MARKER).expect("current marker parses");
        assert_eq!(start, 0);
        assert_eq!(end, BEGIN_MARKER.len());
        assert_eq!(version, MANAGED_BLOCK_VERSION, "BEGIN_MARKER version must match MANAGED_BLOCK_VERSION");
    }

    #[test]
    fn find_begin_marker_recognizes_legacy_and_versioned() {
        // The old unversioned marker is version 1.
        let (_, _, v_legacy) = find_begin_marker("<!-- amenbo:begin (managed) -->").expect("legacy parses");
        assert_eq!(v_legacy, 1, "unversioned (managed) is treated as version 1");
        // An explicit version reads back as that number.
        let (_, _, v2) = find_begin_marker("<!-- amenbo:begin (managed v2) -->").expect("v2 parses");
        assert_eq!(v2, 2);
        let (_, _, v7) = find_begin_marker("<!-- amenbo:begin (managed v7) -->").expect("v7 parses");
        assert_eq!(v7, 7);
        // No marker → None.
        assert!(find_begin_marker("no marker here").is_none());
    }

    #[test]
    fn upsert_upgrades_a_legacy_unversioned_block_in_place() {
        // Backward compatibility: upserting a file that carries an old `(managed)` block swaps the
        // marker up to the current version, leaves exactly one pair of markers (old and new do not
        // both survive) and preserves the Class P content outside them.
        let original = format!("# My project\n\nClass P.\n\n{}\n\n## notes\nkeep me.\n", legacy_block());
        let updated = upsert_managed(Some(&original), &managed_block_body("English", "amenbo"));
        assert!(updated.contains("# My project") && updated.contains("keep me."), "Class P preserved");
        assert!(updated.contains(BEGIN_MARKER), "upgraded to the current versioned marker");
        assert!(!updated.contains("(managed) -->"), "legacy unversioned marker is gone");
        assert_eq!(updated.matches("amenbo:begin").count(), 1, "exactly one begin marker (no duplicate)");
        assert_eq!(updated.matches(END_MARKER).count(), 1);
        assert!(updated.contains("in English."));
    }

    #[test]
    fn detection_helpers_accept_the_legacy_marker() {
        // extract / strip / dir_has_managed_block all recognise the old unversioned marker.
        let legacy = legacy_block();
        assert_eq!(extract_managed_language(&legacy).as_deref(), Some("Japanese"));
        assert_eq!(strip_managed(&legacy).as_deref(), Some(""), "a legacy block-only file strips to empty");

        let dir = amenbo_scratch::scratch("agents-legacy");
        std::fs::write(dir.join("CLAUDE.md"), format!("# P\n\n{legacy}\n")).unwrap();
        assert!(dir_has_managed_block(&dir), "clobber guard detects a legacy marker");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upsert_into_dir_resyncs_legacy_block_to_current_version_preserving_language() {
        // The behaviour `sync-guide` rests on: upserting a folder that carries an unversioned
        // `(managed)` block with `lang_code = None` (1) raises the marker to the current version,
        // (2) keeps the existing block's language label (None means "not specified — respect what
        // is there"), and (3) changes nothing on a second call (idempotent).
        let dir = amenbo_scratch::scratch("agents-resync");
        // An old unversioned block an earlier binary wrote with a Japanese label — a language that
        // is distinguishable from the English default None would fall back to.
        let legacy_ja = format!(
            "<!-- amenbo:begin (managed) -->\n{}\n<!-- amenbo:end -->",
            managed_block_body("Japanese", "amenbo")
        );
        std::fs::write(dir.join("AGENTS.md"), format!("# Class P\n\n{legacy_ja}\n")).unwrap();
        assert_eq!(managed_block_version(&std::fs::read_to_string(dir.join("AGENTS.md")).unwrap()), Some(1));

        // Resync with no language given → the version goes up and the Japanese label is kept (None
        // must not downgrade it to the English default).
        let touched = upsert_into_dir(&dir, None, "amenbo");
        assert!(touched.contains(&"AGENTS.md"), "the stale block was rewritten: {touched:?}");
        let agents = std::fs::read_to_string(dir.join("AGENTS.md")).unwrap();
        assert_eq!(managed_block_version(&agents), Some(MANAGED_BLOCK_VERSION), "upgraded to current version");
        assert!(agents.contains("in Japanese."), "language label preserved (not downgraded to the English default)");
        assert!(!agents.contains("in English."), "not downgraded to English");
        assert!(agents.contains("# Class P"), "Class P preserved");
        assert_eq!(agents.matches("amenbo:begin").count(), 1, "no duplicate marker");

        // Idempotent: already at the current version, so a second call changes nothing.
        assert!(upsert_into_dir(&dir, None, "amenbo").is_empty(), "no-op once at current version");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn upsert_into_dir_absent_sibling_inherits_folder_language_not_english() {
        // In a folder of a Japanese store where only CLAUDE.md carries a Japanese block and
        // AGENTS.md is absent, a resync with `lang_code = None` regenerates AGENTS.md **in
        // Japanese**, matching its sibling — it must not fall back to English, so the two files in
        // one folder never disagree on the language.
        let dir = amenbo_scratch::scratch("agents-sibling");
        // CLAUDE.md carries a current-version Japanese block; AGENTS.md is left absent.
        std::fs::write(dir.join("CLAUDE.md"), wrap(&managed_block_body("Japanese", "amenbo"))).unwrap();
        assert!(!dir.join("AGENTS.md").exists(), "AGENTS.md starts absent");

        let touched = upsert_into_dir(&dir, None, "amenbo");
        assert!(touched.contains(&"AGENTS.md"), "the absent AGENTS.md was created: {touched:?}");
        let agents = std::fs::read_to_string(dir.join("AGENTS.md")).unwrap();
        assert!(agents.contains("in Japanese."), "AGENTS.md inherits the sibling's Japanese, not the English default");
        assert!(!agents.contains("in English."), "not created in English");
        // CLAUDE.md was already current and Japanese, so it is not touched (idempotent, low churn).
        assert!(!touched.contains(&"CLAUDE.md"), "the already-current CLAUDE.md is untouched: {touched:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_and_resync_bound_blocks_share_the_registry_scan() {
        // The shared path: given a bound folder holding an unversioned block, `stale_bound_blocks`
        // detects it without rewriting, `resync_bound_blocks` raises it to the current version, and
        // the stale list then comes back empty. The GUI and the CLI's doctor/sync-guide both go
        // through these two functions.
        let base = amenbo_scratch::scratch("stale-resync");
        let bound = base.join("repo");
        std::fs::create_dir_all(&bound).unwrap();
        let legacy = format!(
            "# Class P\n\n<!-- amenbo:begin (managed) -->\n{}\n<!-- amenbo:end -->\n",
            managed_block_body("Japanese", "amenbo")
        );
        std::fs::write(bound.join("CLAUDE.md"), &legacy).unwrap();

        // Register the folder so `all_dirs` picks it up. We hand the two functions an in-memory
        // `Registry` to keep the check independent of the persistence backend.
        let mut registry = crate::binding::Registry::default();
        registry.record_project_ref(1, bound.to_string_lossy().to_string());

        // Detection: version 1 counts as stale (only below the current one does), and nothing is
        // rewritten.
        let stale = stale_bound_blocks(&registry);
        assert_eq!(stale.len(), 1, "the one legacy block is detected: {stale:?}");
        assert_eq!(stale[0].file, "CLAUDE.md");
        assert_eq!(stale[0].version, 1);
        assert_eq!(managed_block_version(&std::fs::read_to_string(bound.join("CLAUDE.md")).unwrap()), Some(1), "not rewritten by detection");

        // Resync: walks the folders that exist and brings them current — scanned = 1, with
        // CLAUDE.md in updated.
        let report = resync_bound_blocks(&registry, None, "amenbo");
        assert_eq!(report.scanned, 1);
        assert!(report.updated.iter().any(|(_, f)| *f == "CLAUDE.md"), "CLAUDE.md was rewritten: {:?}", report.updated);
        let after = std::fs::read_to_string(bound.join("CLAUDE.md")).unwrap();
        assert_eq!(managed_block_version(&after), Some(MANAGED_BLOCK_VERSION), "upgraded to current");
        assert!(after.contains("# Class P"), "Class P preserved");
        assert!(after.contains("in Japanese."), "language label preserved");

        // Once followed, nothing is stale and a further resync is a no-op (idempotent).
        assert!(stale_bound_blocks(&registry).is_empty(), "no stale blocks remain");
        assert!(resync_bound_blocks(&registry, None, "amenbo").updated.is_empty(), "idempotent once current");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A throwaway folder for one test, keyed by name so tests running in parallel in the same
    /// process do not collide.
    fn scratch_dir(key: &str) -> std::path::PathBuf {
        let dir = amenbo_scratch::scratch(&format!("agents-{key}"));
        dir
    }

    #[test]
    fn follow_upgrades_a_stale_block_in_place_and_keeps_its_language() {
        // An unversioned block in a bound folder is quietly brought up to the current version the
        // moment Amenbo runs there. The existing block's language label (Japanese) is kept, and the
        // Class P content outside the markers is untouched.
        let dir = scratch_dir("follow-stale");
        let legacy_ja = format!(
            "# Class P\n\n<!-- amenbo:begin (managed) -->\n{}\n<!-- amenbo:end -->\n",
            managed_block_body("Japanese", "amenbo")
        );
        std::fs::write(dir.join("CLAUDE.md"), &legacy_ja).unwrap();

        let touched = follow_stale_block(&dir, "amenbo");
        assert!(touched.contains(&"CLAUDE.md"), "the stale block was followed: {touched:?}");
        let claude = std::fs::read_to_string(dir.join("CLAUDE.md")).unwrap();
        assert_eq!(managed_block_version(&claude), Some(MANAGED_BLOCK_VERSION), "upgraded to the current version");
        assert!(claude.contains("in Japanese."), "language label preserved");
        assert!(claude.contains("# Class P"), "Class P untouched");
        assert_eq!(claude.matches("amenbo:begin").count(), 1, "no duplicate marker");

        // The second call finds it current and does nothing (idempotent).
        assert!(follow_stale_block(&dir, "amenbo").is_empty(), "no-op once at the current version");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn follow_writes_nothing_when_the_block_is_already_current() {
        // Low churn: in a folder already at the current version, not a byte is written and the
        // mtimes do not move.
        let dir = scratch_dir("follow-current");
        std::fs::write(dir.join("CLAUDE.md"), wrap(&body())).unwrap();
        std::fs::write(dir.join("AGENTS.md"), wrap(&body())).unwrap();
        let mtime = |name: &str| std::fs::metadata(dir.join(name)).unwrap().modified().unwrap();
        let (before_claude, before_agents) = (mtime("CLAUDE.md"), mtime("AGENTS.md"));

        assert!(follow_stale_block(&dir, "amenbo").is_empty(), "nothing to follow");
        assert_eq!(mtime("CLAUDE.md"), before_claude, "an up-to-date file is not rewritten");
        assert_eq!(mtime("AGENTS.md"), before_agents, "an up-to-date file is not rewritten");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn follow_leaves_a_folder_with_no_managed_block_alone() {
        // Anything Amenbo does not own is out of scope: a hand-written CLAUDE.md with no block is
        // not rewritten, and the absent AGENTS.md is **not created** — merely running Amenbo does
        // not sprout files in the user's filesystem.
        let dir = scratch_dir("follow-noblock");
        std::fs::write(dir.join("CLAUDE.md"), "# hand-written only\n").unwrap();

        assert!(follow_stale_block(&dir, "amenbo").is_empty(), "no managed block, nothing to follow");
        assert_eq!(std::fs::read_to_string(dir.join("CLAUDE.md")).unwrap(), "# hand-written only\n");
        assert!(!dir.join("AGENTS.md").exists(), "no file is created out of thin air");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn follow_survives_a_file_it_cannot_write() {
        // A failed write must not kill the command — Amenbo still runs on a read-only filesystem:
        // the file it could not write is left out of touched, with no panic and no Err.
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch_dir("follow-readonly");
        let legacy = format!(
            "<!-- amenbo:begin (managed) -->\n{}\n<!-- amenbo:end -->\n",
            managed_block_body("Japanese", "amenbo")
        );
        let path = dir.join("CLAUDE.md");
        std::fs::write(&path, &legacy).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();

        // Stale but unwritable → CLAUDE.md stays out of touched (AGENTS.md is writable, so it does
        // follow).
        let touched = follow_stale_block(&dir, "amenbo");
        assert!(!touched.contains(&"CLAUDE.md"), "the unwritable file is skipped, not failed on: {touched:?}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), legacy, "the unwritable file is left as-is");

        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dir_has_managed_block_detects_existing_markers() {
        let dir = amenbo_scratch::scratch("agents-hasblock");
        assert!(!dir_has_managed_block(&dir), "empty dir has no managed block");
        // A hand-written CLAUDE.md with no markers does not count.
        std::fs::write(dir.join("CLAUDE.md"), "# hand-written only\n").unwrap();
        assert!(!dir_has_managed_block(&dir), "a plain CLAUDE.md is not a managed block");
        // With a managed block, it does.
        std::fs::write(dir.join("CLAUDE.md"), wrap(&managed_block_body("Japanese", "amenbo"))).unwrap();
        assert!(dir_has_managed_block(&dir), "detects the begin marker");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unset_language_preserves_an_existing_block_instead_of_downgrading_to_english() {
        // Updating an existing Japanese block with the language unset (None) keeps it Japanese
        // rather than flattening it to English.
        let dir = amenbo_scratch::scratch("agents-langpreserve");
        // Given: a Japanese managed block, as another store with language ja would have written it.
        let ja = wrap(&managed_block_body("Japanese", "amenbo"));
        std::fs::write(dir.join("CLAUDE.md"), &ja).unwrap();
        std::fs::write(dir.join("AGENTS.md"), &ja).unwrap();

        // Re-upsert from a store with no language set → it stays Japanese (no downgrade to English,
        // and nothing is touched).
        let touched = upsert_into_dir(&dir, None, "amenbo");
        assert!(touched.is_empty(), "unset language must not rewrite an existing ja block to English: {touched:?}");
        let claude = std::fs::read_to_string(dir.join("CLAUDE.md")).unwrap();
        assert!(claude.contains("in Japanese."), "existing language is preserved");
        assert!(!claude.contains("in English."), "not downgraded to English");

        // An explicit en, on the other hand, does swap it — an explicit setting is respected.
        let touched_en = upsert_into_dir(&dir, Some("en"), "amenbo");
        assert!(touched_en.contains(&"CLAUDE.md"), "an explicit language still updates the block");
        assert!(std::fs::read_to_string(dir.join("CLAUDE.md")).unwrap().contains("in English."));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
