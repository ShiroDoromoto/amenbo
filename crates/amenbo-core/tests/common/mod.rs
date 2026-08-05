//! Shared scale-seed builder for the read-hotpath **scaling bench** (`benches/read_hotpath.rs`) and
//! the **CI scaling guard** (`tests/read_scaling_guard.rs`), so the two observe and assert against
//! the exact same store shape. Why one builder, two consumers: the bench (`cargo bench`) is for
//! humans to *watch* how each read scales with N; the guard (`cargo test`) is the *executable*
//! version of the read budget — it fails CI red when a read regresses to O(total). Both must seed
//! identically or they would measure different things, so the seed lives here and is shared by
//! `#[path]` from the bench. Speed: the read-model is built by projecting an **in-memory** `Database`
//! straight into an in-memory `StoreEngine` ([`store_engine::record::put_database`]) — the very
//! mapping a real migration produces (so it cannot drift from production) — and never opens a
//! `Store`. That skips a transaction per row, so seeding 10k tasks stays cheap enough for a unit
//! test. The reads then run against `engine.conn()`, exactly as `Store::read_model()` serves them in
//! production (a borrowed engine read-model). Store shape (the invariant the guard relies on): a
//! **fixed-size** hot carve-out independent of N, plus N "bulk" background tasks that the selective
//! hot queries must *not* touch. So the O(result) reads stay flat as N grows, and a regression that
//! starts scanning the bulk shows up. The carve-out is what the word search's terms
//! ([`HOT_TERM_SCAN`] / [`HOT_TERM_INDEX`]) are written into, for the same reason: a fixed answer, over
//! a copy that grows.

#![allow(dead_code)] // each consumer (bench / guard) uses a different subset of these helpers.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use amenbo_core::model::{
    ActorKind, Database, Decision, DecisionStatus, Priority, Project, Task, TaskComment,
    TaskStatus, View,
};
use amenbo_core::store_engine::{self, StoreEngine};
use amenbo_core::query::Filter;
use amenbo_core::time::{self, Timestamp};

/// Size of the fixed hot carve-out — the set the selective reads (mailbox list, decisions) match,
/// regardless of N. Above [`amenbo_core::perf::COMPLEXITY_MIN_ROWS`]'s noise floor would defeat the
/// "ratio stays ~1" guard, so keep it small: a realistic mailbox / decision page.
pub const HOT_TASKS: usize = 50;
/// In-progress tasks assigned to the AI facet. Small and fixed: these are reserved (in_progress), so
/// status alone keeps them out of the `status:todo` mailbox.
pub const IN_PROGRESS_TASKS: usize = 5;

/// A word the word-search reads for, written **only** into the fixed hot carve-out's titles — so the
/// answer is the same fixed set at every N, exactly as the selective list reads' is, and a search whose
/// cost grows with the store is growing on the store rather than on its answer (`AMB-D-509`).
///
/// Two of them, because which path a term takes is decided by its length
/// ([`store_engine::search::TRIGRAM_MIN_CHARS`]) and the two paths are physically different reads: this
/// one is under it, so it takes the **scan** of the normalised copy.
pub const HOT_TERM_SCAN: &str = "qz";
/// The **index** half of [`HOT_TERM_SCAN`]: long enough for the trigram index to answer it. Sharing no
/// characters with the short one, so a search for either reaches the hot titles by its own path alone.
pub const HOT_TERM_INDEX: &str = "wombat";

/// A seeded read-model plus the ids a read needs to address its hot slice.
pub struct Seeded {
    /// In-memory read-model the reads run against (`.conn()` is the SQLite handle).
    pub engine: StoreEngine,
    /// The project holding the hot mailbox tasks and the hot decisions.
    pub project_id: i64,
    /// Total live tasks seeded (hot + in_progress + N bulk) — for the bench's throughput label.
    pub total_tasks: usize,
    tmp: PathBuf,
}

