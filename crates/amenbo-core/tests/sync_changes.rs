//! Reading the ledger from a cursor: the second of the two roads a carrier takes off this device
//! (`AMB-D-582`). The version says *whether* to come; this says *what*, so a carrier that already holds a
//! copy re-reads the records that moved instead of the window entire.
//!
//! Four properties are what these pin, and each is one a carrier's copy goes wrong without:
//!
//! - **Only the unread, and in order.** A cursor read hands back what happened after it, oldest first —
//!   what a copy outside has to apply in that order — and never what the reader already saw.
//! - **A deletion is named as one.** There is nothing left to read back, so the `op` is the whole of how
//!   the copy learns to drop it. "Re-read it and notice it is missing" is not a road: the carrier would
//!   have to ask after every record it holds, on every pass.
//! - **It closes on the window.** A record next door is not named, not counted, and not hinted at —
//!   the same strictness the whole-device roads are held to (`AMB-T-2789` / `AMB-T-2791`).
//! - **A gap is said out loud.** A cursor outside what the ledger can speak for — fallen behind its
//!   window, or ahead of anything it has ever reached — is told so, because an empty page is
//!   indistinguishable from nothing having happened, and a copy that reads it as such sits stale
//!   believing it is current.
//!
//! Like the sibling suites they go through the **public ops** (`Store::…`) rather than poking the engine,
//! so a mutation added tomorrow inherits all of this without anyone remembering to.

use amenbo_core::config::Paths;
use amenbo_core::model::ActorKind;
use amenbo_core::reach::Reach;
use amenbo_core::store::SyncChanges;
use amenbo_core::store_engine::read::FeedRow;
use amenbo_core::Store;

fn temp_store() -> Store {
    let base = amenbo_scratch::scratch("sync-changes");
    Store::open_at(Paths::at(base)).unwrap()
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
    }
}

/// File a task and finish creating it — both stages (`AMB-D-554`), the way the surfaces do.
fn filed(store: &mut Store, input: amenbo_core::ops::task::NewTask) -> i64 {
    let id = store.add_task(input).unwrap().id;
    store.finish_task_creation(id, ActorKind::Human).unwrap();
    id
}

/// A page read the way a carrier reads it — through the window it was launched to observe, which is the
/// only reach that asks about one project rather than the whole device. `with_reach` consumes, so this
/// reads through a clone of the open rather than narrowing the caller's.
fn read(store: &Store, project: i64, after: i64, limit: i64) -> SyncChanges {
    Store::open_at(store.paths.clone())
        .unwrap()
        .with_reach(Reach::window(project))
        .sync_changes(after, limit)
        .unwrap()
}

/// The whole of one window's unread changes, and the cursor it ends at. Panics on a gap — the tests that
/// are about a gap ask for it by name.
fn drained(store: &Store, project: i64, after: i64) -> (Vec<FeedRow>, i64) {
    match read(store, project, after, 10_000) {
        SyncChanges::Changes { rows, cursor, more } => {
            assert!(!more, "10_000 rows is the whole of any of these");
            (rows, cursor)
        }
        SyncChanges::Gap => panic!("the cursor was still inside the feed's window"),
    }
}

/// Where a carrier starts: everything up to now is read, so the next read is about what happens next.
fn caught_up(store: &Store, project: i64) -> i64 {
    drained(store, project, 0).1
}

/// The plain case, and the two halves of it that a copy outside depends on: the page holds what happened
/// after the cursor and nothing the reader already saw, and a second read from the cursor it was handed
/// back is empty. Draining twice must not deliver the same change twice — the copy would apply it again.
#[test]
fn a_cursor_hands_back_the_unread_and_then_nothing() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let cursor = caught_up(&store, project);

    let task = filed(&mut store, new_task("タスク", project));
    let (rows, next) = drained(&store, project, cursor);
    assert!(
        rows.iter().any(|r| r.dataset == "task" && r.row_id == task),
        "the task that was filed is named: {rows:?}",
    );
    assert!(next > cursor, "the cursor moved: {cursor} → {next}");

    // Nothing has happened since, so the read is empty and hands the same cursor straight back.
    let (again, held) = drained(&store, project, next);
    assert_eq!(again, Vec::new(), "the changes it already read do not come round again");
    assert_eq!(held, next, "an empty page hands back the cursor it was given");
}

/// The order is the order the changes were committed in — which is the order a copy outside has to apply
/// them in, since a comment's task has to exist before the comment does.
#[test]
fn the_changes_come_back_in_the_order_they_happened() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let cursor = caught_up(&store, project);

    let first = filed(&mut store, new_task("1件目", project));
    let second = filed(&mut store, new_task("2件目", project));
    let comment = store.add_task_comment(second, ActorKind::Ai, "コメント").unwrap().id;

    let (rows, _) = drained(&store, project, cursor);
    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let mut ordered = ids.clone();
    ordered.sort();
    assert_eq!(ids, ordered, "the page is oldest first: {rows:?}");

    let at = |dataset: &str, row: i64| {
        rows.iter()
            .position(|r| r.dataset == dataset && r.row_id == row)
            .unwrap_or_else(|| panic!("{dataset} {row} is not in the page: {rows:?}"))
    };
    assert!(at("task", first) < at("task", second), "the first task was filed first");
    assert!(
        at("task", second) < at("task_comment", comment),
        "the task exists before the comment on it does",
    );
}

