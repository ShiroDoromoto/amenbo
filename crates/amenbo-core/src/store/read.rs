//! Serving reads. Everything here queries the truth-source engine through indexed SQL.

use crate::error::Result;
use crate::reach::Reach;
use crate::store_engine::StoreEngine;

use super::Store;

impl Store {
    // ── Reach ─────────────────────────────────────────────────────────────────────
    //
    // An AI facet must not see anything outside the project its binding (`.amenbo`) names. The guard sits
    // at **exactly two entry points** — we do not sprinkle predicates through the individual SQL:
    //
    // - **Listings** fold the reach into the scope slot (`project_id`) via `Reach::narrow`
    //   (`query::list` / `decision_list` / `activity` / `status` read `reach` off their params, and fold
    //   a `project:` filter into the same slot). The project listing itself narrows to the bound project.
    // - **Naming an id** is checked during reference resolution (`resolve_*_ref`), which looks up the
    //   entity's owning project. The surface (the CLI) always goes through resolution before it holds an
    //   id, for reads as much as writes, so an out-of-reach id is rejected **before anything touches it**.
    //
    // But ids that are **not conversational refs** — comment ids, attachment ids, dimension /
    // dimension-value ids — arrive here straight from the surface without passing any `resolve_*_ref`
    // (`attach show`, `attach ls --task-comment`, `dimension show`, …). Those are checked at the read
    // entry point itself (the `reachable_*` helpers below), using the same owner lookup as the write side
    // (`super::owner`) — two copies of that lookup would mean closing one side and leaving a hole in the
    // other.
    //
    // The guard is *not* placed on the detail reads (`task_detail` / `project_detail`, …). Those double as
    // the **echo right after a write** (`task add` printing the new task back, `project add` the new
    // project), and a guard there would produce the worst possible answer: "the write went through, and
    // here is an error". Once the entry points hold, a detail read only ever receives an in-reach id.
    //
    // Out of reach is `out_of_reach`, never not_found: we do not deny that the entity exists, we only say
    // it cannot be reached.

    /// Whether this task is within reach (`out_of_reach` if not).
    fn reachable_task(&self, task_id: i64) -> Result<()> {
        if self.reach == Reach::All {
            return Ok(());
        }
        let owner = crate::store_engine::read::task_project(self.engine.conn(), task_id)
            .map_err(crate::error::engine_on(self.engine.conn()))?;
        self.reach.check(&crate::idref::task(task_id), owner)
    }

    /// Whether this decision is within reach (`out_of_reach` if not).
    fn reachable_decision(&self, decision_id: i64) -> Result<()> {
        if self.reach == Reach::All {
            return Ok(());
        }
        let owner = crate::store_engine::read::decision_project(self.engine.conn(), decision_id)
            .map_err(crate::error::engine_on(self.engine.conn()))?;
        self.reach.check(&crate::idref::decision(decision_id), owner)
    }

    /// Check a read that names an id without going through a conversational ref. `what` is the display
    /// ref to quote in the error; `owner` looks the owning project up via [`super::owner`]. Under `All`
    /// the lookup does not even run, so humans, the GUI and library use pay nothing for this.
    fn reachable(
        &self,
        what: &str,
        owner: impl FnOnce(&rusqlite::Connection) -> Result<Option<i64>>,
    ) -> Result<()> {
        if self.reach == Reach::All {
            return Ok(());
        }
        self.reach.check(what, owner(self.engine.conn())?)
    }

    /// Whether this project is within reach — the entry point for reads that take a project id directly
    /// (`dimensions` and friends).
    fn reachable_project(&self, project_id: i64) -> Result<()> {
        self.reach.check(&format!("project #{project_id}"), Some(project_id))
    }

    /// The read behind `task list`. Queries the truth-source engine with indexed SQL
    /// ([`crate::query::list`]), which computes the whole selection — filter, project, sort, total.
    pub fn list_tasks(&self, params: crate::query::ListParams) -> Result<crate::query::TaskListResult> {
        crate::query::list(self.engine.conn(), self.reach, params)
    }

