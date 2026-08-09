//! amenbo-scenario — the schema and validating loader for pre-distribution
//! verification scenarios.
//!
//! A scenario is one declarative YAML file: what is being proved, stated once as the `title`,
//! and the way there written per driver. `steps_cli` is the road the CLI driver
//! (`verification/cli`) walks as shipped-binary invocations; `steps_gui` is the one the mac GUI
//! harness (`verification/gui`) walks as screen instructions. The Linux OCR harness
//! (`scripts/docker/gui-e2e.sh`) is fed the card title its host launcher (`make verify-gui-linux`)
//! resolves through this crate's JSON face (the `emit` bin), since that container carries no
//! toolchain to read the YAML itself. Nothing here knows about a command line or a pixel.
//!
//! Steps having an owner is what says which drivers run the file: a list with steps in it is that
//! driver's to carry, an empty one is a road it is not asked to walk. There is no separate
//! declaration to disagree with the steps.
//!
//! Two layers of checking, both surfaced as clear failures:
//!   * [`load_str`] / [`load_file`] — the YAML must parse into the typed model
//!     (`deny_unknown_fields` catches misspelled keys).
//!   * [`Scenario::validate`] — the semantic pass, run over each driver's steps on its own:
//!     known ops only, required args present, each arg of the type its op takes, and every
//!     `target:` resolving to an earlier `as:` binding in the same list.

use std::collections::HashSet;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// A whole scenario: one goal, under an id and a human title, and the ordered steps each driver
/// takes to reach it.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// Stable kebab-case identifier, unique across the scenario set.
    pub id: String,
    /// One-line human description of what the scenario proves.
    pub title: String,
    /// Optional longer prose (rationale, preconditions).
    #[serde(default)]
    pub description: Option<String>,
    /// The world that has to be standing before any road is walked — the records a road takes for
    /// granted and does not make: a project that is already there, a catalog already registered, a
    /// plugin already installed, a folder already linked. Written as actions, and left to the driver
    /// to stand up before it starts, so an operator reads what a screen needs rather than guessing
    /// it. What it may not carry is the screen's own moves: those are what a road is for, and a
    /// premise that carried them would verify itself.
    #[serde(default)]
    pub given: Vec<Step>,
    /// The road the CLI driver walks. Empty means the CLI is not asked to walk one.
    #[serde(default)]
    pub steps_cli: Vec<Step>,
    /// The road the GUI harnesses walk — written on its own because a screen's road is a different
    /// shape, not a rendering of the CLI's. Linking a folder to a project is one command to type and
    /// three things to do on screen; written once for both, one of the two comes out bent.
    #[serde(default)]
    pub steps_gui: Vec<Step>,
}

/// A driver a scenario can be run through — the harnesses that map the one goal to their world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Driver {
    /// `verify-cli` / `verify-all`: drives the shipped binary and judges `--json`.
    Cli,
    /// `verify-gui`: renders the scenario as a screen checklist, judged by OCR (the mac harness, and
    /// the Linux container the same card is passed into).
    Gui,
}

impl Driver {
    /// Both drivers, in the order a report names them.
    pub const ALL: [Driver; 2] = [Driver::Cli, Driver::Gui];

    /// The wire token, as it is written in a scenario.
    pub fn as_str(self) -> &'static str {
        match self {
            Driver::Cli => "cli",
            Driver::Gui => "gui",
        }
    }

    /// The key this driver's steps are written under, for a message that has to name it.
    pub fn steps_key(self) -> &'static str {
        match self {
            Driver::Cli => "steps_cli",
            Driver::Gui => "steps_gui",
        }
    }
}

/// One step. `type` selects the variant; every step names the [`Domain`] object it
/// touches and the `op` performed on it.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Step {
    /// A domain operation that changes state — or, with `refused:` among its args, one the
    /// scenario says amenbo will turn away.
    Action {
        domain: Domain,
        op: String,
        /// Named arguments for the op. A string value under the key `target` is a
        /// reference to an earlier step's `as:` binding.
        ///
        /// One key is not the op's own: `refused: <error code>` says this operation is expected to
        /// be **rejected**, and names the code it must be rejected with. Then the step is judged
        /// like an assert — the refusal is what passes, going through is what fails, and a refusal
        /// for some other reason fails too, since a guard that turns the operation away for the
        /// wrong reason is not the guard under test.
        #[serde(default)]
        with: Args,
        /// Optional binding name so later steps can refer to what this produced.
        #[serde(default, rename = "as")]
        bind: Option<String>,
    },
    /// An expected result about domain state.
    Assert {
        domain: Domain,
        op: String,
        #[serde(default)]
        with: Args,
    },
}

impl Step {
    fn kind(&self) -> Kind {
        match self {
            Step::Action { .. } => Kind::Action,
            Step::Assert { .. } => Kind::Assert,
        }
    }
    fn domain(&self) -> Domain {
        match self {
            Step::Action { domain, .. } | Step::Assert { domain, .. } => *domain,
        }
    }
    fn op(&self) -> &str {
        match self {
            Step::Action { op, .. } | Step::Assert { op, .. } => op,
        }
    }
    fn with(&self) -> &Args {
        match self {
            Step::Action { with, .. } | Step::Assert { with, .. } => with,
        }
    }
}

/// Free-form named arguments. Values stay as YAML so a driver interprets them; the loader
/// only inspects the few keys it validates (`target`, `present`, `ok`, `refused`).
pub type Args = std::collections::BTreeMap<String, serde_yaml::Value>;

/// The domain object a step touches. Kept small and closed on purpose — an unknown domain
/// is a scenario bug, not an extension point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    Task,
    Decision,
    Comment,
    Project,
    /// A classification axis and its values. The axis and its value are named by the words a user
    /// types — a dimension is reached by name, not by an id an earlier step bound.
    Dimension,
    /// This device's amenbo itself, rather than anything filed in it: its configuration, the
    /// identity it answers `whoami` with, the build in place — and the store as a whole, which is
    /// what comes out of it (`export`), what is set aside (`backup`), what goes back in (`restore`)
    /// and whether it is sound (`doctor`).
    Store,
    /// A folder and the project its `.amenbo` pointer names — what an AI launched there may reach.
    Folder,
    /// A file or a link hung on a task, a decision or a comment — the one place amenbo carries bytes.
    Attachment,
    /// The working folder amenbo is used from, rather than anything in the store: the files a person
    /// has lying there, and the git repository the lint hooks stand in front of the commits of.
    Repo,
    /// A plugin on this machine: what is installed, whose gate is open, what a call returned, and
    /// what the execution log kept. Named by the name it carries in the catalog, never by a binding.
    Plugin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Action,
    Assert,
}

// ---------------------------------------------------------------------------
// The op registry — the closed vocabulary a scenario may use. The loader fails
// closed: an op not listed here is rejected, so a typo never runs as a no-op.
// Drivers grow this table (and their own mapping) as new ops are needed.
// ---------------------------------------------------------------------------

struct OpSpec {
    kind: Kind,
    domain: Domain,
    op: &'static str,
    /// `with` keys that must be present.
    required: &'static [&'static str],
    /// `with` keys whose value names an earlier `as:` binding. `target` is the usual one, but an op
    /// that joins two objects names both sides, and each has to be checked or a typo on the second
    /// one reaches the driver as a binding that was never produced.
    refs: &'static [&'static str],
    /// `with` keys whose value must be a string when the step carries it — the words the op hands
    /// the binary, rather than the yes/no and the counts it also takes. YAML types an unquoted
    /// scalar by its shape, so a value that happens to look like a number or a date arrives as one
    /// (a SHA of nothing but digits is the case that bit), and the driver only meets it at the far
    /// end of a run. Named here, the lint meets it first.
    strings: &'static [&'static str],
    /// Whether this op may carry an `as:` binding (true only for ops that produce something a
    /// later step can name — an object in the store, or the file a `store` action writes beside it).
    binds: bool,
}

