//! The store's **version chain**.
//!
//! A store carries one monotonic integer — its format version, stamped in `store_meta`
//! ([`super::META_FORMAT_VERSION`], missing = v0). This module is the chain that moves it: a list of
//! numbered [`Step`]s, each of which takes a store from the version below it to its own, applied
//! forward from whatever the store carries. Nothing else may advance the version.
//!
//! **This is a history, not a diff.** A declarative diff — presence-guarded "drop the column if it is
//! there" calls replayed on every open — can align *structure* but cannot carry *meaning*: it cannot
//! tell a rename from a drop-and-add, or a split of one column into two, so data quietly disappears.
//! Once strangers upgrade from versions we do not know in advance, only the history survives.
//!
//! **A step is not necessarily SQL.** The truth source is one SQLite file, but a store is more than
//! that file: attachment blobs are files under the store directory, and the physical layout itself can
//! change. So a step is given both the transaction and the
//! store directory ([`Ctx`]) and may do either. Most will be one [`Apply::Sql`] batch.
//!
//! **Each step is one transaction, and the version is stamped inside it** — so a step and the version
//! that says it ran commit together, and an interrupted chain resumes at the step that did not finish.
//! The DB half of a step is therefore all-or-nothing; the file half is not (a rename is not
//! transactional), which is why the whole run is wrapped in a pre-migration backup — the one restore
//! path when a run fails.
//!
//! **Downgrades do not exist.** A store stamped above [`LATEST_VERSION`] has nothing pending here
//! ([`pending`] returns nothing); refusing to open it by name is the gate's job.

use std::ops::ControlFlow;
use std::path::Path;

use rusqlite::Transaction;

use super::{Result, StoreEngine, META_FORMAT_VERSION, META_FORMAT_VERSION_SET_BY};
use crate::progress::{Phase, Progress};

/// The version of every store this build can open — the floor the chain starts from. A store below it
/// reads as v0 and is refused by name at open, not translated.
pub const BASELINE_VERSION: i64 = 2;

/// One numbered step of the chain: it brings a store **to** version [`to`](Step::to), from the version
/// below it.
pub struct Step {
    /// The version a store carries once this step has committed. Strictly greater than the previous
    /// step's, and greater than [`BASELINE_VERSION`].
    pub to: i64,
    /// What it does, for the log and for whoever reads the chain later.
    pub name: &'static str,
    pub apply: Apply,
}

