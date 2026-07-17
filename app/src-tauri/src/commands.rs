//! GUI ↔ core wiring. Every command opens the store, reads or writes, and drops it right away
//! (open-per-action). The lock is held for an instant only, so the CLI can touch the same store
//! concurrently. DTOs are shaped into camelCase here in the command layer, and `#[derive(TS)]`
//! generates `app/src/bindings/bindings.ts` during `cargo test` (the single source of the TS
//! types). Literal unions are pinned with `#[ts(type = ...)]`; `skip_serializing_if` is made
//! optional with `#[ts(optional)]`.

use crate::error::CmdError;
use amenbo_core::model::{ActorKind, DimensionRole, Priority, TaskStatus};
use amenbo_core::time::Timestamp;
use amenbo_core::{query, Store};
use chrono::NaiveDate;
use serde::Serialize;
use ts_rs::TS;

/// Fail with the reason if the startup migration ([`crate::migrate::run`]) is **still running** or
/// has **failed**. A store mid-migration sits between versions, and a failed migration is rolled
/// back whole — the store is intact but stuck at the old version, and nobody knows what this build
/// would do if it read that. So every path that opens the store comes through here and shows the
/// reason instead of the data — **a new command that opens the store must go through here**
/// (automatic via `open_store` / `open_store_read` / `with_store_read`).
fn ensure_migrated() -> Result<(), CmdError> {
    crate::migrate::gate()
}

/// Open the store for writing. There is exactly one store, so the target is always `resolve()`
/// (**directory-independent** — the GUI process has no `.amenbo` of its own).
fn open_store() -> Result<Store, CmdError> {
    ensure_migrated()?;
    Store::open_at(amenbo_core::config::Paths::resolve()?).map_err(CmdError::from)
}

/// Lightweight read-only open: the same store as `open_store`, opened through the persistent
/// engine's back-projection instead of paying for a full hydrate (`Store::open_read_at`).
/// **Read commands only** — never call a write on the `Store` it hands back. Falls back to a full
/// open internally if the engine has not been primed yet.
fn open_store_read() -> Result<Store, CmdError> {
    ensure_migrated()?;
    Store::open_read_at(amenbo_core::config::Paths::resolve()?).map_err(CmdError::from)
}

/// Write side. Opens the store and hands out `&mut Store` to mutate (the write wrappers commit
/// per operation).
/// **Projection (build_snapshot) is done separately, after the lock is released** — it reopens the
/// same store, so projecting in here would collide re-entrantly with our own lock.
fn with_store_mut<T>(f: impl FnOnce(&mut Store) -> Result<T, CmdError>) -> Result<T, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("store.write");
    let mut store = open_store()?;
    let out = f(&mut store);
    drop(store);
    out
}

/// The read entry point. If there is no store yet (store file not created), nothing is opened and
/// `f` is never called — the GUI draws an empty state (we do not silently genesis one). Lightweight
/// read-only open (`Store::open_read_at`).
fn with_store_read(f: impl FnOnce(&Store) -> Result<(), CmdError>) -> Result<(), CmdError> {
    ensure_migrated()?;
    let paths = amenbo_core::config::Paths::resolve()?;
    if amenbo_core::env::home().is_some() || paths.store_file.exists() {
        let store = Store::open_read_at(paths)?;
        f(&store)?;
    }
    Ok(())
}

/// Value-returning flavour of [`with_store_read`]: `None` when there is no store.
fn find_in_store<T>(
    f: impl FnOnce(&Store) -> Result<Option<T>, CmdError>,
) -> Result<Option<T>, CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    if amenbo_core::env::home().is_some() || paths.store_file.exists() {
        let store = Store::open_read_at(paths)?;
        return f(&store);
    }
    Ok(None)
}

/// Emit a system event into the file ledger (same shape as the CLI's `emit_event`). The GUI actor
/// is always human. Call it **after** the mutation wrapper has committed. Activity is not a system
/// of record, so a failed row write must not fail the operation — warn, carry on, and err on the
/// side of a missing line.
fn emit(store: &mut Store, target_id: i64, event: serde_json::Value) {
    if let Err(e) = store.add_system_event(ActorKind::Human, target_id, event) {
        tracing::warn!("could not record the activity event: {e}");
    }
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ActorDto {
    user_id: String,
    name: String,
    #[ts(type = "\"human\" | \"ai\"")]
    kind: &'static str,
    /// Optional avatar image for the facet (data URL). The roster loads it from config; other
    /// ActorDto uses (assignee, author) leave it unset. Omitted when unset, and the front end draws
    /// an identicon instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    avatar: Option<String>,
}

/// One value of a dimension (a choice on the axis). Ordered dimensions arrive in `order_key` order.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DimensionValueDto {
    #[ts(type = "number")]
    id: i64,
    name: String,
    /// Start of the period, `YYYY-MM-DD` (inclusive). Omitted means an open start. A period is the
    /// payload of `role: time_axis`, not a generic attribute of a value — reads pass it straight
    /// through, and the gatekeeper for showing the date fields sits in the GUI.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    start_on: Option<String>,
    /// End of the period, `YYYY-MM-DD` (inclusive). Omitted means "ongoing" (an open end).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    end_on: Option<String>,
}

/// One unified dimension (classification axis), values included, so the GUI's dimension editor and
/// assignment selects render from real data. `role` is `none` or `time_axis` (phase); `ordered`
/// says whether the values have an order.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DimensionDto {
    #[ts(type = "number")]
    id: i64,
    name: String,
    notes: String,
    #[ts(type = "\"none\" | \"time_axis\"")]
    role: String,
    ordered: bool,
    values: Vec<DimensionValueDto>,
}

/// One task × dimension assignment (`valueId` is set on the `dimensionId` axis). The detail pane's
/// assignment selects use it to reflect the current value.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct TaskDimensionAssignmentDto {
    #[ts(type = "number")]
    dimension_id: i64,
    #[ts(type = "number")]
    value_id: i64,
}

/// The per-task assigned value for one project × dimension (`taskId`→`valueId`). The board uses it
/// to bundle tasks by value on the chosen dimension (browsing/grouping).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DimensionTaskValueDto {
    #[ts(type = "number")]
    task_id: i64,
    #[ts(type = "number")]
    value_id: i64,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    #[ts(type = "number")]
    id: i64,
    name: String,
    color: String,
    #[ts(type = "\"list\" | \"board\" | \"calendar\" | \"timeline\"")]
    view: String,
    /// Open task count (todo/in_progress/blocked — anything but done, live only). The sidebar's
    /// count badge.
    open_count: usize,
    /// Unified dimensions (classification axes). Empty means none are in use. Task classification
    /// happens on these axes and nowhere else.
    dimensions: Vec<DimensionDto>,
}

/// The editable fields of one project, so the project settings screen can prefill its form.
/// The snapshot's `ProjectDto` does not carry notes/archived (every project rides in it — keep it
/// light), so we fetch them with `project_get` only when the settings screen opens. `archived` is
/// included (unarchiving is driven from this screen too).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettingsDto {
    #[ts(type = "number")]
    id: i64,
    name: String,
    notes: String,
    color: String,
    #[ts(type = "\"list\" | \"board\" | \"calendar\" | \"timeline\"")]
    view: String,
    archived: bool,
}

/// One row of the collapsible "Archived (N)" section at the foot of the sidebar. These never ride
/// in the snapshot's `ProjectDto` (which comes from `project_overview` — active projects only), so
/// they are fetched over a dedicated read path, `project_list_archived`. Restoring navigates to the
/// settings screen by this id and calls `project_set_archived`.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ArchivedProjectDto {
    #[ts(type = "number")]
    id: i64,
    name: String,
    color: String,
}

/// One bound folder, as listed by the folder manager on the project settings screen. `path` is the
/// absolute path where the `.amenbo` pointer was placed; `exists` says whether that folder is still
/// there (false means moved or deleted — stale, and we offer a way to clean it up). Same shape as
/// the CLI's `project show` `bound_folders` (`bound_folders_json`).
/// `mismatch` is the verdict "that folder's `.amenbo` belongs to a different store"
/// ([`SlugMismatchDto`]). `legacy` means "the pointer is in the old format (`project_id` is not
/// readable as an integer)" — both are fixed by the same relink (rewriting the pointer in the
/// current format). A pointer with no `project_id` cannot mismatch, so the two are exclusive.
/// `pointer_missing` means "the folder is there, but it has no readable `.amenbo`" — the registry
/// points at this project, yet an AI started in that folder will not resolve here (it walks up to a
/// parent, or falls back to `init` recovery). Exclusive with the other two (no pointer, nothing to
/// inspect inside it), and the fix is the same relink.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct BoundFolderDto {
    path: String,
    exists: bool,
    mismatch: Option<SlugMismatchDto>,
    legacy: bool,
    pointer_missing: bool,
}

/// The slug in `.amenbo` disagrees with what the store actually holds
/// ([`amenbo_core::binding::SlugMismatch`]). The CLI prints an English warning in its location
/// header; the GUI hands over the raw material only and lets i18n compose the wording (same verdict,
/// said differently). **Resolution is not blocked** (the id is authoritative) — we report it and
/// nudge a relink (`project_bind_folder`), nothing more.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SlugMismatchDto {
    /// Primary key of the project the pointer names (whatever this number leads to, it is not the
    /// project whose slug was recorded).
    /// `number` on the TS side (the default `bigint` cannot be interpolated into the warning text).
    #[ts(type = "number")]
    project_id: i64,
    /// The slug that was written in `.amenbo`.
    recorded: String,
    /// The slug of the project `project_id` actually points at (it may not have one).
    actual: Option<String>,
}

/// A reference to a record a decision points at, or is pointed at by (id + display name +
/// conversational ref). For cross-link display: `D-<n>` when the target is a decision, `#<n>` when
/// it is a task (the numbering spaces are separate). Both ids are integer keys.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DecisionRefDto {
    #[ts(type = "number")]
    id: i64,
    /// `null` when a forward edge dangles (a supersedes / amends target no longer live); the screen
    /// composes the placeholder in `config.language`. Reverse edges always carry a name.
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    r#ref: Option<String>,
}

/// A reference to a premise decision (the far end of builds_on). It is more than a
/// [`DecisionRefDto`] because it carries **whether the premise is still alive** — surfacing on
/// screen the decisions that stand on a rotten premise (the whole reason this type exists).
/// `superseded_by` is the conversational ref (`AMB-D-<n>`) of the decision that overturned the premise,
/// and is omitted when the premise is current (currency is not stored anywhere else — whether this
/// field is empty *is* the answer).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PremiseRefDto {
    #[ts(type = "number")]
    id: i64,
    /// `null` when the premise target dangles (a `builds_on` onto a decision no longer live); the screen
    /// composes the placeholder in `config.language`.
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    r#ref: Option<String>,
    /// Ref (`D-<n>`) of the decision that overturned the premise. Absent means the premise is current.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    superseded_by: Option<String>,
}

/// A reference with no entity key behind it (a decision's `decided_by` — an opaque token that
/// cannot be looked up).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PlainRefDto {
    id: String,
    name: String,
}

/// One decision record. The real data behind the list, the detail view and the cross-links.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DecisionDto {
    #[ts(type = "number")]
    id: i64,
    /// Conversational ref (`D-<n>`, a numbering space of its own, separate from tasks). The display
    /// form of `id`.
    r#ref: String,
    title: String,
    body: String,
    /// proposed / accepted / rejected. "Superseded" is not a status — look at `current`.
    #[ts(type = "\"proposed\" | \"accepted\" | \"rejected\"")]
    status: String,
    /// Is it current? (derived projection, never stored): true unless a live `supersedes` edge
    /// points at it.
    current: bool,
    /// The project it lives under (the id is an integer key).
    project: Option<MembershipRefDto>,
    /// Decisions this one replaced (supersession, forward). One decision can replace several.
    supersedes: Vec<DecisionRefDto>,
    /// Decisions that replaced this one (reverse lookup).
    superseded_by: Vec<DecisionRefDto>,
    /// Decisions this one partially revised (amends, forward; the target stays current).
    amends: Vec<DecisionRefDto>,
    /// Decisions that partially revised this one (reverse lookup).
    amended_by: Vec<DecisionRefDto>,
    /// Decisions this one takes as a premise (builds_on, forward) — read them first. They stay current.
    builds_on: Vec<PremiseRefDto>,
    /// Decisions that take this one as a premise (reverse lookup) — what would need revisiting if
    /// this one were overturned (the blast radius).
    built_on_by: Vec<DecisionRefDto>,
    decided_at: Option<String>,
    decided_by: Option<PlainRefDto>,
    /// Linked tasks (cross-link), carrying status — is the work this decision created still open?
    linked_tasks: Vec<LinkedTaskRefDto>,
    created_at: String,
}

/// A reference to a task a decision spawned. A [`DecisionRefDto`] plus **status**, so the screen can
/// answer "is this decision's work finished yet?". Completed ones are muted on the screen side.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct LinkedTaskRefDto {
    #[ts(type = "number")]
    id: i64,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    r#ref: Option<String>,
    #[ts(type = "\"todo\" | \"in_progress\" | \"done\" | \"blocked\"")]
    status: String,
}

/// A reference to the project a membership names (id + display name). The id is an integer key.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct MembershipRefDto {
    #[ts(type = "number")]
    id: i64,
    name: String,
}

/// A reference to a task (id + title). The id is an integer key.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct TaskRefDto {
    #[ts(type = "number")]
    id: i64,
    name: String,
}

/// The membership of one task (project only — classification lives on the dimension axes). The real
/// data behind the membership row in the task detail view.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct MembershipDto {
    project: MembershipRefDto,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct TaskCardDto {
    #[ts(type = "number")]
    id: i64,
    title: String,
    /// The name it goes by on screen, `#<n>` (the display form of `id`).
    r#ref: String,
    notes: String,
    #[ts(type = "number | null")]
    project_id: Option<i64>,
    #[ts(type = "\"todo\" | \"in_progress\" | \"done\" | \"blocked\"")]
    status: &'static str,
    assignee: Option<ActorDto>,
    #[ts(type = "\"high\" | \"medium\" | \"low\" | null")]
    priority: Option<&'static str>,
    due: Option<String>,
    due_label: Option<String>,
    /// Completion timestamp (RFC3339 UTC). Used to sort the Done column newest-first, among other
    /// things. None while the task is still open.
    completed_at: Option<String>,
    comments: usize,
    /// Derived from dependencies: can it be reserved? — no open blockers, and every decision it
    /// rests on has been settled, the two reasons [`amenbo_core::view::ReserveBlocker`] enumerates.
    ready: bool,
    /// Dependencies: blockers that are not done yet (id + name). Drives the "waiting on X" line in
    /// the detail pane. Empty means it can be started.
    blocked_by: Vec<TaskRefDto>,
    /// Membership (with the project's display name), so the detail pane's membership row renders
    /// from real data. One task has one membership, but an unfiled task (inbox) can have none, so
    /// this stays a Vec. The primary membership is `memberships[0]`.
    memberships: Vec<MembershipDto>,
    created_by: Option<ActorDto>,
    /// The decision records that motivated this task (cross-link). Symmetric with
    /// `DecisionDto.linked_tasks`; drives navigation from the task detail view to the decision record.
    linked_decisions: Vec<DecisionRefDto>,
    /// Those `linked_decisions` that are not settled yet as grounds. Together with `blocked_by` they
    /// determine `ready` (both empty means ready). The reason a reservation was refused
    /// (`not_ready`) only ever appears in a toast that vanishes in seconds, so we name the decisions
    /// that are holding it back, letting the detail pane hold the same fact permanently.
    blocked_by_decisions: Vec<DecisionRefDto>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ActivityTargetDto {
    /// Decisions are destinations too (`decision.deleted` names a decision in the ledger's decision
    /// column). If the type only said task/project, the front end's branch would drop decisions on
    /// the floor.
    #[serde(rename = "type")]
    #[ts(type = "\"task\" | \"project\" | \"decision\"")]
    target_type: String,
    #[ts(type = "number")]
    id: i64,
    title: String,
    /// Is the target still around? Only a live target can be a destination — rows for deleted tasks,
    /// projects and decisions stay in the ledger but have nowhere to open, so it is this, not the
    /// type, that decides whether the row is clickable.
    live: bool,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct EventDto {
    kind: String,
    text: String,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ActivityItemDto {
    #[ts(type = "number")]
    id: i64,
    at: String,
    ago: String,
    #[ts(type = "\"system\" | \"comment\"")]
    kind: String,
    author: ActorDto,
    target: ActivityTargetDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    event: Option<EventDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    text: Option<String>,
    /// Comment rows only: relative time of a later in-place edit of the body. Absent when it was
    /// never edited. No revision history is kept, so this is the only hint a reader gets that the
    /// body is not what they read a moment ago.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    edited_ago: Option<String>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    me_user_id: String,
    /// The user's language (config.json, global). Decides how the GUI localizes its UI labels. Null
    /// when unset.
    language: Option<String>,
    /// First-run setup completed (config.json). False makes the GUI show first-run setup.
    onboarded: bool,
    /// This person's roster — the two facets that come from config (human / ai). It is the one
    /// supply line for every roster in the GUI: the assignee picker (unassigned / human name / AI
    /// name), the display name and avatar in settings, and display-name resolution. `user_id` is the
    /// facet token (`human`/`ai`); `name` is the effective display name from `config.human_name` /
    /// `ai_name`.
    roster: Vec<ActorDto>,
    projects: Vec<ProjectDto>,
    // Tasks and decisions are not carried here in full. Lists come from `task_page` and
    // `decision_page`, and single records from `tasks_by_ids` / `decisions_by_ids` — each fetching
    // only the window it needs (bounded memory).
    activity: Vec<ActivityItemDto>,
    /// Findings of the read-only integrity check run at startup. If anything is wrong, the GUI
    /// raises a warning banner (it never repairs anything by itself). A store with
    /// `config.startup_integrity_check` off adds nothing here.
    startup_health: StartupHealthDto,
    /// Whether an update exists. If the published `latest.json` names a version newer than the one
    /// running, `updateAvailable=true` — the material for the GUI's "an update is available (open
    /// the installer)" banner.
    version_status: VersionStatusDto,
    /// Level of perf instrumentation (the explicit value of `config.perf_log` — `off`,
    /// `budget-only` or `verbose`). Null when unset, and the front end falls back to the dev-build
    /// default of on (budget-only).
    perf_log: Option<String>,
    /// Update checking on or off (`config.update_check`, default true). Exposed so the settings
    /// screen's toggle can reflect the current value. When off, upstream latest.json is never
    /// queried, so `update_available` can never be raised.
    update_check: bool,
}

/// The startup integrity check, shaped for the GUI: it feeds a read-only warning banner. Empty means
/// no warning (the counterpart of the CLI's stderr warning).
#[derive(Serialize, Default, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct StartupHealthDto {
    /// The problems doctor found (orphaned or dangling references, and so on). No prose sentence
    /// rides along — the GUI composes one from the kind and params in `config.language`
    /// (`src/core/i18n.ts`), so we hand these over as the same [`DoctorIssueDto`] the doctor screen
    /// uses.
    issues: Vec<DoctorIssueDto>,
}

impl StartupHealthDto {
    /// Absorb the startup_check of an opened store. A read open (`open_read_at`) deliberately
    /// **does not compute** the O(total) doctor pass (it keeps per-click reads inside their budget),
    /// so we compute it here, and only when it is needed. A full open (`open_at`) already computed
    /// it while opening, so that result is used. Does nothing when the startup integrity check is
    /// disabled.
    fn absorb(&mut self, store: &Store) {
        let computed;
        let h = match &store.startup_check {
            Some(h) => h,
            None if store.config.startup_integrity_check => {
                let Ok(health) = store.compute_startup_health() else { return };
                computed = health;
                &computed
            }
            None => return,
        };
        self.issues
            .extend(h.doctor.issues.iter().map(DoctorIssueDto::from));
    }
}

/// The **update available** state, for the GUI. Takes the store's `version_status` and raises
/// `update_available` when upstream (the published `latest.json`) names a version newer than the one
/// running. That is what puts up the GUI's "an update is available" banner and its "open the
/// installer" affordance.
#[derive(Serialize, Default, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct VersionStatusDto {
    /// Version of the GUI binary that is running.
    app_version: String,
    /// A newer version exists in the published distribution.
    update_available: bool,
    /// The version being offered (for display; the first one found). `None` means no update.
    newer_version: Option<String>,
}

impl VersionStatusDto {
    /// Absorb this store's version state. `update_available` is raised when `upstream` (the
    /// published latest.json) names a version newer than the one running. `None` — update checking
    /// disabled, not fetched, or the fetch failed — means no update.
    fn absorb(&mut self, store: &Store, upstream: Option<&amenbo_core::update_check::LatestRelease>) {
        let vs = store.version_status().with_upstream(upstream);
        if self.app_version.is_empty() {
            self.app_version = vs.app_version.to_string();
        }
        if vs.update_available {
            self.update_available = true;
            if self.newer_version.is_none() {
                self.newer_version = vs.newer_version;
            }
        }
    }
}

/// What `task_page` returns: the task cards on the page, plus the total number of matches before
/// paging. The front end sizes its pager or virtual scroller from `total_matched` and draws only the
/// window in `tasks` (it never holds them all).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct TaskPageDto {
    tasks: Vec<TaskCardDto>,
    /// Total number of matches, before paging (limit/offset) is applied.
    total_matched: usize,
    /// The offset that was applied (how many were skipped).
    offset: usize,
    /// The limit that was applied (page size). None means no cap — everything from `offset` on.
    limit: Option<usize>,
}

/// Shape a facet (human / ai) into the GUI's [`ActorDto`]. A single local store has two facets —
/// "me (the human)" and "my AI" — and both belong to the same one person: me. `user_id` is the
/// person token (the human facet's `"human"`, which in a single store is always me), `kind`
/// separates human from ai, and the display name is looked up in config (`human_name`/`ai_name`) —
/// the read-model carries no names. An [`ActorDto`] used as a label (assignee, author) gets no face:
/// the roster is the only thing that supplies avatars from config.
fn facet_actor(config: &amenbo_core::config::Config, kind: Option<ActorKind>) -> ActorDto {
    let kind = kind.unwrap_or(ActorKind::Human);
    let name = match kind {
        ActorKind::Ai => config.ai_display_name(),
        ActorKind::Human => config.human_display_name(),
    };
    ActorDto { user_id: ActorKind::Human.as_str().to_string(), name, kind: kind.as_str(), avatar: None }
}

fn date_iso(d: NaiveDate) -> String {
    amenbo_core::time::date_to_string(d)
}

/// The human label for a due date (today, tomorrow, in N days, ...). Core returns the bare date, so
/// the GUI side does the wording.
fn due_label(d: NaiveDate) -> String {
    let diff = (d - amenbo_core::time::today()).num_days();
    match diff {
        0 => "今日".to_string(),
        1 => "明日".to_string(),
        -1 => "昨日".to_string(),
        n if n > 1 => format!("{n}日後"),
        n => format!("{}日前", -n),
    }
}

/// The relative-time label (just now, N minutes ago, ...).
fn ago_label(at: &Timestamp) -> String {
    let secs = (chrono::Utc::now() - at.0).num_seconds().max(0);
    if secs < 60 {
        "たった今".to_string()
    } else if secs < 3600 {
        format!("{}分前", secs / 60)
    } else if secs < 86400 {
        format!("{}時間前", secs / 3600)
    } else {
        format!("{}日前", secs / 86400)
    }
}

/// Normalize config.language to "ja" or "en" (unset or unknown falls back to the default, ja). Used
/// to pick the wording of system event lines (`render_event`).
fn lang_code(language: &Option<String>) -> &'static str {
    match language.as_deref() {
        Some(l) if l.eq_ignore_ascii_case("en") || l.to_ascii_lowercase().starts_with("en-") => "en",
        _ => "ja",
    }
}

/// The stand-in name for a target whose name could not be recovered. Core returns an empty
/// `Item.title` when the target is not there as a live row and no ledger row carrying its name could
/// be found either (it fell out in the ledger's self-compaction, or it lay outside the name lookback
/// budget `NAME_SCAN_BUDGET`) — both end in the same place, a vanished target, so both get the same
/// wording. Core has no notion of language, so the emptiness is carried this far and worded here.
fn nameless_title(lang: &str) -> &'static str {
    if lang == "en" { "(deleted)" } else { "（削除済み）" }
}

/// Turn a system event into the line the GUI shows. Under Tauri the wording is chosen here; the
/// browser fallback goes through tf() in mutations.ts. Keys and wording must stay in step with
/// act.* in app/src/core/i18n.ts.
fn render_event(ev: &serde_json::Value, title: &str, lang: &str) -> EventDto {
    let kind = ev
        .get("kind")
        .and_then(|k| k.as_str())
        .unwrap_or("event")
        .to_string();
    let en = lang == "en";
    let status_label = |s: &str| -> String {
        match (s, en) {
            ("todo", false) => "未着手",
            ("todo", true) => "To do",
            ("in_progress", false) => "着手中",
            ("in_progress", true) => "In progress",
            ("done", false) => "完了",
            ("done", true) => "Done",
            ("blocked", false) => "ブロック",
            ("blocked", true) => "Blocked",
            (other, _) => other,
        }
        .to_string()
    };
    let text = match kind.as_str() {
        "task.created" => if en { format!("Created “{title}”") } else { format!("「{title}」を作成") },
        "task.status_changed" => {
            let new = ev.get("new").and_then(|x| x.as_str()).unwrap_or("");
            if en {
                format!("Changed “{title}” to {}", status_label(new))
            } else {
                format!("「{title}」を{}に変更", status_label(new))
            }
        }
        "task.assigned" => {
            let to_user_null = ev.get("to_user").map(|v| v.is_null()).unwrap_or(true);
            let ai = ev.get("to_kind").and_then(|x| x.as_str()) == Some("ai");
            if to_user_null {
                if en { format!("Unassigned “{title}”") } else { format!("「{title}」の担当を外す") }
            } else if ai {
                if en { format!("Delegated “{title}” to AI") } else { format!("「{title}」を AI に委任") }
            } else if en {
                format!("Assigned “{title}”")
            } else {
                format!("「{title}」を担当に割り当て")
            }
        }
        "task.moved" => if en { format!("Moved “{title}”") } else { format!("「{title}」を移動") },
        "task.unblocked" => if en { format!("“{title}” is now unblocked (ready)") } else { format!("「{title}」が着手可能に（依存解除）") },
        "task.deleted" | "decision.deleted" => {
            if en { format!("Deleted “{title}”") } else { format!("「{title}」を削除") }
        }
        "project.deleted" => {
            let count = |field: &str| ev.get(field).and_then(|x| x.as_u64()).unwrap_or(0);
            let (tasks, decisions) = (count("tasks"), count("decisions"));
            match (en, tasks + decisions) {
                (true, 0) => format!("Deleted “{title}”"),
                (false, 0) => format!("「{title}」を削除"),
                (true, _) => {
                    let unit = |n: u64, one: &str| if n == 1 { format!("{n} {one}") } else { format!("{n} {one}s") };
                    format!("Deleted “{title}” ({}, {})", unit(tasks, "task"), unit(decisions, "decision"))
                }
                (false, _) => format!("「{title}」を削除（タスク{tasks}件・決定{decisions}件）"),
            }
        }
        _ => if en { format!("Updated “{title}”") } else { format!("「{title}」を更新") },
    };
    EventDto { kind, text }
}

