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

use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::path::Path;

use rusqlite::{OptionalExtension, Transaction};

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
    Step {
        to: 8,
        name: "add the decision edge's intent column, seeded from when each row was written",
        // The third premise edge, arriving after its two siblings (v7): a supersession had no instant to be
        // dated by, so a premise that lost currency under a holder could not be surfaced (`AMB-D-373`). The
        // seed is `created_at` for the same reason as v7's — an edge row is written once, and the one
        // rewrite it does admit (a pair's kind promoted in place) moves `updated_at`, which would date the
        // *promotion* rather than the edge. On a store this runs on the two are the same instant for every
        // row that was never promoted, and for one that was, the honest reading of a column that did not
        // exist yet is "drawn no later than this" — the quiet side, as v6's unseeded column is.
        //
        // `NULLIF` guards the `''` a row caught mid-create carries: the column's `CHECK` admits an instant
        // or NULL, and `''` is neither. Spelled in frozen text, as every step's is.
        apply: Apply::Sql(
            "ALTER TABLE decision_edge ADD COLUMN drawn_at TEXT \
                 CHECK(drawn_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z');
             UPDATE decision_edge SET drawn_at = NULLIF(created_at, '');",
        ),
    },
    Step {
        to: 9,
        name: "let task.status admit 'rejected', the terminal for work decided against",
        // `AMB-D-397`: a task nobody is going to do gets a terminal of its own, so the closed set the
        // column's `CHECK` names has to grow by one. Widening a `CHECK` is the one schema change SQLite has
        // no `ALTER TABLE` for, and the rebuild-and-swap its documentation prescribes cannot be done from
        // here — see the function.
        apply: Apply::Custom(admit_rejected_task_status),
    },
    Step {
        to: 10,
        name: "hold the concept rows to RESTRICT, so no delete of theirs happens outside a delete op",
        // `AMB-D-403`: a row that stands for a concept is deleted by an op or not at all, and `RESTRICT` is
        // how the database holds that. The ops already take their children row by row (`AMB-T-2195`), so on
        // a store that has been driven by this build nothing changes — what changes is that leaving one
        // behind now stops the parent's `DELETE` instead of sweeping it.
        apply: Apply::Custom(restrict_the_concept_references),
    },
    Step {
        to: 11,
        name: "add plugin_outbox.project, the project an event is stamped with when it is appended",
        // `AMB-D-405`: the fan-out stopped reading the project back off the record and reads it off the
        // event instead, so the outbox grew a column. A store already on disk has the table (the outbox
        // predates this), and `CREATE TABLE IF NOT EXISTS` leaves a table that is present alone — without
        // this step the first drain fails with `no such column`.
        //
        // **Unseeded, on purpose.** The rows an old store carries are events already appended, and the
        // project they belong to is exactly what was never written down; reading it back off the record
        // now is the guess this decision removed (the record may have moved, or be gone). `NULL` is the
        // column's own word for "in no project, or unknown", which is what these rows are. The window is
        // short in any case — the outbox is trimmed as soon as the fan-out has copied a row.
        apply: Apply::Custom(add_outbox_project),
    },
    Step {
        to: 12,
        name: "add plugin_queue.project, so a queued row carries the project it was fanned out for",
        // `AMB-D-405`, the other half: the runner resolves the subscription a second time, and it can only
        // answer a project-scoped plugin's gate with the project the event happened in. The fan-out has it
        // (v11 put it on the outbox row) and now copies it forward, so nothing between the queue and the
        // run reads the record back — which is the whole point on the row that has none left.
        //
        // **Unseeded, like v11's.** A row already on a queue was fanned out before anything wrote the
        // project down; `NULL` is what it is, and a project-scoped subscription fires nothing for it. The
        // window is a queue's depth, not a store's age.
        apply: Apply::Custom(add_queue_project),
    },
    Step {
        to: 13,
        name: "add plugin_outbox.record and plugin_queue.record, so a deletion carries the record that is gone",
        // `AMB-D-407`: a live record is read back by name (`AMB-D-406`), so only what cannot be read is
        // carried — and that is the deleted record's own shape, captured at the append and copied onto
        // every queue it is fanned out to. Both tables in one step because they are one path: a column on
        // the outbox alone would be dropped at the fan-out, and one on the queue alone would have nothing
        // to copy.
        //
        // **Unseeded, like v11's and v12's.** The rows an old store carries describe records that are
        // already gone, and their shape is exactly what was never written down; `NULL` is what they are,
        // and a subscriber reads it as an event from a build that did not carry one.
        apply: Apply::Custom(add_gone_record),
    },
    Step {
        to: 14,
        name: "add plugin_outbox.parent and plugin_queue.parent, so a child's deletion names what it hung on",
        // `AMB-D-407`, the other half of what a deletion cannot be asked for afterwards: `record_id` names
        // the row the event is about, so a subscriber that hears only "comment 5 is gone" cannot say which
        // task it was on. Both tables again, for the reason v13 gives — the two are one path.
        //
        // **Unseeded.** The rows an old store carries describe records already gone, and what they hung on
        // is exactly what was never written down.
        apply: Apply::Custom(add_parent),
    },
    Step {
        to: 15,
        name: "move a plugin's settings and secrets out of the user area and into each project's rows",
        // A plugin is a project's, so its values are too: the machine-wide default in `config.json` and
        // the secret in `plugin-secrets.json` become a row per project in `plugin_config` /
        // `plugin_secret`, and the two user-area homes go.
        //
        // **Seeded, unlike the four steps above it** — and this is the one place a seed is not a guess. A
        // machine-wide default is, by construction, the value every project without one of its own is
        // handed at run time, so writing it as each project's row is what that sentence *meant*, not an
        // inference about it. A project holding a value of its own keeps it: the insert stands down, and
        // the closer of the two answers stands.
        //
        // A store with no project carries the values nowhere, and that is right: nothing could ever have
        // fired for them, and inventing a project to hold them would be worse than letting them go.
        apply: Apply::Custom(move_plugin_settings_into_the_store),
    },
    Step {
        to: 16,
        name: "add harness_consent, a project's answer on being asked to start its AI on Amenbo",
        // `AMB-D-440`. The genesis batch creates a table an older store is missing at open, so this DDL
        // has usually run before the chain reaches here — writing it down anyway is what makes the chain
        // say when the table arrived, rather than leaving a reader of the frozen shapes to guess.
        //
        // **Unseeded, and there is nothing it could be seeded from.** No row is the unanswered state, and
        // nobody has been asked this question yet: inventing a `yes` for every existing project would be
        // consenting on their behalf, and a `no` would silence a question never put.
        apply: Apply::Sql(
            "CREATE TABLE IF NOT EXISTS harness_consent (\
               project_id INTEGER PRIMARY KEY REFERENCES project(id) ON DELETE CASCADE NOT NULL, \
               allowed INTEGER NOT NULL, \
               asked_again INTEGER NOT NULL\
             );",
        ),
    },
    Step {
        to: 17,
        name: "fill in the word index from the text every record already holds",
        // `AMB-D-450`. The genesis batch creates the copy table, its FTS5 index and the triggers between
        // them, so by the time the chain reaches here an older store has them — **empty**. Nothing has
        // been written since they appeared, and the index is only ever written by a field write, so
        // without this step every record that existed before the upgrade would be invisible to a word.
        //
        // Seeded, unlike every column-adding step above, and seeded from the records themselves rather
        // than from a guess: the index is derived, so "what it should hold" is not a judgement call —
        // it is what `search::rebuild` reads back out of the columns it copies.
        apply: Apply::Custom(fill_the_word_index),
    },
    Step {
        to: 18,
        name: "widen the word index past the bodies, onto the labels and what is attached",
        // `AMB-D-450`'s remaining faces: the names a person gave an axis and its values, and what an
        // attachment is called. They are text that was already in the store and simply had no copy, so
        // the step is the same rebuild v17 ran — driven by the face list, which is what changed.
        //
        // A rebuild rather than a top-up: the list is the truth about what belongs in the index, so
        // reading the store back through it leaves exactly what a store born today would hold, and
        // cannot drift by however many faces were added at once.
        apply: Apply::Custom(fill_the_word_index),
    },
    Step {
        to: 19,
        name: "fold the main-folder table into the set of bound folders, and drop it",
        // `AMB-D-531`. The bindings stop having a main folder: `binding_project_dir` is the whole of
        // them, so every row `binding_path` still holds has to arrive there before the table goes.
        //
        // Seeded, and the seed is not a guess: the two tables held the same fact — this folder is bound
        // to that project — and readers already took their union, so folding one into the other is what
        // that union *was*. `INSERT OR IGNORE` because a folder recorded in both is one folder, and the
        // pair is the key.
        //
        // Both tables are device-local and excluded from `export`, so nothing written out has to be
        // reconciled with this.
        //
        // `CREATE TABLE IF NOT EXISTS` ahead of the fold, as v4's does: the registry no longer declares
        // the table, so open stopped creating it, and a store that never had one then takes the same
        // path out as a store that did.
        apply: Apply::Sql(
            "CREATE TABLE IF NOT EXISTS binding_path (project_id INTEGER PRIMARY KEY, dir TEXT);
             INSERT OR IGNORE INTO binding_project_dir (project_id, dir) \
                 SELECT project_id, dir FROM binding_path WHERE dir IS NOT NULL;
             DROP TABLE binding_path;",
        ),
    },
    Step {
        to: 20,
        name: "add nudge_fired, the log of which nudges have already been put to the person here",
        // `AMB-D-542`. The genesis batch creates a table an older store is missing at open, so this DDL
        // has usually run before the chain reaches here — writing it down anyway is what makes the chain
        // say when the table arrived, as v16's does.
        //
        // **Unseeded, and there is nothing it could be seeded from.** An empty log is exactly the truth
        // about a store upgrading into this: no nudge has ever been put on it, because there was nothing
        // to put one with. Nor would a seed be harmless — a row here is a veto, so inventing one would
        // silence a nudge before it was ever shown.
        apply: Apply::Sql(
            "CREATE TABLE IF NOT EXISTS nudge_fired (\
               nudge_id TEXT PRIMARY KEY NOT NULL, \
               at TEXT NOT NULL\
             );",
        ),
    },
    Step {
        to: 21,
        name: "add task.draft, the fourth premise of ready",
        // `AMB-D-553`. Creation becomes two stages, and which stage a task is at is a premise of `ready`
        // rather than a sixth status — so it arrives as a column on `task`, not as a widened `CHECK`.
        //
        // **Seeded, and the seed is not a guess: `0` on every row.** A task that already exists was
        // written by a build that had no second stage, so its creation is finished by construction —
        // there is no half-built task anywhere in an older store to mistake for one. `NOT NULL DEFAULT 0`
        // is what writes that into every existing row, so there is nothing further to backfill.
        //
        // The column is spelled out here in frozen text, as every step's is: the registry may rename it
        // tomorrow, and what this step added must keep meaning what it meant.
        apply: Apply::Sql(
            "ALTER TABLE task ADD COLUMN draft BOOLEAN NOT NULL DEFAULT 0 CHECK(draft IN (0, 1));",
        ),
    },
    Step {
        to: 22,
        name: "add project_version, the version a project answers a sync with",
        // `AMB-D-582`. The genesis batch creates a table an older store is missing at open, so this DDL
        // has usually run before the chain reaches here — writing it down anyway is what makes the chain
        // say when the table arrived, as v20's does.
        //
        // **Unseeded, and `0` — the absent row — is the honest seed.** The version is the feed id of the
        // last transaction that touched the project, and no build before this one stamped one, so there
        // is nothing in an upgrading store to read it off. Absent reads as `0`, which sits below every id
        // the feed will hand out next: the first write after the upgrade moves the project forward, and
        // whoever carries it out sends the whole thing once. Seeding it with today's feed head would
        // instead claim the project last changed at an instant nothing about it did, and hold back the
        // send that upgrade owes.
        apply: Apply::Sql(
            "CREATE TABLE IF NOT EXISTS project_version (\
               project_id INTEGER PRIMARY KEY REFERENCES project(id) ON DELETE CASCADE NOT NULL, \
               version BIGINT NOT NULL\
             );",
        ),
    },
    Step {
        to: 23,
        name: "add change_feed.project, the window each change belongs to",
        apply: Apply::Custom(add_feed_project),
    },
    Step {
        to: 24,
        name: "let a plugin's gate, settings and secrets sit at the device layer",
        apply: Apply::Custom(open_the_plugin_layer_key),
    },
    Step {
        to: 25,
        name: "give a bound folder an id something else can point at",
        apply: Apply::Custom(key_the_bindings_by_id),
    },
    Step {
        to: 26,
        name: "add task.at_binding_id, the bound folder a task is worked in",
        // `AMB-D-648`. A task may name one of its project's bound folders, and it names it by the id v25
        // gave that row.
        //
        // **Unseeded, and NULL is the only honest seed.** The place is never inferred — not from where a
        // task was filed, not from its title, not from its classification (the decision says so in as
        // many words) — so there is nothing in an existing store to read one off. NULL is what every task
        // written before this means, and what it will go on meaning until somebody says otherwise.
        //
        // The column is spelled out here in frozen text, as every step's is, and carries no `REFERENCES`:
        // the bindings are device-local and `export` leaves them behind, so a constraint would keep a
        // task from travelling (see the registry's note on the column).
        apply: Apply::Sql("ALTER TABLE task ADD COLUMN at_binding_id BIGINT;"),
    },
    Step {
        to: 27,
        name: "add dimension.show_on_card, whether an axis belongs on the task card",
        // `AMB-D-651`. Which axes a board puts on its cards is settled by the axis itself rather than by
        // the device looking at it, so it arrives as a column on `dimension` — one flag per axis, and
        // none per value.
        //
        // **Seeded, and the seed is not a guess: `0` on every row.** The decision starts every axis on
        // the "not shown" side, the one an upgrading store's axes were already read as, so what the
        // cards drew yesterday is what they draw after this runs. `NOT NULL DEFAULT 0` writes that into
        // every existing row and there is nothing further to backfill — the same shape as v21's.
        //
        // The column is spelled out here in frozen text, as every step's is: the registry may rename it
        // tomorrow, and what this step added must keep meaning what it meant.
        apply: Apply::Sql(
            "ALTER TABLE dimension ADD COLUMN show_on_card BOOLEAN NOT NULL DEFAULT 0 \
                 CHECK(show_on_card IN (0, 1));",
        ),
    },
    Step {
        to: 28,
        name: "index the tasks that carry a due day",
        // `AMB-D-718`: the tick's banner asks, on every launch, whether anything is still owed a warning.
        // Unindexed that is one full pass over the task table per launch, heaviest on the store that
        // answers no — the person who never puts a day on anything.
        //
        // **The version is what this step is for.** `due_on` is a genesis column, so the index itself is
        // declared in `schema::EXTRA_SQL` and every open creates it, an existing store's included — a
        // partial index needs no column carried and no row rewritten. What a store cannot do for itself
        // is say which shape it is now in, and the frozen shapes are dated by this chain
        // (`super::schema_frozen`), so moving the genesis DDL is what appends a step here. The statement
        // is repeated in frozen text rather than referenced, as every step's is.
        apply: Apply::Sql(
            "CREATE INDEX IF NOT EXISTS task_by_due ON task(due_on) WHERE due_on IS NOT NULL;",
        ),
    },
    Step {
        to: 29,
        name: "add dimension.required, whether an axis refuses to be left empty",
        // `AMB-D-734`. Whether a task may finish its creation without a value on this axis is the axis's
        // own answer, so it arrives as a column on `dimension` — one flag per axis, and none per value.
        //
        // **Seeded, and the seed is not a guess: `0` on every row.** The decision starts every axis on
        // the "not required" side, which is what an upgrading store's axes were already read as, so no
        // creation that could be finished yesterday is refused after this runs. `NOT NULL DEFAULT 0`
        // writes that into every existing row and there is nothing further to backfill — the same shape
        // as v27's.
        //
        // The column is spelled out here in frozen text, as every step's is: the registry may rename it
        // tomorrow, and what this step added must keep meaning what it meant.
        apply: Apply::Sql(
            "ALTER TABLE dimension ADD COLUMN required BOOLEAN NOT NULL DEFAULT 0 \
                 CHECK(required IN (0, 1));",
        ),
    },
    Step {
        to: 30,
        name: "give a classification axis and its values a slug, unique where each is named",
        apply: Apply::Custom(slug_the_dimension_model),
    },
    Step {
        to: 31,
        name: "add decision_dimension_value, the decision's side of the classification axes",
        // `AMB-D-781`: a decision answers the same axes a task does, with the same values, so the
        // assignment arrives as a table of its own beside `task_dimension_value` rather than as a
        // polymorphic arm on it.
        //
        // **The version is what this step is for**, as v28's is. A whole table is not a column: genesis
        // is `CREATE TABLE IF NOT EXISTS` over the registry and runs at every open, so an existing store
        // grows this table on its next one, with no row to carry and nothing to backfill (there are no
        // rows yet — the decision starts every existing decision unclassified, and classifying one is a
        // deliberate act from here on). What a store cannot do for itself is say which shape it is now
        // in, and the frozen shapes are dated by this chain (`super::schema_frozen`), so moving the
        // genesis DDL is what appends a step here.
        //
        // The DDL is repeated in frozen text rather than referenced, as every step's is: the registry may
        // rename a column tomorrow, and what this step added must keep meaning what it meant.
        apply: Apply::Sql(
            "CREATE TABLE IF NOT EXISTS decision_dimension_value (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL, \
                 decision_id BIGINT NOT NULL DEFAULT 0 REFERENCES decision(id) \
                     ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED, \
                 dimension_id BIGINT NOT NULL DEFAULT 0 REFERENCES dimension(id) \
                     ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED, \
                 value_id BIGINT NOT NULL DEFAULT 0 REFERENCES dimension_value(id) \
                     ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED, \
                 created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB \
                     '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'), \
                 updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB \
                     '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')\
             );\
             CREATE INDEX IF NOT EXISTS decision_dimension_value_by_decision \
                 ON decision_dimension_value(decision_id);",
        ),
    },
    Step {
        to: 32,
        name: "add dimension.applies_to, which of the two entities an axis classifies",
        // `AMB-D-789`. An axis is one mechanism serving tasks and decisions alike, and until this column
        // it served them both whether or not that made sense — a work-shaped axis leaked into every
        // decision pane. Which side it means anything on is the axis's own answer, so it arrives as a
        // column on `dimension`, beside `show_on_card` and `required`.
        //
        // **Seeded, and the seed is the wide side: `both` on every row.** This is where it parts from
        // v27's and v29's shape. Those two start at their column's own `DEFAULT`, so `NOT NULL DEFAULT 0`
        // was the whole backfill; here the default is the `''` every required text column carries — the
        // not-yet-written sentinel, which is not one of the three values a reader may hydrate. So the
        // column is added at the registry's own declaration (the two DDL sites must land on the same
        // table) and every existing row is then written to `both`, which is how every axis in an
        // upgrading store was already read.
        //
        // The column is spelled out here in frozen text, as every step's is: the registry may rename it
        // tomorrow, and what this step added must keep meaning what it meant.
        apply: Apply::Sql(
            "ALTER TABLE dimension ADD COLUMN applies_to TEXT NOT NULL DEFAULT '' \
                 CHECK(applies_to IN ('', 'task', 'decision', 'both'));\
             UPDATE dimension SET applies_to = 'both';",
        ),
    },
    Step {
        to: 33,
        name: "rename the dependency dataset to task_dependency in the change feed",
        // `AMB-D-807`. The registry used to carry a dataset's key and its table as two words, and
        // `task_dependency` was the only entry where they differed: the feed named it `dependency` while
        // the road that reads records back answered under `task_dependency`, so a carrier keyed the same
        // record twice and a delete never landed. Folding the two into one word is what fixes it, and the
        // rows already in the feed still carry the old key.
        //
        // **The rewrite and the switch have to land together.** A feed still saying `dependency` while
        // `sync records` no longer answers to it is the same mismatch pointed the other way, so the
        // rewrite rides in the release that folds the field — not a step of its own ahead of it.
        //
        // The feed is a 5,000-row window and would turn over on its own in a few days; those days are the
        // ones a carrier would lose the edges written in them.
        //
        // Frozen text, like every step's: the registry may rename the dataset again tomorrow, and what
        // this step rewrote must keep meaning what it meant.
        apply: Apply::Sql(
            "UPDATE change_feed SET dataset = 'task_dependency' WHERE dataset = 'dependency';",
        ),
    },
    Step {
        to: 34,
        name: "widen dimension.cardinality by one value, so an axis can admit several at once",
        apply: Apply::Custom(admit_multi_cardinality),
    },
    Step {
        to: 35,
        name: "let an axis's values be closed: widen dimension.role, and give dimension_value its flag",
        apply: Apply::Custom(let_a_value_be_closed),
    },
    Step {
        to: 36,
        name: "give the project row its icon, and the hash of the original that icon was baked from",
        // `AMB-D-839`: an image a human registers is kept twice — the small version everything displays,
        // and the original, so a different size can be baked later without asking for the file again. The
        // facet avatars keep the pair in config; a project keeps it on its own row, which is what these
        // two columns are. Both are nullable and seeded at null: no project could have had an icon before
        // this, so "has none" is the truth about every existing row, not a gap to fill in.
        //
        // The declarations are spelled out here in frozen text, as every step's are — the registry may
        // rename or reshape the columns tomorrow; what this step added must keep meaning what it meant.
        apply: Apply::Sql(
            "ALTER TABLE project ADD COLUMN icon TEXT;
             ALTER TABLE project ADD COLUMN icon_source TEXT;",
        ),
    },
    Step {
        to: 37,
        name: "add secret, where Amenbo's own features keep a credential",
        // `AMB-D-884`: mail, Slack and the Viewer become the body's own features, and the table their
        // secrets lived in (`plugin_secret`) goes with the mechanism that held them. `config.json` says of
        // itself that it holds no secrets, so this is the body's first place for one.
        //
        // **The version is what this step is for**, as v31's is. A whole table is not a column: genesis is
        // `CREATE TABLE IF NOT EXISTS` over the registry and runs at every open, so an existing store grows
        // this table on its next one, with nothing to backfill — no build before this one wrote a row that
        // belongs here, and the migration off the plugins is a program of its own rather than a step in
        // this chain. What a store cannot do for itself is say which shape it is now in, and the frozen
        // shapes are dated by this chain (`super::schema_frozen`), so moving the genesis DDL is what
        // appends a step here.
        //
        // The DDL is repeated in frozen text rather than referenced, as every step's is: the registry may
        // rename a column tomorrow, and what this step added must keep meaning what it meant.
        apply: Apply::Sql(
            "CREATE TABLE IF NOT EXISTS secret (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL, \
                 project_id BIGINT REFERENCES project(id) \
                     ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED, \
                 area TEXT NOT NULL DEFAULT '' CHECK(area IN ('', 'notify', 'viewer')), \
                 owner_id BIGINT, \
                 field_key TEXT NOT NULL DEFAULT '', \
                 value TEXT NOT NULL DEFAULT '', \
                 created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB \
                     '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'), \
                 updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB \
                     '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')\
             );\
             CREATE UNIQUE INDEX IF NOT EXISTS secret_address \
                 ON secret(project_id, area, COALESCE(owner_id, 0), field_key);\
             CREATE UNIQUE INDEX IF NOT EXISTS secret_address_device \
                 ON secret(area, COALESCE(owner_id, 0), field_key) WHERE project_id IS NULL;",
        ),
    },
    Step {
        to: 38,
        name: "add the notification tables — the device's targets, and each project's row",
        // `AMB-D-885`: mail and Slack become one feature, whose connections sit on the device under a name
        // and whose projects select from them. Four tables' worth of settings had nowhere to live: the
        // plugins kept theirs in `plugin_config`, which goes with the mechanism.
        //
        // **The version is what this step is for**, exactly as v37's is. Genesis is
        // `CREATE TABLE IF NOT EXISTS` over the registry and runs at every open, so an existing store grows
        // these tables on its next one with nothing to backfill — carrying what the plugins wrote across is
        // a program of its own, not a step in this chain. What a store cannot do for itself is say which
        // shape it is now in, and the frozen shapes are dated here (`super::schema_frozen`).
        //
        // The DDL is repeated in frozen text rather than referenced, as every step's is: the registry may
        // rename a column tomorrow, and what this step added must keep meaning what it meant.
        apply: Apply::Sql(
            "CREATE TABLE IF NOT EXISTS notify_target (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL, \
                 kind TEXT NOT NULL DEFAULT '' CHECK(kind IN ('', 'slack', 'mail')), \
                 name TEXT NOT NULL DEFAULT '', \
                 is_default BOOLEAN NOT NULL DEFAULT 0 CHECK(is_default IN (0, 1)), \
                 smtp_host TEXT, \
                 smtp_port BIGINT, \
                 smtp_user TEXT, \
                 mail_from TEXT, \
                 created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB \
                     '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'), \
                 updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB \
                     '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')\
             );\
             CREATE TABLE IF NOT EXISTS project_notify (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL, \
                 project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) \
                     ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED, \
                 enabled BOOLEAN NOT NULL DEFAULT 0 CHECK(enabled IN (0, 1)), \
                 mail_to TEXT NOT NULL DEFAULT '', \
                 created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB \
                     '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'), \
                 updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB \
                     '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'), \
                 UNIQUE (project_id)\
             );\
             CREATE TABLE IF NOT EXISTS project_notify_target (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL, \
                 project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) \
                     ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED, \
                 target_id BIGINT NOT NULL DEFAULT 0 REFERENCES notify_target(id) \
                     ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED, \
                 created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB \
                     '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'), \
                 updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB \
                     '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'), \
                 UNIQUE (project_id, target_id)\
             );\
             CREATE TABLE IF NOT EXISTS project_notify_event (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL, \
                 project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) \
                     ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED, \
                 event TEXT NOT NULL DEFAULT '' CHECK(event IN ('', 'task.created', \
                     'task.status_changed', 'task.done', 'task.rejected', 'task.assigned', \
                     'task.moved', 'task.deleted', 'decision.accepted', 'decision.rejected', \
                     'comment.added', 'comment.removed', 'task.due', 'task.due_tomorrow')), \
                 created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB \
                     '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'), \
                 updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB \
                     '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'), \
                 UNIQUE (project_id, event)\
             );\
             CREATE INDEX IF NOT EXISTS project_notify_target_by_target \
                 ON project_notify_target(target_id);",
        ),
    },
    Step {
        to: 39,
        name: "add the Viewer's carrier tables — where it was left, and what it has yet to send",
        // `AMB-D-884`: the carrier that puts the backlog in the reader's own Cloudflare account becomes the
        // body's own, and what it was left holding came with it. A plugin kept that in a file beside its
        // binary; there is no such file any more, and no `plugin_config` to put it in either.
        //
        // **The version is what this step is for**, as v37's and v38's are. Genesis is
        // `CREATE TABLE IF NOT EXISTS` over the registry and runs at every open, so an existing store grows
        // these on its next one. There is nothing to backfill and nothing that could be: a carrier with no
        // memory places the whole backlog, which is what a first run does — and that is the honest state of
        // a store upgrading into this, since nothing has ever been sent from it.
        //
        // The DDL is repeated in frozen text rather than referenced, as every step's is: the registry may
        // rename a column tomorrow, and what this step added must keep meaning what it meant.
        apply: Apply::Sql(
            "CREATE TABLE IF NOT EXISTS viewer_send (\
                 id INTEGER PRIMARY KEY CHECK (id = 1) NOT NULL, \
                 version BIGINT NOT NULL, \
                 cursor BIGINT NOT NULL, \
                 placed BIGINT NOT NULL, \
                 seq BIGINT NOT NULL, \
                 quiet_until TEXT, \
                 spent BIGINT NOT NULL, \
                 spent_on TEXT, \
                 build BIGINT NOT NULL\
             );\
             CREATE TABLE IF NOT EXISTS viewer_pending (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL, \
                 record_key TEXT NOT NULL, \
                 op TEXT CHECK(op IN ('put', 'del')) NOT NULL, \
                 body TEXT\
             );",
        ),
    },
    Step {
        to: 40,
        name: "add what the Viewer's repair last counted, so the press after it is the one that spends it",
        // `AMB-D-884` and `AMB-D-814`: comparing the server with this machine is cheap and placing the
        // difference is not, so the first press counts and the second places. The two presses are two runs
        // of a process that remembers nothing, and this row is what stands between them.
        //
        // It is a table of its own rather than three more columns on `viewer_send`: a turn reads that row,
        // sends, and writes it back, so a count landing inside one would be written over by the turn that
        // never saw it.
        //
        // **The version is what this step is for**, as v39's is. Genesis is `CREATE TABLE IF NOT EXISTS`
        // over the registry and runs at every open, so an existing store grows this on its next one. There
        // is nothing to backfill: a store upgrading into this has been shown no count, which is what an
        // absent row says.
        apply: Apply::Sql(
            "CREATE TABLE IF NOT EXISTS viewer_asked (\
                 id INTEGER PRIMARY KEY CHECK (id = 1) NOT NULL, \
                 asked_at TEXT NOT NULL, \
                 to_place BIGINT NOT NULL, \
                 to_drop BIGINT NOT NULL\
             );",
        ),
    },
    Step {
        to: 41,
        name: "add the Viewer's switch, and when it last placed anything",
        // `AMB-D-884`: the screen says whether this device is carrying and when it last did, and neither
        // was anywhere to be read. The switch is a table of its own because standing a server up throws
        // the carrier's row away — a switch that came back on because somebody pressed setup is a switch
        // nobody threw.
        //
        // **The column is added only where it is missing, and that is not belt and braces.** Genesis is
        // `CREATE TABLE IF NOT EXISTS` over today's registry and runs before this chain on every open, so
        // a store from before `viewer_send` existed at all has the table created here complete — column
        // and all — and a plain `ALTER TABLE … ADD COLUMN` would then fail on a column already there.
        // Every earlier `ADD COLUMN` step in this chain alters a table that exists in the oldest store
        // this build opens, so none of them could meet that; this is the first to alter one the chain
        // itself introduced.
        apply: Apply::Custom(add_the_viewers_switch),
    },
    Step {
        to: 42,
        name: "carry what the four official plugins held into the body, and take them away",
        apply: Apply::Custom(carry_the_official_plugins_into_the_body),
    },
    Step {
        to: 43,
        name: "drop the plugin mechanism's tables — nothing reads or writes them any more",
        // `AMB-D-884`: the mechanism is gone, and with it every reader and every writer of these five.
        // v42 above already moved what the four official plugins held into the body and emptied them, so
        // what is dropped here is either nothing or a third party's rows, which no build after this one
        // can run anything with.
        //
        // The indexes go with their tables — SQLite drops an index when the table under it goes — so
        // naming them would be naming what is already gone.
        //
        // **`plugin_outbox` stays**, name and all: the drive at the write seam walks it for the
        // notifications (`AMB-D-901`), and only its spelling is the mechanism's. v44 below is where that
        // spelling goes.
        //
        // The execution log goes here too — a file rather than a table, which is why this is not SQL.
        apply: Apply::Custom(drop_the_plugin_mechanism),
    },
    Step {
        to: 44,
        name: "rename the outbox and its three keys — the mechanism they were spelled for is gone",
        // `AMB-D-901` left the spelling where it was: the names were stored data, and the mechanism they
        // came from was still standing. `AMB-T-4770` took it away, so `plugin_outbox`,
        // `plugin_dispatch_cursor`, `plugin_dispatch_cursor_face` and `plugin_outbox_truncated_through`
        // now name a reader that does not exist. The table and the three keys are the store's, and this
        // is what moves them.
        apply: Apply::Custom(rename_the_outbox),
    },
    Step {
        to: 45,
        name: "draw each pane's id afresh as a UUID, and move the homes held under the old ones",
        // `AMB-D-897`: a counted id is only as unique as the count it came from, and that count sat
        // in the same row as the panes — so a row that would not parse took the count down with it
        // and the next run began at "1" again, onto ids a name and a way back into a session were
        // already held against (`AMB-T-4843`).
        apply: Apply::Custom(draw_the_pane_ids_afresh),
    },
    Step {
        to: 46,
        name: "add task_made_in and decision_made_in, the session a task or a decision was made in",
        // `AMB-D-897`. The genesis batch creates a table an older store is missing at open, so this DDL
        // has usually run before the chain reaches here — writing it down anyway is what makes the chain
        // say when the tables arrived, rather than leaving a reader of the frozen shapes to guess.
        //
        // **Unseeded, and there is nothing to seed them from.** What pane a task was made in was never
        // written anywhere, so every task that already exists has no row here — which is the same thing
        // a task made outside the talk window has, and reads the same way.
        apply: Apply::Sql(
            "CREATE TABLE IF NOT EXISTS task_made_in (\
               id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL, \
               task_id BIGINT NOT NULL DEFAULT 0 REFERENCES task(id) \
                 ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED, \
               pane TEXT NOT NULL DEFAULT '', \
               pane_name TEXT, \
               pane_resume TEXT, \
               created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'), \
               updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')\
             );
             CREATE UNIQUE INDEX IF NOT EXISTS task_made_in_by_task ON task_made_in(task_id);
             CREATE TABLE IF NOT EXISTS decision_made_in (\
               id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL, \
               decision_id BIGINT NOT NULL DEFAULT 0 REFERENCES decision(id) \
                 ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED, \
               pane TEXT NOT NULL DEFAULT '', \
               pane_name TEXT, \
               pane_resume TEXT, \
               created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'), \
               updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')\
             );
             CREATE UNIQUE INDEX IF NOT EXISTS decision_made_in_by_decision \
               ON decision_made_in(decision_id);",
        ),
    },
    Step {
        to: 47,
        name: "add decision.draft, the premise that says the writing is not finished",
        // `AMB-D-918`. A decision under discussion stops being a status and becomes a flag, the shape
        // `task.draft` has carried since v21 — so it arrives as a column on `decision`, not as a
        // widened `CHECK`. `status` is left exactly where it is; the step that folds `proposed` away
        // comes later.
        //
        // **Seeded, and the seed is not a guess: `0` on every row.** A decision that already exists
        // was written by a build with no second stage, so its writing is finished by construction —
        // there is no half-written decision anywhere in an older store to mistake for one.
        // `NOT NULL DEFAULT 0` is what writes that into every existing row, so there is nothing
        // further to backfill.
        apply: Apply::Sql(
            "ALTER TABLE decision ADD COLUMN draft BOOLEAN NOT NULL DEFAULT 0 CHECK(draft IN (0, 1));",
        ),
    },
    Step {
        to: 48,
        name: "fold decision.proposed away, and rename accepted to decided",
        apply: Apply::Custom(fold_the_proposal_away),
    },
    Step {
        to: 49,
        name: "put the split a project was held at onto each of that project's panes, as a size",
        // `AMB-D-939`: a pane holds an order and a size, and the pages fall out of laying them down.
        // The split was one answer for a whole project, so it is read once here and written onto the
        // panes — after which nothing in the build knows what a count was. Reading both shapes
        // instead would leave every layer to decide which one it writes in, which is the split the
        // decision is about.
        apply: Apply::Custom(lay_the_panes_out_by_size),
    },
    Step {
        to: 50,
        name: "lay the automation tables down, and let a comment and an attachment name a step of a run",
        apply: Apply::Custom(lay_the_automation_tables_down),
    },
    Step {
        to: 51,
        name: "take the queue out of automation_run.status",
        apply: Apply::Custom(take_the_run_queue_away),
    },
    Step {
        to: 52,
        name: "admit no_way_on as a reason a run stopped",
        apply: Apply::Custom(admit_the_run_with_nowhere_to_go),
    },
    Step {
        to: 53,
        name: "fold the automation definition into three layers — automation, action, step",
        apply: Apply::Custom(fold_the_automation_into_three_layers),
    },
    Step {
        to: 54,
        name: "rename automation_step to automation_action_step, after the owner it now hangs on",
        apply: Apply::Custom(name_the_step_after_its_action),
    },
    Step {
        to: 55,
        name: "join what an action declares to the step inside it",
        apply: Apply::Custom(join_the_action_to_its_step),
    },
    Step {
        to: 56,
        name: "drop automation_step_note, the last name v50 lays down that no registry has",
        // `AMB-D-949`. v50 lays `automation_step_note` down in frozen text, and v53's fold drops it
        // wherever the fold runs. The fold is skipped whole on a store born below v50 — genesis
        // handed that store today's registry, which has no such table — so there v50's name is laid
        // down over a store that never asked for it and stays, with nothing in the build that reads
        // it. `automation_step` was the other name of that pair, and v54's rename cleared it away.
        //
        // **Empty by construction, which is why the drop is the whole of the step.** No op has ever
        // known this name on that path: the registry did not carry it when the store was born, and
        // no op runs during a migration. Nothing is rewritten in `change_feed` for the same reason —
        // a dataset key that was never handed out leaves no row for a carrier to be served. It is
        // also what makes the `REFERENCES` the table still spells at `automation_step`, a name v54
        // took away, cost nothing: dropping an empty table deletes no row for a foreign key to be
        // checked against.
        apply: Apply::Sql("DROP TABLE IF EXISTS automation_step_note;"),
    },
    Step {
        to: 57,
        name: "take the preamble off an automation — the build writes it now",
        apply: Apply::Custom(take_the_preamble_off_the_definition),
    },
    Step {
        to: 58,
        name: "wire what a lone step hands on out to the action it is inside",
        apply: Apply::Custom(wire_the_step_s_outputs_out_to_the_action),
    },
    Step {
        to: 59,
        name: "drop the shared documents, and give an action a note of its own",
        apply: Apply::Custom(take_the_shared_documents_away),
    },
    Step {
        to: 60,
        name: "say how a run ended — completed, failed or canceled — instead of done or stopped",
        apply: Apply::Custom(say_how_a_run_ended),
    },
    Step {
        to: 61,
        name: "let a person say they have seen a failed run",
        apply: Apply::Custom(let_a_failure_be_acknowledged),
    },
    Step {
        to: 62,
        name: "key the ways out — lines, the run's copies and its records name a way out by its row",
        apply: Apply::Custom(key_the_ways_out),
    },
    Step {
        to: 63,
        name: "copy the wires into a run's copy of each step it can still open",
        apply: Apply::Custom(copy_the_wires_into_the_run),
    },
    Step {
        to: 64,
        name: "choose a step's agent and model where its action is placed, not on the step",
        apply: Apply::Custom(choose_the_agent_where_the_action_is_placed),
    },
    Step {
        to: 65,
        name: "key the ports — wires, the run's copies and its values name a port by its row",
        apply: Apply::Custom(key_the_ports),
    },
    Step {
        to: 66,
        name: "copy what follows each way out into a run's copies, and mark the one it starts at",
        apply: Apply::Custom(copy_the_lines_into_the_run),
    },
    Step {
        to: 67,
        name: "add the task comment's intent column, seeded from when each row was written",
        // `AMB-D-963`. A comment posted while a task is held is shown to the holder when they close it,
        // and "while held" is a comparison against the task's `status_changed_at` — which a comment had
        // no intent column to stand on (`AMB-D-372` keeps `created_at` out of it). The seed is
        // `created_at` for the reason v7's and v8's are: a comment row is written once and an edit
        // moves `updated_at` and `edited_at`, never the instant it was posted.
        //
        // `NULLIF` guards the `''` a row caught mid-create carries: the column's `CHECK` admits an instant
        // or NULL, and `''` is neither. Spelled in frozen text, as every step's is.
        apply: Apply::Sql(
            "ALTER TABLE task_comment ADD COLUMN posted_at TEXT \
                 CHECK(posted_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z');
             UPDATE task_comment SET posted_at = NULLIF(created_at, '');",
        ),
    },
];

/// v66: a run's copy of a step holds what follows each of its ways out, and the copy the run starts at
/// is marked (`AMB-D-961`).
///
/// A run read the edges live while it moved — what follows a way out, which of the action's ways out a
/// line inside it returns to, and where the run starts. A launch now resolves them into the copies:
/// each way out in `automation_run_def.exits` carries the line walked after it (`then`) and the action's
/// way out it returns to (`returns_to`), and `entry` marks the copy the run starts at.
///
/// **Only a run that can still open a step is filled**, `running` or `paused`, for v63's reason: a run
/// that has ended opens nothing, and today's picture is not the one it ran with.
///
/// **What is filled is today's picture**, followed the way a launch follows it: the line inside the
/// action, and — where that returns to one of the action's ways out — the automation's line from that
/// way out on the placement, going on to the step the next placement's action opens first. A build that
/// can write v62 already refuses to edit a definition a run is using, so today's picture is the one the
/// run launched with.
///
/// **The column is appended only where it is missing**, v53's guard and for its reason.
fn copy_the_lines_into_the_run(ctx: &Ctx<'_>) -> Result<()> {
    let tx = ctx.tx;
    if !column_names(tx, "automation_run_def")?.iter().any(|c| c == "entry") {
        tx.execute_batch(
            "ALTER TABLE automation_run_def ADD COLUMN entry BOOLEAN NOT NULL DEFAULT 0 CHECK(entry IN (0, 1));",
        )?;
    }
    let mut copies: Vec<(i64, i64, i64, i64, String)> = Vec::new();
    {
        let mut stmt = tx.prepare(
            "SELECT d.id, r.automation_id, d.placement_id, d.step_id, d.exits FROM automation_run_def d
               JOIN automation_run r ON r.id = d.run_id
              WHERE r.status IN ('running', 'paused')
                AND d.placement_id IS NOT NULL AND d.step_id IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
        for row in rows {
            copies.push(row?);
        }
    }
    // (id, ends, to_id, exit_to_id, max_times) of the line one box's way out has on one picture.
    type Line = (i64, String, Option<i64>, Option<i64>, Option<i64>);
    let line_for = |owner_kind: &str, from_id: i64, exit_id: i64| -> Result<Option<Line>> {
        Ok(tx
            .query_row(
                "SELECT id, ends, to_id, exit_to_id, max_times FROM automation_edge
                  WHERE owner_kind = ?1 AND from_id = ?2 AND exit_id = ?3 ORDER BY id LIMIT 1",
                rusqlite::params![owner_kind, from_id, exit_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?)
    };
    // The step one placement's action opens first.
    let opens_first = |placement_id: i64| -> Result<Option<i64>> {
        Ok(tx
            .query_row(
                "SELECT a.entry_step_id FROM automation_placement p
                   JOIN automation_action a ON a.id = p.action_id WHERE p.id = ?1",
                rusqlite::params![placement_id],
                |r| r.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten())
    };
    let kept = |picture: &str, from_id: i64, exit_id: i64, line: &Line, placement: Option<i64>, step: Option<i64>| {
        serde_json::json!({
            "edge_id": line.0,
            "picture": picture,
            "from_id": from_id,
            "exit_id": exit_id,
            "ends": line.1,
            "placement_id": placement,
            "step_id": step,
            "max_times": line.4,
        })
    };
    for (def_id, automation_id, placement_id, step_id, exits) in copies {
        let Ok(mut exits) = serde_json::from_str::<Vec<serde_json::Value>>(&exits) else { continue };
        let mut changed = false;
        for exit in exits.iter_mut() {
            let Some(map) = exit.as_object_mut() else { continue };
            if map.contains_key("then") || map.contains_key("returns_to") {
                continue;
            }
            let Some(exit_id) = map.get("id").and_then(|i| i.as_i64()) else { continue };
            let Some(inner) = line_for("action", step_id, exit_id)? else { continue };
            let (then, returns_to) = if inner.1 != "exit" {
                let (placement, step) = match (inner.1.as_str(), inner.2) {
                    ("go", Some(to)) => (Some(placement_id), Some(to)),
                    _ => (None, None),
                };
                (Some(kept("action", step_id, exit_id, &inner, placement, step)), None)
            } else {
                let Some(action_exit) = inner.3 else { continue };
                let then = match line_for("automation", placement_id, action_exit)? {
                    Some(outer) if outer.1 != "exit" => {
                        let (placement, step) = match (outer.1.as_str(), outer.2) {
                            ("go", Some(to)) => (Some(to), opens_first(to)?),
                            _ => (None, None),
                        };
                        Some(kept("automation", placement_id, action_exit, &outer, placement, step))
                    }
                    _ => None,
                };
                (then, Some(action_exit))
            };
            if let Some(then) = then {
                map.insert("then".to_string(), then);
            }
            if let Some(returns_to) = returns_to {
                map.insert("returns_to".to_string(), serde_json::Value::from(returns_to));
            }
            changed = true;
        }
        if changed {
            tx.execute(
                "UPDATE automation_run_def SET exits = ?1 WHERE id = ?2",
                rusqlite::params![serde_json::Value::Array(exits).to_string(), def_id],
            )?;
        }
        let entry: Option<i64> = tx
            .query_row(
                "SELECT entry_placement_id FROM automation WHERE id = ?1",
                rusqlite::params![automation_id],
                |r| r.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten();
        if entry == Some(placement_id) && opens_first(placement_id)? == Some(step_id) {
            tx.execute("UPDATE automation_run_def SET entry = 1 WHERE id = ?1", rusqlite::params![def_id])?;
        }
    }
    Ok(())
}

/// v63: a run's copy of a step holds the wires joined to each of its inputs (`AMB-D-961`).
///
/// A run read the wires live while it moved, the one part of the definition it did not copy at launch.
/// A launch now resolves them into `automation_run_def.ins` — each input carries the step outputs wired
/// into it — and opening a step reads only that.
///
/// **Only a run that can still open a step is filled**, `running` or `paused`. A run that has ended
/// opens nothing, and the wires standing today are not the ones it ran with, so writing them in would
/// make its copy say something that was never true. Those copies keep inputs with nothing wired, which
/// no reader asks of them.
///
/// **What is filled is today's picture**, followed the way a launch follows it: a wire inside the action
/// from one of its steps, or a wire from the action's own input out across the automation to the step
/// behind the far placement's way out. A build that can write v62 already refuses to edit a definition
/// a run is using, so for a run started under it today's picture is the one it launched with.
///
/// **Probed, not bare**: an input that already names its sources was copied by a launch, and is left.
fn copy_the_wires_into_the_run(ctx: &Ctx<'_>) -> Result<()> {
    let tx = ctx.tx;
    let mut copies: Vec<(i64, i64, i64, String)> = Vec::new();
    {
        let mut stmt = tx.prepare(
            "SELECT d.id, d.placement_id, d.step_id, d.ins FROM automation_run_def d
               JOIN automation_run r ON r.id = d.run_id
              WHERE r.status IN ('running', 'paused')
                AND d.placement_id IS NOT NULL AND d.step_id IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        for row in rows {
            copies.push(row?);
        }
    }
    let action_of = |placement_id: i64| -> Result<Option<i64>> {
        Ok(tx
            .query_row(
                "SELECT action_id FROM automation_placement WHERE id = ?1",
                rusqlite::params![placement_id],
                |r| r.get(0),
            )
            .optional()?)
    };
    // (from_id, from_exit_id, from_port_name) of the wires in one picture that end at one port.
    let wires_to = |owner_kind: &str,
                    owner_id: Option<i64>,
                    to_id: i64,
                    port: &str|
     -> Result<Vec<(i64, Option<i64>, String)>> {
        let mut stmt = tx.prepare(
            "SELECT from_id, from_exit_id, from_port_name FROM automation_wire
              WHERE owner_kind = ?1 AND (?2 IS NULL OR owner_id = ?2) AND to_id = ?3 AND to_port_name = ?4
              ORDER BY id",
        )?;
        let rows = stmt.query_map(rusqlite::params![owner_kind, owner_id, to_id, port], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    };
    for (def_id, placement_id, step_id, ins) in copies {
        let Ok(mut ins) = serde_json::from_str::<Vec<serde_json::Value>>(&ins) else { continue };
        let Some(action_id) = action_of(placement_id)? else { continue };
        let mut changed = false;
        for input in ins.iter_mut() {
            let Some(map) = input.as_object_mut() else { continue };
            if map.contains_key("from") {
                continue;
            }
            let Some(name) = map.get("name").and_then(|n| n.as_str()).map(str::to_string) else { continue };
            let mut from = Vec::new();
            for (from_id, exit, port) in wires_to("action", Some(action_id), step_id, &name)? {
                if from_id != 0 {
                    from.push(serde_json::json!({
                        "placement_id": placement_id, "step_id": from_id, "exit_id": exit, "port": port,
                    }));
                    continue;
                }
                for (far_id, far_exit, far_port) in wires_to("automation", None, placement_id, &port)? {
                    let Some(far_action) = action_of(far_id)? else { continue };
                    let inner = wires_to("action", Some(far_action), 0, &far_port)?;
                    for (inner_from, inner_exit, inner_port) in inner {
                        let Some(inner_exit) = inner_exit else { continue };
                        let leaves_by: Option<i64> = tx
                            .query_row(
                                "SELECT ends, exit_to_id FROM automation_edge
                                  WHERE owner_kind = 'action' AND from_id = ?1 AND exit_id = ?2
                                  ORDER BY id LIMIT 1",
                                rusqlite::params![inner_from, inner_exit],
                                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?)),
                            )
                            .optional()?
                            .and_then(|(ends, to)| if ends == "exit" { to } else { None });
                        if leaves_by.is_some() && leaves_by == far_exit {
                            from.push(serde_json::json!({
                                "placement_id": far_id, "step_id": inner_from,
                                "exit_id": inner_exit, "port": inner_port,
                            }));
                        }
                    }
                }
            }
            map.insert("from".to_string(), serde_json::Value::Array(from));
            changed = true;
        }
        if changed {
            tx.execute(
                "UPDATE automation_run_def SET ins = ?1 WHERE id = ?2",
                rusqlite::params![serde_json::Value::Array(ins).to_string(), def_id],
            )?;
        }
    }
    Ok(())
}

/// v62: a way out is keyed, not named (`AMB-D-961`).
///
/// An edge named the way out it hangs on and the way out of the action it returns to, a wire the way out
/// it leaves by, and a run's records the way out a step left by — all by name. So renaming a way out
/// parted every line on it, and the unnamed way out had no name a step could type. Each of them now
/// holds the `automation_exit` row's id, and a run's copy of a step (`automation_run_def.exits`) holds
/// the id beside the name, which is what the run's records are read against.
///
/// **What cannot be keyed goes.** An edge or a wire whose name no way out of its box carries any more
/// was already parted — it decided nothing and carried nothing — and a key has nowhere to point it. An
/// `exit` edge whose action no longer declares the way out it returns to keeps its row with no key, and
/// reads as leaving the action by nothing, which is what it did.
///
/// **A copy whose step is gone keeps its ways out under ids of its own.** Nothing live answers for
/// those names, but the run's records still say which of them a step left by, so each is numbered
/// below zero — an id no row can have — and the records key to that.
///
/// **In a record, no name was two things.** A step that had finished and a value it handed on with no
/// name left by the unnamed way out; a step still running had left by nothing yet. Only `done` and
/// `running` are ever written for a step, so the status says which.
///
/// **Probed, not bare**, table by table: a store born from today's registry has the keys and never had
/// the names, and arrives here with nothing to move.
fn key_the_ways_out(ctx: &Ctx<'_>) -> Result<()> {
    let tx = ctx.tx;
    let has = |table: &str, column: &str| -> Result<bool> {
        Ok(column_names(tx, table)?.iter().any(|c| c == column))
    };

    if has("automation_edge", "exit_name")? {
        if !has("automation_edge", "exit_id")? {
            tx.execute_batch("ALTER TABLE automation_edge ADD COLUMN exit_id BIGINT NOT NULL DEFAULT 0;")?;
        }
        if !has("automation_edge", "exit_to_id")? {
            tx.execute_batch("ALTER TABLE automation_edge ADD COLUMN exit_to_id BIGINT;")?;
        }
        tx.execute_batch(
            "UPDATE automation_edge SET exit_id = COALESCE((
                 SELECT x.id FROM automation_exit x
                   JOIN automation_placement p ON p.action_id = x.owner_id
                  WHERE x.owner_kind = 'action' AND p.id = automation_edge.from_id
                    AND x.name IS automation_edge.exit_name), 0)
              WHERE owner_kind = 'automation';
             UPDATE automation_edge SET exit_id = COALESCE((
                 SELECT x.id FROM automation_exit x
                  WHERE x.owner_kind = 'step' AND x.owner_id = automation_edge.from_id
                    AND x.name IS automation_edge.exit_name), 0)
              WHERE owner_kind = 'action';
             UPDATE automation_edge SET exit_to_id = (
                 SELECT x.id FROM automation_exit x
                  WHERE x.owner_kind = 'action' AND x.owner_id = automation_edge.owner_id
                    AND x.name IS automation_edge.exit_to)
              WHERE ends = 'exit';
             DELETE FROM automation_edge WHERE exit_id = 0;
             ALTER TABLE automation_edge DROP COLUMN exit_name;
             ALTER TABLE automation_edge DROP COLUMN exit_to;",
        )?;
    }

    if has("automation_wire", "from_exit_name")? {
        if !has("automation_wire", "from_exit_id")? {
            tx.execute_batch("ALTER TABLE automation_wire ADD COLUMN from_exit_id BIGINT;")?;
        }
        tx.execute_batch(
            "UPDATE automation_wire SET from_exit_id = (
                 SELECT x.id FROM automation_exit x
                   JOIN automation_placement p ON p.action_id = x.owner_id
                  WHERE x.owner_kind = 'action' AND p.id = automation_wire.from_id
                    AND x.name IS automation_wire.from_exit_name)
              WHERE owner_kind = 'automation';
             UPDATE automation_wire SET from_exit_id = (
                 SELECT x.id FROM automation_exit x
                  WHERE x.owner_kind = 'step' AND x.owner_id = automation_wire.from_id
                    AND x.name IS automation_wire.from_exit_name)
              WHERE owner_kind = 'action' AND from_id <> 0;
             DELETE FROM automation_wire WHERE from_id <> 0 AND from_exit_id IS NULL;
             ALTER TABLE automation_wire DROP COLUMN from_exit_name;",
        )?;
    }

    // The run's copies: an id beside each way out's name, from the live row where the step still has
    // it, and below zero where it does not.
    let mut copies: Vec<(i64, Option<i64>, String)> = Vec::new();
    {
        let mut stmt = tx.prepare("SELECT id, step_id, exits FROM automation_run_def")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        for row in rows {
            copies.push(row?);
        }
    }
    // (copy id, name) → the id the copy now keys that way out by.
    let mut keyed: std::collections::HashMap<(i64, Option<String>), i64> = std::collections::HashMap::new();
    for (def_id, step_id, exits) in copies {
        let Ok(mut exits) = serde_json::from_str::<Vec<serde_json::Value>>(&exits) else { continue };
        let mut changed = false;
        let mut stand_in = -1;
        for exit in exits.iter_mut() {
            let name = exit.get("name").and_then(|n| n.as_str()).map(str::to_string);
            let id = match exit.get("id").and_then(|i| i.as_i64()) {
                Some(id) => id,
                None => {
                    let live: Option<i64> = match step_id {
                        Some(step_id) => tx
                            .query_row(
                                "SELECT id FROM automation_exit \
                                  WHERE owner_kind = 'step' AND owner_id = ?1 AND name IS ?2",
                                rusqlite::params![step_id, name],
                                |r| r.get(0),
                            )
                            .optional()?,
                        None => None,
                    };
                    let id = match live {
                        Some(id) => id,
                        None => {
                            stand_in -= 1;
                            stand_in + 1
                        }
                    };
                    if let Some(map) = exit.as_object_mut() {
                        map.insert("id".to_string(), serde_json::Value::from(id));
                    }
                    changed = true;
                    id
                }
            };
            keyed.insert((def_id, name), id);
        }
        if changed {
            tx.execute(
                "UPDATE automation_run_def SET exits = ?1 WHERE id = ?2",
                rusqlite::params![serde_json::Value::Array(exits).to_string(), def_id],
            )?;
        }
    }

    if has("automation_run_step", "exit_name")? {
        if !has("automation_run_step", "exit_id")? {
            tx.execute_batch("ALTER TABLE automation_run_step ADD COLUMN exit_id BIGINT;")?;
        }
        let mut steps: Vec<(i64, i64, Option<String>)> = Vec::new();
        {
            let mut stmt = tx.prepare(
                "SELECT id, run_def_id, exit_name FROM automation_run_step WHERE status = 'done'",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            for row in rows {
                steps.push(row?);
            }
        }
        for (id, def_id, name) in steps {
            if let Some(exit) = keyed.get(&(def_id, name)) {
                tx.execute(
                    "UPDATE automation_run_step SET exit_id = ?1 WHERE id = ?2",
                    rusqlite::params![exit, id],
                )?;
            }
        }
    }

    if has("automation_run_value", "exit_name")? {
        if !has("automation_run_value", "exit_id")? {
            tx.execute_batch("ALTER TABLE automation_run_value ADD COLUMN exit_id BIGINT;")?;
        }
        let mut values: Vec<(i64, i64, Option<String>)> = Vec::new();
        {
            let mut stmt = tx.prepare(
                "SELECT v.id, s.run_def_id, v.exit_name FROM automation_run_value v
                   JOIN automation_run_step s ON s.id = v.run_step_id
                  WHERE v.direction = 'out' AND (v.exit_name IS NOT NULL OR s.status = 'done')",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            for row in rows {
                values.push(row?);
            }
        }
        for (id, def_id, name) in values {
            if let Some(exit) = keyed.get(&(def_id, name)) {
                tx.execute(
                    "UPDATE automation_run_value SET exit_id = ?1 WHERE id = ?2",
                    rusqlite::params![exit, id],
                )?;
            }
        }
    }

    // The names go last: every read above is from them.
    if has("automation_run_step", "exit_name")? {
        tx.execute_batch("ALTER TABLE automation_run_step DROP COLUMN exit_name;")?;
    }
    if has("automation_run_value", "exit_name")? {
        tx.execute_batch("ALTER TABLE automation_run_value DROP COLUMN exit_name;")?;
    }
    Ok(())
}

/// v65: a port is keyed, not named (`AMB-D-961`) — v62's move, one layer down.
///
/// A wire named the ports at its two ends, a run's copy of a step named each port it declared, a copy's
/// input named the outputs wired into it, and a run's values named the port they went through. So
/// renaming a port parted every wire on it. Each of them now holds the `automation_port` row's id, and
/// a copy (`automation_run_def.exits` and `.ins`) holds the id beside the name, which is what the run's
/// values are read against.
///
/// **Where each end of a wire is read from.** A wire leaving a way out leaves from an output on that way
/// out; one leaving the action itself (`from_id = 0`) hands on an input the action declares. It lands on
/// an input of the box it reaches — on an automation's picture, an input the action standing there
/// declares — or, into the action itself, on an output of the action's way out that the line from the
/// wire's source returns to.
///
/// **What cannot be keyed goes**, v62's rule: a wire whose name no port at that end carries any more was
/// already parted, and carried nothing.
///
/// **A copy whose port is gone keeps it under an id of its own**, below zero, as v62 numbers ways out —
/// the run's values still say which of them they went through.
///
/// **A value put down on a step still running had no way out yet** (v62 left it NULL, to be stamped when
/// the step reported). A value is one output's now, so it takes the way out its port hangs on — the
/// first that declares the name, where two do.
///
/// **Probed, not bare**, for v62's reason.
fn key_the_ports(ctx: &Ctx<'_>) -> Result<()> {
    let tx = ctx.tx;
    let has = |table: &str, column: &str| -> Result<bool> {
        Ok(column_names(tx, table)?.iter().any(|c| c == column))
    };

    if has("automation_wire", "from_port_name")? {
        for column in ["from_port_id", "to_port_id"] {
            if !has("automation_wire", column)? {
                tx.execute_batch(&format!(
                    "ALTER TABLE automation_wire ADD COLUMN {column} BIGINT NOT NULL DEFAULT 0;"
                ))?;
            }
        }
        tx.execute_batch(
            "UPDATE automation_wire SET from_port_id = COALESCE((
                 SELECT p.id FROM automation_port p
                  WHERE p.owner_kind = 'exit' AND p.owner_id = automation_wire.from_exit_id
                    AND p.direction = 'out' AND p.name = automation_wire.from_port_name), 0)
              WHERE from_exit_id IS NOT NULL;
             UPDATE automation_wire SET from_port_id = COALESCE((
                 SELECT p.id FROM automation_port p
                  WHERE p.owner_kind = 'action' AND p.owner_id = automation_wire.owner_id
                    AND p.direction = 'in' AND p.name = automation_wire.from_port_name), 0)
              WHERE owner_kind = 'action' AND from_id = 0;
             UPDATE automation_wire SET to_port_id = COALESCE((
                 SELECT p.id FROM automation_port p
                   JOIN automation_placement pl ON pl.action_id = p.owner_id
                  WHERE p.owner_kind = 'action' AND pl.id = automation_wire.to_id
                    AND p.direction = 'in' AND p.name = automation_wire.to_port_name), 0)
              WHERE owner_kind = 'automation';
             UPDATE automation_wire SET to_port_id = COALESCE((
                 SELECT p.id FROM automation_port p
                  WHERE p.owner_kind = 'step' AND p.owner_id = automation_wire.to_id
                    AND p.direction = 'in' AND p.name = automation_wire.to_port_name), 0)
              WHERE owner_kind = 'action' AND to_id <> 0;
             UPDATE automation_wire SET to_port_id = COALESCE((
                 SELECT p.id FROM automation_port p
                   JOIN automation_edge e ON e.exit_to_id = p.owner_id
                  WHERE p.owner_kind = 'exit' AND p.direction = 'out'
                    AND p.name = automation_wire.to_port_name
                    AND e.owner_kind = 'action' AND e.ends = 'exit'
                    AND e.from_id = automation_wire.from_id
                    AND e.exit_id = automation_wire.from_exit_id), 0)
              WHERE owner_kind = 'action' AND to_id = 0;
             DELETE FROM automation_wire WHERE from_port_id = 0 OR to_port_id = 0;
             ALTER TABLE automation_wire DROP COLUMN from_port_name;
             ALTER TABLE automation_wire DROP COLUMN to_port_name;",
        )?;
    }

    // The run's copies: an id beside every port's name, from the live row where it is still there and
    // below zero where it is not. Read whole first, since an input's sources are keyed against the
    // outputs of other copies of the same run.
    struct Copy {
        id: i64,
        run_id: i64,
        placement_id: Option<i64>,
        step_id: Option<i64>,
        exits: Vec<serde_json::Value>,
        ins: Vec<serde_json::Value>,
    }
    let mut copies: Vec<Copy> = Vec::new();
    {
        let mut stmt =
            tx.prepare("SELECT id, run_id, placement_id, step_id, exits, ins FROM automation_run_def")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        for row in rows {
            let (id, run_id, placement_id, step_id, exits, ins) = row?;
            copies.push(Copy {
                id,
                run_id,
                placement_id,
                step_id,
                exits: serde_json::from_str(&exits).unwrap_or_default(),
                ins: serde_json::from_str(&ins).unwrap_or_default(),
            });
        }
    }
    let live_port = |owner_kind: &str, owner_id: i64, direction: &str, name: &str| -> Result<Option<i64>> {
        Ok(tx
            .query_row(
                "SELECT id FROM automation_port \
                  WHERE owner_kind = ?1 AND owner_id = ?2 AND direction = ?3 AND name = ?4",
                rusqlite::params![owner_kind, owner_id, direction, name],
                |r| r.get(0),
            )
            .optional()?)
    };
    // (run, placement, step, way out, output name) → the id a copy keys that output by, for the inputs'
    // sources; (copy, name, way out) → the same, for the values.
    type Source = (i64, Option<i64>, Option<i64>, Option<i64>, String);
    let mut outputs: std::collections::HashMap<Source, i64> = std::collections::HashMap::new();
    let mut stand_in = -1;
    let mut next_stand_in = || {
        stand_in -= 1;
        stand_in + 1
    };
    for copy in copies.iter_mut() {
        for exit in copy.exits.iter_mut() {
            let exit_id = exit.get("id").and_then(|i| i.as_i64());
            let Some(outs) = exit.get_mut("outs").and_then(|o| o.as_array_mut()) else { continue };
            for port in outs.iter_mut() {
                let name = port.get("name").and_then(|n| n.as_str()).unwrap_or_default().to_string();
                let id = match port.get("id").and_then(|i| i.as_i64()) {
                    Some(id) => id,
                    None => {
                        let live = match exit_id {
                            Some(exit_id) if exit_id > 0 => live_port("exit", exit_id, "out", &name)?,
                            _ => None,
                        };
                        let id = live.unwrap_or_else(&mut next_stand_in);
                        if let Some(map) = port.as_object_mut() {
                            map.insert("id".to_string(), serde_json::Value::from(id));
                        }
                        id
                    }
                };
                outputs.insert((copy.run_id, copy.placement_id, copy.step_id, exit_id, name), id);
            }
        }
        for input in copy.ins.iter_mut() {
            if input.get("id").and_then(|i| i.as_i64()).is_some() {
                continue;
            }
            let name = input.get("name").and_then(|n| n.as_str()).unwrap_or_default().to_string();
            let live = match copy.step_id {
                Some(step_id) => live_port("step", step_id, "in", &name)?,
                None => None,
            };
            let id = live.unwrap_or_else(&mut next_stand_in);
            if let Some(map) = input.as_object_mut() {
                map.insert("id".to_string(), serde_json::Value::from(id));
            }
        }
    }
    for copy in copies.iter_mut() {
        let run_id = copy.run_id;
        for input in copy.ins.iter_mut() {
            let Some(from) = input.get_mut("from").and_then(|f| f.as_array_mut()) else { continue };
            let mut keyed = Vec::with_capacity(from.len());
            for mut source in from.drain(..) {
                let Some(map) = source.as_object_mut() else { continue };
                if map.contains_key("port_id") {
                    keyed.push(source);
                    continue;
                }
                let Some(name) = map.remove("port").and_then(|n| n.as_str().map(str::to_string)) else {
                    continue;
                };
                let exit_id = map.get("exit_id").and_then(|v| v.as_i64());
                let key = (
                    run_id,
                    map.get("placement_id").and_then(|v| v.as_i64()),
                    map.get("step_id").and_then(|v| v.as_i64()),
                    exit_id,
                    name.clone(),
                );
                // The copy of the source step keys it; failing that, the way out's live output of the
                // name. An output neither declares could never have been handed on.
                let id = match outputs.get(&key) {
                    Some(id) => Some(*id),
                    None => match exit_id {
                        Some(exit_id) if exit_id > 0 => live_port("exit", exit_id, "out", &name)?,
                        _ => None,
                    },
                };
                if let Some(id) = id {
                    map.insert("port_id".to_string(), serde_json::Value::from(id));
                    keyed.push(source);
                }
            }
            *from = keyed;
        }
        tx.execute(
            "UPDATE automation_run_def SET exits = ?1, ins = ?2 WHERE id = ?3",
            rusqlite::params![
                serde_json::Value::Array(copy.exits.clone()).to_string(),
                serde_json::Value::Array(copy.ins.clone()).to_string(),
                copy.id
            ],
        )?;
    }

    if has("automation_run_value", "name")? {
        if !has("automation_run_value", "port_id")? {
            tx.execute_batch(
                "ALTER TABLE automation_run_value ADD COLUMN port_id BIGINT NOT NULL DEFAULT 0;",
            )?;
        }
        let by_copy: std::collections::HashMap<i64, &Copy> = copies.iter().map(|c| (c.id, c)).collect();
        let mut values: Vec<(i64, i64, String, Option<i64>, String)> = Vec::new();
        {
            let mut stmt = tx.prepare(
                "SELECT v.id, s.run_def_id, v.direction, v.exit_id, v.name FROM automation_run_value v
                   JOIN automation_run_step s ON s.id = v.run_step_id",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
            for row in rows {
                values.push(row?);
            }
        }
        for (id, def_id, direction, exit_id, name) in values {
            let Some(copy) = by_copy.get(&def_id) else { continue };
            let found: Option<(i64, Option<i64>)> = if direction == "in" {
                copy.ins
                    .iter()
                    .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(name.as_str()))
                    .and_then(|p| p.get("id").and_then(|i| i.as_i64()))
                    .map(|port| (port, None))
            } else {
                copy.exits
                    .iter()
                    .filter(|e| exit_id.is_none() || e.get("id").and_then(|i| i.as_i64()) == exit_id)
                    .find_map(|e| {
                        let exit = e.get("id").and_then(|i| i.as_i64());
                        e.get("outs")?
                            .as_array()?
                            .iter()
                            .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(name.as_str()))
                            .and_then(|p| p.get("id").and_then(|i| i.as_i64()))
                            .map(|port| (port, exit))
                    })
            };
            let Some((port, exit)) = found else { continue };
            tx.execute(
                "UPDATE automation_run_value SET port_id = ?1, exit_id = COALESCE(exit_id, ?2) WHERE id = ?3",
                rusqlite::params![port, exit, id],
            )?;
        }
        tx.execute_batch("ALTER TABLE automation_run_value DROP COLUMN name;")?;
    }
    Ok(())
}

/// v59: the shared documents go, and `automation_action.note` arrives (`AMB-D-952`).
///
/// **Why the rows are not carried anywhere.** A document was a place to write material a prompt would
/// otherwise repeat, handed to a placement and read into every step opened under it. There are two ways
/// into a prompt from here on — the preamble and the step's own prompt — and neither takes a row: the
/// preamble is Amenbo's own text, and a step's prompt is the one a person wrote in it. So there is no
/// column these bodies belong in, and copying them into one would put the same words in two places
/// where the point of the change is that they are in one.
///
/// The order is the `RESTRICT` clause's: a link names the document, so the link table goes before the
/// document table does — dropping a table something still points rows at is refused. The third name of
/// that family, `automation_step_note`, went at v56 (the_note_table_v50_lays_down_is_dropped).
///
/// **`DROP` alone, with no `DELETE` in front of it.** A `DROP` takes the rows with it and is prepared
/// against no parent, which is what keeps this two statements rather than four.
///
/// **The word index is swept by hand.** A document's `body` was the one face on the automation side
/// (`search::FACES`), and its copies in `search_doc` are not reached by dropping the table they were
/// taken from — nothing joins them back. `search_fts` follows on its own: the triggers on `search_doc`
/// carry every delete into it.
///
/// **The column is appended only where it is missing**, v53's guard and for its reason: a store born
/// from today's registry already carries it, stamped back to a version below this one.
fn take_the_shared_documents_away(ctx: &Ctx<'_>) -> Result<()> {
    let tx = ctx.tx;
    tx.execute_batch(
        "DROP TABLE IF EXISTS automation_placement_note;
         DROP TABLE IF EXISTS automation_note;",
    )?;
    tx.execute("DELETE FROM search_doc WHERE owner_kind = 'automation_note'", [])?;

    let carries_note: i64 = tx.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('automation_action') WHERE name = 'note'",
        [],
        |r| r.get(0),
    )?;
    if carries_note == 0 {
        tx.execute_batch("ALTER TABLE automation_action ADD COLUMN note TEXT NOT NULL DEFAULT '';")?;
    }
    Ok(())
}

/// v61: `automation_run.acknowledged_at` — when a person said they had seen a failed run
/// (`AMB-D-955`).
///
/// **A plain `ALTER TABLE … ADD COLUMN`.** The column is nullable, so every row already there reads as
/// not acknowledged — which is true of every failure in a store that had no way to say it. The `CHECK`
/// is the one every timestamp column carries, frozen here as text. A store born from a registry that
/// already has the column, stamped back to an earlier version, is left as it is.
fn let_a_failure_be_acknowledged(ctx: &Ctx<'_>) -> Result<()> {
    let has_column: i64 = ctx.tx.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('automation_run') WHERE name = 'acknowledged_at'",
        [],
        |r| r.get(0),
    )?;
    if has_column == 0 {
        ctx.tx.execute_batch(
            "ALTER TABLE automation_run ADD COLUMN acknowledged_at TEXT \
             CHECK(acknowledged_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z');",
        )?;
    }
    Ok(())
}

/// v64: a step's agent and model move off the step, onto the placement of its action
/// (`automation_placement_step`, `AMB-D-960`).
///
/// **What each step said is written onto every placement of its action**, one row per pair. An action
/// placed twice is asked for by the same agent at both spots until somebody chooses otherwise, which is
/// what it did before this step — so no run launched after the move is carried out by anyone else. A
/// step with the empty sentinel for an agent said nothing, and is left with nobody chosen.
///
/// **The table is laid down here in frozen text** as well as by genesis, for the reason v53's are: the
/// registry may reshape it tomorrow, and what this step built must keep meaning what it meant. A pair
/// already there is left alone, so a store stamped back and run forward again lands on what it had.
///
/// **The two columns go after the copy**, and only where they are still there: a store born from a
/// registry that no longer has them, stamped back to an earlier version, has nothing to copy or drop.
fn choose_the_agent_where_the_action_is_placed(ctx: &Ctx<'_>) -> Result<()> {
    let tx = ctx.tx;
    tx.execute_batch(PLACEMENT_STEP_TABLE)?;
    let carries: i64 = tx.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('automation_action_step') WHERE name = 'agent'",
        [],
        |r| r.get(0),
    )?;
    if carries == 0 {
        return Ok(());
    }
    let now = crate::time::Timestamp::now().to_rfc3339_z();
    tx.execute(
        "INSERT INTO automation_placement_step (placement_id, step_id, agent, model, created_at, \
             updated_at) \
         SELECT p.id, s.id, s.agent, s.model, ?1, ?1 \
         FROM automation_placement p \
         JOIN automation_action_step s ON s.action_id = p.action_id \
         WHERE s.agent <> '' \
           AND NOT EXISTS (SELECT 1 FROM automation_placement_step c \
                           WHERE c.placement_id = p.id AND c.step_id = s.id) \
         ORDER BY p.id, s.id",
        rusqlite::params![now],
    )?;
    tx.execute_batch(
        "ALTER TABLE automation_action_step DROP COLUMN agent;
         ALTER TABLE automation_action_step DROP COLUMN model;",
    )?;
    Ok(())
}

/// The table v64 lays down — frozen text, like every step's.
const PLACEMENT_STEP_TABLE: &str = r"
CREATE TABLE IF NOT EXISTS automation_placement_step (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    placement_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_placement(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    step_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_action_step(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    agent TEXT NOT NULL DEFAULT '',
    model TEXT,
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    UNIQUE (placement_id, step_id)
);
";

/// v60: `automation_run.status` ends on `completed`, `failed` or `canceled` in place of `done` and
/// `stopped` (`AMB-D-955`).
///
/// **What the rows become.** `done` is the picture run out, which is `completed`. A `stopped` run that a
/// person stopped (`by_human`) is `canceled`, and loses the reason — a cancel carries none. Every other
/// `stopped` run is `failed` and keeps the reason it has. The reason set takes `no_input` and `halted` — what
/// an input nothing filled and a way out that calls a person write — and drops `by_human`.
///
/// **A `stopped` row with no reason stays without one.** It is a run that halted on a way out, ran out
/// of picture mid-walk, or had an input nothing filled, and the row does not say which. It becomes
/// `failed`, because each of the three is; naming one of them would make the record say something it
/// never knew.
///
/// **The declarations first, the rows second, one transaction** — v51's order. SQLite checks a `CHECK`
/// on write and never on the rows already there, so the rows can be moved onto the new values once the
/// columns admit them. This is v9's procedure met an eighth time, copied rather than called, for the
/// reasons [`admit_rejected_task_status`] gives.
fn say_how_a_run_ended(ctx: &Ctx<'_>) -> Result<()> {
    /// The two closed sets as every store from v52 on declares them — frozen text, like every step's.
    const OLD_STATUS: &str = "CHECK(status IN ('', 'running', 'paused', 'done', 'stopped'))";
    const OLD_REASON: &str =
        "CHECK(stopped_reason IN ('crashed', 'max_times', 'no_agent', 'by_human', 'no_way_on'))";
    /// The same columns with the three endings, and the reasons only a failure carries.
    const NEW_STATUS: &str =
        "CHECK(status IN ('', 'running', 'paused', 'completed', 'failed', 'canceled'))";
    const NEW_REASON: &str = "CHECK(stopped_reason IN ('crashed', 'max_times', 'no_agent', 'no_input', \
                              'no_way_on', 'halted'))";

    let declared: String = ctx.tx.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'automation_run'",
        [],
        |r| r.get(0),
    )?;
    if !(declared.contains(NEW_STATUS) && declared.contains(NEW_REASON)) {
        // Not already rewritten: a store born from a registry that carries the new sets, stamped back
        // to an earlier version, is the one that arrives here with nothing to rewrite.
        for expected in [OLD_STATUS, OLD_REASON] {
            if !declared.contains(expected) {
                return Err(super::StoreEngineError::UnrecognisedDdl {
                    table: "automation_run",
                    expected,
                });
            }
        }
        let rewritten = declared.replace(OLD_STATUS, NEW_STATUS).replace(OLD_REASON, NEW_REASON);

        let before = column_names(ctx.tx, "automation_run")?;
        ctx.tx.execute_batch("PRAGMA writable_schema = ON;")?;
        let wrote = ctx.tx.execute(
            "UPDATE sqlite_master SET sql = ?1 WHERE type = 'table' AND name = 'automation_run'",
            [&rewritten],
        );
        // `RESET` both shuts the door and drops the connection's parsed schema, so the `UPDATE`s below
        // see the new sets instead of the ones this connection read at open.
        ctx.tx.execute_batch("PRAGMA writable_schema = RESET;")?;
        wrote?;
        let after = column_names(ctx.tx, "automation_run")?;
        if before != after {
            return Err(super::StoreEngineError::UnrecognisedDdl {
                table: "automation_run",
                expected: OLD_STATUS,
            });
        }
    }

    ctx.tx.execute_batch(
        "UPDATE automation_run SET status = 'completed' WHERE status = 'done';
         UPDATE automation_run SET status = 'canceled', stopped_reason = NULL
          WHERE status = 'stopped' AND stopped_reason = 'by_human';
         UPDATE automation_run SET status = 'failed' WHERE status = 'stopped';",
    )?;
    Ok(())
}

/// v52: `automation_run.stopped_reason` admits `no_way_on`.
///
/// The value reached the model with the watch that writes it
/// ([`crate::model::AutomationStoppedReason::NoWayOn`], `AMB-T-5296`) and never reached the column, so
/// every store refused the write: a run the watch found nowhere to open for stayed `running`, holding
/// the task it had taken, and the watch said so in the log once a second. The set is widened rather
/// than the write being softened, because the reason is what tells that ending apart from the three
/// the reader could have caused.
///
/// **This is v9's procedure met a seventh time, copied rather than called** — the reasons
/// [`admit_rejected_task_status`] gives at length. Nothing is written to the rows: no row can be
/// carrying a value the column never accepted.
fn admit_the_run_with_nowhere_to_go(ctx: &Ctx<'_>) -> Result<()> {
    /// The closed set as every store from v50 on declares it — frozen text, like every step's.
    const NARROW: &str =
        "CHECK(stopped_reason IN ('crashed', 'max_times', 'no_agent', 'by_human'))";
    /// The same set with the ending the watch writes.
    const WIDE: &str =
        "CHECK(stopped_reason IN ('crashed', 'max_times', 'no_agent', 'by_human', 'no_way_on'))";

    let declared: String = ctx.tx.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'automation_run'",
        [],
        |r| r.get(0),
    )?;
    if declared.contains(WIDE) || (!declared.contains(NARROW) && declared.contains("'no_way_on'")) {
        // Already wide: a store born from a registry that carries the value, stamped back to an
        // earlier version. Nothing to widen, and nothing wrong. The registry's set may have moved on
        // since (v60 renames it), so what is read is the value, not this step's own spelling of the set.
        return Ok(());
    }
    if !declared.contains(NARROW) {
        return Err(super::StoreEngineError::UnrecognisedDdl {
            table: "automation_run",
            expected: NARROW,
        });
    }
    let widened = declared.replace(NARROW, WIDE);

    let before = column_names(ctx.tx, "automation_run")?;
    ctx.tx.execute_batch("PRAGMA writable_schema = ON;")?;
    let wrote = ctx.tx.execute(
        "UPDATE sqlite_master SET sql = ?1 WHERE type = 'table' AND name = 'automation_run'",
        [&widened],
    );
    // `RESET` both shuts the door and drops the connection's parsed schema, so the very next
    // statement sees the widened `CHECK` instead of the one this connection read at open.
    ctx.tx.execute_batch("PRAGMA writable_schema = RESET;")?;
    wrote?;
    let after = column_names(ctx.tx, "automation_run")?;
    if before != after {
        return Err(super::StoreEngineError::UnrecognisedDdl {
            table: "automation_run",
            expected: NARROW,
        });
    }
    Ok(())
}

/// v51: `automation_run.status` loses `queued` (`AMB-D-947`).
///
/// **Why the value cannot simply be left declared and unused.** A row still reading `queued` is one no
/// build from here on can hydrate — the value has no variant to parse into — and nothing would ever
/// clear it: what promoted a queued run was a lane being handed back, and there are no lanes any more.
/// It would sit there holding its automation undeletable (`RESTRICT`) for as long as the store lives.
///
/// **Where those rows land: `stopped`, reason `crashed`.** That is the answer the startup sweep already
/// gave them ([`crate::ops::automation_stop::sweep`]), and it is the true one — the run was waiting for
/// a terminal in a window that is gone, and no window since has been able to give it one. `ended_at` is
/// stamped with the moment of the migration, because a run with no end is one a face reads as still
/// going.
///
/// **The declaration first, the rows second, one transaction** — v48's order, and for its reason.
/// SQLite checks a `CHECK` on write and never on the rows already there, so narrowing the set while
/// `queued` is still written in the column is safe, and `stopped` is in both sets either way.
///
/// **This is v9's procedure met a sixth time, copied rather than called.** SQLite has no
/// `ALTER TABLE … DROP CONSTRAINT`, and the rebuild-and-swap its documentation prescribes is closed for
/// the reason [`admit_rejected_task_status`] gives at length — enforcement is on and four tables
/// reference `automation_run`. A step is frozen at the meaning it had when it was written, so it names
/// its own clause in its own text.
fn take_the_run_queue_away(ctx: &Ctx<'_>) -> Result<()> {
    /// The closed set as every store from v50 on declares it — frozen text, like every step's.
    const WITH_QUEUE: &str =
        "CHECK(status IN ('', 'queued', 'running', 'paused', 'done', 'stopped'))";
    /// The same column with the four states a run can still be in.
    const WITHOUT_QUEUE: &str = "CHECK(status IN ('', 'running', 'paused', 'done', 'stopped'))";

    let declared: String = ctx.tx.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'automation_run'",
        [],
        |r| r.get(0),
    )?;
    // Already narrowed: a store born from a registry that no longer carries the value, stamped back to
    // an earlier version, is the one that arrives here with nothing to rewrite. The registry's set may
    // have moved on since (v60 renames the endings), so what is read is the value's absence, not this
    // step's own spelling of the set.
    let narrowed_already =
        declared.contains(WITHOUT_QUEUE) || !declared.contains("'queued'");
    if !narrowed_already {
        if !declared.contains(WITH_QUEUE) {
            return Err(super::StoreEngineError::UnrecognisedDdl {
                table: "automation_run",
                expected: WITH_QUEUE,
            });
        }
        let narrowed = declared.replace(WITH_QUEUE, WITHOUT_QUEUE);

        let before = column_names(ctx.tx, "automation_run")?;
        ctx.tx.execute_batch("PRAGMA writable_schema = ON;")?;
        let wrote = ctx.tx.execute(
            "UPDATE sqlite_master SET sql = ?1 WHERE type = 'table' AND name = 'automation_run'",
            [&narrowed],
        );
        // `RESET` both shuts the door and drops the connection's parsed schema, so the `UPDATE` below
        // sees the new set instead of the one this connection read at open.
        ctx.tx.execute_batch("PRAGMA writable_schema = RESET;")?;
        wrote?;
        let after = column_names(ctx.tx, "automation_run")?;
        if before != after {
            return Err(super::StoreEngineError::UnrecognisedDdl {
                table: "automation_run",
                expected: WITH_QUEUE,
            });
        }
    }

    ctx.tx.execute_batch(
        "UPDATE automation_run
            SET status = 'stopped',
                stopped_reason = 'crashed',
                ended_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
          WHERE status = 'queued';",
    )?;
    Ok(())
}

/// v50: the floor the automation feature stands on — fifteen tables, and three widenings of what the
/// store already had.
///
/// **The tables are `CREATE TABLE IF NOT EXISTS` over frozen text.** Genesis runs the registry's DDL
/// before this chain on every open, so a store arriving here has already been given every one of them
/// and there is nothing left for these statements to do; what they are for is the store that is *not*
/// opened by this build first — and, more to the point, the version. The text is the step's own and
/// does not follow the registry: renaming a column tomorrow must not reach back into a store migrated
/// today.
///
/// **Three things the tables alone would not give an existing store.**
///
/// 1. `task_comment.automation_run_step_id` — an `ALTER TABLE ADD COLUMN`, which is the one shape that
///    reaches a table `IF NOT EXISTS` has already left alone. It is nullable with no default, which is
///    also what SQLite requires of a column added with a `REFERENCES` clause.
/// 2. `attachment.target_type` admits one more kind. A `CHECK` cannot be altered, and the
///    rebuild-and-swap SQLite documents is closed here for the reason
///    [`admit_rejected_task_status`] gives at length — so the declaration is rewritten in place, the
///    fifth time that procedure is met and the fifth time it is written out rather than shared: a
///    step is frozen at the meaning it had when it was written, and a helper would move under it.
/// 3. The index on `automation_run_task(task_id)`, which is `IF NOT EXISTS` for the same reason the
///    tables are.
fn lay_the_automation_tables_down(ctx: &Ctx<'_>) -> Result<()> {
    /// The `target_type` set as every store from the baseline on declares it — frozen text.
    const NARROW: &str =
        "CHECK(target_type IN ('', 'task', 'decision', 'task_comment', 'decision_comment'))";
    /// The same set with the step execution a produced file hangs off.
    const WIDE: &str = "CHECK(target_type IN ('', 'task', 'decision', 'task_comment', \
         'decision_comment', 'automation_run_step'))";

    ctx.tx.execute_batch(TABLES)?;

    // The column is added only where it is missing: a store born from today's registry already has it,
    // and `ADD COLUMN` has no `IF NOT EXISTS`.
    if !column_names(ctx.tx, "task_comment")?.iter().any(|c| c == "automation_run_step_id") {
        ctx.tx.execute_batch(
            "ALTER TABLE task_comment ADD COLUMN automation_run_step_id BIGINT \
                 REFERENCES automation_run_step(id) \
                 ON DELETE SET NULL ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED;",
        )?;
    }

    let declared: String = ctx.tx.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'attachment'",
        [],
        |r| r.get(0),
    )?;
    if !declared.contains(WIDE) {
        // Not already wide: a store born from a registry that carries the kind, stamped back to an
        // earlier version, is the one that arrives here with nothing to rewrite.
        if !declared.contains(NARROW) {
            return Err(super::StoreEngineError::UnrecognisedDdl {
                table: "attachment",
                expected: NARROW,
            });
        }
        let widened = declared.replace(NARROW, WIDE);

        let before = column_names(ctx.tx, "attachment")?;
        ctx.tx.execute_batch("PRAGMA writable_schema = ON;")?;
        let wrote = ctx.tx.execute(
            "UPDATE sqlite_master SET sql = ?1 WHERE type = 'table' AND name = 'attachment'",
            [&widened],
        );
        // `RESET` both shuts the door and drops the connection's parsed schema, so the next statement
        // sees the widened `CHECK` instead of the one this connection read at open.
        ctx.tx.execute_batch("PRAGMA writable_schema = RESET;")?;
        wrote?;
        let after = column_names(ctx.tx, "attachment")?;
        if before != after {
            return Err(super::StoreEngineError::UnrecognisedDdl {
                table: "attachment",
                expected: NARROW,
            });
        }
    }

    ctx.tx.execute_batch(
        "CREATE INDEX IF NOT EXISTS automation_run_task_by_task ON automation_run_task(task_id);",
    )?;
    Ok(())
}

/// The fifteen tables as this step lays them down — the shape the registry emitted when it was
/// written, spelled out here so that what a store already migrated holds cannot be moved by a later
/// edit to the registry.
const TABLES: &str = r"
CREATE TABLE IF NOT EXISTS automation_action (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT REFERENCES project(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL DEFAULT '',
    prompt TEXT NOT NULL DEFAULT '',
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    preamble TEXT NOT NULL DEFAULT '',
    entry_step_id BIGINT REFERENCES automation_step(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    archived BOOLEAN NOT NULL DEFAULT 0 CHECK(archived IN (0, 1)),
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_note (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    automation_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_step (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    automation_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL DEFAULT '',
    action_id BIGINT REFERENCES automation_action(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    prompt TEXT,
    agent TEXT NOT NULL DEFAULT '',
    model TEXT,
    interactive BOOLEAN NOT NULL DEFAULT 0 CHECK(interactive IN (0, 1)),
    work_dir_ref TEXT,
    report_to_task BOOLEAN NOT NULL DEFAULT 0 CHECK(report_to_task IN (0, 1)),
    show_history BOOLEAN NOT NULL DEFAULT 0 CHECK(show_history IN (0, 1)),
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_cfg (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT '' CHECK(owner_kind IN ('', 'step', 'action')),
    owner_id BIGINT NOT NULL DEFAULT 0,
    name TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL DEFAULT '' CHECK(kind IN ('', 'taskfilter', 'folder', 'choice', 'number', 'text')),
    required BOOLEAN NOT NULL DEFAULT 0 CHECK(required IN (0, 1)),
    options TEXT,
    value TEXT,
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_step_note (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    step_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_step(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    note_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_note(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_exit (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT '' CHECK(owner_kind IN ('', 'step', 'action')),
    owner_id BIGINT NOT NULL DEFAULT 0,
    name TEXT,
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_port (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT '' CHECK(owner_kind IN ('', 'step', 'action', 'exit')),
    owner_id BIGINT NOT NULL DEFAULT 0,
    direction TEXT NOT NULL DEFAULT '' CHECK(direction IN ('', 'in', 'out')),
    name TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL DEFAULT '' CHECK(kind IN ('', 'value', 'file', 'task_take', 'task_make')),
    required BOOLEAN NOT NULL DEFAULT 0 CHECK(required IN (0, 1)),
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_edge (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    automation_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    from_step_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_step(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    exit_name TEXT,
    to_step_id BIGINT REFERENCES automation_step(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    ends TEXT NOT NULL DEFAULT '' CHECK(ends IN ('', 'go', 'done', 'halt')),
    max_times BIGINT,
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_wire (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    automation_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    from_step_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_step(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    from_exit_name TEXT,
    from_port_name TEXT NOT NULL DEFAULT '',
    to_step_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_step(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    to_port_name TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_run (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    automation_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    status TEXT NOT NULL DEFAULT '' CHECK(status IN ('', 'queued', 'running', 'paused', 'done', 'stopped')),
    pause_requested BOOLEAN NOT NULL DEFAULT 0 CHECK(pause_requested IN (0, 1)),
    stopped_reason TEXT CHECK(stopped_reason IN ('crashed', 'max_times', 'no_agent', 'by_human')),
    started_by_kind TEXT CHECK(started_by_kind IN ('human', 'ai')),
    started_at TEXT CHECK(started_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    ended_at TEXT CHECK(ended_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_run_def (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    run_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_run(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    step_id BIGINT REFERENCES automation_step(id) ON DELETE SET NULL ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL DEFAULT '',
    prompt TEXT,
    agent TEXT NOT NULL DEFAULT '',
    model TEXT,
    interactive BOOLEAN NOT NULL DEFAULT 0 CHECK(interactive IN (0, 1)),
    work_dir_ref TEXT,
    report_to_task BOOLEAN NOT NULL DEFAULT 0 CHECK(report_to_task IN (0, 1)),
    show_history BOOLEAN NOT NULL DEFAULT 0 CHECK(show_history IN (0, 1)),
    exits TEXT NOT NULL DEFAULT '',
    ins TEXT NOT NULL DEFAULT '',
    cfg TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_run_task (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    run_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_run(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    seq BIGINT NOT NULL DEFAULT 0,
    task_id BIGINT REFERENCES task(id) ON DELETE SET NULL ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    started_at TEXT CHECK(started_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    ended_at TEXT CHECK(ended_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_run_step (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    run_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_run(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    run_def_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_run_def(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    run_task_id BIGINT REFERENCES automation_run_task(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    seq BIGINT NOT NULL DEFAULT 0,
    exit_name TEXT,
    report TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT '' CHECK(status IN ('', 'running', 'done', 'failed', 'stopped')),
    started_at TEXT CHECK(started_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    ended_at TEXT CHECK(ended_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_run_value (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    run_step_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_run_step(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    direction TEXT NOT NULL DEFAULT '' CHECK(direction IN ('', 'in', 'out')),
    exit_name TEXT,
    name TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL DEFAULT '' CHECK(kind IN ('', 'value', 'file', 'task_take', 'task_make')),
    value TEXT,
    attachment_id BIGINT REFERENCES attachment(id) ON DELETE SET NULL ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    task_id BIGINT REFERENCES task(id) ON DELETE SET NULL ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    from_run_step_id BIGINT REFERENCES automation_run_step(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
";

/// v43: take the plugin mechanism's tables and its execution log away (`AMB-D-884`).
///
/// The tables first, on the step's own transaction. The log is `<base>/plugin-runs.jsonl` and its lock
/// sidecar — a file, so it cannot ride that transaction, and it goes last for the reason v42's directories
/// do: an interruption leaves a log nothing reads rather than a table nothing can be read out of. Neither
/// is missed if it is already gone, which is every store born after this step.
fn drop_the_plugin_mechanism(ctx: &Ctx<'_>) -> Result<()> {
    ctx.tx.execute_batch(
        "DROP TABLE IF EXISTS plugin_queue;
         DROP TABLE IF EXISTS plugin_runner;
         DROP TABLE IF EXISTS plugin_enable;
         DROP TABLE IF EXISTS plugin_secret;
         DROP TABLE IF EXISTS plugin_config;",
    )?;
    for name in ["plugin-runs.jsonl", "plugin-runs.jsonl.lock"] {
        let _ = std::fs::remove_file(ctx.base_dir.join(name));
    }
    Ok(())
}

/// v44: spell the outbox and its cursors for what reads them (`AMB-D-901`, `AMB-T-4770`).
///
/// **The genesis table is dropped before the rename, and that is the whole reason this is not one
/// `ALTER TABLE`.** Genesis is `CREATE TABLE IF NOT EXISTS` over today's registry and runs before this
/// chain on every open, so a store arriving here already carries an `outbox` — created empty moments ago,
/// since nothing writes between genesis and the chain — and `ALTER TABLE … RENAME TO outbox` would fail
/// on a name already taken. Dropping it first is what leaves the rename a rename: the rows, the
/// `AUTOINCREMENT` high-water mark in `sqlite_sequence` and the reader's cursor all stay the numbers they
/// were, which is what keeps a drive mid-walk from reading its own store as a gap.
///
/// A store below v4 predates the outbox entirely and has nothing under the old name — genesis gave it
/// today's `outbox` and there is nothing to carry, so the rename is skipped and the keys, which such a
/// store never wrote either, simply match nothing.
fn rename_the_outbox(ctx: &Ctx<'_>) -> Result<()> {
    if table_is_here(ctx.tx, "plugin_outbox")? {
        ctx.tx.execute_batch(
            "DROP TABLE IF EXISTS outbox;
             ALTER TABLE plugin_outbox RENAME TO outbox;",
        )?;
    }
    // Frozen text, like every step's: the three keys the store had then. A plain `UPDATE` and not an
    // upsert — the new spellings are written by builds from this version on, and those have already run
    // this step, so there is nothing here for them to collide with.
    ctx.tx.execute_batch(
        "UPDATE store_meta SET key = 'outbox_cursor'
             WHERE key = 'plugin_dispatch_cursor';
         UPDATE store_meta SET key = 'outbox_cursor_face'
             WHERE key = 'plugin_dispatch_cursor_face';
         UPDATE store_meta SET key = 'outbox_truncated_through'
             WHERE key = 'plugin_outbox_truncated_through';",
    )?;
    Ok(())
}

/// v45: the panes' ids stop being counted and start being drawn (`AMB-D-897`, `AMB-T-4843`).
///
/// Every id in the kept arrangement is replaced with a version 4 UUID, once. What the old numbers
/// cost is in the step's note above; what they cannot do afterwards is collide, whatever becomes of
/// the row they were kept beside.
///
/// **The two per-pane homes move with them.** Codex and Gemini come back by the directory they run
/// in, and that directory is named by the pane's id (`app/src-tauri/src/pane_home.rs`) — an id
/// rewritten on its own would leave each of those panes opening an empty conversation, with the one
/// the reader was in left under a name nothing claims. The two names are spelled here in frozen
/// text, as every step's are: what they are called tomorrow is that file's to say, and what they
/// were called when this ran is this step's to remember.
///
/// **The handle on those two rows moves with the directory.** What they come back by *is* that
/// path, so a row left naming the old one would send the pane to a directory that is no longer
/// there. The other four come back by a session id, which carries nothing of the pane's id and is
/// left alone — which is what the separator before the old name is asked about.
///
/// **What will not move is let go rather than raised.** A home that cannot be renamed costs one
/// pane its way back; a step that failed over it would cost the reader the whole arrangement. The
/// row is rewritten either way, so the ids and the directories cannot end up half converted in
/// opposite directions.
fn draw_the_pane_ids_afresh(ctx: &Ctx<'_>) -> Result<()> {
    // The key the talk window's arrangement is kept under, and the two directories its panes' homes
    // sit in — all three as they were spelled when this step was written.
    const LAYOUT: &str = "talk.layout";
    const HOMES: &[&str] = &["codex-homes", "gemini-homes"];

    let kept: Option<String> = ctx
        .tx
        .query_row("SELECT value FROM store_meta WHERE key = ?1", [LAYOUT], |row| row.get(0))
        .optional()?;
    // A store that never laid a window out, and one whose row will not parse, both have nothing to
    // convert: the window reads such a row as no arrangement at all and lays itself out afresh.
    let Some(Ok(mut row)) = kept.map(|json| serde_json::from_str::<serde_json::Value>(&json)) else {
        return Ok(());
    };
    let Some(panes) = row.get_mut("panes").and_then(|panes| panes.as_array_mut()) else {
        return Ok(());
    };
    for pane in panes {
        let Some(was) = pane.get("id").and_then(|id| id.as_str()).map(str::to_owned) else {
            continue;
        };
        let now = crate::harness::uuid_v4();
        for homes in HOMES {
            let root = ctx.base_dir.join(homes);
            if root.join(&was).is_dir() {
                let _ = std::fs::rename(root.join(&was), root.join(&now));
            }
        }
        if let Some(head) = pane
            .get("resume")
            .and_then(|resume| resume.as_str())
            .and_then(|resume| resume.strip_suffix(&was))
            .filter(|head| head.ends_with('/') || head.ends_with('\\'))
            .map(str::to_owned)
        {
            pane["resume"] = serde_json::Value::String(format!("{head}{now}"));
        }
        pane["id"] = serde_json::Value::String(now);
    }
    ctx.tx.execute(
        "UPDATE store_meta SET value = ?2 WHERE key = ?1",
        rusqlite::params![LAYOUT, row.to_string()],
    )?;
    Ok(())
}

/// v49: give every pane the size that the split its project was held at comes to (`AMB-D-939`).
///
/// **One answer for a project becomes one answer per pane.** The split said how many panes a page of
/// that project drew; the size says how much of a page one pane takes, and the two say the same thing
/// wherever a project's panes were all the same — which they were, there being no way to say anything
/// else. So every pane of a project takes its project's split, and a project nobody answered for
/// leaves its panes at the whole page.
///
/// **The one split an older build wrote is folded in on the way**, under the project the face was on
/// — the same fold `crate::frames` did on every read until this step took it over. A row naming no
/// project has nowhere to put it, and lets it go: a count with nothing to hold it against is not an
/// answer about anything.
///
/// A count this build has no size for is let go the same way. It was written by a build that offered
/// some other split, and a pane put back at a guess would be this step inventing what the reader left.
///
/// The three keys go with the write, so the row that comes out has one shape in it and not two.
fn lay_the_panes_out_by_size(ctx: &Ctx<'_>) -> Result<()> {
    // The key the talk window's arrangement is kept under, and the fields as they were spelled when
    // this step was written.
    const LAYOUT: &str = "talk.layout";

    /// The size a split comes to, or `None` for a count this build cannot draw.
    fn size_of(count: u64, orient: Option<&str>) -> Option<&'static str> {
        Some(match count {
            1 => "whole",
            2 if orient == Some("down") => "half-down",
            2 => "half",
            4 => "quarter",
            6 => "sixth",
            8 => "eighth",
            _ => return None,
        })
    }

    let kept: Option<String> = ctx
        .tx
        .query_row("SELECT value FROM store_meta WHERE key = ?1", [LAYOUT], |row| row.get(0))
        .optional()?;
    // A store that never laid a window out, and one whose row will not parse, both have nothing to
    // convert: the window reads such a row as no arrangement at all and lays itself out afresh.
    let Some(Ok(mut row)) = kept.map(|json| serde_json::from_str::<serde_json::Value>(&json)) else {
        return Ok(());
    };
    // The answers by project, as this row has them — the set, or the one split an older build wrote
    // put back under the project it was set on.
    let mut splits: std::collections::BTreeMap<String, &'static str> = row
        .get("splits")
        .and_then(|splits| splits.as_object())
        .map(|splits| {
            splits
                .iter()
                .filter_map(|(project, split)| {
                    let count = split.get("count")?.as_u64()?;
                    let orient = split.get("orient").and_then(|orient| orient.as_str());
                    Some((project.clone(), size_of(count, orient)?))
                })
                .collect()
        })
        .unwrap_or_default();
    if splits.is_empty() {
        if let (Some(count), Some(project)) = (
            row.get("count").and_then(serde_json::Value::as_u64),
            row.get("project").and_then(serde_json::Value::as_u64),
        ) {
            let orient = row.get("orient").and_then(|orient| orient.as_str());
            if let Some(size) = size_of(count, orient) {
                splits.insert(project.to_string(), size);
            }
        }
    }
    if let Some(panes) = row.get_mut("panes").and_then(|panes| panes.as_array_mut()) {
        for pane in panes {
            let size = pane
                .get("project")
                .and_then(serde_json::Value::as_u64)
                .and_then(|project| splits.get(&project.to_string()))
                .copied()
                .unwrap_or("whole");
            pane["size"] = serde_json::Value::String(size.to_owned());
        }
    }
    if let Some(row) = row.as_object_mut() {
        row.remove("splits");
        row.remove("count");
        row.remove("orient");
    }
    ctx.tx.execute(
        "UPDATE store_meta SET value = ?2 WHERE key = ?1",
        rusqlite::params![LAYOUT, row.to_string()],
    )?;
    Ok(())
}

/// The four plugins Amenbo published itself, whose work the body took over (`AMB-D-881`,
/// `AMB-D-884`). Frozen text, like every step's: whatever this build calls them later, these are the
/// names the rows on disk were written under.
const PLUGINS_TAKEN_IN: &[&str] = &["mail", "slack", "viewer", "worktree"];

/// What the mail and slack manifests both declared as the events a project reported when nobody had
/// ticked anything. A manifest is a file on the plugin's own disk, gone by the time anybody reads this
/// step again, so the list is written out here — a project whose `events` row is absent chose nothing,
/// and this is what "nothing" meant.
const EVENTS_REPORTED_BY_DEFAULT: &[&str] = &[
    "task.created",
    "task.status_changed",
    "task.done",
    "task.rejected",
    "task.due",
    "task.due_tomorrow",
];

/// The `store_meta` key the handover leaves its account under, for the surfaces to say once
/// (`crate::handover`).
const HANDOVER_KEY: &str = "plugins_carried_in";

/// One plugin setting or secret, addressed the way both tables address it.
type PluginRows = std::collections::BTreeMap<(Option<i64>, String, String), String>;

/// v42: **carry what the four official plugins held into the body, then take them away**
/// (`AMB-D-884`).
///
/// `plugin_uninstall` is deliberately not the road (`AMB-D-357`): it always purges the secrets, and the
/// Viewer's `encryption_key` going with them would leave every paired phone reading nothing, with no way
/// back but standing a new server up and photographing a new code on each one. So the taking-in happens
/// first, by hand, and only what has been taken in is removed.
///
/// Where each one lands:
///
/// | plugin | carried to |
/// |---|---|
/// | slack | a `notify_target` on the device's shelf, its webhook in `secret`; the project's own rows say what it reports (`AMB-D-885`) |
/// | mail | the same, the relay on the row and the password in `secret`; `to` becomes the project's `mail_to` |
/// | viewer | the three keys into `secret` under the device's own address (`AMB-D-886`); the switch only where it was off |
/// | worktree | nothing — it never had a setting (`AMB-D-881`) |
///
/// **One target per connection, not one per project.** Three projects pointing at the same webhook were
/// three copies of it; the shelf holds it once and all three select it, which is the whole of why the
/// shelf exists. A migrated target is named after its kind and numbered — there is nothing in a webhook
/// a screen may show, and a name is one edit to change.
///
/// **What a project reported replaces what its row says, rather than joining it.** A project created by
/// a build that already had notifications carries the six events every project starts with, which nobody
/// chose for it; the plugin's set is the one somebody actually ticked. The targets are added rather than
/// replaced — a selection is not restored by dropping one.
///
/// **The carrier's memory is not carried.** `sync-state.json` goes with the plugin's directory, cursor
/// and queue together. Keeping the cursor alone would say the ledger had been read past rows that were
/// copied out and never sent, and those fall out of the ledger's window and are never readable again; a
/// carrier that starts from nothing places the store whole, which costs one large send and no accuracy.
///
/// **Nothing is written to the change feed.** The feed is what an incremental carrier reads, and the
/// only carrier is the Viewer, whose memory this very step throws away — so the rows land in its next
/// whole placement rather than as a page it would never come back for.
///
/// **The execution log is left where it lies.** `plugin-runs.jsonl` is a bounded, machine-local record of
/// what ran and why it failed, and it is the last trace of these four ever having run here; `AMB-D-387`
/// purges a plugin's lines on `plugin_uninstall`, which this deliberately is not. The file goes with the
/// mechanism (v43 below), not with the handover.
///
/// **What is deliberately not rolled back.** The rows ride the step's transaction; removing the
/// plugins' directories cannot. They go last, so an interruption leaves a plugin whose settings are
/// already in the body — inert, since no build after this one runs it — rather than a setting that
/// exists nowhere.
fn carry_the_official_plugins_into_the_body(ctx: &Ctx<'_>) -> Result<()> {
    // Each table is asked for on its own (table_is_here): they arrived at different versions, and a
    // store born after v43 took them away has none of them at all.
    let config = read_plugin_rows(ctx.tx, "plugin_config")?;
    let secret = read_plugin_rows(ctx.tx, "plugin_secret")?;
    let enabled = read_plugin_enables(ctx.tx)?;

    let viewer = carry_the_viewer(ctx, &config, &secret, &enabled)?;
    let carried = carry_the_notifiers(ctx, &config, &secret, &enabled)?;

    // The rows first, then the bodies. Both are "what the plugin was", and neither is read by any build
    // that has run this step.
    let held: Vec<String> = PLUGINS_TAKEN_IN.iter().map(|p| format!("'{p}'")).collect();
    let held = held.join(", ");
    for table in ["plugin_config", "plugin_secret", "plugin_enable", "plugin_queue"] {
        if !table_is_here(ctx.tx, table)? {
            continue;
        }
        ctx.tx.execute_batch(&format!("DELETE FROM {table} WHERE plugin IN ({held});"))?;
    }

    let mut found: Vec<&str> = Vec::new();
    for plugin in PLUGINS_TAKEN_IN {
        let dir = ctx.base_dir.join("plugins").join(plugin);
        let installed = dir.is_dir();
        if installed {
            found.push(plugin);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    if found.is_empty() && carried.targets == 0 && !viewer {
        return Ok(());
    }
    let note = serde_json::json!({
        "plugins": found,
        "targets": carried.targets,
        "projects": carried.projects,
        "viewer": viewer,
        "told": Vec::<String>::new(),
    });
    ctx.tx.execute(
        "INSERT INTO store_meta (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![HANDOVER_KEY, note.to_string()],
    )?;
    Ok(())
}

/// Every row of one of the two plugin tables, keyed by the address both of them use.
fn read_plugin_rows(tx: &Transaction<'_>, table: &str) -> Result<PluginRows> {
    if !table_is_here(tx, table)? {
        return Ok(PluginRows::new());
    }
    let sql = format!("SELECT project_id, plugin, field_key, value FROM {table}");
    let mut stmt = tx.prepare(&sql)?;
    let rows = stmt.query_map([], |r| {
        Ok((
            (r.get::<_, Option<i64>>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?),
            r.get::<_, String>(3)?,
        ))
    })?;
    let mut out = PluginRows::new();
    for row in rows {
        let (address, value) = row?;
        out.insert(address, value);
    }
    Ok(out)
}

/// Which `(layer, plugin)` pairs have an enable row — `None` as the layer being the device's
/// (`AMB-D-601`).
fn read_plugin_enables(
    tx: &Transaction<'_>,
) -> Result<std::collections::BTreeSet<(Option<i64>, String)>> {
    if !table_is_here(tx, "plugin_enable")? {
        return Ok(std::collections::BTreeSet::new());
    }
    let mut stmt = tx.prepare("SELECT project_id, plugin FROM plugin_enable")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, String>(1)?)))?;
    rows.collect::<rusqlite::Result<_>>().map_err(Into::into)
}

/// One plugin's value for a field at a layer, `None` where the row is absent or empty — an empty
/// required-text column is what a half-written row carries, and it says no more than an absent one.
fn plugin_value(rows: &PluginRows, project: Option<i64>, plugin: &str, key: &str) -> Option<String> {
    rows.get(&(project, plugin.to_string(), key.to_string())).filter(|v| !v.is_empty()).cloned()
}

/// A device-layer value, falling back to whatever a project holds. The Viewer declared itself the
/// device's (`AMB-D-601`), but builds before that layer existed wrote its settings per project
/// (`AMB-D-434`) — and a key read from the wrong layer is better than a paired phone that goes dark.
fn device_or_any(rows: &PluginRows, plugin: &str, key: &str) -> Option<String> {
    if let Some(value) = plugin_value(rows, None, plugin, key) {
        return Some(value);
    }
    rows.iter()
        .find(|((_, p, k), v)| p == plugin && k == key && !v.is_empty())
        .map(|(_, v)| v.clone())
}

/// The Viewer's three keys into `secret`, and its switch where it was off. Answers whether anything of
/// the Viewer was there to carry.
///
/// **What is not in those two tables is not reported, and cannot be.** An absent row and an absent
/// table read alike here, and both read as a device that never had the plugin — which is the ordinary
/// case and the one this must stay quiet about. The shape it cannot tell from that is a store that
/// wrote these under a project and later deleted it: the rows went with the project, these two tables
/// keeping the cascade that Amenbo's own settings for a project keep (`RESTRICTED_TABLES`), long
/// before this step ever ran. The notifiers lose theirs the same way and are right to — a notifier is
/// the project's — and the Viewer is the device's, which is what makes the loss silent and wrong.
/// The chain itself carries them, from as far down as the layer key has been open
/// (`the_viewers_keys_ride_the_whole_chain_and_go_with_a_deleted_project`).
fn carry_the_viewer(
    ctx: &Ctx<'_>,
    config: &PluginRows,
    secret: &PluginRows,
    enabled: &std::collections::BTreeSet<(Option<i64>, String)>,
) -> Result<bool> {
    let mut carried = false;
    // The address the body keeps them at hangs off no row: one set of keys per device (`AMB-D-886`).
    for (rows, key) in
        [(config, "worker_url"), (secret, "auth_token"), (secret, "encryption_key")]
    {
        let Some(value) = device_or_any(rows, "viewer", key) else { continue };
        ctx.tx.execute(
            "INSERT OR IGNORE INTO secret \
                 (project_id, area, owner_id, field_key, value, created_at, updated_at) \
             VALUES (NULL, 'viewer', NULL, ?1, ?2, \
                 strftime('%Y-%m-%dT%H:%M:%SZ','now'), strftime('%Y-%m-%dT%H:%M:%SZ','now'))",
            rusqlite::params![key, value],
        )?;
        carried = true;
    }

    // **An absent switch reads as on**, which is right for a device that never had the plugin: standing
    // the server up is what turns the Viewer on, and nobody who never stood one up is carrying. The one
    // device this has to write for is the one that installed the plugin and turned it off — there, an
    // absent row would silently start carrying again.
    let installed = ctx.base_dir.join("plugins").join("viewer").is_dir();
    let switched_on = enabled.iter().any(|(_, plugin)| plugin == "viewer");
    if installed && !switched_on {
        ctx.tx.execute_batch("INSERT OR IGNORE INTO viewer_switch (id, sending) VALUES (1, 0);")?;
        carried = true;
    }
    Ok(carried || installed)
}

/// How much the notifiers' handover came to, for the account the surfaces read.
struct Carried {
    targets: usize,
    projects: usize,
}

/// Mail and slack into the device's shelf and each project's notification rows (`AMB-D-885`).
fn carry_the_notifiers(
    ctx: &Ctx<'_>,
    config: &PluginRows,
    secret: &PluginRows,
    enabled: &std::collections::BTreeSet<(Option<i64>, String)>,
) -> Result<Carried> {
    let projects: Vec<i64> = {
        let mut stmt = ctx.tx.prepare("SELECT id FROM project ORDER BY id")?;
        let rows = stmt.query_map([], |r| r.get::<_, i64>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    // One row per connection, however many projects pointed at it. The key is the connection itself, so
    // two projects holding the same webhook meet on the shelf rather than each raising a target.
    let mut shelf: std::collections::BTreeMap<(String, Vec<String>), i64> =
        std::collections::BTreeMap::new();
    let mut named: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut touched = 0usize;

    for project in projects {
        let at = Some(project);
        let mut selected: Vec<i64> = Vec::new();
        let mut events: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut mail_to: Option<String> = None;
        let mut on = false;

        for plugin in ["slack", "mail"] {
            let cfg = |key: &str| plugin_value(config, at, plugin, key);
            let connection: Vec<String> = match plugin {
                "slack" => match plugin_value(secret, at, plugin, "webhook_url") {
                    Some(url) => vec![url],
                    // A slack target *is* its webhook: with none there is no connection to shelve.
                    None => continue,
                },
                _ => {
                    let relay = [
                        cfg("smtp_host"),
                        cfg("smtp_port"),
                        cfg("smtp_user"),
                        cfg("from"),
                        plugin_value(secret, at, plugin, "smtp_password"),
                    ];
                    if relay.iter().all(Option::is_none) {
                        continue;
                    }
                    relay.into_iter().map(Option::unwrap_or_default).collect()
                }
            };
            on |= enabled.contains(&(at, plugin.to_string()));
            events.extend(match cfg("events") {
                Some(chosen) => chosen.split(',').map(str::trim).map(str::to_string).collect(),
                None => EVENTS_REPORTED_BY_DEFAULT.iter().map(|e| (*e).to_string()).collect::<Vec<_>>(),
            });
            if plugin == "mail" {
                mail_to = cfg("to");
            }
            let kind = if plugin == "slack" { "slack" } else { "mail" };
            let id = match shelf.get(&(kind.to_string(), connection.clone())) {
                Some(id) => *id,
                None => {
                    let nth = named.entry(kind).and_modify(|n| *n += 1).or_insert(1);
                    let id = raise_target(ctx, kind, *nth, &connection)?;
                    shelf.insert((kind.to_string(), connection), id);
                    id
                }
            };
            selected.push(id);
        }

        if selected.is_empty() {
            continue;
        }
        touched += 1;
        settle_project_notify(ctx, project, on, mail_to.as_deref())?;
        for target in selected {
            ctx.tx.execute(
                "INSERT OR IGNORE INTO project_notify_target \
                     (project_id, target_id, created_at, updated_at) \
                 VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%SZ','now'), \
                     strftime('%Y-%m-%dT%H:%M:%SZ','now'))",
                rusqlite::params![project, target],
            )?;
        }
        // Replaced, not joined: see this step's own docs.
        ctx.tx.execute("DELETE FROM project_notify_event WHERE project_id = ?1", [project])?;
        for event in &events {
            // A name this build's column does not admit is dropped rather than refused: the plugin's
            // catalog and the column's are the same thirteen, and a row that is neither is a value
            // nothing was ever going to fire for.
            if !EVENT_NAMES_THE_BODY_REPORTS.contains(&event.as_str()) {
                continue;
            }
            ctx.tx.execute(
                "INSERT OR IGNORE INTO project_notify_event \
                     (project_id, event, created_at, updated_at) \
                 VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%SZ','now'), \
                     strftime('%Y-%m-%dT%H:%M:%SZ','now'))",
                rusqlite::params![project, event],
            )?;
        }
    }

    Ok(Carried { targets: shelf.len(), projects: touched })
}

/// The thirteen a project may report, frozen against the column's own `CHECK` — an event the plugin
/// held that is not one of these is dropped rather than taking the step down.
const EVENT_NAMES_THE_BODY_REPORTS: &[&str] = &[
    "task.created",
    "task.status_changed",
    "task.done",
    "task.rejected",
    "task.assigned",
    "task.moved",
    "task.deleted",
    "decision.accepted",
    "decision.rejected",
    "comment.added",
    "comment.removed",
    "task.due",
    "task.due_tomorrow",
];

/// Put one connection on the device's shelf and hand back its id. `nth` numbers it within its kind, so
/// a device carrying two Slack channels reads as `Slack` and `Slack 2` rather than as one name twice.
/// `connection` is the relay's five values for a mail target, and the webhook alone for a slack one.
fn raise_target(ctx: &Ctx<'_>, kind: &str, nth: usize, connection: &[String]) -> Result<i64> {
    let name = match (kind, nth) {
        ("slack", 1) => "Slack".to_string(),
        ("slack", n) => format!("Slack {n}"),
        (_, 1) => "Mail".to_string(),
        (_, n) => format!("Mail {n}"),
    };
    // The first one carried is where a project made after this starts out (`AMB-D-885`), and a device
    // that already marked one keeps its mark: the partial index cannot say "at most one true", so the
    // mark is only laid where nothing carries it.
    let marked: bool =
        ctx.tx.prepare("SELECT 1 FROM notify_target WHERE is_default = 1")?.exists([])?;
    let empty = String::new();
    let value = |n: usize| connection.get(n).unwrap_or(&empty).clone();
    let (host, port, user, from) = match kind {
        "slack" => (None, None, None, None),
        _ => (
            Some(value(0)),
            value(1).parse::<i64>().ok(),
            Some(value(2)),
            Some(value(3)),
        ),
    };
    ctx.tx.execute(
        "INSERT INTO notify_target \
             (kind, name, is_default, smtp_host, smtp_port, smtp_user, mail_from, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, strftime('%Y-%m-%dT%H:%M:%SZ','now'), \
             strftime('%Y-%m-%dT%H:%M:%SZ','now'))",
        rusqlite::params![kind, name, i64::from(!marked), host, port, user, from],
    )?;
    let id = ctx.tx.last_insert_rowid();
    let (field, held) = match kind {
        "slack" => ("webhook_url", value(0)),
        _ => ("smtp_password", value(4)),
    };
    if !held.is_empty() {
        ctx.tx.execute(
            "INSERT OR IGNORE INTO secret \
                 (project_id, area, owner_id, field_key, value, created_at, updated_at) \
             VALUES (NULL, 'notify', ?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%SZ','now'), \
                 strftime('%Y-%m-%dT%H:%M:%SZ','now'))",
            rusqlite::params![id, field, held],
        )?;
    }
    Ok(id)
}

/// This project's notification row, raised or brought level with what the plugin held. `mail_to` is left
/// alone where the plugin named nobody — an empty one means "the relay's own account", which is what the
/// row already says.
fn settle_project_notify(
    ctx: &Ctx<'_>,
    project: i64,
    enabled: bool,
    mail_to: Option<&str>,
) -> Result<()> {
    ctx.tx.execute(
        "INSERT INTO project_notify (project_id, enabled, mail_to, created_at, updated_at) \
         VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%SZ','now'), \
             strftime('%Y-%m-%dT%H:%M:%SZ','now')) \
         ON CONFLICT(project_id) DO UPDATE SET \
             enabled = excluded.enabled, \
             mail_to = CASE WHEN excluded.mail_to = '' THEN project_notify.mail_to \
                            ELSE excluded.mail_to END, \
             updated_at = excluded.updated_at",
        rusqlite::params![project, i64::from(enabled), mail_to.unwrap_or_default()],
    )?;
    Ok(())
}

/// v41: the Viewer's switch, and the moment it last placed anything (`AMB-D-884`).
fn add_the_viewers_switch(ctx: &Ctx<'_>) -> Result<()> {
    ctx.tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS viewer_switch (\
             id INTEGER PRIMARY KEY CHECK (id = 1) NOT NULL, \
             sending INTEGER CHECK(sending IN (0, 1)) NOT NULL\
         );",
    )?;
    let already: bool = ctx
        .tx
        .prepare("SELECT 1 FROM pragma_table_info('viewer_send') WHERE name = 'last_placed_at'")?
        .exists([])?;
    if !already {
        ctx.tx.execute_batch("ALTER TABLE viewer_send ADD COLUMN last_placed_at TEXT;")?;
    }
    Ok(())
}

/// v23: give the change feed the window each instruction belongs to (`AMB-D-582`), so a reader closed to
/// one project can be handed its own changes — a question the row itself cannot answer once it is gone.
///
/// Probed rather than bare, for the reason [`add_outbox_project`] gives: a store handed the table whole
/// by today's genesis already has the column, and a bare `ALTER TABLE … ADD COLUMN` would fail on exactly
/// those with `duplicate column name`.
///
/// **The rows already in the feed stay unstamped, and the store says how far that reaches.** There is
/// nothing to derive them from: a change is attributed from what the write door declared it touched, and
/// no build before this one wrote that down — the rows a deletion names are gone, and a re-homed row
/// names only where it landed. Backfilling from the live rows would therefore be a guess that is silently
/// wrong on exactly the changes a carrier most needs. So the watermark records where stamping begins, and
/// a window whose cursor is below it is told its cursor is gone rather than handed a page with holes in
/// it. No window can hold such a cursor yet — the road that reads the feed from one arrives with this
/// column — so this costs nobody a reconcile in practice; it is what keeps the silent hole from existing.
fn add_feed_project(ctx: &Ctx<'_>) -> Result<()> {
    let held: i64 = ctx.tx.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('change_feed') WHERE name = 'project'",
        [],
        |r| r.get(0),
    )?;
    if held == 0 {
        // Frozen text, like every step's: a nullable integer, whatever the registry names the kind later.
        ctx.tx.execute_batch("ALTER TABLE change_feed ADD COLUMN project BIGINT;")?;
    }
    let head: i64 = ctx.tx.query_row("SELECT COALESCE(MAX(id), 0) FROM change_feed", [], |r| r.get(0))?;
    if head > 0 {
        ctx.tx.execute(
            "INSERT INTO store_meta (key, value) VALUES ('change_feed_windows_from', ?1) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [head.to_string()],
        )?;
    }
    Ok(())
}

/// v17: build the word index for a store whose records predate it. It is exactly the rebuild any repair
/// would run (`AMB-D-450` — the index holds no truth of its own), so there is nothing here for the chain
/// to freeze: a store carrying no text ends with an empty index, which is what an empty store means.
fn fill_the_word_index(ctx: &Ctx<'_>) -> Result<()> {
    super::search::rebuild(ctx.tx)?;
    Ok(())
}

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

/// Whether `table` is in this store at all — what every step touching a table the registry no longer
/// declares has to ask first (`AMB-D-884`).
///
/// A step older than v43 may name one of the plugin mechanism's tables, and genesis builds a store from
/// **today's** registry, which no longer declares them. So a store born before such a step reaches it
/// without the table the step was written against: it has nothing to carry, and saying so is the honest
/// answer rather than a failure. A store that does have it — one written by a build from before the
/// registry moved — is the one the step still exists for.
fn table_is_here(tx: &rusqlite::Transaction<'_>, table: &str) -> Result<bool> {
    let held: i64 = tx.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |r| r.get(0),
    )?;
    Ok(held > 0)
}

/// v11: give the outbox the project column the fan-out routes on (`AMB-D-405`).
///
/// **Why this is not one `ALTER TABLE`.** The outbox itself arrived after the baseline, so a store older
/// than the table is handed it whole by genesis — built from today's registry, `project` included — and a
/// bare `ALTER TABLE … ADD COLUMN` would then fail with `duplicate column name` on exactly the oldest
/// stores. The probe is what makes one step serve both shapes: the store that has the column was born with
/// it, and the store that does not is the one this step exists for. Same shape as v6's, for the same
/// reason.
///
/// **The table itself is probed too**, as [`add_queue_project`]'s is. Since v44 the outbox is spelled
/// `outbox`, so genesis no longer hands a store older than the table one under this name — a store below
/// v4 arrives here with no `plugin_outbox` at all, and the column it would gain is already on the
/// `outbox` genesis gave it.
fn add_outbox_project(ctx: &Ctx<'_>) -> Result<()> {
    if !table_is_here(ctx.tx, "plugin_outbox")? {
        return Ok(());
    }
    let held: i64 = ctx.tx.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('plugin_outbox') WHERE name = 'project'",
        [],
        |r| r.get(0),
    )?;
    if held == 0 {
        // Frozen text, like every step's: a nullable integer, whatever the registry names the kind later.
        ctx.tx.execute_batch("ALTER TABLE plugin_outbox ADD COLUMN project BIGINT;")?;
    }
    Ok(())
}

/// v12: give a queue row the project its event was fanned out for (`AMB-D-405`).
///
/// Probed rather than bare, for the same reason [`add_outbox_project`] is: the queue arrived after the
/// baseline too, so the oldest stores are handed the table whole by genesis — `project` included — and a
/// bare `ALTER TABLE … ADD COLUMN` would fail on exactly those with `duplicate column name`.
fn add_queue_project(ctx: &Ctx<'_>) -> Result<()> {
    if !table_is_here(ctx.tx, "plugin_queue")? {
        return Ok(());
    }
    let held: i64 = ctx.tx.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('plugin_queue') WHERE name = 'project'",
        [],
        |r| r.get(0),
    )?;
    if held == 0 {
        // Frozen text, like every step's: a nullable integer, whatever the registry names the kind later.
        ctx.tx.execute_batch("ALTER TABLE plugin_queue ADD COLUMN project BIGINT;")?;
    }
    Ok(())
}

/// v13: carry the vanished record's shape on the two tables a deletion travels through (`AMB-D-407`).
///
/// Probed rather than bare, for the reason [`add_outbox_project`] gives: both tables arrived after the
/// baseline, so the oldest stores are handed them whole by genesis — `record` included — and a bare
/// `ALTER TABLE … ADD COLUMN` would fail on exactly those with `duplicate column name`.
fn add_gone_record(ctx: &Ctx<'_>) -> Result<()> {
    for table in ["plugin_outbox", "plugin_queue"] {
        if !table_is_here(ctx.tx, table)? {
            continue;
        }
        let held: i64 = ctx.tx.query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = 'record'",
            [table],
            |r| r.get(0),
        )?;
        if held == 0 {
            // Frozen text, like every step's: a nullable text column, whatever the registry names the
            // kind later.
            ctx.tx.execute(&format!("ALTER TABLE {table} ADD COLUMN record TEXT"), [])?;
        }
    }
    Ok(())
}

/// v14: name the record a vanished child hung on, on the two tables a deletion travels through
/// (`AMB-D-407`).
///
/// Probed rather than bare, for the reason [`add_outbox_project`] gives.
fn add_parent(ctx: &Ctx<'_>) -> Result<()> {
    for table in ["plugin_outbox", "plugin_queue"] {
        if !table_is_here(ctx.tx, table)? {
            continue;
        }
        let held: i64 = ctx.tx.query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = 'parent'",
            [table],
            |r| r.get(0),
        )?;
        if held == 0 {
            // Frozen text, like every step's: a nullable integer, whatever the registry names the kind
            // later.
            ctx.tx.execute(&format!("ALTER TABLE {table} ADD COLUMN parent BIGINT"), [])?;
        }
    }
    Ok(())
}

/// The three settings tables as they stood at v42, the last version that had them — frozen text, like
/// every step's, and written here because genesis no longer raises them (`AMB-D-884`).
///
/// **A store older than v15 needs them raised before anything can be carried into them.** Genesis builds
/// from today's registry, which no longer declares them, so a store from before they existed now arrives
/// at v15 with nowhere to put what `plugin-secrets.json` holds — and v42, which moves what the four
/// official plugins held into the body, would then find nothing and the credentials would be gone.
/// Raising them here is what keeps that upgrade whole; v43 takes them away again once everything worth
/// keeping is in the body.
///
/// The `project_id` is the opened form v24 arrives at. A table raised here is raised opened, so v24 finds
/// nothing left to rewrite and passes over it.
const PLUGIN_SETTINGS_TABLES: &str = "\
CREATE TABLE IF NOT EXISTS plugin_config (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT REFERENCES project(id) ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    plugin TEXT NOT NULL DEFAULT '',
    field_key TEXT NOT NULL DEFAULT '',
    value TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS plugin_secret (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT REFERENCES project(id) ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    plugin TEXT NOT NULL DEFAULT '',
    field_key TEXT NOT NULL DEFAULT '',
    value TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS plugin_enable (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT REFERENCES project(id) ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    plugin TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL DEFAULT ''
);
CREATE UNIQUE INDEX IF NOT EXISTS plugin_config_triple ON plugin_config(project_id, plugin, field_key);
CREATE UNIQUE INDEX IF NOT EXISTS plugin_secret_triple ON plugin_secret(project_id, plugin, field_key);
CREATE UNIQUE INDEX IF NOT EXISTS plugin_enable_pair ON plugin_enable(project_id, plugin);
";

/// v15: carry a plugin's settings and secrets from the user area into each project's rows (`AMB-D-434`),
/// and take the two user-area homes away.
///
/// Everything here names its tables, columns and JSON keys in frozen text, per this module's contract.
/// The instant stamped on a carried row is SQLite's own `strftime` in the audit columns' format, not a
/// helper this crate could reshape later.
///
/// **What is deliberately not rolled back.** The row writes ride the step's transaction; removing
/// `config.json`'s plugin key and deleting `plugin-secrets.json` cannot — a file is not transactional.
/// They are done last, after the rows are in hand, so an interruption leaves a readable copy of what was
/// already carried rather than a value that exists nowhere. Residue in the other direction (files that
/// outlive a commit) is inert: nothing reads either home after this build.
fn move_plugin_settings_into_the_store(ctx: &Ctx<'_>) -> Result<()> {
    // Raised first, because genesis no longer does (PLUGIN_SETTINGS_TABLES): a store older than these
    // tables would otherwise have nowhere to carry the two homes to, and the credentials in them would go
    // with the files at the end of this step.
    ctx.tx.execute_batch(PLUGIN_SETTINGS_TABLES)?;

    let projects: Vec<i64> = {
        let mut stmt = ctx.tx.prepare("SELECT id FROM project")?;
        let rows = stmt.query_map([], |r| r.get::<_, i64>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    // `OR IGNORE` is what lets a machine-wide default stand down where a project answered for itself:
    // the `(project_id, plugin, field_key)` unique index refuses the second row, so the project's own
    // value — the closer of the two answers — is the one left standing.
    for (table, values) in [
        ("plugin_config", read_user_area_map(&ctx.base_dir.join("config.json"), Some("plugin_config"))),
        ("plugin_secret", read_user_area_map(&ctx.base_dir.join("plugin-secrets.json"), None)),
    ] {
        let sql = format!(
            "INSERT OR IGNORE INTO {table}(project_id, plugin, field_key, value, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%SZ','now'), strftime('%Y-%m-%dT%H:%M:%SZ','now'))"
        );
        let mut stmt = ctx.tx.prepare(&sql)?;
        for (plugin, key, value) in values {
            for project in &projects {
                stmt.execute(rusqlite::params![project, &plugin, &key, &value])?;
            }
        }
    }

    drop_config_plugin_key(ctx.base_dir);
    let _ = std::fs::remove_file(ctx.base_dir.join("plugin-secrets.json"));
    Ok(())
}

/// The `{ "<plugin>": { "<key>": "<value>" } }` document both user-area homes were, flattened to
/// `(plugin, key, value)`. `key` names the field to read it out of when the map is nested inside a larger
/// document (`config.json`); `None` is the whole file (`plugin-secrets.json`). A file that is absent,
/// unreadable or not of that shape carries nothing — a store with no plugin settings is the ordinary case,
/// and refusing to migrate over a malformed preferences file would be the worse of the two outcomes.
fn read_user_area_map(path: &Path, field: Option<&str>) -> Vec<(String, String, String)> {
    let Some(doc) = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
    else {
        return Vec::new();
    };
    let map = match field {
        Some(name) => doc.get(name),
        None => Some(&doc),
    };
    let Some(map) = map.and_then(|v| v.as_object()) else { return Vec::new() };
    let mut out = Vec::new();
    for (plugin, fields) in map {
        let Some(fields) = fields.as_object() else { continue };
        for (key, value) in fields {
            if let Some(value) = value.as_str() {
                out.push((plugin.clone(), key.clone(), value.to_string()));
            }
        }
    }
    out
}

/// Take the `plugin_config` key out of `config.json`, leaving every other key exactly as it was — the
/// read-modify-write twin of [`write_config_hook_consent`], and frozen for the same reason.
fn drop_config_plugin_key(base_dir: &Path) {
    let path = base_dir.join("config.json");
    let Some(mut doc) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
    else {
        return;
    };
    let Some(obj) = doc.as_object_mut() else { return };
    if obj.remove("plugin_config").is_none() {
        return;
    }
    if let Ok(text) = serde_json::to_string_pretty(&doc) {
        let _ = crate::store::write_atomic(&path, text.as_bytes());
    }
}

/// v9: widen `task.status`'s closed set by one value (`AMB-D-397`).
///
/// **Why this is not SQL.** SQLite has no `ALTER TABLE … DROP CONSTRAINT`; the documented way to change a
/// `CHECK` is to build a new table, copy the rows, drop the old one and rename — and that path is closed
/// here. Enforcement is on (`super::engine::init`), six child tables reference `task` with
/// `ON DELETE CASCADE`, and dropping a referenced table performs an implicit `DELETE` that fires those
/// actions: the rebuild would take every comment, dependency, dimension assignment, decision link and
/// commit anchor with it. The escape SQLite prescribes — `PRAGMA foreign_keys = OFF` around the swap — is
/// a no-op inside a transaction, and a step *is* a transaction (that is what commits the version stamp
/// with the change). So the table is left exactly where it is and only its declaration is rewritten.
///
/// **Why the text is read rather than written.** A step's SQL is normally frozen text, but the whole
/// declaration cannot be: `status_changed_at` reaches a store either as a column of its birth
/// `CREATE TABLE` (registry order) or appended by v6's `ALTER TABLE`, so two stores at the same version
/// legitimately carry the same columns in different order. Writing one fixed declaration over both would
/// make one of them describe a table that is not there. What *is* frozen is the clause: the closed set
/// [`enum_col`](super::schema) emits has been one string since the baseline, so the step takes the store's
/// own declaration and replaces that clause alone — everything else, column order included, survives
/// untouched. A store whose declaration does not carry it is refused rather than guessed at.
///
/// The rewrite is checked both ways round it: the column list before and after must be identical (this
/// changes a constraint, never the shape), and `writable_schema` is shut again before anything else can
/// fail, because it is connection state that would outlive this step's rolled-back transaction.
fn admit_rejected_task_status(ctx: &Ctx<'_>) -> Result<()> {
    /// The closed set as every store from the baseline on declares it — frozen text, like every step's.
    const NARROW: &str = "CHECK(status IN ('', 'todo', 'in_progress', 'done', 'blocked'))";
    /// The same set with the new terminal in it.
    const WIDE: &str = "CHECK(status IN ('', 'todo', 'in_progress', 'done', 'blocked', 'rejected'))";

    let declared: String = ctx.tx.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'task'",
        [],
        |r| r.get(0),
    )?;
    if declared.contains(WIDE) {
        // Already wide: a store born from a registry that carries the value, stamped back to an earlier
        // version. Nothing to widen, and nothing wrong.
        return Ok(());
    }
    if !declared.contains(NARROW) {
        return Err(super::StoreEngineError::UnrecognisedDdl { table: "task", expected: NARROW });
    }
    let widened = declared.replace(NARROW, WIDE);

    let before = column_names(ctx.tx, "task")?;
    ctx.tx.execute_batch("PRAGMA writable_schema = ON;")?;
    let wrote = ctx.tx.execute(
        "UPDATE sqlite_master SET sql = ?1 WHERE type = 'table' AND name = 'task'",
        [&widened],
    );
    // `RESET` both shuts the door and drops the connection's parsed schema, so the very next statement
    // sees the widened `CHECK` instead of the one this connection read at open.
    ctx.tx.execute_batch("PRAGMA writable_schema = RESET;")?;
    wrote?;
    let after = column_names(ctx.tx, "task")?;
    if before != after {
        return Err(super::StoreEngineError::UnrecognisedDdl { table: "task", expected: NARROW });
    }
    Ok(())
}

/// v48: the decision's closed set loses a value and renames another — `proposed` / `accepted` /
/// `rejected` becomes `decided` / `rejected` (`AMB-D-918`).
///
/// **The declaration first, the rows second, one transaction.** SQLite checks a `CHECK` on write and
/// never on the rows already there, so narrowing the set while `accepted` is still written in the
/// column is safe — and it is the only order that works, because the `UPDATE` that writes `decided`
/// would be refused by the set it is moving out of. Both halves ride the step's transaction, so a
/// store never comes to rest holding one without the other.
///
/// **This is v9's procedure met a fourth time, copied rather than called.** SQLite has no
/// `ALTER TABLE … DROP CONSTRAINT`, and the rebuild-and-swap its documentation prescribes is closed for
/// the reason [`admit_rejected_task_status`] gives at length — enforcement is on and four tables
/// reference `decision`, so dropping it would fire them. A step is frozen at the meaning it had when it
/// was written, so it names its own clause in its own text: folded into a helper, an edit to that
/// helper would reach back into stores migrated years ago. What is *not* frozen is the declaration
/// around the clause — `draft` reaches a store either from its birth `CREATE TABLE` or from v47's
/// `ALTER TABLE`, so two stores at this version carry the same columns in a different order, which is
/// why the text is read rather than written.
///
/// **Where each old value lands.** `accepted` is `decided` under a new name: same rows, same
/// `decided_at` / `decided_by`, nothing about them reconsidered. `proposed` goes to `rejected`, which
/// is the only honest home left for it — `draft` was seeded `0` by v47, so a proposal carried over as
/// `decided` would read as a settled policy nobody settled, and one carried over as a draft would be a
/// writing nobody is going to finish. A store where the old build never rejected anything has no
/// `proposed` row to move, and the `UPDATE` touches nothing.
fn fold_the_proposal_away(ctx: &Ctx<'_>) -> Result<()> {
    /// The closed set as every store from the baseline on declares it — frozen text, like every step's.
    const NARROW: &str = "CHECK(status IN ('', 'proposed', 'accepted', 'rejected'))";
    /// The same column with the two values that outlive the acceptance.
    const WIDE: &str = "CHECK(status IN ('', 'decided', 'rejected'))";

    let declared: String = ctx.tx.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'decision'",
        [],
        |r| r.get(0),
    )?;
    if !declared.contains(WIDE) {
        // Not already folded: a store born from a registry that carries the new set, stamped back to an
        // earlier version, is the one that arrives here with nothing to rewrite.
        if !declared.contains(NARROW) {
            return Err(super::StoreEngineError::UnrecognisedDdl {
                table: "decision",
                expected: NARROW,
            });
        }
        let folded = declared.replace(NARROW, WIDE);

        let before = column_names(ctx.tx, "decision")?;
        ctx.tx.execute_batch("PRAGMA writable_schema = ON;")?;
        let wrote = ctx.tx.execute(
            "UPDATE sqlite_master SET sql = ?1 WHERE type = 'table' AND name = 'decision'",
            [&folded],
        );
        // `RESET` both shuts the door and drops the connection's parsed schema, so the `UPDATE` below
        // sees the new set instead of the one this connection read at open.
        ctx.tx.execute_batch("PRAGMA writable_schema = RESET;")?;
        wrote?;
        let after = column_names(ctx.tx, "decision")?;
        if before != after {
            return Err(super::StoreEngineError::UnrecognisedDdl {
                table: "decision",
                expected: NARROW,
            });
        }
    }

    ctx.tx.execute_batch(
        "UPDATE decision SET status = 'decided' WHERE status = 'accepted';
         UPDATE decision SET status = 'rejected' WHERE status = 'proposed';",
    )?;
    Ok(())
}

/// v34: widen `dimension.cardinality`'s closed set by one value (`AMB-D-826`).
///
/// **Why this is not SQL, and not a rebuild either.** It is v9's case on another table: SQLite has no
/// `ALTER TABLE … DROP CONSTRAINT`, and the rebuild-and-swap its documentation prescribes is closed for
/// the reason [`admit_rejected_task_status`] gives at length — enforcement is on, three tables reference
/// `dimension` (`dimension_value`, `task_dimension_value`, `decision_dimension_value`), and dropping a
/// referenced table performs an implicit `DELETE` that fires their `RESTRICT`. So the table is left where
/// it is and only its declaration is rewritten.
///
/// **The clause is frozen, the declaration around it is not.** `applies_to` reaches a store either as a
/// column of its birth `CREATE TABLE` (registry order) or appended by v32's `ALTER TABLE`, so two stores
/// at this version legitimately carry the same columns in a different order — the same reason v9 reads the
/// declaration rather than writing one. What is frozen is the clause [`enum_col`](super::schema) emits,
/// which has read the same since the baseline. A store that does not carry it is refused rather than
/// guessed at.
///
/// The rewrite is checked both ways round it, as v9's is: the column list before and after must be
/// identical, and `writable_schema` is shut before anything else can fail, since it is connection state
/// that would outlive this step's rolled-back transaction.
fn admit_multi_cardinality(ctx: &Ctx<'_>) -> Result<()> {
    /// The closed set as every store from the baseline on declares it — frozen text, like every step's.
    const NARROW: &str = "CHECK(cardinality IN ('', 'single'))";
    /// The same set with the axis's other answer in it.
    const WIDE: &str = "CHECK(cardinality IN ('', 'single', 'multi'))";

    let declared: String = ctx.tx.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'dimension'",
        [],
        |r| r.get(0),
    )?;
    if declared.contains(WIDE) {
        // Already wide: a store born from a registry that carries the value, stamped back to an earlier
        // version. Nothing to widen, and nothing wrong.
        return Ok(());
    }
    if !declared.contains(NARROW) {
        return Err(super::StoreEngineError::UnrecognisedDdl { table: "dimension", expected: NARROW });
    }
    let widened = declared.replace(NARROW, WIDE);

    let before = column_names(ctx.tx, "dimension")?;
    ctx.tx.execute_batch("PRAGMA writable_schema = ON;")?;
    let wrote = ctx.tx.execute(
        "UPDATE sqlite_master SET sql = ?1 WHERE type = 'table' AND name = 'dimension'",
        [&widened],
    );
    // `RESET` both shuts the door and drops the connection's parsed schema, so the very next statement
    // sees the widened `CHECK` instead of the one this connection read at open.
    ctx.tx.execute_batch("PRAGMA writable_schema = RESET;")?;
    wrote?;
    let after = column_names(ctx.tx, "dimension")?;
    if before != after {
        return Err(super::StoreEngineError::UnrecognisedDdl { table: "dimension", expected: NARROW });
    }
    Ok(())
}

/// v35: an axis whose values can be closed (`AMB-D-829`) — one role to nominate it, one flag per value
/// to hold the answer.
///
/// **Two writes, one step, because they are one meaning.** The role without the column nominates an axis
/// for something no value can say, and the column without the role is a flag no door would ever set. A
/// store that carried one and not the other would be a shape this build has no name for.
///
/// **The role half is v9's case, met a third time.** SQLite has no `ALTER TABLE … DROP CONSTRAINT`, and
/// the rebuild-and-swap its documentation prescribes is closed here for the reason
/// [`admit_rejected_task_status`] gives at length — enforcement is on and three tables reference
/// `dimension` with `RESTRICT`, so dropping it would fire them. So the declaration alone is rewritten,
/// checked both ways round, with `writable_schema` shut before anything else can fail.
///
/// **The clause is frozen; this is a copy of v34's procedure and not a call into it.** A step is frozen
/// at the meaning it had when it was written, so it names its own clause in its own text: fold the three
/// rewrites into one helper and an edit to that helper would silently reach back into stores migrated
/// years ago. What is not frozen is the declaration around the clause — `applies_to` reaches a store
/// either from its birth `CREATE TABLE` or from v32's `ALTER TABLE`, so two stores at this version carry
/// the same columns in a different order, which is why the text is read rather than written.
///
/// **The column half needs no backfill.** `closed` is a `BOOLEAN NOT NULL DEFAULT 0`, and `0` is exactly
/// what every value in an upgrading store means: none of them was ever closed, because nothing could
/// close one. That parts it from v32, whose `''` sentinel was no value a reader could hydrate.
fn let_a_value_be_closed(ctx: &Ctx<'_>) -> Result<()> {
    /// The closed set as every store from the baseline on declares it — frozen text, like every step's.
    const NARROW: &str = "CHECK(role IN ('', 'none', 'time_axis'))";
    /// The same set with the closable nomination in it.
    const WIDE: &str = "CHECK(role IN ('', 'none', 'time_axis', 'closable'))";

    let declared: String = ctx.tx.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'dimension'",
        [],
        |r| r.get(0),
    )?;
    if declared.contains(NARROW) {
        let widened = declared.replace(NARROW, WIDE);
        let before = column_names(ctx.tx, "dimension")?;
        ctx.tx.execute_batch("PRAGMA writable_schema = ON;")?;
        let wrote = ctx.tx.execute(
            "UPDATE sqlite_master SET sql = ?1 WHERE type = 'table' AND name = 'dimension'",
            [&widened],
        );
        // `RESET` both shuts the door and drops the connection's parsed schema, so the very next
        // statement sees the widened `CHECK` instead of the one this connection read at open.
        ctx.tx.execute_batch("PRAGMA writable_schema = RESET;")?;
        wrote?;
        let after = column_names(ctx.tx, "dimension")?;
        if before != after {
            return Err(super::StoreEngineError::UnrecognisedDdl {
                table: "dimension",
                expected: NARROW,
            });
        }
    } else if !declared.contains(WIDE) {
        // Neither shape: a declaration this step has no name for, refused rather than guessed at. Already
        // wide is the other way here — a store born from a registry that carries the value, stamped back
        // to an earlier version — and there is nothing to widen and nothing wrong.
        return Err(super::StoreEngineError::UnrecognisedDdl {
            table: "dimension",
            expected: NARROW,
        });
    }

    // The value's half. Frozen text, as every step's is: the registry may rename the column tomorrow,
    // and what this step added must keep meaning what it meant.
    ctx.tx.execute_batch(
        "ALTER TABLE dimension_value ADD COLUMN closed BOOLEAN NOT NULL DEFAULT 0 \
             CHECK(closed IN (0, 1));",
    )?;
    Ok(())
}

/// A table's columns, in physical order — the invariant every declaration rewrite holds itself to, since
/// a rewritten declaration that changed the shape would be a corrupted store rather than a migrated one.
fn column_names(tx: &Transaction<'_>, table: &str) -> Result<Vec<String>> {
    let mut stmt = tx.prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid")?;
    let names =
        stmt.query_map([table], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(names)
}

/// The `ON DELETE` clause a concept reference carried up to v9, and the one v10 leaves it with — frozen
/// text, like every step's. Each carries the whole tail of the declaration `fk!` emitted, not the two
/// words alone, so a rewrite can only land on a reference's clause and never on some other `CASCADE`.
const REFERENCE_CASCADES: &str = "ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED";
/// The same clause, restricted (`AMB-D-403`).
const REFERENCE_RESTRICTS: &str =
    "ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED";

/// The tables v10 rewrites, and how many references each of them declared. The count is what makes the
/// rewrite exact: a table that has grown a reference since would otherwise have that one changed too,
/// silently and outside what the decision named. What is deliberately absent is Amenbo's own settings for
/// a project — `plugin_config`, `plugin_enable`, `hook_optout` — which keep their cascade.
const RESTRICTED_TABLES: &[(&str, usize)] = &[
    ("task_comment", 1),
    ("decision_comment", 1),
    ("task_dependency", 2),
    ("task_commit", 1),
    ("decision_task_link", 2),
    ("decision_edge", 2),
    ("dimension_value", 1),
    ("task_dimension_value", 3),
];

/// v10: hold the rows that stand for a concept to `RESTRICT` (`AMB-D-403`).
///
/// **Why this is not SQL, and not a rebuild either.** SQLite has no `ALTER TABLE … DROP CONSTRAINT`, and
/// the rebuild-and-swap its documentation prescribes is closed for the reason [`admit_rejected_task_status`]
/// gives at length: dropping a referenced table performs an implicit `DELETE` that fires the very actions
/// being changed, and the `PRAGMA foreign_keys = OFF` escape is a no-op inside the transaction a step is.
/// So each table stays where it is and only its declaration is rewritten — the same handle v9 used, on a
/// different clause.
///
/// **Read everything before writing anything.** Every table is recognised first, and a store that does not
/// declare what this step expects is refused with nothing written — a half-restricted store would carry a
/// version stamp saying the whole set had moved. A table already restricted is passed over rather than
/// refused: a store born from a registry that carries the clause, stamped back to an earlier version, has
/// nothing left for this step to do.
///
/// The check on the way out is the same as v9's: the column list before and after must be identical, since
/// this changes a constraint and never the shape. It runs after `writable_schema` is shut, which is both
/// where the connection re-parses what was written and where that door has to close regardless.
fn restrict_the_concept_references(ctx: &Ctx<'_>) -> Result<()> {
    let mut rewrites = Vec::new();
    for (table, references) in RESTRICTED_TABLES {
        let declared: String = ctx.tx.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |r| r.get(0),
        )?;
        if !declared.contains(REFERENCE_CASCADES)
            && declared.matches(REFERENCE_RESTRICTS).count() == *references
        {
            continue;
        }
        if declared.matches(REFERENCE_CASCADES).count() != *references {
            return Err(super::StoreEngineError::UnrecognisedDdl {
                table,
                expected: REFERENCE_CASCADES,
            });
        }
        let restricted = declared.replace(REFERENCE_CASCADES, REFERENCE_RESTRICTS);
        rewrites.push((*table, restricted, column_names(ctx.tx, table)?));
    }

    ctx.tx.execute_batch("PRAGMA writable_schema = ON;")?;
    let wrote = rewrites.iter().try_for_each(|(table, sql, _)| {
        ctx.tx
            .execute(
                "UPDATE sqlite_master SET sql = ?1 WHERE type = 'table' AND name = ?2",
                rusqlite::params![sql, table],
            )
            .map(|_| ())
    });
    // `RESET` both shuts the door and drops the connection's parsed schema, so the very next statement
    // sees the restricted references instead of the ones this connection read at open.
    ctx.tx.execute_batch("PRAGMA writable_schema = RESET;")?;
    wrote?;

    for (table, _, before) in &rewrites {
        if column_names(ctx.tx, table)? != *before {
            return Err(super::StoreEngineError::UnrecognisedDdl {
                table,
                expected: REFERENCE_CASCADES,
            });
        }
    }
    Ok(())
}

/// The `project_id` declaration a plugin's three settings tables carried up to v23 — a key every row had
/// to hold — and the one v24 leaves them with, where NULL is admissible and means the device layer
/// (`AMB-D-601`). Frozen text, like every step's, and the whole column line rather than the two words that
/// move: `NOT NULL DEFAULT 0` appears on other columns too, and a rewrite has to land on this one alone.
const PROJECT_KEY_REQUIRED: &str = "project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) \
     ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED";
/// The same line, opened (`AMB-D-601`). The `DEFAULT 0` goes with the `NOT NULL` it existed for: `0` was
/// the not-yet-written sentinel a deferred reference passes through, and it would now read as a silent
/// dangling key on a table where the absent value is NULL and means something.
const PROJECT_KEY_OPTIONAL: &str = "project_id BIGINT REFERENCES project(id) \
     ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED";

/// The three tables v24 opens: everything a plugin's layer decides where to write (`AMB-D-601`).
const LAYERED_TABLES: &[&str] = &["plugin_config", "plugin_secret", "plugin_enable"];

/// v24: let a plugin's gate, settings and secrets be written at the **device** layer (`AMB-D-601`) — the
/// row whose `project_id` is NULL, which a `scope: machine` plugin holds one of for the whole machine.
///
/// **Why this is a declaration rewrite.** Dropping a `NOT NULL` is not something `ALTER TABLE` can do, and
/// the rebuild-and-swap SQLite's documentation prescribes is closed here for the reason
/// [`restrict_the_concept_references`] gives: dropping a table that `project` references performs an
/// implicit `DELETE` that fires the very cascades these tables are built on. So the tables stay where they
/// are and only their stored declaration is rewritten — the same handle v9 and v10 used, on a third clause.
///
/// **Nothing is written to the rows.** Every row already holds a project's id, and every one of them still
/// means what it did: the layer this opens is a place no row stands yet. The device rows' uniqueness comes
/// from the partial indexes in the genesis batch, which a store reaching here already carries — they are
/// legal (and empty) on the `NOT NULL` shape too, which is why they are not this step's to create.
///
/// Read everything before writing anything, and check the column list on the way out, both for the reasons
/// [`restrict_the_concept_references`] states. A table already opened is passed over rather than refused.
fn open_the_plugin_layer_key(ctx: &Ctx<'_>) -> Result<()> {
    let mut rewrites = Vec::new();
    for &table in LAYERED_TABLES {
        if !table_is_here(ctx.tx, table)? {
            continue;
        }
        let declared: String = ctx.tx.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |r| r.get(0),
        )?;
        if declared.contains(PROJECT_KEY_OPTIONAL) {
            continue;
        }
        if declared.matches(PROJECT_KEY_REQUIRED).count() != 1 {
            return Err(super::StoreEngineError::UnrecognisedDdl {
                table,
                expected: PROJECT_KEY_REQUIRED,
            });
        }
        let opened = declared.replace(PROJECT_KEY_REQUIRED, PROJECT_KEY_OPTIONAL);
        rewrites.push((table, opened, column_names(ctx.tx, table)?));
    }

    ctx.tx.execute_batch("PRAGMA writable_schema = ON;")?;
    let wrote = rewrites.iter().try_for_each(|(table, sql, _)| {
        ctx.tx
            .execute(
                "UPDATE sqlite_master SET sql = ?1 WHERE type = 'table' AND name = ?2",
                rusqlite::params![sql, table],
            )
            .map(|_| ())
    });
    ctx.tx.execute_batch("PRAGMA writable_schema = RESET;")?;
    wrote?;

    for (table, _, before) in &rewrites {
        if column_names(ctx.tx, table)? != *before {
            return Err(super::StoreEngineError::UnrecognisedDdl {
                table,
                expected: PROJECT_KEY_REQUIRED,
            });
        }
    }
    Ok(())
}

/// The shape `binding_project_dir` takes from v25 on — frozen text, like every step's: the registry may
/// rename or reshape the table tomorrow, and what this step built must keep meaning what it meant. Built
/// under a name of its own and renamed into place, which is the whole of the rebuild below.
const BINDINGS_KEYED_BY_ID: &str = "CREATE TABLE binding_project_dir_v25 (\n    \
       id         INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,\n    \
       project_id BIGINT NOT NULL,\n    \
       dir        TEXT NOT NULL,\n    \
       UNIQUE (project_id, dir)\n\
     );";

/// v25: give each bound folder an `id` something else can point at (`AMB-D-648`) — a task says which
/// bound folder it is worked in, and it says so by this id rather than by the path, so moving or renaming
/// the folder leaves the pointer standing.
///
/// **A rebuild, unlike v9/v10/v24.** Those three rewrote a stored declaration in place because the tables
/// they touched are held by `REFERENCES` on both sides, and dropping such a table performs an implicit
/// `DELETE` that fires the very actions being changed. This table is held by none: nothing references it
/// and it references nothing (the folder pointer deliberately outlives the project it names), so
/// SQLite's own rebuild-and-swap is open here — and it is what is needed, since a rowid alias is
/// something no `ALTER TABLE … ADD COLUMN` can add.
///
/// **The pairs come across as they are, and the ids are new.** There is nothing in an upgrading store to
/// read an id off — no build before this one issued any — so the rows are numbered here, in the set's own
/// ascending order, which makes the numbering the same on every machine that upgrades the same index. The
/// pair stays the row's identity as `UNIQUE (project_id, dir)`: the key moves from being the pair to being
/// the id, and what the pair meant — one folder recorded for one project once — is unchanged.
///
/// **Probed, not bare.** A store that never had the table is handed it whole by genesis, `id` included, so
/// it arrives here already keyed and the rebuild would only renumber what is already right. Same shape as
/// [`add_outbox_project`]'s probe, for the same reason.
fn key_the_bindings_by_id(ctx: &Ctx<'_>) -> Result<()> {
    let held: i64 = ctx.tx.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('binding_project_dir') WHERE name = 'id'",
        [],
        |r| r.get(0),
    )?;
    if held > 0 {
        return Ok(());
    }
    ctx.tx.execute_batch(BINDINGS_KEYED_BY_ID)?;
    ctx.tx.execute_batch(
        "INSERT INTO binding_project_dir_v25 (project_id, dir) \
             SELECT project_id, dir FROM binding_project_dir ORDER BY project_id, dir;
         DROP TABLE binding_project_dir;
         ALTER TABLE binding_project_dir_v25 RENAME TO binding_project_dir;",
    )?;
    Ok(())
}

/// The shape `dimension` takes from v30 on — frozen text, like every step's: the registry may rename or
/// reshape the table tomorrow, and what this step built must keep meaning what it meant.
const DIMENSION_WITH_SLUG: &str = "CREATE TABLE dimension (\n    \
       id           INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,\n    \
       project_id   BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,\n    \
       name         TEXT NOT NULL DEFAULT '',\n    \
       notes        TEXT NOT NULL DEFAULT '',\n    \
       cardinality  TEXT NOT NULL DEFAULT '' CHECK(cardinality IN ('', 'single')),\n    \
       ordered      BOOLEAN NOT NULL DEFAULT 0 CHECK(ordered IN (0, 1)),\n    \
       role         TEXT NOT NULL DEFAULT '' CHECK(role IN ('', 'none', 'time_axis')),\n    \
       show_on_card BOOLEAN NOT NULL DEFAULT 0 CHECK(show_on_card IN (0, 1)),\n    \
       required     BOOLEAN NOT NULL DEFAULT 0 CHECK(required IN (0, 1)),\n    \
       order_key    TEXT NOT NULL DEFAULT '',\n    \
       slug         TEXT,\n    \
       created_at   TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),\n    \
       updated_at   TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),\n    \
       UNIQUE (project_id, slug)\n\
     );";

/// The shape `dimension_value` takes from v30 on — frozen text, as above.
const DIMENSION_VALUE_WITH_SLUG: &str = "CREATE TABLE dimension_value (\n    \
       id           INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,\n    \
       dimension_id BIGINT NOT NULL DEFAULT 0 REFERENCES dimension(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,\n    \
       name         TEXT NOT NULL DEFAULT '',\n    \
       order_key    TEXT NOT NULL DEFAULT '',\n    \
       slug         TEXT,\n    \
       start_on     TEXT CHECK(start_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),\n    \
       end_on       TEXT CHECK(end_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),\n    \
       created_at   TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),\n    \
       updated_at   TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),\n    \
       UNIQUE (dimension_id, slug)\n\
     );";

/// The columns this step carries across, named in full — the copy is by name, not by `SELECT *`, because
/// a store that reached here from an older shape may hold them in another order (two stores at one
/// version legitimately do).
const DIMENSION_COLUMNS: &str = "id, project_id, name, notes, cardinality, ordered, role, \
     show_on_card, required, order_key, created_at, updated_at";
const DIMENSION_VALUE_COLUMNS: &str =
    "id, dimension_id, name, order_key, start_on, end_on, created_at, updated_at";
const TASK_DIMENSION_VALUE_COLUMNS: &str =
    "id, task_id, dimension_id, value_id, created_at, updated_at";

/// v30: give a classification axis and each of its values a **slug** — a readable, stable key that can
/// be spoken outside Amenbo, where a Japanese display name cannot go and `AMB-DIMV-46` can go but says
/// nothing (`AMB-D-735`).
///
/// **A rebuild, because the uniqueness is a table constraint and nothing else reaches every store.** The
/// column alone would be one `ALTER TABLE … ADD COLUMN` per table; what needs the rebuild is
/// `UNIQUE (project_id, slug)` / `UNIQUE (dimension_id, slug)`. SQLite refuses `ADD COLUMN … UNIQUE`, and
/// a `CREATE UNIQUE INDEX` in the genesis batch would run on every open *before* this chain does and
/// break the store it names a column of that is not there yet. A table constraint is the one form both
/// DDL sites emit, so both halves of the population land on the same table — which is the whole reason
/// [`super::schema::Dataset`] grew a constraint slot.
///
/// **The children are emptied first, and that is not an ornament.** `dimension` is referenced by
/// `dimension_value` and `task_dimension_value`, and with foreign keys on, `DROP TABLE` performs an
/// implicit `DELETE` that fires their `RESTRICT` — immediately, and even under
/// `PRAGMA defer_foreign_keys` the violation simply resurfaces at `COMMIT`, since nothing afterwards
/// decrements the counter it left behind. (This is exactly the case v25's note excludes itself from: the
/// table it rebuilt was held by no reference on either side.) So the assignments go to one side, the two
/// parents are rebuilt with no child row pointing at them, and everything comes back in parent order.
/// `task_dimension_value` keeps its own table throughout — only its rows travel — so its index and its
/// declaration are untouched.
///
/// **Backfilled from the id, never from the name.** `crate::slug::base` keeps runs of ASCII
/// alphanumerics, so a Japanese axis name yields one fallback word and every row in this store would
/// come out of the backfill identical. The id is already unique, so `d<id>` / `v<id>` is unique for
/// free, and the leading letter is what a D-Bus name element needs (`AMB-D-733`) and what the door's own
/// shape check demands. Editing one afterwards is `ops::dimension`'s business, not this step's.
///
/// **The retired numbers are carried across.** A record id, once issued, is never reissued
/// (`schema::RECORD_ID`), and the high-water mark that holds that true lives in `sqlite_sequence`, whose
/// row `DROP TABLE` takes with it. Re-inserting the rows would set the mark back to the largest *live*
/// id, handing a deleted axis's number to the next one, so the mark is read before and written after.
///
/// **Probed, not bare.** A store handed its tables whole by today's genesis already has both columns, so
/// it arrives here finished and the rebuild would only churn it. Same shape as [`add_outbox_project`]'s
/// probe, for the same reason.
fn slug_the_dimension_model(ctx: &Ctx<'_>) -> Result<()> {
    let held: i64 = ctx.tx.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('dimension') WHERE name = 'slug'",
        [],
        |r| r.get(0),
    )?;
    if held > 0 {
        return Ok(());
    }

    // The high-water marks, before the drop takes their rows out of `sqlite_sequence`.
    let mut retired: Vec<(&str, i64)> = Vec::new();
    for table in ["dimension", "dimension_value"] {
        let seq: Option<i64> = ctx
            .tx
            .query_row("SELECT seq FROM sqlite_sequence WHERE name = ?1", [table], |r| r.get(0))
            .optional()?;
        if let Some(seq) = seq {
            retired.push((table, seq));
        }
    }

    ctx.tx.execute_batch(&format!(
        "CREATE TABLE dimension_v30 AS SELECT {DIMENSION_COLUMNS} FROM dimension;
         CREATE TABLE dimension_value_v30 AS SELECT {DIMENSION_VALUE_COLUMNS} FROM dimension_value;
         CREATE TABLE task_dimension_value_v30 AS \
             SELECT {TASK_DIMENSION_VALUE_COLUMNS} FROM task_dimension_value;
         DELETE FROM task_dimension_value;
         DROP TABLE dimension_value;
         DROP TABLE dimension;
         {DIMENSION_WITH_SLUG}
         {DIMENSION_VALUE_WITH_SLUG}
         INSERT INTO dimension ({DIMENSION_COLUMNS}, slug) \
             SELECT {DIMENSION_COLUMNS}, 'd' || id FROM dimension_v30;
         INSERT INTO dimension_value ({DIMENSION_VALUE_COLUMNS}, slug) \
             SELECT {DIMENSION_VALUE_COLUMNS}, 'v' || id FROM dimension_value_v30;
         INSERT INTO task_dimension_value ({TASK_DIMENSION_VALUE_COLUMNS}) \
             SELECT {TASK_DIMENSION_VALUE_COLUMNS} FROM task_dimension_value_v30;
         DROP TABLE dimension_v30;
         DROP TABLE dimension_value_v30;
         DROP TABLE task_dimension_value_v30;"
    ))?;

    for (table, seq) in retired {
        // An `UPDATE` alone would miss a table whose rows were all deleted before this ran: it holds a
        // mark and no rows, so re-inserting nothing leaves it with no `sqlite_sequence` row to update.
        let moved = ctx.tx.execute(
            "UPDATE sqlite_sequence SET seq = ?1 WHERE name = ?2 AND seq < ?1",
            rusqlite::params![seq, table],
        )?;
        if moved == 0 {
            ctx.tx.execute(
                "INSERT INTO sqlite_sequence (name, seq) SELECT ?2, ?1 \
                 WHERE NOT EXISTS (SELECT 1 FROM sqlite_sequence WHERE name = ?2)",
                rusqlite::params![seq, table],
            )?;
        }
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
///
/// The one shape that arrives without anyone writing it is two branches appending a step on the same
/// number, which the second merge leaves side by side. `make schema-renumber` is what moves the
/// trailing steps back into order and freezes the number the last one lands on.
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

/// The three tables v53 lays down or rebuilds under a name of its own, and the four it rebuilds in
/// place — frozen text, like every step's: the registry may reshape them tomorrow, and what this step
/// built must keep meaning what it meant.
const THREE_LAYER_TABLES: &str = r"
CREATE TABLE IF NOT EXISTS automation_placement (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    automation_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    action_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_action(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_placement_note (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    placement_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_placement(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    note_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_note(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_action (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT REFERENCES project(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL DEFAULT '',
    entry_step_id BIGINT REFERENCES automation_step(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_step (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    action_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_action(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL DEFAULT '',
    prompt TEXT NOT NULL DEFAULT '',
    agent TEXT NOT NULL DEFAULT '',
    model TEXT,
    interactive BOOLEAN NOT NULL DEFAULT 0 CHECK(interactive IN (0, 1)),
    work_dir_ref TEXT,
    report_to_task BOOLEAN NOT NULL DEFAULT 0 CHECK(report_to_task IN (0, 1)),
    show_history BOOLEAN NOT NULL DEFAULT 0 CHECK(show_history IN (0, 1)),
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_cfg (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT '' CHECK(owner_kind IN ('', 'action', 'placement')),
    owner_id BIGINT NOT NULL DEFAULT 0,
    name TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL DEFAULT '' CHECK(kind IN ('', 'taskfilter', 'folder', 'choice', 'number', 'text')),
    required BOOLEAN NOT NULL DEFAULT 0 CHECK(required IN (0, 1)),
    options TEXT,
    value TEXT,
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_edge (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT '' CHECK(owner_kind IN ('', 'automation', 'action')),
    owner_id BIGINT NOT NULL DEFAULT 0,
    from_id BIGINT NOT NULL DEFAULT 0,
    exit_name TEXT,
    to_id BIGINT,
    ends TEXT NOT NULL DEFAULT '' CHECK(ends IN ('', 'go', 'done', 'halt')),
    max_times BIGINT,
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_wire (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT '' CHECK(owner_kind IN ('', 'automation', 'action')),
    owner_id BIGINT NOT NULL DEFAULT 0,
    from_id BIGINT NOT NULL DEFAULT 0,
    from_exit_name TEXT,
    from_port_name TEXT NOT NULL DEFAULT '',
    to_id BIGINT NOT NULL DEFAULT 0,
    to_port_name TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
";

/// `automation.entry_step_id` as every store from v50 on declares it — frozen text.
const AUTOMATION_ENTRY_STEP: &str = "entry_step_id BIGINT REFERENCES automation_step(id) ON DELETE \
     RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED";
/// The same column pointed at the picture's new boxes.
const AUTOMATION_ENTRY_PLACEMENT: &str = "entry_placement_id BIGINT REFERENCES \
     automation_placement(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED";

/// One step as v52 held it: either it pointed at a library action or it carried its own prompt.
struct StepAtV52 {
    id: i64,
    automation_id: i64,
    name: String,
    action_id: Option<i64>,
    prompt: Option<String>,
    agent: String,
    model: Option<String>,
    interactive: i64,
    work_dir_ref: Option<String>,
    report_to_task: i64,
    show_history: i64,
    order_key: String,
    created_at: String,
    updated_at: String,
}

/// One library action as v52 held it: a prompt, and no steps.
struct ActionAtV52 {
    id: i64,
    project_id: Option<i64>,
    name: String,
    prompt: String,
    order_key: String,
    created_at: String,
    updated_at: String,
}

/// One way out or one port, whichever table it came out of — the two are mirrored the same way.
struct ExitAtV52 {
    id: i64,
    owner_kind: String,
    owner_id: i64,
    name: Option<String>,
    order_key: String,
    created_at: String,
    updated_at: String,
}

struct PortAtV52 {
    id: i64,
    owner_kind: String,
    owner_id: i64,
    direction: String,
    name: String,
    kind: String,
    required: i64,
    order_key: String,
    created_at: String,
    updated_at: String,
}

/// One edge of one automation's picture, drawn between two steps.
struct EdgeAtV52 {
    id: i64,
    automation_id: i64,
    from_step_id: i64,
    exit_name: Option<String>,
    to_step_id: Option<i64>,
    ends: String,
    max_times: Option<i64>,
    order_key: String,
    created_at: String,
    updated_at: String,
}

/// One wire of one automation's picture, drawn between two steps.
struct WireAtV52 {
    id: i64,
    automation_id: i64,
    from_step_id: i64,
    from_exit_name: Option<String>,
    from_port_name: String,
    to_step_id: i64,
    to_port_name: String,
    created_at: String,
    updated_at: String,
}

struct CfgAtV52 {
    id: i64,
    owner_kind: String,
    owner_id: i64,
    name: String,
    kind: String,
    required: i64,
    options: Option<String>,
    value: Option<String>,
    order_key: String,
    created_at: String,
    updated_at: String,
}

/// The counters the new rows are numbered from — max + 1 of what is already there, walked in source-id
/// order so that two machines upgrading the same store land on the same numbers.
struct Minting {
    action: i64,
    step: i64,
    placement: i64,
    exit: i64,
    port: i64,
    cfg: i64,
}

impl Minting {
    fn next(slot: &mut i64) -> i64 {
        let id = *slot;
        *slot += 1;
        id
    }
}

/// v53: fold the automation definition into **three layers** — an automation places actions, an action
/// holds steps, one step is one terminal (`AMB-D-949`).
///
/// At v52 `automation_step` meant two things: a row with a prompt of its own was one terminal, and a row
/// with an `action_id` was a call of a library action. The same word named both on the build screen, and
/// what could be re-used was one terminal's worth. This step gives each word one layer.
///
/// **Every v52 step becomes a placement**, and what it stood for becomes an action:
///
/// - A step that carried its own prompt gives a new library action of its own, in the project the
///   automation is in. The step row stays where it is and becomes that action's one step; what it
///   declared stays on it and is mirrored onto the action, which is what a placement of it is wired by.
/// - A step that pointed at a library action gives a placement of that action. The step row goes: what
///   it carried — the prompt's agent, its model, the three flags — is the action's one step's from here
///   on.
///
/// **Every v52 library action gains one step**, carrying the prompt the action itself used to carry.
///
/// **The one thing this cannot carry across** is two spots running one library action under two
/// different agents. `agent` and `model` are a step's at this version (`AMB-D-950`; v64 moves them
/// onto the placement), and an action has one step here, so the first spot's answer becomes the
/// action's and the second's is not written. An action
/// nothing pointed at gets no agent at all, which the launch check names rather than guesses at.
///
/// **Settings are split in two.** A v52 step declared and answered on one row; an action declared and
/// the step answered on a row of its own. Both become the one shape: the action declares, and the
/// placement answers.
///
/// **Why the tables are rebuilt and `automation` is not.** `automation_step`, `automation_action`,
/// `automation_cfg`, `automation_edge` and `automation_wire` are each either unreferenced or referenced
/// only by rows this step has already taken away, so SQLite's own build-copy-drop-rename is open to
/// them. `automation` is not: `automation_run.automation_id` is `RESTRICT`, and dropping a referenced
/// table performs an implicit `DELETE` that fires it — so for that one table the declaration is
/// rewritten where it stands, v9's procedure met an eighth time. It is one column's `REFERENCES` and its
/// name, never the shape, so the rows are exactly as wide afterwards as before.
///
/// **Skipped whole where there is nothing to fold**: a store born below v50 is handed today's
/// registry by genesis and arrives here already in three layers. What says so is
/// `automation_action.prompt` — the column only the v52 shape has, and the one every read below takes
/// its prompts from. The new tables are no test of it: genesis creates every table a store is
/// *missing* from today's registry, so `automation_placement` is there on a store whose other tables
/// are still v50's. Neither is the presence of `automation_step`, since v50 lays that name down
/// itself ([`name_the_step_after_its_action`] is what clears it away again).
fn fold_the_automation_into_three_layers(ctx: &Ctx<'_>) -> Result<()> {
    let tx = ctx.tx;
    let two_layer: i64 = tx.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('automation_action') WHERE name = 'prompt'",
        [],
        |r| r.get(0),
    )?;
    if two_layer == 0 {
        return Ok(());
    }

    // ── read the whole definition side before a row moves ──
    let steps: Vec<StepAtV52> = {
        let mut stmt = tx.prepare(
            "SELECT id, automation_id, name, action_id, prompt, agent, model, interactive, \
                    work_dir_ref, report_to_task, show_history, order_key, created_at, updated_at \
             FROM automation_step ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(StepAtV52 {
                id: r.get(0)?,
                automation_id: r.get(1)?,
                name: r.get(2)?,
                action_id: r.get(3)?,
                prompt: r.get(4)?,
                agent: r.get(5)?,
                model: r.get(6)?,
                interactive: r.get(7)?,
                work_dir_ref: r.get(8)?,
                report_to_task: r.get(9)?,
                show_history: r.get(10)?,
                order_key: r.get(11)?,
                created_at: r.get(12)?,
                updated_at: r.get(13)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let actions: Vec<ActionAtV52> = {
        let mut stmt = tx.prepare(
            "SELECT id, project_id, name, prompt, order_key, created_at, updated_at \
             FROM automation_action ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ActionAtV52 {
                id: r.get(0)?,
                project_id: r.get(1)?,
                name: r.get(2)?,
                prompt: r.get(3)?,
                order_key: r.get(4)?,
                created_at: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let exits: Vec<ExitAtV52> = {
        let mut stmt = tx.prepare(
            "SELECT id, owner_kind, owner_id, name, order_key, created_at, updated_at \
             FROM automation_exit ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ExitAtV52 {
                id: r.get(0)?,
                owner_kind: r.get(1)?,
                owner_id: r.get(2)?,
                name: r.get(3)?,
                order_key: r.get(4)?,
                created_at: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let ports: Vec<PortAtV52> = {
        let mut stmt = tx.prepare(
            "SELECT id, owner_kind, owner_id, direction, name, kind, required, order_key, \
                    created_at, updated_at FROM automation_port ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(PortAtV52 {
                id: r.get(0)?,
                owner_kind: r.get(1)?,
                owner_id: r.get(2)?,
                direction: r.get(3)?,
                name: r.get(4)?,
                kind: r.get(5)?,
                required: r.get(6)?,
                order_key: r.get(7)?,
                created_at: r.get(8)?,
                updated_at: r.get(9)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let cfgs: Vec<CfgAtV52> = {
        let mut stmt = tx.prepare(
            "SELECT id, owner_kind, owner_id, name, kind, required, options, value, order_key, \
                    created_at, updated_at FROM automation_cfg ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(CfgAtV52 {
                id: r.get(0)?,
                owner_kind: r.get(1)?,
                owner_id: r.get(2)?,
                name: r.get(3)?,
                kind: r.get(4)?,
                required: r.get(5)?,
                options: r.get(6)?,
                value: r.get(7)?,
                order_key: r.get(8)?,
                created_at: r.get(9)?,
                updated_at: r.get(10)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let edges: Vec<EdgeAtV52> = {
        let mut stmt = tx.prepare(
            "SELECT id, automation_id, from_step_id, exit_name, to_step_id, ends, max_times, \
                    order_key, created_at, updated_at FROM automation_edge ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(EdgeAtV52 {
                id: r.get(0)?,
                automation_id: r.get(1)?,
                from_step_id: r.get(2)?,
                exit_name: r.get(3)?,
                to_step_id: r.get(4)?,
                ends: r.get(5)?,
                max_times: r.get(6)?,
                order_key: r.get(7)?,
                created_at: r.get(8)?,
                updated_at: r.get(9)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let wires: Vec<WireAtV52> = {
        let mut stmt = tx.prepare(
            "SELECT id, automation_id, from_step_id, from_exit_name, from_port_name, to_step_id, \
                    to_port_name, created_at, updated_at FROM automation_wire ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(WireAtV52 {
                id: r.get(0)?,
                automation_id: r.get(1)?,
                from_step_id: r.get(2)?,
                from_exit_name: r.get(3)?,
                from_port_name: r.get(4)?,
                to_step_id: r.get(5)?,
                to_port_name: r.get(6)?,
                created_at: r.get(7)?,
                updated_at: r.get(8)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let step_notes: Vec<(i64, i64, i64, String, String, String)> = {
        let mut stmt = tx.prepare(
            "SELECT id, step_id, note_id, order_key, created_at, updated_at \
             FROM automation_step_note ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let run_defs: Vec<(i64, Option<i64>)> = {
        let mut stmt =
            tx.prepare("SELECT id, step_id FROM automation_run_def ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    // Which project each automation is in — the shelf a new action made from one of its steps lands on.
    let in_project: BTreeMap<i64, i64> = {
        let mut stmt = tx.prepare("SELECT id, project_id FROM automation ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        rows.collect::<rusqlite::Result<BTreeMap<_, _>>>()?
    };
    let entries: Vec<(i64, Option<i64>)> = {
        let mut stmt = tx.prepare("SELECT id, entry_step_id FROM automation ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut mint = Minting {
        action: actions.iter().map(|a| a.id).max().unwrap_or(0) + 1,
        step: steps.iter().map(|s| s.id).max().unwrap_or(0) + 1,
        placement: 1,
        exit: exits.iter().map(|e| e.id).max().unwrap_or(0) + 1,
        port: ports.iter().map(|p| p.id).max().unwrap_or(0) + 1,
        cfg: cfgs.iter().map(|c| c.id).max().unwrap_or(0) + 1,
    };

    // ── take the old rows out of the way, then lay the new tables down ──
    tx.execute_batch(
        "UPDATE automation SET entry_step_id = NULL;
         DELETE FROM automation_step_note;
         DELETE FROM automation_edge;
         DELETE FROM automation_wire;
         DELETE FROM automation_cfg;
         DELETE FROM automation_step;
         DROP TABLE automation_step_note;
         DROP TABLE automation_edge;
         DROP TABLE automation_wire;
         DROP TABLE automation_cfg;
         DROP TABLE automation_step;
         DROP TABLE automation_action;",
    )?;
    tx.execute_batch(THREE_LAYER_TABLES)?;
    // `automation_run_def` is the one table here that is neither rebuilt nor dropped — the run half
    // stands where it is — so the column is appended, and only where the store does not already carry
    // it (a store whose run table genesis created from today's registry does).
    let carries_placement: i64 = tx.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('automation_run_def') WHERE name = 'placement_id'",
        [],
        |r| r.get(0),
    )?;
    if carries_placement == 0 {
        tx.execute_batch(
            "ALTER TABLE automation_run_def ADD COLUMN placement_id BIGINT \
                 REFERENCES automation_placement(id) \
                 ON DELETE SET NULL ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED;",
        )?;
    }
    point_the_entry_at_a_placement(ctx)?;

    // ── the library: every action keeps its row and gains the one step its prompt becomes ──
    let mut inner_step: BTreeMap<i64, i64> = BTreeMap::new();
    for action in &actions {
        let carried = steps.iter().find(|s| s.action_id == Some(action.id));
        let step_id = Minting::next(&mut mint.step);
        tx.execute(
            "INSERT INTO automation_step (id, action_id, name, prompt, agent, model, interactive, \
                 work_dir_ref, report_to_task, show_history, order_key, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            rusqlite::params![
                step_id,
                action.id,
                action.name,
                action.prompt,
                carried.map(|s| s.agent.as_str()).unwrap_or(""),
                carried.and_then(|s| s.model.as_deref()),
                carried.map(|s| s.interactive).unwrap_or(0),
                carried.and_then(|s| s.work_dir_ref.as_deref()),
                carried.map(|s| s.report_to_task).unwrap_or(0),
                carried.map(|s| s.show_history).unwrap_or(1),
                "a0",
                action.created_at,
                action.updated_at,
            ],
        )?;
        tx.execute(
            "INSERT INTO automation_action (id, project_id, name, entry_step_id, order_key, \
                 created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                action.id,
                action.project_id,
                action.name,
                step_id,
                action.order_key,
                action.created_at,
                action.updated_at,
            ],
        )?;
        mirror_declarations(tx, &mut mint, &exits, &ports, ("action", action.id), ("step", step_id))?;
        inner_step.insert(action.id, step_id);
    }

    // ── every v52 step becomes a placement, and an action of its own where it carried a prompt ──
    let mut placement_of: BTreeMap<i64, i64> = BTreeMap::new();
    let mut step_now: BTreeMap<i64, i64> = BTreeMap::new();
    let mut kept_steps: BTreeSet<i64> = inner_step.values().copied().collect();
    let mut action_of_step: BTreeMap<i64, i64> = BTreeMap::new();
    for step in &steps {
        let action_id = match step.action_id {
            Some(action_id) => {
                step_now.insert(step.id, *inner_step.get(&action_id).unwrap_or(&0));
                action_id
            }
            None => {
                let action_id = Minting::next(&mut mint.action);
                tx.execute(
                    "INSERT INTO automation_action (id, project_id, name, entry_step_id, order_key, \
                         created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        action_id,
                        in_project.get(&step.automation_id),
                        step.name,
                        step.id,
                        step.order_key,
                        step.created_at,
                        step.updated_at,
                    ],
                )?;
                tx.execute(
                    "INSERT INTO automation_step (id, action_id, name, prompt, agent, model, \
                         interactive, work_dir_ref, report_to_task, show_history, order_key, \
                         created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    rusqlite::params![
                        step.id,
                        action_id,
                        step.name,
                        step.prompt.clone().unwrap_or_default(),
                        step.agent,
                        step.model,
                        step.interactive,
                        step.work_dir_ref,
                        step.report_to_task,
                        step.show_history,
                        "a0",
                        step.created_at,
                        step.updated_at,
                    ],
                )?;
                mirror_declarations(
                    tx,
                    &mut mint,
                    &exits,
                    &ports,
                    ("step", step.id),
                    ("action", action_id),
                )?;
                kept_steps.insert(step.id);
                step_now.insert(step.id, step.id);
                action_id
            }
        };
        let placement_id = Minting::next(&mut mint.placement);
        tx.execute(
            "INSERT INTO automation_placement (id, automation_id, action_id, order_key, created_at, \
                 updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                placement_id,
                step.automation_id,
                action_id,
                step.order_key,
                step.created_at,
                step.updated_at,
            ],
        )?;
        placement_of.insert(step.id, placement_id);
        action_of_step.insert(step.id, action_id);
    }

    // ── the ways out and ports of a step that is gone go with it ──
    for exit in &exits {
        if exit.owner_kind == "step" && !kept_steps.contains(&exit.owner_id) {
            tx.execute(
                "DELETE FROM automation_port WHERE owner_kind = 'exit' AND owner_id = ?1",
                [exit.id],
            )?;
            tx.execute("DELETE FROM automation_exit WHERE id = ?1", [exit.id])?;
        }
    }
    for port in &ports {
        if port.owner_kind == "step" && !kept_steps.contains(&port.owner_id) {
            tx.execute("DELETE FROM automation_port WHERE id = ?1", [port.id])?;
        }
    }

    // ── settings: the action declares, the placement answers ──
    for cfg in &cfgs {
        if cfg.owner_kind == "action" {
            write_cfg(tx, cfg, cfg.id, "action", cfg.owner_id, None)?;
            continue;
        }
        let Some(&placement_id) = placement_of.get(&cfg.owner_id) else { continue };
        let carried_its_own = steps
            .iter()
            .find(|s| s.id == cfg.owner_id)
            .is_some_and(|s| s.action_id.is_none());
        if carried_its_own {
            // One row was both halves. The declaration goes onto the action the step became, and the
            // answer — where there is one — onto the placement of it.
            let action_id = *action_of_step.get(&cfg.owner_id).unwrap_or(&0);
            write_cfg(tx, cfg, cfg.id, "action", action_id, None)?;
            if cfg.value.is_some() {
                let id = Minting::next(&mut mint.cfg);
                write_cfg(tx, cfg, id, "placement", placement_id, cfg.value.as_deref())?;
            }
        } else {
            write_cfg(tx, cfg, cfg.id, "placement", placement_id, cfg.value.as_deref())?;
        }
    }

    // ── the picture: the same lines, between placements now ──
    for edge in &edges {
        let Some(&from_id) = placement_of.get(&edge.from_step_id) else { continue };
        let to_id = match edge.to_step_id {
            Some(to_step) => match placement_of.get(&to_step) {
                Some(&to_id) => Some(to_id),
                // The step it went on to did not become a placement, so the line decides nothing.
                None => continue,
            },
            None => None,
        };
        tx.execute(
            "INSERT INTO automation_edge (id, owner_kind, owner_id, from_id, exit_name, to_id, ends, \
                 max_times, order_key, created_at, updated_at) \
             VALUES (?1, 'automation', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                edge.id,
                edge.automation_id,
                from_id,
                edge.exit_name,
                to_id,
                edge.ends,
                edge.max_times,
                edge.order_key,
                edge.created_at,
                edge.updated_at,
            ],
        )?;
    }
    for wire in &wires {
        let (Some(&from_id), Some(&to_id)) =
            (placement_of.get(&wire.from_step_id), placement_of.get(&wire.to_step_id))
        else {
            continue;
        };
        tx.execute(
            "INSERT INTO automation_wire (id, owner_kind, owner_id, from_id, from_exit_name, \
                 from_port_name, to_id, to_port_name, created_at, updated_at) \
             VALUES (?1, 'automation', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                wire.id,
                wire.automation_id,
                from_id,
                wire.from_exit_name,
                wire.from_port_name,
                to_id,
                wire.to_port_name,
                wire.created_at,
                wire.updated_at,
            ],
        )?;
    }
    for (id, step_id, note_id, order_key, created, updated) in &step_notes {
        let Some(&placement_id) = placement_of.get(step_id) else { continue };
        tx.execute(
            "INSERT INTO automation_placement_note (id, placement_id, note_id, order_key, \
                 created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, placement_id, note_id, order_key, created, updated],
        )?;
    }

    // ── what the entry and the runs point at ──
    for (automation_id, entry_step) in &entries {
        let Some(entry_step) = entry_step else { continue };
        let Some(&placement_id) = placement_of.get(entry_step) else { continue };
        tx.execute(
            "UPDATE automation SET entry_placement_id = ?1 WHERE id = ?2",
            rusqlite::params![placement_id, automation_id],
        )?;
    }
    for (def_id, step_id) in &run_defs {
        let Some(step_id) = step_id else { continue };
        tx.execute(
            "UPDATE automation_run_def SET placement_id = ?1, step_id = ?2 WHERE id = ?3",
            rusqlite::params![
                placement_of.get(step_id),
                step_now.get(step_id),
                def_id,
            ],
        )?;
    }
    Ok(())
}

/// Write one `automation_cfg` row, taking everything but the owner and the answer from the v52 row it
/// came out of.
fn write_cfg(
    tx: &Transaction<'_>,
    from: &CfgAtV52,
    id: i64,
    owner_kind: &str,
    owner_id: i64,
    value: Option<&str>,
) -> Result<()> {
    tx.execute(
        "INSERT INTO automation_cfg (id, owner_kind, owner_id, name, kind, required, options, \
             value, order_key, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            id,
            owner_kind,
            owner_id,
            from.name,
            from.kind,
            from.required,
            from.options,
            value,
            from.order_key,
            from.created_at,
            from.updated_at,
        ],
    )?;
    Ok(())
}

/// **Copy one owner's ways out and inputs onto another**, with the outputs hanging on each way out.
///
/// An action of one step declares the same names twice over: on the action, which is what a placement of
/// it is wired by, and on the step, which is what the picture inside it is drawn with. The copy is what
/// makes an action of one step behave exactly as the step it was folded out of did; what joins the two
/// copies is drawn a step later, by [`join_the_action_to_its_step`], off the names this one left
/// matching.
fn mirror_declarations(
    tx: &Transaction<'_>,
    mint: &mut Minting,
    exits: &[ExitAtV52],
    ports: &[PortAtV52],
    from: (&str, i64),
    to: (&str, i64),
) -> Result<()> {
    for exit in exits.iter().filter(|e| e.owner_kind == from.0 && e.owner_id == from.1) {
        let exit_id = Minting::next(&mut mint.exit);
        tx.execute(
            "INSERT INTO automation_exit (id, owner_kind, owner_id, name, order_key, created_at, \
                 updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                exit_id,
                to.0,
                to.1,
                exit.name,
                exit.order_key,
                exit.created_at,
                exit.updated_at,
            ],
        )?;
        for port in ports
            .iter()
            .filter(|p| p.owner_kind == "exit" && p.owner_id == exit.id && p.direction == "out")
        {
            let port_id = Minting::next(&mut mint.port);
            tx.execute(
                "INSERT INTO automation_port (id, owner_kind, owner_id, direction, name, kind, \
                     required, order_key, created_at, updated_at) \
                 VALUES (?1, 'exit', ?2, 'out', ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    port_id,
                    exit_id,
                    port.name,
                    port.kind,
                    port.required,
                    port.order_key,
                    port.created_at,
                    port.updated_at,
                ],
            )?;
        }
    }
    for port in ports
        .iter()
        .filter(|p| p.owner_kind == from.0 && p.owner_id == from.1 && p.direction == "in")
    {
        let port_id = Minting::next(&mut mint.port);
        tx.execute(
            "INSERT INTO automation_port (id, owner_kind, owner_id, direction, name, kind, \
                 required, order_key, created_at, updated_at) \
             VALUES (?1, ?2, ?3, 'in', ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                port_id,
                to.0,
                to.1,
                port.name,
                port.kind,
                port.required,
                port.order_key,
                port.created_at,
                port.updated_at,
            ],
        )?;
    }
    Ok(())
}

/// v55: an action's declarations are **joined** to the step inside it (`AMB-T-5311`).
///
/// v53 left an action of one step declaring the same names twice over — on the action, which is what a
/// placement of it is wired by, and on the step, which is what the picture inside it is drawn with
/// ([`mirror_declarations`]). Carrying the same name is not a join: nothing said that leaving the step
/// leaves the action, and nothing said that what the action is handed reaches the step. A run walking
/// into such an action would reach its one step and find no way out of the action at all.
///
/// Three things happen here, in this order:
///
/// 1. **`automation_edge` gains `exit_to`** — the way out of the action an `Exit` edge returns to. A
///    plain `ALTER TABLE … ADD COLUMN`: it is nullable, so every row already there reads as saying
///    nothing, which is what they mean.
/// 2. **`ends` admits `'exit'`.** v9's procedure met a ninth time, copied rather than called, for the
///    reasons [`admit_rejected_task_status`] gives at length — the set is widened in place because four
///    tables reference this picture's neighbours and the rebuild SQLite prescribes is shut. Nothing is
///    written to the rows: no row can be carrying a value the column never accepted.
/// 3. **The lines are drawn.** For every action holding exactly one step — which is every action v53
///    made — each of the step's ways out is given an `Exit` edge onto the action's way out of the same
///    name, and each input the action declares is wired from the boundary (`0`) onto the step's input of
///    the same name. The name is what the mirror left behind, so it is what the join is read off; from
///    here on the line is the truth and the names may part.
///
/// **Only where nothing is drawn yet.** A way out that already says what happens after it is left
/// alone, and so is a wire already drawn between the same two ends — a store stamped back and run
/// forward again lands on what it had, rather than a second copy of it.
///
/// An action with two steps or more is skipped whole: which of them leaves by which way out is a
/// picture only its author can draw, and guessing it would draw a wrong one that looks deliberate.
///
/// **The third line came later.** What the step hands *on* is wired out to the action by
/// [`wire_the_step_s_outputs_out_to_the_action`] (v58) — this step drew the other two only, which left
/// a run leaving such an action carrying nothing.
fn join_the_action_to_its_step(ctx: &Ctx<'_>) -> Result<()> {
    /// The closed set as every store from v50 on declares it — frozen text, like every step's.
    const NARROW: &str = "CHECK(ends IN ('', 'go', 'done', 'halt'))";
    /// The same set with the way out of an action a picture inside it returns to.
    const WIDE: &str = "CHECK(ends IN ('', 'go', 'done', 'halt', 'exit'))";

    let tx = ctx.tx;
    let has_column: i64 = tx.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('automation_edge') WHERE name = 'exit_to'",
        [],
        |r| r.get(0),
    )?;
    if has_column == 0 {
        tx.execute_batch("ALTER TABLE automation_edge ADD COLUMN exit_to TEXT;")?;
    }

    let declared: String = tx.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'automation_edge'",
        [],
        |r| r.get(0),
    )?;
    if !declared.contains(WIDE) {
        if !declared.contains(NARROW) {
            return Err(super::StoreEngineError::UnrecognisedDdl {
                table: "automation_edge",
                expected: NARROW,
            });
        }
        let widened = declared.replace(NARROW, WIDE);
        let before = column_names(tx, "automation_edge")?;
        tx.execute_batch("PRAGMA writable_schema = ON;")?;
        let wrote = tx.execute(
            "UPDATE sqlite_master SET sql = ?1 WHERE type = 'table' AND name = 'automation_edge'",
            [&widened],
        );
        // `RESET` both shuts the door and drops the connection's parsed schema, so the very next
        // statement sees the widened `CHECK` instead of the one this connection read at open.
        tx.execute_batch("PRAGMA writable_schema = RESET;")?;
        wrote?;
        let after = column_names(tx, "automation_edge")?;
        if before != after {
            return Err(super::StoreEngineError::UnrecognisedDdl {
                table: "automation_edge",
                expected: NARROW,
            });
        }
    }

    // ── the lines ──
    let lone_steps: Vec<(i64, i64)> = {
        let mut stmt = tx.prepare(
            "SELECT action_id, MIN(id) FROM automation_action_step GROUP BY action_id \
             HAVING COUNT(*) = 1",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if lone_steps.is_empty() {
        return Ok(());
    }
    let mut edge_id: i64 =
        tx.query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM automation_edge", [], |r| r.get(0))?;
    let mut wire_id: i64 =
        tx.query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM automation_wire", [], |r| r.get(0))?;
    let now = crate::time::Timestamp::now().to_rfc3339_z();

    for (action_id, step_id) in lone_steps {
        let mut last_key: Option<String> = tx.query_row(
            "SELECT MAX(order_key) FROM automation_edge WHERE owner_kind = 'action' AND owner_id = ?1",
            [action_id],
            |r| r.get(0),
        )?;
        let shared_exits: Vec<Option<String>> = {
            let mut stmt = tx.prepare(
                "SELECT s.name FROM automation_exit s \
                 WHERE s.owner_kind = 'step' AND s.owner_id = ?1 \
                   AND EXISTS (SELECT 1 FROM automation_exit a \
                               WHERE a.owner_kind = 'action' AND a.owner_id = ?2 \
                                 AND a.name IS s.name) \
                   AND NOT EXISTS (SELECT 1 FROM automation_edge e \
                                   WHERE e.owner_kind = 'action' AND e.from_id = ?1 \
                                     AND e.exit_name IS s.name) \
                 ORDER BY s.order_key, s.id",
            )?;
            let rows = stmt.query_map(rusqlite::params![step_id, action_id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for name in shared_exits {
            let order_key = crate::order::key_between(last_key.as_deref(), None);
            tx.execute(
                "INSERT INTO automation_edge (id, owner_kind, owner_id, from_id, exit_name, to_id, \
                     ends, exit_to, max_times, order_key, created_at, updated_at) \
                 VALUES (?1, 'action', ?2, ?3, ?4, NULL, 'exit', ?4, NULL, ?5, ?6, ?6)",
                rusqlite::params![edge_id, action_id, step_id, name, order_key, now],
            )?;
            edge_id += 1;
            last_key = Some(order_key);
        }
        let shared_inputs: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT a.name FROM automation_port a \
                 WHERE a.owner_kind = 'action' AND a.owner_id = ?1 AND a.direction = 'in' \
                   AND EXISTS (SELECT 1 FROM automation_port s \
                               WHERE s.owner_kind = 'step' AND s.owner_id = ?2 \
                                 AND s.direction = 'in' AND s.name = a.name) \
                   AND NOT EXISTS (SELECT 1 FROM automation_wire w \
                                   WHERE w.owner_kind = 'action' AND w.owner_id = ?1 \
                                     AND w.from_id = 0 AND w.from_port_name = a.name \
                                     AND w.to_id = ?2 AND w.to_port_name = a.name) \
                 ORDER BY a.order_key, a.id",
            )?;
            let rows = stmt.query_map(rusqlite::params![action_id, step_id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for name in shared_inputs {
            tx.execute(
                "INSERT INTO automation_wire (id, owner_kind, owner_id, from_id, from_exit_name, \
                     from_port_name, to_id, to_port_name, created_at, updated_at) \
                 VALUES (?1, 'action', ?2, 0, NULL, ?3, ?4, ?3, ?5, ?5)",
                rusqlite::params![wire_id, action_id, name, step_id, now],
            )?;
            wire_id += 1;
        }
    }
    Ok(())
}

/// v58: what a lone step hands on is **wired out to the action** around it (`AMB-T-5341`).
///
/// [`join_the_action_to_its_step`] drew two of the three lines an action of one step needs: the way out
/// of the step returns to the way out of the action, and what the action takes in reaches the step.
/// What the step hands *on* was left where it was. So a run walking such an action reached the step,
/// took its way out, and arrived at the next placement with nothing to fill its inputs with — and where
/// one of those was required, the run stopped there ([`crate::ops::automation_step`]).
///
/// The line runs from the step's way out into the boundary (`0` — [`crate::model::ACTION_BOUNDARY`]),
/// which is where a placement of the action is read from. What is joined is what the mirror left
/// behind: an output on a way out of the step is wired out where the action carries a way out of the
/// same name with an output of the same name on it. From here the line is the truth and the names may
/// part, exactly as v55 says of the other two.
///
/// **An action of two steps or more is skipped whole**, for v55's reason: which of them hands the
/// action's output on is a picture only its author can draw.
///
/// **Only where nothing is drawn yet.** A wire already joining those same two ends is left alone, so a
/// store stamped back and run forward again lands on what it had rather than a second copy of it.
///
/// **No shape moves here** — the step writes rows and no DDL — so this version's frozen file
/// (`super::schema_frozen`, test-only) is byte-identical to v57's, the way v6's is to v5's.
fn wire_the_step_s_outputs_out_to_the_action(ctx: &Ctx<'_>) -> Result<()> {
    let tx = ctx.tx;
    let lone_steps: Vec<(i64, i64)> = {
        let mut stmt = tx.prepare(
            "SELECT action_id, MIN(id) FROM automation_action_step GROUP BY action_id \
             HAVING COUNT(*) = 1",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if lone_steps.is_empty() {
        return Ok(());
    }
    let mut wire_id: i64 =
        tx.query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM automation_wire", [], |r| r.get(0))?;
    let now = crate::time::Timestamp::now().to_rfc3339_z();

    for (action_id, step_id) in lone_steps {
        // An `out` hangs on the way out rather than on the step, so both halves of the match are a
        // pair: the way out's name and the port's. `IS` is what compares the unnamed way out, whose
        // name is NULL on both sides.
        let shared_outputs: Vec<(Option<String>, String)> = {
            let mut stmt = tx.prepare(
                "SELECT se.name, sp.name FROM automation_exit se \
                 JOIN automation_port sp \
                   ON sp.owner_kind = 'exit' AND sp.owner_id = se.id AND sp.direction = 'out' \
                 WHERE se.owner_kind = 'step' AND se.owner_id = ?1 \
                   AND EXISTS (SELECT 1 FROM automation_exit ae \
                               JOIN automation_port ap \
                                 ON ap.owner_kind = 'exit' AND ap.owner_id = ae.id \
                                    AND ap.direction = 'out' \
                               WHERE ae.owner_kind = 'action' AND ae.owner_id = ?2 \
                                 AND ae.name IS se.name AND ap.name = sp.name) \
                   AND NOT EXISTS (SELECT 1 FROM automation_wire w \
                                   WHERE w.owner_kind = 'action' AND w.owner_id = ?2 \
                                     AND w.from_id = ?1 AND w.from_exit_name IS se.name \
                                     AND w.from_port_name = sp.name \
                                     AND w.to_id = 0 AND w.to_port_name = sp.name) \
                 ORDER BY se.order_key, se.id, sp.order_key, sp.id",
            )?;
            let rows = stmt.query_map(rusqlite::params![step_id, action_id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (exit_name, port_name) in shared_outputs {
            tx.execute(
                "INSERT INTO automation_wire (id, owner_kind, owner_id, from_id, from_exit_name, \
                     from_port_name, to_id, to_port_name, created_at, updated_at) \
                 VALUES (?1, 'action', ?2, ?3, ?4, ?5, 0, ?5, ?6, ?6)",
                rusqlite::params![wire_id, action_id, step_id, exit_name, port_name, now],
            )?;
            wire_id += 1;
        }
    }
    Ok(())
}

/// Rewrite `automation`'s entry column where it stands: a new name and a new table to point at, and not
/// one column more or fewer. v9's procedure, and its reasons — the table is referenced by
/// `automation_run` with `RESTRICT`, so the build-copy-drop-rename SQLite prescribes would take every
/// run of it, and `PRAGMA foreign_keys = OFF` is a no-op inside the transaction a step is.
///
/// Checked both ways round it: the declaration must carry the clause this step knows, and the column
/// count must be the same afterwards. `writable_schema` is shut before anything else can fail, since it
/// is connection state that would outlive this step's rolled-back transaction.
fn point_the_entry_at_a_placement(ctx: &Ctx<'_>) -> Result<()> {
    let declared: String = ctx.tx.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'automation'",
        [],
        |r| r.get(0),
    )?;
    if declared.contains("entry_placement_id") {
        return Ok(());
    }
    if !declared.contains(AUTOMATION_ENTRY_STEP) {
        return Err(super::StoreEngineError::UnrecognisedDdl {
            table: "automation",
            expected: AUTOMATION_ENTRY_STEP,
        });
    }
    let pointed = declared.replace(AUTOMATION_ENTRY_STEP, AUTOMATION_ENTRY_PLACEMENT);

    let before = column_names(ctx.tx, "automation")?.len();
    ctx.tx.execute_batch("PRAGMA writable_schema = ON;")?;
    let wrote = ctx.tx.execute(
        "UPDATE sqlite_master SET sql = ?1 WHERE type = 'table' AND name = 'automation'",
        [&pointed],
    );
    ctx.tx.execute_batch("PRAGMA writable_schema = RESET;")?;
    wrote?;
    let after = column_names(ctx.tx, "automation")?;
    if after.len() != before || !after.iter().any(|c| c == "entry_placement_id") {
        return Err(super::StoreEngineError::UnrecognisedDdl {
            table: "automation",
            expected: AUTOMATION_ENTRY_STEP,
        });
    }
    Ok(())
}

/// v54: `automation_step` becomes `automation_action_step` — the table takes the name of the owner
/// v53 moved it to (`AMB-D-949`).
///
/// Every table here whose rows hang off one other table is spelled `<owner>_<thing>`:
/// `task_comment`, `task_commit`, `decision_comment`, `project_notify_target`. A step hung on an
/// automation until v53 and hangs on an action from v53 on, so the name owed one word more.
/// `automation_action` keeps the spelling it has: an action is a project's or the device's, and its
/// `automation_` is the feature it belongs to rather than an owner.
///
/// **The genesis table is dropped before the rename, for the reason [`rename_the_outbox`] gives.**
/// Genesis is `CREATE TABLE IF NOT EXISTS` over today's registry and runs before this chain on
/// every open, so a store arriving here already carries an empty `automation_action_step` and
/// `ALTER TABLE … RENAME TO` would fail on a name already taken. Dropping it leaves the rename a
/// rename: the rows and the ids stay exactly what they were, and nothing references the empty one
/// — what `automation_action.entry_step_id` and `automation_run_def.step_id` name is still the old
/// name at this point, which is also what SQLite rewrites for us as the rename lands.
///
/// **Two shapes answer to the old name, and only one of them is carried.** A store that reached v53
/// with steps in it holds the three-layer table, which is this one under its old name. A store born
/// below v50 holds something else: [`lay_the_automation_tables_down`] writes the two-layer table out
/// in frozen text, so from here on it lays down a name today's registry no longer has, on a store
/// genesis has already handed the right one to. That table is a leftover — no op runs during a
/// migration, so nothing has ever written a row into it — and what it owes is to go, not to be
/// carried over the table genesis built. `automation_id` is what tells them apart: the column only
/// the two-layer shape has.
///
/// **The feed is rewritten with it**, the way v33 rewrote `dependency`. `change_feed.dataset` is
/// the key a carrier reads off the ledger and hands back to `sync records`, which answers only to
/// the datasets the registry declares — so a row still saying `automation_step` would be refused
/// rather than served. Frozen text, like every step's: the registry may rename the table again
/// tomorrow, and what this step rewrote must keep meaning what it meant.
fn name_the_step_after_its_action(ctx: &Ctx<'_>) -> Result<()> {
    if table_is_here(ctx.tx, "automation_step")? {
        let leftover: i64 = ctx.tx.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('automation_step') WHERE name = 'automation_id'",
            [],
            |r| r.get(0),
        )?;
        if leftover > 0 {
            ctx.tx.execute_batch("DROP TABLE automation_step;")?;
        } else {
            ctx.tx.execute_batch(
                "DROP TABLE IF EXISTS automation_action_step;
                 ALTER TABLE automation_step RENAME TO automation_action_step;",
            )?;
        }
    }
    ctx.tx.execute_batch(
        "UPDATE change_feed SET dataset = 'automation_action_step' \
             WHERE dataset = 'automation_step';",
    )?;
    Ok(())
}

/// v57: `automation.preamble` goes (`AMB-D-952`).
///
/// What a step is told before its own prompt says how a run works, not what this automation does, so
/// it reads the same on every automation there is. It is written in the build from here on
/// ([`crate::agents::preamble`]) and composed into each launch from there, which leaves the column
/// written by nobody and read by nobody — and a column in that state is one every later reader has to
/// work out again. It goes with the reason for it.
///
/// **Nothing is carried across.** What a row holds is either the standing text a build put there or a
/// sentence somebody typed over it, and neither has anywhere to go: no launch looks at the row again.
/// A store's own history is what the pre-migration backup keeps.
///
/// **Dropped where it stands.** The column is plain text with no index, no key and no `CHECK` naming
/// it, which is the case SQLite's own `DROP COLUMN` takes. Rebuilding the table is not open to this
/// step anyway — `automation_run` and `automation_placement` both reference `automation`, and v30's
/// note says what a `DROP TABLE` does to a `RESTRICT` that points at it.
///
/// **Probed, not bare.** A store born from today's registry never had the column: genesis creates a
/// table it is missing whole, from a registry this change has already taken it out of. Such a store
/// arrives here finished, and a bare `ALTER TABLE` would fail on it rather than pass over it.
fn take_the_preamble_off_the_definition(ctx: &Ctx<'_>) -> Result<()> {
    let held: i64 = ctx.tx.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('automation') WHERE name = 'preamble'",
        [],
        |r| r.get(0),
    )?;
    if held == 0 {
        return Ok(());
    }
    ctx.tx.execute_batch("ALTER TABLE automation DROP COLUMN preamble;")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_engine::schema;
    use crate::store_engine::schema_frozen::{frozen_or_panic, OLDEST_FROZEN_VERSION};

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = amenbo_scratch::scratch(&format!("migrate-{tag}"));
        dir
    }

    /// A store laid down from `ddl` and stamped at `stamp`.
    ///
    /// The DDL goes in **before the engine opens**, so genesis's `CREATE TABLE IF NOT EXISTS` leaves what
    /// is there alone and creates only what is missing around it — which is exactly what open does to a
    /// real store of that age, tables a later registry gained included. Building the store from this
    /// build's registry and undoing the difference afterwards is the one thing this must not do: the
    /// undo could only be driven by what the chain *declares* it added, and a column that reached the
    /// registry with no step would then be invisible to the fixture as well (`AMB-D-375`).
    fn store_declared_as(dir: &Path, ddl: &str, stamp: i64) -> StoreEngine {
        let path = dir.join("store.sqlite");
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            // Raised the way a store is raised (`schema::genesis_sql`): the journal mode first, then the
            // rest in one transaction. Statement by statement in the rollback journal, the sixty-odd
            // objects here cost a durable write apiece — and this fixture builds one store per version of
            // the chain, per test, which is where the Windows leg's minutes were going.
            let (journal_mode, ddl) = schema::genesis_sql_from(ddl);
            conn.execute_batch(journal_mode).unwrap();
            conn.execute_batch(&format!("BEGIN;\n{ddl}\nCOMMIT;")).unwrap();
        }
        let engine = StoreEngine::open(&path).unwrap();
        engine
            .conn()
            .execute(
                "INSERT INTO store_meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![META_FORMAT_VERSION, stamp.to_string()],
            )
            .unwrap();
        assert_eq!(engine.format_version().unwrap(), stamp);
        engine
    }

    /// A store born at `version` and stamped there — the shape an older build left behind, which is what
    /// the chain exists to move, read from that version's frozen DDL.
    fn store_at(dir: &Path, version: i64) -> StoreEngine {
        store_declared_as(dir, frozen_or_panic(version), version)
    }

    /// A store **born** at `born` and carried by the chain to `stamp` — the other shape a version can
    /// legitimately have. A column that reached the registry before it had a step gives one version two
    /// real shapes: every new store of that window was born with the column, every older one arrives
    /// without it. [`store_at`] is the first; this is the second.
    fn store_born_at(dir: &Path, born: i64, stamp: i64) -> StoreEngine {
        store_declared_as(dir, frozen_or_panic(born), stamp)
    }

    /// The chain up to and including the step that ends at `to` — what a step's own test runs when a later
    /// one takes away what it wrote. v43 drops the plugin mechanism's tables, so a test that walked the
    /// whole chain to look at one of them would be proving the last step rather than the one it names.
    fn steps_through(to: i64) -> &'static [Step] {
        let n = STEPS.iter().position(|s| s.to == to).expect("no step ends at that version") + 1;
        &STEPS[..n]
    }

    /// A store at the baseline: the oldest one this build still opens, and so the one every step runs on.
    ///
    /// Its own shape is not in this repository's history — the history begins with the chain already at
    /// [`OLDEST_FROZEN_VERSION`] — so the oldest frozen shape stands in, stamped at the baseline. What
    /// that leaves untested is the shape across that one interval; what it carries faithfully is the
    /// data every step from the baseline works on, which is what the tests below assert.
    fn baseline_store(dir: &Path) -> StoreEngine {
        store_born_at(dir, OLDEST_FROZEN_VERSION, BASELINE_VERSION)
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

    /// The fixtures take their shape from the frozen files, and a store of one age is not a store of
    /// another. Every test below rests on that: the chain is asked to move a store an older build left
    /// behind, so a fixture built from *this* build's registry would hand the chain nothing to move and
    /// every step would pass over a store that already looked finished.
    ///
    /// Nothing in the fixture says out loud where its shape comes from, so this asks the question the
    /// only way that cannot be satisfied by accident: two ages must not build the same store. Swap the
    /// frozen text for `schema_sql()` and every age builds one shape, and this is what goes red.
    #[test]
    fn two_ages_do_not_build_the_same_store() {
        let objects = |engine: &StoreEngine| -> Vec<String> {
            let mut q = engine
                .conn()
                .prepare("SELECT type || ' ' || name || ' ' || COALESCE(sql, '') FROM sqlite_master ORDER BY 1")
                .unwrap();
            q.query_map([], |r| r.get::<_, String>(0)).unwrap().map(|r| r.unwrap()).collect()
        };
        let oldest = scratch("ages-oldest");
        let newest = scratch("ages-newest");
        let old_shape = objects(&store_at(&oldest, OLDEST_FROZEN_VERSION));
        let new_shape = objects(&store_at(&newest, LATEST_VERSION));
        assert!(!old_shape.is_empty(), "the fixture builds something");
        assert_ne!(
            old_shape, new_shape,
            "a v{OLDEST_FROZEN_VERSION} fixture and a v{LATEST_VERSION} one came out identical — the \
             shape is no longer coming from the frozen files, and the chain has nothing left to move",
        );
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

    /// **A column declared in the registry with no step to carry it.** Every column the read model names
    /// must be on the table of a store the chain has finished with — a store an older build wrote, not one
    /// born from today's registry.
    ///
    /// This is the failure the chain exists to prevent and the only one it cannot notice by itself. A
    /// column added to the registry is there the instant a *new* store is created, so every test that
    /// makes its own store passes; the store that breaks is the one already on someone's disk, which no
    /// test and no reviewer ever sees — and "there is no step" is an absence, which does not show up in
    /// a diff. It has happened (`AMB-D-374`): a column reached the registry alone and every existing store
    /// failed at the first read of a task with `no such column`, all the way to a release.
    ///
    /// **Both halves of the registry.** The record tables ([`schema::DATASETS`]) and the plain ones
    /// ([`schema::PLAIN_TABLES`]) are laid down by the same genesis batch and so share the same hole:
    /// `CREATE TABLE IF NOT EXISTS` creates a table that is absent and touches one that is present, which
    /// is why a column added to either needs a step. The plain tables carry the outbox, the queue and the
    /// runner lease — state a read fails on as readily as a task's.
    ///
    /// The starting shape must come from the frozen DDL and not from this build's registry-minus-the-
    /// declared-diff: the subtraction can only remove what a step *says* it added, so the column with no
    /// step would never be taken off, and the check would pass on a store that already had it.
    ///
    /// **Every frozen version is a starting point, not just the baseline.** Genesis creates what a store
    /// is *missing* whole, from today's registry — so a table that came along after the baseline is born
    /// complete on a baseline store, new column included, and a check that started only there would be
    /// blind to exactly the tables the project keeps adding. The store that breaks is the one that already
    /// had the table, which is any store from the version the table arrived at onwards.
    ///
    /// If this goes red, the missing column is one the registry gained without a step. The fix is the
    /// step, not the registry: append one that carries existing stores, which bumps the version.
    #[test]
    fn every_column_the_registry_declares_survives_the_chain() {
        // The whole registry as one list of (table, the columns it declares). A record table's `id` is
        // implicit and never written as a field, so `all_columns` is what it owes; a plain table declares
        // every column it has, key included.
        let declared: Vec<(&str, Vec<&str>)> = schema::DATASETS
            .iter()
            .map(|d| (d.name, d.all_columns().map(|c| c.name).collect()))
            .chain(
                schema::PLAIN_TABLES
                    .iter()
                    .map(|p| (p.name, p.columns.iter().map(|c| c.name).collect())),
            )
            .collect();

        for born in OLDEST_FROZEN_VERSION..=LATEST_VERSION {
            let dir = scratch(&format!("registry-vs-chain-v{born}"));
            let engine = store_at(&dir, born);

            run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

            let mut missing: Vec<String> = Vec::new();
            for (table, owed) in &declared {
                let columns = {
                    let conn = engine.conn();
                    let mut stmt = conn.prepare("SELECT name FROM pragma_table_info(?1)").unwrap();
                    let rows = stmt.query_map([table], |r| r.get::<_, String>(0)).unwrap();
                    rows.filter_map(|r| r.ok()).collect::<Vec<_>>()
                };
                assert!(!columns.is_empty(), "a store born at v{born} has no `{table}` table at all");
                // Every missing column is collected, not just the first: they are usually one change's
                // worth, and naming them together is what makes the one step to write obvious.
                missing.extend(
                    owed.iter()
                        .filter(|c| !columns.iter().any(|h| h == *c))
                        .map(|c| format!("{table}.{c}")),
                );
            }
            assert!(
                missing.is_empty(),
                "a store born at v{born} reaches v{LATEST_VERSION} without {} column(s) the registry \
                 declares: {}. Append a step that adds them (which bumps the version) — a store already \
                 on disk does not get them from the registry.",
                missing.len(),
                missing.join(", ")
            );
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// One table's declaration, as the store carries it.
    fn declared_sql(engine: &StoreEngine, table: &str) -> String {
        engine
            .conn()
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |r| r.get(0),
            )
            .unwrap_or_else(|e| panic!("no declaration for `{table}`: {e}"))
    }

    /// **v10 on every shape the chain starts from** (`AMB-D-403`). A store born at any frozen version comes
    /// out of the chain declaring the same `ON DELETE` as one born from today's registry — which is the
    /// whole point of a step for a constraint that `CREATE TABLE IF NOT EXISTS` can never revisit.
    ///
    /// The check is on the clauses and not on the whole declaration, because two stores at one version
    /// legitimately carry their columns in different order (see [`admit_rejected_task_status`]); the count
    /// is what says every reference moved rather than the first one found.
    ///
    /// It also pins the exclusion the decision named: Amenbo's own per-project settings come out still
    /// cascading. A rewrite that swept the whole schema would take those too, silently, and only a delete
    /// op growing a sweep it never needed would eventually say so.
    #[test]
    fn the_chain_restricts_the_concept_references_and_leaves_the_settings_cascading() {
        for born in OLDEST_FROZEN_VERSION..=LATEST_VERSION {
            let dir = scratch(&format!("restrict-v{born}"));
            let engine = store_at(&dir, born);

            run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

            for (table, references) in RESTRICTED_TABLES {
                let sql = declared_sql(&engine, table);
                assert_eq!(
                    sql.matches(REFERENCE_RESTRICTS).count(),
                    *references,
                    "a store born at v{born} leaves `{table}` with the wrong number of restricted \
                     references:\n{sql}"
                );
                assert!(
                    !sql.contains(REFERENCE_CASCADES),
                    "a store born at v{born} still lets `{table}` be swept:\n{sql}"
                );
            }
            for table in ["secret", "project_notify"] {
                assert!(
                    declared_sql(&engine, table).contains(REFERENCE_CASCADES),
                    "a store born at v{born} stopped cascading `{table}`, which is Amenbo's own setting"
                );
            }
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// The version `dimension.applies_to` arrives at — the ages below it are the ones the seed exists
    /// for. Named rather than written into the loop bound so the test says which step it is about.
    const APPLIES_TO_VERSION: i64 = 32;

    /// **v32 on every shape the chain starts from** (`AMB-D-789`). Every axis an upgrading store holds
    /// comes out classifying `both`, which is how it was already being read.
    ///
    /// This step is the one that could not lean on its column's `DEFAULT` the way v27's and v29's did.
    /// A required text column defaults to `''`, the not-yet-written sentinel, and `''` is not one of the
    /// three values [`crate::model::DimensionAppliesTo::parse`] admits — so a bare `ALTER TABLE` would
    /// leave every existing axis unreadable, and the failure would surface as a hydration error on the
    /// first list after an upgrade rather than here.
    ///
    /// The ages at and above the column's own are left out on purpose: there the axis is born with the
    /// column and it is `ops::dimension` that fills it, which this fixture's raw `INSERT` bypasses.
    #[test]
    fn the_chain_leaves_every_existing_axis_classifying_both() {
        for born in OLDEST_FROZEN_VERSION..APPLIES_TO_VERSION {
            let dir = scratch(&format!("applies-to-v{born}"));
            let engine = store_at(&dir, born);
            engine
                .conn()
                .execute_batch(
                    "INSERT INTO project (id, name) VALUES (1, 'p');
                     INSERT INTO dimension (id, project_id, name) VALUES (1, 1, '占有');",
                )
                .unwrap();

            run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

            let seeded: String = engine
                .conn()
                .query_row("SELECT applies_to FROM dimension WHERE id = 1", [], |r| r.get(0))
                .unwrap();
            assert_eq!(
                seeded, "both",
                "a store born at v{born} came out of the chain with an axis no reader can hydrate"
            );
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// A store whose plugin table does not declare what v24 expects is **refused**, and refused before
    /// anything is written — the same posture v10 takes, and for the same reason.
    #[test]
    fn a_plugin_table_the_layer_step_does_not_recognise_stops_the_chain() {
        let dir = scratch("layer-unrecognised");
        let engine = store_at(&dir, 23);
        // A `plugin_enable` whose key was never declared the way the step reads it. It is last in the list,
        // so the tables ahead of it are what must be found untouched afterwards.
        engine
            .conn()
            .execute_batch(
                "PRAGMA writable_schema = ON;
                 UPDATE sqlite_master SET sql = replace(sql, 'project_id BIGINT NOT NULL DEFAULT 0', 'project_id BIGINT NOT NULL DEFAULT 1')
                  WHERE type = 'table' AND name = 'plugin_enable';
                 PRAGMA writable_schema = RESET;",
            )
            .unwrap();

        let err = run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap_err();
        assert!(
            matches!(
                err,
                super::super::StoreEngineError::UnrecognisedDdl { table: "plugin_enable", .. }
            ),
            "{err}"
        );
        assert!(
            declared_sql(&engine, "plugin_config").contains(PROJECT_KEY_REQUIRED),
            "the tables ahead of the one that stopped the step are untouched"
        );
        assert_eq!(engine.format_version().unwrap(), 23, "and the store is still where it was");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A store whose table does not declare what v10 expects is **refused**, and refused before anything is
    /// written: a half-restricted store would carry a version stamp saying the whole set had moved.
    #[test]
    fn a_table_the_restriction_step_does_not_recognise_stops_the_chain_with_nothing_written() {
        let dir = scratch("restrict-unrecognised");
        let engine = store_at(&dir, 9);
        // A `task_commit` that never declared the clause — the shape this step cannot speak for. It is late
        // in the list, so the tables ahead of it are what must be found untouched afterwards.
        engine
            .conn()
            .execute_batch(
                "PRAGMA writable_schema = ON;
                 UPDATE sqlite_master SET sql = replace(sql, 'ON DELETE CASCADE', 'ON DELETE NO ACTION')
                  WHERE type = 'table' AND name = 'task_commit';
                 PRAGMA writable_schema = RESET;",
            )
            .unwrap();

        let err = run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap_err();
        assert!(
            matches!(err, super::super::StoreEngineError::UnrecognisedDdl { table: "task_commit", .. }),
            "{err}"
        );
        assert!(
            declared_sql(&engine, "task_comment").contains(REFERENCE_CASCADES),
            "the tables ahead of the one that stopped the step are untouched"
        );
        assert_eq!(engine.format_version().unwrap(), 9, "and the store is still where it was");
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
        engine
            .conn()
            .query_row("SELECT COUNT(drawn_at) FROM decision_edge", [], |r| r.get::<_, i64>(0))
            .expect("v8 put the decision edge's intent column back");
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
        // A store that answered the hook question has been used, so a config.json is already there. Seed a
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
                // point at live projects, exactly as production data does. The table itself comes with
                // the v3 shape; a store that answered the question is one that had it.
                "INSERT INTO project (id, name) VALUES (1, 'A'), (2, 'B'), (3, 'C');
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

    /// The clause v9 rewrites, as every store from the baseline to v8 declares it.
    const NARROW_STATUS_SET: &str = " CHECK(status IN ('', 'todo', 'in_progress', 'done', 'blocked'))";

    /// v9 in full, on the store shape v8 left behind: `task.status` admits four values, and the terminal
    /// for work decided against (`AMB-D-397`) is not one of them.
    ///
    /// The rows around the task are the point. Widening a `CHECK` has no `ALTER TABLE`, and the
    /// rebuild-and-swap SQLite's documentation prescribes would drop a table six child tables reference
    /// with `ON DELETE CASCADE` — so this asserts the comment, the dependency and the commit anchor are
    /// still there afterwards. A step that took the documented route would pass every other assertion here
    /// and empty them.
    #[test]
    fn the_task_status_set_widens_without_disturbing_the_table_or_its_children() {
        let dir = scratch("task-status-widen");
        let engine = store_at(&dir, 8);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO task (id, title, status, project_id) VALUES (1, 'kept', 'todo', 1), (2, 'blocker', 'done', 1);
                 INSERT INTO task_comment (task_id, text) VALUES (1, 'why');
                 INSERT INTO task_dependency (task_id, blocked_by_id) VALUES (1, 2);
                 INSERT INTO task_commit (task_id, sha) VALUES (1, 'abc');",
            )
            .unwrap();
        let before = {
            let tx = engine.conn().unchecked_transaction().unwrap();
            column_names(&tx, "task").unwrap()
        };
        assert!(
            engine.conn().execute("UPDATE task SET status = 'rejected' WHERE id = 1", []).is_err(),
            "the store starts out refusing the value the step exists to admit"
        );

        // Stop at v9, the step this test is about, and read the shape there. Steps further along add
        // columns of their own (v21's `draft`), and folding those into the comparison would accuse v9 of
        // reshaping a table it only re-declared.
        let through_v9 = STEPS.iter().position(|s| s.to == 9).expect("v9 is in the chain") + 1;
        let v9 = run(&engine, &dir, &STEPS[..through_v9], &mut crate::progress::ignore).unwrap();
        assert!(v9.applied.iter().any(|s| s.contains("'rejected'")), "v9 ran: {:?}", v9.applied);
        let after = {
            let tx = engine.conn().unchecked_transaction().unwrap();
            column_names(&tx, "task").unwrap()
        };
        assert_eq!(before, after, "a constraint changed, not the shape");

        // Then the rest of the chain, so the store ends where every other store ends.
        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();
        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        engine
            .conn()
            .execute("UPDATE task SET status = 'rejected' WHERE id = 1", [])
            .expect("the widened set admits the new terminal");
        let count = |table: &str| {
            engine
                .conn()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0))
                .unwrap()
        };
        assert_eq!(count("task"), 2, "no row moved");
        assert_eq!(count("task_comment"), 1, "the cascade never fired: the comment is still there");
        assert_eq!(count("task_dependency"), 1, "…and so is the dependency edge");
        assert_eq!(count("task_commit"), 1, "…and so is the commit anchor");
        assert!(
            engine.conn().execute("UPDATE task SET status = 'shipped' WHERE id = 1", []).is_err(),
            "the set is still closed — one value wider, not open"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A store whose `task` declaration does not carry the clause v9 rewrites is **refused**, not guessed
    /// at. The step edits the store's own DDL text in place, so the one thing it may never do is write a
    /// declaration over a table it did not recognise — that would leave the store describing itself
    /// wrongly, which is worse than a migration that stops with the pre-migration backup beside it.
    #[test]
    fn a_task_table_the_step_does_not_recognise_stops_the_chain() {
        let dir = scratch("task-status-unknown");
        let engine = store_declared_as(
            &dir,
            // v8's shape with the closed set struck off `status` — same columns, a declaration this
            // build never wrote.
            &frozen_or_panic(8).replace(NARROW_STATUS_SET, ""),
            8,
        );

        let err = run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap_err();

        assert!(matches!(err, super::super::StoreEngineError::UnrecognisedDdl { table: "task", .. }), "{err}");
        assert_eq!(engine.format_version().unwrap(), 8, "the failed step left the version where it was");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The clause v34 rewrites, as every store from the baseline to v33 declares it.
    const NARROW_CARDINALITY_SET: &str = " CHECK(cardinality IN ('', 'single'))";

    /// v34 in full, on the store shape v33 left behind: `dimension.cardinality` admits one value, and the
    /// axis's other answer (`AMB-D-826`) is not one of them.
    ///
    /// The rows hanging off the axis are the point, as they are in v9's test. Widening a `CHECK` has no
    /// `ALTER TABLE`, and the rebuild-and-swap SQLite prescribes would drop a table three tables reference
    /// with `RESTRICT` — so this asserts the value, the task assignment and the decision assignment are
    /// still there afterwards. A step that took the documented route would fail at the drop, or empty them.
    #[test]
    fn the_cardinality_set_widens_without_disturbing_the_axis_or_what_hangs_off_it() {
        let dir = scratch("cardinality-widen");
        let engine = store_at(&dir, 33);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO task (id, title, status, project_id) VALUES (1, 'classified', 'todo', 1);
                 INSERT INTO decision (id, title, project_id) VALUES (1, 'why', 1);
                 INSERT INTO dimension (id, project_id, name, cardinality, applies_to) \
                     VALUES (1, 1, 'プロダクト', 'single', 'both');
                 INSERT INTO dimension_value (id, dimension_id, name) VALUES (1, 1, 'Amenbo本体');
                 INSERT INTO task_dimension_value (task_id, dimension_id, value_id) VALUES (1, 1, 1);
                 INSERT INTO decision_dimension_value (decision_id, dimension_id, value_id) VALUES (1, 1, 1);",
            )
            .unwrap();
        let before = {
            let tx = engine.conn().unchecked_transaction().unwrap();
            column_names(&tx, "dimension").unwrap()
        };
        assert!(
            engine.conn().execute("UPDATE dimension SET cardinality = 'multi' WHERE id = 1", []).is_err(),
            "the store starts out refusing the value the step exists to admit"
        );

        let run = run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert!(run.applied.iter().any(|s| s.contains("cardinality")), "v34 ran: {:?}", run.applied);
        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        let after = {
            let tx = engine.conn().unchecked_transaction().unwrap();
            column_names(&tx, "dimension").unwrap()
        };
        assert_eq!(before, after, "a constraint changed, not the shape");
        engine
            .conn()
            .execute("UPDATE dimension SET cardinality = 'multi' WHERE id = 1", [])
            .expect("the widened set admits the axis's other answer");
        let count = |table: &str| {
            engine
                .conn()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0))
                .unwrap()
        };
        assert_eq!(count("dimension"), 1, "no row moved");
        assert_eq!(count("dimension_value"), 1, "the axis kept its values");
        assert_eq!(count("task_dimension_value"), 1, "…and the task is still classified");
        assert_eq!(count("decision_dimension_value"), 1, "…and so is the decision");
        assert!(
            engine.conn().execute("UPDATE dimension SET cardinality = 'many' WHERE id = 1", []).is_err(),
            "the set is still closed — one value wider, not open"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The v34 twin of v9's refusal: a `dimension` declaration the step does not recognise stops the
    /// chain rather than being written over, for the reason that test gives.
    #[test]
    fn a_dimension_table_the_step_does_not_recognise_stops_the_chain() {
        let dir = scratch("cardinality-unknown");
        let engine = store_declared_as(
            &dir,
            &frozen_or_panic(33).replace(NARROW_CARDINALITY_SET, ""),
            33,
        );

        let err = run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap_err();

        assert!(
            matches!(err, super::super::StoreEngineError::UnrecognisedDdl { table: "dimension", .. }),
            "{err}"
        );
        assert_eq!(engine.format_version().unwrap(), 33, "the failed step left the version where it was");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The clause v35 rewrites, as every store from the baseline to v34 declares it.
    const NARROW_ROLE_SET: &str = " CHECK(role IN ('', 'none', 'time_axis'))";

    /// v35 in full, on the store shape v34 left behind: `dimension.role` knows two nominations and no
    /// value carries a `closed` flag (`AMB-D-829`). Both halves are one step, so both are asserted from
    /// one run — and the rows hanging off the axis are asserted with them, for the reason v9's and v34's
    /// tests give: the declaration is rewritten in place precisely because the rebuild SQLite prescribes
    /// would fire the `RESTRICT` of the three tables referencing `dimension`.
    ///
    /// **The values in the store come out open.** `0` is the column's default and what every value in an
    /// upgrading store means — nothing could have closed one — so there is nothing to backfill and
    /// nothing that quietly changed meaning.
    #[test]
    fn the_role_set_widens_and_every_value_arrives_open() {
        let dir = scratch("closable-role");
        let engine = store_at(&dir, 34);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO task (id, title, status, project_id) VALUES (1, 'classified', 'todo', 1);
                 INSERT INTO decision (id, title, project_id) VALUES (1, 'why', 1);
                 INSERT INTO dimension (id, project_id, name, cardinality, role, applies_to) \
                     VALUES (1, 1, 'リリース', 'single', 'none', 'both');
                 INSERT INTO dimension_value (id, dimension_id, name) VALUES (1, 1, 'v19');
                 INSERT INTO task_dimension_value (task_id, dimension_id, value_id) VALUES (1, 1, 1);
                 INSERT INTO decision_dimension_value (decision_id, dimension_id, value_id) VALUES (1, 1, 1);",
            )
            .unwrap();
        let before = {
            let tx = engine.conn().unchecked_transaction().unwrap();
            column_names(&tx, "dimension").unwrap()
        };
        assert!(
            engine.conn().execute("UPDATE dimension SET role = 'closable' WHERE id = 1", []).is_err(),
            "the store starts out refusing the nomination the step exists to admit"
        );

        let run = run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert!(run.applied.iter().any(|s| s.contains("closed")), "v35 ran: {:?}", run.applied);
        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        let after = {
            let tx = engine.conn().unchecked_transaction().unwrap();
            column_names(&tx, "dimension").unwrap()
        };
        assert_eq!(before, after, "on `dimension` a constraint changed, not the shape");
        engine
            .conn()
            .execute("UPDATE dimension SET role = 'closable' WHERE id = 1", [])
            .expect("the widened set admits the nomination");
        assert!(
            engine.conn().execute("UPDATE dimension SET role = 'retired' WHERE id = 1", []).is_err(),
            "the set is still closed — one nomination wider, not open"
        );

        let closed: i64 = engine
            .conn()
            .query_row("SELECT closed FROM dimension_value WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(closed, 0, "the value the store already held arrives open");
        assert!(
            engine.conn().execute("UPDATE dimension_value SET closed = 2 WHERE id = 1", []).is_err(),
            "and the flag holds itself to a truth value"
        );
        let count = |table: &str| {
            engine
                .conn()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0))
                .unwrap()
        };
        assert_eq!(count("dimension"), 1, "no row moved");
        assert_eq!(count("dimension_value"), 1, "the axis kept its values");
        assert_eq!(count("task_dimension_value"), 1, "…and the task is still classified");
        assert_eq!(count("decision_dimension_value"), 1, "…and so is the decision");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The v35 twin of the refusal above: a `dimension` declaration carrying neither the narrow role set
    /// nor the wide one stops the chain rather than being written over.
    #[test]
    fn a_dimension_role_the_step_does_not_recognise_stops_the_chain() {
        let dir = scratch("closable-unknown");
        let engine =
            store_declared_as(&dir, &frozen_or_panic(34).replace(NARROW_ROLE_SET, ""), 34);

        let err = run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap_err();

        assert!(
            matches!(err, super::super::StoreEngineError::UnrecognisedDdl { table: "dimension", .. }),
            "{err}"
        );
        assert_eq!(engine.format_version().unwrap(), 34, "the failed step left the version where it was");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v11 in full, on the store shape v10 left behind: an outbox whose rows carry no project. The column
    /// arrives, the events already sitting there keep every field they had, and their project reads back
    /// as `NULL` — the honest word for a routing fact nobody wrote down at the time. Filling it in now by
    /// re-reading the record is exactly the guess `AMB-D-405` removed, and on the row that matters most (a
    /// deletion) there is no record left to read.
    #[test]
    fn the_outbox_gains_a_project_column_and_leaves_the_events_already_in_it_alone() {
        let dir = scratch("outbox-project");
        let engine = store_at(&dir, 10);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO plugin_outbox (id, event, record_id, actor, at, new_state)
                     VALUES (1, 'task.deleted', 7, 'ai', '2026-07-26T09:00:00Z', NULL),
                            (2, 'task.status_changed', 8, 'human', '2026-07-26T09:00:01Z', 'in_progress');",
            )
            .unwrap();

        let run = run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert!(run.applied.iter().any(|s| s.contains("plugin_outbox.project")), "v11 ran: {:?}", run.applied);
        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        let rows: Vec<(i64, String, Option<i64>)> = {
            let conn = engine.conn();
            let mut stmt = conn.prepare("SELECT id, event, project FROM outbox ORDER BY id").unwrap();
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };
        assert_eq!(
            rows,
            vec![(1, "task.deleted".to_string(), None), (2, "task.status_changed".to_string(), None)],
            "the events that were already there keep their fields and gain an unstamped project",
        );
        engine
            .conn()
            .execute(
                "INSERT INTO outbox (event, record_id, actor, at, project)
                     VALUES ('task.created', 9, 'ai', '2026-07-26T09:00:02Z', 3)",
                [],
            )
            .expect("what is emitted from here on can carry its project");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Lay a plugin's body down where the handover looks for it — the directory is the whole of what
    /// says a plugin was installed on this device.
    fn plugin_body(dir: &Path, name: &str) {
        let home = dir.join("plugins").join(name);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("manifest.yaml"), b"name: x\n").unwrap();
    }

    /// One `secret` row's value, by the address the body keeps it at.
    fn secret_at(engine: &StoreEngine, area: &str, owner: Option<i64>, field: &str) -> Option<String> {
        engine
            .conn()
            .query_row(
                "SELECT value FROM secret \
                 WHERE project_id IS NULL AND area = ?1 AND COALESCE(owner_id, 0) = ?2 AND field_key = ?3",
                rusqlite::params![area, owner.unwrap_or(0), field],
                |r| r.get::<_, String>(0),
            )
            .ok()
    }

    /// v42 in full: two projects sending through one Slack channel, a third on its own relay.
    ///
    /// The shelf holds one row per connection, not one per project — which is the whole reason it is a
    /// shelf — and each project selects what it had. What the plugin reported is what the project
    /// reports afterwards: the six a project starts with are nobody's answer, and the ticked set is.
    #[test]
    fn what_the_notifiers_carried_lands_on_the_shelf_and_each_project_selects_its_own() {
        let dir = scratch("handover-notify");
        let engine = store_at(&dir, 41);
        plugin_body(&dir, "slack");
        plugin_body(&dir, "mail");
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name) VALUES (1, 'alpha'), (2, 'beta'), (3, 'gamma');
                 INSERT INTO plugin_enable (project_id, plugin)
                     VALUES (1, 'slack'), (3, 'mail');
                 INSERT INTO plugin_secret (project_id, plugin, field_key, value) VALUES
                     (1, 'slack', 'webhook_url', 'https://hooks.example/one'),
                     (2, 'slack', 'webhook_url', 'https://hooks.example/one'),
                     (3, 'mail', 'smtp_password', 'pw');
                 INSERT INTO plugin_config (project_id, plugin, field_key, value) VALUES
                     (1, 'slack', 'events', 'task.done,comment.added'),
                     (3, 'mail', 'smtp_host', 'smtp.example'),
                     (3, 'mail', 'smtp_port', '587'),
                     (3, 'mail', 'smtp_user', 'alice@example.com'),
                     (3, 'mail', 'from', 'alice@example.com'),
                     (3, 'mail', 'to', 'team@example.com');",
            )
            .unwrap();

        // Stopped at v42: the next step takes the tables this reads what is left in away.
        let run = run(&engine, &dir, steps_through(42), &mut crate::progress::ignore).unwrap();
        assert!(run.applied.iter().any(|s| s.contains("into the body")), "v42 ran: {:?}", run.applied);
        assert_eq!(engine.format_version().unwrap(), 42);

        // Two connections, two rows — the webhook the first two projects shared is on the shelf once.
        let shelf: Vec<(i64, String, String, i64)> = {
            let mut stmt = engine
                .conn()
                .prepare("SELECT id, kind, name, is_default FROM notify_target ORDER BY id")
                .unwrap();
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap();
            rows.filter_map(std::result::Result::ok).collect()
        };
        assert_eq!(
            shelf,
            vec![
                (1, "slack".to_string(), "Slack".to_string(), 1),
                (2, "mail".to_string(), "Mail".to_string(), 0),
            ],
            "one row per connection, named by kind, the first one marked",
        );
        assert_eq!(
            secret_at(&engine, "notify", Some(1), "webhook_url").as_deref(),
            Some("https://hooks.example/one"),
            "the webhook is in the table no road out walks",
        );
        assert_eq!(secret_at(&engine, "notify", Some(2), "smtp_password").as_deref(), Some("pw"));

        let relay: (Option<String>, Option<i64>, Option<String>, Option<String>) = engine
            .conn()
            .query_row(
                "SELECT smtp_host, smtp_port, smtp_user, mail_from FROM notify_target WHERE id = 2",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            relay,
            (
                Some("smtp.example".to_string()),
                Some(587),
                Some("alice@example.com".to_string()),
                Some("alice@example.com".to_string()),
            ),
        );

        let notify = |project: i64| -> (i64, String) {
            engine
                .conn()
                .query_row(
                    "SELECT enabled, mail_to FROM project_notify WHERE project_id = ?1",
                    [project],
                    |r| Ok((r.get(0).unwrap(), r.get(1).unwrap())),
                )
                .unwrap()
        };
        assert_eq!(notify(1), (1, String::new()), "the project that had it on has it on");
        assert_eq!(notify(2), (0, String::new()), "and the one that never turned it on is off");
        assert_eq!(notify(3), (1, "team@example.com".to_string()), "`to` becomes the project's own");

        let selected = |project: i64| -> Vec<i64> {
            let mut stmt = engine
                .conn()
                .prepare("SELECT target_id FROM project_notify_target WHERE project_id = ?1 ORDER BY target_id")
                .unwrap();
            let rows = stmt.query_map([project], |r| r.get::<_, i64>(0)).unwrap();
            rows.filter_map(std::result::Result::ok).collect()
        };
        assert_eq!(selected(1), vec![1]);
        assert_eq!(selected(2), vec![1], "both projects reach the one shelved webhook");
        assert_eq!(selected(3), vec![2]);

        let events = |project: i64| -> Vec<String> {
            let mut stmt = engine
                .conn()
                .prepare("SELECT event FROM project_notify_event WHERE project_id = ?1 ORDER BY event")
                .unwrap();
            let rows = stmt.query_map([project], |r| r.get::<_, String>(0)).unwrap();
            rows.filter_map(std::result::Result::ok).collect()
        };
        assert_eq!(
            events(1),
            vec!["comment.added".to_string(), "task.done".to_string()],
            "what was ticked is what is reported",
        );
        assert_eq!(
            events(3).len(),
            6,
            "a project that ticked nothing reports what the plugin reported for it: {:?}",
            events(3),
        );

        // The plugins are gone, bodies and rows together.
        for plugin in ["slack", "mail"] {
            assert!(!dir.join("plugins").join(plugin).exists(), "{plugin}'s body is taken away");
        }
        let left: i64 = engine
            .conn()
            .query_row(
                "SELECT (SELECT COUNT(*) FROM plugin_config) + (SELECT COUNT(*) FROM plugin_secret) \
                      + (SELECT COUNT(*) FROM plugin_enable)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(left, 0, "nothing of the four is left in the tables");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The Viewer's three keys move to the device's own address, and the switch is written **only** where
    /// the plugin was installed and turned off. An absent switch reads as on, which is right for every
    /// device that never stood a server up — so writing one there would be inventing an answer.
    #[test]
    fn the_viewers_keys_move_and_only_a_switch_that_was_off_is_written() {
        let dir = scratch("handover-viewer-off");
        let engine = store_at(&dir, 41);
        plugin_body(&dir, "viewer");
        engine
            .conn()
            .execute_batch(
                "INSERT INTO plugin_config (project_id, plugin, field_key, value)
                     VALUES (NULL, 'viewer', 'worker_url', 'https://amenbo.workers.dev');
                 INSERT INTO plugin_secret (project_id, plugin, field_key, value) VALUES
                     (NULL, 'viewer', 'auth_token', 'the-token'),
                     (NULL, 'viewer', 'encryption_key', 'the-key');",
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert_eq!(
            secret_at(&engine, "viewer", None, "worker_url").as_deref(),
            Some("https://amenbo.workers.dev"),
        );
        assert_eq!(secret_at(&engine, "viewer", None, "auth_token").as_deref(), Some("the-token"));
        assert_eq!(
            secret_at(&engine, "viewer", None, "encryption_key").as_deref(),
            Some("the-key"),
            "the key without which every paired phone reads nothing",
        );
        let sending: Option<i64> =
            engine.conn().query_row("SELECT sending FROM viewer_switch", [], |r| r.get(0)).ok();
        assert_eq!(sending, Some(0), "installed and never enabled: the switch says off");
        std::fs::remove_dir_all(&dir).ok();

        // The same device with the plugin enabled writes no switch at all.
        let dir = scratch("handover-viewer-on");
        let engine = store_at(&dir, 41);
        plugin_body(&dir, "viewer");
        engine
            .conn()
            .execute_batch("INSERT INTO plugin_enable (project_id, plugin) VALUES (NULL, 'viewer');")
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        let rows: i64 =
            engine.conn().query_row("SELECT COUNT(*) FROM viewer_switch", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 0, "an absent switch is the answer for a device that was carrying");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The Viewer's keys ride the whole chain — and go with the project they were written under.
    ///
    /// **The chain is not where a store loses them.** The step that carries them starts at v42, and the
    /// test above it stands a store at v41 to read that step alone; what a store actually does is arrive
    /// from far below, through every step between. This walks that: a store shaped as one left by the
    /// build before the plugins were taken in, carried the whole way, with the keys landing where the
    /// body reads them.
    ///
    /// **Both places they could have been written.** The Viewer declared itself the device's
    /// (`AMB-D-601`), and builds before that layer existed wrote its settings under a project
    /// (`AMB-D-434`). Either is carried, which is what `device_or_any` is for.
    ///
    /// **And the one way they go missing quietly.** These two tables keep their `ON DELETE CASCADE` on
    /// the project — deliberately, as Amenbo's own settings for a project rather than rows standing for
    /// a concept (`RESTRICTED_TABLES`, `AMB-D-403`) — so a store that wrote them under a project and
    /// later deleted that project reaches the carrying step with nothing in hand. Nothing is said about
    /// it at the time or afterwards: an absent row and an absent table read alike here, and both read as
    /// a device that never had the plugin.
    #[test]
    fn the_viewers_keys_ride_the_whole_chain_and_go_with_a_deleted_project() {
        // Where the row was written, and whether the project it hangs on is still there when the chain
        // runs. The last of the three is the shape with nothing to carry.
        for (tag, project, kept) in
            [("device", "NULL", true), ("project", "1", true), ("project-deleted", "1", false)]
        {
            let dir = scratch(&format!("handover-viewer-chain-{tag}"));
            // The oldest shape that already has the layer key open (`AMB-D-601`), which is where a store
            // carrying the Viewer's own settings would have been written.
            let engine = store_at(&dir, 27);
            plugin_body(&dir, "viewer");
            engine
                .conn()
                .execute_batch(&format!(
                    "INSERT INTO project (id, name) VALUES (1, 'alpha');
                     INSERT INTO plugin_config (project_id, plugin, field_key, value)
                         VALUES ({project}, 'viewer', 'worker_url', 'https://carried.workers.dev');
                     INSERT INTO plugin_secret (project_id, plugin, field_key, value) VALUES
                         ({project}, 'viewer', 'auth_token', 'the-token'),
                         ({project}, 'viewer', 'encryption_key', 'the-key');",
                ))
                .unwrap();
            if !kept {
                engine.conn().execute_batch("DELETE FROM project WHERE id = 1;").unwrap();
            }

            run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

            let want = kept.then_some("https://carried.workers.dev");
            assert_eq!(
                secret_at(&engine, "viewer", None, "worker_url").as_deref(),
                want,
                "the address, carried from 27 ({tag})",
            );
            assert_eq!(
                secret_at(&engine, "viewer", None, "auth_token").as_deref(),
                kept.then_some("the-token"),
                "the token ({tag})",
            );
            assert_eq!(
                secret_at(&engine, "viewer", None, "encryption_key").as_deref(),
                kept.then_some("the-key"),
                "the key without which every paired phone reads nothing ({tag})",
            );
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// A device that never installed one of the four is carried past this step untouched: no shelf, no
    /// project rows, and nothing for a surface to announce.
    #[test]
    fn a_device_that_never_had_a_plugin_is_left_exactly_as_it_was() {
        let dir = scratch("handover-none");
        let engine = store_at(&dir, 41);
        engine.conn().execute_batch("INSERT INTO project (id, name) VALUES (1, 'alpha');").unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        let raised: i64 = engine
            .conn()
            .query_row(
                "SELECT (SELECT COUNT(*) FROM notify_target) + (SELECT COUNT(*) FROM project_notify) \
                      + (SELECT COUNT(*) FROM secret) + (SELECT COUNT(*) FROM viewer_switch)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(raised, 0);
        let note: Option<String> = engine
            .conn()
            .query_row("SELECT value FROM store_meta WHERE key = 'plugins_carried_in'", [], |r| r.get(0))
            .ok();
        assert_eq!(note, None, "nothing was carried, so there is nothing to say");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// What the handover leaves for the surfaces to say once: which plugins were on this device, and how
    /// much came across. `worktree` is in the list and carries nothing — it never had a setting, and the
    /// person still wants to be told where it went.
    #[test]
    fn the_handover_leaves_an_account_for_the_surfaces_to_read() {
        let dir = scratch("handover-note");
        let engine = store_at(&dir, 41);
        plugin_body(&dir, "worktree");
        plugin_body(&dir, "slack");
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name) VALUES (1, 'alpha');
                 INSERT INTO plugin_enable (project_id, plugin) VALUES (1, 'slack');
                 INSERT INTO plugin_secret (project_id, plugin, field_key, value)
                     VALUES (1, 'slack', 'webhook_url', 'https://hooks.example/one');",
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        let note: String = engine
            .conn()
            .query_row("SELECT value FROM store_meta WHERE key = 'plugins_carried_in'", [], |r| r.get(0))
            .unwrap();
        let note: serde_json::Value = serde_json::from_str(&note).unwrap();
        assert_eq!(note["plugins"], serde_json::json!(["slack", "worktree"]));
        assert_eq!(note["targets"], 1);
        assert_eq!(note["projects"], 1);
        assert_eq!(note["viewer"], false);
        assert_eq!(note["told"], serde_json::json!([]), "nobody has been told yet");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v12 in full, on the store shape v11 left behind: queues whose rows carry no project. The column
    /// arrives, the rows waiting on a queue keep every field they had, and their project reads back as
    /// `NULL` — they were fanned out before anyone wrote it down, and a project-scoped subscription fires
    /// nothing for them rather than the fan-out's answer being invented here.
    #[test]
    fn the_queue_gains_a_project_column_and_leaves_the_rows_already_on_it_alone() {
        let dir = scratch("queue-project");
        let engine = store_at(&dir, 11);
        engine
            .conn()
            .execute_batch(
                // A third party's plugin: v42 sweeps the queue of the four Amenbo published, and this
                // test is about the column rather than about whose row is on it.
                "INSERT INTO plugin_queue (id, plugin, face, event, record_id, actor, at, new_state)
                     VALUES (1, 'notes', 'cli', 'task.deleted', 7, 'ai', '2026-07-26T09:00:00Z', NULL);",
            )
            .unwrap();

        let run = run(&engine, &dir, steps_through(42), &mut crate::progress::ignore).unwrap();

        assert!(run.applied.iter().any(|s| s.contains("plugin_queue.project")), "v12 ran: {:?}", run.applied);
        assert_eq!(engine.format_version().unwrap(), 42, "the chain stopped where this test looks");
        let row: (String, String, Option<i64>) = engine
            .conn()
            .query_row("SELECT plugin, event, project FROM plugin_queue", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(
            row,
            ("notes".to_string(), "task.deleted".to_string(), None),
            "the row that was already queued keeps its fields and gains an unstamped project",
        );
        engine
            .conn()
            .execute(
                "INSERT INTO plugin_queue (plugin, face, event, record_id, actor, at, project)
                     VALUES ('notes', 'cli', 'task.created', 9, 'ai', '2026-07-26T09:00:02Z', 3)",
                [],
            )
            .expect("what is fanned out from here on can carry its project");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v15 in full, on the store shape v14 left behind: a machine default in `config.json` and a secret in
    /// `plugin-secrets.json`, with two projects to carry them to. Both land as each project's own row —
    /// the machine default *was* what a project without one of its own ran on, so writing it per project
    /// is that sentence, not a guess — a project that had answered for itself keeps its answer, and the
    /// two user-area homes are gone afterwards.
    #[test]
    fn the_plugins_settings_move_into_every_projects_rows_and_the_user_area_homes_go() {
        let dir = scratch("plugin-settings-move");
        let engine = store_at(&dir, 14);
        engine
            .conn()
            .execute_batch(
                // A third party's plugin throughout: v42 carries the four Amenbo published into the
                // body and takes their rows away, and what is under test here is the carrying v15 did.
                "INSERT INTO project (id, name) VALUES (1, 'alpha'), (2, 'beta');
                 INSERT INTO plugin_config (id, project_id, plugin, field_key, value, created_at, updated_at)
                     VALUES (1, 2, 'notes', 'events', 'answered-for-itself',
                             '2026-07-26T09:00:00Z', '2026-07-26T09:00:00Z');",
            )
            .unwrap();
        std::fs::write(
            dir.join("config.json"),
            br#"{"language":"ja","plugin_config":{"notes":{"events":"push"}}}"#,
        )
        .unwrap();
        std::fs::write(dir.join("plugin-secrets.json"), br#"{"notes":{"token":"s3cret"}}"#).unwrap();

        let run = run(&engine, &dir, steps_through(42), &mut crate::progress::ignore).unwrap();

        assert!(run.applied.iter().any(|s| s.contains("into each project's rows")), "{:?}", run.applied);
        assert_eq!(engine.format_version().unwrap(), 42, "the chain stopped where this test looks");

        let value = |table: &str, project: i64, key: &str| -> Option<String> {
            engine
                .conn()
                .query_row(
                    &format!(
                        "SELECT value FROM {table} WHERE project_id = ?1 AND plugin = 'notes' AND field_key = ?2"
                    ),
                    rusqlite::params![project, key],
                    |r| r.get::<_, String>(0),
                )
                .ok()
        };
        assert_eq!(value("plugin_config", 1, "events").as_deref(), Some("push"), "the default is carried");
        assert_eq!(
            value("plugin_config", 2, "events").as_deref(),
            Some("answered-for-itself"),
            "the project that had its own answer keeps it",
        );
        assert_eq!(value("plugin_secret", 1, "token").as_deref(), Some("s3cret"));
        assert_eq!(value("plugin_secret", 2, "token").as_deref(), Some("s3cret"));

        // The homes are gone, and what else `config.json` held is untouched.
        assert!(!dir.join("plugin-secrets.json").exists(), "the secret file is taken away");
        let config = std::fs::read_to_string(dir.join("config.json")).unwrap();
        assert!(!config.contains("plugin_config"), "the config key is gone: {config}");
        assert!(config.contains("language"), "and nothing else in the file moved: {config}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A store with no project carries the values nowhere — there was never anything a plugin could have
    /// fired for — and the migration is still the end of the user-area homes. Where they were carried to is
    /// no longer readable here: v43 takes those tables away, and what stays true across the whole chain is
    /// that neither file outlives it.
    #[test]
    fn a_store_with_no_project_still_loses_the_user_area_homes() {
        let dir = scratch("plugin-settings-no-project");
        let engine = store_at(&dir, 14);
        std::fs::write(dir.join("config.json"), br#"{"plugin_config":{"notes":{"events":"push"}}}"#).unwrap();
        std::fs::write(dir.join("plugin-secrets.json"), br#"{"notes":{"token":"s3cret"}}"#).unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert!(!dir.join("plugin-secrets.json").exists());
        let config = std::fs::read_to_string(dir.join("config.json")).unwrap();
        assert!(!config.contains("plugin_config"), "the config key is gone: {config}");
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
        // Born before the registry declared the column (v3), carried by the chain to v5 — the older of
        // the two shapes a v5 store has, and the one v6 exists for.
        let engine = store_born_at(&dir, 3, 5);
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

    /// The same step on the *other* shape of v5 store: one born with the column — which is the shape v5's
    /// frozen DDL carries, the registry having declared it two versions before any step did. Both are real
    /// stores at the same version, and the step has to pass over this one rather than take the migration
    /// down with a duplicate column.
    #[test]
    fn the_task_status_clock_step_passes_over_a_store_that_already_has_it() {
        let dir = scratch("task-status-clock-born-with");
        let engine = store_at(&dir, 5);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO task (id, title, status, status_changed_at, created_at, updated_at)
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

    /// v67 in full, on the store shape v66 left behind: task comments with no intent column. Seeded from
    /// `created_at`, and NULL rather than the `''` a row caught mid-create carries — without the seed every
    /// comment in the backlog would be undatable, and a holder closing a task would be shown none of them.
    #[test]
    fn the_task_comment_is_seeded_from_when_it_was_posted() {
        let dir = scratch("comment-intent-column");
        let engine = store_at(&dir, 66);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO task (id, title, status, project_id) VALUES (1, 't', 'todo', 1);
                 INSERT INTO task_comment (id, task_id, text, created_at, updated_at) VALUES
                     (1, 1, 'posted', '2026-04-05T06:07:08Z', '2026-05-01T00:00:00Z'),
                     (2, 1, 'mid-create', '', '');",
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        let conn = engine.conn();
        let mut stmt = conn.prepare("SELECT id, posted_at FROM task_comment ORDER BY id").unwrap();
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))).unwrap();
        assert_eq!(
            rows.filter_map(|r| r.ok()).collect::<Vec<_>>(),
            vec![(1, Some("2026-04-05T06:07:08Z".to_string())), (2, None)],
            "a comment is seeded at the instant it was posted, not when it was last edited, and `''` is no instant"
        );
        drop(stmt);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v8 in full, on the store shape v7 left behind: decision edges with no intent column. The seed is the
    /// same shape as v7's — `created_at`, the instant the edge was drawn, and NULL rather than the `''` a
    /// row caught mid-create carries — because without it a supersession drawn before this ran has no
    /// instant at all, and the reopen axis would flag every superseded premise in the backlog at once.
    #[test]
    fn the_decision_edge_is_seeded_from_when_it_was_drawn() {
        let dir = scratch("edge-intent-column");
        let engine = store_at(&dir, 7);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO decision (id, project_id, title, body, status, created_at, updated_at) VALUES
                     (1, 1, 'old', '', 'accepted', '2025-10-01T00:00:00Z', '2025-10-01T00:00:00Z'),
                     (2, 1, 'new', '', 'accepted', '2025-10-01T00:00:00Z', '2025-10-01T00:00:00Z');
                 INSERT INTO decision_edge (id, decision_id, target_decision_id, kind, created_at, updated_at)
                 VALUES (1, 2, 1, 'supersedes', '2026-04-05T06:07:08Z', '2026-04-05T06:07:08Z'),
                        (2, 1, 2, 'builds_on',  '',                    '');",
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        let conn = engine.conn();
        let mut stmt = conn.prepare("SELECT id, drawn_at FROM decision_edge ORDER BY id").unwrap();
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))).unwrap();
        assert_eq!(
            rows.filter_map(|r| r.ok()).collect::<Vec<_>>(),
            vec![(1, Some("2026-04-05T06:07:08Z".to_string())), (2, None)],
            "an edge is seeded at the instant it was drawn, and `''` is no instant"
        );
        drop(stmt);
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

    /// **v23 says how far the feed is unattributable, instead of leaving a silent hole.** The rows a
    /// store already carries name no window and cannot be given one — the ones a deletion named are gone —
    /// so the step records where stamping begins, and a window whose cursor is below that is told its
    /// cursor is gone rather than handed a page with holes in it.
    #[test]
    fn the_step_that_stamps_the_feed_says_where_the_unattributable_rows_end() {
        let dir = scratch("feed-windows-from");
        let engine = store_at(&dir, 22);
        for row in 1..=3 {
            engine
                .conn()
                .execute(
                    "INSERT INTO change_feed (id, dataset, row_id, op) VALUES (?1, 'task', ?1, 'insert')",
                    [row],
                )
                .unwrap();
        }

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert_eq!(
            engine.get_meta(crate::store_engine::engine::META_FEED_WINDOWS_FROM).unwrap().as_deref(),
            Some("3"),
            "the three rows that predate the column are declared unattributable",
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **v33 rewrites the feed rows the old dataset name left behind.** The registry now carries one word
    /// for the key and the table alike (`AMB-D-807`), so a row still saying `dependency` names a dataset
    /// `sync records` no longer answers to — the very mismatch the fold exists to end.
    #[test]
    fn the_step_that_folds_the_dataset_name_rewrites_the_feed_rows_that_predate_it() {
        let dir = scratch("feed-dataset-fold");
        let engine = store_at(&dir, 32);
        for (id, dataset) in [(1, "dependency"), (2, "task"), (3, "dependency")] {
            engine
                .conn()
                .execute(
                    "INSERT INTO change_feed (id, dataset, row_id, op) VALUES (?1, ?2, ?1, 'insert')",
                    rusqlite::params![id, dataset],
                )
                .unwrap();
        }

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        let named: Vec<String> = engine
            .conn()
            .prepare("SELECT dataset FROM change_feed ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            named,
            ["task_dependency", "task", "task_dependency"],
            "the edges are renamed and nothing else is touched",
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A store with an empty feed has nothing unattributable, so the step leaves no watermark at all —
    /// which reads as `0` and lets a carrier start from the beginning without being told of a gap that
    /// does not exist.
    #[test]
    fn the_step_leaves_no_watermark_on_a_store_whose_feed_is_empty() {
        let dir = scratch("feed-windows-none");
        let engine = store_at(&dir, 22);

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert_eq!(engine.get_meta(crate::store_engine::engine::META_FEED_WINDOWS_FROM).unwrap(), None);
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

    /// **The index-filling steps on every shape the chain starts from.** The word index is created by
    /// genesis, so an older store gets the tables at open — and empty. A record written before the
    /// upgrade is only in the index because a step put it there, and without that step it would be a
    /// task nobody can find by its own title, on exactly the stores nobody's tests are run against
    /// (`AMB-D-450`).
    ///
    /// Both a long term and a short one, since they take different paths to the same copy: seeding one
    /// and not the other would leave half the question unasked. And both an old face and one a later
    /// step widened the index onto — a face added without a step of its own is the same silent hole.
    #[test]
    fn the_word_index_is_filled_in_for_records_that_predate_it() {
        // The newest version whose step fills the index, named literally: this is a question about
        // *those* steps, and a store born at or after the last of them already writes its copies
        // through the field-write funnel.
        const WORD_INDEX_VERSION: i64 = 18;
        for born in OLDEST_FROZEN_VERSION..WORD_INDEX_VERSION {
            let dir = scratch(&format!("word-index-v{born}"));
            let engine = store_at(&dir, born);
            // Written straight into the tables, as a build of that age wrote them: the index did not
            // exist then, so nothing about these rows can have reached it.
            engine
                .conn()
                .execute("INSERT INTO task (id, title, notes) VALUES (1, ?1, '')", ["全文検索の索引"])
                .unwrap();
            engine
                .conn()
                .execute(
                    "INSERT INTO attachment (id, target_type, target_id, kind, filename) \
                       VALUES (1, 'task', 1, 'blob', ?1)",
                    ["計測ログ.md"],
                )
                .unwrap();
            let found = |term: &str| -> bool {
                use crate::store_engine::{schema::col, search, sql::Expr};
                const SD: col::search_doc::Cols = col::search_doc::of("sd");
                let pred = search::term_pred(SD, &search::normalize(term));
                let sql = format!(
                    "SELECT EXISTS(SELECT 1 FROM search_doc sd WHERE {} = 1 AND {})",
                    SD.owner_id.to_sql(),
                    pred.sql(),
                );
                engine
                    .conn()
                    .query_row(&sql, rusqlite::params_from_iter(pred.params()), |r| r.get::<_, bool>(0))
                    .unwrap()
            };
            assert!(!found("全文検索"), "v{born}: nothing is in the index before the chain runs");

            run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

            assert!(found("全文検索"), "v{born}: a long term reaches the record the step indexed");
            assert!(found("検索"), "v{born}: and so does a short one, by the scan path");
            assert!(found("計測ログ"), "v{born}: and so does a face a later step widened the index onto");
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// **v19** (`AMB-D-531`): a folder recorded as a project's main directory arrives in the set of bound
    /// folders, and the table that held it is gone. The fold is what keeps a folder bound to its project
    /// across the upgrade — dropping the table without it would unbind whatever lived only there.
    #[test]
    fn the_main_folder_of_a_binding_lands_in_the_set_and_its_table_goes() {
        // The last version that still had the table, named literally: the question is about that step.
        const MAIN_FOLDER_VERSION: i64 = 18;
        for born in OLDEST_FROZEN_VERSION..MAIN_FOLDER_VERSION {
            let dir = scratch(&format!("binding-fold-v{born}"));
            let engine = store_at(&dir, born);
            engine
                .conn()
                .execute_batch(
                    // A build of that age wrote both tables: one folder recorded on each side, and one
                    // recorded on both.
                    "INSERT INTO binding_path (project_id, dir) VALUES (1, '/work/main'), (2, '/work/both');
                     INSERT INTO binding_project_dir (project_id, dir) VALUES (1, '/work/extra'), (2, '/work/both');",
                )
                .unwrap();

            run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

            let dirs: Vec<(i64, String)> = {
                let conn = engine.conn();
                let mut stmt = conn
                    .prepare("SELECT project_id, dir FROM binding_project_dir ORDER BY project_id, dir")
                    .unwrap();
                let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
                rows.filter_map(|r| r.ok()).collect()
            };
            assert_eq!(
                dirs,
                vec![
                    (1, "/work/extra".to_string()),
                    (1, "/work/main".to_string()),
                    (2, "/work/both".to_string()),
                ],
                "v{born}: the main folder joins the set, and a folder that was in both collapses to one row",
            );
            assert!(
                !engine
                    .conn()
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'binding_path')",
                        [],
                        |r| r.get::<_, bool>(0),
                    )
                    .unwrap(),
                "v{born}: the table the main folder lived in is gone",
            );
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// **v25** (`AMB-D-648`): every folder a store already had bound comes out of the chain carrying an id
    /// something else can point at, with the pairs themselves untouched — the folders are what the store
    /// holds, and a rebuild that lost one would unbind a folder to gain a key.
    ///
    /// The numbering is the set's own ascending order and not the order the rows happen to sit in, so two
    /// machines upgrading the same index arrive at the same ids. What the pair keeps is its uniqueness, now
    /// as a `UNIQUE` rather than the key; what the id gains is retirement, so a folder unbound does not
    /// hand its number to the next one bound.
    /// **v30 on every shape the chain starts from** (`AMB-D-735`). A store born at any frozen version
    /// comes out of the chain with a slug on every axis and every value, with both pairs held unique, and
    /// with the classification it carried still attached — which is the part the rebuild puts at risk,
    /// since the assignments have to leave their table and come back.
    #[test]
    fn the_chain_slugs_the_dimension_model_and_keeps_each_unique() {
        // The last version before the column: a store born at v30 is handed it by genesis with nothing
        // to migrate.
        const SLUGLESS_DIMENSION_VERSION: i64 = 29;
        for born in OLDEST_FROZEN_VERSION..=SLUGLESS_DIMENSION_VERSION {
            let dir = scratch(&format!("dimension-slug-v{born}"));
            let engine = store_at(&dir, born);
            engine
                .conn()
                .execute_batch(
                    "INSERT INTO project (id, name) VALUES (1, 'P'), (2, 'Q');
                     INSERT INTO task (id, title) VALUES (1, 'T');
                     INSERT INTO dimension (id, project_id, name) \
                       VALUES (1, 1, 'フェーズ'), (2, 1, '製品'), (3, 2, 'フェーズ');
                     INSERT INTO dimension_value (id, dimension_id, name) \
                       VALUES (1, 1, '運用第2期'), (2, 1, '運用第1期'), (3, 2, 'Amenbo本体');
                     INSERT INTO task_dimension_value (id, task_id, dimension_id, value_id) \
                       VALUES (1, 1, 1, 1);
                     -- A number issued and then given up: the mark it left must not be handed on.
                     INSERT INTO dimension (id, project_id, name) VALUES (9, 1, 'gone');
                     DELETE FROM dimension WHERE id = 9;",
                )
                .unwrap();

            run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

            let slugs = |sql: &str| -> Vec<(i64, String)> {
                let conn = engine.conn();
                let mut stmt = conn.prepare(sql).unwrap();
                let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
                rows.filter_map(|r| r.ok()).collect()
            };
            assert_eq!(
                slugs("SELECT id, slug FROM dimension ORDER BY id"),
                vec![(1, "d1".to_string()), (2, "d2".to_string()), (3, "d3".to_string())],
                "v{born}: every axis is backfilled from its id, name or no name",
            );
            assert_eq!(
                slugs("SELECT id, slug FROM dimension_value ORDER BY id"),
                vec![(1, "v1".to_string()), (2, "v2".to_string()), (3, "v3".to_string())],
                "v{born}: every value is backfilled from its id",
            );
            let assignments: Vec<(i64, i64, i64, i64)> = {
                let conn = engine.conn();
                let mut stmt = conn
                    .prepare("SELECT id, task_id, dimension_id, value_id FROM task_dimension_value")
                    .unwrap();
                let rows = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                    .unwrap();
                rows.filter_map(|r| r.ok()).collect()
            };
            assert_eq!(
                assignments,
                vec![(1, 1, 1, 1)],
                "v{born}: the classification comes back exactly as it left — the rebuild moves the \
                 assignments out of the way and must put every one of them back",
            );
            assert!(
                engine
                    .conn()
                    .execute("UPDATE dimension SET slug = 'd2' WHERE id = 1", [])
                    .is_err(),
                "v{born}: two axes of one project cannot answer to the same slug",
            );
            engine
                .conn()
                .execute("UPDATE dimension SET slug = 'd1' WHERE id = 3", [])
                .expect("the same slug in another project is another slug");
            assert!(
                engine
                    .conn()
                    .execute("UPDATE dimension_value SET slug = 'v2' WHERE id = 1", [])
                    .is_err(),
                "v{born}: two values of one axis cannot answer to the same slug",
            );
            engine
                .conn()
                .execute("UPDATE dimension_value SET slug = 'v1' WHERE id = 3", [])
                .expect("the same slug on another axis is another slug");
            engine
                .conn()
                .execute("INSERT INTO dimension (project_id, name) VALUES (1, 'next')", [])
                .unwrap();
            assert_eq!(
                slugs("SELECT id, name FROM dimension WHERE name = 'next'"),
                vec![(10, "next".to_string())],
                "v{born}: the number a deleted axis held is still retired — the rebuild carried the \
                 high-water mark across",
            );
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// **v53 folds a two-layer definition into three** (`AMB-D-949`).
    ///
    /// The store is written in v50's shape — a library action carrying a prompt, one step running it and
    /// one step carrying a prompt of its own — and comes out with every step a placement, every prompt
    /// an action holding one step, and the picture drawn between placements instead of steps.
    #[test]
    fn the_chain_folds_the_automation_into_three_layers() {
        // The versions whose automation tables are the two-layer shape, named literally: a store born
        // below v50 is handed today's registry by genesis and arrives already folded.
        for born in 50..=52 {
            let dir = scratch(&format!("three-layers-v{born}"));
            let engine = store_at(&dir, born);
            engine
                .conn()
                .execute_batch(
                    "INSERT INTO project (id, name, notes, order_key, created_at, updated_at) \
                       VALUES (1, 'amenbo', '', 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                     INSERT INTO automation_action (id, project_id, name, prompt, order_key, created_at, updated_at) \
                       VALUES (7, 1, '点検する', 'look at it', 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                     INSERT INTO automation (id, project_id, name, notes, preamble, archived, order_key, created_at, updated_at) \
                       VALUES (3, 1, '1件やりきる', '', '', 0, 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                     INSERT INTO automation_step (id, automation_id, name, action_id, prompt, agent, model, interactive, work_dir_ref, report_to_task, show_history, order_key, created_at, updated_at) \
                       VALUES (11, 3, '取る', NULL, 'take one', 'claude', NULL, 0, NULL, 0, 1, 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                     INSERT INTO automation_step (id, automation_id, name, action_id, prompt, agent, model, interactive, work_dir_ref, report_to_task, show_history, order_key, created_at, updated_at) \
                       VALUES (12, 3, '点検', 7, NULL, 'codex-cli', 'gpt', 0, NULL, 0, 1, 'a1', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                     UPDATE automation SET entry_step_id = 11 WHERE id = 3;
                     INSERT INTO automation_exit (id, owner_kind, owner_id, name, order_key, created_at, updated_at) \
                       VALUES (21, 'step', 11, NULL, 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                     INSERT INTO automation_exit (id, owner_kind, owner_id, name, order_key, created_at, updated_at) \
                       VALUES (22, 'action', 7, NULL, 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                     INSERT INTO automation_port (id, owner_kind, owner_id, direction, name, kind, required, order_key, created_at, updated_at) \
                       VALUES (31, 'exit', 21, 'out', 'タスク', 'task_take', 1, 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                     INSERT INTO automation_cfg (id, owner_kind, owner_id, name, kind, required, options, value, order_key, created_at, updated_at) \
                       VALUES (41, 'action', 7, 'どこまで', 'text', 0, NULL, NULL, 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                     INSERT INTO automation_cfg (id, owner_kind, owner_id, name, kind, required, options, value, order_key, created_at, updated_at) \
                       VALUES (42, 'step', 12, 'どこまで', 'text', 0, NULL, '\"全部\"', 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                     INSERT INTO automation_cfg (id, owner_kind, owner_id, name, kind, required, options, value, order_key, created_at, updated_at) \
                       VALUES (43, 'step', 11, '作業フォルダ', 'folder', 1, NULL, '\"~/work\"', 'a1', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                     INSERT INTO automation_edge (id, automation_id, from_step_id, exit_name, to_step_id, ends, max_times, order_key, created_at, updated_at) \
                       VALUES (51, 3, 11, NULL, 12, 'go', 10, 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                     INSERT INTO automation_note (id, automation_id, name, body, order_key, created_at, updated_at) \
                       VALUES (61, 3, '運転規約', '…', 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                     INSERT INTO automation_step_note (id, step_id, note_id, order_key, created_at, updated_at) \
                       VALUES (71, 11, 61, 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');",
                )
                .unwrap();

            run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

            let conn = engine.conn();
            let one = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
            let text = |sql: &str| -> String { conn.query_row(sql, [], |r| r.get(0)).unwrap() };

            // Every v52 step is a placement, in the order it was added.
            assert_eq!(
                one("SELECT COUNT(*) FROM automation_placement WHERE automation_id = 3"),
                2,
                "v{born}: one placement per step",
            );
            let took = one("SELECT id FROM automation_placement WHERE order_key = 'a0'");
            let looked = one("SELECT id FROM automation_placement WHERE order_key = 'a1'");
            // The step that ran the library action is that action's placement, and the prompt the
            // action used to carry is now the one step inside it.
            assert_eq!(
                one(&format!("SELECT action_id FROM automation_placement WHERE id = {looked}")),
                7,
                "v{born}: the step pointing at action 7 is a placement of it",
            );
            let inside = one("SELECT entry_step_id FROM automation_action WHERE id = 7");
            assert_eq!(
                text(&format!("SELECT prompt FROM automation_action_step WHERE id = {inside}")),
                "look at it",
            );
            assert_eq!(
                text(&format!(
                    "SELECT agent FROM automation_placement_step \
                     WHERE placement_id = {looked} AND step_id = {inside}"
                )),
                "codex-cli",
                "v{born}: the agent is chosen for the step at the spot that ran it (AMB-D-960)",
            );
            assert_eq!(one("SELECT COUNT(*) FROM automation_action_step WHERE id = 12"), 0,
                "v{born}: the row that only called an action is not a step any more");

            // The step that carried its own prompt keeps its row, inside an action written for it.
            let mine = one("SELECT action_id FROM automation_action_step WHERE id = 11");
            assert_eq!(one(&format!("SELECT entry_step_id FROM automation_action WHERE id = {mine}")), 11);
            assert_eq!(
                one(&format!("SELECT action_id FROM automation_placement WHERE id = {took}")),
                mine,
            );
            assert_eq!(text("SELECT prompt FROM automation_action_step WHERE id = 11"), "take one");
            assert_eq!(
                one(&format!(
                    "SELECT COUNT(*) FROM automation_exit WHERE owner_kind = 'action' AND owner_id = {mine}"
                )),
                1,
                "v{born}: what the step declared is mirrored onto the action a placement is wired by",
            );
            assert_eq!(
                one(&format!(
                    "SELECT COUNT(*) FROM automation_port p JOIN automation_exit x ON p.owner_id = x.id \
                     WHERE p.owner_kind = 'exit' AND x.owner_kind = 'action' AND x.owner_id = {mine} \
                       AND p.name = 'タスク'"
                )),
                1,
                "v{born}: and so is what that way out hands on",
            );

            // The entry and the picture both name placements now. What v53 did with the documents is
            // not readable here — v55 takes their tables away, and the chain runs whole
            // (the_shared_documents_and_their_leftover_table_go).
            assert_eq!(one("SELECT entry_placement_id FROM automation WHERE id = 3"), took);
            assert_eq!(text("SELECT owner_kind FROM automation_edge WHERE id = 51"), "automation");
            assert_eq!(one("SELECT owner_id FROM automation_edge WHERE id = 51"), 3);
            assert_eq!(one("SELECT from_id FROM automation_edge WHERE id = 51"), took);
            assert_eq!(one("SELECT to_id FROM automation_edge WHERE id = 51"), looked);

            // A setting was one row where the step declared it and two where the action did; both come
            // out as the one shape — the action declares, the placement answers.
            assert_eq!(text("SELECT owner_kind FROM automation_cfg WHERE id = 41"), "action");
            assert_eq!(text("SELECT owner_kind FROM automation_cfg WHERE id = 42"), "placement");
            assert_eq!(one("SELECT owner_id FROM automation_cfg WHERE id = 42"), looked);
            assert_eq!(text("SELECT owner_kind FROM automation_cfg WHERE id = 43"), "action");
            assert_eq!(
                one("SELECT COUNT(*) FROM automation_cfg WHERE id = 43 AND value IS NULL"),
                1,
                "v{born}: the half that was a declaration keeps no answer",
            );
            assert_eq!(
                one(&format!(
                    "SELECT COUNT(*) FROM automation_cfg WHERE owner_kind = 'placement' \
                       AND owner_id = {took} AND name = '作業フォルダ' AND value = '\"~/work\"'"
                )),
                1,
                "v{born}: and the answer it carried is the placement's",
            );
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// **v54 names the step table after the action it hangs on** (`AMB-D-949`).
    ///
    /// Two shapes answer to the old name, and the step tells them apart. A store that reached v53
    /// with steps in it is carried over whole — the rows keep their ids, what points at them still
    /// points at them, and the ledger hands a carrier a key the registry answers to. A store born
    /// below v50 holds only what v50's frozen text laid down, a two-layer table genesis never asked
    /// for, and comes out without it and on the table genesis built.
    #[test]
    fn the_step_table_takes_the_name_of_the_action_it_hangs_on() {
        let dir = scratch("step-after-its-action-v53");
        let engine = store_at(&dir, 53);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name, notes, order_key, created_at, updated_at) \
                   VALUES (1, 'amenbo', '', 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                 INSERT INTO automation_action (id, project_id, name, order_key, created_at, updated_at) \
                   VALUES (7, 1, '点検する', 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                 INSERT INTO automation_step (id, action_id, name, prompt, agent, model, interactive, work_dir_ref, report_to_task, show_history, order_key, created_at, updated_at) \
                   VALUES (11, 7, '取る', 'take one', 'claude', NULL, 0, NULL, 0, 1, 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                 UPDATE automation_action SET entry_step_id = 11 WHERE id = 7;
                 INSERT INTO change_feed (id, dataset, row_id, op) \
                   VALUES (91, 'automation_step', 11, 'insert');",
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        let conn = engine.conn();
        let one = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        let text = |sql: &str| -> String { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(
            one("SELECT COUNT(*) FROM sqlite_master \
                 WHERE type = 'table' AND name = 'automation_step'"),
            0,
            "nothing answers to the old name afterwards",
        );
        assert_eq!(text("SELECT prompt FROM automation_action_step WHERE id = 11"), "take one");
        assert_eq!(
            one("SELECT entry_step_id FROM automation_action WHERE id = 7"),
            11,
            "the step an action starts at is the row it was",
        );
        assert_eq!(
            text("SELECT dataset FROM change_feed WHERE id = 91"),
            "automation_action_step",
            "and what the ledger hands a carrier is a key `sync records` answers to",
        );
        // The `REFERENCES` clauses were rewritten with the table, and a write is the proof of it:
        // left naming a table that is gone, this insert would fail rather than land.
        conn.execute(
            "INSERT INTO automation_action_step \
                 (id, action_id, name, prompt, order_key, created_at, updated_at) \
             VALUES (12, 7, '点検', 'look at it', 'a1', '2026-01-02T03:04:05Z', \
                 '2026-01-02T03:04:05Z')",
            [],
        )
        .expect("a step still hangs on its action");
        conn.execute("UPDATE automation_action SET entry_step_id = 12 WHERE id = 7", [])
            .expect("and an action still names one of them");
        std::fs::remove_dir_all(&dir).ok();

        let dir = scratch("step-after-its-action-baseline");
        let engine = store_at(&dir, OLDEST_FROZEN_VERSION);
        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();
        let conn = engine.conn();
        let one = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(
            one("SELECT COUNT(*) FROM sqlite_master \
                 WHERE type = 'table' AND name = 'automation_step'"),
            0,
            "the table v50 lays down on a store that never had one goes",
        );
        assert_eq!(
            one("SELECT COUNT(*) FROM pragma_table_info('automation_action_step') \
                 WHERE name = 'automation_id'"),
            0,
            "and it is not carried over the table genesis built",
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **v56 drops the note table no registry has** (`AMB-D-949`).
    ///
    /// The store that carries it this far is the one the fold passed over: born below v50, handed
    /// today's registry by genesis, and then handed v50's frozen text on top of that. The chain is
    /// run to v55 first so the table is seen standing, and stopped at v56 rather than run whole —
    /// what the assertions after it name is this step's work and no other's.
    #[test]
    fn the_note_table_v50_lays_down_is_dropped() {
        let dir = scratch("step-note-baseline");
        let engine = store_at(&dir, OLDEST_FROZEN_VERSION);
        let standing = |table: &str| -> i64 {
            engine
                .conn()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap()
        };

        run(&engine, &dir, steps_through(55), &mut crate::progress::ignore).unwrap();
        assert_eq!(
            standing("automation_step_note"),
            1,
            "v50 lays the table down on a store that never asked for it",
        );

        run(&engine, &dir, steps_through(56), &mut crate::progress::ignore).unwrap();

        assert_eq!(standing("automation_step_note"), 0, "and the step after v55 takes it away");
        std::fs::remove_dir_all(&dir).ok();

        // That this step reaches **only** the name no registry ever had is drawn on a store that
        // carries the declared one: a store born at v53 gets `automation_placement_note` from that
        // version's frozen text, and stands there at v56 — v59 is what takes it
        // (the_shared_documents_go_and_an_action_says_what_it_is_for).
        let dir = scratch("step-note-v53");
        let engine = store_at(&dir, 53);
        run(&engine, &dir, steps_through(56), &mut crate::progress::ignore).unwrap();
        assert_eq!(
            engine
                .conn()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master \
                     WHERE type = 'table' AND name = 'automation_placement_note'",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap(),
            1,
            "the note table the registry did declare is left where it stands",
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **v55 joins what an action declares to the step inside it** (`AMB-T-5311`).
    ///
    /// The store is written in v53's shape — an action of one step, declaring the same names twice over
    /// — and comes out with a line drawn for every name the two share, and none for a name only one of
    /// them carries. An action of two steps is left alone: which of them leaves by which way out is
    /// nobody's to guess.
    #[test]
    fn the_chain_joins_the_action_to_the_step_inside_it() {
        const STAMP: &str = "'2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z'";
        let dir = scratch("join-the-action-v53");
        let engine = store_at(&dir, 53);
        engine
            .conn()
            .execute_batch(&format!(
                "INSERT INTO project (id, name, notes, order_key, created_at, updated_at) \
                   VALUES (1, 'amenbo', '', 'a0', {STAMP});
                 INSERT INTO automation_action (id, project_id, name, entry_step_id, order_key, created_at, updated_at) \
                   VALUES (7, 1, '点検する', NULL, 'a0', {STAMP});
                 INSERT INTO automation_step (id, action_id, name, prompt, agent, model, interactive, work_dir_ref, report_to_task, show_history, order_key, created_at, updated_at) \
                   VALUES (11, 7, '点検する', 'look', 'claude', NULL, 0, NULL, 0, 1, 'a0', {STAMP});
                 INSERT INTO automation_action (id, project_id, name, entry_step_id, order_key, created_at, updated_at) \
                   VALUES (8, 1, '二手で直す', NULL, 'a1', {STAMP});
                 INSERT INTO automation_step (id, action_id, name, prompt, agent, model, interactive, work_dir_ref, report_to_task, show_history, order_key, created_at, updated_at) \
                   VALUES (12, 8, '直す', 'fix', 'claude', NULL, 0, NULL, 0, 1, 'a0', {STAMP});
                 INSERT INTO automation_step (id, action_id, name, prompt, agent, model, interactive, work_dir_ref, report_to_task, show_history, order_key, created_at, updated_at) \
                   VALUES (13, 8, '確かめる', 'check', 'claude', NULL, 0, NULL, 0, 1, 'a1', {STAMP});
                 UPDATE automation_action SET entry_step_id = 11 WHERE id = 7;
                 UPDATE automation_action SET entry_step_id = 12 WHERE id = 8;
                 INSERT INTO automation_exit (id, owner_kind, owner_id, name, order_key, created_at, updated_at) VALUES \
                   (21, 'step', 11, NULL, 'a0', {STAMP}), \
                   (22, 'step', 11, '*', 'a1', {STAMP}), \
                   (23, 'step', 11, '直すところがある', 'a2', {STAMP}), \
                   (24, 'step', 11, '中だけの終わり', 'a3', {STAMP}), \
                   (25, 'action', 7, NULL, 'a0', {STAMP}), \
                   (26, 'action', 7, '*', 'a1', {STAMP}), \
                   (27, 'action', 7, '直すところがある', 'a2', {STAMP}), \
                   (28, 'step', 12, NULL, 'a0', {STAMP}), \
                   (29, 'action', 8, NULL, 'a0', {STAMP});
                 INSERT INTO automation_port (id, owner_kind, owner_id, direction, name, kind, required, order_key, created_at, updated_at) VALUES \
                   (31, 'action', 7, 'in', '差分', 'file', 1, 'a0', {STAMP}), \
                   (32, 'step', 11, 'in', '差分', 'file', 1, 'a0', {STAMP}), \
                   (33, 'action', 7, 'in', '誰も読まない', 'value', 0, 'a1', {STAMP});",
            ))
            .unwrap();

        // Through v61: v62 keys what this step names (`key_the_ways_out`), and is read on its own.
        run(&engine, &dir, steps_through(61), &mut crate::progress::ignore).unwrap();

        let conn = engine.conn();
        let one = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        let names = {
            let mut stmt = conn
                .prepare(
                    "SELECT exit_name, exit_to FROM automation_edge \
                     WHERE owner_kind = 'action' AND owner_id = 7 AND ends = 'exit' \
                     ORDER BY order_key",
                )
                .unwrap();
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)))
                .unwrap();
            rows.filter_map(|r| r.ok()).collect::<Vec<_>>()
        };
        assert_eq!(
            names,
            vec![
                (None, None),
                (Some("*".to_string()), Some("*".to_string())),
                (Some("直すところがある".to_string()), Some("直すところがある".to_string())),
            ],
            "every way out the two share is joined, and the one only the step has is not",
        );
        assert_eq!(
            one("SELECT COUNT(*) FROM automation_edge WHERE owner_kind = 'action' AND owner_id = 8"),
            0,
            "an action of two steps is left to its author",
        );
        assert_eq!(
            one("SELECT COUNT(*) FROM automation_wire WHERE owner_kind = 'action' AND owner_id = 7 \
                   AND from_id = 0 AND from_port_name = '差分' AND to_id = 11 AND to_port_name = '差分'"),
            1,
            "the input both declare is wired from the action onto the step",
        );
        assert_eq!(
            one("SELECT COUNT(*) FROM automation_wire WHERE from_port_name = '誰も読まない'"),
            0,
            "and the one the step does not take in reaches nothing",
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **v58 wires what the lone step hands on out to the action** (`AMB-T-5341`).
    ///
    /// The store is written in v53's shape, with an output on two of the step's ways out: one the
    /// action carries a way out and an output of the same name for, and one it does not. Only the
    /// first is wired out. A wire the author had already drawn between those same two ends is not
    /// drawn twice, and an action of two steps is left alone.
    #[test]
    fn the_chain_wires_what_a_lone_step_hands_on_out_to_its_action() {
        const STAMP: &str = "'2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z'";
        let dir = scratch("wire-the-outputs-v53");
        let engine = store_at(&dir, 53);
        engine
            .conn()
            .execute_batch(&format!(
                "INSERT INTO project (id, name, notes, order_key, created_at, updated_at) \
                   VALUES (1, 'amenbo', '', 'a0', {STAMP});
                 INSERT INTO automation_action (id, project_id, name, entry_step_id, order_key, created_at, updated_at) \
                   VALUES (7, 1, '点検する', NULL, 'a0', {STAMP});
                 INSERT INTO automation_step (id, action_id, name, prompt, agent, model, interactive, work_dir_ref, report_to_task, show_history, order_key, created_at, updated_at) \
                   VALUES (11, 7, '点検する', 'look', 'claude', NULL, 0, NULL, 0, 1, 'a0', {STAMP});
                 INSERT INTO automation_action (id, project_id, name, entry_step_id, order_key, created_at, updated_at) \
                   VALUES (8, 1, '二手で直す', NULL, 'a1', {STAMP});
                 INSERT INTO automation_step (id, action_id, name, prompt, agent, model, interactive, work_dir_ref, report_to_task, show_history, order_key, created_at, updated_at) VALUES \
                   (12, 8, '直す', 'fix', 'claude', NULL, 0, NULL, 0, 1, 'a0', {STAMP}), \
                   (13, 8, '確かめる', 'check', 'claude', NULL, 0, NULL, 0, 1, 'a1', {STAMP});
                 UPDATE automation_action SET entry_step_id = 11 WHERE id = 7;
                 UPDATE automation_action SET entry_step_id = 12 WHERE id = 8;
                 INSERT INTO automation_exit (id, owner_kind, owner_id, name, order_key, created_at, updated_at) VALUES \
                   (21, 'step', 11, NULL, 'a0', {STAMP}), \
                   (23, 'step', 11, '直すところがある', 'a2', {STAMP}), \
                   (24, 'step', 11, '中だけの終わり', 'a3', {STAMP}), \
                   (25, 'action', 7, NULL, 'a0', {STAMP}), \
                   (27, 'action', 7, '直すところがある', 'a2', {STAMP}), \
                   (28, 'step', 12, NULL, 'a0', {STAMP}), \
                   (29, 'action', 8, NULL, 'a0', {STAMP});
                 INSERT INTO automation_port (id, owner_kind, owner_id, direction, name, kind, required, order_key, created_at, updated_at) VALUES \
                   (41, 'exit', 21, 'out', '報告', 'file', 1, 'a0', {STAMP}), \
                   (42, 'exit', 25, 'out', '報告', 'file', 1, 'a0', {STAMP}), \
                   (43, 'exit', 23, 'out', '直すところ', 'value', 1, 'a0', {STAMP}), \
                   (44, 'exit', 27, 'out', '直すところ', 'value', 1, 'a0', {STAMP}), \
                   (45, 'exit', 24, 'out', '中だけの値', 'value', 0, 'a0', {STAMP}), \
                   (46, 'exit', 28, 'out', '報告', 'file', 1, 'a0', {STAMP}), \
                   (47, 'exit', 29, 'out', '報告', 'file', 1, 'a0', {STAMP});
                 INSERT INTO automation_wire (id, owner_kind, owner_id, from_id, from_exit_name, from_port_name, to_id, to_port_name, created_at, updated_at) \
                   VALUES (51, 'action', 7, 11, NULL, '報告', 0, '報告', {STAMP});",
            ))
            .unwrap();

        // Through v61: v62 keys what this step names (`key_the_ways_out`), and is read on its own.
        run(&engine, &dir, steps_through(61), &mut crate::progress::ignore).unwrap();

        let conn = engine.conn();
        let drawn = {
            let mut stmt = conn
                .prepare(
                    "SELECT from_exit_name, from_port_name FROM automation_wire \
                     WHERE owner_kind = 'action' AND owner_id = 7 AND to_id = 0 ORDER BY id",
                )
                .unwrap();
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?)))
                .unwrap();
            rows.filter_map(|r| r.ok()).collect::<Vec<_>>()
        };
        assert_eq!(
            drawn,
            vec![
                (None, "報告".to_string()),
                (Some("直すところがある".to_string()), "直すところ".to_string()),
            ],
            "the output both carry is wired out once, and the wire already there is not doubled; \
             the output only the step carries reaches nothing",
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM automation_wire WHERE owner_kind = 'action' AND owner_id = 8",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap(),
            0,
            "an action of two steps is left to its author",
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **v59 takes the shared documents away and gives an action a note** (`AMB-D-952`).
    ///
    /// The store reached v53 with a document on it and a link handing it to a placement, and with
    /// the document's body copied into the word index. It comes out with neither table, with
    /// nothing left of the document in `search_doc`, and with the column the build screen draws
    /// what an action is for from. The third name of that family went at v56
    /// (the_note_table_v50_lays_down_is_dropped).
    #[test]
    fn the_shared_documents_go_and_an_action_says_what_it_is_for() {
        let dir = scratch("shared-documents-v53");
        let engine = store_at(&dir, 53);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name, notes, order_key, created_at, updated_at) \
                   VALUES (1, 'amenbo', '', 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                 INSERT INTO automation_action (id, project_id, name, order_key, created_at, updated_at) \
                   VALUES (7, 1, '点検する', 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                 INSERT INTO automation (id, project_id, name, notes, preamble, archived, order_key, created_at, updated_at) \
                   VALUES (3, 1, '1件やりきる', '', '', 0, 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                 INSERT INTO automation_placement (id, automation_id, action_id, order_key, created_at, updated_at) \
                   VALUES (11, 3, 7, 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                 INSERT INTO automation_note (id, automation_id, name, body, order_key, created_at, updated_at) \
                   VALUES (61, 3, '運転規約', '…', 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                 INSERT INTO automation_placement_note (id, placement_id, note_id, order_key, created_at, updated_at) \
                   VALUES (71, 11, 61, 'a0', '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z');
                 INSERT INTO search_doc (owner_kind, owner_id, field, norm) \
                   VALUES ('automation_note', 61, 'body', '…');",
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        let conn = engine.conn();
        let one = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        for gone in ["automation_note", "automation_placement_note"] {
            assert_eq!(
                one(&format!(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '{gone}'"
                )),
                0,
                "`{gone}` is gone",
            );
        }
        assert_eq!(
            one("SELECT COUNT(*) FROM search_doc WHERE owner_kind = 'automation_note'"),
            0,
            "and the copies the index took of their bodies went with them",
        );
        assert_eq!(
            one("SELECT COUNT(*) FROM pragma_table_info('automation_action') WHERE name = 'note'"),
            1,
            "an action says what it is for",
        );
        conn.execute("UPDATE automation_action SET note = 'ここを読んでから' WHERE id = 7", [])
            .expect("and the column takes a write");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_chain_gives_a_bound_folder_an_id_and_keeps_the_pair_unique() {
        // The last version whose bindings were keyed by the pair alone, named literally: the question is
        // about that step, and a store born at v25 is handed the id by genesis with nothing to migrate.
        const KEYLESS_BINDING_VERSION: i64 = 24;
        for born in OLDEST_FROZEN_VERSION..=KEYLESS_BINDING_VERSION {
            let dir = scratch(&format!("binding-id-v{born}"));
            let engine = store_at(&dir, born);
            // Written out of order on purpose: what numbers them is the set's order, not this one.
            engine
                .conn()
                .execute_batch(
                    "INSERT INTO binding_project_dir (project_id, dir) \
                       VALUES (2, '/work/c'), (1, '/work/b'), (1, '/work/a');",
                )
                .unwrap();

            run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

            let rows = |sql: &str| -> Vec<(i64, i64, String)> {
                let conn = engine.conn();
                let mut stmt = conn.prepare(sql).unwrap();
                let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
                rows.filter_map(|r| r.ok()).collect()
            };
            assert_eq!(
                rows("SELECT id, project_id, dir FROM binding_project_dir ORDER BY id"),
                vec![
                    (1, 1, "/work/a".to_string()),
                    (2, 1, "/work/b".to_string()),
                    (3, 2, "/work/c".to_string()),
                ],
                "v{born}: every folder keeps its project and its path, and is numbered in the set's order",
            );
            assert!(
                engine
                    .conn()
                    .execute(
                        "INSERT INTO binding_project_dir (project_id, dir) VALUES (1, '/work/a')",
                        [],
                    )
                    .is_err(),
                "v{born}: one folder is still recorded for one project once",
            );
            engine.conn().execute("DELETE FROM binding_project_dir WHERE id = 3", []).unwrap();
            engine
                .conn()
                .execute("INSERT INTO binding_project_dir (project_id, dir) VALUES (2, '/work/d')", [])
                .unwrap();
            assert_eq!(
                rows("SELECT id, project_id, dir FROM binding_project_dir WHERE dir = '/work/d'"),
                vec![(4, 2, "/work/d".to_string())],
                "v{born}: the number an unbound folder held is retired, not handed on",
            );
            std::fs::remove_dir_all(&dir).ok();
        }
    }

    /// v44 in full, on the shape v43 left behind: an outbox under the mechanism's spelling, with an event
    /// in it and all three of its `store_meta` keys written beside it. The rename must be a rename —
    /// the row, the id it was numbered with and the high-water mark the next id comes from all stay what
    /// they were, and each key's value arrives under the new spelling with nothing left at the old one. A
    /// cursor left behind would read back as a drive that had walked nothing, and every event still in
    /// the table would be carried out a second time.
    #[test]
    fn the_outbox_and_its_cursors_lose_the_mechanisms_spelling() {
        let dir = scratch("outbox-rename");
        let engine = store_at(&dir, 43);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO plugin_outbox (id, event, record_id, actor, at, project)
                     VALUES (17, 'task.done', 9, 'ai', '2026-09-15T09:00:00Z', 3);
                 INSERT INTO store_meta (key, value) VALUES
                     ('plugin_dispatch_cursor', '17'),
                     ('plugin_dispatch_cursor_face', 'cli'),
                     ('plugin_outbox_truncated_through', '4');",
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        assert!(
            !table_is_here(&engine.conn().unchecked_transaction().unwrap(), "plugin_outbox").unwrap(),
            "nothing is left under the old name for a later reader to find",
        );
        let row: (i64, String, i64, Option<i64>) = engine
            .conn()
            .query_row("SELECT id, event, record_id, project FROM outbox", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .expect("the event that was in the outbox is in the outbox");
        assert_eq!(row, (17, "task.done".to_string(), 9, Some(3)));
        for (key, value) in [
            (crate::outbox_drive::CURSOR_META, "17"),
            (crate::outbox_drive::CURSOR_FACE_META, "cli"),
            ("outbox_truncated_through", "4"),
        ] {
            assert_eq!(engine.get_meta(key).unwrap().as_deref(), Some(value), "{key} carries what it held");
        }
        for gone in
            ["plugin_dispatch_cursor", "plugin_dispatch_cursor_face", "plugin_outbox_truncated_through"]
        {
            assert_eq!(engine.get_meta(gone).unwrap(), None, "{gone} is not left standing beside its heir");
        }
        engine
            .conn()
            .execute(
                "INSERT INTO outbox (event, record_id, actor, at) \
                 VALUES ('task.created', 10, 'ai', '2026-09-15T09:00:01Z')",
                [],
            )
            .unwrap();
        let next: i64 = engine
            .conn()
            .query_row("SELECT MAX(id) FROM outbox", [], |r| r.get(0))
            .unwrap();
        assert_eq!(next, 18, "the id the table had reached is still the one the next event comes after");
        std::fs::remove_dir_all(&dir).ok();
    }
    /// v45 in full: the panes kept in the arrangement come out with drawn ids, and the home each of
    /// the two place-resumed providers was running in comes out under the same new name. A home left
    /// behind would be the pane's conversation, standing under a number nothing claims any more —
    /// and the pane itself opening an empty one beside it.
    #[test]
    fn the_panes_are_drawn_afresh_and_their_homes_move_with_them() {
        let dir = scratch("pane-ids");
        let engine = store_at(&dir, 44);
        engine
            .set_meta(
                "talk.layout",
                Some(
                    r#"{"project":1,"panes":[
                        {"id":"1","project":1,"agent":"codex-cli","resume":"/data/codex-homes/1"},
                        {"id":"2","project":1,"agent":"gemini-cli","resume":"/data/gemini-homes/2"},
                        {"id":"3","project":1,"agent":"claude-code","resume":"0f9c-3"}]}"#,
                ),
            )
            .unwrap();
        std::fs::create_dir_all(dir.join("codex-homes/1")).unwrap();
        std::fs::write(dir.join("codex-homes/1/auth.json"), "{}").unwrap();
        std::fs::create_dir_all(dir.join("gemini-homes/2/.gemini")).unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        let kept = engine.get_meta("talk.layout").unwrap().expect("the arrangement");
        let row: serde_json::Value = serde_json::from_str(&kept).unwrap();
        let ids: Vec<String> = row["panes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pane| pane["id"].as_str().unwrap().to_string())
            .collect();
        for id in &ids {
            assert_eq!(id.len(), 36, "a pane's id is a UUID now, not a count: {id}");
            assert_eq!(id.matches('-').count(), 4, "{id}");
        }
        assert_ne!(ids[0], ids[1], "and each pane is drawn its own");

        assert!(
            dir.join("codex-homes").join(&ids[0]).join("auth.json").is_file(),
            "the home the first pane was running in is under its new name, with what was in it",
        );
        assert!(!dir.join("codex-homes/1").exists(), "and nothing is left under the old one");
        assert!(dir.join("gemini-homes").join(&ids[1]).join(".gemini").is_dir());
        assert!(!dir.join("gemini-homes/2").exists());

        // The way back for those two *is* the path of that directory, so it moves with the name.
        let resume = |at: usize| row["panes"][at]["resume"].as_str().unwrap().to_string();
        assert_eq!(resume(0), format!("/data/codex-homes/{}", ids[0]));
        assert_eq!(resume(1), format!("/data/gemini-homes/{}", ids[1]));
        // And a handle that is a session id rather than a place is left as it was, however it ends.
        assert_eq!(resume(2), "0f9c-3", "a session id carries nothing of the pane's id");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v46: the two tables the session a task or a decision was made in is kept in, on a store that
    /// predates them. Genesis usually gets there first, so what this asks is that the chain lands on
    /// the same shape either way — the unique index included, which is what says one owner has at most
    /// one answer.
    #[test]
    fn the_chain_gives_a_store_the_tables_a_pane_is_recorded_in() {
        let dir = scratch("made-in");
        let engine = store_at(&dir, 45);

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        let tx = engine.conn().unchecked_transaction().unwrap();
        for (table, owner, index) in [
            ("task_made_in", "task_id", "task_made_in_by_task"),
            ("decision_made_in", "decision_id", "decision_made_in_by_decision"),
        ] {
            assert!(table_is_here(&tx, table).unwrap(), "{table}");
            let cols = column_names(&tx, table).unwrap();
            for name in [owner, "pane", "pane_name", "pane_resume"] {
                assert!(cols.iter().any(|c| c == name), "{table}.{name}");
            }
            let unique: bool = tx
                .prepare("SELECT \"unique\" FROM pragma_index_list(?1) WHERE name = ?2")
                .unwrap()
                .query_row(rusqlite::params![table, index], |r| r.get(0))
                .unwrap();
            assert!(unique, "{index} is what says one owner has at most one answer");
            let rows: i64 = tx
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(rows, 0, "{table}: there is nothing to seed it from");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A machine that never opened the talk window has no row to convert, and one whose row will not
    /// parse has nothing readable in it — neither is a reason to refuse the rest of the chain.
    #[test]
    fn a_store_with_no_arrangement_to_draw_passes_the_step() {
        let dir = scratch("pane-ids-none");
        let engine = store_at(&dir, 44);
        engine.set_meta("talk.layout", Some("{not json")).unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        assert_eq!(engine.get_meta("talk.layout").unwrap().as_deref(), Some("{not json"));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v48 in full: the closed set narrows to the two values that outlive the acceptance, and every
    /// row lands on one of them (`AMB-D-918`). `accepted` is `decided` under a new name; `proposed`
    /// goes to `rejected`, because `draft` was seeded `0` by v47 and neither of the other two homes
    /// tells the truth about a proposal nobody ruled on. The empty status a row caught mid-create
    /// carries is left where it is — it is not one of the three the step speaks for.
    #[test]
    fn the_proposal_is_folded_away_and_the_acceptance_is_renamed() {
        let dir = scratch("decision-status-fold");
        let engine = store_at(&dir, 47);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO decision (id, project_id, title, body, status, created_at, updated_at) VALUES
                     (1, 1, 'settled',  '', 'accepted', '2025-12-01T00:00:00Z', '2025-12-01T00:00:00Z'),
                     (2, 1, 'proposed', '', 'proposed', '2025-11-01T00:00:00Z', '2025-11-01T00:00:00Z'),
                     (3, 1, 'declined', '', 'rejected', '2025-10-01T00:00:00Z', '2025-10-01T00:00:00Z'),
                     (4, 1, '',         '', '',         '',                     '');",
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        let folded: Vec<(i64, String)> = {
            let conn = engine.conn();
            let mut stmt = conn.prepare("SELECT id, status FROM decision ORDER BY id").unwrap();
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };
        assert_eq!(
            folded,
            vec![
                (1, "decided".to_string()),
                (2, "rejected".to_string()),
                (3, "rejected".to_string()),
                (4, String::new()),
            ],
        );

        let declared = declared_sql(&engine, "decision");
        assert!(
            declared.contains("CHECK(status IN ('', 'decided', 'rejected'))"),
            "the closed set is the new one: {declared}"
        );
        assert!(
            engine
                .conn()
                .execute(
                    "UPDATE decision SET status = 'accepted' WHERE id = 1",
                    [],
                )
                .is_err(),
            "and the retired value is refused on the way in, not merely absent"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v62 in full: every line and every record keys the way out it named (`AMB-D-961`). A line whose
    /// name no way out carries any more goes; a copy whose step is gone keeps its ways out under ids
    /// below zero; and a record with no name reads as the unnamed way out where the step had finished,
    /// and as none where it was still running.
    #[test]
    fn the_chain_keys_every_way_out_a_line_or_a_record_named() {
        let dir = scratch("key-the-ways-out");
        let engine = store_at(&dir, 61);
        engine
            .conn()
            .execute_batch(
                r#"INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO automation (id, project_id, name) VALUES (1, 1, 'A');
                 INSERT INTO automation_action (id, project_id, name) VALUES (7, 1, '点検する');
                 INSERT INTO automation_action_step (id, action_id, name, agent) VALUES (11, 7, '点検する', 'claude');
                 INSERT INTO automation_placement (id, automation_id, action_id) VALUES (3, 1, 7);
                 INSERT INTO automation_exit (id, owner_kind, owner_id, name) VALUES
                     (21, 'step', 11, NULL), (22, 'step', 11, '*'), (23, 'step', 11, '直す'),
                     (25, 'action', 7, NULL), (26, 'action', 7, '*'), (27, 'action', 7, '直す');
                 INSERT INTO automation_port (id, owner_kind, owner_id, direction, name, kind) VALUES
                     (41, 'exit', 23, 'out', 'メモ', 'value'), (42, 'exit', 27, 'out', 'メモ', 'value'),
                     (43, 'action', 7, 'in', '差分', 'value'), (44, 'step', 11, 'in', '差分', 'value'),
                     (45, 'action', 7, 'in', 'メモ', 'value');
                 INSERT INTO automation_edge (id, owner_kind, owner_id, from_id, exit_name, ends, exit_to) VALUES
                     (61, 'automation', 1, 3, '直す', 'done', NULL),
                     (62, 'automation', 1, 3, NULL, 'done', NULL),
                     (63, 'action', 7, 11, '直す', 'exit', '直す'),
                     (64, 'action', 7, 11, NULL, 'exit', NULL),
                     (65, 'action', 7, 11, '改名前', 'done', NULL),
                     (66, 'action', 7, 11, '*', 'exit', '消えた');
                 INSERT INTO automation_wire (id, owner_kind, owner_id, from_id, from_exit_name, from_port_name, to_id, to_port_name) VALUES
                     (71, 'action', 7, 11, '直す', 'メモ', 0, 'メモ'),
                     (72, 'action', 7, 0, NULL, '差分', 11, '差分'),
                     (73, 'automation', 1, 3, '直す', 'メモ', 3, 'メモ'),
                     (74, 'action', 7, 11, '改名前', 'メモ', 0, 'メモ');
                 INSERT INTO automation_run (id, automation_id, project_id, status) VALUES (1, 1, 1, 'running');
                 INSERT INTO automation_run_def (id, run_id, placement_id, step_id, name, agent, exits, ins, cfg) VALUES
                     (81, 1, 3, 11, '点検する', 'claude',
                      '[{"name":null,"outs":[]},{"name":"*","outs":[]},{"name":"直す","outs":[]}]', '[]', '{}'),
                     (82, 1, NULL, NULL, '消えた', 'claude',
                      '[{"name":null,"outs":[]},{"name":"戻す","outs":[]}]', '[]', '{}');
                 INSERT INTO automation_run_step (id, run_id, run_def_id, seq, exit_name, status) VALUES
                     (91, 1, 81, 1, '直す', 'done'),
                     (92, 1, 81, 2, NULL, 'done'),
                     (93, 1, 82, 3, '戻す', 'done'),
                     (94, 1, 81, 4, NULL, 'running');
                 INSERT INTO automation_run_value (id, run_step_id, direction, exit_name, name, kind) VALUES
                     (101, 91, 'out', '直す', 'メモ', 'value'),
                     (102, 92, 'out', NULL, 'メモ', 'value'),
                     (103, 94, 'out', NULL, 'メモ', 'value'),
                     (104, 91, 'in', NULL, '差分', 'value');"#,
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        let conn = engine.conn();
        let rows = |sql: &str| -> Vec<(i64, Option<i64>, Option<i64>)> {
            let mut stmt = conn.prepare(sql).unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        assert_eq!(
            rows("SELECT id, exit_id, exit_to_id FROM automation_edge ORDER BY id"),
            vec![
                (61, Some(27), None),
                (62, Some(25), None),
                (63, Some(23), Some(27)),
                (64, Some(21), Some(25)),
                (66, Some(22), None),
            ],
            "the edge on a name nobody carries went; the one returning to one keeps no key",
        );
        assert_eq!(
            rows("SELECT id, from_exit_id, to_id FROM automation_wire ORDER BY id"),
            vec![(71, Some(23), Some(0)), (72, None, Some(11)), (73, Some(27), Some(3))],
        );
        let copies: Vec<(i64, String)> = {
            let mut stmt = conn.prepare("SELECT id, exits FROM automation_run_def ORDER BY id").unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(|r| r.unwrap()).collect()
        };
        let ids = |json: &str| -> Vec<i64> {
            serde_json::from_str::<Vec<crate::model::RunDefExit>>(json)
                .expect("a copy reads back as today's shape")
                .iter()
                .map(|e| e.id)
                .collect()
        };
        assert_eq!(ids(&copies[0].1), vec![21, 22, 23], "from the live rows");
        assert_eq!(ids(&copies[1].1), vec![-1, -2], "the step is gone, so below zero");
        assert_eq!(
            rows("SELECT id, exit_id, NULL FROM automation_run_step ORDER BY id"),
            vec![(91, Some(23), None), (92, Some(21), None), (93, Some(-2), None), (94, None, None)],
            "a finished step with no name left by the unnamed way out; a running one by none",
        );
        assert_eq!(
            rows("SELECT id, exit_id, NULL FROM automation_run_value ORDER BY id"),
            vec![(101, Some(23), None), (102, Some(21), None), (103, None, None), (104, None, None)],
        );
        for (table, column) in [
            ("automation_edge", "exit_name"),
            ("automation_edge", "exit_to"),
            ("automation_wire", "from_exit_name"),
            ("automation_run_step", "exit_name"),
            ("automation_run_value", "exit_name"),
        ] {
            let held: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
                    [table, column],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(held, 0, "{table}.{column} is gone");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v66 in full: a copy in a run that can still open a step carries what follows each of its ways
    /// out — the line inside the action as it stands, and, where that returns to one of the action's
    /// ways out, the automation's line from that way out going on to the step the next placement opens
    /// first — and the copy the run starts at is marked. A copy in an ended run is left.
    #[test]
    fn the_chain_copies_the_lines_into_a_run_that_can_still_open_a_step() {
        let dir = scratch("copy-the-lines");
        let engine = store_at(&dir, 65);
        engine
            .conn()
            .execute_batch(
                r#"INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO automation (id, project_id, name) VALUES (1, 1, 'A');
                 INSERT INTO automation_action (id, project_id, name) VALUES (7, 1, '書いて見直す'), (8, 1, '出す');
                 INSERT INTO automation_action_step (id, action_id, name) VALUES
                     (11, 7, '書く'), (12, 7, '見直す'), (13, 8, '出す');
                 UPDATE automation_action SET entry_step_id = 11 WHERE id = 7;
                 UPDATE automation_action SET entry_step_id = 13 WHERE id = 8;
                 INSERT INTO automation_placement (id, automation_id, action_id) VALUES (3, 1, 7), (4, 1, 8);
                 UPDATE automation SET entry_placement_id = 3 WHERE id = 1;
                 INSERT INTO automation_exit (id, owner_kind, owner_id, name) VALUES
                     (21, 'step', 11, NULL), (22, 'step', 12, NULL), (25, 'action', 7, NULL);
                 INSERT INTO automation_edge (id, owner_kind, owner_id, from_id, exit_id, to_id, ends, exit_to_id, max_times) VALUES
                     (61, 'action', 7, 11, 21, 12, 'go', NULL, 3),
                     (62, 'action', 7, 12, 22, NULL, 'exit', 25, NULL),
                     (63, 'automation', 1, 3, 25, 4, 'go', NULL, NULL);
                 INSERT INTO automation_run (id, automation_id, project_id, status) VALUES
                     (1, 1, 1, 'running'), (2, 1, 1, 'completed');
                 INSERT INTO automation_run_def (id, run_id, placement_id, step_id, name, agent, exits, ins, cfg) VALUES
                     (81, 1, 3, 11, '書く', 'claude', '[{"id":21,"name":null,"outs":[]}]', '[]', '[]'),
                     (82, 1, 3, 12, '見直す', 'claude', '[{"id":22,"name":null,"outs":[]}]', '[]', '[]'),
                     (83, 1, 4, 13, '出す', 'claude', '[]', '[]', '[]'),
                     (84, 2, 3, 11, '書く', 'claude', '[{"id":21,"name":null,"outs":[]}]', '[]', '[]');"#,
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        let conn = engine.conn();
        let exits = |id: i64| -> Vec<crate::model::RunDefExit> {
            let json: String = conn
                .query_row("SELECT exits FROM automation_run_def WHERE id = ?1", [id], |r| r.get(0))
                .unwrap();
            serde_json::from_str(&json).expect("a copy reads back as today's shape")
        };
        use crate::model::{AutomationEnds, AutomationPictureOwner, RunDefLine};
        let inside = exits(81);
        assert_eq!(
            inside[0].then,
            Some(RunDefLine {
                edge_id: 61,
                picture: AutomationPictureOwner::Action,
                from_id: 11,
                exit_id: 21,
                ends: AutomationEnds::Go,
                placement_id: Some(3),
                step_id: Some(12),
                max_times: Some(3),
            }),
            "inside the action, on to its next step",
        );
        assert_eq!(inside[0].returns_to, None);
        let across = exits(82);
        assert_eq!(
            across[0].then,
            Some(RunDefLine {
                edge_id: 63,
                picture: AutomationPictureOwner::Automation,
                from_id: 3,
                exit_id: 25,
                ends: AutomationEnds::Go,
                placement_id: Some(4),
                step_id: Some(13),
                max_times: None,
            }),
            "out across the automation, to the step the next placement opens first",
        );
        assert_eq!(across[0].returns_to, Some(25));
        assert_eq!(exits(84)[0].then, None, "an ended run is not given today's lines");
        let marked: Vec<i64> = {
            let mut stmt = conn.prepare("SELECT id FROM automation_run_def WHERE entry = 1 ORDER BY id").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
        };
        assert_eq!(marked, vec![81], "the copy the running run starts at, and no other");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v65 in full: every wire keys its two ports, a run's copies hold each port's id beside its name —
    /// below zero where the port is gone — an input's sources key the output they come from, and a
    /// value keys the port it went through, taking the way out that port hangs on where it had none yet
    /// (`AMB-D-961`). A wire on a name no port carries any more goes.
    #[test]
    fn the_chain_keys_every_port_a_wire_a_copy_or_a_value_named() {
        let dir = scratch("key-the-ports");
        let engine = store_at(&dir, 64);
        engine
            .conn()
            .execute_batch(
                r#"INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO automation (id, project_id, name) VALUES (1, 1, 'A');
                 INSERT INTO automation_action (id, project_id, name) VALUES (7, 1, '書く'), (8, 1, '読む');
                 INSERT INTO automation_action_step (id, action_id, name) VALUES (11, 7, '書く'), (12, 8, '読む');
                 INSERT INTO automation_placement (id, automation_id, action_id) VALUES (3, 1, 7), (4, 1, 8);
                 INSERT INTO automation_exit (id, owner_kind, owner_id, name) VALUES
                     (21, 'step', 11, NULL), (25, 'action', 7, NULL);
                 INSERT INTO automation_edge (id, owner_kind, owner_id, from_id, exit_id, ends, exit_to_id) VALUES
                     (61, 'action', 7, 11, 21, 'exit', 25);
                 INSERT INTO automation_port (id, owner_kind, owner_id, direction, name, kind) VALUES
                     (31, 'exit', 21, 'out', 'メモ', 'value'), (32, 'exit', 25, 'out', 'メモ', 'value'),
                     (33, 'action', 8, 'in', 'メモ', 'value'), (34, 'step', 12, 'in', 'メモ', 'value');
                 INSERT INTO automation_wire (id, owner_kind, owner_id, from_id, from_exit_id, from_port_name, to_id, to_port_name) VALUES
                     (71, 'action', 7, 11, 21, 'メモ', 0, 'メモ'),
                     (72, 'automation', 1, 3, 25, 'メモ', 4, 'メモ'),
                     (73, 'action', 8, 0, NULL, 'メモ', 12, 'メモ'),
                     (74, 'action', 8, 0, NULL, '消えた', 12, 'メモ');
                 INSERT INTO automation_run (id, automation_id, project_id, status) VALUES (1, 1, 1, 'running');
                 INSERT INTO automation_run_def (id, run_id, placement_id, step_id, name, agent, exits, ins, cfg) VALUES
                     (81, 1, 3, 11, '書く', 'claude',
                      '[{"id":21,"name":null,"outs":[{"name":"メモ","kind":"value","required":true}]}]', '[]', '[]'),
                     (82, 1, 4, 12, '読む', 'claude', '[]',
                      '[{"name":"メモ","kind":"value","required":false,"from":[{"placement_id":3,"step_id":11,"exit_id":21,"port":"メモ"}]}]', '[]'),
                     (83, 1, NULL, NULL, '消えた', 'claude',
                      '[{"id":-1,"name":null,"outs":[{"name":"古い","kind":"value","required":false}]}]', '[]', '[]');
                 INSERT INTO automation_run_step (id, run_id, run_def_id, seq, exit_id, status) VALUES
                     (91, 1, 81, 1, 21, 'done'), (92, 1, 83, 2, NULL, 'running');
                 INSERT INTO automation_run_value (id, run_step_id, direction, exit_id, name, kind) VALUES
                     (101, 91, 'out', 21, 'メモ', 'value'), (102, 92, 'out', NULL, '古い', 'value');"#,
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        let conn = engine.conn();
        let wires: Vec<(i64, i64, i64)> = {
            let mut stmt =
                conn.prepare("SELECT id, from_port_id, to_port_id FROM automation_wire ORDER BY id").unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(|r| r.unwrap()).collect()
        };
        assert_eq!(
            wires,
            vec![(71, 31, 32), (72, 32, 33), (73, 33, 34)],
            "into the action's way out, across the picture, out of the action — and the parted one went",
        );
        let copy = |id: i64, column: &str| -> String {
            conn.query_row(&format!("SELECT {column} FROM automation_run_def WHERE id = ?1"), [id], |r| r.get(0))
                .unwrap()
        };
        let exits: Vec<crate::model::RunDefExit> = serde_json::from_str(&copy(81, "exits")).unwrap();
        assert_eq!(exits[0].outs[0].id, 31, "from the live row");
        let ins: Vec<crate::model::RunDefIn> = serde_json::from_str(&copy(82, "ins")).unwrap();
        assert_eq!(ins[0].port.id, 34);
        assert_eq!(
            ins[0].from,
            vec![crate::model::RunDefSource { placement_id: 3, step_id: 11, exit_id: Some(21), port_id: 31 }],
        );
        let gone: Vec<crate::model::RunDefExit> = serde_json::from_str(&copy(83, "exits")).unwrap();
        assert!(gone[0].outs[0].id < 0, "the step is gone, so below zero");
        let values: Vec<(i64, i64, Option<i64>)> = {
            let mut stmt =
                conn.prepare("SELECT id, port_id, exit_id FROM automation_run_value ORDER BY id").unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(|r| r.unwrap()).collect()
        };
        assert_eq!(
            values,
            vec![(101, 31, Some(21)), (102, gone[0].outs[0].id, Some(-1))],
            "a value on a step still running takes the way out its port hangs on",
        );
        for (table, column) in [
            ("automation_wire", "from_port_name"),
            ("automation_wire", "to_port_name"),
            ("automation_run_value", "name"),
        ] {
            let held: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
                    [table, column],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(held, 0, "{table}.{column} is gone");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v64 in full: what each step said is written onto every placement of its action, a step that said
    /// nothing is left with nobody chosen, an action placed nowhere carries its answer nowhere, and the
    /// two columns are gone from the step (`AMB-D-960`).
    #[test]
    fn a_step_s_agent_is_chosen_at_every_placement_of_its_action() {
        let dir = scratch("placement-step");
        let engine = store_at(&dir, 63);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO automation (id, project_id, name) VALUES (1, 1, 'A');
                 INSERT INTO automation_action (id, project_id, name) VALUES (1, 1, '書く'), (2, 1, '誰も置かない');
                 INSERT INTO automation_action_step (id, action_id, name, agent, model) VALUES
                     (1, 1, '書く', 'claude', 'opus'),
                     (2, 1, '見直す', 'codex', NULL),
                     (3, 1, 'まだ', '', NULL),
                     (4, 2, '置かれない', 'claude', NULL);
                 INSERT INTO automation_placement (id, automation_id, action_id) VALUES (1, 1, 1), (2, 1, 1);",
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        let chosen: Vec<(i64, i64, String, Option<String>)> = {
            let conn = engine.conn();
            let mut stmt = conn
                .prepare(
                    "SELECT placement_id, step_id, agent, model FROM automation_placement_step \
                     ORDER BY placement_id, step_id",
                )
                .unwrap();
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };
        assert_eq!(
            chosen,
            vec![
                (1, 1, "claude".to_string(), Some("opus".to_string())),
                (1, 2, "codex".to_string(), None),
                (2, 1, "claude".to_string(), Some("opus".to_string())),
                (2, 2, "codex".to_string(), None),
            ],
            "both placements carry what each step said, and the step that said nothing has nobody"
        );
        let left: i64 = engine
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('automation_action_step') \
                 WHERE name IN ('agent', 'model')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(left, 0, "the step names no agent and no model any more");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v63 in full: a copy of a step in a run that can still open one carries the wires joined to each
    /// input — inside the action, and out across the automation to the step behind the far placement's
    /// way out — while a copy in an ended run, and an input a launch already resolved, are left.
    #[test]
    fn the_chain_copies_the_wires_into_a_run_that_can_still_open_a_step() {
        let dir = scratch("copy-the-wires");
        let engine = store_at(&dir, 62);
        engine
            .conn()
            .execute_batch(
                r#"INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO automation (id, project_id, name) VALUES (1, 1, 'A');
                 INSERT INTO automation_action (id, project_id, name) VALUES (7, 1, '書く'), (8, 1, '直す');
                 INSERT INTO automation_action_step (id, action_id, name, agent) VALUES
                     (11, 7, '書く', 'claude'), (12, 8, '読む', 'claude'), (13, 8, '直す', 'claude');
                 INSERT INTO automation_placement (id, automation_id, action_id) VALUES (3, 1, 7), (4, 1, 8);
                 INSERT INTO automation_exit (id, owner_kind, owner_id, name) VALUES
                     (21, 'step', 11, NULL), (25, 'action', 7, NULL), (22, 'step', 12, NULL);
                 INSERT INTO automation_edge (id, owner_kind, owner_id, from_id, exit_id, ends, exit_to_id) VALUES
                     (61, 'action', 7, 11, 21, 'exit', 25);
                 INSERT INTO automation_port (id, owner_kind, owner_id, direction, name, kind) VALUES
                     (31, 'exit', 21, 'out', 'メモ', 'value'), (32, 'exit', 25, 'out', 'メモ', 'value'),
                     (33, 'action', 8, 'in', '下書き', 'value'), (34, 'step', 13, 'in', '下書き', 'value'),
                     (35, 'exit', 22, 'out', '所見', 'value'), (36, 'step', 13, 'in', '所見', 'value'),
                     (37, 'step', 13, 'in', '済み', 'value');
                 INSERT INTO automation_wire (id, owner_kind, owner_id, from_id, from_exit_id, from_port_name, to_id, to_port_name) VALUES
                     (71, 'action', 7, 11, 21, 'メモ', 0, 'メモ'),
                     (72, 'automation', 1, 3, 25, 'メモ', 4, '下書き'),
                     (73, 'action', 8, 0, NULL, '下書き', 13, '下書き'),
                     (74, 'action', 8, 12, 22, '所見', 13, '所見');
                 INSERT INTO automation_run (id, automation_id, project_id, status) VALUES
                     (1, 1, 1, 'running'), (2, 1, 1, 'completed');
                 INSERT INTO automation_run_def (id, run_id, placement_id, step_id, name, agent, exits, ins, cfg) VALUES
                     (81, 1, 4, 13, '直す', 'claude', '[]',
                      '[{"name":"下書き","kind":"value","required":true},{"name":"所見","kind":"value","required":false},{"name":"済み","kind":"value","required":false,"from":[]}]', '[]'),
                     (82, 2, 4, 13, '直す', 'claude', '[]',
                      '[{"name":"下書き","kind":"value","required":true}]', '[]');"#,
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        let ins = |id: i64| -> Vec<crate::model::RunDefIn> {
            let json: String = engine
                .conn()
                .query_row("SELECT ins FROM automation_run_def WHERE id = ?1", [id], |r| r.get(0))
                .unwrap();
            serde_json::from_str(&json).expect("a copy reads back as today's shape")
        };
        // Keyed by the port rows from v65 on, which is the shape a copy reads back as today.
        let source = |placement_id, step_id, exit_id, port_id| crate::model::RunDefSource {
            placement_id,
            step_id,
            exit_id,
            port_id,
        };
        let running = ins(81);
        assert_eq!(
            running[0].from,
            vec![source(3, 11, Some(21), 31)],
            "across the automation, to the step behind the way out the far action leaves by",
        );
        assert_eq!(running[1].from, vec![source(4, 12, Some(22), 35)], "inside the action");
        assert!(running[2].from.is_empty(), "an input a launch resolved is left as it was");
        assert!(ins(82)[0].from.is_empty(), "an ended run is not given today's wires");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v61 in full: the column arrives empty on every run, a failure can then be marked as seen, and
    /// the mark is held to the shape every timestamp column has.
    #[test]
    fn a_failed_run_can_be_marked_as_seen_and_none_arrives_marked() {
        let dir = scratch("run-acknowledged");
        let engine = store_at(&dir, 60);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO automation (id, project_id, name) VALUES (1, 1, 'A');
                 INSERT INTO automation_run (id, automation_id, project_id, status, stopped_reason) VALUES
                     (1, 1, 1, 'failed', 'crashed');",
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        let seen: Option<String> = engine
            .conn()
            .query_row("SELECT acknowledged_at FROM automation_run WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(seen, None, "no failure arrives already seen");
        engine
            .conn()
            .execute("UPDATE automation_run SET acknowledged_at = '2026-09-23T00:00:00Z' WHERE id = 1", [])
            .expect("a time goes in");
        assert!(
            engine
                .conn()
                .execute("UPDATE automation_run SET acknowledged_at = 'yesterday' WHERE id = 1", [])
                .is_err(),
            "and anything else is refused"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v60 in full: every ending lands on the word for what happened (`AMB-D-955`). A finished run is
    /// `completed`, one a person stopped is `canceled` with no reason, and every other stop is `failed`
    /// with the reason it had — or with none, where the row never said which of three it was. What is
    /// still going is untouched, and the retired words are refused on the way in.
    #[test]
    fn a_run_s_ending_lands_on_the_word_for_what_happened() {
        let dir = scratch("run-endings");
        let engine = store_at(&dir, 59);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO automation (id, project_id, name) VALUES (1, 1, 'A');
                 INSERT INTO automation_run (id, automation_id, project_id, status, stopped_reason) VALUES
                     (1, 1, 1, 'done',    NULL),
                     (2, 1, 1, 'stopped', 'by_human'),
                     (3, 1, 1, 'stopped', 'crashed'),
                     (4, 1, 1, 'stopped', NULL),
                     (5, 1, 1, 'running', NULL),
                     (6, 1, 1, 'paused',  NULL);",
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        let after: Vec<(String, Option<String>)> = {
            let conn = engine.conn();
            let mut stmt =
                conn.prepare("SELECT status, stopped_reason FROM automation_run ORDER BY id").unwrap();
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };
        let said = |status: &str, reason: Option<&str>| (status.to_string(), reason.map(str::to_string));
        assert_eq!(
            after,
            vec![
                said("completed", None),
                said("canceled", None),
                said("failed", Some("crashed")),
                said("failed", None),
                said("running", None),
                said("paused", None),
            ],
        );

        let declared = declared_sql(&engine, "automation_run");
        assert!(
            declared.contains("CHECK(status IN ('', 'running', 'paused', 'completed', 'failed', 'canceled'))"),
            "the status set is the new one: {declared}"
        );
        for retired in [
            "UPDATE automation_run SET status = 'stopped' WHERE id = 5",
            "UPDATE automation_run SET status = 'done' WHERE id = 5",
            "UPDATE automation_run SET stopped_reason = 'by_human' WHERE id = 3",
        ] {
            assert!(engine.conn().execute(retired, []).is_err(), "refused on the way in: {retired}");
        }
        engine
            .conn()
            .execute("UPDATE automation_run SET stopped_reason = 'halted' WHERE id = 4", [])
            .expect("the reason a halt now writes goes in");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v52 in full: the reason set admits `no_way_on`, and the rows are left alone — no store can be
    /// carrying a value its column never accepted (`AMB-T-5296` wrote the variant and not the
    /// `CHECK`, so the write was refused wherever it was tried).
    #[test]
    fn the_run_with_nowhere_to_go_gets_a_reason_the_column_accepts() {
        let dir = scratch("run-no-way-on");
        let engine = store_at(&dir, 51);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO automation (id, project_id, name) VALUES (1, 1, 'A');
                 INSERT INTO automation_run (id, automation_id, project_id, status, stopped_reason) VALUES
                     (1, 1, 1, 'stopped', 'by_human');",
            )
            .unwrap();

        // Stop at v52: v60 renames both sets, and the whole chain would be proving that step.
        run(&engine, &dir, steps_through(52), &mut crate::progress::ignore).unwrap();

        assert_eq!(engine.format_version().unwrap(), 52);
        let declared = declared_sql(&engine, "automation_run");
        assert!(
            declared.contains(
                "CHECK(stopped_reason IN ('crashed', 'max_times', 'no_agent', 'by_human', 'no_way_on'))"
            ),
            "the set is the new one: {declared}"
        );
        engine
            .conn()
            .execute("UPDATE automation_run SET stopped_reason = 'no_way_on' WHERE id = 1", [])
            .expect("the ending the watch writes goes in");
        assert!(
            engine
                .conn()
                .execute("UPDATE automation_run SET stopped_reason = 'gave_up' WHERE id = 1", [])
                .is_err(),
            "and the set is still closed",
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v51 in full: the closed set loses `queued`, and the runs that were sitting in it are stopped
    /// as crashes (`AMB-D-947`). That is the answer the startup sweep already gave them, and it is
    /// the only honest one left — nothing promotes a run any more, so a row left `queued` would hold
    /// its automation undeletable for as long as the store lived. Every other state is untouched,
    /// and the empty status a row caught mid-create carries is left where it is.
    #[test]
    fn the_runs_that_were_waiting_for_a_lane_are_stopped_and_the_value_goes() {
        let dir = scratch("run-queue-away");
        let engine = store_at(&dir, 50);
        engine
            .conn()
            .execute_batch(
                "INSERT INTO project (id, name) VALUES (1, 'A');
                 INSERT INTO automation (id, project_id, name) VALUES (1, 1, 'A');
                 INSERT INTO automation_run (id, automation_id, project_id, status, created_at, updated_at) VALUES
                     (1, 1, 1, 'queued',  '2025-12-01T00:00:00Z', '2025-12-01T00:00:00Z'),
                     (2, 1, 1, 'running', '2025-12-01T00:00:00Z', '2025-12-01T00:00:00Z'),
                     (3, 1, 1, 'done',    '2025-12-01T00:00:00Z', '2025-12-01T00:00:00Z'),
                     (4, 1, 1, '',        '',                     '');",
            )
            .unwrap();

        // Stop at v51: v60 renames the endings, and the whole chain would be proving that step.
        run(&engine, &dir, steps_through(51), &mut crate::progress::ignore).unwrap();

        assert_eq!(engine.format_version().unwrap(), 51);
        let after: Vec<(i64, String, Option<String>, Option<String>)> = {
            let conn = engine.conn();
            let mut stmt = conn
                .prepare("SELECT id, status, stopped_reason, ended_at FROM automation_run ORDER BY id")
                .unwrap();
            let rows =
                stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };
        assert_eq!(after[0].1, "stopped", "the one that was waiting");
        assert_eq!(after[0].2.as_deref(), Some("crashed"));
        assert!(after[0].3.is_some(), "and it is over, so it has an end");
        assert_eq!(
            after[1..].iter().map(|r| r.1.as_str()).collect::<Vec<_>>(),
            vec!["running", "done", ""],
            "nothing else is touched",
        );

        let declared = declared_sql(&engine, "automation_run");
        assert!(
            declared.contains("CHECK(status IN ('', 'running', 'paused', 'done', 'stopped'))"),
            "the closed set is the new one: {declared}"
        );
        assert!(
            engine
                .conn()
                .execute("UPDATE automation_run SET status = 'queued' WHERE id = 2", [])
                .is_err(),
            "and the retired value is refused on the way in, not merely absent"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// v49 in full: the split each project was held at lands on every one of that project's panes as
    /// a size, and the three fields that said it go with the write (`AMB-D-939`). A project nobody
    /// answered for leaves its panes at the whole page, and a count this build has no size for is let
    /// go rather than rounded to one.
    #[test]
    fn the_split_a_project_was_held_at_lands_on_each_of_its_panes() {
        let dir = scratch("pane-sizes");
        let engine = store_at(&dir, 48);
        engine
            .set_meta(
                "talk.layout",
                Some(
                    r#"{"project":1,"splits":{"1":{"count":2,"orient":"down"},"2":{"count":6},
                        "4":{"count":5}},
                        "panes":[{"id":"a","project":1},{"id":"b","project":1},
                                 {"id":"c","project":2},{"id":"d","project":3},
                                 {"id":"e","project":4}]}"#,
                ),
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        let kept = engine.get_meta("talk.layout").unwrap().expect("the arrangement");
        let row: serde_json::Value = serde_json::from_str(&kept).unwrap();
        let size = |at: usize| row["panes"][at]["size"].as_str().unwrap().to_string();
        assert_eq!(size(0), "half-down", "two panes laid down the page is half of it, that way round");
        assert_eq!(size(1), "half-down", "and so is the pane beside it — one answer, two panes");
        assert_eq!(size(2), "sixth");
        assert_eq!(size(3), "whole", "a project nobody answered for is one pane to a page");
        assert_eq!(size(4), "whole", "and so is a count this build has no size for");
        assert!(!kept.contains("splits"), "the answers are on the panes now: {kept}");
        assert!(!kept.contains("count"), "{kept}");
        assert!(!kept.contains("orient"), "{kept}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The one split an older build wrote for the whole face is folded in under the project the face
    /// was on — the fold `crate::frames` did on every read until this step took it over. A row naming
    /// no project has nowhere to put it, and lets it go.
    #[test]
    fn the_one_split_an_older_build_wrote_lands_under_the_project_it_was_set_on() {
        let dir = scratch("pane-sizes-older");
        let engine = store_at(&dir, 48);
        engine
            .set_meta(
                "talk.layout",
                Some(r#"{"count":4,"project":2,"panes":[{"id":"a","project":2}]}"#),
            )
            .unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        let kept = engine.get_meta("talk.layout").unwrap().expect("the arrangement");
        let row: serde_json::Value = serde_json::from_str(&kept).unwrap();
        assert_eq!(row["panes"][0]["size"].as_str(), Some("quarter"));
        assert!(!kept.contains("count"), "{kept}");
    }

    /// A row that names no project has nowhere to put its one split: a count with nothing to hold it
    /// against is not an answer about anything, so the pane comes out at the whole page.
    #[test]
    fn a_split_with_no_project_to_hold_it_is_let_go() {
        let dir = scratch("pane-sizes-no-project");
        let engine = store_at(&dir, 48);
        engine.set_meta("talk.layout", Some(r#"{"count":4,"panes":[{"id":"a","project":2}]}"#)).unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        let kept = engine.get_meta("talk.layout").unwrap().expect("the arrangement");
        let row: serde_json::Value = serde_json::from_str(&kept).unwrap();
        assert_eq!(row["panes"][0]["size"].as_str(), Some("whole"));
    }
}
