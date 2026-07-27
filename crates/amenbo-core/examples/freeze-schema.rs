//! Freeze the store's current shape: `cargo run -p amenbo-core --example freeze-schema`
//! (or `make schema-freeze`). With `--renumber` (`make schema-renumber`), move a step that landed
//! on a version number another branch had already taken, and freeze the number it moves to.
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
//!
//! **Renumbering.** Two branches that each append a step both write the next number, and whichever
//! merges second is holding a number that is taken. What the merge leaves behind is a chain that no
//! longer ascends — the one thing `migrate::is_well_formed` refuses — and a step whose number has to
//! move in more places than the one that is obvious. `--renumber` moves the trailing steps back into
//! ascending order and then freezes the number the last one lands on, so the chain, the frozen
//! file, its `include_str!` arm and the doc table all say the same thing in one run.
//!
//! It renumbers from the compiled chain and edits the source that produced it, so it checks the
//! two agree before writing a byte: the `to:` fields it finds in `STEPS` must read, in order,
//! exactly what this build's `STEPS` holds. Numbering is all it moves — `schema_sql()` is the
//! registry, which a renumber does not touch, so the shape written for the new number is the
//! shape this build already emits.
//!
//! What it does not touch is the step's own tests: which of them name a version, and which name it
//! as the version *before* the step, is not something a text edit can tell. It names the steps that
//! moved and leaves those to their author.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use amenbo_core::store_engine::migrate::{LATEST_VERSION, STEPS};
use amenbo_core::store_engine::schema::schema_sql;

/// The boilerplate the newest row of the doc table carries while its version is the current one.
const NEWEST_ROW_NOTE: &str = "held equal to the live registry by the test below";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let renumber = match args.as_slice() {
        [] => false,
        [flag] if flag == "--renumber" => true,
        _ => {
            eprintln!("freeze-schema: usage: freeze-schema [--renumber]");
            std::process::exit(2);
        }
    };
    // Not `main() -> Result`: the refusals here run to several lines, and Rust reports a returned
    // error through `Debug`, which prints the escapes rather than the lines.
    if let Err(why) = run(renumber) {
        eprintln!("freeze-schema: {why}");
        std::process::exit(1);
    }
}

fn run(renumber: bool) -> Result<(), String> {
    let engine = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/store_engine");
    let version = if renumber { renumber_chain(&engine)? } else { LATEST_VERSION };
    freeze(&engine, version)
}

/// Moves the steps that a merge left out of order onto free numbers, and hands back the version the
/// chain now ends at.
///
/// The first step that does not rise above the one before it is where the collision landed, and
/// everything from there on is pushed onto the next free number in turn — so the chain comes out
/// contiguous rather than merely ascending, and the steps before the collision keep the numbers
/// stores in the wild are already stamped with.
fn renumber_chain(engine: &Path) -> Result<i64, String> {
    let old: Vec<i64> = STEPS.iter().map(|s| s.to).collect();
    let Some(first) = old.windows(2).position(|w| w[0] >= w[1]).map(|i| i + 1) else {
        return Err(format!(
            "the chain already ascends and ends at v{LATEST_VERSION} — no step is sitting on a \
             number another one took, so there is nothing to renumber. If what you owe is the \
             shape of v{LATEST_VERSION}, that is `make schema-freeze`."
        ));
    };

    let mut new = old.clone();
    let mut moved = Vec::new();
    for i in first..new.len() {
        new[i] = new[i - 1] + 1;
        if new[i] != old[i] {
            moved.push((old[i], new[i], STEPS[i].name));
        }
    }

    let source = engine.join("migrate.rs");
    let text = std::fs::read_to_string(&source).map_err(|e| format!("read {}: {e}", source.display()))?;
    let text = rewrite_step_numbers(&text, &old, &new)?;
    std::fs::write(&source, text).map_err(|e| format!("write {}: {e}", source.display()))?;

    println!("renumbered in {}:", source.display());
    for (from, to, name) in &moved {
        println!("  v{from} -> v{to}  {name}");
    }
    println!(
        "the tests belonging to {} step(s) above still name the old version — a step's own test \
         names the version it starts from as well as the one it reaches, so read both.",
        moved.len()
    );
    Ok(*new.last().expect("a chain with a collision has at least two steps"))
}