/// Build a [`TaskCardDto`] from a read-model [`amenbo_core::store_engine::read::TaskCardRow`].
/// This is the card path: the row already carries the resolved project names, the actors' facets,
/// the open blockers and the comment count, so a card costs one indexed query. Actors are facet
/// one — the display name comes from `config` (`human_name`/`ai_name`). The top-level project id
/// comes from the primary (first) membership.
fn task_card_from_row(store: &Store, row: amenbo_core::store_engine::read::TaskCardRow) -> TaskCardDto {
    let config = &store.config;
    let card_kind = |a: &amenbo_core::store_engine::read::CardActor| a.kind.as_deref().and_then(ActorKind::parse);

    let project_id = row.memberships.first().map(|m| m.project_id);

    let membership_dtos: Vec<MembershipDto> = row
        .memberships
        .iter()
        .map(|m| MembershipDto {
            project: MembershipRefDto {
                id: m.project_id,
                name: m.project_name.clone().unwrap_or_default(),
            },
        })
        .collect();

    let assignee = row.assignee.as_ref().map(|a| facet_actor(config, card_kind(a)));
    let created_by = row.created_by.as_ref().map(|a| facet_actor(config, card_kind(a)));

    let due_date = row.due_on.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let ready = row.blocked_by.is_empty() && row.blocked_by_decisions.is_empty();
    let blocked_by: Vec<TaskRefDto> = row
        .blocked_by
        .into_iter()
        .map(|(id, name)| TaskRefDto { id, name })
        .collect();

    let linked_decisions: Vec<DecisionRefDto> = row
        .linked_decisions
        .into_iter()
        .map(|r| DecisionRefDto { id: r.id, name: r.name, r#ref: r.display_ref })
        .collect();
    let blocked_by_decisions: Vec<DecisionRefDto> = row
        .blocked_by_decisions
        .into_iter()
        .map(|r| DecisionRefDto { id: r.id, name: r.name, r#ref: r.display_ref })
        .collect();

    TaskCardDto {
        r#ref: amenbo_core::idref::task(row.id),
        id: row.id,
        title: row.title,
        notes: row.notes,
        project_id,
        status: TaskStatus::parse(&row.status).unwrap_or_default().as_str(),
        assignee,
        priority: row.priority.as_deref().and_then(Priority::parse).map(|p| p.as_str()),
        due: due_date.map(date_iso),
        due_label: due_date.map(due_label),
        completed_at: row
            .completed_at
            .as_deref()
            .and_then(amenbo_core::time::Timestamp::parse_rfc3339)
            .map(|ts| ts.to_rfc3339_z()),
        comments: row.num_comments,
        ready,
        blocked_by,
        memberships: membership_dtos,
        created_by,
        linked_decisions,
        blocked_by_decisions,
    }
}

/// Scratch accumulator for building the snapshot projection.
#[derive(Default)]
struct Acc {
    me_user_id: String,
    projects: Vec<ProjectDto>,
    activity: Vec<ActivityItemDto>,
}

/// Build a GUI [`DecisionDto`] from a read-model [`amenbo_core::store_engine::read::DecisionCardRow`] (the
/// decision twin of [`task_card_from_row`]). The row already carries every cross-ref's `D-n`/`#n`, so
/// the card costs one query and no scan of the decisions or the tasks. Timestamps are re-normalized
/// through `Timestamp` so the rendered rfc3339 is the one shape the GUI ever sees.
fn decision_card_from_row(row: amenbo_core::store_engine::read::DecisionCardRow) -> DecisionDto {
    use amenbo_core::time::Timestamp;
    let to_ref = |r: amenbo_core::store_engine::read::DecisionCardRef| DecisionRefDto {
        id: r.id,
        name: r.name,
        r#ref: r.display_ref,
    };
    let plain_ref = |r: amenbo_core::view::Ref| PlainRefDto { id: r.id, name: r.name };
    DecisionDto {
        r#ref: amenbo_core::idref::decision(row.id),
        id: row.id,
        title: row.title,
        body: row.body,
        status: row.status,
        current: row.current,
        project: row.project.map(|p| MembershipRefDto { id: p.id, name: p.name }),
        supersedes: row.supersedes.into_iter().map(to_ref).collect(),
        superseded_by: row.superseded_by.into_iter().map(to_ref).collect(),
        amends: row.amends.into_iter().map(to_ref).collect(),
        amended_by: row.amended_by.into_iter().map(to_ref).collect(),
        builds_on: row
            .builds_on
            .into_iter()
            .map(|p| PremiseRefDto {
                id: p.decision.id,
                name: p.decision.name,
                r#ref: p.decision.display_ref,
                superseded_by: p.superseded_by,
            })
            .collect(),
        built_on_by: row.built_on_by.into_iter().map(to_ref).collect(),
        decided_at: row
            .decided_at
            .as_deref()
            .and_then(Timestamp::parse_rfc3339)
            .map(|t| t.to_rfc3339_z()),
        decided_by: row.decided_by.map(plain_ref),
        linked_tasks: row
            .linked_tasks
            .into_iter()
            .map(|t| LinkedTaskRefDto {
                id: t.task.id,
                // A linked task is always live, so its title is always present — the `Option` on the
                // shared `DecisionCardRef` is only there for dangling decision edges, never for tasks.
                name: t.task.name.unwrap_or_default(),
                r#ref: t.task.display_ref,
                status: t.status,
            })
            .collect(),
        created_at: Timestamp::parse_rfc3339(&row.created_at).unwrap_or_default().to_rfc3339_z(),
    }
}

/// Build the store's projection into `acc` (projects + activity).
fn collect_store(store: &Store, acc: &mut Acc, lang: &str) -> Result<(), CmdError> {
    use amenbo_core::store_engine;

    let read_model = store.read_model();
    let conn = read_model.conn();

    if acc.me_user_id.is_empty() {
        acc.me_user_id = ActorKind::Human.as_str().to_string();
    }

    let project_rows = store_engine::read::project_overview(conn, store.reach())?;
    for p in &project_rows {
        let dimensions: Vec<DimensionDto> = p
            .dimensions
            .iter()
            .map(|d| DimensionDto {
                id: d.id,
                name: d.name.clone(),
                notes: d.notes.clone(),
                role: d.role.clone(),
                ordered: d.ordered,
                values: d
                    .values
                    .iter()
                    .map(|v| DimensionValueDto {
                        id: v.id,
                        name: v.name.clone(),
                        start_on: v.start_on.map(|d| d.to_string()),
                        end_on: v.end_on.map(|d| d.to_string()),
                    })
                    .collect(),
            })
            .collect();
        acc.projects.push(ProjectDto {
            id: p.id,
            name: p.name.clone(),
            color: p.color.clone().unwrap_or_else(|| "#9aa7b2".to_string()),
            view: p.default_view.clone(),
            open_count: p.open_count,
            dimensions,
        });
    }

    let items = amenbo_core::activity::page(
        &amenbo_core::activity::Ledger::open(&store.paths.activity_file),
        conn,
        &amenbo_core::activity::Filter { limit: Some(100), ..Default::default() },
    )?;
    acc.activity.extend(items.into_iter().map(|it| activity_dto(it, lang, &store.config)));
    Ok(())
}

/// The store's activity (the latest `limit` items, newest first), shaped into DTOs. This is what
/// `activity_page` reaches back with, over the same path as `collect_store`'s default read of 100
/// (the file ledger merged with `task_comment`).
fn store_activity_dtos(store: &Store, limit: usize, lang: &str) -> Result<Vec<ActivityItemDto>, CmdError> {
    let read_model = store.read_model();
    let items = amenbo_core::activity::page(
        &amenbo_core::activity::Ledger::open(&store.paths.activity_file),
        read_model.conn(),
        &amenbo_core::activity::Filter { limit: Some(limit), ..Default::default() },
    )?;
    Ok(items.into_iter().map(|it| activity_dto(it, lang, &store.config)).collect())
}

/// Has the first snapshot after process start already bypassed the update-check cache and asked
/// upstream fresh? Only the first one wins the `false→true` flip; every tick after it goes through
/// the 24h cache (we do not talk to the network on every tick).
static UPDATE_CHECK_REFRESHED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Assemble the read data for every screen into one sheet. **Directory-independent**: what it opens
/// is the single store in this machine's app-data. If there is no store yet it returns an **empty
/// snapshot** (we never quietly create a default empty store — the empty state is explicit).
fn build_snapshot() -> Result<Snapshot, CmdError> {
    use std::sync::atomic::Ordering;
    let _perf = amenbo_core::perf::Timer::start("build_snapshot");
    let mut acc = Acc::default();

    let paths = amenbo_core::config::Paths::resolve().ok();
    let config = paths
        .as_ref()
        .map(|p| amenbo_core::config::Config::load(&p.config_file))
        .unwrap_or_default();
    let language = config.language.clone();
    let onboarded = config.onboarded;
    let lang = lang_code(&language);

    let first_snapshot = UPDATE_CHECK_REFRESHED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok();
    let upstream = if first_snapshot {
        amenbo_core::update_check::check_fresh(config.update_check)
    } else {
        amenbo_core::update_check::check(config.update_check)
    };

    let mut startup_health = StartupHealthDto::default();
    let mut version_status = VersionStatusDto::default();
    with_store_read(|store| {
        startup_health.absorb(store);
        version_status.absorb(store, upstream.as_ref());
        collect_store(store, &mut acc, lang)
    })?;

    Ok(Snapshot {
        me_user_id: acc.me_user_id,
        language,
        onboarded,
        roster: config
            .roster()
            .into_iter()
            .map(|(kind, name)| ActorDto {
                user_id: kind.as_str().to_string(),
                name,
                kind: kind.as_str(),
                avatar: config.avatar_for(kind),
            })
            .collect(),
        projects: acc.projects,
        activity: acc.activity,
        startup_health,
        version_status,
        perf_log: config.perf_log.map(|p| p.as_config_str().to_string()),
        update_check: config.update_check,
    })
}

/// Return the read data for every screen in one sheet (the seam in adapter.ts receives it).
#[tauri::command]
pub fn snapshot() -> Result<Snapshot, CmdError> {
    let snap = build_snapshot()?;
    log::info!(
        "snapshot: me={} projects={} activity={}",
        snap.me_user_id,
        snap.projects.len(),
        snap.activity.len(),
    );
    Ok(snap)
}

/// What a write command returns: the ids it touched and the scopes to invalidate, and nothing else
/// (the output contract for writes — affected ids only, never bodies or secrets). The GUI takes this
/// and invalidates exactly those query keys (there is no optimistic update). `scopes` are the
/// coarse-grained key namespaces: "tasks" (lists and boards) and "decisions" (decision records).
/// **There is deliberately no escape hatch that invalidates everything** — a write command knows
/// what it touched, and any coarse hammer within reach would get used. The surfaces that swap the
/// data wholesale (a full restore) return no ack at all; the front end explicitly refetches every
/// query (`runRestore`).
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WriteAck {
    /// Task ids that were touched (the single-record query `["task", id]` gets invalidated).
    tasks: Vec<i64>,
    /// Decision ids that were touched (the single-record query `["decision", id]` gets invalidated).
    decisions: Vec<i64>,
    /// Coarse-grained scopes to invalidate ("tasks"/"decisions"). Empty means there is no query to
    /// invalidate — as with a roster write, where refetching the snapshot in `loadSnapshot` is
    /// enough to show the change.
    scopes: Vec<&'static str>,
}

impl WriteAck {
    fn new(scopes: &[&'static str]) -> WriteAck {
        WriteAck { scopes: scopes.to_vec(), ..Default::default() }
    }
    fn task(mut self, id: i64) -> WriteAck {
        self.tasks.push(id);
        self
    }
    fn decision(mut self, id: i64) -> WriteAck {
        self.decisions.push(id);
        self
    }
}

/// The store file being watched (there is only one store). `None` when the path cannot be resolved.
/// Kept in one place so the watcher (change detection) and `store_signature` (deduping our own
/// writes) look at the same file.
fn store_file() -> Option<std::path::PathBuf> {
    amenbo_core::config::Paths::resolve().ok().map(|p| p.store_file)
}

/// A file's identity (mtime, size). **Not for detecting changes** — its only job is to tell whether
/// the file itself was swapped out from under us (see [`store_signature_string`] below).
fn file_identity(p: &std::path::Path) -> (u128, u64) {
    let Ok(m) = std::fs::metadata(p) else { return (0, 0) };
    let mtime = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    (mtime, m.len())
}

/// The **read connection we keep open** for change detection, plus the identity of the file it is
/// reading. `PRAGMA data_version` is a value SQLite guarantees will answer "**has another connection
/// committed?**" — but **it is only comparable against values read from the same connection** (SQLite
/// states outright that values from another connection cannot be compared). So we hold exactly one
/// connection for the life of the process: the watcher (which wakes the UI) and `store_signature`
/// (which dedupes our own writes) look at the **same connection**, so their verdicts on "did it
/// change?" can never disagree.
struct Watch {
    store: Option<Store>,
    path: std::path::PathBuf,
    file: (u128, u64),
}

fn watch() -> &'static std::sync::Mutex<Watch> {
    static WATCH: std::sync::OnceLock<std::sync::Mutex<Watch>> = std::sync::OnceLock::new();
    WATCH.get_or_init(|| {
        std::sync::Mutex::new(Watch { store: None, path: std::path::PathBuf::new(), file: (0, 0) })
    })
}

/// The store's change signature, on two legs: **`PRAGMA data_version` plus the identity of the main
/// file**. `data_version` is the value SQLite guarantees will tell you that **some connection has
/// committed**; in WAL mode an external writer's commit lands only in `-wal` and never moves the
/// main file's mtime, so guessing from mtime/size would miss the arrival entirely (system events
/// number themselves in the same transaction — a DB commit — so there is no need to stat the ledger
/// separately). The GUI's own writes commit on another connection and move this value too, and
/// filtering those out is the front end's job: after a write, `loadSnapshot` records this signature,
/// and when `store-changed` arrives with a matching one, it does not refetch. We watch the file's
/// identity alongside it because `fold`, `restore` and migration **swap the file out wholesale** —
/// the connection we are holding would go on reading a dead inode where nobody will ever commit
/// again, so when mtime/size moves we reopen it. That degrades cleanly into "the whole file changed
/// → gap → refetch everything".
fn store_signature_string() -> String {
    let Some(path) = store_file() else { return String::new() };
    let Ok(mut w) = watch().lock() else { return String::new() };

    let file = file_identity(&path);
    if w.store.is_none() || w.file != file || w.path != path {
        w.store = amenbo_core::config::Paths::resolve().ok().and_then(|p| Store::open_read_at(p).ok());
        w.file = file;
        w.path = path;
    }
    let version = w
        .store
        .as_ref()
        .and_then(|s| amenbo_core::store_engine::read::data_version(s.read_model().conn()).ok())
        .unwrap_or(0);
    format!("{}:{}:{}", file.0, file.1, version)
}

/// The signature (`store_signature_string`) the GUI uses to filter out the `store-changed` events
/// its own writes caused.
#[tauri::command]
pub fn store_signature() -> String {
    store_signature_string()
}

/// For the "location" line under Settings > Data. Returns the real, OS-independent path (the
/// app-data root).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct StoreLocationsDto {
    /// Absolute path of the app-data root (the parent directory of the single `store.sqlite`).
    root: String,
}

/// Return the real path of the app-data root, for the "location" line under Settings > Data.
#[tauri::command]
pub fn store_locations() -> StoreLocationsDto {
    StoreLocationsDto {
        root: amenbo_core::config::Paths::data_root()
            .to_string_lossy()
            .into_owned(),
    }
}

/// The paged read behind history mode. Skips `offset` items newest-first and returns the next
/// `limit`. The default `snapshot` stays light by carrying only the latest 100; when the GUI's
/// virtual scroller reaches back past those, it calls this for its scroll window and nothing more.
#[tauri::command]
pub fn activity_page(offset: usize, limit: usize) -> Result<Vec<ActivityItemDto>, CmdError> {
    let language = amenbo_core::config::Paths::resolve()
        .ok()
        .and_then(|p| amenbo_core::config::Config::load(&p.config_file).language);
    let lang = lang_code(&language);

    let need = offset.saturating_add(limit);
    let mut all: Vec<ActivityItemDto> = Vec::new();
    with_store_read(|store| {
        all.extend(store_activity_dtos(store, need, lang)?);
        Ok(())
    })?;
    all.sort_by(|a, b| b.at.cmp(&a.at));
    Ok(all.into_iter().skip(offset).take(limit).collect())
}

/// One row of the change feed. **Which row of which table changed, and how** — that is all; no
/// values, no bodies (the caller refetches from the source of truth).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRowDto {
    /// Dataset the changed row belongs to (`task`, `task_comment`, `decision`, ...).
    dataset: String,
    /// Id of the changed row (the conversational number itself).
    row_id: i64,
    /// `insert` / `update` / `delete`.
    op: String,
}

/// The changes after a cursor. The GUI folds them into scopes and invalidates **only what moved**.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangesDto {
    /// Oldest first. Empty when `expired`.
    rows: Vec<ChangeRowDto>,
    /// The cursor to pass next time: the id of the last row if there were any, otherwise the cursor
    /// that came in. When `expired`, it is **the feed's current head** — after a full refetch
    /// (reconcile), the caller can resume incremental reads from there (changes that landed during
    /// the refetch stay ahead of the cursor, so none are lost).
    cursor: i64,
    /// The page was cut short by `limit` — there is more. The caller calls again with the returned
    /// cursor.
    more: bool,
    /// **The cursor has expired.** Truncation discarded rows the caller had not read, and the feed
    /// can no longer say what changed. Reading the empty response as "nothing changed" would freeze
    /// the screen on stale data, so the caller sees this and falls back to refetching from the source
    /// of truth.
    expired: bool,
}

/// The default page size. Bounds a single incremental read (if the feed piled up while the user was
/// away, the caller watches `more` and pages through it).
const CHANGES_PAGE: usize = 500;

/// Read the change feed from a cursor onward. Returns the rows after `cursor`, oldest first (just a
/// forward read of the feed table with `id > ?`). Passing 0 means "from the oldest the feed still
/// has", and `expired` is raised if that has been truncated away. **Not the same thing as activity's
/// `cur1_`/`cur2_` cursors** — those are a three-part cursor merging the file ledger and
/// `task_comment` in time order, whereas this is a single table with monotonic ids, where a plain
/// `id > ?` is all it takes.
#[tauri::command]
pub fn changes_since(cursor: i64, limit: Option<usize>) -> Result<ChangesDto, CmdError> {
    use amenbo_core::store_engine::read::{self, FeedSlice};

    let _perf = amenbo_core::perf::Timer::start("changes_since");
    let store = open_store_read()?;
    let conn = store.read_model().conn();
    let limit = limit.unwrap_or(CHANGES_PAGE);
    match read::changes_since(conn, cursor, limit as i64)? {
        FeedSlice::Changes { rows, more } => Ok(ChangesDto {
            cursor: rows.last().map(|r| r.id).unwrap_or(cursor),
            rows: rows
                .into_iter()
                .map(|r| ChangeRowDto { dataset: r.dataset, row_id: r.row_id, op: r.op })
                .collect(),
            more,
            expired: false,
        }),
        FeedSlice::Gap => Ok(ChangesDto {
            rows: Vec::new(),
            cursor: read::change_feed_head(conn)?,
            more: false,
            expired: true,
        }),
    }
}

/// The feed's current head id. The starting cursor for a caller that has just read the store from
/// the source of truth and now wants to wait for "only the changes after this point". 0 on an empty
/// feed. **Take it before you read, not after** — take it first and then refetch, and any change
/// that lands in between stays ahead of the cursor (you see it twice, but you never lose it). Do it
/// the other way round and changes falling in the gap between the refetch and the read are lost for
/// good.
#[tauri::command]
pub fn change_cursor() -> Result<i64, CmdError> {
    let store = open_store_read()?;
    Ok(amenbo_core::store_engine::read::change_feed_head(store.read_model().conn())?)
}

/// Shape one row of the persistent read-model into a GUI DTO. Wording an `event` (system events
/// only) and the relative-time label happen here, so core stays free of rendering and i18n.
fn activity_dto(it: amenbo_core::activity::Item, lang: &str, config: &amenbo_core::config::Config) -> ActivityItemDto {
    let ago = ago_label(&it.at);
    let title = if it.title.is_empty() { nameless_title(lang).to_string() } else { it.title };
    let event = it.event.as_ref().map(|ev| render_event(ev, &title, lang));
    ActivityItemDto {
        id: it.id,
        at: it.at.to_rfc3339_z(),
        ago,
        kind: it.kind.as_str().to_string(),
        author: facet_actor(config, it.author_kind),
        target: ActivityTargetDto {
            target_type: it.target_type.as_str().to_string(),
            id: it.target_id,
            title,
            live: it.target_live,
        },
        event,
        text: it.text,
        edited_ago: it.edited_at.as_ref().map(ago_label),
    }
}

/// One task's activity (comments included), newest first, for the comment list in the detail pane.
/// The latest-100 window in `snapshot` is not enough: as the task count grows, an older task's
/// comments fall outside the window and go missing (the 💬 count stays right, since it comes from
/// num_comments, while the list below it goes empty). So this bypasses the window and queries the
/// persistent read-model directly, per task.
#[tauri::command]
pub fn task_activity(task_id: i64, limit: Option<usize>) -> Result<Vec<ActivityItemDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("task_activity");
    let language = amenbo_core::config::Paths::resolve()
        .ok()
        .and_then(|p| amenbo_core::config::Config::load(&p.config_file).language);
    let lang = lang_code(&language);

    let collect_from = |store: &Store| -> Result<Vec<ActivityItemDto>, CmdError> {
        let read_model = store.read_model();
        let items = amenbo_core::activity::page(
            &amenbo_core::activity::Ledger::open(&store.paths.activity_file),
            read_model.conn(),
            &amenbo_core::activity::Filter { task_id: Some(task_id), limit, ..Default::default() },
        )?;
        Ok(items.into_iter().map(|it| activity_dto(it, lang, &store.config)).collect())
    };

    let found = find_in_store(|store| {
        let items = collect_from(store)?;
        Ok((!items.is_empty()).then_some(items))
    })?;
    Ok(found.unwrap_or_default())
}

/// The **paged read** behind the task list. `store_engine::read::list_task_ids` gives back **one
/// page of task ids** through an indexed `WHERE`/`ORDER BY`/`LIMIT`/`OFFSET`, and only those ids are
/// hydrated, per task, into `TaskCardDto` (no all-rows blob comes back — memory stays bounded). The
/// `filter` grammar is exactly that of `task list --filter` (they share `query::Filter`). The `Store`
/// opened here queries the persistent read-model directly, projects just the window, and is released
/// on return (it adds nothing resident). A read that goes straight to the engine **passes its reach
/// explicitly** (the GUI is the human's place — the whole machine): leave it to the default, and on
/// the day another surface appears, the reach could quietly fall back to All with nobody noticing.
#[tauri::command]
pub fn task_page(
    project_id: Option<i64>,
    filter: Option<String>,
    sort: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<TaskPageDto, CmdError> {
    use amenbo_core::store_engine::{self, TaskQuery};

    let _perf = amenbo_core::perf::Timer::start("task_page");
    let store = open_store_read()?;
    let today = amenbo_core::time::today();

    let filter_expr = filter.unwrap_or_default();
    let mut parsed = query::Filter::parse(&filter_expr, today)?;
    let sort = sort.unwrap_or_else(|| "order".to_string());

    let read_model = store.read_model();

    parsed.resolve(read_model.conn())?;

    let page = store_engine::list_task_ids(
        read_model.conn(),
        &TaskQuery {
            reach: store.reach(),
            project_id,
            filter: &parsed,
            sort: &sort,
            today,
            limit,
            offset,
        },
    )
    ?;

    let conn = read_model.conn();
    let mut tasks: Vec<TaskCardDto> = Vec::with_capacity(page.ids.len());
    for &id in &page.ids {
        if let Some(row) = amenbo_core::store_engine::read::task_card_row(conn, id)? {
            tasks.push(task_card_from_row(&store, row));
        }
    }

    Ok(TaskPageDto { tasks, total_matched: page.total_matched, offset: offset.unwrap_or(0), limit })
}

/// Hydrate the given ids into `TaskCardDto` (input order preserved). `task_page` returns "the ids on
/// this page" and `tasks_by_ids` returns "any set of ids" — a pair of reads that lets the front end
/// get by without ever holding an array of every task. It is used (1) to fetch a single task for the
/// detail pane (getTask), and (2) to hydrate the inbox's comment tasks (the ids
/// `mailbox_comment_tasks` returns) so they can be unioned into the view's set. The cost is bounded
/// by the number of ids, not by the size of the store. Ids that do not exist are dropped silently
/// (the caller treats such a task as deleted or out of reach).
#[tauri::command]
pub fn tasks_by_ids(ids: Vec<i64>) -> Result<Vec<TaskCardDto>, CmdError> {
    use std::collections::HashMap;

    let _perf = amenbo_core::perf::Timer::start("tasks_by_ids");
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut found: HashMap<i64, TaskCardDto> = HashMap::new();

    let scan = |store: &Store| -> Result<(), CmdError> {
        let pending: Vec<i64> = ids.iter().copied().filter(|id| !found.contains_key(id)).collect();
        if pending.is_empty() {
            return Ok(());
        }
        let read_model = store.read_model();
        let conn = read_model.conn();
        let present = amenbo_core::store_engine::read::present_task_ids(conn, &pending)?;
        for id in present {
            if let Some(row) = amenbo_core::store_engine::read::task_card_row(conn, id)? {
                found.insert(id, task_card_from_row(store, row));
            }
        }
        Ok(())
    };

    with_store_read(scan)?;

    Ok(ids.iter().filter_map(|id| found.remove(id)).collect())
}

/// What `decision_page` returns: the decisions on the page, plus the total count before paging.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DecisionPageDto {
    decisions: Vec<DecisionDto>,
    total_matched: usize,
}

/// Return a project's decision records (the decisions tab fetches just its own window). Status
/// filtering, search and sorting are layered on in the client, since the count is bounded. Omitting
/// `limit` means everything (from `offset` on).
#[tauri::command]
pub fn decision_page(
    project_id: i64,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<DecisionPageDto, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("decision_page");
    let store = open_store_read()?;
    let read_model = store.read_model();
    let conn = read_model.conn();
    let page = amenbo_core::store_engine::decision_page(
        conn,
        store.reach(),
        project_id,
        limit,
        offset.unwrap_or(0),
    )?;
    let mut decisions: Vec<DecisionDto> = Vec::with_capacity(page.ids.len());
    for id in &page.ids {
        if let Some(row) = amenbo_core::store_engine::read::decision_card_row(conn, *id)? {
            decisions.push(decision_card_from_row(row));
        }
    }
    Ok(DecisionPageDto { decisions, total_matched: page.total_matched })
}

