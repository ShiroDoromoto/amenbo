//! The domain model — the logical schema.
//!
//! The structs in this module are, as written, **the format of the local store**. Every record carries
//! its audit metadata (id / created_at / updated_at). Deletion is physical.
//!

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::time::Timestamp;

/// A project's default view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum View {
    #[default]
    List,
    Board,
    Calendar,
    Timeline,
}

impl View {
    pub fn as_str(&self) -> &'static str {
        match self {
            View::List => "list",
            View::Board => "board",
            View::Calendar => "calendar",
            View::Timeline => "timeline",
        }
    }

    pub fn parse(s: &str) -> Option<View> {
        match s {
            "list" => Some(View::List),
            "board" => Some(View::Board),
            "calendar" => Some(View::Calendar),
            "timeline" => Some(View::Timeline),
            _ => None,
        }
    }
}

/// A task's priority — a fixed enum, not a user-defined scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    High,
    Medium,
    Low,
}

impl Priority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::High => "high",
            Priority::Medium => "medium",
            Priority::Low => "low",
        }
    }

    pub fn parse(s: &str) -> Option<Priority> {
        match s {
            "high" => Some(Priority::High),
            "medium" => Some(Priority::Medium),
            "low" => Some(Priority::Low),
            _ => None,
        }
    }

    /// Sort weight — `high` comes first.
    pub fn rank(&self) -> u8 {
        match self {
            Priority::High => 0,
            Priority::Medium => 1,
            Priority::Low => 2,
        }
    }
}

/// A task's subtype. Classification belongs to a separate entity (the dimension), so all that is left
/// here is `default` / `milestone`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Subtype {
    #[default]
    Default,
    Milestone,
}

impl Subtype {
    pub fn as_str(&self) -> &'static str {
        match self {
            Subtype::Default => "default",
            Subtype::Milestone => "milestone",
        }
    }

    pub fn parse(s: &str) -> Option<Subtype> {
        match s {
            "default" => Some(Subtype::Default),
            "milestone" => Some(Subtype::Milestone),
            _ => None,
        }
    }
}

/// A task's status. It is **the authority on completion**: being done is derived from `Done`
/// ([`Task::completed`]), never stored alongside it.
///
/// `Done` and `Rejected` are the two terminals, and the difference between them is whether the work was
/// *carried out* (`AMB-D-397`): a task nobody is going to do ends at `Rejected`, where recording it as
/// `Done` would make the history claim something that never happened and deleting it would take the
/// reasoning away with the row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    Todo,
    InProgress,
    Done,
    Blocked,
    /// Considered, and decided against. A terminal that is not an achievement — so `completed_at`, the
    /// day the work was *finished*, stays unset, and when the rejection happened is what
    /// [`Task::status_changed_at`] holds.
    Rejected,
}

impl TaskStatus {
    /// Every status there is, in the order a board reads them. The one place they are enumerated, so a
    /// surface that lists them (the filter grammar an agent reads before it queries) cannot fall behind
    /// the enum.
    pub const ALL: [TaskStatus; 5] = [
        TaskStatus::Todo,
        TaskStatus::InProgress,
        TaskStatus::Done,
        TaskStatus::Blocked,
        TaskStatus::Rejected,
    ];

    /// The **terminals**: the statuses that mean the work is not coming back. Written once here so the
    /// two readings of an ended task cannot drift apart — *closed* is this set, *carried out* is `Done`
    /// alone ([`Task::completed`]).
    pub const CLOSED: [TaskStatus; 2] = [TaskStatus::Done, TaskStatus::Rejected];

    /// Is this task over, whichever way it ended? What a dependency asks before it releases what it was
    /// holding back, and what an "outstanding work" count asks before it counts a task. Not the same
    /// question as [`Task::completed`], which asks whether the work was *carried out*.
    pub fn is_closed(&self) -> bool {
        Self::CLOSED.contains(self)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Todo => "todo",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Done => "done",
            TaskStatus::Blocked => "blocked",
            TaskStatus::Rejected => "rejected",
        }
    }

    pub fn parse(s: &str) -> Option<TaskStatus> {
        match s {
            "todo" => Some(TaskStatus::Todo),
            "in_progress" => Some(TaskStatus::InProgress),
            "done" => Some(TaskStatus::Done),
            "blocked" => Some(TaskStatus::Blocked),
            "rejected" => Some(TaskStatus::Rejected),
            _ => None,
        }
    }
}

/// The facet an action or an attribution belongs to: the human, or the human's AI.
/// It is **a label that assumes an honest actor** — a guardrail against accidents, not a security
/// boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    #[default]
    Human,
    Ai,
}

impl ActorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActorKind::Human => "human",
            ActorKind::Ai => "ai",
        }
    }

    pub fn parse(s: &str) -> Option<ActorKind> {
        match s {
            "human" => Some(ActorKind::Human),
            "ai" => Some(ActorKind::Ai),
            _ => None,
        }
    }
}

/// The lifecycle state of a decision record: `Decided`, or `Rejected` — the two ways a decision can
/// end, and there is no third (`AMB-D-918`). Saving one is what decides it (`AMB-D-917`), so there is
/// no stage before the verdict for a status to name: **"still being written" is `Decision::draft`**, a
/// flag beside this one, the shape the task side has carried since `AMB-D-553`.
/// Decisions have no todo/in_progress workflow the way tasks do, and they never show up in a mailbox.
/// **"Superseded" is not a state.** It is a *relationship between decisions* — the `supersedes` edge —
/// and currency is a projection derived from it (`current` = not pointed at by a live `supersedes` edge).
/// Stored as a flag, deleting the superseding decision would strand its target at `superseded` forever: a
/// decision nothing overturns, drifting on as history.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStatus {
    /// Decided. The word `decided_at` / `decided_by` already speak, so the three read as one
    /// (`AMB-D-918`). It is what a decision is from the moment it is recorded; the stamps are filled
    /// in one stage on, where the writing ends.
    #[default]
    Decided,
    /// Rejected — considered, and not adopted.
    Rejected,
}

