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
    /// Is this value closed — retired from what a record is newly filed under, while everything already
    /// filed under it stays (`AMB-D-829`)? It is the payload of `role: closable`, the way a period is the
    /// time axis's. Sent for every value, meaningful only where the axis carries that role — the shape
    /// the period fields already have.
    ///
    /// The screens read it to three different ends: the filter offers a closed value like any other
    /// (filtering by it is the whole point of closing rather than deleting), the value picker hides it
    /// unless the record already carries it, and the classification panel shows every value with the
    /// switch that closes and reopens one.
    pub(crate) closed: bool,
}

/// One unified dimension (classification axis), values included, so the GUI's dimension editor and
/// assignment selects render from real data. `role` is `none`, `time_axis` (phase) or `closable` —
/// the axis whose values can be closed rather than deleted (`AMB-D-829`); `cardinality`
/// is `single` or `multi` — how many of the axis's values one record may hold (`AMB-D-826`), which is
/// what the detail pane reads to draw one select or a row of chips; `ordered`
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
    /// How many of this axis's values one record may hold (`AMB-D-826`). Every axis starts `single`,
    /// where one value replaces the last; `multi` is the one that gains a value and keeps what it had.
    /// The detail pane reads it to draw one select or a row of chips.
    #[ts(type = "\"single\" | \"multi\"")]
    pub(crate) cardinality: String,
    /// What this axis is nominated as (`amenbo_core::model::DimensionRole`). `time_axis` is the one
    /// whose values carry periods and resolve the current era; `closable` is the one whose values can
    /// be closed — retired from what a record is newly filed under, while everything already filed
    /// under them stays (`AMB-D-829`). One role per axis, and `none` is where every axis starts.
    #[ts(type = "\"none\" | \"time_axis\" | \"closable\"")]
    pub(crate) role: String,
    pub(crate) ordered: bool,
    pub(crate) show_on_card: bool,
    pub(crate) required: bool,
    /// Which of the two entities this axis classifies (`AMB-D-789`). The screens read it to decide
    /// which of them offer the axis at all — the board and the task card the task side, the decision
    /// pane the decision side — while the manager, which is where it is set, offers every axis.
    #[ts(type = "\"task\" | \"decision\" | \"both\"")]
    pub(crate) applies_to: String,
    pub(crate) values: Vec<DimensionValueDto>,
}

/// One task × dimension assignment (`valueId` is set on the `dimensionId` axis). The detail pane
/// reads them to show what the task carries — one row per assignment, so a multi-select axis
/// (`AMB-D-826`) answers with several rows naming the same `dimensionId`.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct TaskDimensionAssignmentDto {
    #[ts(type = "number")]
    pub(crate) dimension_id: i64,
    #[ts(type = "number")]
    pub(crate) value_id: i64,
}

/// One decision × dimension assignment (`valueId` is set on the `dimensionId` axis) — the decision
/// side of [`TaskDimensionAssignmentDto`], a type of its own because the two ends are (`AMB-D-781`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DecisionDimensionAssignmentDto {
    #[ts(type = "number")]
    pub(crate) dimension_id: i64,
    #[ts(type = "number")]
    pub(crate) value_id: i64,
}

/// One assignment on one project × dimension (`decisionId`→`valueId`) — the decision side of
/// [`DimensionTaskValueDto`]. The decisions tab uses it to narrow its list by classification. One row
/// per assignment, so an axis admitting several values at once (`AMB-D-826`) sends several for the one
/// decision.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct DimensionDecisionValueDto {
    #[ts(type = "number")]
    pub(crate) decision_id: i64,
    #[ts(type = "number")]
    pub(crate) value_id: i64,
}

/// One assignment on one project × dimension (`taskId`→`valueId`). The board uses it to bundle tasks by
/// value on the chosen dimension (browsing/grouping), and to draw the values its cards carry. One row per
/// assignment, so an axis admitting several values at once (`AMB-D-826`) sends several for the one task —
/// which is why the axis splitting the columns is never one of those.
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
    /// The image the project shows for itself, as the `data:image/…` URL of the small square that was
    /// baked when it was registered, or `null` where there is none (`AMB-D-839`). The tabs down the
    /// edge of the face draw it in place of the colour and the first character (`AMB-D-838`), and they
    /// draw every project — so it rides here rather than being fetched a project at a time. Only the
    /// display version: the file it was baked from stays in the blob store and never comes out to the
    /// webview.
    pub(crate) icon: Option<String>,
    #[ts(type = "\"list\" | \"board\" | \"calendar\" | \"timeline\"")]
    pub(crate) view: String,
    /// Open task count (todo/in_progress/blocked — anything but done, live only). The sidebar's
    /// count badge.
    pub(crate) open_count: usize,
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
    /// The image the project shows for itself, as a `data:image/…` URL, or `null` where it has none and
    /// the surfaces fall back to the colour and the first letter of the name (`AMB-D-839`). The
    /// original it was baked from stays in the blob store and never rides out here — the screen shows
    /// the display version and sends a whole new pair when the human registers another image.
    pub(crate) icon: Option<String>,
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
    /// decided / rejected — the two ways a decision ends (`AMB-D-918`). "Superseded" is not a status
    /// — it is an edge, and `superseded_by` is where it is read; neither is "still being written",
    /// which is `draft` below.
    #[ts(type = "\"decided\" | \"rejected\"")]
    pub(crate) status: String,
    /// Is this decision still being written (`AMB-D-918`)? The pane draws its doors off this rather
    /// than off the status: while it is up, the decision is unfinished and the tasks resting on it are
    /// held back; `decision finish-writing` lowers it.
    pub(crate) draft: bool,
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
    /// How many automation runs may hold a lane at once (`config.automation_lanes`, default 3).
    /// Exposed so the settings screen can show and change it, and so the workspace band can draw it
    /// as the second half of `2/3`.
    ///
    /// It crosses projects, so the band's number and the panes of the project being looked at are not
    /// one to one — which is why the band draws the number alone and says nothing about which project
    /// is holding a lane.
    #[ts(type = "number")]
    pub(crate) automation_lanes: i64,
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

/// The one font a skin carries, on the way to the window. The bytes ride as base64 with the
/// wrapping already taken out — the window decodes once and hands the buffer to `FontFace`, which
/// is measurably quicker than a `data:` URI and touches no CSP directive.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SkinFontDto {
    /// The name the generated face is given, and the one the skin's `font` puts at the head of its
    /// stack.
    pub(crate) family: String,
    /// The woff2, base64, clean.
    pub(crate) data: String,
}

/// One picture a skin lays behind one of its surfaces, on the way to the window: the file it is in
/// and the two words that say how it is laid. The url is not here — where a custom protocol lives
/// differs by platform, and building one is the window's (`app/src/core/customScheme.ts`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SkinBackgroundDto {
    /// The file inside the skin's zip, by the name it has in there. What the window puts on the
    /// address it builds for [`crate::skinproto`].
    pub(crate) file: String,
    /// `cover`, `contain` or `tile`, as the check left it.
    pub(crate) fit: String,
    /// Where in its place the picture sits — one of the nine spots, as the check left it.
    pub(crate) at: String,
}

/// The skin this device has on, as the window wears it: the two sides' tables of token name to
/// value, with the leading `--` left off the way the file writes them. `null` from the command
/// rather than an empty pair when nothing is on — "no skin" and "a skin that sets nothing" are not
/// the same answer, and only one of them can happen.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SkinTablesDto {
    /// The skin's own name, the one the config holds and the file is kept under.
    pub(crate) name: String,
    /// What it is called on screen, in the author's own words.
    pub(crate) title: String,
    /// The same name per language, where the author wrote any: language code to the name in that
    /// language. Empty on a skin that wrote none, which is every skin that came from before
    /// `titles:` was a key. Picking one of them is the window's (`AMB-D-396`).
    pub(crate) titles: std::collections::BTreeMap<String, String>,
    /// The light side's values, as the check left them: known names a skin may set, text only.
    pub(crate) light: std::collections::BTreeMap<String, String>,
    /// The dark side's values, on the same terms.
    pub(crate) dark: std::collections::BTreeMap<String, String>,
    /// The drawings it lays in place of this build's own, by the name of the icon each one stands
    /// in for — each one a `data:` URI the window lays as a mask (`AMB-D-937`). Empty on a skin
    /// that replaced none, which is every skin that carries no drawings.
    ///
    /// The bytes ride here rather than being fetched, the way the face's do: a drawing is a few
    /// kilobytes, every replaced one is wanted the moment the window draws anything, and a mask
    /// laid from a `data:` URI needs no door opened in the CSP that is not open already. What
    /// the icon is held against is not here — the name is one this build draws and the bytes are
    /// a drawing a mask can be read off, both settled by the check.
    pub(crate) icons: std::collections::BTreeMap<String, String>,
    /// The font it carries, where it carries one the check took.
    pub(crate) font: Option<SkinFontDto>,
    /// The pictures it lays, by the place each one goes (`c-bg`, `c-surface`, `c-sunken`,
    /// `c-pane-bg`). Empty on a skin that carries none, which is every skin that is not packed.
    pub(crate) backgrounds: std::collections::BTreeMap<String, SkinBackgroundDto>,
    /// What the skin's file was when these tables were read off it, for the window to put on the
    /// addresses it builds. A skin taken in again under the same name keeps the name and the
    /// filenames inside it, so without this the webview would draw the pictures it already has.
    /// `built-in` on the ones that ship inside the build, whose materials cannot change under a
    /// running window.
    pub(crate) stamp: String,
}

/// One skin this device holds, as the settings screen lists it. The fields are the header's, and
/// `error` is what came back where the file would not read — it is one of the person's own files,
/// sitting in the directory, so it is listed rather than left out.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SkinRowDto {
    pub(crate) name: String,
    pub(crate) title: String,
    /// The name per language, on the terms `SkinTablesDto` holds it. Empty on a row that would not
    /// read, the way the rest of the header is.
    pub(crate) titles: std::collections::BTreeMap<String, String>,
    pub(crate) author: Option<String>,
    pub(crate) version: Option<String>,
    /// The sides the author says they made, `light` and `dark` in the order written.
    pub(crate) themes: Vec<String>,
    pub(crate) license: Option<String>,
    pub(crate) homepage: Option<String>,
    /// Why this file could not be read, where it could not. `null` on every skin that reads.
    pub(crate) error: Option<String>,
    /// The face it carries and the licence that face is under, where it carries one the check
    /// took. The licence in full is not here — it is a document, and it is fetched when it is
    /// opened rather than on every listing.
    pub(crate) font_family: Option<String>,
    pub(crate) font_license: Option<String>,
    /// The name of the file this device keeps the skin under, extension and all. `null` for the
    /// four that ship inside the build, which are held in no file — which is also what says
    /// whether there is anything to write out.
    pub(crate) file_name: Option<String>,
}

