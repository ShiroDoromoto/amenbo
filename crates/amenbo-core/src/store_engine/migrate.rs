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
pub const STEPS: &[Step] = &[
    Step {
        to: 3,
        name: "drop the orphaned owner_account meta row",
        // `owner_account` is a store-wide scalar the retired account dimension left behind: stores born
        // before it was dropped still carry the row, and nothing names it.
        apply: Apply::Sql("DELETE FROM store_meta WHERE key = 'owner_account';"),
    },
    Step {
        to: 4,
        name: "fold the per-project hook consent into one device answer, keeping each refusal as an opt-out",
        apply: Apply::Custom(fold_hook_consent_to_device),
    },
    Step {
        to: 5,
        name: "add decision.status_changed_at, seeded from when each decision was last settled",
        // The column the reopen axis compares against (`AMB-D-373`): when a decision's status last changed.
        // Existing rows have no such instant recorded, so they are seeded once — `decided_at` for one that
        // was settled, its creation for one still under discussion. A seed taken from a record column is
        // sound precisely because it is taken *once*: from here on the intent column is what moves, and what
        // `created_at` does afterwards no longer reaches the judgement (`AMB-D-372`).
        //
        // What the seed cannot recover: a reopen that happened before this ran left no dated trace (the
        // activity log is not a system of record), so such a decision is seeded at its creation and reads as
        // "unchanged since", i.e. no warn. Erring quiet on history we cannot date is the safe side.
        //
        // `NULLIF` guards the `''` a half-written row's required-text column carries: the column's `CHECK`
        // admits an instant or NULL, and `''` is neither. The declaration is spelled out here in frozen
        // text, as every step's is — the registry may rename or reshape the column tomorrow; what this step
        // added must keep meaning what it meant.
        apply: Apply::Sql(
            "ALTER TABLE decision ADD COLUMN status_changed_at TEXT \
                 CHECK(status_changed_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z');
             UPDATE decision SET status_changed_at = COALESCE(NULLIF(decided_at, ''), NULLIF(created_at, ''));",
        ),
    },
    Step {
        to: 6,
        name: "give task.status_changed_at to a store that never got it",
        // The task-side twin of v5, arriving late: the column was declared in the registry (`AMB-D-366`'s
        // data floor) without a step to carry it, so a store any earlier build wrote has never had it and
        // would fail at the first read of a task with `no such column`. This is that step.
        //
        // **Unseeded, on purpose.** There is no honest instant to put in it. A task's creation is not when
        // its current status began, and dating every old task there would say "reserved at creation" — so
        // every premise the backlog has gathered since would read as *added after the reservation* and warn,
        // on the whole backlog at once. `NULL` is what the column already means for a row that predates it
        // (`Task::status_changed_at`), and the judgement skips a task that carries it rather than guessing.
        // The clock starts for real at that task's next status change.
        apply: Apply::Custom(add_task_status_clock),
    },
    Step {
        to: 7,
        name: "add the premise edges' intent columns, seeded from when each row was written",
        // `AMB-D-372`: the premise-change judgement dates a blocker edge and a decision link by an intent
        // column, not by `created_at`. Both columns are new, so every existing row is seeded once — from
        // `created_at`, which on these rows *is* the instant the edge was drawn (both tables are
        // insert-and-hard-delete only, with no UPDATE path to have moved it since). Taking a record column
        // as a seed is sound precisely because it is taken once: from here on the intent column is what the
        // judgement reads, and what `created_at` does afterwards no longer reaches it.
        //
        // `NULLIF` guards the `''` a row caught mid-create carries: the column's `CHECK` admits an instant
        // or NULL, and `''` is neither. Spelled in frozen text, as every step's is.
        apply: Apply::Sql(
            "ALTER TABLE task_dependency ADD COLUMN established_at TEXT \
                 CHECK(established_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z');
             ALTER TABLE decision_task_link ADD COLUMN linked_at TEXT \
                 CHECK(linked_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z');
             UPDATE task_dependency SET established_at = NULLIF(created_at, '');
             UPDATE decision_task_link SET linked_at = NULLIF(created_at, '');",
        ),
    },
];

/// v4: the lint-hook question stopped being one per project and became one for the device
/// (`crate::hooks`), so the `hook_consent` table has no one left to answer for. Dropping it would throw
/// away an answer the user already gave; this carries it across, and it is a `Custom` step because the
/// answer's new home is `config.json` beside the store rather than a column in it.
///
/// **The fold: any `yes` wins.** The rows are the same person answering the same question in several
/// places, and consent is to the lint as a feature — so one `yes` is that person having said yes to it.
/// Rows that are all `no` fold to `no`; no rows at all is the unanswered state and stays unanswered,
/// which is what keeps a store that was never asked from being treated as having refused.
///
/// **Each `no` also survives as an opt-out.** A device-wide `yes` would otherwise install into the very
/// repositories that refused, at the first startup after the upgrade — the fold must not turn a refusal
/// into its opposite, so every `no` row becomes a `hook_optout` row and the repository stays as the user
/// left it. Under a folded `no` the rows are redundant but harmless, and writing them unconditionally
/// keeps this step one statement rather than a branch.
///
/// Everything here names its columns and its config key in frozen text, per this module's contract: the
/// step must keep meaning what it meant, whatever the typed layer is called tomorrow. A config that
/// cannot be read is left to its defaults rather than failing the migration — an unreadable config is
/// one the user's own next write repairs, and refusing to migrate the store over it would be the worse
/// of the two outcomes.
fn fold_hook_consent_to_device(ctx: &Ctx<'_>) -> Result<()> {
    // A store that predates the table has nothing to fold. `IF NOT EXISTS` rather than a probe: the two
    // shapes then take the same path out.
    ctx.tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS hook_consent (project_id INTEGER PRIMARY KEY, answer TEXT);
         CREATE TABLE IF NOT EXISTS hook_optout (project_id INTEGER PRIMARY KEY);
         INSERT OR IGNORE INTO hook_optout (project_id)
             SELECT project_id FROM hook_consent WHERE answer = 'no';",
    )?;
    let answers: Vec<String> = {
        let mut stmt = ctx.tx.prepare("SELECT answer FROM hook_consent")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    if let Some(folded) = fold_answers(&answers) {
        write_config_hook_consent(ctx.base_dir, folded);
    }
    ctx.tx.execute_batch("DROP TABLE hook_consent;")?;
    Ok(())
}

/// The fold itself, apart from the store so it can be tested as the rule it is: any `yes` is a yes, any
/// answer at all with no `yes` is a no, and nothing answered is `None` (leave the device unasked). An
/// answer the old `CHECK` should have refused is not an answer and takes no part.
fn fold_answers(answers: &[String]) -> Option<&'static str> {
    if answers.iter().any(|a| a == "yes") {
        return Some("yes");
    }
    answers.iter().any(|a| a == "no").then_some("no")
}