const REGISTRY: &[OpSpec] = &[
    // Actions
    // `project` names the board it lands on, for a world that has to put work in front of a project
    // the run does not itself stand in — a board with a card on it is a different screen from an
    // empty one, and which project it is drawn for is the whole question where a road walks to a
    // named project. Left out, it is the run's own project like everything else.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "create", required: &["title"], refs: &["project"], strings: &["title"], binds: true },
    // The other half of that creation. Between the two the task is on the board and in every listing,
    // out of the mailbox and refused a reservation, so a road that means to hand work over walks this
    // step — and one that reserves has to, or it meets the guard instead. It names the task and
    // nothing else: nobody is being asked to approve it, so there is nothing further for a step to
    // say, and it binds nothing because the task it finishes was bound where it was created.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "finish-creating", required: &["target"], refs: &["target"], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "assign", required: &["target", "assignee"], refs: &["target"], strings: &["assignee"], binds: false },
    // Posting binds the comment, since editing, removing and promoting one all name it afterwards.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "comment", required: &["target", "text"], refs: &["target"], strings: &["text"], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Comment, op: "edit", required: &["target", "text"], refs: &["target"], strings: &["text"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Comment, op: "rm", required: &["target"], refs: &["target"], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Comment, op: "promote", required: &["target", "title"], refs: &["target"], strings: &["title"], binds: true },
    // The progress states, each by the command a user reaches for: `status` is the explicit move
    // (and the reserve), `done` / `reopen` / `block` are the three the CLI gives their own verb.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "status", required: &["target", "status"], refs: &["target"], strings: &["status"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "done", required: &["target"], refs: &["target"], strings: &[], binds: false },
    // The other terminal. A reason is required by the command, so it is required here: what separates
    // work decided against from work carried out is why, and it is recorded rather than remembered.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "reject", required: &["target", "reason"], refs: &["target"], strings: &["reason"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "reopen", required: &["target"], refs: &["target"], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "block", required: &["target", "reason"], refs: &["target"], strings: &["reason"], binds: false },
    // Editing a task's own fields: `update` sets the ones it names, `clear` takes one back.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "update", required: &["target"], refs: &["target"], strings: &["title", "notes", "due", "start", "priority"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "clear", required: &["target", "field"], refs: &["target"], strings: &["field"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "move", required: &["target"], refs: &["target", "project"], strings: &["position"], binds: false },
    // Narrowing a listing that is already drawn, by the words a reader types over it. The words travel
    // as words and not as a line of filter grammar — that grammar splits on whitespace, so a phrase
    // would arrive as its first word alone — and they are ANDed over the record, across every face the
    // word index covers rather than the two a card shows.
    //
    // A screen road alone. A terminal has no listing standing in front of it to narrow: the words and
    // the reading are one command there, which is what `found` walks.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "narrow", required: &["words"], refs: &[], strings: &["words"], binds: false },
    // Pressing a hit through to the record it points at. The excerpt beside a hit is cut to say where
    // the words are written and never to be read in place of the record, so the press is what the hit
    // is for. The words are named here because a hit has to be standing before there is one to press,
    // and asking for them is the move that draws it.
    //
    // A screen road alone. A terminal prints its hits as text and the reader types the ref it read
    // into `show`, so there is nothing there to press.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "open-hit", required: &["words", "target"], refs: &["target"], strings: &["words"], binds: false },
    // A project's own life: its fields, where it sits in the list, and whether it is still in play.
    OpSpec { kind: Kind::Action, domain: Domain::Project, op: "create", required: &["name"], refs: &[], strings: &["name"], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Project, op: "update", required: &["target"], refs: &["target"], strings: &["name", "notes", "view"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Project, op: "move", required: &["target", "position"], refs: &["target"], strings: &["position"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Project, op: "archive", required: &["target"], refs: &["target"], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Project, op: "unarchive", required: &["target"], refs: &["target"], strings: &[], binds: false },
    // The end of the line, and the one project operation that reaches outside the store: the folders
    // bound to a deleted project are released with it, so a scenario naming this op is asking about the
    // teardown as much as about the row.
    OpSpec { kind: Kind::Action, domain: Domain::Project, op: "delete", required: &["target"], refs: &["target"], strings: &[], binds: false },
    // Standing on the screen a project keeps for itself, which is one of the two faces a project ×
    // plugin crossing is read from. The project is named rather than bound, for the reason `enable-in`'s
    // is: the world a plugin road wants is stood up outside the run, so there is no earlier step to have
    // made this project.
    //
    // A screen road alone. A terminal is already standing in a project — the folder it is run from says
    // which — so there is nowhere to move to and nothing this would do.
    OpSpec { kind: Kind::Action, domain: Domain::Project, op: "open-settings", required: &["project"], refs: &[], strings: &["project"], binds: false },
    // And back onto the board that project keeps. It is the move a road needs where what is under test
    // is drawn when a project is opened rather than held from before: walking out and back in is the
    // only way to ask a screen what it does on arrival, and a road that assumed the arrival would be
    // reading the screen it never left.
    OpSpec { kind: Kind::Action, domain: Domain::Project, op: "open", required: &["project"], refs: &[], strings: &["project"], binds: false },
    // A classification axis, its values, and the assignment that files a task under one. The axis and
    // the value travel as names — that is how the CLI takes them, and how a person says them.
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "create", required: &["name"], refs: &[], strings: &["name"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "value-add", required: &["dimension", "value"], refs: &[], strings: &["dimension", "value"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "set", required: &["target", "dimension", "value"], refs: &["target"], strings: &["dimension", "value"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "unset", required: &["target", "dimension", "value"], refs: &["target"], strings: &["dimension", "value"], binds: false },
    // Ordering between two tasks, and the anchor back to the history that carried the work out.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "depend", required: &["target", "on"], refs: &["target", "on"], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "undepend", required: &["target", "on"], refs: &["target", "on"], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "commit-add", required: &["target", "sha"], refs: &["target"], strings: &["sha"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "commit-rm", required: &["target", "sha"], refs: &["target"], strings: &["sha"], binds: false },
    // `project` names the shelf it is filed on, for a scenario about where a record ends up; left
    // out, it is the run's own project like everything else.
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "create", required: &["title"], refs: &["project"], strings: &["title"], binds: true },
    // A decision's own life: the body is edited while it is still proposed, accepting freezes it,
    // and the link is what makes it a task's premise.
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "edit", required: &["target", "body"], refs: &["target"], strings: &["body"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "accept", required: &["target"], refs: &["target"], strings: &[], binds: false },
    // The other two rulings a proposal can meet: turned down, and un-settled to be discussed again.
    // A `reason` is optional here as it is on the command, and lands on the decision's timeline.
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "reject", required: &["target"], refs: &["target"], strings: &["reason"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "reopen", required: &["target"], refs: &["target"], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "link", required: &["target", "task"], refs: &["target", "task"], strings: &[], binds: false },
    // The edges between decisions, each named from the newer one: `supersede` replaces an older
    // decision, `builds-on` names a premise to read first, and `unlink` takes an edge back. A pair
    // carries one edge, so naming the pair is how the last one names which.
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "supersede", required: &["target", "replaces"], refs: &["target", "replaces"], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "builds-on", required: &["target", "on"], refs: &["target", "on"], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "unlink", required: &["target", "from"], refs: &["target", "from"], strings: &[], binds: false },
    // A decision's timeline is its own: the body freezes on acceptance, the comments do not.
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "comment", required: &["target", "text"], refs: &["target"], strings: &["text"], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "comment-edit", required: &["target", "text"], refs: &["target"], strings: &["text"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "comment-rm", required: &["target"], refs: &["target"], strings: &[], binds: false },
    // The other promotion. A decision's comment is raised into a record of its own, and it is an op
    // apart from the task side's because the two comment tables number independently: which one a
    // number names is said in the ref, and a step cannot leave that to be guessed.
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "comment-promote", required: &["target", "title"], refs: &["target"], strings: &["title"], binds: true },
    // The store as a whole. `export` and `backup` write a file the run keeps, and bind it by the
    // same `as:` every other producing op uses — so `restore` names the archive it puts back the
    // way a step names any other earlier result, and a mistyped name is caught here rather than in
    // a driver.
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "export", required: &[], refs: &[], strings: &[], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "backup", required: &[], refs: &[], strings: &[], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "restore", required: &["target"], refs: &["target"], strings: &[], binds: false },
    // The road out for something that keeps a copy of this store elsewhere, in the two faces it is
    // used in. `sync-version` asks the one number the window is at, and binds **the number** rather
    // than a file — a third thing an `as:` can hold, alongside an object's id and an archive's path,
    // and the only shape in which a road can say later that it moved (or did not). `sync-snapshot`
    // writes the whole window as one document and binds that, the way `export` binds what it wrote.
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "sync-version", required: &[], refs: &[], strings: &[], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "sync-snapshot", required: &[], refs: &[], strings: &[], binds: true },
    // Erasing content from the truth source itself: a comment goes in full, a decision keeps its
    // number and loses its body to the replacement text.
    OpSpec { kind: Kind::Action, domain: Domain::Comment, op: "hard-erase", required: &["target"], refs: &["target"], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "hard-erase", required: &["target", "body"], refs: &["target"], strings: &["body"], binds: false },
    // The store's own settings, changed the one way a user can change them.
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "config-set", required: &["key", "value"], refs: &[], strings: &["key", "value"], binds: false },
    // The other face of the integrity check: the one that puts right what the reading face reports.
    // It is an action and not an assert precisely because it writes — what it swept is read back by
    // asking the reading face again.
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "doctor-fix", required: &[], refs: &[], strings: &[], binds: false },
    // The bytes an attachment left behind, aged past the boundary that spares a young blob
    // (`GC_MIN_AGE`, an hour). Removing an attachment reclaims its blob only if it is already old
    // enough, so what a run creates is always too young to sweep — and the sweep that exists for
    // exactly this would go unproven. Backdating is the only way to reach the state from a scenario,
    // the way `folder legacy-pointer` reaches its own.
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "age-blobs", required: &[], refs: &[], strings: &[], binds: false },
    // A device somebody has been coming back to. What says so is two numbers the store keeps and no
    // command sets: how many times the app has been launched here, and on how many separate days
    // something was written. They are what a usage nudge is held behind, and both are reached by
    // living rather than by doing — a run that tried to earn them would have to be several days long.
    // So the driver backdates them, the way `age-blobs` backdates the bytes a sweep is about:
    // `launches` is written to the tally as it stands, and `days` spreads the records already in the
    // store back over that many separate days (the store has to hold at least that many).
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "worn-in", required: &["launches", "days"], refs: &[], strings: &[], binds: false },
    // The answer given to a nudge that came up on its own. A screen road alone: nothing in a terminal
    // puts one, so the CLI driver never meets it.
    //
    // The answer travels as a value rather than in the op's name, the way a consent answer does. The
    // one this road gives is the refusal, and it is the only one the driver takes: an acceptance
    // registers this machine's login with the OS, which is the one piece of state no throwaway store
    // can hold and no run can hand back — that half is walked on real machines instead.
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "nudge-answer", required: &["nudge", "answer"], refs: &[], strings: &["nudge", "answer"], binds: false },
    // What a folder's binding is made of. A folder is named, not pointed at: `dir` is a plain name
    // the driver places somewhere of its own, since a pointer is answered by where a folder sits.
    // `init` raises a project of its own and binds it (hence the binding), `bind` points a folder at
    // one that already exists — this run's, unless `project` names another.
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "init", required: &["dir"], refs: &[], strings: &["dir"], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "bind", required: &["dir"], refs: &["project"], strings: &["dir"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "unbind", required: &["dir"], refs: &[], strings: &["dir"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "sync-guide", required: &["dir"], refs: &[], strings: &["dir"], binds: false },
    // A pointer left in the shape an older amenbo wrote, in a folder that is bound. Nothing amenbo
    // does today writes one — it is the state a repair exists for, so a scenario about the repair has
    // to put the folder in it, the way `repo write-file` puts a file a person already had.
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "legacy-pointer", required: &["dir"], refs: &[], strings: &["dir"], binds: false },
    // The moves a screen's road is made of where a terminal's road has none. Linking a folder to a
    // project is one command to type; on screen it is a card that opens, and that then asks which of
    // this device's projects the folder is to be linked to. Both are written down as steps rather
    // than as a note beside the file, and for two reasons: a move nobody wrote down is a move nobody
    // shoots, so the card as it opens is evidence that exists nowhere else — and an assert reached by
    // a hand quietly tidying the screen between shots passes without the run ever proving it could
    // get there.
    //
    // A screen road alone: there is no card to open in a terminal, so the CLI driver never meets
    // these and maps neither.
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "open-existing-card", required: &[], refs: &[], strings: &[], binds: false },
    // Which project is answered by name, the way a person answers it — the card lists what the store
    // holds, and a name is what they read there. It is not a binding: nothing in a screen road made
    // this project, it was already on the device before the run started.
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "choose-project", required: &["project"], refs: &[], strings: &["project"], binds: false },
    // Hanging bytes or a link on a record. Each `attach` names either a `file` the run wrote or a
    // `url`, and binds the attachment, since managing one afterwards means naming it.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "attach", required: &["target"], refs: &["target"], strings: &["file", "url", "name"], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "attach", required: &["target"], refs: &["target"], strings: &["file", "url", "name"], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Comment, op: "attach", required: &["target"], refs: &["target"], strings: &["file", "url", "name"], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Attachment, op: "rm", required: &["target"], refs: &["target"], strings: &[], binds: false },
    // The folder the run works in: the files a person already has there, the repository the hooks
    // are written into, and the two hook commands themselves.
    // `dir` names one of the folders a `folder` step binds, for a file that has to be lying in *that*
    // folder rather than in the one the run stands in — what a folder traces is read off its own
    // contents, so a world where a bound folder already carries a provider's settings is only
    // reachable by writing inside it. Left out, the file lands in the run's own folder.
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "write-file", required: &["path", "content"], refs: &[], strings: &["path", "content", "dir"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "copy-fixture", required: &["from", "path"], refs: &[], strings: &["from", "path"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "git-init", required: &[], refs: &[], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "hooks-install", required: &[], refs: &[], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "hooks-uninstall", required: &[], refs: &[], strings: &[], binds: false },
    // The paste that starts this folder's AI on amenbo at every session, put where the build says it
    // goes. amenbo hands the text over and never writes that file, so somebody has to do it for the
    // road to carry on — the driver stands in for the hand that pastes, the way `write-file` stands
    // in for a file a person already had. `tool` is the provider, by the name the build's own
    // catalog answers to.
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "wire-ai", required: &["tool"], refs: &[], strings: &["tool"], binds: false },
    // The moves the same road is made of on screen, where the report stands on the project's own board and
    // carries every button there is. The copy is the handing over itself — it puts the text on the
    // clipboard, which no screenshot reads, so the road ends at the press and what the text says is held
    // from the other end (`ai-launch-text`).
    //
    // `ai-launch-consent` is the refusal, and the screen takes no other answer: nothing here asks, so
    // there is no yes to give, and the one button that writes anything is the one that ends the report.
    // It is a move and not a note beside the file, because the record it leaves is only evidence of
    // anything if the run is what wrote it. The answer travels as a value rather than being folded into
    // the op's name, so a road reads as an answer given rather than as a button pressed.
    //
    // Choosing which tool the text is for, where more than one is on offer. It is the move that says the
    // offer is a catalog and not a single line: a folder that points at no provider is handed every one
    // amenbo knows, and the only way to show they are all reachable is to reach one the folder shows no
    // trace of and read the text change to it (`ai-launch-notice` on that tool's own file).
    //
    // And dropping the answer, which is a move of its own rather than a third answer: it puts the project
    // back to never having been asked, so the report comes back. A refusal is silent from then on, and
    // this is the only way out of one.
    //
    // A screen road alone: a terminal asks inline and prints the text where it stands, so there is
    // nothing there to answer, to choose between, or to press — and the answer it writes it never reads
    // back, so it has no face to clear it from either.
    //
    // And putting the report aside, which is the move that answers nothing: it takes the report off the
    // screen in front of the reader and writes no record, so what proves it apart from the refusal is
    // the report standing again when the project is next opened. It is a road of its own for that
    // reason — two buttons side by side, one of which is for good and one of which is not, is exactly
    // where a build can swap them and nothing else notice.
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "ai-launch-consent", required: &["answer"], refs: &[], strings: &["answer"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "ai-launch-close", required: &[], refs: &[], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "ai-launch-pick", required: &["tool"], refs: &[], strings: &["tool"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "ai-launch-copy", required: &["tool"], refs: &[], strings: &["tool"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "ai-launch-consent-clear", required: &[], refs: &[], strings: &[], binds: false },
    // A plugin's life on this machine. `install` fetches it from the catalog and `enable` opens its
    // gate — two separate acts on purpose, since an installed plugin that never fires is the normal
    // state. `run` calls the command face: `command` is the word the plugin's own face takes, `task`
    // hands it the id of a task an earlier step created, and `args` carries anything else verbatim.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "install", required: &["name"], refs: &[], strings: &["name"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "enable", required: &["name"], refs: &[], strings: &["name"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "disable", required: &["name"], refs: &[], strings: &["name"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "uninstall", required: &["name"], refs: &[], strings: &["name"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "run", required: &["name", "command"], refs: &["task"], strings: &["name", "command"], binds: false },
    // Push what is waiting on the queues through, here and now. Delivery otherwise rides along with
    // whatever was being done, so this is the door for a backlog that has stopped moving — and, like
    // `plugin run`, what it reports is read by an assert that has to follow it (`flushed`).
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "flush", required: &[], refs: &[], strings: &[], binds: false },
    // Moving an installed plugin onto the build the catalog publishes, and back off it again. `update`
    // re-walks the install door over the new asset and retains the build it replaced; `rollback` puts
    // that retained pair back, and consumes it, so a second one has nothing to return to.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "update", required: &["name"], refs: &[], strings: &["name"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "rollback", required: &["name"], refs: &[], strings: &["name"], binds: false },
    // An installed plugin left recording a build the catalog has moved past. What amenbo calls an update
    // is the installed manifest's checksum differing from the catalog's, and a scenario cannot reach that
    // state by using amenbo: the catalog publishes one build, and the trust model means no other one can
    // be signed into existence to install first. So the driver writes the disagreement, and the real
    // catalog is the build that is moved to — the same idea as `folder legacy-pointer`.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "stale-manifest", required: &["name"], refs: &[], strings: &["name"], binds: false },
    // An installed plugin declaring the plainest setting there is: a line the reader types, kept in the
    // ordinary store and read back as it was written. The same reason the two below it exist reaches this
    // one first — **no plugin in the official catalog declares any setting at all**, so a road that fills
    // one in has nothing to fill in until the driver writes the declaration. It is the shape most of what
    // `plugin config` does is about, and the only one the two below cannot stand in for: a secret is never
    // read back, and a choice answers with candidates rather than with what was typed.
    //
    // `required: true` writes the flag that says the plugin cannot work without an answer, which is what
    // an enable at a crossing holding no value for it is refused over. It is a word on this declaration
    // rather than an op of its own: the field written is the same field, and what the flag changes is
    // what amenbo then does about an empty one.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "declare-setting", required: &["name", "key"], refs: &[], strings: &["name", "key", "label"], binds: false },
    // An installed plugin declaring a setting its author marked secret. Which settings a plugin takes
    // is the author's word and amenbo never invents one, so the only honest way to reach this state is
    // for a plugin that declares one to be published — and no plugin in the official catalog does. The
    // secret route (off the store, off every backup, injected as an environment variable) is the half
    // of `plugin config` that fails silently and in plain text, so it is not left unwalked until one
    // is: the driver writes the declaration onto the installed manifest, the way `stale-manifest`
    // writes the disagreement it needs. Everything after it is amenbo's own doing.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "declare-secret", required: &["name", "key"], refs: &[], strings: &["name", "key", "label"], binds: false },
    // An installed plugin declaring a setting whose answers its author listed, and the one that stands
    // while nobody has answered. Same reason as `declare-secret`: which settings a plugin takes is the
    // author's word, and no plugin in the official catalog offers candidates — so the half of
    // `plugin config` that keeps three answers apart (a choice made, none of them chosen, nobody asked
    // yet) would go unwalked until one does. `options` is the candidates as their stored values, joined
    // by commas the way an answer is; `default` is a subset of them, and leaving it out is the other
    // shape a choice comes in.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "declare-choice", required: &["name", "key", "options"], refs: &[], strings: &["name", "key", "label", "options", "default"], binds: false },
    // An installed plugin saying, in its author's words, when to reach for it and what to type. What a
    // plugin says for itself is written in its manifest and amenbo invents none of it, so this is the
    // author's block arriving the only way it can — written onto the installed manifest, the way
    // `declare-secret` writes a declaration no published plugin carries. Which is also why the scenario
    // does not read the catalog's own wording back: an author may reword their block any day, and a line
    // asserting today's sentence would go red on a change amenbo had no part in. `when` is the occasion;
    // `cmd` and `does` are one call, which is enough to see the calling form amenbo puts in front of it.
    // `steps` is where that call says it is a tool — the ids of amenbo's own steps, comma-separated, the
    // way an author writes them. It is the author's word too, and no published plugin writes one yet, so
    // the road to a step carrying a tool is only walkable once a block here declares it.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "declare-agent", required: &["name", "when"], refs: &[], strings: &["name", "when", "cmd", "does", "steps"], binds: false },
    // An installed plugin declaring the layer it lives at — one project's rows, or the device's.
    // Same reason as the declarations above it: the layer is the author's word, a manifest
    // saying nothing means `project`, and **every plugin the official catalog serves says nothing** — so
    // the device layer is a state no install reaches, and the road a machine-wide plugin walks is only
    // walkable once this writes the declaration onto the installed manifest. Everything after it is
    // amenbo's own: which rows the enable opens, and how wide a window the run is handed.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "declare-scope", required: &["name", "scope"], refs: &[], strings: &["name", "scope"], binds: false },
    // An installed plugin that is nobody's but its author's. The badge is the catalog's to grant and no
    // author can write it onto themselves, which is what makes it the one thing amenbo can safely split
    // a stranger from a colleague by — and it is also why a road cannot reach a stranger by installing
    // one: every plugin the official catalog serves comes back badged. So the badge is taken off the
    // installed manifest here, the way `declare-agent` writes the block onto it, and what follows is the
    // state a user reaches the moment they install from anywhere else.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "unbadge", required: &["name"], refs: &[], strings: &["name"], binds: false },
    // An installed plugin whose program answers with the secrets it was handed. A secret travels to a
    // run as an environment variable on the child process — off argv, off the log, out of the store —
    // so the only place it can be seen arriving is inside the run, and only a plugin willing to say
    // what it was given can say it. None of the published ones is (they use their settings, they do
    // not report them), so the driver stands one in that prints its injected config and nothing else.
    // What it reads back is amenbo's own doing: which value, at which tier, and whether there is one
    // left at all.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "echo-program", required: &["name"], refs: &[], strings: &["name"], binds: false },
    // An installed plugin whose program calls amenbo back. A payload names a record and carries none of
    // it, so the route to the content is the binary itself, run from inside the plugin with the store and
    // the window amenbo put in its environment — and no plugin in the official catalog takes it (the one
    // published there works out everything it does from the repository it is called in). So the only
    // witness that the environment really arrives, that a call made through it needs no facet, and that
    // the window is what bounds it, is a plugin that makes the call: the driver stands one in, the way
    // `echo-program` stands in the only witness an injected secret has. Its faces are `read` and `write`,
    // each taking the id of a task an earlier step bound and handing everything under `args` to amenbo
    // verbatim — so the call under test is written in the scenario rather than buried in the driver.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "read-back-program", required: &["name"], refs: &[], strings: &["name"], binds: false },
    // An installed plugin that takes `seconds` to answer. A queue only holds rows while its plugin is
    // still on one — the runner takes the row off the moment the plugin replies, whichever end it
    // reached — so a backlog is not a state a scenario can arrive at by using amenbo: it would be
    // racing the runner it just started. Every plugin the catalog publishes answers in the time a
    // process takes to start, and slowness is exactly what the backlog display exists to diagnose, so
    // the driver leaves one answering slowly, the way `declare-secret` writes a declaration no
    // published plugin carries. `seconds` is the window the asserts after it have to read in.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "slow-program", required: &["name", "seconds"], refs: &[], strings: &["name"], binds: false },
    // Whether amenbo can read what is installed at all — the one way to leave a write's delivery
    // standing. Delivery rides along with the write that caused it, so anything a scenario writes is
    // carried out before the next step: a push by hand has something to carry only where that drive
    // never happened. amenbo skips it when the installed plugins will not read, since it will not walk
    // its cursor past events a subscriber list it could not resolve was never offered — so the event
    // stays where the write appended it, queued to nobody, with no runner started. `readable` is both
    // halves: `false` leaves the next write undelivered, `true` gives the directory back, which
    // whatever reads or delivers afterwards needs.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "installed-dir", required: &["readable"], refs: &[], strings: &[], binds: false },
    // What an installed plugin is told, for the project the run stands in. `key` is a setting its
    // author declared; an empty value is how one is taken back, which is why it is a value here and
    // not an op of its own.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "config-set", required: &["name", "key", "value"], refs: &[], strings: &["name", "key", "value"], binds: false },
    // A catalog of the run's own, answering on the loopback for as long as the scenario lasts.
    // Registering one is a trust decision taken on the key it publishes beside its `catalog.json`,
    // and a key is only published by something that answers on a port — no URL a scenario can write
    // down serves one, so the run stands the catalog it is about to trust.
    // `publishes_key` is the trust half: a catalog that publishes none is the other side of the rule,
    // browsable and uninstallable. `offers` is the shelf — the rows this catalog's own document
    // carries, each written as the words that document holds (`name`, `desc`, the `claims_official`
    // badge it is not entitled to, and the one `setting` its author declares, under the `label` a
    // form shows). Naming none is an empty shelf, which is what a road about the trust root alone
    // wants. It is the only arg written as a list of rows, and the loader checks it as one.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "catalog-stand", required: &["publishes_key"], refs: &[], strings: &[], binds: true },
    // The same catalog, publishing a different key than the one pinned on it — a publisher rotating
    // their key, which is the event the pin exists to meet.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "catalog-rotate-key", required: &["target"], refs: &["target"], strings: &[], binds: false },
    // The catalogs a browse reads. A third-party one is named by the URL of its `catalog.json`, and
    // that URL is the handle for taking it back off again — there is nothing else to name it by.
    // A catalog the run stood up has no URL to write down (its port is handed out at run time), so
    // it is named by the `as:` binding instead: one of `url` / `target`, which the driver settles
    // since neither alone can be required here.
    // `name` is what the shelf is called on screen, and what a row coming off it is badged with. A
    // registration that gives none is called after the host of its URL — which for a catalog the run
    // stood up is an address with a port picked this run, and so is nothing a road can read back.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "catalog-add", required: &[], refs: &["target"], strings: &["url", "name"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "catalog-remove", required: &[], refs: &["target"], strings: &["url"], binds: false },
    // Opening one row of the browsing view — the move between the list and the panel under it, which
    // is a move only a screen has. It names the shelf as well as the plugin because a name is a
    // catalog's to give and two of them may serve one: which row is opened is the whole question the
    // panel after it answers.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "open-entry", required: &["name", "source"], refs: &[], strings: &["name", "source"], binds: false },
    // The switch as a screen draws it: one project at a time, each one named. `enable-in` picks a
    // project from those offered beside the row — and picking one *is* the enable, since turning a
    // plugin on is itself the permission to run its code, so there is no second question under the
    // picker. `disable-in` shuts the gate for one of the projects the row names, leaving whatever else
    // it names still firing.
    //
    // A screen road alone, and not by omission: a plugin has one switch, and a terminal says which
    // project it is moving by standing in a folder bound to that project. There is no flag for another
    // one — so `enable` / `disable` are the terminal's road to this same act, and naming the project in
    // the step is the screen's.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "enable-in", required: &["name", "project"], refs: &[], strings: &["name", "project"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "disable-in", required: &["name", "project"], refs: &[], strings: &["name", "project"], binds: false },
    // Drawing the crossing and nothing else: the picker beside the rows puts one there, and leaves the
    // switch in it where it was. It is a step of its own rather than the first half of `enable-in`
    // because what it leaves behind is a state worth reading — a row standing with the plugin still off
    // — and a road that only ever draws a row on its way to turning one on has nowhere to read it.
    //
    // A screen road alone, like the switch it stands next to: a terminal has no picker, and nothing to
    // draw a row on.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "draw-crossing", required: &["name", "project"], refs: &[], strings: &["name", "project"], binds: false },
    // Opening the settings a crossing holds, inside that crossing's own row. It is the move a refusal
    // leaves a person needing: an enable turned away for want of a value is turned away about one
    // crossing, and the row that said so is where the value goes in. Which project the form writes for is
    // therefore never asked — the row has answered it — and a step that names the crossing is naming the
    // row, not a second picker.
    //
    // A screen road alone: a terminal writes the value with a command that names the setting, so there is
    // no form to open and nowhere for one to be opened inside.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "open-config-in-row", required: &["name", "project"], refs: &[], strings: &["name", "project"], binds: false },
    // A setting offering candidates, as the form answers it: a box per candidate, and a button under
    // the field. `config-choose` leaves the named ones ticked and every other one clear;
    // `config-choose-none` clears them all, which is the answer that is not the same as never having
    // been asked; `config-restore-default` presses the button, which is the door back to what the
    // author put behind the field.
    //
    // A screen road alone, for the reason `enable-in` is: a terminal answers this setting by writing
    // the value down (`config-set`), and a form has boxes and a button where the value would be — the
    // three answers are one string apiece to type, and three different moves to make.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "config-choose", required: &["name", "key", "options"], refs: &[], strings: &["name", "key", "options"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "config-choose-none", required: &["name", "key"], refs: &[], strings: &["name", "key"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "config-restore-default", required: &["name", "key"], refs: &[], strings: &["name", "key"], binds: false },
    // Asserts
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "listed", required: &["filter"], refs: &["target"], strings: &["filter", "position"], binds: false },
    // Where a word is written. Separate from `listed` because the question is a different one: a
    // listing answers which records match, and this answers which *places* carry the word — so the
    // step names the face it expects to be found on, which a listing has no way to say. The side is
    // the domain's, since a hit says whose it is and a bound id alone does not.
    //
    // The face appears twice under two names because the step asks two different things of it:
    // `face` is what the answer is read against (the hit landed *there*), `only_face` is the
    // narrowing put to the search on the way in. One key could not be both — a step that narrows to
    // a face and then reads the face back would be asserting the narrowing against itself.
    //
    // `standing` is the other thing a row says: where the record it points at stands, which is what
    // separates a place in work still to be taken from a place in work that is over.
    //
    // `landed_on` and `marked` are the two things only a screen says, so they belong on a `steps_gui`
    // road and the CLI driver turns them away rather than passing over them. `landed_on` is what the
    // row calls the place — a task, a decision, or a remark on either — which is the one reading that
    // tells an attachment on a record from an attachment on a remark, the pair a face alone leaves
    // together. `marked` is the run of characters the excerpt has to show marked, which does not
    // survive a pipe at all: a highlight is a pair of offsets in `--json` and paint on the screen.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "found", required: &["words", "target"], refs: &["target", "project"], strings: &["words", "face", "only_face", "kind", "filter", "standing", "landed_on", "marked"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Decision, op: "found", required: &["words", "target"], refs: &["target", "project"], strings: &["words", "face", "only_face", "kind", "filter", "standing", "landed_on", "marked"], binds: false },
    // The box `filter` is written into, asked whether it can be used at all. The grammar it is read in
    // is the side's own, so with no side chosen there is none for the words to be read in and the box
    // takes nothing — which is a state only a screen has. A flag either arrives on a command line or it
    // does not, so a terminal holds nothing that stands where it always stands and refuses the hand;
    // what a screen holds instead is a control a reader can see, reach and not use.
    //
    // It takes no argument, and neither the side nor a `present` is one of them. Naming a side would be
    // asking about a screen that has already moved past this state, and the other half — the same box
    // taking typing once a side is named — is not a second reading of the box but every `found` below it
    // that carries a `filter`.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "narrowing-shut", required: &[], refs: &[], strings: &[], binds: false },
    // What the typed words left standing. Separate from `listed` because there is no filter to write it
    // as: the narrowing is the screen's own, and the question it answers is which of the cards drawn a
    // moment ago are drawn still. The words belong to the `narrow` that put them in — repeating them
    // here would be the one place the two could disagree.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "narrowed", required: &["target"], refs: &["target"], strings: &[], binds: false },
    // Whose record the press opened. The title is no witness — the hit row carries it too — so the step
    // names a phrase only the record's own face holds, and `present: false` puts the same question to a
    // record that was not the one pressed.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "opened", required: &["target", "shows"], refs: &["target"], strings: &["shows"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "field", required: &["target", "field", "equals"], refs: &["target"], strings: &["field"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Decision, op: "field", required: &["target", "field", "equals"], refs: &["target"], strings: &["field"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Decision, op: "listed", required: &["filter"], refs: &["target"], strings: &["filter", "position"], binds: false },
    // Whether one decision points at another, named by the side the edge is read from (`supersedes`
    // / `superseded_by` / `builds_on` / `built_on_by` / `amends` / `amended_by`). A `field` path can
    // say an edge is *there*; only this can say it is gone, which is the whole of `unlink`.
    OpSpec { kind: Kind::Assert, domain: Domain::Decision, op: "edge", required: &["target", "kind", "other"], refs: &["target", "other"], strings: &["kind"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "commit", required: &["target", "sha"], refs: &["target"], strings: &["sha"], binds: false },
    // What a timeline holds: the comments on an object, and the shared stream a task's own events
    // land in. `text` is what to look for; `present: false` asks that it is gone.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "commented", required: &["target", "text"], refs: &["target"], strings: &["text"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Decision, op: "commented", required: &["target", "text"], refs: &["target"], strings: &["text"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "activity", required: &["target"], refs: &["target"], strings: &["text", "kind"], binds: false },
    // What a `store` action left behind: the archive on disk, and whether an export carries the row
    // for an object an earlier step made. `from` names the export the same way `target` names the
    // object, so both sides are checked back to a binding. `absent` asks the archive's bytes for a
    // word that must not be in them — the one question about a file amenbo hands out that needs no
    // reading of its layout, and the only way to say a secret really stayed out of it.
    OpSpec { kind: Kind::Assert, domain: Domain::Store, op: "snapshot", required: &["target"], refs: &["target"], strings: &["absent", "contains"], binds: false },
    // What the number a carrier asks for did between two steps. `since` names the version an earlier
    // `sync-version` bound, and `moved` is the whole question: a write inside the window moves it, and
    // anything else leaves it alone. It is asked as *moved or not* rather than as a value, because the
    // number itself means nothing outside the store that issued it — only that it is another one does.
    OpSpec { kind: Kind::Assert, domain: Domain::Store, op: "version", required: &["since", "moved"], refs: &["since"], strings: &[], binds: false },
    // Whether an object is in the snapshot a carrier was handed — `exported`'s question, put to the
    // other document. `from` names what a `sync-snapshot` bound, and `present: false` asks the half the
    // window exists for: that what lies outside it did **not** travel.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "synced", required: &["target", "from"], refs: &["target", "from"], strings: &[], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "exported", required: &["target", "from"], refs: &["target", "from"], strings: &[], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Decision, op: "exported", required: &["target", "from"], refs: &["target", "from"], strings: &[], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Comment, op: "exported", required: &["target", "from"], refs: &["target", "from"], strings: &[], binds: false },
    // The two integrity reads, each by the command a user reaches for. `ok` is the verdict asked
    // of it; `validate` narrows to one object when a `target` is given. `doctor` also takes an
    // `issue` — the kind of problem to look for in what it listed — since most of what it raises is
    // a warning, and a warning leaves the verdict alone: without naming the kind there is no way to
    // say a problem appeared, or that a repair took it away.
    OpSpec { kind: Kind::Assert, domain: Domain::Store, op: "doctor", required: &["ok"], refs: &[], strings: &["issue"], binds: false },
    // How many blob files the store is holding. The sweep that reclaims them raises no issue and
    // reports nothing a machine reads, so what says it ran is the count going down.
    OpSpec { kind: Kind::Assert, domain: Domain::Store, op: "blobs", required: &["count"], refs: &[], strings: &[], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Store, op: "validate", required: &["ok"], refs: &["target"], strings: &[], binds: false },
    // Whether the nudge named is standing on screen. A screen road alone — a nudge is put on a
    // surface, and a terminal has none to put it on. `shows` is the sentence the question is put in,
    // and it is what the reading is matched against: the id is the name the build declares the nudge
    // under, it never reaches a screen, and a line naming only that could not be judged from a shot.
    OpSpec { kind: Kind::Assert, domain: Domain::Store, op: "nudge", required: &["nudge", "present", "shows"], refs: &[], strings: &["nudge", "shows"], binds: false },
    // An attachment read back three ways: its own row, the owner's list it hangs in, and — for a
    // blob — the bytes coming out again, which is the only proof the ingest kept them.
    OpSpec { kind: Kind::Assert, domain: Domain::Attachment, op: "field", required: &["target", "field", "equals"], refs: &["target"], strings: &["field"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Attachment, op: "listed", required: &["target", "owner", "owner_kind"], refs: &["target", "owner"], strings: &["owner_kind", "position"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Attachment, op: "saved", required: &["target", "content"], refs: &["target"], strings: &["content"], binds: false },
    // The repository-side gates: what the lint found in a file, and what is in a hook slot.
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "lint", required: &["path", "hits"], refs: &[], strings: &["path"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "hooks", required: &["hook", "state"], refs: &[], strings: &["hook", "state"], binds: false },
    // Whether anything in this folder starts its AI on amenbo at session start (`wired`), and — while
    // nothing does — which provider the folder is told about by name (`tool`). The two are one
    // question asked from either end: the answer amenbo carries on every response until the paste
    // lands, and the silence that follows it.
    //
    // `wired` is the whole vocabulary here. A folder is wired or it is not: whether the hook then
    // fires, and whether what it injects reaches the model, is outside amenbo, so nothing here says
    // enabled and nothing says it works.
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "ai-launch", required: &["wired"], refs: &[], strings: &["tool"], binds: false },
    // The text handed over to make that happen: what it carries (`carries` — the launch instruction,
    // which is the one part of it that is not the provider's own shape) and the file it says to put
    // it in (`paste_into`). What is under test is the handing over, since amenbo's whole part in this
    // is the text: a snippet that named the wrong file, or that injected something other than the
    // launch instruction, would leave a reader pasting in good faith and no better off.
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "ai-launch-text", required: &["tool", "carries"], refs: &[], strings: &["tool", "carries", "paste_into"], binds: false },
    // What the screen says about the same wiring: the report standing on the project's own board, which
    // names the provider it traces (`tool`) and the file the text goes into (`paste_into`). The file is
    // the half that carries the reading, since it is the one thing on the road that appears nowhere else
    // on that screen — and that the report outlives an answer is the line the screen road exists for:
    // consent is not wiring, and only the paste landing ends the telling.
    //
    // `present: false` asks for the board without it, which is how a road says the report went: the file
    // is the one word that would be there, so its absence from the shot is the report's. It reads a
    // board and never a blank — a road that shot some other screen would pass it — so it is written
    // between a step that put the report there and one that brings it back.
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "ai-launch-notice", required: &["tool", "paste_into"], refs: &[], strings: &["tool", "paste_into"], binds: false },
    // The record, read where the project keeps its own face. `answer` is `yes`, `no` or `unanswered` —
    // three states and not a truth value, because unanswered is what a project starts in: a screen that
    // could only say yes or no would report a refusal from a project nobody has answered for, and the way
    // out of a refusal would be offered where there is nothing to take back. The yes reaches the record
    // from the terminal, which is where the question is still put.
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "ai-launch-answer", required: &["answer"], refs: &[], strings: &["answer"], binds: false },
    // The other half of that face: the button the answer is taken back with, read where there is nothing
    // left to take back. `ai-launch-answer: unanswered` says the record is gone and stops there, so a
    // build that left the way out standing open beside it passes that read — and offers a press that
    // undoes nothing.
    //
    // What it asks about is the drawing, not the wiring. A button shut in the markup is turned away
    // whatever it looks like, so the miss this exists to catch is the one only a hand meets: shut, and
    // drawn pixel for pixel like a button that can be pressed. Naming the attribute would close it on the
    // half that was already right.
    //
    // Hence a `Review`, twice over: a control refusing the hand leaves no text on a shot either way, and
    // what separates a shut one from a live one is paint. A screen road alone, like the press it reads —
    // a terminal writes the answer and never reads it back, so it has no face to clear it from, and none
    // to draw a way out shut on.
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "ai-launch-consent-clear-shut", required: &[], refs: &[], strings: &[], binds: false },
    // A folder named among the ones that one text is still waiting on. Consent is answered for a project
    // and the paste lands in a folder, so what a reader who answered yes still owes is a list — and the
    // report puts its text up once with that list under it rather than repeating the request per folder.
    // The two halves need an op apiece: `ai-launch-notice` reads the text and the file it goes into,
    // this reads a folder standing under it, and a road that names more than one folder writes one step
    // each. `dir` is a plain name, the way `folder bind` takes one: where a folder sits is the run's to
    // decide, and what the scenario writes down is what it called the one it placed.
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "ai-launch-folder", required: &["tool", "dir"], refs: &[], strings: &["tool", "dir"], binds: false },
    // The same folder read where it is listed whatever the board is carrying: the project's own settings.
    // A board holds one standing notice, so the report is only on it when nothing with a missing premise
    // is standing ahead of it — and the folders behind it go on waiting either way. This is the place
    // they are counted, which is what makes the board's single notice a choice of where to act rather
    // than a list quietly cut short.
    //
    // It names no tool, because the inventory does not: a folder that names no tool it uses waits on
    // every one in the catalog, so grouping the list by tool would put the same path up five times and
    // read as five folders left. What is outstanding is a folder.
    //
    // A `Review`, and not by omission: the same path is listed a second time on that screen, among the
    // folders bound to the project. A reading answers which words are on a shot and never which part of
    // it they came from, so one taken here would pass over a build that dropped the inventory entirely.
    // The shot is what an eye closes it by.
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "ai-launch-waiting", required: &["dir"], refs: &[], strings: &["dir"], binds: false },
    // A project as it is read back: one row's fields, and whether it is in the listing at all (and
    // where). `archived: true` asks the listing that carries the archived ones.
    OpSpec { kind: Kind::Assert, domain: Domain::Project, op: "field", required: &["target", "field", "equals"], refs: &["target"], strings: &["field"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Project, op: "listed", required: &["target"], refs: &["target"], strings: &["position"], binds: false },
    // The same crossing `plugin fires-in` reads, read from the other face: there a plugin's rows are
    // its projects, here a project's rows are its plugins.
    //
    // `state` and not a yes/no, for the reason `plugin config`'s is: this face has three states and a
    // truth value has two. A row that is not there at all and a row standing with the plugin off are
    // different screens — and telling them apart is the whole point, since the picker here draws the
    // row rather than turning anything on, so a person who pressed it and reads "not on" has to be able
    // to see that something did happen. `absent` is no row, `drawn` a row with the plugin off in it,
    // `firing` a row saying the plugin is on.
    //
    // A screen road alone, and a `Review` like `fires-in`: whether the plugin is on here is drawn as a
    // button, and a button's label is a word of the interface — so what separates `drawn` from `firing`
    // is not something the presence of text on a shot can settle.
    OpSpec { kind: Kind::Assert, domain: Domain::Project, op: "plugin-row", required: &["project", "plugin", "state"], refs: &[], strings: &["project", "plugin", "state"], binds: false },
    // An axis as it is read back, by name: is it defined, and does it carry the value named?
    OpSpec { kind: Kind::Assert, domain: Domain::Dimension, op: "listed", required: &["dimension"], refs: &[], strings: &["dimension", "value"], binds: false },
    // Which bucket of the "what to do now" view a task lands in (`overdue` / `due_today` /
    // `in_progress`) — the view is assembled from days, so the bucket is not the task's status field.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "status-bucket", required: &["target", "bucket"], refs: &["target"], strings: &["bucket"], binds: false },
    // The three faces the store shows of itself, each a dotted path into what that read prints:
    // `config` its settings, `identity` the name and the hardware it was raised on, `update` what a
    // check for a newer build comes back with.
    OpSpec { kind: Kind::Assert, domain: Domain::Store, op: "config", required: &["field", "equals"], refs: &[], strings: &["field"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Store, op: "identity", required: &["field", "equals"], refs: &[], strings: &["field"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Store, op: "update", required: &["field", "equals"], refs: &[], strings: &["field"], binds: false },
    // Whether a folder is bound, asked from inside it — and, with `project`, which one it names.
    // `resynced` asks the other half: whether the guidance block there is at this build's version,
    // which is answered by a resync finding nothing left to write.
    OpSpec { kind: Kind::Assert, domain: Domain::Folder, op: "bound", required: &["dir"], refs: &["project"], strings: &["dir"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Folder, op: "resynced", required: &["dir"], refs: &[], strings: &["dir"], binds: false },
    // How many folders the project the last `unbind` was taken from has left. Taking the last one off
    // goes through like any other — re-homing folders means taking them all off before putting them
    // back, so a count that refused would force an order rather than protect anything — and what
    // stands in for the refusal is the answer saying what is left. `left: 0` is that line under test.
    //
    // It has to **follow its unbind**, the way `plugin returned` follows its call: the count is part
    // of what that command answered, and afterwards there is only the state it left, which reads the
    // same whether the answer said anything about it or not. The reading is taken on the number and
    // not on the sentence beside it: the same fact is published both ways on purpose, so that a
    // machine has something to read without parsing a line written in the reader's own language.
    OpSpec { kind: Kind::Assert, domain: Domain::Folder, op: "folders-left", required: &["left"], refs: &[], strings: &[], binds: false },
    // The warning a project with no folder linked carries on its own board, and the one move that ends
    // it standing beside it. How much work the board holds is not part of the question: a project
    // carrying forty cards and no folder is exactly the one nothing else on the screen speaks about, so
    // the warning stands above those cards rather than in place of them.
    //
    // `absent` is judged the way `ways-in` is, the other way round. What it names is the command the
    // board's other two standing notices both hand over — the first loop's request, and the wiring text
    // — so a reading that comes back without it is what says this notice is standing alone. The
    // warning's own words are the interface's, in whatever language the app is in, and there is nothing
    // in them a reading could be held to.
    //
    // GUI only, and not by omission: which notice a board carries leaves behind exactly the rows the
    // others do. There is no read to ask, and the screen is the only witness.
    OpSpec { kind: Kind::Assert, domain: Domain::Folder, op: "none-linked", required: &["absent"], refs: &[], strings: &["absent"], binds: false },
    // What a project with no work in it yet hands its reader: the loop that joins the two ends — the
    // reader asks their AI, the AI writes to amenbo, and what it wrote lands on the board. Every move
    // the interface can make on their behalf it makes, so what the screen carries is a terminal
    // already inside the linked folder and a request finished enough to paste.
    //
    // `hands_over` is the words that request has to carry, and what they name is the command the AI is
    // sent to run before it does anything else. It is the one part of the card that is the same in
    // whatever language the app is in, so it is the part a reading can be held to.
    //
    // GUI only, and not by omission: none of this reaches the store. A request that lost the command,
    // and a terminal opened somewhere other than the linked folder, leave exactly the rows behind
    // that a working one does — so there is no read to ask, and the screen is the only witness.
    OpSpec { kind: Kind::Assert, domain: Domain::Folder, op: "first-loop", required: &["hands_over"], refs: &[], strings: &["hands_over"], binds: false },
    // Where that loop sits among everything else the same screen offers, written as one line naming
    // the order. A reading says which words are on a shot, never which of them came first, so this is
    // a `Review` — and is written down for exactly that reason: an arrangement is what a build
    // reorders without a single assert going red.
    OpSpec { kind: Kind::Assert, domain: Domain::Folder, op: "first-loop-order", required: &["order"], refs: &[], strings: &["order"], binds: false },
    // The ways in a reader is offered before there is a folder to work in: raise a project, or open
    // one this device already holds. Both are the interface's own to carry out, and what says so is
    // that neither hands over anything to type — `absent` names the words a card pointing at a
    // terminal would carry, and they have to be nowhere on the screen.
    //
    // GUI only, and not by omission: a folder linked from a terminal is a folder linked, so a card
    // that hands over the command leaves behind exactly the rows the one that links it does. There
    // is no read to ask, and the screen is the only witness.
    OpSpec { kind: Kind::Assert, domain: Domain::Folder, op: "ways-in", required: &["absent"], refs: &[], strings: &["absent"], binds: false },
    // The second of those ways, once the steps above have opened it and answered it: it asks which
    // project the folder is to be linked to, and the choices are the projects this device holds.
    // `project` names the one the card is left asking for. It is a `Review` rather than a reading:
    // the same name sits in the list of projects down
    // the side of every screen, and a reading says which words are on a shot and not which part of
    // it they came from — so the card is what an eye is shown, and the shot is what it is closed by.
    OpSpec { kind: Kind::Assert, domain: Domain::Folder, op: "open-existing", required: &["project"], refs: &[], strings: &["project"], binds: false },
    // What is on this machine and whose gate is open (`enabled` asks the gate; without it the
    // question is only whether the plugin is there at all), what the last call returned on its own
    // stdout, and what the execution log kept of a run.
    // `desc` asks whether the author's one required sentence is readable here — not what it says. The
    // wording is the author's and they may change it any day, while where it is readable is amenbo's
    // and is the whole of what the split between a colleague's plugin and a stranger's decides.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "listed", required: &["name"], refs: &[], strings: &["name"], binds: false },
    // One project a row names among those the plugin fires in — or, with `present: false`, one it does
    // not. The question is asked a project at a time because that is what a list can be wrong about: a
    // gate read as a single yes/no hides a plugin still firing where nobody is looking, and what has to
    // be true after one project's gate is shut is an answer about the project left alone.
    //
    // A screen road alone, like the moves it reads after. A row names every project wherever it is
    // read, but a terminal can only put one name on it — the switch it moves is the one belonging to
    // the folder it stands in — so the state this is about, a plugin on in more than one project, is
    // reachable on the screen's road and on no other.
    //
    // A `Review` rather than a reading, for the reason `folder open-existing` is: the list of projects
    // runs down the side of every screen, and a reading answers which words are on a shot and never
    // which part of it they came from — so finding the name proves nothing about the row.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "fires-in", required: &["name", "project", "present"], refs: &[], strings: &["name", "project"], binds: false },
    // What the last call handed back. `contains` is the word to find in it; `present: false` puts the
    // same question the other way, which is how a road says a value the window shuts out did not come
    // back in what a plugin was handed.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "returned", required: &["contains"], refs: &[], strings: &["contains"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "ran", required: &["name"], refs: &[], strings: &["name", "outcome"], binds: false },
    // What one plugin's queue owes, and whether anything is working it. The execution log
    // answers for what ran; this answers for what has not — and the two questions have to be asked
    // together, since what never ran wrote no line to read. `count` is how many events are waiting,
    // and `running` whether a runner still holds the lease: the same count with and without one are
    // different diagnoses (a plugin taking its time, against a queue nobody is on).
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "waiting", required: &["name"], refs: &[], strings: &["name"], binds: false },
    // What the flush just before it got through: `delivered` is how many events came off the queues it
    // worked, and `held` names a plugin whose queue it left to the runner already on it. The second is
    // the half a state read cannot answer — a queue still standing looks the same whether the flush
    // stepped around it or was never run at all — which is why the report is read rather than the store.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "flushed", required: &["delivered"], refs: &[], strings: &["held"], binds: false },
    // Whether the catalog holds a different build of an installed plugin — the question `update --check`
    // answers, and the only way to read from outside which build a machine is on (a manifest carries no
    // version number, so there is no number to compare).
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "outdated", required: &["name", "present"], refs: &[], strings: &["name"], binds: false },
    // A setting read back as this project holds it — `equals` for the value, or `set: false` to ask
    // that it holds none. `secret: true` asks the other thing a read of a secret has to be true of:
    // that it says so, and that the value does not come out with it. `state` asks which of the three
    // answers a field holds (`chosen` / `none` / `unanswered`), which is the one question a value
    // cannot answer for itself: a choice answered with none of them and one nobody has answered both
    // read as no value chosen, and only the second follows the author's default. The screen asks that
    // same question of the settings form — which boxes are ticked, and which of the three the field
    // says it is holding — so `state` is what a road there is written on, with `equals` naming the
    // candidates a chosen one leaves ticked.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "config", required: &["name", "key"], refs: &[], strings: &["name", "key", "state"], binds: false },
    // What a crossing's row says about the settings kept there, which is a reading of the row and not of
    // one field: the mark a crossing wears while it owes a value, the form standing open inside the row,
    // and the row saying a value is held. Whether the plugin fires there is the other reading of the same
    // row, and the two are asked apart — an enable refused for want of a value leaves a row that is
    // marked and off, and one word could not say both halves of that.
    //
    // Three screens rather than two, for the reason the row's other reading has three: `required-empty`
    // is the mark, worn before anything is pressed, since a warning that arrives only after the refusal
    // arrives too late; `open` is the settings standing open in that same row, asking for no project,
    // which is the whole of what reaching them from the row means; `filled` is the row saying the value
    // is in, and saying nothing about it that a value standing there has made untrue — the refusal an
    // enable met names what was missing when the switch was pressed, and it does not outlive the filling.
    //
    // A screen road alone, and a `Review` on every state. The marks are words of the interface, and what
    // `open` turns on is a picker that is *not* there — a reading answers which words are on a shot, so
    // neither is something it can settle.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "settings-in", required: &["name", "project", "state"], refs: &[], strings: &["name", "project", "state"], binds: false },
    // A catalog in the browsing view: whether it is a source at all (`present`), whether the browse
    // could reach it, and — `pinned_key` — whether a key of its is what plugins from it would be
    // trusted on. The last is the half that decides installability rather than visibility, and it is
    // asked as a yes/no because the fingerprint itself belongs to whichever key the driver stood the
    // catalog on, and no scenario is written against one driver's key.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "catalog", required: &[], refs: &["target"], strings: &["url"], binds: false },
    // One row of the browsing view: which catalog served the entry (`source`) and whether it wears
    // the official badge (`official`). The two are one question — the badge is the official index's
    // to grant, so an entry a registered catalog serves must read as that shelf's name however
    // loudly its own document claims otherwise, and the merge is what makes that true.
    //
    // There is no CLI here on purpose rather than by omission: `plugin catalog list` answers per
    // catalog, `plugin list` per installed manifest, and an entry's own claim reaches a person only
    // through the market screen. That is why the scenario carrying this is written for the screen.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "browsed", required: &["name", "source", "official"], refs: &[], strings: &["name", "source"], binds: false },
    // One row opened, and what the catalog's own detail document says installing it would mean. A
    // catalog is served as two documents, and the detail is fetched only when someone opens a row —
    // from whichever catalog served that row, which is the reach a merged view has to get right and
    // the list alone never exercises.
    //
    // `declares` is one line of that document the panel prints back: an event the plugin is woken
    // for, or the label of a setting it will ask for. Both are the author's own words, which is what
    // makes them readable — everything else on the panel is either the entry (which the list already
    // held) or a phrase of the interface, and neither says which document was fetched.
    //
    // GUI only, for the same reason `browsed` is: no CLI reads a catalog's detail document. `plugin
    // list` answers per installed manifest, and installing off a registered catalog needs a signed
    // asset — so before an install, the panel is where the declaration is.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "detail", required: &["name", "source", "declares"], refs: &[], strings: &["name", "source", "declares"], binds: false },
    // What an AI is told about this plugin where it reads how to work in this folder — the `plugins` key
    // of the entry point. `present` is whether the plugin is offered there at all, which is the gate's
    // answer and not the install's: an installed plugin nobody switched on is one a call would refuse,
    // and naming it would spend a reader's turn learning what amenbo already knew.
    //
    // `when` is the author's own line, read back to prove it is relayed rather than paraphrased. `cmd` is
    // the author's own command face, and what is checked is what amenbo puts *in front* of it — the
    // calling form is assembled from the name read off disk, so what an AI receives is a line it can
    // type. The command word itself is left out of the step: a build reached by another name hands out
    // lines naming it, and that is the point rather than a mismatch.
    //
    // `because` is for the other half of the key: when nothing is offered, the entry point says which
    // empty-handed state this is, and a reader who cannot tell "nothing installed" from "nothing
    // switched on here" cannot tell which move would fix it. It is matched as a fragment rather than a
    // sentence — what is under test is that the right state is named, not today's wording, which amenbo
    // is free to reword without breaking a promise. Whichever reading a step asks for, the document is
    // held to its own floor first: a reason stands exactly where there is nothing to list.
    //
    // A CLI road alone: the entry point is a document for whoever drives the terminal, and no screen
    // prints it.
    // `absent` is the reading the other three cannot give: naming a field asks what it says, and a field
    // that is not there says nothing to compare. It takes the field names a step expects to find nothing
    // under, comma-separated — what an author wrote and this reader does not get, whether because the
    // author is a stranger or because what they wrote no longer passes the rules.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "at-entry", required: &["name", "present"], refs: &[], strings: &["name", "when", "cmd", "because", "absent"], binds: false },
    // The other half of the same document: a step of amenbo's own working cycle, and whether this
    // plugin's call is hanging on it. The two shelves are kept apart on purpose — a step's body is
    // amenbo's own and a plugin's sentences stay in its entry — so what crosses is the line to type and
    // the id the author named it by, and this is the reading that says the join really happened.
    //
    // `step` is that id (`<run>.<step>`, as the author writes it) and `cmd` the call's own face, since
    // what hangs there is the calling form amenbo builds, not the bare subcommand. `present: false` is
    // the reading with more work to do: a step nobody named, and a ref naming a step this build does not
    // have, both leave a document where nothing hung — which is what says an unknown ref costs a reader
    // one absent line and nothing else.
    //
    // A CLI road alone, like `at-entry` and for the same reason: no screen prints this document.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "at-step", required: &["name", "step", "cmd", "present"], refs: &[], strings: &["name", "step", "cmd"], binds: false },
    // The author's own door, before anything is installed anywhere: a manifest file is held up to the
    // catalog rules. `ok` is the verdict, and `problem` names the code a failing one must report —
    // a manifest can be wrong in more ways than one, and a line about the wrong reason proves nothing.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "validated", required: &["path", "ok"], refs: &[], strings: &["path", "problem"], binds: false },
];

fn lookup(kind: Kind, domain: Domain, op: &str) -> Option<&'static OpSpec> {
    REGISTRY
        .iter()
        .find(|s| s.kind == kind && s.domain == domain && s.op == op)
}