impl DecisionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            DecisionStatus::Decided => "decided",
            DecisionStatus::Rejected => "rejected",
        }
    }

    pub fn parse(s: &str) -> Option<DecisionStatus> {
        match s {
            "decided" => Some(DecisionStatus::Decided),
            "rejected" => Some(DecisionStatus::Rejected),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Project {
    /// Primary key (INTEGER). It stays an integer across every boundary — Rust, TS, `--json`.
    pub id: i64,
    pub name: String,
    pub notes: String,
    pub color: Option<String>,
    /// The image the project shows for itself, as a small `data:image/…` URL — a 96px square PNG,
    /// capped at [`crate::config::AVATAR_MAX_BYTES`]. `None` means the project has none, and the
    /// surfaces fall back to the colour and the first letter of the name. Its original lives in the
    /// blob store, named by [`Project::icon_source`] (`AMB-D-839`).
    #[serde(default)]
    pub icon: Option<String>,
    /// The BLAKE3 hash of the icon's original image in the blob store (`<store>/blobs/<hash>`), kept so
    /// a different size can be baked later without asking the human to choose the file again
    /// (`AMB-D-839`). `None` where there is no icon, or where one was registered without an original.
    /// It moves with [`Project::icon`] and never apart from it — a hash left standing beside another
    /// image's display version would name the wrong original.
    #[serde(default)]
    pub icon_source: Option<String>,
    pub default_view: View,
    pub archived: bool,
    pub order_key: String,
    /// A short human-readable identifier, unique on this device (e.g. "amenbo"). Derived from the name at
    /// creation and immutable thereafter. The primary key (`id`) is the real identifier; the slug is
    /// **corroborating evidence** — an `.amenbo` pointer carries both, and if the slug disagrees with the
    /// one on the project `id` names, we warn that the pointer belongs to a different store.
    #[serde(default)]
    pub slug: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Task {
    /// The primary key, and also the conversational number (displayed as `AMB-T-<n>`). They are one and the
    /// same number — there is no separate `number` field.
    pub id: i64,
    pub title: String,
    pub notes: String,
    pub subtype: Subtype,
    /// When it was completed. Set only while `status == Done`; `None` otherwise. *Whether* a task is done
    /// is derived from `status` ([`Task::completed`]), so this field says only *when*.
    pub completed_at: Option<Timestamp>,
    /// The authority on status, and the single truth about completion. There is no independent `completed`
    /// boolean: being done is derived from `status == Done` ([`Task::completed`]).
    #[serde(default)]
    pub status: TaskStatus,
    /// When the current `status` began — updated **only** on a status change, never on an ordinary field
    /// write (`updated_at` moves on any write, so it cannot answer "when did this status begin"). Because
    /// `in_progress` is exclusive (reserving is a CAS from `todo`), while a task is reserved this holds the
    /// reservation instant, and a reopen re-stamps it to the latest reservation. It is the basis for
    /// judging whether a premise changed *after* a task was reserved (`AMB-D-366`). `None` for a task that
    /// predates the column (an older store never stamped it).
    #[serde(default)]
    pub status_changed_at: Option<Timestamp>,
    /// Is the task still being put together? Creation is two stages, and this is the one it is at
    /// (`AMB-D-552`): `true` while it is being assembled, `false` once the creation was finished. It is a
    /// premise of `ready` and not a sixth status (`AMB-D-553`) — a draft is on the board and in every
    /// listing, it just cannot be reserved (`AMB-D-555`). `false` is what a task from a store that
    /// predates the column means, and it is the truth about it: the build that wrote it had no second
    /// stage to leave it in.
    #[serde(default)]
    pub draft: bool,
    /// The creator's facet. `None` means unknown (older data), which reads as "not authored by the AI".
    #[serde(default)]
    pub created_by_kind: Option<ActorKind>,
    /// The assignee's facet. `ai` means "for this person's AI" (me-ai). In a single local store, an
    /// assignee is one of exactly two facets, human or ai; `None` means unassigned.
    #[serde(default)]
    pub assignee_kind: Option<ActorKind>,
    pub start_on: Option<NaiveDate>,
    pub due_on: Option<NaiveDate>,
    pub priority: Option<Priority>,
    /// The project it belongs to. A task lives in exactly one project — it never multi-homes — so
    /// placement is held on the task itself. `None` means unfiled (the inbox).
    #[serde(default)]
    pub project_id: Option<i64>,
    /// Where it sits within its project. `None` when unfiled — the inbox has no ordering.
    #[serde(default)]
    pub order_key: Option<String>,
    /// Which of the project's bound folders this task is worked in (`AMB-D-648`), as the id of the
    /// binding row — never a path, so the folder can be moved or renamed under it. `None` is a task that
    /// names no folder, which is every task unless someone said otherwise: nothing infers a place from
    /// where the task was filed. Having one refuses nothing on its own — no reservation and no worktree
    /// is stopped for it here; it is expressiveness the surfaces around Amenbo may act on.
    #[serde(default)]
    pub at_binding_id: Option<i64>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Task {
    /// Is this task done? `status` is the sole source of truth for completion — there is no independent
    /// `completed` boolean, only this derivation of `done ⟺ completed`.
    pub fn completed(&self) -> bool {
        self.status == TaskStatus::Done
    }
}

/// A dependency between two tasks, as an edge object — one edge, one record. `task_id` depends on
/// `blocked_by_id`, i.e. the latter should be done first. The reverse direction (`blocks`) is not stored;
/// it is derived at query time. Removing a dependency deletes the row.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TaskDependency {
    pub id: i64,
    /// The blocked side — this depends on …
    pub task_id: i64,
    /// The blocker — … must be done first.
    pub blocked_by_id: i64,
    /// The creator's facet.
    #[serde(default)]
    pub created_by_kind: Option<ActorKind>,
    /// When this edge was established — what the premise-change judgement dates it by (`AMB-D-372`).
    /// Stamped once when the edge is drawn and never rewritten; `created_at` records the same instant but
    /// is a record column, so it takes no part in the judgement. `None` only on a row that predates the
    /// column and was never backfilled — read as "not after the status clock", i.e. no premise change.
    #[serde(default)]
    pub established_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A git commit SHA recorded against a task — one row, one commit (a task carries many). Amenbo keeps the
/// SHA as an opaque string: it never reads git, verifies the commit exists, or knows which forge it lives
/// on. It is the anchor from history back to a task: a public commit carries no store-local reference, so
/// the chain can only be drawn on the task side. `sha` is the full-length lower-case hex the ops layer
/// admits at the door (40 hex = SHA-1, 64 = SHA-256); short forms and refs are refused before they land.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TaskCommit {
    pub id: i64,
    /// The task this commit belongs to.
    pub task_id: i64,
    /// The full commit SHA, lower-case hex.
    pub sha: String,
    /// The creator's facet. `None` reads as human (older data).
    #[serde(default)]
    pub created_by_kind: Option<ActorKind>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// The session a task was made in (`AMB-D-897`) — one row a task, and none for a task made outside the
/// talk window.
///
/// **It names the pane, not the terminal.** A terminal is a process and ends with it; the pane it was
/// drawn in outlives every terminal opened there, so the pane is the thing a reader can still be taken
/// back to ([`crate::session::PANE_VAR`]).
///
/// The three are read in turn, and each answers where the one before it cannot: the pane while it is
/// open, the handle where it has been closed but the conversation is still there, and the name where
/// neither can be reached — which is the question this was raised for ("which session made this?"),
/// answerable even when nothing can be opened.
///
/// **No road out carries it** ([`crate::export::WITHHELD_ON_THE_WAY_OUT`]): a pane is this machine's,
/// and on another device there is nothing here to open.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TaskMadeIn {
    pub id: i64,
    /// The task this was made in the course of.
    pub task_id: i64,
    /// The id of the pane it was made in ([`crate::frames::SavedPane::id`]).
    pub pane: String,
    /// What that pane was called at the time, or `None` for a pane nobody had named.
    #[serde(default)]
    pub pane_name: Option<String>,
    /// The handle that pane's provider is resumed from ([`crate::frames::SavedPane::resume`]), or
    /// `None` for a pane there was no way back into.
    #[serde(default)]
    pub pane_resume: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// The decision's side of [`TaskMadeIn`] — same three values, its own table, for the reason
/// [`DecisionComment`] is a table of its own beside [`TaskComment`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DecisionMadeIn {
    pub id: i64,
    /// The decision this was made in the course of.
    pub decision_id: i64,
    /// The id of the pane it was made in ([`crate::frames::SavedPane::id`]).
    pub pane: String,
    /// What that pane was called at the time, or `None` for a pane nobody had named.
    #[serde(default)]
    pub pane_name: Option<String>,
    /// The handle that pane's provider is resumed from, or `None` for a pane there was no way back
    /// into.
    #[serde(default)]
    pub pane_resume: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Which of Amenbo's own features a [`Secret`] belongs to (`AMB-D-884`). Closed, because the features
/// that hold a credential are the body's own and are added one deliberate step at a time — a new one
/// widens this and the column's `CHECK` together.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretArea {
    /// A notification target's connection — a Slack webhook, an SMTP password (`AMB-D-885`).
    #[default]
    Notify,
    /// The Viewer's server: the token it is reached with, the key its rows are sealed with
    /// (`AMB-D-886`).
    Viewer,
}

impl SecretArea {
    pub fn as_str(&self) -> &'static str {
        match self {
            SecretArea::Notify => "notify",
            SecretArea::Viewer => "viewer",
        }
    }

    pub fn parse(s: &str) -> Option<SecretArea> {
        match s {
            "notify" => Some(SecretArea::Notify),
            "viewer" => Some(SecretArea::Viewer),
            _ => None,
        }
    }
}

/// **A secret one of Amenbo's own features holds, at one layer** (`AMB-D-884`). The body had no place for
/// a credential until this: `config.json` holds none by its own account, and the features that needed one
/// were plugins then, keeping theirs in a table of the mechanism's, which went with it.
///
/// A table of its own rather than a flag on a settings row: an exclusion
/// stated once, about a whole table, holds for the path nobody remembered to teach. It is named in
/// [`crate::export::WITHHELD_ON_THE_WAY_OUT`], so no road out of the store carries it; a backup does,
/// copying the file whole, because that road leads back to the same person's own machine.
///
/// The value is never handed to a face. A screen asks whether a field is set and stops there; the
/// plaintext is read at the moment the feature connects.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Secret {
    pub id: i64,
    /// The project this secret belongs to, or `None` for the device row. Both layers are real here: a
    /// notification target and the Viewer are the device's, and a project may still hold one of its own.
    pub project_id: Option<i64>,
    /// Which feature holds it.
    pub area: SecretArea,
    /// The row inside that area the secret hangs off — a notification target's id — or `None` where the
    /// area itself holds it (the Viewer's keys hang off no row). Polymorphic: which table it names is
    /// `area`'s to say, so no constraint holds it and the delete op sweeps it.
    #[serde(default)]
    pub owner_id: Option<i64>,
    /// The field's key (spelled out because `key` is a SQLite keyword).
    pub field_key: String,
    /// The secret value, in plaintext — at-rest secrecy is the OS's full-disk encryption, the same
    /// delegation the truth source itself makes.
    pub value: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// **What a notification is carried by** (`AMB-D-885`) — the kind a [`NotifyTarget`] is. It decides which
/// of the target's connection columns mean anything, and which credential the target keeps in [`Secret`].
/// Closed, because a new kind is a deliberate step: it widens this, the column's `CHECK`, and the shelf's
/// colour and glyph together.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyKind {
    /// A Slack channel, reached by an incoming webhook. The URL *is* the channel, so it is the whole
    /// connection — and a credential, so it is held in `secret` under
    /// [`NotifyTarget::SLACK_WEBHOOK_URL`] rather than on the row.
    #[default]
    Slack,
    /// An SMTP relay. Server, port, account and sender sit on the row; only the password is held in
    /// `secret` ([`NotifyTarget::SMTP_PASSWORD`]).
    Mail,
}

impl NotifyKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NotifyKind::Slack => "slack",
            NotifyKind::Mail => "mail",
        }
    }

    pub fn parse(s: &str) -> Option<NotifyKind> {
        match s {
            "slack" => Some(NotifyKind::Slack),
            "mail" => Some(NotifyKind::Mail),
            _ => None,
        }
    }
}