    /// The read behind `status` (the overdue / today / week / in_progress buckets plus the summary
    /// counts), served by indexed SQL ([`crate::query::status`]).
    pub fn status(&self, scope: &str) -> Result<crate::query::StatusResult> {
        crate::query::status(self.engine.conn(), scope, self.reach)
    }

    /// The read behind `comment list`, served by indexed SQL ([`crate::query::comment_list`]).
    pub fn comment_list(&self, task_id: i64, offset: Option<usize>, limit: Option<usize>) -> Result<crate::query::CommentListResult> {
        crate::query::comment_list(self.engine.conn(), task_id, offset, limit)
    }

    /// The read behind `activity` (the unified timeline). System events come from the file ledger and
    /// comments from `task_comment`; the two are merged at read time ([`crate::activity`]).
    pub fn activity(&self, params: crate::query::ActivityParams) -> Result<crate::query::ActivityResult> {
        if let Some(task_id) = params.task_id {
            self.reachable_task(task_id)?;
        }
        crate::query::activity(&self.paths.activity_file, self.engine.conn(), self.reach, params)
    }

    /// The read behind `decision comment list`, served by indexed SQL
    /// ([`crate::query::decision_comment_list`]) — the decision-side twin of [`Self::comment_list`].
    /// `decision_id` is an already-resolved id.
    pub fn decision_comment_list(&self, decision_id: i64, offset: Option<usize>, limit: Option<usize>) -> Result<crate::query::DecisionCommentListResult> {
        crate::query::decision_comment_list(self.engine.conn(), decision_id, offset, limit)
    }

    /// Decisions linked to a task, for `task show` to surface the "why" inline. Read from the
    /// store-engine read-model (`decision_task_link` reverse lookup). Never errors — a failed read
    /// yields an empty list, since this is additive context alongside notes/comments.
    pub fn decisions_for_task(&self, task_id: i64) -> Vec<crate::store_engine::read::LinkedDecisionRow> {
        crate::store_engine::read::decisions_for_task(self.engine.conn(), task_id).unwrap_or_default()
    }

    /// The read behind `decision list`. Served by indexed SQL ([`crate::query::decision_list`]), which
    /// avoids `view::decision_compact`'s per-decision re-scan.
    pub fn decision_list(&self, params: crate::query::DecisionListParams) -> Result<crate::query::DecisionListResult> {
        crate::query::decision_list(self.engine.conn(), self.reach, params)
    }

    /// The read behind `decision show`. Served by indexed SQL ([`crate::query::decision_detail`]), which
    /// avoids `view::decision_detail`'s per-decision full scans (the superseded_by reverse lookup, the
    /// decided_by name resolution, the linked_tasks title resolution). Pass an already-resolved
    /// `decision_id` (the CLI's `resolve_decision` has run).
    pub fn decision_detail(&self, decision_id: i64) -> Result<crate::view::DecisionDetail> {
        crate::query::decision_detail(self.engine.conn(), decision_id)
    }

    /// The **blast radius** of overturning this decision: a reverse lookup of the live edges pointing at
    /// it (all three kinds, one hop). This is what lets `supersede` / `reject` / `delete` show which
    /// decisions may need revisiting first. It only surfaces them — it never blocks the operation, and
    /// currency does not cascade (the relation is not transitive).
    pub fn decision_impact_radius(
        &self,
        decision_id: i64,
    ) -> Result<Vec<crate::store_engine::read::ReverseEdge>> {
        crate::store_engine::read::decision_reverse_edges(self.engine.conn(), decision_id)
            .map_err(crate::error::engine_on(self.engine.conn()))
    }

    /// The read behind `task show`. Served by indexed SQL ([`crate::query::task_detail`]), which avoids
    /// `view::task_detail`'s per-task full scans (resolving placement, the assignee's name, open
    /// blockers, the comment count). Pass an already-resolved `task_id` (the CLI's `resolve_task` has
    /// run).
    pub fn task_detail(&self, task_id: i64) -> Result<crate::view::TaskDetail> {
        crate::query::task_detail(self.engine.conn(), task_id)
    }

