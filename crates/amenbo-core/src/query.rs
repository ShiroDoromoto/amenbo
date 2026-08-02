//! Read queries: `task list` (filter/sort), `status`, and project list/detail. Every read is served
//! by indexed SQL against the read-model (the engine that holds the source of truth).

use chrono::NaiveDate;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::model::{ActorKind, DecisionStatus, Priority, TaskStatus};
use crate::time::{self, Timestamp};
use crate::view::{DecisionCompact, DecisionRef, Ref, TaskCompact};

// ───────────────────────── filter / sort skeleton (shared) ─────────────────────────

/// Shared parsing skeleton for filter expressions written as space-separated `key:value` tokens.
/// Each token is split on `:`; anything not in `key:value` form yields the same error. `apply` is
/// called once per `(key, value)` and folds it into the caller's filter (unknown keys and bad
/// values are rejected by returning `Err` from `apply`).
fn parse_filter_tokens(expr: &str, mut apply: impl FnMut(&str, &str) -> Result<()>) -> Result<()> {
    for token in expr.split_whitespace() {
        // The whitespace split happens first, so a value holding a space never arrives whole: it
        // arrives as a bare fragment with no `:` in it, which reads exactly like a typo. Nothing
        // looks at quotes or backslashes, so there is no way to write such a value — the one road
        // is the value's id. Name both readings, or the writer cannot tell which one they hit.
        let (key, value) = token.split_once(':').ok_or_else(|| {
            Error::invalid(format!(
                "filter '{token}' must be in key:value form — either a mistyped token, or a value holding whitespace (the split comes before any quoting, so such a value cannot be written: name it by its id instead)"
            ))
        })?;
        apply(key, value)?;
    }
    Ok(())
}

/// Shared skeleton for sort strings where a leading `-` means descending. The stripped `key` is
/// handed to `apply`, which sorts the slice (unknown keys are rejected there with `Err`); if the
/// spec asked for descending, the slice is reversed afterwards.
fn sort_by_spec<T>(
    items: &mut [T],
    sort: &str,
    apply: impl FnOnce(&mut [T], &str) -> Result<()>,
) -> Result<()> {
    let desc = sort.starts_with('-');
    let key = sort.trim_start_matches('-');
    apply(items, key)?;
    if desc {
        items.reverse();
    }
    Ok(())
}

// ───────────────────────── filter ─────────────────────────

#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub done: Option<bool>,
    /// Allowed set for `status:` — comma-separated values (`status:todo,in_progress`) match if the
    /// task has any of them (OR within the key). Never empty: the parser demands at least one value.
    pub status: Option<Vec<TaskStatus>>,
    pub due: Option<DueFilter>,
    /// `start:` — the declared start day, read as arrived / still ahead / never declared. The way to look
    /// at the waiting queue on purpose, rather than inferring it from a `ready:no` that three premises
    /// share.
    pub start: Option<StartFilter>,
    pub priority: Option<Option<Priority>>, // Some(None)=none, Some(Some(p))=that priority
    /// The reference written in `project:` (an id, or an exact name). Parsing only looks at grammar
    /// (it has no `conn`), so this stays an **unresolved raw string**; the entry point of the read
    /// turns it into an id via [`Filter::resolve`]. A reference that cannot be resolved is an error,
    /// not an empty result — silently returning nothing leaves the caller unable to tell "nothing
    /// matched" from "the name did not resolve".
    pub(crate) project_ref: Option<String>,
    /// The resolved project id. This is the only field the filtering looks at ([`Filter::resolve`]
    /// fills it in).
    pub project_id: Option<i64>,
    /// The words to narrow by. No longer reachable from the filter grammar — it is set structurally,
    /// from [`ListParams::text`] or from `search`'s own words — so nothing a caller *types* into a
    /// `--filter` fills it in (`AMB-D-449`).
    pub text: Option<String>,
    /// `number:` / `ref:` — filter by conversational ref (`AMB-T-<n>` / `AMB-D-<n>` / a bare number).
    pub number: Option<NumberFilter>,
    pub assignee: Option<AssigneeFilter>,
    /// `ai:true|false` — the AI-delegation dimension (`assignee_kind=ai`). Independent of the
    /// assignee dimension: it gathers everything delegated to an AI, whoever's. `me-ai` (my own AI)
    /// still exists separately on the assignee side.
    pub ai: Option<bool>,
    /// Derived: `ready:yes` = actionable, `ready:no` = blocked. `ready:no` catches not only tasks
    /// with open blockers but also tasks linked to a decision that is not live as a rationale — the
    /// two reasons [`crate::view::ReserveBlocker`] enumerates, which the reservation guard reads off
    /// the same derivation.
    pub ready: Option<bool>,
    /// Filtering by classification axis (dimension). Folds together `dim:<axis>=<value>` and
    /// `time_axis:<value>`, the sugar that names only the time axis. The key may appear several
    /// times (`dim:a=x dim:b=y`) and the elements AND together.
    pub dimensions: Vec<DimensionFilter>,
    /// `decision:<AMB-D-n | D-n | n>` — tasks linked to this decision (forward lookup through
    /// `decision_task_link`). Symmetric with `task:` on the `decision list` side: it makes the
    /// decision ⇄ task relation **traversable by query**. Compose it with `status:` and friends to
    /// ask for "the unfinished tasks this decision produced".
    pub decision: Option<u32>,
    /// `commit:<sha>` — tasks recording this commit SHA, the **reverse chain git → task**. A public
    /// commit carries no store-local ref, so the chain lives only on the task side; this key
    /// is the one face that walks it back. The value is normalised through the same
    /// `ops::commit::normalize` the door stores through, so a SHA looks up by the bytes it was stored as.
    /// Unlike `dim:` / `project:`, a SHA is a **free value, not a name the store knows**, so one that
    /// matches nothing is an empty result, not an error — and a short/non-full SHA (never stored, since
    /// the door admits full hex only) simply matches nothing rather than being rejected here.
    pub commit: Option<String>,
}

/// One `dim:<axis>=<value>` / `time_axis:<value>` token. The time axis is not a first-class
/// attribute — it is just a dimension carrying `role: time_axis` — so `time_axis:` is only sugar for
/// "the axis designated for that role" (it does not depend on the axis's name, so it works whatever
/// the user calls the axis, in any language).
#[derive(Clone, Debug)]
pub struct DimensionFilter {
    /// The axis reference (exact name, case-insensitive, or an exact id). `None` = `time_axis:` =
    /// whichever axis has role=time_axis. Parsing only looks at grammar (it has no `conn`), so this
    /// stays an **unresolved raw string** — [`Filter::resolve`] turns it into an id.
    pub(crate) axis: Option<String>,
    pub(crate) value: DimensionValueFilter,
    /// The resolved axis/value ([`Filter::resolve`] fills it in). This is the only field the
    /// filtering looks at, and an axis or value name that fails to resolve is an error, not an empty
    /// result (same contract as `project:`).
    pub(crate) resolved: Option<ResolvedDimension>,
}

/// The result of resolving a [`DimensionFilter`].
#[derive(Clone, Debug)]
pub(crate) struct ResolvedDimension {
    /// Ids of the axes matching the reference. Axis names are defined **per project**, so a name
    /// shared by several projects resolves to several ids — a cross-project `task list` ORs over all
    /// of them. Under `project:` the candidate tasks are already confined to that project, so the
    /// extra axis ids cannot widen the result.
    pub(crate) axis_ids: Vec<i64>,
    /// Ids of the values. `None` = `=none` (no live value of that axis is attached — unclassified).
    pub(crate) value_ids: Option<Vec<i64>>,
}

/// The value side of `dim:` / `time_axis:`.
#[derive(Clone, Debug)]
pub enum DimensionValueFilter {
    /// `none` — not a single live value of that axis is attached (unclassified). Same vocabulary as
    /// `priority:none` and friends.
    Unassigned,
    /// A value name (exact, case-insensitive) or an exact value id (resolved either way).
    Named(String),
}

impl DimensionValueFilter {
    /// Reject an empty value. Empty names nothing — neither a name nor an id — so `time_axis:` would
    /// degenerate into "no value given" and the filter would pass everything through. Catch it at
    /// the grammar level.
    fn parse(value: &str, empty: impl Fn() -> Error) -> Result<DimensionValueFilter> {
        if value.trim().is_empty() {
            return Err(empty());
        }
        Ok(if value == "none" {
            DimensionValueFilter::Unassigned
        } else {
            DimensionValueFilter::Named(value.to_string())
        })
    }
}

impl DimensionFilter {
    /// Parse the value of `dim:` (`<axis>=<value>`). Split on the first `=`, so the value side may
    /// itself contain `=`.
    fn parse_axis_value(spec: &str) -> Result<DimensionFilter> {
        let invalid = || {
            Error::invalid("dim must be <axis>=<value> (e.g. dim:Category=bug, dim:Category=none)")
        };
        let (axis, value) = spec.split_once('=').ok_or_else(invalid)?;
        if axis.trim().is_empty() {
            return Err(invalid());
        }
        Ok(DimensionFilter {
            axis: Some(axis.to_string()),
            value: DimensionValueFilter::parse(value, invalid)?,
            resolved: None,
        })
    }

    /// Resolve the axis and value references to ids. A mistyped axis or value name is an **error** —
    /// returning zero rows would leave the caller unable to tell "nothing matched" from "I mistyped
    /// the name", and on the `=none` (unclassified) side it is worse than zero rows: it turns into
    /// **every row**, because `NOT EXISTS` against a nonexistent axis is true for everyone.
    fn resolve(&mut self, conn: &rusqlite::Connection) -> Result<()> {
        use crate::ops::dimension::{NOUN, VALUE_NOUN};
        use crate::store_engine::read;
        let oops = crate::error::engine_on(conn);

        let axis_ids = match &self.axis {
            Some(reference) => {
                let hits = read::resolve_dimension_by_ref(conn, reference).map_err(&oops)?;
                if hits.is_empty() {
                    return Err(NOUN.not_found(reference));
                }
                hits
            }
            // `time_axis:` is sugar for **the axis designated by role**, not for an axis name. With
            // nothing designated, there is nothing to point at.
            None => {
                let hits = read::time_axis_dimensions(conn).map_err(&oops)?;
                if hits.is_empty() {
                    return Err(Error::not_found("no dimension is designated as the time axis"));
                }
                hits
            }
        };

        let value_ids = match &self.value {
            DimensionValueFilter::Unassigned => None,
            DimensionValueFilter::Named(reference) => {
                let hits =
                    read::resolve_dimension_value_by_ref(conn, &axis_ids, reference).map_err(&oops)?;
                if hits.is_empty() {
                    return Err(VALUE_NOUN.not_found(reference));
                }
                Some(hits)
            }
        };

        self.resolved = Some(ResolvedDimension { axis_ids, value_ids });
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum DueFilter {
    Today,
    Overdue,
    Week,
    None,
    On(NaiveDate),
}

/// The value of `start:` — the read-side counterpart of the start day that `ready` now stands on. It
/// answers "which tasks is a start day holding back, and until when", which `ready:no` alone cannot: that
/// lumps the three premises together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartFilter {
    /// The start day has arrived (declared, and on or before today) — the arm that is startable as far as
    /// the start day is concerned. Not "declared as today": a day that came and went is still arrived.
    Today,
    /// The start day is still ahead. The waiting queue.
    Future,
    /// No start day declared at all.
    None,
}

/// The value of `assignee:`. `me` / `me-ai` resolve through the facet (`assignee_kind` human/ai);
/// with a single store there is no id to name.
#[derive(Clone, Debug)]
pub enum AssigneeFilter {
    /// Unassigned.
    None,
    /// Me (the human facet). `me`.
    Me,
    /// My AI. `me-ai`.
    MeAi,
}

/// The value of `number:` / `ref:` — filtering by conversational number. Besides plain digits
/// (`123` / `#123`), it accepts the kind codes: `AMB-T-n` (task) and `AMB-D-n` (decision), the bare
/// `T-n` / `D-n` included. With a code the filter only matches on that side (`AMB-D-<n>` matches no
/// task; `AMB-T-<n>` matches no decision). Tasks and decisions live in separate number spaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumberFilter {
    /// The conversational number.
    pub number: u32,
    /// `Some` only when a kind prefix was given (`true` = decisions, `false` = tasks). A bare number
    /// or a bare `#n` leaves it `None`.
    pub require_decision: Option<bool>,
}

impl NumberFilter {
    /// Parse a filter value (`AMB-T-<n>` / `AMB-D-<n>`, or the bare `123` / `#123` / `T-123` / `D-123`). It
    /// shares the grammar helpers with
    /// reference resolution, so "what a number means" cannot drift apart between filtering and
    /// resolving.
    fn parse(value: &str) -> Result<NumberFilter> {
        use crate::ops::task::{parse_number_ref, parse_typed_ref, TypedKind};
        if let Some((kind, number)) = parse_typed_ref(value) {
            return Ok(NumberFilter { number, require_decision: Some(kind == TypedKind::Decision) });
        }
        if let Some(number) = parse_number_ref(value) {
            return Ok(NumberFilter { number, require_decision: None });
        }
        Err(Error::invalid(
            "number must be a conversational ref like AMB-T-<n> or AMB-D-<n> (the bare <n> / #<n> / T-<n> are read too)",
        ))
    }

    /// Match on the decision side (a `T-` prefix never matches a decision).
    fn matches_decision(&self, d: &crate::model::Decision) -> bool {
        if self.require_decision == Some(false) {
            return false;
        }
        d.id == i64::from(self.number)
    }
}

/// The filter value of `decision:` / `task:` — the conversational number of the *other* side (a bare
/// number, `#n`, or a kind-coded form such as `AMB-T-n`). Tasks and decisions have separate number spaces, so **the
/// opposite prefix is an error rather than an empty result**: a `T-12` handed to `decision:` is a
/// mix-up by the writer, and quietly returning nothing would read as "there is no such link".
fn parse_cross_ref(value: &str, want_decision: bool) -> Result<u32> {
    let nf = NumberFilter::parse(value)?;
    if nf.require_decision.is_some_and(|is_decision| is_decision != want_decision) {
        return Err(if want_decision {
            Error::invalid("decision must name a decision (AMB-D-<n> / D-<n> / <n>)")
        } else {
            Error::invalid("task must name a task (AMB-T-<n> / T-<n> / <n>)")
        });
    }
    Ok(nf.number)
}

