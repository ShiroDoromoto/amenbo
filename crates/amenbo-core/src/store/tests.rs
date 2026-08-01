//! Integration tests for the `store` module: round-tripping through the engine truth source, the
//! write seam, the read layer, and the activity ledger beside it.

use super::Store;
use crate::config::Paths;
use crate::model::{Database, TaskStatus, View};
use crate::ops::task::NewTask;
use std::fs;

/// Hydrate a `Database` from the truth source. A tool for inspecting what actually landed there as
/// raw rows — the production read path itself is SQL.
fn hydrated(s: &Store) -> Database {
    crate::store_engine::hydrate_database(s.engine.conn()).unwrap()
}

    /// Another writer can write to the store while the GUI holds it open. Mutual exclusion is left
    /// entirely to SQLite's writer lock, taken per transaction, so a resident reader or writer never
    /// shuts anyone else out.
    #[test]
    fn a_resident_store_does_not_shut_out_another_writer() {
        let dir = amenbo_scratch::scratch("no-flock");
        // Create the store first — as it would exist before the GUI opens it.
        drop(Store::open_at(Paths::at(dir.clone())).unwrap());

        // The GUI's resident read open and resident write open, both kept alive.
        let gui_read = Store::open_read_at(Paths::at(dir.clone())).unwrap();
        let mut resident_writer = Store::open_at(Paths::at(dir.clone())).unwrap();

        // Meanwhile a further write open succeeds, and really can write.
        let mut other = Store::open_at(Paths::at(dir.clone())).unwrap();
        let t = other
            .add_task(NewTask {
                title: "written while the GUI holds the store".to_string(),
                project_id: None,
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: None,
            })
            .unwrap();

        // The resident side can write too. SQLite decides the order; neither is locked out.
        resident_writer.set_task_status(t.id, TaskStatus::InProgress, crate::model::ActorKind::Human).unwrap();
        assert_eq!(hydrated(&resident_writer).tasks.len(), 1);
        drop(gui_read);
    }

    /// The engine is the **truth source**: write, reopen, and the same content (task plus comment)
    /// comes back out of it.
    #[test]
    fn reopen_serves_database_from_the_engine() {
        let dir = amenbo_scratch::scratch("engine-truth");

        let tid = {
            let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
            let t = s.add_task(NewTask {
                title: "engine-served".to_string(),
                project_id: None,
                due_on: None,
                start_on: None,
                priority: None,
                notes: "n".to_string(),
                created_by_kind: None,
            })
            .unwrap();
            s.add_task_comment(t.id, crate::model::ActorKind::Human, "hi").unwrap();
            t.id
        };

        // Reopen: the read is served out of the engine truth source.
        let s = Store::open_at(Paths::at(dir.clone())).unwrap();
        let db = hydrated(&s);
        let t = db.tasks.iter().find(|t| t.id == tid).expect("task served from the engine");
        assert_eq!(t.title, "engine-served");
        assert_eq!(
            db.task_comments.iter().filter(|c| c.task_id == tid).count(),
            1,
            "comment served from the engine"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// Open keeps its hands off the files beside the store. All open owns is the gate (the version
    /// check); it owns no repair.
    #[test]
    fn open_leaves_the_files_beside_the_store_alone() {
        let dir = amenbo_scratch::scratch("open-untouched");

        // Create a live store.
        let tid = {
            let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
            let t = s.add_task(NewTask {
                title: "survives-the-open".to_string(),
                project_id: None,
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: None,
            })
            .unwrap();
            t.id
        };

        // Drop a file next to it that has nothing to do with the truth source.
        fs::write(dir.join("store.automerge"), b"not this build's business").unwrap();

        let s = Store::open_at(Paths::at(dir.clone())).unwrap();

        assert!(dir.join("store.automerge").exists(), "open deletes nothing beside the store");
        assert!(dir.join("store.sqlite").is_file(), "the sqlite truth source is untouched");
        assert!(s.task(tid).unwrap().is_some(), "and its content is served as before");

        let _ = fs::remove_dir_all(&dir);
    }

    /// `Store::list_tasks` selects through the indexed SQL path against the engine truth source. This
    /// pins what selection and ordering mean.
    #[test]
    fn list_tasks_selects_and_orders_through_the_engine() {
        use crate::ops::task::NewTask;
        let dir = amenbo_scratch::scratch("list-route");

        let (a, b, c) = {
            let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
            let mut add = |title: &str, priority: Option<crate::model::Priority>| {
                s.add_task(NewTask {
                    title: title.to_string(),
                    project_id: None,
                    due_on: None,
                    start_on: None,
                    priority,
                    notes: String::new(),
                    created_by_kind: None,
                })
                .unwrap()
                .id
            };
            let b = add("b-task", Some(crate::model::Priority::High));
            let a = add("a-task", None);
            let c = add("c-task", None);
            (a, b, c)
        };

        let mk_params = |sort: &str, filter: Option<&str>| crate::query::ListParams {
            project_id: None,
            filter_expr: filter.map(str::to_string),
            text: None,
            sort: sort.to_string(),
            limit: None,
            offset: None,
        };
        let ids = |r: &crate::query::TaskListResult| r.tasks.iter().map(|t| t.id).collect::<Vec<_>>();

        let s = Store::open_at(Paths::at(dir.clone())).unwrap();
        assert_eq!(ids(&s.list_tasks(mk_params("title", None)).unwrap()), vec![a, b, c]);
        assert_eq!(ids(&s.list_tasks(mk_params("-title", None)).unwrap()), vec![c, b, a], "- is descending");
        // A filter descends into the SQL WHERE clause. `total_matched` is the count before paging —
        // here, one.
        let high = s.list_tasks(mk_params("title", Some("priority:high"))).unwrap();
        assert_eq!(ids(&high), vec![b]);
        assert_eq!(high.total_matched, 1);

        let _ = fs::remove_dir_all(&dir);
    }

    /// Reopening never re-backfills an engine that already holds data. Run a representative set of
    /// mutations, reopen, and measure by the counts staying put: the comment row the write seam
    /// committed, and the ledger line. (A system event has no DB row of its own, so the ledger line is
    /// what says it landed.)
    #[test]
    fn reopen_does_not_regrow_the_engine() {
        use crate::model::Priority;
        let dir = amenbo_scratch::scratch("p2-idem");
        let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
        let count = |s: &Store| -> (i64, usize) {
            let comments =
                s.engine.conn().query_row("SELECT count(*) FROM task_comment", [], |r| r.get(0)).unwrap();
            (comments, crate::activity_log::read(&s.paths.activity_file).len())
        };

        // Run real mutations (project / task / status / priority) plus a system event.
        let proj = s
            .project_add(crate::ops::project::NewProject {
                name: "PJ".into(),
                view: View::List,
                notes: String::new(),
                color: None,
            })
            .unwrap();
        let t = s.add_task(crate::ops::task::NewTask {
            title: "T".into(),
            project_id: Some(proj.id),
            due_on: None,
            start_on: None,
            priority: Some(Priority::High),
            notes: String::new(),
            created_by_kind: None,
        })
        .unwrap();
        s.set_task_status(t.id, TaskStatus::InProgress, crate::model::ActorKind::Human).unwrap();
        s.add_task_comment(t.id, crate::model::ActorKind::Human, "コメント").unwrap();
        s.add_system_event(
            crate::model::ActorKind::Ai,
            t.id,
            crate::activity_log::event::task_status_changed("todo", "in_progress"),
        )
        .unwrap();
        let after_mut = count(&s);
        assert_eq!(after_mut, (1, 1), "the write seam must have committed the comment and the event");

        // Reopen: the engine is non-empty, so nothing is backfilled and the counts do not move.
        drop(s);
        let s2 = Store::open_at(Paths::at(dir.clone())).unwrap();
        assert_eq!(count(&s2), after_mut, "reopen keeps the committed engine without re-backfill");

        let _ = fs::remove_dir_all(&dir);
    }

    /// `Store::read_model()` **borrows** the persistent store engine rather than reprojecting it.
    /// That is what lets the read commands query this connection directly instead of projecting the
    /// whole store on every read. Here: the borrowed read-model serves the current state (three task
    /// rows) that the writes maintained incrementally.
    #[test]
    fn read_model_borrows_persistent_engine_without_reprojection() {
        use crate::model::ActorKind;

        let dir = amenbo_scratch::scratch("readmodel");

        // Feed the engine through real ops; each write wrapper commits its own operation.
        {
            let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
            let proj = s
                .project_add(crate::ops::project::NewProject {
                    name: "PJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap();
            for title in ["alpha", "beta", "gamma"] {
                s.add_task(crate::ops::task::NewTask {
                    title: title.into(),
                    project_id: Some(proj.id),
                    due_on: None,
                    start_on: None,
                    priority: None,
                    notes: String::new(),
                    created_by_kind: Some(ActorKind::Human),
                })
                .unwrap();
            }
        }

        // Reopen: the engine is non-empty, so nothing is backfilled — this is the persistent
        // read-model the writes kept up to date.
        let s = Store::open_at(Paths::at(dir.clone())).unwrap();
        let rm = s.read_model();

        // The borrowed connection can be queried directly and already holds the current state.
        let n: i64 = rm
            .conn()
            .query_row("SELECT count(*) FROM task", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 3, "a live task can be pulled straight from the persistent read-model");

        let _ = fs::remove_dir_all(&dir);
    }


/// `version_is_newer` is pure local version-string comparison — no network, no store.
#[test]
fn version_is_newer_compares_numerically_and_ignores_metadata() {
    use super::version_is_newer;

    assert!(version_is_newer("0.1.1", "0.1.0"));
    assert!(version_is_newer("0.2.0", "0.1.9"));
    assert!(version_is_newer("0.1.10", "0.1.9")); // numeric, so the carry a string compare would lose
    assert!(!version_is_newer("0.1.0", "0.1.0"));
    assert!(!version_is_newer("0.1.0", "0.1.1"));
    assert!(!version_is_newer("0.1.1-rc.1", "0.1.1")); // metadata ignored: same version, so not newer
    assert!(!version_is_newer("garbage", "0.1.0")); // unparseable falls to false, the safe side
}

/// `version_status()` on its own touches no network, so it can never raise the update flag; only
/// folding in the upstream release can.
#[test]
fn version_status_alone_never_flags_an_update() {
    let dir = amenbo_scratch::scratch("updavail");
    let s = Store::open_at(Paths::at(dir.clone())).unwrap();
    let vs = s.version_status();
    assert!(!vs.update_available, "the zero-network standalone check does not flag an update");
    assert_eq!(vs.newer_version, None);
    assert_eq!(vs.latest_version, None);
    assert_eq!(vs.format_version, crate::model::FORMAT_VERSION, "the format version is read from the source of truth");
    fs::remove_dir_all(&dir).ok();
}

/// `with_upstream()`: only the upstream `latest.json` can set `update_available`.
#[test]
fn with_upstream_folds_in_upstream_release() {
    use super::VersionStatus;
    use crate::update_check::LatestRelease;

    let base = || VersionStatus {
        app_version: "0.1.0",
        format_version: 1,
        max_supported_format: 1,
        update_available: false,
        newer_version: None,
        latest_version: None,
    };
    let rel = |v: &str| LatestRelease {
        version: v.into(),
        notes_url: None,
        assets: Default::default(),
    };

    // Upstream is newer: the flag goes up and both `latest_version` and `newer_version` are filled in.
    let vs = base().with_upstream(Some(&rel("0.2.0")));
    assert!(vs.update_available, "a newer upstream version means an update is available");
    assert_eq!(vs.newer_version.as_deref(), Some("0.2.0"));
    assert_eq!(vs.latest_version.as_deref(), Some("0.2.0"));

    // Upstream is the same version, or older: `latest_version` is still reported for information, but
    // no update is flagged.
    let vs = base().with_upstream(Some(&rel("0.1.0")));
    assert!(!vs.update_available, "the same upstream version means no update");
    assert_eq!(vs.newer_version, None);
    assert_eq!(vs.latest_version.as_deref(), Some("0.1.0"), "still reported for information even at the same version");

    let vs = base().with_upstream(Some(&rel("0.0.9")));
    assert!(!vs.update_available, "an older upstream means no update");
    assert_eq!(vs.latest_version.as_deref(), Some("0.0.9"));

    // No upstream at all (check disabled, not fetched yet, or the fetch failed): no update, no network.
    let vs = base().with_upstream(None);
    assert!(!vs.update_available, "an unknown upstream falls back to no update");
    assert_eq!(vs.newer_version, None);
    assert_eq!(vs.latest_version, None);
}

// ───────────────────────── the write seam ─────────────────────────

/// Open a store in a fresh temp dir and hand back `(store, dir)`.
fn fresh_store(tag: &str) -> (Store, std::path::PathBuf) {
    let dir = amenbo_scratch::scratch(tag);
    let s = Store::open_at(Paths::at(dir.clone())).unwrap();
    (s, dir)
}

fn task(title: &str, project_id: Option<i64>) -> NewTask {
    NewTask {
        title: title.into(),
        project_id,
        due_on: None,
        start_on: None,
        priority: None,
        notes: String::new(),
        created_by_kind: None,
    }
}

/// **One logical operation, one transaction**: by the time a write wrapper returns, the operation is
/// committed to the truth source. Drop the store, reopen it as another process would, and everything
/// is still there. The conversational number is taken the same way — `next_id` read **inside**
/// the transaction — so consecutive adds number densely. Read outside it, and two concurrent writers
/// would both take 1.
#[test]
fn task_writes_commit_per_operation() {
    let (mut s, dir) = fresh_store("seam-nosave");
    let proj = s
        .project_add(crate::ops::project::NewProject {
            name: "PJ".into(),
            view: View::List,
            notes: String::new(),
            color: None,
        })
        .unwrap();

    let t1 = s.add_task(task("first", Some(proj.id))).unwrap();
    let t2 = s.add_task(task("second", Some(proj.id))).unwrap();
    s.set_task_status(t2.id, TaskStatus::InProgress, crate::model::ActorKind::Human).unwrap();
    assert_eq!((t1.id, t2.id), (1, 2), "numbering is MAX+1 within the transaction");
    drop(s);

    let s = Store::open_at(Paths::at(dir.clone())).unwrap();
    let db = hydrated(&s);
    let live: Vec<_> = db.tasks.iter().collect();
    assert_eq!(live.len(), 2, "each operation is already committed");
    let reopened = s.task(t2.id).unwrap().unwrap();
    assert_eq!(reopened.status, TaskStatus::InProgress, "the status change is committed too");
    fs::remove_dir_all(&dir).ok();
}

/// Reserving a task is a compare-and-swap: it reads the status from the **truth source** inside the
/// same transaction that writes the reservation. Move that read outside, and a reservation another
/// process takes in the gap goes unseen — which is the whole double-start guard, gone.
#[test]
fn reserving_is_a_compare_and_swap_against_the_truth_source() {
    let (mut s, dir) = fresh_store("seam-cas");
    let t = s.add_task(task("reserve me", None)).unwrap();
    s.set_task_status(t.id, TaskStatus::InProgress, crate::model::ActorKind::Human).unwrap();

    let err = s.set_task_status(t.id, TaskStatus::InProgress, crate::model::ActorKind::Human).unwrap_err();
    assert_eq!(err.code(), "already_reserved", "reservation is rejected when not todo");
    fs::remove_dir_all(&dir).ok();
}

/// The ready guard likewise reads the blockers from the **truth source** inside the reservation's own
/// transaction. An unfinished blocker rejects the reservation with `not_ready`, and the status does
/// not move.
#[test]
fn the_ready_guard_reads_the_blockers_from_the_truth_source() {
    let (mut s, dir) = fresh_store("seam-ready");
    let t = s.add_task(task("reserve me", None)).unwrap();
    let blocker = s.add_task(task("do me first", None)).unwrap();
    s.depend_task(t.id, blocker.id, None).unwrap();

    let err = s.set_task_status(t.id, TaskStatus::InProgress, crate::model::ActorKind::Human).unwrap_err();
    assert_eq!(err.code(), "not_ready", "reservation is rejected when there is an unfinished blocker");
    assert_eq!(
        crate::store_engine::read::task_status(s.engine.conn(), t.id).unwrap(),
        Some(TaskStatus::Todo),
        "rejected, so the status does not move"
    );

    // Finish the blocker and the precondition is met, so the reservation goes through.
    s.set_task_status(blocker.id, TaskStatus::Done, crate::model::ActorKind::Human).unwrap();
    assert_eq!(s.set_task_status(t.id, TaskStatus::InProgress, crate::model::ActorKind::Human).unwrap().status, TaskStatus::InProgress);
    fs::remove_dir_all(&dir).ok();
}

/// `delete` removes the task and its dependency edges in **one transaction**. Leave one behind and
/// the store keeps a dangling edge. Ask the truth source directly that both are gone.
#[test]
fn delete_removes_the_task_and_its_edges_together() {
    let (mut s, dir) = fresh_store("seam-delete");
    let t = s.add_task(task("doomed", None)).unwrap();
    let blocker = s.add_task(task("blocker", None)).unwrap();
    s.depend_task(t.id, blocker.id, None).unwrap();
    s.delete_task(t.id, crate::model::ActorKind::Human).unwrap();

    let conn = s.engine.conn();
    let live_tasks: i64 = conn
        .query_row("SELECT count(*) FROM task WHERE id = ?1", [&t.id], |r| r.get(0))
        .unwrap();
    let live_edges: i64 = conn
        .query_row("SELECT count(*) FROM task_dependency WHERE task_id = ?1", [&t.id], |r| r.get(0))
        .unwrap();
    assert_eq!((live_tasks, live_edges), (0, 0), "both the task and its edges go in the same transaction");
    fs::remove_dir_all(&dir).ok();
}

/// A plain project.
fn project(name: &str) -> crate::ops::project::NewProject {
    crate::ops::project::NewProject {
        name: name.into(),
        view: View::List,
        notes: String::new(),
        color: None,
    }
}

/// The AI-harness consent is per project, because the answer changes with the place: an AI runs the
/// backlog in one folder and the person runs it in another. No row is "never asked", which is the state
/// the whole question hangs on.
#[test]
fn the_harness_consent_is_recorded_per_project_and_survives_a_reopen() {
    use crate::harness::Consent;

    let (mut s, dir) = fresh_store("harness-consent");
    let a = s.project_add(project("PJ A")).unwrap();
    let b = s.project_add(project("PJ B")).unwrap();

    // Never asked — not a `no`, which is the distinction a row cannot lose.
    assert_eq!(s.harness_consent(a.id).unwrap(), None);

    s.set_harness_consent(a.id, Consent::answered(true)).unwrap();
    assert_eq!(s.harness_consent(a.id).unwrap(), Some(Consent { allowed: true, asked_again: false }));
    assert_eq!(s.harness_consent(b.id).unwrap(), None, "and only that project's");

    // The one re-ask is recorded as spent, whichever way it went.
    s.set_harness_consent(a.id, Consent::answered_again(true)).unwrap();
    assert_eq!(s.harness_consent(a.id).unwrap(), Some(Consent { allowed: true, asked_again: true }));

    // A refusal replaces the answer rather than adding a second row.
    s.set_harness_consent(a.id, Consent::answered(false)).unwrap();
    assert_eq!(s.harness_consent(a.id).unwrap(), Some(Consent { allowed: false, asked_again: false }));

    drop(s);
    let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
    assert_eq!(
        s.harness_consent(a.id).unwrap(),
        Some(Consent { allowed: false, asked_again: false }),
        "the answer outlives the process that recorded it"
    );

    // The answer is about the project, so it goes when the project does.
    s.set_harness_consent(b.id, Consent::answered(true)).unwrap();
    s.project_delete(b.id, crate::model::ActorKind::Human).unwrap();
    assert_eq!(s.harness_consent(b.id).unwrap(), None);
    assert!(s.harness_consent(a.id).unwrap().is_some(), "and only that project's");
    fs::remove_dir_all(&dir).ok();
}

/// Clearing is the way back from a refusal, which is silent for good on its own. It lands on "never
/// asked" — the state the question is put from — and not on a `no` that merely reads as one.
#[test]
fn clearing_the_harness_consent_returns_the_project_to_never_having_been_asked() {
    use crate::harness::Consent;

    let (mut s, dir) = fresh_store("harness-consent-clear");
    let a = s.project_add(project("PJ A")).unwrap();
    let b = s.project_add(project("PJ B")).unwrap();

    // Clearing what was never answered is the state asked for, not an error.
    s.clear_harness_consent(a.id).unwrap();
    assert_eq!(s.harness_consent(a.id).unwrap(), None);

    s.set_harness_consent(a.id, Consent::answered(false)).unwrap();
    s.set_harness_consent(b.id, Consent::answered(false)).unwrap();
    s.clear_harness_consent(a.id).unwrap();
    assert_eq!(s.harness_consent(a.id).unwrap(), None);
    assert_eq!(
        s.harness_consent(b.id).unwrap(),
        Some(Consent { allowed: false, asked_again: false }),
        "and only that project's"
    );

    // The spent re-ask goes with the answer: what it was spent against is gone.
    s.set_harness_consent(a.id, Consent::answered_again(true)).unwrap();
    s.clear_harness_consent(a.id).unwrap();
    assert_eq!(s.harness_consent(a.id).unwrap(), None);

    drop(s);
    let s = Store::open_at(Paths::at(dir.clone())).unwrap();
    assert_eq!(
        s.harness_consent(a.id).unwrap(),
        None,
        "and the record stays gone for the process that asks next"
    );
    fs::remove_dir_all(&dir).ok();
}

/// The lint-hook opt-out is per project and outlives the process that recorded it — `hooks uninstall`
/// says "not this one", and it keeps saying it on this clone and the next. The *answer* is not here at
/// all: it is one per device, in `config.hook_consent`.
#[test]
fn a_hook_optout_is_recorded_per_project_and_survives_a_reopen() {
    let (mut s, dir) = fresh_store("hook-optout");
    let a = s.project_add(project("PJ A")).unwrap();
    let b = s.project_add(project("PJ B")).unwrap();

    // Never opted out: no row, which is what lets the device's answer reach a repository.
    assert!(!s.hook_opted_out(a.id).unwrap());

    // Each project stands for itself, and setting it twice is setting it once.
    s.set_hook_optout(a.id, true).unwrap();
    s.set_hook_optout(a.id, true).unwrap();
    assert!(s.hook_opted_out(a.id).unwrap());
    assert!(!s.hook_opted_out(b.id).unwrap(), "and only that project's");

    // `hooks install` takes it back, which is what makes the pair symmetric.
    s.set_hook_optout(a.id, false).unwrap();
    assert!(!s.hook_opted_out(a.id).unwrap());
    s.set_hook_optout(a.id, true).unwrap();

    drop(s);
    let mut s = Store::open_at(Paths::at(dir.clone())).unwrap();
    assert!(s.hook_opted_out(a.id).unwrap(), "the opt-out outlives the process that recorded it");

    // The veto is about the project, so it goes when the project does.
    s.set_hook_optout(b.id, true).unwrap();
    s.project_delete(b.id, crate::model::ActorKind::Human).unwrap();
    assert!(!s.hook_opted_out(b.id).unwrap());
    assert!(s.hook_opted_out(a.id).unwrap(), "and only that project's");
    fs::remove_dir_all(&dir).ok();
}

/// A store written before `hook_optout` was declared has no such table, and opening it is what gives
/// it one — the declaration is `CREATE TABLE IF NOT EXISTS`, run on every open, so a new plain table
/// reaches an old store without a migration step. Standing in for that older store: drop the table
/// from a store this build made, and reopen.
#[test]
fn an_old_store_gains_the_optout_table_on_open() {
    let (mut s, dir) = fresh_store("hook-optout-old-store");
    let p = s.project_add(project("PJ")).unwrap();
    s.engine.conn().execute_batch("DROP TABLE hook_optout;").unwrap();
    drop(s);

    let s = Store::open_at(Paths::at(dir.clone())).unwrap();
    assert!(!s.hook_opted_out(p.id).unwrap(), "an old store has vetoed nothing, and reads that way");
    s.set_hook_optout(p.id, true).unwrap();
    assert!(s.hook_opted_out(p.id).unwrap());
    fs::remove_dir_all(&dir).ok();
}

/// Projects and dimensions ride the same write seam: committed by the time the wrapper returns.
/// Create a project, a dimension, a value and a task assignment, drop the store, and find all four
/// still there on reopen.
#[test]
fn project_and_dimension_writes_commit_per_operation() {
    let (mut s, dir) = fresh_store("seam-dim-nosave");
    let p = s.project_add(project("PJ")).unwrap();
    let d = s
        .dimension_add(p.id, crate::ops::dimension::NewDimension { name: "軸".into(), ..Default::default() })
        .unwrap();
    let v = s.dimension_value_add(d.id, "値", None).unwrap();
    let t = s.add_task(task("分類されるタスク", Some(p.id))).unwrap();
    s.set_task_dimension_value(t.id, v.id).unwrap();
    drop(s);

    let s = Store::open_at(Paths::at(dir.clone())).unwrap();
    let db = hydrated(&s);
    assert!(db.projects.iter().any(|x| x.id == p.id), "the project is in the source of truth");
    assert!(db.dimensions.iter().any(|x| x.id == d.id), "the dimension is in the source of truth");
    assert!(db.dimension_values.iter().any(|x| x.id == v.id), "the dimension value is in the source of truth");
    assert!(
        db.task_dimension_values.iter().any(|x| x.task_id == t.id && x.value_id == v.id),
        "the task assignment is in the source of truth"
    );
    fs::remove_dir_all(&dir).ok();
}

/// The three-level cascade behind `project::delete` — the project row, its tasks, their dependency
/// edges — rides **one transaction**. Applied partially, it would leave live tasks orphaned under a
/// deleted project, or dangling edges hanging off deleted tasks. Ask the truth source directly that
/// all three are gone.
#[test]
fn deleting_a_project_removes_its_tasks_and_edges_together() {
    let (mut s, dir) = fresh_store("seam-project-delete");
    let p = s.project_add(project("消えるPJ")).unwrap();
    let t1 = s.add_task(task("属タスク1", Some(p.id))).unwrap();
    let t2 = s.add_task(task("属タスク2", Some(p.id))).unwrap();
    s.depend_task(t1.id, t2.id, None).unwrap();
    s.project_delete(p.id, crate::model::ActorKind::Human).unwrap();

    let conn = s.engine.conn();
    let live = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(
        (
            live("SELECT count(*) FROM project"),
            live("SELECT count(*) FROM task"),
            live("SELECT count(*) FROM task_dependency"),
        ),
        (0, 0, 0),
        "the project, its member tasks, and dependency edges go together"
    );
    fs::remove_dir_all(&dir).ok();
}

/// Placing a row reads its siblings' `order_key` from the **truth source**, inside the transaction
/// that writes the placement. Move that read outside and two writers take the same trailing key —
/// exactly the `duplicate_order_key` that doctor reports.
#[test]
fn order_key_placement_reads_the_siblings_from_the_truth_source() {
    let (mut s, dir) = fresh_store("seam-order-key");
    let p1 = s.project_add(project("先住PJ")).unwrap();
    let d = s
        .dimension_add(p1.id, crate::ops::dimension::NewDimension { name: "軸".into(), ..Default::default() })
        .unwrap();
    let v1 = s.dimension_value_add(d.id, "先住の値", None).unwrap();

    let p2 = s.project_add(project("あとから来たPJ")).unwrap();
    let v2 = s.dimension_value_add(d.id, "あとから来た値", None).unwrap();
    assert_ne!(p2.order_key, p1.order_key, "a project's tail key is placed after its siblings in the source of truth");
    assert_ne!(v2.order_key, v1.order_key, "a dimension value's tail key is placed after its siblings in the source of truth too");
    fs::remove_dir_all(&dir).ok();
}

/// A project's slug is derived from the set of slugs already taken in the **truth source**, read
/// afresh on every create. So a second project of the same name lands on `alpha-2` instead of
/// colliding, and the unique index (`project_by_slug`) never has to reject the commit.
#[test]
fn the_project_slug_is_derived_from_the_slugs_in_the_truth_source() {
    let (mut s, dir) = fresh_store("seam-project-slug");
    let slug_of = |s: &Store, id: i64| {
        crate::store_engine::read::project(s.engine.conn(), id).unwrap().unwrap().slug
    };

    let p1 = s.project_add(project("Alpha")).unwrap();
    assert_eq!(slug_of(&s, p1.id).as_deref(), Some("alpha"), "fixed at creation");

    let p2 = s.project_add(project("Alpha")).unwrap();
    assert_eq!(
        slug_of(&s, p2.id).as_deref(),
        Some("alpha-2"),
        "used slugs are read from the source of truth"
    );
    fs::remove_dir_all(&dir).ok();
}

/// On a single-select axis, `(task, dimension)` must hold exactly one row. That invariant survives
/// only because **the delete that removes the old value and the insert that adds the new one share
/// a transaction**. Commit them separately and a crash in between leaves zero rows or two. After the
/// replacement, the truth source holds exactly one — the new value.
#[test]
fn setting_a_dimension_value_replaces_within_the_axis_in_one_transaction() {
    let (mut s, dir) = fresh_store("seam-dim-set");
    let p = s.project_add(project("PJ")).unwrap();
    let d = s
        .dimension_add(p.id, crate::ops::dimension::NewDimension { name: "軸".into(), ..Default::default() })
        .unwrap();
    let a = s.dimension_value_add(d.id, "A", None).unwrap();
    let b = s.dimension_value_add(d.id, "B", None).unwrap();
    let t = s.add_task(task("分類されるタスク", Some(p.id))).unwrap();
    s.set_task_dimension_value(t.id, a.id).unwrap();
    s.set_task_dimension_value(t.id, b.id).unwrap();

    let conn = s.engine.conn();
    let mut stmt = conn
        .prepare("SELECT value_id FROM task_dimension_value WHERE task_id = ?1 AND dimension_id = ?2")
        .unwrap();
    // `value_id` is an INTEGER column, comparable as-is against `DimensionValue.id: i64` at the boundary.
    let live: Vec<i64> = stmt
        .query_map(rusqlite::params![&t.id, d.id], |r| r.get::<_, i64>(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    drop(stmt);
    assert_eq!(live, vec![b.id], "before and after replacement, (task, dimension) is exactly one row");
    fs::remove_dir_all(&dir).ok();
}

/// The GUI board builds its columns from a single query for one project's `(task, value)` assignments
/// (`project_dimension_assignments`). It has to get two things right: return assignments for the axis
/// asked for and no other, and for the project asked for and no other. Get either wrong and the SQL
/// still runs — the board just quietly lists someone else's rows.
#[test]
fn a_board_reads_only_its_own_project_and_only_the_axis_it_asked_for() {
    let (mut s, dir) = fresh_store("board-assignments");
    let p = s.project_add(project("こちら")).unwrap();
    let other = s.project_add(project("よそ")).unwrap();

    let axis = s
        .dimension_add(p.id, crate::ops::dimension::NewDimension { name: "状態".into(), ..Default::default() })
        .unwrap();
    let doing = s.dimension_value_add(axis.id, "着手", None).unwrap();
    // A second axis in the same project — it must not bleed in.
    let sibling = s
        .dimension_add(p.id, crate::ops::dimension::NewDimension { name: "区分".into(), ..Default::default() })
        .unwrap();
    let bug = s.dimension_value_add(sibling.id, "バグ", None).unwrap();
    let t = s.add_task(task("こちらのタスク", Some(p.id))).unwrap();
    s.set_task_dimension_value(t.id, doing.id).unwrap();
    s.set_task_dimension_value(t.id, bug.id).unwrap();

    // Pin a task from the *other* project onto **this** project's axis value. `ops::dimension::set`
    // refuses to write that (a classification may not cross projects), so it goes in at the truth source:
    // what is under test is the reader's project filter, the last thing standing should a row like this
    // ever exist — an older store, a restore, a file edited by hand.
    let far = s.add_task(task("よそのタスク", Some(other.id))).unwrap();
    s.engine
        .conn()
        .execute(
            "INSERT INTO task_dimension_value (task_id, dimension_id, value_id, created_at, updated_at) \
             VALUES (?1, ?2, ?3, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            rusqlite::params![far.id, axis.id, doing.id],
        )
        .unwrap();

    let rows =
        crate::store_engine::read::project_dimension_assignments(s.engine.conn(), p.id, axis.id)
            .unwrap();
    assert_eq!(
        rows,
        vec![(t.id, doing.id)],
        "only the requested axis and project's assignments (no neighboring axis, no other project mixed in)"
    );
    fs::remove_dir_all(&dir).ok();
}

/// A task's own page names what it is filed under, in words (`task_classification`). Two things have to
/// hold for that to read straight: the pairs come back by **axis** order rather than by the order the
/// filings happened in, and a filing whose value has since been deleted names nothing and so is not
/// there at all.
#[test]
fn a_tasks_classification_reads_by_axis_and_drops_a_deleted_value() {
    use crate::ops::dimension::NewDimension;

    let (mut s, dir) = fresh_store("task-classification");
    let p = s.project_add(project("PJ")).unwrap();
    let first =
        s.dimension_add(p.id, NewDimension { name: "リリース".into(), ..Default::default() }).unwrap();
    let second =
        s.dimension_add(p.id, NewDimension { name: "区分".into(), ..Default::default() }).unwrap();
    let bucket = s.dimension_value_add(first.id, "第1弾", None).unwrap();
    let kind = s.dimension_value_add(second.id, "バグ", None).unwrap();

    let t = s.add_task(task("分類されたタスク", Some(p.id))).unwrap();
    // Filed on the second axis first, so the order that comes back cannot be the order they were made.
    s.set_task_dimension_value(t.id, kind.id).unwrap();
    s.set_task_dimension_value(t.id, bucket.id).unwrap();

    let named = |s: &Store| {
        crate::store_engine::read::task_classification(s.engine.conn(), t.id).unwrap()
    };
    assert_eq!(
        named(&s),
        vec![("リリース".to_string(), "第1弾".to_string()), ("区分".to_string(), "バグ".to_string())],
        "the axes read in their own order, not in the order the task was filed"
    );

    // A value deleted takes its filings with it: what is left names an axis and nothing on it, which is
    // not a classification anyone can be shown.
    s.dimension_value_delete(kind.id).unwrap();
    assert_eq!(named(&s), vec![("リリース".to_string(), "第1弾".to_string())]);
    fs::remove_dir_all(&dir).ok();
}

/// On create, a task looks at its project's time axis and defaults to whichever value's window covers
/// today. This is a default, not a requirement: with no time axis, with no window covering today, or
/// in the inbox, the task is simply created unassigned — creation is never refused. An axis that is
/// not a time axis takes no part, even if its values carry dates.
#[test]
fn add_task_defaults_to_the_time_axis_value_covering_today() {
    use crate::model::DimensionRole;
    use crate::ops::dimension::NewDimension;

    let (mut s, dir) = fresh_store("time-axis-default");
    let proj = s.project_add(project("PJ")).unwrap();

    // Read the assignments back from the truth source.
    let assigned = |s: &Store, task_id: i64| -> Vec<i64> {
        crate::store_engine::read::task_dimension_assignments(
            s.engine.conn(),
            task_id,
        )
        .unwrap()
        .into_iter()
        .map(|(_dimension_id, value_id)| value_id)
        .collect()
    };

    // No time axis: nothing is assigned.
    let bare = s.add_task(task("軸なし", Some(proj.id))).unwrap();
    assert!(assigned(&s, bare.id).is_empty());

    // A non-time axis whose values carry dates produces no default — the role is the gatekeeper.
    let category = s
        .dimension_add(proj.id, NewDimension { name: "カテゴリー".into(), ..NewDimension::default() })
        .unwrap();
    s.dimension_value_add(category.id, "バグ", Some((Some(crate::time::today()), None))).unwrap();
    let uncategorized = s.add_task(task("非時間軸", Some(proj.id))).unwrap();
    assert!(assigned(&s, uncategorized.id).is_empty(), "a non-time_axis dimension creates no default assignment");

    // While the time axis holds only a past window that does not cover today, tasks stay unassigned —
    // the default is never forced.
    let axis = s
        .dimension_add(
            proj.id,
            NewDimension { name: "時代".into(), role: DimensionRole::TimeAxis, ordered: true, ..NewDimension::default() },
        )
        .unwrap();
    let yesterday = crate::time::today().pred_opt().unwrap();
    let past = s.dimension_value_add(axis.id, "開発期", Some((None, Some(yesterday)))).unwrap();
    let outside = s.add_task(task("窓の外", Some(proj.id))).unwrap();
    assert!(assigned(&s, outside.id).is_empty(), "with no era covering today, nothing is assigned");

    // Add an open-ended, current window and every task created from here on picks it up by default.
    let current = s
        .dimension_value_add(axis.id, "運用第1期", Some((Some(crate::time::today()), None)))
        .unwrap();
    let fresh = s.add_task(task("既定つき", Some(proj.id))).unwrap();
    assert_eq!(assigned(&s, fresh.id), vec![current.id], "assigns by default the era that contains today");

    // An inbox task belongs to no project, hence to no time axis: unassigned.
    let inbox = s.add_task(task("inbox", None)).unwrap();
    assert!(assigned(&s, inbox.id).is_empty());

    // The default is not binding — it can be cleared, and it can be overwritten with another window.
    assert!(s.unset_task_dimension_value(fresh.id, current.id).unwrap());
    assert!(assigned(&s, fresh.id).is_empty());
    let re = s.add_task(task("上書き", Some(proj.id))).unwrap();
    s.set_task_dimension_value(re.id, past.id).unwrap();
    assert_eq!(assigned(&s, re.id), vec![past.id], "single-select, so the default is replaced");

    // The default assignment rides the write seam too: committed to the truth source per operation.
    drop(s);
    let reopened = Store::open_at(Paths::at(dir.clone())).unwrap();
    assert_eq!(assigned(&reopened, fresh.id), Vec::<i64>::new());
    assert_eq!(assigned(&reopened, re.id), vec![past.id]);

    fs::remove_dir_all(&dir).ok();
}

/// Classification named at creation lands with the task, in one transaction — and it **wins over the
/// time-axis default**: the axis the caller named is theirs, and the default fills only the axis they
/// left alone. Naming the time axis itself is the case that matters, because the default would otherwise
/// have written a period the task never belonged to and then replaced it.
#[test]
fn a_value_named_at_creation_lands_with_the_task_and_beats_the_default() {
    use crate::model::DimensionRole;
    use crate::ops::dimension::NewDimension;

    let (mut s, dir) = fresh_store("create-with-dimensions");
    let proj = s.project_add(project("PJ")).unwrap();

    let assigned = |s: &Store, task_id: i64| -> Vec<i64> {
        let mut ids: Vec<i64> =
            crate::store_engine::read::task_dimension_assignments(s.engine.conn(), task_id)
                .unwrap()
                .into_iter()
                .map(|(_dimension_id, value_id)| value_id)
                .collect();
        ids.sort_unstable();
        ids
    };

    let category = s
        .dimension_add(proj.id, NewDimension { name: "カテゴリー".into(), ..NewDimension::default() })
        .unwrap();
    let bug = s.dimension_value_add(category.id, "バグ", None).unwrap();
    let area = s
        .dimension_add(proj.id, NewDimension { name: "領域".into(), ..NewDimension::default() })
        .unwrap();
    let core = s.dimension_value_add(area.id, "コア", None).unwrap();
    let axis = s
        .dimension_add(
            proj.id,
            NewDimension { name: "時代".into(), role: DimensionRole::TimeAxis, ordered: true, ..NewDimension::default() },
        )
        .unwrap();
    let past = s.dimension_value_add(axis.id, "開発期", Some((None, Some(crate::time::today().pred_opt().unwrap())))).unwrap();
    let current = s.dimension_value_add(axis.id, "運用第1期", Some((Some(crate::time::today()), None))).unwrap();

    // Two axes at once, neither of them the time axis: both land, and the default still fills the era.
    let filed = s.add_task_with_dimensions(task("分類つき", Some(proj.id)), &[bug.id, core.id]).unwrap();
    let mut want = vec![bug.id, core.id, current.id];
    want.sort_unstable();
    assert_eq!(assigned(&s, filed.id), want, "what was named, plus the default on the axis nobody named");

    // The time axis, named outright: the past era stands and today's is never written.
    let backdated = s.add_task_with_dimensions(task("過去の時代", Some(proj.id)), &[past.id]).unwrap();
    assert_eq!(assigned(&s, backdated.id), vec![past.id], "the named era wins; the default does not overwrite it");

    // It commits with the task, per operation — reopen as another process would and read it back.
    drop(s);
    let reopened = Store::open_at(Paths::at(dir.clone())).unwrap();
    assert_eq!(assigned(&reopened, backdated.id), vec![past.id]);
    assert_eq!(assigned(&reopened, filed.id), want);

    fs::remove_dir_all(&dir).ok();
}

fn new_decision(title: &str, project_id: i64) -> crate::ops::decision::NewDecision {
    crate::ops::decision::NewDecision { title: title.into(), body: String::new(), project_id }
}

/// **One logical operation, one transaction**, for decisions too: each write commits to the truth
/// source on its own. A decision's conversational number lives in its own space, separate from tasks',
/// and is taken from `next_id` read **inside** that transaction, so consecutive adds number
/// densely.
#[test]
fn decision_writes_commit_per_operation_and_number_from_the_truth_source() {
    let (mut s, dir) = fresh_store("seam-decision");
    let pid = s.project_add(project("P")).unwrap().id;

    let d1 = s.add_decision(new_decision("first", pid)).unwrap();
    assert_eq!(d1.id, 1);
    let d2 = s.add_decision(new_decision("second", pid)).unwrap();
    assert_eq!(d2.id, 2, "numbering comes from the high-water mark in the source of truth");

    // Drop the store, reopen as another process would, and find both decisions still there.
    drop(s);
    let re = Store::open_at(Paths::at(dir.clone())).unwrap();
    let mut numbers: Vec<i64> =
        hydrated(&re).decisions.iter().map(|d| d.id).collect();
    numbers.sort_unstable();
    assert_eq!(numbers, vec![1, 2], "both are committed");
    fs::remove_dir_all(&dir).ok();
}

/// `decision::delete` removes the decision and its task links in **one transaction**. Leave one
/// behind and the store keeps a dangling link. Ask the truth source directly that both are gone.
#[test]
fn deleting_a_decision_removes_its_task_links_together() {
    let (mut s, dir) = fresh_store("seam-decision-delete");
    let pid = s.project_add(project("P")).unwrap().id;
    let t = s.add_task(task("linked", Some(pid))).unwrap();
    let d = s.add_decision(new_decision("doomed", pid)).unwrap();
    s.link_decision(d.id, t.id).unwrap();
    s.delete_decision(d.id, crate::model::ActorKind::Human).unwrap();

    let conn = s.engine.conn();
    let live_decisions: i64 = conn
        .query_row("SELECT count(*) FROM decision WHERE id = ?1", [&d.id], |r| r.get(0))
        .unwrap();
    let live_links: i64 = conn
        .query_row(
            "SELECT count(*) FROM decision_task_link WHERE decision_id = ?1",
            [&d.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!((live_decisions, live_links), (0, 0), "both the decision and its links go in the same transaction");
    fs::remove_dir_all(&dir).ok();
}

/// Appending an attachment is a read-then-write on `order_key`, and that read goes to the truth
/// source. Two attachments on the same target taking the same key would destroy their ordering, so
/// pin that the second key sorts after the first.
#[test]
fn attachment_order_key_appends_from_the_truth_source() {
    let (mut s, dir) = fresh_store("seam-attachment");
    let t = s.add_task(task("has attachments", None)).unwrap();
    let target = crate::model::AttachmentTarget::Task;

    let first = s.attach_url(target, t.id, "https://example.com/a", None, crate::model::ActorKind::Ai).unwrap();
    let second = s.attach_url(target, t.id, "https://example.com/b", None, crate::model::ActorKind::Ai).unwrap();
    assert!(first.order_key < second.order_key, "the tail key goes after MAX(order_key) in the source of truth");

    drop(s);
    let re = Store::open_at(Paths::at(dir.clone())).unwrap();
    assert_eq!(re.attachments_for_target(target, t.id).unwrap().len(), 2, "both are committed");
    fs::remove_dir_all(&dir).ok();
}

/// A system event lands **only** in the file ledger. Each ledger line carries its own project: a file
/// cannot be joined against the DB, so without that field there is nothing to filter a project by.
#[test]
fn a_system_event_lands_in_the_ledger_beside_the_store() {
    let (mut s, dir) = fresh_store("activity-ledger");
    let proj = s
        .project_add(crate::ops::project::NewProject {
            name: "PJ".into(),
            view: View::List,
            notes: String::new(),
            color: None,
        })
        .unwrap();
    let t = s.add_task(task("filed", Some(proj.id))).unwrap();

    let event = s
        .add_system_event(
            crate::model::ActorKind::Ai,
            t.id,
            crate::activity_log::event::task_status_changed("todo", "in_progress"),
        )
        .unwrap();

    let text = fs::read_to_string(&s.paths.activity_file).expect("the ledger sits beside the store");
    let line: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    assert_eq!(line["v"], serde_json::json!(2));
    assert_eq!(line["id"].as_i64(), Some(event.id), "the row id is the activity sequence number the DB assigned");
    assert_eq!(line["actor"], serde_json::json!("ai"));
    assert_eq!(line["task"].as_i64(), Some(t.id));
    assert_eq!(line["project"].as_i64(), Some(proj.id), "a ledger row carries its own project");
    assert_eq!(line["event"]["kind"], serde_json::json!("task.status_changed"));

    // For an unfiled (inbox) task, the project field is null.
    let inbox = s.add_task(task("unfiled", None)).unwrap();
    s.add_system_event(crate::model::ActorKind::Human, inbox.id, crate::activity_log::event::task_created("unfiled")).unwrap();
    let text = fs::read_to_string(&s.paths.activity_file).unwrap();
    let last: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(last["project"], serde_json::Value::Null);
    assert_eq!(last["actor"], serde_json::json!("human"));

    fs::remove_dir_all(&dir).ok();
}

/// Read the ledger's lines in order — one line, one JSON object.
fn ledger(s: &Store) -> Vec<serde_json::Value> {
    fs::read_to_string(&s.paths.activity_file)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).expect("a whole line of JSON"))
        .collect()
}

/// A deletion leaves its only trace in the **ledger**. The row is gone from the truth source, so the
/// ledger line is the last thing that remembers what the task was called — which is why the line
/// carries its own title and project.
#[test]
fn deleting_a_task_leaves_its_only_trace_in_the_ledger() {
    let (mut s, dir) = fresh_store("ledger-task-deleted");
    let pid = s.project_add(project("PJ")).unwrap().id;
    let t = s.add_task(task("doomed", Some(pid))).unwrap();
    s.add_system_event(crate::model::ActorKind::Ai, t.id, crate::activity_log::event::task_created("doomed")).unwrap();

    s.delete_task(t.id, crate::model::ActorKind::Human).unwrap();

    let lines = ledger(&s);
    let last = lines.last().unwrap();
    assert_eq!(last["event"]["kind"], serde_json::json!("task.deleted"));
    assert_eq!(last["event"]["title"], serde_json::json!("doomed"), "a deleted row's name is held only by the ledger");
    assert_eq!(last["task"].as_i64(), Some(t.id));
    assert_eq!(last["project"].as_i64(), Some(pid));
    assert_eq!(last["actor"], serde_json::json!("human"));
    assert_eq!(last["decision"], serde_json::Value::Null);
    assert!(
        last["id"].as_i64() > lines[lines.len() - 2]["id"].as_i64(),
        "the activity sequence keeps advancing after a deletion (a deleted event's id is not reused)"
    );

    fs::remove_dir_all(&dir).ok();
}

/// A deletion leaves no DB row, but it **still spends an activity sequence number**. Skip the
/// increment and two consecutive deletions produce two lines with the same id, breaking the
/// `(at, source, id)` tie-break that gives the ledger and the comment table a single total order.
#[test]
fn row_less_events_still_spend_the_activity_sequence() {
    let (mut s, dir) = fresh_store("ledger-seq");
    let a = s.add_task(task("a", None)).unwrap();
    let b = s.add_task(task("b", None)).unwrap();

    s.delete_task(a.id, crate::model::ActorKind::Ai).unwrap();
    s.delete_task(b.id, crate::model::ActorKind::Ai).unwrap();
    // A system event after those deletions carries on from where they left off.
    let c = s.add_task(task("c", None)).unwrap();
    s.add_system_event(crate::model::ActorKind::Ai, c.id, crate::activity_log::event::task_created("c")).unwrap();

    let ids: Vec<i64> = ledger(&s).iter().map(|l| l["id"].as_i64().unwrap()).collect();
    assert_eq!(ids, vec![1, 2, 3], "an event with no row still uses one sequence number");
    fs::remove_dir_all(&dir).ok();
}

/// Deleting a project writes one line, which reports the subtree it took with it as counts. A line
/// per casualty would bury the ledger under a single delete.
#[test]
fn deleting_a_project_says_how_much_went_with_it() {
    let (mut s, dir) = fresh_store("ledger-project-deleted");
    let pid = s.project_add(project("PJ")).unwrap().id;
    s.add_task(task("a", Some(pid))).unwrap();
    s.add_task(task("b", Some(pid))).unwrap();
    s.add_decision(new_decision("D", pid)).unwrap();

    s.project_delete(pid, crate::model::ActorKind::Ai).unwrap();

    let lines = ledger(&s);
    assert_eq!(lines.len(), 1, "one row per project");
    assert_eq!(lines[0]["event"]["kind"], serde_json::json!("project.deleted"));
    assert_eq!(lines[0]["event"]["name"], serde_json::json!("PJ"));
    assert_eq!(lines[0]["event"]["tasks"], serde_json::json!(2));
    assert_eq!(lines[0]["event"]["decisions"], serde_json::json!(1));
    assert_eq!(lines[0]["project"].as_i64(), Some(pid));
    assert_eq!(lines[0]["task"], serde_json::Value::Null);

    fs::remove_dir_all(&dir).ok();
}

/// Deleting a decision names its target in the `decision` field. The line schema has a field per kind,
/// so events about things other than tasks can be filtered without unpacking the payload.
#[test]
fn deleting_a_decision_names_it_in_the_decision_key() {
    let (mut s, dir) = fresh_store("ledger-decision-deleted");
    let pid = s.project_add(project("PJ")).unwrap().id;
    let d = s.add_decision(new_decision("doomed", pid)).unwrap();

    s.delete_decision(d.id, crate::model::ActorKind::Ai).unwrap();

    let lines = ledger(&s);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["event"]["kind"], serde_json::json!("decision.deleted"));
    assert_eq!(lines[0]["event"]["title"], serde_json::json!("doomed"));
    assert_eq!(lines[0]["decision"].as_i64(), Some(d.id));
    assert_eq!(lines[0]["project"].as_i64(), Some(pid), "a decision belongs to a project (filtering works)");
    assert_eq!(lines[0]["task"], serde_json::Value::Null);

    fs::remove_dir_all(&dir).ok();
}