/// **One connection this device can send a notification through, under a name** (`AMB-D-885`).
///
/// It belongs to the device and to no project. A webhook is written here once and every project that
/// wants it **selects** it ([`ProjectNotifyTarget`]), so a URL that changes is one edit rather than one
/// per project. The shape that was turned down is a device default each project overrides: that gives
/// every field an inherited/overridden state of its own, and reading where a project's notifications
/// actually go then takes two screens.
///
/// **The credential is not on this row.** A Slack target's webhook URL and a mail target's SMTP password
/// are [`Secret`] rows addressed `(None, SecretArea::Notify, Some(id), field_key)` — the table no road
/// out of the store walks. What stays here is what a screen may draw: the kind, the name, and the parts
/// of a mail connection that are not a password.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NotifyTarget {
    pub id: i64,
    /// What carries the message — and which of the columns below mean anything.
    pub kind: NotifyKind,
    /// The name the person gave it. This is how a project's screen offers it, so it is the target's
    /// whole identity there: two Slack webhooks are told apart by nothing else.
    pub name: String,
    /// Is this the target a newly created project starts out pointing at? At most one row holds it
    /// ([`crate::ops::notify`] moves the mark rather than raising a second). Not an inherited value: it
    /// decides where a project **starts**, and from then on the project's own selection is the whole
    /// answer — which is what keeps "default" from becoming a tier.
    pub is_default: bool,
    /// The relay a mail target hands the message to. `None` on a Slack target, which has no such field
    /// at all — the two kinds share this table the way `attachment`'s two modes share theirs.
    pub smtp_host: Option<String>,
    /// The port that relay listens on (587 on nearly every provider). Mail-mode.
    pub smtp_port: Option<i64>,
    /// The account to authenticate as, written out in full. Mail-mode; empty where the relay asks for
    /// neither account nor password.
    pub smtp_user: Option<String>,
    /// The address the message is sent from. Mail-mode; empty falls back to the account, which is what
    /// most providers will accept.
    pub mail_from: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl NotifyTarget {
    /// The `secret` field key a Slack target's incoming-webhook URL is held under.
    pub const SLACK_WEBHOOK_URL: &'static str = "webhook_url";
    /// The `secret` field key a mail target's SMTP password is held under.
    pub const SMTP_PASSWORD: &'static str = "smtp_password";
}

/// **One project's notification row** (`AMB-D-885`): whether it notifies at all, and where its mail is
/// addressed. Which targets carry it is [`ProjectNotifyTarget`] and what it reports is
/// [`ProjectNotifyEvent`] — both sets rather than columns, so one project reaches a Slack channel and an
/// inbox at once, which the per-carrier shape could not write.
///
/// **`enabled` is not "is a target selected".** They are deliberately apart: someone going away for a
/// fortnight stops the notifications with one switch and finds the settings still standing on the way
/// back. And it is one switch rather than two steps: a feature of the body runs nobody else's code, so
/// there is no consent for a second switch to be (`AMB-D-434`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProjectNotify {
    pub id: i64,
    pub project_id: i64,
    /// Does this project notify? Off keeps the targets and the events where they are.
    pub enabled: bool,
    /// Where a mail target's message is addressed — several addresses on one line, separated by commas,
    /// as the person typed them. Empty falls back to the target's own account.
    ///
    /// The project's and not the target's, because it answers **who is told**, which is this project's
    /// business, while the target answers what carries it. It is also the one field here a kind decides
    /// the meaning of: a project with no mail target among its selection has no use for it.
    pub mail_to: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// **A target this project's notifications are carried by** (`AMB-D-885`) — one row per
/// `(project, target)`. A set and not a column: `AMB-D-885` keeps Slack and mail in one feature, and the
/// thing that makes them one is that a project answers "where" with as many targets as it likes.
///
/// It names the project directly rather than the [`ProjectNotify`] row, as [`ProjectNotifyEvent`] does:
/// all three hang off the project, so deleting one takes all three the same way and no order between
/// them has to be remembered.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProjectNotifyTarget {
    pub id: i64,
    pub project_id: i64,
    /// The [`NotifyTarget`] that carries it.
    pub target_id: i64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// **An event this project reports** (`AMB-D-885`) — one row per `(project, event)`, naming one of the
/// thirteen in [`crate::lifecycle::V1_EVENTS`] that say what happened. Six of them are what a
/// project starts with (`AMB-D-714`).
///
/// `store.changed` is not among the thirteen and is refused here: it says only that *something* moved,
/// which is a signal for a mirror to re-read on and nothing a person can be told.
///
/// **No rows is an answer**, not an unset: the [`ProjectNotify`] row's existence is what says the
/// project has been set up, so a project that ticked every box off reports nothing and stays on.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProjectNotifyEvent {
    pub id: i64,
    pub project_id: i64,
    /// The event's name, as [`crate::lifecycle::name`] spells it (`task.done`). Text rather than a
    /// fourth copy of the catalog: the names are one versioned set that belongs to `plugin_payload`, the
    /// column's `CHECK` states which of them this column admits, and `ops::notify` holds the two to each
    /// other.
    pub event: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A decision — a premise that holds now, settled with the human — as a first-class entity that sits
/// beside Task, under Project. Its body is edited in place, while it is still being written and after it
/// is settled alike; what an edit cannot do is overturn, and a conclusion that changes is a new decision
/// that `supersedes` this one (`AMB-D-363`). It carries a status of its own ([`DecisionStatus`]) and, while
/// the writing is still open, a `draft` flag — but no mailbox workflow, so decisions never clutter a task
/// list. Their numbers are a global sequence in a space of their own, separate from tasks.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Decision {
    /// The primary key, and also the decision number (displayed as `AMB-D-<n>` — a number space separate from
    /// tasks').
    pub id: i64,
    /// The project it lives under. Decisions do not multi-home: one decision, one project.
    pub project_id: i64,
    pub title: String,
    /// The decision itself: the conclusion and the grounds for it. Distil rather than transcribe — the raw
    /// discussion does not belong here, and watch for PII on the way in.
    pub body: String,
    /// Lifecycle state.
    #[serde(default)]
    pub status: DecisionStatus,
    /// Is the decision still being written? A premise of `ready` on the tasks that rest on it, and a
    /// flag rather than a status for the reason [`Task::draft`] is one (`AMB-D-553`, `AMB-D-918`): a
    /// decision half-written is visible everywhere, and what it cannot do is hold a reserve open.
    /// `false` is "the writing is finished", which is what every decision in a store older than the
    /// column means.
    #[serde(default)]
    pub draft: bool,
    /// When the decision last moved between settled and unsettled — stamped by the four routes that move
    /// it (recording it, ending the writing, rejecting, reopening) and never by an ordinary edit: a body
    /// rewritten in place moves `updated_at` and leaves this still answering "when did this decision last
    /// change what it is". The symmetric twin of [`Task::status_changed_at`], and the far side of the
    /// comparison that says a premise came back open *after* a task was reserved (`AMB-D-373`).
    ///
    /// **It outlived the column's name.** Ending the writing and reopening move `draft` rather than
    /// `status` (`AMB-D-918`), and they are exactly the two the reopen axis has to date, so they keep
    /// stamping it. Reading it as "when `status` last changed" would leave that axis deaf.
    ///
    /// Not `decided_at`: that one is the moment of *settling* and is cleared by a reopen, so it cannot
    /// answer for a decision that is being written again — which is the one state the reopen axis judges.
    /// `None` only for a decision no migration reached (every existing row was backfilled).
    #[serde(default)]
    pub status_changed_at: Option<Timestamp>,
    /// When the writing ended and the decision was settled — unset while it is still being written, and
    /// cleared again by a reopen.
    #[serde(default)]
    pub decided_at: Option<Timestamp>,
    /// The decider token, for display.
    #[serde(default)]
    pub decided_by: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// The kind of an edge between decisions. The variants are cut by **behaviour** — what this edge tells
/// you to do with the older decision from now on — not by taxonomy. All three sit on one axis, "how to
/// read it": don't read it any more (`Supersedes`), read it together with this one (`Amends`), read it
/// first (`BuildsOn`). A generic `related` edge would change nothing about how the target is read, so it
/// has no place on that axis — which is why there isn't one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionEdgeKind {
    /// Consigns the target to history: it stops being current, and is greyed out.
    #[default]
    Supersedes,
    /// Partially revises the target. The target **stays current**, and the two are read together.
    Amends,
    /// **Builds on** the target. It changes neither the target's currency nor how it is read; it supplies
    /// only the **reading order** (read that one first) and the **blast radius of overturning it** (the
    /// reverse lookup: the decisions that need revisiting if this one falls). `Supersedes` and `Amends`
    /// both imply it, so it is never laid on top of either for the same pair.
    BuildsOn,
}

impl DecisionEdgeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            DecisionEdgeKind::Supersedes => "supersedes",
            DecisionEdgeKind::Amends => "amends",
            DecisionEdgeKind::BuildsOn => "builds_on",
        }
    }

    pub fn parse(s: &str) -> Option<DecisionEdgeKind> {
        match s {
            "supersedes" => Some(DecisionEdgeKind::Supersedes),
            "amends" => Some(DecisionEdgeKind::Amends),
            "builds_on" => Some(DecisionEdgeKind::BuildsOn),
            _ => None,
        }
    }
}

/// A decision → decision edge. One edge, one record (the same shape as `TaskDependency`), so a single
/// decision can point at **several** older ones, per kind — the edges form a DAG. The direction is always
/// new → old: `decision_id` is the side that drew the edge, `target_decision_id` the older side it points
/// at, and the older row is never rewritten (the reverse lookup, "who overturned me?", is derived from the
/// index on `target_decision_id`). A pair can carry only one kind (`decision_edge_pair` UNIQUE):
/// supersedes and amends contradict each
/// other, and builds_on is implied by both, so stacking it adds nothing. Removing an edge deletes the row.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DecisionEdge {
    pub id: i64,
    /// The side that drew the edge — the newer decision.
    pub decision_id: i64,
    /// The side pointed at — the older decision.
    pub target_decision_id: i64,
    pub kind: DecisionEdgeKind,
    /// When the edge came to carry its current `kind` — the third premise-change intent column, beside
    /// [`TaskDependency::established_at`] and [`DecisionTaskLink::linked_at`] (`AMB-D-372`). What dates a
    /// supersession, the target's row being left alone by it (`AMB-D-373`).
    #[serde(default)]
    pub drawn_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A bidirectional link between a decision and a task. One link, one record (the same shape as
/// `TaskDependency`). It is many-to-many and cheap to traverse either way — the implementation tasks a
/// decision spawned, and the decision that motivated a task — because the SQLite truth source can just
/// join. Removing a link deletes the row.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DecisionTaskLink {
    pub id: i64,
    pub decision_id: i64,
    pub task_id: i64,
    /// When the link was drawn — the twin of [`TaskDependency::established_at`], and what dates a linked
    /// decision for the premise-change judgement (`AMB-D-372`).
    #[serde(default)]
    pub linked_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

// ───────────────────────── the unified dimension model ─────────────────────────
// Every axis a task can be classified along — phase, category, whatever the user invents — goes through
// one mechanism: the Dimension.

/// How many values of an axis one record may hold — the axis's own answer, like `show_on_card` and
/// `required` (`AMB-D-826`). `Single` constrains `(task, dimension)` to one row and is where every axis
/// starts and where every axis an upgrade brings in stays; `Multi` lets one record answer the axis with
/// several of its values, so a record that genuinely spans a set of them is filed under all of them
/// rather than under a representative one.
///
/// **`Multi` and `role: TimeAxis` do not go together.** A time axis resolves the "current era" to a
/// single value ([`crate::store_engine::read::current_time_axis_value`]) and writes that one onto a new
/// record; belonging to several eras leaves both halves of that undefined. `ops::dimension` refuses the
/// pair at both doors it can be built through.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionCardinality {
    #[default]
    Single,
    Multi,
}

