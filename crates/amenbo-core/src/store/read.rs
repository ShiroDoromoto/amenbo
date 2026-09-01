//! Serving reads. Everything here queries the truth-source engine through indexed SQL.

use crate::error::Result;
use crate::reach::Reach;
use crate::store_engine::read::FeedRow;
use crate::store_engine::StoreEngine;

use super::Store;

/// What a carrier gets back when it asks this reach for everything after its cursor
/// ([`Store::sync_changes`], `AMB-D-582`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncChanges {
    /// The changes since the cursor, **oldest first** — the order they were committed in, which is the
    /// order a copy outside has to apply them in.
    Changes {
        /// One instruction per record that moved: the dataset, the record's id, and the `op`.
        rows: Vec<FeedRow>,
        /// The cursor to come back with — the last row's id, or the one that came in on an empty page.
        cursor: i64,
        /// The `limit` cut this page short and there is more waiting; come straight back with `cursor`.
        more: bool,
    },
    /// **The cursor is gone.** The feed is a window a reader catches up through, not a history, and this
    /// one has been away longer than it holds: the changes between are no longer there to be named.
    /// Saying so is the point — an empty page would be indistinguishable from nothing having happened,
    /// and the copy outside would sit stale believing it was current. The way back is to take the whole
    /// window again (`AMB-D-583`); that snapshot names the position it was taken at, which is the cursor
    /// to resume from.
    Gap,
}

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
    // other. A resolver that also takes a **name** (`Store::resolve_dimension`) runs that same lookup
    // over the whole hit set instead of over one id: names are per-project, so what is out of reach has to
    // leave before the set is collapsed, not after.
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

    /// The read behind `search` — every place the words are written, hit by hit
    /// ([`crate::query::search`]). The one read whose answer is not a list of records but a list of
    /// places, so it is not a variant of [`Self::list_tasks`].
    pub fn search(&self, params: crate::query::SearchParams) -> Result<crate::query::SearchResult> {
        crate::query::search(self.engine.conn(), self.reach, params)
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

    /// The read behind the holder-side surface of `AMB-D-366`: premises a task acquired **after its current
    /// status began** — a blocker or unsettled decision pinned on after it was reserved, which silently
    /// dropped `ready`. Read-only; the caller (a quiet note on `task show`, a firm warn at completion) picks
    /// how strongly to react. Pass an already-resolved `task_id`.
    pub fn premise_change_since(&self, task_id: i64) -> Result<crate::view::PremiseChange> {
        crate::query::premise_change_since(self.engine.conn(), task_id)
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

    /// Resolve a reference that must name its own kind — `AMB-T-n` or `AMB-D-n`, never a bare number
    /// ([`crate::query::resolve_typed_ref`]). What this answers decides which row is written to, and
    /// the two kinds number independently, so the caller asks rather than guesses.
    pub fn resolve_typed_ref(&self, input: &str) -> Result<crate::ops::Ref> {
        let r = crate::query::resolve_typed_ref(self.engine.conn(), input)?;
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

    /// **The version of what this reach can see** — the one number that answers "has anything changed
    /// here?" for a reader carrying a copy of this store out (`AMB-D-582`). It moves on every write within
    /// the reach and stays put when nothing is written; what it never says is *what* changed, because
    /// whoever asks re-sends the whole window either way.
    ///
    /// Through a closed reach — the AI facet's binding, or the window a plugin fires in — it is that one
    /// project's version, so churn in another project does not send anyone re-reading. Through `All` it is
    /// the change feed's head: the whole device is the window, and the feed's own cursor is already the
    /// number that moves with every committed write. Both are ids from the same feed, so a project's
    /// version never runs ahead of the store's; neither is a count, and nothing but their order means
    /// anything.
    ///
    /// A project no write has reached since the store began stamping reads as `0` — below every id the
    /// feed will hand out, so the first write after that carries it forward and the copy is sent once.
    ///
    /// Two edges are worth knowing before building on it. **Compare it for inequality, not for order**:
    /// `restore` replaces the truth source with a snapshot, and the version arrives with it, so a store
    /// wound back reads *lower* than the number a carrier last saw — which is a change, and is meant to
    /// be read as one. And what it covers is the project's own records, reached through what the write
    /// door declares it touches: store-wide plugin bookkeeping that spans every project at once (a
    /// plugin's uninstall forgetting its settings) moves no project's version.
    ///
    /// See [`crate::store_engine::write::WriteTx::touches_project`] for where the number is stamped.
    pub fn sync_version(&self) -> Result<i64> {
        let conn = self.engine.conn();
        match self.reach.project() {
            Some(project_id) => crate::store_engine::read::project_version(conn, project_id),
            None => crate::store_engine::read::change_feed_head(conn),
        }
        .map_err(crate::error::engine_on(conn))
    }

    /// **What has changed in this reach since `after`** — the second of the two roads a carrier takes off
    /// this device (`AMB-D-582`). The version above says *whether* to come; this says *what*, so a carrier
    /// that has a copy already re-reads only the records that moved instead of the window entire.
    ///
    /// What comes back is the ledger's own instruction and nothing more: which dataset, which record, and
    /// which of insert / update / delete. **The record itself is not carried** — the feed holds no column
    /// names and no values by construction (`AMB-D-367`), so a carrier reads the record back by name and
    /// gets the current one. A `delete` is the exception that makes the road work at all: there is nothing
    /// left to read back, and the `op` is what lets the copy outside drop it rather than keep a record
    /// that no longer exists.
    ///
    /// **It closes on the window.** Through a closed reach — a carrier's window, the AI facet's binding —
    /// it is that project's changes and no others, by the window the write door stamped on each row; a
    /// record next door is not named, not counted, and not hinted at by a hole in the cursor. Through
    /// `All` it is the device's whole feed.
    ///
    /// `limit` bounds one read: a carrier that has been away pages through with the cursor it is handed
    /// back, and `more` says another page is waiting. The cursor to come back with is the last row's id,
    /// or the one that came in when the page is empty — so an unread-nothing read costs one indexed seek
    /// and hands back what it was given.
    ///
    /// [`SyncChanges::Gap`] is the honest answer when the cursor has fallen out of the feed's window: the
    /// changes between are gone, and saying nothing would be indistinguishable from nothing having
    /// happened. The way back is the full snapshot (`AMB-D-583`), which names the position it was taken
    /// at, so there is no cursor to hand out here.
    pub fn sync_changes(&self, after: i64, limit: i64) -> Result<SyncChanges> {
        use crate::store_engine::read::FeedSlice;
        let conn = self.engine.conn();
        let slice = crate::store_engine::read::changes_since(conn, after, limit, self.reach.project())
            .map_err(crate::error::engine_on(conn))?;
        Ok(match slice {
            FeedSlice::Changes { rows, more } => SyncChanges::Changes {
                cursor: rows.last().map(|r| r.id).unwrap_or(after),
                rows,
                more,
            },
            FeedSlice::Gap => SyncChanges::Gap,
        })
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
    /// the startup check and the GUI's per-tick snapshot — because it reads and parses every body still
    /// open to a reader; [`crate::doctor::report`] is what runs it, when a reader has actually asked.
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

    /// One plugin text field's value at this layer, or `None` when it is unset (`AMB-D-434` / `AMB-D-601`).
    pub fn plugin_config_value(
        &self,
        project_id: Option<i64>,
        plugin: &str,
        field_key: &str,
    ) -> Result<Option<String>> {
        Ok(crate::store_engine::read::plugin_config_value(
            self.engine.conn(),
            project_id,
            plugin,
            field_key,
        )?)
    }

    /// One plugin secret field's value at this layer, or `None` when it is unset (`AMB-D-434`) — read
    /// from the table an `export` must leave. The only caller that wants the value itself is the run-time
    /// injection ([`crate::plugin_inject`]); a face asks whether it is set and stops there.
    pub fn plugin_secret_value(
        &self,
        project_id: Option<i64>,
        plugin: &str,
        field_key: &str,
    ) -> Result<Option<String>> {
        Ok(crate::store_engine::read::plugin_secret_value(
            self.engine.conn(),
            project_id,
            plugin,
            field_key,
        )?)
    }

    /// Whether this layer holds a plugin's gate open (`AMB-D-434` / `AMB-D-601`) — the row's presence, which
    /// is the whole answer ([`crate::plugin_trust::effective_enabled_in`] is the boundary's name for it).
    pub fn plugin_enabled_in_project(&self, project_id: Option<i64>, plugin: &str) -> Result<bool> {
        Ok(crate::store_engine::read::plugin_enabled_in_project(
            self.engine.conn(),
            project_id,
            plugin,
        )?)
    }

    /// Every **layer** holding a plugin's gate open (`AMB-D-434` / `AMB-D-601`), whether or not the caller
    /// is standing in one of them — the twin of [`Self::plugin_enabled_in_project`] for the judgements that
    /// are about the plugin rather than about a screen, such as the `required` re-check an update runs
    /// ([`crate::plugin_config::required_unset_for_update`]).
    ///
    /// Layers rather than projects, because a `scope: machine` plugin's one gate is nobody's project and a
    /// list of project ids would leave it out — silently, and exactly where an update is deciding whether a
    /// running plugin would be left without a value its author marked `required`.
    pub fn layers_with_plugin_enabled(&self, plugin: &str) -> Result<Vec<crate::plugin_layer::Layer>> {
        use crate::plugin_layer::Layer;
        let conn = self.engine.conn();
        let mut layers = Vec::new();
        for id in crate::store_engine::read::plugin_enable_row_ids(conn, plugin)? {
            if let Some(row) = crate::store_engine::read::plugin_enable_row_by_id(conn, id)? {
                layers.push(row.project_id.map_or(Layer::Device, Layer::Project));
            }
        }
        Ok(layers)
    }

    /// Every project holding a value for one plugin — either road, since a setting is a setting whichever
    /// table the author's `secret` flag sent it to (`AMB-D-356`). Ascending, each project once.
    ///
    /// The value-side twin of [`Self::layers_with_plugin_enabled`], and asked for the same reason: a
    /// project that filled a plugin in and then turned it off still has settings there
    /// ([`crate::plugin_config::intersections`] draws it a row), and no gate would name it.
    ///
    /// Projects only: this answers a face that lists **project** crossings, and the device layer is not one
    /// of them (`AMB-D-601`). A device row is skipped rather than folded into some project's.
    pub fn projects_with_plugin_values(&self, plugin: &str) -> Result<Vec<i64>> {
        let conn = self.engine.conn();
        let mut projects = Vec::new();
        for id in crate::store_engine::read::plugin_config_row_ids(conn, plugin)? {
            if let Some(row) = crate::store_engine::read::plugin_config_row_by_id(conn, id)? {
                projects.extend(row.project_id);
            }
        }
        for id in crate::store_engine::read::plugin_secret_row_ids(conn, plugin)? {
            if let Some(row) = crate::store_engine::read::plugin_secret_row_by_id(conn, id)? {
                projects.extend(row.project_id);
            }
        }
        projects.sort_unstable();
        projects.dedup();
        Ok(projects)
    }

    /// A single task comment; `None` if there is none (a row exists ⇒ it is live). The id is a comment id, which
    /// is not a conversational ref, so this is itself a reach entry point — `decision promote` reads the
    /// body of a task comment through here.
    pub fn task_comment(&self, id: i64) -> Result<Option<crate::model::TaskComment>> {
        self.reachable(&crate::idref::task_comment(id), |c| super::owner::task_comment(c, id))?;
        Ok(crate::store_engine::read::task_comment(self.engine.conn(), id)?)
    }

    /// A single decision comment; `None` if there is none (a row exists ⇒ it is live). The decision-side
    /// twin of [`Self::task_comment`], and a reach entry point for the same reason — `decision promote`
    /// reads the body of either kind of comment through one of these two.
    pub fn decision_comment(&self, id: i64) -> Result<Option<crate::model::DecisionComment>> {
        self.reachable(&crate::idref::decision_comment(id), |c| super::owner::decision_comment(c, id))?;
        Ok(crate::store_engine::read::decision_comment(self.engine.conn(), id)?)
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
        let ids =
            crate::store_engine::read::live_attachment_ids_for_target(conn, target_type, target_id)?;
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

    /// The names of the project's required axes a decision carries no value on, in display order
    /// (`AMB-D-790`). Empty means every demand this project makes of a decision is answered — which is
    /// the same question `decision accept` asks at its door, put here so a surface can ask it without
    /// having to be turned away first. A decision that is gone answers with nothing to fill in.
    pub fn unmet_required_decision_axes(&self, decision_id: i64) -> Result<Vec<String>> {
        self.reachable_decision(decision_id)?;
        let conn = self.engine.conn();
        let Some(decision) = crate::store_engine::read::decision(conn, decision_id)? else {
            return Ok(Vec::new());
        };
        crate::ops::decision::unmet_required_axes(conn, &decision)
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

    /// Resolve a dimension reference (an id, its slug, or an exact name match — `AMB-D-735`). Passing
    /// `project_id` confines the search to that project; a call that does **not** confine it
    /// (`dimension show <name>`) searches the whole machine.
    ///
    /// Whatever the scope, **what this facet cannot reach is dropped before the hit set is collapsed**.
    /// Names are per-project, so a name a second project also uses would otherwise collapse to
    /// `ambiguous` — and the candidates that error lists are ids of rows outside the binding, which is
    /// the very content a closed reach exists to keep out of the answer. Filtering first leaves exactly
    /// what the caller could have meant.
    ///
    /// Filtering away every hit must not turn into "it does not exist": a reference that still matches
    /// something out there is answered `out_of_reach`, the same as one naming an id outright.
    pub fn resolve_dimension(&self, project_id: Option<i64>, reference: &str) -> Result<i64> {
        let conn = self.engine.conn();
        let hits = crate::store_engine::read::resolve_dimension_in(conn, project_id, reference)?;
        let hits = match self.reach {
            // Under `All` nothing is dropped and the owner lookups do not even run: humans, the GUI and
            // library use pay nothing for this.
            Reach::All => hits,
            _ => {
                let mut kept = Vec::with_capacity(hits.len());
                for id in hits {
                    if self.reach.allows(super::owner::dimension(conn, id)?) {
                        kept.push(id);
                    }
                }
                if kept.is_empty() {
                    // Nothing left in scope. Look once across the store before answering not-found: a
                    // reference that does match a row we cannot reach is `out_of_reach`. The reference is
                    // quoted back rather than the ids it found, so the answer names nothing outside.
                    let anywhere =
                        crate::store_engine::read::resolve_dimension_in(conn, None, reference)?;
                    if let Some(&outside) = anywhere.first() {
                        self.reach.check(
                            &format!("dimension '{reference}'"),
                            super::owner::dimension(conn, outside)?,
                        )?;
                    }
                }
                kept
            }
        };
        crate::ops::pick_id(hits, reference, || {
            crate::ops::dimension::NOUN.not_found(reference)
        })
    }

    /// Resolve a value reference (an id, its slug, or an exact name match — `AMB-D-735`) inside a
    /// dimension. A value exists only within its dimension, so reach-checking the dimension is enough,
    /// and a slug is unique only that far.
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
