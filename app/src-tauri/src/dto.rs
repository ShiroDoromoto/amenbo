//! The shapes the GUI is answered in. Every type here carries `#[derive(TS)]`, which writes
//! `app/src/bindings/bindings.ts` during `cargo test` — the single source of the TypeScript types
//! (`AMB-D-54`) — so this file is what a reader opens to learn what a command hands back, and what
//! a change to one of them moves on the front end.
//!
//! Definitions only. The shaping — reading the store and filling these in — stays in the command
//! layer beside the wiring that needs it (`crate::commands`), which is also where the `impl`s live.
//! Literal unions are pinned with `#[ts(type = ...)]`; `skip_serializing_if` is made optional with
//! `#[ts(optional)]`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ActorDto {
    pub(crate) name: String,
    #[ts(type = "\"human\" | \"ai\"")]
    pub(crate) kind: &'static str,
    /// Optional avatar image for the facet (data URL). The roster loads it from config; other
    /// ActorDto uses (assignee, author) leave it unset. Omitted when unset, and the front end draws
    /// an identicon instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) avatar: Option<String>,
    /// The talk window session this write came from, when the row records one. Set only where an
    /// ActorDto is an activity row's **author**; a roster entry or an assignee names a facet, not an
    /// act, and leaves it unset. Two AI sessions share one facet, so this is the only thing that tells
    /// them apart — and it is read, never inferred: absent means unknown (`AMB-T-3549`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) session: Option<String>,
}

/// One value of a dimension (a choice on the axis). Ordered dimensions arrive in `order_key` order.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DimensionValueDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    pub(crate) name: String,
    /// The value's readable key (`AMB-D-735`) — what names it outside Amenbo, where its display name
    /// cannot go. Omitted only for a row still being written; every saved value carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) slug: Option<String>,
    /// Start of the period, `YYYY-MM-DD` (inclusive). Omitted means an open start. A period is the
    /// payload of `role: time_axis`, not a generic attribute of a value — reads pass it straight
    /// through, and the gatekeeper for showing the date fields sits in the GUI.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) start_on: Option<String>,
    /// End of the period, `YYYY-MM-DD` (inclusive). Omitted means "ongoing" (an open end).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) end_on: Option<String>,
}

/// One unified dimension (classification axis), values included, so the GUI's dimension editor and
/// assignment selects render from real data. `role` is `none` or `time_axis` (phase); `ordered`
/// says whether the values have an order; `showOnCard` says whether a task's value on this axis
/// belongs on its card (`AMB-D-651`) — the axis's own answer, so it reads the same on every device;
/// `required` says the axis refuses to be left empty (`AMB-D-734`), which the detail pane reads to
/// hold "finish creating" back rather than letting the write be refused at the door; `slug` is the
/// readable key the axis and each of its values answer to outside Amenbo (`AMB-D-735`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DimensionDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    pub(crate) name: String,
    /// The axis's readable key (`AMB-D-735`), the counterpart of a value's. Omitted only for a row
    /// still being written; every saved axis carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) slug: Option<String>,
    pub(crate) notes: String,
    #[ts(type = "\"none\" | \"time_axis\"")]
    pub(crate) role: String,
    pub(crate) ordered: bool,
    pub(crate) show_on_card: bool,
    pub(crate) required: bool,
    pub(crate) values: Vec<DimensionValueDto>,
}

/// One task × dimension assignment (`valueId` is set on the `dimensionId` axis). The detail pane's
/// assignment selects use it to reflect the current value.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct TaskDimensionAssignmentDto {
    #[ts(type = "number")]
    pub(crate) dimension_id: i64,
    #[ts(type = "number")]
    pub(crate) value_id: i64,
}

/// The per-task assigned value for one project × dimension (`taskId`→`valueId`). The board uses it
/// to bundle tasks by value on the chosen dimension (browsing/grouping).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DimensionTaskValueDto {
    #[ts(type = "number")]
    pub(crate) task_id: i64,
    #[ts(type = "number")]
    pub(crate) value_id: i64,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) color: String,
    #[ts(type = "\"list\" | \"board\" | \"calendar\" | \"timeline\"")]
    pub(crate) view: String,
    /// Open task count (todo/in_progress/blocked — anything but done, live only). The sidebar's
    /// count badge.
    pub(crate) open_count: usize,
    /// Proposed (under-discussion) decision count — decisions still awaiting a ruling. Feeds the
    /// sidebar row and the header decision button's under-discussion badge.
    pub(crate) proposed_decision_count: usize,
    /// Unified dimensions (classification axes). Empty means none are in use. Task classification
    /// happens on these axes and nowhere else.
    pub(crate) dimensions: Vec<DimensionDto>,
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
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) notes: String,
    pub(crate) color: String,
    #[ts(type = "\"list\" | \"board\" | \"calendar\" | \"timeline\"")]
    pub(crate) view: String,
    pub(crate) archived: bool,
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
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) color: String,
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
/// `foreign` is the one finding that says the folder is **refused** rather than merely suspect
/// ([`ForeignStoreDto`], `AMB-D-685`): the pointer names another store, so a command run there stops
/// before it reads anything. The row would otherwise look healthy, which is the whole reason it is
/// carried here — and the fix is again the same relink, this build claiming the folder for itself.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct BoundFolderDto {
    pub(crate) path: String,
    pub(crate) exists: bool,
    pub(crate) mismatch: Option<SlugMismatchDto>,
    pub(crate) legacy: bool,
    pub(crate) pointer_missing: bool,
    pub(crate) foreign: Option<ForeignStoreDto>,
}

/// That folder's `.amenbo` was written by a build of another channel
/// ([`amenbo_core::binding::DirBinding::mismatched_store`]) — production against `amenbo-dev`, or a
/// throwaway `amenbo-dev-<task>`. The CLI refuses outright there (`pointer_other_store`); the GUI,
/// having no cwd to be refused in, says so on the row instead. Both names travel, because the
/// sentence needs the pair: whose the folder is, and who is looking at it. `running` is the same for
/// every row of a listing (it is this build's own name) and is repeated on each rather than read from
/// a second call — the row then holds everything its wording needs, the way [`SlugMismatchDto`] does.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ForeignStoreDto {
    /// The store name written in the folder's `.amenbo`.
    pub(crate) recorded: String,
    /// The store name of the build that is listing it.
    pub(crate) running: String,
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
    pub(crate) project_id: i64,
    /// The slug that was written in `.amenbo`.
    pub(crate) recorded: String,
    /// The slug of the project `project_id` actually points at (it may not have one).
    pub(crate) actual: Option<String>,
}

/// A reference to a record a decision points at, or is pointed at by (id + display name +
/// conversational ref). For cross-link display: `D-<n>` when the target is a decision, `#<n>` when
/// it is a task (the numbering spaces are separate). Both ids are integer keys.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DecisionRefDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    /// `null` when a forward edge dangles (a supersedes / amends target no longer live); the screen
    /// composes the placeholder in `config.language`. Reverse edges always carry a name.
    pub(crate) name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) r#ref: Option<String>,
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
    pub(crate) id: i64,
    /// `null` when the premise target dangles (a `builds_on` onto a decision no longer live); the screen
    /// composes the placeholder in `config.language`.
    pub(crate) name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) r#ref: Option<String>,
    /// Ref (`D-<n>`) of the decision that overturned the premise. Absent means the premise is current.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) superseded_by: Option<String>,
}

/// A reference with no entity key behind it (a decision's `decided_by` — an opaque token that
/// cannot be looked up).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PlainRefDto {
    pub(crate) id: String,
    pub(crate) name: String,
}

/// One decision record. The real data behind the list, the detail view and the cross-links.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DecisionDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    /// Conversational ref (`D-<n>`, a numbering space of its own, separate from tasks). The display
    /// form of `id`.
    pub(crate) r#ref: String,
    pub(crate) title: String,
    pub(crate) body: String,
    /// proposed / accepted / rejected. "Superseded" is not a status — it is an edge, and
    /// `superseded_by` is where it is read.
    #[ts(type = "\"proposed\" | \"accepted\" | \"rejected\"")]
    pub(crate) status: String,
    /// The project it lives under (the id is an integer key).
    pub(crate) project: Option<ProjectRefDto>,
    /// Decisions this one replaced (supersession, forward). One decision can replace several.
    pub(crate) supersedes: Vec<DecisionRefDto>,
    /// Decisions that replaced this one (reverse lookup).
    pub(crate) superseded_by: Vec<DecisionRefDto>,
    /// Decisions this one partially revised (amends, forward; the target stays current).
    pub(crate) amends: Vec<DecisionRefDto>,
    /// Decisions that partially revised this one (reverse lookup).
    pub(crate) amended_by: Vec<DecisionRefDto>,
    /// Decisions this one takes as a premise (builds_on, forward) — read them first. They stay current.
    pub(crate) builds_on: Vec<PremiseRefDto>,
    /// Decisions that take this one as a premise (reverse lookup) — what would need revisiting if
    /// this one were overturned (the blast radius).
    pub(crate) built_on_by: Vec<DecisionRefDto>,
    pub(crate) decided_at: Option<String>,
    pub(crate) decided_by: Option<PlainRefDto>,
    /// Linked tasks (cross-link), carrying status — is the work this decision created still open?
    pub(crate) linked_tasks: Vec<LinkedTaskRefDto>,
    pub(crate) created_at: String,
    /// When it last changed in any way — a body edit and a status transition alike. The pane hides it
    /// where it only repeats `created_at` or `decided_at`, so it reads as "changed since".
    pub(crate) updated_at: String,
}

/// A reference to a task a decision spawned. A [`DecisionRefDto`] plus **status**, so the screen can
/// answer "is this decision's work finished yet?". Completed ones are muted on the screen side.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct LinkedTaskRefDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) r#ref: Option<String>,
    #[ts(type = "\"todo\" | \"in_progress\" | \"done\" | \"blocked\" | \"rejected\"")]
    pub(crate) status: String,
}

/// A reference to a project (id + display name). The id is an integer key.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ProjectRefDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    pub(crate) name: String,
}

/// A reference to a task (id + title). The id is an integer key.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct TaskRefDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    pub(crate) name: String,
}

/// Where one task sits (project only — classification lives on the dimension axes). The real data
/// behind the project row in the task detail view.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PlacementDto {
    pub(crate) project: ProjectRefDto,
}