impl DimensionCardinality {
    pub fn as_str(&self) -> &'static str {
        match self {
            DimensionCardinality::Single => "single",
            DimensionCardinality::Multi => "multi",
        }
    }

    pub fn parse(s: &str) -> Option<DimensionCardinality> {
        match s {
            "single" => Some(DimensionCardinality::Single),
            "multi" => Some(DimensionCardinality::Multi),
            _ => None,
        }
    }
}

/// A dimension's role. `TimeAxis` nominates this dimension as the project's axis of time, which is what
/// earns it special treatment in the views' filter affordances. The user is free to call that axis
/// "phase", or "sprint", or anything else — the role is the engine's vocabulary, the name is data. The
/// mechanism is identical to every other axis; only this flag is added.
///
/// `Closable` nominates an axis whose values can be **closed**: a value nobody is to be filed under any
/// more, which stays where it is and keeps every record already filed under it (`AMB-D-829`). Without
/// it the only way to retire a value is to delete it, and that takes the classification with it — what
/// was filed under a finished release stops being findable at all. The flag is on the axis rather than
/// on the value because closing means something on the axis that keeps raising values and retiring them
/// (a release, a theme) and nothing on the one whose values are the vocabulary itself.
///
/// **One axis holds one role.** An axis is either the time axis or the closable one, and the two say
/// different things about what its values are: a time axis retires a value by its period running out,
/// which is the very thing `DimensionValue::covers` reads, so it needs no second way of saying the same.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionRole {
    #[default]
    None,
    TimeAxis,
    Closable,
}

impl DimensionRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            DimensionRole::None => "none",
            DimensionRole::TimeAxis => "time_axis",
            DimensionRole::Closable => "closable",
        }
    }

    pub fn parse(s: &str) -> Option<DimensionRole> {
        match s {
            "none" => Some(DimensionRole::None),
            "time_axis" => Some(DimensionRole::TimeAxis),
            "closable" => Some(DimensionRole::Closable),
            _ => None,
        }
    }
}

/// What a classification axis is allowed to classify: tasks, decisions, or both (`AMB-D-789`). An axis
/// is one mechanism serving two entities, and until this column existed it served them both whether or
/// not that made sense — a project's "occupancy" axis is an exclusive lane on a real device, and a
/// decision record occupies no device, yet opening one offered the choice. The values are shared either
/// way: narrowing an axis says where it *means* something, never which values it holds.
///
/// `Both` is the default, and deliberately the wide side — the opposite of `show_on_card` and
/// `required`, which start `false`. Those widen into crowding; this one, started narrow, would betray
/// the plain expectation that an axis somebody raised applies where they raise it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DimensionAppliesTo {
    Task,
    Decision,
    #[default]
    Both,
}

impl DimensionAppliesTo {
    pub fn as_str(&self) -> &'static str {
        match self {
            DimensionAppliesTo::Task => "task",
            DimensionAppliesTo::Decision => "decision",
            DimensionAppliesTo::Both => "both",
        }
    }

    pub fn parse(s: &str) -> Option<DimensionAppliesTo> {
        match s {
            "task" => Some(DimensionAppliesTo::Task),
            "decision" => Some(DimensionAppliesTo::Decision),
            "both" => Some(DimensionAppliesTo::Both),
            _ => None,
        }
    }

    /// Does this axis mean anything on a task?
    pub fn on_task(&self) -> bool {
        matches!(self, DimensionAppliesTo::Task | DimensionAppliesTo::Both)
    }

    /// Does this axis mean anything on a decision?
    pub fn on_decision(&self) -> bool {
        matches!(self, DimensionAppliesTo::Decision | DimensionAppliesTo::Both)
    }
}

/// Which of the two classified entities a question is being asked about — the side an axis has to
/// classify to mean anything there (`AMB-D-789`). The same two words as [`DimensionAppliesTo`]'s narrow
/// arms, kept a type apart because this one has no `Both`: a listing, a filter or a page is always read
/// on exactly one side, and it is the axis, never the reader, that gets to say "both".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassifiedSide {
    Task,
    Decision,
}

impl ClassifiedSide {
    /// The `applies_to` values an axis may carry and still classify this side.
    pub fn accepted(self) -> [DimensionAppliesTo; 2] {
        match self {
            ClassifiedSide::Task => [DimensionAppliesTo::Task, DimensionAppliesTo::Both],
            ClassifiedSide::Decision => [DimensionAppliesTo::Decision, DimensionAppliesTo::Both],
        }
    }

    /// What a refusal calls the records on this side ("does not classify decisions").
    pub fn plural(self) -> &'static str {
        match self {
            ClassifiedSide::Task => "tasks",
            ClassifiedSide::Decision => "decisions",
        }
    }
}

/// The classification axis itself — one "column". Scoped to a project; its set of values lives in
/// [`DimensionValue`], its assignments to tasks in [`TaskDimensionValue`] and its assignments to
/// decisions in [`DecisionDimensionValue`]. Categories, phases and any
/// axis a user invents all fold into this one mechanism. Every dimension is a plain, user-editable
/// classification axis: there are no built-in fixed axes and no locked values (status and priority are
/// first-class task attributes instead, not dimensions). `order_key` is where the dimension itself sits in
/// the display order; `ordered` says whether its **values** have an order (if they do, their `order_key`
/// is what sorts them).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Dimension {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    /// Free-form description (Markdown).
    #[serde(default)]
    pub notes: String,
    pub cardinality: DimensionCardinality,
    /// Do the values have an order? If so they sort by their `order_key`; if not, they are an unordered
    /// set.
    pub ordered: bool,
    pub role: DimensionRole,
    /// Does a task's value on this axis belong on its card? The axis holds the answer, not the device
    /// looking at it (`AMB-D-651`): which axes a board shows is a reading of the project that whoever
    /// raised the axis settles, so every face and every machine gets the same one. `false` is where an
    /// axis starts and where every axis an upgrade brings in stays, so the cards keep the surface
    /// `AMB-D-40` drew until somebody names an axis to widen it (`AMB-D-650`).
    #[serde(default)]
    pub show_on_card: bool,
    /// Does this axis refuse to be left empty? A task carrying no value here cannot have its creation
    /// finished (`AMB-D-734`): a task that was never classified is one nobody can hand to a release
    /// later, so the axis's raiser gets to say the axis has to be answered. It is the axis's own answer,
    /// like `show_on_card`, and it bites at one point only — `ops::task::finish_creating`. It is
    /// deliberately **not** a `ready` premise: raising the flag would otherwise drop every task already
    /// filed to `ready:no` in one stroke, and a flag that can be raised at any time must not be able to
    /// stop the whole backlog. `false` is where an axis starts and where every axis an upgrade brings in
    /// stays.
    #[serde(default)]
    pub required: bool,
    /// Which side of the store this axis classifies (`AMB-D-789`). `Both` is where every axis starts and
    /// where every axis an upgrade brings in stays, so nothing an existing store classified changes
    /// meaning. Narrowing "both → one side" leaves the assignments already made on the other side in
    /// place; they simply stop meaning anything, the way a time axis's dates do once `role` goes back to
    /// `None`.
    #[serde(default)]
    pub applies_to: DimensionAppliesTo,
    /// The axis's readable, stable key — what names it **outside** Amenbo (`AMB-D-735`). The id is the
    /// real identifier; the slug is the one that can be read and typed where the id cannot be and the
    /// display name (Japanese, spaces and all) may not go. Unique within the project. `None` is only
    /// what a store written before the column says, and what a row holds for the instant between being
    /// created and being filled: every saved row carries one, and it is `ops::dimension` that holds
    /// that true, not the table.
    #[serde(default)]
    pub slug: Option<String>,
    /// Where the dimension itself sits in the display order.
    pub order_key: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// One value a dimension can take (say, "bug" on a category axis). On an ordered dimension `order_key` is