/// What this device holds, and which of them is on. `on` may name a skin that is not in `skins` —
/// the file can be moved aside from underneath the setting, and saying so is better than quietly
/// showing nothing selected.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SkinListDto {
    pub(crate) on: Option<String>,
    pub(crate) skins: Vec<SkinRowDto>,
}

/// One thing the check set aside while reading a file the reader is about to take in. `theme` is
/// `null` for a header key, which belongs to no side.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SkinWarningDto {
    /// `unknown` (a name this build has no meaning for), `closed` (one it keeps to itself),
    /// `notText` (a value that did not arrive as text), `unsafeValue` (one that is text and is not
    /// a shape a value may have), `scale` (a multiplier not taken as written), `frame` (a frame
    /// value not taken as written), `choice` (a word outside the list this build draws),
    /// `background` (a background that is not laid), `icon` (a drawing that is not laid in place
    /// of an icon), or `font` (the one embedded font).
    pub(crate) kind: String,
    pub(crate) theme: Option<String>,
    pub(crate) key: String,
    /// Why, where the kind alone does not say it — in English, the way a refusal reads. `null` for
    /// the three that are about one named key and say the whole of it by kind.
    pub(crate) detail: Option<String>,
}

/// One pairing that came out under its floor, with the numbers. The names are token names and the
/// numbers are numbers, so the row reads the same in every language.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SkinReadingDto {
    pub(crate) theme: String,
    pub(crate) ink: String,
    pub(crate) ground: String,
    pub(crate) ratio: f64,
    pub(crate) floor: f64,
}

/// What reading one file gave, for the screen that is about to take it in. A file the check turns
/// away does not come back as one of these — it comes back as the command failing, because there is
/// nothing of it to show and nothing to decide.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SkinJudgementDto {
    pub(crate) name: String,
    pub(crate) title: String,
    /// The name per language, on the terms `SkinTablesDto` holds it — here as well, because the
    /// panel names the skin before it is taken in, and it names it to the same reader.
    pub(crate) titles: std::collections::BTreeMap<String, String>,
    pub(crate) author: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) themes: Vec<String>,
    /// The version of the skin already kept under this name, where one is. `Some(None)` cannot be
    /// spelled here, so a held skin whose author wrote no version comes back as `held` with a null
    /// version — the screen says "the one there" rather than naming it.
    pub(crate) held: bool,
    pub(crate) held_version: Option<String>,
    pub(crate) warnings: Vec<SkinWarningDto>,
    /// The pairings under their floor, worst first.
    pub(crate) short: Vec<SkinReadingDto>,
    /// The colours no number could be read from, as `side.name`.
    pub(crate) unread: Vec<String>,
    /// The grounds a picture is laid over, by token name. Nothing on one of them was measured, so
    /// the screen says so rather than leaving a reader to read `measured` as the whole screen.
    pub(crate) covered: Vec<String>,
    /// How many pairings were measured, so "nothing fell" is told from "nothing ran".
    pub(crate) measured: u32,
    /// The font it carries, where the check took one. Here as well as in the tables because the
    /// screen asks what the face has glyphs for before the file is taken in, not after.
    pub(crate) font: Option<SkinFontDto>,
    /// Every file the skin carries beside its document, in the order its zip holds them. What the
    /// file holds rather than what the document names, so a reader is shown the whole of what
    /// would land on their machine.
    pub(crate) carries: Vec<SkinMaterialDto>,
}

/// One file a skin carries, for the panel that lists what is in it before it is taken in.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SkinMaterialDto {
    /// The name it has in the zip, which is the name the document points at it by.
    pub(crate) file: String,
    /// What it weighs unpacked.
    #[ts(type = "number")]
    pub(crate) bytes: u64,
    /// What the bytes turned out to be — `png`, `jpeg`, `webp`, `svg`, `woff2` — or `null` where
    /// they are neither a picture nor a face. A form is not translated: it reads the same in every
    /// language, the way a token name and a ratio do.
    pub(crate) kind: Option<String>,
    /// Where the document points at this file, as the panel writes a key — `font_file`,
    /// `backgrounds.<place>` or `icons.<name>`. `null` where nothing in the document names it,
    /// which is a file that rides along and is drawn with nowhere.
    pub(crate) named_at: Option<String>,
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
    /// `todo` / `in_progress` / `done` / `blocked` / `rejected` for a task, `decided` / `rejected`
    /// for a decision.
    pub(crate) status: String,
    /// Tasks only, and only where one was set.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) priority: Option<String>,
    /// Whether the writing is still unfinished — decisions only, and `false` for a task. The status
    /// cannot stand in for it: a decision is `decided` from the moment it is saved (`AMB-D-918`).
    pub(crate) draft: bool,
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

/// A folder to work in and the project it belongs to — the first loop's one press, on its way from
/// the ledger to the workspace (`app/src/components/FirstLoop.tsx`).
///
/// It travels only when the two faces are in two windows: the press is made on the board and the
/// face is in the other window, so it goes out to the host and comes back as the `terminal-open-in`
/// event (`crate::windows::talk_raise`). In one window the same pair is handed down the tree and
/// never comes here.
///
/// **Both halves travel, and neither is filled in at the far end.** A pane belongs to a project and
/// can never be moved to another, so a folder that arrived without its project is one the face would
/// have to guess at — and that guess put a pane under the wrong project for good (`AMB-T-3708`).
// Clone because Tauri's `emit` takes the payload by value and may hand it to more than one listener.
#[derive(Clone, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct OpenInDto {
    /// The project the pane will belong to, named by the screen the press was made on.
    #[ts(type = "number")]
    pub(crate) project: i64,
    /// The folder to work in, or nothing where the ask names a pane and the record it came from
    /// holds no folder. The face checks it against that project's bindings before a pane is made,
    /// and opens nothing where the pair does not hold (`app/src/shell/WorkspaceFace.tsx`); an ask
    /// with no folder is answered on the face instead, by the question a pane is always made
    /// through (`app/src/shell/FolderChoice.tsx`).
    #[ts(optional)]
    pub(crate) dir: Option<String>,
    /// The pane the ask is about, where it is about one: the place a task or a decision was made in
    /// (`AMB-D-897`). The face goes to it where it is open, and opens it again under this same id
    /// where it is not — the id is what a provider's own home is named after, so a place opened
    /// again under a new one would be a different place (`crate::pane_home`).
    #[ts(optional)]
    pub(crate) pane: Option<String>,
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

/// The session a task or a decision was made in, for the row its detail pane draws (`AMB-D-897`).
///
/// **The way back is not in it.** The row holds three values, and the third is the handle the pane's
/// provider is resumed from — which is the one thing a webview has no business carrying. What the
/// screen needs is what to draw and what to press; going back in is asked for by naming the record,
/// and the handle is read and used where it already lives
/// (`crate::commands::task_pane_opens_again`, `crate::pty`).
///
/// **`paneName` is what the pane was called when the record was made**, which is not always what it
/// is called now. A pane still open in this run has a live name, and the screen prefers it
/// (`crate::frames::frame_names`) — this is what is left when there is no live one to prefer.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct MadeInDto {
    /// The id of the pane it was made in (`crate::frames`).
    pub(crate) pane: String,
    /// What that pane was called at the time, or nothing for a pane nobody had named.
    pub(crate) pane_name: Option<String>,
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
    /// How many skin files were written beside the restored store (a skin the destination already
    /// held under that name is left alone, and is not counted).
    #[ts(type = "number")]
    pub(crate) skins: u64,
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

/// What the plugins became, for the band that says so once (`AMB-D-884` / [`amenbo_core::handover`]).
///
/// It carries the counts and the names, never the sentence — the wording is the screen's, in the
/// reader's own language, the way [`HookNoticeDto`] hands over its slots rather than its warning.
///
/// The Viewer's line is its own field rather than a name in [`HandoverDto::plugins`], because what is
/// said about it is not "it moved": a device that was carrying starts its next send by placing the whole
/// backlog again, and that is a fact about what happens next rather than about where a setting went.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct HandoverDto {
    /// Which of the four were installed on this device, in the order the migration takes them.
    pub(crate) plugins: Vec<String>,
    /// How many connections landed on this device's notification shelf.
    #[ts(type = "number")]
    pub(crate) targets: usize,
    /// How many projects came away with notification settings of their own.
    #[ts(type = "number")]
    pub(crate) projects: usize,
    /// Whether anything of the Viewer was taken in — its keys, or the switch.
    pub(crate) viewer: bool,
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
    /// Attachment rows swept because the record they hung off is gone. Counted apart from
    /// `reclaimed_blobs`: that counts **files**, and a `url`-mode orphan frees none.
    pub(crate) swept_attachments: usize,
    pub(crate) reclaimed_blobs: usize,
    pub(crate) freed_bytes: usize,
    pub(crate) forgotten_bindings: usize,
}

/// One agent a folder's pane could be opened with, and what the folder and this machine say about
/// it (`crate::wake`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct WakeCandidateDto {
    /// The id, which is what a face hands back when the reader picks this one — a catalog row's, or
    /// one of this device's own registrations.
    pub(crate) id: String,
    /// The product's own name for itself, or what the reader called their own row.
    pub(crate) label: String,
    /// What it is started as — shown where it is missing, because that is the word to install. For a
    /// registered row it is the first word of `line`, which is the whole of what can be looked for.
    pub(crate) command: String,
    /// The command line as the reader wrote it — present **exactly** for a row they registered
    /// themselves (`AMB-D-794`), and absent for a catalogued one.
    ///
    /// Its presence is what tells a face which rows it may correct and delete, and its text is what
    /// the face shows before the row is pressed: a registered line runs in a terminal, so somebody
    /// choosing it should be able to read what that starts.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) line: Option<String>,
    /// Whether this folder shows a trace of the provider being used here. Always false for a
    /// registered row: a trace is left against a wiring catalog row, and a registration has none.
    pub(crate) traced: bool,
    /// Whether this machine can start it. Never more than that: see `amenbo_core::wake`.
    pub(crate) installed: bool,
}

/// One model an agent can be started on, as a face draws it (`AMB-D-865`).
///
/// **Two spellings, and both cross.** `id` is what the agent is started with and `label` is what the
/// reader is shown, and for Gemini CLI they are different words for the same model — so a face
/// drawing `id`, or a launch passing `label`, would each be showing or saying something nobody
/// recognises (`amenbo_core::agent_models`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AgentModelDto {
    /// The spelling the agent takes behind its model flag, exactly as the agent gave it.
    pub(crate) id: String,
    /// The agent's own name for it, for a person to read.
    pub(crate) label: String,
}

