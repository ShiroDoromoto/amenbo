//! The store's shape at each version of the chain, **frozen** — one file per version under
//! `schema_frozen/`, each holding exactly what [`super::schema::schema_sql`] emitted while that version
//! was the latest (`AMB-D-375`).
//!
//! **Why a file and not a derivation.** "What did a v7 store look like?" used to be answered by taking
//! today's registry and subtracting the columns the chain declares it added — which makes the answer only
//! as complete as the declarations, so a column that reached the registry without a step is invisible to
//! the subtraction *and* to anything built on it. The frozen text is the shape itself, and owes nothing to
//! what the chain says about it. The diffs between these files show why that matters: the column
//! `task.status_changed_at` is in the registry from v4 and gets its step at v6, and half the tables
//! here arrived with no step at all.
//!
//! **Append only.** A frozen version is never rewritten — rewriting one makes a past shape a lie. What a
//! change to the registry owes is a *new* file, and [`tests::the_latest_version_is_frozen`] is what makes
//! that debt come due: it goes red the moment the registry moves ahead of the newest frozen file. Paying
//! it is `make schema-freeze`, which writes the file, adds its arm below, and settles the table's newest
//! row onto the commit that froze it. The arms stay literal text, so freezing a version is still a
//! deliberate act that shows up in the diff.
//!
//! **Not the production genesis.** A new store is still born from the registry
//! ([`super::schema::schema_sql`] is the single source of truth, as `schema.rs` states); these files are
//! read by checks and test fixtures only, which is why the whole module is `cfg(test)` — a shipped binary
//! carries none of this text.
//!
//! **Where each file came from.** Each is the output of `schema_sql()` at the newest commit whose chain
//! ends at that version and whose steps are a prefix of today's (a commit whose chain was renumbered on
//! a branch describes a version that never was, and is skipped):
//!
//! | version | commit | |
//! |---|---|---|
//! | v3 | `620e56b` | the oldest shape this repository holds |
//! | v4 | `de0a774` | |
//! | v5 | `f56a34a` | |
//! | v6 | `2a0d731` | byte-identical to v5 — v6's step carries a column the registry already had |
//! | v7 | `afc5396` | |
//! | v8 | `c71e87a` | |
//! | v9 | `7db6826` | |
//! | v10 | `17e3897` | |
//! | v11 | `c232113` | |
//! | v12 | `6e84a48` | |
//! | v13 | `a96200f` | |
//! | v14 | `9742d7e` | |
//! | v15 | this commit | held equal to the live registry by the test below |
//!
//! [`super::migrate::BASELINE_VERSION`] itself is **not** here: this repository's history begins with the
//! chain already at [`OLDEST_FROZEN_VERSION`], so no build in it ever emitted a v2 store and there is no
//! shape to freeze. The interval below the oldest frozen version is out of scope for shape assertions —
//! what the baseline fixtures still exercise faithfully is the *data* the steps from there work on.

use super::migrate::{BASELINE_VERSION, LATEST_VERSION};

/// The oldest version this module can produce a shape for. Below it lies
/// [`BASELINE_VERSION`] — a version this repository's history never emitted (see the module doc).
pub const OLDEST_FROZEN_VERSION: i64 = 3;

/// A shape below the baseline names no store this build opens, so it would be a file nothing could ask
/// for. Held at compile time — the two constants move for unrelated reasons.
const _: () = assert!(OLDEST_FROZEN_VERSION >= BASELINE_VERSION);

/// The DDL a store carried at `version`, or `None` for a version with no frozen shape — the versions
/// below [`OLDEST_FROZEN_VERSION`], and the next one, until whoever moves the registry freezes it.
///
/// The arms are written out rather than generated: freezing a version is a deliberate act, and the
/// missing arm is what the check below names.
pub fn frozen(version: i64) -> Option<&'static str> {
    Some(match version {
        3 => include_str!("schema_frozen/v3.sql"),
        4 => include_str!("schema_frozen/v4.sql"),
        5 => include_str!("schema_frozen/v5.sql"),
        6 => include_str!("schema_frozen/v6.sql"),
        7 => include_str!("schema_frozen/v7.sql"),
        8 => include_str!("schema_frozen/v8.sql"),
        9 => include_str!("schema_frozen/v9.sql"),
        10 => include_str!("schema_frozen/v10.sql"),
        11 => include_str!("schema_frozen/v11.sql"),
        12 => include_str!("schema_frozen/v12.sql"),
        13 => include_str!("schema_frozen/v13.sql"),
        14 => include_str!("schema_frozen/v14.sql"),
        15 => include_str!("schema_frozen/v15.sql"),
        _ => return None,
    })
}

/// The DDL a store carried at `version`, for a caller that has no answer if it is missing.
#[track_caller]
pub fn frozen_or_panic(version: i64) -> &'static str {
    frozen(version).unwrap_or_else(|| {
        panic!(
            "v{version} has no frozen DDL. Versions below v{OLDEST_FROZEN_VERSION} were never emitted \
             by this repository's history; a version above it is one whose shape is waiting for \
             `make schema-freeze`"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The freeze debt, made due.** The newest frozen shape must be exactly what the registry emits
    /// today: touch the registry and this goes red until the version has been bumped and its shape
    /// written down. Without it the freezing stops silently the first time someone forgets, and every
    /// file here starts drifting from what it claims to describe.
    #[test]
    fn the_latest_version_is_frozen() {
        let frozen = frozen_or_panic(LATEST_VERSION);
        let live = super::super::schema::schema_sql();
        if frozen == live {
            return;
        }
        // Not `assert_eq!`: these are two whole schemas, and printing both leaves the one line that
        // actually moved buried in forty kilobytes. Name that line instead.
        let (at, was, now) = frozen
            .lines()
            .zip(live.lines())
            .enumerate()
            .find(|(_, (a, b))| a != b)
            .map_or((frozen.lines().count(), "<end of file>", "<more lines>"), |(i, (a, b))| (i + 1, a, b));
        panic!(
            "v{LATEST_VERSION} is frozen as something the registry no longer emits.\n\
             first divergence at line {at}:\n  frozen:   {was}\n  registry: {now}\n\
             If the registry moved on purpose, append a step (which bumps the version) and run \
             `make schema-freeze`. A frozen file is never edited in place — rewriting one makes a \
             past shape a lie."
        );
    }

    /// No gaps: every version from the oldest frozen one to the latest has a shape, so a fixture may ask
    /// for any version the chain passes through and a step is never tested against a shape two versions
    /// away from the one it runs on.
    #[test]
    fn every_version_the_chain_passes_through_has_a_shape() {
        for v in OLDEST_FROZEN_VERSION..=LATEST_VERSION {
            assert!(frozen(v).is_some(), "v{v} has no frozen DDL");
        }
    }

    /// Every frozen shape is a shape SQLite accepts — a file that does not apply is a record of nothing,
    /// and the fixtures built on it would fail far from the cause.
    #[test]
    fn every_frozen_shape_applies() {
        for v in OLDEST_FROZEN_VERSION..=LATEST_VERSION {
            let conn = rusqlite::Connection::open_in_memory().unwrap();
            conn.execute_batch(frozen_or_panic(v)).unwrap_or_else(|e| panic!("v{v}: {e}"));
        }
    }
}