impl Filter {
    /// Resolve the `project:` reference (an id, or an exact name) to a single project id. Parsing
    /// only looks at grammar (it has no `conn`), so the entry point of the read — which does have a
    /// `conn` — runs this once, right after parsing. A reference that fails to resolve is an
    /// **error**: returning zero rows would swallow the typo.
    pub fn resolve(&mut self, conn: &rusqlite::Connection) -> Result<()> {
        if let Some(reference) = self.project_ref.take() {
            self.project_id = Some(resolve_project_ref(conn, &reference)?);
        }
        for dimension in &mut self.dimensions {
            dimension.resolve(conn)?;
        }
        Ok(())
    }

    pub fn parse(expr: &str, today: NaiveDate) -> Result<Filter> {
        let mut f = Filter::default();
        parse_filter_tokens(expr, |key, value| {
            match key {
                "done" => {
                    f.done = Some(match value {
                        "true" => true,
                        "false" => false,
                        _ => return Err(Error::invalid("done must be true / false")),
                    })
                }
                "status" => {
                    // Several values may be given, comma-separated (`status:todo,in_progress`). A
                    // single unknown value is an error, and so is an empty element
                    // (`status:todo,`).
                    let mut statuses = Vec::new();
                    for part in value.split(',') {
                        let parsed = TaskStatus::parse(part).ok_or_else(|| {
                            Error::invalid("status must be todo / in_progress / done / blocked / rejected")
                        })?;
                        if !statuses.contains(&parsed) {
                            statuses.push(parsed);
                        }
                    }
                    f.status = Some(statuses)
                }
                "due" => {
                    f.due = Some(match value {
                        "today" => DueFilter::Today,
                        "overdue" => DueFilter::Overdue,
                        "week" => DueFilter::Week,
                        "none" => DueFilter::None,
                        other => DueFilter::On(time::parse_date(other, today)?),
                    })
                }
                "start" => {
                    // Three named arms and no bare date: `due:` takes one because a deadline is asked about
                    // by the day it falls on, whereas a start day is asked about by whether it has come.
                    f.start = Some(match value {
                        "today" => StartFilter::Today,
                        "future" => StartFilter::Future,
                        "none" => StartFilter::None,
                        _ => {
                            return Err(Error::invalid("start must be today / future / none"))
                        }
                    })
                }
                "priority" => {
                    f.priority = Some(match value {
                        "high" => Some(Priority::High),
                        "medium" => Some(Priority::Medium),
                        "low" => Some(Priority::Low),
                        "none" => None,
                        _ => return Err(Error::invalid("priority must be high / medium / low / none")),
                    })
                }
                "project" => f.project_ref = Some(value.to_string()),
                // `number:` and its alias `ref:` (synonyms — filter by conversational number).
                "number" | "ref" => f.number = Some(NumberFilter::parse(value)?),
                "assignee" => {
                    f.assignee = Some(match value {
                        "none" => AssigneeFilter::None,
                        "me" | "human" => AssigneeFilter::Me,
                        "me-ai" | "ai" => AssigneeFilter::MeAi,
                        _ => {
                            return Err(Error::invalid("assignee must be none / me / me-ai"))
                        }
                    })
                }
                // `ai:true|false` — the AI-delegation dimension (independent of assignee: it selects
                // what is delegated to an AI, whoever's).
                "ai" => {
                    f.ai = Some(match value {
                        "true" => true,
                        "false" => false,
                        _ => return Err(Error::invalid("ai must be true / false")),
                    })
                }
                // `ready:yes|no` and its alias `blocked:none|open` (synonyms).
                "ready" => {
                    f.ready = Some(match value {
                        "yes" => true,
                        "no" => false,
                        _ => return Err(Error::invalid("ready must be yes / no")),
                    })
                }
                "blocked" => {
                    f.ready = Some(match value {
                        "none" => true,
                        "open" => false,
                        _ => return Err(Error::invalid("blocked must be none / open")),
                    })
                }
                // Traverse the decision ⇄ task link (symmetric with `task:` on `decision list`).
                "decision" => f.decision = Some(parse_cross_ref(value, true)?),
                // Reverse chain git → task: tasks recording this commit SHA. Normalise through the same
                // `normalize` the door stores through, so lookup sees the stored bytes; no shape check —
                // a SHA is a free value, so a non-match (short/unknown SHA included) is an empty result,
                // not an error. Only an empty value is refused, being no SHA at all.
                "commit" => {
                    let sha = crate::ops::commit::normalize(value);
                    if sha.is_empty() {
                        return Err(Error::invalid("commit needs a sha (e.g. commit:<full 40/64-hex sha>)"));
                    }
                    f.commit = Some(sha);
                }
                // Filter across classification axes. `dim:` names any axis; `time_axis:` is sugar for
                // the axis designated for that role. Repeated tokens AND together (`dim:` is not a
                // single-value key — it accumulates).
                "dim" | "dimension" => f.dimensions.push(DimensionFilter::parse_axis_value(value)?),
                "time_axis" => {
                    let value = DimensionValueFilter::parse(value, || {
                        Error::invalid(
                            "time_axis must name a value of the time axis (e.g. time_axis:ops, time_axis:none)",
                        )
                    })?;
                    f.dimensions.push(DimensionFilter { axis: None, value, resolved: None })
                }
                // Words are no longer a filter key: they are `search`'s, which answers with the places
                // they are written rather than with a list of rows (`AMB-D-449`). Named on its own so
                // the one key that was taken away says where it went, instead of reading as a typo.
                "text" => {
                    return Err(Error::invalid(
                        "words are not a filter key — `search <word> …` finds where they are written (add --filter for the structural narrowing)",
                    ))
                }
                other => {
                    return Err(Error::invalid(
                        format!("unknown filter key '{other}' (done/status/due/start/priority/project/number/ref/assignee/ai/ready/decision/commit/dim/time_axis)"),
                    ))
                }
            }
            Ok(())
        })?;
        Ok(f)
    }

}

// ───────────────────────── task list ─────────────────────────