    /// Resolve a `task` reference (`AMB-T-n`, or the bare `T-n` / `#n` / `n`) to a single live task id. Served by indexed SQL
    /// ([`crate::query::resolve_task_ref`]), so the lookup a write does first is not an O(n) full scan.
    /// Numbers are **globally unique on this machine**, so no project context is needed.
    pub fn resolve_task_ref(&self, input: &str) -> Result<i64> {
        let id = crate::query::resolve_task_ref(self.engine.conn(), input)?;
        self.reachable_task(id)?;
        Ok(id)
    }

    /// Resolve a `decision` reference (`AMB-D-n`, or the bare `D-n` / `#n` / `n`) to a single live decision id, served by indexed
    /// SQL ([`crate::query::resolve_decision_ref`]). Decisions live in **their own number space, separate
    /// from tasks**, and their numbers are likewise globally unique.
    pub fn resolve_decision_ref(&self, input: &str) -> Result<i64> {
        let id = crate::query::resolve_decision_ref(self.engine.conn(), input)?;
        self.reachable_decision(id)?;
        Ok(id)
    }

    /// Resolve a cross-type conversational reference (`AMB-T-n` / `AMB-D-n`, or a bare `#n` / `n`) to either a Task or a
    /// Decision, served by indexed SQL ([`crate::query::resolve_any`]). An uncoded `#n` spans both
    /// number spaces, so it is ambiguous when the same number exists in each.
    pub fn resolve_any_ref(&self, input: &str) -> Result<crate::ops::Ref> {
        let r = crate::query::resolve_any(self.engine.conn(), input)?;
        match r {
            crate::ops::Ref::Task(id) => self.reachable_task(id)?,
            crate::ops::Ref::Decision(id) => self.reachable_decision(id)?,
        }
        Ok(r)
    }

    /// Resolve a `project` reference (an id, or an exact name match), served by indexed SQL
    /// ([`crate::query::resolve_project_ref`]).
    pub fn resolve_project_ref(&self, reference: &str) -> Result<i64> {
        let id = crate::query::resolve_project_ref(self.engine.conn(), reference)?;
        self.reach.narrow(Some(id))?;
        Ok(id)
    }

    /// Resolve an assignee token to a facet (human / ai). There are exactly two facets, addressed by the
    /// reserved words (`me`, `me-ai`, …) and by the two display names in config
    /// ([`crate::config::Config::resolve_facet`]). Anything that matches neither is not_found.
    pub fn resolve_assignee_facet(&self, token: &str) -> Result<crate::model::ActorKind> {
        self.config
            .resolve_facet(token)
            .ok_or_else(|| crate::ops::user::NOUN.not_found(token))
    }

    /// The read behind `project list`. Served by indexed SQL ([`crate::query::project_list`]), which
    /// avoids the full scans that counting `num_dimensions` / `num_tasks` would otherwise take.
    pub fn project_list(&self, include_archived: bool) -> Result<crate::query::ProjectListResult> {
        let mut result = crate::query::project_list(self.engine.conn(), include_archived)?;
        // Under a narrowed reach, even "the project list" is the bound project and nothing else — not
        // even the names of the other projects enter the context.
        if let Some(pid) = self.reach.project() {
            result.projects.retain(|p| p.id == pid);
            result.count = result.projects.len();
        }
        Ok(result)
    }

    /// The read behind `project show`. Served by indexed SQL ([`crate::query::project_detail`]), which
    /// folds the five scans the count summary would take into a single aggregate. `project_id` is an
    /// already-resolved id.
    pub fn project_detail(&self, project_id: i64) -> Result<crate::query::ProjectDetail> {
        crate::query::project_detail(self.engine.conn(), project_id)
    }

    /// The read behind bare `amenbo` (discover). The `status` material it builds on comes from the
    /// engine's indexed SQL ([`crate::query::discover`]).
    pub fn discover(&self) -> Result<crate::query::DiscoverResult> {
        crate::query::discover(self.engine.conn(), self.reach)
    }