/// A deletion is named as a deletion. There is nothing left to read back, so the `op` is the whole of how
/// the copy outside learns to drop the record — and a carrier that had to notice by re-reading every
/// record it holds would be asking the store about all of them on every pass.
#[test]
fn a_deletion_is_told_by_its_op_and_not_by_a_record_that_fails_to_read_back() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = filed(&mut store, new_task("消す", project));
    let cursor = caught_up(&store, project);

    store.delete_task(task, ActorKind::Ai).unwrap();

    let (rows, _) = drained(&store, project, cursor);
    let named: Vec<&FeedRow> =
        rows.iter().filter(|r| r.dataset == "task" && r.row_id == task).collect();
    assert_eq!(named.len(), 1, "the deleted task is named once: {rows:?}");
    assert_eq!(named[0].op, "delete", "and it is named as gone, not as changed");
}

/// The window holds. Everything the busy project next door does — filing, updating, commenting,
/// deleting — is invisible from this one: not the record, not its id, not a count of how many there were.
/// A hole in the cursor would be a leak of its own, so the ids handed back are checked to be dense within
/// what this window sees.
#[test]
fn not_one_record_from_outside_the_window_comes_through() {
    let mut store = temp_store();
    let mine = store.project_add(new_project("こちら")).unwrap().id;
    let theirs = store.project_add(new_project("よそ")).unwrap().id;
    let cursor = caught_up(&store, mine);

    let ours = filed(&mut store, new_task("こちらのタスク", mine));
    let churn = filed(&mut store, new_task("よそのタスク", theirs));
    store.set_task_status(churn, amenbo_core::model::TaskStatus::InProgress, ActorKind::Ai).unwrap();
    store.add_task_comment(churn, ActorKind::Ai, "よそのコメント").unwrap();
    store.delete_task(churn, ActorKind::Ai).unwrap();

    let (rows, _) = drained(&store, mine, cursor);
    let mut ids: Vec<i64> = rows.iter().filter(|r| r.dataset == "task").map(|r| r.row_id).collect();
    ids.dedup();
    assert_eq!(ids, vec![ours], "the window names its own task and no other: {rows:?}");
    assert!(
        !rows.iter().any(|r| r.dataset == "task_comment"),
        "the comment next door is not even counted: {rows:?}",
    );

    // And the mirror image: the other window sees its own churn and nothing of ours.
    let (theirs_rows, _) = drained(&store, theirs, cursor);
    assert!(
        theirs_rows.iter().all(|r| r.dataset != "task" || r.row_id != ours),
        "our task is not in their page either: {theirs_rows:?}",
    );
}

/// A task re-homed leaves one window and joins another, and **both** have to hear about it. The one it
/// left cannot learn it from the record — the record now names where it landed — so the change is
/// stamped with each window at the moment it happened, not looked up afterwards.
#[test]
fn re_homing_reaches_the_window_it_left_as_well_as_the_one_it_joined() {
    let mut store = temp_store();
    let from = store.project_add(new_project("元")).unwrap().id;
    let to = store.project_add(new_project("先")).unwrap().id;
    let task = filed(&mut store, new_task("引っ越す", from));
    let (from_cursor, to_cursor) = (caught_up(&store, from), caught_up(&store, to));

    store.move_task(task, Some(to), amenbo_core::ops::Position::Bottom, ActorKind::Ai).unwrap();

    for (window, cursor, whose) in [(from, from_cursor, "left"), (to, to_cursor, "joined")] {
        let (rows, _) = drained(&store, window, cursor);
        assert!(
            rows.iter().any(|r| r.dataset == "task" && r.row_id == task),
            "the window it {whose} was told: {rows:?}",
        );
    }
}

/// A cursor the feed's window has outrun is told so. The changes between are gone, and an empty page
/// would be read as "nothing changed" — the copy outside would sit stale believing it was current.
#[test]
fn a_cursor_the_feed_outran_is_a_gap_and_not_an_empty_page() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let stale = caught_up(&store, project);

    // Push the feed past its bound, so the truncation passes the cursor held above.
    let churn = filed(&mut store, new_task("回す", project));
    for i in 0..amenbo_core::store_engine::CHANGE_FEED_RETAIN + 600 {
        store.add_task_comment(churn, ActorKind::Ai, &format!("{i}")).unwrap();
    }

    assert_eq!(read(&store, project, stale, 100), SyncChanges::Gap, "the cursor is gone, and says so");

    // The current end of the feed is **not** a gap: caught up is not the same as fallen behind, and a
    // carrier that reads on every few seconds must not be sent for a snapshot every time.
    let head = store.sync_version().unwrap();
    let (rows, _) = drained(&store, project, head);
    assert_eq!(rows, Vec::new(), "a cursor at the head is served, and there is nothing after it");
}