#[derive(Clone, Debug, Serialize)]
pub struct ListQueryEcho {
    pub project: Option<i64>,
    pub filter: Option<String>,
    pub sort: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskListResult {
    pub query: ListQueryEcho,
    pub count: usize,
    pub total_matched: usize,
    pub tasks: Vec<TaskCompact>,
    /// What a `ready:yes` query with no matches is not showing: the tasks the same query would have
    /// matched but for a start day still ahead. Always serialized (`null` when there is nothing to say),
    /// and only ever filled in on that path — see [`WaitingOnStart`].
    pub waiting_on_start: Option<WaitingOnStart>,
}

/// The waiting queue behind an empty mailbox: how many tasks a start day is holding back, and the earliest
/// of those days.
///
/// It exists because a start day mistyped far into the future is invisible in exactly the way that matters
/// — the task drops out of the mailbox, and an empty mailbox reads as "nothing to do" rather than "three
/// tasks, the first in eleven months". The count is what makes the mistake noticeable; the date is what
/// makes it obviously a mistake.
#[derive(Clone, Debug, Serialize)]
pub struct WaitingOnStart {
    pub count: usize,
    pub earliest: NaiveDate,
}

#[derive(Default)]
pub struct ListParams {
    pub project_id: Option<i64>,
    pub filter_expr: Option<String>,
    /// The free-text term, given **structurally** — the words reach every face of a task (its title and
    /// notes, the bodies of its live comments, the labels it is placed on, the names of what is attached
    /// to it). It is not in the filter grammar, and never was in a form that would do: the grammar splits
    /// on whitespace and so cannot carry more than one word, whereas a search box hands over whatever was
    /// typed, spaces and all. Several words are ANDed, each free to land on a different face. This is the
    /// list's own narrowing — a screen that already holds a list asking which of its rows the words
    /// touch; asking *where* words are written is `search`'s ([`search`], `AMB-D-449`). The twin of
    /// [`DecisionListParams::text`].
    pub text: Option<String>,
    pub sort: String,
    /// Page size (the first `limit` items in sort order). `None` = unlimited.
    pub limit: Option<usize>,
    /// How many items to skip from the front (paging further back through history). `None` = 0.
    pub offset: Option<usize>,
}

/// Paging of the same shape as activity's (`--limit` / `--offset`). Takes the fully sorted set of
/// matches `items`, skips `offset` of them and truncates to `limit`. Returns `(total matches before
/// paging, the page)` — the caller reports the total as `total_matched` and the page length as
/// `count`. `offset >= len` yields an empty page; `limit = None` means "everything from `offset` on".
pub(crate) fn paginate<T>(mut items: Vec<T>, offset: Option<usize>, limit: Option<usize>) -> (usize, Vec<T>) {
    let total = items.len();
    let off = offset.unwrap_or(0);
    if off >= items.len() {
        items.clear();
    } else if off > 0 {
        items.drain(0..off);
    }
    if let Some(n) = limit {
        items.truncate(n);
    }
    (total, items)
}

/// The `task list` read. Selection (filter / project / sort / total) is computed by **indexed SQL**
/// over the engine read-model ([`crate::store_engine::list_task_ids`]): placement, dependency and
/// sort are all index-served `WHERE` / `ORDER BY` terms. Only the ids that made it onto the page are
/// hydrated, straight from the SQL read-model
/// ([`crate::store_engine::hydrate_task_cards`] — O(output), never a full-store walk). `reach` is
/// **always** taken as an argument: forcing the scope to be declared in the type means a read that
/// forgets it does not compile, so containment does not rest on the author remembering it.
pub fn list(
    conn: &rusqlite::Connection,
    reach: crate::reach::Reach,
    params: ListParams,
) -> Result<TaskListResult> {
    use crate::store_engine::{self, TaskQuery};

    let today = time::today();
    let mut filter = match &params.filter_expr {
        Some(e) => Filter::parse(e, today)?,
        None => Filter::default(),
    };
    if let Some(text) = &params.text {
        filter.text = Some(text.clone());
    }
    // `project:` may be written as a name or an id. This is the entry point of the read — the layer
    // that holds the `conn` — so resolve it exactly once, before building any SQL. A name that does
    // not resolve errors here instead of degenerating into an empty result.
    filter.resolve(conn)?;

    // Folding the reach into the scope happens in the same one place. If the reach is closed over a
    // single project, an unspecified scope is filled in with the bound project. Naming a project
    // with `project:` is human vocabulary, and inside a closed reach it is an error — even when it
    // names the bound project itself. Same discipline as never quietly returning zero rows for what
    // you cannot see: do not make what you cannot choose look choosable.
    if filter.project_id.is_some() {
        reach.refuse_project_choice("the `project:` filter")?;
    }
    let project_id = reach.narrow(params.project_id)?;
    filter.project_id = reach.narrow(filter.project_id)?;

    // Paging is pushed down to SQL `LIMIT`/`OFFSET` to keep the read O(result). `total_matched` (the
    // number of matches before paging) comes back from `list_task_ids` as a separate COUNT.
    let page = store_engine::list_task_ids(
        conn,
        &TaskQuery {
            reach,
            project_id,
            filter: &filter,
            sort: &params.sort,
            today,
            limit: params.limit,
            offset: params.offset,
        },
    )
    .map_err(crate::error::engine_on(conn))?;

    // Keeping the order SQL produced, hydrate only the ids on this page, straight from the
    // read-model. Ids with no live row drop out (an invariant of `hydrate_task_cards`).
    let tasks = store_engine::hydrate_task_cards(conn, reach, &page.ids, today)
        .map_err(crate::error::engine_on(conn))?;
    let count = tasks.len();

    // Nothing matched a query that asked for ready work: say what a start day is holding back, so an empty
    // mailbox cannot be read as "nothing to do" when it means "not yet". The same filter is reused with
    // `ready:` dropped and `start:future` in its place — anything else would count tasks the caller never
    // asked about (someone else's, another project's). Off the empty path this is not read at all.
    let waiting_on_start = if page.total_matched == 0 && filter.ready == Some(true) {
        let waiting = Filter { ready: None, start: Some(StartFilter::Future), ..filter.clone() };
        store_engine::waiting_on_start(
            conn,
            &TaskQuery {
                reach,
                project_id,
                filter: &waiting,
                sort: &params.sort,
                today,
                limit: None,
                offset: None,
            },
        )
        .map_err(crate::error::engine_on(conn))?
        .map(|(count, earliest)| WaitingOnStart { count, earliest })
    } else {
        None
    };

    Ok(TaskListResult {
        query: ListQueryEcho {
            project: params.project_id,
            filter: params.filter_expr,
            sort: params.sort,
        },
        total_matched: page.total_matched,
        count,
        tasks,
        waiting_on_start,
    })
}

/// Comparison that always sorts `None` last.
fn cmp_opt<T: Ord>(a: Option<T>, b: Option<T>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

// ───────────────────────── status ─────────────────────────

#[derive(Clone, Debug, Serialize)]
pub struct OverdueTask {
    #[serde(flatten)]
    pub task: TaskCompact,
    pub days_overdue: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Suggestion {
    pub id: i64,
    pub title: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct StatusCounts {
    pub overdue: usize,
    pub due_today: usize,
    pub in_progress: usize,
    pub upcoming_7d: usize,
    pub no_due: usize,
    pub completed_today: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct StatusResult {
    pub scope: String,
    pub generated_at: Timestamp,
    pub today_date: NaiveDate,
    pub counts: StatusCounts,
    pub overdue: Vec<OverdueTask>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_today: Option<Vec<TaskCompact>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_this_week: Option<Vec<TaskCompact>>,
    pub in_progress: Vec<TaskCompact>,
    pub next_suggested: Vec<Suggestion>,
}

/// Serves the `status` read from the source-of-truth engine with indexed SQL. Bucket selection and
/// counting are done by [`crate::store_engine::read::status_bucket_ids`] and card hydration by
/// [`crate::store_engine::hydrate_task_cards`], both O(output) — neither walks the whole store. The
/// order within each bucket is decided by the read-model's `ORDER BY`.
pub fn status(conn: &rusqlite::Connection, scope: &str, reach: crate::reach::Reach) -> Result<StatusResult> {
    let today = time::today();
    // Buckets and counts alike only see inside the reach (an AI's `status` must not mirror the whole
    // store back at it).
    let buckets = crate::store_engine::read::status_bucket_ids(conn, today, reach)
        .map_err(crate::error::engine_on(conn))?;

    let hydrate = |ids: &[i64]| -> Result<Vec<TaskCompact>> {
        crate::store_engine::hydrate_task_cards(conn, reach, ids, today)
            .map_err(crate::error::engine_on(conn))
    };

    // The overdue rows and the raw material for `next_suggested` (overdue / due_today) are needed
    // whatever the scope, so always hydrate them. `due_week` only for the week scope.
    let overdue_cards = hydrate(&buckets.overdue)?;
    let due_today_cards = hydrate(&buckets.due_today)?;
    let in_progress_cards = hydrate(&buckets.in_progress)?;
    let due_week_cards = if scope == "week" { Some(hydrate(&buckets.due_week)?) } else { None };

    let counts = StatusCounts {
        overdue: overdue_cards.len(),
        due_today: due_today_cards.len(),
        in_progress: in_progress_cards.len(),
        upcoming_7d: buckets.upcoming_7d,
        no_due: buckets.no_due,
        completed_today: buckets.completed_today,
    };

    // next_suggested: up to 3, taken from overdue first (worst first), then due today (high priority
    // first).
    let mut suggestions: Vec<Suggestion> = Vec::new();
    for t in overdue_cards.iter().take(3) {
        let days = t.due_on.map(|d| today.signed_duration_since(d).num_days()).unwrap_or(0);
        let pri = t.priority.map(|p| format!("・優先度 {}", p.as_str())).unwrap_or_default();
        suggestions.push(Suggestion {
            id: t.id,
            title: t.title.clone(),
            reason: format!("{days}日 期限超過{pri}"),
        });
    }
    for t in due_today_cards.iter() {
        if suggestions.len() >= 3 {
            break;
        }
        let pri = t.priority.map(|p| format!("・優先度 {}", p.as_str())).unwrap_or_default();
        suggestions.push(Suggestion {
            id: t.id,
            title: t.title.clone(),
            reason: format!("本日締切{pri}"),
        });
    }

    let overdue: Vec<OverdueTask> = overdue_cards
        .into_iter()
        .map(|t| {
            let days_overdue =
                t.due_on.map(|d| today.signed_duration_since(d).num_days()).unwrap_or(0);
            OverdueTask { task: t, days_overdue }
        })
        .collect();

    let (due_today_out, due_this_week) = match scope {
        "week" => (None, due_week_cards),
        "overdue" => (None, None),
        _ => (Some(due_today_cards), None),
    };

    Ok(StatusResult {
        scope: scope.to_string(),
        generated_at: Timestamp::now(),
        today_date: today,
        counts,
        overdue,
        due_today: due_today_out,
        due_this_week,
        in_progress: in_progress_cards,
        next_suggested: suggestions,
    })
}

// ───────────────────────── project ─────────────────────────

#[derive(Clone, Debug, Serialize)]
pub struct ProjectListItem {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
    pub default_view: crate::model::View,
    pub archived: bool,
    pub num_dimensions: usize,
    pub num_tasks: usize,
    pub order_key: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectListResult {
    pub count: usize,
    pub projects: Vec<ProjectListItem>,
}

/// The **language-independent default display name** of a facet (human / ai). Core's projection
/// layers (activity / comment / roster), which have no config, name authors and assignees with this
/// default label. Callers (CLI/GUI) may override it with `human_name` / `ai_name` from the config.
pub fn facet_label(kind: Option<ActorKind>) -> String {
    match kind {
        Some(ActorKind::Ai) => crate::config::default_ai_name(None),
        _ => crate::config::default_human_name(None),
    }
}

/// The stable token of a facet (for the `id` field of DTOs): `human` / `ai`, or empty when unset.
pub fn facet_kind_str(kind: Option<ActorKind>) -> String {
    kind.map(|k| k.as_str().to_string()).unwrap_or_default()
}

/// The SQL path of `project list`. Indexed SQL over the engine read-model
/// ([`crate::store_engine::read::project_list`]) pulls the live projects in `order_key` order and
/// folds `num_dimensions` / `num_tasks` in with correlated subqueries — no re-walking dimensions and
/// tasks once per project.
pub fn project_list(
    conn: &rusqlite::Connection,
    include_archived: bool,
) -> Result<ProjectListResult> {
    let rows = crate::store_engine::read::project_list(conn, include_archived)
        .map_err(crate::error::engine_on(conn))?;
    let items: Vec<ProjectListItem> = rows
        .into_iter()
        .map(|r| ProjectListItem {
            id: r.id,
            name: r.name,
            color: r.color,
            default_view: crate::model::View::parse(&r.default_view).unwrap_or_default(),
            archived: r.archived,
            num_dimensions: r.num_dimensions,
            num_tasks: r.num_tasks,
            order_key: r.order_key,
        })
        .collect();
    Ok(ProjectListResult {
        count: items.len(),
        projects: items,
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectTaskCounts {
    pub total: usize,
    pub completed: usize,
    pub incomplete: usize,
    pub overdue: usize,
    pub due_today: usize,
    pub no_due: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectDetail {
    pub id: i64,
    pub resource_type: &'static str,
    pub name: String,
    pub notes: String,
    pub color: Option<String>,
    pub default_view: crate::model::View,
    pub archived: bool,
    pub order_key: String,
    pub task_counts: ProjectTaskCounts,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// The SQL path of `project show`. The project row comes from [`crate::store_engine::read::project`]
/// and the count summary from [`crate::store_engine::read::project_task_counts`], which returns it
/// in a single aggregate. No live row for the project means `not_found`.
pub fn project_detail(
    conn: &rusqlite::Connection,
    project_id: i64,
) -> Result<ProjectDetail> {
    let today = time::today();
    let p = crate::store_engine::read::project(conn, project_id)
        .map_err(crate::error::engine_on(conn))?
        
        .ok_or_else(|| {
            Error::not_found(format!("project '{project_id}' not found"))
        })?;

    let c = crate::store_engine::read::project_task_counts(conn, project_id, today)
        .map_err(crate::error::engine_on(conn))?;
    let task_counts = ProjectTaskCounts {
        total: c.total,
        completed: c.completed,
        incomplete: c.total - c.completed,
        overdue: c.overdue,
        due_today: c.due_today,
        no_due: c.no_due,
    };

    Ok(ProjectDetail {
        id: p.id,
        resource_type: "project",
        name: p.name,
        notes: p.notes,
        color: p.color,
        default_view: p.default_view,
        archived: p.archived,
        order_key: p.order_key,
        task_counts,
        created_at: p.created_at,
        updated_at: p.updated_at,
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct DiscoverResult {
    pub today_date: NaiveDate,
    pub summary: StatusCounts,
    pub today: Vec<TaskCompact>,
    pub next_suggested: Vec<Suggestion>,
    pub hints: Vec<String>,
}

// ───────────────────────── member / comment ─────────────────────────

/// One entry of the member roster. The actor is a facet (human / ai); `is_self` marks the human one.
#[derive(Clone, Debug, Serialize)]
pub struct MemberItem {
    pub name: String,
    pub is_self: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct MemberListResult {
    pub count: usize,
    pub members: Vec<MemberItem>,
}

/// The member roster (the GUI's assignee dropdown and the like). It is the two names from the config
/// (human / ai): `is_self` marks the human facet (this local actor).
pub fn members(config: &crate::config::Config) -> MemberListResult {
    let members: Vec<MemberItem> = config
        .roster()
        .into_iter()
        .map(|(kind, name)| MemberItem {
            name,
            is_self: kind == ActorKind::Human,
        })
        .collect();
    MemberListResult { count: members.len(), members }
}

#[derive(Clone, Debug, Serialize)]
pub struct CommentItem {
    pub id: i64,
    /// The author, for display (the default label). The real thing is the facet; `author_kind` is
    /// authoritative.
    pub author: Ref,
    /// The author's facet (human / ai). Callers look the display name up from the config.
    pub author_kind: Option<ActorKind>,
    pub text: String,
    pub created_at: Timestamp,
    /// When the body was later corrected (an in-place edit); `None` if it never was. This is the only
    /// clue a reader gets that "this is not the text I read a moment ago" — no revision history is
    /// kept, so the fact of the edit surfaces nowhere else.
    pub edited_at: Option<Timestamp>,
}

/// Turns a read-model row ([`crate::store_engine::read::CommentRow`]) into a [`CommentItem`] for
/// display. Task comments and decision comments arrive as the same row type, so hydration goes
/// through this one place too.
fn comment_item(r: crate::store_engine::read::CommentRow) -> CommentItem {
    let created_at = crate::time::Timestamp::parse_rfc3339(&r.created_at).unwrap_or_default();
    let edited_at =
        r.edited_at.as_deref().map(|t| crate::time::Timestamp::parse_rfc3339(t).unwrap_or_default());
    let author_kind = r.author_kind.as_deref().and_then(ActorKind::parse);
    CommentItem {
        author: Ref { id: facet_kind_str(author_kind), name: facet_label(author_kind) },
        author_kind,
        id: r.id,
        text: r.text,
        created_at,
        edited_at,
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CommentListResult {
    pub task: crate::view::TaskRef,
    /// Total number of comments before paging (the whole count when `--limit` / `--offset` are in
    /// play).
    pub total_matched: usize,
    pub count: usize,
    pub comments: Vec<CommentItem>,
}

/// The SQL path of `comment list`. Live comments come from indexed SQL over the engine read-model
/// ([`crate::store_engine::read::comment_list`] — the `task_comment` table). If the task is not
/// live (no live row in the read-model), `not_found`.
pub fn comment_list(conn: &rusqlite::Connection, task_id: i64, offset: Option<usize>, limit: Option<usize>) -> Result<CommentListResult> {
    use crate::store_engine::read;
    let title = read::task_title(conn, task_id)
        .map_err(crate::error::engine_on(conn))?
        .ok_or_else(|| Error::not_found(format!("task '{task_id}' not found")))?;
    // Rows arrive oldest-first (`read::comment_list` = `created_at ASC`). Slice out just the page
    // with offset/limit — O(result). `total_matched` is the count before paging.
    let rows = read::comment_list(conn, task_id)
        .map_err(crate::error::engine_on(conn))?;
    let (total_matched, rows) = paginate(rows, offset, limit);
    let comments: Vec<CommentItem> = rows.into_iter().map(comment_item).collect();
    Ok(CommentListResult {
        task: crate::view::TaskRef { id: task_id, name: title },
        total_matched,
        count: comments.len(),
        comments,
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct DecisionCommentListResult {
    pub decision: DecisionRef,
    /// Total number of comments before paging (the whole count when `--limit` / `--offset` are in
    /// play).
    pub total_matched: usize,
    pub count: usize,
    pub comments: Vec<CommentItem>,
}

/// The SQL path of `comment list` for decision records (O(result)). Live comments come from indexed
/// SQL over the engine read-model ([`crate::store_engine::read::decision_comment_list`]). If the
/// decision is not live (no live row in the read-model), `not_found`.
pub fn decision_comment_list(conn: &rusqlite::Connection, decision_id: i64, offset: Option<usize>, limit: Option<usize>) -> Result<DecisionCommentListResult> {
    use crate::store_engine::read;
    let title = read::decision_title(conn, decision_id)
        .map_err(crate::error::engine_on(conn))?
        .ok_or_else(|| Error::not_found(format!("decision '{decision_id}' not found")))?;
    let rows = read::decision_comment_list(conn, decision_id)
        .map_err(crate::error::engine_on(conn))?;
    let (total_matched, rows) = paginate(rows, offset, limit);
    let comments: Vec<CommentItem> = rows.into_iter().map(comment_item).collect();
    Ok(DecisionCommentListResult {
        decision: DecisionRef { id: decision_id, name: Some(title) },
        total_matched,
        count: comments.len(),
        comments,
    })
}

/// Resolves a `task` reference (`AMB-T-n`, or the bare `T-n` / `#n` / `n`) to a single live task id. It is an indexed SQL
/// lookup against the read-model, so a pre-write lookup never drags an O(n) scan in with it. To
/// avoid defining the grammar twice, the pieces are reused straight from ops: parsing the reference
/// (`parse_*_ref`), collapsing the hits (`pick_id` over 0 / 1 / many) and the not-found message
/// (`NOUN`). Numbers are **globally unique on this machine**, so no project context is needed.
/// Ambiguity yields `AmbiguousId`.
pub fn resolve_task_ref(conn: &rusqlite::Connection, input: &str) -> Result<i64> {
    use crate::store_engine::read;
    use crate::ops::pick_id;
    use crate::ops::task::{parse_number_ref, parse_typed_ref, TypedKind, NOUN};

    let s = input.trim();
    let oops = crate::error::engine_on(conn);

    // Resolution by number. Numbers are globally unique, so no project context is needed.
    let by_number = |number: u32, token: &str| -> Result<i64> {
        let hits = read::task_ids_by_number(conn, number).map_err(&oops)?;
        pick_id(hits, token, || NOUN.not_found(token))
    };

    // 0) Kind prefixes `T-` / `D-`. As a task, `D-n` is simply "not found".
    if let Some((kind, number)) = parse_typed_ref(s) {
        return match kind {
            TypedKind::Task => by_number(number, s),
            TypedKind::Decision => Err(NOUN.not_found(s)),
        };
    }
    // 1) `#n` / `n` (decimal).
    if let Some(number) = parse_number_ref(s) {
        return by_number(number, s);
    }
    Err(NOUN.not_found(s))
}

/// Resolves a `decision` reference (`AMB-D-n`, or the bare `D-n` / `#n` / `n`) to a single live decision id, by indexed SQL
/// against the read-model. Decisions live in **a number space of their own, separate from tasks**, so
/// only the `decision` table is consulted and passing `AMB-T-n` yields "decision not found". Parsing
/// (`parse_*_ref`), collapsing the hits (`pick_id`) and the not-found message (decision's `NOUN`) are
/// reused from ops.
pub fn resolve_decision_ref(conn: &rusqlite::Connection, input: &str) -> Result<i64> {
    use crate::store_engine::read;
    use crate::ops::decision::NOUN;
    use crate::ops::pick_id;
    use crate::ops::task::{parse_number_ref, parse_typed_ref, TypedKind};

    let s = input.trim();
    let oops = crate::error::engine_on(conn);

    // Resolution by number. Numbers are globally unique, so no project context is needed.
    let by_number = |number: u32, token: &str| -> Result<i64> {
        let hits = read::decision_ids_by_number(conn, number).map_err(&oops)?;
        pick_id(hits, token, || NOUN.not_found(token))
    };

    // 0) Kind prefixes `T-` / `D-`. As a decision, `T-n` is simply "not found".
    if let Some((kind, number)) = parse_typed_ref(s) {
        return match kind {
            TypedKind::Decision => by_number(number, s),
            TypedKind::Task => Err(NOUN.not_found(s)),
        };
    }
    // 1) `#n` / `n` (decimal).
    if let Some(number) = parse_number_ref(s) {
        return by_number(number, s);
    }
    Err(NOUN.not_found(s))
}

/// Resolves a cross-kind conversational reference (`AMB-T-n` / `AMB-D-n`, or a bare `#n` / `n`) to either a Task or a
/// Decision, by indexed SQL against the read-model. The kind prefixes `T-` / `D-` are delegated to
/// the per-kind resolvers; a bare `#n` / `n` (carrying no kind code) is looked up across **both tables** (task and decision),
/// and if the same number exists on both sides the result is an ambiguity error that asks for a
/// prefix. Collapsing the hits (0 / 1 / many) reuses `pick` / `pick_anywhere` from ops.
pub fn resolve_any(conn: &rusqlite::Connection, input: &str) -> Result<crate::ops::Ref> {
    use crate::store_engine::read;
    use crate::ops::task::{parse_number_ref, parse_typed_ref, TypedKind};
    use crate::ops::{pick, pick_anywhere, Ref};

    let s = input.trim();
    let oops = crate::error::engine_on(conn);

    // Collect the tasks and decisions carrying this number as `Ref`s. Numbers are globally unique, so
    // no project context is needed — the only ambiguity left across the two tables is the kind
    // itself, `#n` as a task versus `D-n` as a decision.
    let by_number = |number: u32| -> Result<Vec<Ref>> {
        let mut v = Vec::new();
        for id in read::task_ids_by_number(conn, number).map_err(&oops)? {
            v.push(Ref::Task(id));
        }
        for id in read::decision_ids_by_number(conn, number).map_err(&oops)? {
            v.push(Ref::Decision(id));
        }
        Ok(v)
    };

    // 0) Kind prefixes `T-` / `D-`. Delegate to the per-kind SQL resolver and wrap the result in a
    // `Ref`.
    if let Some((kind, _)) = parse_typed_ref(s) {
        return match kind {
            TypedKind::Task => resolve_task_ref(conn, s).map(Ref::Task),
            TypedKind::Decision => resolve_decision_ref(conn, s).map(Ref::Decision),
        };
    }
    // 1) `#n` / `n` (decimal).
    if let Some(number) = parse_number_ref(s) {
        return pick_anywhere(by_number(number)?, number);
    }
    pick(Vec::new(), s)
}

/// Resolves a `project` reference (an id, or an exact name) to a single live project id, by indexed
/// SQL against the read-model. Collapsing the hits (`pick_id`) and the not-found message (project's
/// `NOUN`) are reused from ops.
pub fn resolve_project_ref(conn: &rusqlite::Connection, reference: &str) -> Result<i64> {
    use crate::ops::pick_id;
    use crate::ops::project::NOUN;
    let oops = crate::error::engine_on(conn);
    let hits = crate::store_engine::read::resolve_project(conn, reference).map_err(&oops)?;
    pick_id(hits, reference, || NOUN.not_found(reference))
}

// ───────────────────────── activity (unified timeline) ─────────────────────────

/// Who emitted an activity row (a person, or that person's AI).
#[derive(Clone, Debug, Serialize)]
pub struct ActivityAuthor {
    pub name: String,
    pub kind: Option<ActorKind>,
}

/// What an activity row is about (a task, a decision, or a project).
#[derive(Clone, Debug, Serialize)]
pub struct ActivityTarget {
    #[serde(rename = "type")]
    pub target_type: String,
    pub id: i64,
    pub title: String,
    /// Whether the target still exists in the read-model. Rows about a deleted target stay in the
    /// ledger (that is what a ledger is for), but there is nothing left to open — this lets a reader
    /// tell apart the rows that have a name but no destination.
    pub live: bool,
}

/// The output shape of one activity row (a system event or a comment).
#[derive(Clone, Debug, Serialize)]
pub struct ActivityItem {
    pub id: i64,
    pub at: Timestamp,
    #[serde(rename = "type")]
    pub kind: String,
    pub author: ActivityAuthor,
    pub target: ActivityTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Comment rows only: when the body was later corrected (an in-place edit). Absent if it never
    /// was. The timeline (`amenbo activity`) is the main way people read this stream, so it states
    /// the same fact `comment list` does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<Timestamp>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ActivityResult {
    pub count: usize,
    /// An opaque cursor: pass it back as `--since <cursor>` to get only what is newer than this
    /// response, moving forward in time (the equivalent of an AI watching the stream). In incremental
    /// mode it points at the newest event returned; in history mode it points at the current head of
    /// the matching stream (the bootstrap point from which to start subscribing incrementally).
    /// `None` when nothing matched. The token is opaque so that callers cannot come to depend on its
    /// internal representation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// In incremental mode: whether unconsumed events remain beyond the returned window (i.e. whether
    /// `--limit` cut it short).
    pub has_more: bool,
    pub items: Vec<ActivityItem>,
}

/// Prefix of activity's opaque incremental cursor, which denotes one point in the total order
/// `(created_at, seq, id)`. The internal representation is hidden: it travels as a base64url string
/// starting with `cur2_`. The prefix exists so that `--since` can tell a cursor from a date
/// expression (`today` / `+3d` / `YYYY-MM-DD` never start with it). `seq` is in the key because the
/// timeline's sources do **not** all share one id sequence: the file ledger and `task_comment` do
/// ([`crate::store_engine::read::next_activity_id`]), but `decision_comment` numbers its own rows, so
/// without it the same id appears twice within one second and cutting the stream at a cursor starts
/// dropping or duplicating rows (see [`crate::activity`]).
const ACTIVITY_CURSOR_PREFIX: &str = "cur2_";

/// The first cursor spelling — still **read, never written**. It carried `(at, id)` with no sequence,
/// from before the timeline had a third source, so every token of it names a row on the shared activity
/// sequence and reads back as one. Accepting it is what keeps a reader that was mid-stream when this build
/// arrived from losing its place: the alternative is `--since` rejecting the token it was handed a minute
/// earlier.
const ACTIVITY_CURSOR_PREFIX_V1: &str = "cur1_";

/// Encodes `(at, seq, id)` into an opaque cursor.
pub fn encode_activity_cursor(at: &Timestamp, seq: crate::activity::Seq, id: i64) -> String {
    use base64::Engine;
    let raw = format!("{}\n{}\n{}", at.to_rfc3339_z(), seq.rank(), id);
    format!("{ACTIVITY_CURSOR_PREFIX}{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw.as_bytes()))
}

/// Whether a string looks like a cursor. Used to tell dates from cursors in `--since` — and it answers for
/// the older spelling too, so a token this build cannot read is reported as a bad *cursor* rather than
/// being taken for a date and refused as a bad date.
pub fn looks_like_activity_cursor(s: &str) -> bool {
    s.starts_with(ACTIVITY_CURSOR_PREFIX) || s.starts_with(ACTIVITY_CURSOR_PREFIX_V1)
}

/// Decodes a cursor back into `(at, seq, id)`. Malformed input (missing prefix, broken base64, a rank no
/// sequence answers to, …) gives `None`.
pub fn parse_activity_cursor(s: &str) -> Option<(Timestamp, crate::activity::Seq, i64)> {
    use base64::Engine;
    let (body, versioned) = match s.strip_prefix(ACTIVITY_CURSOR_PREFIX) {
        Some(body) => (body, true),
        None => (s.strip_prefix(ACTIVITY_CURSOR_PREFIX_V1)?, false),
    };
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(body).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let (at_s, rest) = text.split_once('\n')?;
    let at = Timestamp::parse_rfc3339(at_s)?;
    if !versioned {
        // The v1 form is `(at, id)`: no sequence was recorded because there was only one to record.
        return Some((at, crate::activity::Seq::Activity, rest.parse().ok()?));
    }
    let (rank, id) = rest.split_once('\n')?;
    let seq = crate::activity::Seq::from_rank(rank.parse().ok()?)?;
    Some((at, seq, id.parse().ok()?))
}

#[derive(Default)]
pub struct ActivityParams {
    /// A resolved task id (only this task's activity).
    pub task_id: Option<i64>,
    /// A resolved project id (only the activity of tasks belonging to this project).
    pub project_id: Option<i64>,
    /// From this date on (from 00:00 of that day). History mode, newest first. Mutually exclusive
    /// with `since_cursor`.
    pub since: Option<NaiveDate>,
    /// The `(at, seq, id)` origin decoded from an opaque cursor (`--since <cursor>`). When set, the read
    /// runs in incremental mode: only what is strictly newer than the cursor, oldest-first (moving
    /// forward in time). Mutually exclusive with `since` (the date).
    pub since_cursor: Option<(Timestamp, crate::activity::Seq, i64)>,
    /// Narrow to system events or comments.
    pub kind: Option<crate::activity::Kind>,
    /// Narrow to the facet that emitted the row.
    pub actor: Option<ActorKind>,
    /// Recipient scope (`--for me`): keep only rows whose target task is assigned to this facet. A
    /// different axis from `actor` (`--by`), which narrows by emitter — this one narrows by
    /// addressee, and it is how an AI picks out "the part I am supposed to act on".
    pub for_facet: Option<ActorKind>,
    /// Maximum number of rows (taken newest first).
    pub limit: Option<usize>,
    /// How many rows to skip, newest first (paging further back through history). Defaults to 0.
    pub offset: Option<usize>,
}

/// Returns the unified timeline **newest first** (people and AIs read the same stream). The stream is
/// a merge of three sources ([`crate::activity`]): system events come from the file ledger (outside the
/// source of truth, bounded), while comments come from the first-class `task_comment` and
/// `decision_comment` tables (permanent data, never lost). All that is left here is the arithmetic of
/// the window, the cursor
/// and `has_more`. `has_more` is not a COUNT: we **ask for `limit + 1` rows and see whether they
/// overflow**, so no extra full aggregate is needed. History mode's bootstrap cursor is the newest
/// row *before* offset is applied, so it comes from a separate `limit 1` read. `reach` is **always**
/// taken as an argument: forcing the scope to be declared in the type means a read that forgets it
/// does not compile.
pub fn activity(
    ledger: &std::path::Path,
    conn: &rusqlite::Connection,
    reach: crate::reach::Reach,
    params: ActivityParams,
) -> Result<ActivityResult> {
    let incremental = params.since_cursor.is_some();
    // The timeline is closed over the reach too: no events or comments from tasks outside the bound
    // scope.
    let project_id = reach.narrow(params.project_id)?;
    let filter = crate::activity::Filter {
        task_id: params.task_id,
        project_id,
        since: params.since,
        after: params.since_cursor,
        kind: params.kind,
        author_kind: params.actor,
        for_facet: params.for_facet,
        oldest_first: incremental,
        // Ask for one row more than the window: that alone tells us whether anything follows it,
        // without counting the whole set.
        limit: params.limit.map(|n| n + 1),
        offset: if incremental { 0 } else { params.offset.unwrap_or(0) },
    };

    // The ledger is read once per request — history mode then pages over it twice, for the window and
    // for the cursor.
    let lines = crate::activity::Ledger::open(ledger);
    let mut rows = crate::activity::page(&lines, conn, &filter)?;
    let has_more = params.limit.map(|n| rows.len() > n).unwrap_or(false);
    if let Some(n) = params.limit {
        rows.truncate(n);
    }
    // The cursor is cut from the merged row, not from its output shape: the key carries the row's id
    // sequence, and `ActivityItem` does not — a reader has no use for it, only the stream's own paging does.
    let tail = rows.last().map(|it| (it.at, it.seq(), it.id));
    let items: Vec<ActivityItem> = rows.into_iter().map(activity_item).collect();

    if let Some((cur_at, cur_seq, cur_id)) = &params.since_cursor {
        // Incremental mode: the last row of the returned window (the newest) becomes the next cursor.
        // If the window is empty, keep the incoming cursor so the reader does not lose its place.
        let cursor = tail
            .map(|(at, seq, id)| encode_activity_cursor(&at, seq, id))
            .or_else(|| Some(encode_activity_cursor(cur_at, *cur_seq, *cur_id)));
        return Ok(ActivityResult { count: items.len(), cursor, has_more, items });
    }

    // History mode: the bootstrap cursor is the current head of the matching stream, taken before
    // offset/limit trim anything.
    let newest = crate::activity::page(
        &lines,
        conn,
        &crate::activity::Filter { limit: Some(1), offset: 0, ..filter },
    )?;
    let cursor = newest.first().map(|it| encode_activity_cursor(&it.at, it.seq(), it.id));
    Ok(ActivityResult { count: items.len(), cursor, has_more, items })
}

/// Turns one merged row into its output shape. Only comment rows carry `text`, only system rows carry
/// `event`.
fn activity_item(it: crate::activity::Item) -> ActivityItem {
    ActivityItem {
        id: it.id,
        at: it.at,
        kind: it.kind.as_str().to_string(),
        author: ActivityAuthor {
            name: facet_label(it.author_kind),
            kind: it.author_kind,
        },
        target: ActivityTarget {
            target_type: it.target_type.as_str().to_string(),
            id: it.target_id,
            title: it.title,
            live: it.target_live,
        },
        event: it.event,
        text: it.text,
        edited_at: it.edited_at,
    }
}

// ───────────────────────── decision list / search ─────────────────────────

/// Search filter for decision records (`status:` / `superseded:` / `project:` / `number:` / `task:` /
/// `decided_before:` / `decided_after:`). Decisions have no mailbox state the way tasks do, so there are
/// few keys: status, time and the edges are enough. Words are not among them — they are `search`'s
/// ([`search`], `AMB-D-449`) — though [`DecisionFilter::text`] is still how the read carries them.
#[derive(Clone, Debug, Default)]
pub struct DecisionFilter {
    pub status: Option<DecisionStatus>,
    /// `superseded:yes|no` — whether another decision draws a `supersedes` edge at this one. It keys on
    /// the edge itself, which is a fact the author declared, rather than on a word for "still in force"
    /// that nothing here can know (`AMB-D-410`).
    pub superseded: Option<bool>,
    /// The words to narrow by, over the word index (title, body, comment bodies, attachment names).
    /// Set structurally — from [`DecisionListParams::text`] or from `search` — never from a `--filter`
    /// anyone types.
    pub text: Option<String>,
    /// The reference written in `project:` (same meaning as [`Filter::project_ref`] on the task side —
    /// an unresolved raw string).
    pub(crate) project_ref: Option<String>,
    /// The resolved project id ([`DecisionFilter::resolve`] fills it in).
    pub project_id: Option<i64>,
    /// `number:` / `ref:` — filter by conversational number (`D-80` / `#80` / a bare number).
    pub number: Option<NumberFilter>,
    /// `task:<AMB-T-n | T-n | n>` — decisions linked to this task (reverse lookup through
    /// `decision_task_link`). Symmetric with `decision:` on the `task list` side: it makes the
    /// decision ⇄ task relation **traversable by query**.
    pub task: Option<u32>,
    /// `decided_before:<date>` — accepted on or before this day (the day `decided_at` fell on where
    /// the reader is ≤ date; the day itself is **included**). "What had been decided as of some point
    /// in time" is not a feature of its own: it falls out of composing this ordinary filter key with
    /// `superseded:`. Decisions never accepted (proposed / rejected, with no `decided_at`) match
    /// neither direction.
    pub decided_before: Option<NaiveDate>,
    /// `decided_after:<date>` — accepted on or after this day (the day `decided_at` fell on where the
    /// reader is ≥ date; the day itself is **included**). The counterpart of `decided_before`: give
    /// both and you have a span, inclusive at each end.
    pub decided_after: Option<NaiveDate>,
}

impl DecisionFilter {
    /// Resolve the `project:` reference (an id or a name) to an id. Same contract as [`Filter::resolve`]
    /// on the task side: a reference that fails to resolve is an error.
    pub fn resolve(&mut self, conn: &rusqlite::Connection) -> Result<()> {
        if let Some(reference) = self.project_ref.take() {
            self.project_id = Some(resolve_project_ref(conn, &reference)?);
        }
        Ok(())
    }

    /// `today` is the reference day for relative dates (`today`, `-30d`, …). Same entry point as
    /// [`Filter::parse`] on the task side.
    pub fn parse(expr: &str, today: NaiveDate) -> Result<DecisionFilter> {
        let mut f = DecisionFilter::default();
        parse_filter_tokens(expr, |key, value| {
            match key {
                "status" => {
                    f.status = Some(DecisionStatus::parse(value).ok_or_else(|| {
                        Error::invalid("status must be proposed / accepted / rejected")
                    })?)
                }
                "superseded" => {
                    f.superseded = Some(match value {
                        "yes" | "true" => true,
                        "no" | "false" => false,
                        _ => {
                            return Err(Error::invalid("superseded must be yes / no"))
                        }
                    })
                }
                "project" => f.project_ref = Some(value.to_string()),
                // `number:` and its alias `ref:` (synonyms — filter by conversational number).
                "number" | "ref" => f.number = Some(NumberFilter::parse(value)?),
                // Traverse the decision ⇄ task link (symmetric with `decision:` on `task list`).
                "task" => f.task = Some(parse_cross_ref(value, false)?),
                // Filter by acceptance time. Day granularity, and the named day is included.
                "decided_before" => f.decided_before = Some(time::parse_date(value, today)?),
                "decided_after" => f.decided_after = Some(time::parse_date(value, today)?),
                // Same as the task side: the key that was taken away names its successor.
                "text" => {
                    return Err(Error::invalid(
                        "words are not a filter key — `search <word> … --kind decision` finds where they are written",
                    ))
                }
                other => {
                    return Err(Error::invalid(
                        format!("unknown filter key '{other}' (status/superseded/project/number/ref/task/decided_before/decided_after)"),
                    ))
                }
            }
            Ok(())
        })?;
        Ok(f)
    }

    /// Whether a decision was superseded lives in the edges, not in a column on the decision, so the
    /// caller looks it up and passes it in. Likewise the two id sets read once and passed in so nothing
    /// is re-queried per decision: `linked_to_task` = the live decisions linked to the task named by
    /// `task:` (or `None` when `task:` was not given), and `text_hits` = the decisions the words landed on
    /// (or `None` when no words were given), read whole off the word index — title, body and comment
    /// bodies alike — so the words are folded there and not a second time here.
    fn matches(
        &self,
        d: &crate::model::Decision,
        superseded: bool,
        linked_to_task: Option<&[i64]>,
        text_hits: Option<&[i64]>,
    ) -> bool {
        if self.task.is_some() && !linked_to_task.is_some_and(|ids| ids.contains(&d.id)) {
            return false;
        }
        if let Some(status) = self.status {
            if d.status != status {
                return false;
            }
        }
        if let Some(want) = self.superseded {
            if superseded != want {
                return false;
            }
        }
        if self.text.is_some() && !text_hits.is_some_and(|ids| ids.contains(&d.id)) {
            // The whole of the word narrowing is the caller's one read off the word index (a set membership, not a
            // per-decision re-query): every face a word may land on is in there, so there is nothing
            // left to re-check against the record's own columns.
            return false;
        }
        debug_assert!(self.project_ref.is_none(), "DecisionFilter::resolve was not run");
        if let Some(project_id) = self.project_id {
            if project_id != d.project_id {
                return false;
            }
        }
        if let Some(nf) = &self.number {
            if !nf.matches_decision(d) {
                return false;
            }
        }
        // Acceptance time. A decision that was never accepted has no date, so it matches neither
        // direction.
        if self.decided_before.is_some() || self.decided_after.is_some() {
            let Some(decided) = d.decided_at.map(|t| t.local_date()) else {
                return false;
            };
            if self.decided_before.is_some_and(|d0| decided > d0) {
                return false;
            }
            if self.decided_after.is_some_and(|d0| decided < d0) {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct DecisionListResult {
    pub query: ListQueryEcho,
    /// Total number of matches before paging (the whole count when `--limit` / `--offset` are in
    /// play).
    pub total_matched: usize,
    pub count: usize,
    pub decisions: Vec<DecisionCompact>,
}

/// The order `decision list` uses when none is named: newest first. Named here rather than only in the
/// CLI's `default_value`, so the in-code default and the command-line one are the same string.
pub const DECISION_SORT_DEFAULT: &str = "-created";

#[derive(Default)]
pub struct DecisionListParams {
    pub project_id: Option<i64>,
    pub filter_expr: Option<String>,
    /// The free-text term, given **structurally** — the words reach the same three places (title, body,
    /// any live comment body). It is not in the filter grammar, and never was in a form that would do: the
    /// grammar splits on whitespace and so cannot carry more than one word, whereas a search box hands over
    /// whatever was typed, spaces and all. Several words are ANDed, each free to land on a different face
    /// (`AMB-D-450`). The twin of [`ListParams::text`]: this is the listing's own narrowing, and asking
    /// where words are written is `search`'s ([`search`], `AMB-D-449`).
    pub text: Option<String>,
    /// `decided` / `created` / `number` / `title` / `status` (leading `-` for descending). Empty is the
    /// default ([`DECISION_SORT_DEFAULT`]), so a caller that builds these params in code and leaves the
    /// field to `Default` gets the documented order rather than an unknown-key error.
    pub sort: String,
    /// Page size (the first `limit` items in sort order). `None` = unlimited.
    pub limit: Option<usize>,
    /// How many items to skip from the front. `None` = 0.
    pub offset: Option<usize>,
    /// Projection that puts each decision's `body` on its card. Off by default (a light,
    /// title-only listing). It **composes** with `--filter` / `--limit` / `--offset` — it only adds a
    /// body column to whatever page the filter and paging already selected, it is not a dump of
    /// everything. The point is to read the bodies of a page bounded by keyword or axis in one go:
    /// it removes the N round-trips of calling `decision show` once per decision while keeping the
    /// amount read bounded.
    pub with_body: bool,
}

/// The SQL path of `decision list`. Live decisions come from indexed SQL over the engine read-model
/// ([`crate::store_engine::read::decision_list`] — a `decision` × `project` LEFT JOIN plus a
/// correlated subquery over live links and live tasks for `linked_task_count`), which avoids
/// re-walking projects and tasks once per decision. The filter (status/text/project) and the sort
/// (decided/created/number/title/status) are applied by [`DecisionFilter::matches`] and
/// [`sort_decisions`] to a partial `Decision` assembled from each row; the `project` ref and
/// `linked_task_count` come straight from the row. `reach` is **always** taken as an argument:
/// forcing the scope to be declared in the type means a read that forgets it does not compile.
pub fn decision_list(
    conn: &rusqlite::Connection,
    reach: crate::reach::Reach,
    params: DecisionListParams,
) -> Result<DecisionListResult> {
    use crate::store_engine::read;
    let mut filter = match &params.filter_expr {
        Some(e) => DecisionFilter::parse(e, time::today())?,
        None => DecisionFilter::default(),
    };
    if let Some(text) = &params.text {
        filter.text = Some(text.clone());
    }
    filter.resolve(conn)?; // As on the task side: names are accepted, unresolvable refs are an error.

    // Fold the reach into the scope, and refuse a `project:` by name from inside a closed reach (same
    // shape as the task side).
    if filter.project_id.is_some() {
        reach.refuse_project_choice("the `project:` filter")?;
    }
    let project_id = reach.narrow(params.project_id)?;
    filter.project_id = reach.narrow(filter.project_id)?;

    let rows = read::decision_list(conn, reach, project_id)
        .map_err(crate::error::engine_on(conn))?;

    // The link set behind `task:` — read once, live links and live decisions only.
    let linked_to_task = match filter.task {
        Some(n) => Some(
            read::decisions_for_task(conn, i64::from(n))
                .map_err(crate::error::engine_on(conn))?
                .into_iter()
                .map(|r| r.id)
                .collect::<Vec<i64>>(),
        ),
        None => None,
    };

    // The words in full — the decisions every term lands on, over the word index, read once. The
    // in-memory match below then asks only whether a decision is in this set, so the words are folded
    // the one way the index folds them rather than a second way here. None when no words were given.
    let text_hits = match &filter.text {
        Some(t) => {
            let terms = crate::store_engine::search::terms(t);
            Some(read::decisions_matching_text(conn, &terms).map_err(crate::error::engine_on(conn))?)
        }
        None => None,
    };

    // Row → a partial `Decision` (only the fields filter/sort need) plus what the card needs on the
    // side (the project ref and `linked_task_count`).
    struct Entry {
        decision: crate::model::Decision,
        project: Option<crate::view::ProjectRef>,
        linked_task_count: usize,
        /// The ids of the decisions that superseded it, as the row carried them. The filter reads
        /// whether there are any; the card spells them into refs.
        superseded_by: Vec<i64>,
    }
    let mut entries: Vec<Entry> = rows
        .into_iter()
        .map(|r| {
            let decision = crate::model::Decision {
                id: r.id,
                project_id: r.project_id,
                title: r.title,
                body: r.body,
                status: crate::model::DecisionStatus::parse(&r.status).unwrap_or_default(),
                decided_at: r.decided_at.as_deref().and_then(crate::time::Timestamp::parse_rfc3339),
                created_at: crate::time::Timestamp::parse_rfc3339(&r.created_at).unwrap_or_default(),
                ..Default::default()
            };
            let project = r.project_name.map(|name| crate::view::ProjectRef { id: r.project_id, name });
            Entry {
                decision,
                project,
                linked_task_count: r.linked_task_count,
                superseded_by: r.superseded_by,
            }
        })
        .filter(|e| {
            filter.matches(
                &e.decision,
                !e.superseded_by.is_empty(),
                linked_to_task.as_deref(),
                text_hits.as_deref(),
            )
        })
        .collect();

    // `sort_decisions` sorts a `&mut [&Decision]`. To bring `entries` into the same order, sort the
    // references, then read off each id's rank and sort `entries` by it.
    let mut refs: Vec<&crate::model::Decision> = entries.iter().map(|e| &e.decision).collect();
    // No sort named is the documented default, not an unknown key. The CLI always names one (clap fills
    // it in), so this is the door for a caller that builds the params in code and takes the derived
    // `Default` for the field — which the field's own documentation promises is `-created`.
    let sort = if params.sort.is_empty() { DECISION_SORT_DEFAULT } else { &params.sort };
    sort_decisions(&mut refs, sort)?;
    let order: std::collections::HashMap<i64, usize> =
        refs.iter().enumerate().map(|(i, d)| (d.id, i)).collect();
    entries.sort_by_key(|e| order[&e.decision.id]);

    let mut all: Vec<DecisionCompact> = entries
        .iter()
        .map(|e| {
            crate::view::decision_compact_with(
                &e.decision,
                e.project.clone(),
                e.linked_task_count,
                &e.superseded_by,
            )
        })
        .collect();
    // `--with-body`: add the body column to each card (using the row's `body`).
    if params.with_body {
        for (card, e) in all.iter_mut().zip(entries.iter()) {
            card.body = Some(e.decision.body.clone());
        }
    }
    // Paging: slice the full, filtered and sorted match set with offset/limit.
    let (total_matched, decisions) = paginate(all, params.offset, params.limit);
    let count = decisions.len();
    Ok(DecisionListResult {
        query: ListQueryEcho {
            project: params.project_id,
            filter: params.filter_expr,
            sort: params.sort,
        },
        total_matched,
        count,
        decisions,
    })
}

/// The SQL path of `decision show`. Indexed SQL over the engine read-model
/// ([`crate::store_engine::read::decision_detail`]) pulls every field of a single decision in one go
/// — the project name, both directions of supersedes/superseded_by, the `decided_by` name, and
/// `linked_tasks` (live links × live tasks) — and the row is assembled into a
/// [`crate::view::DecisionDetail`]. If the resolved id has no row, `not_found`.
pub fn decision_detail(
    conn: &rusqlite::Connection,
    decision_id: i64,
) -> Result<crate::view::DecisionDetail> {
    use crate::store_engine::read;
    use crate::time::Timestamp;
    let row = read::decision_detail(conn, decision_id)
        .map_err(crate::error::engine_on(conn))?
        .ok_or_else(|| {
            Error::not_found(format!("decision '{decision_id}' not found"))
        })?;

    let project = row.project_name.map(|name| crate::view::ProjectRef { id: row.project_id, name });
    // Edges between decisions. The forward direction (supersedes / amends) fetches the target's title
    // whether or not the target is still live, leaving `name` as `None` when it dangles — the face
    // composes the placeholder. The reverse direction only ever yields live decisions, so a title is
    // always there (wrapped in `Some`).
    let forward = |edges: Vec<(i64, Option<String>)>| -> Vec<DecisionRef> {
        edges.into_iter().map(|(id, title)| DecisionRef { id, name: title }).collect()
    };
    let reverse = |edges: Vec<(i64, String)>| -> Vec<DecisionRef> {
        edges.into_iter().map(|(id, name)| DecisionRef { id, name: Some(name) }).collect()
    };
    let supersedes = forward(row.edges.supersedes);
    let superseded_by = reverse(row.edges.superseded_by);
    let amends = forward(row.edges.amends);
    let amended_by = reverse(row.edges.amended_by);
    // A premise carries what replaced it along with the reference (the successor is given as a
    // conversational reference of the form `D-40`).
    let builds_on = row
        .edges
        .builds_on
        .into_iter()
        .map(|p| crate::view::PremiseRef {
            id: p.id,
            name: p.title,
            superseded_by: p.superseded_by.map(crate::idref::decision),
        })
        .collect();
    let built_on_by = reverse(row.edges.built_on_by);
    // `decided_by` is a TEXT token read into both id and name, so whenever the id is present the name is
    // too — the `unwrap_or_default` never fires. Core keeps no display placeholder here.
    let decided_by = row.decided_by_id.map(|id| Ref { id, name: row.decided_by_name.unwrap_or_default() });
    // The tasks a decision produced come out with their status attached, so that "is the work this
    // decision called for finished?" can be read from the decision's side.
    let linked_tasks = row
        .linked_tasks
        .into_iter()
        .map(|t| crate::view::LinkedTaskRef {
            id: t.id,
            name: t.title,
            status: crate::model::TaskStatus::parse(&t.status).unwrap_or_default(),
        })
        .collect();

    Ok(crate::view::DecisionDetail {
        r#ref: crate::idref::decision(row.id),
        id: row.id,
        resource_type: "decision",
        project,
        title: row.title,
        body: row.body,
        status: crate::model::DecisionStatus::parse(&row.status).unwrap_or_default(),
        supersedes,
        superseded_by,
        amends,
        amended_by,
        builds_on,
        built_on_by,
        decided_at: row.decided_at.as_deref().and_then(Timestamp::parse_rfc3339),
        decided_by,
        linked_tasks,
        created_at: Timestamp::parse_rfc3339(&row.created_at).unwrap_or_default(),
        updated_at: Timestamp::parse_rfc3339(&row.updated_at).unwrap_or_default(),
    })
}

/// The SQL path of `task show`. Indexed SQL over the engine read-model
/// ([`crate::store_engine::read::task_detail`]) pulls every field of a single task in one go — its
/// placement, the assignee facet, open blockers (with titles) and the comment count — and the row is
/// assembled into a [`crate::view::TaskDetail`]. If the resolved id has no row, `not_found`.
pub fn task_detail(
    conn: &rusqlite::Connection,
    task_id: i64,
) -> Result<crate::view::TaskDetail> {
    use crate::store_engine::read;
    let row = read::task_detail(conn, task_id)
        .map_err(crate::error::engine_on(conn))?
        .ok_or_else(|| {
            Error::not_found(format!("task '{task_id}' not found"))
        })?;

    let parse_date =
        |s: &Option<String>| s.as_deref().and_then(|v| NaiveDate::parse_from_str(v, "%Y-%m-%d").ok());
    let parse_kind = |s: &Option<String>| s.as_deref().and_then(ActorKind::parse);

    let placement = row.placement.map(|m| crate::view::PlacementView {
        project: crate::view::ProjectRef {
            id: m.project_id,
            name: m.project_name.unwrap_or_default(),
        },
        order_key: m.order_key,
    });

    let blocked_by: Vec<crate::view::TaskRef> =
        row.blocked_by.into_iter().map(|(id, name)| crate::view::TaskRef { id, name }).collect();
    let blocked_by_decisions: Vec<DecisionRef> =
        row.blocked_by_decisions.into_iter().map(|(id, name)| DecisionRef { id, name: Some(name) }).collect();
    let start_on = parse_date(&row.start_on);
    let today = time::today();
    // The one ready predicate, shared with the task card and the `ready:` filter, and beside it the
    // third reason it can return false.
    let ready =
        crate::view::is_ready(!blocked_by.is_empty(), !blocked_by_decisions.is_empty(), start_on, today);
    let not_started_until = crate::view::not_started_until(start_on, today);
    let blocks: Vec<crate::view::TaskRef> =
        row.blocks.into_iter().map(|(id, name)| crate::view::TaskRef { id, name }).collect();

    let status = TaskStatus::parse(&row.status).unwrap_or_default();
    let dimensions = read::task_classification(conn, task_id)
        .map_err(crate::error::engine_on(conn))?
        .into_iter()
        .map(|(dimension, value)| crate::view::ClassifiedAs { dimension, value })
        .collect();

    Ok(crate::view::TaskDetail {
        r#ref: crate::idref::task(row.id),
        id: row.id,
        resource_type: "task",
        title: row.title,
        notes: row.notes,
        subtype: crate::model::Subtype::parse(&row.subtype).unwrap_or_default(),
        completed: status == TaskStatus::Done,
        completed_at: row.completed_at.as_deref().and_then(Timestamp::parse_rfc3339),
        status,
        created_by_kind: parse_kind(&row.created_by_kind),
        assignee_kind: parse_kind(&row.assignee_kind),
        start_on,
        due_on: parse_date(&row.due_on),
        priority: row.priority.as_deref().and_then(Priority::parse),
        placement,
        dimensions,
        blocked_by,
        blocked_by_decisions,
        not_started_until,
        ready,
        blocks,
        num_comments: row.num_comments,
        created_at: Timestamp::parse_rfc3339(&row.created_at).unwrap_or_default(),
        updated_at: Timestamp::parse_rfc3339(&row.updated_at).unwrap_or_default(),
    })
}

/// Premises that moved **after a task's current status began** (`AMB-D-366`, `AMB-D-373`) — the read a
/// caller invokes to surface, to a holder, that their reservation may have been silently undercut: a blocker
/// or an unsettled decision pinned on after they reserved, or a decision that was already linked and has
/// since stopped being settled. Read-only: it reports *what* changed; the caller decides how strongly to
/// react. A missing task is `not_found`; a task never stamped (an older store) reports no change.
pub fn premise_change_since(
    conn: &rusqlite::Connection,
    task_id: i64,
) -> Result<crate::view::PremiseChange> {
    use crate::store_engine::read;
    let row = read::premise_change_since(conn, task_id)
        .map_err(crate::error::engine_on(conn))?
        .ok_or_else(|| {
            Error::not_found(format!("task '{task_id}' not found"))
        })?;
    Ok(crate::view::PremiseChange {
        added_blockers: row
            .added_blockers
            .into_iter()
            .map(|(id, name)| crate::view::TaskRef { id, name })
            .collect(),
        added_decisions: row
            .added_decisions
            .into_iter()
            .map(|(id, name)| DecisionRef { id, name: Some(name) })
            .collect(),
        reopened_decisions: row
            .reopened_decisions
            .into_iter()
            .map(|(id, name)| DecisionRef { id, name: Some(name) })
            .collect(),
    })
}

/// Orders the fetched rows by the sort key (for [`decision_list`]). Decision ordering is deliberately
/// not pushed down to SQL: the count is bounded per project, so sorting in memory is more direct than
/// assembling an `ORDER BY`.
fn sort_decisions(decisions: &mut [&crate::model::Decision], sort: &str) -> Result<()> {
    sort_by_spec(decisions, sort, |decisions, key| {
        match key {
            // Chronological (accepted at / created at). `None` (never accepted) sorts last.
            "decided" => decisions.sort_by(|a, b| cmp_opt(a.decided_at, b.decided_at)),
            "created" => decisions.sort_by_key(|d| d.created_at),
            "number" => decisions.sort_by_key(|d| d.id),
            "title" => decisions.sort_by(|a, b| a.title.cmp(&b.title)),
            "status" => decisions.sort_by(|a, b| a.status.as_str().cmp(b.status.as_str())),
            other => {
                return Err(Error::invalid(
                    format!("unknown sort key '{other}' (decided/created/number/title/status; - for descending)"),
                ))
            }
        }
        Ok(())
    })
}

// ───────────────────────── search ─────────────────────────

/// The order hits come back in (`AMB-D-449`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SearchSort {
    /// The default: the face first ([`HitFace`]), and within a face the newest first. A word in a name is
    /// a stronger answer to "where is this written" than the same word in a paragraph.
    #[default]
    Face,
    /// The plain timeline, newest first — the face's weight taken off.
    Newest,
    /// The plain timeline, oldest first.
    Oldest,
}

/// The default sort, named here rather than only in the CLI's `default_value`, so the in-code default and
/// the command-line one are the same string.
pub const SEARCH_SORT_DEFAULT: &str = "face";

/// How many hits a search returns when the caller names no limit. Unlike a listing, `search` **has** a
/// default ceiling (`AMB-D-449`): one hit carries a snippet, so a query with no limit on it would answer
/// a common word with a wall of text. The total says how much was left behind.
pub const SEARCH_LIMIT_DEFAULT: usize = 20;

impl SearchSort {
    /// Parse the `--sort` spec. `-` leads a descending key, as everywhere else here; `face` has no
    /// descending form, because the weight of a face is not a scale to walk backwards.
    pub fn parse(spec: &str) -> Result<Self> {
        match spec.trim() {
            "" | "face" => Ok(Self::Face),
            "-time" => Ok(Self::Newest),
            "time" => Ok(Self::Oldest),
            other => Err(Error::invalid(format!(
                "unknown sort key '{other}' (face/time/-time; `face` weights the face, `-time` is newest first)"
            ))),
        }
    }

    /// The spec this order was written as — what the result echoes back.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Face => "face",
            Self::Newest => "-time",
            Self::Oldest => "time",
        }
    }
}

/// Which hits a search keeps — three narrowings, not a partition. Two of them name **whose** words they
/// are and the third **which face**, because that is how the three are asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchKind {
    /// The words on a task: its own faces, its comments, its labels, what is attached to it.
    Task,
    /// The words on a decision, the same way.
    Decision,
    /// The words in a comment, on either side.
    Comment,
}

impl SearchKind {
    /// Parse the `--kind` value.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "task" => Ok(Self::Task),
            "decision" => Ok(Self::Decision),
            "comment" => Ok(Self::Comment),
            other => Err(Error::invalid(format!(
                "unknown kind '{other}' (task/decision/comment — the words on a task, on a decision, or in a comment)"
            ))),
        }
    }

    /// The value this narrowing was written as — what the result echoes back.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Decision => "decision",
            Self::Comment => "comment",
        }
    }
}

pub use crate::store_engine::search::HitFace;

/// One place a word is written: the face it landed on, the record that face belongs to, and a short
/// excerpt around the match.
#[derive(Clone, Debug, Serialize)]
pub struct SearchHit {
    /// Which face of the record the words are on.
    pub face: HitFace,
    /// Which side the record is: `task` or `decision`. The face alone does not say — a title is either.
    pub kind: String,
    /// The record's conversational ref (`AMB-T-<n>` / `AMB-D-<n>`) and its title: what the reader opens to
    /// read the whole of it.
    pub r#ref: String,
    pub title: String,
    /// The comment the words are in, or the one the attachment hangs off (`AMB-TC-<n>` / `AMB-DC-<n>`).
    /// `None` when the hit is on the record's own faces.
    pub comment: Option<String>,
    /// The hit's own instant: a comment's posting time, or when the text it sits in was last written.
    pub at: Timestamp,
    /// The excerpt, in the characters the person wrote ([`crate::store_engine::search::snippet`]). A
    /// pointer at where something is written, not the reading itself — the whole text is `show` and
    /// `comment list`'s to give.
    pub snippet: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SearchQueryEcho {
    /// The words as they were typed.
    pub text: String,
    pub filter: Option<String>,
    /// The `--kind` value, as it was written (`null` when the search was not narrowed to one).
    pub kind: Option<String>,
    pub sort: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SearchResult {
    pub query: SearchQueryEcho,
    /// How many hits this page holds.
    pub count: usize,
    /// How many there are in all — what tells the reader a default ceiling left something behind.
    pub total_matched: usize,
    pub hits: Vec<SearchHit>,
}

#[derive(Default)]
pub struct SearchParams {
    /// The words, as typed. Split on whitespace and folded by the index ([`crate::store_engine::search`]).
    pub text: String,
    /// The structural narrowing, in `task list`'s own grammar. Task vocabulary, so a search carrying one
    /// is a search of tasks.
    pub filter_expr: Option<String>,
    pub kind: Option<SearchKind>,
    pub sort: SearchSort,
    /// Page size. `None` takes [`SEARCH_LIMIT_DEFAULT`] — this is the one read where "no limit named" is
    /// not "everything".
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// The `search` read: every place the words are written, hit by hit (`AMB-D-449`).
///
/// Selection, order and paging are all the engine's ([`crate::store_engine::read::search_hits`]) — the
/// page is cut in SQL, so a common word costs the page rather than the store. What is left here is the
/// shape a face reads: the record's ref, the comment's ref where there is one, the instant parsed back
/// out of its stored spelling, and the snippet — which is cut here, on the page's rows alone, because
/// cutting it is work per hit and only these hits are shown.
///
/// **A word that is a ref names the record itself.** Nothing about a record carries the ref it is called
/// by, so `AMB-T-12` reaches only the places that *mention* it — never the task. A word in that shape
/// therefore pins the record it names to the top ([`pinned`]), and the words go on searching as words. That
/// is what lets someone holding a number type the same command as someone holding a phrase.
///
/// `reach` is **always** taken as an argument: forcing the scope to be declared in the type means a read
/// that forgets it does not compile.
pub fn search(
    conn: &rusqlite::Connection,
    reach: crate::reach::Reach,
    params: SearchParams,
) -> Result<SearchResult> {
    let today = time::today();
    let mut filter = match &params.filter_expr {
        Some(e) => Some(Filter::parse(e, today)?),
        None => None,
    };
    // The same entry-point discipline as `list`: `project:` is resolved exactly once, here where the
    // `conn` is, and naming a project inside a closed reach is refused rather than quietly obeyed.
    if let Some(f) = filter.as_mut() {
        f.resolve(conn)?;
        if f.project_id.is_some() {
            reach.refuse_project_choice("the `project:` filter")?;
        }
        f.project_id = reach.narrow(f.project_id)?;
    }

    // The refs among the words, resolved to the records they name. A pin is a line of the **first** page,
    // and it takes its share of that page — a `--limit 5` is five lines whatever they are. Paging past
    // them walks the index alone, shifted by however many lines they took.
    let pins = pinned(conn, reach, &params.text)?;
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(SEARCH_LIMIT_DEFAULT);
    let shown_pins = if offset == 0 { pins.clone() } else { Vec::new() };

    let terms = crate::store_engine::search::terms(&params.text);
    let page = crate::store_engine::read::search_hits(
        conn,
        &crate::store_engine::read::SearchQuery {
            reach,
            terms: &terms,
            filter: filter.as_ref(),
            today,
            kind: params.kind,
            sort: params.sort,
            limit: Some(limit.saturating_sub(shown_pins.len())),
            offset: offset.saturating_sub(pins.len()),
        },
    )
    .map_err(crate::error::engine_on(conn))?;

    let mut hits = shown_pins;
    hits.extend(page.hits.into_iter().map(|h| {
        let is_task = h.owner_kind == crate::store_engine::search::DATASET_TASK;
        SearchHit {
            face: h.face,
            r#ref: if is_task {
                crate::idref::task(h.owner_id)
            } else {
                crate::idref::decision(h.owner_id)
            },
            comment: h.comment_id.map(|id| {
                if is_task {
                    crate::idref::task_comment(id)
                } else {
                    crate::idref::decision_comment(id)
                }
            }),
            kind: h.owner_kind,
            title: h.owner_title,
            at: Timestamp::parse_rfc3339(&h.at).unwrap_or_default(),
            snippet: crate::store_engine::search::snippet(&h.text, &terms),
        }
    }));

    Ok(SearchResult {
        query: SearchQueryEcho {
            text: params.text,
            filter: params.filter_expr,
            kind: params.kind.map(|k| k.as_str().to_string()),
            sort: params.sort.as_str().to_string(),
        },
        count: hits.len(),
        // A pin is a line the index could not have produced, so it is counted beside the ones it did —
        // and counted on every page, or the total would shrink as the reader walked forward.
        total_matched: page.total_matched + pins.len(),
        hits,
    })
}

/// The records the words name outright — a word written as a ref (`AMB-T-<n>` / `AMB-D-<n>`, the bare
/// `T-<n>` / `D-<n>` included), read as the record it points at rather than as a word.
///
/// The **raw** words are read, not the folded terms: a ref is a spelling, and the fold has already
/// lower-cased it. A ref naming nothing live, or something outside the reach, pins nothing — a search must
/// not become a way to ask whether a record exists somewhere it cannot be read.
fn pinned(
    conn: &rusqlite::Connection,
    reach: crate::reach::Reach,
    text: &str,
) -> Result<Vec<SearchHit>> {
    use crate::ops::task::{parse_typed_ref, TypedKind};
    let mut out = Vec::new();
    for word in text.split_whitespace() {
        let Some((kind, number)) = parse_typed_ref(word) else { continue };
        let is_task = kind == TypedKind::Task;
        let dataset = if is_task {
            crate::store_engine::search::DATASET_TASK
        } else {
            crate::store_engine::search::DATASET_DECISION
        };
        let id = i64::from(number);
        let Some(head) = crate::store_engine::read::record_headline(conn, dataset, id)
            .map_err(crate::error::engine_on(conn))?
        else {
            continue;
        };
        if !reach.allows(head.project_id) {
            continue;
        }
        out.push(SearchHit {
            face: HitFace::Title,
            kind: dataset.to_string(),
            r#ref: if is_task { crate::idref::task(id) } else { crate::idref::decision(id) },
            title: head.title.clone(),
            comment: None,
            at: Timestamp::parse_rfc3339(&head.at).unwrap_or_default(),
            snippet: head.title,
        });
    }
    Ok(out)
}


/// `discover` (bare `amenbo`, with no arguments): today's tasks plus what to do next. The raw material
/// is `status`, read from the engine with indexed SQL ([`status`]); [`discover_from`] assembles it.
pub fn discover(conn: &rusqlite::Connection, reach: crate::reach::Reach) -> Result<DiscoverResult> {
    Ok(discover_from(status(conn, "today", reach)?))
}

/// Builds discover out of the `status --today` result. The "today" column is overdue tasks followed by
/// tasks due today.
fn discover_from(st: StatusResult) -> DiscoverResult {
    let mut today = st.overdue.iter().map(|o| o.task.clone()).collect::<Vec<_>>();
    if let Some(dt) = &st.due_today {
        today.extend(dt.iter().cloned());
    }
    DiscoverResult {
        today_date: st.today_date,
        summary: st.counts,
        today,
        next_suggested: st.next_suggested,
        hints: {
            let cmd = crate::config::Paths::command_name();
            vec![
                format!("全コマンド仕様は `{cmd} agent --json`"),
                format!("新規タスクは `{cmd} task add --title \"...\" --project <id>`"),
                format!("今やることは `{cmd} status`"),
            ]
        },
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    use crate::ops;
    use crate::ops::test_support::{mk_project, mk_task_in, new_engine};
    use crate::store_engine::WriteTx;

    fn proj(tx: &WriteTx<'_>, name: &str) -> i64 {
        mk_project(tx, name)
    }

    fn task(tx: &WriteTx<'_>, title: &str, project_id: Option<i64>) -> i64 {
        mk_task_in(tx, title, project_id)
    }

    /// The ids of the tasks matching a filter expression. The read goes through the same indexed SQL
    /// as production ([`list`]), which pins the meaning of the grammar against the source of truth.
    fn ids(tx: &WriteTx<'_>, filter: Option<&str>) -> Vec<i64> {
        let mut v: Vec<i64> = list(
            tx.conn(),
            crate::reach::Reach::All,
            ListParams {
                project_id: None,
                filter_expr: filter.map(|s| s.to_string()),
                text: None,
                sort: "title".to_string(),
                limit: None,
                offset: None,
            },
        )
        .unwrap()
        .tasks
        .into_iter()
        .map(|t| t.id)
        .collect();
        v.sort();
        v
    }

    /// The message a rejected filter expression produces — the hook for asserting that it errors
    /// rather than matching nothing.
    fn err(tx: &WriteTx<'_>, filter: &str) -> String {
        list(
            tx.conn(),
            crate::reach::Reach::All,
            ListParams {
                project_id: None,
                filter_expr: Some(filter.to_string()),
                text: None,
                sort: "title".to_string(),
                limit: None,
                offset: None,
            },
        )
        .expect_err(&format!("`{filter}` does not resolve, so it must error"))
        .to_string()
    }

    /// A value holding whitespace is cut apart by the split before any key sees it, so its tail
    /// reaches the parser as a bare fragment — the very shape a typo makes. The message names both
    /// readings and points at the id, which is the only way such a value can be written at all. The
    /// task face and the decision face share one skeleton, so the answer is the same on both.
    #[test]
    fn a_value_holding_whitespace_is_told_apart_from_a_typo() {
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

        for message in [
            Filter::parse("dim:リリース=AI 起票導線", day).unwrap_err().to_string(),
            DecisionFilter::parse("project:検索 の面", day).unwrap_err().to_string(),
        ] {
            assert!(message.contains("must be in key:value form"), "the grammar is still stated: {message}");
            assert!(
                message.contains("whitespace"),
                "and the cause the fragment cannot show on its own: {message}",
            );
            assert!(message.contains("its id"), "and the one road that carries such a value: {message}");
        }

        let fragment = Filter::parse("dim:リリース=AI 起票導線", day).unwrap_err().to_string();
        assert!(fragment.contains("'起票導線'"), "the fragment is quoted as written: {fragment}");
    }

    /// `project:` takes an id or a **name** (the same entry point as `task add --project`). A
    /// reference that fails to resolve is an error, not an empty result — silently returning nothing
    /// leaves the caller unable to tell "nothing matched" from "I mistyped the name".
    #[test]
    fn project_filter_takes_a_name_or_an_id_and_refuses_an_unknown_one() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let alpha = proj(tx, "アルファ");
        let beta = proj(tx, "ベータ");
        let a = task(tx, "a", Some(alpha));
        task(tx, "b", Some(beta));

        assert_eq!(ids(tx, Some(&format!("project:{alpha}"))), vec![a], "an id resolves");
        assert_eq!(ids(tx, Some("project:アルファ")), vec![a], "a name finds the same single row");

        let err = list(
            tx.conn(),
            crate::reach::Reach::All,
            ListParams {
                project_id: None,
                filter_expr: Some("project:存在しないPJ".to_string()),
                text: None,
                sort: "title".to_string(),
                limit: None,
                offset: None,
            },
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("存在しないPJ"),
            "a reference that fails to resolve is an error, not an empty result, and it names the reference as written: {err}"
        );
    }

    /// `number:` / `ref:` select tasks by conversational number. A bare number, `#n`, `T-n` and `AMB-T-n` match
    /// on the number; `D-n` (a decision reference) and a number nobody holds match nothing. `ref:` is
    /// an alias of `number:`.
    #[test]
    fn number_filter_discriminates_tasks() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = proj(tx, "PJ");
        let t1 = task(tx, "one", Some(p)); // number 1
        let t2 = task(tx, "two", Some(p)); // number 2
        let _t3 = task(tx, "three", Some(p)); // number 3

        assert_eq!(ids(tx, Some("number:1")), vec![t1]);
        assert_eq!(ids(tx, Some("number:#2")), vec![t2]);
        assert_eq!(ids(tx, Some("ref:2")), vec![t2], "`ref:` is an alias of `number:`");
        assert_eq!(ids(tx, Some("number:T-1")), vec![t1], "T-n matches on the task side");
        assert!(ids(tx, Some("number:D-2")).is_empty(), "D-n is a decision reference — no task matches");
        assert!(ids(tx, Some("number:999")).is_empty(), "a number nobody holds matches nothing");
    }

    /// `commit:<sha>` walks the reverse chain **git → task**: the tasks that recorded a commit. A public
    /// commit carries no store-local ref, so this is the only face back to a task. The SHA is
    /// case-folded to the bytes the door stored; the same commit on two tasks finds both; a SHA nobody
    /// recorded — a short SHA included, since the door stores full hex only — is an empty result, not an
    /// error (a SHA is a free value, not a name the store knows); an empty value is refused.
    #[test]
    fn commit_filter_walks_the_reverse_chain() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = proj(tx, "PJ");
        let t1 = task(tx, "one", Some(p));
        let t2 = task(tx, "two", Some(p));
        let _t3 = task(tx, "three", Some(p));
        let sha_a = "a".repeat(40); // SHA-1 form
        let sha_b = "b".repeat(64); // SHA-256 form
        ops::commit::add(tx, t1, &sha_a, Some(ActorKind::Ai)).unwrap();
        ops::commit::add(tx, t2, &sha_a, Some(ActorKind::Ai)).unwrap(); // same commit on two tasks
        ops::commit::add(tx, t2, &sha_b, Some(ActorKind::Ai)).unwrap();

        let mut both = vec![t1, t2];
        both.sort();
        assert_eq!(ids(tx, Some(&format!("commit:{sha_a}"))), both, "both tasks that recorded the commit");
        assert_eq!(ids(tx, Some(&format!("commit:{sha_b}"))), vec![t2], "the SHA-256 form finds its one task");
        assert_eq!(
            ids(tx, Some(&format!("commit:{}", sha_a.to_uppercase()))),
            both,
            "an upper-case SHA folds to the stored lower-case bytes",
        );
        assert!(
            ids(tx, Some(&format!("commit:{}", "c".repeat(40)))).is_empty(),
            "a full SHA nobody recorded is an empty result, not an error",
        );
        assert!(
            ids(tx, Some("commit:abc1234")).is_empty(),
            "a short SHA is never stored, so it simply matches nothing (not rejected)",
        );

        let day = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        assert_eq!(
            Filter::parse("commit:ABCdef", day).unwrap().commit,
            Some("abcdef".to_string()),
            "the value is normalised to lower-case at parse time",
        );
        assert!(Filter::parse("commit:", day).is_err(), "an empty value is no SHA at all — refused");
    }

    /// `ai:true|false` selects on the AI-delegation dimension (`assignee_kind=ai`). Independent of the
    /// assignee dimension: `true` gathers everything delegated to an AI, whoever's, while what is not
    /// delegated (assigned to a human, or unassigned) falls under `false`. A bad value is an error.
    #[test]
    fn ai_facet_discriminates_delegation() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = proj(tx, "PJ");
        let ai = task(tx, "ai", Some(p));
        let human = task(tx, "human", Some(p));
        let unassigned = task(tx, "unassigned", Some(p));
        ops::task::set_assignee(tx, ai, Some(ActorKind::Ai)).unwrap();
        ops::task::set_assignee(tx, human, Some(ActorKind::Human)).unwrap();

        assert_eq!(ids(tx, Some("ai:true")), vec![ai], "only what is delegated to an AI");
        let mut not_ai = vec![human, unassigned];
        not_ai.sort();
        assert_eq!(ids(tx, Some("ai:false")), not_ai, "assigned to a human, or unassigned, is not delegated");
        let day = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        assert!(Filter::parse("ai:maybe", day).is_err(), "anything other than true/false is an error");
    }

    /// `status:` is a comma-separated any-of set. `status:todo,in_progress` picks up both todo and
    /// in_progress and leaves out done and blocked — the foundation that keeps an AI's mailbox from
    /// dropping the tasks it has already started.
    #[test]
    fn status_filter_accepts_comma_separated_any_of() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = proj(tx, "PJ");
        let td = task(tx, "todo", Some(p));
        let ip = task(tx, "in_progress", Some(p));
        let dn = task(tx, "done", Some(p));
        let bl = task(tx, "blocked", Some(p));
        ops::task::set_status(tx, ip, TaskStatus::InProgress).unwrap();
        ops::task::set_status(tx, dn, TaskStatus::Done).unwrap();
        ops::task::set_status(tx, bl, TaskStatus::Blocked).unwrap();

        let mut expected = vec![td, ip];
        expected.sort();
        assert_eq!(ids(tx, Some("status:todo,in_progress")), expected, "todo and in_progress only");
        assert_eq!(ids(tx, Some("status:in_progress")), vec![ip], "a single value works as before");
        assert_eq!(ids(tx, Some("status:blocked")), vec![bl], "blocked is outside the set");

        let day = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        assert!(Filter::parse("status:todo,bogus", day).is_err(), "an unknown value anywhere in the set is an error");
        assert!(Filter::parse("status:todo,", day).is_err(), "an empty element is an error");
    }

    /// `dim:<axis>=<value>` selects on any classification axis, and `time_axis:<value>` is sugar for
    /// **whichever axis is designated role=time_axis** (independent of the axis's name). `=none` means
    /// tasks with no live value on that axis. Several axis tokens AND together.
    #[test]
    fn dimension_and_time_axis_filters_slice_by_axis_value() {
        use crate::model::{DimensionCardinality, DimensionRole};

        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = proj(tx, "PJ");
        let axis = |name: &str, role: DimensionRole| {
            ops::dimension::add(
                tx,
                p,
                ops::dimension::NewDimension {
                    name: name.to_string(),
                    notes: String::new(),
                    cardinality: DimensionCardinality::Single,
                    ordered: true,
                    role,
                },
            )
            .unwrap()
            .id
        };
        // Name the time axis "Era" — to show that `time_axis:` looks it up by role, not by name.
        let era = axis("Era", DimensionRole::TimeAxis);
        let category = axis("Category", DimensionRole::None);
        let dev = ops::dimension::value_add(tx, era, "dev").unwrap().id;
        let ops_era = ops::dimension::value_add(tx, era, "ops").unwrap().id;
        let bug = ops::dimension::value_add(tx, category, "bug").unwrap().id;

        let t_dev_bug = task(tx, "dev bug", Some(p));
        let t_ops = task(tx, "ops", Some(p));
        let t_bare = task(tx, "bare", Some(p));
        ops::dimension::set(tx, t_dev_bug, dev).unwrap();
        ops::dimension::set(tx, t_dev_bug, bug).unwrap();
        ops::dimension::set(tx, t_ops, ops_era).unwrap();

        assert_eq!(ids(tx, Some("time_axis:dev")), vec![t_dev_bug], "the time axis is looked up by role");
        assert_eq!(ids(tx, Some("time_axis:DEV")), vec![t_dev_bug], "a value name is case-insensitive");
        assert_eq!(ids(tx, Some("dim:Era=ops")), vec![t_ops], "the axis name resolves too");
        assert_eq!(ids(tx, Some("dimension:era=ops")), vec![t_ops], "the alias and the axis name are case-insensitive too");
        assert_eq!(ids(tx, Some("dim:Category=bug")), vec![t_dev_bug]);
        assert_eq!(
            ids(tx, Some("time_axis:dev dim:Category=bug")),
            vec![t_dev_bug],
            "axis tokens AND together"
        );
        assert!(ids(tx, Some("time_axis:ops dim:Category=bug")).is_empty(), "an AND, so nothing matches");
        assert_eq!(ids(tx, Some("time_axis:none")), vec![t_bare], "the tasks with no value on the time axis");
        let mut no_category = vec![t_ops, t_bare];
        no_category.sort();
        assert_eq!(ids(tx, Some("dim:Category=none")), no_category);
        // Both axis and value can be given as ids (they resolve either by name or by id).
        assert_eq!(
            ids(tx, Some(&format!("dim:{category}={bug}"))),
            vec![t_dev_bug],
            "axis and value both given as ids"
        );

        // Delete a value and the assignments to it fall back to "unclassified" (an assignment only
        // counts when both hops, assignment → value, are live). The deleted value's name now points at
        // nothing, so filtering by it is an error rather than an empty result (see the dedicated test
        // below).
        ops::dimension::value_delete(tx, dev).unwrap();
        let mut time_axis_none = vec![t_dev_bug, t_bare];
        time_axis_none.sort();
        assert_eq!(ids(tx, Some("time_axis:none")), time_axis_none, "an assignment to a deleted value counts as unclassified");

        let day = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        assert!(Filter::parse("dim:Category", day).is_err(), "no `=` is an error");
        assert!(Filter::parse("dim:=bug", day).is_err(), "an empty axis is an error");
        // An empty value is always rejected: filtering on a value that names nothing turns what was
        // meant as a filter into an unconditional pass.
        assert!(Filter::parse("dim:Category=", day).is_err(), "an empty value is an error");
        assert!(Filter::parse("time_axis:", day).is_err(), "an empty `time_axis` value is an error");
        // Gatekeeping the vocabulary: `phase` is only a name a user might give an axis, never a key of
        // the grammar.
        assert!(Filter::parse("phase:dev", day).is_err(), "`phase:` is an unknown key");
    }

    /// An axis or value name that fails to resolve is **an error, not an empty result** (the same
    /// contract as `project:`). A typo that quietly matched nothing would leave the caller unable to
    /// tell "nothing matched" from "I mistyped the name", and on the `=none` side it is worse still:
    /// "unclassified with respect to an axis that does not exist" is true of everyone, so the query
    /// would hand back **every row**.
    #[test]
    fn unknown_axis_or_value_is_an_error_not_an_empty_or_a_full_list() {
        use crate::model::{DimensionCardinality, DimensionRole};

        let e = new_engine();
        let tx = &e.write().unwrap();
        let p = proj(tx, "PJ");
        let add_axis = |name: &str, role: DimensionRole| {
            ops::dimension::add(
                tx,
                p,
                ops::dimension::NewDimension {
                    name: name.to_string(),
                    notes: String::new(),
                    cardinality: DimensionCardinality::Single,
                    ordered: true,
                    role,
                },
            )
            .unwrap()
            .id
        };
        let era = add_axis("Era", DimensionRole::TimeAxis);
        let category = add_axis("Category", DimensionRole::None);
        ops::dimension::value_add(tx, era, "dev").unwrap();
        ops::dimension::value_add(tx, category, "bug").unwrap();
        let t = task(tx, "task", Some(p));

        assert!(err(tx, "dim:Nosuch=bug").contains("Nosuch"), "a nonexistent axis errors, naming it");
        assert!(err(tx, "dim:Category=nosuch").contains("nosuch"), "a nonexistent value errors, naming it");
        assert!(err(tx, "time_axis:nosuch").contains("nosuch"), "a nonexistent value on the time axis errors too");
        // This is the side where an empty result would not be the worst outcome: `=none` on a
        // nonexistent axis makes `NOT EXISTS` always true, so a filter meant to narrow returns
        // everything.
        assert!(!ids(tx, Some("dim:Category=none")).is_empty(), "`=none` on a live axis resolves as before");
        assert!(err(tx, "dim:Nosuch=none").contains("Nosuch"), "`=none` on a nonexistent axis is an error, not every row");
        assert_eq!(ids(tx, Some("dim:Category=bug")), Vec::<i64>::new(), "merely matching nothing stays an empty result");
        assert_eq!(ids(tx, None), vec![t], "the error is confined to a reference that fails to resolve");

        // `time_axis:` names an axis by role. With no axis designated there is nothing to point at,
        // and that too is an error rather than an empty result.
        let bare = new_engine();
        let tx = &bare.write().unwrap();
        proj(tx, "時間軸の無い PJ");
        assert!(err(tx, "time_axis:dev").contains("time axis"), "no axis is designated as the time axis: {}", err(tx, "time_axis:dev"));
        assert!(err(tx, "time_axis:none").contains("time axis"), "the same for `=none`");
    }

    /// The grammar of a filter value (`AMB-T-<n>` / `AMB-D-<n>`, or the bare `123` / `#123` / `T-123` /
    /// `D-123`, the prefix case-insensitive) and the error on a bad one.
    #[test]
    fn number_filter_parse_forms() {
        assert_eq!(
            NumberFilter::parse("123").unwrap(),
            NumberFilter { number: 123, require_decision: None }
        );
        assert_eq!(
            NumberFilter::parse("#123").unwrap(),
            NumberFilter { number: 123, require_decision: None }
        );
        assert_eq!(
            NumberFilter::parse("T-7").unwrap(),
            NumberFilter { number: 7, require_decision: Some(false) }
        );
        assert_eq!(
            NumberFilter::parse("d-80").unwrap(),
            NumberFilter { number: 80, require_decision: Some(true) },
            "the prefix is case-insensitive"
        );
        assert!(NumberFilter::parse("abc").is_err());
        assert!(NumberFilter::parse("").is_err());
    }

    /// A cursor is an opaque token hiding `(at, seq, id)`: it round-trips carrying the sequence its row's
    /// id was drawn from, and it can be told apart from a date expression or from garbage.
    #[test]
    fn activity_cursor_roundtrips_and_is_distinguishable() {
        use crate::activity::Seq;
        let at = Timestamp::parse_rfc3339("2026-07-05T01:02:03Z").unwrap();
        let c = encode_activity_cursor(&at, Seq::Activity, 12);
        assert!(looks_like_activity_cursor(&c), "it starts with cur2_");
        assert_eq!(parse_activity_cursor(&c), Some((at, Seq::Activity, 12)));
        // The sequence is part of the token, so two rows that share `(at, id)` do not share a cursor.
        let d = encode_activity_cursor(&at, Seq::DecisionComment, 12);
        assert_ne!(c, d);
        assert_eq!(parse_activity_cursor(&d), Some((at, Seq::DecisionComment, 12)));
        // Date expressions and garbage are not cursors (so `--since` branches to the date side).
        assert!(!looks_like_activity_cursor("today"));
        assert!(!looks_like_activity_cursor("2026-07-05"));
        assert!(parse_activity_cursor("today").is_none());
        assert!(parse_activity_cursor("cur2_@@@not-base64@@@").is_none());
    }

    /// A `cur1_` token — written before the timeline had a third source — is still read, and reads back on
    /// the shared activity sequence, which is the only one it could ever have named. A reader that was
    /// mid-stream when this build arrived keeps its place instead of being told its cursor is malformed.
    #[test]
    fn the_first_cursor_spelling_is_still_read_as_the_activity_sequence() {
        use base64::Engine;
        let at = Timestamp::parse_rfc3339("2026-07-05T01:02:03Z").unwrap();
        let raw = format!("{}\n{}", at.to_rfc3339_z(), 12);
        let v1 = format!("cur1_{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw.as_bytes()));

        assert!(looks_like_activity_cursor(&v1), "it is a cursor, not a date");
        assert_eq!(parse_activity_cursor(&v1), Some((at, crate::activity::Seq::Activity, 12)));
        assert!(
            encode_activity_cursor(&at, crate::activity::Seq::Activity, 12).starts_with("cur2_"),
            "read, never written: what goes out carries the sequence"
        );
    }

    /// The reach cannot be **left undeclared**: `list` / `activity` / `decision_list` take it as an
    /// argument and `TaskQuery` takes it as a field, so forgetting it does not compile. Underneath
    /// that, this pins down that the engine's own listing is closed over the reach as well — reads
    /// that do not go through `query::list` (the GUI's task page) need that floor.
    #[test]
    fn a_closed_reach_narrows_the_engines_own_list_too() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let mine = proj(tx, "bound");
        let theirs = proj(tx, "other");
        let a = task(tx, "mine", Some(mine));
        task(tx, "theirs", Some(theirs));

        let page = crate::store_engine::list_task_ids(
            tx.conn(),
            &crate::store_engine::TaskQuery {
                reach: crate::reach::Reach::binding(mine),
                // Even with no scope given (i.e. asking for the whole machine), nothing outside the
                // closed reach comes out.
                project_id: None,
                filter: &Filter::default(),
                sort: "title",
                today: time::today(),
                limit: None,
                offset: None,
            },
        )
        .unwrap();
        assert_eq!(page.ids, vec![a], "only the bound project's rows");
        assert_eq!(page.total_matched, 1, "the pre-paging total is counted within the reach too");
    }

    /// One task with the same word on its title, its notes and a comment — the fixture the search reads.
    fn worded_task(tx: &WriteTx<'_>, title: &str, notes: &str, project_id: i64) -> i64 {
        ops::task::add(
            tx,
            ops::task::NewTask {
                title: title.to_string(),
                notes: notes.to_string(),
                project_id: Some(project_id),
                due_on: None,
                start_on: None,
                priority: None,
                created_by_kind: None,
            },
        )
        .expect("add task")
        .id
    }

    /// What a face is handed: the record's own ref and title, the comment's ref when the words are on a
    /// timeline, and an excerpt of the text they landed in.
    #[test]
    fn search_hands_back_the_place_the_words_are_written() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pj = proj(tx, "PJ");
        let t = worded_task(tx, "全文検索の索引", "索引は走査で引く", pj);
        let c = ops::comment::add_comment(tx, t, ActorKind::Ai, "索引の話をここでする").expect("comment");

        let r = search(
            tx.conn(),
            crate::reach::Reach::All,
            SearchParams { text: "索引".to_string(), ..Default::default() },
        )
        .unwrap();

        assert_eq!((r.count, r.total_matched), (3, 3));
        assert_eq!(r.query.sort, SEARCH_SORT_DEFAULT, "the order is echoed as it was written");
        let faces: Vec<HitFace> = r.hits.iter().map(|h| h.face).collect();
        assert_eq!(faces, vec![HitFace::Title, HitFace::Body, HitFace::Comment]);
        assert!(r.hits.iter().all(|h| h.kind == "task" && h.r#ref == crate::idref::task(t)));
        assert_eq!(r.hits[0].title, "全文検索の索引");
        assert_eq!(r.hits[1].snippet, "索引は走査で引く", "the excerpt is of the face it landed on");
        assert_eq!(r.hits[0].comment, None, "the record's own face sits on no timeline");
        assert_eq!(
            r.hits[2].comment.as_deref(),
            Some(crate::idref::task_comment(c.id).as_str()),
            "the comment to open to find the words"
        );
    }

    /// A hit carries a snippet, so `search` is the one read that **has** a ceiling of its own: no limit
    /// named is not "everything". The total says what the ceiling left behind.
    #[test]
    fn search_holds_an_unlimited_query_to_its_default_ceiling() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pj = proj(tx, "PJ");
        for i in 0..SEARCH_LIMIT_DEFAULT + 5 {
            worded_task(tx, &format!("索引 {i}"), "", pj);
        }
        let run = |limit| {
            search(
                tx.conn(),
                crate::reach::Reach::All,
                SearchParams { text: "索引".to_string(), limit, ..Default::default() },
            )
            .unwrap()
        };
        let capped = run(None);
        assert_eq!(capped.count, SEARCH_LIMIT_DEFAULT);
        assert_eq!(capped.total_matched, SEARCH_LIMIT_DEFAULT + 5, "the total is of the hits, not the page");
        assert_eq!(run(Some(3)).count, 3, "a caller's own limit is taken as it is");
    }

    /// A word written as a ref names the record itself, which no word could reach: nothing about a task
    /// carries the ref it is called by. So it is pinned to the top, counted like any other line, and the
    /// words go on searching as words.
    #[test]
    fn a_word_written_as_a_ref_pins_the_record_it_names() {
        let e = new_engine();
        let tx = &e.write().unwrap();
        let pj = proj(tx, "PJ");
        let t = worded_task(tx, "全文検索の索引", "", pj);
        let mentions = worded_task(tx, "後で読む", &format!("AMB-T-{t} を読むこと"), pj);

        let run = |text: &str| {
            search(
                tx.conn(),
                crate::reach::Reach::All,
                SearchParams { text: text.to_string(), ..Default::default() },
            )
            .unwrap()
        };

        let r = run(&crate::idref::task(t));
        assert_eq!(r.hits[0].r#ref, crate::idref::task(t), "the record it names comes first");
        assert_eq!(r.hits[0].face, HitFace::Title);
        assert_eq!(
            r.hits[1].r#ref,
            crate::idref::task(mentions),
            "and the word goes on being a word: the task that mentions it is a hit too"
        );
        assert_eq!((r.count, r.total_matched), (2, 2), "the pin is counted beside the hits");

        let r = run(&crate::idref::task(9999));
        assert!(r.hits.is_empty(), "a ref naming nothing live pins nothing");
    }

    /// The `--sort` spec: the default weights the face, and the two timeline forms take that weight off.
    /// An unknown key is an error rather than a quiet fallback to the default order.
    #[test]
    fn search_sort_parses_the_face_and_the_two_timeline_forms() {
        assert_eq!(SearchSort::parse(SEARCH_SORT_DEFAULT).unwrap(), SearchSort::Face);
        assert_eq!(SearchSort::parse("").unwrap(), SearchSort::default());
        assert_eq!(SearchSort::parse("-time").unwrap(), SearchSort::Newest);
        assert_eq!(SearchSort::parse("time").unwrap(), SearchSort::Oldest);
        assert_eq!(SearchSort::Newest.as_str(), "-time", "the spec round-trips into the echo");
        assert!(SearchSort::parse("-face").is_err(), "a face has no descending form");
        assert!(SearchSort::parse("due").unwrap_err().to_string().contains("face/time/-time"));
    }
}