/// What an agent answered when it was asked what it can be started on (`AMB-D-865`).
///
/// **`current` is read and never written.** It is the model the agent comes up on when it is handed
/// no model flag, as the agent itself says — so a face can put a name where it would otherwise draw
/// "the agent's own default" and nothing more. Two of the six say which that is; for the rest it is
/// `null`, which is "the agent did not say" and never "there isn't one". Keeping it as this device's
/// choice would put the name on every launch and turn "whatever the agent is on" into "whatever it
/// was on the day this was read" (`amenbo_core::agent_models::Answer`).
#[derive(Default, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AgentModelListDto {
    /// Everything the agent offered, in the order it gave them.
    pub(crate) models: Vec<AgentModelDto>,
    /// The `id` of the one it is on now, or `null` where it did not say.
    pub(crate) current: Option<String>,
}

/// What a model choice this device has already made looks like to a face (`AMB-D-865`).
///
/// **Two lists, because they answer two different questions.** `chosen` is what the next pane opened
/// with this agent starts on, and `null` is the agent's own default — the state a person is in
/// before they have chosen and the one they go back to. `history` is what was chosen for it before,
/// newest first, and it is the whole of what a face has to offer for an agent whose command cannot
/// be asked for a list at all (`amenbo_core::config::Config::agent_model_history`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AgentModelKeptDto {
    /// What this agent comes up on, or `null` for the agent's own default.
    pub(crate) chosen: Option<AgentModelDto>,
    /// What was chosen for it before, newest first.
    pub(crate) history: Vec<AgentModelDto>,
    /// The flag this agent's command takes a model behind
    /// (`amenbo_core::harness::Launch::model_flag`), so a face can draw the line a press will run
    /// before it is pressed — the same thing the registration form draws before it saves
    /// (`AMB-D-794`). `null` for a command the reader registered themselves: that line is theirs as
    /// they wrote it and nothing is added to it.
    pub(crate) flag: Option<String>,
}

/// What a running pane is asked to change model with, and what a press would actually put into it
/// (`AMB-D-865`, `crate::wake::wake_switch`).
///
/// **The first three are drawn before the press and the last three are what the press does.** What is
/// typed, whether the model's name goes on that line, and where the machine keeps the change
/// afterwards are the whole of what a person needs to judge it — and two of the six read a named line
/// as a prompt and bill for it (`AMB-T-4581`).
///
/// **Where the name may go is answered here and nowhere else.** A face that composed the line itself
/// would be a second answer to the question this catalog exists to hold, and the two would drift on
/// the row where it costs money (`amenbo_core::harness::switching`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AgentSwitchDto {
    /// The provider's own command, as it is typed — `/model`, or `/models` for OpenCode.
    pub(crate) command: String,
    /// Whether the model's name may go on that command's line.
    pub(crate) carries: AgentSwitchCarriesDto,
    /// Where this machine keeps the change past the session — a path in the reader's home — or
    /// `null` where the provider changes only the session in front of them.
    pub(crate) keeps: Option<String>,
    /// What goes into the pane's input box and is submitted, as a person typing it would.
    pub(crate) line: String,
    /// What is pasted after that line has gone, submitting nothing — the picker's own search text —
    /// or `null` where there is no second half.
    pub(crate) then: Option<String>,
    /// Whether the line settles the model on its own. `false` is the provider's picker left standing
    /// open with the choosing still to do, which is a thing to say rather than a model to claim has
    /// moved.
    pub(crate) settles: bool,
}

/// Where a model's name may go when a running pane is moved (`amenbo_core::harness::Carries`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "snake_case")]
pub enum AgentSwitchCarriesDto {
    /// On the command's own line, and the return settles it: `/model sonnet`.
    Named,
    /// Nowhere. The line is the command alone and it opens the provider's picker, where the choosing
    /// is the person's — a name behind it would be sent to the model as a prompt.
    Picker,
    /// In the picker's search box, pasted after the command has opened it and submitted by nobody.
    Filter,
}

/// Whether what a row says about being installed was got from this machine at all (`AMB-D-792`).
///
/// **It is not `candidates` being empty, and it cannot be read off one.** The row is the whole
/// catalog whatever this machine has on it, so nothing in the list moves when the probe fails —
/// every row simply says `installed: false`, which is the same shape as a machine that really has
/// none. Drawn as that, a machine with four agents on it tells its owner they installed nothing.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "snake_case")]
pub enum WakeReachDto {
    /// This machine answered, so every row's `installed` is a fact about it — including a row that
    /// says false, and including an answer where none of them is true.
    Answered,
    /// This machine could not be asked: the probe's shell would not start, or was still reading the
    /// reader's profile when the deadline ran out (`crate::launch::installed`). **No row's
    /// `installed` means anything**, and what the reader is owed is that it could not be checked and
    /// a way to ask again — never the word "installed" in either direction.
    Unreachable,
}

/// Which agent a folder's pane opens with, and what to put to the reader when that is not settled.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct WakeDto {
    /// The folder this answers for, canonical — what the face opens the pane in, so that the pane
    /// and the answer are about the same folder.
    ///
    /// Absent where the question was asked about a project rather than about one folder
    /// (`crate::wake::wake_choices`): a project's folders are several, and none of them is the
    /// one a pane that has not been opened yet will run in.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) folder: Option<String>,
    /// Every catalogued agent, in catalog order. The install notice is drawn from this, which is why
    /// the ones this machine does not have are here too.
    pub(crate) candidates: Vec<WakeCandidateDto>,
    /// The ids the row is drawn from, in catalog order — every catalogued agent (`AMB-D-792`).
    ///
    /// **Not the ids a press may open.** Which of them this machine can start is each row's own
    /// `installed`, and a face that opened one without reading it puts a pane on `command not
    /// found`. It is the whole catalog because a provider left off the row is one the reader cannot
    /// install their way onto.
    pub(crate) offered: Vec<String>,
    /// The id to open with, when nothing needs asking. `None` is the question — put to a person as a
    /// row with nothing on it, whether or not anything on that row can be pressed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) settled: Option<String>,
    /// What this project has settled on **as written** ([`amenbo_core::config::Config::agent_for`]),
    /// rather than what the rank arrives at.
    ///
    /// The two come apart on a machine with one agent installed, where `settled` names it and
    /// nothing was ever kept. A face asking the reader to change the answer draws this: "nothing
    /// kept" and "kept the only one there is" read the same off `settled` and are different answers
    /// to the question being put.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) kept: Option<String>,
    /// Whether this machine was reached at all, which is what every row's `installed` stands or
    /// falls on ([`WakeReachDto`]).
    pub(crate) reach: WakeReachDto,
}

/// One thing an AI said about the session it is running in, on its way to the pane drawing it.
///
/// It is the surface layer's record ([`amenbo_core::session::Said`]) in the shape the webview reads.
/// Every spoken verb carries one line, which is `text`; `briefed` is not spoken and carries none
/// (`AMB-D-805`), and `made` is not spoken either and carries the record it filed instead
/// (`AMB-D-897`).
// Clone for the same reason `PtyChunkDto` is: `emit_to` takes its payload by value.
#[derive(Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SessionSaidDto {
    /// The pane it was said in — the same id the terminal was opened under.
    pub(crate) session: String,
    #[ts(type = "\"name\" | \"briefed\" | \"made\"")]
    pub(crate) verb: &'static str,
    /// When it was said (RFC3339 UTC).
    pub(crate) at: String,
    /// The folder the agent was in when it said it. It starts as the one the terminal was opened in
    /// and moves with every `cd`, so it is the agent's own rather than the pane's.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) cwd: Option<String>,
    /// The line said.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) text: Option<String>,
    /// The record a `made` statement says was filed from this pane, and `None` on every other verb.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) made: Option<SessionMadeDto>,
}

/// A record a create left the pane a note about (`AMB-D-897`) — the two things it takes to open one.
///
/// **A pair rather than two fields beside the line**: the space and the number mean nothing apart,
/// and a verb carrying one of them would be a statement the band could count and not open.
// Clone for the same reason [`SessionSaidDto`] is: `emit_to` takes its payload by value.
#[derive(Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct SessionMadeDto {
    /// Which of the two spaces it is in.
    #[ts(type = "\"task\" | \"decision\"")]
    pub(crate) kind: &'static str,
    /// Its number, which is what the press opens. `number` on the TS side, like every other id the
    /// webview draws: a `bigint` cannot be written into a ref or handed to the command that opens one.
    #[ts(type = "number")]
    pub(crate) id: i64,
}

/// What one frame of the talk window is called, and who called it that — which is what says whether
/// the next naming may replace it ([`amenbo_core::frames`]). It is this run's: a name is about a
/// place, and the places go when the app does (`crate::frames`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FrameNameDto {
    /// The frame it is the name of. Names belong to frames, never to the session running in one.
    pub(crate) frame: String,
    pub(crate) name: String,
    #[ts(type = "\"session\" | \"person\"")]
    pub(crate) by: &'static str,
}

/// A terminal this process has open, as a pane putting itself up is told about it.
///
/// It answers both of the pane's ways in: the terminal it just started, and the one it found already
/// running and adopted (`crate::pty::pty_sessions`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PtySessionDto {
    /// The id the terminal was opened under.
    pub(crate) session: String,
    /// The folder the terminal is running in, as the filesystem spells it — `None` for one opened
    /// without any.
    ///
    /// It is here for the pane that **adopts** this session: where a terminal runs was settled when it
    /// started, and a frame that took one over rather than starting it has no other way to learn it. A
    /// frame that does not know would have to ask the person for the folder again the next time it has
    /// a terminal to start, which is the one flow the face has asked for twice.
    pub(crate) folder: Option<String>,
    /// The id the agent in this terminal was started as — a catalog row
    /// (`amenbo_core::harness::LAUNCHES`) or a command the reader registered — or `None` for a plain
    /// prompt.
    ///
    /// It is here for the reason `folder` is, and for the same pane: what is running was settled when
    /// the terminal started, and a frame that adopted one has no other way to learn it. What reads it
    /// is the control that moves a running pane to another model — a question about the provider in
    /// the pane, so a pane that cannot name the provider draws no control (`AMB-D-865`).
    pub(crate) agent: Option<String>,
}