/// A cursor the ledger has **never reached** is a gap as well — a position from another store's ledger,
/// or from this one before a `restore` wound it back. Read as an empty page it would swallow every change
/// until the feed climbed past it, silently and for as long as that took. A negative cursor is the same
/// answer from the other side: it sits below the floor.
#[test]
fn a_cursor_the_ledger_has_never_reached_is_a_gap_and_so_is_one_below_the_floor() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let _ = filed(&mut store, new_task("タスク", project));

    assert_eq!(read(&store, project, i64::MAX / 2, 100), SyncChanges::Gap, "far past the head");
    assert_eq!(read(&store, project, -1, 100), SyncChanges::Gap, "below the floor");
    // The device's own reach answers the same: it is the ledger that has not been there, not the window.
    assert_eq!(store.sync_changes(i64::MAX / 2, 100).unwrap(), SyncChanges::Gap);
}

/// **The retention boundary, exactly.** A cursor sitting on the last id the truncation removed has seen
/// everything that went, so it is not a gap — and what it is handed starts at the first row that
/// survived. One off either way is a real fault: declaring a gap here sends a carrier re-fetching the
/// window for nothing, and starting a row early delivers a change the reader already applied.
#[test]
fn a_cursor_on_the_last_id_the_truncation_took_is_not_a_gap_and_misses_nothing() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = filed(&mut store, new_task("回す", project));
    for i in 0..amenbo_core::store_engine::CHANGE_FEED_RETAIN + 600 {
        store.add_task_comment(task, ActorKind::Ai, &format!("{i}")).unwrap();
    }

    // Opening a store enforces the feed's bound as well, so let one open settle the cut before the
    // watermark is read: otherwise the read below trims again and moves the very boundary being tested.
    let _ = read(&store, project, 1, 1);

    // How far the truncation reached — the store's own watermark, read the way it was written.
    let cut: i64 = store
        .read_model()
        .conn()
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM store_meta WHERE key = 'change_feed_truncated_through'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(cut > 0, "the feed was actually trimmed");

    let (from_the_boundary, _) = drained(&store, project, cut);
    assert!(!from_the_boundary.is_empty(), "a cursor on the boundary is served, not turned away");
    assert_eq!(
        from_the_boundary.first().map(|r| r.id),
        Some(cut + 1),
        "and it starts at the first row the cut left standing — nothing skipped, nothing repeated",
    );

    // One id earlier is one change the reader never saw, and that *is* a gap.
    assert_eq!(read(&store, project, cut - 1, 100), SyncChanges::Gap, "one below the cut is gone");
}

/// The page is bounded, and says when it cut one short — a carrier that has been away pages through with
/// the cursor it is handed back rather than materialising the whole feed. The seam between two pages is
/// where a change would go missing or arrive twice, so the pages are checked to join up exactly.
#[test]
fn a_page_that_was_cut_short_says_so_and_the_next_one_joins_it_exactly() {
    let mut store = temp_store();
    let project = store.project_add(new_project("PJ")).unwrap().id;
    let task = filed(&mut store, new_task("タスク", project));
    let cursor = caught_up(&store, project);

    for i in 0..10 {
        store.add_task_comment(task, ActorKind::Ai, &format!("{i}")).unwrap();
    }
    let (whole, _) = drained(&store, project, cursor);
    assert!(whole.len() > 4, "there is something to page through: {}", whole.len());

    let mut paged = Vec::new();
    let mut at = cursor;
    loop {
        match read(&store, project, at, 3) {
            SyncChanges::Changes { rows, cursor, more } => {
                assert!(rows.len() <= 3, "the page is bounded: {rows:?}");
                paged.extend(rows);
                at = cursor;
                if !more {
                    break;
                }
            }
            SyncChanges::Gap => panic!("nothing has been trimmed here"),
        }
    }
    assert_eq!(paged, whole, "the pages join up: no change missed at a seam, and none delivered twice");
}

/// The device's own reach reads the whole feed — the human's, and the GUI's. A window narrows the same
/// road; it is not a different one.
#[test]
fn the_open_reach_reads_every_window_at_once() {
    let mut store = temp_store();
    let left = store.project_add(new_project("左")).unwrap().id;
    let right = store.project_add(new_project("右")).unwrap().id;
    let cursor = match store.sync_changes(0, 10_000).unwrap() {
        SyncChanges::Changes { cursor, .. } => cursor,
        SyncChanges::Gap => panic!("a fresh store has trimmed nothing"),
    };

    let here = filed(&mut store, new_task("左のタスク", left));
    let there = filed(&mut store, new_task("右のタスク", right));

    let rows = match store.sync_changes(cursor, 10_000).unwrap() {
        SyncChanges::Changes { rows, .. } => rows,
        SyncChanges::Gap => panic!("a fresh store has trimmed nothing"),
    };
    for task in [here, there] {
        assert!(
            rows.iter().any(|r| r.dataset == "task" && r.row_id == task),
            "the whole device sees both projects' changes: {rows:?}",
        );
    }
}
