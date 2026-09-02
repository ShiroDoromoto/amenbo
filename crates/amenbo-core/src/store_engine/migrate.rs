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
];

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

/// v11: give the outbox the project column the fan-out routes on (`AMB-D-405`).
///
/// **Why this is not one `ALTER TABLE`.** The outbox itself arrived after the baseline, so a store older
/// than the table is handed it whole by genesis — built from today's registry, `project` included — and a
/// bare `ALTER TABLE … ADD COLUMN` would then fail with `duplicate column name` on exactly the oldest
/// stores. The probe is what makes one step serve both shapes: the store that has the column was born with
/// it, and the store that does not is the one this step exists for. Same shape as v6's, for the same
/// reason.
fn add_outbox_project(ctx: &Ctx<'_>) -> Result<()> {
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
            for table in ["plugin_config", "plugin_enable"] {
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

    /// **v24 on every shape the chain starts from** (`AMB-D-601`). A store born at any frozen version comes
    /// out declaring `project_id` the way today's registry emits it — admitting NULL, and still cascading,
    /// since opening the key is not a change of what happens when a project goes.
    ///
    /// The rows are checked too: a store carrying a project's settings keeps them, untouched and still
    /// pointing at their project. This step rewrites a declaration and nothing else, and a rewrite that
    /// reached the rows would be a corrupted store rather than a migrated one.
    #[test]
    fn the_chain_opens_the_plugin_layer_key_and_leaves_the_rows_alone() {
        for born in OLDEST_FROZEN_VERSION..=LATEST_VERSION {
            let dir = scratch(&format!("layer-v{born}"));
            let engine = store_at(&dir, born);
            engine
                .conn()
                .execute_batch(
                    "INSERT INTO project (id, name) VALUES (1, 'p');
                     INSERT INTO plugin_enable (project_id, plugin) VALUES (1, 'slack');
                     INSERT INTO plugin_config (project_id, plugin, field_key, value)
                       VALUES (1, 'slack', 'channel', '#ops');
                     INSERT INTO plugin_secret (project_id, plugin, field_key, value)
                       VALUES (1, 'slack', 'token', 's3cret');",
                )
                .unwrap();

            run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

            for table in LAYERED_TABLES {
                let sql = declared_sql(&engine, table);
                assert!(
                    sql.contains(PROJECT_KEY_OPTIONAL),
                    "a store born at v{born} leaves `{table}` unable to hold a device row:\n{sql}"
                );
                assert!(
                    sql.contains(REFERENCE_CASCADES),
                    "a store born at v{born} stopped cascading `{table}`, which is Amenbo's own setting"
                );
                let held: i64 = engine
                    .conn()
                    .query_row(&format!("SELECT COUNT(*) FROM {table} WHERE project_id = 1"), [], |r| {
                        r.get(0)
                    })
                    .unwrap();
                assert_eq!(held, 1, "a store born at v{born} lost `{table}`'s existing row");
            }
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
            let mut stmt = conn.prepare("SELECT id, event, project FROM plugin_outbox ORDER BY id").unwrap();
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
                "INSERT INTO plugin_outbox (event, record_id, actor, at, project)
                     VALUES ('task.created', 9, 'ai', '2026-07-26T09:00:02Z', 3)",
                [],
            )
            .expect("what is emitted from here on can carry its project");
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
                "INSERT INTO plugin_queue (id, plugin, face, event, record_id, actor, at, new_state)
                     VALUES (1, 'slack', 'cli', 'task.deleted', 7, 'ai', '2026-07-26T09:00:00Z', NULL);",
            )
            .unwrap();

        let run = run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert!(run.applied.iter().any(|s| s.contains("plugin_queue.project")), "v12 ran: {:?}", run.applied);
        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);
        let row: (String, String, Option<i64>) = engine
            .conn()
            .query_row("SELECT plugin, event, project FROM plugin_queue", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(
            row,
            ("slack".to_string(), "task.deleted".to_string(), None),
            "the row that was already queued keeps its fields and gains an unstamped project",
        );
        engine
            .conn()
            .execute(
                "INSERT INTO plugin_queue (plugin, face, event, record_id, actor, at, project)
                     VALUES ('slack', 'cli', 'task.created', 9, 'ai', '2026-07-26T09:00:02Z', 3)",
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
                "INSERT INTO project (id, name) VALUES (1, 'alpha'), (2, 'beta');
                 INSERT INTO plugin_config (id, project_id, plugin, field_key, value, created_at, updated_at)
                     VALUES (1, 2, 'slack', 'events', 'answered-for-itself',
                             '2026-07-26T09:00:00Z', '2026-07-26T09:00:00Z');",
            )
            .unwrap();
        std::fs::write(
            dir.join("config.json"),
            br#"{"language":"ja","plugin_config":{"slack":{"events":"push"}}}"#,
        )
        .unwrap();
        std::fs::write(dir.join("plugin-secrets.json"), br#"{"slack":{"token":"s3cret"}}"#).unwrap();

        let run = run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        assert!(run.applied.iter().any(|s| s.contains("into each project's rows")), "{:?}", run.applied);
        assert_eq!(engine.format_version().unwrap(), LATEST_VERSION);

        let value = |table: &str, project: i64, key: &str| -> Option<String> {
            engine
                .conn()
                .query_row(
                    &format!(
                        "SELECT value FROM {table} WHERE project_id = ?1 AND plugin = 'slack' AND field_key = ?2"
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
    /// fired for — and the migration is still the end of the user-area homes.
    #[test]
    fn a_store_with_no_project_still_loses_the_user_area_homes() {
        let dir = scratch("plugin-settings-no-project");
        let engine = store_at(&dir, 14);
        std::fs::write(dir.join("config.json"), br#"{"plugin_config":{"slack":{"events":"push"}}}"#).unwrap();
        std::fs::write(dir.join("plugin-secrets.json"), br#"{"slack":{"token":"s3cret"}}"#).unwrap();

        run(&engine, &dir, STEPS, &mut crate::progress::ignore).unwrap();

        let rows: i64 = engine
            .conn()
            .query_row(
                "SELECT (SELECT COUNT(*) FROM plugin_config) + (SELECT COUNT(*) FROM plugin_secret)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rows, 0);
        assert!(!dir.join("plugin-secrets.json").exists());
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
}
