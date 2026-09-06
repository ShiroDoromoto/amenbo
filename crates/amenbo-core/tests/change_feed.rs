//! The change feed: what a committed transaction touched, as the GUI's invalidation source. These
//! tests pin the two properties the design rests on — **nothing leaks** (every record row a mutation
//! touches shows up in the feed, down to the last child of a subtree delete and the rows only SQLite
//! knows about; this is what a hand-written emit at each write site cannot promise, and why the
//! collection is an `update_hook`) and **nothing lies** (the feed is written
//! inside the operation's own transaction, so a rolled-back batch leaves no rows behind, and no row
//! appears for a change the truth source does not have). They exercise the feed through the **public
//! ops** (`Store::…`), not by poking the engine: a mutation added tomorrow goes through the same seam,
//! so it inherits the feed without anyone remembering to.

use std::collections::HashSet;

use amenbo_core::config::Paths;
use amenbo_core::model::ActorKind;
use amenbo_core::store_engine::read::{self, FeedRow, FeedSlice};
use amenbo_core::Store;

fn temp_store() -> Store {
    let base = amenbo_scratch::scratch("feed");
    Store::open_at(Paths::at(base)).unwrap()
}

fn feed(store: &Store, after: i64) -> Vec<FeedRow> {
    match read::changes_since(store.read_model().conn(), after, 10_000, None).unwrap() {
        FeedSlice::Changes { rows, .. } => rows,
        FeedSlice::Gap => panic!("the cursor was still in the feed's window"),
    }
}

fn new_project(name: &str) -> amenbo_core::ops::project::NewProject {
    amenbo_core::ops::project::NewProject {
        name: name.to_string(),
        view: amenbo_core::model::View::Board,
        notes: String::new(),
        color: None,
    }
}

fn new_task(title: &str, project_id: i64) -> amenbo_core::ops::task::NewTask {
    amenbo_core::ops::task::NewTask {
        title: title.to_string(),
        project_id: Some(project_id),
        due_on: None,
        start_on: None,
        priority: None,
        notes: String::new(),
        created_by_kind: Some(ActorKind::Ai),
        at_binding_id: None,
    }
}

/// File a task and finish creating it — **both stages** (`AMB-D-554`). A creation lands unfinished, and a
/// task still being created cannot be reserved, so a test that goes on to move its status has to close the
/// creation the way the surfaces do.
fn filed(store: &mut Store, input: amenbo_core::ops::task::NewTask) -> i64 {
    let id = store.add_task(input).unwrap().id;
    store.finish_task_creation(id, ActorKind::Human).unwrap();
    id
}

/// The plain case: a mutation puts its row in the feed, with the dataset, the id the reader knows
/// (the row's primary key), and what happened to it.
#[test]
fn a_committed_mutation_names_the_row_it_changed() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;

    let head = read::change_feed_head(store.read_model().conn()).unwrap();
    let task = filed(&mut store, new_task("タスク", project));

    let rows = feed(&store, head);
    assert!(
        rows.iter().any(|r| r.dataset == "task" && r.row_id == task && r.op == "insert"),
        "the created task is in the feed: {rows:?}"
    );

    let head = read::change_feed_head(store.read_model().conn()).unwrap();
    store.set_task_status(task, amenbo_core::model::TaskStatus::InProgress, ActorKind::Ai).unwrap();
    let rows = feed(&store, head);
    assert!(
        rows.iter().any(|r| r.dataset == "task" && r.row_id == task && r.op == "update"),
        "the status change is in the feed: {rows:?}"
    );
}

/// **What the `update_hook` buys.** Deleting a task deletes its comments, its dependency edges, its
/// dimension assignments and its decision links — many rows from one call. An emit written by hand at
/// the call site would have to name each of them or tell a reader "the task is gone" while leaving it
/// showing a comment count that no longer exists. The hook reports what the statement actually touched,
/// so the feed cannot fall behind the sweep.
#[test]
fn a_subtree_delete_reports_every_child_row() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = store.add_task(new_task("消えるタスク", project)).unwrap().id;
    let blocker = store.add_task(new_task("前提", project)).unwrap().id;
    let comment = store.add_task_comment(task, ActorKind::Ai, "コメント").unwrap().id;
    store.depend_task(task, blocker, Some(ActorKind::Ai)).unwrap();

    let head = read::change_feed_head(store.read_model().conn()).unwrap();
    store.delete_task(task, ActorKind::Ai).unwrap();
    let rows = feed(&store, head);

    let deleted: HashSet<(String, i64)> = rows
        .iter()
        .filter(|r| r.op == "delete")
        .map(|r| (r.dataset.clone(), r.row_id))
        .collect();
    assert!(deleted.contains(&("task".to_string(), task)), "the task itself: {rows:?}");
    assert!(
        deleted.contains(&("task_comment".to_string(), comment)),
        "its comment went with it, and the feed says so: {rows:?}"
    );
    assert!(
        rows.iter().any(|r| r.dataset == "task_dependency" && r.op == "delete"),
        "so did the dependency edge naming it: {rows:?}"
    );
}