/// What a pane is handed when it adopts a session already running (`crate::pty::pty_attach`).
///
/// **Three things, because a pane that was not on the screen missed all three.** The bytes are the
/// screen to draw; the name is what the session called this frame; the records are what was filed
/// from it. Each of them also travels as it happens, to whichever window is drawing the pane — and a
/// window drawing no pane for this session is told with nothing listening, the drop box already read
/// past (`AMB-T-5196`). So the pane asks for all of it at the one moment it can ask.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PtyAdoptDto {
    /// The tail, in the runs it was written in.
    pub(crate) replay: Vec<PtyReplayDto>,
    /// The last name the session gave itself, or `None` where it never did. Whether it goes on the
    /// frame is the window's call: a name a person typed is not one a session may replace
    /// (`amenbo_core::frames`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) name: Option<String>,
    /// The records filed from this pane, in the order they were filed, each one once.
    pub(crate) made: Vec<SessionMadeDto>,
}

/// One run of a session's tail, as the pane adopting it is handed it (`crate::pty::pty_attach`).
///
/// **A run is as much of the tail as was written at one size**, and the tail is handed over as the
/// runs it was written in rather than as one stretch of bytes. Where a line ended was decided when
/// it was written, and an emulator told a different width folds it somewhere else — so a pane that
/// read the whole tail at one size would leave every part written before the last change folded in
/// the wrong places (`AMB-T-4514`, `AMB-T-4516`). Read run by run, each at its own size, every part
/// is folded where it was written and the reflow at the end moves all of it together.
///
/// A pane has no other way to find these sizes out. It measures the space it has been given, and
/// that is the size the tail is *going* to be drawn in: while nobody was drawing the session,
/// nothing told the host the pane had changed, so the program inside went on writing to the width
/// it was last told.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PtyReplayDto {
    /// The terminal's width in characters while these bytes were written.
    pub(crate) cols: u16,
    /// The terminal's height in characters while these bytes were written.
    pub(crate) rows: u16,
    /// The bytes, base64-encoded — the way a chunk is, and for the same reason.
    pub(crate) base64: String,
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

/// A terminal's end, on its way to the pane that was drawing it (the payload of the `pty://closed`
/// event).
///
/// **`code` is here because some endings are Amenbo's fault and the screen cannot say which.** What
/// a program leaves on the screen is usually the whole of why it stopped, and a pane says no more
/// than that it ended. The exception is a provider that stopped over a file Amenbo redirected: it
/// names the per-pane home it was pointed at, which is thrown away with the pane, so a reader who
/// follows that message edits a file nobody will read again. A provider's own exit status tells the two
/// apart without anything reading its screen — the numbers are distinct per cause and measured, and
/// which of them is worth a word is the pane's (`app/src/talk/terminal.ts`).
///
/// It is `None` where the program was ended rather than ending — a signal, or Amenbo taking the
/// pane away — and where the status could not be collected at all.
// Clone for the same reason `PtyChunkDto` is: `emit_to` takes its payload by value.
#[derive(Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct PtyClosedDto {
    /// The session whose terminal ended.
    pub(crate) session: String,
    /// What it exited with, where it exited on its own.
    #[ts(optional)]
    pub(crate) code: Option<i32>,
    /// Whether this pane was opened again on a way back a record had written down, and the way back
    /// led nowhere (`AMB-D-897`).
    ///
    /// The taking back of such a handle is not new — a program that ends within moments of starting
    /// never got as far as a session, so what was written down for it is cleared either way
    /// (`crate::frames::TalkFace::gave_up`). What is new is saying so: a person who pressed to go
    /// back into a conversation is owed the sentence that it is no longer there, where a pane that
    /// merely issued itself a handle has nothing to report.
    pub(crate) no_way_back: bool,
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
    /// Whether the repository's own ignore rules cover it. The row is drawn either way and drawn
    /// faintly for this, since what git does not record is still something somebody wrote
    /// (`AMB-D-786`) — it is the watch, and the search, that leave these out.
    pub(crate) ignored: bool,
}

/// What a move or a copy got through, and what it stopped on (`crate::folder_write`).
///
/// A carry of several rows is not one act: it is that many, taken in order, and the first failure
/// ends it. So the answer is not a yes or a no but a line through the list — these arrived, this one
/// did not and here is what the machine said, and the rest were never touched. A refusal instead
/// would say the same word for "none of them moved" and "two of the three did", which is the word a
/// reader most needs not to hear (`AMB-D-782`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderCarriedDto {
    /// The names that are now in the folder they were carried into, in the order they got there.
    pub(crate) arrived: Vec<String>,
    /// The one it stopped on. Absent when the whole list arrived.
    pub(crate) stopped: Option<FolderStoppedDto>,
}

/// The row a run over several of them stopped on, and what stopped it (`crate::folder_write`,
/// `crate::trash`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderStoppedDto {
    /// The name the run was on when it stopped.
    pub(crate) name: String,
    /// What stopped it, where what stopped it was Amenbo. Absent where the machine did, and then
    /// `why` is the whole of the answer.
    pub(crate) code: Option<FolderStopDto>,
    /// What the machine said, in the machine's own words. It is not a code with a template behind
    /// it: what a disk that filled up or a permission that was not there has to say is its own, and
    /// a sentence written here would be a guess at which of them it was.
    ///
    /// Amenbo's own refusals fill it too, in English, and a face that knows the `code` draws from
    /// its own words instead. Keeping the sentence here is what leaves a face that does not know a
    /// code something to say rather than nothing.
    pub(crate) why: String,
}

/// Why a run stopped, where it was Amenbo that stopped it (`crate::folder_write`, `crate::trash`).
///
/// **Only Amenbo's own refusals are named.** They are decided before anything is written and they
/// are the same few every time, so a screen can put them in the reader's language — which the
/// machine's own sentences cannot be, and are not asked to be. Each of these was Amenbo's English
/// standing on a Japanese screen until it had a name.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "lowercase")]
pub enum FolderStopDto {
    /// The folder being carried into already holds that name.
    Taken,
    /// A folder was being carried into itself.
    Inside,
    /// The path has no last name, so there is nothing to carry it in under.
    Nameless,
    /// The drive the row is on has no bin, and the shell there would have deleted it instead of
    /// binning it (`crate::trash`). Only one operating system can answer this, and it is the only
    /// one that builds the code raising it — a wire value, not a branch every platform reaches.
    #[allow(dead_code)]
    NoBin,
    /// The row is not in the bin any more, so there is nothing left to bring back.
    Emptied,
}

/// What went to the machine's bin, and what it stopped on (`crate::trash`).
///
/// The same line through the list a carry answers with, for the same reason: the rows go one at a
/// time, and once one of them is in the bin no single word covers the press.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderTrashedDto {
    /// The names that are in the bin now, in the order they went there.
    pub(crate) gone: Vec<String>,
    /// The one it stopped on. Absent when the whole list went.
    pub(crate) stopped: Option<FolderStoppedDto>,
}

/// What came back out of the bin, and what it stopped on (`crate::trash`).
///
/// The names are in the order they were put back, which is the reverse of the order they went in:
/// undoing a press retraces it rather than replaying it, so a folder is back before what was inside
/// it.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderRestoredDto {
    /// The names that are where they were again.
    pub(crate) back: Vec<String>,
    /// The one it stopped on. What is still in the bin stays undoable, so pressing undo again
    /// carries on from here.
    pub(crate) stopped: Option<FolderStoppedDto>,
}

/// One path git named inside the folder the file face is showing (`crate::folder_git`).
///
/// The two letters are git's own and are carried across as they came: the first is what the index
/// says about the path, the second what the working tree says, and a space in either is git saying
/// nothing about that half. Turning them into a word here would be deciding what a colour means,
/// which is the face's to decide and not the same question on every road.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct GitEntryDto {
    /// Where it is, as segments from the bound folder — the spelling a tree row is built from, with
    /// the repository's own front already taken off.
    pub(crate) path: Vec<String>,
    /// What the index says: git's `X`, one character.
    pub(crate) index: String,
    /// What the working tree says: git's `Y`, read the same way.
    pub(crate) worktree: String,
    /// Whether git named a folder as a whole rather than what is inside it, which is what it does
    /// with an untracked one. A folded folder is somewhere a colour still has to appear.
    pub(crate) is_dir: bool,
}

/// Where the branch the folder is on stands against the one it is measured by
/// (`crate::folder_git`).
///
/// It rides on the same `git status` the rows come from: the answer is one line of that call's own
/// output, so asking for it costs nothing over asking for the rows, where a `rev-list --count` to
/// find it would be a second process (`AMB-T-4899` measured 14ms for one).
#[derive(Default, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct GitBranchDto {
    /// The branch that is checked out — nothing where HEAD is not on one, which is a checkout made
    /// at a commit rather than at a branch.
    pub(crate) name: Option<String>,
    /// The branch it is measured against, spelled the way git spells it (`origin/main`) — nothing
    /// where it is measured against none, and where the one it named is gone.
    pub(crate) upstream: Option<String>,
    /// Commits this branch has that its upstream does not, and the other way round. Both are zero
    /// where there is no upstream to count against.
    pub(crate) ahead: u32,
    pub(crate) behind: u32,
}

/// What git says about one bound folder: where its branch stands, and every path it named
/// (`crate::folder_git`).
#[derive(Default, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderGitDto {
    /// Where the bound folder sits inside its repository, ending in `/` and empty where it is the
    /// repository's own root.
    ///
    /// **It is what turns a row of the tree into the name git knows it by.** The tree is drawn from
    /// the bound folder down and git answers about the repository, so a file history asked for from
    /// a tree row has to put this back on the front (`folder_git_log`).
    pub(crate) prefix: String,
    /// Nothing where the folder is no repository, and where git could not be run at all.
    pub(crate) branch: Option<GitBranchDto>,
    /// What git wrote in refusing to answer about this folder — nothing where it answered.
    ///
    /// **It is what tells a refusal apart from a folder that is no repository.** Both leave
    /// `branch` empty, and the face draws a sentence off that emptiness — so without this one, a
    /// `git status` that came back 128 is drawn as "this folder is not a repository", which is a
    /// thing nobody can act on (`AMB-T-4982`). It is git's own words and is drawn as git wrote
    /// them (`AMB-D-906`, 3-4).
    pub(crate) said: Option<String>,
    pub(crate) rows: Vec<GitEntryDto>,
    /// Whether a merge is under way here — git holding a commit it has been told to bring in, and
    /// waiting to be told the result is settled.
    ///
    /// **It is what turns the commit button into the one that ends the merge.** A commit made while
    /// this is on *is* the merge commit, and the message git has already written for it is the one
    /// it will use — so the box a reader types into is not drawn over it (`crate::folder_git`).
    pub(crate) merging: bool,
}