/// The ops a premise may use — the second closed list, cut out of the first. What separates them is
/// not the op's shape but who carries it out: a premise is stood up by a driver before a road
/// begins, and everything on it is a record the store would hold whether or not this scenario ran.
/// A screen's own moves are the other half of the vocabulary — opening a card, answering the
/// question it puts, typing words over a listing — and a premise that stood those up would be
/// carrying out the very steps the road exists to watch.
///
/// It stays short on purpose: an entry here is a claim that a driver can reach this state without
/// the screen, so the list grows one line at a time, beside the driver that learned to seed it.
const PREMISE_OPS: &[(Domain, &str)] = &[
    // Where work is filed, and the work itself in whatever state the screen needs to find it.
    (Domain::Project, "create"),
    (Domain::Task, "create"),
    // With the step that ends the creation, since a world of work already on the board is work
    // somebody finished writing. Left out, every task a premise stands up would be a creation nobody
    // closed — a state a store really can be in, but not the one a road about ordinary work is
    // standing on, and the screen says so on every card.
    (Domain::Task, "finish-creating"),
    (Domain::Task, "assign"),
    (Domain::Task, "status"),
    (Domain::Task, "update"),
    // And the record on the other side, for a road that reads across both. A screen narrowed to one
    // project has to have a decision standing in another to leave out, and which project a decision
    // was filed under is nothing such a road proves — recording one is a road of its own.
    (Domain::Decision, "create"),
    // A device that has been used for a while. It is the one premise no amount of doing reaches: what
    // it stands up is the passage of time itself — launches tallied across days written on — which a
    // road can only be given, never earn.
    (Domain::Store, "worn-in"),
    // A folder already answering for a project — what a screen showing bindings has to be looking at.
    // Taking a pointer back off is here for the state it leaves rather than for the act: a project
    // with no folder left is what one whole notice is about, and creating a project links one, so
    // there is no other way to arrive at it.
    (Domain::Folder, "init"),
    (Domain::Folder, "bind"),
    (Domain::Folder, "unbind"),
    // A file already lying in one of those folders. What a folder traces is read off its contents and
    // recorded nowhere, so a bound folder that already carries a provider's settings — the state every
    // road about wiring an AI starts from — is a world no amount of store seeding reaches.
    (Domain::Repo, "write-file"),
    // A catalog registered and a plugin already on the machine. Both are worlds a screen only reads:
    // the browsing view draws rows a catalog served, and a plugin's row is there once one is
    // installed. Standing a catalog of the run's own comes with them, since a catalog is trusted on
    // the key it serves and there is no other way to have one to register.
    (Domain::Plugin, "catalog-stand"),
    (Domain::Plugin, "catalog-add"),
    (Domain::Plugin, "install"),
    (Domain::Plugin, "enable"),
    // And what an installed plugin says it takes. Which settings a plugin declares is its author's
    // word, no published one declares any, and a screen road about answering them has to find them
    // already declared — the declaration is the world, and the answering is the road.
    (Domain::Plugin, "declare-setting"),
    (Domain::Plugin, "declare-choice"),
];