/// what sorts them; on an unordered one it is carried only as a stable key. `start_on` / `end_on` are the
/// **payload of the `DimensionRole::TimeAxis` role** — when the period this
/// value names begins and ends. The physical columns exist on every value, but their meaning, their
/// editability, and the resolution of "the current period" apply only to values on a time_axis dimension;
/// the layers above (ops / CLI / GUI) are the gatekeepers. They are held independently of `cardinality`
/// and `ordered`, so overlapping time axes (campaign-style) can reuse the same columns. This is a
/// different layer from [`Dimension::role`], which is the nomination flag — do not conflate the two.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DimensionValue {
    pub id: i64,
    pub dimension_id: i64,
    pub name: String,
    /// The value's readable, stable key — [`Dimension::slug`]'s counterpart, unique within the axis
    /// (`AMB-D-735`). It is what a caller outside Amenbo names this value by: a theme's branch, a
    /// release's folder, anything that has to spell a classification and cannot spell `AMB-DIMV-46`.
    #[serde(default)]
    pub slug: Option<String>,
    pub order_key: String,
    /// First day of the period, inclusive. `None` means the start is open.
    #[serde(default)]
    pub start_on: Option<NaiveDate>,
    /// Last day of the period, inclusive. `None` means "ongoing" — an open end.
    #[serde(default)]
    pub end_on: Option<NaiveDate>,
    /// Is this value closed — retired from the choices a record is newly filed under, while everything
    /// already filed under it stays (`AMB-D-829`)? It is the **payload of the
    /// [`DimensionRole::Closable`] role**, the way `start_on` / `end_on` are the time axis's: the column
    /// is on every value, and closing one is refused unless its axis carries the role
    /// (`ops::dimension::value_set_closed`). Reopening is free on any axis, so an axis that loses the
    /// role never strands a value nobody can bring back.
    ///
    /// A closed value keeps its id, its name, its key and its place in the order — reopening it has to
    /// find all four where they were, and a filter naming it goes on resolving, which is the whole point
    /// of closing rather than deleting. What it stops doing is taking new records:
    /// `ops::dimension::set` and its decision twin refuse it, and the guards that ask whether an axis
    /// still offers anything count the open values alone.
    ///
    /// `false` is where every value starts and where every value an upgrade brings in stays.
    #[serde(default)]
    pub closed: bool,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl DimensionValue {
    /// Does `date` fall inside this value's period `[start_on, end_on]`, both ends inclusive? An open end
    /// is unbounded — a `None` side is always satisfied. Both ends `None` means the period was never set,
    /// and that covers **no** date at all: "the current period" has to resolve to exactly one of the values
    /// that actually drew a window, and letting the window-less ones match would empty the window of
    /// meaning.
    pub fn covers(&self, date: NaiveDate) -> bool {
        if self.start_on.is_none() && self.end_on.is_none() {
            return false;
        }
        self.start_on.is_none_or(|s| s <= date) && self.end_on.is_none_or(|e| date <= e)
    }
}

/// The assignment of a dimension value to a task — the join record. `dimension_id` is denormalised onto
/// it so that ops and reads can hold a single-select axis to its `(task, dimension)` one row, sweep a
/// multi-select one's rows together (`AMB-D-826`), and filter on an axis directly, without joining
/// through to the value. Removing an assignment deletes the row.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TaskDimensionValue {
    pub id: i64,
    pub task_id: i64,
    pub dimension_id: i64,
    pub value_id: i64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// The assignment of a dimension value to a **decision** — the task join record's twin (`AMB-D-781`).
/// Same shape and same denormalised `dimension_id`, and the values it names are the axis's own: a
/// decision is classified along the axes the project already has, never along a set raised for
/// decisions. A separate table rather than a `target_type` column on [`TaskDimensionValue`], for the
/// reason `decision_comment` is separate from `task_comment` — each end keeps a real foreign key.
/// Removing an assignment deletes the row.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DecisionDimensionValue {
    pub id: i64,
    pub decision_id: i64,
    pub dimension_id: i64,
    pub value_id: i64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A durable comment. It is durable, human-authored data, so it gets a table of its own, `task_comment`
/// (the `task_` prefix is what distinguishes it from `decision_comment` and the like). A comment is always
/// addressed to a task and always has a body. The other half of the timeline — the system events — exists
/// as no row at all: those live only in the ledger file ([`crate::activity_log`]).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TaskComment {
    pub id: i64,
    pub task_id: i64,
    /// The author's facet. `None` reads as human (older data).
    #[serde(default)]
    pub author_kind: Option<ActorKind>,
    pub text: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// When the body was edited in place afterwards; `None` if it never was. `updated_at` cannot stand in
    /// for it: instants are second-precision, so an edit made within the same second leaves `updated_at`
    /// equal to `created_at`. We keep no revision history by design, which makes this the only clue a
    /// reader has that the text is not the text they read a moment ago.
    #[serde(default)]
    pub edited_at: Option<Timestamp>,
    /// Which step of which automation run carried this comment onto the task, where one did. `None` is
    /// every comment a person wrote, and also the ones an AI typed itself while a step of a run had the
    /// terminal — what this records is a step's own report being carried onto the task, not who was at
    /// the keyboard. The run's id alone would not answer it: a run walks several tasks in turn, so the
    /// step execution is the smallest thing that says which of them a report was about.
    #[serde(default)]
    pub automation_run_step_id: Option<i64>,
}

/// A durable comment on a decision record. Its own table, `decision_comment`, mirroring `TaskComment`
/// rather than sharing one polymorphic table: every comment holds a real FK to its parent
/// (`decision_id → decision.id`), and anything decision-specific that shows up later can grow here
/// without littering `task_comment` with nullable columns.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DecisionComment {
    pub id: i64,
    pub decision_id: i64,
    /// The author's facet. `None` reads as human (older data).
    #[serde(default)]
    pub author_kind: Option<ActorKind>,
    pub text: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// When the body was edited in place afterwards; `None` if it never was. `updated_at` cannot stand in
    /// for it: instants are second-precision, so an edit made within the same second leaves `updated_at`
    /// equal to `created_at`. We keep no revision history by design, which makes this the only clue a
    /// reader has that the text is not the text they read a moment ago.
    #[serde(default)]
    pub edited_at: Option<Timestamp>,
}

/// How an attachment was taken in. `Blob` is the default — the file is ingested into the store,
/// content-addressed, and the bytes `blob_hash` points at live out-of-band rather than in the engine.
/// `Url` is an external link, which we do not manage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    #[default]
    Blob,
    Url,
}

impl AttachmentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttachmentKind::Blob => "blob",
            AttachmentKind::Url => "url",
        }
    }

    pub fn parse(s: &str) -> Option<AttachmentKind> {
        match s {
            "blob" => Some(AttachmentKind::Blob),
            "url" => Some(AttachmentKind::Url),
            _ => None,
        }
    }
}

/// What an attachment hangs off. Tasks and decision records themselves, the comments on either
/// (`task_comment` / `decision_comment`), and one execution of one step of an automation run
/// (`automation_run_step`). A comment's attachments are kept **separately** from the parent record's,
/// which is what preserves the chronology of which comment a file arrived with. The target is
/// polymorphic — `target_type` names the table, and SQL cannot enforce a reference across it. The
/// `target_type` column is a string, so adding a variant is purely additive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentTarget {
    #[default]
    Task,
    Decision,
    /// Attached to a durable comment on a task ([`TaskComment`]).
    TaskComment,
    /// Attached to a durable comment on a decision record ([`DecisionComment`]).
    DecisionComment,
    /// Attached to one execution of one step of an automation run (`automation_run_step`) — the file a
    /// step produced, filed where it was produced. The run side has no model shape yet; this variant is
    /// what lets the column's fifth value be read back.
    AutomationRunStep,
}

impl AttachmentTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttachmentTarget::Task => "task",
            AttachmentTarget::Decision => "decision",
            AttachmentTarget::TaskComment => "task_comment",
            AttachmentTarget::DecisionComment => "decision_comment",
            AttachmentTarget::AutomationRunStep => "automation_run_step",
        }
    }

    pub fn parse(s: &str) -> Option<AttachmentTarget> {
        match s {
            "task" => Some(AttachmentTarget::Task),
            "decision" => Some(AttachmentTarget::Decision),
            "task_comment" => Some(AttachmentTarget::TaskComment),
            "decision_comment" => Some(AttachmentTarget::DecisionComment),
            "automation_run_step" => Some(AttachmentTarget::AutomationRunStep),
            _ => None,
        }
    }

    /// The ref space the target is numbered in — what turns `(target_type, target_id)` back into the
    /// ref a reader knows it by (`AMB-T-12`, `AMB-TC-12`). The pair is polymorphic, so this mapping is
    /// the one place the column's values line up with [`crate::idref::RefKind`]; everything that has to
    /// name a target quotes it through here rather than spelling the cases again.
    ///
    /// `None` for a step execution: it is named by the run it sits in, not by a number a person types
    /// back, so there is no ref space to render it in ([`Self::target_ref`] says what is quoted instead).
    pub const fn ref_kind(self) -> Option<crate::idref::RefKind> {
        match self {
            AttachmentTarget::Task => Some(crate::idref::RefKind::Task),
            AttachmentTarget::Decision => Some(crate::idref::RefKind::Decision),
            AttachmentTarget::TaskComment => Some(crate::idref::RefKind::TaskComment),
            AttachmentTarget::DecisionComment => Some(crate::idref::RefKind::DecisionComment),
            AttachmentTarget::AutomationRunStep => None,
        }
    }

    /// The target rendered as the ref a reader quotes it by. A target with no ref space of its own falls
    /// to the raw `(type, id)` pair — the same thing [`crate::validate`] prints for a `target_type` the
    /// model does not know, and the whole of what can be quoted either way.
    pub fn target_ref(self, id: i64) -> String {
        match self.ref_kind() {
            Some(kind) => crate::idref::render(kind, id),
            None => format!("{}:{}", self.as_str(), id),
        }
    }
}