/// Premises that moved under a task **after it was reserved** (`AMB-D-366`, `AMB-D-373`) — the holder-side
/// surface. Each list is a way readiness was withdrawn since the task went `in_progress`: a blocker that
/// has not ended pinned on, a decision linked but not yet settled, or a decision that was already linked
/// and has stopped being settled. Carried on the card only when there is a change to show (see
/// [`TaskCardDto::premise_change`]), so the screen draws the note exactly when it matters.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PremiseChangeDto {
    /// Not-done blockers whose dependency edge was added after the reservation, in edge order.
    pub(crate) added_blockers: Vec<TaskRefDto>,
    /// Unsettled decisions linked after the reservation, in link order.
    pub(crate) added_decisions: Vec<DecisionRefDto>,
    /// Decisions already linked that stopped being settled after the reservation, in link order.
    pub(crate) reopened_decisions: Vec<DecisionRefDto>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct TaskCardDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    pub(crate) title: String,
    /// The name it goes by on screen, `#<n>` (the display form of `id`).
    pub(crate) r#ref: String,
    pub(crate) notes: String,
    #[ts(type = "number | null")]
    pub(crate) project_id: Option<i64>,
    #[ts(type = "\"todo\" | \"in_progress\" | \"done\" | \"blocked\" | \"rejected\"")]
    pub(crate) status: &'static str,
    pub(crate) assignee: Option<ActorDto>,
    #[ts(type = "\"high\" | \"medium\" | \"low\" | null")]
    pub(crate) priority: Option<&'static str>,
    pub(crate) due: Option<String>,
    /// The declared start day (`YYYY-MM-DD`), whether or not it has come. `not_started_until` below is
    /// the premise this field can raise; this is the field itself, which is what the pane editing it has
    /// to show — a day that has already come is still a value the person put there and may want back.
    pub(crate) start_on: Option<String>,
    /// Completion timestamp (RFC3339 UTC). Used to sort the Done column newest-first, among other
    /// things. None while the task is still open.
    pub(crate) completed_at: Option<String>,
    pub(crate) comments: usize,
    /// Can it be reserved? — no open blockers, every decision it rests on settled, the declared start
    /// day arrived, and the creation finished: the reasons
    /// [`amenbo_core::view::ReserveBlocker`] enumerates.
    pub(crate) ready: bool,
    /// Dependencies: blockers that are not done yet (id + name). Drives the "waiting on X" line in
    /// the detail pane. Empty means it can be started.
    pub(crate) blocked_by: Vec<TaskRefDto>,
    /// Where the task sits (with the project's display name), so the detail pane's project row
    /// renders from real data. Absent when the task is unplaced (inbox).
    pub(crate) placement: Option<PlacementDto>,
    pub(crate) created_by: Option<ActorDto>,
    /// The decision records that motivated this task (cross-link). Symmetric with
    /// `DecisionDto.linked_tasks`; drives navigation from the task detail view to the decision record.
    pub(crate) linked_decisions: Vec<DecisionRefDto>,
    /// Those `linked_decisions` that are not settled yet as grounds. Together with `blocked_by` they
    /// determine `ready` (both empty means ready). The reason a reservation was refused
    /// (`not_ready`) only ever appears in a toast that vanishes in seconds, so we name the decisions
    /// that are holding it back, letting the detail pane hold the same fact permanently.
    pub(crate) blocked_by_decisions: Vec<DecisionRefDto>,
    /// The declared start day, when it is still ahead (`YYYY-MM-DD`) — the third reason `ready` is
    /// false, beside `blocked_by` and `blocked_by_decisions`. Always serialized, `null` when the start
    /// day is no reason, so every `ready: false` the GUI draws carries a reason it can name on screen.
    pub(crate) not_started_until: Option<String>,
    /// Is the task still being put together — the fourth reason `ready` is false (`AMB-D-553`). A draft
    /// is drawn on the board like any other card (`AMB-D-555`), so the card has to carry the reason it
    /// cannot be picked up, the way `not_started_until` does for the third.
    pub(crate) draft: bool,
    /// Premises pinned on **after this task was reserved** (`AMB-D-366`, the holder-side surface): a
    /// blocker or an unsettled decision added since it went `in_progress`, silently withdrawing readiness
    /// the holder never asked to give up. Present only for an `in_progress` task that actually acquired
    /// one — `null` for every other status and when nothing changed — so the surface (a chip on the row,
    /// a firm warn when the holder leaves `in_progress`) draws exactly when it should.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) premise_change: Option<PremiseChangeDto>,
    /// When the task was written (RFC3339 UTC). The detail pane's answer to "how long has this been
    /// sitting here" — nothing else on the card dates the task itself.
    pub(crate) created_at: String,
    /// When the task was last written to (RFC3339 UTC). **Any** write moves it — a comment, a due date,
    /// a title fix — so it dates the record, not the status: what a status last moved is
    /// `status_changed_at`'s to say, and neither is a judgement input (`AMB-D-372`).
    pub(crate) updated_at: String,
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
    pub(crate) target_type: String,
    #[ts(type = "number")]
    pub(crate) id: i64,
    pub(crate) title: String,
    /// Is the target still around? Only a live target can be a destination — rows for deleted tasks,
    /// projects and decisions stay in the ledger but have nowhere to open, so it is this, not the
    /// type, that decides whether the row is clickable.
    pub(crate) live: bool,
}

/// A system event as the GUI needs it: the kind names the sentence template, and the rest are the
/// values that go into it. No prose — the wording lives in the GUI's dictionary, in the reader's
/// language, and the target's own name comes from [`ActivityTargetDto::title`] beside this.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct EventDto {
    pub(crate) kind: String,
    /// `task.status_changed`: the status the task moved to.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) status: Option<String>,
    /// `task.assigned`: the facet the task went to. Absent means the assignee was taken away, which
    /// is a different sentence rather than a missing value.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) to_kind: Option<String>,
    /// `project.deleted`: how much went with the project. Both are always sent together, so the
    /// sentence can say "none of either" without having to tell absent from zero.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(type = "number")]
    pub(crate) tasks: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(type = "number")]
    pub(crate) decisions: Option<u64>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ActivityItemDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    /// Which id sequence this row's `id` was drawn from (`amenbo_core::activity::Seq::rank`). The
    /// timeline merges sources that number independently, so `id` alone names no row: a task comment
    /// and a decision comment can carry the same one (`AMB-D-388`). A front end that identifies rows —
    /// to de-duplicate a page boundary, or to key a list — has to pair the two.
    #[ts(type = "number")]
    pub(crate) seq: i64,
    pub(crate) at: String,
    #[ts(type = "\"system\" | \"comment\"")]
    pub(crate) kind: String,
    pub(crate) author: ActorDto,
    pub(crate) target: ActivityTargetDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) event: Option<EventDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) text: Option<String>,
    /// Comment rows only: when the body was later edited in place. Absent when it was never edited.
    /// No revision history is kept, so this is the only hint a reader gets that the body is not what
    /// they read a moment ago.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) edited_at: Option<String>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// The user's language (config.json, global). Decides how the GUI localizes its UI labels. Null
    /// when unset.
    pub(crate) language: Option<String>,
    /// How dates are written (config.json, global) — a BCP-47 tag. Null means the one that goes
    /// with `language`, which is what most people want; a value is the reader whose two answers
    /// differ. Passed through as written: whether a tag is usable is the formatter's judgement, and
    /// the front end falls back to the language's rather than failing to draw a date.
    pub(crate) date_locale: Option<String>,
    /// This person's roster — the two facets that come from config (human / ai). It is the one
    /// supply line for every roster in the GUI: the assignee picker (unassigned / human name / AI
    /// name), the display name and avatar in settings, and display-name resolution. `kind` is the
    /// facet (`human`/`ai`); `name` is the effective display name from `config.human_name` /
    /// `ai_name`.
    pub(crate) roster: Vec<ActorDto>,
    pub(crate) projects: Vec<ProjectDto>,
    // Tasks and decisions are not carried here in full. Lists come from `task_page` and
    // `decision_page`, and single records from `tasks_by_ids` / `decisions_by_ids` — each fetching
    // only the window it needs (bounded memory).
    pub(crate) activity: Vec<ActivityItemDto>,
    /// Findings of the read-only integrity check run at startup. If anything is wrong, the GUI
    /// raises a warning banner (it never repairs anything by itself). A store with
    /// `config.startup_integrity_check` off adds nothing here.
    pub(crate) startup_health: StartupHealthDto,
    /// Whether an update exists. If the published `latest.json` names a version newer than the one
    /// running, `updateAvailable=true` — the material for the GUI's "an update is available (open
    /// the installer)" banner.
    pub(crate) version_status: VersionStatusDto,
    /// Level of perf instrumentation (the explicit value of `config.perf_log` — `off`,
    /// `budget-only` or `verbose`). Null when unset, and the front end falls back to the dev-build
    /// default of on (budget-only).
    pub(crate) perf_log: Option<String>,
    /// Update checking on or off (`config.update_check`, default true). Exposed so the settings
    /// screen's toggle can reflect the current value. When off, upstream latest.json is never
    /// queried, so `update_available` can never be raised.
    pub(crate) update_check: bool,
    /// Start at login on or off (`config.autostart`, default false). Exposed so the settings screen's
    /// switch can reflect the current value. It carries what the user asked for, not a reading of the
    /// OS — the registration itself lives outside the app, and only a shipped build ever draws the
    /// switch (a development build registers nothing, `AMB-D-547`).
    pub(crate) autostart: bool,
    /// What this device answered about the hourly tick (`config.tick_consent`) — `"yes"`, `"no"`, or
    /// null for a device nobody has asked yet (`AMB-D-707`). Exposed so the settings screen's switch
    /// can show the answer on record, the way `autostart` above is.
    ///
    /// Three states and a two-way switch, because the third is not a setting: never having answered is
    /// the *absence* of one, and what it means on the machine — no timer registered — is what "off"
    /// already says. The difference the null carries is whether the band may still put the question
    /// (`AMB-D-718`), which core decides and this screen never asks about.
    pub(crate) tick_consent: Option<String>,
    /// Whether taking the tick's registration away leaves a row behind in the OS's own list
    /// ([`amenbo_core::tick::removal_leaves_a_row`] — macOS, and only macOS). A fact about the build,
    /// so it rides in the snapshot rather than being asked for: what the settings switch needs it for
    /// is the sentence to say the moment it is switched off, and left unsaid the row that stays reads
    /// as a removal that failed.
    pub(crate) tick_removal_leaves_a_row: bool,
    /// The view a project created without one of its own opens in (`config.default_view`, default
    /// board). Exposed so the settings screen can show and change it. It is only the answer nobody
    /// gave: a project already carries its own `view`, and this never repaints one.
    #[ts(type = "\"list\" | \"board\" | \"calendar\" | \"timeline\"")]
    pub(crate) default_view: String,
}

/// The startup integrity check, shaped for the GUI: it feeds a read-only warning banner. Empty means
/// no warning (the counterpart of the CLI's stderr warning).
#[derive(Serialize, Default, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct StartupHealthDto {
    /// The problems doctor found (orphaned or dangling references, and so on). No prose sentence
    /// rides along — the GUI composes one from the kind and params in `config.language`
    /// (`src/core/i18n/`), so we hand these over as the same [`DoctorIssueDto`] the doctor screen
    /// uses.
    pub(crate) issues: Vec<DoctorIssueDto>,
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
    pub(crate) app_version: String,
    /// A newer version exists in the published distribution.
    pub(crate) update_available: bool,
    /// The version being offered (for display; the first one found). `None` means no update.
    pub(crate) newer_version: Option<String>,
}