/// Whether this op may stand a world up (see [`PREMISE_OPS`]).
fn may_stand(domain: Domain, op: &str) -> bool {
    PREMISE_OPS.iter().any(|(d, o)| *d == domain && *o == op)
}

/// What is wrong with an `offers:` value, if anything — the rows a stood catalog publishes.
///
/// Two words are a row's floor: the `name` it is fetched and badged by, and the `desc` a row draws
/// under it. The rest is optional, and each is held to its own shape — a `claims_official` that
/// arrived as the word "true" would be a badge nobody claimed, and a `label` that arrived as a
/// number would reach a form as one.
fn offers_problems(value: &serde_yaml::Value) -> Vec<String> {
    let Some(rows) = value.as_sequence() else {
        return vec!["`offers` must be a list of the entries this catalog serves".to_string()];
    };
    let mut problems = Vec::new();
    for (n, row) in rows.iter().enumerate() {
        let at_row = |m: String| format!("`offers` entry {}: {m}", n + 1);
        if row.as_mapping().is_none() {
            problems.push(at_row("must be a mapping of the fields a catalog entry carries".into()));
            continue;
        }
        for key in ["name", "desc"] {
            match row.get(key) {
                Some(v) if v.as_str().is_some() => {}
                Some(_) => problems.push(at_row(format!("`{key}` must be a string"))),
                None => problems.push(at_row(format!("missing required field `{key}`"))),
            }
        }
        for key in ["setting", "label"] {
            if row.get(key).is_some_and(|v| v.as_str().is_none()) {
                problems.push(at_row(format!("`{key}` must be a string")));
            }
        }
        if row.get("claims_official").is_some_and(|v| v.as_bool().is_none()) {
            problems.push(at_row("`claims_official` must be a boolean".into()));
        }
        // A label with nothing to label is a field that never reaches a form: the key is what a
        // setting is declared under, and naming only its display text declares nothing.
        if row.get("label").is_some() && row.get("setting").is_none() {
            problems.push(at_row("`label` names a `setting`, so one has to be declared".into()));
        }
    }
    problems
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// A failure to turn bytes on disk into a typed [`Scenario`].
#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Parse(serde_yaml::Error),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "read failed: {e}"),
            LoadError::Parse(e) => write!(f, "not a valid scenario: {e}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// Parse a scenario from a YAML string.
pub fn load_str(yaml: &str) -> Result<Scenario, LoadError> {
    serde_yaml::from_str(yaml).map_err(LoadError::Parse)
}

/// Read and parse a scenario file.
pub fn load_file(path: impl AsRef<Path>) -> Result<Scenario, LoadError> {
    let text = std::fs::read_to_string(path).map_err(LoadError::Io)?;
    load_str(&text)
}

// ---------------------------------------------------------------------------
// Semantic validation
// ---------------------------------------------------------------------------

/// A single semantic problem, anchored to the step that carries it (0-based) inside the list it
/// sits in. A step on no driver is one of the premise's, since the world belongs to the scenario
/// rather than to a road; both are `None` for a scenario-wide problem such as an empty id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// Whose road the step is on. Two lists number from one apiece, so a step number alone would
    /// name two places.
    pub driver: Option<Driver>,
    pub step: Option<usize>,
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.driver, self.step) {
            (Some(d), Some(i)) => write!(f, "{} step {}: {}", d.steps_key(), i + 1, self.message),
            // A step on no driver's road is one of the premise's: the world belongs to the scenario
            // rather than to either road, which is what having no driver says.
            (None, Some(i)) => write!(f, "given step {}: {}", i + 1, self.message),
            _ => write!(f, "{}", self.message),
        }
    }
}