/// An attachment on a task or a decision record. Two modes: `blob` (the default — ingested into the store,
/// content-addressed) and `url` (an external link we do not manage). A blob's bytes are not held in the
/// engine but out-of-band in the content-addressed blob store, and all the truth source keeps here is the
/// metadata: `blob_hash` / `filename` / `mime` / `size_bytes`. In url mode the external link sits in `url`
/// and the blob metadata columns are empty.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Attachment {
    pub id: i64,
    /// What it hangs off (task / decision / task_comment / decision_comment).
    pub target_type: AttachmentTarget,
    /// The id (INTEGER primary key) of the record it hangs off. Which table that is, `target_type` says.
    pub target_id: i64,
    pub kind: AttachmentKind,
    /// The content-address in blob mode — a BLAKE3 fingerprint of the bytes. It is what gives us dedup,
    /// tamper detection, and identity across devices. `None` in url mode.
    #[serde(default)]
    pub blob_hash: Option<String>,
    /// What the attachment is called: the file's own name (blob) or the display label (url). A blob
    /// renamed at attach time carries the label with the file's suffix kept on it, because this one
    /// field is read as a name again past the ingest — `attach save` writes under it, and `attach open`
    /// gives the OS a copy carrying its extension. What the bytes *are* is [`Attachment::mime`]'s, read
    /// off the file itself and never off this.
    #[serde(default)]
    pub filename: Option<String>,
    /// MIME type — the GUI picks its viewer by it.
    #[serde(default)]
    pub mime: Option<String>,
    /// The blob's byte length, for keeping an eye on how much space attachments take. `None` in url mode.
    #[serde(default)]
    pub size_bytes: Option<i64>,
    /// The external link in url mode. `None` in blob mode.
    #[serde(default)]
    pub url: Option<String>,
    /// The provenance facet (human / ai). `None` for placeholders and older data.
    #[serde(default)]
    pub created_by_kind: Option<ActorKind>,
    /// Where it sits among the attachments on the same target.
    pub order_key: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

// ───────────────────────── automation: what is built ─────────────────────────
//
// Eleven records for the definition, mirroring the eleven definition tables the store_engine schema
// declares. What ran is five more tables that no model shape covers yet — they are the launch side's,
// and nothing here reads them.
//
// Three layers, one word each (`AMB-D-949`): an Automation places AutomationActions, an action holds
// AutomationSteps, and one step is one terminal. An AutomationPlacement is one action standing at one
// spot of one automation.
//
// Four of the records hang off more than one kind of owner, because what they say is the same
// whichever of them says it: a way out (AutomationExit) and a port (AutomationPort) are a step's or
// an action's, a setting (AutomationCfg) is declared by an action and answered by a placement, and
// the picture (AutomationEdge, AutomationWire) is drawn either on an automation or inside an action.
// Each carries an owner enum instead of one nullable key per kind.

/// Which of the two an [`AutomationExit`] or an [`AutomationPort`] is declared by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationOwner {
    Step,
    Action,
}

impl AutomationOwner {
    pub fn as_str(&self) -> &'static str {
        match self {
            AutomationOwner::Step => "step",
            AutomationOwner::Action => "action",
        }
    }

    pub fn parse(s: &str) -> Option<AutomationOwner> {
        match s {
            "step" => Some(AutomationOwner::Step),
            "action" => Some(AutomationOwner::Action),
            _ => None,
        }
    }
}

/// Which of the two halves of a setting an [`AutomationCfg`] row is: the action's declaration, or one
/// placement's answer to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationCfgOwner {
    Action,
    Placement,
}

impl AutomationCfgOwner {
    pub fn as_str(&self) -> &'static str {
        match self {
            AutomationCfgOwner::Action => "action",
            AutomationCfgOwner::Placement => "placement",
        }
    }

    pub fn parse(s: &str) -> Option<AutomationCfgOwner> {
        match s {
            "action" => Some(AutomationCfgOwner::Action),
            "placement" => Some(AutomationCfgOwner::Placement),
            _ => None,
        }
    }
}

/// Which picture an [`AutomationEdge`] or an [`AutomationWire`] is drawn on — and with it, what the
/// boxes at either end are: an automation's are [`AutomationPlacement`]s, an action's are
/// [`AutomationStep`]s.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationPictureOwner {
    #[default]
    Automation,
    Action,
}

impl AutomationPictureOwner {
    pub fn as_str(&self) -> &'static str {
        match self {
            AutomationPictureOwner::Automation => "automation",
            AutomationPictureOwner::Action => "action",
        }
    }

    pub fn parse(s: &str) -> Option<AutomationPictureOwner> {
        match s {
            "automation" => Some(AutomationPictureOwner::Automation),
            "action" => Some(AutomationPictureOwner::Action),
            _ => None,
        }
    }

    /// The table the boxes of this picture are rows of — what `from_id` and `to_id` name.
    pub fn box_table(&self) -> &'static str {
        match self {
            AutomationPictureOwner::Automation => "automation_placement",
            AutomationPictureOwner::Action => "automation_step",
        }
    }
}

/// Which of the three an [`AutomationPort`] hangs off. What a step or an action takes in is declared by
/// that step or action; what it hands on is declared by the way out it left through, so the exit is a
/// third owner here and nowhere else.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationPortOwner {
    Step,
    Action,
    Exit,
}

impl AutomationPortOwner {
    pub fn as_str(&self) -> &'static str {
        match self {
            AutomationPortOwner::Step => "step",
            AutomationPortOwner::Action => "action",
            AutomationPortOwner::Exit => "exit",
        }
    }

    pub fn parse(s: &str) -> Option<AutomationPortOwner> {
        match s {
            "step" => Some(AutomationPortOwner::Step),
            "action" => Some(AutomationPortOwner::Action),
            "exit" => Some(AutomationPortOwner::Exit),
            _ => None,
        }
    }

    /// The same owner read as an [`AutomationOwner`] — `None` for an exit, which that pair does not
    /// admit. It is what lets one declaration carry both tables' owners without a second spelling of
    /// step-or-action.
    pub fn declarer(&self) -> Option<AutomationOwner> {
        match self {
            AutomationPortOwner::Step => Some(AutomationOwner::Step),
            AutomationPortOwner::Action => Some(AutomationOwner::Action),
            AutomationPortOwner::Exit => None,
        }
    }
}

/// What a setting is, which is what the build screen draws for it: a task filter, a folder, a choice
/// out of a list, a number, or free text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationCfgKind {
    TaskFilter,
    Folder,
    Choice,
    Number,
    Text,
}

impl AutomationCfgKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AutomationCfgKind::TaskFilter => "taskfilter",
            AutomationCfgKind::Folder => "folder",
            AutomationCfgKind::Choice => "choice",
            AutomationCfgKind::Number => "number",
            AutomationCfgKind::Text => "text",
        }
    }

    pub fn parse(s: &str) -> Option<AutomationCfgKind> {
        match s {
            "taskfilter" => Some(AutomationCfgKind::TaskFilter),
            "folder" => Some(AutomationCfgKind::Folder),
            "choice" => Some(AutomationCfgKind::Choice),
            "number" => Some(AutomationCfgKind::Number),
            "text" => Some(AutomationCfgKind::Text),
            _ => None,
        }
    }
}

/// Which way a port faces: `In` is what a step takes, `Out` is what a way out of it hands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationPortDirection {
    In,
    Out,
}

impl AutomationPortDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            AutomationPortDirection::In => "in",
            AutomationPortDirection::Out => "out",
        }
    }

    pub fn parse(s: &str) -> Option<AutomationPortDirection> {
        match s {
            "in" => Some(AutomationPortDirection::In),
            "out" => Some(AutomationPortDirection::Out),
            _ => None,
        }
    }
}

/// What a port carries. `TaskTake` is the one that decides what the run is about — the task it comes
/// out holding is the task every step after it works on — and `TaskMake` is a task a step raised along
/// the way, which the run does not work and nothing reserves.
///
/// Ordered so a set of them lists in the order they are declared in here, which is the order the way
/// out's own line reads in ([`crate::ops::automation_step`]'s "how to hand your work back").
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationPortKind {
    Value,
    File,
    TaskTake,
    TaskMake,
}

impl AutomationPortKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AutomationPortKind::Value => "value",
            AutomationPortKind::File => "file",
            AutomationPortKind::TaskTake => "task_take",
            AutomationPortKind::TaskMake => "task_make",
        }
    }

    pub fn parse(s: &str) -> Option<AutomationPortKind> {
        match s {
            "value" => Some(AutomationPortKind::Value),
            "file" => Some(AutomationPortKind::File),
            "task_take" => Some(AutomationPortKind::TaskTake),
            "task_make" => Some(AutomationPortKind::TaskMake),
            _ => None,
        }
    }
}

/// What happens once a way out is taken: go on to another step, close the run, or stop it and call a
/// person.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationEnds {
    #[default]
    Go,
    Done,
    Halt,
}

impl AutomationEnds {
    pub fn as_str(&self) -> &'static str {
        match self {
            AutomationEnds::Go => "go",
            AutomationEnds::Done => "done",
            AutomationEnds::Halt => "halt",
        }
    }

    pub fn parse(s: &str) -> Option<AutomationEnds> {
        match s {
            "go" => Some(AutomationEnds::Go),
            "done" => Some(AutomationEnds::Done),
            "halt" => Some(AutomationEnds::Halt),
            _ => None,
        }
    }
}

/// The name of the way out every step and every action carries and nobody writes: the one taken when
/// the step fell over. It is spelled apart from every name a person can give
/// ([`crate::ops::automation::exit_add`] refuses it as input), so an exit list can hold it without a
/// flag column saying which row it is.
///
/// **With no edge on it, it halts** — stops the run and calls a person. That is what lets the launch
/// check pass an automation nobody wrote an edge for on it
/// ([`crate::ops::automation_run::check`]): every step is born carrying this way out, so asking for
/// one more edge per step would be asking for the case that is already handled. An
/// [`AutomationEdge`] on it is how somebody says otherwise.
pub const ERROR_EXIT: &str = "*";

/// The number of times a way back may be taken for one task before the run is stopped
/// ([`AutomationEdge::max_times`]). Ten, because the thing it guards against is a loop that never
/// converges, not a review that goes round three times.
pub const DEFAULT_MAX_TIMES: i64 = 10;