impl Drop for Seeded {
    fn drop(&mut self) {
        // The engine is in-memory and independent of the store dir once projected; reclaim the temp.
        let _ = std::fs::remove_dir_all(&self.tmp);
    }
}

fn temp_paths() -> (PathBuf, amenbo_core::config::Paths) {
    let base = amenbo_scratch::scratch("scale");
    let paths = amenbo_core::config::Paths::at(base.clone());
    (base, paths)
}

/// A live task with its project placement, pushed **directly** onto the database (O(1)) — see
/// [`seed`] for why this bypasses `ops::task::add`. `idx` drives the key (the id *is* the
/// conversational number) and a lexicographically-increasing order_key. Placement is task-held. The task starts
/// `todo`/unassigned; the caller tweaks the just-pushed row via `db.tasks.last_mut()` (O(1)).
fn push_task(db: &mut Database, pid: i64, idx: usize, title: String, pri: Priority) -> i64 {
    let now = Timestamp::now();
    let id = (idx + 1) as i64;
    db.tasks.push(Task {
        id,
        title,
        // status=Todo, subtype=Default, and every other field left to Default.
        created_by_kind: Some(ActorKind::Ai),
        priority: Some(pri),
        project_id: Some(pid),
        order_key: Some(format!("{idx:010}")),
        created_at: now,
        updated_at: now,
        ..Default::default()
    });
    id
}

/// Seed a store with `bulk` background tasks plus the fixed hot carve-out, project it into an
/// in-memory read-model, and return the handle. See the module doc for the shape and why.
///
/// Speed (this must seed 10k for a unit test): every record is built by **direct struct push**
/// (tasks via [`push_task`]; the project / comments / decisions inline below), never through the
/// `Store` write wrappers or `ops::*` — those open a `BEGIN IMMEDIATE` transaction per row and run
/// `next_number` (scans every task) on each add, which is O(N) per add and turns the bulk loop into
/// O(N²) (an ops-built 10k seed took ~36 min). Direct push plus a `db.tasks.last_mut()` tweak keeps
/// the whole loop O(N). `project_database` reads these fields verbatim, so the read-model is
/// identical to an ops-built one — what the reads see is the same, only the construction is cheap.
pub fn seed(bulk: usize) -> Seeded {
    let (tmp, paths) = temp_paths();

    // Build the database in memory and project it straight into the read-model — no `Store`, no
    // transaction per row, no reopen.
    let _ = paths;
    let mut database = Database::default();
    let db = &mut database;

    let now = Timestamp::now();
    let project_id: i64 = 1;
    db.projects.push(Project {
        id: project_id,
        name: "Scale".to_string(),
        default_view: View::Board,
        order_key: "m".to_string(),
        created_at: now,
        updated_at: now,
        ..Default::default()
    });
    let pid = project_id;
    let mut idx = 0;

    // Hot mailbox slice: todo + assigned to my AI + ready. Matches the mailbox filter; a fixed count
    // regardless of N. The two search terms ride on the title, so the word search has a fixed answer
    // here too — nothing outside this loop writes either of them.
    for i in 0..HOT_TASKS {
        push_task(
            db,
            pid,
            idx,
            format!("hot mailbox #{i} {HOT_TERM_SCAN} {HOT_TERM_INDEX}"),
            Priority::High,
        );
        idx += 1;
        let t = db.tasks.last_mut().unwrap();
        t.assignee_kind = Some(ActorKind::Ai);
    }

    // A few AI-assigned in-progress tasks: in_progress reserves them, so status alone keeps them
    // outside the `status:todo` mailbox.
    for i in 0..IN_PROGRESS_TASKS {
        push_task(db, pid, idx, format!("in-progress #{i}"), Priority::Medium);
        idx += 1;
        let t = db.tasks.last_mut().unwrap();
        t.assignee_kind = Some(ActorKind::Ai);
        t.status = TaskStatus::InProgress;
    }

    // Bulk background: done + unassigned, with one comment each. These grow with N and must NOT be
    // touched by the selective hot reads (done ⇒ excluded by the `status:todo,in_progress` index; the comment
    // feeds the store-wide activity read the bench observes). A direct `TaskComment` push is O(1), so
    // the loop stays O(N).
    for i in 0..bulk {
        let id = push_task(db, pid, idx, format!("bulk #{i}"), Priority::Low);
        idx += 1;
        let now = Timestamp::now();
        {
            let t = db.tasks.last_mut().unwrap();
            t.status = TaskStatus::Done;
            t.completed_at = Some(now);
        }
        db.task_comments.push(TaskComment {
            id: (i + 1) as i64,
            task_id: id,
            author_kind: Some(ActorKind::Ai),
            text: format!("bulk comment {i}"),
            created_at: now,
            updated_at: now,
            edited_at: None,
        });
    }

    // Hot decisions in the project (fixed count) for the paged decision read. A direct `Decision`
    // push with a pre-assigned id (the id *is* the number) keeps this a fixed, small,
    // O(1)-per-row loop — the count is fixed regardless of N, so it never touches the hot bulk loop.
    for i in 0..HOT_TASKS {
        let now = Timestamp::now();
        db.decisions.push(Decision {
            id: (i + 1) as i64,
            project_id: pid,
            title: format!("decision #{i}"),
            body: String::new(),
            status: DecisionStatus::Proposed,
            created_at: now,
            updated_at: now,
            ..Default::default()
        });
    }

    let total_tasks = HOT_TASKS + IN_PROGRESS_TASKS + bulk;

    let engine = StoreEngine::open_in_memory().unwrap();
    {
        let tx = engine.write().unwrap();
        store_engine::record::put_database(&tx, db).unwrap();
        tx.commit().unwrap();
    }

    Seeded { engine, project_id, total_tasks, tmp }
}