/// The feed rides inside the operation's transaction, so a batch that never commits leaves nothing —
/// the feed can never claim a change the truth source does not have.
#[test]
fn a_rolled_back_batch_leaves_no_trace_in_the_feed() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let head = read::change_feed_head(store.read_model().conn()).unwrap();

    // A mutation that fails: an empty title is refused by the op, after the transaction is open.
    let err = store.add_task(new_task("", project));
    assert!(err.is_err(), "the op refuses an empty title");

    assert!(feed(&store, head).is_empty(), "a batch that never committed wrote no feed rows");

    // And the abandoned rows are not attributed to the next transaction either.
    let task = store.add_task(new_task("次のタスク", project)).unwrap().id;
    let rows = feed(&store, head);
    assert!(
        rows.iter().all(|r| r.row_id == task || r.dataset != "task"),
        "only the committed task is named: {rows:?}"
    );
}

/// One operation, one instruction per row it touched. A record is written column by column, so the hook
/// hears about the same task once per field — but the reader has one thing to do either way (re-read
/// that one task), so the repeats are collapsed inside the transaction. Without this the feed grows several
/// times faster than the work it describes, and its bound arrives that much sooner.
#[test]
fn one_operation_writes_one_feed_row_per_row_it_touched() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = filed(&mut store, new_task("タスク", project));

    let head = read::change_feed_head(store.read_model().conn()).unwrap();
    store.set_task_status(task, amenbo_core::model::TaskStatus::InProgress, ActorKind::Ai).unwrap();

    let rows = feed(&store, head);
    let on_task: Vec<&FeedRow> = rows.iter().filter(|r| r.dataset == "task").collect();
    assert_eq!(on_task.len(), 1, "the status change touched several columns, but says one thing: {rows:?}");
    assert_eq!(on_task[0].op, "update");
}

/// The feed carries the instruction and nothing else: a task's notes — the most private text in the
/// store — must not be readable out of `change_feed`.
#[test]
fn the_feed_carries_no_values_only_ids() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let mut task = new_task("タイトル", project);
    task.notes = "秘密のメモ".to_string();
    store.add_task(task).unwrap();

    let conn = store.read_model().conn();
    let dumped: String = conn
        .query_row(
            "SELECT COALESCE(GROUP_CONCAT(dataset || ':' || row_id || ':' || op), '') FROM change_feed",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!dumped.contains("秘密"), "no body text rides in the feed");
    // The table has no room for one: the instruction — id, dataset, row_id, op — and the window it
    // belongs to, which says who may be told and nothing about what the record holds.
    let cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(change_feed)").unwrap();
        let out = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        out
    };
    assert_eq!(cols, vec!["id", "dataset", "row_id", "op", "project"]);
}

/// The shadow tables SQLite touches on its own — `sqlite_sequence`, `store_meta` and `change_feed`
/// itself — are not records a reader can re-read by id. They stay out of the feed, or every write would
/// produce a burst of rows naming tables the GUI has never heard of (and the feed's own inserts would
/// feed on themselves, without end).
#[test]
fn the_feed_ignores_the_tables_that_are_not_records() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let head = read::change_feed_head(store.read_model().conn()).unwrap();
    store.add_task(new_task("全文検索に載るタスク", project)).unwrap();

    let datasets: HashSet<String> = feed(&store, head).into_iter().map(|r| r.dataset).collect();
    for shadow in ["sqlite_sequence", "change_feed", "store_meta"] {
        assert!(!datasets.contains(shadow), "{shadow} is not a record dataset: {datasets:?}");
    }
    assert!(datasets.contains("task"), "the record itself is there: {datasets:?}");
}

/// A hard-erase names the row it destroyed: its DELETE / in-place overwrite goes through the same seam
/// as every other record write, so the commit carries it into the feed — and the VACUUM that follows
/// (which cannot run inside a transaction) runs after that commit.
#[test]
fn a_hard_erase_names_the_row_it_destroyed() {
    use amenbo_core::store::HardEraseTarget;

    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = store.add_task(new_task("タスク", project)).unwrap().id;
    let comment = store.add_task_comment(task, ActorKind::Ai, "消される秘密").unwrap().id;

    let head = read::change_feed_head(store.read_model().conn()).unwrap();
    store.hard_erase(&[HardEraseTarget::TaskComment { id: comment }]).unwrap();

    let rows = feed(&store, head);
    assert!(
        rows.iter().any(|r| r.dataset == "task_comment" && r.row_id == comment && r.op == "delete"),
        "the erased comment is named in the feed: {rows:?}"
    );
    assert!(
        rows.iter().all(|r| r.dataset != "change_feed"),
        "and the VACUUM that follows the commit did not feed the feed on itself: {rows:?}"
    );
}