impl Scenario {
    /// The road `driver` walks. Empty when this scenario does not give it one.
    pub fn steps(&self, driver: Driver) -> &[Step] {
        match driver {
            Driver::Cli => &self.steps_cli,
            Driver::Gui => &self.steps_gui,
        }
    }

    /// Whether this scenario is written to be run through `driver` — which is to say, whether it
    /// hands that driver any steps.
    pub fn runs_on(&self, driver: Driver) -> bool {
        !self.steps(driver).is_empty()
    }

    /// The drivers that have a road here, as their wire tokens, for a message that has to name them.
    pub fn driver_tokens(&self) -> Vec<&'static str> {
        Driver::ALL.iter().filter(|d| self.runs_on(**d)).map(|d| d.as_str()).collect()
    }

    /// Check the semantic rules the type system cannot: non-empty id/title, at least one driver's
    /// road, a known op for each step, its required args present and of the type it takes, and
    /// every `target:` resolving to an earlier `as:` binding. Returns every problem found, not
    /// just the first.
    ///
    /// Each driver's steps are checked on their own, bindings included: one road cannot name what
    /// the other one made, because the two are never walked together.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errs = Vec::new();
        let whole =
            |m: &str| ValidationError { driver: None, step: None, message: m.to_string() };

        if self.id.trim().is_empty() {
            errs.push(whole("id is empty"));
        }
        if self.title.trim().is_empty() {
            errs.push(whole("title is empty"));
        }
        // A scenario no driver walks is one nothing keeps honest: it rots silently while the set it
        // sits in reports green.
        if Driver::ALL.iter().all(|d| !self.runs_on(*d)) {
            errs.push(whole("scenario has no steps — give at least one of `steps_cli` / `steps_gui` a road"));
        }

        // The premise is walked first, and what it names is in scope on both roads: a card the world
        // was stood up with is the card a road then points at, and it is named the way any earlier
        // step is named.
        let standing = self.validate_list(None, &self.given, &HashSet::new(), &mut errs);
        for driver in Driver::ALL {
            self.validate_list(Some(driver), self.steps(driver), &standing, &mut errs);
        }

        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }

    /// One list of steps, checked end to end — a driver's road, or (with no driver) the premise that
    /// stands before both. `standing` is what the premise already named; the bindings this list
    /// introduces come back, which is how the premise hands its names on.
    ///
    /// Two rules are the premise's alone, and both say the same thing: what stands before a road is
    /// a world, not the walking of it. So it takes actions rather than asserts — nothing is being
    /// proved yet — and only ops that can be reached without the screen ([`PREMISE_OPS`]).
    fn validate_list<'a>(
        &self,
        driver: Option<Driver>,
        steps: &'a [Step],
        standing: &HashSet<&'a str>,
        errs: &mut Vec<ValidationError>,
    ) -> HashSet<&'a str> {
        let at = |i: usize, m: String| ValidationError { driver, step: Some(i), message: m };
        let premise = driver.is_none();

        let mut bound: HashSet<&str> = standing.clone();
        for (i, step) in steps.iter().enumerate() {
            if premise && step.kind() == Kind::Assert {
                errs.push(at(
                    i,
                    "a premise stands a world up rather than proving anything — `given` takes actions alone"
                        .to_string(),
                ));
                continue;
            }
            let spec = match lookup(step.kind(), step.domain(), step.op()) {
                Some(s) => s,
                None => {
                    let verb = match step.kind() {
                        Kind::Action => "action",
                        Kind::Assert => "assert",
                    };
                    errs.push(at(i, format!(
                        "unknown {verb} op `{}` for domain `{:?}`",
                        step.op(),
                        step.domain()
                    )));
                    continue;
                }
            };

            if premise && !may_stand(step.domain(), step.op()) {
                errs.push(at(i, format!(
                    "op `{}` cannot stand a world up — a premise is what a driver arranges before the road, and this is a step on one",
                    step.op()
                )));
                continue;
            }

            for key in spec.required {
                if !step.with().contains_key(*key) {
                    errs.push(at(i, format!("missing required arg `{key}`")));
                }
            }

            // Every reference key this op declares must name a binding introduced by an earlier
            // action's `as:`.
            for key in spec.refs {
                let Some(v) = step.with().get(*key) else { continue };
                match v.as_str() {
                    Some(name) if bound.contains(name) => {}
                    Some(name) => errs.push(at(
                        i,
                        format!("`{key}: {name}` does not resolve to an earlier `as:` binding"),
                    )),
                    None => errs.push(at(i, format!("`{key}` must be a string binding name"))),
                }
            }

            // Every key the op declared as a word has to have arrived as one.
            for key in spec.strings {
                let Some(v) = step.with().get(*key) else { continue };
                if v.as_str().is_none() {
                    errs.push(at(i, format!("`{key}` must be a string")));
                }
            }

            // The yes/no args are booleans wherever they appear: `present` asks whether something is
            // there, `ok` what verdict a check is expected to come back with, `running` whether
            // anything is working a queue, `required` whether a declared setting is one its plugin
            // cannot work without, and the two key questions whether a catalog serves a signing key
            // and whether one of its is pinned.
            for key in ["present", "ok", "running", "required", "publishes_key", "pinned_key"] {
                if let Some(v) = step.with().get(key) {
                    if v.as_bool().is_none() {
                        errs.push(at(i, format!("`{key}` must be a boolean")));
                    }
                }
            }

            // The shelf a stood catalog serves — the one arg written as a list of rows rather than
            // as a word. Its rows are a document's fields, not amenbo's arguments, so the loader
            // reads them here instead of through `strings`: a row is where a typo would otherwise
            // travel all the way to a catalog served with a blank line under a name.
            if let Some(v) = step.with().get("offers") {
                for problem in offers_problems(v) {
                    errs.push(at(i, problem));
                }
            }

            // A step that says its operation will be turned away. It is an action's word — an
            // assert already comes back with a verdict of its own — and what it names is the code
            // the refusal has to carry, so a step written against one guard cannot pass on another
            // guard's refusal.
            if let Some(v) = step.with().get("refused") {
                if premise {
                    errs.push(at(i, "a premise is the world a road starts from, so it cannot be an operation that was turned away — nothing stands up".to_string()));
                } else if step.kind() == Kind::Assert {
                    errs.push(at(i, "`refused` belongs on an action — an assert already carries a verdict".to_string()));
                } else if v.as_str().is_none() {
                    errs.push(at(i, "`refused` must be the error code the operation is expected to be rejected with".to_string()));
                }
                // Nothing came of an operation that was turned away, so there is nothing to name.
                if let Step::Action { bind: Some(name), .. } = step {
                    errs.push(at(i, format!("a refused op produces nothing, so `as: {name}` is not allowed")));
                }
            }

            // Record / guard the binding this step introduces.
            if let Step::Action { bind: Some(name), .. } = step {
                if !spec.binds {
                    errs.push(at(i, format!("op `{}` does not produce a binding, so `as:` is not allowed", step.op())));
                } else if !bound.insert(name.as_str()) {
                    errs.push(at(i, format!("binding `{name}` is already defined")));
                }
            }
        }
        // What this list named, for whoever walks after it. A road's own go nowhere — the two are
        // never walked together — and the premise's are what both roads then stand on.
        bound
    }
}