/// How a step is applied. Two shapes, one concept ("a step of the store's migration") — the store is a
/// SQLite file *and* the directory around it.
pub enum Apply {
    /// SQL run inside the step's transaction (`execute_batch`, so several statements are fine).
    ///
    /// **Raw on purpose**, and the one place the typed layer must not reach: a step is *frozen* at
    /// the meaning it had when it was written. Built from the registry, it would follow the registry —
    /// rename a column tomorrow and a step that ran on stores years ago would silently start saying
    /// something else, which is the one thing a migration chain may never do. A step names the columns the
    /// store had **then**, in text, and stays wrong-proof by never moving.
    Sql(&'static str),
    /// Anything the chain cannot say in SQL: blobs on disk, the layout of the store directory. Gets the
    /// same transaction, so the DB half of a mixed step still commits with the version stamp.
    Custom(fn(&Ctx<'_>) -> Result<()>),
}

/// What a step is allowed to touch.
pub struct Ctx<'a> {
    /// The step's transaction — commits with the version stamp, or not at all.
    pub tx: &'a Transaction<'a>,
    /// The store directory: the truth-source file's home, and the home of everything beside it
    /// (attachment blobs, the activity ledger). A file a step moves here is **not** rolled back by
    /// `tx` — that is what the pre-migration backup is for.
    pub base_dir: &'a Path,
}

/// The chain. A change that moves a store appends a step here — and that alone bumps
/// [`LATEST_VERSION`], and with it [`crate::model::FORMAT_VERSION`].
pub const STEPS: &[Step] = &[Step {
    to: 3,
    name: "drop the orphaned owner_account meta row",
    // `owner_account` is a store-wide scalar the retired account dimension left behind: stores born
    // before it was dropped still carry the row, and nothing names it.
    apply: Apply::Sql("DELETE FROM store_meta WHERE key = 'owner_account';"),
}];

/// The version a store ends at once the chain has run — the last step's, or the baseline if there is
/// no step. **The chain defines the format version**, so a step cannot be added without the version
/// moving with it, and the version cannot move without a step to carry a store there.
pub const fn latest_version(steps: &[Step]) -> i64 {
    match steps.last() {
        Some(step) => step.to,
        None => BASELINE_VERSION,
    }
}

/// [`latest_version`] of the real chain — what [`crate::model::FORMAT_VERSION`] is.
pub const LATEST_VERSION: i64 = latest_version(STEPS);

/// The steps a store stamped at `from` still has to run. Empty when it is current — and when it is
/// *ahead* of this build, which is not this module's problem to report.
pub fn pending(from: i64, steps: &'static [Step]) -> &'static [Step] {
    let start = steps.partition_point(|s| s.to <= from);
    &steps[start..]
}

/// A chain is well-formed when its steps are strictly increasing and all above the baseline — which is
/// what lets [`pending`] find the resume point by a single partition, and what makes "the version a
/// store carries" name exactly one point in the chain. A malformed chain is a coding defect; the test
/// below holds [`STEPS`] to it.
pub fn is_well_formed(steps: &[Step]) -> bool {
    steps.windows(2).all(|w| w[0].to < w[1].to)
        && steps.first().is_none_or(|s| s.to > BASELINE_VERSION)
}

/// What a run of the chain did — for the caller that tells the human (and for the tests).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Run {
    /// The version the store carried when the run started.
    pub from: i64,
    /// The version it carries now. Equals `from` when nothing was pending.
    pub to: i64,
    /// The steps applied, in order.
    pub applied: Vec<&'static str>,
}

impl Run {
    /// Did this run change the store?
    pub fn migrated(&self) -> bool {
        !self.applied.is_empty()
    }
}

/// Run the chain forward over an open store: every pending step, in order, each in its own transaction
/// together with the version stamp that records it.
///
/// A step that fails leaves its own transaction rolled back and the store stamped at the last step that
/// committed — so a re-run resumes there rather than replaying what already landed. (A `Custom` step's
/// file half is outside the transaction and cannot be undone that way: the run as a whole is wrapped in
/// a pre-migration backup.)
///
/// Takes the chain as an argument rather than reading [`STEPS`] so a test can drive a chain of its own.
///
/// `progress` ticks [`Phase::Migrating`] at each step's boundary — a step is one transaction, so that is
/// the finest seam there is, and without it a surface goes silent for the whole chain (the pre-migration
/// backup is the only thing that reports otherwise, and the longer the chain grows the longer the silence).
/// A `Break` from it is **ignored**: a migration is not something to abandon halfway — stopping leaves the
/// store at a version this build cannot open, so a cancel would be a button that only breaks things.
pub fn run(
    engine: &StoreEngine,
    base_dir: &Path,
    steps: &'static [Step],
    progress: &mut impl FnMut(&Progress) -> ControlFlow<()>,
) -> Result<Run> {
    debug_assert!(is_well_formed(steps), "the version chain is not strictly increasing");

    let from = engine.format_version()?;
    let mut run = Run { from, to: from, applied: Vec::new() };

    let todo = pending(from, steps);
    let total = todo.len() as u64;
    for (done, step) in todo.iter().enumerate() {
        let _ = progress(&Progress { phase: Phase::Migrating, done: done as u64, total: Some(total) });
        let tx = engine.transaction()?;
        match step.apply {
            Apply::Sql(sql) => tx.execute_batch(sql)?,
            Apply::Custom(f) => f(&Ctx { tx: &tx, base_dir })?,
        }
        stamp(&tx, step.to)?;
        tx.commit()?;
        run.to = step.to;
        run.applied.push(step.name);
    }
    Ok(run)
}

/// Stamp the store's format version **inside the step's transaction** — the version and the change it
/// describes are one commit. The app version doing the stamping goes with it: it is what a later, older
/// build names when it refuses the store it cannot open.
fn stamp(tx: &Transaction<'_>, version: i64) -> Result<()> {
    for (key, value) in [
        (META_FORMAT_VERSION, version.to_string()),
        (META_FORMAT_VERSION_SET_BY, crate::agent::VERSION.to_string()),
    ] {
        super::engine::upsert_meta(tx, key, Some(&value))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("amenbo-migrate-{tag}-{}", crate::tmpdir::suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A store stamped at `version` — the shape an older build left behind, which is what the chain
    /// exists to move.
    fn store_at(dir: &Path, version: i64) -> StoreEngine {
        let engine = StoreEngine::open(&dir.join("store.sqlite")).unwrap();
        engine
            .conn()
            .execute(
                "INSERT INTO store_meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![META_FORMAT_VERSION, version.to_string()],
            )
            .unwrap();
        assert_eq!(engine.format_version().unwrap(), version);
        engine
    }

    /// A store at the baseline: the oldest one this build still opens, and so the one every step runs on.
    fn baseline_store(dir: &Path) -> StoreEngine {
        store_at(dir, BASELINE_VERSION)
    }

    /// A store as this build creates one — born at the latest shape, with no step to run.
    fn current_store(dir: &Path) -> StoreEngine {
        let engine = StoreEngine::open(&dir.join("store.sqlite")).unwrap();
        engine.stamp_format_version().unwrap();
        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        engine
    }

    const ADD_COLUMN: &[Step] = &[Step {
        to: 3,
        name: "add store_meta_note",
        apply: Apply::Sql("CREATE TABLE store_meta_note (note TEXT NOT NULL);"),
    }];

    #[test]
    fn the_shipped_chain_is_well_formed_and_defines_the_format_version() {
        assert!(is_well_formed(STEPS));
        assert_eq!(LATEST_VERSION, crate::model::FORMAT_VERSION);
    }

    #[test]
    fn a_current_store_has_nothing_pending() {
        let dir = scratch("current");
        let engine = current_store(&dir);

        let run = run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert!(!run.migrated());
        assert_eq!(run, Run { from: LATEST_VERSION, to: LATEST_VERSION, applied: vec![] });
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The shipped chain, run on the oldest store this build opens: it lands, and it carries the store to
    /// the version this build says it can open.
    #[test]
    fn the_shipped_chain_carries_a_baseline_store_to_the_latest_version() {
        let dir = scratch("shipped");
        let engine = baseline_store(&dir);
        engine
            .conn()
            .execute("INSERT INTO store_meta (key, value) VALUES ('owner_account', 'P0')", [])
            .unwrap();

        let run = run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert!(run.migrated());
        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        assert_eq!(engine.get_meta("owner_account").unwrap(), None, "the orphan row is gone");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_sql_step_runs_and_stamps_the_version_it_carries_the_store_to() {
        let dir = scratch("sql");
        let engine = baseline_store(&dir);

        let run = run(&engine, &dir, ADD_COLUMN, &mut crate::progress::ignore).unwrap();

        assert_eq!(run, Run { from: 2, to: 3, applied: vec!["add store_meta_note"] });
        assert_eq!(engine.format_version().unwrap(), 3);
        engine.conn().execute("INSERT INTO store_meta_note (note) VALUES ('x')", []).unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A step is not necessarily SQL: this one touches the store directory *and* the DB, and both land
    /// with the version.
    #[test]
    fn a_custom_step_may_touch_the_store_directory() {
        const MIXED: &[Step] = &[Step {
            to: 3,
            name: "move a blob and record it",
            apply: Apply::Custom(|ctx| {
                std::fs::write(ctx.base_dir.join("blob-moved"), b"x")?;
                ctx.tx.execute(
                    "INSERT INTO store_meta (key, value) VALUES ('blob_layout', '2')",
                    [],
                )?;
                Ok(())
            }),
        }];

        let dir = scratch("custom");
        let engine = baseline_store(&dir);

        let run = run(&engine, &dir, MIXED, &mut crate::progress::ignore).unwrap();

        assert_eq!(run.to, 3);
        assert!(dir.join("blob-moved").is_file(), "the step's file half landed");
        assert_eq!(engine.get_meta("blob_layout").unwrap().as_deref(), Some("2"));
        assert_eq!(engine.format_version().unwrap(), 3);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A failing step takes its own transaction down with it — and leaves the store stamped at the last
    /// step that committed, so a re-run resumes rather than replays.
    #[test]
    fn a_failing_step_rolls_itself_back_and_the_store_resumes_at_the_last_one_that_committed() {
        const CHAIN: &[Step] = &[
            Step {
                to: 3,
                name: "add store_meta_note",
                apply: Apply::Sql("CREATE TABLE store_meta_note (note TEXT NOT NULL);"),
            },
            Step {
                to: 4,
                name: "half-write, then fail",
                apply: Apply::Custom(|ctx| {
                    ctx.tx.execute("INSERT INTO store_meta_note (note) VALUES ('half')", [])?;
                    // The table does not exist: this is the step failing partway through.
                    ctx.tx.execute("INSERT INTO no_such_table (x) VALUES (1)", [])?;
                    Ok(())
                }),
            },
        ];

        let dir = scratch("fail");
        let engine = baseline_store(&dir);

        assert!(run(&engine, &dir, CHAIN, &mut crate::progress::ignore).is_err());

        assert_eq!(engine.format_version().unwrap(), 3, "step 3 committed, step 4 did not");
        let notes: i64 = engine
            .conn()
            .query_row("SELECT COUNT(*) FROM store_meta_note", [], |r| r.get(0))
            .unwrap();
        assert_eq!(notes, 0, "the failing step's half-write is rolled back");

        // Resuming: step 3 does not run again (it would fail — the table is already there).
        let resumed = run(&engine, &dir, ADD_COLUMN, &mut crate::progress::ignore).unwrap();
        assert!(!resumed.migrated());
        std::fs::remove_dir_all(&dir).ok();
    }

    const TWO_STEPS: &[Step] = &[
        Step { to: 3, name: "one", apply: Apply::Sql("CREATE TABLE one (x TEXT);") },
        Step { to: 4, name: "two", apply: Apply::Sql("CREATE TABLE two (x TEXT);") },
    ];

    /// A long chain is not a silent one: each step reports itself at its boundary, counted against
    /// the steps that were pending — which is all a surface needs to draw a bar that moves.
    #[test]
    fn every_step_ticks_at_its_boundary() {
        let dir = scratch("ticks");
        let engine = baseline_store(&dir);

        let mut ticks = Vec::new();
        run(&engine, &dir, TWO_STEPS, &mut |p: &Progress| {
            ticks.push((p.phase, p.done, p.total));
            ControlFlow::Continue(())
        })
        .unwrap();

        assert_eq!(ticks, vec![(Phase::Migrating, 0, Some(2)), (Phase::Migrating, 1, Some(2))]);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The chain is not a cancellation point: a sink that asks to stop is heard and ignored, because the
    /// store it would leave behind is one this build cannot open.
    #[test]
    fn a_cancel_from_the_sink_does_not_stop_the_chain() {
        let dir = scratch("no-cancel");
        let engine = baseline_store(&dir);

        let run = run(&engine, &dir, TWO_STEPS, &mut |_: &Progress| ControlFlow::Break(())).unwrap();

        assert_eq!(run.applied, vec!["one", "two"]);
        assert_eq!(engine.format_version().unwrap(), 4);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_store_ahead_of_this_build_has_nothing_pending_here() {
        assert!(pending(9, ADD_COLUMN).is_empty());
        assert_eq!(pending(2, ADD_COLUMN).len(), 1);
    }

    #[test]
    fn a_malformed_chain_is_caught() {
        const BACKWARDS: &[Step] =
            &[Step { to: 4, name: "b", apply: Apply::Sql("") }, Step { to: 3, name: "a", apply: Apply::Sql("") }];
        const BELOW_BASELINE: &[Step] = &[Step { to: 1, name: "old", apply: Apply::Sql("") }];

        assert!(!is_well_formed(BACKWARDS));
        assert!(!is_well_formed(BELOW_BASELINE));
        assert!(is_well_formed(ADD_COLUMN));
    }
}