/// Hydrate the given ids into `DecisionDto` (input order preserved). The decision twin of
/// `tasks_by_ids`; the decision detail pane uses it to fetch a single decision. Ids that do not
/// exist are dropped silently.
#[tauri::command]
pub fn decisions_by_ids(ids: Vec<i64>) -> Result<Vec<DecisionDto>, CmdError> {
    use std::collections::HashMap;

    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut found: HashMap<i64, DecisionDto> = HashMap::new();
    let scan = |store: &Store| -> Result<(), CmdError> {
        let pending: Vec<i64> = ids.iter().copied().filter(|id| !found.contains_key(id)).collect();
        if pending.is_empty() {
            return Ok(());
        }
        let read_model = store.read_model();
        let conn = read_model.conn();
        let present = amenbo_core::store_engine::read::present_decision_ids(conn, &pending)?;
        for id in present {
            if let Some(row) = amenbo_core::store_engine::read::decision_card_row(conn, id)? {
                found.insert(id, decision_card_from_row(row));
            }
        }
        Ok(())
    };

    with_store_read(scan)?;

    Ok(ids.iter().filter_map(|id| found.remove(id)).collect())
}

/// What a reference in a body resolves to (`kind` — task or decision — and the entity's id). The GUI
/// branches on it to decide which detail pane a body link opens.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct RefTargetDto {
    #[ts(type = "\"task\" | \"decision\"")]
    kind: String,
    /// The entity's primary key (an integer for both tasks and decisions). `kind` says which table
    /// it points into.
    #[ts(type = "number")]
    id: i64,
}

/// Resolve one conversational reference from a body (`#NNN`, `T-NN`, `D-NN`, ...) to the id of the
/// task or decision it names, so the GUI can turn it into a link. It is the GUI's way in to core's
/// `resolve_any_ref`, so the grammar is never defined twice. Numbers are **globally unique on the
/// machine**, so no project context is needed: `#NNN` names exactly one entity.
/// Ambiguous or unknown gives `Ok(None)` and the UI quietly no-ops (a false positive in link
/// detection must not raise an error dialog).
#[tauri::command]
pub fn resolve_ref(input: String) -> Result<Option<RefTargetDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("resolve_ref");
    let mut hit: Option<RefTargetDto> = None;
    with_store_read(|store| {
        if hit.is_some() {
            return Ok(());
        }
        if let Ok(r) = store.resolve_any_ref(input.trim()) {
            hit = Some(match r {
                amenbo_core::ops::Ref::Task(id) => RefTargetDto { kind: "task".into(), id },
                amenbo_core::ops::Ref::Decision(id) => {
                    RefTargetDto { kind: "decision".into(), id }
                }
            });
        }
        Ok(())
    })?;
    Ok(hit)
}

/// Hand the GUI the `amenbo agent --json` spec (philosophy, every command with its flags and
/// examples, capabilities, workflows). The source of truth is [`amenbo_core::agent`], the same one
/// the CLI's `amenbo agent` prints.
/// The "commands" screen and ⌘K's search over real data consume it (display only — the GUI never
/// runs the CLI). The CLI side, which the AI reads, stays in English. The GUI passes `locale`
/// (config.language) and only the prose is swapped for a translation just before display; the
/// English source of truth is untouched ([`amenbo_core::agent::build_localized`]). Unspecified means
/// en.
#[tauri::command]
pub fn agent_spec(locale: Option<String>) -> serde_json::Value {
    amenbo_core::agent::build_localized(locale.as_deref().unwrap_or("en"))
}

use amenbo_core::read_receipts::ReadReceipts;

/// Return this machine's read state (per-task last_seen plus the mailbox-wide last_seen). Read at
/// GUI startup and when the inbox renders.
#[tauri::command]
pub fn read_receipts() -> Result<ReadReceipts, CmdError> {
    Ok(open_store()?.read_receipts()?)
}

/// Mark a task as seen (last viewed = now). Called when the detail pane opens. Returns the whole
/// updated state.
#[tauri::command]
pub fn mark_task_seen(task_id: i64) -> Result<ReadReceipts, CmdError> {
    let store = open_store()?;
    store.mark_task_seen(task_id, &Timestamp::now().to_rfc3339_z())?;
    Ok(store.read_receipts()?)
}

/// Mark the whole inbox as seen (advance the reference time for badge freshness to now). Called when
/// the inbox view opens.
#[tauri::command]
pub fn mark_mailbox_seen() -> Result<ReadReceipts, CmdError> {
    let store = open_store()?;
    store.mark_mailbox_seen(&Timestamp::now().to_rfc3339_z())?;
    Ok(store.read_receipts()?)
}

/// Return the inbox items archived on this machine (a list of task_ids). The inbox reads it while
/// rendering and leaves those items out of the list.
#[tauri::command]
pub fn inbox_archived() -> Result<Vec<i64>, CmdError> {
    Ok(open_store()?.inbox_archive_ids()?)
}

/// Archive an inbox item (drop it from the list). Returns the full id list afterwards.
#[tauri::command]
pub fn inbox_archive(task_id: i64) -> Result<Vec<i64>, CmdError> {
    let store = open_store()?;
    store.inbox_archive_add(task_id)?;
    Ok(store.inbox_archive_ids()?)
}

/// Unarchive an inbox item (put it back in the inbox). Returns the full id list afterwards.
#[tauri::command]
pub fn inbox_unarchive(task_id: i64) -> Result<Vec<i64>, CmdError> {
    let store = open_store()?;
    store.inbox_archive_remove(task_id)?;
    Ok(store.inbox_archive_ids()?)
}

/// GC for device state (read receipts and archive). Both accumulate a task id on every view and
/// every dismissal, including ids of tasks that have since been deleted. So we build **the complete
/// set of live task ids** and DELETE any row in either whose id is not in it. Writes only when
/// something actually changed. Does nothing if the store could not be opened — otherwise an empty
/// set would wipe everything. Meant to be called once, at startup. Failure is not fatal (the caller
/// only logs it).
pub fn gc_device_state() -> Result<(), CmdError> {
    use std::collections::HashSet;
    let mut live: HashSet<i64> = HashSet::new();
    let mut opened = false;
    let scan = |store: &Store| -> Result<(), CmdError> {
        opened = true;
        let read_model = store.read_model();
        for id in amenbo_core::store_engine::read::live_task_ids(read_model.conn())? {
            live.insert(id);
        }
        Ok(())
    };

    with_store_read(scan)?;

    if !opened {
        return Ok(());
    }

    let store = open_store()?;
    if store.retain_live_read_receipts(|id| live.contains(&id))? {
        log::info!("gc_device_state: pruned stale read_receipts entries");
    }
    if store.retain_live_inbox_archive(|id| live.contains(&id))? {
        log::info!("gc_device_state: pruned stale inbox_archive entries");
    }
    Ok(())
}

/// The comment slot of the inbox, independent of read state: of the open tasks assigned to me,
/// return every one that has at least one **comment addressed to me** (something my AI facet said),
/// as `(task_id, unread)`. The GUI unions these into the inbox view. Membership is decided by **the
/// existence of a comment** — marking it read does not remove it; only archiving does — and each
/// task's comments are pulled straight from the read-model over indexed SQL (the single-pass SQL in
/// `store_engine::read::mailbox_comment_tasks`). "Is it me?" is decided on the facet alone (the human
/// facet token `"human"`), and what I said myself, as the human facet, does not count as received.
/// `unread` is an unread flag relative to the per-task last_seen
/// (`ReadReceipts::has_unread_comment`); it is purely for display (the unread dot) and has no say in
/// membership.
#[tauri::command]
pub fn mailbox_comment_tasks() -> Result<Vec<(i64, bool)>, CmdError> {
    let rr = open_store_read()?.read_receipts()?;
    let mut out: Vec<(i64, bool)> = Vec::new();

    let scan = |store: &Store| -> Result<(), CmdError> {
        let me = ActorKind::Human.as_str();
        let read_model = store.read_model();
        for mt in
            amenbo_core::store_engine::read::mailbox_comment_tasks(read_model.conn(), store.reach())?
        {
            let unread = rr.has_unread_comment(
                mt.task_id,
                me,
                mt.comments.iter().map(|(u, h, a)| (u.as_str(), *h, a.as_str())),
            );
            out.push((mt.task_id, unread));
        }
        Ok(())
    };

    with_store_read(scan)?;
    Ok(out)
}

/// Work out, per task_id, when the activity that put the item in the inbox happened (triggeredAt).
/// Two things put an item there: the latest comment from someone other than me (the human facet),
/// and the latest `task.assigned` naming me (a fresh assignment); the later of the two is what the
/// inbox displays and sorts on (matching is on the facet alone). What comes back is the timestamp
/// (RFC3339 UTC) of the most recent such activity, with tasks that have none left out.
/// `amenbo_core::activity::mailbox_triggered_at` folds every inbox id in one go (one pass over the
/// ledger plus one comment query).
#[tauri::command]
pub fn mailbox_triggered_at(task_ids: Vec<i64>) -> Result<Vec<(i64, String)>, CmdError> {
    if task_ids.is_empty() {
        return Ok(Vec::new());
    }
    let want = task_ids.clone();
    let mut out: Vec<(i64, String)> = Vec::new();

    let scan = |store: &Store| -> Result<(), CmdError> {
        let read_model = store.read_model();
        out.extend(amenbo_core::activity::mailbox_triggered_at(
            &store.paths.activity_file,
            read_model.conn(),
            &want,
        )?);
        Ok(())
    };

    with_store_read(scan)?;
    Ok(out)
}

/// Tell the front end, via `store-changed`, that the store file (`store.sqlite` and its WAL sidecar
/// `-wal`) has moved — this is how writes from another process (the AI, the CLI, another session)
/// reach the GUI. **Waking up is left to the kernel**: the OS-specific watching and coalescing live
/// in [`crate::store_watch`], and all that happens here is "once woken, check whether it really
/// changed, then emit", with `store_signature_string` answering from `PRAGMA data_version` plus the
/// file's identity (file watching also fires on things that mean nothing to us, such as SHM updates
/// from a read, and this gate drops those spurious emits). **There is no payload — it is a wake-up
/// signal saying "something changed", nothing more**: what changed is something the front end learns
/// by reading the **change feed** (`changes_since`, written at the same seam as the write
/// transaction) from its cursor onward, and it refetches only the queries touching those scopes (a
/// watcher looking at a file cannot say which dataset moved). It takes no lock on the store itself
/// (one `PRAGMA` on a read connection contends with no writer), and **this watcher is for rendering
/// only**. The GUI's own writes move the signature too, so the front end holds on to
/// `store_signature` and filters those out.
pub fn watch_store(app: tauri::AppHandle) {
    use tauri::Emitter;

    let mut last = store_signature_string();
    let mut emit_if_changed = move || {
        let cur = store_signature_string();
        if cur == last {
            return;
        }
        last = cur;
        let _ = app.emit("store-changed", ());
    };

    let dir = store_file().and_then(|f| f.parent().map(std::path::Path::to_path_buf));
    crate::store_watch::run(dir.as_deref(), &mut emit_if_changed);
}

/// Create a task (directly under a project, or in the inbox; stamped created_by=human).
/// Classification is added afterwards, on the dimension axes.
#[tauri::command]
pub fn task_add(
    project_id: Option<i64>,
    title: String,
    notes: Option<String>,
) -> Result<WriteAck, CmdError> {
    let id = with_store_mut(|store| {
        let t = store.add_task(amenbo_core::ops::task::NewTask {
            title,
            project_id,
            due_on: None,
            start_on: None,
            priority: None,
            notes: notes.unwrap_or_default(),
            created_by_kind: Some(ActorKind::Human),
        })?;
        emit(store, t.id, amenbo_core::activity_log::event::task_created(&t.title));
        Ok(t.id)
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// Complete or reopen a task (done ⟺ completed). Idempotent — already in that state means no-op.
#[tauri::command]
pub fn task_done(id: i64, done: bool) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let cur = store.task(id)?;
        let already = cur.as_ref().is_some_and(|t| t.completed());
        let old = cur.map(|t| t.status).unwrap_or_default();
        if already != done {
            let t = store.set_task_completed(id, done)?;
            emit(store, id, amenbo_core::activity_log::event::task_status_changed(old.as_str(), t.status.as_str()));
        }
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// Set the status explicitly (done keeps completed in step). Setting the same status again is a
/// no-op, with one exception: `in_progress → in_progress` is never waved through. It goes down to
/// `set_status` so the reservation CAS is not defused, and a second session trying to start the same
/// task is turned away with `AlreadyReserved` (same shape as the CLI; the collision surfaces as a
/// CmdError and reaches the front end's toast).
#[tauri::command]
pub fn task_status(id: i64, status: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let new_status = TaskStatus::parse(&status)
            .ok_or_else(|| format!("status '{status}' は不正です（todo / in_progress / done / blocked）"))?;
        let current = store.task(id)?.map(|t| t.status);
        if current != Some(new_status) || new_status == TaskStatus::InProgress {
            let old = current.unwrap_or_default();
            store.set_task_status(id, new_status)?;
            emit(store, id, amenbo_core::activity_log::event::task_status_changed(old.as_str(), new_status.as_str()));
        }
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// Delete a task for good (core's rules decide how the subtree is swept). Same shape as the CLI's
/// `task delete`. The GUI's actor is always human, so the guardrail aimed at the AI — the limit on
/// deleting human-created tasks — never applies. The confirmation dialog is the UI's job.
#[tauri::command]
pub fn task_delete(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.delete_task(id, ActorKind::Human)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// Add a comment (facet = human).
#[tauri::command]
pub fn comment_add(task_id: i64, text: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.add_task_comment(task_id, ActorKind::Human, &text)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(task_id))
}

/// Take back a comment posted by mistake — delete it for good (same shape as the CLI's
/// `comment rm`). Any attachments on it go with it (core's delete op sweeps them). The confirmation
/// dialog is the UI's job.
#[tauri::command]
pub fn comment_remove(id: i64, task_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.remove_task_comment(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(task_id))
}

/// Rewrite a comment's body in place (same shape as the CLI's `comment edit`). It is not a repost,
/// so the id, the position in the timeline and the attachments all stay. Overwriting alone needs no
/// confirmation dialog (taking a comment back does).
#[tauri::command]
pub fn comment_edit(id: i64, task_id: i64, text: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.edit_task_comment(id, &text)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(task_id))
}

/// One permanent comment on a decision record, for the GUI. Task comments ride in the per-task
/// `task_activity` (kind=comment), but decisions have no activity path, so they get a read DTO of
/// their own. The author's facet is resolved to a display name from config, and the relative time
/// `ago` is worded here (the front end does nothing but render).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DecisionCommentDto {
    #[ts(type = "number")]
    id: i64,
    ago: String,
    author: ActorDto,
    text: String,
    /// Relative time of a later edit of the body. Absent when it was never edited (same meaning and
    /// same treatment as [`ActivityItemDto::edited_ago`] on task comments).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    edited_ago: Option<String>,
}

/// Shape one read-model row (`CommentRow`) into a decision comment DTO.
fn decision_comment_dto_from_row(
    row: amenbo_core::store_engine::read::CommentRow,
    config: &amenbo_core::config::Config,
) -> DecisionCommentDto {
    let ago = Timestamp::parse_rfc3339(&row.created_at).map(|ts| ago_label(&ts)).unwrap_or_default();
    let author_kind = row.author_kind.as_deref().and_then(ActorKind::parse);
    let edited_ago = row
        .edited_at
        .as_deref()
        .map(|t| Timestamp::parse_rfc3339(t).map(|ts| ago_label(&ts)).unwrap_or_default());
    DecisionCommentDto {
        id: row.id,
        ago,
        author: facet_actor(config, author_kind),
        text: row.text,
        edited_ago,
    }
}

/// One decision's live comments, oldest first, for the thread in the decision detail pane. Like
/// `task_activity`, it queries the read-model directly, per decision, bypassing the window. Empty if
/// the decision is not found.
#[tauri::command]
pub fn decision_comments(decision_id: i64) -> Result<Vec<DecisionCommentDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("decision_comments");
    let found = find_in_store(|store| {
        let read_model = store.read_model();
        let conn = read_model.conn();
        if amenbo_core::store_engine::read::decision_title(conn, decision_id)?.is_none() {
            return Ok(None);
        }
        let rows = amenbo_core::store_engine::read::decision_comment_list(conn, decision_id)?;
        let dtos = rows
            .into_iter()
            .map(|r| decision_comment_dto_from_row(r, &store.config))
            .collect::<Vec<_>>();
        Ok(Some(dtos))
    })?;
    Ok(found.unwrap_or_default())
}

/// Add a comment to a decision record (facet = human). The decision twin of the task's
/// [`comment_add`], writing to the dedicated `decision_comment` table. The reason comment attached
/// when accepting or rejecting is thin sugar over the same path — the front end composes it and adds
/// one comment here.
#[tauri::command]
pub fn decision_comment_add(decision_id: i64, text: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.add_decision_comment(decision_id, ActorKind::Human, &text)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(decision_id))
}

/// Take back a decision comment — delete it for good (the decision twin of [`comment_remove`]).
#[tauri::command]
pub fn decision_comment_remove(id: i64, decision_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.remove_decision_comment(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(decision_id))
}

/// Rewrite a decision comment's body in place (the decision twin of [`comment_edit`]).
#[tauri::command]
pub fn decision_comment_edit(id: i64, decision_id: i64, text: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.edit_decision_comment(id, &text)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(decision_id))
}

/// One attachment on a task or decision record, for the GUI's viewer. The blob's bytes do not ride
/// along — only the metadata needed to branch on `mime` and to assemble the stream URL
/// (`blobHash`). `present` says whether the blob's bytes are on this machine (metadata survives
/// without them, and then it is false; there is no way to get them back, so the viewer cannot open
/// it). In `url` mode, `url` is set and the blob metadata is empty.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AttachmentDto {
    #[ts(type = "number")]
    id: i64,
    #[ts(type = "\"blob\" | \"url\"")]
    kind: String,
    blob_hash: Option<String>,
    filename: Option<String>,
    mime: Option<String>,
    size_bytes: Option<i64>,
    url: Option<String>,
    /// Are the blob's bytes on this machine? (Meaningless in `url` mode, where it is always false.)
    present: bool,
    #[ts(type = "\"human\" | \"ai\" | null")]
    created_by_kind: Option<String>,
}

/// The live attachments of a target (task/decision), in the order they were attached. A direct
/// read-model query, O(result).
#[tauri::command]
pub fn attachments_for(target_type: String, target_id: i64) -> Result<Vec<AttachmentDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("attachments_for");
    if amenbo_core::model::AttachmentTarget::parse(&target_type).is_none() {
        return Err(format!("添付先種別 '{target_type}' は不正です（task / decision / task_comment / decision_comment）").into());
    }
    let found = find_in_store(|store| {
        let read_model = store.read_model();
        let conn = read_model.conn();
        let rows = amenbo_core::store_engine::read::attachments_for_target(conn, &target_type, target_id)?;
        if rows.is_empty() {
            return Ok(None);
        }
        let blobs = store.blobs();
        let dtos = rows
            .into_iter()
            .map(|r| {
                let present = r.blob_hash.as_deref().is_some_and(|h| blobs.path(h).is_some());
                AttachmentDto {
                    id: r.id,
                    kind: r.kind,
                    blob_hash: r.blob_hash,
                    filename: r.filename,
                    mime: r.mime,
                    size_bytes: r.size_bytes,
                    url: r.url,
                    present,
                    created_by_kind: r.created_by_kind,
                }
            })
            .collect::<Vec<_>>();
        Ok(Some(dtos))
    })?;
    Ok(found.unwrap_or_default())
}

/// Ingest a file as a blob and attach it to a task or decision record. Same shape as the CLI's
/// `task/decision attach`: check the per-file size cap, ingest content-addressed, record the
/// metadata. The MIME type is guessed from the extension.
#[tauri::command]
pub fn attachment_add(
    target_type: String,
    target_id: i64,
    path: String,
) -> Result<WriteAck, CmdError> {
    let target = amenbo_core::model::AttachmentTarget::parse(&target_type)
        .ok_or_else(|| format!("添付先種別 '{target_type}' は不正です（task / decision / task_comment / decision_comment）"))?;
    with_store_mut(|store| {
        let src = std::path::Path::new(&path);
        let meta = std::fs::metadata(src).map_err(|e| format!("ファイル '{path}' を読めません: {e}"))?;
        if !meta.is_file() {
            return Err(format!("'{path}' は通常ファイルではありません").into());
        }
        let filename = src
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("attachment")
            .to_string();
        let mime = amenbo_core::blob::mime_from_filename(&filename);
        store.config.attachment_limits.check_per_file(mime, meta.len())?;
        let blob = store.blobs().ingest_path(src)?;
        store.attach_blob(
            target,
            target_id,
            &blob.hash,
            &filename,
            mime,
            blob.size_bytes as i64,
            ActorKind::Human,
        )?;
        Ok(())
    })?;
    let scope: &[&'static str] = if target == amenbo_core::model::AttachmentTarget::Decision {
        &["decisions"]
    } else {
        &["tasks"]
    };
    let ack = WriteAck::new(scope);
    Ok(if target == amenbo_core::model::AttachmentTarget::Decision {
        ack.decision(target_id)
    } else {
        ack.task(target_id)
    })
}

/// Ingest raw bytes as a blob and attach them. HTML5 drag-and-drop inside the webview cannot give us
/// an OS path (`dragDropEnabled:false` is the setting that lets card drag-and-drop on the board work
/// at all), so the front end reads the dropped File itself and hands the bytes over this path. Large
/// files are better off going through the file picker ([`attachment_add`] takes a path and ingests
/// as a stream). The body is the same as [`attachment_add`]: check the cap, ingest
/// content-addressed, record the metadata.
#[tauri::command]
pub fn attachment_add_bytes(
    target_type: String,
    target_id: i64,
    filename: String,
    bytes: Vec<u8>,
) -> Result<WriteAck, CmdError> {
    let target = amenbo_core::model::AttachmentTarget::parse(&target_type)
        .ok_or_else(|| format!("添付先種別 '{target_type}' は不正です（task / decision / task_comment / decision_comment）"))?;
    let filename = if filename.trim().is_empty() { "attachment".to_string() } else { filename };
    with_store_mut(|store| {
        let mime = amenbo_core::blob::mime_from_filename(&filename);
        store.config.attachment_limits.check_per_file(mime, bytes.len() as u64)?;
        let blob = store.blobs().ingest_bytes(&bytes)?;
        store.attach_blob(
            target,
            target_id,
            &blob.hash,
            &filename,
            mime,
            blob.size_bytes as i64,
            ActorKind::Human,
        )?;
        Ok(())
    })?;
    let scope: &[&'static str] = if target == amenbo_core::model::AttachmentTarget::Decision {
        &["decisions"]
    } else {
        &["tasks"]
    };
    let ack = WriteAck::new(scope);
    Ok(if target == amenbo_core::model::AttachmentTarget::Decision {
        ack.decision(target_id)
    } else {
        ack.task(target_id)
    })
}

/// Open a url-mode attachment in the OS's default application — "open externally". A url is the only
/// kind that has anywhere to be opened: a blob is a file, so it is written where the user asked for it
/// (`attachment_save`) rather than into a temp copy they cannot find again. The front end passes the
/// DTO's `url` through as it is (no need to resolve the id again). Even though the entry point
/// (`ops::attachment::add_url`) admits web schemes only, the scheme is checked again right before
/// opening: rows written before that validation existed still come through here, and an OS opener
/// will interpret whatever it is handed (`file:` is a local file; a leading `-` is an option to the
/// command).
#[tauri::command]
pub fn attachment_open(url: String) -> Result<(), CmdError> {
    let url = url.trim().to_string();
    if !amenbo_core::ops::attachment::is_web_url(&url) {
        return Err(format!("この URL は開けません（http/https/mailto のみ）: {url}").into());
    }
    os_open(&url).map_err(|e| format!("'{url}' を開けません: {e}").into())
}

/// Write a blob attachment to the path the user picked — "download", and the only way to take an
/// attachment out of the store as a file the user keeps (`export` writes the whole device, which is
/// data sovereignty, not "I want this one file"). The destination is the user's own choice — somewhere
/// they picked and can find again — so it is written with ordinary permissions. The front end passes
/// the DTO's `blobHash` through as it is, and has already resolved `dest` through the OS save dialog,
/// which is where overwrite confirmation happens.
#[tauri::command]
pub fn attachment_save(blob_hash: String, dest: String) -> Result<(), CmdError> {
    let bytes = blob_bytes(&blob_hash)?;
    std::fs::write(&dest, &bytes).map_err(|e| format!("'{dest}' へ書けません: {e}").into())
}

/// A blob's contents, by hash, out of this device's blob store — the read both attachment faces
/// (open externally, download) start from. A hash whose bytes are not here is a miss, not an error
/// in the store: attachment rows travel while blobs are fetched separately.
fn blob_bytes(hash: &str) -> Result<Vec<u8>, CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    let blobs =
        amenbo_core::blob::BlobStore::at(paths.base_dir.join(amenbo_core::blob::BLOBS_SUBDIR));
    if !blobs.has(hash) {
        return Err(format!("blob {hash} の実体がこの端末にありません").into());
    }
    blobs
        .read(hash)
        .map_err(|e| format!("blob {hash} を読めません: {e}").into())
}

/// Open a path or URL in the OS's default application (macOS `open`, Windows `cmd /C start`,
/// otherwise `xdg-open`). Same shape as the CLI helper of the same name — a minimal copy, since the
/// GUI cannot reuse it.
fn os_open(target: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(target);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", target]);
        c
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(target);
        c
    };
    cmd.status().map(|_| ())
}

/// One of the "what next" affordances on the project-created screen: open the bound folder in the
/// OS's file manager (Finder on macOS, Explorer on Windows, the default file manager elsewhere).
/// Handing `os_open` a folder path opens that folder. If the folder is not there, fail with a
/// message that says so — it may have been dropped after we got hold of it.
#[tauri::command]
pub fn reveal_folder(path: String) -> Result<(), CmdError> {
    if !std::path::Path::new(&path).is_dir() {
        return Err(format!("フォルダが見つかりません: {path}").into());
    }
    os_open(&path).map_err(|e| format!("'{path}' を開けません: {e}").into())
}

/// One of the "what next" affordances on the project-created screen: open the bound folder in a
/// terminal. Launches the terminal application per OS (`open -a Terminal` on macOS, `cmd start` on
/// Windows, `x-terminal-emulator` elsewhere). Where there is no terminal (on Linux, say, if none is
/// installed) this is best-effort — a failure to launch simply comes back as the error.
#[tauri::command]
pub fn open_terminal(path: String) -> Result<(), CmdError> {
    if !std::path::Path::new(&path).is_dir() {
        return Err(format!("フォルダが見つかりません: {path}").into());
    }
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.args(["-a", "Terminal", &path]);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", "cmd", "/K", "cd", "/d", &path]);
        c
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut cmd = {
        let mut c = std::process::Command::new("x-terminal-emulator");
        c.current_dir(&path);
        c
    };
    cmd.status()
        .map(|_| ())
        .map_err(|e| format!("ターミナルを開けません: {e}").into())
}

