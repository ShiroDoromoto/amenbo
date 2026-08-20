//! `amenbo lint`: catch an Amenbo ref in text on its way out of this store.
//!
//! An id Amenbo renders means something only to someone holding this store. Anywhere else — a commit
//! message, a source comment, a PR body, and after a release the public record — `AMB-T-<n>` is a
//! reference into nothing: noise the next reader cannot resolve and cannot act on. This module is the
//! reader that stops one at the door.
//!
//! **The spelling is the whole judgement.** `AMB-` makes a ref self-declaring ([`crate::idref`]), so a hit
//! is decided by the text alone: no store is opened, no id is resolved, nothing is checked for existence.
//! That is what lets the answer be identical in a working copy, in CI where there is no store to find, and
//! over any text at all — the "degraded without a store" caveat simply does not arise. It is also why a
//! dangling `AMB-T-<n>` is a hit like any other: what leaks is the amenbo-shaped reference, and whether
//! the number is live has no bearing on the reader who cannot follow it.
//!
//! **What it does not do.** It reads and reports; it never edits (there is no `--fix`).
//! And it does not touch a bare `#<n>`: that is a GitHub issue, and a `T-<n>` may be another tracker's —
//! claiming either would hijack a reference that was never ours.
//!
//! The scanners here are pure functions over text ([`scan_text`] / [`scan_diff`]); [`staged_diff`] is the
//! one place that shells out, and it only fetches the text the others read.

use std::path::Path;

use serde::Serialize;

use crate::error::Msg;
use crate::idref::RefKind;
use crate::{Error, Result};

/// One leaked ref, located where the reader can go and remove it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hit {
    /// The text's name: a file path (for a staged diff, the path on the **new** side).
    pub path: String,
    /// The 1-based line the ref sits on. For a staged diff this is the line number in the new file, not an
    /// offset into the diff, so the report names a place that exists in the checkout.
    pub line: usize,
    /// The ref exactly as it was written, case included.
    #[serde(rename = "ref")]
    pub reference: String,
}

/// Is this byte part of a word? A ref has to stand on its own: `AMB-T-<n>` is one, the tail of
/// `FOO_AMB-T-<n>` is not.
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Does `hay` start with `needle`, folding ASCII case? A ref renders uppercase, but a leak is a leak
/// however it was typed, so reading stays as loose here as it is in [`crate::idref::strip`].
fn starts_ci(hay: &[u8], needle: &[u8]) -> bool {
    hay.len() >= needle.len() && hay[..needle.len()].eq_ignore_ascii_case(needle)
}

/// Find the refs on one line, in the order they appear.
///
/// A hit is `<namespace>-<kind code>-<digits>`, bounded by non-word bytes on both sides. The bounds are
/// what keep the pattern honest: `AMB-T-<n>` is ours, while `AMB-T-<n>abc` and `xAMB-T-<n>` are some other
/// string that happens to contain those bytes.
///
/// It allocates nothing and reads each byte a bounded number of times: every added line of every staged
/// diff comes through here.
pub fn refs_in_line(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        // The left bound. A multi-byte char's bytes are all >= 0x80, so they are never word bytes and a ref
        // right after one still stands on its own.
        if i > 0 && is_word_byte(bytes[i - 1]) {
            i += 1;
            continue;
        }
        match ref_end(bytes, i) {
            // `i` is on an ASCII `A`/`a` and `end` is past an ASCII digit, so both are char boundaries.
            Some(end) => {
                out.push(&line[i..end]);
                i = end;
            }
            None => i += 1,
        }
    }
    out
}

/// Where the ref starting at `i` ends, or `None` if none starts there.
///
/// The kinds come from the renderer's own list rather than being written out again, so a kind added there
/// is caught here without anyone remembering to: the lint looks for exactly what Amenbo can spell. Trying
/// each in turn cannot let one shadow another, because the `-` after the code must match too — `AMB-DIM-<n>`
/// can never be read as `AMB-D` with junk after it, whatever order the kinds come in.
fn ref_end(bytes: &[u8], i: usize) -> Option<usize> {
    let namespace = crate::idref::NAMESPACE.as_bytes();
    if !starts_ci(&bytes[i..], namespace) || bytes.get(i + namespace.len()) != Some(&b'-') {
        return None;
    }
    let codes_at = i + namespace.len() + 1;
    for kind in RefKind::ALL {
        let code = kind.code().as_bytes();
        if !starts_ci(&bytes[codes_at..], code) || bytes.get(codes_at + code.len()) != Some(&b'-') {
            continue;
        }
        let mut k = codes_at + code.len() + 1;
        let digits_at = k;
        while matches!(bytes.get(k), Some(b) if b.is_ascii_digit()) {
            k += 1;
        }
        if k == digits_at {
            continue; // `AMB-T-` with no number is not a ref
        }
        // The right bound.
        if matches!(bytes.get(k), Some(&b) if is_word_byte(b)) {
            continue;
        }
        return Some(k);
    }
    None
}

