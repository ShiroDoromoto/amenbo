//! Reading the timeline — the merge of the **file ledger** (system events) and the two **comment**
//! tables (`task_comment`, `decision_comment`), in one total order.
//!
//! The halves of the timeline live in different places, and deliberately so. A comment is permanent
//! data: it is the only record of what someone said, so it stays a first-class row that nothing ages out.
//! A system event narrates *how* a row reached the state its columns already hold — it is a bounded
//! viewing stream ([`crate::activity_log`]), and the oldest of them may be trimmed away without losing a
//! fact. Merging them at read time is what lets the two storage rules coexist behind one timeline.
//!
//! **The order is `(at, seq, id)` and it is total.** `at` is the part a reader sees: rows come in the
//! order they happened. The other two only ever break a tie *inside* one second, and both are needed
//! because the sources do not share one id space. The ledger and `task_comment` draw from **one counter**
//! ([`crate::store_engine::read::next_activity_id`]), so between those two `id` alone separates every row;
//! `decision_comment` numbers its own rows against its own table, so its ids repeat theirs by
//! construction — a decision comment and a task comment posted in the same second can hold the same `id`.
//! [`Seq`] names which sequence a row's `id` was drawn from, and sits between `at` and `id` so that no two
//! rows can hold the same key. That is what a cursor cut on this key needs in order not to lose or repeat
//! a row.
//!
//! **No half is read whole.** Each comment half is an indexed `WHERE` over its table; the file half is
//! read **backwards from the end** and abandoned as soon as the window is filled ([`Ledger`] /
//! [`crate::activity_log::rev_lines`]). A file with no index is not an excuse to load it: a
//! timeline read costs the rows it shows, not the history it sits on.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use rusqlite::Connection;
use serde_json::Value;

use crate::activity_log;
use crate::error::Result;
use crate::model::ActorKind;
use crate::store_engine::schema::col;
use crate::store_engine::sql::{same, Col, Int, NotNull, Pred, Select, Sort, Sql, Text};
use crate::time::Timestamp;

/// Which half of the timeline an item came from. The vocabulary the CLI and the JSON speak (`system` /
/// `comment`) names the *stream* — the file ledger or the comment table — not a column: nothing in the
/// database records the kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    System,
    Comment,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::System => "system",
            Kind::Comment => "comment",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "system" => Some(Kind::System),
            "comment" => Some(Kind::Comment),
            _ => None,
        }
    }
}

/// Which id sequence a row's `id` was drawn from — the middle of the ordering key, and the reason it stays
/// total now that the timeline merges three sources between which only two share a counter.
///
/// It is a property of the *source*, not something the reader chose, so it is derived
/// ([`Item::seq`]) rather than stored: nothing in the database records it, exactly as nothing records
/// [`Kind`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Seq {
    /// The file ledger and `task_comment` — one counter between them
    /// ([`crate::store_engine::read::next_activity_id`]), so they interleave within a second by `id`.
    Activity,
    /// `decision_comment` — its own dense per-table sequence, unrelated to the one above.
    DecisionComment,
}

impl Seq {
    /// The integer the key sorts on and the cursor carries. **Stable**: it travels inside an opaque cursor,
    /// so a value that moved would silently shift a reader's place in the stream.
    pub fn rank(self) -> i64 {
        match self {
            Seq::Activity => 0,
            Seq::DecisionComment => 1,
        }
    }

    /// The sequence a cursor's rank names. Unknown = `None`, so a cursor written by a build that knew a
    /// sequence this one does not is refused rather than silently read as some other stream.
    pub fn from_rank(rank: i64) -> Option<Seq> {
        match rank {
            0 => Some(Seq::Activity),
            1 => Some(Seq::DecisionComment),
            _ => None,
        }
    }
}

/// What a row is about. A system event names whichever subject it happened to (a decision deleted out of
/// a project names the decision); a comment names whatever it hangs on — its task, or its decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TargetType {
    Task,
    Project,
    Decision,
}

impl TargetType {
    pub fn as_str(self) -> &'static str {
        match self {
            TargetType::Task => "task",
            TargetType::Project => "project",
            TargetType::Decision => "decision",
        }
    }
}

/// One row of the merged timeline.
#[derive(Clone, Debug)]
pub struct Item {
    pub id: i64,
    pub at: Timestamp,
    pub kind: Kind,
    /// The facet that caused it. `None` on a row that records none.
    pub author_kind: Option<ActorKind>,
    pub target_type: TargetType,
    pub target_id: i64,
    /// The subject's live title, or — when the subject is gone — the name the event carried with it
    /// (a deletion's line is the only place that name still exists).
    pub title: String,
    /// Whether the subject still has a row to open. A deleted task / project / decision keeps its lines on
    /// the timeline (that is the point of the ledger), but there is nothing left to go to — so a reader
    /// that offers to open it is offering a door onto nothing. Answered by the same read that joins the
    /// title: a subject the read-model can name is a subject that is still there.
    pub target_live: bool,
    /// System rows only.
    pub event: Option<Value>,
    /// Comment rows only.
    pub text: Option<String>,
    /// Comment rows only: when the text was rewritten in place, if it ever was. The timeline keeps no
    /// correction history, so this stamp is all a reader has to tell that the line no longer says what it
    /// said. System rows never carry it.
    pub edited_at: Option<Timestamp>,
}

impl Item {
    /// Which id sequence this row's `id` came from. A comment on a decision is the one row numbered against
    /// its own table; everything else on the timeline — every ledger line, and every task comment — is on
    /// the shared activity counter.
    pub fn seq(&self) -> Seq {
        match (self.kind, self.target_type) {
            (Kind::Comment, TargetType::Decision) => Seq::DecisionComment,
            _ => Seq::Activity,
        }
    }

    /// The key the whole timeline is ordered and paged on.
    fn key(&self) -> (Timestamp, Seq, i64) {
        (self.at, self.seq(), self.id)
    }
}

/// What to read. Every field means the same thing on every half; the ones that need a task's *current*
/// columns (`project_id` / `for_facet`) resolve against the live `task` table on every side, so a row
/// follows its task rather than the project the event was filed under years ago.
///
/// Two fields ask a question a decision comment has no answer to, and it drops out of both rather than
/// being let through: `task_id` names a task it does not hang on, and `for_facet` asks for the target
/// *task's* assignee, which it has none of. `project_id` does reach it — through `decision.project_id`.
#[derive(Default)]
pub struct Filter {
    pub task_id: Option<i64>,
    pub project_id: Option<i64>,
    /// Rows stamped on or after this day (local `00:00`).
    pub since: Option<NaiveDate>,
    /// Strictly-greater cursor over `(at, seq, id)` — the incremental window.
    pub after: Option<(Timestamp, Seq, i64)>,
    /// `system` / `comment`. None = both.
    pub kind: Option<Kind>,
    /// The issuing facet (`--by`).
    pub author_kind: Option<ActorKind>,
    /// The **target task's** assignee facet (`--for`) — a different axis from `author_kind`.
    pub for_facet: Option<ActorKind>,
    /// Incremental mode reads forward (oldest first); history reads newest first.
    pub oldest_first: bool,
    /// None = every matching row. A caller wanting a `has_more` signal asks for one row more than it needs.
    pub limit: Option<usize>,
    pub offset: usize,
}

