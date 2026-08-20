//! Sync snapshot — the one instant of a project a carrier plugin takes off this device.
//!
//! A plugin that carries Amenbo's data outward (a viewer, an audit trail, a mirror in another tool)
//! resends **everything** each time (`AMB-D-583`), so what it needs is not a diff but one whole,
//! internally consistent picture of what it may see. That is this module, and it is a road of its own
//! rather than a flag on an existing one (`AMB-D-581`): `export` is the whole device on its way to
//! another tool (`AMB-D-180`) and `backup` is the road back to this same store, so bending either to a
//! window would break what it is for.
//!
//! Three things make the picture safe to hand out, and each is enforced here rather than trusted to the
//! caller:
//!
//! - **It closes on the window.** A plugin observes one project (`AMB-D-434`), so the snapshot carries
//!   that project and nothing else — every table narrowed by [`project_predicate`], and a join row only
//!   when **both** of its ends are inside. A consumer therefore never holds a reference it cannot
//!   resolve, and never learns that another project exists. A reach that is open ([`Reach::All`] — a
//!   human, the GUI) narrows nothing and takes the device.
//! - **It withholds the plugin secrets.** The same line `export` draws, drawn by the same list
//!   ([`crate::export::WITHHELD_ON_THE_WAY_OUT`]) rather than a second one beside it.
//! - **It is one instant.** Every table is read inside a single read transaction, so a write landing
//!   mid-stream cannot put a comment in the snapshot whose task is not — the tearing that nothing in the
//!   artifact would confess to. Same handling as the export's (`AMB-T-2790`).
//! - **It says which instant.** The header names the change-feed position the picture stands at
//!   ([`ledger_position`]), read from that same transaction, so the carrier reads the feed on from there
//!   and the whole and the stream are continuous (`AMB-D-582`). A snapshot that did not say would leave
//!   its reader guessing between losing the writes that landed while it read, and replaying what it
//!   already holds.
//!
//! **What it returns is JSON, records only.** The shape is the export's plain `{table: [rows]}` — a
//! consumer that can read one can read the other, and there is nothing about a snapshot that wants a
//! shape of its own. Attachment **rows** travel; attachment **bytes** do not. Folding the bytes in would
//! mean base64 in the JSON, which `AMB-D-178` already rejected for the export, and the cost lands on
//! every resend rather than once. So the snapshot carries what the metadata says — hash, filename, mime,
//! size — and a carrier that wants the bytes asks for them by their own road.
//!
//! It is plaintext, and deliberately so: this store is plaintext at rest, and encrypting what is carried
//! is the carrier's job, not Amenbo's (`AMB-D-585`). Nothing reads a snapshot back in — the road out is
//! one-way (`AMB-D-578`).
//!
//! **Beside the whole picture, the same rows by id** ([`records_from`]). A carrier reading the ledger is
//! told which records moved and never what they now hold (`AMB-D-582`), so it has to read those rows back
//! — and taking the whole window to see one changed task is the ledger's whole point undone. That read is
//! here rather than in a module of its own because what makes it safe is what makes the snapshot safe:
//! the same [`project_predicate`], so a row outside the window falls out of both alike, and the same
//! `{table: [rows]}` shape, so a carrier holds one form of Amenbo's data and not two.

use std::io::Write;
use std::path::Path;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::export::{self, Scope};
use crate::reach::Reach;
use crate::store_engine::schema::Dataset;
use crate::time::Timestamp;

/// Layout version of the snapshot envelope — a label for the carrier reading it, bumped only when the
/// envelope changes shape. Distinct from the read-model `schema_version` inside it, and from the store
/// version a carrier polls to decide whether to send at all.
const SNAPSHOT_VERSION: u32 = 1;

/// Format tag written into the header so a carrier can recognise what it is holding at a glance — and
/// tell it apart from an export, which is the same shape and a different promise.
const SNAPSHOT_FORMAT: &str = "amenbo-sync-json";

/// Layout version of the by-id read's envelope, kept apart from [`SNAPSHOT_VERSION`] because the two
/// documents are versioned by what they promise, not by when they were written.
const RECORDS_VERSION: u32 = 1;

/// Format tag of the by-id read ([`stream_records`]). The document is the snapshot's shape with one table
/// in it, so the tag is what says which of the two a reader is holding — the difference being the promise,
/// not the layout: a snapshot is the whole window at one instant, this is the rows that were asked for.
const RECORDS_FORMAT: &str = "amenbo-sync-records";

/// Header object of a document off this road (`amenbo_sync` key): what produced it, when, and how far it
/// reaches. One header for both roads — the snapshot and the by-id read — so a carrier parses one thing
/// and reads `format` to know which it has.
#[derive(Debug, Clone, Serialize)]
struct SnapshotHeader {
    /// Format tag ([`SNAPSHOT_FORMAT`] / [`RECORDS_FORMAT`]).
    format: &'static str,
    /// Envelope layout version ([`SNAPSHOT_VERSION`] / [`RECORDS_VERSION`]).
    format_version: u32,
    /// Read-model schema version of the producing binary ([`crate::model::SCHEMA_VERSION`]).
    schema_version: &'static str,
    /// Producing binary's human-readable version.
    app_version: &'static str,
    /// When the document was produced (RFC3339).
    taken_at: String,
    /// The project this document is closed to, or `null` when it carries the whole device. Stated
    /// because a carrier holding two snapshots has no other way to tell what each one was allowed to
    /// see, and "everything I got" is not the same claim as "everything there is".
    project_id: Option<i64>,
    /// **Where in the ledger this snapshot stands** — the change-feed position to read on from, so the
    /// full picture and the stream of changes are one continuous thing rather than two that nearly meet
    /// ([`ledger_position`]).
    ///
    /// Absent on the by-id read, and deliberately: those rows are read at no defensible position — a row
    /// may have moved again between the feed naming it and this reading it, and a carrier that took this
    /// for a cursor would skip the changes in between. The position comes from the snapshot and from the
    /// feed, and from nowhere else.
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<i64>,
}