/// The feed is a **window, not a history**: rows past the retention are trimmed, so the table stops
/// growing.
#[test]
fn the_feed_is_bounded_and_stops_growing() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;

    // Enough operations to run well past the retention (each writes a few rows).
    let task = filed(&mut store, new_task("回転させるタスク", project));
    for i in 0..6_000 {
        let status = if i % 2 == 0 {
            amenbo_core::model::TaskStatus::InProgress
        } else {
            amenbo_core::model::TaskStatus::Todo
        };
        store.set_task_status(task, status, ActorKind::Ai).unwrap();
    }

    let rows: i64 = store
        .read_model()
        .conn()
        .query_row("SELECT COUNT(*) FROM change_feed", [], |r| r.get(0))
        .unwrap();
    let ceiling = amenbo_core::store_engine::CHANGE_FEED_RETAIN + 500;
    assert!(
        rows <= ceiling,
        "the feed is trimmed back to its retention (held {rows}, ceiling {ceiling})"
    );
    // What it kept is the newest end: the ids climbed past the retention, and the oldest survivor is
    // near the head, not at 1 — a reader that is up to date still finds its place.
    let head = read::change_feed_head(store.read_model().conn()).unwrap();
    let oldest: i64 = store
        .read_model()
        .conn()
        .query_row("SELECT MIN(id) FROM change_feed", [], |r| r.get(0))
        .unwrap();
    assert!(head > amenbo_core::store_engine::CHANGE_FEED_RETAIN, "ids kept climbing: {head}");
    assert!(oldest > 1, "the old end was trimmed away (oldest {oldest}, head {head})");
}

/// A reader the truncation has outrun is **told so**. This is the whole reason truncation records how far
/// it reached: an empty answer to "what changed since 3?" is indistinguishable from "nothing changed",
/// and a GUI that believes it freezes with a stale screen. It gets a gap instead, and re-reads.
#[test]
fn a_cursor_the_truncation_outran_is_declared_a_gap_not_an_empty_answer() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = filed(&mut store, new_task("タスク", project));

    // A reader that saw the very first change, then went away for a long time.
    let stale_cursor = 1;
    for i in 0..6_000 {
        let status = if i % 2 == 0 {
            amenbo_core::model::TaskStatus::InProgress
        } else {
            amenbo_core::model::TaskStatus::Todo
        };
        store.set_task_status(task, status, ActorKind::Ai).unwrap();
    }

    let conn = store.read_model().conn();
    assert_eq!(
        read::changes_since(conn, stale_cursor, 100, None).unwrap(),
        FeedSlice::Gap,
        "the changes it never saw are gone, and it is told rather than reassured"
    );

    // A reader that is current still gets its changes, not a gap.
    let head = read::change_feed_head(conn).unwrap();
    store.set_task_status(task, amenbo_core::model::TaskStatus::Done, ActorKind::Ai).unwrap();
    let conn = store.read_model().conn();
    match read::changes_since(conn, head, 100, None).unwrap() {
        FeedSlice::Changes { rows, more } => {
            assert!(!more, "one operation fits in a page");
            assert!(rows.iter().any(|r| r.dataset == "task" && r.row_id == task));
        }
        FeedSlice::Gap => panic!("a current cursor is not a gap"),
    }
}

/// A page shorter than the feed says so, so a reader that has fallen behind drains it in bounded reads
/// instead of materialising an unknown number of rows at once.
#[test]
fn a_short_page_says_there_is_more() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = filed(&mut store, new_task("タスク", project));
    let head = read::change_feed_head(store.read_model().conn()).unwrap();
    for _ in 0..5 {
        store.set_task_status(task, amenbo_core::model::TaskStatus::InProgress, ActorKind::Ai).unwrap();
        store.set_task_status(task, amenbo_core::model::TaskStatus::Todo, ActorKind::Ai).unwrap();
    }

    match read::changes_since(store.read_model().conn(), head, 3, None).unwrap() {
        FeedSlice::Changes { rows, more } => {
            assert_eq!(rows.len(), 3, "the page is the size that was asked for");
            assert!(more, "and it says the feed holds more");
        }
        FeedSlice::Gap => panic!("nothing has been trimmed here"),
    }
}