/// Load and validate in one call — the check a `lint` run performs on each file.
pub fn lint_file(path: impl AsRef<Path>) -> Result<Scenario, Vec<String>> {
    let scenario = load_file(path).map_err(|e| vec![e.to_string()])?;
    match scenario.validate() {
        Ok(()) => Ok(scenario),
        Err(errs) => Err(errs.into_iter().map(|e| e.to_string()).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
id: sample
title: A task assigned to me-ai surfaces in the me-ai listing
steps_cli:
  - type: action
    domain: task
    op: create
    with: { title: SEED }
    as: seed
  - type: action
    domain: task
    op: assign
    with: { target: seed, assignee: me-ai }
  - type: assert
    domain: task
    op: listed
    with: { filter: "assignee:me-ai status:todo", target: seed, present: true }
"#;

    #[test]
    fn good_scenario_loads_and_validates() {
        let s = load_str(GOOD).expect("parses");
        s.validate().expect("valid");
        assert_eq!(s.steps(Driver::Cli).len(), 3);
    }

    #[test]
    fn unknown_key_is_a_parse_error() {
        let yaml = "id: x\ntitle: y\nbogus: 1\nsteps_cli: []\n";
        assert!(load_str(yaml).is_err());
    }

    #[test]
    fn unknown_op_is_rejected() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: action
    domain: task
    op: frobnicate
    with: { title: T }
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("unknown action op")));
    }

    #[test]
    fn dangling_target_is_rejected() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: action
    domain: task
    op: assign
    with: { target: ghost, assignee: me-ai }
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("does not resolve")));
    }

    /// An op that joins two objects is checked on both sides: the second reference is as easy to
    /// mistype as the first, and a driver would only meet it as a binding that was never produced.
    #[test]
    fn a_dangling_second_reference_is_rejected() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: action
    domain: decision
    op: create
    with: { title: D }
    as: rec
  - type: action
    domain: decision
    op: link
    with: { target: rec, task: ghost }
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`task: ghost` does not resolve")));
    }

    #[test]
    fn missing_required_arg_is_rejected() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: action
    domain: task
    op: create
    with: {}
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("missing required arg `title`")));
    }

    /// The case that bit: a SHA of nothing but digits is a number to YAML, and the lint used to
    /// pass it straight through to a run that could only fail on it.
    #[test]
    fn an_arg_that_must_be_a_word_is_rejected_as_a_number() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: action
    domain: task
    op: create
    with: { title: T }
    as: t
  - type: action
    domain: task
    op: commit-add
    with: { target: t, sha: 4011201120112011201120112011201120112011 }
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message == "`sha` must be a string"));
    }

    /// The check is on the value's type, not on the key being there: an op's optional words are
    /// only judged when a step carries them.
    #[test]
    fn an_absent_optional_word_is_not_a_problem() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: action
    domain: task
    op: create
    with: { title: T }
    as: t
  - type: assert
    domain: task
    op: activity
    with: { target: t }