/// **A unit worth using twice** — one entry of the library. `project_id` `None` is one held by the
/// device rather than by a project, and it is reachable from every project on it.
///
/// It is a picture of its own: the [`AutomationStep`]s hang off it, the edges and wires between them
/// are drawn on it, and `entry_step_id` is the step a placement of it opens first.
///
/// It names no agent and no model: who is asked to carry a prompt out is each [`AutomationStep`]'s
/// answer, so two automations can run the same action with different agents.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AutomationAction {
    pub id: i64,
    /// The project whose library this is in, or `None` for the device's own.
    #[serde(default)]
    pub project_id: Option<i64>,
    pub name: String,
    /// The step this action opens first. `None` while it is still being built; launching an
    /// automation that places it is refused at the launch check, not here.
    #[serde(default)]
    pub entry_step_id: Option<i64>,
    pub order_key: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// **One automation** — the actions placed on it, what runs after what, and the preamble every step's
/// launch carries.
///
/// `entry_placement_id` is where a run starts; from it the edges are walked, and the place a
/// placement sits in the picture and the number it is drawn with both fall out of that walk rather
/// than out of `order_key`, which records only the order things were added in.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Automation {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub notes: String,
    /// Prepended to every step's launch. Kept short — the material a prompt would otherwise repeat
    /// belongs in an [`AutomationNote`].
    pub preamble: String,
    /// The placement a run opens first. `None` while the automation is still being built; launching
    /// without one is refused at the launch check, not here.
    #[serde(default)]
    pub entry_placement_id: Option<i64>,
    #[serde(default)]
    pub archived: bool,
    pub order_key: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// **One action, placed on one automation.** It carries no prompt, no agent and no way out of its own
/// — those are the action's. What is this row's is which action stands here, and, through the
/// [`AutomationCfg`] answers and the lines drawn onto it, everything that belongs to this spot rather
/// than to the library. The same action placed twice gives two rows.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AutomationPlacement {
    pub id: i64,
    pub automation_id: i64,
    pub action_id: i64,
    pub order_key: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// **A document the placements of one automation share.** Long is fine here; which placements are
/// handed it is [`AutomationPlacementNote`]'s to say.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AutomationNote {
    pub id: i64,
    pub automation_id: i64,
    pub name: String,
    pub body: String,
    pub order_key: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// **One step of one action**, and one terminal when it is opened. Its `prompt` is its own — an action
/// holds the prompts rather than being one.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AutomationStep {
    pub id: i64,
    pub action_id: i64,
    pub name: String,
    pub prompt: String,
    pub agent: String,
    /// `None` leaves the agent's own default model.
    #[serde(default)]
    pub model: Option<String>,
    /// May this step wait for a person? A step that does not say so is not left standing on one.
    #[serde(default)]
    pub interactive: bool,
    /// The name of the setting or the input the working folder is taken from — a name, not a path, so
    /// the answer is given where the action is placed rather than baked in here.
    #[serde(default)]
    pub work_dir_ref: Option<String>,
    /// Does this step's report also land as a comment on the task?
    #[serde(default)]
    pub report_to_task: bool,
    /// Is the run's story so far handed to this step? On unless somebody turns it off.
    #[serde(default)]
    pub show_history: bool,
    pub order_key: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// **A setting, declared by an action and answered where it is placed.** The action's row is the
/// declaration alone, so its `value` is `None`; each placement of that action carries a row of its own
/// under the same `name`, and that is where the answer written while building sits.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomationCfg {
    pub id: i64,
    pub owner_kind: AutomationCfgOwner,
    pub owner_id: i64,
    pub name: String,
    pub kind: AutomationCfgKind,
    pub required: bool,
    /// The choices, as JSON, for `kind = Choice`. `None` for every other kind.
    #[serde(default)]
    pub options: Option<String>,
    /// The answer written while building, as JSON. `None` on the action's declaration row.
    #[serde(default)]
    pub value: Option<String>,
    pub order_key: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// **Which shared documents a placement is handed**, and with it every step opened under it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AutomationPlacementNote {
    pub id: i64,
    pub placement_id: i64,
    pub note_id: i64,
    pub order_key: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// **A way out of a step or an action**, named by whoever built it. Which one the agent took is the
/// whole condition the next box is chosen by. `name` `None` is the unnamed way out, which is what an
/// owner with only one has; [`ERROR_EXIT`] is the one every owner carries from birth.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomationExit {
    pub id: i64,
    pub owner_kind: AutomationOwner,
    pub owner_id: i64,
    #[serde(default)]
    pub name: Option<String>,
    pub order_key: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// **What a step or an action takes in, and what a way out of it hands on.** One record for both, told
/// apart by `direction`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomationPort {
    pub id: i64,
    pub owner_kind: AutomationPortOwner,
    pub owner_id: i64,
    pub direction: AutomationPortDirection,
    pub name: String,
    pub kind: AutomationPortKind,
    pub required: bool,
    pub order_key: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// **What happens after a way out is taken.** The edge carries no condition of its own: the exit *is*
/// the condition.
///
/// `owner_kind` says which picture the line is drawn on, and with it what `from_id` and `to_id` name —
/// a placement on an automation, a step inside an action.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomationEdge {
    pub id: i64,
    pub owner_kind: AutomationPictureOwner,
    pub owner_id: i64,
    pub from_id: i64,
    /// The way out this edge hangs on — `None` for the unnamed one, [`ERROR_EXIT`] for the error one.
    #[serde(default)]
    pub exit_name: Option<String>,
    /// Where it goes, for `ends = Go`. `None` for `Done` and `Halt`, which go nowhere.
    #[serde(default)]
    pub to_id: Option<i64>,
    pub ends: AutomationEnds,
    /// How often this edge may be taken for one task. `None` is no limit, which is the right answer for
    /// an edge into a box that takes a fresh task.
    #[serde(default)]
    pub max_times: Option<i64>,
    pub order_key: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// **What is handed from one box to the next**, on either of the two pictures an [`AutomationEdge`] is
/// drawn on. Both ends are named rather than keyed: one action placed twice on an automation gives two
/// placements whose ports carry the same names, so only `from_id` + `from_exit_name` + `from_port_name`
/// says which of them is meant.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AutomationWire {
    pub id: i64,
    pub owner_kind: AutomationPictureOwner,
    pub owner_id: i64,
    pub from_id: i64,
    #[serde(default)]
    pub from_exit_name: Option<String>,
    pub from_port_name: String,
    pub to_id: i64,
    pub to_port_name: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

// ───────────────────────── automation: what ran ─────────────────────────

/// Where one launch of one automation stands.
///
/// `Running` is where every launch begins: nothing limits how many may be under way at once, so a
/// launch never waits (`AMB-D-947`). `Paused` keeps the place, `Done` and `Stopped` are the two ends —
/// reached by running out of picture, and by everything else.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationRunStatus {
    #[default]
    Running,
    Paused,
    Done,
    Stopped,
}

impl AutomationRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AutomationRunStatus::Running => "running",
            AutomationRunStatus::Paused => "paused",
            AutomationRunStatus::Done => "done",
            AutomationRunStatus::Stopped => "stopped",
        }
    }

    pub fn parse(s: &str) -> Option<AutomationRunStatus> {
        match s {
            "running" => Some(AutomationRunStatus::Running),
            "paused" => Some(AutomationRunStatus::Paused),
            "done" => Some(AutomationRunStatus::Done),
            "stopped" => Some(AutomationRunStatus::Stopped),
            _ => None,
        }
    }
}

/// Why a run stopped, where stopping was not the picture running out.
///
/// It is stored rather than worked out afterwards: the records show a crash (the step execution is left
/// `failed`) and say nothing about a loop that ran out of turns, an agent that was not there, or a
/// person who pressed stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationStoppedReason {
    Crashed,
    MaxTimes,
    NoAgent,
    ByHuman,
    /// Nothing was left for it to open: the picture it was copied from stopped leading anywhere while
    /// it was out ([`crate::ops::automation_run::Waiting::NoWayOn`]).
    NoWayOn,
}

impl AutomationStoppedReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            AutomationStoppedReason::Crashed => "crashed",
            AutomationStoppedReason::MaxTimes => "max_times",
            AutomationStoppedReason::NoAgent => "no_agent",
            AutomationStoppedReason::ByHuman => "by_human",
            AutomationStoppedReason::NoWayOn => "no_way_on",
        }
    }

    pub fn parse(s: &str) -> Option<AutomationStoppedReason> {
        match s {
            "crashed" => Some(AutomationStoppedReason::Crashed),
            "max_times" => Some(AutomationStoppedReason::MaxTimes),
            "no_agent" => Some(AutomationStoppedReason::NoAgent),
            "by_human" => Some(AutomationStoppedReason::ByHuman),
            "no_way_on" => Some(AutomationStoppedReason::NoWayOn),
            _ => None,
        }
    }
}

/// **One launch of one automation.**
///
/// `pause_requested` is the gap between the button and the pause: a step is under way and cannot be cut
/// in half, so the request is recorded and the run reaches `paused` when that step reports.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AutomationRun {
    pub id: i64,
    pub automation_id: i64,
    /// The project the automation was launched from, carried here so a run can be found without
    /// walking back through a definition that may since have been archived.
    pub project_id: i64,
    pub status: AutomationRunStatus,
    pub pause_requested: bool,
    /// Why it stopped. Set only while `status` is `Stopped`.
    #[serde(default)]
    pub stopped_reason: Option<AutomationStoppedReason>,
    /// Who pressed launch. `None` for a run whose launcher said nothing about itself.
    #[serde(default)]
    pub started_by_kind: Option<ActorKind>,
    /// When it began. Set at launch, since a launch starts on the spot (`AMB-D-947`); `None` only on a
    /// run written by a build that could still leave one waiting.
    #[serde(default)]
    pub started_at: Option<Timestamp>,
    #[serde(default)]
    pub ended_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// **The step as it was at launch** — one record per step of the automation, written when the run is