/// Put the folded answer in `config.json` under `hook_consent`, leaving every other key exactly as it
/// was. Read-modify-write on the JSON rather than through `crate::config::Config`, for the reason the
/// module doc gives: a step is frozen, and a struct is not.
fn write_config_hook_consent(base_dir: &Path, answer: &str) {
    let path = base_dir.join("config.json");
    let mut doc: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let Some(obj) = doc.as_object_mut() else { return };
    obj.insert("hook_consent".to_string(), serde_json::Value::String(answer.to_string()));
    if let Ok(text) = serde_json::to_string_pretty(&doc) {
        let _ = crate::store::write_atomic(&path, text.as_bytes());
    }
}

/// v6: add `task.status_changed_at` — **only where it is missing**, which is the one thing this step
/// cannot say in plain SQL (`ALTER TABLE … ADD COLUMN` on a column that is already there is an error, and
/// it would take the whole migration down with it).
///
/// Both shapes are out there, and neither is a mistake in the store. The column was declared in the
/// registry two versions before it had a step, so for that window every *new* store was born with it (a
/// fresh store is created from the registry) while every *existing* one stayed without — the version a
/// store carries does not tell the two apart. Asking the table is the only way to know.
///
/// This is not the presence-guarded diff the chain exists to replace (`AMB-D-231`): that was a pile of
/// `IF EXISTS` operations replayed on every open, standing in for a history. This is one numbered step,
/// run once at one version, repairing a window that is closed and dated. What it must never become is a
/// habit — a column and the step that carries it belong in the same change.
fn add_task_status_clock(ctx: &Ctx<'_>) -> Result<()> {
    let held: i64 = ctx.tx.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('task') WHERE name = 'status_changed_at'",
        [],
        |r| r.get(0),
    )?;
    if held == 0 {
        // Frozen text, like every step's: the `CHECK` is the instant form the column admitted when this
        // was written, whatever the registry calls it later.
        ctx.tx.execute_batch(
            "ALTER TABLE task ADD COLUMN status_changed_at TEXT \
                 CHECK(status_changed_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z');",
        )?;
    }
    Ok(())
}

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
    use rusqlite::OptionalExtension;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = amenbo_scratch::scratch(&format!("migrate-{tag}"));
        dir
    }

    /// The columns a step adds, and the version whose step adds them — so a store made here can be put
    /// back to the shape its version claims. Append a row when a step adds a column.
    const COLUMNS_ADDED_BY_A_STEP: &[(i64, &str, &str)] = &[
        (5, "decision", "status_changed_at"),
        (6, "task", "status_changed_at"),
        (7, "task_dependency", "established_at"),
        (7, "decision_task_link", "linked_at"),
    ];

    /// A store stamped at `version` — the shape an older build left behind, which is what the chain
    /// exists to move.
    ///
    /// Stamping the version is not enough to *be* that store: every store here is created by this build,
    /// so its `CREATE TABLE` already carries the columns later steps add, and an `ADD COLUMN` step run on
    /// one would fail on a column that is already there. So the columns those steps add are dropped back
    /// off, and the store really does have the shape its version claims.
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
        for (added_at, table, column) in COLUMNS_ADDED_BY_A_STEP {
            if version < *added_at {
                engine
                    .conn()
                    .execute_batch(&format!("ALTER TABLE {table} DROP COLUMN {column};"))
                    .unwrap();
            }
        }
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

    /// The genesis DDL runs at open, and the chain runs on the engine that open returns — so the DDL
    /// necessarily meets an un-migrated store first. It must therefore name only what the **baseline**
    /// store already has: an index over a column a step adds would fail on exactly the store that step
    /// exists for, and it would fail at open, before the chain could rescue it. Re-running the whole
    /// batch over a baseline-shaped store is that check (`IF NOT EXISTS` makes the re-run a no-op where
    /// the object is already there, so what is left is whether every column it names resolves).
    ///
    /// If this goes red, the fix is not to move the DDL: put the index in the step that adds its column,
    /// beside the `ALTER TABLE`.
    #[test]
    fn the_genesis_ddl_applies_to_a_baseline_store() {
        let dir = scratch("genesis-ddl");
        let engine = baseline_store(&dir);

        engine
            .conn()
            .execute_batch(&crate::store_engine::schema::schema_sql())
            .expect("the genesis DDL names a column the baseline store does not have");

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
        engine
            .conn()
            .query_row("SELECT COUNT(status_changed_at) FROM decision", [], |r| r.get::<_, i64>(0))
            .expect("v5 put the decision status clock back");
        engine
            .conn()
            .query_row("SELECT COUNT(status_changed_at) FROM task", [], |r| r.get::<_, i64>(0))
            .expect("v6 put the task status clock back");
        engine
            .conn()
            .query_row("SELECT COUNT(established_at) FROM task_dependency", [], |r| r.get::<_, i64>(0))
            .expect("v7 put the edge's intent column back");
        engine
            .conn()
            .query_row("SELECT COUNT(linked_at) FROM decision_task_link", [], |r| r.get::<_, i64>(0))
            .expect("v7 put the link's intent column back");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The fold rule on its own: any `yes` wins, an all-`no` set folds to `no`, and an empty set stays
    /// unanswered (`None`), so a store never asked is not treated as having refused. A stray value the old
    /// `CHECK` should have refused takes no part.
    #[test]
    fn the_hook_consent_fold_takes_any_yes_and_leaves_an_empty_set_unanswered() {
        let s = |v: &[&str]| fold_answers(&v.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        assert_eq!(s(&[]), None, "never asked stays unanswered");
        assert_eq!(s(&["no", "no"]), Some("no"), "all refusals fold to a refusal");
        assert_eq!(s(&["no", "yes", "no"]), Some("yes"), "one yes is a yes");
        assert_eq!(s(&["maybe"]), None, "a value the CHECK should have refused is not an answer");
    }

    /// v4 in full, on the store shape v3 left behind: a `hook_consent` table with a row per project. The
    /// answer must survive into `config.json`, every `no` must survive as a `hook_optout` row so a
    /// device-wide `yes` cannot reinstall where the user removed the hook, and the old table must be gone.
    #[test]
    fn the_hook_consent_fold_carries_the_answer_to_the_config_and_keeps_each_refusal() {
        let dir = scratch("hookfold");
        // A store that answered the hook question was onboarded, so a config.json is already there. Seed a
        // default one and give it a non-default field, to prove the fold adds its key without disturbing
        // the rest.
        {
            let cfg = crate::config::Config { language: Some("ja".to_string()), ..Default::default() };
            cfg.save(&dir.join("config.json")).unwrap();
        }
        let engine = store_at(&dir, 3);
        engine
            .conn()
            .execute_batch(
                // Real projects, because the old `hook_consent` (and the new `hook_optout`) reference
                // `project(id)` — the fold moves rows between two FK-guarded tables, so its inputs must
                // point at live projects, exactly as production data does.
                "INSERT INTO project (id, name) VALUES (1, 'A'), (2, 'B'), (3, 'C');
                 CREATE TABLE hook_consent (project_id INTEGER PRIMARY KEY, answer TEXT);
                 INSERT INTO hook_consent (project_id, answer) VALUES (1, 'yes'), (2, 'no'), (3, 'no');",
            )
            .unwrap();

        let run = run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();
        assert!(run.applied.iter().any(|s| s.contains("hook consent")), "v4 ran: {:?}", run.applied);
        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);

        // The device answer landed in the config beside the store, leaving the rest of it intact.
        let cfg = crate::config::Config::load(&dir.join("config.json"));
        assert_eq!(cfg.hook_consent, Some(crate::hooks::HookConsent::Yes), "one yes among the rows is a device yes");
        assert_eq!(cfg.language.as_deref(), Some("ja"), "the fold adds its key and disturbs nothing else");

        // Each refusal became an opt-out, so the two `no` projects stay as the user left them.
        let opted: Vec<i64> = {
            let conn = engine.conn();
            let mut stmt = conn.prepare("SELECT project_id FROM hook_optout ORDER BY project_id").unwrap();
            let rows = stmt.query_map([], |r| r.get::<_, i64>(0)).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };
        assert_eq!(opted, vec![2, 3], "every no is kept as an opt-out; the yes is not");

        // The old table is gone — the answer has one home now.
        let has_old: Option<String> = engine
            .conn()
            .query_row("SELECT name FROM sqlite_master WHERE type='table' AND name='hook_consent'", [], |r| r.get(0))
            .optional()
            .unwrap();
        assert_eq!(has_old, None, "the per-project table is retired");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A store that predates the `hook_consent` table (nobody ever answered) migrates cleanly and leaves
    /// the device unanswered — the fold invents no answer where there was none to carry.
    #[test]
    fn the_hook_consent_fold_leaves_an_unasked_store_unasked() {
        let dir = scratch("hookfold-empty");
        let engine = store_at(&dir, 3);
        // No hook_consent table at all — the shape of a store born before the feature.
        engine.conn().execute_batch("DROP TABLE IF EXISTS hook_consent;").unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        assert_eq!(crate::config::Config::load(&dir.join("config.json")).hook_consent, None, "nothing to carry");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v5 in full, on the store shape v4 left behind: a `decision` table with no status clock. Every
    /// existing row must come out of it seeded — a settled decision at the instant it was settled, one
    /// still under discussion at its creation — because the reopen axis (`AMB-D-373`) compares against this
    /// column and a NULL is a decision it would never judge. A row still mid-create carries `''` rather
    /// than an instant, and `''` is not one: it seeds to NULL rather than to a value the column's own
    /// `CHECK` would refuse.
    #[test]
    fn the_decision_status_clock_is_seeded_from_when_each_decision_was_settled() {
        let dir = scratch("decision-status-clock");
        let engine = store_at(&dir, 4);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO decision (id, project_id, title, body, status, decided_at, created_at, updated_at) VALUES
                     (1, 1, 'settled',  '', 'accepted', '2026-01-02T03:04:05Z', '2025-12-01T00:00:00Z', '2026-01-02T03:04:05Z'),
                     (2, 1, 'proposed', '', 'proposed', NULL,                   '2025-11-01T00:00:00Z', '2025-11-01T00:00:00Z'),
                     (3, 1, '',         '', '',         NULL,                   '',                     '');",
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        let seeded: Vec<(i64, Option<String>)> = {
            let conn = engine.conn();
            let mut stmt =
                conn.prepare("SELECT id, status_changed_at FROM decision ORDER BY id").unwrap();
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };
        assert_eq!(
            seeded,
            vec![
                (1, Some("2026-01-02T03:04:05Z".to_string())),
                (2, Some("2025-11-01T00:00:00Z".to_string())),
                (3, None),
            ],
            "settled rows seed from decided_at, unsettled ones from their creation, and `''` is no instant"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v6 on the store every earlier build wrote: `task` without the status clock. The column has to be
    /// there afterwards — a read of any task names it — and every existing row has to come out `NULL`,
    /// which is the column's own word for "this task predates it" and what keeps the whole backlog from
    /// warning at once on a date that was never true.
    #[test]
    fn the_task_status_clock_lands_on_a_store_that_never_had_it() {
        let dir = scratch("task-status-clock");
        let engine = store_at(&dir, 5);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO task (id, title, status, created_at, updated_at)
                 VALUES (1, 'reserved long ago', 'in_progress', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z');",
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        let clock: Option<String> = engine
            .conn()
            .query_row("SELECT status_changed_at FROM task WHERE id = 1", [], |r| r.get(0))
            .expect("v6 put the column there");
        assert_eq!(clock, None, "a task that predates the column is left saying so, not dated by a guess");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The same step on the *other* shape of v5 store: one born with the column, from the window where the
    /// registry declared it before any step carried it. Both are real stores at the same version, and the
    /// step has to pass over this one rather than take the migration down with a duplicate column.
    #[test]
    fn the_task_status_clock_step_passes_over_a_store_that_already_has_it() {
        let dir = scratch("task-status-clock-born-with");
        let engine = store_at(&dir, 5);
        engine
            .conn()
            .execute_batch(
                "ALTER TABLE task ADD COLUMN status_changed_at TEXT;
                 INSERT INTO task (id, title, status, status_changed_at, created_at, updated_at)
                 VALUES (1, 'reserved', 'in_progress', '2026-07-01T00:00:00Z', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z');",
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION, "the run got all the way through");
        let clock: Option<String> = engine
            .conn()
            .query_row("SELECT status_changed_at FROM task WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(clock.as_deref(), Some("2026-07-01T00:00:00Z"), "and left what the store already held");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v7 in full, on the store shape v6 left behind: premise edges with no intent column. Every existing
    /// row must come out of it seeded from `created_at` — the instant it was in fact drawn, these tables
    /// having no UPDATE path — because the premise-change judgement (`AMB-D-372`) now reads only the intent
    /// column, and a NULL there is a premise it would never flag. A row caught mid-create carries `''`
    /// rather than an instant, and `''` is not one: it seeds to NULL rather than to a value the column's own
    /// `CHECK` would refuse.
    #[test]
    fn the_premise_edges_are_seeded_from_when_each_row_was_written() {
        let dir = scratch("premise-intent-columns");
        let engine = store_at(&dir, 6);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO task (id, project_id, title, created_at, updated_at) VALUES
                     (1, 1, 'held', '2025-10-01T00:00:00Z', '2025-10-01T00:00:00Z'),
                     (2, 1, 'blocker', '2025-10-01T00:00:00Z', '2025-10-01T00:00:00Z');
                 INSERT INTO decision (id, project_id, title, body, status, created_at, updated_at) VALUES
                     (1, 1, 'd', '', 'proposed', '2025-10-01T00:00:00Z', '2025-10-01T00:00:00Z');
                 INSERT INTO task_dependency (id, task_id, blocked_by_id, created_at, updated_at) VALUES
                     (1, 1, 2, '2026-02-03T04:05:06Z', '2026-02-03T04:05:06Z'),
                     (2, 2, 1, '',                     '');
                 INSERT INTO decision_task_link (id, decision_id, task_id, created_at, updated_at) VALUES
                     (1, 1, 1, '2026-03-04T05:06:07Z', '2026-03-04T05:06:07Z');",
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        let seeded = |table: &str, column: &str| -> Vec<(i64, Option<String>)> {
            let conn = engine.conn();
            let mut stmt = conn.prepare(&format!("SELECT id, {column} FROM {table} ORDER BY id")).unwrap();
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };
        assert_eq!(
            seeded("task_dependency", "established_at"),
            vec![(1, Some("2026-02-03T04:05:06Z".to_string())), (2, None)],
            "an edge is seeded at the instant it was drawn, and `''` is no instant"
        );
        assert_eq!(
            seeded("decision_task_link", "linked_at"),
            vec![(1, Some("2026-03-04T05:06:07Z".to_string()))],
            "a link is seeded at the instant it was drawn"
        );
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