/// The change-feed position a snapshot names: **read the feed after this and you get exactly what
/// happened to this window since, with nothing missed and nothing replayed** (`AMB-D-582`). Without it a
/// carrier that has just taken the full picture has no defensible place to start reading, and picks
/// between losing the writes that landed while it read and re-applying the ones it already has.
///
/// It is the reach's own version ([`crate::store::Store::sync_version`] — this project's, or the feed's
/// head for the device), which is the last feed id that reached the window. Every window row at or below
/// it is *in* this snapshot, so an exclusive read (`> cursor`) replays none of them; every window change
/// after this instant is above it, so none is missed. Read here rather than through `Store` because it
/// has to come from the **same transaction as the tables** — asked afterwards on another connection it
/// would name an instant the document does not show, and the writes in between would fall down the seam.
///
/// **Floored at the feed's own floor for this reader**
/// ([`read::feed_floor`](crate::store_engine::read::feed_floor)), which is the case the reach's version
/// alone gets wrong: a window nothing has written for a long time keeps a version from below the floor,
/// and a cursor below it reads back as [`FeedSlice::Gap`](crate::store_engine::read::FeedSlice::Gap) —
/// telling a carrier that just took a complete picture that it has lost changes, and sending it round to
/// take another. Nothing is skipped by raising it: the rows between the two are, by definition, not this
/// window's, and they are past reading either way. It is the same floor
/// [`changes_since`](crate::store_engine::read::changes_since) tests a cursor against, asked here so the
/// position handed out and the position accepted cannot drift apart.
fn ledger_position(conn: &rusqlite::Connection, reach: Reach) -> Result<i64> {
    use crate::store_engine::read;
    let version = match reach.project() {
        Some(project_id) => read::project_version(conn, project_id),
        None => read::change_feed_head(conn),
    }
    .map_err(crate::error::engine_on(conn))?;
    let floor = read::feed_floor(conn, reach.project()).map_err(crate::error::engine_on(conn))?;
    Ok(version.max(floor))
}

/// How each dataset's table reaches the project a snapshot is closed to: the `WHERE` predicate that keeps
/// its rows inside the window, with `?1` bound to the project id.
///
/// **Raw SQL, for the same reason the export's `SELECT` is** — the walk is dataset-generic, so there is
/// no statically named table for the typed identifiers (`store_engine::sql`) to be built from. Nothing
/// here interpolates a value: the only value is the bound parameter.
///
/// Two rules decide every line:
///
/// 1. **A row reaches its project through whatever it hangs on.** A comment through its task, a
///    dimension value through its axis, an attachment through the polymorphic target it was attached to.
/// 2. **A join row travels only when every end of it is inside.** An edge between two real projects is
///    already refused at the door (`ops::guard_same_project`), but the **inbox** is not a project, so an
///    edge onto an unplaced task goes through — and an unplaced task is outside every closed reach. Such
///    a row carried in would be a reference the consumer cannot resolve; and were a crossing row ever to
///    reach the store another way (a restore, a hand-edit), the id in it would say that a project the
///    carrier may not see exists.
///
/// `None` means the dataset declares no way to reach a project. That is a gap, not permission to carry it
/// whole, and a scoped stream refuses it ([`Scope`]) — with
/// `tests::every_carried_dataset_declares_how_it_reaches_a_project` standing so the gap is found at
/// build time instead.
fn project_predicate(dataset: &Dataset) -> Option<&'static str> {
    Some(match dataset.name {
        // The one row the window is about.
        "project" => "id = ?1",
        // An unplaced (inbox) task carries no project, and so is outside every closed reach — the same
        // answer `Reach::allows(None)` gives.
        "task" => "project_id = ?1",
        "decision" => "project_id = ?1",
        "dimension" => "project_id = ?1",
        // A device-layer row's key is NULL (`AMB-D-601`), and `project_id = ?1` is never true of NULL — so
        // it stays home, which is the right answer twice over: it is no project's content, and a window
        // closed to one project is exactly the reader that must not learn what the whole device holds.
        "plugin_config" => "project_id = ?1",
        "plugin_enable" => "project_id = ?1",

        // Hangs on one parent.
        "task_comment" => "task_id IN (SELECT id FROM task WHERE project_id = ?1)",
        "decision_comment" => "decision_id IN (SELECT id FROM decision WHERE project_id = ?1)",
        "task_commit" => "task_id IN (SELECT id FROM task WHERE project_id = ?1)",
        "dimension_value" => "dimension_id IN (SELECT id FROM dimension WHERE project_id = ?1)",

        // Joins — every end inside, or the row stays home.
        "dependency" => concat!(
            "task_id IN (SELECT id FROM task WHERE project_id = ?1)",
            " AND blocked_by_id IN (SELECT id FROM task WHERE project_id = ?1)",
        ),
        "decision_edge" => concat!(
            "decision_id IN (SELECT id FROM decision WHERE project_id = ?1)",
            " AND target_decision_id IN (SELECT id FROM decision WHERE project_id = ?1)",
        ),
        "decision_task_link" => concat!(
            "decision_id IN (SELECT id FROM decision WHERE project_id = ?1)",
            " AND task_id IN (SELECT id FROM task WHERE project_id = ?1)",
        ),
        // The axis is named twice on purpose: the value's axis is checked as well as the task's, so a row
        // that somehow crossed projects is dropped rather than carried with a `value_id` pointing out.
        "task_dimension_value" => concat!(
            "task_id IN (SELECT id FROM task WHERE project_id = ?1)",
            " AND dimension_id IN (SELECT id FROM dimension WHERE project_id = ?1)",
            " AND value_id IN (SELECT id FROM dimension_value WHERE dimension_id IN",
            " (SELECT id FROM dimension WHERE project_id = ?1))",
        ),

        // Polymorphic: no constraint can branch on a sibling column, so the predicate does it by hand —
        // one arm per `target_type` the column admits, each reaching the project the way its own kind
        // does. A row naming a kind no arm covers is carried by none of them, which is the safe answer.
        "attachment" => concat!(
            "(target_type = 'task'",
            " AND target_id IN (SELECT id FROM task WHERE project_id = ?1))",
            " OR (target_type = 'decision'",
            " AND target_id IN (SELECT id FROM decision WHERE project_id = ?1))",
            " OR (target_type = 'task_comment' AND target_id IN (SELECT id FROM task_comment",
            " WHERE task_id IN (SELECT id FROM task WHERE project_id = ?1)))",
            " OR (target_type = 'decision_comment' AND target_id IN (SELECT id FROM decision_comment",
            " WHERE decision_id IN (SELECT id FROM decision WHERE project_id = ?1)))",
        ),

        _ => return None,
    })
}