/// What `task_page` returns: the task cards on the page, plus the total number of matches before
/// paging. The front end sizes its pager or virtual scroller from `total_matched` and draws only the
/// window in `tasks` (it never holds them all).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct TaskPageDto {
    pub(crate) tasks: Vec<TaskCardDto>,
    /// Total number of matches, before paging (limit/offset) is applied.
    pub(crate) total_matched: usize,
    /// The offset that was applied (how many were skipped).
    pub(crate) offset: usize,
    /// The limit that was applied (page size). None means no cap — everything from `offset` on.
    pub(crate) limit: Option<usize>,
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
    pub(crate) tasks: Vec<i64>,
    /// Decision ids that were touched (the single-record query `["decision", id]` gets invalidated).
    pub(crate) decisions: Vec<i64>,
    /// Coarse-grained scopes to invalidate ("tasks"/"decisions"). Empty means there is no query to
    /// invalidate — as with a roster write, where refetching the snapshot in `loadSnapshot` is
    /// enough to show the change.
    pub(crate) scopes: Vec<&'static str>,
}

/// For the "location" line under Settings > Data. Returns the real, OS-independent path (the
/// app-data root).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct StoreLocationsDto {
    /// Absolute path of the app-data root (the parent directory of the single `store.sqlite`).
    pub(crate) root: String,
}

/// One row of the change feed. **Which row of which table changed, and how** — that is all; no
/// values, no bodies (the caller refetches from the source of truth).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRowDto {
    /// Dataset the changed row belongs to (`task`, `task_comment`, `decision`, ...).
    pub(crate) dataset: String,
    /// Id of the changed row (the conversational number itself).
    pub(crate) row_id: i64,
    /// `insert` / `update` / `delete`.
    pub(crate) op: String,
}

/// The changes after a cursor. The GUI folds them into scopes and invalidates **only what moved**.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangesDto {
    /// Oldest first. Empty when `expired`.
    pub(crate) rows: Vec<ChangeRowDto>,
    /// The cursor to pass next time: the id of the last row if there were any, otherwise the cursor
    /// that came in. When `expired`, it is **the feed's current head** — after a full refetch
    /// (reconcile), the caller can resume incremental reads from there (changes that landed during
    /// the refetch stay ahead of the cursor, so none are lost).
    pub(crate) cursor: i64,
    /// The page was cut short by `limit` — there is more. The caller calls again with the returned
    /// cursor.
    pub(crate) more: bool,
    /// **The cursor has expired.** Truncation discarded rows the caller had not read, and the feed
    /// can no longer say what changed. Reading the empty response as "nothing changed" would freeze
    /// the screen on stale data, so the caller sees this and falls back to refetching from the source
    /// of truth.
    pub(crate) expired: bool,
}

/// What `decision_page` returns: the decisions on the page, plus the total count before paging.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DecisionPageDto {
    pub(crate) decisions: Vec<DecisionDto>,
    pub(crate) total_matched: usize,
}

/// Which face of a record the words landed on — the wire form of
/// [`amenbo_core::query::HitFace`]. Crossing as a name rather than a rank keeps the face something the
/// screen can label and icon; the rank is the engine's business.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "snake_case")]
pub enum SearchFaceDto {
    Title,
    Body,
    Comment,
    Label,
    Attachment,
}

/// One place the words are written: the face, the record that face belongs to, and the excerpt that
/// points at it. The wire form of [`amenbo_core::query::SearchHit`].
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SearchHitDto {
    pub(crate) face: SearchFaceDto,
    /// Which side the record is on — `task` or `decision`. The face alone does not say: a title is either.
    pub(crate) kind: String,
    /// The record's ref (`AMB-T-<n>` / `AMB-D-<n>`) — what the row opens, and where the number in it
    /// comes from.
    pub(crate) r#ref: String,
    pub(crate) title: String,
    /// The comment the words are in (`AMB-TC-<n>` / `AMB-DC-<n>`), when the hit is not on the record's
    /// own faces.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) comment: Option<String>,
    /// The hit's own instant, RFC3339 — a comment's posting time, or when the text it sits in was last
    /// written.
    pub(crate) at: String,
    pub(crate) snippet: String,
    /// Where in `snippet` the words landed, for the row to highlight. Sorted and never overlapping, and
    /// counted in the excerpt's **characters** — `Array.from(snippet)` splits it in that unit, `snippet[i]`
    /// does not.
    ///
    /// The core says this so that the screen does not have to match anything itself: the folding a match
    /// takes (NFKC, case, kana) lives with the index, and a second one on this side would be a second
    /// answer to what a term matches (`AMB-D-566`).
    pub(crate) matches: Vec<SearchMatchDto>,
    /// Where the record this row points at stands — what the row shows past the ref and the title, so the
    /// reader can tell a task still to be done from one that is over without opening it. Absent only when
    /// the record stopped being readable between the page and the read that fills this in.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) standing: Option<SearchStandingDto>,
}

/// A record's state, and — for a task — its priority and what it is filed under. The wire form of
/// [`amenbo_core::query::HitStanding`]; `kind` on the row says which vocabulary `status` is drawn from.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SearchStandingDto {
    /// `todo` / `in_progress` / `done` / `blocked` / `rejected` for a task, `proposed` / `accepted` /
    /// `rejected` for a decision.
    pub(crate) status: String,
    /// Tasks only, and only where one was set.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) priority: Option<String>,
    /// Tasks only, in axis order — empty for a task placed on no axis.
    pub(crate) labels: Vec<SearchLabelDto>,
}

/// One placement, in the words a person gave it.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SearchLabelDto {
    pub(crate) axis: String,
    pub(crate) value: String,
}

/// One run of `snippet` a term landed on — half-open, in characters. The wire form of
/// [`amenbo_core::query::MatchRange`].
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SearchMatchDto {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

/// One page of hits, and how many there are in all.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SearchResultDto {
    pub(crate) hits: Vec<SearchHitDto>,
    /// How many there are in all — what tells the screen its page left something behind.
    pub(crate) total_matched: usize,
}

/// What a reference resolves to (`kind` — task or decision — and the entity's id). The GUI branches
/// on it to decide which detail pane a link opens.
///
/// It is the answer to `resolve_ref` and the payload of the board's `ref-activated` event, which are
/// the two ways a ref becomes a destination: one clicked in a body, and one clicked in a pane of the
/// talk window (`crate::windows::show_ref`). The same shape for both on purpose — a ref that came
/// from the other window is not a second kind of destination, and giving it one would be an invitation
/// for the two to drift over what a click opens.
// Clone because Tauri's `emit` takes the payload by value and may hand it to more than one listener.
#[derive(Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct RefTargetDto {
    #[ts(type = "\"task\" | \"decision\"")]
    pub(crate) kind: String,
    /// The entity's primary key (an integer for both tasks and decisions). `kind` says which table
    /// it points into.
    #[ts(type = "number")]
    pub(crate) id: i64,
}

/// One permanent comment on a decision record, for the GUI. Task comments ride in the per-task
/// `task_activity` (kind=comment), but decisions have no activity path, so they get a read DTO of
/// their own. The author's facet is resolved to a display name from config; the times are sent as
/// they are, for the front end to word.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DecisionCommentDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    pub(crate) at: String,
    pub(crate) author: ActorDto,
    pub(crate) text: String,
    /// When the body was later edited. Absent when it was never edited (same meaning and same
    /// treatment as [`ActivityItemDto::edited_at`] on task comments).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) edited_at: Option<String>,
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
    pub(crate) id: i64,
    #[ts(type = "\"blob\" | \"url\"")]
    pub(crate) kind: String,
    pub(crate) blob_hash: Option<String>,
    pub(crate) filename: Option<String>,
    pub(crate) mime: Option<String>,
    pub(crate) size_bytes: Option<i64>,
    pub(crate) url: Option<String>,
    /// Are the blob's bytes on this machine? (Meaningless in `url` mode, where it is always false.)
    pub(crate) present: bool,
    #[ts(type = "\"human\" | \"ai\" | null")]
    pub(crate) created_by_kind: Option<String>,
}

/// One git commit SHA recorded on a task. Amenbo keeps the SHA as an opaque string — it
/// never reads git, verifies the commit, or knows which forge it lives on; the AI does that with
/// `git show <sha>`. `createdByKind` is who recorded it (the GUI's actor is always human).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct TaskCommitDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    /// The full commit SHA, lower-case hex (40 for SHA-1, 64 for SHA-256).
    pub(crate) sha: String,
    #[ts(type = "\"human\" | \"ai\" | null")]
    pub(crate) created_by_kind: Option<String>,
}

/// The payload of the `data-progress` event: the camelCase DTO of core's
/// [`amenbo_core::progress::Progress`]. `phase` is the stable string from `phase_str`, which the
/// GUI localizes. The startup migration ([`crate::migrate`]) reports itself in the same shape — one
/// way of showing progress is enough.
#[derive(Debug, Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DataProgressDto {
    /// What it is doing (`snapshotting`, `verifying`, `copying`, ...; the GUI localizes it).
    pub(crate) phase: String,
    /// Units completed (from 0).
    pub(crate) done: u32,
    /// Total units, when known.
    pub(crate) total: Option<u32>,
}

/// What [`run_backup`](crate::commands::run_backup) returns: the camelCase DTO of core's
/// [`amenbo_core::archive::BackupReport`].
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct BackupReportDto {
    /// Path of the archive that was written.
    pub(crate) path: String,
    /// Size of the archive, in bytes.
    pub(crate) bytes: usize,
}

/// What [`run_restore`](crate::commands::run_restore) returns: the camelCase DTO of core's
/// [`amenbo_core::archive::RestoreReport`].
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct RestoreReportDto {
    /// Where the old source of truth was set aside when it was replaced. None when nothing was
    /// replaced (a fresh creation).
    pub(crate) previous_saved_to: Option<String>,
    /// How many attachment blobs were written (blobs the destination already had, by hash, are not
    /// counted).
    #[ts(type = "number")]
    pub(crate) blobs: u64,
    /// How many older rollback points this restore's set-aside copy overtook and deleted. It is a
    /// report so that nothing is deleted silently, so the screen shows it only when it is non-zero.
    #[ts(type = "number")]
    pub(crate) superseded: usize,
    /// What the version chain did to the staged store. **Some only when it actually ran**, so a null
    /// check is all the front end needs to say "the archive you restored is not in the shape it was
    /// taken in".
    pub(crate) migration: Option<MigrationRunDto>,
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
    pub(crate) from: i64,
    /// The format version it carries now.
    #[ts(type = "number")]
    pub(crate) to: i64,
    /// Names of the steps that were applied, in order.
    pub(crate) applied: Vec<String>,
}

/// What [`run_export`](crate::commands::run_export) returns: the directory it wrote to, how big it is, and how many attachments
/// were carried out.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ExportReportDto {
    pub(crate) path: String,
    /// Total bytes of the directory that was written (`export.json` plus the attachment files). This
    /// is the number the completion message shows in KB — count only the JSON and a bundle carrying
    /// heavy attachments would claim to be far smaller than it is.
    pub(crate) bytes: usize,
    /// How many attachment files were written into `attachments/`.
    pub(crate) attachments: usize,
    /// How many attachments could not be carried out because their bytes are gone (we do not drop
    /// them silently).
    pub(crate) missing: usize,
}