/// A question git or ssh is waiting on an answer to (`crate::folder_git_askpass`).
///
/// **`asked` is git's own sentence and is drawn as it came** (`AMB-D-913`) — in whatever language
/// git wrote it, and never said again in Amenbo's words. Everything else here is the frame the
/// window puts around it, which is the part that is translated.
// `Clone` is Tauri's: an event is handed to every window, so the payload is cloned per window.
#[derive(Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct GitAskDto {
    /// Which question this is, so the answer reaches the call waiting on it. It names one question
    /// in this run of the app and nothing outside it.
    #[ts(type = "number")]
    pub(crate) id: u64,
    /// What is waiting, as a person would have typed it — `git push`. The question itself never says
    /// this, and a password box that does not say what asked for it is one nobody can place.
    pub(crate) doing: String,
    /// The question, word for word.
    pub(crate) asked: String,
    /// Whether what is typed is hidden while it is typed. False for git's first question, which asks
    /// who the credential is for.
    pub(crate) secret: bool,
    /// Whether "keep it" can be offered — whether this question names a credential complete enough
    /// for `git credential approve` to be handed one.
    pub(crate) savable: bool,
}

/// One commit, as a history list draws a row of it (`crate::folder_git`).
///
/// **The parents come across and the lines do not.** `--graph` costs git a commit-graph it does not
/// always have (20ms against 38ms without one, `AMB-T-4899`), and what it draws is characters the
/// face would have to read back anyway — so the face is handed what the shape is made of and draws
/// it itself.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct GitCommitDto {
    /// The whole of it, which is what everything else here is asked by.
    pub(crate) sha: String,
    /// The same commit as git abbreviates it, which is how long this repository needs it to be.
    pub(crate) short: String,
    /// What it was made on top of — two or more is a merge, and none is the first commit there was.
    pub(crate) parents: Vec<String>,
    /// Who wrote it, as the name on the commit.
    pub(crate) author: String,
    /// When it was written, ISO 8601 with the offset it was written at — the moment kept as it was
    /// read, since what a reader wants to see it in is the face's question.
    pub(crate) at: String,
    /// The first line of the message, which is the whole of what a row has room for.
    pub(crate) subject: String,
}

/// One stash, as the list a reader restores from draws a row of it (`crate::folder_git`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct GitStashDto {
    /// What git knows it by — `stash@{0}`. It is where the stash sits in the list rather than a name
    /// of its own: making another one moves every one of them down, which is why a row is restored
    /// by the name the list was just read with and not by one kept from an earlier read.
    pub(crate) name: String,
    /// The line git wrote on it — the branch it was made on and the message, together
    /// (`On main: what I was in the middle of`). It is git's own sentence and is carried across as
    /// it came, the way every other word from git here is.
    pub(crate) message: String,
    /// When it was made, ISO 8601 with the offset it was made at — the moment kept as it was read,
    /// the way a commit's is.
    pub(crate) at: String,
}

/// One path a commit touched, as the layer under a commit draws a row of it (`crate::folder_git`).
///
/// The path is the one git knows, from the repository's root: a commit is the repository's and
/// reaches files the bound folder does not hold.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct GitFileDto {
    /// Where it is after the commit.
    pub(crate) path: String,
    /// Where it was before, where the commit moved it — and nothing where it did not.
    pub(crate) from: Option<String>,
    /// Lines added and lines taken away. Both are nothing for a file git counts no lines in, which
    /// is what it says of one it reads as bytes rather than as text.
    pub(crate) added: Option<u32>,
    pub(crate) removed: Option<u32>,
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

/// The word that one watched folder moved, and the two things about the watch itself that the face
/// cannot see for itself (`crate::folder_watch`).
///
/// **It carries nothing about what moved.** The face goes and asks — the tree for the names, git
/// for the colour beside them (`AMB-D-785`) — and what arrives here is the moment to ask, not the
/// answer. A list carried along would be a second copy of the truth to keep in step with the disk's.
///
/// `capped`, `unwatched` and `gone` are what is left, and none can be worked out from asking. Where
/// every folder needs a watch of its own the kernel's limit is per user, so some may be refused
/// while the rest work; drawn as a whole watch, that reads as "nothing has changed" in the half
/// nobody is watching. And a folder that was removed answers the same as one nobody has written in,
/// which is the one of the two worth telling a reader about.
///
/// **The two unwatched halves are separate answers, not one flag with two causes** (`AMB-D-778`).
/// A folder too big to walk to the end of and a machine whose watches have run out are different
/// things to have happened, and what a reader can do about them is different too — folded into one
/// sentence, neither of them can be acted on.
// Clone because the answer to the first call is also what the thread starts out holding.
#[derive(Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderChangesDto {
    /// Which folder this is about, spelled the way the caller asked for it.
    ///
    /// A project can be bound to several folders and each is watched on its own, so an answer that
    /// did not name one would leave a face with several of them unable to say which of its
    /// sections had moved. It is the caller's own spelling rather than the canonical one because
    /// the caller is what has to match it against a folder it is already drawing.
    pub(crate) root: String,
    /// Whether the walk stopped at its cap before it reached the end of the folder
    /// ([`crate::folder_walk::Scan::capped`]). What was never walked is not watched either, and it is
    /// the size of the folder that decided which part that is.
    pub(crate) capped: bool,
    /// Whether the kernel refused a watch this folder needs — its per-user limit, which the
    /// reader's editor is drawing on at the same time.
    ///
    /// Which folders went unwatched is deliberately not carried: they are whichever ones the walk
    /// happened to reach last, so a reader told the names would be told nothing about their own
    /// project (`AMB-T-3753`).
    pub(crate) unwatched: bool,
    /// Whether the folder itself is no longer there. It can come back: a folder made again where
    /// this one was is watched again, and this goes back to false.
    pub(crate) gone: bool,
}

/// How a file's lines end — the wire form of [`crate::encoding::LineEnding`].
///
/// `mixed` is a value of its own rather than the commoner of the two rounded up, because an editor
/// hands back one kind of newline for both and nothing could tell them apart again: a file written
/// back in the commoner kind comes out changed on every line that was the other kind. What to do
/// about one is the reader's to say (`AMB-D-773`).
///
/// It is the one folder answer that travels **both** ways: a file is written back in the newline it
/// was read in, and this side remembers nothing between the two calls, so what the read handed the
/// panel is what the save takes back (`crate::folder_save`).
#[derive(Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "snake_case")]
pub enum FolderLineEndingDto {
    /// Every newline is `\n` — or there is none at all, which writes back the same either way.
    Lf,
    /// Every newline is `\r\n`.
    Crlf,
    /// Both, in the same file.
    Mixed,
}

/// What a file has to show for itself, as far as a panel can show it (`crate::folder`).
///
/// At most one of `text`, `image`, `pdf` and `oversize` is filled, and all four are empty for a
/// file that is none of them — what a reader is then told is that it cannot be read here, which is
/// the honest answer for a binary. Text is cut at a cap, because a panel is not a pager and a very
/// long file would be paid for in full to draw a screen of it.
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
    /// The picture, where the bytes say they are one and there are few enough of them to draw.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) image: Option<FolderImageDto>,
    /// The PDF, where the bytes say they are one and there are few enough of them to draw.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) pdf: Option<FolderPdfDto>,
    /// The file that was refused, where it is a picture or a PDF and there is too much of it to
    /// carry.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) oversize: Option<FolderOversizeDto>,
    /// What the bytes were read as (`UTF-8`, `Shift_JIS`, …), for a file that is text. It travels
    /// because a file is written back in what it was read in, and because a guess that went wrong is
    /// only visible to the reader — who cannot be asked about an encoding nobody named to them
    /// (`AMB-D-773`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) encoding: Option<String>,
    /// Whether the file began with a byte order mark. Encoding text does not put one back, so
    /// nothing but this remembers that 178 files in a real folder have one.
    pub(crate) bom: bool,
    /// How its lines end.
    pub(crate) line_ending: FolderLineEndingDto,
    /// Whether writing this text back would produce the bytes that were read. A file that is not
    /// clean — cut at the cap, not wholly decodable, or in an encoding nothing here writes — is one
    /// to read and not to save.
    pub(crate) clean: bool,
    /// The short mark of the bytes this was read from, for a file the panel draws
    /// (`crate::folder_bytes::digest`).
    ///
    /// **It is what the panel knows the file by while it has it open** (`AMB-D-784`). The folder
    /// says it moved and the panel reads again: a mark that came back the same is this file
    /// standing still, and one that came back different is somebody having written to it. The same
    /// mark travels back into the save, which refuses to write over a file that moved since — this
    /// side remembers nothing between the two calls, so what remembers is the panel.
    ///
    /// **A picture is marked too, over the whole of it** (`AMB-D-797`), and so is a PDF. Their bytes
    /// are not carried here, so the mark is also what makes the redraw happen: it rides on the
    /// door's URL, and an `<img>` whose address never changes is never fetched again however the
    /// file moved.
    ///
    /// Absent where there is nothing drawn to mark, or where marking it would mean reading a file
    /// with no cap on it: a file too large to draw, and a binary, both come back without one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) digest: Option<String>,
}

/// A file the panel would not carry — a picture (`AMB-D-783`) or a PDF (`AMB-D-907`) — and what it
/// was measured against.
///
/// **The numbers travel because silence reads as a broken file.** A reader shown nothing where a
/// picture was concludes the file is damaged; one shown how large it is concludes it is large, and
/// goes on to open it in something built for that.
///
/// The pixels belong to a picture alone, and they are there because two caps are being kept and
/// they guard different things: the bytes stand for what the host would hold, the pixels for what
/// the webview would decode. They are absent where the front of the file did not say — a picture
/// whose size could not be read is let through on the bytes alone — and absent for a PDF, which is
/// refused on its bytes and nothing else. So a refusal with no size in it is always a refusal about
/// bytes.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderOversizeDto {
    /// The whole file, in bytes.
    #[ts(type = "number")]
    pub(crate) bytes: u64,
    /// How wide the picture says it is, where the front of the file said.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) width: Option<u32>,
    /// How tall it says it is, on the same terms as `width` — the two are always both there or both
    /// absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) height: Option<u32>,
}

/// A batch of what a folder-wide search has found so far (`crate::folder_search`).
///
/// **The answer arrives in pieces because it is found in pieces.** The first one percent of the
/// hits is ready in 5-9 ms and the last of them a second later (`AMB-T-4917`), so a face handed the
/// whole answer at the end would stand still through a search that was all but done at once.
///
/// `tag` is the face's own count of which search this is, handed to the command and given back on
/// every event: a batch from a search the reader has already typed past is dropped rather than
/// drawn.
#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderSearchFoundDto {
    /// Which folder this is about, spelled the way the caller asked for it.
    pub(crate) root: String,
    /// Which search it belongs to.
    #[ts(type = "number")]
    pub(crate) tag: u64,
    /// The files in this batch. A file appears in one batch only, with all of its lines.
    pub(crate) files: Vec<FolderSearchFileDto>,
}