/// The file ledger a read draws its system events from — a **handle**, not the lines. Every read through
/// it walks the file **backwards from the end** and stops as soon as it has what it was asked for
/// ([`activity_log::rev_lines`]). Nothing loads the whole ledger: a window of a few rows costs a block or
/// two whatever the file has grown to. The ledger is append-only, so file order **is** id order, and the
/// end **is** the newest line — that is what a backward walk leans on: the rows a request wants are always
/// the ones it reaches first.
pub struct Ledger(PathBuf);

impl Ledger {
    /// A handle on the ledger at `path`. Opening nothing and reading nothing — a missing file is an empty
    /// ledger, never a failure.
    pub fn open(path: &Path) -> Self {
        Ledger(path.to_path_buf())
    }

    /// The lines, newest first.
    fn newest_first(&self) -> activity_log::RevLines {
        activity_log::rev_lines(&self.0)
    }
}

/// How far back a **name lookup** may walk before giving up. It is the one read that cannot bound
/// itself by what it finds: the name of a deleted subject may be anywhere in the ledger, or nowhere in it,
/// so without a budget one nameless row would drag the whole file back in. Beyond this, the row keeps the
/// empty title it already had — the same outcome as a name that was never written.
const NAME_SCAN_BUDGET: u64 = 1024 * 1024;

/// One window of the merged timeline, ordered by `(at, seq, id)` in the direction the filter asks for.
///
/// Each half is asked for the newest (or oldest) `offset + limit` rows on its own — that is enough for
/// the merge to be able to fill the window whichever half the rows come from — and the window is cut once
/// they are interleaved. Titles are joined **after** the cut, so the join is O(window), not O(timeline).
pub fn page(ledger: &Ledger, conn: &Connection, f: &Filter) -> Result<Vec<Item>> {
    let need = f.limit.map(|n| n + f.offset);
    // The two filters that read a task's *current* columns need the task table; nothing else does.
    let tasks = (f.project_id.is_some() || f.for_facet.is_some()).then(|| task_index(conn)).transpose()?;

    let mut items: Vec<Item> = Vec::new();
    if f.kind != Some(Kind::System) {
        // Both comment tables answer to `--kind comment`: a comment is a comment whatever it hangs on.
        items.extend(task_comment_rows(conn, f, need)?);
        items.extend(decision_comment_rows(conn, f, need)?);
    }
    if f.kind != Some(Kind::Comment) {
        items.extend(system_rows(ledger, f, tasks.as_ref(), need));
    }

    if f.oldest_first {
        items.sort_by_key(Item::key);
    } else {
        items.sort_by_key(|it| std::cmp::Reverse(it.key()));
    }
    if f.offset > 0 {
        items.drain(0..f.offset.min(items.len()));
    }
    if let Some(n) = f.limit {
        items.truncate(n);
    }

    resolve_targets(ledger, conn, &mut items)?;
    Ok(items)
}

/// The system half: the ledger walked **backwards from the end**, filtered line by line, and abandoned as
/// soon as the request is answered. A line whose subject cannot be named at all is skipped, like any other
/// line this build cannot make sense of.
///
/// Two things end the walk early, and both come from the ledger being append-only (so file order is id
/// order, newest last):
/// - a **cursor** (`after`), but **only one sitting on this very sequence** ([`Seq::Activity`]): every
///   line before its id is older than it, so the first line at or below that id is where the incremental
///   window ends. A cursor on `decision_comment` carries an id from *another* counter, which says nothing
///   about how far back the ledger's own ids reach — stopping on it would cut the walk at an unrelated
///   number and silently drop lines. So it does not bound this walk at all; the precise filter below is
///   what applies it.
/// - a **window** (`need`): the walk starts at the newest line, so the first `need` matching lines *are*
///   the newest `need` — the rest of the file cannot contain a row that outranks them.
///
/// A request with no limit (`need` is `None`) is asking for the whole matching history and pays for it;
/// a `since` day alone does not cut the walk, because a clock that stepped backwards could leave an older
/// timestamp ahead of a newer one, and stopping on it would silently drop rows.
fn system_rows(ledger: &Ledger, f: &Filter, tasks: Option<&TaskIndex>, need: Option<usize>) -> Vec<Item> {
    let stop_below = f.after.and_then(|(_, seq, id)| (seq == Seq::Activity).then_some(id));
    let rows = ledger
        .newest_first()
        .take_while(|l| stop_below.map(|cur_id| l.id > cur_id).unwrap_or(true))
        .filter(|l| f.task_id.is_none() || (l.task == f.task_id))
        .filter(|l| f.since.map(|d| l.at.0.date_naive() >= d).unwrap_or(true))
        .filter(|l| after_ok(f.after, Seq::Activity, l.at, l.id))
        .filter(|l| f.author_kind.map(|a| l.actor == Some(a)).unwrap_or(true))
        .filter(|l| line_project_ok(l, f, tasks))
        .filter(|l| line_for_ok(l, f, tasks))
        .filter_map(|l| {
            let (target_type, target_id) = match (l.decision, l.task, l.project) {
                (Some(d), _, _) => (TargetType::Decision, d),
                (None, Some(t), _) => (TargetType::Task, t),
                (None, None, Some(p)) => (TargetType::Project, p),
                (None, None, None) => return None,
            };
            Some(Item {
                id: l.id,
                at: l.at,
                kind: Kind::System,
                author_kind: l.actor,
                target_type,
                target_id,
                title: String::new(),
                target_live: false, // as above (one place decides, though a comment hangs only on a live task).
                event: Some(l.event),
                text: None,
                edited_at: None, // a system row has no body, so there is nothing to have corrected.
            })
        });
    match need {
        // History reads newest first, so the first `need` matches are the window. Incremental reads the
        // other way — the window it wants is the *oldest* rows past the cursor — so a cut here would take
        // the wrong end; the cursor is what bounds that walk, and it already has.
        Some(n) if !f.oldest_first => rows.take(n).collect(),
        _ => rows.collect(),
    }
}

/// `--project` on a ledger line: the task's *live* project decides, so history follows a task that moved.
/// When the task is gone (a deletion — the one event whose subject the DB cannot answer for), the line's
/// own `project` is all there is, and it answers.
fn line_project_ok(l: &activity_log::Line, f: &Filter, tasks: Option<&TaskIndex>) -> bool {
    let Some(pid) = f.project_id else { return true };
    match l.task.and_then(|t| tasks.and_then(|ix| ix.get(&t))) {
        Some(task) => task.project_id == Some(pid),
        None => l.project == Some(pid),
    }
}