/// Delete an attachment for good. The blob's bytes stay until nothing references them and are
/// reclaimed on a separate path (GC).
/// Invalidates the affected target (task/decision) so any open detail view refetches.
#[tauri::command]
pub fn attachment_remove(
    id: i64,
    target_type: String,
    target_id: i64,
) -> Result<WriteAck, CmdError> {
    let target = amenbo_core::model::AttachmentTarget::parse(&target_type)
        .ok_or_else(|| format!("添付先種別 '{target_type}' は不正です（task / decision / task_comment / decision_comment）"))?;
    with_store_mut(|store| {
        store.remove_attachment(id)?;
        Ok(())
    })?;
    let scope: &[&'static str] = if target == amenbo_core::model::AttachmentTarget::Decision {
        &["decisions"]
    } else {
        &["tasks"]
    };
    let ack = WriteAck::new(scope);
    Ok(if target == amenbo_core::model::AttachmentTarget::Decision {
        ack.decision(target_id)
    } else {
        ack.task(target_id)
    })
}

/// Record a decision (Proposed), created under `project_id`. The GUI's actor is always human.
#[tauri::command]
pub fn decision_add(project_id: i64, title: String, body: Option<String>) -> Result<WriteAck, CmdError> {
    let id = with_store_mut(|store| {
        let d = store.add_decision(amenbo_core::ops::decision::NewDecision {
            title, body: body.unwrap_or_default(), project_id,
        })?;
        Ok(d.id)
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(id))
}

/// Accept a decision (Proposed → Accepted). decided_by is me.
#[tauri::command]
pub fn decision_accept(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let by = ActorKind::Human.as_str().to_string();
        store.accept_decision(id, Some(by))?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(id))
}

/// Reject a decision (Proposed → Rejected).
#[tauri::command]
pub fn decision_reject(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.reject_decision(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(id))
}

/// Put an accepted decision back under discussion (Accepted → Proposed, clearing decided_*). The
/// sanctioned way to fix a minor flaw without dirtying the supersession chain, while keeping the
/// freeze meaningful. Non-destructive, reversible, auditable.
#[tauri::command]
pub fn decision_reopen(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.reopen_decision(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(id))
}

/// Edit a decision's body (Proposed only — once accepted, it is frozen).
#[tauri::command]
pub fn decision_edit(id: i64, title: Option<String>, body: Option<String>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.update_decision(id, amenbo_core::ops::decision::DecisionPatch { title, body })?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(id))
}

/// Have decision `new_id` replace `old_id` (the supersession chain).
#[tauri::command]
pub fn decision_supersede(new_id: i64, old_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let by = ActorKind::Human.as_str().to_string();
        store.supersede_decision(new_id, old_id, Some(by))?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(new_id).decision(old_id))
}

/// Have decision `new_id` partially revise `old_id` (amends — the target stays current).
#[tauri::command]
pub fn decision_amend(new_id: i64, old_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.amend_decision(new_id, old_id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(new_id).decision(old_id))
}

/// Record that decision `new_id` builds on `old_id` (builds_on). Both decisions stay current; all
/// that changes is the order they should be read in and the blast radius.
#[tauri::command]
pub fn decision_builds_on(new_id: i64, old_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.decision_builds_on(new_id, old_id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(new_id).decision(old_id))
}

/// Remove one edge between decisions (all three kinds share this). The edge is named by its pair —
/// `decision_edge_pair` is UNIQUE, so the kind is not needed. This corrects wiring that was drawn by
/// mistake; it does not undo a decision — remove a `supersedes` edge and the target simply becomes
/// current again, since currency is a derived projection (there is nothing to clean up after).
#[tauri::command]
pub fn decision_unlink_edge(decision_id: i64, target_decision_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.unlink_decision_edge(decision_id, target_decision_id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(decision_id).decision(target_decision_id))
}

/// Link a decision to a task, or unlink it (`link=false` unlinks). The editing affordance behind
/// cross-links.
#[tauri::command]
pub fn decision_set_link(decision_id: i64, task_id: i64, link: bool) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        if link {
            store.link_decision(decision_id, task_id)?;
        } else {
            store.unlink_decision(decision_id, task_id)?;
        }
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions", "tasks"]).decision(decision_id).task(task_id))
}

/// Promote a task comment (task_comment) into a decision: the comment's text becomes the body, the
/// task's project becomes the decision's project, and the new decision is linked back to that task.
#[tauri::command]
pub fn decision_promote(comment_id: i64, title: String) -> Result<WriteAck, CmdError> {
    let (decision_id, task_id) = with_store_mut(|store| {
        let c = store.task_comment(comment_id)?
            .ok_or_else(|| format!("コメント '{comment_id}' が見つかりません"))?;
        let task_id = c.task_id;
        let body = c.text.clone();
        let project_id = store.task(task_id)?
            .and_then(|t| t.project_id)
            .ok_or_else(|| "コメントのタスクにプロジェクトがありません".to_string())?;
        let d = store.add_decision(amenbo_core::ops::decision::NewDecision {
            title, body, project_id,
        })?;
        let did = d.id;
        store.link_decision(did, task_id)?;
        Ok((did, task_id))
    })?;
    Ok(WriteAck::new(&["decisions", "tasks"]).decision(decision_id).task(task_id))
}

/// Set or edit a task's description (notes, Markdown); an empty string clears it. Like core's
/// `task::update`, it emits no system event (same shape as the CLI's `task update`). The watcher
/// picks the change up, so other sessions see it too.
#[tauri::command]
pub fn task_set_notes(id: i64, notes: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.update_task(id, amenbo_core::ops::task::TaskPatch {
            notes: Some(notes),
            ..Default::default()
        })?;
        Ok(())
    })?;
    Ok(WriteAck::new(&[]).task(id))
}