/// One file a search found something in.
#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderSearchFileDto {
    /// The path under the searched folder, one name per segment - the same spelling the reading
    /// doors take (`crate::folder_bytes::folder_read`).
    pub(crate) path: Vec<String>,
    /// The mark of the bytes the search read (`crate::folder_bytes::digest`). A replacement hands
    /// it back, and a file that no longer answers to it is left alone rather than written at the
    /// places a search of the file as it was found them (`AMB-D-911`).
    pub(crate) digest: String,
    /// The matching lines, in the order they stand in the file.
    pub(crate) lines: Vec<FolderSearchLineDto>,
    /// Whether this file holds more than is carried here - more matching lines than one file's
    /// share, or more matches on one line than are marked. A minified bundle is one file and one
    /// line, and it can hold thousands of either.
    pub(crate) more: bool,
}

/// One matching line, and where on it the matches are.
#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderSearchLineDto {
    /// Which line of the file, counted from one.
    pub(crate) line: u32,
    /// Where `text` starts in the line, in UTF-16 code units. Zero unless the line was too long to
    /// carry whole.
    pub(crate) from: u32,
    /// The line itself, or - where it was too long - a window of it holding the first match.
    pub(crate) text: String,
    /// Whether `text` is a window rather than the whole line.
    pub(crate) cut: bool,
    /// Every match on the line, counted from the start of the **line** and not of `text`.
    pub(crate) spans: Vec<FolderSearchSpanDto>,
}

/// Where one match sits on its line.
///
/// **Counted in UTF-16 code units**, which is what a webview counts a string in: the face slices
/// the line it is handed with these numbers, and a count in bytes or in characters would be wrong
/// by however much of the line is Japanese or an emoji respectively.
#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderSearchSpanDto {
    /// Where the match starts.
    pub(crate) at: u32,
    /// How long it is.
    pub(crate) length: u32,
}

/// That a search is over, and how it ended (`crate::folder_search`).
#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderSearchDoneDto {
    /// Which folder this is about, spelled the way the caller asked for it.
    pub(crate) root: String,
    /// Which search it belongs to.
    #[ts(type = "number")]
    pub(crate) tag: u64,
    /// How many files were read and looked through.
    pub(crate) files: u32,
    /// How many matching lines were sent.
    pub(crate) hits: u32,
    /// Whether the search stopped at the ceiling on the answer rather than at the end of the
    /// folder. What is drawn is then a part of what is there, and a face that did not say so would
    /// be telling the reader there is no more.
    pub(crate) capped: bool,
    /// Whether it was called off - by the reader leaving, or by their next search taking over.
    pub(crate) stopped: bool,
}

/// One file a replacement is to be written into, named the way the search named it
/// (`crate::folder_replace`).
#[derive(Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderReplaceFileDto {
    /// The path under the searched folder, one name per segment.
    pub(crate) path: Vec<String>,
    /// The mark the search read this file by ([`FolderSearchFileDto::digest`]). A file that does not
    /// answer to it any more is left alone (`AMB-D-911`).
    pub(crate) seen: String,
    /// Which of the file's hits to write over. A file named with none is a file this run has
    /// nothing to do to, and it is dropped before anything is checked.
    pub(crate) at: Vec<FolderReplaceAtDto>,
}

/// Where one replacement goes, in the numbers the search gave for it.
#[derive(Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderReplaceAtDto {
    /// Which line of the file, counted from one.
    pub(crate) line: u32,
    /// Where on the line it starts, in UTF-16 code units ([`FolderSearchSpanDto`]).
    pub(crate) at: u32,
    /// How long it is, in the same units.
    pub(crate) length: u32,
}

/// What a replacement run came to (`crate::folder_replace`).
///
/// **Three lists rather than one answer**, because a run over many files has three outcomes and a
/// reader needs all of them: what was written, what was left alone and why, and — where the
/// filesystem stopped it part way — which file it stopped on. What was written before that stays
/// written; there is no undo of Amenbo's own (`AMB-D-911`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderReplacedDto {
    /// The files written, in the order they were.
    pub(crate) done: Vec<FolderReplacedFileDto>,
    /// The files left alone, each with why.
    pub(crate) skipped: Vec<FolderSkippedFileDto>,
    /// Where the run stopped, where it did not reach the end.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) stopped: Option<FolderStoppedFileDto>,
}

/// One file a replacement was written into.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderReplacedFileDto {
    /// The path under the searched folder.
    pub(crate) path: Vec<String>,
    /// How many places were written over.
    pub(crate) hits: u32,
    /// The mark of what was written, which is what a panel holding this file goes on knowing it by.
    pub(crate) digest: String,
}

/// One file a replacement was not written into, and why.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderSkippedFileDto {
    /// The path under the searched folder.
    pub(crate) path: Vec<String>,
    /// Which of the reasons it was.
    pub(crate) why: FolderSkippedDto,
}

/// Why one file of a replacement was left alone.
///
/// Every one of these is about that file alone, which is what separates them from the refusal that
/// stops a whole run before it starts: a file nothing may write to is the run's business, because a
/// run half applied over a folder is the thing this shape has to avoid (`AMB-D-911`).
#[derive(Clone, Copy, Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub enum FolderSkippedDto {
    /// It moved between the search and the replacement, so the places found in it name something
    /// else now.
    Changed,
    /// It is no longer there, or no longer a file.
    Unreadable,
    /// Its bytes are not text — a picture put where a text file was.
    NotText,
    /// Its bytes and its text are not the same thing said twice, so writing the text back would
    /// change what was not replaced: a file cut at the read cap, or one in an encoding nothing here
    /// promises to write (`AMB-D-773`).
    NotClean,
    /// A place named does not land on this file, although its mark says it has not moved — a line
    /// past its end, a column inside a character, or two places over each other.
    Unplaced,
    /// The pattern the replacement was asked with no longer calls any of this file's places a match,
    /// so there are no groups to read out for them. It is the seam being checked rather than the
    /// file: the mark says the bytes have not moved, so this is a caller that looked with one
    /// pattern and wrote with another (`crate::folder_replace`).
    Unmatched,
    /// The replacement holds a character this file's encoding cannot write — a tick typed into a
    /// Shift_JIS file (`AMB-D-773`).
    Unwritable,
}

/// The file a replacement run stopped on, and what the machine said.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderStoppedFileDto {
    /// The path under the searched folder.
    pub(crate) path: Vec<String>,
    /// What the filesystem said, in its own words — a disk that filled up, a folder taken away. A
    /// sentence written here instead would be a guess at which it was.
    pub(crate) reason: String,
}

/// The rows a name filter found under one bound folder (`crate::folder::folder_names`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderNamesDto {
    /// What was found, in the order the tree reads them.
    pub(crate) rows: Vec<FolderNameDto>,
    /// Whether the walk stopped at a cap rather than at the end of the folder — either too many
    /// matches to draw, or too many names to look at. The face says so: a list that stopped and did
    /// not say would be telling the reader there is no more.
    pub(crate) capped: bool,
}

/// One name a filter found, wherever in the folder it stands.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderNameDto {
    /// Its path under the bound folder, one name per segment.
    pub(crate) path: Vec<String>,
    /// Whether it is a folder.
    pub(crate) is_dir: bool,
    /// Whether the repository ignores it — drawn all the same, and drawn as ignored
    /// (`AMB-D-786`).
    pub(crate) ignored: bool,
}

/// What a drop asked for, as the keys held at the moment it landed say it (`crate::dropped`).
///
/// It crosses on its own, out of a command of its own, because **no operating system puts the
/// modifier keys on the drag event** (`AMB-T-3740`). The keyboard is read where it can be read — the
/// host, at the instant of the drop — and the answer is this and nothing else.
///
/// `default` is neither key: what a plain drop means belongs to the face receiving it, not here. It
/// travels back the other way too — the face hands it to [`crate::folder_write::folder_import`] as
/// the whole of what the reader asked of the drop.
#[derive(Deserialize, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "lowercase")]
pub enum DropEffectDto {
    /// Copy: Option on macOS, Ctrl on Windows and Linux.
    Copy,
    /// Move: Command on macOS, Shift on Windows and Linux.
    Move,
    /// Neither was held.
    Default,
}

/// A picture out of a folder, named rather than carried: the webview asks [`crate::fileproto`] for
/// the bytes at the path it already has in hand (`AMB-D-783`).
///
/// **The bytes used to come over this seam base64-encoded**, which cost a third again in size, put
/// the whole picture in one message, and held every byte of it in both processes before a single
/// pixel was drawn. The door that hands out a file by its path is fenced by the project's folders,
/// the same fence this answer was resolved through, so there is nothing left for the seam to carry.
/// A reader is meant to see the picture arrive top to bottom rather than all at once.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderImageDto {
    /// The type the bytes themselves say they are — read off the first of them, never off the name.
    ///
    /// It travels because the door is told what to serve as: the sniff happened here, and asking the
    /// webview to name the type of a file it has not read would be asking it to guess from the name.
    pub(crate) mime: String,
}

/// A PDF out of a folder, named rather than carried — the seam a picture crosses, on the same terms
/// and for the same reasons ([`FolderImageDto`], `AMB-D-907`).
///
/// **It is the one form judged before the text judgement.** A PDF written without compression holds
/// no NUL in its head, so the NUL test took it and the panel drew its insides as a document to read
/// and to write back over ([`crate::folder_bytes`]).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct FolderPdfDto {
    /// The type the bytes themselves say they are — `application/pdf`, read off the first of them.
    ///
    /// It travels rather than being written on the panel's side for the reason a picture's does: the
    /// door that hands out the bytes is told what to serve as, and the one place that has read them
    /// is this one.
    pub(crate) mime: String,
}

/// How much of a page one pane takes ([`amenbo_core::frames::PaneSize`]).
///
/// It crosses because it is the person's answer rather than a measurement: what a pane is drawn at is
/// chosen on the face and kept in the store, and the two windows hand it between themselves along
/// with the pane it is about.
#[derive(Clone, Copy, Default, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "kebab-case")]
pub enum PaneSizeDto {
    /// The whole page.
    #[default]
    Whole,
    /// Half of it, side by side.
    Half,
    /// Half of it, one above the other.
    HalfDown,
    /// A quarter of it.
    Quarter,
    /// A sixth of it.
    Sixth,
    /// An eighth of it.
    Eighth,
}

impl From<amenbo_core::frames::PaneSize> for PaneSizeDto {
    fn from(size: amenbo_core::frames::PaneSize) -> Self {
        use amenbo_core::frames::PaneSize as Size;
        match size {
            Size::Whole => Self::Whole,
            Size::Half => Self::Half,
            Size::HalfDown => Self::HalfDown,
            Size::Quarter => Self::Quarter,
            Size::Sixth => Self::Sixth,
            Size::Eighth => Self::Eighth,
        }
    }
}