/// created and never rewritten.
///
/// This is what makes a run readable months later: the automation it came from has moved on, and these
/// columns still say what was actually asked. The three JSON fields hold what has no column of its own —
/// the ways out and each one's outputs, the step's inputs, and the settings' answers — and are read back
/// whole, never queried into.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AutomationRunDef {
    pub id: i64,
    pub run_id: i64,
    /// Which spot of the picture this copy was opened from, or `None` where that placement has since
    /// been taken off.
    #[serde(default)]
    pub placement_id: Option<i64>,
    /// The way back to the live definition, or `None` where that step has since been deleted.
    #[serde(default)]
    pub step_id: Option<i64>,
    pub name: String,
    /// The prompt as it read at launch.
    #[serde(default)]
    pub prompt: Option<String>,
    pub agent: String,
    #[serde(default)]
    pub model: Option<String>,
    pub interactive: bool,
    #[serde(default)]
    pub work_dir_ref: Option<String>,
    pub report_to_task: bool,
    pub show_history: bool,
    /// The ways out, with the outputs declared on each — JSON ([`RunDefExit`]).
    pub exits: String,
    /// The inputs the step takes — JSON ([`RunDefPort`]).
    pub ins: String,
    /// The settings and the answers written for them — JSON ([`RunDefCfg`]).
    pub cfg: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// One way out, as [`AutomationRunDef::exits`] holds it: the name, and what leaves through it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunDefExit {
    /// `None` is the unnamed way out; [`ERROR_EXIT`] is the error one.
    #[serde(default)]
    pub name: Option<String>,
    pub outs: Vec<RunDefPort>,
}

/// One port, as the snapshot holds it — a name, what it carries, and whether it has to be there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunDefPort {
    pub name: String,
    pub kind: AutomationPortKind,
    pub required: bool,
}

/// One setting and the answer written for it while the automation was built.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunDefCfg {
    pub name: String,
    pub kind: AutomationCfgKind,
    pub required: bool,
    #[serde(default)]
    pub options: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
}

/// **How one step execution ended** — or that it has not.
///
/// `Failed` is the step that fell over, which is a different fact from the agent reporting that it found
/// nothing: the first leaves through the error way out and the second through one somebody declared.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationRunStepStatus {
    #[default]
    Running,
    Done,
    Failed,
    Stopped,
}

impl AutomationRunStepStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AutomationRunStepStatus::Running => "running",
            AutomationRunStepStatus::Done => "done",
            AutomationRunStepStatus::Failed => "failed",
            AutomationRunStepStatus::Stopped => "stopped",
        }
    }

    pub fn parse(s: &str) -> Option<AutomationRunStepStatus> {
        match s {
            "running" => Some(AutomationRunStepStatus::Running),
            "done" => Some(AutomationRunStepStatus::Done),
            "failed" => Some(AutomationRunStepStatus::Failed),
            "stopped" => Some(AutomationRunStepStatus::Stopped),
            _ => None,
        }
    }
}

/// **One task a run worked on**, in the order it took them — a stretch of the run rather than a moment
/// of it.
///
/// `seq` is 1 for a run that never goes back, and the row is what the per-task edge counts are kept
/// against. `task_id` is empty until a `task_take` output hands one over, so the row exists before it
/// has a subject.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AutomationRunTask {
    pub id: i64,
    pub run_id: i64,
    pub seq: i64,
    /// The task this stretch is about, or `None` while nothing has handed one over — and where the
    /// task has since been deleted.
    #[serde(default)]
    pub task_id: Option<i64>,
    #[serde(default)]
    pub started_at: Option<Timestamp>,
    #[serde(default)]
    pub ended_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// **One step, run once.** `seq` is the move number within the run, so a step the run comes back to has
/// a row per visit.
///
/// `report` is what the agent said when it finished, kept whole. The story handed to later steps shows
/// its first line only, and the full text is here.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AutomationRunStep {
    pub id: i64,
    pub run_id: i64,
    /// The step as it stood at launch — what this execution was asked to do.
    pub run_def_id: i64,
    /// The stretch of the run this execution belongs to. `None` only where a step went looking for a
    /// task and found none.
    #[serde(default)]
    pub run_task_id: Option<i64>,
    pub seq: i64,
    /// The way out the agent left through — `None` for the unnamed one, and while it is still running.
    #[serde(default)]
    pub exit_name: Option<String>,
    pub report: String,
    pub status: AutomationRunStepStatus,
    #[serde(default)]
    pub started_at: Option<Timestamp>,
    #[serde(default)]
    pub ended_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// **One value that went in or came out of a step execution**, under the port's *name* — the port row
/// itself may be re-declared or gone by the time this is read, so the name is what is kept.
///
/// Which of the three payload fields means anything is [`AutomationRunValue::kind`]'s to say.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomationRunValue {
    pub id: i64,
    /// The step execution that received it (`In`) or produced it (`Out`).
    pub run_step_id: i64,
    pub direction: AutomationPortDirection,
    /// The way out it left through, for an `Out`. `None` on an `In`, and on the unnamed way out.
    #[serde(default)]
    pub exit_name: Option<String>,
    pub name: String,
    pub kind: AutomationPortKind,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub attachment_id: Option<i64>,
    #[serde(default)]
    pub task_id: Option<i64>,
    /// Where an `In` value came from — without it the chain from a value back to what produced it is
    /// not in the store. An `Out` has nothing to say here.
    #[serde(default)]
    pub from_run_step_id: Option<i64>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// A serde-shaped vessel holding every record of one store at once. It is **not the store's contents**:
/// the truth source is SQLite, and [`crate::store::Store`] does not hold one of these. The shape exists
/// for the two places that need the records handed over **as a single lump**: verifying a backup or a
/// restore (that this vessel can be raised from a snapshot is the proof that the snapshot has not just the
/// structure but the contents — [`mod@crate::archive`]), and the projection-parity tests (raise the vessel
/// from the truth source, project it back onto the engine, and compare for fidelity). It is not the
/// substrate of export: export streams rows straight out of the read model and never
/// hydrates into this ([`crate::export`]). Nor is there any way to load rows into a store through it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Database {
    pub schema_version: String,
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub task_dependencies: Vec<TaskDependency>,
    /// A task's recorded commit SHAs. Hydration tolerates their absence — a store predating the table
    /// yields an empty vec.
    #[serde(default)]
    pub task_commits: Vec<TaskCommit>,
    #[serde(default)]
    pub decisions: Vec<Decision>,
    /// Edges between decisions. Hydration tolerates their absence — a store without them yields an empty
    /// vec.
    #[serde(default)]
    pub decision_edges: Vec<DecisionEdge>,
    #[serde(default)]
    pub decision_task_links: Vec<DecisionTaskLink>,
    /// The unified dimension model. Hydration tolerates its absence — a store without it yields an empty
    /// vec.
    #[serde(default)]
    pub dimensions: Vec<Dimension>,
    #[serde(default)]
    pub dimension_values: Vec<DimensionValue>,
    #[serde(default)]
    pub task_dimension_values: Vec<TaskDimensionValue>,
    /// The decision side of the dimension model. Hydration tolerates its absence — a store predating
    /// the table yields an empty vec.
    #[serde(default)]
    pub decision_dimension_values: Vec<DecisionDimensionValue>,
    #[serde(default)]
    pub task_comments: Vec<TaskComment>,
    /// Comments on decision records. Hydration tolerates their absence — a store without them yields an
    /// empty vec.
    #[serde(default)]
    pub decision_comments: Vec<DecisionComment>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

/// The current schema version.
pub const SCHEMA_VERSION: &str = "1";

/// The store **format version** this binary reads and writes — a monotonically increasing scalar, and a
/// different thing entirely from the frozen `SCHEMA_VERSION` (`"1"`). It is bumped only by a **breaking
/// migration**: one that drops or renames a column or table an older reader's SQL needs. Every write-path
/// open stamps this version into `store_meta.format_version`. The forward-migration gate compares the
/// `format_version` a store recorded against this constant, and if `store > FORMAT_VERSION` it fails with
/// a clear error: this store has been updated by a newer Amenbo, so update to the latest one
/// (`amenbo update`). **The chain decides the version.** This constant *is* the end of the version chain
/// ([`crate::store_engine::migrate::LATEST_VERSION`]), and **a number is never written here**: add one
/// step and the version goes up; without a step it cannot. The thing that carries a breaking migration is
/// that numbered step, not the open. A store with no `format_version` key **reads as v0**
/// ([`crate::store_engine::read_format_version`]).
/// The chain's baseline ([`crate::store_engine::migrate::BASELINE_VERSION`]) is the oldest store this
/// build can open; there is no path back to anything older.
pub const FORMAT_VERSION: i64 = crate::store_engine::migrate::LATEST_VERSION;

impl Default for Database {
    fn default() -> Self {
        Database {
            schema_version: SCHEMA_VERSION.to_string(),
            projects: Vec::new(),
            tasks: Vec::new(),
            task_dependencies: Vec::new(),
            task_commits: Vec::new(),
            decisions: Vec::new(),
            decision_edges: Vec::new(),
            decision_task_links: Vec::new(),
            dimensions: Vec::new(),
            dimension_values: Vec::new(),
            task_dimension_values: Vec::new(),
            decision_dimension_values: Vec::new(),
            task_comments: Vec::new(),
            decision_comments: Vec::new(),
            attachments: Vec::new(),
        }
    }
}