/// One bound folder whose managed block is out of date. `version` is the version of that folder's
/// block; `current` is this binary's version ([`amenbo_core::agents::MANAGED_BLOCK_VERSION`]).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct StaleBlockDto {
    pub(crate) dir: String,
    pub(crate) file: String,
    pub(crate) version: u32,
    pub(crate) current: u32,
}

/// What `resync_managed_blocks` returns. `scanned` is how many folders that actually exist were
/// walked; `updated` lists the `(dir, file)` pairs rewritten to the current version — only the ones
/// whose content really changed.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ResyncReportDto {
    pub(crate) scanned: u32,
    pub(crate) updated: Vec<ResyncedDto>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ResyncedDto {
    pub(crate) dir: String,
    pub(crate) file: String,
}

/// One issue on the doctor screen (the same shape as core's
/// [`amenbo_core::validate::DoctorIssue`]). **No prose sentence rides along**: core returns only a
/// `kind` (the id of a message template) and `params` (what differs), and the surface composes the
/// sentence a person reads (the GUI localizes it by `config.language`; the CLI is always English).
/// The GUI's message table, and the affordances for how to fix each issue, live in
/// `src/core/i18n/locales/`, and they point at affordances that really exist in the GUI (the repair button
/// under Settings > Integrity, the folder list in project settings).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DoctorIssueDto {
    pub(crate) kind: String,
    pub(crate) severity: String,
    pub(crate) target: String,
    pub(crate) params: std::collections::BTreeMap<String, String>,
}

/// What `doctor_report` returns.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DoctorReportDto {
    pub(crate) ok: bool,
    pub(crate) errors: usize,
    pub(crate) warnings: usize,
    pub(crate) issues: Vec<DoctorIssueDto>,
}

/// The question waiting to be put to the user: may Amenbo wire its lint into your git hooks?
///
/// **There is one of it, ever** — not one per repository. It carries only what the wording needs, which is
/// the name of this build, and nothing about where an answer would land. Which repositories are bound,
/// which slots are empty, which a stranger holds, whether the hooks directory is one the whole team shares
/// — all of that is `amenbo_core::hooks::install`'s to act on, and none of it is a fork in the user's
/// road: nobody wants an AMB-T-… in their commits *here* but not *there*, so a screen that laid the
/// machinery out — or listed the folders — would be asking them to solve Amenbo's problem. What is still
/// unwired afterwards is the setup banner's to report ([`HookNoticeDto`]), where it is a statement rather
/// than a question.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct HookOfferDto {
    /// What this build of Amenbo is called on the command line, which is what its hooks will actually
    /// run and what its guidance tells the user to type. The dev channel answers `amenbo-dev`, so the
    /// name travels rather than being spelled into the wording.
    pub(crate) cmd: String,
}

/// One bound repository the banner has something to say about — the raw material for its wording, never
/// the sentence, as with [`HookOfferDto`].
///
/// Its two lists are two different things: [`HookNoticeDto::unwired`] is a standing state (the lint is not
/// running in these slots, and `hooks install` wires them — coexisting with another tool's hook where one
/// is there), while [`HookNoticeDto::restored`] is a transient event (a block of ours was found damaged or
/// stale this session and put back). A repository appears when either list is non-empty.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct HookNoticeDto {
    /// The project's name, so the banner can say which one it is about.
    pub(crate) project_name: String,
    /// The git repository this notice is about, which is also what identifies it.
    pub(crate) dir: String,
    /// What this build is called on the command line, for the same reason [`HookOfferDto::cmd`]
    /// carries it: the dev channel answers `amenbo-dev` and the wording must not spell either in.
    pub(crate) cmd: String,
    /// Slots with no block of ours (empty, or another tool's hook without Amenbo's block), which
    /// `hooks install` wires.
    pub(crate) unwired: Vec<String>,
    /// Slots whose block of ours was found damaged or stale this session and restored — something had
    /// changed or removed it (a tool regenerating its hook, a hand-edit). Empty in the ordinary case.
    pub(crate) restored: Vec<String>,
}

/// One AI harness a folder could start its session on `amenbo agent` with, and the text that would do it
/// ([`amenbo_core::harness`]).
///
/// The request travels with the row rather than being fetched on a click, because the surface it is on
/// both shows it and copies it: text fetched on the click would be text nobody read, and a button that
/// had to go and ask first could hand over an empty clipboard with no second chance to notice. It is a
/// few hundred bytes per unwired tool.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AgentHookToolDto {
    /// The catalog's own id for it (`claude-code`), which is also what `agent-hook snippet` takes.
    pub(crate) tool: String,
    /// The product's name for itself, for the sentence.
    pub(crate) label: String,
    /// The file the configuration goes into, relative to the folder.
    pub(crate) paste_into: String,
    /// What the reader is handed: the request to give the AI they work with, carrying the configuration
    /// and this build's launch command ([`amenbo_core::harness::request`]).
    pub(crate) request: String,
}

/// One harness a project is still waiting to be wired to, and the folders waiting for it — what the
/// project screen's standing row is drawn from (`AMB-D-459`).
///
/// **One text, many folders.** The request for a harness is the same wherever it is pasted; only the path
/// it goes into changes. So the tool is carried once and the folders are a list beside it, rather than the
/// text being repeated per folder — which is what kept the startup banner from being readable at four
/// folders.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AgentHookWiringDto {
    /// The harness, with the text that asks for its wiring.
    pub(crate) tool: AgentHookToolDto,
    /// This project's folders where that tool is not wired. Never empty — a tool nothing is waiting for
    /// is left out rather than carried with an empty list.
    pub(crate) dirs: Vec<String>,
}

/// The whole catalog and this project's folders — what the settings screen's "take the request" face is
/// drawn from (`AMB-D-670`).
///
/// **Two lists, not rows.** [`agent_hook_project_wiring`](crate::commands::agent_hook_project_wiring) answers with the tools a folder is waiting on,
/// so its unit is a pairing and a tool nothing waits for is left out. This one is the reader coming to
/// fetch text, so the tool is theirs to pick out of the whole catalog and the folders are the same
/// wherever they paste it — pairing them would be inventing an order the reader already knows.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AgentHookRequestsDto {
    /// Every harness Amenbo knows, in catalog order, each with the text that asks for its wiring.
    pub(crate) tools: Vec<AgentHookToolDto>,
    /// This project's bound folders — where the picked tool's request is pasted. Empty for a project
    /// nothing is bound to.
    pub(crate) dirs: Vec<String>,
}

/// Everything the screen that connects an AI draws (`AMB-D-681`): the projects a server can be
/// pointed at, and one row per app.
///
/// The two are asked for together because the screen reads them against each other — a row's ticks
/// are the projects whose folder that app already reaches, and neither half says that on its own.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct McpSetupDto {
    /// The projects that can be reached at all: the ones with a folder bound. A project with none
    /// has nowhere to point a server, and a tick beside it would write an entry naming nothing.
    pub(crate) projects: Vec<McpProjectDto>,
    /// Every app Amenbo knows, in the catalog's order.
    pub(crate) apps: Vec<McpAppDto>,
}

/// One project a reader can let an app reach.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct McpProjectDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    /// Its name as its owner wrote it — what the tick is labelled with.
    pub(crate) name: String,
    /// The folder a server would be pointed at. It is drawn beside the name because two projects can
    /// read alike and their folders never do, and because it is what the entry will actually carry.
    pub(crate) folder: String,
}

/// One app Amenbo can be reached from over MCP, as a screen draws its row (`AMB-D-672`,
/// `AMB-D-673`, `AMB-D-681`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct McpAppDto {
    /// The catalog's own id for it (`claude-desktop`).
    pub(crate) app: String,
    /// The product's name for itself, for the row.
    pub(crate) label: String,
    /// Whether Amenbo writes this one a file to open, rather than handing over a request
    /// (`AMB-D-672`). It is what decides which button the row draws.
    pub(crate) writes_file: bool,
    /// Whether this app already holds Amenbo's server (`AMB-D-673`).
    pub(crate) configured: bool,
    /// The folders those entries reach. Shown beside "set up", because set up for *which* folders is
    /// the half a reader cannot work out for themselves — and it is what the row's ticks open on.
    pub(crate) folders: Vec<String>,
    /// The entries this app still holds under a name Amenbo used to write (`AMB-D-679`), each with
    /// the request that clears it. They are drawn apart from the row's own state: an old entry is not
    /// this app being set up, it is something to take away.
    pub(crate) stale: Vec<McpStaleDto>,
}

/// One entry left behind under a name Amenbo no longer writes, as a row offers to clear it.
///
/// The request travels with it for the reason [`McpRequestDto`]'s do not have to: nothing the reader
/// picks changes it, so it is settled the moment the row is drawn.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct McpStaleDto {
    /// The name it is filed under — what tells two of these apart, and what the request names.
    pub(crate) name: String,
    /// The folder its arguments bind it to, where it names one.
    pub(crate) folder: Option<String>,
    /// The request that asks the reader's AI to delete it and nothing else.
    pub(crate) remove_request: String,
}

/// The two texts a row hands over, for the projects ticked on it right now.
///
/// They are fetched as the ticks move rather than when a button is pressed: the surface both shows a
/// text and copies it, and a button that had to go and ask first could hand over an empty clipboard
/// with no second chance to notice. Empty for the app Amenbo writes a file for — there is no request
/// to give anybody there, and the button beside it writes the file instead.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct McpRequestDto {
    pub(crate) add: String,
    pub(crate) remove: String,
}

/// What [`repair_pointers`](crate::commands::repair_pointers) returns: how many folders were fixed, and how many were left waiting on
/// a human's judgement.
#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts", rename_all = "camelCase")]
pub struct PointerRepairDto {
    /// Folders whose pointer was rewritten, or written back, in the current format.
    pub(crate) repaired: Vec<String>,
    /// Folders left untouched because their owner could not be determined uniquely (the human
    /// rebinds them through "open folder").
    pub(crate) unresolved: Vec<String>,
}

/// What `doctor_fix` returns: what was cleaned up, and how much of it.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DoctorFixDto {
    pub(crate) reclaimed_blobs: usize,
    pub(crate) freed_bytes: usize,
    pub(crate) forgotten_bindings: usize,
}