/// The bound holds for a **CLI-only store**, where the amortised trim would never get its chance: every
/// command is a fresh process that writes a few rows and exits, so a counter starting at zero would sit
/// far below its threshold forever while the feed grew without limit. A fresh engine starts its counter
/// already due, so the first write of each process cuts the feed — the shape of the client cannot decide
/// whether the feed is bounded.
#[test]
fn the_bound_holds_when_every_write_is_a_new_process() {
    let base = amenbo_scratch::scratch("feed-cli");
    let paths = Paths::at(base);

    // Grow the feed past the retention, then close the store — as a run of CLI commands leaves it.
    let (project, task) = {
        let mut store = Store::open_at(paths.clone()).unwrap();
        let project = store.project_add(new_project("PJ")).unwrap().id;
        let task = filed(&mut store, new_task("タスク", project));
        (project, task)
    };
    {
        let mut store = Store::open_at(paths.clone()).unwrap();
        for i in 0..6_000 {
            let status = if i % 2 == 0 {
                amenbo_core::model::TaskStatus::InProgress
            } else {
                amenbo_core::model::TaskStatus::Todo
            };
            store.set_task_status(task, status, ActorKind::Ai).unwrap();
        }
    }
    // A fresh process — one `amenbo task add` — finds the feed over its window and cuts it back on the
    // write.
    let mut store = Store::open_at(paths).unwrap();
    store.add_task(new_task("次のコマンド", project)).unwrap();

    let rows: i64 = store
        .read_model()
        .conn()
        .query_row("SELECT COUNT(*) FROM change_feed", [], |r| r.get(0))
        .unwrap();
    let ceiling = amenbo_core::store_engine::CHANGE_FEED_RETAIN + 500;
    assert!(rows <= ceiling, "the write trimmed the feed back (held {rows}, ceiling {ceiling})");
}

/// **A command that only reads writes nothing** (`AMB-D-857`). The feed's bound rides the first write of
/// each process, not the open, because an open happens before the command it was opened for has said
/// what it wants: a cut made there puts a `DELETE` and the write lock behind `amenbo task list`, on a
/// store that command never changes. The cut still comes — one write later — and this pins that the
/// reading open is not what makes it.
#[test]
fn a_reading_open_does_not_cut_the_feed() {
    let base = amenbo_scratch::scratch("feed-read-only");
    let paths = Paths::at(base);

    let (project, task) = {
        let mut store = Store::open_at(paths.clone()).unwrap();
        let project = store.project_add(new_project("PJ")).unwrap().id;
        let task = filed(&mut store, new_task("タスク", project));
        (project, task)
    };
    {
        let mut store = Store::open_at(paths.clone()).unwrap();
        for i in 0..6_000 {
            let status = if i % 2 == 0 {
                amenbo_core::model::TaskStatus::InProgress
            } else {
                amenbo_core::model::TaskStatus::Todo
            };
            store.set_task_status(task, status, ActorKind::Ai).unwrap();
        }
    }
    // Put the feed **over its window on purpose**, so a build that trimmed at open would have something
    // to trim. The first of these writes cuts the feed back to exactly the retention; the two after it
    // push past it again, and nothing trims until the next process writes.
    {
        let mut store = Store::open_at(paths.clone()).unwrap();
        for n in 0..3 {
            store.add_task(new_task(&format!("押し出す{n}"), project)).unwrap();
        }
    }
    let span = |store: &Store| -> (i64, i64) {
        let conn = store.read_model().conn();
        let rows = conn.query_row("SELECT COUNT(*) FROM change_feed", [], |r| r.get(0)).unwrap();
        let oldest = conn.query_row("SELECT MIN(id) FROM change_feed", [], |r| r.get(0)).unwrap();
        (rows, oldest)
    };
    let before = {
        let store = Store::open_read_at(paths.clone()).unwrap();
        let seen = span(&store);
        // Over by more than one row: an open-time trim reads the span as `MAX - MIN`, so a feed holding
        // exactly one row past the retention would not move it either, and the reading below would go
        // green over a build that trims at open.
        assert!(
            seen.0 > amenbo_core::store_engine::CHANGE_FEED_RETAIN + 1,
            "the feed has to be over its window for this to be a test (held {})",
            seen.0
        );
        seen
    };

    // The reading command: a full open, and not one write through it.
    {
        let store = Store::open_at(paths.clone()).unwrap();
        let _ = store.read_model();
    }

    let after = {
        let store = Store::open_read_at(paths).unwrap();
        span(&store)
    };
    assert_eq!(before, after, "an open that read nothing but rows left the feed as it found it");
}
