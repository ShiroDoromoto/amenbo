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
//! a copy that grows. **Every** seeded task is filed on both axes ([`AXES`]), the bulk included, so the
//! classification the search reads once its page is settled (`AMB-D-567`) sits in a child table that
//! grows with N — a follow-up read that stopped being bounded by the page has somewhere to grow into.

#![allow(dead_code)] // each consumer (bench / guard) uses a different subset of these helpers.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use amenbo_core::model::{
    ActorKind, Database, Decision, DecisionStatus, Dimension, DimensionValue, Priority, Project,
    Task, TaskComment, TaskDimensionValue, TaskStatus, View,
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

/// How many axes every seeded task is filed on — the one-to-many the search collapses after its page is
/// settled (`AMB-D-567`). Two, because a single label per task would read like one more column on the
/// task and never put the second read under the weight it exists to carry.
pub const AXES: usize = 2;
/// How many values each axis carries. Small and fixed: what the follow-up read pays for is the
/// assignments, and the values only have to be several so two tasks are not filed identically.
const VALUES_PER_AXIS: usize = 3;
/// The axes' and the values' names, which the index carries as the **label** face — so they are spelled
/// apart from [`HOT_TERM_SCAN`] / [`HOT_TERM_INDEX`], or a search for either would reach every placed
/// task through its label and the fixed answer would be gone.
const AXIS_NAME: &str = "axis";
/// See [`AXIS_NAME`].
const VALUE_NAME: &str = "value";

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
///
/// It is also filed on every axis ([`file_on_axes`]), hot task and bulk task alike — the classification
/// has to grow with N for a follow-up read that outgrew the page to show it.
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
    file_on_axes(db, id, idx);
    id
}

/// File one task on every axis — the placements the search reads back once its page is settled
/// (`AMB-D-567`). `idx` picks the value on each axis, so the rows point at different values rather than
/// all at one, as a real store's do. O(1) per task, like the push it follows, so the bulk loop stays
/// O(N).
fn file_on_axes(db: &mut Database, task_id: i64, idx: usize) {
    let now = Timestamp::now();
    for axis in 0..AXES {
        let id = (db.task_dimension_values.len() + 1) as i64;
        db.task_dimension_values.push(TaskDimensionValue {
            id,
            task_id,
            dimension_id: axis_id(axis),
            value_id: value_id(axis, (idx + axis) % VALUES_PER_AXIS),
            created_at: now,
            updated_at: now,
        });
    }
}

/// The id of one axis, and of one value on it. Pre-assigned rather than allocated, the same way every
/// other row in this seed is: the ids are what [`file_on_axes`] points at without reading anything back.
fn axis_id(axis: usize) -> i64 {
    (axis + 1) as i64
}

/// See [`axis_id`].
fn value_id(axis: usize, value: usize) -> i64 {
    (axis * VALUES_PER_AXIS + value + 1) as i64
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

    // The axes and their values: fixed and small in themselves — what grows with N is every task's
    // placement on them (file_on_axes), which is where a follow-up read that outgrew the page would
    // show. Pushed before the tasks so the rows the tasks point at are already there.
    for axis in 0..AXES {
        db.dimensions.push(Dimension {
            id: axis_id(axis),
            project_id: pid,
            name: format!("{AXIS_NAME} #{axis}"),
            order_key: format!("{axis:03}"),
            created_at: now,
            updated_at: now,
            ..Default::default()
        });
        for value in 0..VALUES_PER_AXIS {
            db.dimension_values.push(DimensionValue {
                id: value_id(axis, value),
                dimension_id: axis_id(axis),
                name: format!("{VALUE_NAME} #{axis}-{value}"),
                order_key: format!("{value:03}"),
                created_at: now,
                updated_at: now,
                ..Default::default()
            });
        }
    }

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

/// Run the **whole word search** ([`amenbo_core::query::search`]) once for one term. The term is the
/// whole query, unnarrowed by kind or filter — the shape `amenbo search <word>` runs, which is the widest
/// one: every face of both sides is asked.
///
/// The whole read, not `search_hits` alone: where each hit's record stands is read *after* the page is
/// settled (`AMB-D-567`), so a guard that called the engine directly would leave that second read
/// unwatched — the one place the search touches the classification, which is one-to-many and grows with
/// the store. What a face pays for is both reads together, so both are what is timed.
///
/// The seeded terms ([`HOT_TERM_SCAN`] / [`HOT_TERM_INDEX`]) are written only into the hot carve-out, so
/// the answer is a fixed set at any N while the *copy* the search reads grows with the store — which is
/// what makes this read's cost worth timing (`AMB-D-509`).
pub fn run_search(s: &Seeded, term: &str) -> amenbo_core::query::SearchResult {
    amenbo_core::query::search(
        s.engine.conn(),
        amenbo_core::reach::Reach::All,
        amenbo_core::query::SearchParams {
            text: term.to_string(),
            limit: Some(HOT_TASKS),
            ..Default::default()
        },
    )
    .unwrap()
}

/// The ids of the hot carve-out's tasks — the page a search for either seeded term settles on (the guard
/// asserts it really does, rather than taking this on trust). Known without reading anything back: the
/// hot tasks are written first and a task's id *is* its number.
pub fn hot_task_ids() -> Vec<i64> {
    (1..=HOT_TASKS as i64).collect()
}

/// Run the search's **follow-up read** on its own ([`store_engine::read::hit_standings`]) — where each
/// record the settled page names stands, classification included (`AMB-D-567`). The ids are the page's at
/// either N, which is the read's whole contract: what it costs follows the page, never the store.
///
/// Timed apart from the search it belongs to, and not only inside it. The search's own time grows with
/// the store legitimately — the short term scans a copy that grows — so a follow-up read that started
/// reading the child table whole is a handful of milliseconds hidden inside a read that is already
/// several times slower at N=BIG. Measured, that regression moved the whole search from ×2.3 to ×5.4,
/// nowhere near the ×20 a regression has to cross; the same regression measured alone is ×34. So the
/// whole search is timed for what a face pays, and this for what would otherwise hide inside it.
pub fn run_hit_standings(s: &Seeded) -> usize {
    let rows = store_engine::read::hit_standings(
        s.engine.conn(),
        amenbo_core::reach::Reach::All,
        &hot_task_ids(),
        &[],
    )
    .unwrap();
    rows.labels.len()
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