impl From<PaneSizeDto> for amenbo_core::frames::PaneSize {
    fn from(size: PaneSizeDto) -> Self {
        match size {
            PaneSizeDto::Whole => Self::Whole,
            PaneSizeDto::Half => Self::Half,
            PaneSizeDto::HalfDown => Self::HalfDown,
            PaneSizeDto::Quarter => Self::Quarter,
            PaneSizeDto::Sixth => Self::Sixth,
            PaneSizeDto::Eighth => Self::Eighth,
        }
    }
}

/// The talk window's arrangement, as the window drawing the face has it (`crate::frames`).
///
/// The shape only: the frames in the order they were opened, how much of a page each takes, and the
/// folder each was working in. **What was running is not here** — a session is a process, and a pane
/// drawn as though one were still in it would be the window saying something untrue (`AMB-T-3607`).
///
/// **Where a pane sits is not here either** (`AMB-D-939`). The window lays the panes down in order
/// and the pages fall out of that, so a place carried across would be a second answer to a question
/// the order already settles.
///
/// It is what the two windows hand the face between themselves with. What of it outlives the run is
/// the store's word (`amenbo_core::frames::SavedLayout`): the project, and a row a pane — so an
/// arrangement read at the start of a run comes with the places the reader left and nothing running
/// in any of them (`AMB-D-869`).
#[derive(Clone, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct TalkLayoutDto {
    /// The project whose panes the face was showing. It is what the window the terminal is split out
    /// into opens as, where the arrangement came with no panes to name one — which is every window
    /// that comes up after a run (`app/src/shell/WorkspaceFace.tsx`); absent where nothing has told
    /// the face of a project yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) project: Option<u32>,
    /// The frames, in slot order.
    pub(crate) frames: Vec<TalkFrameDto>,
    /// The pane being worked in when the arrangement was last written — the one the window split out
    /// of the board comes up on (`AMB-D-753`). Absent where nobody has worked in a pane in this run.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) split_out: Option<String>,
}

/// One frame of the arrangement: where it sits, and what it is working on.
#[derive(Clone, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct TalkFrameDto {
    /// The id its name is held against (`FrameNameDto`).
    pub(crate) id: String,
    /// The project this pane is one of. Absent in an arrangement written before panes belonged to a
    /// project, which the window answers for (`app/src/talk/layout.ts`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) project: Option<u32>,
    /// How much of a page it takes ([`PaneSizeDto`]). It goes on to the store, because how much room
    /// a piece of work wants outlives the run (`amenbo_core::frames::SavedPane::size`).
    #[serde(default)]
    pub(crate) size: PaneSizeDto,
    /// The folder its terminal was working in, where it had one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) folder: Option<String>,
    /// The id the agent in it was started as, and absent for a plain prompt. It is read off the
    /// session rather than off what the pane asked for (`app/src/talk/layout.ts`), so a pane that
    /// took up a terminal somebody else started carries what is actually in it.
    ///
    /// It goes on to the store, because coming back to a pane means coming back to what was running
    /// in it (`amenbo_core::frames::SavedPane::agent`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) agent: Option<String>,
    /// What has been written in the box under this pane and not sent yet (`AMB-D-864`). Absent where
    /// nothing is.
    ///
    /// **It rides the arrangement because the arrangement is how the two windows hand the face over**
    /// (`crate::frames`), and it stops there: a half-written sentence is the window's, so it is held
    /// for as long as the process is up and is never written down.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) written: Option<String>,
    /// Whether this place came back with a way into what was running in it (`AMB-D-869`).
    ///
    /// **It travels one way only.** The host answers it as the arrangement comes back, and the
    /// window reads it to know which places to open without being pressed (`AMB-T-4641`); an
    /// arrangement sent the other way says nothing about it, because the handle it stands for is
    /// never a window's to hold (`crate::frames::TalkFace`).
    ///
    /// A place with no way back — a plain shell, a line the reader registered (`AMB-D-794`) — comes
    /// back false and is drawn as the place it is, with the way in on it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) resumes: bool,
    /// Whether the box under this pane is open (`AMB-D-890`). Absent in an arrangement written before
    /// it was carried, which the window answers for.
    ///
    /// **It rides the arrangement for the reason the draft does** — that is how the two windows hand
    /// the face over — **and it goes on to the store as well**, which the draft does not: a
    /// half-written sentence is the moment's, and which panes a reader writes in outlives the run
    /// (`amenbo_core::frames::SavedPane::compose_open`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) compose_open: Option<bool>,
}

/// **The store's identity, in the parts a reader has to tell apart** (`AMB-D-856`). It was one string
/// once, and one string can only answer "did anything move"; the screen's question is narrower — *what*
/// moved decides whether every query is re-read or one scope is.
///
/// - `file` is the main file's identity (mtime and size). It moves when the file is **swapped out from
///   under the app** — `fold`, `stage_and_swap`, a `backup` restore, a migration — and nothing the feed
///   holds survives that, so a reader that sees this move re-reads everything.
/// - `config` is `config.json`'s identity. That file is not in the database at all, so no row in the feed
///   will ever speak for it: a language, a theme or a default view set from the CLI moves this and nothing
///   else. A reader that sees it move re-reads everything too.
/// - `version` is `PRAGMA data_version` — the value SQLite guarantees answers "has another connection
///   committed?". It moves on every commit, ours included, and says nothing about **what** was committed;
///   that is the change feed's word.
///
/// All three are strings so the front end can compare them without knowing what is inside: a `u128` mtime
/// does not survive a JSON number, and nothing here is ever done arithmetic on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct StoreSignatureDto {
    /// The main store file's identity — `"<mtime nanos>:<size>"`.
    pub(crate) file: String,
    /// `config.json`'s identity, in the same shape.
    pub(crate) config: String,
    /// `PRAGMA data_version`, as text.
    pub(crate) version: String,
}

/// **One connection this device can send a notification through, under a name** (`AMB-D-885`) — a row on
/// the shelf, and what the form that opens over it starts filled in with.
///
/// **The credential is never here.** A Slack target's webhook URL and a mail target's SMTP password are
/// `secret` rows — the table no road out of the store walks — and reading one back into a webview would
/// put a copy of it in the place that decision keeps it out of. `secretSet` says whether one is held,
/// which is all the form needs to mask the box and all the shelf needs to say the row can send.
///
/// It is also why the shelf's line under a Slack target's name is not its URL: two Slack targets are told
/// apart by the name they were given (`amenbo_core::model::NotifyTarget::name`), and a mail target has a
/// server and an account that are not secret to show instead.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct NotifyTargetDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    /// What carries the message — and which of the fields below mean anything.
    #[ts(type = "\"slack\" | \"mail\"")]
    pub(crate) kind: &'static str,
    /// The name the person gave it, which is its whole identity on a project's screen.
    pub(crate) name: String,
    /// Does a newly created project start out pointing at this one? At most one row carries it.
    pub(crate) is_default: bool,
    /// The relay a mail target hands the message to. Absent on a Slack target, and on a mail one
    /// nobody has filled in yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) smtp_host: Option<String>,
    /// The port that relay listens on. Mail-mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub(crate) smtp_port: Option<i64>,
    /// The account to authenticate as. Mail-mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) smtp_user: Option<String>,
    /// The address the message is sent from. Mail-mode; absent falls back to the account.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) mail_from: Option<String>,
    /// Whether this target holds its credential — a Slack webhook URL, a mail password. The value
    /// itself never leaves core.
    pub(crate) secret_set: bool,
    /// How many projects have selected this target. It is read for the delete, which says what the
    /// press costs before it is made; afterwards there is nobody left to ask.
    #[ts(type = "number")]
    pub(crate) projects_using: usize,
}

/// **What a connection check found** (`AMB-D-885`) — and how much of it was actually asked.
///
/// A check either reads the settings or speaks to the server, and which of the two happened decides what
/// a screen may claim afterwards. A mail target is connected to and authenticated as, so "it works" is
/// honest; a Slack webhook has no door but posting, so all that was read is the URL's shape — and a
/// webhook revoked yesterday still has it. One `bool` rather than two commands, because the caller does
/// not choose: the target's kind does.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct NotifyCheckedDto {
    /// Was a server spoken to? False means the settings were read and nothing more.
    pub(crate) reached: bool,
}

/// **What one project does with the device's shelf** (`AMB-D-885`): whether it notifies at all, which
/// targets carry it, what it reports, and where its mail is addressed.
///
/// The four are read together because the screen draws them together, and because three of them are sets
/// rather than columns — which is what lets one project reach a Slack channel and an inbox at once.
///
/// **`enabled` is not "is a target selected".** They are deliberately apart: a fortnight away costs one
/// switch, and the selection is still standing on the way back.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ProjectNotifyDto {
    /// Does this project notify? Off keeps the targets and the events where they are.
    pub(crate) enabled: bool,
    /// Where a mail target's message is addressed — several addresses on one line, separated by commas,
    /// as the person typed them. Empty falls back to the target's own account.
    pub(crate) mail_to: String,
    /// The targets this project's notifications are carried by, as ids into the device's shelf.
    #[ts(type = "number[]")]
    pub(crate) target_ids: Vec<i64>,
    /// Which of the events below this project reports. A subset of `reportable`, and empty is an answer:
    /// a project that reports nothing stays on and reports nothing.
    pub(crate) events: Vec<String>,
    /// **The events a project may report**, in the catalog's own order — core's list rather than the
    /// screen's, so a name added there reaches the form without the form being told.
    ///
    /// `store.changed` is not among them: it says only that *something* moved, which is a signal for a
    /// mirror to re-read on and nothing a person can be told (`AMB-D-582`).
    pub(crate) reportable: Vec<String>,
}

// ───────────────────────── the Viewer (`AMB-D-884`) ─────────────────────────