    /// The read behind `validate` (the shape checks). It pulls only `(id, title)` for the live tasks
    /// ([`crate::validate::validate`]). Under a narrowed reach it inspects only the bound project's
    /// tasks, and a named task outside the reach comes back as `out_of_reach` rather than "does not
    /// exist".
    pub fn validate(&self, target_ids: &[String]) -> Result<crate::validate::ValidateResult> {
        for r in target_ids {
            if let Some(id) = crate::ops::parse_id_ref(crate::idref::RefKind::Task, r) {
                self.reachable_task(id)?;
            }
        }
        crate::validate::validate(self.engine.conn(), target_ids, self.reach)
            .map_err(crate::error::engine_on(self.engine.conn()))
    }

    /// The read behind `doctor` (the data-integrity checks), served by indexed SQL
    /// ([`crate::validate::doctor`]). Under a narrowed reach it only looks for breakage inside the bound
    /// project.
    pub fn doctor(&self) -> Result<crate::validate::DoctorResult> {
        crate::validate::doctor(self.engine.conn(), self.reach)
            .map_err(crate::error::engine_on(self.engine.conn()))
    }

    /// The bodies pointing at refs that resolve to nothing. Kept off [`Store::doctor`] — and so off
    /// the startup check and the GUI's per-tick snapshot — because it reads and parses every body on the
    /// device; [`crate::doctor::report`] is what runs it, when a reader has actually asked.
    pub fn dead_refs(&self) -> Result<Vec<crate::doctor::DoctorIssue>> {
        crate::validate::dead_ref_issues(self.engine.conn(), self.reach)
            .map_err(crate::error::engine_on(self.engine.conn()))
    }

    /// A single task; `None` if there is none (a row exists ⇒ it is live). This is how a surface (CLI / GUI)
    /// reads the pre-mutation state — to decide an idempotent no-op, or to report the status a
    /// transition started from.
    pub fn task(&self, id: i64) -> Result<Option<crate::model::Task>> {
        Ok(crate::store_engine::read::task(self.engine.conn(), id)?)
    }

    /// A single project; `None` if there is none (a row exists ⇒ it is live).
    pub fn project(&self, id: i64) -> Result<Option<crate::model::Project>> {
        Ok(crate::store_engine::read::project(self.engine.conn(), id)?)
    }

    /// A task's recorded commit SHAs, oldest first (created_at, id). The list read behind
    /// `task commit list`; the caller resolves the task ref first, so reach is already settled.
    pub fn task_commits(&self, task_id: i64) -> Result<Vec<crate::model::TaskCommit>> {
        Ok(crate::store_engine::read::task_commits(self.engine.conn(), task_id)?)
    }

    /// A single task comment; `None` if there is none (a row exists ⇒ it is live). The id is a comment id, which
    /// is not a conversational ref, so this is itself a reach entry point — it is the path
    /// `decision promote <comment id>` takes to read the body.
    pub fn task_comment(&self, id: i64) -> Result<Option<crate::model::TaskComment>> {
        self.reachable(&format!("comment #{id}"), |c| super::owner::task_comment(c, id))?;
        Ok(crate::store_engine::read::task_comment(self.engine.conn(), id)?)
    }

    /// The task comment ids a comment reference (an exact id match) hits. A row exists ⇒ it is live.
    pub fn resolve_task_comment(&self, reference: &str) -> Result<Vec<i64>> {
        Ok(crate::store_engine::read::resolve_task_comment(self.engine.conn(), reference)?)
    }

    /// The decision comment ids a comment reference hits (the decision-side twin of
    /// [`Self::resolve_task_comment`]).
    pub fn resolve_decision_comment(&self, reference: &str) -> Result<Vec<i64>> {
        Ok(crate::store_engine::read::resolve_decision_comment(self.engine.conn(), reference)?)
    }

    /// A single attachment; `None` if there is none (a row exists ⇒ it is live). `attach show` / `attach open`
    /// come through here — the filename, the URL and the bytes themselves are exactly the "content" that
    /// must not escape the reach, so the guard fires the moment an id is in hand.
    pub fn attachment(&self, id: i64) -> Result<Option<crate::model::Attachment>> {
        self.reachable(&format!("attachment #{id}"), |c| super::owner::attachment(c, id))?;
        Ok(crate::store_engine::read::attachment(self.engine.conn(), id)?)
    }

