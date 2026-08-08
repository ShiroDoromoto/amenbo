//! Sync snapshot — the one instant of a project a carrier plugin takes off this device.
//!
//! A plugin that carries amenbo's data outward (a viewer, an audit trail, a mirror in another tool)
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
//!
//! **What it returns is JSON, records only.** The shape is the export's plain `{table: [rows]}` — a
//! consumer that can read one can read the other, and there is nothing about a snapshot that wants a
//! shape of its own. Attachment **rows** travel; attachment **bytes** do not. Folding the bytes in would
//! mean base64 in the JSON, which `AMB-D-178` already rejected for the export, and the cost lands on
//! every resend rather than once. So the snapshot carries what the metadata says — hash, filename, mime,
//! size — and a carrier that wants the bytes asks for them by their own road.
//!
//! It is plaintext, and deliberately so: this store is plaintext at rest, and encrypting what is carried
//! is the carrier's job, not amenbo's (`AMB-D-585`). Nothing reads a snapshot back in — the road out is
//! one-way (`AMB-D-578`).

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

/// Header object of a snapshot (`amenbo_sync` key): what produced it, when, and how far it reaches.
#[derive(Debug, Clone, Serialize)]
struct SnapshotHeader {
    /// Format tag ([`SNAPSHOT_FORMAT`]).
    format: &'static str,
    /// Envelope layout version ([`SNAPSHOT_VERSION`]).
    format_version: u32,
    /// Read-model schema version of the producing binary ([`crate::model::SCHEMA_VERSION`]).
    schema_version: &'static str,
    /// Producing binary's human-readable version.
    app_version: &'static str,
    /// When the snapshot was taken (RFC3339).
    taken_at: String,
    /// The project this snapshot is closed to, or `null` when it carries the whole device. Stated
    /// because a carrier holding two snapshots has no other way to tell what each one was allowed to
    /// see, and "everything I got" is not the same claim as "everything there is".
    project_id: Option<i64>,
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

    let scope = match reach.project() {
        None => None,
        Some(project_id) => {
            // The narrowing has to be **total** before a byte is written: a dataset that declares no way
            // to reach a project would stream whole, past the window. Failing here means the caller gets
            // an error instead of a half-document with one table too wide in it.
            if let Some(gap) = datasets.iter().find(|d| project_predicate(d).is_none()) {
                return Err(Error::invalid(format!(
                    "cannot take a snapshot of one project: the `{}` dataset does not declare how its \
                     table reaches one",
                    gap.name
                )));
            }
            Some(Scope { project_id, predicate: project_predicate })
        }
    };

    let header = SnapshotHeader {
        format: SNAPSHOT_FORMAT,
        format_version: SNAPSHOT_VERSION,
        schema_version: crate::model::SCHEMA_VERSION,
        app_version: export::APP_VERSION,
        taken_at: Timestamp::now().0.to_rfc3339(),
        project_id: reach.project(),
    };

    let conn = export::open_source(db_path)?;
    // **One snapshot for every table** — the whole point of the word. Each statement would otherwise
    // take its own, so a write committing between two tables lands in the later one and not the earlier,
    // and the carrier mirrors a state the store was never in. Deferred is the only shape a read-only
    // connection can take, and it is enough: the instant is fixed by the first table's read and held to
    // the last.
    let snapshot = conn.unchecked_transaction().map_err(crate::error::sqlite_at(db_path))?;

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

        // A second connection, standing exactly where another amenbo process would.
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