/// Set or edit a task's title. Like core's `task::update`, an empty title is refused by core, and no
/// system event is emitted (same shape as the CLI's `task update --title`). The title also shows on
/// the list cards, so we raise the tasks scope and let the board and list refetch too.
#[tauri::command]
pub fn task_set_title(id: i64, title: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.update_task(id, amenbo_core::ops::task::TaskPatch {
            title: Some(title),
            ..Default::default()
        })?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// Change the priority; priority=None clears it. Same shape as the CLI's
/// `task update --priority/--clear-priority`.
#[tauri::command]
pub fn task_set_priority(id: i64, priority: Option<String>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let patch = match priority.as_deref() {
            None | Some("") => amenbo_core::ops::task::TaskPatch {
                clear_priority: true,
                ..Default::default()
            },
            Some(p) => {
                let pri = match p {
                    "high" => Priority::High,
                    "medium" => Priority::Medium,
                    "low" => Priority::Low,
                    other => return Err(format!("priority '{other}' は不正です（high / medium / low）").into()),
                };
                amenbo_core::ops::task::TaskPatch {
                    priority: Some(pri),
                    ..Default::default()
                }
            }
        };
        store.update_task(id, patch)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// Create **one project row** in the store. What it brings into being is a project, not a store — it
/// doubles as genesis only on a machine that has no store yet (the GUI's first launch). Both of the
/// GUI's project-creation paths (by name, by folder) come through here. Returns `(the still-open,
/// already-saved store, project_id)`.
fn provision_project(name: &str) -> Result<(Store, i64), CmdError> {
    ensure_migrated()?;
    let paths = amenbo_core::config::Paths::resolve()?;
    let mut store = if amenbo_core::store_engine::probe_is_populated(&paths.store_file) {
        amenbo_core::store::Store::open_at(paths)?
    } else {
        amenbo_core::store::Store::init(paths, None)?
    };
    let pname = {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            amenbo_core::config::default_project_name(store.config.language.as_deref())
        } else {
            trimmed.to_string()
        }
    };
    let project = store.project_add(amenbo_core::ops::project::NewProject {
        name: pname,
        view: amenbo_core::model::View::Board,
        notes: String::new(),
        color: None,
    })?;
    let project_id = project.id;
    store.save_config()?;
    Ok((store, project_id))
}

/// Create a project by name, with no folder bound to it — one more project row in the store. Every
/// GUI action is the human facet, so the CLI's `guard_ai_project_ops` (the guardrail aimed at the
/// AI) never comes into play.
#[tauri::command]
pub fn project_add(name: String) -> Result<WriteAck, CmdError> {
    let (_store, _project_id) = provision_project(&name)?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Turn the chosen folder into **a new amenbo project**. The flow: (1) if a `.amenbo` already exists
/// (in the folder or above it), refuse and respect what is there (`init_pointer_exists`); (2) if
/// there is no `.amenbo` but an amenbo managed block is present, **do not refuse on the marker
/// alone** — look up living projects in the bindings registry and branch (the same shape as guard 2
/// in the CLI's `init`): exactly one living project means the pointer was lost and we **recover** it
/// (`recover_lost_pointer`); several means `init_ambiguous_owners`, offering the candidates; none
/// means carry on; (3) bring one project into being; (4) write the `.amenbo` pointer into the
/// folder, record it in the reference registry, and upsert the managed block in the AI guidance
/// files (AGENTS.md / CLAUDE.md). **The folder's own contents (the source) are never touched** — all
/// we place there is `.amenbo` and the managed block of guidance. The project's name is the `name`
/// the creation screen passes, falling back to the folder's basename if it is omitted. A marker is
/// thin and is no proof of ownership (it is a borrowed surface, carried along by clones, copies and
/// sync), so the truth about ownership is taken from amenbo's own artifacts (`.amenbo` plus the
/// bindings registry) — and the reverse lookup counts **only the projects that still read back**: a
/// deleted project's rows are physically gone, while the teardown that forgets its bindings entry is
/// best-effort, so an entry can outlive the project it names. Recover onto one of those and the folder
/// is bound to an id that names nothing, leaving nothing at all in the sidebar. Once the pointer is
/// written, always call `set` (the primary directory) and `record_project_ref` (the project → folders
/// reverse index) as a pair; forget one and the folder you just bound goes missing from the list on
/// the settings screen.
#[tauri::command]
pub fn project_add_folder(dir: String, name: Option<String>) -> Result<WriteAck, CmdError> {
    let path = std::path::Path::new(&dir);
    if let Some((bound_dir, _)) = amenbo_core::binding::find_upward(path) {
        return Err(CmdError::coded(
            "init_pointer_exists",
            format!(
                "このフォルダ（または上位）は既に amenbo プロジェクトに紐付いています: {}",
                bound_dir.display()
            ),
            format!(
                "this folder (or an ancestor) is already bound to an amenbo project: {}",
                bound_dir.display()
            ),
        ));
    }
    if amenbo_core::agents::dir_has_managed_block(path) {
        let owners: Vec<i64> = match open_store_read() {
            Ok(store) => amenbo_core::binding::live_projects_claiming(&store, path),
            Err(_) => Vec::new(),
        };
        match owners.as_slice() {
            [project_id] => {
                return recover_lost_pointer(path, *project_id);
            }
            many if many.len() > 1 => {
                let candidates =
                    many.iter().map(|pid| pid.to_string()).collect::<Vec<_>>().join(", ");
                return Err(CmdError::coded(
                    "init_ambiguous_owners",
                    format!(
                        "このフォルダは複数の生存プロジェクトが所有を主張しており、どれを復旧すべきか一意に決められません（候補: {candidates}）: {}",
                        path.display()
                    ),
                    format!(
                        "several living projects claim this folder, so the lost pointer can't be recovered unambiguously (candidates: {candidates}): {}",
                        path.display()
                    ),
                ));
            }
            _ => {}
        }
    }
    let name = name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| {
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string()
        });
    let (store, project_id) = provision_project(&name)?;
    amenbo_core::binding::pointer_for(&store, project_id).write(path)?;
    let mut registry = store.bindings();
    registry.set(project_id, path.to_string_lossy());
    registry.record_project_ref(project_id, path.to_string_lossy());
    let _ = store.save_bindings(&registry);
    amenbo_core::agents::upsert_into_dir(
        path,
        store.config.language.as_deref(),
        amenbo_core::config::Paths::APP_NAME,
    );
    Ok(WriteAck::new(&["tasks"]))
}

/// When the `.amenbo` is gone but the bindings registry's reverse lookup names **exactly one living
/// project** as this folder's owner, **recover** the pointer rather than quietly creating a new
/// project — the same shape as the CLI `init` helper of the same name, equivalent to
/// `bind --project`. Rewrites the pointer and the bindings index, and upserts the managed block
/// idempotently (everything outside the markers is preserved).
fn recover_lost_pointer(path: &std::path::Path, project_id: i64) -> Result<WriteAck, CmdError> {
    let store = open_store()?;
    amenbo_core::binding::pointer_for(&store, project_id).write(path)?;
    {
        let mut reg = store.bindings();
        reg.set(project_id, path.to_string_lossy().to_string());
        reg.record_project_ref(project_id, path.to_string_lossy());
        let _ = store.save_bindings(&reg);
    }
    amenbo_core::agents::upsert_into_dir(
        path,
        store.config.language.as_deref(),
        amenbo_core::config::Paths::APP_NAME,
    );
    Ok(WriteAck::new(&["tasks"]))
}

/// Return one project's editable fields (name/notes/color/view/archived) straight from the
/// read-model, to prefill the project settings screen. Archived projects are returned too (this
/// screen is where they get unarchived). A project that is not found (deleted, say) yields a coded
/// error.
#[tauri::command]
pub fn project_get(project_id: i64) -> Result<ProjectSettingsDto, CmdError> {
    let store = open_store_read()?;
    let read_model = store.read_model();
    let row = amenbo_core::store_engine::read::project_settings(read_model.conn(), project_id)?
        .ok_or_else(|| {
            amenbo_core::Error::not_found(
                format!("project '{project_id}' not found"),
                format!("プロジェクト '{project_id}' が見つかりません"),
            )
        })?;
    Ok(ProjectSettingsDto {
        id: row.id,
        name: row.name,
        notes: row.notes,
        color: row.color.unwrap_or_else(|| "#9aa7b2".to_string()),
        view: row.default_view,
        archived: row.archived,
    })
}

/// Return the archived (but not deleted) projects straight from the read-model, for the sidebar's
/// "Archived" section. Complementary to the snapshot that supplies the active sidebar list
/// (`project_overview`, which is `archived = 0`): no project ever appears in both. Most recently
/// updated first, with id as a stable tiebreak.
#[tauri::command]
pub fn project_list_archived() -> Result<Vec<ArchivedProjectDto>, CmdError> {
    let store = open_store_read()?;
    let read_model = store.read_model();
    let rows = amenbo_core::store_engine::read::archived_projects(read_model.conn())?;
    Ok(rows
        .into_iter()
        .map(|r| ArchivedProjectDto {
            id: r.id,
            name: r.name,
            color: r.color.unwrap_or_else(|| "#9aa7b2".to_string()),
        })
        .collect())
}

/// Update a project's settings — rename, notes, color, default view (same shape as the CLI's
/// `project update`). Only the fields that were passed are changed; None leaves a field alone.
/// `view` arrives as an enum string (list/board/calendar/timeline), and anything else is an error.
#[tauri::command]
pub fn project_update(
    project_id: i64,
    name: Option<String>,
    notes: Option<String>,
    color: Option<String>,
    view: Option<String>,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let view = match view {
            Some(v) => Some(
                amenbo_core::model::View::parse(&v)
                    .ok_or_else(|| format!("ビュー '{v}' は不正です（list / board / calendar / timeline）"))?,
            ),
            None => None,
        };
        store.project_update(
            project_id,
            amenbo_core::ops::project::ProjectPatch { name, notes, view, color },
        )?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Reorder a project (the command/data layer the sidebar's drag-and-drop rests on). `position` is
/// one of `top`, `bottom`, `before`, `after`, and `before`/`after` need an `anchor_id` (the project
/// to sit next to) — drag-and-drop resolves its drop target mainly through those two. The order is
/// nothing but `project.order_key` in the single DB (the sidebar list draws in that order), and
/// `Store::project_move` (→ `ops::project::move_to`) resolves the anchor on `order_key` and computes
/// the key that goes between. The anchor is another project id in the same `project` table, and
/// always resolves.
#[tauri::command]
pub fn project_move(
    project_id: i64,
    position: String,
    anchor_id: Option<i64>,
) -> Result<WriteAck, CmdError> {
    let pos = match position.as_str() {
        "top" => amenbo_core::ops::Position::Top,
        "bottom" => amenbo_core::ops::Position::Bottom,
        "before" => amenbo_core::ops::Position::Before(
            anchor_id.ok_or("position 'before' は anchor_id（並べ替え先）が必要です")?,
        ),
        "after" => amenbo_core::ops::Position::After(
            anchor_id.ok_or("position 'after' は anchor_id（並べ替え先）が必要です")?,
        ),
        other => return Err(format!("position '{other}' は不正です（top / bottom / before / after）").into()),
    };
    with_store_mut(|store| {
        store.project_move(
            project_id, pos)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Archive or unarchive a project (same shape as the CLI's `project archive` / `unarchive`).
/// Archiving takes it out of the sidebar list (`project_overview` — live and not archived).
#[tauri::command]
pub fn project_set_archived(project_id: i64, archived: bool) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.project_set_archived(
            project_id, archived)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Delete a project **destructively** (same shape as the CLI's `project delete`). Its tasks, decisions
/// and dimensions are physically deleted with it — the op walks the subtree child-first, the schema
/// refusing to let a project go out from under a surviving child — and the `.amenbo`, the managed block
/// and the bindings entry of every folder bound to it are released. Keeping it around but out of sight
/// is archiving's job
/// ([`project_set_archived`]).
#[tauri::command]
pub fn project_delete(project_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.project_delete(project_id, ActorKind::Human)?;
        Ok(())
    })?;
    if let Ok(store) = open_store() {
        let _ = amenbo_core::project_teardown::teardown_deleted_project(&store, project_id);
    }
    Ok(WriteAck::new(&["tasks"]))
}

/// Folder management on the project settings screen: the folders bound to this project (folders
/// whose `.amenbo` points at it), found by reverse lookup — many folders to one project. Reads
/// `project_dirs` and flags each folder for existence (stale, moved or deleted folders come back
/// with `exists:false`). Same shape as `bound_folders` in the CLI's `project show`. Each row checks
/// its own `.amenbo` against the store, and carries a `mismatch` when the recorded slug disagrees
/// with reality — "this pointer belongs to a different store (its id may quietly name something
/// else)". **The listing is never blocked**; the id is authoritative. A folder with no pointer at
/// all comes back on the same row as `pointer_missing` (the registry still names this project, so it
/// shows up in the list, yet an AI started in that folder will not resolve here). That verdict comes
/// from core's shared path [`amenbo_core::binding::is_pointer_missing`], the same one behind the
/// CLI's `doctor` and `project show`. A pointer in the old format (`project_id` unreadable) comes
/// back as `legacy` on the same row: in the CLI, running a command in that folder lets
/// `resolve_upward` upgrade it automatically, but the GUI has no cwd and so the upgrade never gets
/// its chance. We surface it here and steer the user to a relink.
#[tauri::command]
pub fn project_bound_folders(project_id: i64) -> Result<Vec<BoundFolderDto>, CmdError> {
    let store = open_store_read()?;
    let registry = store.bindings();
    Ok(registry
        .dirs_for_project(project_id)
        .into_iter()
        .map(|dir| {
            let path = std::path::Path::new(dir);
            let exists = path.is_dir();
            let pointer = amenbo_core::binding::read_pointer(path);
            let mismatch = pointer
                .as_ref()
                .and_then(|b| amenbo_core::binding::slug_mismatch(&store, b))
                .map(|m| SlugMismatchDto {
                    project_id: m.project_id,
                    recorded: m.recorded,
                    actual: m.actual,
                });
            let legacy = amenbo_core::binding::is_legacy_pointer(path);
            let pointer_missing = amenbo_core::binding::is_pointer_missing(path);
            BoundFolderDto { path: dir.to_string(), exists, mismatch, legacy, pointer_missing }
        })
        .collect())
}

/// Folder management on the project settings screen: bind an existing folder to this **existing
/// project** (the Tauri path for `bind --project`). Places `.amenbo` in the folder, records it in
/// the store's binding tables (project_dirs / paths), and upserts the managed block in the
/// AI guidance files (AGENTS.md / CLAUDE.md). **The folder's own contents (the source) are never
/// touched.** The nested-binding guard — refuse when an ancestor is already a managed tree — is the
/// CLI `bind`'s same "respect the tree that is already there". Unlike `project_add_folder`, which
/// creates a new project, this binds a folder to a project that already exists.
#[tauri::command]
pub fn project_bind_folder(project_id: i64, dir: String) -> Result<WriteAck, CmdError> {
    use amenbo_core::binding::find_upward_ancestor;
    let path = std::path::Path::new(&dir);
    if !path.is_dir() {
        return Err(CmdError::from(amenbo_core::Error::not_found(
            format!("folder not found: {dir}"),
            format!("フォルダが見つかりません: {dir}"),
        )));
    }
    let cwd = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some((bound_dir, _)) = find_upward_ancestor(&cwd) {
        return Err(CmdError::coded(
            "binding_nested_tree",
            format!(
                "このフォルダは既に amenbo 管理ツリーの中にあります（{} で紐付け済み）。サブフォルダを紐付けると上位のポインタをシャドウします。",
                bound_dir.display()
            ),
            format!(
                "this folder is already inside an amenbo-managed tree (bound at {}); binding a subfolder would shadow that pointer",
                bound_dir.display()
            ),
        ));
    }
    let store = open_store()?;
    amenbo_core::binding::pointer_for(&store, project_id).write(&cwd)?;
    let mut registry = store.bindings();
    registry.set(project_id, cwd.to_string_lossy().to_string());
    registry.record_project_ref(project_id, cwd.to_string_lossy());
    store.save_bindings(&registry)?;
    amenbo_core::agents::upsert_into_dir(
        &cwd,
        store.config.language.as_deref(),
        amenbo_core::config::Paths::APP_NAME,
    );
    Ok(WriteAck::new(&[]))
}

/// Folder management on the project settings screen: unbind this folder (the Tauri path for
/// `unbind`). Removes only the folder's `.amenbo` pointer and amenbo's managed block (AGENTS.md /
/// CLAUDE.md), and forgets the folder in the registry — many folders map to one project, so the
/// other folders pointing at the same project are left alone. **The store itself is never deleted**:
/// this severs a binding, it does not remove a store. Confirming the destructive part is the GUI's
/// job (plugin-dialog). For a stale folder (moved or deleted), cleaning up the registry entry still
/// works.
#[tauri::command]
pub fn project_unbind_folder(dir: String) -> Result<WriteAck, CmdError> {
    let target = std::path::PathBuf::from(&dir);
    let marker = target.join(".amenbo");
    if marker.is_file() {
        std::fs::remove_file(&marker)
            .map_err(|e| CmdError::from(format!("{} を削除できません: {e}", marker.display())))?;
    }
    let _ = amenbo_core::agents::remove_from_dir(&target);
    let store = open_store()?;
    let mut registry = store.bindings();
    let mut forgot = registry.forget_dir(&dir);
    if let Ok(canon) = std::fs::canonicalize(&target) {
        let canon_str = canon.to_string_lossy().to_string();
        if canon_str != dir {
            forgot += registry.forget_dir(&canon_str);
        }
    }
    if forgot > 0 {
        store.save_bindings(&registry)?;
    }
    Ok(WriteAck::new(&[]))
}

/// Add a dimension (classification axis), scoped to the project and appended at the end. The GUI
/// creates it in its plain default form: single-select, unordered, no role — a generic user axis.
/// Same shape as the CLI's `dimension add`.
#[tauri::command]
pub fn dimension_add(project_id: i64, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.dimension_add(
            project_id,
            amenbo_core::ops::dimension::NewDimension { name, ..Default::default() },
        )?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Rename a dimension (same shape as the CLI's `dimension rename`).
#[tauri::command]
pub fn dimension_rename(id: i64, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.dimension_update(id, Some(&name), None, None, None)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Update a dimension's description (notes), whether its values are ordered (ordered), and whether
/// it is the time axis (time_axis). Only the fields passed are changed — same shape as the CLI's
/// `dimension update`. Turning `ordered` on makes reordering values (`dimension_value_move`) take
/// effect; turning `time_axis` on makes that axis's values carry periods.
#[tauri::command]
pub fn dimension_update(
    id: i64,
    notes: Option<String>,
    ordered: Option<bool>,
    time_axis: Option<bool>,
) -> Result<WriteAck, CmdError> {
    let role = time_axis.map(|on| if on { DimensionRole::TimeAxis } else { DimensionRole::None });
    with_store_mut(|store| {
        store.dimension_update(id, None, notes.as_deref(), ordered, role)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Reorder a dimension (give the anchor dimension's id to exactly one of `before` / `after` — same
/// shape as the CLI's `dimension move`).
#[tauri::command]
pub fn dimension_move(id: i64, before: Option<i64>, after: Option<i64>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let pos = amenbo_core::ops::Position::from_flags(false, false, before, after)?;
        store.dimension_move(id, pos)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Delete a dimension permanently (its values and the task assignments on them go with it by the
/// schema's `ON DELETE CASCADE` — same shape as the CLI's `dimension rm`).
#[tauri::command]
pub fn dimension_rm(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.dimension_delete(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Add a value (a choice) to a dimension, at the end (same shape as the CLI's
/// `dimension value-add`).
#[tauri::command]
pub fn dimension_value_add(dimension_id: i64, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.dimension_value_add(dimension_id, &name, None)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Rename a dimension value (same shape as the CLI's `dimension value-rename`).
#[tauri::command]
pub fn dimension_value_rename(value_id: i64, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.dimension_value_update(value_id, Some(&name), None)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Replace a dimension value's period `[startOn, endOn]` wholesale (both ends inclusive,
/// `YYYY-MM-DD`) — the same landing place as the CLI's
/// `dimension value-update --start/--end/--clear-*`. `None` opens that end (`endOn: None` means
/// ongoing). The GUI's date fields always send both ends, so there is no partial-update merge to do
/// as in the CLI. A period is the payload of `role: time_axis`, so a value on a non-time_axis
/// dimension is **refused here** — following core's arrangement that the gatekeeper lives above it,
/// in the CLI and the GUI.
#[tauri::command]
pub fn dimension_value_set_period(
    value_id: i64,
    start_on: Option<String>,
    end_on: Option<String>,
) -> Result<WriteAck, CmdError> {
    let start = parse_iso_date(start_on.as_deref())?;
    let end = parse_iso_date(end_on.as_deref())?;
    with_store_mut(|store| {
        let value = store
            .dimension_value(value_id)?
            .ok_or_else(|| amenbo_core::Error::not_found(
                format!("dimension value '{value_id}' not found"),
                format!("次元の値 '{value_id}' が見つかりません"),
            ))?;
        let role = store.dimension(value.dimension_id)?.map(|d| d.role);
        if !matches!(role, Some(amenbo_core::model::DimensionRole::TimeAxis)) {
            return Err(amenbo_core::Error::invalid(
                "only a time-axis dimension's values carry a period",
                "期間を持てるのは時間軸の次元の値だけです",
            )
            .into());
        }
        store.dimension_value_update(value_id, None, Some((start, end)))?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Turn `YYYY-MM-DD` into a date. An empty string (a date field the user cleared) opens that end,
/// just as `None` does.
fn parse_iso_date(s: Option<&str>) -> Result<Option<NaiveDate>, CmdError> {
    match s.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d").map(Some).map_err(|_| {
            CmdError::from(amenbo_core::Error::invalid(
                format!("'{s}' is not a date (expected YYYY-MM-DD)"),
                format!("'{s}' は日付ではありません（YYYY-MM-DD 形式）"),
            ))
        }),
    }
}

/// Reorder a dimension value (give the anchor value's id to exactly one of `before` / `after` — same
/// shape as the CLI's `dimension value-move`).
#[tauri::command]
pub fn dimension_value_move(value_id: i64, before: Option<i64>, after: Option<i64>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let pos = amenbo_core::ops::Position::from_flags(false, false, before, after)?;
        store.dimension_value_move(value_id, pos)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Delete a dimension value permanently (the task assignments on it go with it by the schema's
/// `ON DELETE CASCADE` — same shape as the CLI's `dimension value-rm`).
#[tauri::command]
pub fn dimension_value_rm(value_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.dimension_value_delete(value_id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Assign a dimension value to a task (on a single-select dimension this replaces whatever was
/// assigned on that axis — same shape as the CLI's `dimension set`).
#[tauri::command]
pub fn task_set_dimension_value(task_id: i64, value_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.set_task_dimension_value(task_id, value_id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(task_id))
}

/// Take a particular dimension value off a task (a no-op if it was not assigned — same shape as the
/// CLI's `dimension unset`).
#[tauri::command]
pub fn task_unset_dimension_value(task_id: i64, value_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.unset_task_dimension_value(task_id, value_id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(task_id))
}

/// The dimension assignments a task currently carries (`dimensionId`→`valueId`), straight from the
/// read-model. The detail pane's assignment selects use it to reflect the current value.
#[tauri::command]
pub fn task_dimensions(task_id: i64) -> Result<Vec<TaskDimensionAssignmentDto>, CmdError> {
    let store = open_store_read()?;
    let read_model = store.read_model();
    let rows = amenbo_core::store_engine::read::task_dimension_assignments(read_model.conn(), task_id)?;
    Ok(rows
        .into_iter()
        .map(|(dimension_id, value_id)| TaskDimensionAssignmentDto { dimension_id, value_id })
        .collect())
}

/// Every task assignment (`taskId`→`valueId`) for one project on one dimension, in a single read
/// straight from the read-model. The board uses it to bundle tasks by value on the chosen dimension
/// (browsing/grouping).
#[tauri::command]
pub fn project_dimension_assignments(project_id: i64, dimension_id: i64) -> Result<Vec<DimensionTaskValueDto>, CmdError> {
    let store = open_store_read()?;
    let read_model = store.read_model();
    let rows = amenbo_core::store_engine::read::project_dimension_assignments(read_model.conn(), project_id, dimension_id)?;
    Ok(rows
        .into_iter()
        .map(|(task_id, value_id)| DimensionTaskValueDto { task_id, value_id })
        .collect())
}

/// Assign the task to someone (`kind="ai"` means the person's AI — it lands in the mailbox).
/// Assignment is on the facet alone: `user_id=Some(_)` is the signal that there is an assignee (the
/// facet is `kind`), and `None` clears it. Idempotent — same assignee and facet is a no-op — because
/// `set_task_assignee` commits in a transaction of its own, so calling it with an unchanged value
/// would still move `updated_at`. From the GUI, the path that actually gets used is "hand it to my
/// AI".
#[tauri::command]
pub fn task_assign(id: i64, user_id: Option<String>, kind: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let actor_kind = match kind.as_str() {
            "ai" => ActorKind::Ai,
            "human" => ActorKind::Human,
            other => return Err(format!("facet '{other}' は不正です（human / ai）").into()),
        };
        let kind_arg = user_id.as_ref().map(|_| actor_kind);
        let noop = store.task(id)?.is_some_and(|t| t.assignee_kind == kind_arg);
        if !noop {
            store.set_task_assignee(id, kind_arg)?;
            let ev = amenbo_core::activity_log::event::task_assigned(user_id.as_deref(), kind_arg.map(|k| k.as_str()));
            emit(store, id, ev);
        }
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// Rewrite this person's facet display names (the roster — `config.human_name` / `ai_name`). `human`
/// and `ai` are each set only when they are `Some` and non-empty; `None` or empty leaves that facet
/// alone. Both None does nothing. Does nothing if there is no store yet (we never quietly genesis
/// one).
fn write_facet_names(human: Option<&str>, ai: Option<&str>) -> Result<(), CmdError> {
    let human = human.map(str::trim).filter(|s| !s.is_empty());
    let ai = ai.map(str::trim).filter(|s| !s.is_empty());
    if human.is_none() && ai.is_none() {
        return Ok(());
    }
    ensure_migrated()?;
    let paths = amenbo_core::config::Paths::resolve()?;
    if amenbo_core::env::home().is_none() && !paths.store_file.exists() {
        return Ok(());
    }
    let mut store = Store::open_at(paths)?;
    if let Some(h) = human {
        store.config.set("human_name", h)?;
    }
    if let Some(a) = ai {
        store.config.set("ai_name", a)?;
    }
    store.save_config()?;
    Ok(())
}

/// Save first-run setup: apply the language (config.language, optional) and the roster's two display
/// names (human / AI, each optional), then raise `config.onboarded=true` so the flow never shows
/// again. Skipping is calling it with everything null — only the flag goes up. language and
/// onboarded live in the user-level config; the display names live in the store's own config.
#[tauri::command]
pub fn onboarding_save(
    language: Option<String>,
    human_name: Option<String>,
    ai_name: Option<String>,
) -> Result<WriteAck, CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    let mut config = amenbo_core::config::Config::load(&paths.config_file);
    if let Some(l) = language.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
        config.language = Some(l.to_string());
    }
    config.onboarded = true;
    config.save(&paths.config_file)?;

    write_facet_names(human_name.as_deref(), ai_name.as_deref())?;
    Ok(WriteAck::new(&[]))
}

/// Change the user's language `config.language` from the settings screen (the way to change it at
/// any time, long after first-run onboarding). The language lives in the user-level global
/// `config.json`, outside the store, so it can be written whether or not a store exists. When the
/// front end applies the `language` in the snapshot we return, i18n switches over **without a
/// restart**, with no help from `watch_store`. The change is also carried into the managed block of
/// AGENTS.md and CLAUDE.md in every bound directory — closing the gap where the GUI switched to
/// English while the AI kept being told Japanese. That part is best-effort and re-syncs only the
/// directories the registry knows about (unregistered ones fall into line at the next bind).
#[tauri::command]
pub fn config_set_language(language: String) -> Result<WriteAck, CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    let mut config = amenbo_core::config::Config::load(&paths.config_file);
    config
        .set("language", language.trim())
        ?;
    config.save(&paths.config_file)?;
    let lang_code = config.language.as_deref();
    let registry = open_store_read().map(|s| s.bindings()).unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    for dir in registry.paths.values() {
        if seen.insert(dir.clone()) {
            amenbo_core::agents::upsert_into_dir(
                std::path::Path::new(dir),
                lang_code,
                amenbo_core::config::Paths::APP_NAME,
            );
        }
    }
    Ok(WriteAck::new(&[]))
}

/// Switch the level of perf instrumentation (`config.perf_log`) from the settings screen. The values
/// are `off`, `budget-only` and `verbose`. Saves to config.json and then `reload`s the running
/// tracing filter, so it takes effect **without a restart** (if `AMENBO_PERF` is set in the
/// environment, it wins).
#[tauri::command]
pub fn config_set_perf_log(mode: String) -> Result<WriteAck, CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    let mut config = amenbo_core::config::Config::load(&paths.config_file);
    config.set("perf_log", mode.trim())?;
    config.save(&paths.config_file)?;
    crate::perf::reload(config.perf_log);
    Ok(WriteAck::new(&[]))
}

/// Turn update checking on or off from the settings screen. A thin wrapper straight onto core's
/// `Config::set("update_check", …)` (on by default). The next snapshot reflects the new value in
/// `updateCheck`, and when it is off, upstream latest.json is no longer queried.
#[tauri::command]
pub fn config_set_update_check(enabled: bool) -> Result<WriteAck, CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    let mut config = amenbo_core::config::Config::load(&paths.config_file);
    config.set("update_check", if enabled { "true" } else { "false" })?;
    config.save(&paths.config_file)?;
    Ok(WriteAck::new(&[]))
}

/// Write the roster's two avatars (human / AI) to `config.human_avatar` / `ai_avatar`. Each argument
/// has three states: `None` leaves that facet alone, `Some("")` clears it (back to the identicon),
/// and `Some(dataUrl)` sets it. Unlike display names ([`write_facet_names`]), an avatar **can be
/// cleared**, so an empty string and an absent key mean different things. Format and size limits are
/// checked by core's [`amenbo_core::config::validate_avatar`] before anything is written.
fn write_facet_avatars(human: Option<&str>, ai: Option<&str>) -> Result<(), CmdError> {
    if human.is_none() && ai.is_none() {
        return Ok(());
    }
    for (key, v) in [("human_avatar", human), ("ai_avatar", ai)] {
        if let Some(val) = v.map(str::trim).filter(|s| !s.is_empty()) {
            amenbo_core::config::validate_avatar(key, val)?;
        }
    }
    ensure_migrated()?;
    let paths = amenbo_core::config::Paths::resolve()?;
    if amenbo_core::env::home().is_none() && !paths.store_file.exists() {
        return Ok(());
    }
    let mut store = Store::open_at(paths)?;
    if let Some(h) = human {
        store.config.set("human_avatar", h.trim())?;
    }
    if let Some(a) = ai {
        store.config.set("ai_avatar", a.trim())?;
    }
    store.save_config()?;
    Ok(())
}

/// Set or clear the per-facet (human / AI) avatars from the settings screen. The counterpart of
/// [`write_facet_names`] for display names: the roster's two faces live in config. For each
/// argument, an absent key leaves it alone, an empty string clears it, and a data URL sets it.
#[tauri::command]
pub fn set_facet_avatars(human_avatar: Option<String>, ai_avatar: Option<String>) -> Result<WriteAck, CmdError> {
    write_facet_avatars(human_avatar.as_deref(), ai_avatar.as_deref())?;
    Ok(WriteAck::new(&[]))
}

/// Rewrite the roster's two display names (human / AI) from the settings screen; config is
/// authoritative for display names. Only a facet given as `Some(non-empty)` is updated; the other is
/// left alone. Both None or empty is an error — a call that would change nothing is refused.
#[tauri::command]
pub fn set_facet_names(human_name: Option<String>, ai_name: Option<String>) -> Result<WriteAck, CmdError> {
    let human = human_name.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let ai = ai_name.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if human.is_none() && ai.is_none() {
        return Err("表示名は空にできません。".into());
    }
    write_facet_names(human, ai)?;
    Ok(WriteAck::new(&[]))
}

/// The cancellation flag for the whole-store operations (backup/restore/export). "Abort" in the
/// progress modal raises it through [`cancel_data_op`], and core's per-store progress callback reads
/// it at each boundary and `Break`s. Every operation resets it to false when it starts, so a
/// cancellation never carries over into the next one.
static DATA_OP_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Abort a running whole-store backup/restore at the next store boundary ("Abort" in the progress
/// modal). Core leaves nothing half-applied: a backup deletes its unfinished archive, and a restore
/// rolls back every swap it had completed.
#[tauri::command]
pub fn cancel_data_op() {
    DATA_OP_CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Return `config.language` (or `None` when unset) **without opening the store**. Normally the UI
/// language rides in the snapshot — but a store that a newer build has moved past yields no
/// snapshot, and the restart screen still has to speak the user's language. `config.json` is a file
/// of its own, outside the store, and can be read without passing the version gate.
#[tauri::command]
pub fn ui_language() -> Option<String> {
    amenbo_core::config::Paths::resolve()
        .ok()
        .and_then(|p| amenbo_core::config::Config::load(&p.config_file).language)
}

/// Exit this process and launch the same executable again (the button on the restart screen). A
/// long-running GUI that a newer process has overtaken is nothing but **an old process still sitting
/// in memory**: the GUI and the CLI ship together, so the executable on disk is already the new
/// version, and relaunching `current_exe` simply becomes it (on Linux, `dpkg` swaps the inode, so
/// only the running process is stale). This is not self-update: it touches no network and fetches no
/// new binary — it relaunches what is already there, and the user is the one who presses it.
#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    app.restart();
}

/// What the startup migration is doing. The front end asks **first thing** at startup and, if it is
/// `running`, goes to the migration screen without reading the store (`idle` is the normal case —
/// straight into the app). After that it follows the `migration-changed` / `migration-progress`
/// events — subscribing alone is not enough, because the phase can advance before the window is even
/// mounted.
#[tauri::command]
pub fn migration_status() -> crate::migrate::MigrationStatusDto {
    crate::migrate::status()
}

/// Retry a failed migration ("Retry" on the migration screen). A failure means it was rolled back
/// whole and the store still stands exactly as it did before it began (core's envelope), so once
/// whatever was in the way is cleared — freeing disk space, say — the same path can simply be walked
/// again. On success, the store's resident threads (watching, GC), which the first launch skipped,
/// are started here. Heavy I/O goes off the main thread via `spawn_blocking`.
#[tauri::command]
pub async fn migration_retry(app: tauri::AppHandle) -> Result<(), CmdError> {
    crate::migrate::begin();
    tauri::async_runtime::spawn_blocking(move || {
        if crate::migrate::run(&app) {
            crate::start_store_threads(app.clone());
        }
    })
    .await
    .map_err(|e| CmdError::from(format!("移行のやり直しに失敗しました: {e}")))?;
    Ok(())
}

/// Map core's [`amenbo_core::progress::Phase`] to the stable string the GUI localizes.
fn phase_str(phase: amenbo_core::progress::Phase) -> &'static str {
    use amenbo_core::progress::Phase;
    match phase {
        Phase::Snapshotting => "snapshotting",
        Phase::Blobs => "blobs",
        Phase::Unpacking => "unpacking",
        Phase::Verifying => "verifying",
        Phase::Exporting => "exporting",
        Phase::Copying => "copying",
        Phase::Migrating => "migrating",
    }
}

/// A progress sink: it streams progress to the webview as `data-progress` events, and returns
/// `Break` to cancel when [`DATA_OP_CANCEL`] is raised. It owns its `window`, so it is `'static` and
/// can be handed to `spawn_blocking`.
fn progress_sink(
    window: tauri::Window,
) -> impl FnMut(&amenbo_core::progress::Progress) -> std::ops::ControlFlow<()> {
    use tauri::Emitter;
    move |p| {
        let _ = window.emit("data-progress", DataProgressDto::of(p));
        if DATA_OP_CANCEL.load(std::sync::atomic::Ordering::SeqCst) {
            std::ops::ControlFlow::Break(())
        } else {
            std::ops::ControlFlow::Continue(())
        }
    }
}

/// The payload of the `data-progress` event: the camelCase DTO of core's
/// [`amenbo_core::progress::Progress`]. `phase` is the stable string from [`phase_str`], which the
/// GUI localizes. The startup migration ([`crate::migrate`]) reports itself in the same shape — one
/// way of showing progress is enough.
#[derive(Debug, Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DataProgressDto {
    /// What it is doing (`snapshotting`, `verifying`, `copying`, ...; the GUI localizes it).
    phase: String,
    /// Units completed (from 0).
    done: u32,
    /// Total units, when known.
    total: Option<u32>,
}

impl DataProgressDto {
    /// Map a tick from core into the shape the webview can be fed.
    pub fn of(p: &amenbo_core::progress::Progress) -> Self {
        DataProgressDto {
            phase: phase_str(p.phase).to_string(),
            done: p.done as u32,
            total: p.total.map(|t| t as u32),
        }
    }
}

/// What [`run_backup`] returns: the camelCase DTO of core's
/// [`amenbo_core::archive::BackupReport`].
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct BackupReportDto {
    /// Path of the archive that was written.
    path: String,
    /// Size of the archive, in bytes.
    bytes: usize,
}

/// What [`run_restore`] returns: the camelCase DTO of core's
/// [`amenbo_core::archive::RestoreReport`].
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct RestoreReportDto {
    /// Where the old source of truth was set aside when it was replaced. None when nothing was
    /// replaced (a fresh creation).
    previous_saved_to: Option<String>,
    /// How many attachment blobs were written (blobs the destination already had, by hash, are not
    /// counted).
    #[ts(type = "number")]
    blobs: u64,
    /// How many older rollback points this restore's set-aside copy overtook and deleted. It is a
    /// report so that nothing is deleted silently, so the screen shows it only when it is non-zero.
    #[ts(type = "number")]
    superseded: usize,
    /// What the version chain did to the staged store. **Some only when it actually ran**, so a null
    /// check is all the front end needs to say "the archive you restored is not in the shape it was
    /// taken in".
    migration: Option<MigrationRunDto>,
}

/// The camelCase DTO of a version-chain run (core's
/// [`amenbo_core::store_engine::migrate::Run`]).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct MigrationRunDto {
    /// The format version the store carried before the run. `number` on the TS side (the default
    /// `bigint` cannot be interpolated into a sentence).
    #[ts(type = "number")]
    from: i64,
    /// The format version it carries now.
    #[ts(type = "number")]
    to: i64,
    /// Names of the steps that were applied, in order.
    applied: Vec<String>,
}

/// "Back up everything" under Settings > Data: write this machine's store out as a single, verified
/// archive (core's [`amenbo_core::archive::backup_from`]). The GUI is a thin wrapper, streaming
/// progress to the progress modal as `data-progress` events. The heavy I/O (VACUUM, verification)
/// goes off the main thread via `spawn_blocking` so the progress modal never freezes.
#[tauri::command]
pub async fn run_backup(window: tauri::Window, path: String) -> Result<BackupReportDto, CmdError> {
    DATA_OP_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(move || -> Result<BackupReportDto, CmdError> {
        let Some(source) = amenbo_core::archive::enumerate_store() else {
            return Err(CmdError::from(
                "この端末にバックアップ対象のストアがありません。".to_string(),
            ));
        };
        let mut progress = progress_sink(window);
        let report =
            amenbo_core::archive::backup_from(&source, std::path::Path::new(&path), &mut progress)?;
        Ok(BackupReportDto { path: report.path, bytes: report.bytes as usize })
    })
    .await
    .map_err(|e| CmdError::from(format!("まるごとバックアップの実行に失敗しました: {e}")))?
}

/// "Restore everything" under Settings > Data (**destructive**). Swaps the whole store for the one
/// carried in a `.amenbo-backup` archive, migrating it forward to this build's generation on the way
/// (core's [`amenbo_core::archive::restore_into`]: stage-and-swap, rollback on failure, and the old
/// source of truth set aside under a timestamp). Progress goes to the progress modal as events, and
/// the heavy I/O goes off the main thread via `spawn_blocking`. On success the front end rebuilds
/// the screen by invalidating every query and refetching the snapshot.
#[tauri::command]
pub async fn run_restore(window: tauri::Window, path: String) -> Result<RestoreReportDto, CmdError> {
    DATA_OP_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(move || -> Result<RestoreReportDto, CmdError> {
        let stamp = Timestamp::now().0.format("%Y%m%dT%H%M%SZ").to_string();
        let mut progress = progress_sink(window);
        let report = amenbo_core::archive::restore_into(
            std::path::Path::new(&path),
            &stamp,
            &amenbo_core::archive::restore_dest(),
            &mut progress,
        )?;
        let m = report.migration;
        let migration = m.migrated().then(|| MigrationRunDto {
            from: m.from,
            to: m.to,
            applied: m.applied.iter().map(|s| s.to_string()).collect(),
        });
        Ok(RestoreReportDto {
            previous_saved_to: report.previous_saved_to,
            blobs: report.blobs,
            superseded: report.superseded.len(),
            migration,
        })
    })
    .await
    .map_err(|e| CmdError::from(format!("まるごと復元の実行に失敗しました: {e}")))?
}

/// The progress sink for export (the sibling of [`progress_sink`]). It emits `data-progress` only
/// for store-boundary ticks (those with `total` set to `Some`); export's in-row cancel-poll ticks
/// (`total` of `None`, once every 256 rows) are used for the cancellation check alone and never
/// reach the progress modal — so even on a huge store the modal does not flicker and the IPC channel
/// does not flood. It owns its `window`, so it is `'static` and can be handed to `spawn_blocking`.
fn boundary_progress_sink(
    window: tauri::Window,
) -> impl FnMut(&amenbo_core::progress::Progress) -> std::ops::ControlFlow<()> {
    use tauri::Emitter;
    move |p| {
        if p.total.is_some() {
            let _ = window.emit("data-progress", DataProgressDto::of(p));
        }
        if DATA_OP_CANCEL.load(std::sync::atomic::Ordering::SeqCst) {
            std::ops::ControlFlow::Break(())
        } else {
            std::ops::ControlFlow::Continue(())
        }
    }
}

/// What [`run_export`] returns: the directory it wrote to, how big it is, and how many attachments
/// were carried out.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ExportReportDto {
    path: String,
    /// Total bytes of the directory that was written (`export.json` plus the attachment files). This
    /// is the number the completion message shows in KB — count only the JSON and a bundle carrying
    /// heavy attachments would claim to be far smaller than it is.
    bytes: usize,
    /// How many attachment files were written into `attachments/`.
    attachments: usize,
    /// How many attachments could not be carried out because their bytes are gone (we do not drop
    /// them silently).
    missing: usize,
}

/// "Export" under Settings > Data: write everything on this machine out into an **export
/// directory** (core's [`amenbo_core::export::export_bundle`], bounded memory) — an `export.json`
/// plus an `attachments/` directory holding the attachment files themselves. There is no import, so
/// this bundle *is* the artifact you migrate with, and without the files themselves nothing has
/// really been carried out. The destination is the `path` chosen in the front end's dialog (an
/// existing path is refused). Progress is streamed to the progress modal as `data-progress` events,
/// and `cancel_data_op` can stop it partway (core builds the export aside and only renames it into
/// place once whole, so an abort or failure leaves no directory at all rather than a truncated one).
/// Heavy I/O goes off the main thread via `spawn_blocking`.
#[tauri::command]
pub async fn run_export(window: tauri::Window, path: String) -> Result<ExportReportDto, CmdError> {
    DATA_OP_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
    let out = std::path::PathBuf::from(&path);
    let report = tauri::async_runtime::spawn_blocking(move || {
        let mut progress = boundary_progress_sink(window);
        amenbo_core::export::export_bundle(&out, &mut progress).map_err(CmdError::from)
    })
    .await
    .map_err(|e| CmdError::from(format!("エクスポートの実行に失敗しました: {e}")))??;

    Ok(ExportReportDto {
        path: report.path,
        bytes: report.bytes as usize,
        attachments: report.attachments as usize,
        missing: report.missing as usize,
    })
}

/// Raise an OS notification when something arrives in the inbox. macOS delivers it ourselves through
/// UNUserNotificationCenter, Windows through notify-rust (with the click wired to inbox navigation),
/// and Linux through `tauri-plugin-notification` (D-Bus). If the OS drops it — permission not
/// granted, say — that is not fatal (the app has no sound of its own; the arrival sound is the OS
/// notification's).
#[tauri::command]
#[cfg_attr(any(target_os = "macos", target_os = "windows"), allow(unused_variables))]
pub fn notify_os(app: tauri::AppHandle, title: String, body: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        crate::macos_notify::send(&title, &body);
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        crate::windows_notify::send(&app, title, body);
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        use tauri_plugin_notification::NotificationExt;
        app.notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .map_err(|e| e.to_string())
    }
}

/// One bound folder whose managed block is out of date. `version` is the version of that folder's
/// block; `current` is this binary's version ([`amenbo_core::agents::MANAGED_BLOCK_VERSION`]).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct StaleBlockDto {
    dir: String,
    file: String,
    version: u32,
    current: u32,
}

/// After the binary is updated, the `CLAUDE.md` / `AGENTS.md` of a bound folder can be left holding
/// an older managed block. A read-only command that lists those for the GUI over **the same core
/// detection path** as the CLI's `doctor` (`agents::stale_bound_blocks`) — no side effects, nothing
/// rewritten.
#[tauri::command]
pub fn stale_managed_blocks() -> Result<Vec<StaleBlockDto>, CmdError> {
    let current = amenbo_core::agents::MANAGED_BLOCK_VERSION;
    Ok(amenbo_core::agents::stale_bound_blocks(&open_store_read()?.bindings())
        .into_iter()
        .map(|s| StaleBlockDto { dir: s.dir, file: s.file.to_string(), version: s.version, current })
        .collect())
}

/// What `resync_managed_blocks` returns. `scanned` is how many folders that actually exist were
/// walked; `updated` lists the `(dir, file)` pairs rewritten to the current version — only the ones
/// whose content really changed.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ResyncReportDto {
    scanned: u32,
    updated: Vec<ResyncedDto>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ResyncedDto {
    dir: String,
    file: String,
}

/// Re-sync stale managed blocks to the current version, over **the same core path** as the CLI's
/// `sync-guide` (`agents::resync_bound_blocks`). Give it a `dir` for one folder; omit it for every
/// bound folder. Low churn — it writes only when the content actually changes — and each folder's
/// language label is preserved, never degraded. Nothing outside the markers is touched. It writes
/// `CLAUDE.md` / `AGENTS.md` on the filesystem and leaves the store alone, so there is no snapshot to
/// refetch (and no `WriteAck` to return).
#[tauri::command]
pub fn resync_managed_blocks(dir: Option<String>) -> Result<ResyncReportDto, CmdError> {
    let report = amenbo_core::agents::resync_bound_blocks(
        &open_store_read()?.bindings(),
        dir.as_deref(),
        amenbo_core::config::Paths::APP_NAME,
    );
    Ok(ResyncReportDto {
        scanned: report.scanned as u32,
        updated: report
            .updated
            .into_iter()
            .map(|(dir, file)| ResyncedDto { dir, file: file.to_string() })
            .collect(),
    })
}

/// A read-only command listing the bound-folder rows that no living project claims — the debris a
/// deleted project left behind in the index. Over **the same core detection path** as the CLI's
/// `doctor` (`binding::orphan_dirs`). No other GUI surface can show these, structurally: the folder
/// list ([`project_bound_folders`]) does its reverse lookup per project, so a row with no claimant
/// appears under no project at all. (`legacy` and `pointer_missing` show up in the folder list;
/// stale managed blocks in the [`stale_managed_blocks`] banner.)
#[tauri::command]
pub fn orphan_bindings() -> Result<Vec<String>, CmdError> {
    Ok(amenbo_core::binding::orphan_dirs(&open_store_read()?))
}

/// Forget the debris folder rows in the index (over **the same core path** as the CLI's
/// `doctor --fix`, `Store::forget_orphan_dirs`). It drops rows in the binding table and nothing
/// else — not the folder's contents, not its `.amenbo` — so it is not destructive and asks for no
/// confirmation (the CLI does not either). Returns how many were forgotten. Since the only rows it
/// touches are the ones no project claims, not a single row of a living project's reads (the
/// snapshot, the folder list) moves — so it returns no `WriteAck`: there is nothing to refetch.
#[tauri::command]
pub fn forget_orphan_bindings() -> Result<u32, CmdError> {
    let store = open_store()?;
    Ok(store.forget_orphan_dirs()? as u32)
}

/// One issue on the doctor screen (the same shape as core's
/// [`amenbo_core::validate::DoctorIssue`]). **No prose sentence rides along**: core returns only a
/// `kind` (the id of a message template) and `params` (what differs), and the surface composes the
/// sentence a person reads (the GUI localizes it by `config.language`; the CLI is always English).
/// The GUI's message table, and the affordances for how to fix each issue, live in
/// `src/core/i18n.ts`, and they point at affordances that really exist in the GUI (the repair button
/// under Settings > Integrity, the folder list in project settings).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DoctorIssueDto {
    kind: String,
    severity: String,
    target: String,
    params: std::collections::BTreeMap<String, String>,
}

impl From<&amenbo_core::validate::DoctorIssue> for DoctorIssueDto {
    fn from(i: &amenbo_core::validate::DoctorIssue) -> Self {
        Self {
            kind: i.kind.as_str().to_string(),
            severity: i.severity.to_string(),
            target: i.target.clone(),
            params: i.params.clone(),
        }
    }
}

/// What `doctor_report` returns.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DoctorReportDto {
    ok: bool,
    errors: usize,
    warnings: usize,
    issues: Vec<DoctorIssueDto>,
}

/// A read-only command listing the issues where a bound folder's `.amenbo` is broken (old format, or
/// gone). The GUI calls it **once, at app startup**, and adds the rows to the startup health banner
/// (the fix, [`repair_pointers`], can be pressed right there in the banner). It is deliberately not
/// folded into the snapshot ([`StartupHealthDto`]), because that is recomputed on every
/// store-changed tick, and inspecting the environment — an FS walk per bound folder — has no
/// business on that path. Detection goes through core's
/// [`amenbo_core::doctor::pointer_issues`] alone, so what surfaces here and what surfaces on the
/// doctor screen ([`doctor_report`]) always agree.
#[tauri::command]
pub fn pointer_issues() -> Result<Vec<DoctorIssueDto>, CmdError> {
    Ok(amenbo_core::doctor::pointer_issues(&open_store_read()?)
        .iter()
        .map(DoctorIssueDto::from)
        .collect())
}

/// A question waiting to be put to the user: may amenbo write the lint hooks into this repository?
///
/// It carries what the question is *about* and nothing about how it would be answered. Which slots are
/// empty, which a stranger holds, whether the hooks directory is one the whole team shares — all of that is
/// `amenbo_core::hooks::install`'s to act on, and none of it is a fork in the user's road: nobody wants an
/// AMB-T-… in their commits, so a screen that laid the machinery out would be asking them to solve amenbo's
/// problem. What is still unwired afterwards is the setup banner's to report ([`HookNoticeDto`]), where it
/// is a statement rather than a question.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct HookOfferDto {
    /// The project the answer is recorded on: consent is one per project, for the lint as a feature
    /// rather than per file.
    /// `number` on the TS side (the default `bigint` cannot be passed back through `hook_answer`).
    #[ts(type = "number")]
    project_id: i64,
    /// The project's name, so the modal can say what it is asking about.
    project_name: String,
    /// The git repository the hooks would be written into, which is also what identifies this offer.
    dir: String,
    /// What this build of amenbo is called on the command line, which is what its hooks will actually
    /// run and what its guidance tells the user to type. The dev channel answers `amenbo-dev`, so the
    /// name travels rather than being spelled into the wording.
    cmd: String,
}

/// The questions the GUI should ask about the lint hooks, and the acts that need no asking — the
/// GUI's half of [`amenbo_core::hooks::reconcile`], where the CLI's is `lint_hook_setup`. The GUI
/// calls it **once, at app startup**, for the reason [`pointer_issues`] is called there and not on
/// the snapshot path: probing costs a `git` spawn per folder, and the environment does not change on
/// a store tick.
///
/// The CLI asks about the repository it was run in. The GUI has no cwd, so it asks about every bound
/// folder, taking each folder's own `.amenbo` as the answer to which project it belongs to — the same
/// question an AI started in that folder would resolve. A folder that is not a git repository has no
/// hooks to have and no question to ask.
///
/// Judgment stays in core: this only carries out what `reconcile` returns. Two of its answers are not
/// questions and are settled here — a hook of ours found under a record that does not say yes moves
/// the record (the disk wins), and a slot this build added is completed under the consent already
/// given — while `Ask` is the only one that needs a user, and so the only one that comes back. Both
/// acts are best-effort: a hook is a convenience, and failing the startup over one would help no one.
#[tauri::command]
pub fn hook_offers() -> Result<Vec<HookOfferDto>, CmdError> {
    use amenbo_core::hooks::{self, HookAction, HookConsent};

    let store = open_store()?;
    let cmd = amenbo_core::config::Paths::APP_NAME;
    let mut offers = Vec::new();
    for dir in store.bindings().all_dirs() {
        let path = std::path::Path::new(&dir);
        let Some(project_id) = amenbo_core::binding::read_pointer(path).and_then(|b| b.project_id) else {
            continue;
        };
        let Some(states) = hooks::probe(path) else { continue };
        let consent = store.hook_consent(project_id).ok().flatten();
        match hooks::reconcile(&hooks::HookContext { states: Some(states), consent, can_ask: true }) {
            HookAction::Nothing => {}
            HookAction::SyncConsentToYes => {
                let _ = store.set_hook_consent(project_id, HookConsent::Yes);
            }
            HookAction::Install => {
                let _ = hooks::install(path, cmd);
            }
            HookAction::Ask => {
                let Ok(Some(project)) = store.project(project_id) else { continue };
                offers.push(HookOfferDto {
                    project_id,
                    project_name: project.name,
                    dir: dir.clone(),
                    cmd: cmd.to_string(),
                });
            }
        }
    }
    Ok(offers)
}

/// One bound repository where the lint is not actually running — the raw material for the banner's
/// wording, never the sentence, as with [`HookOfferDto`].
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct HookNoticeDto {
    /// The project's name, so the banner can say which one is unfinished.
    project_name: String,
    /// The git repository whose setup is unfinished, which is also what identifies this notice.
    dir: String,
    /// What this build is called on the command line, for the same reason [`HookOfferDto::cmd`]
    /// carries it: the dev channel answers `amenbo-dev` and the wording must not spell either in.
    cmd: String,
    /// Slots with no hook at all, which `hooks install` writes.
    unwired: Vec<String>,
    /// Slots a stranger holds, which amenbo will not write.
    foreign: Vec<String>,
    /// The line to add by hand, one per [`HookNoticeDto::foreign`] slot and in the same order.
    guidance: Vec<String>,
}

/// Where setup is still unfinished — the GUI's third channel for it, alongside the CLI's `--json`
/// field and stderr line. This is the standing report ([`amenbo_core::hooks::setup_notice`]), not
/// [`hook_offers`]'s one-time question: it warns and offers no button, because the answer to it was
/// either already given or already declined.
///
/// The GUI calls it **once, after [`hook_offers`] has had its turn**, and that order is the point
/// rather than a detail of scheduling. Both probe, so this pays a second `git` spawn per folder — and
/// what it buys is a probe of the disk the first one *changed*: an answered yes has installed hooks by
/// then, and a notice computed from the earlier probe would report slots that are now wired. The
/// double cost is the price of the report being true, which for a report is the whole of its value.
#[tauri::command]
pub fn hook_notices() -> Result<Vec<HookNoticeDto>, CmdError> {
    use amenbo_core::hooks;

    let store = open_store_read()?;
    let cmd = amenbo_core::config::Paths::APP_NAME;
    let mut notices = Vec::new();
    for dir in store.bindings().all_dirs() {
        let path = std::path::Path::new(&dir);
        let Some(project_id) = amenbo_core::binding::read_pointer(path).and_then(|b| b.project_id) else {
            continue;
        };
        let consent = store.hook_consent(project_id).ok().flatten();
        let Some(notice) = hooks::setup_notice(hooks::probe(path), consent) else { continue };
        let Ok(Some(project)) = store.project(project_id) else { continue };
        notices.push(HookNoticeDto {
            project_name: project.name,
            dir: dir.clone(),
            cmd: cmd.to_string(),
            unwired: notice.unwired.iter().map(|s| s.name().to_string()).collect(),
            guidance: notice.foreign.iter().map(|s| hooks::guidance_line(*s, cmd)).collect(),
            foreign: notice.foreign.iter().map(|s| s.name().to_string()).collect(),
        });
    }
    Ok(notices)
}

/// Write down what the user answered to a [`HookOfferDto`], and install on a yes. The record is what
/// decides whether the question is ever asked again, so it is written **only when an answer was
/// actually given**: a modal the user dismissed calls nothing at all, and the project stays
/// unanswered for the next startup to ask again. That is why this takes a `yes` rather than
/// an "outcome" — there is no third value to pass, because the third outcome is this command not
/// running.
///
/// A failed install records nothing: consent is the note that the hooks are where the user asked for
/// them, and keeping it after failing to put them there would make the record a lie and silence the
/// question that should be asked again. A stranger's slot is not a failure — the install steps around it,
/// and the setup banner is where that gets said ([`HookNoticeDto`]), after the fact and out of the way of
/// an answer that was never about it.
#[tauri::command]
pub fn hook_answer(project_id: i64, dir: String, yes: bool) -> Result<(), CmdError> {
    use amenbo_core::hooks::{self, HookConsent};

    let store = open_store()?;
    if yes {
        hooks::install(std::path::Path::new(&dir), amenbo_core::config::Paths::APP_NAME)?;
    }
    let _ = store.set_hook_consent(project_id, if yes { HookConsent::Yes } else { HookConsent::No });
    Ok(())
}

/// What [`repair_pointers`] returns: how many folders were fixed, and how many were left waiting on
/// a human's judgement.
#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts", rename_all = "camelCase")]
pub struct PointerRepairDto {
    /// Folders whose pointer was rewritten, or written back, in the current format.
    repaired: Vec<String>,
    /// Folders left untouched because their owner could not be determined uniquely (the human
    /// rebinds them through "open folder").
    unresolved: Vec<String>,
}

/// Fix a broken `.amenbo` (old format, or gone) **right there**. The repair button on the startup
/// health banner calls it. Core already knows how — run amenbo in that folder and `resolve_upward`
/// quietly fixes it — so we put the same fix within reach in the banner, and the user does not have
/// to go hunting through the settings screen. All it writes is each folder's `.amenbo`; the store is
/// untouched, so there is no snapshot to refetch.
#[tauri::command]
pub fn repair_pointers() -> Result<PointerRepairDto, CmdError> {
    let repair = amenbo_core::binding::repair_pointers(&open_store_read()?);
    Ok(PointerRepairDto { repaired: repair.repaired, unresolved: repair.unresolved })
}

/// The read-only command the GUI's doctor screen (Settings > Integrity) reads. It goes over **the
/// same core path** as the CLI's `doctor` (`doctor::report` — the store's internal consistency plus
/// this machine's environment), so the issues raised on the two surfaces never diverge (only the
/// prose differs: the GUI's UI language, the CLI's English). The startup health banner
/// ([`StartupHealthDto`]) sees only the store-internal doctor and the binding pointers
/// ([`pointer_issues`]); stale managed blocks and debris folder rows have banners of their own. So
/// this screen is the only place that shows **all of it together**.
#[tauri::command]
pub fn doctor_report() -> Result<DoctorReportDto, CmdError> {
    let store = open_store_read()?;
    let result = amenbo_core::doctor::report(&store)?;
    Ok(DoctorReportDto {
        ok: result.ok,
        errors: result.summary.error,
        warnings: result.summary.warning,
        issues: result.issues.iter().map(DoctorIssueDto::from).collect(),
    })
}

/// What `doctor_fix` returns: what was cleaned up, and how much of it.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DoctorFixDto {
    reclaimed_blobs: usize,
    freed_bytes: usize,
    forgotten_bindings: usize,
}

/// Run the repair from the GUI, calling **the same core cleanup entry points** as the CLI's
/// `doctor --fix`, in the same order. Every one of them is **non-destructive** (blobs nothing
/// references; folder rows nobody claims), so the surface may run it without asking for
/// confirmation. And since nothing it cleans up is referenced by a single row of any live read,
/// there is no snapshot and no query to refetch — hence no `WriteAck`.
#[tauri::command]
pub fn doctor_fix() -> Result<DoctorFixDto, CmdError> {
    let store = open_store()?;
    let gc = store.gc_blobs(amenbo_core::blob::GC_MIN_AGE)?;
    let forgotten_bindings = store.forget_orphan_dirs()?;
    Ok(DoctorFixDto {
        reclaimed_blobs: gc.removed as usize,
        freed_bytes: gc.freed_bytes as usize,
        forgotten_bindings,
    })
}

/// The shortest path to an update: open this OS's all-in-one installer (GUI and CLI together) in the
/// OS's default browser. Core resolves the installer URL for the current platform from the published
/// `latest.json` — falling back to the latest release page when it has not been fetched, is not
/// listed, or the check is disabled by the environment — and `os_open` opens it. There is no
/// self-update; it only opens. Because this is an explicit user action (the button on the update
/// banner), it goes and fetches regardless of the update_check toggle in config. Returns the URL it
/// opened, which the front end can display or log. The store is untouched: the only side effect is
/// launching an external browser.
#[tauri::command]
pub fn open_latest_installer() -> Result<String, CmdError> {
    let url = amenbo_core::update_check::resolve_update_url();
    os_open(&url).map_err(|e| -> CmdError { format!("インストーラ URL を開けません: {e}").into() })?;
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use amenbo_core::model::View;
    use std::sync::Mutex;

    /// These tests all swap out AMENBO_HOME, which is shared across the process, so they are
    /// serialized to keep parallel runs from treading on each other.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// When the target is gone and no ledger row carrying its name can be recovered either (dropped
    /// in compaction, or beyond the lookback budget), core returns an empty title. Rendering that as
    /// it is would leave empty quotation marks on screen, so the DTO's entry point says the target
    /// is deleted — in **both** the event line and the destination label, in the right language.
    #[test]
    fn a_subject_whose_name_is_gone_is_named_deleted_not_left_blank() {
        let nameless = |title: &str| amenbo_core::activity::Item {
            id: 1,
            at: Timestamp::now(),
            kind: amenbo_core::activity::Kind::System,
            author_kind: Some(ActorKind::Ai),
            target_type: amenbo_core::activity::TargetType::Task,
            target_id: 42,
            title: title.to_string(),
            target_live: false,
            event: Some(serde_json::json!({"kind": "task.status_changed", "new": "done"})),
            text: None,
            edited_at: None,
        };
        let config = amenbo_core::config::Config::default();

        let ja = activity_dto(nameless(""), "ja", &config);
        assert_eq!(ja.target.title, "（削除済み）");
        assert_eq!(ja.event.unwrap().text, "「（削除済み）」を完了に変更");

        let en = activity_dto(nameless(""), "en", &config);
        assert_eq!(en.target.title, "(deleted)");
        assert_eq!(en.event.unwrap().text, "Changed “(deleted)” to Done");

        let alive = activity_dto(nameless("生きているタスク"), "ja", &config);
        assert_eq!(alive.target.title, "生きているタスク");
        assert_eq!(alive.event.unwrap().text, "「生きているタスク」を完了に変更");
    }

    /// Deleting a project or a decision also lands in the ledger (`activity_log::event`). Without a
    /// branch for it, the line falls through to the default "updated" and reports a deletion as an
    /// update.
    #[test]
    fn deleting_a_project_or_a_decision_is_told_as_a_deletion() {
        let project = |tasks: u64, decisions: u64| {
            serde_json::json!({"kind": "project.deleted", "name": "旧サイト", "tasks": tasks, "decisions": decisions})
        };
        let decision = serde_json::json!({"kind": "decision.deleted", "title": "旧方針の決定"});

        assert_eq!(render_event(&project(4, 1), "旧サイト", "ja").text, "「旧サイト」を削除（タスク4件・決定1件）");
        assert_eq!(render_event(&project(4, 1), "旧サイト", "en").text, "Deleted “旧サイト” (4 tasks, 1 decision)");

        assert_eq!(render_event(&project(0, 0), "空の PJ", "ja").text, "「空の PJ」を削除");
        assert_eq!(render_event(&project(0, 0), "空の PJ", "en").text, "Deleted “空の PJ”");

        assert_eq!(render_event(&decision, "旧方針の決定", "ja").text, "「旧方針の決定」を削除");
        assert_eq!(render_event(&decision, "旧方針の決定", "en").text, "Deleted “旧方針の決定”");
    }

    /// The tests' env guard. It takes ENV_LOCK to serialize, and disables the update check so the
    /// `build_snapshot` path talks to no upstream and touches no real OS cache — hermetic. Every
    /// test that goes through a snapshot goes through this.
    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        let g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("AMENBO_UPDATE_CHECK", "0");
        g
    }

    /// In WAL mode, an external writer's commit (the CLI, the AI) lands only in `store.sqlite-wal`,
    /// and the mtime of `store.sqlite` itself does not move until a checkpoint. The change signature
    /// rests on **`PRAGMA data_version`**, so a write from another process always moves it, even
    /// though the main file was never touched — pinned here against a real store.
    #[test]
    fn store_signature_moves_on_an_external_writers_wal_only_commit() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-wal-sig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let before = store_signature_string();
        assert!(!before.is_empty(), "a signature is produced when a store exists");

        {
            let mut writer = Store::open().unwrap();
            writer
                .add_task(amenbo_core::ops::task::NewTask {
                    title: "外から届いたタスク".into(),
                    project_id: Some(project_id),
                    due_on: None,
                    start_on: None,
                    priority: None,
                    notes: String::new(),
                    created_by_kind: Some(ActorKind::Ai),
                })
                .unwrap();
        }

        assert_ne!(before, store_signature_string(), "an external writer's commit moves the signature");

        assert!(!store_signature_string().contains('|'), "does not mix in the `|` separator");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Pins that one wing of the reservation guard — **a reservation is refused while its premises
    /// are unmet** — holds on the GUI path too. `task_status` **passes core's error straight
    /// through**, and the front end's mutator puts the exception in a toast (`run` in `store.tsx`),
    /// so as long as `code` stays `not_ready` and `message` / `message_en` state **the reason and
    /// the way out**, a drop on the kanban board becomes a toast that says why. Let that slip and,
    /// from the GUI, it turns into "I dragged it and it silently snapped back". A card's column is
    /// drawn from the source of truth (`status`), so a refused reservation never moved the column in
    /// the first place — no optimistic-update rollback is needed. Also pins that the status does not
    /// regress.
    #[test]
    fn task_status_surfaces_not_ready_with_its_reason() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-notready-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };

        let blocker = task_add(Some(project_id), "先行".into(), None).unwrap().tasks[0];
        let dependent = task_add(Some(project_id), "後続".into(), None).unwrap().tasks[0];
        {
            let mut store = Store::open().unwrap();
            store.depend_task(dependent, blocker, Some(ActorKind::Human)).unwrap();
        }

        let err = task_status(dependent, "in_progress".into())
            .err()
            .expect("reservation is rejected when the premise is unmet");
        assert_eq!(err.code, "not_ready", "code reaches the webview as not_ready");
        assert!(err.message.contains("先行"), "the Japanese reason names the blocker task: {}", err.message);
        assert!(err.message_en.contains("blocker"), "the English reason names it too: {}", err.message_en);

        let card = |id: i64| tasks_by_ids(vec![id]).unwrap().into_iter().next().unwrap();
        assert_eq!(card(dependent).status, "todo", "a rejected reservation does not move the column (no rollback needed)");

        task_done(blocker, true).unwrap();
        task_status(dependent, "in_progress".into()).expect("reservation succeeds once the premise clears");
        assert_eq!(card(dependent).status, "in_progress");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The reason a reservation was refused shows up only in a toast that vanishes in seconds. The
    /// card holds the same fact permanently and names **which decision is holding it down** — the
    /// detail pane draws that as a clickable affordance, and the ref leads somewhere from the card,
    /// not from the toast. The unsettled premises are a subset of `linked_decisions`; settle them
    /// and they disappear and `ready` goes up.
    #[test]
    fn task_card_names_the_unsettled_premise_that_holds_ready_down() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-premise-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };

        let task = task_add(Some(project_id), "実装".into(), None).unwrap().tasks[0];
        let did = decision_add(project_id, "決めごと".into(), Some("結論".into())).unwrap().decisions[0];
        decision_set_link(did, task, true).unwrap();

        let card = |id: i64| tasks_by_ids(vec![id]).unwrap().into_iter().next().unwrap();

        let c = card(task);
        assert!(!c.ready, "ready stays false while the basis is unsettled");
        assert_eq!(c.blocked_by_decisions.len(), 1, "names the decision it is held on");
        assert_eq!(c.blocked_by_decisions[0].id, did, "the detail pane can navigate by that decision's id");
        assert!(c.blocked_by_decisions[0].r#ref.is_some(), "the conversational ref (D-n) is carried too");
        assert_eq!(c.linked_decisions.len(), 1, "an unsettled premise is a subset of linked_decisions");
        assert!(task_status(task, "in_progress".into()).is_err(), "reservation is rejected");

        decision_accept(did).unwrap();
        let c = card(task);
        assert!(c.ready, "ready once the basis is settled");
        assert!(c.blocked_by_decisions.is_empty(), "a settled premise no longer holds it back");
        assert_eq!(c.linked_decisions.len(), 1, "the link itself remains (traceability)");
        task_status(task, "in_progress".into()).expect("reservation succeeds once the premise settles");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Round-trips the write commands against an isolated store and checks that they land in the
    /// snapshot. What is under test is the wiring — args, emit, save, projection; the core ops
    /// themselves are already tested in core.
    #[test]
    fn write_commands_round_trip() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let (project_id, me_user) = {
            let mut store = Store::open().unwrap();
            let me = ActorKind::Human.as_str().to_string();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                },
            )
            .unwrap();
            (p.id, me)
        };

        let ack = task_add(Some(project_id), "結線テスト".into(), None).unwrap();
        assert_eq!(ack.tasks.len(), 1, "task_add returns the new task id");
        assert!(ack.scopes.contains(&"tasks"), "task_add invalidates the task lists");
        let id = ack.tasks[0];
        let card = |id: i64| tasks_by_ids(vec![id]).unwrap().into_iter().next();
        let t = card(id).expect("task added");
        assert_eq!(t.project_id, Some(project_id));
        assert_eq!(t.status, "todo");
        assert_eq!(t.created_by.as_ref().map(|a| a.kind), Some("human"));
        assert_eq!(t.id, 1, "the id is the conversational number");
        assert_eq!(t.r#ref, "AMB-T-1", "ref is the namespaced form (D-272)");

        let ack = task_status(id, "in_progress".into()).unwrap();
        assert_eq!(ack.tasks, vec![id], "status acks the task");
        assert!(ack.scopes.contains(&"tasks"), "status invalidates the task lists");
        let t = card(id).unwrap();
        assert_eq!(t.status, "in_progress", "todo→in_progress reserves it");

        let err = task_status(id, "in_progress".into())
            .err()
            .expect("re-reserving an already in_progress task must be rejected");
        assert_eq!(err.code, "already_reserved", "GUI double-reserve is rejected via the core CAS");
        assert_eq!(card(id).unwrap().status, "in_progress", "rejected reserve does not regress the status");

        let _ = task_status(id, "blocked".into()).unwrap();
        assert_eq!(card(id).unwrap().status, "blocked");

        let ack = comment_add(id, "コメント".into()).unwrap();
        assert_eq!(ack.tasks, vec![id], "comment acks its task");
        assert_eq!(card(id).unwrap().comments, 1);

        let comment_id = {
            let store = Store::open().unwrap();
            store.comment_list(id, None, None).unwrap().comments[0].id
        };
        let ack = decision_promote(comment_id, "昇格した決定".into()).unwrap();
        let did = ack.decisions[0];
        let promoted = decisions_by_ids(vec![did]).unwrap().into_iter().next().unwrap();
        assert_eq!(promoted.body, "コメント", "promoted decision body is the task_comment text");
        assert!(promoted.linked_tasks.iter().any(|l| l.id == id), "promoted decision links its task");

        let _ = task_done(id, true).unwrap();
        assert_eq!(card(id).unwrap().status, "done");

        let snap = snapshot().unwrap();
        assert!(snap.activity.iter().any(|a| a.kind == "system"), "system events emitted");
        assert!(snap.activity.iter().any(|a| a.kind == "comment"), "comment in activity");

        let ack = dimension_add(project_id, "軸2".into()).unwrap();
        assert!(ack.scopes.contains(&"tasks"), "dimension change invalidates the board lists");
        let proj_snap = snapshot().unwrap();
        let proj = proj_snap.projects.iter().find(|p| p.id == project_id).unwrap();
        assert!(proj.dimensions.iter().any(|d| d.name == "軸2"), "dimension added");

        let del_id = task_add(Some(project_id), "消す対象".into(), None).unwrap().tasks[0];
        let ack = task_delete(del_id).unwrap();
        assert_eq!(ack.tasks, vec![del_id], "delete acks the removed task");
        assert!(card(del_id).is_none(), "deleted task drops from the list");
        let snap = snapshot().unwrap();
        let deleted = snap
            .activity
            .iter()
            .find(|a| a.event.as_ref().is_some_and(|e| e.kind == "task.deleted"))
            .expect("the deletion is on the timeline");
        assert_eq!(deleted.kind, "system");
        assert_eq!(deleted.target.id, del_id);
        assert!(
            deleted.event.as_ref().unwrap().text.contains("消す対象"),
            "a deleted row's name lives only in the ledger payload (the DB cannot join to it)"
        );

        let ack = task_assign(id, Some(me_user.clone()), "ai".into()).unwrap();
        assert!(ack.scopes.contains(&"tasks"), "assign invalidates the assignee-filtered lists");
        let t = card(id).unwrap();
        assert_eq!(t.assignee.as_ref().map(|a| a.kind), Some("ai"), "delegated to my AI");
        let _ = task_assign(id, None, "human".into()).unwrap();
        assert!(card(id).unwrap().assignee.is_none(), "unassigned");
        let snap = snapshot().unwrap();
        assert!(snap.activity.iter().any(|a| a.kind == "system" && a.event.as_ref().map(|e| e.kind == "task.assigned").unwrap_or(false)), "assigned event emitted");

        let sig_before = store_signature();
        let _ = task_add(Some(project_id), "シグネチャ確認".into(), None).unwrap();
        assert!(!sig_before.is_empty(), "store signature is non-empty when a store exists");
        assert_ne!(store_signature(), sig_before, "a write advances the store signature");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Checks the decision-comment wiring end to end against an isolated store.
    /// `decision_comment_add` writes to the dedicated `decision_comment` table, and
    /// `decision_comments` reads back DTOs, oldest first, carrying the author's facet and the
    /// relative time. The ack invalidates the decisions scope and the target decision — what makes
    /// the GUI refetch the thread. Reading an unknown decision is empty; posting to one is an error.
    #[test]
    fn decision_comment_commands_round_trip() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-deccomment-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "決定PJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                },
            )
            .unwrap();
            p.id
        };
        let did = decision_add(project_id, "決めごと".into(), Some("結論".into()))
            .unwrap()
            .decisions[0];

        assert!(decision_comments(did).unwrap().is_empty(), "no comments initially");

        let ack = decision_comment_add(did, "一言目".into()).unwrap();
        assert_eq!(ack.decisions, vec![did], "comment_add acks its decision");
        assert!(ack.scopes.contains(&"decisions"), "invalidates the decision views");
        let _ = decision_comment_add(did, "二言目".into()).unwrap();

        let comments = decision_comments(did).unwrap();
        assert_eq!(comments.len(), 2, "both comments read back");
        assert_eq!(comments[0].text, "一言目", "oldest first");
        assert_eq!(comments[1].text, "二言目");
        assert_eq!(comments[0].author.kind, "human", "human facet author");
        assert!(!comments[0].ago.is_empty(), "relative time label is populated");

        let rm = decision_comment_remove(comments[0].id, did).unwrap();
        assert_eq!(rm.decisions, vec![did], "comment_remove acks its decision");
        assert!(rm.scopes.contains(&"decisions"), "invalidates the decision views");
        let left = decision_comments(did).unwrap();
        assert_eq!(left.len(), 1, "only the deleted comment is gone");
        assert_eq!(left[0].text, "二言目");
        assert!(decision_comment_remove(9999, did).is_ok(), "removing a gone comment is a noop");

        assert!(decision_comments(9999).unwrap().is_empty(), "unknown decision reads empty");
        assert!(decision_comment_add(9999, "x".into()).is_err(), "unknown decision rejects a comment");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Pins that the decision card (`decisions_by_ids` → `decision_card_row` →
    /// `decision_card_from_row`) carries every one of its cross-link fields. Everything the decision
    /// detail pane draws — the supersession chain, amendments, premises and their rot, the status of
    /// the work it spawned — rides on this one DTO.
    #[test]
    fn decision_card_carries_every_cross_link() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-deccard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "決定PJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let add = |title: &str| decision_add(project_id, title.into(), Some("結論".into())).unwrap().decisions[0];
        let card = |id: i64| decisions_by_ids(vec![id]).unwrap().into_iter().next().unwrap();

        let old = add("UTC で保存する");
        let partial = add("端では現地時刻で出す");
        let premise = add("台帳は末尾から読む");
        let head = add("整数キーで持つ");

        decision_supersede(head, old).unwrap();
        decision_amend(head, partial).unwrap();
        decision_builds_on(head, premise).unwrap();
        let shipped = task_add(Some(project_id), "整数キーへ移行".into(), None).unwrap().tasks[0];
        let pending = task_add(Some(project_id), "GUI を追従させる".into(), None).unwrap().tasks[0];
        decision_set_link(head, shipped, true).unwrap();
        decision_set_link(head, pending, true).unwrap();
        task_done(shipped, true).unwrap();

        let c = card(head);
        assert_eq!(c.r#ref, amenbo_core::idref::decision(head), "the conversational ref is the display form of the id");
        assert!(c.current, "a decision replaced by nothing is current");
        assert_eq!(c.status, "accepted", "supersede promotes the drawing side to accepted");
        assert!(c.decided_at.is_some(), "an accepted decision has a decided-on date");
        assert!(!c.decided_by.as_ref().unwrap().name.is_empty(), "who decided is carried too");
        assert_eq!(c.supersedes.len(), 1, "the decision it superseded");
        assert_eq!(c.supersedes[0].id, old);
        assert_eq!(c.supersedes[0].r#ref, Some(amenbo_core::idref::decision(old)), "carries the other side's ref too");
        assert_eq!(c.amends.len(), 1, "the decision it partly amended");
        assert_eq!(c.amends[0].id, partial);
        assert_eq!(c.builds_on.len(), 1, "the decision it builds on");
        assert_eq!(c.builds_on[0].id, premise);
        assert!(c.builds_on[0].superseded_by.is_none(), "no rot note when the premise is current");
        assert!(c.superseded_by.is_empty(), "the reverse lookup is still empty");
        assert!(c.amended_by.is_empty());
        assert!(c.built_on_by.is_empty());

        let mut linked: Vec<_> = c.linked_tasks.iter().map(|t| (t.id, t.status.as_str())).collect();
        linked.sort();
        assert_eq!(linked, vec![(shipped, "done"), (pending, "todo")], "remaining work and finished work");
        assert!(c.linked_tasks[0].r#ref.is_some(), "the task's conversational ref is carried too (the detail pane uses it to navigate)");

        let c_old = card(old);
        assert_eq!(c_old.superseded_by.iter().map(|r| r.id).collect::<Vec<_>>(), vec![head]);
        assert!(!c_old.current, "a superseded decision is no longer current");
        assert_eq!(card(partial).amended_by.iter().map(|r| r.id).collect::<Vec<_>>(), vec![head]);
        assert!(card(partial).current, "the target stays current even when amended");
        assert_eq!(card(premise).built_on_by.iter().map(|r| r.id).collect::<Vec<_>>(), vec![head], "the radius of impact");

        let killer = add("台帳は先頭から読む");
        decision_supersede(killer, premise).unwrap();
        let c = card(head);
        assert_eq!(
            c.builds_on[0].superseded_by,
            Some(amenbo_core::idref::decision(killer)),
            "surfaces the decision standing on the rotted premise",
        );

        decision_unlink_edge(head, old).unwrap();
        assert!(card(head).supersedes.is_empty(), "an unlinked edge disappears from the card");
        assert!(card(old).current, "unlinking supersedes returns the target to current (no cleanup)");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Checks taking back a task comment (deleting it for good) at the GUI command layer. A comment
    /// added with `comment_add` is removed by `comment_remove` and drops out of the task's activity,
    /// which is where the GUI's comment list comes from. The ack has the same scope as `comment_add`
    /// (tasks plus the target task), so the card's comment count is refetched too.
    #[test]
    fn comment_remove_drops_it_from_the_task_activity() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-commentrm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let _ = project_add("PJ".into()).unwrap();
        let project_id = snapshot().unwrap().projects[0].id;
        let tid = task_add(Some(project_id), "コメントを消す".into(), None).unwrap().tasks[0];

        let _ = comment_add(tid, "誤投稿".into()).unwrap();
        let _ = comment_add(tid, "残すコメント".into()).unwrap();
        let comments = |id: i64| -> Vec<ActivityItemDto> {
            task_activity(id, None).unwrap().into_iter().filter(|a| a.kind == "comment").collect()
        };
        let before = comments(tid);
        assert_eq!(before.len(), 2, "both comments are on the timeline");
        let mistaken = before.last().unwrap();
        assert_eq!(mistaken.text.as_deref(), Some("誤投稿"));

        let ack = comment_remove(mistaken.id, tid).unwrap();
        assert_eq!(ack.tasks, vec![tid], "comment_remove acks its task (the card's comment count moves)");
        assert!(ack.scopes.contains(&"tasks"), "invalidates the lists");

        let left = comments(tid);
        assert_eq!(left.len(), 1, "only the deleted comment is gone");
        assert_eq!(left[0].text.as_deref(), Some("残すコメント"));
        assert!(comment_remove(9999, tid).is_ok(), "removing a gone comment is a noop");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A comment can be fixed in place — not deleted and reposted. The id does not change, and its
    /// position on the timeline does not move. The ack has the same scope as `comment_add`.
    #[test]
    fn comment_edit_rewrites_the_body_in_place() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-commentedit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let _ = project_add("PJ".into()).unwrap();
        let project_id = snapshot().unwrap().projects[0].id;
        let tid = task_add(Some(project_id), "コメントを直す".into(), None).unwrap().tasks[0];
        let _ = comment_add(tid, "誤字のある投稿".into()).unwrap();

        let posted: Vec<ActivityItemDto> =
            task_activity(tid, None).unwrap().into_iter().filter(|a| a.kind == "comment").collect();
        let cid = posted[0].id;

        let ack = comment_edit(cid, tid, "直した投稿".into()).unwrap();
        assert_eq!(ack.tasks, vec![tid], "comment_edit acks its task");
        assert!(ack.scopes.contains(&"tasks"), "invalidates the lists");

        let after: Vec<ActivityItemDto> =
            task_activity(tid, None).unwrap().into_iter().filter(|a| a.kind == "comment").collect();
        assert_eq!(after.len(), 1, "editing does not post a second comment");
        assert_eq!(after[0].id, cid, "the id survives the edit");
        assert_eq!(after[0].text.as_deref(), Some("直した投稿"));
        assert!(comment_edit(9999, tid, "x".into()).is_err(), "editing a gone comment is an error");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `project_add` brings a store into being and puts one project row in it. A bare `Store::init`
    /// leaves no project row, so nothing appears in the snapshot (which comes from
    /// `project_overview`) and the sidebar stays empty. Checks the creation on disk, the ack, and
    /// **that it becomes visible in the snapshot** — the evidence that the wiring holds.
    #[test]
    fn project_add_provisions_store() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-projadd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let ack = project_add("新規プロジェクト".into()).unwrap();
        assert!(ack.scopes.contains(&"tasks"), "project_add invalidates the board/project lists");

        let engine = amenbo_core::config::Paths::resolve().unwrap().store_file;
        assert!(engine.is_file(), "the store is created on disk at {}", engine.display());

        let snap = snapshot().unwrap();
        assert!(
            snap.projects.iter().any(|p| p.name == "新規プロジェクト"),
            "the created project is visible in the sidebar projection"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Round-trips the project settings commands: `project_get` returns the fields for the prefill,
    /// `project_update` applies the delta (name/notes/color/view), `project_set_archived` takes the
    /// project out of the snapshot (`project_overview` — live and not archived) and brings it back,
    /// and `project_delete` destroys it for good. The evidence that the wiring holds.
    #[test]
    fn project_settings_round_trip_via_command() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-projset-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        project_add("設定PJ".into()).unwrap();
        let project_id = snapshot()
            .unwrap()
            .projects
            .iter()
            .find(|p| p.name == "設定PJ")
            .expect("created project is visible")
            .id;

        let got = project_get(project_id).unwrap();
        assert_eq!(got.name, "設定PJ");
        assert_eq!(got.notes, "");
        assert!(!got.archived);

        let ack = project_update(
            project_id,
            Some("改名PJ".into()),
            Some("メモ本文".into()),
            None,
            Some("list".into()),
        )
        .unwrap();
        assert!(ack.scopes.contains(&"tasks"), "project_update invalidates the board/project lists");
        let got = project_get(project_id).unwrap();
        assert_eq!(got.name, "改名PJ", "rename persisted");
        assert_eq!(got.notes, "メモ本文", "notes persisted");
        assert_eq!(got.view, "list", "default view persisted");

        assert!(
            project_update(project_id, None, None, None, Some("kanban".into())).is_err(),
            "an invalid view is rejected"
        );

        project_set_archived(project_id, true).unwrap();
        assert!(
            !snapshot().unwrap().projects.iter().any(|p| p.id == project_id),
            "archived project drops out of the sidebar projection"
        );
        assert!(
            project_list_archived().unwrap().iter().any(|p| p.id == project_id),
            "archived project appears in the archived read path"
        );
        assert!(project_get(project_id).unwrap().archived, "get still reads the archived project");

        project_set_archived(project_id, false).unwrap();
        assert!(
            snapshot().unwrap().projects.iter().any(|p| p.id == project_id),
            "unarchived project returns to the sidebar projection"
        );
        assert!(
            !project_list_archived().unwrap().iter().any(|p| p.id == project_id),
            "unarchived project leaves the archived read path"
        );

        project_delete(project_id).unwrap();
        assert!(
            !snapshot().unwrap().projects.iter().any(|p| p.id == project_id),
            "deleted project is gone from the projection"
        );
        assert!(project_get(project_id).is_err(), "get of a deleted project errors");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The `project_move` command wires through to `project.order_key` (`Store::project_move`) and
    /// changes the order of the sidebar projection (`build_snapshot` stacks projects in `order_key`
    /// order). Creates three projects — appended at the bottom, so they come out in creation
    /// order — reorders them with `before`, `top` and `bottom`, and checks that the snapshot's order
    /// moves with them. This is what drag-and-drop rests on. An invalid position, or a missing
    /// anchor, is refused.
    #[test]
    fn project_move_reorders_via_command() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-projmove-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let (a, b, c) = {
            let mut store = Store::open().unwrap();
            let mk = |store: &mut Store, name: &str| {
                store.project_add(
                    amenbo_core::ops::project::NewProject {
                        name: name.into(),
                        view: View::List,
                        notes: String::new(),
                        color: None,
                    },
                )
                .unwrap()
                .id
            };
            let a = mk(&mut store, "A");
            let b = mk(&mut store, "B");
            let c = mk(&mut store, "C");
            (a, b, c)
        };

        let order = || -> Vec<i64> { snapshot().unwrap().projects.iter().map(|p| p.id).collect() };
        assert_eq!(order(), vec![a, b, c], "initial order is creation (bottom) order");

        let ack = project_move(c, "before".into(), Some(a)).unwrap();
        assert!(ack.scopes.contains(&"tasks"), "project_move invalidates the board/project lists");
        assert_eq!(order(), vec![c, a, b], "C moved before A");

        project_move(a, "bottom".into(), None).unwrap();
        assert_eq!(order(), vec![c, b, a], "A moved to the bottom");

        project_move(b, "top".into(), None).unwrap();
        assert_eq!(order(), vec![b, c, a], "B moved to the top");

        assert!(project_move(a, "sideways".into(), None).is_err(), "an invalid position is rejected");
        assert!(project_move(a, "before".into(), None).is_err(), "before without an anchor is rejected");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Round-trips per-comment attachments at the command layer. `attachment_add_bytes` with
    /// `target_type="task_comment"` hangs an attachment off a comment id, `attachments_for` reads it
    /// back by the same id, and it **never bleeds into the task body's attachments** — they are
    /// different targets. The ack puts the comment id in `tasks` so the attachments query gets
    /// invalidated (`applyAck` matches on `["attachments", type, id]`).
    #[test]
    fn comment_attachment_round_trips_via_command() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-cattach-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "添付PJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                },
            )
            .unwrap();
            p.id
        };
        let task_id = task_add(Some(project_id), "添付親タスク".into(), None).unwrap().tasks[0];
        comment_add(task_id, "添付を付けるコメント".into()).unwrap();
        let comment_id = {
            let store = Store::open().unwrap();
            store.comment_list(task_id, None, None).unwrap().comments[0].id
        };

        let ack = attachment_add_bytes(
            "task_comment".into(),
            comment_id,
            "note.txt".into(),
            b"hello".to_vec(),
        )
        .unwrap();
        assert!(
            ack.tasks.contains(&comment_id),
            "comment attach acks the comment id for attachments invalidation"
        );

        let on_comment = attachments_for("task_comment".into(), comment_id).unwrap();
        assert_eq!(on_comment.len(), 1, "the comment carries its own attachment");
        assert_eq!(on_comment[0].filename.as_deref(), Some("note.txt"));
        let on_body = attachments_for("task".into(), task_id).unwrap();
        assert!(on_body.is_empty(), "the comment attachment does not bleed into the task body");

        attachment_remove(on_comment[0].id, "task_comment".into(), comment_id).unwrap();
        assert!(attachments_for("task_comment".into(), comment_id).unwrap().is_empty(), "removed comment attachment is gone");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `project_add_folder` turns the chosen folder into a new project: (1) the project row, named
    /// after the folder, appears in the snapshot, (2) a `.amenbo` pointer is written into the folder,
    /// and (3) a folder that already has a `.amenbo` is refused with `init_pointer_exists`. The
    /// native folder picker cannot be driven from a Rust test, so the command itself (which takes a
    /// dir argument) is called directly to check the wiring: guard, creation, pointer.
    #[test]
    fn project_add_folder_inits_visible_project_and_guards() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-projfolder-home-{}", std::process::id()));
        let dir = std::env::temp_dir().join(format!("amenbo-app-projfolder-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        project_add_folder(dir.to_string_lossy().to_string(), None).unwrap();

        assert!(dir.join(".amenbo").is_file(), ".amenbo pointer is written into the folder");

        let folder_name = dir.file_name().unwrap().to_string_lossy().to_string();
        let snap = snapshot().unwrap();
        assert!(
            snap.projects.iter().any(|p| p.name == folder_name),
            "the folder-init project (named after the folder) is visible"
        );

        match project_add_folder(dir.to_string_lossy().to_string(), None) {
            Ok(_) => panic!("re-init on a bound folder must be rejected"),
            Err(e) => assert_eq!(e.code, "init_pointer_exists", "re-init on a bound folder is rejected"),
        }

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A folder with no `.amenbo` but an amenbo managed block — a stale marker left by a clone, a
    /// copy, or debris — is **not hard-blocked**. A marker is no proof of ownership, so when no
    /// living project in the registry claims it, init carries on: it brings a project into being and
    /// writes the pointer.
    #[test]
    fn project_add_folder_marker_only_continues_and_recovers_the_pointer() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-markeronly-home-{}", std::process::id()));
        let dir = std::env::temp_dir().join(format!("amenbo-app-markeronly-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        amenbo_core::agents::upsert_into_dir(&dir, None, amenbo_core::config::Paths::APP_NAME);
        assert!(amenbo_core::agents::dir_has_managed_block(&dir), "precondition: a borrowed managed block is present");
        assert!(!dir.join(".amenbo").is_file(), "precondition: no owning pointer yet");

        project_add_folder(dir.to_string_lossy().to_string(), None).unwrap();
        assert!(dir.join(".amenbo").is_file(), "the pointer is written after continuing init");

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The input step of the creation screen is "name required, folder optional". Checks that when a
    /// folder is bound, the `name` the front end passes is what the project is named — **not the
    /// folder's name** — with surrounding whitespace trimmed.
    #[test]
    fn project_add_folder_uses_provided_name_over_basename() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-projfolder-name-home-{}", std::process::id()));
        let dir = std::env::temp_dir().join(format!("amenbo-app-projfolder-name-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        project_add_folder(dir.to_string_lossy().to_string(), Some("  マイPJ  ".to_string())).unwrap();

        let folder_name = dir.file_name().unwrap().to_string_lossy().to_string();
        let snap = snapshot().unwrap();
        assert!(
            snap.projects.iter().any(|p| p.name == "マイPJ"),
            "the project is named after the provided name (trimmed), not the folder"
        );
        assert!(
            !snap.projects.iter().any(|p| p.name == folder_name),
            "the folder basename is not used when a name is provided"
        );

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Folder management in project settings, round-tripped: bind an existing folder to an existing
    /// project (`project_bind_folder`), list it by reverse lookup (`project_bound_folders`), then
    /// unbind it (`project_unbind_folder`). The native folder picker cannot be driven from Rust, so
    /// the commands themselves (taking a dir argument) are called directly to check that (1) bind
    /// places `.amenbo` and the AI guidance managed block and one row appears in the reverse lookup,
    /// (2) a nested binding is refused, (3) a folder that does not exist is refused, and (4) unbind
    /// removes the pointer and the managed block and the row leaves the reverse lookup, while the
    /// store itself remains.
    #[test]
    fn project_bind_unbind_folder_round_trips() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-bindfolder-home-{}", std::process::id()));
        let dir = std::env::temp_dir().join(format!("amenbo-app-bindfolder-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "紐付けPJ".into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                },
            )
            .unwrap();
            p.id
        };

        match project_bind_folder(project_id, dir.join("does-not-exist").to_string_lossy().to_string()) {
            Ok(_) => panic!("binding a non-existent folder must be rejected"),
            Err(e) => assert_eq!(e.code, "not_found", "a missing folder is rejected"),
        }

        project_bind_folder(project_id, dir.to_string_lossy().to_string()).unwrap();
        assert!(dir.join(".amenbo").is_file(), ".amenbo pointer is written into the bound folder");
        assert!(amenbo_core::agents::dir_has_managed_block(&dir), "bind upserts the AI guidance managed block");
        let listed = project_bound_folders(project_id).unwrap();
        assert_eq!(listed.len(), 1, "the reverse lookup shows exactly the bound folder");
        assert!(listed[0].exists, "an existing bound folder is flagged AI-ready (exists)");

        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        match project_bind_folder(project_id, sub.to_string_lossy().to_string()) {
            Ok(_) => panic!("binding a subfolder of a managed tree must be rejected"),
            Err(e) => assert_eq!(e.code, "binding_nested_tree", "a nested binding is rejected"),
        }

        project_unbind_folder(dir.to_string_lossy().to_string()).unwrap();
        assert!(!dir.join(".amenbo").is_file(), "unbind removes the .amenbo pointer");
        assert!(!amenbo_core::agents::dir_has_managed_block(&dir), "unbind strips the managed block");
        assert!(project_bound_folders(project_id).unwrap().is_empty(), "the folder is gone from the reverse lookup");
        assert!(
            snapshot().unwrap().projects.iter().any(|p| p.id == project_id),
            "unbind keeps the project (store) — it only detaches the folder"
        );

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// When a bound folder loses its `.amenbo`, the list says so (`pointer_missing`). The registry
    /// still names this project, so the row stays, but an AI in that folder no longer resolves here.
    /// Round-trips all the way through a relink (`project_bind_folder`), which writes the pointer
    /// back and clears the flag.
    #[test]
    fn a_bound_folder_that_lost_its_pointer_says_so() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-nopointer-home-{}", std::process::id()));
        let dir = std::env::temp_dir().join(format!("amenbo-app-nopointer-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "ポインタ喪失PJ".into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        project_bind_folder(project_id, dir.to_string_lossy().to_string()).unwrap();
        let bound = project_bound_folders(project_id).unwrap();
        assert!(!bound[0].pointer_missing, "a freshly bound folder has its pointer");

        std::fs::remove_file(dir.join(".amenbo")).unwrap();
        let lost = project_bound_folders(project_id).unwrap();
        assert_eq!(lost.len(), 1, "the folder still shows up (the registry still points here)");
        assert!(lost[0].exists, "the folder itself is not stale — only the pointer is gone");
        assert!(lost[0].pointer_missing, "the missing pointer is reported instead of passing as AI-ready");
        assert!(lost[0].mismatch.is_none() && !lost[0].legacy, "with no pointer there is nothing to inspect");

        project_bind_folder(project_id, dir.to_string_lossy().to_string()).unwrap();
        let relinked = project_bound_folders(project_id).unwrap();
        assert!(dir.join(".amenbo").is_file(), "relink writes the pointer back");
        assert!(!relinked[0].pointer_missing, "the relinked folder is AI-ready again");

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every row of the list inspects its own `.amenbo` (`mismatch` and `legacy`). The verdicts
    /// themselves (core's `slug_mismatch` and `is_legacy_pointer`) are covered by core's own tests,
    /// so what is pinned here is **whether the command assembles the row correctly**: a current
    /// pointer written by bind says nothing, a pointer carried over from another store reports the
    /// disagreement along with the recorded slug and the real one, and an old-format pointer comes
    /// back as `legacy`. In none of these cases does the listing stop — the id is authoritative.
    #[test]
    fn bound_folder_rows_inspect_their_pointer() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-pointerscan-home-{}", std::process::id()));
        let dir = std::env::temp_dir().join(format!("amenbo-app-pointerscan-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        let (project_id, slug) = {
            let mut store = Store::open().unwrap();
            let id = store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "検分PJ".into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id;
            (id, store.project(id).unwrap().unwrap().slug)
        };
        assert!(slug.is_some(), "a project carries a slug — it is the material the pointer is checked against");
        project_bind_folder(project_id, dir.to_string_lossy().to_string()).unwrap();

        let row = project_bound_folders(project_id).unwrap().remove(0);
        assert!(row.mismatch.is_none(), "a pointer written by bind matches the store");
        assert!(!row.legacy, "a pointer written by bind is the current format");
        assert!(!row.pointer_missing, "the pointer is there");

        amenbo_core::binding::DirBinding::new(Some(project_id), Some("wharfy".into())).write(&dir).unwrap();
        let row = project_bound_folders(project_id).unwrap().remove(0);
        let mismatch = row.mismatch.expect("a pointer from another store is reported");
        assert_eq!(mismatch.project_id, project_id);
        assert_eq!(mismatch.recorded, "wharfy", "the row carries the slug the pointer recorded");
        assert_eq!(mismatch.actual, slug, "the row carries the slug the id actually resolves to");
        assert!(!row.legacy, "a mismatched pointer is still the current format");
        assert!(row.exists, "the folder is listed as before — the mismatch does not hide it");

        std::fs::write(dir.join(".amenbo"), r#"{"v":1,"project_id":"01LEGACY","slug":"wharfy"}"#).unwrap();
        let row = project_bound_folders(project_id).unwrap().remove(0);
        assert!(row.legacy, "a pointer whose project_id cannot be read is reported as legacy");
        assert!(row.mismatch.is_none(), "with no readable id there is nothing to check the slug against");
        assert!(!row.pointer_missing, "the pointer is there — it is just old");

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The GUI can see the folder rows nobody claims and clean them up itself, over the same core
    /// path as the CLI's `doctor` / `doctor --fix`. Detection is covered by core's own tests, so what
    /// is pinned here is **the command's wiring**: only the debris is raised, forgetting it leaves a
    /// living project's folder in the index, and neither the folder's contents nor its `.amenbo` are
    /// touched.
    #[test]
    fn the_gui_sees_and_forgets_folder_bindings_no_project_claims() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-orphanbind-home-{}", std::process::id()));
        let dir = std::env::temp_dir().join(format!("amenbo-app-orphanbind-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "残骸PJ".into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        project_bind_folder(project_id, dir.to_string_lossy().to_string()).unwrap();
        assert!(orphan_bindings().unwrap().is_empty(), "a folder claimed by a live project is not an orphan");

        let orphan = std::env::temp_dir().join(format!("amenbo-app-orphanbind-left-{}", std::process::id()));
        std::fs::create_dir_all(&orphan).unwrap();
        {
            let store = Store::open().unwrap();
            let mut reg = store.bindings();
            reg.record_project_ref(project_id + 1_000, orphan.to_string_lossy());
            store.save_bindings(&reg).unwrap();
        }

        assert_eq!(
            orphan_bindings().unwrap(),
            vec![orphan.to_string_lossy().to_string()],
            "only rows with no claimant are surfaced to the GUI"
        );
        assert_eq!(forget_orphan_bindings().unwrap(), 1, "the cleanup drops it from the index");
        assert!(orphan_bindings().unwrap().is_empty(), "no orphans remain after the cleanup");
        assert!(orphan.is_dir(), "only the index row was dropped (the folder is untouched)");
        let bound = project_bound_folders(project_id).unwrap();
        assert_eq!(bound.len(), 1, "a live project's folder stays in the index");
        assert!(dir.join(".amenbo").is_file(), "that folder's pointer is intact too");

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&orphan);
    }

    /// The GUI's doctor screen sees **the same issues** as the CLI and repairs them through **the
    /// same cleanup entry points**. What is pinned here is the command's wiring: detection (core's
    /// `doctor::report`) carries the environment's issues through to the GUI, and the repair
    /// (`doctor_fix`) clears them.
    #[test]
    fn the_gui_doctor_face_shows_the_same_issues_and_repairs_them() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-doctor-home-{}", std::process::id()));
        let orphan = std::env::temp_dir().join(format!("amenbo-app-doctor-left-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&orphan);
        std::fs::create_dir_all(&orphan).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "整合PJ".into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let clean = doctor_report().unwrap();
        assert!(clean.ok && clean.issues.is_empty(), "a plain store has no issues");

        {
            let store = Store::open().unwrap();
            let mut reg = store.bindings();
            reg.record_project_ref(project_id + 1_000, orphan.to_string_lossy());
            store.save_bindings(&reg).unwrap();
        }

        let dirty = doctor_report().unwrap();
        assert_eq!(dirty.issues.len(), 1, "an environment issue reaches the GUI surface");
        assert_eq!(dirty.issues[0].kind, "orphan_binding");
        assert_eq!(dirty.warnings, 1);
        assert_eq!(
            dirty.issues[0].params.get("dir").map(String::as_str),
            Some(orphan.to_string_lossy().as_ref()),
            "the GUI receives the details it needs (which folder) to compose a sentence in its own language",
        );

        let fixed = doctor_fix().unwrap();
        assert_eq!(fixed.forgotten_bindings, 1, "the GUI's repair drops it from the index");
        assert!(doctor_report().unwrap().issues.is_empty(), "the re-check after repair is clean");
        assert!(orphan.is_dir(), "only the index row was dropped (the folder is untouched)");

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&orphan);
    }

    /// The "what next" affordances on the completion screen (reveal_folder / open_terminal) refuse a
    /// folder that does not exist, with an error that says so. The success path would really launch
    /// Finder or a terminal, so it is left untested; only the guard (the is_dir check) is checked.
    #[test]
    fn reveal_and_terminal_reject_missing_folder() {
        let missing = std::env::temp_dir().join(format!("amenbo-app-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing);
        let path = missing.to_string_lossy().to_string();
        assert!(reveal_folder(path.clone()).is_err(), "reveal_folder rejects a non-existent folder");
        assert!(open_terminal(path).is_err(), "open_terminal rejects a non-existent folder");
    }

    /// Round-trips the axis (dimension) assignment commands driven from the task detail view. The
    /// wiring — args, save, projection — is checked through per-task hydration and `task_dimensions`
    /// (the core ops themselves are already tested in core).
    #[test]
    fn axis_commands_round_trip() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-axis-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let (project_id, dim_id, v1, v2) = {
            let mut store = Store::open().unwrap();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "AxisPJ".into(), view: View::List, notes: String::new(), color: None,                },
            ).unwrap();
            let d = store.dimension_add(
                p.id,
                amenbo_core::ops::dimension::NewDimension { name: "軸".into(), ..Default::default() },
            ).unwrap();
            let v1 = store.dimension_value_add(d.id, "V1", None).unwrap();
            let v2 = store.dimension_value_add(d.id, "V2", None).unwrap();
            (p.id, d.id, v1.id, v2.id)
        };
        let card = |id: i64| tasks_by_ids(vec![id]).unwrap().into_iter().next();

        let id = task_add(Some(project_id), "軸テスト".into(), None).unwrap().tasks[0];
        assert_eq!(card(id).unwrap().project_id, Some(project_id));

        let ack = task_set_dimension_value(id, v1).unwrap();
        assert!(ack.scopes.contains(&"tasks"), "dimension change invalidates the board lists");
        assert_eq!(
            task_dimensions(id).unwrap().into_iter().map(|a| a.value_id).collect::<Vec<_>>(),
            vec![v1],
            "assigned V1"
        );
        let _ = task_set_dimension_value(id, v2).unwrap();
        assert_eq!(
            task_dimensions(id).unwrap().into_iter().map(|a| (a.dimension_id, a.value_id)).collect::<Vec<_>>(),
            vec![(dim_id, v2)],
            "single-select axis replaced V1 with V2"
        );
        let _ = task_unset_dimension_value(id, v2).unwrap();
        assert!(task_dimensions(id).unwrap().is_empty(), "cleared the axis assignment");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A task can be added straight into an empty project by passing `project_id` to `task_add` —
    /// what the + in the "To do" column of the GUI's status board does. The project membership is
    /// created, so the task shows up in a project-scoped `task_page`, which is the board's read path.
    #[test]
    fn task_add_into_empty_project_uses_project_membership() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-emptypj-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "空PJ".into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                },
            )
            .unwrap();
            p.id
        };

        let ack = task_add(Some(project_id), "空PJタスク".into(), None).unwrap();
        assert!(ack.scopes.contains(&"tasks"), "task_add invalidates the board lists");
        let task_id = ack.tasks[0];

        let page = task_page(Some(project_id), Some(String::new()), None, None, None).unwrap();
        assert_eq!(page.total_matched, 1, "the task belongs to the project");
        assert!(page.tasks.iter().any(|t| t.id == task_id), "the new task shows on the project board");

        let card = tasks_by_ids(vec![task_id]).unwrap().into_iter().next().unwrap();
        assert_eq!(card.project_id, Some(project_id), "belongs to the project");
        assert_eq!(card.r#ref, "AMB-T-1", "the task is numbered");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Checks the wiring by which `task_page` returns "the total number of matches, plus just that
    /// window", over a SQLite projection and a paged read: the filter grammar is shared with
    /// task list, per-task hydration carries the id, the number and the status, limit/offset take
    /// effect, and the filter narrows. The semantics of the indexed read itself (WHERE/ORDER
    /// BY/LIMIT) are covered by the tests on core's read layer.
    #[test]
    fn task_page_pages_and_filters() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-taskpage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "PagePJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                },
            )
            .unwrap();
            let mut done_id = None;
            for i in 0..5 {
                let t = store.add_task(amenbo_core::ops::task::NewTask {
                    title: format!("T{i}"),
                    project_id: Some(p.id),
                    due_on: None,
                    start_on: None,
                    priority: None,
                    notes: String::new(),
                    created_by_kind: Some(ActorKind::Human),
                })
                .unwrap();
                if i == 4 {
                    done_id = Some(t.id);
                }
            }
            store.set_task_completed(done_id.unwrap(), true).unwrap();
            p.id
        };

        let p1 = task_page(None, Some(String::new()), Some("created".into()), Some(2), Some(0)).unwrap();
        assert_eq!(p1.total_matched, 5, "total counts every match before paging");
        assert_eq!(p1.tasks.len(), 2, "page returns only the window");
        assert_eq!(p1.offset, 0);
        assert_eq!(p1.limit, Some(2));
        assert!(p1.tasks[0].r#ref.starts_with("AMB-T-"), "hydrated card carries its ref");

        let p2 = task_page(None, Some(String::new()), Some("created".into()), Some(2), Some(2)).unwrap();
        let p3 = task_page(None, Some(String::new()), Some("created".into()), Some(2), Some(4)).unwrap();
        assert_eq!(p2.tasks.len(), 2);
        assert_eq!(p3.tasks.len(), 1, "last page holds the remainder");
        let mut titles: Vec<String> = p1.tasks.iter().chain(&p2.tasks).chain(&p3.tasks).map(|t| t.title.clone()).collect();
        titles.sort();
        titles.dedup();
        assert_eq!(titles, vec!["T0", "T1", "T2", "T3", "T4"], "paging covers every task exactly once");

        let todo = task_page(None, Some("status:todo".into()), Some("created".into()), None, None).unwrap();
        assert_eq!(todo.total_matched, 4, "status:todo excludes the done task");
        assert_eq!(todo.tasks.len(), 4, "no limit returns all matches");
        assert!(todo.tasks.iter().all(|t| t.status == "todo"));

        let scoped = task_page(Some(project_id), Some(String::new()), None, None, None).unwrap();
        assert_eq!(scoped.total_matched, 5, "project scope matches the whole set");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `tasks_by_ids` hydrates the given ids in input order and drops silently any that do not
    /// exist. It is what the detail pane's single fetch and the inbox's union hydration (tasks with
    /// unread comments) rest on.
    #[test]
    fn tasks_by_ids_hydrates_in_input_order_and_drops_missing() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-byids-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let mut ids = Vec::new();
        {
            let mut store = Store::open().unwrap();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "ByIdsPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                },
            )
            .unwrap();
            for i in 0..3 {
                let t = store.add_task(amenbo_core::ops::task::NewTask {
                    title: format!("B{i}"),
                    project_id: Some(p.id),
                    due_on: None,
                    start_on: None,
                    priority: None,
                    notes: String::new(),
                    created_by_kind: Some(ActorKind::Human),
                })
                .unwrap();
                ids.push(t.id);
            }
        }

        let req = vec![ids[2], 999_999, ids[0]];
        let cards = tasks_by_ids(req).unwrap();
        assert_eq!(cards.len(), 2, "missing id is dropped");
        assert_eq!(cards[0].id, ids[2], "preserves input order");
        assert_eq!(cards[1].id, ids[0]);
        assert_eq!(cards[0].title, "B2");
        assert!(cards[0].r#ref.starts_with("AMB-T-"), "hydrated card carries its ref");

        assert!(tasks_by_ids(Vec::new()).unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `task_activity` queries the persistent read-model directly
    /// (`store_engine::read::task_activity`) and returns comments and system events newest first.
    /// Checks the comment bodies, the wording of the system events, and that other tasks' rows are
    /// left out.
    #[test]
    fn task_activity_reads_newest_first_from_read_model() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-activity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let (task_id, other_id) = {
            let mut store = Store::open().unwrap();
            let add_task = |store: &mut Store, title: &str| {
                store.add_task(amenbo_core::ops::task::NewTask {
                    title: title.into(),
                    project_id: None,
                    due_on: None,
                    start_on: None,
                    priority: None,
                    notes: String::new(),
                    created_by_kind: Some(ActorKind::Human),
                })
                .unwrap()
                .id
            };
            let task_id = add_task(&mut store, "Subject");
            let other_id = add_task(&mut store, "Other");

            store
                .add_system_event(
                    ActorKind::Human,
                    task_id,
                    amenbo_core::activity_log::event::task_status_changed("todo", "in_progress"),
                )
                .unwrap();
            store.add_task_comment(task_id, ActorKind::Human, "進めます").unwrap();
            store.add_task_comment(other_id, ActorKind::Human, "別件").unwrap();
            (task_id, other_id)
        };

        let items = task_activity(task_id, None).unwrap();
        assert_eq!(items.len(), 2, "only this task's stories, not the other task's");
        assert!(
            items.iter().all(|it| it.target.id == task_id),
            "every row targets the queried task"
        );
        assert!(items.iter().all(|it| it.target.title == "Subject"), "title resolved by join");

        let oracle_ids: Vec<i64> = {
            let store = Store::open().unwrap();
            store
                .activity(query::ActivityParams { task_id: Some(task_id), ..Default::default() })
                .unwrap()
                .items
                .into_iter()
                .map(|it| it.id)
                .collect()
        };
        assert_eq!(
            items.iter().map(|it| it.id).collect::<Vec<_>>(),
            oracle_ids,
            "the direct-SQL ordering matches core's own activity reader"
        );

        let comment = items.iter().find(|it| it.kind == "comment").expect("comment present");
        assert_eq!(comment.text.as_deref(), Some("進めます"));
        assert!(comment.event.is_none(), "comments carry no rendered event");
        let system = items.iter().find(|it| it.kind == "system").expect("system event present");
        assert!(system.event.is_some(), "system stories carry a rendered event");

        let one = task_activity(task_id, Some(1)).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].id, oracle_ids[0], "limit keeps the newest, matching the oracle");

        assert!(task_activity(999_999, None).unwrap().is_empty());
        let _ = other_id;

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Read state: read_receipts / mark_task_seen / mark_mailbox_seen round-trip through the
    /// commands, and persist across calls — that is, across rereading the file.
    #[test]
    fn read_receipts_round_trip() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-rr-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let rr = read_receipts().unwrap();
        assert!(rr.tasks.is_empty() && rr.mailbox_last_seen.is_none(), "empty to start");

        let after = mark_task_seen(12345).unwrap();
        assert!(after.tasks.contains_key(&12345), "mark_task is reflected");
        let reloaded = read_receipts().unwrap();
        assert!(reloaded.tasks.contains_key(&12345), "it persists across a separate call");

        let mb = mark_mailbox_seen().unwrap();
        assert!(mb.mailbox_last_seen.is_some(), "mark_mailbox is reflected");
        assert!(read_receipts().unwrap().mailbox_last_seen.is_some(), "it persists across a separate call");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The inbox's comment slot, independent of read state: a comment addressed to me on a task
    /// assigned to me shows up in `mailbox_comment_tasks`. Marking it seen (mark_task_seen)
    /// **does not remove it** — only the unread flag goes false (leaving the inbox on archive is
    /// reads.ts's job). A comment I made myself, as the human, does not show up.
    #[test]
    fn mailbox_comment_tasks_stays_after_read() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-m5-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let id = task_add(None, "受信箱D".into(), None).unwrap().tasks[0];
        let me = snapshot().unwrap().me_user_id.clone();
        task_assign(id, Some(me.clone()), "human".into()).unwrap();
        assert!(mailbox_comment_tasks().unwrap().is_empty(), "no comments = no membership");

        {
            let mut store = Store::open().unwrap();
            store.add_task_comment(id, ActorKind::Ai, "AIからの確認").unwrap();
        }
        assert_eq!(
            mailbox_comment_tasks().unwrap(),
            vec![(id, true)],
            "an AI comment makes it present and unread (unread=true)"
        );

        mark_task_seen(id).unwrap();
        assert_eq!(
            mailbox_comment_tasks().unwrap(),
            vec![(id, false)],
            "presence/unread clears after mark_task_seen"
        );

        comment_add(id, "了解".into()).unwrap();
        assert_eq!(
            mailbox_comment_tasks().unwrap(),
            vec![(id, false)],
            "your own (human) remark does not affect presence/unread"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// mailbox triggeredAt: returns the latest time of whatever put the item in the inbox (an
    /// assignment naming me, or a comment from someone other than me). Empty input gives empty
    /// output, ids with no such cause are left out, and a comment I made myself, as the human, is
    /// not a cause.
    #[test]
    fn mailbox_triggered_at_reports_latest_cause() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-trig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        assert!(mailbox_triggered_at(vec![]).unwrap().is_empty(), "empty input is empty");

        let id = task_add(None, "triggeredAt".into(), None).unwrap().tasks[0];
        let me = snapshot().unwrap().me_user_id.clone();

        let get = |ids: Vec<i64>| -> Option<String> {
            mailbox_triggered_at(ids).unwrap().into_iter().find(|(i, _)| *i == id).map(|(_, at)| at)
        };

        assert!(get(vec![id]).is_none(), "a task with no inbox trigger is omitted");
        assert!(mailbox_triggered_at(vec![999_999]).unwrap().is_empty(), "an unknown id is omitted");

        task_assign(id, Some(me.clone()), "human".into()).unwrap();
        let after_assign = get(vec![id]).expect("an assignment yields a triggeredAt");

        {
            let mut store = Store::open().unwrap();
            store.add_task_comment(id, ActorKind::Ai, "確認お願いします").unwrap();
        }
        let after_comment = get(vec![id]).expect("a comment yields a triggeredAt");
        assert!(after_comment >= after_assign, "triggeredAt follows the latest inbox trigger (the later comment)");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The lightweight read-only open (`Store::open_read_at`) reads the same thing as a full open
    /// (`Store::open_at`). Checks that (1) startup health (doctor) agrees on the read and the write
    /// path, and (2) a read open never writes a single byte to the source-of-truth file.
    #[test]
    fn read_open_matches_full_open() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-readopen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let _seeded = {
            let mut store = Store::open().unwrap();
            let me = ActorKind::Human.as_str().to_string();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "読みPJ".into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                },
            )
            .unwrap();
            let mk = |store: &mut Store, title: &str| {
                store.add_task(amenbo_core::ops::task::NewTask {
                    title: title.into(),
                    project_id: Some(p.id),
                    due_on: None,
                    start_on: None,
                    priority: Some(amenbo_core::model::Priority::High),
                    notes: "本文".into(),
                    created_by_kind: Some(ActorKind::Human),
                })
                .unwrap()
                .id
            };
            let a = mk(&mut store, "親");
            let b = mk(&mut store, "ブロッカー");
            store.set_task_assignee(a, Some(ActorKind::Ai)).unwrap();
            store.set_task_status(a, TaskStatus::InProgress).unwrap();
            store.depend_task(a, b, Some(ActorKind::Human)).unwrap();
            store.add_task_comment(a, ActorKind::Human, "確認").unwrap();
            let d = store
                .add_decision(amenbo_core::ops::decision::NewDecision {
                    title: "方針X".into(),
                    body: "理由".into(),
                    project_id: p.id,
                })
                .unwrap();
            store.link_decision(d.id, a).unwrap();
            store.accept_decision(d.id, Some(me.clone())).unwrap();
            let d2 = store
                .add_decision(amenbo_core::ops::decision::NewDecision {
                    title: "方針Y".into(),
                    body: "改訂".into(),
                    project_id: p.id,
                })
                .unwrap();
            store.supersede_decision(d2.id, d.id, Some(me.clone())).unwrap();
            let d3 = store
                .add_decision(amenbo_core::ops::decision::NewDecision {
                    title: "方針Y改".into(),
                    body: "一部改訂".into(),
                    project_id: p.id,
                })
                .unwrap();
            store.amend_decision(d3.id, d2.id).unwrap();
            (vec![a, b], vec![d.id, d2.id, d3.id])
        };

        let paths = amenbo_core::config::Paths::resolve().unwrap();
        drop(Store::open_at(paths.clone()).unwrap());

        let store_file = paths.store_file.clone();
        let mtime_before = std::fs::metadata(&store_file).and_then(|m| m.modified()).unwrap();

        let full = Store::open_at(paths.clone()).unwrap();
        let read = Store::open_read_at(paths.clone()).unwrap();

        let mtime_after = std::fs::metadata(&store_file).and_then(|m| m.modified()).unwrap();
        assert_eq!(mtime_before, mtime_after, "read/full open must not rewrite the truth-source file");

        let full_health =
            serde_json::to_value(full.startup_check.as_ref().expect("write open computes health")).unwrap();
        let read_health = serde_json::to_value(read.compute_startup_health().unwrap()).unwrap();
        assert_eq!(full_health, read_health, "startup health diverged between full (doctor) and read (doctor) open");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The GUI can read the feed forward by cursor: **only the changes after the store was read**
    /// come back, and what has been read never comes back again. Let that slip and either
    /// invalidations go missing and the screen freezes on stale data, or everything comes back every
    /// time and it degrades into refetching the world. Also pins that rows swept along by `CASCADE`
    /// are caught: deleting a task deletes its comment rows too, and the ops layer knows nothing
    /// about those rows — only `update_hook` sees them.
    #[test]
    fn changes_since_advances_with_the_cursor() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-feed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };

        let start = change_cursor().unwrap();
        let empty = changes_since(start, None).unwrap();
        assert!(empty.rows.is_empty() && !empty.expired, "empty when unchanged (not expired)");
        assert_eq!(empty.cursor, start, "when empty, the cursor stays as passed");

        let task = task_add(Some(project_id), "実装".into(), None).unwrap().tasks[0];
        let after_add = changes_since(start, None).unwrap();
        assert!(
            after_add.rows.iter().any(|r| r.dataset == "task" && r.row_id == task && r.op == "insert"),
            "the added task's row is included: {:?}",
            after_add.rows
        );
        assert!(!after_add.more && !after_add.expired);
        assert!(after_add.cursor > start, "the cursor advances");

        let drained = changes_since(after_add.cursor, None).unwrap();
        assert!(drained.rows.is_empty(), "no new changes after the cursor: {:?}", drained.rows);

        comment_add(task, "ひとこと".into()).unwrap();
        let before_delete = changes_since(drained.cursor, None).unwrap().cursor;
        task_delete(task).unwrap();
        let after_delete = changes_since(before_delete, None).unwrap();
        assert!(
            after_delete.rows.iter().any(|r| r.dataset == "task" && r.op == "delete"),
            "the task deletion is included: {:?}",
            after_delete.rows
        );
        assert!(
            after_delete.rows.iter().any(|r| r.dataset == "task_comment" && r.op == "delete"),
            "comment rows deleted along with it are included too (a deletion only update_hook sees): {:?}",
            after_delete.rows
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Make `dir` a git repository, so `hooks::probe` has a hook directory to answer with.
    fn git_init(dir: &std::path::Path) {
        let out = std::process::Command::new("git").arg("init").arg("-q").current_dir(dir).output().unwrap();
        assert!(out.status.success(), "git init failed: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// Which folders `hook_offers` even asks about, and which project each question lands on. The
    /// walk is the GUI's own: core decides the answer per folder, but the GUI has no cwd, so it is
    /// here that a folder gets skipped or gets attributed — and attributing it wrong writes the
    /// answer onto another project's record. A folder that is not a git repository has no hooks and
    /// so no question, one whose `.amenbo` is gone names no project to ask about, and where the
    /// registry and the pointer disagree the **pointer** is the answer.
    #[test]
    fn hook_offers_asks_only_git_folders_and_attributes_them_by_their_pointer() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-hookoffers-home-{}", std::process::id()));
        let base = std::env::temp_dir().join(format!("amenbo-app-hookoffers-dirs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
        std::env::set_var("AMENBO_HOME", &tmp);

        let new_project = |name: &str| -> i64 {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: name.into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        // Canonicalized, because binding records the folder that way and the offers are compared to it.
        let dir_of = |leaf: &str| -> std::path::PathBuf {
            let d = base.join(leaf);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::canonicalize(&d).unwrap()
        };

        let plain = new_project("素のPJ");
        let asked = new_project("問われるPJ");
        let lost = new_project("ポインタを失うPJ");
        let repointed = new_project("指し直されたPJ");

        // Not a git repository: bound and pointing at a project, but there are no hooks to have.
        let plain_dir = dir_of("plain");
        project_bind_folder(plain, plain_dir.to_string_lossy().to_string()).unwrap();

        // A git repository with no hooks and no answer on record: the one live question.
        let asked_dir = dir_of("asked");
        git_init(&asked_dir);
        project_bind_folder(asked, asked_dir.to_string_lossy().to_string()).unwrap();

        // A git repository the registry still names, whose pointer has been removed by hand.
        let lost_dir = dir_of("lost");
        git_init(&lost_dir);
        project_bind_folder(lost, lost_dir.to_string_lossy().to_string()).unwrap();
        std::fs::remove_file(lost_dir.join(".amenbo")).unwrap();

        let asked_dirs = |offers: &[HookOfferDto]| -> Vec<String> { offers.iter().map(|o| o.dir.clone()).collect() };

        let offers = hook_offers().unwrap();
        assert_eq!(offers.len(), 1, "only the git folder with a pointer is asked about: {:?}", asked_dirs(&offers));
        assert_eq!(offers[0].project_id, asked, "the offer names the project its pointer resolves to");
        assert_eq!(offers[0].project_name, "問われるPJ", "the modal is told what it is asking about");
        assert_eq!(offers[0].dir, asked_dir.to_string_lossy(), "the offer identifies the repository");
        // The offer carries the question and its subject, and nothing of the plumbing behind it: which slots
        // are empty is what a yes acts on, not what it is asked about.
        assert!(
            !offers.iter().any(|o| o.dir == plain_dir.to_string_lossy() || o.dir == lost_dir.to_string_lossy()),
            "a non-git folder and a folder with no pointer raise no question: {:?}",
            asked_dirs(&offers)
        );

        // Rebind the same folder at another project: the registry now records it under both, but the
        // pointer says `repointed`, and that is the record the answer would be written on.
        project_bind_folder(repointed, asked_dir.to_string_lossy().to_string()).unwrap();
        let after = hook_offers().unwrap();
        assert_eq!(
            after.len(),
            1,
            "the folder is asked about once, not once per project naming it: {:?}",
            asked_dirs(&after)
        );
        assert_eq!(after[0].project_id, repointed, "the folder's own .amenbo decides whose question it is");

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The two answers that are not questions are carried out here and come back as no offer:
    /// `SyncConsentToYes` when a hook of ours is on disk under a record that does not say yes, and
    /// `Install` when a slot this build added is missing from an install written by an older one.
    /// Both are best-effort — a failure must not cost the startup its other offers.
    #[test]
    fn hook_offers_settles_the_answers_that_need_no_asking() {
        let _env = env_guard();
        let tmp = std::env::temp_dir().join(format!("amenbo-app-hooksettle-home-{}", std::process::id()));
        let base = std::env::temp_dir().join(format!("amenbo-app-hooksettle-dirs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
        std::env::set_var("AMENBO_HOME", &tmp);
        std::fs::create_dir_all(&base).unwrap();

        let project = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "同意済みPJ".into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let dir = base.join("repo");
        std::fs::create_dir_all(&dir).unwrap();
        git_init(&dir);
        project_bind_folder(project, dir.to_string_lossy().to_string()).unwrap();

        // Our hooks installed by hand, with nothing on record: the disk wins.
        amenbo_core::hooks::install(&dir, amenbo_core::config::Paths::APP_NAME).unwrap();
        assert!(hook_offers().unwrap().is_empty(), "a settled answer raises no question");
        assert_eq!(
            Store::open().unwrap().hook_consent(project).unwrap(),
            Some(amenbo_core::hooks::HookConsent::Yes),
            "the record follows the disk (SyncConsentToYes), silently"
        );

        // An install from a build that did not know the commit-msg slot: consent already covers it.
        let hooks = amenbo_core::hooks::hooks_dir(&dir).unwrap();
        std::fs::remove_file(hooks.join("commit-msg")).unwrap();
        std::fs::write(hooks.join("pre-commit"), "#!/bin/sh\n# amenbo:hook (managed v1)\nexit 0\n").unwrap();
        assert!(hook_offers().unwrap().is_empty(), "completing a consented install is not a question");
        let states = amenbo_core::hooks::probe(&dir).unwrap();
        assert!(states.all_managed(), "the missing slot was installed under the consent already given: {states:?}");

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
    }
}