"#;
        load_str(yaml).unwrap().validate().expect("valid");
    }

    #[test]
    fn binding_on_a_non_producing_op_is_rejected() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: action
    domain: task
    op: create
    with: { title: T }
    as: a
  - type: action
    domain: task
    op: assign
    with: { target: a, assignee: me-ai }
    as: b
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("does not produce a binding")));
    }

    /// A scenario that writes only a CLI road is run by the CLI alone. There is nothing else to
    /// read: the steps are the declaration.
    #[test]
    fn a_scenario_with_only_a_cli_road_runs_on_the_cli_alone() {
        let s = load_str(GOOD).expect("parses");
        assert!(s.runs_on(Driver::Cli));
        assert!(!s.runs_on(Driver::Gui), "the screen walks a road, or it is not asked to");
        assert_eq!(s.driver_tokens(), vec!["cli"]);
    }

    #[test]
    fn a_road_for_each_driver_puts_both_on_it() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: action
    domain: task
    op: create
    with: { title: T }
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: T }
"#;
        let s = load_str(yaml).expect("parses");
        s.validate().expect("valid");
        assert!(s.runs_on(Driver::Cli) && s.runs_on(Driver::Gui));
        assert_eq!(s.driver_tokens(), vec!["cli", "gui"]);
    }

    /// A road belongs to the driver that walks it, so a binding does not cross between the two —
    /// the step that would read it is in a run the other list was never part of.
    #[test]
    fn a_binding_does_not_cross_from_one_road_to_the_other() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: action
    domain: task
    op: create
    with: { title: T }
    as: seed