/// One entry of the plugin market list. Only what the list draws: identity, the one-line
/// description, and the axes it is filtered on (`AMB-D-347`). Nothing an install needs — the
/// signature, the checksum and the asset map are the detail's, not the list's (`AMB-D-385`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginEntryDto {
    /// The plugin's name, which is its identity in the catalog.
    pub(crate) name: String,
    /// What to call it on screen, when the catalog published one (`AMB-D-739`). It rides **beside**
    /// `name` rather than replacing it, the same way `descI18n` rides beside `desc`: which of the two a
    /// row draws is the front end's, and absent — a plugin whose author wrote none — is the ordinary
    /// case, where the name is what a reader has always seen.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) title: Option<String>,
    pub(crate) desc: String,
    /// The same line in the reader's language, when the catalog published one for this plugin
    /// (`AMB-D-622`). It rides **beside** the base line rather than replacing it: choosing between the
    /// two is the front end's (`AMB-D-623`), and absent is the ordinary case — a plugin nobody
    /// translated, a language nobody published, or a reader reading the base language itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) desc_i18n: Option<String>,
    pub(crate) author: String,
    /// `owner/name` — the GitHub coordinates a detail view reads stars and README from, lazily.
    pub(crate) repo: String,
    /// The operating systems it supports, as the manifest spells them (`macos` / `windows` / `linux`).
    pub(crate) os: Vec<String>,
    pub(crate) category: String,
    /// The official badge: catalog-authoritative, never the manifest author's claim (`AMB-D-347`).
    pub(crate) official: bool,
    /// Whether the official catalog is what served this entry — reviewed onto the official index. The
    /// other axis of the same trust picture as `official`, and not derivable from it: an official
    /// plugin is always listed, a listed one is written by anybody who passed review, and an entry
    /// from a third-party catalog is neither.
    pub(crate) listed: bool,
    /// The URL of the catalog that served it — the identity the source filter narrows on, since a name
    /// is the user's and two catalogs may share one.
    pub(crate) source: String,
    /// What that catalog is called. Carried on the entry rather than looked up in `sources`, because it
    /// is what the row wears on the free layer (`AMB-D-389`): a registered catalog is a trust root with
    /// a name, not an anonymous "other".
    pub(crate) source_name: String,
    /// Whether the official index recommends it — hand curation (`AMB-D-347`), for the "featured"
    /// ordering and the badge beside the trust layer. A third axis again: what a plugin is for, rather
    /// than who wrote it or who reviewed it. Core has already discounted a third-party catalog's claim
    /// on its own entries, so this is answered, not raw.
    pub(crate) featured: bool,
    /// When the catalog first listed it (`YYYY-MM-DD…`), for the "new" ordering. Absent on a catalog
    /// that does not record it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) added_at: Option<String>,
}

/// One catalog that fed the merged list — the official one first, then each registered third-party
/// one in registration order.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginCatalogSourceDto {
    pub(crate) url: String,
    /// What to call it: the name given at registration, or Amenbo's own for the official catalog.
    pub(crate) name: String,
    /// The fingerprint of the key this catalog's plugins are trusted on (`AMB-D-389`). `None` is a
    /// catalog that published none — browsable, and nothing on it installs.
    pub(crate) fingerprint: Option<String>,
    pub(crate) official: bool,
    /// Whether it answered at all — from the network or, failing that, its cache. `false` contributes
    /// nothing to the list, and is what the front end tells the user about rather than failing the view.
    pub(crate) reachable: bool,
    /// How many entries it offered, before cross-catalog de-duplication.
    pub(crate) offered: usize,
}

/// The plugin market view: every entry across the merged catalogs, plus which catalogs answered.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginCatalogDto {
    pub(crate) entries: Vec<PluginEntryDto>,
    pub(crate) sources: Vec<PluginCatalogSourceDto>,
    /// How many entries the merge dropped (a manifest the door refused, or a name a later catalog
    /// repeated). A count, not the rows: the list's job is to show what a catalog *is* shedding, and
    /// the reasons belong to the CLI's `plugin catalog list` (`AMB-D-354`).
    pub(crate) dropped: usize,
}

/// What registering a catalog would mean, worked out before anything is written (`AMB-D-389`) — the
/// material the consent screen puts in front of the user. Asking changes nothing on disk.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginCatalogProbeDto {
    /// The URL as it would be registered (trimmed) — what the agreement is about.
    pub(crate) url: String,
    /// What to call it when the user names nothing: the host serving it, or the name a record already
    /// registered under this URL carries.
    pub(crate) suggested_name: String,
    /// The fingerprint of the key this catalog publishes. `None` is a catalog that publishes none:
    /// browsable, and nothing on it installs.
    pub(crate) fingerprint: Option<String>,
    /// Whether this URL is already registered — a second registration changes nothing but the name,
    /// unless it is bringing a key the record does not have yet.
    pub(crate) registered: bool,
    /// Whether going ahead would pin a key that is not pinned yet. This is the one case that adds a
    /// trust root rather than a bookmark, so it is the one the screen must take consent for.
    pub(crate) pins_a_new_key: bool,
}

/// What GitHub says about one plugin's repository — the figures the catalog deliberately does not
/// carry (`AMB-D-347`). Every one is optional on its own: the requests behind them fail
/// independently, and a repository with no release has no download count to report.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginRepoFactsDto {
    /// `number` on the TS side (the default `bigint` has no `toLocaleString` grouping to draw it with,
    /// and a star count is nowhere near the range that motivates one).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub(crate) stars: Option<u64>,
    /// The current release's downloads, summed over its assets. Whatever else pulls an asset (CI,
    /// mirrors) is in there too, so it is a sense of scale rather than a user count (`AMB-D-347`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub(crate) downloads: Option<u64>,
    /// The README as Markdown, for the front end's renderer (which allows no raw HTML).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) readme: Option<String>,
    /// GitHub refused because too many requests came from this address. A different thing to tell the
    /// user than a failure: the answer is to wait, not to check the network.
    pub(crate) rate_limited: bool,
}

/// One setting a plugin's author declared, and what this machine currently holds for it
/// (`AMB-D-356`) — everything the generic form needs to draw a row and nothing Amenbo judges for
/// itself.
///
/// The value is the one the project on screen holds, and nothing stands under it (`AMB-D-434`): absent
/// reads as absent, which is what lets the form draw "not provided" and clear a field.
///
/// **A secret's value is never here.** The author's flag is what routes it to `plugin_secret`, and a
/// value read back into a webview would be a copy of it in a place `AMB-D-356` keeps it out of — so a
/// secret carries whether it is held, and that is all a form needs to mask it.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginConfigFieldDto {
    /// The key the author declared — what a write names, and what a refusal quotes back.
    pub(crate) key: String,
    /// The author's own label for the field, drawn as the form's caption.
    pub(crate) label: String,
    /// Whether the author marked it secret. The form masks it and never reads it back.
    pub(crate) secret: bool,
    /// Whether the author marked it required. An enable is refused while one of these has no value,
    /// so the form says which before the switch does.
    pub(crate) required: bool,
    /// The text value the project the request named holds, as stored — absent when unset, when no
    /// project was named, and always for a secret.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) value: Option<String>,
    /// Whether that project holds a secret for this key. Always false for a text field, whose value
    /// says it itself.
    pub(crate) secret_set: bool,
    /// Which of the three answers this project is giving (`AMB-D-415`), read by core so the form and the
    /// CLI cannot each decide for themselves what the stored string means: `chosen` (a value is held),
    /// `none` (a choice answered with none of its candidates), `unanswered` (nothing is held, and the
    /// author's default is what a run receives).
    ///
    /// A form that could not tell the last two apart would draw the same empty boxes for "declined" and
    /// "not been here yet", and offer no way back to the default.
    #[ts(type = "\"chosen\" | \"none\" | \"unanswered\"")]
    pub(crate) state: String,
}

/// **One condition still to be judged, on the answers the form is holding** (`AMB-D-727`).
///
/// The platform's half of a `when` is already settled by the time this arrives: what this build's OS hides
/// is not in the list at all ([`amenbo_core::plugin_when::after_platform`]), and no face here ever learns
/// an OS name. What is left reads another setting, and it is left to the form because the form is where
/// the answers are while it is open — someone ticking Cloudflare expects its fields the same moment, and
/// the store has not been told yet.
///
/// Read as an `and`: everything listed has to hold. A setting answers with `has` when `has` is among its
/// values, which for a `multi` setting is one of the comma-joined answers (`AMB-D-415`) and for a text one
/// is the whole of it.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginWhenDto {
    /// The key of the setting whose answer this reads.
    pub(crate) field: String,
    /// The value looked for among that setting's answers.
    pub(crate) has: String,
}

/// One candidate a setting offers (`AMB-D-415`): the value stored when it is ticked, and the words the
/// author wants beside its checkbox. Two audiences, so two strings — the plugin reads `value`, the user
/// reads `label`.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginConfigOptionDto {
    pub(crate) value: String,
    pub(crate) label: String,
    /// The candidate's label in the reader's language, when its author wrote one (`AMB-D-621`). Beside
    /// the base label, never over it (`AMB-D-623`). The `value` has no counterpart here: it is what
    /// travels to the plugin, so it is the same in every language.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) label_i18n: Option<String>,
    /// When this candidate is offered (`AMB-D-727`). Empty is a candidate with no condition on it, which
    /// is every one written before the key existed.
    pub(crate) when: Vec<PluginWhenDto>,
}

/// One setting a plugin will ask for, as its author declared it — and nothing a store holds for it.
///
/// This is what the market shows **before** anything is installed (`AMB-D-385`), and it is also what an
/// installed row carries: a value belongs to one project (`AMB-D-434`), and neither of those faces is
/// standing in one. What is held is read for a named project, through [`plugin_config_read`](crate::commands::plugin_config_read).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginWantedSettingDto {
    /// The key the author declared, which is what a later `plugin config set` names.
    pub(crate) key: String,
    /// The author's label for it, which is what the form will caption.
    pub(crate) label: String,
    /// That caption in the reader's language, when its author wrote one (`AMB-D-621`). Beside the base
    /// label rather than over it, like every other translated field here (`AMB-D-623`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) label_i18n: Option<String>,
    /// The paragraph the author wrote under this field (`AMB-D-656`), and it in the reader's language —
    /// the same two halves, picked the same way. Absent means the label is the whole of it.
    ///
    /// **Plain text at every step.** It is drawn as written, with no Markdown and no link: the form is
    /// where a credential is typed, and a destination its author chose does not belong on that screen.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) help: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) help_i18n: Option<String>,
    /// The example the author wrote for the empty input, and it in the reader's language (`AMB-D-656`).
    /// Shown inside the box and never stored — it is not a [`default_value`](Self::default_value), which
    /// is a value a run really receives (`AMB-D-474`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) placeholder_i18n: Option<String>,
    /// Whether the plugin writes this value rather than the user (`AMB-D-656`). The form draws the value
    /// with no input and no clear button beside it: what is there was generated by the plugin's own
    /// setup, and a button that takes it back is a way to break the plugin, not a way to correct it.
    pub(crate) readonly: bool,
    /// Whether it is a secret — worth knowing before installing, since it means a credential will have
    /// to be handed over for the plugin to do anything.
    pub(crate) secret: bool,
    /// Whether an enable is refused until it is filled in (`AMB-D-356`).
    pub(crate) required: bool,
    /// What kind of answer the field takes (`AMB-D-415`) — a line the user types, or any number of the
    /// candidates below. It rides with the declaration rather than with the held value because it is the
    /// same wherever you stand: it says what to *draw*, and a form is drawn before a project is picked.
    #[ts(type = "\"text\" | \"multi\"")]
    pub(crate) field_type: amenbo_core::plugin_manifest::FieldType,
    /// The candidates a `multi` field offers, in the author's order. Empty for a text field, which is the
    /// form's own answer to whether there is a choice to draw.
    pub(crate) options: Vec<PluginConfigOptionDto>,
    /// The value in force while nobody has answered (`AMB-D-415`) — what a run receives, and what the
    /// form ticks and captions as the default. Absent means an unanswered field is simply unanswered.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) default_value: Option<String>,
    /// When this setting is drawn (`AMB-D-727`). Empty is a setting with no condition on it, which is
    /// every one written before the key existed — and also one whose only condition was the platform,
    /// already settled and held.
    pub(crate) when: Vec<PluginWhenDto>,
}