/// `--for` on a ledger line: the target task's assignee facet. A line whose task is gone has no assignee
/// to match, so it drops out — the same way it does when the filter runs against the DB.
fn line_for_ok(l: &activity_log::Line, f: &Filter, tasks: Option<&TaskIndex>) -> bool {
    let Some(facet) = f.for_facet else { return true };
    l.task
        .and_then(|t| tasks.and_then(|ix| ix.get(&t)))
        .and_then(|t| t.assignee_kind)
        .map(|a| a == facet)
        .unwrap_or(false)
}

/// Whether an in-memory row is strictly past the cursor, compared on the whole key. The ledger's rows are
/// filtered through this; the tables' rows through [`after_pred`], which says the same thing in SQL.
fn after_ok(after: Option<(Timestamp, Seq, i64)>, seq: Seq, at: Timestamp, id: i64) -> bool {
    after.map(|cur| (at, seq, id) > cur).unwrap_or(true)
}

/// The cursor cut for the rows of one sequence, as SQL.
///
/// Only rows on the cursor's **own** sequence ever compare by `id`. Within one second the sequence decides,
/// so a cursor sitting on the other sequence either admits that whole second's rows (its rank is the lower
/// of the two) or excludes them (the higher), with no id compared at all. This is the one place the split
/// id spaces have to be held in mind: an `id` drawn from another counter is not a nearer or further
/// *position* in this stream, it is a number about something else — comparing the two is how a cursor
/// starts dropping rows.
fn after_pred(
    after: Option<(Timestamp, Seq, i64)>,
    seq: Seq,
    at: Col<Text, NotNull>,
    id: Col<Int, NotNull>,
) -> Option<Pred> {
    let (cur_at, cur_seq, cur_id) = after?;
    let newer = Pred::cmp(at, ">", cur_at.to_rfc3339_z());
    let same_second = || Pred::eq(at, cur_at.to_rfc3339_z());
    Some(match seq.cmp(&cur_seq) {
        std::cmp::Ordering::Equal => newer.or(same_second().and(Pred::cmp(id, ">", cur_id))),
        std::cmp::Ordering::Greater => newer.or(same_second()),
        std::cmp::Ordering::Less => newer,
    })
}

/// The task-comment half: an indexed `WHERE` over `task_comment`, with the same predicates the ledger scan
/// applies, so neither side can let a row through the other would have dropped.
fn task_comment_rows(conn: &Connection, f: &Filter, need: Option<usize>) -> Result<Vec<Item>> {
    /// The comment table's columns, spelled with the alias this query gives it.
    const C: col::task_comment::Cols = col::task_comment::of("c");
    /// The task joined onto it — the two filters that read a comment's task rather than the comment.
    const T: col::task::Cols = col::task::of("t");

    let oops = crate::error::sqlite_on(conn);
    let pred = Pred::all(
        [
            f.task_id.map(|t| Pred::eq(C.task_id, t)),
            // `created_at` is fixed-width `%Y-%m-%dT%H:%M:%SZ`, so a lexicographic `>=` against the bare
            // day is the day boundary.
            f.since.map(|d| Pred::cmp(C.created_at, ">=", d.format("%Y-%m-%d").to_string())),
            after_pred(f.after, Seq::Activity, C.created_at, C.id),
            f.author_kind.map(|a| Pred::eq(C.author_kind, a.as_str())),
            f.project_id.map(|p| Pred::eq(T.project_id, p)),
            f.for_facet.map(|a| Pred::eq(T.assignee_kind, a.as_str())),
        ]
        .into_iter()
        .flatten(),
    );

    let mut sel = Select::new();
    let (id, task_id, at, author, text, edited) = (
        sel.col(C.id),
        sel.col(C.task_id),
        sel.col(C.created_at),
        sel.col(C.author_kind),
        sel.col(C.text),
        sel.col(C.edited_at),
    );
    let mut sql = Sql::from(&sel, C.table);
    sql.join(T.table, same(T.id, C.task_id));
    let newest_first = !f.oldest_first;
    sql.push_where(pred.as_ref())
        // The direction is grammar, not a value — it cannot be bound, so it is the key's own
        // (`Sort::dir`), and the `id` tiebreak takes the same one so a reversed read reverses whole.
        .order_by([
            Sort::by(C.created_at).dir(newest_first),
            Sort::by(C.id).dir(newest_first),
        ])
        // `LIMIT -1` is SQLite's "no limit".
        .limit(need.map(|n| n as i64).unwrap_or(-1));

    let mut stmt = conn.prepare(sql.text()).map_err(&oops)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok(Item {
                id: id.get(r)?,
                at: Timestamp::parse_rfc3339(&at.get(r)?).unwrap_or_default(),
                kind: Kind::Comment,
                author_kind: author.get(r)?.as_deref().and_then(ActorKind::parse),
                target_type: TargetType::Task,
                target_id: task_id.get(r)?,
                title: String::new(),
                target_live: false, // as above (a comment only hangs on a live task, but one place decides).
                event: None,
                // `text` is `NOT NULL` in the registry; the `Item` field is optional because a system
                // event has no text, not because a comment's may be missing.
                text: Some(text.get(r)?),
                edited_at: edited
                    .get(r)?
                    .as_deref()
                    .map(|t| Timestamp::parse_rfc3339(t).unwrap_or_default()),
            })
        })
        .map_err(&oops)?
        .collect::<rusqlite::Result<Vec<Item>>>()
        .map_err(&oops)?;
    Ok(rows)
}

