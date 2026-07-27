//! Freeze the store's current shape: `cargo run -p amenbo-core --example freeze-schema`
//! (or `make schema-freeze`).
//!
//! Appending a step to the migration chain bumps `LATEST_VERSION`, and the freeze check
//! (`store_engine::schema_frozen::tests::the_latest_version_is_frozen`) goes red until that
//! version's shape is written down. Paying that debt is mechanical — take what `schema_sql()`
//! emits, put it in a file, and name the file in one `include_str!` arm — but the only thing in
//! the tree that can produce the text is a build of this crate, so it takes a program of its own
//! rather than a command someone can type.
//!
//! It runs beside the code it writes, in the language whose constants decide the answer:
//! `LATEST_VERSION` and `schema_sql()` are both read here directly, so there is no second place
//! that has to be told which version is current.
//!
//! **It never rewrites a frozen file.** A past shape is a record, and a file that disagrees with
//! what this build emits is reported rather than replaced — that disagreement means the registry
//! moved without a step, which is the accident the freeze check exists to catch.
//!
//! The `include_str!` arm is written into the source as literal text, not derived from a directory
//! listing: freezing a version stays a deliberate act that shows up in the author's diff. What is
//! removed here is the typing, not the review.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use amenbo_core::store_engine::migrate::LATEST_VERSION;
use amenbo_core::store_engine::schema::schema_sql;

/// The boilerplate the newest row of the doc table carries while its version is the current one.
const NEWEST_ROW_NOTE: &str = "held equal to the live registry by the test below";

fn main() {
    // Not `main() -> Result`: the refusals here run to several lines, and Rust reports a returned
    // error through `Debug`, which prints the escapes rather than the lines.
    if let Err(why) = freeze() {
        eprintln!("freeze-schema: {why}");
        std::process::exit(1);
    }
}

fn freeze() -> Result<(), String> {
    let engine = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/store_engine");
    let sql = schema_sql();
    let file = engine.join(format!("schema_frozen/v{LATEST_VERSION}.sql"));

    match std::fs::read_to_string(&file) {
        Ok(existing) if existing == sql => {
            println!("v{LATEST_VERSION} is already frozen, and matches what this build emits.");
            return Ok(());
        }
        Ok(_) => {
            return Err(format!(
                "v{LATEST_VERSION} is already frozen as something this build does not emit:\n  {}\n\
                 A frozen file is never rewritten — rewriting one makes a past shape a lie. Either \
                 the registry moved without a step (append one, which bumps the version, and run \
                 this again), or this checkout is behind the file.",
                file.display()
            ));
        }
        Err(_) => {}
    }

    std::fs::write(&file, &sql).map_err(|e| format!("write {}: {e}", file.display()))?;
    println!("wrote {}", file.display());

    let source = engine.join("schema_frozen.rs");
    let text = std::fs::read_to_string(&source).map_err(|e| format!("read {}: {e}", source.display()))?;
    let (text, arm) = add_arm(&text, LATEST_VERSION)?;
    let text = match previous_row_commit(&engine, LATEST_VERSION) {
        Some(sha) => match add_doc_row(&text, LATEST_VERSION, &sha) {
            Ok(updated) => updated,
            Err(why) => {
                println!("the doc table is yours to finish ({why}):\n{}", doc_row(LATEST_VERSION));
                text
            }
        },
        None => {
            println!(
                "the doc table is yours to finish (the previous version's commit is not in this \
                 history yet):\n{}",
                doc_row(LATEST_VERSION)
            );
            text
        }
    };
    std::fs::write(&source, text).map_err(|e| format!("write {}: {e}", source.display()))?;
    println!("added to {}:\n{arm}", source.display());
    println!("`cargo test -p amenbo-core schema_frozen` is what says it took.");
    Ok(())
}

/// Inserts the `include_str!` arm for `version` ahead of the catch-all, and hands back the line it
/// wrote so the caller can show what changed. An arm that is already there is an error rather than
/// a second copy: two arms for one version compile, and the second is dead.
fn add_arm(text: &str, version: i64) -> Result<(String, String), String> {
    let arm = format!("        {version} => include_str!(\"schema_frozen/v{version}.sql\"),\n");
    if text.contains(arm.trim()) {
        return Err(format!("schema_frozen.rs already has an arm for v{version}"));
    }
    let catch_all = "        _ => return None,\n";
    let at = text
        .find(catch_all)
        .ok_or_else(|| format!("schema_frozen.rs has no `{}` to insert ahead of", catch_all.trim()))?;
    let mut out = String::with_capacity(text.len() + arm.len());
    out.push_str(&text[..at]);
    out.push_str(&arm);
    out.push_str(&text[at..]);
    Ok((out, arm.trim_end().to_string()))
}

/// The doc table's row for `version`, while it is the current one.
fn doc_row(version: i64) -> String {
    format!("//! | v{version} | this commit | {NEWEST_ROW_NOTE} |")
}

/// Settles the row the previous version holds — which says "this commit", true only while that
/// version was the newest — onto the commit that actually froze it, and appends the row for
/// `version` in its place.
///
/// The rewrite is refused unless the previous row is still the boilerplate this program wrote:
/// a row someone has annotated by hand (v6 carries one) is prose, and prose is not this
/// program's to overwrite.
fn add_doc_row(text: &str, version: i64, previous_sha: &str) -> Result<String, String> {
    let previous = format!("//! | v{} | this commit | {NEWEST_ROW_NOTE} |\n", version - 1);
    if !text.contains(&previous) {
        return Err(format!("v{}'s row is not the one this program writes", version - 1));
    }
    let mut settled = String::new();
    let _ = write!(settled, "//! | v{} | `{previous_sha}` | |\n{}\n", version - 1, doc_row(version));
    Ok(text.replace(&previous, &settled))
}

/// The commit that introduced the frozen file for `version - 1`, which is what its doc row should
/// name once it is no longer the current version. `None` when git cannot answer — an unreleased
/// checkout, a file not committed yet — and the caller then leaves the table to its author.
fn previous_row_commit(engine: &Path, version: i64) -> Option<String> {
    let file = engine.join(format!("schema_frozen/v{}.sql", version - 1));
    let out = Command::new("git")
        .arg("-C")
        .arg(engine)
        .args(["log", "--diff-filter=A", "-1", "--format=%h", "--"])
        .arg(&file)
        .output()
        .ok()?;
    let sha = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!sha.is_empty()).then_some(sha)
}