/// Rewrites the `to:` field of every step in the `STEPS` literal to `new`, in order.
///
/// The edit is refused unless the numbers already in the text read exactly `old` — this program is
/// compiled from the file it is editing, so that check is what says the build and the source on
/// disk are the same thing. Without it a stale build would renumber against a chain that is no
/// longer there.
fn rewrite_step_numbers(text: &str, old: &[i64], new: &[i64]) -> Result<String, String> {
    const FIELD: &str = "\n        to: ";
    let (start, end) = steps_span(text)?;
    let block = &text[start..end];

    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..start]);
    let (mut cursor, mut step) = (0usize, 0usize);
    while let Some(found_at) = block[cursor..].find(FIELD) {
        let at = cursor + found_at + FIELD.len();
        let stop = at + block[at..].find(',').ok_or_else(|| "a step's `to:` has no comma".to_string())?;
        let found: i64 = block[at..stop]
            .trim()
            .parse()
            .map_err(|_| format!("`to: {}` in STEPS is not a version number", &block[at..stop]))?;
        let expected = *old
            .get(step)
            .ok_or_else(|| "STEPS holds more `to:` fields than this build's chain has steps".to_string())?;
        if found != expected {
            return Err(format!(
                "step {} of STEPS reads v{found}, but this build's chain has v{expected} there — \
                 the source on disk is not what this program was compiled from",
                step + 1
            ));
        }
        out.push_str(&block[cursor..at]);
        let _ = write!(out, "{}", new[step]);
        cursor = stop;
        step += 1;
    }
    if step != old.len() {
        return Err(format!(
            "STEPS holds {step} `to:` field(s), this build's chain has {} — the source on disk is \
             not what this program was compiled from",
            old.len()
        ));
    }
    out.push_str(&block[cursor..]);
    out.push_str(&text[end..]);
    Ok(out)
}

/// The byte range of the `STEPS` literal's contents, which is the only region a renumber may write
/// in: `to:` is a field name the tests below this literal use too, and those numbers are fixtures.
fn steps_span(text: &str) -> Result<(usize, usize), String> {
    const OPENS: &str = "pub const STEPS: &[Step] = &[\n";
    const CLOSES: &str = "\n];\n";
    let start = text.find(OPENS).ok_or_else(|| format!("migrate.rs has no `{}`", OPENS.trim()))? + OPENS.len();
    let end = start
        + text[start..]
            .find(CLOSES)
            .ok_or_else(|| "the STEPS literal is not closed".to_string())?;
    Ok((start, end))
}

fn freeze(engine: &Path, version: i64) -> Result<(), String> {
    let sql = schema_sql();
    let file = engine.join(format!("schema_frozen/v{version}.sql"));

    match std::fs::read_to_string(&file) {
        Ok(existing) if existing == sql => {
            println!("v{version} is already frozen, and matches what this build emits.");
            return Ok(());
        }
        Ok(_) => {
            return Err(format!(
                "v{version} is already frozen as something this build does not emit:\n  {}\n\
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
    let (text, arm) = add_arm(&text, version)?;
    let text = match previous_row_commit(engine, version) {
        Some(sha) => match add_doc_row(&text, version, &sha) {
            Ok(updated) => updated,
            Err(why) => {
                println!("the doc table is yours to finish ({why}):\n{}", doc_row(version));
                text
            }
        },
        None => {
            println!(
                "the doc table is yours to finish (the previous version's commit is not in this \
                 history yet):\n{}",
                doc_row(version)
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