/// **One operation the settings form may raise** (`AMB-D-664`) — a button, and whatever that press has to
/// ask for before it can run.
///
/// `cmd` is not shown to anyone: it is the name the press hands back, and the only thing Amenbo will
/// raise a call by ([`plugin_settings_action`](crate::commands::plugin_settings_action)). What is drawn is the label, plain — no Markdown and no
/// link, like every other author string on this screen (`AMB-D-656`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginActionDto {
    /// The declared call this button raises, as the manifest wrote it — the form's handle on it, never a
    /// line a caller composes (`AMB-D-522`).
    pub(crate) cmd: String,
    /// The words on the button, in the author's language.
    pub(crate) label: String,
    /// Those words in the reader's language, when their author wrote them (`AMB-D-620`). Beside the base
    /// label, never over it — the form picks (`AMB-D-623`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) label_i18n: Option<String>,
    /// What this press asks for and nothing keeps (`AMB-D-664`). Empty is the ordinary operation, which
    /// runs on the values already saved.
    pub(crate) ask: Vec<PluginAskDto>,
    /// When this button is offered (`AMB-D-727`). Empty is an operation with no condition on it.
    pub(crate) when: Vec<PluginWhenDto>,
}

/// **One value an operation asks for at the press** (`AMB-D-664`) — a box drawn only while the press is
/// being made, whose answer is handed to that one run and stored nowhere.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginAskDto {
    /// The name the value travels under — what the press hands back beside it, and never something the
    /// form stores.
    pub(crate) key: String,
    /// The label beside the box, in the author's language.
    pub(crate) label: String,
    /// That label in the reader's language, when their author wrote one (`AMB-D-620`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) label_i18n: Option<String>,
    /// Whether the box hides what is typed into it — the author's declaration, as on a secret setting
    /// (`AMB-D-356`). Here it decides only what the screen shows: there is no store to route it to.
    pub(crate) secret: bool,
}

/// What the catalog's **detail document** says about one plugin — the half of its entry that is fetched
/// for the one plugin someone opened, never for the list (`AMB-D-385`).
///
/// It answers what a reader wants before installing and the list deliberately does not carry: what it
/// will watch, what it will want to be told, and whether this build of Amenbo can speak to it at all. The install coordinates in the same document — the URL, the checksum,
/// the signature — are not here: they are the install path's, verified there over the bytes served
/// (`AMB-D-371`), and a face that displayed them would invite reading them as the assurance they are not.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginDetailDto {
    /// The observation events it subscribes to (`AMB-D-383`), by name — what installing it means it will
    /// be woken for.
    pub(crate) events: Vec<String>,
    /// The form it declares, in the author's order — the settings it will want filled in, and the parts
    /// drawn between them (`AMB-D-727`).
    pub(crate) config: Vec<PluginFormEntryDto>,
    /// **What the plugin is, in its author's own words** (`AMB-D-638`) — the Markdown the detail draws
    /// as its body. Absent is a plugin whose author wrote none, and the face falls back to the
    /// repository's README there; where this is present the README is neither drawn nor fetched.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) about: Option<String>,
    /// The same text in the reader's language, when its author wrote one (`AMB-D-621`). Beside the base
    /// text, never over it: choosing between the two is the front end's (`AMB-D-623`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) about_i18n: Option<String>,
    /// **What layer its author declared it lives at** (`AMB-D-601`) — a project's, or the device's. It is
    /// read here rather than off the list because the declaration rides in the detail document, and this
    /// is the face that draws one: a device-wide plugin reads every project on this machine, which is the
    /// thing worth knowing *before* it is taken on.
    #[ts(type = "\"project\" | \"machine\"")]
    pub(crate) scope: amenbo_core::plugin_manifest::Scope,
    /// Whether this build of Amenbo can run it (`AMB-D-359`). Asked here so the answer arrives before an
    /// install rather than at the enable that would refuse.
    pub(crate) compatible: bool,
    /// Why not, when `compatible` is false — core's own sentence, the same one the installed screen shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) incompatible_reason: Option<String>,
}

/// One "project × plugin" intersection, as both plugin faces draw a row for it (`AMB-D-447`) — the state
/// of that one crossing, read with the install rather than one project at a time.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginProjectRowDto {
    /// The project this row is for.
    #[ts(type = "number")]
    pub(crate) project: i64,
    /// Whether the plugin fires in it (`AMB-D-434`).
    pub(crate) enabled: bool,
    /// Whether it holds a value for any setting the author declares. Off with values is an ordinary state,
    /// so this is a fact of its own and not a reading of the gate.
    pub(crate) has_value: bool,
    /// Whether a `required` setting is empty here — the reason an enable at this crossing would be
    /// refused (`AMB-D-351`), said before the switch is pressed rather than after.
    pub(crate) required_unset: bool,
}

/// The device's own row, for a plugin its author declared the machine's (`AMB-D-601`) — the same three
/// readings a crossing carries, with no project to key them by.
///
/// It is a shape of its own rather than a [`PluginProjectRowDto`] with a hole in it: a device row is not a
/// crossing that lost its project, it is the one row a machine-wide plugin has, and a face handed a
/// project-shaped row would have gone looking for the project it names.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginDeviceRowDto {
    /// Whether the plugin fires on this device — the one gate it has (`AMB-D-601`).
    pub(crate) enabled: bool,
    /// Whether the device holds a value for any setting the author declares.
    pub(crate) has_value: bool,
    /// Whether a `required` setting is empty here — the reason an enable would be refused.
    pub(crate) required_unset: bool,
}

/// One plugin this machine holds, as the market draws its state on top of the catalog entry of the same
/// name (`AMB-D-351`). Installed and enabled are two facts, not one: an installed plugin that fires
/// nothing is the ordinary state.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginInstallDto {
    /// The plugin's name — the key the market joins this row onto a catalog entry by.
    pub(crate) name: String,
    /// What to call it on screen (`AMB-D-739`), read off the manifest kept beside the binary rather than
    /// the catalog — so an installed plugin has a name to draw with no catalog fetch and none reachable.
    /// This row is the one place the name is all there is: the installed list draws no description under
    /// it. Absent is a plugin whose author wrote none, and the name stands as it always did.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) title: Option<String>,
    /// Every "project × plugin" intersection this plugin has a row at (`AMB-D-447`) — the projects holding
    /// its gate open, and the projects holding a value while it is off. Empty means nowhere, which is an
    /// answer; a truth value read from one project is not, because it hides the projects it is still
    /// firing in (`AMB-D-412`).
    pub(crate) projects: Vec<PluginProjectRowDto>,
    /// **The device's row**, and only for a plugin whose author declared it the machine's (`AMB-D-601`).
    /// `None` is the ordinary plugin, whose rows are the projects' — so a face reads which of the two
    /// lists to draw off the declaration and never off an empty one, which every freshly installed plugin
    /// has. Without it a machine-wide plugin's gate has nowhere on screen to be read or moved: it crosses
    /// no project, so `projects` is rightly empty for it, and the row it does have was not in this answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) device: Option<PluginDeviceRowDto>,
    /// **What layer its author declared it lives at** (`AMB-D-601`) — read off the manifest that was
    /// installed, so it says what *this* build of the plugin declares rather than what the catalog now
    /// carries. It is not a switch and there is nothing to set: the declaration is what makes
    /// `plugin enable` mean one thing, and the face says so in words beside the gate it is about to open.
    #[ts(type = "\"project\" | \"machine\"")]
    pub(crate) scope: amenbo_core::plugin_manifest::Scope,
    /// Whether this build can speak to it at all (`AMB-D-359`). An open gate on an incompatible plugin
    /// fires nothing, and Amenbo updates underneath an install, so this is not derivable from a gate.
    pub(crate) compatible: bool,
    /// Why not, when `compatible` is false — the mismatch named, rather than left to the log.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) incompatible_reason: Option<String>,
    /// The form the author declared, in that order — its settings, and the parts drawn between them
    /// (`AMB-D-727`). Empty for a plugin that declares nothing, which is the form's own answer to whether
    /// there is anything to configure. What is held for a key is one project's (`AMB-D-434`) and comes
    /// from [`plugin_config_read`](crate::commands::plugin_config_read).
    pub(crate) config: Vec<PluginFormEntryDto>,
    /// The operations the author declared, in that order (`AMB-D-664`) — the buttons the settings form
    /// draws beside those fields. Empty is a plugin whose form is fields and a save, as every form was.
    pub(crate) actions: Vec<PluginActionDto>,
}

/// One thing a plugin's own run asked to have drawn on the settings form (`AMB-D-727`) — the vocabulary
/// a check's verdict and an operation's answer both come back in.
///
/// **The author supplies strings; Amenbo draws.** There is no markup here and no image: a `qr` carries
/// the text to encode and a `link` a destination and the words on the button, so what appears on screen
/// is the form's own, in the form's own paint. That is what keeps a plugin a child process
/// (`AMB-D-346`) and what keeps a reader able to tell Amenbo's words from a stranger's.
///
/// `qr` and `link` reach a face only for an official plugin (`AMB-D-727`); core drops a third party's
/// before this is built, so nothing on this side has to know the rule.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PluginShowPartDto {
    /// A line of explanation, drawn plain.
    Text {
        /// The author's words.
        text: String,
    },
    /// A heading, breaking a long answer up.
    Heading {
        /// The author's words.
        text: String,
    },
    /// A line that should stand out — a caution, a thing not to miss.
    Note {
        /// The author's words.
        text: String,
    },
    /// A set of lines drawn as a list, in the order they were written.
    List {
        /// The lines.
        items: Vec<String>,
    },
    /// A string with a copy button beside it, for what nobody should have to retype.
    Copy {
        /// What the button copies, and what is drawn beside it.
        text: String,
    },
    /// A QR code drawn from a string. Official plugins only.
    Qr {
        /// What the code encodes. The drawing is the screen's.
        text: String,
    },
    /// A button that opens a page in the reader's browser. Official plugins only.
    Link {
        /// Where it goes — `http` or `https`, which core has already held it to.
        url: String,
        /// The words on the button.
        label: String,
    },
}

/// Build the parts a run answered with, for a face to draw (`AMB-D-727`).
pub(crate) fn show_parts(parts: &[amenbo_core::plugin_show::Part]) -> Vec<PluginShowPartDto> {
    parts.iter().map(show_part).collect()
}