/// The decision-comment half: the shape of [`task_comment_rows`] over `decision_comment`, joined to
/// `decision` for the one filter that reaches it.
///
/// Two of the filters cannot be answered here, and the answer is an empty window rather than a row let
/// through: `task_id` names a task this comment does not hang on, and `for_facet` (`--for`) asks for the
/// target *task's* assignee, which a decision has none of. Narrowing to nothing is what the ledger side
/// already does with a line whose task is gone — a row with no assignee to match does not match an
/// assignee.
fn decision_comment_rows(conn: &Connection, f: &Filter, need: Option<usize>) -> Result<Vec<Item>> {
    /// The comment table's columns, spelled with the alias this query gives it.
    const C: col::decision_comment::Cols = col::decision_comment::of("c");
    /// The decision joined onto it — `--project` reaches these rows through `decision.project_id`.
    const D: col::decision::Cols = col::decision::of("d");

    if f.task_id.is_some() || f.for_facet.is_some() {
        return Ok(Vec::new());
    }

    let oops = crate::error::sqlite_on(conn);
    let pred = Pred::all(
        [
            // The day boundary, as a lexicographic cut on the fixed-width instant (as on the task side).
            f.since.map(|d| Pred::cmp(C.created_at, ">=", d.format("%Y-%m-%d").to_string())),
            after_pred(f.after, Seq::DecisionComment, C.created_at, C.id),
            f.author_kind.map(|a| Pred::eq(C.author_kind, a.as_str())),
            f.project_id.map(|p| Pred::eq(D.project_id, p)),
        ]
        .into_iter()
        .flatten(),
    );

    let mut sel = Select::new();
    let (id, decision_id, at, author, text, edited) = (
        sel.col(C.id),
        sel.col(C.decision_id),
        sel.col(C.created_at),
        sel.col(C.author_kind),
        sel.col(C.text),
        sel.col(C.edited_at),
    );
    let mut sql = Sql::from(&sel, C.table);
    // An inner join drops nothing here: `decision_id` is an FK with `CASCADE`, so a comment whose decision
    // is gone went with it. That is also why these rows are always `target_live` — unlike a ledger line,
    // a comment cannot outlive what it hangs on.
    sql.join(D.table, same(D.id, C.decision_id));
    let newest_first = !f.oldest_first;
    sql.push_where(pred.as_ref())
        .order_by([
            Sort::by(C.created_at).dir(newest_first),
            Sort::by(C.id).dir(newest_first),
        ])
        .limit(need.map(|n| n as i64).unwrap_or(-1));

    let mut stmt = conn.prepare(sql.text()).map_err(&oops)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok(Item {
                id: id.get(r)?,
                at: Timestamp::parse_rfc3339(&at.get(r)?).unwrap_or_default(),
                kind: Kind::Comment,
                author_kind: author.get(r)?.as_deref().and_then(ActorKind::parse),
                target_type: TargetType::Decision,
                target_id: decision_id.get(r)?,
                title: String::new(),
                target_live: false, // as above (one place decides).
                event: None,
                text: Some(text.get(r)?),
                edited_at: edited
                    .get(r)?
                    .as_deref()
                    .map(|t| Timestamp::parse_rfc3339(t).unwrap_or_default()),
            })
        })
        .map_err(&oops)?
        .collect::<rusqlite::Result<Vec<Item>>>()
        .map_err(&oops)?;
    Ok(rows)
}

/// For each of `task_ids`, when the activity that put it in the mailbox happened (its `triggeredAt`):
/// the newest incoming comment (one the human did not write themselves) or `task.assigned` addressed to a
/// facet. A task with no such cause is omitted.
///
/// **The whole set is answered in one pass over each half**, not one timeline read per task. The ledger is
/// a file with no index, so a read per mailbox task would walk it once per task; here it is walked once and
/// folded into `task → newest trigger`, and the comment half is one indexed query over the whole set. The
/// cost follows the ledger, not the ledger times the mailbox.
pub fn mailbox_triggered_at(ledger: &Path, conn: &Connection, task_ids: &[i64]) -> Result<Vec<(i64, String)>> {
    if task_ids.is_empty() {
        return Ok(Vec::new());
    }
    let wanted: std::collections::HashSet<i64> = task_ids.iter().copied().collect();

    // The two halves are ordered on one key — they draw their ids from one counter (see the module doc) —
    // so the newest cause of either kind is simply the largest `(at, id)` seen for that task.
    let mut newest: HashMap<i64, (Timestamp, i64)> = HashMap::new();
    fn keep(newest: &mut HashMap<i64, (Timestamp, i64)>, task: i64, at: Timestamp, id: i64) {
        match newest.entry(task) {
            std::collections::hash_map::Entry::Occupied(mut e) if (at, id) > *e.get() => {
                e.insert((at, id));
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert((at, id));
            }
            _ => {}
        }
    }

    // Backwards from the end, and done as soon as every task has its cause: the newest line naming a task
    // is the newest cause it can have, so nothing older can change the answer. A task with no cause at all
    // has nothing to stop on, so the walk is also bounded by how far back it may look.
    let mut lines = activity_log::rev_lines(ledger);
    while let Some(l) = lines.next() {
        if newest.len() == wanted.len() || lines.read_bytes() > NAME_SCAN_BUDGET {
            break;
        }
        let Some(task) = l.task else { continue };
        if wanted.contains(&task) && is_assignment_to_a_facet(&l.event) {
            keep(&mut newest, task, l.at, l.id);
        }
    }

    const C: col::task_comment::Cols = col::task_comment::ALL;

    let oops = crate::error::sqlite_on(conn);
    let mut sel = Select::new();
    let (task_id, at, id, author) =
        (sel.col(C.task_id), sel.col(C.created_at), sel.col(C.id), sel.col(C.author_kind));
    let mut sql = Sql::from(&sel, C.table);
    sql.push_where(Some(&Pred::is_in(C.task_id, task_ids.iter().copied())));

    let mut stmt = conn.prepare(sql.text()).map_err(&oops)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok((
                task_id.get(r)?,
                Timestamp::parse_rfc3339(&at.get(r)?).unwrap_or_default(),
                id.get(r)?,
                author.get(r)?.as_deref().and_then(ActorKind::parse),
            ))
        })
        .map_err(&oops)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(&oops)?;
    for (task, at, id, author) in rows {
        if author == Some(ActorKind::Ai) {
            keep(&mut newest, task, at, id);
        }
    }

    // Answer in the order asked, so the caller's mailbox order survives the fold.
    Ok(task_ids
        .iter()
        .filter_map(|id| newest.get(id).map(|(at, _)| (*id, at.to_rfc3339_z())))
        .collect())
}

/// Whether a ledger line is the system-event **cause** that surfaced a task in the mailbox: a
/// `task.assigned` that put a facet on it.
fn is_assignment_to_a_facet(event: &Value) -> bool {
    event.get("kind").and_then(Value::as_str) == Some("task.assigned")
        && event.get("to_kind").and_then(Value::as_str).is_some()
}

/// A task's two live columns the timeline filters on.
struct TaskFacts {
    project_id: Option<i64>,
    assignee_kind: Option<ActorKind>,
}

type TaskIndex = HashMap<i64, TaskFacts>;

/// Every live task's `project_id` / `assignee_kind`, read once. Built **only** when `--project` or
/// `--for` is asked for: a ledger line cannot join, so the filter has to bring the table to it.
fn task_index(conn: &Connection) -> Result<TaskIndex> {
    const T: col::task::Cols = col::task::ALL;

    let oops = crate::error::sqlite_on(conn);
    let mut sel = Select::new();
    let (id, project_id, assignee) = (sel.col(T.id), sel.col(T.project_id), sel.col(T.assignee_kind));
    let sql = Sql::from(&sel, T.table);
    let mut stmt = conn.prepare(sql.text()).map_err(&oops)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                id.get(r)?,
                TaskFacts {
                    project_id: project_id.get(r)?,
                    assignee_kind: assignee.get(r)?.as_deref().and_then(ActorKind::parse),
                },
            ))
        })
        .map_err(&oops)?
        .collect::<rusqlite::Result<TaskIndex>>()
        .map_err(&oops)?;
    Ok(rows)
}