    /// The live attachments on a target (a task, a decision, or a comment on either), in attach order
    /// (`order_key`, then `id`) — this is `attach ls`. When the target is a comment id
    /// (`--task-comment` / `--decision-comment`) nothing resolves a reference first, so this is the reach
    /// entry point.
    pub fn attachments_for_target(
        &self,
        target_type: crate::model::AttachmentTarget,
        target_id: i64,
    ) -> Result<Vec<crate::model::Attachment>> {
        self.reachable(&super::owner::attach_target_ref(target_type, target_id), |c| {
            super::owner::attach_target(c, target_type, target_id)
        })?;
        let conn = self.engine.conn();
        let ids = crate::store_engine::read::live_attachment_ids_for_target(
            conn,
            target_type.as_str(),
            target_id,
        )?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(a) = crate::store_engine::read::attachment(conn, id)? {
                out.push(a);
            }
        }
        Ok(out)
    }

    /// The live attachment ids an attachment reference (an exact id match) hits — `attach show` / `open` /
    /// `rm`.
    pub fn resolve_attachment(&self, reference: &str) -> Result<Vec<i64>> {
        Ok(crate::store_engine::read::resolve_attachment(self.engine.conn(), reference)?)
    }

    /// A project's live dimensions, in display order (ascending `order_key`).
    pub fn dimensions(&self, project_id: i64) -> Result<Vec<crate::model::Dimension>> {
        self.reachable_project(project_id)?;
        let conn = self.engine.conn();
        let mut out = Vec::new();
        for (id, _order_key) in crate::store_engine::read::dimension_siblings(conn, project_id, None)? {
            if let Some(d) = crate::store_engine::read::dimension(conn, id)? {
                out.push(d);
            }
        }
        Ok(out)
    }

    /// A single dimension; `None` if there is none (a row exists ⇒ it is live).
    pub fn dimension(&self, id: i64) -> Result<Option<crate::model::Dimension>> {
        self.reachable(&format!("dimension #{id}"), |c| super::owner::dimension(c, id))?;
        Ok(crate::store_engine::read::dimension(self.engine.conn(), id)?)
    }

    /// A dimension's live values. Ordered dimensions come back by ascending `order_key`; unordered ones
    /// come back stably, by ascending id.
    pub fn dimension_values(&self, dimension_id: i64) -> Result<Vec<crate::model::DimensionValue>> {
        self.reachable(&format!("dimension #{dimension_id}"), |c| {
            super::owner::dimension(c, dimension_id)
        })?;
        let conn = self.engine.conn();
        let ordered =
            crate::store_engine::read::dimension(conn, dimension_id)?.is_some_and(|d| d.ordered);
        let mut out = Vec::new();
        for id in crate::store_engine::read::dimension_value_ids(conn, dimension_id)? {
            if let Some(v) = crate::store_engine::read::dimension_value(conn, id)? {
                out.push(v);
            }
        }
        if ordered {
            out.sort_by(|a, b| a.order_key.cmp(&b.order_key));
        } else {
            out.sort_by_key(|a| a.id);
        }
        Ok(out)
    }

    /// A single dimension value; `None` if there is none (a row exists ⇒ it is live).
    pub fn dimension_value(&self, id: i64) -> Result<Option<crate::model::DimensionValue>> {
        self.reachable(&format!("dimension value #{id}"), |c| {
            super::owner::dimension_value(c, id)
        })?;
        Ok(crate::store_engine::read::dimension_value(self.engine.conn(), id)?)
    }

    /// Resolve a dimension reference (an id, or an exact name match). Passing `project_id` confines the
    /// search to that project. A call that does **not** confine it (`dimension show <name>`) searches the
    /// whole machine, so the dimension it lands on is reach-checked here — a name collision must not let
    /// us walk away holding a dimension outside the binding.
    pub fn resolve_dimension(&self, project_id: Option<i64>, reference: &str) -> Result<i64> {
        let hits =
            crate::store_engine::read::resolve_dimension_in(self.engine.conn(), project_id, reference)?;
        let id = crate::ops::pick_id(hits, reference, || {
            crate::ops::dimension::NOUN.not_found(reference)
        })?;
        self.reachable(&format!("dimension #{id}"), |c| super::owner::dimension(c, id))?;
        Ok(id)
    }

    /// Resolve a value reference (an id, or an exact name match) inside a dimension. A value exists only
    /// within its dimension, so reach-checking the dimension is enough.
    pub fn resolve_dimension_value(&self, dimension_id: i64, reference: &str) -> Result<i64> {
        self.reachable(&format!("dimension #{dimension_id}"), |c| {
            super::owner::dimension(c, dimension_id)
        })?;
        let hits = crate::store_engine::read::resolve_dimension_value_in(
            self.engine.conn(),
            dimension_id,
            reference,
        )?;
        crate::ops::pick_id(hits, reference, || {
            crate::ops::dimension::VALUE_NOUN.not_found(reference)
        })
    }

    /// The live tasks that depend directly on `blocker_id` and have just become ready — no other blocker
    /// of theirs is still open, and every decision they rest on has been settled. Call this **after**
    /// marking the blocker done: the unblock signal only holds because that write has committed
    /// ([`crate::store_engine::read::newly_ready_by`]).
    pub fn newly_ready_by(&self, blocker_id: i64) -> Result<Vec<i64>> {
        Ok(crate::store_engine::read::newly_ready_by(self.engine.conn(), blocker_id)?)
    }

    /// Borrow the read-model the read layer queries with indexed SQL (`store_engine::read::list_task_ids`
    /// and friends) — which is **the truth-source engine itself**. Writes maintain it incrementally, so a
    /// read never has to re-project everything: it is a bounded, direct query.
    pub fn read_model(&self) -> &StoreEngine {
        &self.engine
    }

    /// This store's content-addressed blob store, rooted at `<store>/blobs`. The bytes behind
    /// `blob`-mode attachments live here, out-of-band from the engine truth source; the directories
    /// are created lazily on first ingest.
    pub fn blobs(&self) -> crate::blob::BlobStore {
        crate::blob::BlobStore::at(self.paths.base_dir.join(crate::blob::BLOBS_SUBDIR))
    }

    /// Garbage-collect blobs no live attachment references (refcount 0). The GC root set is the live
    /// `blob` attachment hashes from the read-model; present blobs outside it are removed once they are
    /// older than `min_age` (pass [`crate::blob::GC_MIN_AGE`]: a younger unreferenced blob may be an
    /// attach still in flight in another process).
    pub fn gc_blobs(&self, min_age: std::time::Duration) -> Result<crate::blob::GcReport> {
        let referenced = crate::store_engine::read::referenced_blob_hashes(self.engine.conn())?;
        self.blobs().gc(&referenced, min_age)
    }

    /// Reclaim the blobs a delete just orphaned — the **targeted** GC the delete path runs, where
    /// [`Self::gc_blobs`] is the sweep that guarantees nothing is left behind. `candidates` are the hashes
    /// the deleted rows pointed at ([`crate::ops::sweep_polymorphic`] hands them back); a candidate is only
    /// garbage if *no other* attachment still points at it — blobs are content-addressed, so the same bytes
    /// can be shared — which is asked of the read-model here, after the delete has committed (one query +
    /// one `stat` per candidate, so removing an attachment does not pay for every blob the store holds).
    /// Runs **after** the commit: reclaiming inside the transaction would delete bytes a rollback then asks
    /// for back. That ordering is also why this can be best-effort — the rows are already gone, and bytes
    /// this misses (an unreadable directory, a blob still under [`crate::blob::GC_MIN_AGE`]) are
    /// unreferenced garbage the sweep collects later.
    pub fn reclaim_blobs(
        &self,
        candidates: &[String],
        min_age: std::time::Duration,
    ) -> Result<crate::blob::GcReport> {
        let mut report = crate::blob::GcReport::default();
        if candidates.is_empty() {
            return Ok(report);
        }
        let blobs = self.blobs();
        for hash in candidates {
            if crate::store_engine::read::is_blob_referenced(self.engine.conn(), hash)? {
                continue;
            }
            let freed = blobs.reclaim(hash, min_age)?;
            if freed > 0 {
                report.removed += 1;
                report.freed_bytes += freed;
            }
        }
        Ok(report)
    }
}