/// One part, for a face to draw (`AMB-D-727`).
pub(crate) fn show_part(part: &amenbo_core::plugin_show::Part) -> PluginShowPartDto {
    use amenbo_core::plugin_show::Part;
    match part {
        Part::Text(text) => PluginShowPartDto::Text { text: text.clone() },
        Part::Heading(text) => PluginShowPartDto::Heading { text: text.clone() },
        Part::Note(text) => PluginShowPartDto::Note { text: text.clone() },
        Part::List(items) => PluginShowPartDto::List { items: items.clone() },
        Part::Copy(text) => PluginShowPartDto::Copy { text: text.clone() },
        Part::Qr(text) => PluginShowPartDto::Qr { text: text.clone() },
        Part::Link { url, label } => {
            PluginShowPartDto::Link { url: url.clone(), label: label.clone() }
        }
    }
}

/// **One entry on a plugin's settings form** (`AMB-D-727`) — a setting somebody fills in, or a part
/// Amenbo draws where it stands.
///
/// The list is the author's declared order, because where a part sits is what it is for: the way to the
/// page that issues a token belongs above the box the token goes in, and two lists side by side cannot
/// say that.
///
/// A third party's `qr` and `link` are gone before this is built — core drops them (`AMB-D-727`), so a
/// face draws whatever reaches it.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PluginFormEntryDto {
    /// A setting the plugin takes, and what this machine holds for it.
    Field {
        /// The setting.
        field: PluginWantedSettingDto,
    },
    /// Something for Amenbo to draw, filled in by nobody.
    Part {
        /// What is left of the author's condition on it (`AMB-D-727`) — the platform's half already
        /// settled, so what remains reads another setting's answer and is re-read as the form changes,
        /// exactly as a setting's own is. Empty is a part drawn unconditionally.
        when: Vec<PluginWhenDto>,
        /// What to draw.
        part: PluginShowPartDto,
    },
}

/// What the author's own check said about the values, for the screen that shows the form (`AMB-D-664`).
///
/// It rides back with the gate because the check is what an enable raises: the switch is where the run
/// happens, and the form is where its sentences belong. A verdict may say yes and still have something to
/// say, which is why this is not a failure report.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginCheckDto {
    /// Whether the check said the values are usable. `false` with the gate shut is the refusal.
    pub(crate) ok: bool,
    /// Whether the check answered at all. `false` is a run that said nothing this build can read — the
    /// fail-closed silence (`AMB-D-354`), which carries no sentence of the author's and so gets Amenbo's.
    pub(crate) answered: bool,
    /// The one sentence about the settings as a whole, for the head of the form. Absent when the check
    /// wrote none.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) message: Option<String>,
    /// One sentence per setting the check spoke about, keyed by the setting's own key — drawn beside the
    /// box it names. Core has already dropped any key the manifest does not declare.
    #[ts(type = "Record<string, string>")]
    pub(crate) fields: std::collections::BTreeMap<String, String>,
    /// What the check asked to have drawn, in the order it wrote them (`AMB-D-727`). A check runs before
    /// anybody has filled anything in, which is where a way to the page that issues the token is worth
    /// the most. Empty is a check that asked for nothing, which is every one written before this existed.
    pub(crate) show: Vec<PluginShowPartDto>,
}

/// Where a gate ended up, and what closing it threw away (`AMB-D-399`) — what [`plugin_set_enabled`](crate::commands::plugin_set_enabled)
/// answers with.
///
/// The count is here because the discard is real and invisible: disabling a plugin drops whatever was
/// waiting on its queue, and those events are not caught up on when it comes back. The CLI has said so
/// since the drop existed; without this the switch on screen threw the same work away without a word.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginGateMovedDto {
    /// Whether the plugin fires at that gate now.
    pub(crate) enabled: bool,
    /// How many queued events the disable dropped. Zero on an enable, and on a disable that found an
    /// empty queue — the ordinary case, which a face is meant to pass over in silence.
    #[ts(type = "number")]
    pub(crate) dropped_queued: usize,
    /// What the author's own check said, when an enable raised one (`AMB-D-664`). Absent on a disable —
    /// nothing is checked on the way out — and on a plugin that declares no check.
    ///
    /// A verdict here with `enabled: false` is the refusal: the gate did not move, and the reason is the
    /// author's sentences rather than one of Amenbo's.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) check: Option<PluginCheckDto>,
}

/// What pressing one of a plugin's declared operations did (`AMB-D-664`) — the whole of what the form
/// draws afterwards.
///
/// **The exit code is still the whole verdict** (`AMB-D-353`), and the author's one line on stderr is
/// still the sentence beside the button. What a run may now add to that is a set of parts to draw
/// (`AMB-D-727`) — a QR to hold a phone up to, an address with a copy button — which is what gets anybody
/// through a setup that a sentence cannot. Anything past those is on the execution log, where every run
/// of this plugin already is (`AMB-D-361`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginActionRanDto {
    /// Whether the run exited successfully. `false` changes nothing — a failed operation is a line on the
    /// screen, not a state the form has to recover from.
    pub(crate) ok: bool,
    /// The author's own line, as written to stderr and drawn plain. Absent when the run said nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) message: Option<String>,
    /// What the run asked to have drawn, in the order it wrote them (`AMB-D-727`). Empty covers both a
    /// run that asked for nothing and one whose stdout is not an answer this build reads — an
    /// operation's stdout was never consumed before this, so a plugin writing something else there is
    /// drawn exactly as it always was.
    pub(crate) show: Vec<PluginShowPartDto>,
}

/// What an uninstall actually found and removed (`AMB-D-357`) — the receipt the face reports from.
///
/// Every piece is reported separately because the point of the receipt is that a plugin is more than its
/// binary: the settings and the secrets are the part a user does not picture going, and saying so
/// afterwards is what makes "a re-install starts clean" believable.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginRemovedDto {
    /// The plugin was enabled somewhere, and its gates have been closed on the way out.
    pub(crate) was_enabled: bool,
    /// Secrets existed and have been purged (`AMB-D-357`'s non-negotiable).
    pub(crate) secrets: bool,
    /// How many setting rows were deleted, across every project.
    #[ts(type = "number")]
    pub(crate) project_values: usize,
    /// How many per-project gate answers were deleted, across every project.
    #[ts(type = "number")]
    pub(crate) project_gates: usize,
    /// The plugin's home under `plugins/` existed and has been removed.
    pub(crate) directory: bool,
    /// The plugin had runs in the execution log and they have been purged (`AMB-D-387`).
    pub(crate) runs_log: bool,
    /// Whether anything at all was found. `false` is not a failure: the name held nothing on this machine.
    pub(crate) anything: bool,
}

/// One installed plugin the catalog holds a different build of (`AMB-D-359`) — an offer the face can act
/// on, not a diff of two manifests.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginUpdateDto {
    /// The plugin's name — how the face names it and how an apply asks for it.
    pub(crate) name: String,
    /// What the **offered** build calls itself on screen (`AMB-D-739`), when it carries one. It comes off
    /// the same documents the offer was read from, like the line below it, so the row names the build
    /// being offered rather than the one installed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) title: Option<String>,
    /// What the **new** build says it is, for a line the user can recognise it by.
    pub(crate) desc: String,
    /// That line in the reader's language, when the offered build carries one (`AMB-D-622`) — beside the
    /// base line, for the face to pick from (`AMB-D-623`). It comes off the same documents the offer was
    /// read from, so it describes the build being offered and not the one installed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) desc_i18n: Option<String>,
    /// The offered entry's identity — the digest of the detail document it was published as, which is the
    /// same thing detection compared (`AMB-D-438`). A face keys a dismissal by it, so a catalog that moves
    /// the entry again mints a new one and the offer returns. It has to be this and not the asset's digest:
    /// an update that changes no binary is a real update, and keying on the executable would let one
    /// dismissal bury every later manifest-only change behind the same id.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) available_detail_sum: Option<String>,
    /// Why this one needs a decision before it can be applied, or absent when it can just be applied
    /// (`AMB-D-359`: send the user to a screen only when judgment is required). `incompatible` — the
    /// offered build cannot run on this Amenbo; `settings` — it declares `required` settings this machine
    /// has no value for, and the plugin is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = r#""incompatible" | "settings""#)]
    pub(crate) hold: Option<String>,
    /// The settings behind a `settings` hold, named so the face can say which to fill in.
    pub(crate) missing: Vec<String>,
}

/// What a check was measured against (`AMB-D-359`) — the other half of its verdict, so a face can frame the
/// rows it is about to draw.
///
/// The freshness boundary makes "nothing has changed" and "nothing had changed an hour ago" the same empty
/// list, and the two states that read no catalog at all are opposites — nothing to compare, or nothing
/// reachable to compare against — so `read` keeps five arms rather than folding any of them together.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginCatalogReadDto {
    /// How the catalog behind the verdict was read. `fetched` — asked and answered, so the rows are the
    /// index as it stands; `cached` — inside the freshness window, so **no request was made**; `offline` —
    /// one was made and failed, so a copy of that age stood in; `notNeeded` — nothing is installed, so no
    /// catalog was read; `unavailable` — one was wanted and neither fetched nor cached, so nothing below is
    /// a verdict.
    #[ts(type = r#""fetched" | "cached" | "offline" | "notNeeded" | "unavailable""#)]
    pub(crate) read: String,
    /// How old the copy that answered is, for the two arms a cache stood in on. Absent everywhere else —
    /// a fetch has no age to report, and the arms that read no catalog have no copy at all.
    ///
    /// Seconds in a `u32`, which the face reads as a plain number: an age this does not fit is a hundred
    /// years of cache, and saturating there says "as stale as it gets", which is the only reading anyone
    /// wants from it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) age_seconds: Option<u32>,
}

/// A check's whole answer: what has moved, and what that was measured against.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginUpdatesDto {
    /// Every installed plugin the catalog holds a different build of.
    pub(crate) updates: Vec<PluginUpdateDto>,
    /// How current that list is — see [`PluginCatalogReadDto`].
    pub(crate) catalog: PluginCatalogReadDto,
}

/// How one plugin fared in [`plugin_update_apply_all`](crate::commands::plugin_update_apply_all) — a failure is a row, not the end of the run.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PluginUpdateOutcomeDto {
    /// The plugin this row is about.
    pub(crate) name: String,
    /// Whether its build was replaced.
    pub(crate) applied: bool,
    /// Why not, when it was not — core's own sentence, which is the one that knows the reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) error: Option<String>,
}

/// How far a check goes for its catalog (`AMB-D-462`) — the wire form of
/// [`amenbo_core::plugin_update::Reach`].
///
/// It crosses the boundary because only the caller knows which trigger it is. The face re-asks from several
/// (a focus return, a plugin screen opening, a button somebody pressed) and they do not want the same read,
/// so a command that picked one for everybody would be wrong for the rest.
#[derive(Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub enum PluginUpdateReachDto {
    /// An automatic trigger: a cache inside the freshness window answers with no request at all.
    Incidental,
    /// Somebody asked in so many words. Go to the catalog whatever the cache's age.
    Now,
}