steps_gui:
  - type: action
    domain: task
    op: assign
    with: { target: seed, assignee: me-ai }
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.to_string()
            == "steps_gui step 1: `target: seed` does not resolve to an earlier `as:` binding"));
    }

    #[test]
    fn a_steps_key_outside_the_two_is_a_parse_error() {
        let yaml = format!("steps_tui: []{GOOD}");
        assert!(load_str(&yaml).is_err(), "an unknown driver is a typo, not an extension point");
    }

    #[test]
    fn a_scenario_with_no_road_at_all_is_rejected() {
        let errs = load_str("id: x\ntitle: y\n").unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("no steps")));
    }

    /// The file a `store` action writes is named back through the one binding namespace every
    /// other result uses, so an archive named at the wrong step is caught here and not by a driver
    /// looking for a file nobody wrote.
    #[test]
    fn a_store_action_binds_the_file_it_wrote() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: action
    domain: store
    op: backup
    as: snapshot
  - type: action
    domain: store
    op: restore
    with: { target: snapshot }
  - type: assert
    domain: store
    op: snapshot
    with: { target: snapshot, present: true }
"#;
        load_str(yaml).unwrap().validate().expect("valid");
    }

    #[test]
    fn restoring_an_archive_nobody_wrote_is_rejected() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: action
    domain: store
    op: restore
    with: { target: snapshot }
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("does not resolve")));
    }

    /// `ok` is the verdict a check is expected to report, so a string that merely reads like one
    /// ("true") is a scenario bug rather than something a driver should interpret.
    #[test]
    fn a_non_boolean_verdict_is_rejected() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: assert
    domain: store
    op: doctor
    with: { ok: "yes" }
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`ok` must be a boolean")));
    }

    /// `required` says whether a declared setting is one its plugin cannot work without, and a word that
    /// merely reads like a yes would reach the manifest as text rather than as the flag an enable is
    /// refused over.
    #[test]
    fn a_non_boolean_required_flag_is_rejected() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: action
    domain: plugin
    op: declare-setting
    with: { name: worktree, key: base, required: "yes" }
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`required` must be a boolean")));
    }

    /// The refusal vocabulary: an action may declare that amenbo will turn it away, and the code it
    /// will be turned away with. The op and its args are the ordinary ones — what is under test is
    /// the guard in front of them, not a second spelling of the command.
    #[test]
    fn an_action_may_declare_the_refusal_it_expects() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: action
    domain: task
    op: create
    with: { title: T }
    as: held
  - type: action
    domain: task
    op: status
    with: { target: held, status: in_progress }
  - type: action
    domain: task
    op: status
    with: { target: held, status: in_progress, refused: already_reserved }
"#;
        load_str(yaml).unwrap().validate().expect("valid");
    }

    /// The code is the whole of it: a refusal on some other ground is a different guard, so the
    /// arg has to name one rather than merely saying that something went wrong.
    #[test]
    fn a_refusal_without_a_code_is_rejected() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: action
    domain: task
    op: create
    with: { title: T, refused: true }
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("must be the error code")));
    }

    #[test]
    fn a_refusal_declared_on_an_assert_is_rejected() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: assert
    domain: store
    op: doctor
    with: { ok: true, refused: already_reserved }
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("belongs on an action")));
    }

    /// A binding on a refused op would name something that was never made — and every later step
    /// reading it would be asserting about a task the store does not hold.
    #[test]
    fn binding_a_refused_op_is_rejected() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: action
    domain: task
    op: create
    with: { title: T, refused: out_of_reach }
    as: ghost
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("produces nothing")));
    }

    /// Written out and left empty is the same as never written: a road with nothing on it is not
    /// one, whichever way the file says so.
    #[test]
    fn roads_written_out_but_empty_are_rejected() {
        let s = load_str("id: x\ntitle: y\nsteps_cli: []\nsteps_gui: []\n").unwrap();
        let errs = s.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("no steps")));
    }

    const WITH_PREMISE: &str = r#"
id: x
title: y
given:
  - type: action
    domain: project
    op: create
    with: { name: Greenhouse }
    as: greenhouse
  - type: action
    domain: task
    op: create
    with: { title: SEED }
    as: seed
steps_gui:
  - type: assert
    domain: task
    op: listed
    with: { filter: "status:todo", target: seed, present: true }
"#;

    /// What the premise stands up is named by the road that walks afterwards — that is the whole
    /// point of writing it down rather than leaving it to whoever prepares the screen.
    #[test]
    fn a_road_names_what_the_premise_stood_up() {
        let s = load_str(WITH_PREMISE).expect("parses");
        s.validate().expect("valid");
        assert_eq!(s.given.len(), 2);
    }

    /// The premise's names reach **both** roads, unlike a road's own: the world is the scenario's,
    /// and each road walks from it.
    #[test]
    fn both_roads_stand_on_the_premise() {
        let yaml = r#"
id: x
title: y
given:
  - type: action
    domain: task
    op: create
    with: { title: SEED }
    as: seed
steps_cli:
  - type: action
    domain: task
    op: assign
    with: { target: seed, assignee: me-ai }
steps_gui:
  - type: action
    domain: task
    op: assign
    with: { target: seed, assignee: me-ai }
"#;
        load_str(yaml).unwrap().validate().expect("valid");
    }

    /// A premise proves nothing — it is the state a road starts from. An assert written there would
    /// be a verdict nobody reads, since no driver reports on the world it stood up.
    #[test]
    fn an_assert_in_the_premise_is_rejected() {
        let yaml = r#"
id: x
title: y
given:
  - type: assert
    domain: task
    op: listed
    with: { filter: "status:todo", present: true }
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: T }
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("takes actions alone")));
    }

    /// The line the premise cannot cross: a screen's own move is what the road is watching, so a
    /// premise that carried it out would leave the road proving something already done.
    #[test]
    fn a_screen_move_cannot_stand_in_the_premise() {
        let yaml = r#"
id: x
title: y
given:
  - type: action
    domain: folder
    op: open-existing-card
    with: { dir: shared }
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: T }
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("cannot stand a world up")));
    }

    /// A refusal leaves nothing standing, so it cannot be a premise: a driver that seeded one would
    /// hand the road an empty world and report having prepared it.
    #[test]
    fn a_refusal_cannot_be_a_premise() {
        let yaml = r#"
id: x
title: y
given:
  - type: action
    domain: task
    op: create
    with: { title: T, refused: out_of_reach }
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: T }
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("nothing stands up")));
    }

    /// A premise is not a road. A file carrying a world and no way through it is walked by nobody,
    /// which is the same rot an empty road is refused for.
    #[test]
    fn a_premise_alone_is_not_a_road() {
        let yaml = r#"
id: x
title: y
given:
  - type: action
    domain: task
    op: create
    with: { title: SEED }
    as: seed
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("no steps")));
    }

    /// A problem in the premise is reported as the premise's, not as some road's step: the two lists
    /// number from one apiece, and a reader fixing it has to be told which file section to open.
    #[test]
    fn a_premise_problem_names_the_premise() {
        let yaml = r#"
id: x
title: y
given:
  - type: action
    domain: task
    op: assign
    with: { target: ghost, assignee: me-ai }
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: T }
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.to_string().starts_with("given step 1:")), "{errs:?}");
    }

    /// The shelf a stood catalog serves is the one arg written as rows, and a row is where a typo
    /// would otherwise travel all the way to a catalog served with a blank line under a name.
    #[test]
    fn a_shelf_row_is_held_to_the_words_a_catalog_entry_carries() {
        let stand = |offers: &str| {
            let yaml = format!(
                r#"
id: x
title: y
steps_cli:
  - type: action
    domain: plugin
    op: catalog-stand
    with:
      publishes_key: true
      offers:
{offers}
    as: shelf
"#
            );
            load_str(&yaml).unwrap().validate()
        };

        let errs = stand("        - { desc: a row with no name }").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("missing required field `name`")), "{errs:?}");

        let errs = stand("        - { name: standup }").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("missing required field `desc`")), "{errs:?}");

        // A badge is claimed or it is not, and the word "true" is neither.
        let errs = stand("        - { name: standup, desc: d, claims_official: \"true\" }").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`claims_official` must be a boolean")), "{errs:?}");

        // Display text with no field under it declares nothing, so it never reaches a form.
        let errs = stand("        - { name: standup, desc: d, label: Channel webhook }").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`label` names a `setting`")), "{errs:?}");

        assert!(stand("        - { name: standup, desc: d, setting: channel, label: Channel webhook }").is_ok());
    }

    /// A shelf is a list of rows. Written as one word it would reach the driver as a catalog offering
    /// nothing, which reads exactly like a scenario that meant to offer nothing.
    #[test]
    fn a_shelf_written_as_one_word_is_rejected() {
        let yaml = r#"
id: x
title: y
steps_cli:
  - type: action
    domain: plugin
    op: catalog-stand
    with: { publishes_key: true, offers: standup }
    as: shelf
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`offers` must be a list")), "{errs:?}");
    }
}