/// Scan plain text — a commit message, a file, anything piped in — reporting hits against `path` as the
/// text's name.
pub fn scan_text(path: &str, text: &str) -> Vec<Hit> {
    text.lines()
        .enumerate()
        .flat_map(|(n, line)| hits_on(path, n + 1, line))
        .collect()
}

fn hits_on(path: &str, line_no: usize, line: &str) -> Vec<Hit> {
    refs_in_line(line)
        .into_iter()
        .map(|r| Hit { path: path.to_string(), line: line_no, reference: r.to_string() })
        .collect()
}

/// Scan a unified diff, reporting only what it **adds** — at the line number the added line takes in the
/// new file.
///
/// Only the added side is scanned because only it is being introduced: a ref sitting in an untouched line,
/// or in one being deleted, is not what this commit is leaking, and reporting it would make every commit
/// near old text fail for something the author is not writing.
///
/// The hunk header's own counts say where the hunk ends, rather than the leading `+` of a body line. A diff
/// is text, so an added line can itself begin with `+++ b/…` or `@@ …`; counting the body out is what tells
/// the two apart. A malformed hunk stops the count and the rest of the diff is read as headers — it under-
/// reports rather than misreports.
pub fn scan_diff(diff: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    let mut path: Option<String> = None;
    let mut new_line = 0usize;
    let (mut left_old, mut left_new) = (0usize, 0usize);

    for raw in diff.lines() {
        if left_old == 0 && left_new == 0 {
            if let Some(rest) = raw.strip_prefix("+++ ") {
                path = new_side_path(rest);
            } else if let Some(hunk) = hunk_header(raw) {
                new_line = hunk.new_start;
                left_old = hunk.old_count;
                left_new = hunk.new_count;
            }
            // Everything else here is `diff --git` / `index` / `--- a/…` / "Binary files … differ" — a
            // binary file has no hunks, so its bytes are never scanned.
            continue;
        }
        match raw.as_bytes().first() {
            Some(b'+') => {
                if let Some(p) = &path {
                    hits.extend(hits_on(p, new_line, &raw[1..]));
                }
                new_line += 1;
                left_new = left_new.saturating_sub(1);
            }
            Some(b'-') => left_old = left_old.saturating_sub(1),
            // "\ No newline at end of file" annotates the line before it and is not a line of its own.
            Some(b'\\') => {}
            // Context: on both sides. An empty line (`None`) is a context line git wrote without its space.
            _ => {
                new_line += 1;
                left_new = left_new.saturating_sub(1);
                left_old = left_old.saturating_sub(1);
            }
        }
    }
    hits
}

/// The file a `+++ ` header names, or `None` when there is no new side (`/dev/null` — a deletion adds
/// nothing to scan).
fn new_side_path(rest: &str) -> Option<String> {
    // Some diff formats append a tab and a timestamp; git does not, but dropping it costs one line.
    let p = rest.split('\t').next().unwrap_or(rest);
    if p == "/dev/null" {
        return None;
    }
    Some(p.strip_prefix("b/").unwrap_or(p).to_string())
}

/// A hunk header's new-side start and the line counts that say how long its body is.
struct Hunk {
    new_start: usize,
    old_count: usize,
    new_count: usize,
}

/// Read `@@ -<old> +<new> @@ …`.
fn hunk_header(line: &str) -> Option<Hunk> {
    let rest = line.strip_prefix("@@ ")?;
    let (ranges, _) = rest.split_once(" @@")?;
    let (old, new) = ranges.split_once(' ')?;
    let (_, old_count) = parse_range(old.strip_prefix('-')?)?;
    let (new_start, new_count) = parse_range(new.strip_prefix('+')?)?;
    Some(Hunk { new_start, old_count, new_count })
}

/// `<start>,<count>`, or a bare `<start>` — which unified diff writes when the count is 1.
fn parse_range(s: &str) -> Option<(usize, usize)> {
    match s.split_once(',') {
        Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
        None => Some((s.parse().ok()?, 1)),
    }
}