/// One agent a folder's pane could be opened with, and what the folder and this machine say about
/// it (`crate::wake`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct WakeCandidateDto {
    /// The catalogued id, which is what a face hands back when the reader picks this one.
    pub(crate) id: String,
    /// The product's own name for itself.
    pub(crate) label: String,
    /// What it is started as — shown where it is missing, because that is the word to install.
    pub(crate) command: String,
    /// Whether this folder shows a trace of the provider being used here.
    pub(crate) traced: bool,
    /// Whether this machine can start it. Never more than that: see `amenbo_core::wake`.
    pub(crate) installed: bool,
}

/// Which agent a folder's pane opens with, and what to put to the reader when that is not settled.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct WakeDto {
    /// The folder this answers for, canonical — what the face opens the pane in, so that the pane
    /// and the answer are about the same folder.
    pub(crate) folder: String,
    /// Every catalogued agent, in catalog order. The install notice is drawn from this, which is why
    /// the ones this machine does not have are here too.
    pub(crate) candidates: Vec<WakeCandidateDto>,
    /// The ids worth offering, in catalog order. Empty means nothing on this machine can be started.
    pub(crate) offered: Vec<String>,
    /// The id to open with, when nothing needs asking. `None` with a non-empty `offered` is the
    /// question; `None` with an empty one is the notice.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) settled: Option<String>,
}

/// One thing an AI said about the session it is running in, on its way to the pane drawing it.
///
/// It is the surface layer's record ([`amenbo_core::session::Said`]) in the shape the webview reads.
/// The verbs each carry their own body, so the fields below are one verb's or another's: `text` is
/// every verb but `point`, and `target` with `why` is `point` alone.
// Clone for the same reason `PtyChunkDto` is: `emit_to` takes its payload by value.
#[derive(Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SessionSaidDto {
    /// The pane it was said in — the same id the terminal was opened under.
    pub(crate) session: String,
    #[ts(type = "\"name\" | \"note\" | \"waiting\" | \"finished\" | \"point\"")]
    pub(crate) verb: &'static str,
    /// When it was said (RFC3339 UTC).
    pub(crate) at: String,
    /// The folder the agent was in when it said it. It starts as the one the terminal was opened in
    /// and moves with every `cd`, so it is the agent's own rather than the pane's.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) cwd: Option<String>,
    /// The line said. Absent on `point`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) text: Option<String>,
    /// What `point` pointed at. Absent on every other verb.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) target: Option<String>,
    /// Why it is worth opening. Absent on every other verb.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) why: Option<String>,
}

/// What the ledger says the session in one pane has been doing — the reservations it is on, and how
/// many it has ended ([`amenbo_core::session_work`]).
///
/// The tasks come back as ids rather than rows: the pane draws one of them at most, and the same
/// [`crate::commands::tasks_by_ids`] every other screen hydrates with can say the rest. What is being
/// answered here is *whose* they are, which nothing but the ledger knows.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SessionWorkDto {
    /// Reserved and not ended, newest reservation first. One it has stopped on (`blocked`) is still
    /// among them — the reservation stands.
    #[ts(type = "Array<number>")]
    pub(crate) holding: Vec<i64>,
    /// How many it has ended, carried out or decided against. A count, because that is the whole of
    /// what the label says about them.
    #[ts(type = "number")]
    pub(crate) finished: usize,
}

/// One record an empty slot asks about: what to call it, and what it is called.
///
/// It carries what the row draws and nothing else. A card would carry a body and a status the row
/// never shows, and this is read once per screen — the answer is what to put in front of a person, not
/// the record itself, which is a press away on the face that owns it.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AdriftRowDto {
    /// The primary key, which is what a press hands to the face that opens it.
    #[ts(type = "number")]
    pub(crate) id: i64,
    /// The name it goes by on screen (`AMB-T-<n>` / `AMB-D-<n>`), which is also what says which of the
    /// two kinds a row is — the refs are two numbering spaces and a reader knows them apart.
    pub(crate) r#ref: String,
    pub(crate) title: String,
}

/// What in a project was left in the middle by a pane that has gone.
///
/// Two kinds, kept apart because opening one is not opening the other: a task opens on the ledger's
/// task face and a decision on its decision face. What they have in common is the question — nothing is
/// at either, and Amenbo is asking rather than deciding (`AMB-D-748`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AdriftDto {
    /// Reservations nothing is working on any more.
    pub(crate) tasks: Vec<AdriftRowDto>,
    /// Proposals nobody settled.
    pub(crate) decisions: Vec<AdriftRowDto>,
}

/// What one frame of the talk window is called, and who called it that — which is what says whether
/// the next naming may replace it ([`amenbo_core::frames`]).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FrameNameDto {
    /// The frame it is the name of. Names belong to frames, never to the session running in one.
    pub(crate) frame: String,
    pub(crate) name: String,
    #[ts(type = "\"typed\" | \"session\" | \"person\"")]
    pub(crate) by: &'static str,
}

/// A terminal this process has open, as a pane putting itself up is told about it.
///
/// It answers both of the pane's ways in: the terminal it just started, and the one it found already
/// running and adopted (`crate::pty::pty_sessions`). `started_at` is here because a pane cannot work
/// it out — a session that changed windows started when it started, and the moment the pane went up
/// says nothing about it.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PtySessionDto {
    /// The id the terminal was opened under.
    pub(crate) session: String,
    /// When the terminal was started (RFC3339 UTC).
    pub(crate) started_at: String,
    /// The folder the terminal is running in, as the filesystem spells it — `None` for one opened
    /// without any.
    ///
    /// It is here for the pane that **adopts** this session: where a terminal runs was settled when it
    /// started, and a frame that took one over rather than starting it has no other way to learn it. A
    /// frame that does not know would have to ask the person for the folder again the next time it has
    /// a terminal to start, which is the one flow the face has asked for twice.
    pub(crate) folder: Option<String>,
}

/// One chunk of a terminal's output, on its way to the pane drawing it (the payload of the talk
/// window's `pty://output` event).
///
/// The bytes travel base64-encoded because they are not text: an escape sequence is split wherever
/// the read ended, and a multi-byte character with it, and only the emulator that reassembles them
/// may decode. Anything that turned them into a string here would corrupt exactly the chunks that
/// crossed a boundary.
// Clone because Tauri's `emit_to` takes the payload by value and may hand it to more than one
// listener; the chunk is a string this thread just built and nothing else holds.
#[derive(Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PtyChunkDto {
    /// The session whose terminal this came out of.
    pub(crate) session: String,
    /// The chunk, base64-encoded.
    pub(crate) base64: String,
}


/// One name inside a folder, as the file face draws a row of its tree (`crate::folder`).
///
/// It says what the row is and nothing about what is under it: a folder answers for its own
/// children only when it is opened, so a tree that is still folded costs one directory read.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderEntryDto {
    /// The name on its own — one segment, never a path.
    pub(crate) name: String,
    /// Whether opening it lists more names.
    pub(crate) is_dir: bool,
}

/// One application a file could be opened with, as the file face draws a row of the chooser it has
/// to draw itself (`crate::open_with`).
///
/// It exists only where the operating system has no chooser of its own — macOS. The path is what
/// names the application when one is picked, and it is checked against this same list on the way
/// back: what the face was offered is the whole of what it may ask for.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderAppDto {
    /// What the machine calls it — the localised name, spelled the way a file manager spells it.
    pub(crate) name: String,
    /// The application bundle itself, which is what opening with it names.
    pub(crate) path: String,
    /// Whether this is the one the file would have opened with anyway, which is why it is first.
    pub(crate) usual: bool,
}

/// What the file face's second row is drawn from: the rows, and whether they are the whole story
/// (`crate::folder_watch`).
///
/// `partial` is the one thing the rows cannot say for themselves. A watch is a set of watches, one
/// per folder, and the kernel's limit is per user — so some may be refused while the rest work.
/// Drawn as a whole watch, that reads as "nothing has changed" in the half nobody is watching.
// Clone because the answer to the first call is also what the thread starts out holding, and
// PartialEq because a wake-up is only worth telling anybody about when it moved these rows.
#[derive(Clone, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderChangesDto {
    /// The files written to most recently, newest first.
    pub(crate) changed: Vec<FolderChangedDto>,
    /// Whether some of the folder is unwatched — a walk that stopped at its cap, or a watch the
    /// kernel refused.
    pub(crate) partial: bool,
}

/// A file that changed lately, as the file face's second row draws it (`crate::folder`).
///
/// The path is the segments from the folder the face is rooted at, so the row can be opened by
/// handing the same list back — nothing here is a path a caller has to take apart.
#[derive(Clone, PartialEq, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderChangedDto {
    /// The segments from the root, the file's own name last.
    pub(crate) path: Vec<String>,
    /// When it was last written (RFC3339 UTC).
    pub(crate) modified: String,
}

/// What a file has to show for itself, as far as a panel can show it (`crate::folder`).
///
/// Exactly one of `text` and `image` is filled, and both are empty for a file that is neither —
/// what a reader is then told is that it cannot be read here, which is the honest answer for a
/// binary. Text is cut at a cap, because a panel is not a pager and a very long file would be paid
/// for in full to draw a screen of it.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderFileDto {
    /// The text, where the head of the file holds no NUL byte.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) text: Option<String>,
    /// True when `text` stops short of the file's end.
    pub(crate) truncated: bool,
    /// The picture, where the bytes say they are one and there are few enough of them to carry.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) image: Option<FolderImageDto>,
}

/// A picture out of a folder, carried whole so the webview can draw it without a URL of its own.
///
/// The bytes come over the command seam rather than through [`crate::fileproto`], because that door
/// is fenced by a session's folder and this face is rooted at the project's (`AMB-T-3602`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderImageDto {
    /// The type the bytes themselves say they are — read off the first of them, never off the name.
    pub(crate) mime: String,
    /// The whole picture, base64-encoded, for a `data:` URL.
    pub(crate) base64: String,
}

/// The talk window's arrangement, as this device left it (`amenbo_core::frames::SavedLayout`).
///
/// The shape only: how many panes to a page, the frames in slot order, and the folder each was
/// working in. **What was running is not here** — a session died with the last run, and a pane drawn
/// as though it were still there would be the window saying something untrue (`AMB-T-3607`).
#[derive(Deserialize, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct TalkLayoutDto {
    /// How many panes to a page.
    pub(crate) count: u32,
    /// The next frame id to hand out — ids are never reused, so a name stays on its own frame.
    pub(crate) next_id: u32,
    /// The frames, in slot order.
    pub(crate) frames: Vec<TalkFrameDto>,
}

/// One frame of a kept arrangement: where it sat, and what it was working on.
#[derive(Deserialize, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct TalkFrameDto {
    /// The id its name is kept against (`FrameNameDto`).
    pub(crate) id: String,
    /// The project this pane is one of. Absent in an arrangement written before panes belonged to a
    /// project, which the window answers for (`app/src/talk/layout.ts`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) project: Option<u32>,
    /// The folder its terminal was working in, where it had one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) folder: Option<String>,
}