/// **What a screen can say about the Viewer without asking the network** (`AMB-D-884`).
///
/// Everything here is read out of this device's own store, so it is what the settings screen draws on
/// first paint. Whether a phone may read is not here — only the server can answer that, and asking it
/// takes a round trip ([`ViewerPairingDto`]).
///
/// **None of the three secrets setup left behind is here**, and none of them is anywhere else on this
/// wire either: the address, the write token and the encryption key stay in the table no road out of the
/// store walks. `setUp` is the whole of what a screen needs from them.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ViewerStateDto {
    /// Has a server been stood up on this device? Half a route is not a route — the address, the token
    /// and the key are read together, and absence of any of them is absence of the server.
    pub(crate) set_up: bool,
    /// Does this device carry to the Viewer at all? A device that has never touched the switch answers
    /// yes: standing a server up is the act of asking for this.
    pub(crate) carrying: bool,
    /// How many records have been read out and have not landed yet.
    #[ts(type = "number")]
    pub(crate) waiting: i64,
    /// When this device last placed anything. **An empty queue cannot answer this**: it is both "the
    /// phone is up to date" and "nothing has gone out since Tuesday". Absent where nothing ever landed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) last_placed_at: Option<String>,
    /// Which build of the Worker the server last answered a write as being. **Zero is "nothing has been
    /// written yet"**, not "an old Worker" — the number travels on the answer to a write and nowhere
    /// else, so a device that has never sent has never been told one.
    #[ts(type = "number")]
    pub(crate) server_build: i64,
    /// The build this Amenbo carries. Pressing setup is what moves the one above onto it, and pressing it
    /// is the reader's to do — so the two are handed over side by side rather than compared here.
    #[ts(type = "number")]
    pub(crate) worker_build: i64,
    /// Cloudflare's token screen, with the permissions setup needs already ticked. It is the same link on
    /// every device and needs no server, so it is here for the screen that has none yet.
    pub(crate) token_link: String,
}

/// Where the phone's half of this is got — one row per kind of phone, in the order they are drawn.
///
/// **`phone` is a brand and is never translated**: it is the word a reader matches against the thing in
/// their hand.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ViewerAppDto {
    pub(crate) phone: &'static str,
    pub(crate) link: &'static str,
}

/// **Whether a phone may read, as the server answers it** (`AMB-D-884`).
///
/// There is one read code and the server never learns which phone offered it, so this says whether any
/// phone may read — never how many do, and never which.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ViewerPairingDto {
    /// Is the server holding a read code?
    pub(crate) paired: bool,
    /// When that code was issued, in the server's own words (RFC 3339). Absent when none is held.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) issued_at: Option<String>,
}

/// **A freshly drawn read code, for the screen to put in front of a camera** (`AMB-D-884`).
///
/// `carried` is the whole of what the phone reads, and **it carries the encryption key** — which is why
/// this is the one shape on this wire that holds one. It goes to the webview because that is what draws
/// the code; it belongs on a screen and nowhere a pipe or a log can take it.
///
/// Issuing replaces whatever code the server was holding, so whatever phone held the one before has
/// stopped reading.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ViewerCodeDto {
    pub(crate) carried: String,
    /// When the server wrote the code down, in its own words (RFC 3339).
    pub(crate) issued_at: String,
}

/// **What one run of setup stood up** (`AMB-D-884`).
///
/// The account and the database are named because they are what the reader now owns in their own
/// Cloudflare account. The address is here for the same reason the CLI prints it — it is where their own
/// Worker answers, and it is not a secret — while the token and the key it was built with are not.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ViewerStoodDto {
    /// Where the Worker answers.
    pub(crate) url: String,
    /// The account it was built in.
    pub(crate) account: String,
    /// The database it reads and writes.
    pub(crate) database: String,
    /// **Whether the keys already here were kept, or drawn afresh.** A key drawn now opens nothing already
    /// on the server, so every phone has to be paired again — which is the sentence a screen owes the
    /// reader after this press.
    #[ts(type = "\"kept\" | \"generated\"")]
    pub(crate) keys: &'static str,
}

/// **What one turn of carrying did** (`AMB-D-884`).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ViewerSentDto {
    /// How many records reached the server.
    #[ts(type = "number")]
    pub(crate) placed: usize,
    /// How many are still waiting behind them.
    #[ts(type = "number")]
    pub(crate) waiting: i64,
    /// Why the turn did nothing, where it did nothing on purpose. **Neither reason is a failure** and the
    /// queue is where it was under both: another run holds the turn, or this device is not carrying.
    /// Absent is a turn that ran — which may still have placed nothing, there having been nothing to place.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "\"another_turn\" | \"switched_off\"")]
    pub(crate) held_back: Option<&'static str>,
}

/// What the two ends differ by: the records this machine holds that the server is not holding, and the
/// keys the server is holding that this machine no longer has.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ViewerDriftDto {
    #[ts(type = "number")]
    pub(crate) to_place: usize,
    #[ts(type = "number")]
    pub(crate) to_drop: usize,
}

/// **What one press of "put right what has drifted" did** (`AMB-T-4763`).
///
/// `outcome` is the whole branch, and the two fields below it are filled in as it names them:
///
/// | `outcome` | `drift` | `sent` | what happened |
/// |---|---|---|---|
/// | `not_set_up` | — | — | no server on this device. Not a failure — nothing to drift from |
/// | `level` | — | — | the server holds what this machine holds |
/// | `counted` | yes | — | the difference was counted and written down. **The next press places it** |
/// | `placed` | yes | yes | the difference went on the queue, and as much of it as the server would take has gone |
/// | `sending_elsewhere` | — | — | another run is carrying, so nothing was compared |
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct ViewerRepairedDto {
    #[ts(type = "\"not_set_up\" | \"level\" | \"counted\" | \"placed\" | \"sending_elsewhere\"")]
    pub(crate) outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) drift: Option<ViewerDriftDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) sent: Option<ViewerSentDto>,
}

// ───────────────────────── automation: the definition, as a screen reads it ─────────────────────────
//
// The ten definition tables answer as three shapes: a card for the list, a detail for the build screen,
// and the launch check's verdict. A step's ways out, its inputs and its settings are read from the
// library action it points at or from the step itself (`amenbo_core::ops::automation::declarer`), and
// the resolving is done on this side — a screen that had to know which of the two declared a name would
// be drawing the storage rather than the automation.

/// **One automation in the list** — what the "automations" tab draws a row from.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AutomationCardDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    pub(crate) name: String,
    /// How many steps it is built out of. The row says it because "what is this" and "is it built
    /// yet" are the two things a list is read for.
    #[ts(type = "number")]
    pub(crate) steps: usize,
    pub(crate) archived: bool,
}

/// **One automation's whole definition** — every step, every way out of each, and what joins them.
///
/// It is fetched whole rather than paged: an automation is tens of rows, and the build screen's
/// picture, its launch check and its step panel all read the same walk from the entry step.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AutomationDetailDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    #[ts(type = "number")]
    pub(crate) project_id: i64,
    pub(crate) name: String,
    pub(crate) notes: String,
    pub(crate) preamble: String,
    /// The step a run opens its first terminal on. Absent while the automation is still being built.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub(crate) entry_step_id: Option<i64>,
    pub(crate) archived: bool,
    pub(crate) steps: Vec<AutomationStepDto>,
    pub(crate) edges: Vec<AutomationEdgeDto>,
    pub(crate) wires: Vec<AutomationWireDto>,
}

/// **One step**, with the declarations it runs under already resolved.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AutomationStepDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    pub(crate) name: String,
    /// The library action this step runs. Absent when it carries its own prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub(crate) action_id: Option<i64>,
    /// What that action is called, so a row can name it without a second read.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) action_name: Option<String>,
    /// The prompt this step carries, or the one it reads off the action — whichever it runs on.
    pub(crate) prompt: String,
    pub(crate) agent: String,
    /// Absent leaves the agent's own default model.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) model: Option<String>,
    pub(crate) interactive: bool,
    /// The name of the setting or the input the working folder is taken from — a name, not a path.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) work_dir_ref: Option<String>,
    pub(crate) report_to_task: bool,
    pub(crate) show_history: bool,
    pub(crate) exits: Vec<AutomationExitDto>,
    /// What this step takes in, in declaration order.
    pub(crate) inputs: Vec<AutomationPortDto>,
    pub(crate) settings: Vec<AutomationCfgDto>,
}

/// **A way out of a step**, and what leaving through it hands on.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AutomationExitDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    /// Absent is the unnamed way out, which is all a step with a single one needs.
    /// `*` is the error one, which every owner carries from birth.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) name: Option<String>,
    pub(crate) outputs: Vec<AutomationPortDto>,
}

/// **What a step takes, or what a way out of it hands on.**
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AutomationPortDto {
    pub(crate) name: String,
    #[ts(type = "\"value\" | \"file\" | \"task_take\" | \"task_make\"")]
    pub(crate) kind: &'static str,
    pub(crate) required: bool,
}

/// **A setting, and the answer written for it while building.** An action declares and the step
/// answers, so both are folded into one row here.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AutomationCfgDto {
    pub(crate) name: String,
    #[ts(type = "\"taskfilter\" | \"folder\" | \"choice\" | \"number\" | \"text\"")]
    pub(crate) kind: &'static str,
    pub(crate) required: bool,
    /// The choices, as JSON, for `choice`. Absent for every other kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) options: Option<String>,
    /// The answer written while building, as JSON. Absent where nobody has answered.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) value: Option<String>,
}

/// **What happens after a way out is taken.**
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AutomationEdgeDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    #[ts(type = "number")]
    pub(crate) from_step_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) exit_name: Option<String>,
    /// Where it goes, for `go`. Absent for `done` and `halt`, which go nowhere.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub(crate) to_step_id: Option<i64>,
    #[ts(type = "\"go\" | \"done\" | \"halt\"")]
    pub(crate) ends: &'static str,
    /// How often this edge may be taken for one task. Absent is no limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub(crate) max_times: Option<i64>,
}

/// **What is handed from one step to the next.**
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AutomationWireDto {
    #[ts(type = "number")]
    pub(crate) id: i64,
    #[ts(type = "number")]
    pub(crate) from_step_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) from_exit_name: Option<String>,
    pub(crate) from_port_name: String,
    #[ts(type = "number")]
    pub(crate) to_step_id: i64,
    pub(crate) to_port_name: String,
}

/// **Whether this automation can be started, and what is in the way** — what the build screen's
/// launch place draws before anybody presses.
///
/// It is the picture and not the ruling: what refuses a launch is the launch itself, which writes a
/// run. A definition that passes here can still be refused there, by a machine that changed between
/// the reading and the press.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AutomationLaunchCheckDto {
    pub(crate) ready: bool,
    pub(crate) blocks: Vec<AutomationLaunchBlockDto>,
}

/// **One thing standing in the way of a launch.**
///
/// `reason` is what it is, and `stepName` / `at` say where — the way out with nothing after it, the
/// input nothing feeds, the agent this machine cannot start. A reason about the automation as a
/// whole carries neither.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/bindings.ts")]
#[serde(rename_all = "camelCase")]
pub struct AutomationLaunchBlockDto {
    #[ts(
        type = "\"no_steps\" | \"exit_without_next\" | \"input_unfed\" | \"agent_not_here\" | \"task_undecided\" | \"workspace_closed\""
    )]
    pub(crate) reason: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub(crate) step_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) step_name: Option<String>,
    /// What on that step — a way out's name, an input's name, an agent's id.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) at: Option<String>,
}