/// The GUI mailbox query as a [`amenbo_core::store_engine::TaskQuery`]-ready filter.
/// Selective: matches only the hot carve-out, never the bulk.
pub fn mailbox_filter() -> Filter {
    Filter::parse("assignee:me-ai status:todo ready:yes", time::today()).unwrap()
}

/// Run the mailbox `list_task_ids` read once, returning its (scanned=total_matched, returned=page)
/// pair — the complexity-ratio inputs.
pub fn run_mailbox_list(s: &Seeded) -> (usize, usize) {
    let filter = mailbox_filter();
    let q = store_engine::TaskQuery {
        reach: amenbo_core::reach::Reach::All,
        project_id: None,
        filter: &filter,
        sort: "priority",
        today: time::today(),
        limit: Some(HOT_TASKS),
        offset: None,
    };
    let page = store_engine::list_task_ids(s.engine.conn(), &q).unwrap();
    (page.total_matched, page.ids.len())
}

/// Run the **unfiltered, project-scoped board** `list_task_ids` read once (sort=`order`, the GUI's
/// default board page), returning its (scanned=total_matched, returned=page) pair. Unlike the
/// selective mailbox read this matches *every* task in the project, so it grows with N — its job in
/// the guard is to exercise the `order`-sort's per-row placement subquery and the placement
/// EXISTS over the whole set. Without the child-table FK indexes those subqueries table-scan per
/// task → O(N²) (a 10k board took ~13s); with them it is O(N log N) (~16ms). The guard bounds it
/// with an absolute budget (an O(N²) regression cannot be told from O(N log N) by a scanned/returned
/// ratio — both scan ~N — so wall-clock with generous headroom is the only net).
pub fn run_board_list(s: &Seeded) -> (usize, usize) {
    let filter = Filter::parse("", time::today()).unwrap();
    let q = store_engine::TaskQuery {
        reach: amenbo_core::reach::Reach::All,
        project_id: Some(s.project_id),
        filter: &filter,
        sort: "order",
        today: time::today(),
        limit: Some(50),
        offset: None,
    };
    let page = store_engine::list_task_ids(s.engine.conn(), &q).unwrap();
    (page.total_matched, page.ids.len())
}