/// Fill in each row's title from the live row it names, falling back to the name the event carried — for a
/// deletion, that is the only copy of the name left anywhere — and say, on the way, whether that row is
/// still there at all (`target_live`). A subject that is gone leaves rows its *own* event cannot name:
/// `task.status_changed` and its kin carry no title, so once the task row is deleted nothing in that line
/// says what it was about. The name is still in the ledger, though — on another line of the same subject
/// (its creation, its deletion) — so whatever the two passes above leave nameless, a second read of the
/// ledger answers.
fn resolve_targets(ledger: &Ledger, conn: &Connection, items: &mut [Item]) -> Result<()> {
    for (target, id, name) in [
        (TargetType::Task, col::task::ALL.id, col::task::ALL.title),
        (TargetType::Project, col::project::ALL.id, col::project::ALL.name),
        (TargetType::Decision, col::decision::ALL.id, col::decision::ALL.title),
    ] {
        let ids: Vec<i64> = items.iter().filter(|it| it.target_type == target).map(|it| it.target_id).collect();
        if ids.is_empty() {
            continue;
        }
        let live = titles(conn, id, name, &ids)?;
        for it in items.iter_mut().filter(|it| it.target_type == target) {
            let live_title = live.get(&it.target_id).cloned();
            it.target_live = live_title.is_some();
            it.title = live_title.or_else(|| event_name(it.event.as_ref())).unwrap_or_default();
        }
    }

    // Only pay for the extra scan when something in this window is actually nameless — on a timeline with
    // no deleted subjects, this costs one pass over the window and nothing else.
    let nameless: HashSet<(TargetType, i64)> =
        items.iter().filter(|it| it.title.is_empty()).map(|it| (it.target_type, it.target_id)).collect();
    if nameless.is_empty() {
        return Ok(());
    }
    let named = ledger_names(ledger, &nameless);
    for it in items.iter_mut().filter(|it| it.title.is_empty()) {
        if let Some(name) = named.get(&(it.target_type, it.target_id)) {
            it.title = name.clone();
        }
    }
    Ok(())
}

/// The names the ledger itself still holds for the subjects asked about — read off whichever of their lines
/// carried a name (creation, deletion). Later lines win, so a subject that was renamed before it went is
/// remembered by the name it went by.
fn ledger_names(ledger: &Ledger, wanted: &HashSet<(TargetType, i64)>) -> HashMap<(TargetType, i64), String> {
    let mut out: HashMap<(TargetType, i64), String> = HashMap::new();
    let mut lines = ledger.newest_first();
    while let Some(l) = lines.next() {
        // Newest first, so the first name found for a subject is the one it went by; and the walk stops as
        // soon as every subject is named — or once it has looked far enough back.
        if out.len() == wanted.len() || lines.read_bytes() > NAME_SCAN_BUDGET {
            break;
        }
        let key = match (l.decision, l.task, l.project) {
            (Some(d), _, _) => (TargetType::Decision, d),
            (None, Some(t), _) => (TargetType::Task, t),
            (None, None, Some(p)) => (TargetType::Project, p),
            (None, None, None) => continue,
        };
        if !wanted.contains(&key) {
            continue;
        }
        if let Some(name) = event_name(Some(&l.event)) {
            out.insert(key, name);
        }
    }
    out
}