/// The staged diff of the repository at `dir` — what `git commit` is about to record.
///
/// `-U0` asks for no context: only added lines are scanned, so context is text fetched to be skipped.
/// `--no-ext-diff` / `--no-color` keep a user's own diff configuration out of what we parse.
///
/// Outside a repository this says so, in one line. It has to be asked separately (`rev-parse`), because
/// `git diff --cached` does not fail there in any useful way: with no repository to read an index from,
/// git falls back to `--no-index` and answers with its entire usage text.
pub fn staged_diff(dir: &Path) -> Result<String> {
    if !is_git_repo(dir) {
        return Err(Error::Invalid(Msg::new(
            "Not a git repository, so there is no staged diff to lint. Pass a file, or --stdin, to lint that text instead.",
        )));
    }
    let out = crate::sys::command("git")
        .current_dir(dir)
        .args([
            // Leave a non-ASCII path as its bytes rather than as `\346\227\245` escapes, so a hit in one
            // is reported at a path the reader can use.
            "-c",
            "core.quotepath=false",
            "diff",
            "--cached",
            "--no-color",
            "--no-ext-diff",
            "-U0",
        ])
        .output()
        .map_err(|e| {
            Error::Invalid(Msg::new(format!("Cannot run git here: {e}")))
        })?;
    if !out.status.success() {
        // git's first line says what went wrong; the rest is usually usage text, and an error message is
        // not the place to reprint a manual.
        let stderr = String::from_utf8_lossy(&out.stderr);
        let detail = stderr.lines().next().unwrap_or("no detail").trim().to_string();
        return Err(Error::Invalid(Msg::new(format!("`git diff --cached` failed: {detail}"))));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Is `dir` inside a git working tree? Asked of git itself rather than by looking for a `.git`, which is a
/// directory in one checkout, a `gitdir:` file in a worktree, and above you in a subdirectory of either.
fn is_git_repo(dir: &Path) -> bool {
    crate::sys::command("git")
        .current_dir(dir)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|o| o.status.success() && o.stdout.starts_with(b"true"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::idref;

    /// The reason the lint exists: everything the renderer can spell, it can catch. Derived from the same
    /// kinds, so a new one is covered without this test being touched.
    #[test]
    fn every_rendered_ref_is_caught() {
        for kind in RefKind::ALL {
            let rendered = idref::render(*kind, 123);
            assert_eq!(
                refs_in_line(&format!("see {rendered} for the rationale")),
                vec![rendered.as_str()],
                "{rendered} is renderable but not caught",
            );
        }
    }

    /// The boundary the namespace buys: another tracker's refs, and a bare issue number, are not ours to
    /// flag.
    #[test]
    fn foreign_refs_and_bare_numbers_are_left_alone() {
        for line in ["fixes #12", "PROJ-123 is Jira's", "T-45 might be anyone's", "D-9 too", "ENG-1"] {
            assert!(refs_in_line(line).is_empty(), "{line} is not an Amenbo ref");
        }
    }

    #[test]
    fn a_ref_must_stand_on_its_own() {
        for line in ["xAMB-T-12", "FOO_AMB-T-12", "AMB-T-12abc", "AMB-T-", "AMB-T", "AMB--12", "AMB-X-1"] {
            assert!(refs_in_line(line).is_empty(), "{line} is not a ref");
        }
        // Punctuation is not part of a word, so the ordinary ways a ref is written all hold.
        for line in ["(AMB-T-12)", "AMB-T-12.", "see AMB-T-12, then", "「AMB-T-12」", "-AMB-T-12"] {
            assert_eq!(refs_in_line(line), vec!["AMB-T-12"], "{line} holds a ref");
        }
    }

    /// A dangling number is a hit: the reader who cannot follow it is exactly who the lint protects.
    #[test]
    fn a_dangling_ref_is_still_a_leak() {
        assert_eq!(refs_in_line("AMB-T-999999"), vec!["AMB-T-999999"]);
    }

    #[test]
    fn case_is_folded_and_a_line_can_hold_several() {
        assert_eq!(refs_in_line("amb-t-1 and AMB-D-2 and Amb-TC-3"), vec!["amb-t-1", "AMB-D-2", "Amb-TC-3"]);
    }

    /// A longer kind code is not read as a shorter one with junk after it.
    #[test]
    fn kind_codes_do_not_shadow_each_other() {
        assert_eq!(refs_in_line("AMB-DIMV-3"), vec!["AMB-DIMV-3"]);
        assert_eq!(refs_in_line("AMB-DIM-3"), vec!["AMB-DIM-3"]);
        // The comment codes prefix onto the decision's and the task's (`AMB-D-377`).
        assert_eq!(refs_in_line("AMB-DC-3"), vec!["AMB-DC-3"]);
        assert_eq!(refs_in_line("AMB-TC-3"), vec!["AMB-TC-3"]);
    }

    #[test]
    fn text_is_reported_by_line() {
        let hits = scan_text("COMMIT_EDITMSG", "fix the thing\n\nas decided in AMB-D-5\n");
        assert_eq!(hits, vec![Hit { path: "COMMIT_EDITMSG".into(), line: 3, reference: "AMB-D-5".into() }]);
    }

    const DIFF: &str = "\
diff --git a/src/a.rs b/src/a.rs
index 1111111..2222222 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -9,0 +10,2 @@ fn keep() {
+// done in AMB-T-12
+let x = 1;
@@ -40 +42 @@ fn other() {
-// was AMB-T-99
+// now clean
";

    /// The report names the new file at the line the ref lands on — a place the reader can open.
    #[test]
    fn a_diff_is_reported_at_its_new_file_position() {
        assert_eq!(
            scan_diff(DIFF),
            vec![Hit { path: "src/a.rs".into(), line: 10, reference: "AMB-T-12".into() }],
        );
    }

    /// Only what the commit adds. The `AMB-T-99` above is on its way *out*, and flagging it would fail a
    /// commit for text its author is removing.
    #[test]
    fn a_removed_ref_is_not_a_leak() {
        assert!(scan_diff(DIFF).iter().all(|h| h.reference != "AMB-T-99"));
    }

    /// Context lines are neither added nor scanned, but they do move the new-side line number along.
    #[test]
    fn context_lines_advance_the_line_number_without_being_scanned() {
        let diff = "\
--- a/f
+++ b/f
@@ -1,3 +1,4 @@
 ctx AMB-T-1
 ctx
+added AMB-T-2
 ctx
";
        assert_eq!(scan_diff(diff), vec![Hit { path: "f".into(), line: 3, reference: "AMB-T-2".into() }]);
    }

    /// A diff of a diff: an added line that *looks* like a header is body text, and the hunk's own counts
    /// are what say so.
    #[test]
    fn an_added_line_shaped_like_a_header_is_scanned_as_content() {
        let diff = "\
--- a/patch.txt
+++ b/patch.txt
@@ -0,0 +1,2 @@
++++ b/other AMB-T-7
+@@ -1 +1 @@ AMB-T-8
";
        assert_eq!(
            scan_diff(diff),
            vec![
                Hit { path: "patch.txt".into(), line: 1, reference: "AMB-T-7".into() },
                Hit { path: "patch.txt".into(), line: 2, reference: "AMB-T-8".into() },
            ],
        );
    }

    /// A deleted file has no new side, so there is nothing to name and nothing to scan.
    #[test]
    fn a_deletion_reports_nothing() {
        let diff = "\
--- a/gone.rs
+++ /dev/null
@@ -1 +0,0 @@
-// AMB-T-3
";
        assert!(scan_diff(diff).is_empty());
    }

    #[test]
    fn clean_text_and_an_empty_diff_report_nothing() {
        assert!(scan_text("f", "no refs here\nfixes #12\n").is_empty());
        assert!(scan_diff("").is_empty());
    }

    proptest::proptest! {
        /// The scanner walks bytes and slices the line back at the indices it lands on, so a char boundary
        /// it got wrong would panic on text that is not ASCII. It is pointed at whatever a user is
        /// committing, in whatever language — arbitrary text must come back as an answer, never a crash.
        #[test]
        fn scanning_arbitrary_text_never_panics(text: String) {
            let _ = refs_in_line(&text);
            let _ = scan_text("f", &text);
            let _ = scan_diff(&text);
        }

        /// The lint's promise, over text nobody wrote by hand: a rendered ref standing on its own is
        /// found, whatever surrounds it and whatever its number.
        #[test]
        fn a_rendered_ref_is_found_whatever_surrounds_it(
            before in "[^0-9A-Za-z_]*",
            after in "[^0-9A-Za-z_]*",
            id in 0i64..i64::MAX,
        ) {
            for kind in RefKind::ALL {
                let rendered = idref::render(*kind, id);
                let line = format!("{before}{rendered}{after}");
                proptest::prop_assert!(
                    refs_in_line(&line).contains(&rendered.as_str()),
                    "{rendered} was not found in {line:?}",
                );
            }
        }
    }
}