/// Run the **count-only** `list_task_ids` read once — an unfiltered, project-scoped query with
/// `limit 0`, the shape `task_count_assigned` uses to fetch just a badge total (no page rows). It
/// matches the whole project, so `total_matched` ≈ N while the page is empty; returns its
/// (scanned=total_matched, returned=0) pair. The point is that returned=0 is *intentional* here, so
/// the complexity ratio must not flag it.
pub fn run_count_only_list(s: &Seeded) -> (usize, usize) {
    let filter = Filter::parse("", time::today()).unwrap();
    let q = store_engine::TaskQuery {
        reach: amenbo_core::reach::Reach::All,
        project_id: Some(s.project_id),
        filter: &filter,
        sort: "order",
        today: time::today(),
        limit: Some(0),
        offset: None,
    };
    let page = store_engine::list_task_ids(s.engine.conn(), &q).unwrap();
    (page.total_matched, page.ids.len())
}

/// Run the **word search** (`search_hits`) once for one term, returning its (matched, page) pair. The
/// term is the whole query, unnarrowed by kind or filter — the shape `amenbo search <word>` runs, which
/// is the widest one: every face of both sides is asked.
///
/// The seeded terms ([`HOT_TERM_SCAN`] / [`HOT_TERM_INDEX`]) are written only into the hot carve-out, so
/// the answer is a fixed set at any N while the *copy* the search reads grows with the store — which is
/// what makes this read's cost worth timing (`AMB-D-509`).
pub fn run_search(s: &Seeded, term: &str) -> (usize, usize) {
    let terms = store_engine::search::terms(term);
    let page = store_engine::read::search_hits(
        s.engine.conn(),
        &store_engine::read::SearchQuery {
            reach: amenbo_core::reach::Reach::All,
            terms: &terms,
            project_id: None,
            filter: None,
            today: time::today(),
            kind: None,
            sort: amenbo_core::query::SearchSort::default(),
            limit: Some(HOT_TASKS),
            offset: 0,
        },
    )
    .unwrap();
    (page.total_matched, page.hits.len())
}

/// Run the project `decision_page` read once, returning its (scanned, returned) pair.
pub fn run_decision_page(s: &Seeded) -> (usize, usize) {
    let page = store_engine::decision_page(
        s.engine.conn(),
        amenbo_core::reach::Reach::All,
        s.project_id,
        Some(HOT_TASKS),
        0,
    ).unwrap();
    (page.total_matched, page.ids.len())
}

/// Run the store-wide activity read (a bounded page) once. The system half lives in the file ledger,
/// which a projected fixture has none of — so what this measures is the comment half plus the (empty)
/// ledger scan.
pub fn run_store_activity(s: &Seeded) -> usize {
    let filter = amenbo_core::activity::Filter { limit: Some(50), ..Default::default() };
    let ledger = amenbo_core::activity::Ledger::open(&s.tmp.join(amenbo_core::activity_log::FILE_NAME));
    amenbo_core::activity::page(&ledger, s.engine.conn(), &filter)
        .unwrap()
        .len()
}

/// Run the snapshot project-overview read once.
pub fn run_project_overview(s: &Seeded) -> usize {
    store_engine::project_overview(s.engine.conn(), amenbo_core::reach::Reach::All).unwrap().len()
}

/// Median wall-clock of `iters` runs of `f` after a few warm-up calls. Median (not mean) so a
/// single GC/scheduler hiccup does not skew the scaling comparison.
pub fn median_time(iters: usize, mut f: impl FnMut()) -> Duration {
    for _ in 0..3 {
        f();
    }
    let mut samples: Vec<Duration> = (0..iters)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        })
        .collect();
    samples.sort();
    samples[samples.len() / 2]
}