/// The name a deletion event carries (`title` for a task or decision, `name` for a project).
fn event_name(event: Option<&Value>) -> Option<String> {
    let event = event?;
    event
        .get("title")
        .or_else(|| event.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// `id → name` for the rows named by `ids`. The two columns come from the registry (`col::…`), and the
/// table is the id column's own qualifier — so the projection and the `FROM` cannot name different
/// tables, and there is no column spelled as a string to typo.
fn titles(
    conn: &Connection,
    id: Col<Int, NotNull>,
    name: Col<Text, NotNull>,
    ids: &[i64],
) -> Result<HashMap<i64, String>> {
    let oops = crate::error::sqlite_on(conn);
    let mut sel = Select::new();
    let (id_slot, name_slot) = (sel.col(id), sel.col(name));
    let mut sql = Sql::from(&sel, id.table());
    sql.push_where(Some(&Pred::is_in(id, ids.iter().copied())));

    let mut stmt = conn.prepare(sql.text()).map_err(&oops)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| Ok((id_slot.get(r)?, name_slot.get(r)?)))
        .map_err(&oops)?
        .collect::<rusqlite::Result<HashMap<i64, String>>>()
        .map_err(&oops)?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_engine::StoreEngine;
    use rusqlite::types::Value as Sql;
    use serde_json::json;

    fn text(s: &str) -> Sql {
        Sql::Text(s.to_string())
    }

    fn at(sec: u32) -> Timestamp {
        Timestamp::parse_rfc3339(&format!("2026-07-05T00:00:{sec:02}Z")).unwrap()
    }

    /// A store with two tasks (one AI's, one the human's) in project 7, plus an empty ledger beside it.
    fn fixture(tag: &str) -> (StoreEngine, std::path::PathBuf) {
        let e = StoreEngine::open_in_memory_unchecked().unwrap();
        e.put_record("project", 7, &[("name", text("Alpha")), ("order_key", text("a"))]).unwrap();
        e.put_record(
            "task",
            1,
            &[("title", text("AI のタスク")), ("project_id", Sql::Integer(7)), ("assignee_kind", text("ai"))],
        )
        .unwrap();
        e.put_record(
            "task",
            2,
            &[("title", text("人のタスク")), ("project_id", Sql::Integer(7)), ("assignee_kind", text("human"))],
        )
        .unwrap();

        let dir = amenbo_scratch::scratch(&format!("activity-read-{tag}"));
        let ledger = dir.join(activity_log::FILE_NAME);
        std::fs::remove_file(&ledger).ok();
        (e, ledger)
    }

    fn system(ledger: &Path, id: i64, sec: u32, task: Option<i64>, event: Value) {
        activity_log::append(
            ledger,
            &activity_log::Entry {
                id,
                at: at(sec),
                actor: Some(ActorKind::Ai),
                project: Some(7),
                task,
                decision: None,
                event,
            },
        );
    }

    fn decision(e: &StoreEngine, id: i64, project: i64, title_: &str) {
        e.put_record(
            "decision",
            id,
            &[
                ("project_id", Sql::Integer(project)),
                ("title", text(title_)),
                ("body", text("本文")),
                ("status", text("proposed")),
            ],
        )
        .unwrap();
    }

    fn decision_comment(e: &StoreEngine, id: i64, sec: u32, decision_id: i64, text_: &str) {
        e.put_record(
            "decision_comment",
            id,
            &[
                ("decision_id", Sql::Integer(decision_id)),
                ("author_kind", text("human")),
                ("text", text(text_)),
                ("created_at", text(&at(sec).to_rfc3339_z())),
                ("updated_at", text(&at(sec).to_rfc3339_z())),
            ],
        )
        .unwrap();
    }

    fn comment(e: &StoreEngine, id: i64, sec: u32, task: i64, text_: &str) {
        e.put_record(
            "task_comment",
            id,
            &[
                ("task_id", Sql::Integer(task)),
                ("author_kind", text("human")),
                ("text", text(text_)),
                ("created_at", text(&at(sec).to_rfc3339_z())),
                ("updated_at", text(&at(sec).to_rfc3339_z())),
            ],
        )
        .unwrap();
    }

    /// The timeline is one stream out of two stores: system events from the file, comments from the
    /// table, interleaved on `(at, id)` — which is a total order only because both draw ids from one
    /// counter, so no tie is ever left for the sort to break arbitrarily.
    #[test]
    fn the_two_halves_merge_into_one_ordered_stream() {
        let (e, ledger) = fixture("merge");
        system(&ledger, 1, 1, Some(1), json!({ "kind": "task.created", "title": "AI のタスク" }));
        comment(&e, 2, 2, 1, "はじめる");
        system(&ledger, 3, 3, Some(1), json!({ "kind": "task.status_changed", "old": "todo", "new": "in_progress" }));

        let items = page(&Ledger::open(&ledger), e.conn(), &Filter::default()).unwrap();

        let ids: Vec<i64> = items.iter().map(|it| it.id).collect();
        assert_eq!(ids, vec![3, 2, 1], "newest first, across both halves");
        assert_eq!(items[1].kind, Kind::Comment);
        assert_eq!(items[1].text.as_deref(), Some("はじめる"));
        assert_eq!(items[2].kind, Kind::System);
        assert!(items[2].text.is_none(), "a system row carries an event, never a body");
        assert!(items.iter().all(|it| it.title == "AI のタスク"), "the live title is joined onto both halves");
    }

    /// `--kind` picks one half; `--task` and `--by` mean the same thing on both.
    #[test]
    fn the_filters_mean_the_same_thing_on_both_halves() {
        let (e, ledger) = fixture("filters");
        system(&ledger, 1, 1, Some(1), json!({ "kind": "task.created", "title": "AI のタスク" }));
        system(&ledger, 2, 2, Some(2), json!({ "kind": "task.created", "title": "人のタスク" }));
        comment(&e, 3, 3, 1, "AI タスクへの一言");
        comment(&e, 4, 4, 2, "人タスクへの一言");

        let only = |f: Filter| -> Vec<i64> {
            page(&Ledger::open(&ledger), e.conn(), &f).unwrap().into_iter().map(|it| it.id).collect()
        };
        assert_eq!(only(Filter { kind: Some(Kind::System), ..Default::default() }), vec![2, 1]);
        assert_eq!(only(Filter { kind: Some(Kind::Comment), ..Default::default() }), vec![4, 3]);
        assert_eq!(only(Filter { task_id: Some(1), ..Default::default() }), vec![3, 1], "one task, both halves");
        assert_eq!(
            only(Filter { author_kind: Some(ActorKind::Ai), ..Default::default() }),
            vec![2, 1],
            "--by ai: the ledger lines are the AI's; the comments are the human's"
        );
        assert_eq!(
            only(Filter { for_facet: Some(ActorKind::Ai), ..Default::default() }),
            vec![3, 1],
            "--for ai: everything on the AI's task, whoever caused it"
        );
        assert_eq!(only(Filter { project_id: Some(7), ..Default::default() }).len(), 4, "both tasks are in it");
        assert!(only(Filter { project_id: Some(8), ..Default::default() }).is_empty());
    }

    /// The window is cut after the merge — so paging cannot lose a row that happened to sit in the
    /// other half — and the cursor walks forward over `(at, id)` without repeating one.
    #[test]
    fn paging_and_the_forward_cursor_cut_the_merged_stream() {
        let (e, ledger) = fixture("paging");
        system(&ledger, 1, 1, Some(1), json!({ "kind": "task.created", "title": "AI のタスク" }));
        comment(&e, 2, 2, 1, "二番目");
        system(&ledger, 3, 3, Some(1), json!({ "kind": "task.moved", "project": "Alpha" }));
        comment(&e, 4, 4, 1, "四番目");

        let ids = |f: Filter| -> Vec<i64> {
            page(&Ledger::open(&ledger), e.conn(), &f).unwrap().into_iter().map(|it| it.id).collect()
        };
        assert_eq!(ids(Filter { limit: Some(2), ..Default::default() }), vec![4, 3]);
        assert_eq!(ids(Filter { limit: Some(2), offset: 2, ..Default::default() }), vec![2, 1], "the next page");
        assert_eq!(
            ids(Filter { after: Some((at(2), Seq::Activity, 2)), oldest_first: true, ..Default::default() }),
            vec![3, 4],
            "strictly newer than the cursor, oldest first"
        );
    }

    /// A deletion lives **only** in the ledger, and its line carries the name of what it deleted — the
    /// DB cannot answer for a row that is gone, so this is the last copy of that name.
    #[test]
    fn a_deletion_is_read_back_with_the_name_the_database_no_longer_has() {
        let (e, ledger) = fixture("deleted");
        system(&ledger, 1, 1, Some(404), crate::activity_log::event::task_deleted(Some("消したタスク")));
        activity_log::append(
            &ledger,
            &activity_log::Entry {
                id: 2,
                at: at(2),
                actor: Some(ActorKind::Human),
                project: Some(7),
                task: None,
                decision: Some(9),
                event: crate::activity_log::event::decision_deleted(Some("消した決定")),
            },
        );

        let items = page(&Ledger::open(&ledger), e.conn(), &Filter::default()).unwrap();

        assert_eq!(items[0].target_type, TargetType::Decision);
        assert_eq!(items[0].title, "消した決定");
        assert_eq!(items[1].target_type, TargetType::Task);
        assert_eq!(items[1].target_id, 404);
        assert_eq!(items[1].title, "消したタスク", "the payload's name stands in for the row that is gone");
        // The line's own project is what a deletion can be filtered by — its task cannot be joined to.
        assert_eq!(page(&Ledger::open(&ledger), e.conn(), &Filter { project_id: Some(7), ..Default::default() }).unwrap().len(), 2);
    }

    /// A read that fails names the file it was reading. SQLite says only *what* went wrong ("no such
    /// table"), never *where* — and a timeline is read out of one store among many.
    #[test]
    fn a_failed_read_names_the_file_it_was_reading() {
        let dir = amenbo_scratch::scratch("activity-name");
        let db = dir.join(crate::config::STORE_FILE_NAME);
        std::fs::remove_file(&db).ok();

        let e = StoreEngine::open(&db).unwrap();
        e.conn().execute_batch("DROP TABLE task_comment").unwrap();

        let ledger = dir.join(activity_log::FILE_NAME);
        let err = page(&Ledger::open(&ledger), e.conn(), &Filter::default()).unwrap_err().to_string();
        assert!(err.contains(&db.display().to_string()), "it must name the path: {err}");
    }

    /// The ledger is written by whatever build was installed and trimmed by whichever writer crossed
    /// the cap, so the reader must survive a line it cannot understand — a future `v`, a torn tail,
    /// plain garbage — by skipping it, never by failing the timeline.
    #[test]
    fn an_unreadable_line_costs_nothing_but_that_line() {
        let (e, ledger) = fixture("tolerant");
        system(&ledger, 1, 1, Some(1), json!({ "kind": "task.created", "title": "AI のタスク" }));
        let junk = format!(
            "{}\n{}\n{}\n",
            r#"{"v":99,"id":50,"at":"2026-07-05T00:00:05Z","task":1,"event":{"kind":"from.the.future"}}"#,
            r#"{"v":2,"id":51,"at":"2026-07-05T00:00:06Z","task":1"#, // a torn line
            "not json at all",
        );
        std::fs::OpenOptions::new()
            .append(true)
            .open(&ledger)
            .and_then(|mut f| std::io::Write::write_all(&mut f, junk.as_bytes()))
            .unwrap();
        system(&ledger, 2, 7, Some(1), json!({ "kind": "task.moved", "project": "Alpha" }));

        let items = page(&Ledger::open(&ledger), e.conn(), &Filter::default()).unwrap();

        let ids: Vec<i64> = items.iter().map(|it| it.id).collect();
        assert_eq!(ids, vec![2, 1], "the readable lines are all there; the three bad ones are skipped");
    }

    /// The mailbox's `triggeredAt` is the newest *cause* — an incoming comment or an assignment — and
    /// reads across the same merge (the assignment is a ledger line, the comment a DB row).
    #[test]
    fn the_mailbox_trigger_is_the_newest_cause_from_either_half() {
        let (e, ledger) = fixture("trigger");
        system(&ledger, 1, 1, Some(1), json!({ "kind": "task.created", "title": "AI のタスク" }));
        comment(&e, 2, 2, 1, "人からの一言"); // by the human — not an incoming cause
        system(&ledger, 3, 3, Some(1), json!({ "kind": "task.assigned", "to_kind": "ai" }));

        let trig = mailbox_triggered_at(&ledger, e.conn(), &[1, 2, 999]).unwrap();

        assert_eq!(trig, vec![(1, at(3).to_rfc3339_z())], "the assignment; tasks with no cause are omitted");
    }

    /// The whole mailbox is answered from one pass over each half, so the fold — not a per-task
    /// timeline read — is what has to pick the newest cause. It picks across the halves (an incoming
    /// comment written after the assignment wins over it), for every task at once, and hands the answers
    /// back in the order they were asked for.
    #[test]
    fn the_whole_mailbox_is_folded_in_one_pass_and_keeps_the_order_asked_for() {
        let (e, ledger) = fixture("fold");
        // Task 1: assigned, then an AI comment lands on it — the comment is the newer cause.
        system(&ledger, 1, 1, Some(1), json!({ "kind": "task.assigned", "to_kind": "ai" }));
        e.put_record(
            "task_comment",
            2,
            &[
                ("task_id", Sql::Integer(1)),
                ("author_kind", text("ai")),
                ("text", text("届いた一言")),
                ("created_at", text(&at(2).to_rfc3339_z())),
                ("updated_at", text(&at(2).to_rfc3339_z())),
            ],
        )
        .unwrap();
        // Task 2: only the assignment, plus a comment the human wrote themselves (not an incoming cause).
        system(&ledger, 3, 3, Some(2), json!({ "kind": "task.assigned", "to_kind": "human" }));
        comment(&e, 4, 4, 2, "自分で書いた一言");

        let trig = mailbox_triggered_at(&ledger, e.conn(), &[2, 1]).unwrap();

        assert_eq!(
            trig,
            vec![(2, at(3).to_rfc3339_z()), (1, at(2).to_rfc3339_z())],
            "each task's newest cause, in the order asked"
        );
    }

    /// A deleted task's older lines carry no title of their own, and the row they named is gone — so the
    /// name has to come off another line of the same task. Without that, the timeline reads "set  to done",
    /// with a hole where the name should be.
    #[test]
    fn a_deleted_subjects_nameless_lines_take_their_name_from_the_ledger() {
        let (e, ledger) = fixture("deleted-subject");
        system(&ledger, 1, 1, Some(9), json!({ "kind": "task.created", "title": "消えたタスク" }));
        system(&ledger, 2, 2, Some(9), json!({ "kind": "task.status_changed", "old": "todo", "new": "done" }));
        system(&ledger, 3, 3, Some(9), json!({ "kind": "task.deleted", "title": "消えたタスク（改題後）" }));

        let items = page(&Ledger::open(&ledger), e.conn(), &Filter { task_id: Some(9), ..Default::default() }).unwrap();

        let titles: Vec<&str> = items.iter().map(|it| it.title.as_str()).collect();
        assert_eq!(
            titles,
            // Newest first: the deletion and the creation each speak for themselves; the status change is
            // the one with nothing to say, and it borrows the last name the ledger saw the task under.
            vec!["消えたタスク（改題後）", "消えたタスク（改題後）", "消えたタスク"],
            "no line of a gone task is left nameless"
        );
    }

    /// A row says whether its subject is still there, so a reader can tell an address from an epitaph.
    /// Both halves of the same task move together: the line and the comment either both point at
    /// something, or both point at nothing.
    #[test]
    fn a_row_says_whether_its_subject_is_still_there() {
        let (e, ledger) = fixture("target-live");
        system(&ledger, 1, 1, Some(1), json!({ "kind": "task.created", "title": "AI のタスク" }));
        comment(&e, 2, 2, 1, "生きているタスクへの発言");
        system(&ledger, 3, 3, Some(9), json!({ "kind": "task.deleted", "title": "消えたタスク" }));
        // The one line that names a project on its own: a project is deleted physically, so this row's
        // subject is gone by construction.
        activity_log::append(
            &ledger,
            &activity_log::Entry {
                id: 4,
                at: at(4),
                actor: Some(ActorKind::Human),
                project: Some(8),
                task: None,
                decision: None,
                event: json!({ "kind": "project.deleted", "name": "消えたプロジェクト" }),
            },
        );

        let items = page(&Ledger::open(&ledger), e.conn(), &Filter::default()).unwrap();

        let live: Vec<(i64, bool)> = items.iter().map(|it| (it.id, it.target_live)).collect();
        assert_eq!(
            live,
            vec![(4, false), (3, false), (2, true), (1, true)],
            "live where the read-model still has the row, gone where only the ledger remembers it"
        );
    }

    /// A comment on a decision is on the timeline like any other comment: same `kind`, its own body, and
    /// named by the decision it hangs on rather than by a task. It is a *comment*, so `--kind` sorts it with
    /// the other comments and not with the system events.
    #[test]
    fn a_decision_comment_is_on_the_timeline_named_by_its_decision() {
        let (e, ledger) = fixture("decision-comment");
        decision(&e, 9, 7, "決定のタイトル");
        system(&ledger, 1, 1, Some(1), json!({ "kind": "task.created", "title": "AI のタスク" }));
        comment(&e, 2, 2, 1, "タスクへの一言");
        decision_comment(&e, 3, 3, 9, "決定への一言");

        let items = page(&Ledger::open(&ledger), e.conn(), &Filter::default()).unwrap();

        let newest = &items[0];
        assert_eq!(newest.kind, Kind::Comment);
        assert_eq!(newest.target_type, TargetType::Decision);
        assert_eq!(newest.target_id, 9);
        assert_eq!(newest.text.as_deref(), Some("決定への一言"));
        assert_eq!(newest.title, "決定のタイトル", "the decision's live title is joined on");
        assert!(newest.target_live, "a comment cannot outlive what it hangs on (the FK cascades)");

        let only = |f: Filter| -> Vec<TargetType> {
            page(&Ledger::open(&ledger), e.conn(), &f).unwrap().into_iter().map(|it| it.target_type).collect()
        };
        assert_eq!(
            only(Filter { kind: Some(Kind::Comment), ..Default::default() }),
            vec![TargetType::Decision, TargetType::Task],
            "--kind comment takes both comment tables"
        );
        assert_eq!(
            only(Filter { kind: Some(Kind::System), ..Default::default() }),
            vec![TargetType::Task],
            "--kind system takes the ledger alone"
        );
    }

    /// The filters that ask a decision comment a question it has no answer to narrow it away rather than
    /// letting it through: `--task` names a task it does not hang on, and `--for` asks for the target
    /// *task's* assignee, which a decision has none of. `--project` does reach it — through the decision.
    #[test]
    fn the_filters_a_decision_comment_cannot_answer_drop_it() {
        let (e, ledger) = fixture("decision-comment-filters");
        decision(&e, 9, 7, "この PJ の決定");
        e.put_record("project", 8, &[("name", text("Beta")), ("order_key", text("b"))]).unwrap();
        decision(&e, 10, 8, "別 PJ の決定");
        comment(&e, 1, 1, 1, "AI タスクへの一言");
        decision_comment(&e, 2, 2, 9, "この PJ の決定への一言");
        decision_comment(&e, 3, 3, 10, "別 PJ の決定への一言");

        let ids = |f: Filter| -> Vec<(TargetType, i64)> {
            page(&Ledger::open(&ledger), e.conn(), &f)
                .unwrap()
                .into_iter()
                .map(|it| (it.target_type, it.id))
                .collect()
        };
        assert_eq!(
            ids(Filter { task_id: Some(1), ..Default::default() }),
            vec![(TargetType::Task, 1)],
            "--task: only the task's own timeline, never a decision's comments"
        );
        assert_eq!(
            ids(Filter { for_facet: Some(ActorKind::Ai), ..Default::default() }),
            vec![(TargetType::Task, 1)],
            "--for ai: a row with no assignee to match does not match an assignee"
        );
        assert_eq!(
            ids(Filter { project_id: Some(7), ..Default::default() }),
            vec![(TargetType::Decision, 2), (TargetType::Task, 1)],
            "--project reaches a decision comment through decision.project_id"
        );
        assert_eq!(
            ids(Filter { project_id: Some(8), ..Default::default() }),
            vec![(TargetType::Decision, 3)],
            "and it keeps the other project's out"
        );
        assert_eq!(
            ids(Filter { author_kind: Some(ActorKind::Human), ..Default::default() }),
            vec![(TargetType::Decision, 3), (TargetType::Decision, 2), (TargetType::Task, 1)],
            "--by is about who wrote it, which every comment table records"
        );
    }

    /// The two comment tables number their rows against **their own** table, so the same `id` can name a
    /// task comment and a decision comment at once — and the timeline has to keep them apart anyway. The
    /// sequence in the key is what does it: both rows are on the timeline, in a fixed order, and a cursor
    /// stopped between them consumes each exactly once.
    ///
    /// Without it the key is `(at, id)`, the two rows are indistinguishable, and the cursor below returns
    /// nothing — the decision comment is dropped and no later read ever offers it again.
    #[test]
    fn two_rows_sharing_one_at_and_id_stay_two_rows_to_the_cursor() {
        let (e, ledger) = fixture("collision");
        decision(&e, 9, 7, "決定のタイトル");
        comment(&e, 5, 2, 1, "タスクへの一言");
        decision_comment(&e, 5, 2, 9, "決定への一言"); // the same second and the same id

        let keys = |f: Filter| -> Vec<(TargetType, i64)> {
            page(&Ledger::open(&ledger), e.conn(), &f)
                .unwrap()
                .into_iter()
                .map(|it| (it.target_type, it.id))
                .collect()
        };
        assert_eq!(
            keys(Filter::default()),
            vec![(TargetType::Decision, 5), (TargetType::Task, 5)],
            "both are there, and the sequence fixes which comes first inside the second"
        );
        assert_eq!(
            keys(Filter { after: Some((at(2), Seq::Activity, 5)), oldest_first: true, ..Default::default() }),
            vec![(TargetType::Decision, 5)],
            "a cursor on the task comment still owes the reader the decision comment"
        );
        assert!(
            keys(Filter {
                after: Some((at(2), Seq::DecisionComment, 5)),
                oldest_first: true,
                ..Default::default()
            })
            .is_empty(),
            "and once past it, neither row comes again"
        );
    }

    /// A cursor sitting on the decision-comment sequence must not bound the **ledger** walk: its id comes
    /// from another counter, so it says nothing about how far back the ledger's own ids reach. Here the
    /// ledger's newer line carries the *smaller* number, which an id-bounded walk would stop short of and
    /// silently drop.
    #[test]
    fn a_decision_comments_cursor_does_not_cut_the_ledger_walk_short() {
        let (e, ledger) = fixture("cross-sequence-cursor");
        decision(&e, 9, 7, "決定のタイトル");
        decision_comment(&e, 9, 2, 9, "決定への一言"); // a high id, early
        system(&ledger, 1, 5, Some(1), json!({ "kind": "task.created", "title": "AI のタスク" })); // a low id, later

        let items = page(
            &Ledger::open(&ledger),
            e.conn(),
            &Filter {
                after: Some((at(2), Seq::DecisionComment, 9)),
                oldest_first: true,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            items.iter().map(|it| (it.kind, it.id)).collect::<Vec<_>>(),
            vec![(Kind::System, 1)],
            "the later ledger line is past the cursor, whatever its number is next to the other sequence's"
        );
    }
}