/// How a reach narrows what a road out carries: the [`Scope`] every table is read through, or `None` for
/// an open reach (a human, the GUI), which takes the device.
///
/// The narrowing has to be **total** before a byte is written: a dataset in `datasets` that declares no
/// way to reach a project would stream whole, past the window. Failing here means the caller gets an
/// error instead of a half-document with one table too wide in it. Asked by both roads out of this module
/// so the by-id read is closed exactly as far as the snapshot is.
fn window_scope(reach: Reach, datasets: &[&'static Dataset]) -> Result<Option<Scope>> {
    let Some(project_id) = reach.project() else { return Ok(None) };
    if let Some(gap) = datasets.iter().find(|d| project_predicate(d).is_none()) {
        return Err(Error::invalid(format!(
            "cannot close this read to one project: the `{}` dataset does not declare how its table \
             reaches one",
            gap.name
        )));
    }
    Ok(Some(Scope { project_id, predicate: project_predicate }))
}

/// Stream a sync snapshot of the database at `db_path` to `w`, closed to `reach`. Kept free of
/// [`crate::config::Paths`] resolution — like [`crate::export::export_json_from`] — so it is testable
/// against a hand-built store; the OS-glue entry point is [`stream`].
///
/// The document is `{"amenbo_sync": <header>, "tables": {…}}`. The source is opened **read-only** (no
/// migration, no `Database` hydrate) and every table read from **one** transaction, so what lands is one
/// instant rather than several.
pub fn stream_from(db_path: &Path, reach: Reach, w: &mut impl Write) -> Result<()> {
    // The registry every road out walks: each dataset but the plugin secrets.
    let datasets = export::datasets_carried_out();
    let scope = window_scope(reach, &datasets)?;

    let conn = export::open_source(db_path)?;
    // **One snapshot for every table** — the whole point of the word. Each statement would otherwise
    // take its own, so a write committing between two tables lands in the later one and not the earlier,
    // and the carrier mirrors a state the store was never in. Deferred is the only shape a read-only
    // connection can take, and it is enough: the instant is fixed by the first read and held to the last.
    let snapshot = conn.unchecked_transaction().map_err(crate::error::sqlite_at(db_path))?;

    // **The first read, and so the read that fixes the instant** — which is exactly what the position has
    // to name. Taken after the tables it would be a promise about a moment already past.
    let cursor = ledger_position(&snapshot, reach)?;

    let header = SnapshotHeader {
        format: SNAPSHOT_FORMAT,
        format_version: SNAPSHOT_VERSION,
        schema_version: crate::model::SCHEMA_VERSION,
        app_version: export::APP_VERSION,
        taken_at: Timestamp::now().0.to_rfc3339(),
        project_id: reach.project(),
        cursor: Some(cursor),
    };

    w.write_all(b"{\"amenbo_sync\":")?;
    serde_json::to_writer(&mut *w, &header).map_err(Error::from)?;
    w.write_all(b",\"tables\":")?;
    // Records only, row at a time (O(1) memory) — no `bundle`, so no attachment bytes, and no progress
    // sink: a snapshot is machine-facing, with nobody watching a bar and nobody to press Cancel.
    export::stream_store_tables(
        &snapshot,
        &datasets,
        scope.as_ref(),
        &mut *w,
        None,
        &mut crate::progress::ignore,
    )?;
    w.write_all(b"}")?;
    Ok(())
}

/// Stream **this device's** sync snapshot to `w`, closed to `reach` — thin OS-layout glue over
/// [`stream_from`], the sibling of [`crate::export::export_json`]. Refuses when this device holds no
/// store yet: an empty document would read as a project that has nothing in it, and a carrier that
/// believes it would delete everything it mirrors.
pub fn stream(reach: Reach, w: &mut impl Write) -> Result<()> {
    let db_path = crate::config::resolve_store_file(&crate::config::Paths::user_base());
    if !db_path.is_file() {
        return Err(Error::invalid("nothing to snapshot: this device holds no store"));
    }
    stream_from(&db_path, reach, w)
}

/// The most record ids one by-id read takes. It is the page a carrier is handed by the feed
/// (`SYNC_CHANGES_PAGE`), because that is where the ids come from: a carrier that drained one page can
/// always ask for everything in it, and one that cannot is being asked to keep a queue Amenbo never
/// bounded.
///
/// **Over it is refused, not truncated.** A short page of changes is unambiguous — the cursor says where
/// it stopped — but the ids here are the caller's own list in the caller's own order, so a partial answer
/// would leave it comparing what it asked for against what came back to find out which half it holds, and
/// a caller that skipped that comparison would drop rows silently. Splitting the list is one line at the
/// caller; guessing which rows it got is not.
pub const RECORDS_PER_READ: usize = 500;

/// Read named rows of **one** dataset back, as the same shape a snapshot carries them in — the road that
/// makes the ledger usable (`AMB-D-582`). `sync changes` says *which* records moved and never what they
/// now hold, so without this a carrier holding a handful of ids has nowhere to take them but a fresh
/// snapshot of the whole window.
///
/// `dataset` is the stable key the feed names (`task`, `dependency`, …
/// [`FeedRow::dataset`](crate::store_engine::read::FeedRow::dataset)), so what a carrier read off the
/// ledger is what it passes here. A key no carried dataset answers to is refused rather than served
/// empty: an empty answer would read as "those rows are gone" and a carrier would delete what it holds.
/// The plugin secrets are refused by the same line, being on no road out at all
/// ([`export::WITHHELD_ON_THE_WAY_OUT`]).
///
/// **The window is the snapshot's, drawn by the snapshot's own [`project_predicate`].** An id outside it
/// comes back as nothing — the same answer a deleted id gets, and deliberately the same: telling the two
/// apart would say that a row the carrier may not see exists.
///
/// What it writes is `{"amenbo_sync": <header>, "tables": {"<table>": [rows]}}` — the snapshot's document
/// with one table in it, so a carrier that can read a snapshot can read this with the code it already has.
/// The header carries no `cursor`: these rows are read at no position (see [`SnapshotHeader::cursor`]).
pub fn records_from(
    db_path: &Path,
    reach: Reach,
    dataset: &str,
    ids: &[i64],
    w: &mut impl Write,
) -> Result<()> {
    let carried = export::datasets_carried_out();
    let Some(dataset) = carried.iter().find(|d| d.name == dataset).copied() else {
        let mut known: Vec<&str> = carried.iter().map(|d| d.name).collect();
        known.sort_unstable();
        return Err(Error::invalid(format!(
            "`{dataset}` is not a dataset this road carries — it reads back the records the ledger \
             names, which are: {}",
            known.join(", "),
        )));
    };
    if ids.is_empty() {
        return Err(Error::invalid("name the records to read back — this road answers ids, not tables"));
    }
    if ids.len() > RECORDS_PER_READ {
        return Err(Error::invalid(format!(
            "{} ids in one read is more than this road answers ({RECORDS_PER_READ}) — ask for them in \
             pages of that size",
            ids.len(),
        )));
    }
    // Everything that can be refused is refused **before the first byte**: what a caller got wrong is an
    // error it can act on, not a half-document on its stdout.
    let scope = window_scope(reach, &carried)?;

    let header = SnapshotHeader {
        format: RECORDS_FORMAT,
        format_version: RECORDS_VERSION,
        schema_version: crate::model::SCHEMA_VERSION,
        app_version: export::APP_VERSION,
        taken_at: Timestamp::now().0.to_rfc3339(),
        project_id: reach.project(),
        cursor: None,
    };

    let conn = export::open_source(db_path)?;
    w.write_all(b"{\"amenbo_sync\":")?;
    serde_json::to_writer(&mut *w, &header).map_err(Error::from)?;
    w.write_all(b",\"tables\":")?;
    // One statement, so one instant, and no transaction to hold it in.
    export::stream_picked_rows(&conn, dataset, scope.as_ref(), ids, &mut *w)?;
    w.write_all(b"}")?;
    Ok(())
}

/// Read named rows out of **this device's** store — thin OS-layout glue over [`records_from`], the
/// sibling of [`stream`], and refusing on the same terms: with no store here there is nothing to read
/// back, and answering with an empty table would say those records are gone.
pub fn stream_records(reach: Reach, dataset: &str, ids: &[i64], w: &mut impl Write) -> Result<()> {
    let db_path = crate::config::resolve_store_file(&crate::config::Paths::user_base());
    if !db_path.is_file() {
        return Err(Error::invalid("nothing to read back: this device holds no store"));
    }
    records_from(&db_path, reach, dataset, ids, w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Paths;
    use crate::model::{ActorKind, AttachmentTarget, View};
    use crate::store::Store;
    use crate::store_engine::schema::DATASETS;

    /// Every dataset a road out carries has to say how its table reaches a project — the registry cannot
    /// enforce it (a `&str` match is not exhaustive), so this stands in its place. A dataset added
    /// without a line in [`project_predicate`] fails here, at build time, rather than at the door of a
    /// carrier that asked for one project.
    #[test]
    fn every_carried_dataset_declares_how_it_reaches_a_project() {
        let undeclared: Vec<&str> = export::datasets_carried_out()
            .iter()
            .filter(|d| project_predicate(d).is_none())
            .map(|d| d.name)
            .collect();
        assert!(
            undeclared.is_empty(),
            "these datasets are carried out but declare no way to reach a project: {undeclared:?} — \
             add a line to project_predicate (carrying one whole would reach past the window)",
        );
    }

    /// The plugin secrets are withheld from the snapshot, and by the export's own list rather than a
    /// second one beside it (`AMB-D-434`, drawn through by `AMB-D-581`).
    #[test]
    fn a_snapshot_carries_no_plugin_secret() {
        assert!(export::WITHHELD_ON_THE_WAY_OUT.contains(&"plugin_secret"));
        assert!(!export::datasets_carried_out().iter().any(|d| d.name == "plugin_secret"));
        assert!(
            DATASETS.iter().any(|d| d.name == "plugin_secret"),
            "the dataset still exists — it is the road out that leaves it, not the schema",
        );
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        amenbo_scratch::scratch(&format!("sync-snapshot-{tag}"))
    }

    fn store_file(dir: &Path) -> std::path::PathBuf {
        dir.join(crate::config::STORE_FILE_NAME)
    }

    fn seed_project(s: &mut Store, name: &str) -> i64 {
        s.project_add(crate::ops::project::NewProject {
            name: name.into(),
            view: View::List,
            notes: String::new(),
            color: None,
        })
        .unwrap()
        .id
    }

    /// A task in `project_id`, with one comment on it. Returns the task id and the comment's.
    fn seed_task(s: &mut Store, project_id: Option<i64>, title: &str) -> (i64, i64) {
        let t = s
            .add_task(crate::ops::task::NewTask {
                title: title.into(),
                project_id,
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: None,
                at_binding_id: None,
            })
            .unwrap();
        let c = s.add_task_comment(t.id, ActorKind::Ai, &format!("on {title}")).unwrap();
        (t.id, c.id)
    }

    fn take(db_path: &Path, reach: Reach) -> serde_json::Value {
        let mut buf = Vec::new();
        stream_from(db_path, reach, &mut buf).unwrap();
        serde_json::from_slice(&buf).unwrap()
    }

    fn titles(doc: &serde_json::Value) -> Vec<String> {
        doc["tables"]["task"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["title"].as_str().unwrap().to_string())
            .collect()
    }

    /// The whole of what a window may see, and nothing beside it: the other project's rows, and the
    /// unplaced task that belongs to no project at all, stay home — as does the other project's own row,
    /// which would otherwise say that it exists.
    #[test]
    fn a_window_carries_its_own_project_and_nothing_else() {
        let dir = scratch("window");
        std::fs::create_dir_all(&dir).unwrap();

        let (mine, theirs) = {
            let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
            let mine = seed_project(&mut s, "mine");
            let theirs = seed_project(&mut s, "theirs");
            seed_task(&mut s, Some(mine), "in the window");
            seed_task(&mut s, Some(theirs), "out of the window");
            seed_task(&mut s, None, "unplaced");
            (mine, theirs)
        };

        let doc = take(&store_file(&dir), Reach::window(mine));
        assert_eq!(doc["amenbo_sync"]["format"], SNAPSHOT_FORMAT);
        assert_eq!(doc["amenbo_sync"]["format_version"], SNAPSHOT_VERSION);
        assert_eq!(doc["amenbo_sync"]["project_id"], mine);
        assert_eq!(titles(&doc), vec!["in the window"]);

        let projects = doc["tables"]["project"].as_array().unwrap();
        assert_eq!(projects.len(), 1, "the other project is not even named: {projects:?}");
        assert_eq!(projects[0]["id"], mine);

        // The comment travelled with its task, and only that one — a comment whose task is absent would
        // be a reference the carrier cannot resolve.
        let comments = doc["tables"]["task_comment"].as_array().unwrap();
        assert_eq!(comments.len(), 1, "one task in the window, one comment: {comments:?}");

        // The same store through the other window is the mirror image.
        assert_eq!(titles(&take(&store_file(&dir), Reach::window(theirs))), vec!["out of the window"]);
    }

    /// A binding reaches one project too, so a snapshot taken through one is closed exactly as far as a
    /// window's. (Where the two part company is the whole-device operations, which this is not one of.)
    /// An open reach — a human, the GUI — narrows nothing and takes the device.
    #[test]
    fn a_binding_is_closed_as_far_as_a_window_and_an_open_reach_takes_the_device() {
        let dir = scratch("reaches");
        std::fs::create_dir_all(&dir).unwrap();

        let mine = {
            let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
            let mine = seed_project(&mut s, "mine");
            let theirs = seed_project(&mut s, "theirs");
            seed_task(&mut s, Some(mine), "here");
            seed_task(&mut s, Some(theirs), "there");
            mine
        };

        assert_eq!(titles(&take(&store_file(&dir), Reach::binding(mine))), vec!["here"]);

        let all = take(&store_file(&dir), Reach::All);
        assert!(all["amenbo_sync"]["project_id"].is_null(), "it says it took the device");
        let mut took = titles(&all);
        took.sort();
        assert_eq!(took, vec!["here", "there"]);
    }

    /// A dependency on a task the window does not carry stays home. The reachable case is the **inbox**:
    /// an edge between two real projects is refused at the door (`ops::guard_same_project`), but an
    /// unplaced task belongs to no project, so an edge onto one goes through — and an unplaced task is
    /// outside every closed reach. Carried in, the edge would name a task that is not there.
    #[test]
    fn a_join_row_travels_only_when_both_of_its_ends_are_in_the_window() {
        let dir = scratch("join");
        std::fs::create_dir_all(&dir).unwrap();

        let mine = {
            let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
            let mine = seed_project(&mut s, "mine");
            let (inside, _) = seed_task(&mut s, Some(mine), "inside");
            let (also_inside, _) = seed_task(&mut s, Some(mine), "also inside");
            let (unplaced, _) = seed_task(&mut s, None, "unplaced");
            // One edge wholly inside the window, one reaching out of it.
            s.depend_task(inside, also_inside, Some(ActorKind::Human)).unwrap();
            s.depend_task(inside, unplaced, Some(ActorKind::Human)).unwrap();
            mine
        };

        let doc = take(&store_file(&dir), Reach::window(mine));
        let edges = doc["tables"]["task_dependency"].as_array().unwrap();
        assert_eq!(edges.len(), 1, "only the edge with both ends inside travelled: {edges:?}");

        // Every id the edge names is a task the snapshot actually carries.
        let ids: Vec<i64> = doc["tables"]["task"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_i64().unwrap())
            .collect();
        assert!(ids.contains(&edges[0]["task_id"].as_i64().unwrap()));
        assert!(ids.contains(&edges[0]["blocked_by_id"].as_i64().unwrap()));
    }

    /// An attachment reaches its project through whatever it hangs on — a comment included, which is two
    /// hops — and one hanging on another project's task does not travel.
    #[test]
    fn an_attachment_travels_with_the_thing_it_hangs_on() {
        let dir = scratch("attachment");
        std::fs::create_dir_all(&dir).unwrap();

        let mine = {
            let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
            let mine = seed_project(&mut s, "mine");
            let theirs = seed_project(&mut s, "theirs");
            let (inside, on_inside) = seed_task(&mut s, Some(mine), "inside");
            let (outside, _) = seed_task(&mut s, Some(theirs), "outside");

            s.attach_url(AttachmentTarget::Task, inside, "https://example.com/mine", None, ActorKind::Ai)
                .unwrap();
            s.attach_url(AttachmentTarget::Task, outside, "https://example.com/theirs", None, ActorKind::Ai)
                .unwrap();
            s.attach_url(
                AttachmentTarget::TaskComment,
                on_inside,
                "https://example.com/on-a-comment",
                None,
                ActorKind::Ai,
            )
            .unwrap();
            mine
        };

        let doc = take(&store_file(&dir), Reach::window(mine));
        let urls: Vec<&str> = doc["tables"]["attachment"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["url"].as_str().unwrap())
            .collect();
        assert_eq!(urls.len(), 2, "the task's and its comment's, not the other project's: {urls:?}");
        assert!(urls.contains(&"https://example.com/mine"));
        assert!(urls.contains(&"https://example.com/on-a-comment"));
    }

    /// A blob attachment's **row** travels; its bytes do not. The snapshot is records only, so the
    /// metadata says which bytes they are and nothing in the document is a base64 payload.
    #[test]
    fn a_snapshot_carries_attachment_records_and_not_their_bytes() {
        let dir = scratch("bytes");
        std::fs::create_dir_all(&dir).unwrap();

        let mine = {
            let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
            let mine = seed_project(&mut s, "mine");
            let (t, _) = seed_task(&mut s, Some(mine), "has an attachment");
            let blob = s.blobs().ingest_bytes(b"the bytes").unwrap();
            s.attach_blob(
                AttachmentTarget::Task,
                t,
                &blob.hash,
                "note.bin",
                None,
                blob.size_bytes as i64,
                ActorKind::Ai,
            )
            .unwrap();
            mine
        };

        let mut buf = Vec::new();
        stream_from(&store_file(&dir), Reach::window(mine), &mut buf).unwrap();
        let raw = String::from_utf8(buf).unwrap();
        assert!(!raw.contains("the bytes"), "the blob's content is not in the document");
        assert!(!raw.contains("export_path"), "there is no directory for a stream to point into");

        let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let row = &doc["tables"]["attachment"].as_array().unwrap()[0];
        assert_eq!(row["filename"], "note.bin");
        assert_eq!(row["size_bytes"], b"the bytes".len() as i64);
        assert!(row["blob_hash"].is_string(), "the metadata says which bytes, by their address");
    }

    /// Every table comes from one instant. A task and its comment committed **between the two tables
    /// being read** are in neither, rather than the comment turning up on its own — which is what a
    /// snapshot per statement would produce, and what nothing in the document would confess to.
    ///
    /// The seam a test can reach is the writer the stream itself writes through: the tables are emitted
    /// in registry order, so the moment `"task_comment"` appears in the output, `task` has been read and
    /// `task_comment` has not.
    #[test]
    fn a_snapshot_reads_every_table_from_one_instant() {
        /// Far above the seeded ids, so the rows the writer adds cannot collide with one of them.
        const LATE: i64 = 900_001;

        let dir = scratch("instant");
        std::fs::create_dir_all(&dir).unwrap();
        let mine = {
            let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
            let mine = seed_project(&mut s, "mine");
            seed_task(&mut s, Some(mine), "seeded");
            mine
        };
        let db = store_file(&dir);

        // A second connection, standing exactly where another Amenbo process would.
        let writer = rusqlite::Connection::open(&db).unwrap();
        writer.busy_timeout(std::time::Duration::from_secs(5)).unwrap();

        let mut out = Vec::new();
        let fired = {
            let mut tap = Tap {
                out: &mut out,
                at: "\"task_comment\"",
                fired: false,
                on: || {
                    writer
                        .execute(
                            "INSERT INTO task (id, title, project_id) VALUES (?1, 'landed mid-stream', ?2)",
                            (LATE, mine),
                        )
                        .unwrap();
                    writer
                        .execute(
                            "INSERT INTO task_comment (id, task_id, text) \
                             VALUES (?1, ?1, 'on a task the snapshot never saw')",
                            [LATE],
                        )
                        .unwrap();
                },
            };
            stream_from(&db, Reach::window(mine), &mut tap).unwrap();
            tap.fired
        };
        assert!(fired, "the tap never reached the seam — the table order changed under it");

        let doc: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let tasks = doc["tables"]["task"].as_array().unwrap();
        let comments = doc["tables"]["task_comment"].as_array().unwrap();
        assert!(
            !comments.iter().any(|c| c["task_id"] == LATE),
            "a comment whose task is not in this snapshot means the tables came from two instants",
        );
        assert!(
            !tasks.iter().any(|t| t["id"] == LATE),
            "and the task is not there either — one instant, held to the last table",
        );
    }

    /// **The whole and the stream meet exactly at the position the snapshot names.** A write landing
    /// *while the snapshot is being read* is the case that decides it: it is in neither the document nor
    /// anything a carrier could infer, so unless the feed replays it from the stated position it is lost
    /// in the seam between the two roads — silently, and for as long as nothing touches that task again.
    ///
    /// The other half is the boundary: what the snapshot already carries is **not** replayed, so a
    /// carrier that reads on does not re-apply what it just installed. The last write before the
    /// snapshot sits exactly *on* the cursor, which is what pins the read as exclusive.
    #[test]
    fn the_feed_runs_on_from_the_position_the_snapshot_names() {
        let dir = scratch("position");
        std::fs::create_dir_all(&dir).unwrap();

        let mut writer = Store::open_at(Paths::at(dir.clone())).unwrap();
        let mine = seed_project(&mut writer, "mine");
        let (seeded, on_seeded) = seed_task(&mut writer, Some(mine), "seeded");
        let db = store_file(&dir);

        // The seam: the moment `"task_comment"` appears in the output, `task` has been read and the
        // writer's task cannot reach the document — only the ledger can carry it.
        let mut out = Vec::new();
        let mut late = 0;
        {
            let mut tap = Tap {
                out: &mut out,
                at: "\"task_comment\"",
                fired: false,
                on: || late = seed_task(&mut writer, Some(mine), "landed mid-stream").0,
            };
            stream_from(&db, Reach::window(mine), &mut tap).unwrap();
            assert!(tap.fired, "the tap never reached the seam — the table order changed under it");
        }

        let doc: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(titles(&doc), vec!["seeded"], "the mid-stream task is not in the document");

        let conn = rusqlite::Connection::open(&db).unwrap();
        let cursor = doc["amenbo_sync"]["cursor"].as_i64().unwrap();
        let rows = match crate::store_engine::read::changes_since(&conn, cursor, 10_000, Some(mine)).unwrap() {
            crate::store_engine::read::FeedSlice::Changes { rows, .. } => rows,
            crate::store_engine::read::FeedSlice::Gap => {
                panic!("a snapshot handed out a cursor the feed had already lost")
            }
        };
        let named = |dataset: &str, row_id: i64| {
            rows.iter().any(|r| r.dataset == dataset && r.row_id == row_id)
        };
        assert!(named("task", late), "the write that landed mid-stream comes back from the position");
        assert!(!named("task", seeded), "and what the snapshot already carries is not replayed");
        assert!(
            !named("task_comment", on_seeded),
            "the last write before the snapshot sits on the cursor, and the read is exclusive",
        );
    }

    /// A snapshot never hands out a cursor the feed has already lost. A window nothing has written for a
    /// long time keeps a version from before truncation reached it, and a cursor below the feed's floor
    /// reads back as a gap — which would tell a carrier holding a complete picture that it had missed
    /// changes, and send it round for another one, forever. The floor costs nothing: the rows between the
    /// two are not this window's, and they are gone from the feed either way.
    #[test]
    fn a_dormant_window_is_not_handed_a_cursor_the_feed_has_already_lost() {
        let dir = scratch("floor");
        std::fs::create_dir_all(&dir).unwrap();

        let mine = {
            let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
            let mine = seed_project(&mut s, "mine");
            seed_task(&mut s, Some(mine), "written once, long ago");
            // Everything after that happens next door, so this window's version stands still while the
            // ledger climbs past it — which is the whole of what "dormant" means here.
            let theirs = seed_project(&mut s, "theirs");
            for i in 0..5 {
                seed_task(&mut s, Some(theirs), &format!("churn {i}"));
            }
            mine
        };
        let db = store_file(&dir);
        let conn = rusqlite::Connection::open(&db).unwrap();

        // Truncation is amortised over thousands of rows, so the watermark is written by hand rather than
        // earned. It is placed **between** this window's version and the ledger's head, which is where a
        // real cut lands: truncation only ever removes rows below the newest, so a floor above the head is
        // a state no store can be in. The key is `store_engine::engine::META_FEED_TRUNCATED_THROUGH`,
        // private to that module; the gap assertion below is what pins this literal to it — a wrong key
        // would read back as `0` and gap nothing.
        let head: i64 =
            conn.query_row("SELECT COALESCE(MAX(id), 0) FROM change_feed", [], |r| r.get(0)).unwrap();
        let past_the_window = head - 1;
        assert!(past_the_window > 0, "the churn left a ledger to cut into: head={head}");
        conn.execute(
            "INSERT INTO store_meta (key, value) VALUES ('change_feed_truncated_through', ?1)",
            [past_the_window],
        )
        .unwrap();
        assert!(
            matches!(
                crate::store_engine::read::changes_since(&conn, 0, 10, Some(mine)).unwrap(),
                crate::store_engine::read::FeedSlice::Gap,
            ),
            "the watermark is in force — a cursor below it is a gap",
        );

        let cursor = take(&db, Reach::window(mine))["amenbo_sync"]["cursor"].as_i64().unwrap();
        assert_eq!(cursor, past_the_window, "the position is lifted to the feed's floor");
        assert!(
            !matches!(
                crate::store_engine::read::changes_since(&conn, cursor, 10, Some(mine)).unwrap(),
                crate::store_engine::read::FeedSlice::Gap,
            ),
            "and so the carrier that reads on from it is not sent back for another snapshot",
        );
    }

    // ─────────────────── reading the named rows back ───────────────────

    /// Fill **every dataset a road out carries** in one project, and the same shapes again next door.
    /// Returns `(mine, theirs)`.
    ///
    /// The seed is deliberately exhaustive rather than representative: what the by-id read has to be is
    /// total — a table the snapshot carries and this cannot read back is a record a carrier is told
    /// changed and can never fetch — and the only way to hold that is to have a row in every one of them
    /// ([`every_table_the_snapshot_carries_reads_back_the_same_rows`]).
    fn seed_every_carried_dataset(dir: &Path) -> (i64, i64) {
        use crate::model::{DimensionCardinality, DimensionRole};

        let mut s = Store::open_at(Paths::at(dir.to_path_buf())).unwrap();
        let fill = |s: &mut Store, name: &str| {
            let project = seed_project(s, name);
            let (task, comment) = seed_task(s, Some(project), &format!("{name}: a task"));
            let (blocker, _) = seed_task(s, Some(project), &format!("{name}: a blocker"));
            s.depend_task(task, blocker, Some(ActorKind::Ai)).unwrap();
            s.add_task_commit(task, &"a1".repeat(20), Some(ActorKind::Ai)).unwrap();

            let older = s
                .add_decision(crate::ops::decision::NewDecision {
                    title: format!("{name}: the older one"),
                    body: "why".into(),
                    project_id: project,
                })
                .unwrap()
                .id;
            let newer = s
                .add_decision(crate::ops::decision::NewDecision {
                    title: format!("{name}: the newer one"),
                    body: "why, again".into(),
                    project_id: project,
                })
                .unwrap()
                .id;
            s.decision_builds_on(newer, older).unwrap();
            s.link_decision(newer, task).unwrap();
            s.add_decision_comment(newer, ActorKind::Ai, "on the decision").unwrap();

            let axis = s
                .dimension_add(
                    project,
                    crate::ops::dimension::NewDimension {
                        name: format!("{name}-axis"),
                        notes: String::new(),
                        cardinality: DimensionCardinality::Single,
                        ordered: false,
                        role: DimensionRole::None,
                        show_on_card: false,
                    },
                )
                .unwrap()
                .id;
            let value = s.dimension_value_add(axis, "a value", None).unwrap().id;
            s.set_task_dimension_value(task, value).unwrap();

            s.attach_url(AttachmentTarget::Task, task, "https://example.com/seed", None, ActorKind::Ai)
                .unwrap();
            // The two rows a plugin leaves in a project: what it was configured with, and whether its
            // gate is open here. Neither is a secret — those are on no road out at all.
            s.set_plugin_config_value(Some(project), "carrier", "channel", Some(name)).unwrap();
            s.set_plugin_enabled_in_project(Some(project), "carrier", true).unwrap();
            (project, comment)
        };

        let (mine, _) = fill(&mut s, "mine");
        let (theirs, _) = fill(&mut s, "theirs");
        (mine, theirs)
    }

    /// The rows of one table as the snapshot carries them, and their ids.
    fn carried(doc: &serde_json::Value, table: &str) -> Vec<serde_json::Value> {
        doc["tables"][table].as_array().unwrap_or(&Vec::new()).clone()
    }

    fn ids_of(rows: &[serde_json::Value]) -> Vec<i64> {
        rows.iter().map(|r| r["id"].as_i64().unwrap()).collect()
    }

    fn read_back(db_path: &Path, reach: Reach, dataset: &str, ids: &[i64]) -> serde_json::Value {
        let mut buf = Vec::new();
        records_from(db_path, reach, dataset, ids, &mut buf).unwrap();
        serde_json::from_slice(&buf).unwrap()
    }

    /// **The reason this road exists, held whole:** every table a snapshot carries can be read back by
    /// id, and what comes back is the row the snapshot carries — same columns, same values. A carrier is
    /// told by the ledger that a record moved and has nowhere else to take that id, so a table missing
    /// here is a change it can never resolve, and a row that differed here would mean it holds two shapes
    /// of the same record depending on which road it came down.
    ///
    /// The registry is walked rather than a list of tables written out: a dataset added later is carried
    /// by the snapshot the moment it is declared, and this fails until it can be read back too.
    #[test]
    fn every_table_the_snapshot_carries_reads_back_the_same_rows() {
        let dir = scratch("read-back");
        std::fs::create_dir_all(&dir).unwrap();
        let (mine, _) = seed_every_carried_dataset(&dir);
        let db = store_file(&dir);

        let doc = take(&db, Reach::window(mine));
        for dataset in export::datasets_carried_out() {
            let rows = carried(&doc, dataset.table);
            assert!(
                !rows.is_empty(),
                "the seed leaves no row in `{}`, so this proves nothing about reading it back — fill it \
                 in seed_every_carried_dataset",
                dataset.name,
            );
            let read = read_back(&db, Reach::window(mine), dataset.name, &ids_of(&rows));
            assert_eq!(
                carried(&read, dataset.table),
                rows,
                "`{}` does not read back as the snapshot carries it",
                dataset.name,
            );
            assert_eq!(read["amenbo_sync"]["format"], RECORDS_FORMAT);
            assert_eq!(read["amenbo_sync"]["project_id"], mine);
            assert!(
                read["amenbo_sync"]["cursor"].is_null(),
                "these rows stand at no position in the ledger, so the header names none",
            );
        }
    }

    /// **Not one row of the project next door, on any table.** The ids are that project's real ones, read
    /// off its own snapshot, so this is the case a carrier could actually reach: it holds ids from the
    /// ledger and asks for them. The same strictness `AMB-T-2789` / `AMB-T-2791` hold the read model to —
    /// walked over the whole registry, because one table forgetting the window is the whole leak.
    #[test]
    fn no_id_from_outside_the_window_reads_back_a_row() {
        let dir = scratch("read-back-window");
        std::fs::create_dir_all(&dir).unwrap();
        let (mine, theirs) = seed_every_carried_dataset(&dir);
        let db = store_file(&dir);

        let next_door = take(&db, Reach::window(theirs));
        for dataset in export::datasets_carried_out() {
            let ids = ids_of(&carried(&next_door, dataset.table));
            assert!(!ids.is_empty(), "`{}` has nothing next door to ask for", dataset.name);
            let read = read_back(&db, Reach::window(mine), dataset.name, &ids);
            assert_eq!(
                carried(&read, dataset.table),
                Vec::<serde_json::Value>::new(),
                "`{}` handed a window rows from the project next door",
                dataset.name,
            );
        }
    }

    /// A deleted id is simply absent, and an id that never existed with it. Nothing marks the gap: a
    /// carrier learns a record is gone from the `delete` in the feed, so an answer that had to
    /// distinguish "deleted" from "not yours" would be saying that a row it may not see exists.
    #[test]
    fn an_id_that_is_no_longer_there_is_simply_absent() {
        let dir = scratch("read-back-gone");
        std::fs::create_dir_all(&dir).unwrap();

        let (mine, kept, gone) = {
            let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
            let mine = seed_project(&mut s, "mine");
            let (kept, _) = seed_task(&mut s, Some(mine), "still here");
            let (gone, _) = seed_task(&mut s, Some(mine), "deleted after the feed named it");
            s.delete_task(gone, ActorKind::Human).unwrap();
            (mine, kept, gone)
        };
        let db = store_file(&dir);

        let read = read_back(&db, Reach::window(mine), "task", &[kept, gone, 900_001]);
        assert_eq!(ids_of(&carried(&read, "task")), vec![kept], "only what is still there comes back");
    }

    /// More ids than one read answers is **refused, not cut short**. A short page of changes is
    /// unambiguous — the cursor says where it stopped — but these ids are the caller's own list, so a
    /// partial answer would leave it working out which half it holds, and a caller that skipped that
    /// would drop records silently. The cap itself is a page of changes, so a carrier that drained one
    /// page can always ask for everything in it.
    #[test]
    fn more_ids_than_one_read_answers_is_refused_rather_than_cut_short() {
        let dir = scratch("read-back-cap");
        std::fs::create_dir_all(&dir).unwrap();
        let mine = {
            let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
            let mine = seed_project(&mut s, "mine");
            seed_task(&mut s, Some(mine), "the one real row");
            mine
        };
        let db = store_file(&dir);

        let full: Vec<i64> = (1..=RECORDS_PER_READ as i64).collect();
        let mut buf = Vec::new();
        records_from(&db, Reach::window(mine), "task", &full, &mut buf).unwrap();

        let mut over = full.clone();
        over.push(RECORDS_PER_READ as i64 + 1);
        let mut buf = Vec::new();
        let err = records_from(&db, Reach::window(mine), "task", &over, &mut buf).unwrap_err();
        assert!(err.to_string().contains(&RECORDS_PER_READ.to_string()), "it names the cap: {err}");
        assert!(buf.is_empty(), "a refusal leaves no half-document on the caller's stdout");

        // And nothing at all is not a question either: a road that answers ids cannot be asked for a
        // table.
        let mut buf = Vec::new();
        assert!(records_from(&db, Reach::window(mine), "task", &[], &mut buf).is_err());
    }

    /// A dataset no road out carries is refused rather than answered empty — including the plugin
    /// secrets, which are refused by being on no road out at all rather than by a second rule here. An
    /// empty answer would read as "those records are gone", and a carrier that believed it would delete
    /// what it holds.
    #[test]
    fn a_dataset_no_road_out_carries_is_refused() {
        let dir = scratch("read-back-dataset");
        std::fs::create_dir_all(&dir).unwrap();
        let mine = {
            let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
            seed_project(&mut s, "mine")
        };
        let db = store_file(&dir);

        for name in ["plugin_secret", "change_feed", "not_a_dataset"] {
            let mut buf = Vec::new();
            let err = records_from(&db, Reach::window(mine), name, &[1], &mut buf).unwrap_err();
            assert!(
                err.to_string().contains("task"),
                "the refusal says what it does read back, so a caller can correct it: {err}",
            );
            assert!(buf.is_empty(), "and nothing was written before the refusal");
        }
    }

    /// A sink that runs `on` once, the moment what has been written contains `at` — a seam inside the
    /// document, reached from the only place a snapshot lets a test stand (it takes no progress
    /// callback: nobody is watching a machine-facing stream).
    struct Tap<'a, F: FnMut()> {
        out: &'a mut Vec<u8>,
        at: &'static str,
        fired: bool,
        on: F,
    }

    impl<F: FnMut()> Write for Tap<'_, F> {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let n = self.out.write(buf)?;
            if !self.fired && String::from_utf8_lossy(self.out).contains(self.at) {
                self.fired = true;
                (self.on)();
            }
            Ok(n)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.out.flush()
        }
    }
}
