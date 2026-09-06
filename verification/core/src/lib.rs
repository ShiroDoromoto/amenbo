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
    /// scenario says Amenbo will turn away.
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
        /// Which of the app's windows this step is carried out in, named by the title drawn in its
        /// bar. See [`Step::window`].
        #[serde(default)]
        window: Option<String>,
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
        /// Which of the app's windows this step is read against. See [`Step::window`].
        #[serde(default)]
        window: Option<String>,
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

    /// Which of the app's windows this step happens in, named by the title drawn in its bar — or
    /// `None` for the app's one window, which is what a road says by saying nothing.
    ///
    /// A screen driver's business alone: the CLI has no windows, and a road written for it carries
    /// none. It sits beside `with` rather than inside it because it is not an argument of the op —
    /// the same op, in either window, is the same operation on the same store; what differs is which
    /// screen it is done on, and which screen the answer is read off.
    ///
    /// Saying nothing is the honest default while an app draws one window, and it stops being a
    /// default the moment it draws two: the tool behind a screen driver refuses to guess, so a road
    /// that has not said which window fails loudly rather than reading whichever one was in front.
    pub fn window(&self) -> Option<&str> {
        match self {
            Step::Action { window, .. } | Step::Assert { window, .. } => window.as_deref(),
        }
    }
}

/// Free-form named arguments. Values stay as YAML so a driver interprets them; the loader
/// only inspects the few keys it validates (`target`, `present`, `ok`, `refused`).
pub type Args = std::collections::BTreeMap<String, serde_yaml::Value>;

/// Which of Amenbo's two number spaces a binding's id lives in — the answer a driver needs before it
/// can hand that id back to Amenbo. Tasks and decisions number independently, so `dimension set` takes
/// the kind code and refuses a bare number; a driver that spelled every binding `AMB-T-n` would file a
/// decision onto whatever task happened to carry the same digits.
///
/// Only the two kinds a road classifies are here. A binding may stand for a project, a folder or an
/// attachment as well, and none of those is a thing an axis is set on, so [`BoundKind::of_domain`]
/// answers `None` for them rather than inventing a third arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundKind {
    Task,
    Decision,
}

impl BoundKind {
    /// The kind an action of this domain binds, or `None` where what it binds is neither. Read off the
    /// domain rather than off the op: every op that binds in `task` binds a task, and the same in
    /// `decision`, so the domain is the whole of the answer and both drivers can take it here.
    pub fn of_domain(domain: Domain) -> Option<BoundKind> {
        match domain {
            Domain::Task => Some(BoundKind::Task),
            Domain::Decision => Some(BoundKind::Decision),
            _ => None,
        }
    }

    /// The id spelled as the reference Amenbo reads it back by (`AMB-T-n` / `AMB-D-n`).
    pub fn spell(self, id: i64) -> String {
        match self {
            BoundKind::Task => format!("AMB-T-{id}"),
            BoundKind::Decision => format!("AMB-D-{id}"),
        }
    }

    /// What an instruction written for a person calls it.
    pub fn noun(self) -> &'static str {
        match self {
            BoundKind::Task => "task",
            BoundKind::Decision => "decision",
        }
    }
}

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
    /// This device's Amenbo itself, rather than anything filed in it: its configuration, the
    /// identity it answers `whoami` with, the build in place — and the store as a whole, which is
    /// what comes out of it (`export`), what is set aside (`backup`), what goes back in (`restore`)
    /// and whether it is sound (`doctor`).
    Store,
    /// A folder and the project its `.amenbo` pointer names — what an AI launched there may reach.
    Folder,
    /// A file or a link hung on a task, a decision or a comment — the one place Amenbo carries bytes.
    Attachment,
    /// The working folder Amenbo is used from, rather than anything in the store: the files a person
    /// has lying there, and the git repository the lint hooks stand in front of the commits of.
    Repo,
    /// A plugin on this machine: what is installed, whose gate is open, what a call returned, and
    /// what the execution log kept. Named by the name it carries in the catalog, never by a binding.
    Plugin,
    /// Amenbo reached the other way round: a server the host of an AI starts, spoken to over
    /// JSON-RPC rather than typed at. A domain of its own because what a road walks here is the
    /// protocol — a server standing for one folder, the tools it publishes, and what a call through
    /// one comes back with — and none of that is a record in the store.
    Mcp,
    /// The hourly wake-up: the machine's own scheduler starting Amenbo, what Amenbo works out once
    /// it is awake — and the device's consent to any of it, which is the one part of the
    /// tick a person meets on a screen: the band that puts the question across the app, and the
    /// settings row that holds the answer afterwards. A domain of its own because all of it is about
    /// this device's timer and none of it about a record in the store. The wake itself still has
    /// nobody at the keyboard — the caller is the scheduler, the occasion is a calendar day, and
    /// what comes of it leaves through the outbox — while the screen's half decides nothing but
    /// whether that wake is registered at all.
    Tick,
    /// The terminal face: the pane an agent is run in, and whether it is a face of the app's one
    /// window or a window of its own. A domain of its own because none of it is a record — a session
    /// is a process, and which window is drawing it is this machine's arrangement of one screen. The
    /// screen's alone, bar one premise: a terminal is what a reader is already typing in, so the
    /// moves here are the operator's and the CLI driver walks none of them. What it does stand up is
    /// the machine underneath — which agents a pane could be opened with (`can-start`) — because that
    /// is settled before the app comes up and is no more a screen than a project already on the board
    /// is.
    Terminal,
    /// The file face: the folder a project is bound to, read from inside Amenbo — what has changed
    /// in it lately, and the tree folded down it. A domain of its own rather than part of
    /// `Terminal`, because none of it is about the pane it is drawn beside: both of its sections
    /// belong to the **project**, and what they say does not change when the pane beside them does.
    ///
    /// The screen's alone. Reading a file at a shell is `cat`, and there is nothing about that
    /// Amenbo is the subject of.
    Files,
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
    // `dimension` / `value` classify it as it is created — the flag the create itself carries, which
    // is a different road from filing the record and classifying it afterwards: what the two name is
    // written in one transaction, so there is no moment where the record exists unclassified.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "create", required: &["title"], refs: &["project"], strings: &["title", "dimension", "value"], binds: true },
    // The other half of that creation. Between the two the task is on the board and in every listing,
    // out of the mailbox and refused a reservation, so a road that means to hand work over walks this
    // step — and one that reserves has to, or it meets the guard instead. It names the task and
    // nothing else: nobody is being asked to approve it, so there is nothing further for a step to
    // say, and it binds nothing because the task it finishes was bound where it was created.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "finish-creating", required: &["target"], refs: &["target"], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "assign", required: &["target", "assignee"], refs: &["target"], strings: &["assignee"], binds: false },
    // Posting binds the comment, since editing, removing and promoting one all name it afterwards.
    // `mentions` names a record whose **number** is written into the text, after the words the step
    // wrote. It is a key rather than something a road spells out because the store issues the number:
    // a scenario knows which record it means and never what that record was numbered, so a road that
    // wants one record's text to carry another's number has no way to type it. What the number is for
    // is a search: a number is asked as a word underneath the record it pins, and that is only visible
    // where some other record has it written in it.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "comment", required: &["target", "text"], refs: &["target", "mentions"], strings: &["text"], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Comment, op: "edit", required: &["target", "text"], refs: &["target"], strings: &["text"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Comment, op: "rm", required: &["target"], refs: &["target"], strings: &[], binds: false },
    // Promoting writes a decision, so it carries the same `dimension` / `value` a decision's own
    // create does: an axis the project requires is read here too, and a road that could not answer it
    // could only ever walk the refusal.
    OpSpec { kind: Kind::Action, domain: Domain::Comment, op: "promote", required: &["target", "title"], refs: &["target"], strings: &["title", "dimension", "value"], binds: true },
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
    //
    // `at` is the odd one among them. It is written like the others — a word under its own key — but
    // what the word names is a **folder**, one of the folders the task's own project offers, so the
    // driver hands over the path it placed that folder at rather than the word itself. It is also the
    // only field here that another road can take away: unbinding the folder empties it.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "update", required: &["target"], refs: &["target", "mentions"], strings: &["title", "notes", "due", "start", "priority", "at"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "clear", required: &["target", "field"], refs: &["target"], strings: &["field"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "move", required: &["target"], refs: &["target", "project"], strings: &["position"], binds: false },
    // Narrowing a listing that is already drawn, by the words a reader types over it. The words travel
    // as words and not as a line of filter grammar — that grammar splits on whitespace, so a phrase
    // would arrive as its first word alone — and they are ANDed over the record, across every face the
    // word index covers rather than the two a card shows.
    //
    // A screen road alone. A terminal has no listing standing in front of it to narrow: the words and
    // the reading are one command there, which is what `found` walks.
    //
    // What is typed is `words`, or `number_of` naming a record whose **number** is typed instead —
    // one of the two, since they are the one query written two ways (see `takes_a_query`). A number
    // is what a reader types to reach a record they already hold the ref of, and it cannot be written
    // as words: the store issues it, so no scenario knows it in advance. `spelled` is the shape it is
    // typed in — `bare` (`12`) or `hash` (`#12`), both of which the box reads.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "narrow", required: &[], refs: &["number_of"], strings: &["words", "spelled"], binds: false },
    // The same box over the decisions tab, and a separate entry for the reason the panel's three moves
    // are: a road has to say which of the two tabs it is standing on, or it could be walked on either.
    // The side is what the box is read in — a number typed here is a decision's, where the same number
    // typed over the board is a task's.
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "narrow", required: &[], refs: &["number_of"], strings: &["words", "spelled"], binds: false },
    // The other narrowing on that same board, and the three moves it takes: the values are opened, some
    // of them are chosen, and they are folded away again. Opening and folding are written as moves of
    // their own rather than left around the choosing, because what the fold gives back is the room the
    // tasks were drawn in — a thing only the shot after it says.
    //
    // A screen road alone, all three. A terminal writes the whole narrowing as one line of grammar it
    // hands the command, so there is nothing standing in front of it to open, and nothing that would
    // take room back by being shut.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "open-filters", required: &[], refs: &[], strings: &[], binds: false },
    // One press, on one value of one axis. A set is composed by repeating it, which is what the axis
    // takes: the values already chosen there stay chosen, and the board is narrowed to their union.
    // Both words are the CLI's own — the axis is a `--filter` key and the value is what that key takes
    // — since the words on screen are each reader's own language, and the pair the two sides share is
    // the one thing a road can be written in.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "choose-filter", required: &["axis", "value"], refs: &[], strings: &["axis", "value"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "close-filters", required: &[], refs: &[], strings: &[], binds: false },
    // The same three on the decisions tab, which has the same panel over a different list. They are
    // separate entries rather than one shared with the board's because a road has to say which of the
    // two tabs it is standing on — a step that named neither could be walked on either and would prove
    // whichever the operator happened to be looking at.
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "open-filters", required: &[], refs: &[], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "choose-filter", required: &["axis", "value"], refs: &[], strings: &["axis", "value"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "close-filters", required: &[], refs: &[], strings: &[], binds: false },
    // Pressing a hit through to the record it points at. The excerpt beside a hit is cut to say where
    // the words are written and never to be read in place of the record, so the press is what the hit
    // is for. The words are named here because a hit has to be standing before there is one to press,
    // and asking for them is the move that draws it.
    //
    // A screen road alone. A terminal prints its hits as text and the reader types the ref it read
    // into `show`, so there is nothing there to press.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "open-hit", required: &["target"], refs: &["target", "number_of"], strings: &["words", "spelled"], binds: false },
    // Onto a smart view, from the row in the sidebar that stands for it. A smart view is a standing
    // selection of tasks and not a project's board, so opening one is a move of its own: what the row
    // says before it is pressed and what the press lands on are two claims about the same view, and
    // only a road that makes the move between them can hold one to the other.
    //
    // A screen road alone. A terminal has no standing selection to press — it writes the selection out
    // as a `--filter` each time it asks, which is what `listed` walks.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "open-view", required: &["view"], refs: &[], strings: &["view"], binds: false },
    // Going from a task to the pane its work is happening in — the road out of the ledger, and the one
    // the ledger has. The pane is named rather than pointed at: the row carries the
    // pane's own name, so an operator pressing it can see it is the pane the road meant before the
    // press moves the screen. What the press does after that is nowhere in the step: which window
    // holds the terminal face is the run's, and the road reads where it landed with `terminal pane`.
    //
    // A screen road alone. A terminal has no pane to go to and no face to switch.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "go-to-pane", required: &["target", "shows"], refs: &["target"], strings: &["shows"], binds: false },
    // A card carried into one of the columns a cut board draws, which is the board's own way of filing
    // work: what it lands under is the value on the column's heading, so the move names the axis and
    // that value rather than anywhere on a screen. The card is named by `target`, the way every step
    // pointing at a record names it.
    //
    // A closed value is where the road comes apart, which is why this is written as an op that can be
    // turned away: such a column stands while cards are still in it and takes no drop, so the value the
    // picker stopped offering cannot be reached around by hand. The screen holds that line one step
    // earlier than the store does — the column is no drop target at all, rather than a drop the store
    // refuses — so `refused:` names the rule the road walks (`dimension set`'s own) and not a sentence
    // anybody is shown.
    //
    // A screen road alone: a terminal files work with `dimension set`, and has no column to carry a
    // card into.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "drop-into-column", required: &["target", "axis", "value"], refs: &["target"], strings: &["axis", "value"], binds: false },
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
    // The image a project shows for itself, given to it and taken away again — the one pair of moves on
    // that face carrying bytes rather than words. `file` names the image to choose, and it is a file a
    // premise laid down rather than one the operator brings: an attachment is reached by what it is
    // called and this is reached by what it looks like, so any file of the right name would do there and
    // none would do here. Clearing names nothing beside the project, a row holding one image and not a
    // set of them.
    //
    // Both write the form rather than the row, so both end at the button the name and the colour beside
    // them wait for: a road that stopped at the picker would be reading a screen that has changed and a
    // store that has not.
    //
    // A screen road alone. `project update` is the terminal's whole door onto these fields and it takes
    // a name, a note, a colour and a view — there is nowhere to hand it an image, so a road written here
    // would be naming a command that does not exist.
    OpSpec { kind: Kind::Action, domain: Domain::Project, op: "set-icon", required: &["target", "file"], refs: &["target"], strings: &["file"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Project, op: "clear-icon", required: &["target"], refs: &["target"], strings: &[], binds: false },
    // And back onto the board that project keeps. It is the move a road needs where what is under test
    // is drawn when a project is opened rather than held from before: walking out and back in is the
    // only way to ask a screen what it does on arrival, and a road that assumed the arrival would be
    // reading the screen it never left.
    OpSpec { kind: Kind::Action, domain: Domain::Project, op: "open", required: &["project"], refs: &[], strings: &["project"], binds: false },
    // Recutting that board: which axis its columns are split along. It is the one move that changes
    // what a board *is* rather than which of its cards are drawn, and until it existed every screen
    // road could only read the split a board opens on. `axis` names an axis by the name a user typed,
    // and only an axis: the split a board opens on is the status one, and going back to it is a move
    // no road has needed — walking out to another project and back does not do it, the board holding
    // what it was last cut along. A road that needs it grows the op a value, beside the road.
    //
    // A screen road alone: a terminal has no board to cut.
    OpSpec { kind: Kind::Action, domain: Domain::Project, op: "group-by", required: &["axis"], refs: &[], strings: &["axis"], binds: false },
    // A classification axis, its values, and the assignment that files a task under one. The axis and
    // the value travel as words — a name, or the key the row answers to, since the command resolves the
    // key before it tries a name.
    // `slug` names the readable key the axis or the value is to answer to outside Amenbo. Left out,
    // the door derives one from the id, which is what nearly every row keeps — so a road writes it
    // only where the key itself is what is under test.
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "create", required: &["name"], refs: &[], strings: &["name", "slug"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "value-add", required: &["dimension", "value"], refs: &[], strings: &["dimension", "value", "slug"], binds: false },
    // The value gone again, and with it the classification of every task that answered with it. `to`
    // names another value of the same axis for those tasks to land on instead — which a required axis
    // demands whenever any of them exists, since letting the value go would empty them out behind the
    // creation premise's back. Left out where the demand is up, and on the last value of such an axis,
    // the removal is turned away, which is what a road walks it for.
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "value-rm", required: &["dimension", "value"], refs: &[], strings: &["dimension", "value", "to"], binds: false },
    // Filing something under one of the axis's values. `target` is a task **or** a decision — one axis
    // and one set of values, whichever kind is being filed — so the driver spells the binding with its
    // kind code (BoundKind, above) rather than handing back a number the command would refuse.
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "set", required: &["target", "dimension", "value"], refs: &["target"], strings: &["dimension", "value"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "unset", required: &["target", "dimension", "value"], refs: &["target"], strings: &["dimension", "value"], binds: false },
    // Whether the axis belongs on the board's task cards. The answer is the axis's own rather than a
    // reader's setting, which is what lets a screen road stand it up from the command line: what a
    // card draws follows from the store, so the world a `given:` sets is the world any face opens
    // on. `show: false` lowers it again, for a road that needs the axis off the card.
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "show-on-card", required: &["dimension"], refs: &[], strings: &["dimension"], binds: false },
    // Whether the axis refuses to be left empty. It is the axis's own answer like the one above it,
    // and it bites in exactly one place — the step that ends a creation — so a road that raises it
    // proves it by being turned away there and nowhere else. `required: false` lowers it again, for a
    // road that needs the demand out of the way. An axis offering no values could never be answered,
    // so raising it on one is refused, which is a road of its own to walk.
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "required", required: &["dimension"], refs: &[], strings: &["dimension"], binds: false },
    // How many of the axis's values one record may answer it with. It is a flag like the
    // two above it — `multi: false` lowering it again — and the way down is the direction that can be
    // refused: a record still answering with several would have every value but one thrown away, so
    // core turns it back and names the count. A multi-select axis cannot also be the time axis, which
    // resolves one era, so the pair is refused at whichever of the two moved.
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "cardinality", required: &["dimension"], refs: &[], strings: &["dimension"], binds: false },
    // Which of the two sides of the store the axis classifies at all. `side` is a word and not a
    // switch — there are three answers, and unlike the two flags above it the axis starts on the wide
    // one — so a road here narrows rather than raises, and takes `side: both` to widen back.
    // Narrowing takes no filing away, only the offer, which is why a road that walks it reads the
    // filings back afterwards rather than treating the narrowing as a delete.
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "applies-to", required: &["dimension", "side"], refs: &[], strings: &["dimension", "side"], binds: false },
    // Which axis is the project's time axis — the role that makes a value's period mean anything, and
    // that puts the era containing today on a task and a decision as each is created. It is a flag like
    // the two above it, `time_axis: false` taking the role away again, and it is a move of its own
    // rather than an arg on `create` for the reason `rekey` is: the screen names an axis the time axis
    // from the manager, on an axis that already exists, so a road that wrote the role at birth could
    // not be walked there.
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "time-axis", required: &["dimension"], refs: &[], strings: &["dimension"], binds: false },
    // The window one value of that axis covers — `start` and `end` as `YYYY-MM-DD`, either one alone
    // leaving that side open. A period is payload of the time-axis role and not an attribute any value
    // carries, so this is written after the role is named, and on that axis's values alone. Like the
    // role, it is set on a value that already exists: that is the door the screen has, its date fields
    // sitting on the value in the manager.
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "period", required: &["dimension", "value"], refs: &[], strings: &["dimension", "value", "start", "end"], binds: false },
    // Whether the axis retires its values by closing them instead of deleting them. It is the other
    // nomination the role slot holds, so it is written the way `time-axis` above it is — on an axis
    // that already exists, which is the door the manager has — and `closable: false` gives it up
    // again. One slot means one role, so naming this takes the time axis off the same axis.
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "closable", required: &["dimension"], refs: &[], strings: &["dimension"], binds: false },
    // Retiring one of that axis's values, and bringing it back. Closing is the payload of the role
    // above the way a period is the time axis's, and it is not `value-rm` in a quieter form: everything
    // already filed under the value keeps it and a filter naming it goes on resolving, while nothing
    // new is filed under it from here on. The two directions are not one move either — closing asks for
    // the role, and is refused on the last value a required axis still offers, where reopening is free
    // on any axis, so an axis that gave the role up strands nothing.
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "value-close", required: &["dimension", "value"], refs: &[], strings: &["dimension", "value"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "value-reopen", required: &["dimension", "value"], refs: &[], strings: &["dimension", "value"], binds: false },
    // Renaming that key afterwards — the axis's own, or one of its values' where `value` names one.
    // It is a move of its own rather than an arg on the ops above, because naming a key at birth and
    // renaming one are two different doors: the screen has only the second, so a road that wrote the
    // key where the row was created could not be walked there at all.
    OpSpec { kind: Kind::Action, domain: Domain::Dimension, op: "rekey", required: &["dimension", "slug"], refs: &[], strings: &["dimension", "value", "slug"], binds: false },
    // Ordering between two tasks, and the anchor back to the history that carried the work out.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "depend", required: &["target", "on"], refs: &["target", "on"], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "undepend", required: &["target", "on"], refs: &["target", "on"], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "commit-add", required: &["target", "sha"], refs: &["target"], strings: &["sha"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "commit-rm", required: &["target", "sha"], refs: &["target"], strings: &["sha"], binds: false },
    // `project` names the shelf it is filed on, for a scenario about where a record ends up; left
    // out, it is the run's own project like everything else.
    // Classified as it is recorded, the same way a task is — and on this side it is also how a
    // required axis is answered before the acceptance reads it, since a decision has no second stage
    // for its writer to be turned away at.
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "create", required: &["title"], refs: &["project"], strings: &["title", "dimension", "value"], binds: true },
    // Onto the face a project's decisions are read on, which sits beside the views its tasks are read
    // on rather than under them: a decision is not a task drawn another way. A road that wants to read
    // a row of that list has to press it, and pressing it is the only way there.
    //
    // A screen road alone. A terminal asks for decisions by naming them, and never has to be anywhere.
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "open-face", required: &[], refs: &[], strings: &[], binds: false },
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
    // The one of those settings a road walks rather than declares: the language the interface is
    // read in. It is written as a move and not as a premise because what turns on it is what the
    // screen does **when it changes** — a listing drawn in one language and redrawn in another is
    // the whole of what a translated catalog line has to survive, and a store that opened in the
    // second language proves nothing about the first.
    //
    // A screen road alone. The setting is reachable from a terminal (`config-set`), but nothing there
    // is drawn in it: what the CLI prints is English whatever this says, so a road that set it in a
    // terminal would be changing a value nothing it could then read depends on. `language` is the code
    // the store keeps, and the instruction says it in full.
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "set-language", required: &["language"], refs: &[], strings: &["language"], binds: false },
    // The view a project created without one of its own comes up in. Like the language beside it, it
    // is written as a move rather than as a premise, and for the same reason twice over: what turns on
    // it is what happens **after** it changes — the next project raised — and a store that opened with
    // the value already set proves nothing about the setting having been reachable.
    //
    // `view` is the word the store keeps (`list` / `board` / `calendar` / `timeline`), which is also
    // the word `config set default_view` takes. The pull-down draws each of the four in the reader's
    // own language, and a table of those here would be a second copy of nineteen dictionaries.
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "set-default-view", required: &["view"], refs: &[], strings: &["view"], binds: false },
    // The face each of the two facets is given, registered and taken away again on the row that draws
    // the pair of them. `facet` names which slot the step is about — `human` or `ai`, the two words the
    // store files them under — and `file` the image to choose. That file is one a premise laid down
    // rather than one the operator brings, for the reason a project's icon is: what is under test is a
    // picture being drawn, so any file of the right name would prove nothing.
    //
    // Neither move waits for a button beside it. Choosing writes the store where it stands and so does
    // clearing, so each step ends where the picker closes rather than at a form being saved.
    //
    // A screen road alone. `config set human_avatar` is the terminal's whole door onto this and what it
    // takes is the display version as a data URL — the original that version was baked from arrives
    // only where a picker handed the bytes over, so a road written for the CLI would walk past the
    // half this one exists for.
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "set-avatar", required: &["facet", "file"], refs: &[], strings: &["facet", "file"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "clear-avatar", required: &["facet"], refs: &[], strings: &["facet"], binds: false },
    // And back onto the screen those two are made on, for a road that walked off it. It is the move the
    // avatar road turns on: a slot redrawn where the operator is standing says the screen heard, and
    // only an arrival says the store did.
    //
    // A screen road alone: a terminal stands in a folder and not on a screen, so it has nothing to
    // open and nothing to come back to.
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "open-settings", required: &[], refs: &[], strings: &[], binds: false },
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
    // A machine nobody has raised anything on. The driver raises a project as it boots — the store
    // has to have somewhere to file what a premise stands up — so a road that opens on the screen a
    // first-time reader meets could not be given a world at all: the moment it declared one, there
    // was a project on the screen it was written without. This takes the store back to none.
    //
    // It goes **first** in a premise. Losing a project releases the folders still pointing at it, so
    // anything laid on the disk beforehand goes with the projects — what such a road wants lying in
    // a folder is written by the steps after this one, the way `folder legacy-pointer` writes its
    // own. `project delete` is the same move on a road and is not this: a road that emptied the
    // device would be walking `retire-a-project` instead of its own.
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "nothing-raised", required: &[], refs: &[], strings: &[], binds: false },
    // The answer given to a nudge that came up on its own. A screen road alone: nothing in a terminal
    // puts one, so the CLI driver never meets it.
    //
    // The answer travels as a value rather than in the op's name, the way a consent answer does. The
    // one this road gives is the refusal, and it is the only one the driver takes: an acceptance
    // registers this machine's login with the OS, which is the one piece of state no throwaway store
    // can hold and no run can hand back — that half is walked on real machines instead.
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "nudge-answer", required: &["nudge", "answer"], refs: &[], strings: &["nudge", "answer"], binds: false },
    // The app ended and opened again on the same store. It is not a move on a screen at all: it is a
    // run of Amenbo going out and another coming up, which is the one gap a road cannot otherwise
    // reach and the only place several promises are kept. What a person set and comes back to is
    // settled here, against everything that was this run's own and goes with it — the places a
    // terminal was drawn in, the names on them — which the app keeps for a run and no longer.
    //
    // **It is the harness's own step, and the only one that is.** The run owns the app it shoots —
    // the store it is pointed at is the run's, and the pid is how a shot names this window rather
    // than whatever else of the same build is open — so an operator who quit Amenbo and opened it
    // again from their machine would bring up a second app, on their own backlog, that the run
    // cannot shoot. The instruction handed over says so: there is nothing to press, and what is read
    // is the window that came back.
    //
    // The app is ended rather than asked to leave, for the reason a run takes it down at the end:
    // asking goes through the app's name, and a name cannot pick out one instance. It is the harder
    // half of what is under test besides — what outlives a run is written as it is changed rather
    // than on the way out, so a build that only wrote at the door would go red here and be right to.
    //
    // A screen road alone. A CLI command is a run of its own that ends when it has printed, so a
    // terminal carries nothing across and there is no gap here for this op to be.
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "run-again", required: &[], refs: &[], strings: &[], binds: false },
    // What a folder's binding is made of. A folder is named, not pointed at: `dir` is a plain name
    // the driver places somewhere of its own, since a pointer is answered by where a folder sits.
    // `init` raises a project of its own and binds it (hence the binding), `bind` points a folder at
    // one that already exists — this run's, unless `project` names another.
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "init", required: &["dir"], refs: &[], strings: &["dir"], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "bind", required: &["dir"], refs: &["project"], strings: &["dir"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "unbind", required: &["dir"], refs: &[], strings: &["dir"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "sync-guide", required: &["dir"], refs: &[], strings: &["dir"], binds: false },
    // A pointer left in the shape an older Amenbo wrote, in a folder that is bound. Nothing Amenbo
    // does today writes one — it is the state a repair exists for, so a scenario about the repair has
    // to put the folder in it, the way `repo write-file` puts a file a person already had.
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "legacy-pointer", required: &["dir"], refs: &[], strings: &["dir"], binds: false },
    // A pointer another store's build left in that folder. A device carries one store per channel —
    // what a release installs, the one a developer builds, the throwaway one a task is checked in —
    // and a pointer's `project_id` is a primary key in the numbering of the store that wrote it. So a
    // folder claimed by one channel and then read by another lands on whatever that other one happens
    // to keep at the same key, which is a different project under the same number.
    //
    // Only another store can leave one, since a build stamps its own name as it writes: the state is
    // out of reach from the build under test the way a `legacy-pointer` is, and for the same reason it
    // is made here instead. `store` is the name the folder was claimed by, and the road wants one this
    // build is not — claimed by the build's own name is a folder it is welcome in, which is every
    // other road in this file.
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "foreign-pointer", required: &["dir", "store"], refs: &[], strings: &["dir", "store"], binds: false },
    // A pointer naming a project the store under test does not have. It is what a folder is left
    // holding when the store that answered for it goes — a channel wiped, a throwaway store dropped,
    // an export restored somewhere else — and nothing on the folder says so: the file is well formed,
    // this build wrote it, and only the number it carries leads nowhere.
    //
    // Made here for the reason its two neighbours are. A build names a project it has as it writes,
    // so the one state it cannot arrive at is this one, and on screen there is no move that leaves a
    // folder behind at all. What it is made out of is this run's own pointer with the number moved,
    // so everything a build reads before it resolves that number agrees — which is what leaves the
    // number as the whole of what the road is about.
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "lost-pointer", required: &["dir"], refs: &[], strings: &["dir"], binds: false },
    // A folder that went somewhere else. Renamed, moved, restored beside where it was — to the
    // registry they are one thing, since what it holds is a path and the path no longer leads
    // anywhere. `dir` is the folder as it stood and `to` where it stands now, and what is in it
    // travels with it — the pointer included, the way it does when a person drags a folder.
    //
    // Deleting one is a different road and is not written here: a folder that is gone has nowhere to
    // be re-pointed to, so what answers it is `unbind` or a fresh `bind`, both of which are already
    // said. What has no other answer is the folder that is still there under another name.
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "move", required: &["dir", "to"], refs: &[], strings: &["dir", "to"], binds: false },
    // The binding that folder had, brought onto where it stands now — **keeping its id**, which is
    // the whole difference from binding it again. A second bind records a new row, and whatever named
    // the old one — a task that says which of its project's folders it is worked in, among others —
    // is left naming a row nobody points at.
    //
    // `dir` is the folder's new home and `moved` names the folder it was, which is how a road names a
    // binding whose number it cannot know: the id is the store's own, and the one place it is
    // published is the answer `bind` gives in a folder whose project has one that vanished. So this
    // has to **follow that folder's `move`** — before it, there is nothing gone to re-point.
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "rebind", required: &["dir", "moved"], refs: &["project"], strings: &["dir", "moved"], binds: false },
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
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "copy-fixture", required: &["from", "path"], refs: &[], strings: &["from", "path", "dir"], binds: false },
    // The clipboard filled from outside Amenbo, which is the only way a road can put a real one in
    // front of the panel. It is not stood up in a premise: what is under test is the machine's own
    // clipboard, and a premise that filled it with something other than a file manager would be
    // testing a road nobody walks.
    //
    // They are the operator's own hands on their own file manager, so what is named is the file and
    // the folder as the road calls them, never a path or an application.
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "copy-outside", required: &["path"], refs: &[], strings: &["path", "dir"], binds: false },
    // `path` is the folder inside the bound one to paste into, and it is not optional: pasting into
    // the folder the file was copied from makes the file manager rename what it writes, and what it
    // renames it to is that machine's own word in its own language. A road reading that name back
    // would be reading the file manager rather than Amenbo.
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "paste-outside", required: &["path"], refs: &[], strings: &["path", "dir"], binds: false },
    // And a name in that folder that is a link rather than a file. `path` is the name, `to` is what
    // it points at, read in the run's own folder — which is outside every folder a `folder` step
    // binds, so the link a road makes is the one people really make: a file kept in one place and
    // pointed at from a project. A link is what its own name is, not what it leads to, so nothing
    // has to be lying at `to` for the road to walk.
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "symlink", required: &["path", "to"], refs: &[], strings: &["path", "to", "dir"], binds: false },
    // `git-init` takes the same `dir`, and for a reason of its own: what git says about a folder is
    // drawn on the file face of the folder a project is *bound* to, so a road reading those colours
    // needs the repository to be that folder and not the one the run stands in.
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "git-init", required: &[], refs: &[], strings: &["dir"], binds: false },
    // Everything lying in the folder, recorded. It exists for one state no road could otherwise
    // reach: git naming a file while saying nothing about the folder holding it. A repository that
    // has only ever been `init`-ed has nothing tracked in it, so git names the top folder and stops
    // — and the tree's rollup, which is about a folder git did **not** name, is never walked.
    // `dir` follows `git-init`'s rule and for its reason.
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "git-commit", required: &[], refs: &[], strings: &["dir"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "hooks-install", required: &[], refs: &[], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "hooks-uninstall", required: &[], refs: &[], strings: &[], binds: false },
    // The paste that starts this folder's AI on Amenbo at every session, put where the build says it
    // goes. Amenbo hands the text over and never writes that file, so somebody has to do it for the
    // road to carry on — the driver stands in for the hand that pastes, the way `write-file` stands
    // in for a file a person already had. `tool` is the provider, by the name the build's own
    // catalog answers to.
    // Where it lands follows `write-file`'s rule and for the same reason: the run's own folder unless
    // the step names one a `folder` step bound, since a bound folder reads as wired only from what is
    // inside it. That is also what lets this stand a world up — a road that opens on a folder somebody
    // already wired has no other way to arrive there, the wiring being a file and not a record.
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "wire-ai", required: &["tool"], refs: &[], strings: &["tool", "dir"], binds: false },
    // An app already reaching a folder over MCP — the state a folder nothing opens a shell in is in,
    // and the one thing that tells it apart from a folder nobody has set up yet. `app` is the app by
    // the name the build's own catalog answers to, and where it lands follows `write-file`'s rule:
    // the run's own folder unless the step names one a `folder` step bound.
    //
    // **Only an app whose settings live inside the folder may be named**, and the driver refuses the
    // rest. Most of the catalog keeps one file for the whole machine — the operator's own — and a run
    // that wrote into it would be setting the person driving it up as a side effect of a road.
    //
    // Unlike `wire-ai` this cannot ask the build for what to write: the entry an app is set up from is
    // handed over on screen, and the one app Amenbo writes a file for takes a bundle a person opens.
    // So the shape is the driver's, and it drifts the safe way round — an entry the build no longer
    // reads leaves the folder unreached, which turns the road that says the report went red rather
    // than green.
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "mcp-reach", required: &["app"], refs: &[], strings: &["app", "dir"], binds: false },
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
    // Amenbo knows, and the only way to show they are all reachable is to reach one the folder shows no
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
    // The same two moves on the project's own settings, where the way to the text is open whatever the
    // report is doing. They are ops of their own rather than the ones above read on another screen: the
    // report is what those name, and on a folder already wired there is no report to name — so an
    // instruction sending the operator to it would be one nobody could carry out. Which is the whole
    // state this pair exists for, the reader who wired one tool and then moved to another.
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "ai-launch-request-pick", required: &["tool"], refs: &[], strings: &["tool"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "ai-launch-request-copy", required: &["tool"], refs: &[], strings: &["tool"], binds: false },
    // The other way in for an AI whose host cannot open a folder at all, which is folded away until it
    // is asked for. Most readers never need it — somebody working from a terminal has Amenbo already —
    // so it is one item to walk past, and everything about it is behind this one move.
    //
    // Opening it is a step of the road rather than something the operator does on the way, and for the
    // reason every other move on a screen road is: the screen it opens onto is what the asserts after
    // it are read against, and a screen nobody opened leaves them read against the one in front of it.
    // It is also the way *back*: a road that left for another app and returned reads the same rows
    // again, and a re-open is what makes the second reading a reading rather than a cached draw.
    // A screen road alone — a terminal reaches a project by standing in its folder, and has no list of
    // apps at all.
    //
    // Not `Domain::Mcp`, for all that the name says MCP: that domain is the protocol itself — a
    // server standing for a folder and being called over two streams — and nothing here speaks it. What
    // this reads is settings lying on the machine, which is this domain's, and the family it belongs
    // beside is the wiring text one screen over.
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "mcp-open", required: &[], refs: &[], strings: &[], binds: false },
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
    // An installed plugin left recording a build the catalog has moved past. What Amenbo calls an update
    // is the installed manifest's checksum differing from the catalog's, and a scenario cannot reach that
    // state by using Amenbo: the catalog publishes one build, and the trust model means no other one can
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
    // what Amenbo then does about an empty one.
    //
    // `readonly: true` writes the flag that says the value is the plugin's own to fill in and not the
    // user's. It is a word on this declaration for the reason `required` is: the field
    // written is the same field, and what the flag changes is what the faces then do about it — a form
    // draws the value with no box and no button, while the write door stays open, since that door is how
    // the plugin's own value arrives.
    //
    // `translated` writes the words the author put on that same field in other languages, keyed by
    // language code — the `label` a form draws it under, and for a choice the `options` its candidates
    // are drawn under, keyed by the value each one stores. It goes where an install puts what a catalog
    // published, beside the manifest rather than in it, so a form reads it the way it reads a real one.
    // No published plugin declares a setting at all, so no published plugin has one translated either:
    // both halves are unreachable for the same reason and are written by the same door.
    //
    // `when_field` and `when_has` are the condition an author put on that setting — the
    // setting whose answer decides whether this one is drawn, and the value looked for among its answers.
    // The pair is two words rather than one nested block for the reason `ask`/`ask_label` is: a step's
    // `with` is a flat mapping of words, and a condition written as a list of objects inside one is a
    // shape no other op here takes. The platform half of a `when` has no word at all — a road conditioned
    // on the OS walks differently on each runner, which is a scenario that proves something different
    // depending on where it ran.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "declare-setting", required: &["name", "key"], refs: &[], strings: &["name", "key", "label", "when_field", "when_has"], binds: false },
    // An installed plugin declaring a setting its author marked secret. Which settings a plugin takes
    // is the author's word and Amenbo never invents one, so the only honest way to reach this state is
    // for a plugin that declares one to be published — and no plugin in the official catalog does. The
    // secret route (off the store, off every backup, injected as an environment variable) is the half
    // of `plugin config` that fails silently and in plain text, so it is not left unwalked until one
    // is: the driver writes the declaration onto the installed manifest, the way `stale-manifest`
    // writes the disagreement it needs. Everything after it is Amenbo's own doing.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "declare-secret", required: &["name", "key"], refs: &[], strings: &["name", "key", "label"], binds: false },
    // An installed plugin whose author wrote a part into its `config` list for Amenbo to *draw* — a
    // caption, a way to the page that issues a value, a code to hold a phone up to.
    // Written onto the installed manifest for the reason every declaration here is: which parts a plugin
    // draws is its author's word, and no plugin in the official catalog writes one, so a road about a
    // form that says something before anybody has filled anything in has no other way to be standing in
    // front of one. `kind` is the part, `value` the string it carries — for a `list`, its lines joined by
    // commas — and `label` the words on a `link`'s button.
    //
    // Where it lands in the list is where it is drawn, so a road walks the declarations in the order it
    // wants them read: a `declare-part` between two `declare-setting`s is a part between two boxes.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "declare-part", required: &["name", "kind", "value"], refs: &[], strings: &["name", "kind", "value", "label"], binds: false },
    // An installed plugin declaring a setting whose answers its author listed, and the one that stands
    // while nobody has answered. Same reason as `declare-secret`: which settings a plugin takes is the
    // author's word, and no plugin in the official catalog offers candidates — so the half of
    // `plugin config` that keeps three answers apart (a choice made, none of them chosen, nobody asked
    // yet) would go unwalked until one does. `options` is the candidates as their stored values, joined
    // by commas the way an answer is; `default` is a subset of them, and leaving it out is the other
    // shape a choice comes in. `translated` is the same word it is on `declare-setting`, and this is
    // where its `options` half has anything to translate.
    // `when_field` / `when_has` are the same pair `declare-setting` takes, and they land on the choice
    // itself — a candidate's own condition is written where the candidate is, which is `candidate_when_*`.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "declare-choice", required: &["name", "key", "options"], refs: &[], strings: &["name", "key", "label", "options", "default", "when_field", "when_has", "candidate", "candidate_when_field", "candidate_when_has"], binds: false },
    // An installed plugin declaring an operation a reader may press on its settings form. Same reason as
    // the three above it, one door further along: what that form offers is the author's word, and no
    // plugin in the official catalog declares a settings block at all — so the button, and the value a
    // press asks for, are states no install reaches. `cmd` is the call the press raises and `label` the
    // words the button is drawn under; `ask` names the one value asked at the press, under the words
    // `ask_label`, and leaving it out is the other shape an operation comes in — a button that runs the
    // moment it is pressed. `ask_secret` is the author saying that value is a credential, which is a word
    // on this declaration rather than an op of its own: the field written is the same field, and what the
    // flag changes is how the form draws the box in front of it.
    // `when_field` / `when_has` are the same pair the settings take: someone who chose
    // iCloud has no use for a button that raises a Cloudflare tunnel, and a form that hides that
    // transport's fields while keeping its button leaves a step nobody can follow.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "declare-action", required: &["name", "cmd", "label"], refs: &[], strings: &["name", "cmd", "label", "ask", "ask_label", "when_field", "when_has"], binds: false },
    // And the other half of that same block: the check an author has raised on the values before a gate
    // opens on them. It is written onto what is installed for the reason its neighbours are — no plugin in
    // the official catalog declares a settings block — so a gate that turns on somebody else's judgement
    // is a door no install reaches. `cmd` is the call the check raises.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "declare-check", required: &["name", "cmd"], refs: &[], strings: &["name", "cmd"], binds: false },
    // An installed plugin saying, in its author's words, when to reach for it and what to type. What a
    // plugin says for itself is written in its manifest and Amenbo invents none of it, so this is the
    // author's block arriving the only way it can — written onto the installed manifest, the way
    // `declare-secret` writes a declaration no published plugin carries. Which is also why the scenario
    // does not read the catalog's own wording back: an author may reword their block any day, and a line
    // asserting today's sentence would go red on a change Amenbo had no part in. `when` is the occasion;
    // `cmd` and `does` are one call, which is enough to see the calling form Amenbo puts in front of it.
    // `steps` is where that call says it is a tool — the ids of Amenbo's own steps, comma-separated, the
    // way an author writes them. It is the author's word too, and no published plugin writes one yet, so
    // the road to a step carrying a tool is only walkable once a block here declares it.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "declare-agent", required: &["name", "when"], refs: &[], strings: &["name", "when", "cmd", "does", "steps"], binds: false },
    // An installed plugin declaring the layer it lives at — one project's rows, or the device's.
    // Same reason as the declarations above it: the layer is the author's word, a manifest
    // saying nothing means `project`, and **every plugin the official catalog serves says nothing** — so
    // the device layer is a state no install reaches, and the road a machine-wide plugin walks is only
    // walkable once this writes the declaration onto the installed manifest. Everything after it is
    // Amenbo's own: which rows the enable opens, and how wide a window the run is handed.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "declare-scope", required: &["name", "scope"], refs: &[], strings: &["name", "scope"], binds: false },
    // An installed plugin that is nobody's but its author's. The badge is the catalog's to grant and no
    // author can write it onto themselves, which is what makes it the one thing Amenbo can safely split
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
    // What it reads back is Amenbo's own doing: which value, at which tier, and whether there is one
    // left at all.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "echo-program", required: &["name"], refs: &[], strings: &["name"], binds: false },
    // An installed plugin whose program answers a press with a line of its own. An operation raised from
    // the settings form has no return value: what the form draws afterwards is the author's first line on
    // stderr, and the value that press asked for reaches the run as an environment variable and is kept
    // nowhere. Both are only visible from inside the run, and only a program willing to say what it was
    // handed can say either — which no published plugin is, none of them declaring an operation at all.
    // So the driver stands one in that writes its one line, naming what it was asked for.
    //
    // It also answers a check with a yes, on the stream the press never looks at: a press draws stderr
    // and discards stdout, a check reads stdout and only logs stderr. A plugin has one program, so a
    // settings block carrying both halves — a check before the gate and a button behind it — is walked
    // by standing in this one. `check-program` is what a road reaches for when the *verdict* is the
    // thing under test, since that is the half this cannot vary.
    //
    // `writes` and `writes_value` leave it writing one of its own settings back on every press, through
    // `plugin config set` — the door a plugin's own value arrives by, and the only one there
    // is for a field its author marked `readonly`. That is the whole of what a `setup` does: it works
    // something out — an address it registered, a key it generated — and puts it where the form will draw
    // it. Naming neither leaves the program as it was, writing nothing. The value is not read back from
    // in here: what says the write landed is the field on the form afterwards, which is the reading the
    // road is about, and the program says so on its own line only when the write was refused.
    //
    // `shows` and `shows_value` are the other half of what a run may answer with: the kind
    // of part and the string it carries, with `shows_label` for the words on a `link`'s button. A press
    // has no return value a form reads otherwise, so a road about a QR coming back from a `setup` has
    // nothing to look at without this — and the picture is Amenbo's to draw, which is exactly the claim
    // the road exists to hold.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "press-program", required: &["name"], refs: &[], strings: &["name", "writes", "writes_value", "shows", "shows_value", "shows_label"], binds: false },
    // An installed plugin whose program answers the check with a verdict. Whether the values are usable is
    // the author's judgement and Amenbo makes none of its own, so the only thing that can say no is a
    // program that says it — and no published plugin declares a check to answer at all. `ok` is that
    // judgement, which the road picks rather than the program: the same values are the ones a fixed answer
    // could never turn away and then let through. `message` is the sentence for the head of the form and
    // `field_message` the one drawn beside the setting `field` names, both being the author's own words,
    // which is what a road reads back to know whose sentence reached the screen.
    //
    // A plugin has one program, so this stands in for whatever was standing there before — `press-program`
    // included, whose own yes it replaces. A road reaches here when the verdict is what it is about, and
    // for `press-program` when the settings block is walked with the check simply passing.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "check-program", required: &["name", "ok"], refs: &[], strings: &["name", "message", "field", "field_message"], binds: false },
    // An installed plugin whose program calls Amenbo back. A payload names a record and carries none of
    // it, so the route to the content is the binary itself, run from inside the plugin with the store and
    // the window Amenbo put in its environment — and no plugin in the official catalog takes it (the one
    // published there works out everything it does from the repository it is called in). So the only
    // witness that the environment really arrives, that a call made through it needs no facet, and that
    // the window is what bounds it, is a plugin that makes the call: the driver stands one in, the way
    // `echo-program` stands in the only witness an injected secret has. Its faces are `read` and `write`,
    // each taking the id of a task an earlier step bound and handing everything under `args` to Amenbo
    // verbatim — so the call under test is written in the scenario rather than buried in the driver.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "read-back-program", required: &["name"], refs: &[], strings: &["name"], binds: false },
    // An installed plugin that takes `seconds` to answer. A queue only holds rows while its plugin is
    // still on one — the runner takes the row off the moment the plugin replies, whichever end it
    // reached — so a backlog is not a state a scenario can arrive at by using Amenbo: it would be
    // racing the runner it just started. Every plugin the catalog publishes answers in the time a
    // process takes to start, and slowness is exactly what the backlog display exists to diagnose, so
    // the driver leaves one answering slowly, the way `declare-secret` writes a declaration no
    // published plugin carries. `seconds` is the window the asserts after it have to read in.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "slow-program", required: &["name", "seconds"], refs: &[], strings: &["name"], binds: false },
    // Whether Amenbo can read what is installed at all — the one way to leave a write's delivery
    // standing. Delivery rides along with the write that caused it, so anything a scenario writes is
    // carried out before the next step: a push by hand has something to carry only where that drive
    // never happened. Amenbo skips it when the installed plugins will not read, since it will not walk
    // its cursor past events a subscriber list it could not resolve was never offered — so the event
    // stays where the write appended it, queued to nobody, with no runner started. `readable` is both
    // halves: `false` leaves the next write undelivered, `true` gives the directory back, which
    // whatever reads or delivers afterwards needs.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "installed-dir", required: &["readable"], refs: &[], strings: &[], binds: false },
    // What an installed plugin is told, for one crossing. `key` is a setting its author declared; an
    // empty value is how one is taken back, which is why it is a value here and not an op of its own.
    //
    // `project` is the crossing the value belongs to. A setting is held per project, and a terminal
    // says which project it is writing for by standing in a folder bound to it — there is no flag for
    // it — so the driver stands in that project's folder before it types. Naming none is the folder
    // the run itself works from, which is bound to nothing and so answers to the store's default
    // project: the right silence for a road that only needs a value somewhere, and the wrong one for a
    // road whose crossing is named elsewhere, where a write nobody placed lands out of its sight.
    // A screen never names it: the row a form is opened inside has already answered which crossing is
    // being written, so the GUI driver turns the word away.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "config-set", required: &["name", "key", "value"], refs: &[], strings: &["name", "key", "value", "project"], binds: false },
    // A catalog of the run's own, answering on the loopback for as long as the scenario lasts.
    // Registering one is a trust decision taken on the key it publishes beside its `catalog.json`,
    // and a key is only published by something that answers on a port — no URL a scenario can write
    // down serves one, so the run stands the catalog it is about to trust.
    // `publishes_key` is the trust half: a catalog that publishes none is the other side of the rule,
    // browsable and uninstallable. `offers` is the shelf — the rows this catalog's own document
    // carries, each written as the words that document holds (`name`, `desc`, the `claims_official`
    // badge it is not entitled to, the `about` its author describes it at length in, and the one
    // `setting` its author declares, under the `label` a form shows). Naming none is an empty shelf,
    // which is what a road about the trust root alone wants. It is the only arg written as a list of
    // rows, and the loader checks it as one.
    // A row may also carry `translated` — the same `desc`, `about` and `label` as its author wrote
    // them in other languages, keyed by language code. The three are then published the way a real
    // catalog publishes them: the lines beside the list as one document per language, the description
    // text and the labels inside the row's own detail document, every language at once. A language no
    // row drew a *line* in gets no document, which is the 404 a reader of an untranslated language
    // meets — a row translated at length alone leaves none, since that half never travelled that way.
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
    // The same switch for a plugin its author declared the machine's: it crosses no
    // project, so its row is the device's own and there is no name to give one. Pressing it is the
    // consent to let the plugin read every project on the machine — one act, because the declaration
    // already settled what the one switch means.
    //
    // A step of its own rather than `enable-in` with the project left out, for the reason the two rows
    // are different rows: a road that named no project would be read as one that forgot to.
    //
    // A screen road alone, and not by omission: a terminal moves this same gate with `plugin enable`,
    // which needs no word for the layer at all — the declaration picks it. A screen has two kinds of
    // row and which one is pressed is the whole question, so the word is the screen's to need.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "enable-on-device", required: &["name"], refs: &[], strings: &["name"], binds: false },
    // The settings of that same row, opened inside it. It is `open-config-in-row`'s sibling and exists
    // for the same reason: a form reached from the row needs no layer answered, and one reached anywhere
    // else would be asking what the row has already said.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "open-config-on-device", required: &["name"], refs: &[], strings: &["name"], binds: false },
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
    // The operations an author put on that same form, as a screen has them. `press` presses the button
    // drawn under the words it was declared with — which raises the call outright where it asks for
    // nothing, and otherwise opens the boxes it asks for, empty every time; `press-answer` fills the one
    // box and lets the run go, which is the second half of a press and a move of its own.
    //
    // A screen road alone, for the reason `config-choose` is one: a terminal reaches the author's code
    // with `plugin run`, which names the call itself and hands it whatever arguments were typed. What is
    // under test here is the door that does neither — the press chooses among the calls the manifest
    // declared, and the value it needs is asked at the press and kept nowhere afterwards.
    // What the author asked to have drawn, read off the form. `kind` is which part, and
    // `value` the string it carries where an eye can read one back — the words on a `link`'s button, the
    // line a `text` is, the address beside a `copy`. A `qr` names none: what is on the screen is a
    // picture, and the whole claim is that Amenbo drew it rather than the author handing one over.
    //
    // `above` names a setting this part has to stand over, and it is the claim the manifest half of the
    // vocabulary exists for: where a part sits is what it is for. A way to the page that issues a token
    // belongs over the box the token goes in, and a build that drew every part in a block of its own —
    // before the fields, or after them — would pass a read of the words and lose the whole point of
    // writing one into a manifest.
    //
    // A screen road alone, for the reason the press it stands beside is one: none of this reaches a
    // terminal. `plugin run` hands a caller the plugin's stdout verbatim and draws nothing, and the
    // author's words are the settings form's alone — they reach the screen and nowhere else.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "drawn", required: &["name", "kind"], refs: &[], strings: &["name", "kind", "value", "above"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "press", required: &["name", "label"], refs: &[], strings: &["name", "label"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "press-answer", required: &["name", "label", "value"], refs: &[], strings: &["name", "label", "value"], binds: false },
    // Asserts
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "listed", required: &["filter"], refs: &["target"], strings: &["filter", "position"], binds: false },
    // A listing that is **turned away** rather than answered. It is a verdict of its own and not a
    // `refused:` on the line above, for the reason `refused` is an action's word: a listing already
    // comes back with an answer, and "refused" is not one of the answers it can come back with. The
    // two say different things — an empty page is "nobody carries that value", a refusal is "that
    // question does not run here" — and a road that could only write the first could not tell them
    // apart. `code` is the error code the refusal has to carry, so a line written against one guard
    // cannot pass on another's.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "filter-refused", required: &["filter", "code"], refs: &[], strings: &["filter", "code"], binds: false },
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
    //
    // The question is `words`, or `number_of` naming a record whose **number** is asked for instead —
    // one of the two (see `takes_a_query`), and `spelled` is `bare` or `hash`. A number is a question
    // no scenario can write as words, the store being the one that issues it, and it is a different
    // question from a word besides: the record it names is put at the top of the answer whether or not
    // anything is written on it. `first` is that top — a step naming it asks not only that the record
    // is among the hits but that it is ahead of what the words matched.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "found", required: &["target"], refs: &["target", "project", "number_of"], strings: &["words", "spelled", "face", "only_face", "kind", "filter", "standing", "landed_on", "marked"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Decision, op: "found", required: &["target"], refs: &["target", "project", "number_of"], strings: &["words", "spelled", "face", "only_face", "kind", "filter", "standing", "landed_on", "marked"], binds: false },
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
    // What a narrowing left standing. Separate from `listed` because there is no filter to write it
    // as: the narrowing is the screen's own, and the question it answers is which of the cards drawn a
    // moment ago are drawn still. What did the narrowing — words typed over the board, or values chosen
    // on its axes — belongs to the move that did it; repeating it here would be the one place a step and
    // the move in front of it could disagree, so one assert answers for both.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "narrowed", required: &["target"], refs: &["target"], strings: &[], binds: false },
    // The values folded away, and what the control standing in their place says while they are gone.
    // `axes` is how many of them are narrowing — a count and not a list, since what a folded control
    // has room to say is a number. It is its own assert rather than a reading of the board: a narrowing
    // still in force with its values out of sight looks like tasks that are simply gone, so what the
    // count is worth proving against is the fold itself.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "filters-folded", required: &["axes"], refs: &[], strings: &[], binds: false },
    // Whose record the press opened. The title is no witness — the hit row carries it too — so the step
    // names a phrase only the record's own face holds, and `present: false` puts the same question to a
    // record that was not the one pressed.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "opened", required: &["target", "shows"], refs: &["target"], strings: &["shows"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "field", required: &["target", "field", "equals"], refs: &["target"], strings: &["field"], binds: false },
    // Which folder the task says it is worked in — read as a folder rather than as a value, which is
    // why it is not a `field` path: what the task holds is a binding, and what it answers with is the
    // path that binding points at, a path no road can write down (the driver is the one that places
    // its folders). `dir` names the folder the way every `folder` step does; `present: false` is the
    // other half and needs no name, since a task holding none has nothing to name.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "worked-in", required: &["target"], refs: &["target"], strings: &["dir"], binds: false },
    // Whether the task names a pane its work is happening in, and which one. `shows` is the pane's own
    // name — the line a road typed into it — because that is what the row carries and what tells one
    // pane from another.
    //
    // **`present: false` is the half this exists for.** The row is drawn only where a session is
    // holding the task *and* a pane is drawing that session, so it is absent for a reservation made in
    // somebody's own terminal and absent again once the pane has closed — while the reservation itself
    // stands. A road pairs the absent half with a reading of the status, which is what
    // says the ledger is answering "no pane here" rather than "nobody".
    //
    // A screen road alone. The row is a way to press, and a terminal has nowhere to press it to.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "pane", required: &["target", "shows"], refs: &["target"], strings: &["shows"], binds: false },
    // Which of a task's own controls the face draws, and which it does not. `opened` asks
    // *whose* record is standing there and answers it with a phrase off the record's face; this asks
    // what that face **offers to press**, which no phrase witnesses — a control that was taken away
    // leaves no word behind to look for, and the word its absence would be read by is on every other
    // card besides.
    //
    // `control` names one of the four (`status`, `drag`, `finish-creating`, `delete`) and `where` says
    // on which of the two faces a task wears them, the card or its own page. Both halves are worth a
    // step: `present: false` is the guard, and `present: true` beside it is what keeps a build that
    // drew nothing at all from reading green.
    //
    // A screen road alone — a terminal offers no controls, only commands, and what it refuses is
    // already `status`'s `refused:`.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "offers", required: &["target", "control", "where", "present"], refs: &["target"], strings: &["control", "where"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Decision, op: "field", required: &["target", "field", "equals"], refs: &["target"], strings: &["field"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Decision, op: "listed", required: &["filter"], refs: &["target"], strings: &["filter", "position"], binds: false },
    // The row the panel left standing, and the fold it folded into — the decision-side twins of the
    // board's `narrowed` / `filters-folded`, and separate for the same reason the moves are. `listed`
    // above is the terminal's answer to the same question: it carries the narrowing as a `--filter`
    // line, where this one is read off the list a press already narrowed.
    OpSpec { kind: Kind::Assert, domain: Domain::Decision, op: "narrowed", required: &["target"], refs: &["target"], strings: &[], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Decision, op: "filters-folded", required: &["axes"], refs: &[], strings: &[], binds: false },
    // The same verdict on the other face, and the one the pair is most often written for: an axis
    // narrowed to tasks is refused here, and an axis narrowed to decisions is refused there.
    OpSpec { kind: Kind::Assert, domain: Domain::Decision, op: "filter-refused", required: &["filter", "code"], refs: &[], strings: &["filter", "code"], binds: false },
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
    // word that must not be in them — the one question about a file Amenbo hands out that needs no
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
    // Whether anything in this folder starts its AI on Amenbo at session start (`wired`), and — while
    // nothing does — which provider the folder is told about by name (`tool`). The two are one
    // question asked from either end: the answer Amenbo carries on every response until the paste
    // lands, and the silence that follows it.
    //
    // `wired` is the whole vocabulary here. A folder is wired or it is not: whether the hook then
    // fires, and whether what it injects reaches the model, is outside Amenbo, so nothing here says
    // enabled and nothing says it works.
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "ai-launch", required: &["wired"], refs: &[], strings: &["tool"], binds: false },
    // The same report read on one tool's own row (`tool`), and whether that row says wired (`wired`).
    // `ai-launch` above answers for the folder as a whole, and names only what Amenbo can point at —
    // a provider whose directory is right there. This one answers for a provider the folder shows no
    // sign of, which is the reader that opens a folder somebody else wired with a tool of their own:
    // it knows which harness it is where nothing in the folder does, and its own row is the only place
    // the answer is. Where a row says unwired it has to carry the way to that tool's text as well, an
    // answer that leaves the reader unable to act being no answer.
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "ai-launch-tool", required: &["tool", "wired"], refs: &[], strings: &["tool"], binds: false },
    // The text handed over to make that happen: what it carries (`carries` — the launch instruction,
    // which is the one part of it that is not the provider's own shape) and the file it says to put
    // it in (`paste_into`). What is under test is the handing over, since Amenbo's whole part in this
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
    // The text itself, read on the project's own settings rather than on a report: the tool it is for
    // (`tool`) and the file it goes into (`paste_into`), the same two halves `ai-launch-notice` reads
    // and the same file carrying the reading.
    //
    // It is a separate op because what it stands against is the report being the only way there. Every
    // other face of this hangs on the notice, which falls silent the moment a folder is wired — and the
    // reader who most needs the text is the one who wired a tool and then changed to another. So this is
    // read where nothing is being reported, which is a state `ai-launch-notice` cannot be asked in: its
    // own line names a report, and `present: false` there is the absence rather than a way in.
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "ai-launch-request", required: &["tool", "paste_into"], refs: &[], strings: &["tool", "paste_into"], binds: false },
    // One app's row on the screen `mcp-open` opens: whether that app already reaches Amenbo at all, the
    // folder its entry names when it does, and — where the road is about them — the projects standing
    // ticked on it.
    //
    // `projects` is the row's own opening state, which is read off the settings rather than remembered:
    // a reader who has set this app up before has to arrive at their own selection, and a build that
    // opened every row empty would have them rebuild it before touching anything. It is a list because
    // the selection is one, and the reading is exact — a project ticked that the entry does not reach
    // is as wrong as one missing.
    //
    // The folder is a second word rather than part of the first because the two are separately true:
    // an app is set up for *some* folder, and which one is the half a reader cannot work out — an entry
    // pointing at a folder nobody works in is drawn exactly like one pointing at the right folder. A row
    // saying "set up" with nothing after it is still the truth, so `dir` is named only where the road is
    // about which folder was named.
    //
    // `set` alone is a `Review`: both answers are words of the interface, and which of them is standing
    // is not something the presence of text can settle. Named with a `dir`, the folder is the reading —
    // it is the reader's own path, which the interface has no word of its own for.
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "mcp-app", required: &["app", "set"], refs: &[], strings: &["app", "dir"], binds: false },
    // Which of the two roads that same row offers: the file Amenbo writes for the one app that cannot
    // run a command, or the request handed to the AI every other app has of its own. One row draws one
    // of them, and which it is is the catalog's word rather than the screen's.
    //
    // It is read rather than pressed because pressing is where the road would end anyway — a request
    // goes to the clipboard, which no shot reads back, and a file goes wherever the reader picks. What
    // is under test is that the two rows are not offered the same thing, which is what a build folding
    // both roads into one button would break in silence.
    //
    // A `Review`: what separates the two is the label on a button, and a label is a word of the
    // interface.
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "mcp-road", required: &["app", "road"], refs: &[], strings: &["app", "road"], binds: false },
    // That same road, read while the row has nothing ticked. What is ticked *is* what goes over — the
    // file names those folders and the request carries them — so an empty selection has nothing to hand
    // anybody, and the road has to say so before it is pressed rather than after: a request handed over
    // naming no folder is written into another app's settings as an entry that cannot run, and by then
    // it is in a file Amenbo does not own.
    //
    // It takes `road` for the same reason `mcp-road` does: which button a row carries is the catalog's
    // word, so the step names the one it means by what that button does.
    //
    // A `Review`, like the other way out drawn shut: what separates a shut button from a live one is
    // paint, which leaves no text on a shot either way. A screen road alone — a button is a face, and
    // the CLI has none.
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "mcp-road-shut", required: &["app", "road"], refs: &[], strings: &["app", "road"], binds: false },
    // Ticking a row's projects and taking the road it offers — the one move on this screen that writes
    // anything. What goes out is the **whole** selection every time, so the step names every project
    // that is to be ticked and says the rest are to be left clear: a build that added to what was there
    // instead would pass a road that only ever named one.
    //
    // It is an action rather than an assert because what it produces leaves Amenbo — a request goes to
    // the clipboard and a file goes wherever the reader picks — so what is read afterwards is the app
    // it was carried into (`mcp-in-app`) and the row itself, read again.
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "mcp-choose", required: &["app", "projects"], refs: &[], strings: &["app"], binds: false },
    // Where that file ends up: the app itself, with Amenbo standing among its servers and a tool of
    // Amenbo's under it. One app only, and the one the file road is for — it is the single app whose
    // settings Amenbo writes with nobody in between, so a format it got wrong is Amenbo's fault and
    // nothing catches it short of the app reading the file. Every other app is handed a request its own
    // AI carries out, and what Amenbo owns there is the wording, which a harness can hold up on its own.
    //
    // Named tool by tool for the reason the protocol road names them so: what a road is about is one
    // tool being reachable, and the whole set would fail on a tool nobody was asking about.
    //
    // Not `Domain::Mcp`, though this is the protocol coming up: that domain is the harness speaking it
    // — a server it stood and calls it made over two streams — and here the speaking is the app's. What
    // is read is another program's screen, which nothing in this workspace drives, so the step belongs
    // beside the fold that wrote the file rather than beside the calls.
    //
    // It is a `Review`, and further out than the others: the reading behind an OCR verdict is taken off
    // a shot of the build under test, and the window worth reading here is a different program's. The
    // instruction asks the attending AI for that shot, which is the evidence the step leaves.
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "mcp-in-app", required: &["app", "tool"], refs: &[], strings: &["app", "tool"], binds: false },
    // A project as it is read back: one row's fields, and whether it is in the listing at all (and
    // where). `archived: true` asks the listing that carries the archived ones.
    OpSpec { kind: Kind::Assert, domain: Domain::Project, op: "field", required: &["target", "field", "equals"], refs: &["target"], strings: &["field"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Project, op: "listed", required: &["target"], refs: &["target"], strings: &["position"], binds: false },
    // Whether the project draws the image it was given, read on the face it was given one from. The
    // absent side is not a blank: with no image a project draws its colour with the first letter of its
    // name on it, so the instruction names that fallback rather than asking an eye to find nothing.
    //
    // A screen road alone, and a `Review` it could be nothing else than: what stands on the shot is a
    // picture, and a reading answers which words are on one.
    OpSpec { kind: Kind::Assert, domain: Domain::Project, op: "icon", required: &["target"], refs: &["target"], strings: &[], binds: false },
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
    // `device` is the fourth, and it is not a state of a crossing at all: a plugin its author declared
    // the machine's crosses no project, so this face names it and offers nothing to press.
    // It is told apart from `absent` because the two differ where it matters — an absent row is one the
    // picker here offers to draw, and this one is not on that list and never will be.
    //
    // A screen road alone, and a `Review` like `fires-in`: whether the plugin is on here is drawn as a
    // button, and a button's label is a word of the interface — so what separates `drawn` from `firing`
    // is not something the presence of text on a shot can settle.
    OpSpec { kind: Kind::Assert, domain: Domain::Project, op: "plugin-row", required: &["project", "plugin", "state"], refs: &[], strings: &["project", "plugin", "state"], binds: false },
    // Whether the axis is offered as a way to cut the board into columns. Not whether it is defined —
    // `dimension listed` asks that, and an axis that admits several values at once is defined exactly
    // as much as ever. What it is not is a way to say where a task *is*: a task
    // answering three of its values would stand in three columns at once, so the picker leaves it out
    // while the filter chips go on offering it.
    //
    // A screen road alone: a terminal has no columns, which is the same reason `project group-by` is.
    OpSpec { kind: Kind::Assert, domain: Domain::Project, op: "groupable", required: &["axis"], refs: &[], strings: &["axis"], binds: false },
    // Whether the board, cut along that axis, draws a column for one of its values. Not whether the
    // value is defined — `dimension listed` asks that — but whether the board keeps a place to stand a
    // card in. A closed value is where the two come apart: it keeps its column while cards are still
    // in it and loses it once the last one leaves, which is the one reading that says the board
    // neither takes filed work off itself nor grows columns nobody can drop into.
    //
    // A screen road alone, like the `group-by` that cut the board and the `groupable` that reads the
    // row of buttons: a terminal answers with a listing, and a listing has no columns.
    OpSpec { kind: Kind::Assert, domain: Domain::Project, op: "column", required: &["axis", "value"], refs: &[], strings: &["axis", "value"], binds: false },
    // An axis as it is read back, by name: is it defined, and does it carry the value named?
    //
    // `side` asks a different question of the same listing — not whether the axis is defined but
    // whether the side named is offered it at all — and an axis narrowed off a side is defined exactly
    // as much as ever, so the two answers come apart there and nowhere else. `target`
    // rides with it for the face that has no listing to read the offer off: a screen reads it as the
    // control a record's own pane keeps per axis, so the road names the record whose pane is opened.
    //
    // A `value` on that face narrows the reading one step further, to whether the control offers that
    // value as an answer — which is the same question the terminal's listing answers, a closed value
    // being left out of both. The record is what the two come apart on: the pane goes on drawing a
    // closed value the record already carries, so a road reads the picker twice, once on a record
    // holding the value and once on a record that would newly take it.
    OpSpec { kind: Kind::Assert, domain: Domain::Dimension, op: "listed", required: &["dimension"], refs: &["target"], strings: &["dimension", "value", "side"], binds: false },
    // Whether one of the axis's values is closed. `listed` above asks what the axis still offers, and
    // a closed value is exactly what has stopped being offered while staying on the axis — so the two
    // together are what separates a value retired from one deleted, which neither says alone.
    // `equals: false` is the same reading after the way back was taken.
    //
    // The value has to be on the axis: one that is not there is a step naming the wrong value, not a
    // value that is open, and it is called that rather than answered — the line `key` draws too.
    OpSpec { kind: Kind::Assert, domain: Domain::Dimension, op: "closed", required: &["dimension", "value"], refs: &[], strings: &["dimension", "value"], binds: false },
    // The key an axis answers to, or one of its values where `value` names one. Read apart from
    // `listed` because it is a different question: that one asks whether the axis is defined at all,
    // and a row whose key was quietly left as its id-derived default is defined exactly as much as one
    // somebody named.
    OpSpec { kind: Kind::Assert, domain: Domain::Dimension, op: "key", required: &["dimension", "equals"], refs: &[], strings: &["dimension", "value", "equals"], binds: false },
    // What a card says about how its task is classified — the value carried on one axis, read off the
    // board with nothing opened. A screen road alone: a listing has no card, and the question is
    // about a surface rather than about the filing, which `dimension listed` and the
    // `dim:` filter already answer. The axis is named beside the value because whether the card draws
    // it at all is the axis's answer, so a step that named the value alone would not say what is
    // being asked of.
    //
    // `grouping: true` says the axis named is the one the board is currently cut along (`project
    // group-by` put it there). It changes how the step is judged rather than what it claims: the value
    // is standing on the column heading whatever the card does, so a reading of the shot would find the
    // word either way, and an eye closes this one instead.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "carded", required: &["target", "dimension", "value"], refs: &["target"], strings: &["dimension", "value"], binds: false },
    // Which bucket of the "what to do now" view a task lands in (`overdue` / `due_today` /
    // `in_progress`) — the view is assembled from days, so the bucket is not the task's status field.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "status-bucket", required: &["target", "bucket"], refs: &["target"], strings: &["bucket"], binds: false },
    // What a smart view's row says before anyone opens it: how much stands on one of the steps it warns
    // on, read off the badge beside its name. `step` is the ladder's own word for what a colour asks of
    // the reader (`stop` / `heed`) rather than the colour itself, so a road is written against a
    // meaning and not against a palette. `count: 0` is the other half of the claim — a step with
    // nothing on it draws no badge at all, which is what keeps a quiet row quiet.
    //
    // A screen road alone. Being told without asking is the whole subject, and a terminal is only ever
    // asked; what it answers when asked is `status-bucket`'s.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "view-warns", required: &["view", "step", "count"], refs: &[], strings: &["view", "step"], binds: false },
    // And what that view holds once it is opened. Apart from `listed`, whose filter the road writes
    // out: a smart view carries a selection of its own, so what is under test is that selection
    // agreeing with the warning the row gave — never a filter the road could have spelled to match.
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "view-lists", required: &["target", "view"], refs: &["target"], strings: &["view"], binds: false },
    // The three faces the store shows of itself, each a dotted path into what that read prints:
    // `config` its settings, `identity` the name and the hardware it was raised on, `update` what a
    // check for a newer build comes back with.
    OpSpec { kind: Kind::Assert, domain: Domain::Store, op: "config", required: &["field", "equals"], refs: &[], strings: &["field"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Store, op: "identity", required: &["field", "equals"], refs: &[], strings: &["field"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Store, op: "update", required: &["field", "equals"], refs: &[], strings: &["field"], binds: false },
    // Which of the two a facet's slot holds: the image registered for it, or the pattern Amenbo draws
    // for a face that has none. The absent side is not a blank, so the instruction names that pattern
    // rather than asking an eye to find nothing.
    //
    // A screen road alone, and a `Review` it could be nothing else than: both sides are a picture, and
    // a reading answers which words are on a shot.
    OpSpec { kind: Kind::Assert, domain: Domain::Store, op: "avatar", required: &["facet"], refs: &[], strings: &["facet"], binds: false },
    // Whether a folder is bound, asked from inside it — and, with `project`, which one it names.
    // `resynced` asks the other half: whether the guidance block there is at this build's version,
    // which is answered by a resync finding nothing left to write.
    OpSpec { kind: Kind::Assert, domain: Domain::Folder, op: "bound", required: &["dir"], refs: &["project"], strings: &["dir"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Folder, op: "resynced", required: &["dir"], refs: &[], strings: &["dir"], binds: false },
    // The other side of the same relation: not what the folder says, but what a **project** says about
    // its folders — the list its own settings offer, which is where a person goes to open one. The two
    // readings are worth keeping apart, because a folder re-pointed at another project is the one case
    // where they can disagree: the pointer names the new project the moment it is written, while the
    // list is an index that has to be kept up with it. `present: false` is that half — the project the
    // folder left no longer offering a way into it.
    OpSpec { kind: Kind::Assert, domain: Domain::Folder, op: "listed", required: &["dir", "project"], refs: &["project"], strings: &["dir"], binds: false },
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
    // What Amenbo says in a folder whose project has one that vanished: it stops, rather than going
    // quietly to work in whatever is left — and the answer lines the gone bindings up **by id**,
    // beside the command that re-points each of them.
    //
    // That listing is the whole door. An id is a number the store assigns and publishes nowhere else,
    // so a reader who was never shown it cannot pass it, and neither can anything else: this assert is
    // also where the road learns the number its `rebind` then hands back. `dir` is the folder the
    // question is asked from — any folder of that project that is still there, the moved one's new
    // home included — and `gone` names the folder whose old path has to be in the answer, so it
    // follows that folder's `move`.
    OpSpec { kind: Kind::Assert, domain: Domain::Folder, op: "vanished", required: &["dir", "gone"], refs: &[], strings: &["dir", "gone"], binds: false },
    // The binding as the re-point answered for it: the id it kept, the folder it names now (`dir`),
    // and the path it named before (`previously`, the folder that moved).
    //
    // Read off that answer and not off the store, for the reason `folders-left` is: a binding's id is
    // in no read Amenbo offers, so the state left behind is the same shape whether the row moved or a
    // new one was recorded under a new number. What tells the two apart is the answer saying which
    // row it was, and it says it once. So this has to **follow its `rebind`**.
    OpSpec { kind: Kind::Assert, domain: Domain::Folder, op: "repointed", required: &["dir", "previously"], refs: &[], strings: &["dir", "previously"], binds: false },
    // A folder another store claimed, met from a build that is not that store: the work stops, and the
    // answer names the store the folder belongs to — which binary to run instead is the reader's to
    // know, and nothing else on screen or on the wire says it.
    //
    // Refusing is the line under test rather than the reading. What stood here before was a warning,
    // and a warning is answered by the write that follows it: by the time anyone reads the sentence,
    // the command has already gone to work in the wrong project. So the verdict is taken on the door
    // being shut — the guard is asked before a store is opened at all — and the store's name is read
    // off the refusal, since a refusal that will not say whose folder this is leaves the reader
    // nowhere to go.
    //
    // `dir` is the folder the question is asked from, so this follows that folder's `foreign-pointer`,
    // and `store` is the name that has to be in what comes back.
    //
    // On screen there is no command to turn away — the interface holds its own store open the whole
    // time — so what answers for the same line is the row that lists the folder: it names the store
    // the folder belongs to, and stops calling the folder one an AI can be started in. The two roads
    // meet at the naming, which is the half a reader can act on either way.
    OpSpec { kind: Kind::Assert, domain: Domain::Folder, op: "claimed", required: &["dir", "store"], refs: &[], strings: &["dir", "store"], binds: false },
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
    // reader asks their AI, the AI writes to Amenbo, and what it wrote lands on the board. Every move
    // the interface can make on their behalf it makes, so the whole of it is one press: a terminal
    // opens inside the linked folder and the agent in it is handed the request before it starts.
    //
    // `hands_over` is the words that request has to carry, and what they name is the command the AI is
    // sent to run before it does anything else. It is the one part of the card that is the same in
    // whatever language the app is in, so it is the part a reading can be held to — and it is read off
    // the way out to the reader's own terminal, which is where the request is written down now that
    // nobody on the one press has to paste it.
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
    // The press itself — the one move the loop offers, taken rather than read. It goes to the
    // terminal face with a pane already open in the linked folder, which is the whole of what the
    // loop promises and the one part of it no reading of the card reaches.
    //
    // It carries nothing. Which folder the pane works in and which project it belongs to are both
    // the card's own to know — the card is a project's, and the folder is the one it names above the
    // press — so a step that named either would be handing the screen an answer it is under test for
    // having. What the pane landed under is read afterwards, off the face itself, by the roads that
    // read any other pane (`terminal go-project`, `terminal pane`).
    OpSpec { kind: Kind::Action, domain: Domain::Folder, op: "start-terminal", required: &[], refs: &[], strings: &[], binds: false },
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
    // wording is the author's and they may change it any day, while where it is readable is Amenbo's
    // and is the whole of what the split between a colleague's plugin and a stranger's decides.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "listed", required: &["name"], refs: &[], strings: &["name"], binds: false },
    // The layer its author declared, said in words on the row where the plugin is managed. What one
    // press of a gate consents to comes from that declaration and from nothing the reader sets, so the
    // sentence is the whole of how they learn it: a plugin declared the machine's reads every project
    // on the device. `scope` is which of the two the row has to be saying — `machine` for the sentence,
    // `project` for the ordinary case, which says nothing because there is nothing out of the ordinary
    // to say.
    //
    // GUI only, and not by omission: what a manifest declares is already readable from a terminal, and
    // that read is exactly what a build which never drew the sentence would leave untouched — so the
    // screen is the only witness that the declaration reached the person pressing the switch.
    //
    // A `Review`, for the reason `settings-in` is: the sentence is a word of the interface, in whatever
    // language the app is in, and the plugin's own name is on the row either way — so neither state is
    // one the presence of text can settle.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "layer", required: &["name", "scope"], refs: &[], strings: &["name", "scope"], binds: false },
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
    // The same reading for the one gate a machine-wide plugin has. It names no project
    // because there is none to name: what is open is the device's, and reading a project list for it
    // would answer "nowhere" for something firing on the whole machine.
    //
    // A screen road alone, and not by omission: what `plugin list` reports of this gate is a line of
    // text a build could keep answering while the row that moves it was never drawn — which is the state
    // this exists to close.
    //
    // A `Review`, for the reason `fires-in` is one tier along: what tells an open gate from a shut one is
    // the word on the button standing in the row, which is a word of the interface.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "fires-on-device", required: &["name", "present"], refs: &[], strings: &["name"], binds: false },
    // What that same row says about the settings kept there — `settings-in`'s sibling, with the same
    // three states and for the same reasons: `required-empty` is the mark worn before anything is
    // pressed, `open` is the form standing inside the row asking for no layer, and `filled` is the row
    // saying the value is in.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "settings-on-device", required: &["name", "state"], refs: &[], strings: &["name", "state"], binds: false },
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
    //
    // `readonly: true` is the one reading only a screen has, so the CLI driver turns it
    // away rather than passing over it: what it asks is that the value is shown with no box to type in
    // and no button to take it back, and a terminal has neither to withhold. It reads `equals` as the
    // value that has to be standing there — the point of the reading is the value being there and being
    // out of reach, which an empty field would prove for the wrong reason.
    // `holds` is the reading only a screen has beside `readonly`, and it is a word of its own rather than
    // the `equals` a choice takes: what it asks is that a typed line is standing in its box, which is what
    // a road wants after something that could have taken it away — a check that refused the values a save
    // had already written. A terminal reads that same value with `equals` and has no box to draw it in, so
    // the CLI driver turns this one away rather than answering a question it was not asked.
    //
    // `project` names the crossing the value is read from, and it is the same word `config-set` takes,
    // for the same reason: down this pipe the read is a command typed somewhere, and where it is typed
    // is what decides which project answers. A road that named the crossing on the way in and left it
    // unnamed here would be asking the default project about a value it was never told.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "config", required: &["name", "key"], refs: &[], strings: &["name", "key", "state", "holds", "project"], binds: false },
    // The other half of that same form, asked apart from it the way a row's line is asked apart from
    // its badge: not what the field holds, but the words it is drawn under. `label` is those words,
    // written out — the author's, in whichever language they wrote them and the reader is in — and
    // `candidate` moves the reading one level down, to the words one of a choice's answers is drawn
    // under, named by the value it stores rather than by what it says.
    //
    // Quoted whole rather than asked about, for the reason the row's line is: what a reader is shown
    // when their language is untranslated is the author's base wording, unmarked, so the only thing
    // that tells the two apart is which of them is standing there.
    //
    // A screen road alone, and not by omission. A form is a screen; `plugin config` in a terminal
    // answers with values and never draws a label, and what it does print is English whatever the
    // reader's language says.
    //
    // `present: false` is the other half of the same reading, and the half a condition needs
    // a setting whose condition does not hold is not drawn greyed or drawn empty, it is not on
    // the form — so what proves the condition is that its words are nowhere on the screen. Absent, the
    // step reads the way every one of these read before there was anything to hide.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "asks", required: &["name", "key", "label"], refs: &[], strings: &["name", "key", "label", "candidate"], binds: false },
    // Whether the settings form is offering one of the author's operations at all. It is a
    // reading apart from `press-shut`, which is about a button that *is* drawn and cannot be pressed: a
    // condition that does not hold takes the button off the form, and "drawn but refusing" and "not there"
    // are the two states a reader has to be able to tell apart.
    //
    // A screen road alone, like the three presses below it: a terminal has `plugin run`, which takes any
    // call the plugin answers whether or not a form would have drawn a button for it.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "offers", required: &["name", "label", "present"], refs: &[], strings: &["name", "label"], binds: false },
    // The three readings a pressed operation leaves, each asked apart from the others because each is a
    // different promise. `press-said` is the line the run left on the form — the author's own words,
    // quoted whole the way a row's line is, since what a build could draw instead is Amenbo's own sentence
    // and nothing on the screen says which of the two is standing there. `press-asks` is the box in front
    // of that: the words the press asks under, and that it is holding nothing — which on a second press is
    // the whole of what "handed to this run and kept nowhere" looks like from outside. `press-shut` is the
    // button before the gate is open: drawn where it will be, and refusing the hand.
    //
    // A screen road alone, all three. What a terminal has instead is `plugin run`, whose answer is a
    // return value on stdout rather than a line beside a button, which asks for nothing at the press, and
    // which is refused with a code rather than by a control that cannot be used.
    // What the author's check said, where a reader meets it: one sentence at the head of the settings form,
    // and one beside each box the verdict named. `key` picks which of the two is being read — named, it is
    // the line under that setting; left out, the one over the whole form. Both are quoted whole for the
    // reason a row's line is: where the check said nothing Amenbo draws a sentence of its own in the same
    // place, and the screen does not say which of the two is standing there.
    //
    // A screen road alone. A terminal meets the same verdict as the reason an enable was refused, which is
    // read by the `refused:` code on the enable itself — a code being what a driver comparing exit statuses
    // can hold, and the author's sentences being deliberately kept off that face.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "checked", required: &["name", "text"], refs: &[], strings: &["name", "text", "key"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "press-said", required: &["name", "text"], refs: &[], strings: &["name", "text"], binds: false },
    // `press-asks` takes `secret: true` for the box an author marked a credential: what is read then is
    // the same emptiness plus the one thing a screen does about the flag — the characters are not drawn
    // back at whoever typed them.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "press-asks", required: &["name", "label"], refs: &[], strings: &["name", "label"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "press-shut", required: &["name", "label"], refs: &[], strings: &["name", "label"], binds: false },
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
    // The other question about that same row, asked apart from it: the one line drawn under the
    // name, and which language it is in. The badge says which shelf served the row; this says
    // whether what the shelf published in the reader's language is what reached them. One word
    // could not carry both, and a row badged right in the wrong language is exactly the state a
    // build breaks into.
    //
    // `desc` is the line the step expects, written out. It is the author's own sentence — not a
    // phrase of the interface — so a reading can be held to it, and holding it to the sentence
    // rather than to "is it translated" is the point: the fallback to the base line is silent by
    // design, so nothing on the screen distinguishes a line drawn in English because the author wrote
    // none from one drawn in English because the fetch never happened. What tells them apart is which
    // of the two sentences is standing there.
    //
    // A screen road alone, and not by omission: what a terminal prints is English whatever the
    // reader's language says, so the line this is about is drawn in one place.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "line", required: &["name", "desc"], refs: &[], strings: &["name", "desc"], binds: false },
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
    // The body of that same panel — the one block of prose a reader actually reads the plugin by. It
    // is one of two things and never both: the description its author wrote, in the reader's language
    // where they wrote one, or — for a plugin that describes itself nowhere — the README off the
    // repository, which is English whoever is reading.
    //
    // `text` is the words the step expects to find there, and `present: false` is how it says which of
    // the two is *not* standing. Both are needed, because the panel names neither: nothing above the
    // prose says whether it came from the catalog or from GitHub, and nothing marks a description drawn
    // in English because its author wrote no other. What tells all of that apart is which words are
    // there — so a road quotes them, and quotes the words it must not find beside them.
    //
    // GUI only, for the reason `detail` is: no CLI draws a catalog's detail document, and none fetches
    // a README at all.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "body", required: &["name", "source", "text"], refs: &[], strings: &["name", "source", "text"], binds: false },
    // What an AI is told about this plugin where it reads how to work in this folder — the `plugins` key
    // of the entry point. `present` is whether the plugin is offered there at all, which is the gate's
    // answer and not the install's: an installed plugin nobody switched on is one a call would refuse,
    // and naming it would spend a reader's turn learning what Amenbo already knew.
    //
    // `when` is the author's own line, read back to prove it is relayed rather than paraphrased. `cmd` is
    // the author's own command face, and what is checked is what Amenbo puts *in front* of it — the
    // calling form is assembled from the name read off disk, so what an AI receives is a line it can
    // type. The command word itself is left out of the step: a build reached by another name hands out
    // lines naming it, and that is the point rather than a mismatch.
    //
    // `because` is for the other half of the key: when nothing is offered, the entry point says which
    // empty-handed state this is, and a reader who cannot tell "nothing installed" from "nothing
    // switched on here" cannot tell which move would fix it. It is matched as a fragment rather than a
    // sentence — what is under test is that the right state is named, not today's wording, which Amenbo
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
    // The other half of the same document: a step of Amenbo's own working cycle, and whether this
    // plugin's call is hanging on it. The two shelves are kept apart on purpose — a step's body is
    // Amenbo's own and a plugin's sentences stay in its entry — so what crosses is the line to type and
    // the id the author named it by, and this is the reading that says the join really happened.
    //
    // `step` is that id (`<run>.<step>`, as the author writes it) and `cmd` the call's own face, since
    // what hangs there is the calling form Amenbo builds, not the bare subcommand. `present: false` is
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
    // Amenbo spoken to rather than typed at. A host starts one server for a set of folders, and
    // everything after that goes over the two streams — so the road is the protocol's shape rather
    // than the store's: stand a server, read what it publishes, call through it, read what came back.
    //
    // `serve` starts it for the folders named, and holds it up for the rest of the road — a server
    // dropped after the step that stood it would leave every later step talking to a closed pipe. It
    // also settles the handshake and takes the tool list once, which is what `offers` then reads: a
    // list fetched again per assert would be a second question, and the one worth asking is what this
    // server published when it came up.
    //
    // `dirs` is a list even when it holds one folder, because a set of one is what the server is
    // given — the spelling says so rather than leaving a road to imply it.
    OpSpec { kind: Kind::Action, domain: Domain::Mcp, op: "serve", required: &["dirs"], refs: &[], strings: &[], binds: false },
    // `call` is a tool called by name, in the folder `dir` names, with the words the caller sends
    // under it — `args` verbatim, the way `plugin run` carries a plugin's own. The folder is required
    // here because it is required there: every tool takes one and none of them defaults it, so a road
    // that left it out would be walking a call no host can make. Naming one the server was not given
    // is a road too — the answer is out of reach, and that is a document like any other. What comes
    // back is read by `answered`, for the reason every other returned document is read by an assert of
    // its own: a call's answer is a document, and the step that made the call has no verdict to give
    // about it.
    OpSpec { kind: Kind::Action, domain: Domain::Mcp, op: "call", required: &["tool", "dir"], refs: &[], strings: &["tool", "dir"], binds: false },
    // What the standing server publishes: one tool, present or absent. Named one at a time rather
    // than as a list, because what a road is about is a tool being reachable — and a road that named
    // the whole set would fail on a tool nobody was asking about.
    OpSpec { kind: Kind::Assert, domain: Domain::Mcp, op: "offers", required: &["tool", "present"], refs: &[], strings: &["tool"], binds: false },
    // And what the last call came back with. `ok` is the half a protocol error cannot say: a tool that
    // ran and refused is a *result* that is marked as an error, not a fault of the transport, so the
    // model reads the refusal instead of the host swallowing it. `contains` is the words that refusal
    // — or that answer — has to carry, since a road about a door being shut is about which door.
    OpSpec { kind: Kind::Assert, domain: Domain::Mcp, op: "answered", required: &[], refs: &[], strings: &["contains"], binds: false },
    // ── the hourly wake-up ────────────────────────────────────────────────────────────────────────
    // Being woken is the whole of what a tick does, and what it reports on the way past is the whole
    // of what can be read about it: the day mark it leaves behind has no face of its own, and asking
    // for one would be a command written for this harness rather than for a reader. So the wake is
    // the assert — it carries out one hour's turn and judges what came back, which is exactly what a
    // scheduler would have got.
    OpSpec { kind: Kind::Assert, domain: Domain::Tick, op: "woken", required: &["purpose", "carried_out"], refs: &[], strings: &["purpose"], binds: false },
    // What the run did to the registration the scheduler holds, read and never written. **No op here
    // registers one**: a registration is written outside the throwaway store this run makes — into
    // the launchd, systemd or Task Scheduler of whatever machine the gate is running on — and a road
    // that left one behind would leave an hourly timer on a release box. `be-offered-a-start-at-login`
    // draws the same line for the login registration. The half that registers is walked on the real
    // machines. `changed` is the difference across the run and not the machine's absolute answer,
    // because the throwaway store does not reach that far: on a machine where somebody uses the
    // hourly tick, a road asking whether one is held would read their registration and go red.
    OpSpec { kind: Kind::Assert, domain: Domain::Tick, op: "holds", required: &["changed"], refs: &[], strings: &[], binds: false },
    // The other half of the tick is the device's consent, and it is met on a screen:
    // a band across the whole app puts the question, and a row in Amenbo's own settings holds the
    // answer afterwards. Screen roads alone, the next four ops — the terminal's way in is
    // `tick install`, which asks nothing — so the CLI driver never meets them as steps.
    //
    // Whether the band is standing. It comes up only while three conditions hold together — the
    // device unanswered, a dated task still open, a plugin subscribed to `task.due` enabled
    // somewhere — so the `present: false` half is what a road reads after any one of the three has
    // gone, and the road that proves the gate is the one that takes a single condition away.
    OpSpec { kind: Kind::Assert, domain: Domain::Tick, op: "banner", required: &["present"], refs: &[], strings: &[], binds: false },
    // The answer given on it. It travels as a value rather than in the op's name, the way a consent
    // answer does everywhere else: `start` writes the yes, `never` the no, `later` no answer and the
    // day the question was put off to. All three are walked — unlike the login nudge, a yes here can
    // be taken back on the settings row before the run ends — but what a road may read after a
    // `start` is the answer having landed, never the machine's registration as an absolute: the
    // registration lives outside the throwaway store, so `holds` reads it as a difference across the
    // run, and that line holds here.
    OpSpec { kind: Kind::Action, domain: Domain::Tick, op: "banner-answer", required: &["answer"], refs: &[], strings: &["answer"], binds: false },
    // Where the settings row stands: `on` is a device that answered yes, `off` one that answered no
    // — or was never asked, the row having two positions over the answer's three states, since what
    // an unanswered device holds on the machine is what off already says. The row is the way back
    // the band does not give — a no is taken back here and nowhere else — so what a road reads off
    // its position is that the way back is standing where a reader would look for it.
    OpSpec { kind: Kind::Assert, domain: Domain::Tick, op: "setting", required: &["position"], refs: &[], strings: &["position"], binds: false },
    // And moving it. `on` answers yes, `off` answers no, and the registration moves with the answer
    // — written before the yes is kept, taken away with the no — so a switch that moved is a timer
    // that moved. What a road asserts after it stays on the answer's side of that line, as it does
    // after the band.
    OpSpec { kind: Kind::Action, domain: Domain::Tick, op: "set", required: &["position"], refs: &[], strings: &["position"], binds: false },
    // The band already put off — a premise (see PREMISE_OPS), not a screen move. "later"'s whole
    // meaning is one day of quiet, and the band is judged once at launch, so no press this run makes
    // can show either half of it: the same-day silence needs a launch *after* the press, and the
    // return needs a day to have passed — which, like the passage of time `store worn-in` stands up,
    // a road can only be given. `when` is `today` (pressed before this launch, the quiet still
    // running) or `yesterday` (pressed a day ago, the quiet spent); the band returns after one day,
    // so anything further back is the same world as `yesterday`.
    OpSpec { kind: Kind::Action, domain: Domain::Tick, op: "deferred", required: &["when"], refs: &[], strings: &["when"], binds: false },
    // ── the terminal face ─────────────────────────────────────────────────────────────────────────
    // Amenbo is one window with two faces and, for whoever wants them side by side, two windows.
    // Every op below is the screen's, bar the one premise that opens the block: what they are about
    // is where a running terminal is drawn, and a terminal is the one surface a reader is already
    // typing in — there is nothing for the CLI driver to walk, and no gap in it.
    //
    // The exception, and the reason it is one. What a pane can be opened *with* is not a record and
    // not a screen either: it is which agents this machine has installed, which the build asks by
    // running the pane's own login shell over the operator's `PATH`. So a road that means to read the
    // row of them has to be told what is on the machine before the app comes up, and this is the step
    // that says so. It stands programs up in a directory the app is launched with in front of its
    // `PATH` (`amenbo_verify_cli::domain::terminal`), which is a premise like any other — the world
    // standing before the road is walked — and is the screen's own moves' opposite.
    //
    // `count` is a **floor and never a ceiling**: nothing the harness hands the app can take an
    // install away from the operator's machine, so a road may ask for a row with more than one thing
    // on it and may not ask for one with less. That is the one shape worth standing up in any case:
    // where several can be started and nobody has chosen, the frame comes up asking, and that is a
    // state no machine's own `PATH` can be relied on to be in.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "can-start", required: &["count"], refs: &[], strings: &[], binds: false },
    // Which face the one window is showing. Pressed rather than arrived at: the segments are the only
    // way between the two, and a road that could not name which it pressed could not say which face
    // the assert after it read.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "show-face", required: &["face"], refs: &[], strings: &["face"], binds: false },
    // The way in. A face with no folder yet has one control on it, and pressing it is the whole of the
    // first run: the folder chosen is the one the AI is shown, it becomes a project's, and the pane
    // opens in it. It is an action rather than a premise because it is the road every other terminal
    // step stands on — a pane exists because somebody chose where it runs. `dir` is a name and not a
    // path, the way `folder bind`'s is: which folder the run works in is the run's to decide, and what
    // a road writes down is what to call the one it picked.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "open-folder", required: &["dir"], refs: &[], strings: &["dir"], binds: false },
    // Getting the pane to a plain shell — a terminal with no agent started in it. A folder with an
    // agent on it opens on one, which is what a reader wants and what a road cannot speak in: what an
    // agent does with a line typed at it is the agent's own, so a gate resting on one carrying out a
    // command rests on a promise nothing holds it to. Every road that says something *in* a pane
    // takes this step first, and what it says afterwards is said to a shell. One op rather than
    // three, because the shell is reachable from every shape the face can come up in — which is the
    // whole of what lets these roads be walked on any machine.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "open-shell", required: &[], refs: &[], strings: &[], binds: false },
    // A command of the reader's own, written down on the frame. The catalog is a
    // shortcut and not a census, so what a pane can be opened with is not only what Amenbo lists —
    // and this is the road that says so.
    //
    // **It is the one terminal op that may name a program**, and the reason is that the program is
    // the road's own. Every other reading here is written around not naming one (`opens-with`): which
    // agents are on the row is a probe of the run machine's `PATH`, so a road that asked for Claude
    // Code would run on the machines that happen to have it and nowhere else. A registered `line` is
    // not that — it is text the road wrote, judged by its first word alone, so a road may name
    // something every machine has and get the same answer on all of them.
    //
    // `line` is written with an argument in it or the road proves nothing. What is under test is that
    // the whole line reaches the shell as it stands: a line of one bare word would read alike whether
    // Amenbo handed it over or rebuilt it from the first word, which is the fault this exists to
    // catch.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "register-start", required: &["name", "line"], refs: &[], strings: &["name", "line"], binds: false },
    // Opening a pane on one of those. It names the row by the `name` it was registered under and
    // never by a line, because a name is what the row is drawn by and the line is what it runs — the
    // two are separate on purpose, and a road that pressed by the line would be reading the one place
    // the screen does not put it.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "open-registered", required: &["name"], refs: &[], strings: &["name"], binds: false },
    // A line typed into the pane and sent. `text` is the reader's own words rather than the
    // interface's, which is what makes it worth reading back: it is on the screen because a person
    // put it there, in whatever language the app is in, so a road can follow it from one window to
    // the other. It also names the pane's frame — the first line sent into a frame is what it is
    // called — which is how a road says *which* window it means once there are two.
    //
    // `shows` is which pane, named the way `remove-pane` names one: by the words a road typed into it
    // earlier. A pane just opened needs none — it is the only one on the page with nothing on it, and
    // the step before this one made it — but a road that comes *back* to a pane it left does, because
    // by then every box on the page has a terminal in it and "the pane" names three of them.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "type-line", required: &["text"], refs: &[], strings: &["text", "shows"], binds: false },
    // A file dragged in from outside and let go over a pane. It is the file face's `drop-in` aimed at
    // the other half of the screen, and it answers a different question: there the reader chose a
    // folder for it to land in, and here nothing lands anywhere — the file stays where it is and the
    // path it is at goes where the reader types.
    //
    // `brings` is a name rather than a path, for the reason the file face's is: a drop reads the disk
    // the operator is sitting at, and nothing a run lays down is anywhere a hand can reach from
    // there. Which pane is not named — a road drops on the one it just opened, and a road with two
    // says which by what it typed into them.
    //
    // `beside` is a second name carried in the same hand, and it is one arg rather than a second step
    // because what it is about is the gesture: several things picked up together and let go once put
    // every path on the line side by side, and two drops one after the other are two gestures that
    // happen to end in the same place. Only the first can say what a hand full of files does.
    //
    // `onto` is which pane, once saying so is the point: a drop lands where the pointer is, and on a
    // page with two panes that is a thing to prove rather than a thing to assume. Left out, it is
    // the page's one pane, which is what every road walked before there was a second one.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "drop-in", required: &["brings"], refs: &[], strings: &["beside", "brings", "onto"], binds: false },
    // What is on the machine's clipboard put into a pane's input line. It is the drop above made
    // with the keys instead of the hand, and it answers the half the drop cannot: a drop carries
    // files the operator picked up on their own machine, and a paste carries whatever the last copy
    // left — which is how a road follows one copy from the file face through to the pane.
    //
    // Nothing is sent. The line is left standing for `in-the-box` to read, for the reason
    // `hand-to-pane` leaves it standing: a path put in front of an agent is a path the person has
    // still to send.
    //
    // `onto` is which pane, named the way `drop-in` names one. Left out, it is the page's one pane.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "paste", required: &[], refs: &[], strings: &["onto"], binds: false },
    // What is standing in the pane's input line, **unsent**. It is not `pane` with a different
    // sentence: that one reads what a program printed, and this reads what nothing has run yet.
    //
    // The difference is the whole of what a hand-over owes. A path put in front of an agent is a path
    // the person still has to send, and a build that sent the newline for them would have answered
    // whatever the screen was asking at that moment — which on a first run is a question one of the
    // real answers to ran a script off the network. A reading that could not tell the
    // two apart would go green over exactly that.
    //
    // `on` is which pane's line is being read, named by the words a road typed into it. Left out, it
    // is the page's one pane — a road with two says which, the same way it says which one to type
    // into.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "in-the-box", required: &["shows"], refs: &[], strings: &["on", "shows"], binds: false },
    // A command run in the pane, and waited on until what it printed is drawn. It is not `type-line`
    // with a longer word in it: that step's line is the reader's own and is written to be *left* on
    // the screen — the shell is not meant to know it — and this is a program being asked for output
    // the steps after it read. The pane is cleared first, and that is part of the op rather than a
    // nicety: a road that pressed "the ref" on a pane still holding two earlier runs would be naming
    // one of three places on the screen, and only the operator would ever know which they took.
    //
    // `target` is there because a road cannot spell a number the run will mint. Where a command needs
    // a record's own ref, it carries `<ref>` and names the record beside it — the operator puts the
    // ref in, reading it off the same pane a step above had it on.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "run", required: &["command"], refs: &["target"], strings: &["command"], binds: false },
    // Pressing a ref where a program drew it in the pane. The record is named rather than spelled out,
    // for the reason every `target` is: what is on the screen is the run's own numbering.
    //
    // `folded: true` asks for that press on a ref the pane broke across two rows, which is the one
    // place the two ways of finding a ref part company. What Amenbo's own output says of itself
    // travels beside the characters and a fold cannot touch it; what is read back off the drawn
    // screen has to be joined across the fold before it can be found at all. A road that only ever
    // pressed refs sitting whole on one row would leave that joining unwalked, and nothing else
    // reaches it — a pane is narrow, a ref near the end of a line is ordinary, and the miss it would
    // hide looks exactly like characters that were never a link.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "press-ref", required: &["target"], refs: &["target"], strings: &[], binds: false },
    // Something set running in the pane and left running, which is the one thing this face has no
    // other way to reach. The line `type-line` types is a command no shell knows, on purpose, so what
    // it puts on the screen arrives once and is over — and a pane that printed once has already gone
    // still by the time a road can look at it. `text` is the line printed last, and it is the road's
    // own words for the reason `type-line`'s are: what says the run is over has to be something the
    // interface would never write by itself.
    //
    // It is not `run` with a longer command in it. That one is waited on — the step is over when
    // the prompt is back — and this one is walked away from while the output is still arriving,
    // which is the whole of the difference and the whole of what the mark it feeds needs.
    //
    // It runs out on its own rather than being stopped, and that is the whole of how the `out` face of
    // `dot` is reached on a pane that was lit a moment before. Every control a pane has is on the pane
    // — ending it, typing at it — so a road that cut the output short by hand would be pressing on the
    // very pane it is about, and what it read afterwards would be a lamp gone out because the road put
    // it out. Left alone, the same pane crosses from lit to out untouched.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "keep-printing", required: &["text"], refs: &[], strings: &["text"], binds: false },
    // Splitting the terminal out into a window of its own, and folding it back. Two ops rather than
    // one with a direction, because they are pressed in different windows: the way out is on the
    // face, and the way back is in the window it made.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "split-out", required: &[], refs: &[], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "fold-back", required: &[], refs: &[], strings: &[], binds: false },
    // What the pane is showing. `shows` is words a road put there itself with `type-line`, so the
    // reading finds them on the pane drawing that session and nowhere else — which is the whole of
    // how a road tells "the same terminal, moved" from "another terminal, started". The absent half
    // is what a road reads while the other face is up.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "pane", required: &["shows"], refs: &[], strings: &["shows"], binds: false },
    // Which pane of the page is the one being worked in, named the way every other pane step names
    // one. Two things are read at once because they are one fact to a reader: the frame drawn picked
    // out from the others, and the keyboard being in that pane rather than in the one they came from.
    //
    // The second half is read off the cursor and not by typing. A path handed to a pane is standing
    // in its input line unsent, so a character typed to find out where the keyboard is would be a
    // character added to the path — the road would be editing the very thing the step before it read.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "worked-in", required: &["shows"], refs: &[], strings: &["shows"], binds: false },
    // Ending the terminal in the pane. It is the only way out — a pane going away is a pane moving,
    // and the session outlives it — so it is also the only way a road reaches the state that follows
    // one: what a pane says once nothing is running in it any more.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "end-pane", required: &[], refs: &[], strings: &[], binds: false },
    // Getting rid of the place itself, which is the other control and the other outcome: the terminal
    // in it ends, the frame goes, the page closes up behind it, and the next run does not bring it
    // back. **It is the one move on this face nothing undoes**, which is why it asks before it is
    // carried out and why a road walks it at all — what a person has to be able to trust is that the
    // question stands between the press and the loss.
    //
    // `shows` names which pane by the words a road typed into it, the way the `pane` assert does. A
    // page has several and they carry nothing else a road put there, so a step that only said "the
    // pane" would leave the operator to choose one — and the whole of what follows is which one went.
    //
    // **The question is the plain one, whatever the session is holding**, so there is
    // nothing for a road to choose between and no task for it to name. What the volatile area says a
    // pane reserved is drawn on the label above it and is not a fact the ledger may be moved on — a
    // move made outside a pane leaves no row there, so the newest row left can be one the world has
    // already passed.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "remove-pane", required: &["shows"], refs: &[], strings: &["shows"], binds: false },
    // Something the agent in the pane said about its own session — the surface layer, said with the
    // CLI from inside the terminal it is about. `verb` is which of the layer's words was used and
    // `text` is what was said in it; both travel as values because the layer is one seam with several
    // words, and an op per word would be the same instruction written four times.
    //
    // It is walked by a hand rather than by a driver on purpose. The layer exists only inside a pane
    // — said anywhere else it is refused — so there is no way to reach it except the one an agent
    // reaches it by, and a road that stood it up some other way would prove a path nobody walks.
    //
    // `away` is for the one thing that has to be said from behind the face that reads it: what the
    // segment wears is raised only by a turn arriving while the ledger is up, and the layer is only
    // ever spoken inside a pane, which is on the other face. With it the word is armed and the
    // operator crosses over before it lands, so the step ends on the ledger. How long they have is
    // the driver's to say and not the road's.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "say", required: &["verb", "text"], refs: &[], strings: &["verb", "text"], binds: false },
    // A name the person gives a pane, typed on that same row. The words the row carries are otherwise
    // the session's own, said from inside the terminal, and this is the other hand on them: the last
    // word on a frame is the person's, so a name typed here stands over whatever the session called
    // itself (`app/src/talk/frames.ts`).
    //
    // **The way in is the row's menu, and that menu is drawn only while a terminal is running**
    // (`app/src/shell/TerminalPane.tsx`). A frame nobody has opened anything in is a place there is
    // nothing to call, so a road that named one would be pressing something that is not on the screen.
    //
    // `shows` names which pane, the way `press-pane` and `paste` name one: by the words a road typed
    // into it earlier. Left out, it is the page's one pane.
    //
    // There is no way to spell an empty name, and that is the door being closed rather than an
    // omission: an empty box takes nothing, so a road that asked for one would be asking for a step
    // that does nothing and reads as though it had.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "name-pane", required: &["name"], refs: &[], strings: &["name", "shows"], binds: false },
    // And what the pane's label carries afterwards. This is the whole of what the surface layer is
    // for: a word said in a terminal that nothing outside it can find out, arriving where a person
    // reads it. The words are the agent's own, so a reading finds them on the label and nowhere in
    // the interface around it. It reads a name a person typed (`name-pane`) the same way and for the
    // same reason — those words are the operator's own too, and are drawn on the row and nowhere else.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "label", required: &["shows"], refs: &[], strings: &["shows"], binds: false },
    // The row's own answer to being unreadable: a pointer held on it drops a panel under it carrying
    // the whole of what the row holds, wrapped, with nothing cut out (`app/src/talk/nameplate.ts`).
    //
    // **Two ops and not one**, because the panel is up only while the pointer is on the row: an
    // assert that placed the pointer itself would be an assert that changed the screen, and one that
    // did not would read a panel that was never drawn. So the road holds the pointer in a step of its
    // own, and the shot the reading is made from is the one taken after it.
    //
    // A screen road alone, both of them. What the row cuts and what a hover gives back are things a
    // box on a screen does; the binary is handed none of it.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "hold-label", required: &[], refs: &[], strings: &["shows"], binds: false },
    // And what that panel carries. It is read off the shot rather than off the tree, the way almost
    // everything is: the panel is drawn for an eye and marked as the row said twice, so the tree does
    // not carry it at all.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "label-in-full", required: &["shows"], refs: &[], strings: &["shows"], binds: false },
    // The mark the terminal's own segment wears while a turn is standing behind it. It is the whole
    // of what crosses the switch — a dot, with no number and no words — so there is nothing to name
    // in it and nothing to read out: what a road says here is that it is there, or that it is not.
    //
    // The absent half is half the goal. Being on the terminal face is being told, so the mark is
    // spent by crossing to it: a badge still up after the person has looked would be a light saying
    // "something is standing" rather than a knock saying "something came up while you were away".
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "face-badge", required: &[], refs: &[], strings: &[], binds: false },
    // Which project's panes the face is drawing. The tabs down the edge of the face are not a
    // grouping laid over a list of panes: a pane belongs to a project and can work in no folder
    // outside it, so pressing a project is the division itself being moved. What is beside the panes
    // afterwards is that project's, and every other project's pane is off the screen — which is why a
    // road walks this at all, a pane carried off by it being a running terminal like any other.
    // `project` is the name the tab carries, which is the ledger's own word for it.
    //
    // It is pressed rather than arrived at, for the reason `show-face` is. Which project the face
    // opens on is the run's business — whatever the ledger had selected, or the first project where
    // it had none — so a road that named a project without pressing for it would be reading a screen
    // it had not put itself on.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "go-project", required: &["project"], refs: &[], strings: &["project"], binds: false },
    // What one of those tabs is drawn with. A project wears the image it was registered for, or —
    // registered with none, which is nearly every project — its colour with the first letter of its
    // name on it. `present` says which of the two, the way `project icon` does on the settings:
    // neither side is an absence, so a step that asked for one of them to be missing would be asking
    // an eye to find nothing in a square that is never empty.
    //
    // It is read on the tab of a project without going to it, `project` naming the tab as
    // `go-project`'s does. That is the reading: every tab is drawn from the one picture the face is
    // handed, so an image that arrived on the settings and not here would be an image the tabs never
    // heard about.
    //
    // A screen road alone, for `project icon`'s reason — `project update` is the terminal's whole
    // door onto these fields and there is nowhere on it to hand over an image — and one more: a
    // terminal has no face to draw tabs down.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "tab-icon", required: &["project"], refs: &[], strings: &["project"], binds: false },
    // Opening a pane. A pane belongs to a project and works in a folder that project is bound to, so
    // where it is bound to one nothing is asked at all. `from` is which of the two controls is
    // pressed, and they are not the same place: `face` is the empty frame on a page with room in it,
    // and `strip` is the thin control a full page draws beside the panes. Neither names a page —
    // where a pane lands is the project's arithmetic, not the road's.
    //
    // `asks: true` is the other half of the folder. Where the project is bound to several, the press
    // opens nothing: what comes up where the pane would have been is the question of which of them it
    // works in, and `pick-folder` answers it. A road that walked that path without saying so would
    // tell an operator nothing was asked while the question stood on the screen in front of them.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "open-pane", required: &["from"], refs: &[], strings: &["from"], binds: false },
    // Which of this project's folders the pane about to be opened works in. It is only ever reached
    // from an `open-pane` that said `asks: true`: bound to one folder the face does not ask, and
    // bound to none it has no list to offer — that press goes to a picker, which is `open-folder`.
    //
    // The answer is given before the frame is made, which is what lets a reader walk away from the
    // question without leaving a half-opened box behind. `dir` is a name and not a path, the way
    // `open-folder`'s is: where a run keeps its folders is the run's to decide, and what a road
    // writes down is what to call the one it means. What the question *offers* is the whole of the
    // goal — this project's folders, and no way to anywhere outside them — so the list is part of
    // what the step puts in front of the operator rather than something read in a step of its own.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "pick-folder", required: &["dir"], refs: &[], strings: &["dir"], binds: false },
    // Walking away from that question without answering it. A frame is made when the question is
    // answered and not when it is asked, so this is the one move that reaches the state after a
    // question nobody answered: what a face draws when somebody changed their mind. It is an op of
    // its own rather than a value on `open-pane`, because it is pressed after that step and not
    // instead of it — the question has to be standing before there is anything to leave.
    //
    // *How* it is left is the driver's to say and not the road's. The question comes down on a press
    // anywhere else on the face, and which of those places is nearest is the run machine's business;
    // a road that named one would be walking that control's own road instead of this one.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "leave-question", required: &[], refs: &[], strings: &[], binds: false },
    // Whether that question is standing, read by a folder it offers. `dir` is the road's own name for
    // one of the project's folders, the way `open-folder`'s is — so what a reading finds is a word the
    // road put in the world itself, and not one of the interface's, which is what lets both halves be
    // read on a screen in any language.
    //
    // The absent half is what the walking-away is proved by, and it says more than "the question is
    // gone": the question *is* the box, drawn where a pane would be, so a screen with neither on it is
    // a question that left nothing behind.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "asking-folder", required: &["dir"], refs: &[], strings: &["dir"], binds: false },
    // The way in saying it cannot take the folder that was picked, read by the store that folder
    // belongs to. Where a terminal meets a claimed folder as a refusal on the command it typed, the
    // face has no command to be turned away from — so what stands in for it is the way in answering
    // under itself, where the hand that pressed it still is.
    //
    // Read by the store's name for the reason `folder claimed` is read by it: it is a name the world
    // was stood up with rather than a word of the interface's, so it is found on this screen whatever
    // language the machine is set to — and it is the one word a reader turned away can act on. A face
    // that went quiet instead, or answered without saying whose folder this is, is the same verdict:
    // a reader left holding a folder nothing of theirs reaches.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "turned-away", required: &["store"], refs: &[], strings: &["store"], binds: false },
    // What the empty frame will open a pane with, read on the row above the press that opens one.
    // The row is every agent this machine can start with the plain shell beside them, and exactly one
    // of them is on. A road reads it to say that a choice made once is still the answer afterwards,
    // which is the whole of what the row is for and the half no press can show while it is being
    // made: what is on at the moment of choosing is what the hand just put there.
    //
    // `start` is `shell`, and only that. Which agents are on the row is a probe of the run machine's
    // own `PATH`, so a road that named one would be a road that runs on the machines that happen to
    // have that tool and nowhere else. The plain shell is on every row by construction — it is the
    // absence of an agent rather than one of them — which is what makes it the one thing a road can
    // name here, for the reason `open-shell` names it.
    //
    // Where the machine can start nothing at all, no row is drawn: one thing to open with is not a
    // question. The reading is the same on that machine — what the next pane starts with is the plain
    // shell — with less on the screen to read it off.
    //
    // `start: none` is the other reading, and it names no program: it is **nobody having said yet**.
    // The first run on a machine with more than one thing to start comes up with nothing on the row
    // and a press that asks to be told rather than opening on a guess, and that is one state and not
    // two — a build that lit a name it had never been given, and a build that opened on one, are the
    // same fault read from either end. It is the one reading here that cannot be taken on whatever
    // machine the run is on: where a single agent was found that one is on, and where none were there
    // is no row. So a road that asks for it stands the machine up first (`can-start`), and reads this
    // before anything on the frame has been pressed — a choice made anywhere in the run ends the
    // state for good, this person's answer being kept and outliving the press that made it.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "opens-with", required: &["start"], refs: &[], strings: &["start"], binds: false },
    // A registered command as the frame draws it: the `name` on the row, and the `line` written out
    // beside it.
    //
    // **Both halves are read, and the line is the half that matters.** What is registered runs in a
    // terminal exactly as it was written — Amenbo composes none of it — so the promise the screen
    // makes is that a reader can see what a press would start before they press it. A frame that drew
    // the name alone would keep that promise for nobody, and one that drew a line it had tidied up
    // would keep it falsely.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "registered", required: &["name", "line"], refs: &[], strings: &["name", "line"], binds: false },
    // The opening instruction, arrived in a pane Amenbo did not compose the launch line of, and sent
    // rather than left waiting.
    //
    // Every pane gets that sentence, and a row off the catalog takes it as an argument before its
    // program starts — nothing to read afterwards. A line the reader registered has nowhere to put an
    // argument, so the sentence follows it into the pane instead: pasted once the pane has drawn
    // something, and submitted only once the pane draws the sentence back. **That second half is what
    // this reads.** A build that pasted and never sent leaves the reader a sentence sitting in an
    // input box; a build that sent blind would answer whatever the program asked first.
    //
    // `given-back` is the mark the road's own registered line puts in front of what it is handed, and
    // it is the road's word rather than the screen's for the reason every quoted word here is. It is
    // also what makes the reading a reading at all: a pane echoes what is put into it, so the sentence
    // is on the screen either way, and only a line the program gave back says it was ever sent.
    //
    // **The sentence itself is named in the instruction, and it is the one text here that may be.**
    // What a screen draws is in the run machine's language, and no road quotes it; this is not that.
    // It is the fixed English Amenbo hands an agent, the same on a machine set to any language, and
    // what the operator is given to look for stops before the command name in it — so the reading
    // holds on a dev-channel build as well as on the shipped one.
    //
    // **The absent half is the dangerous half, and it is the half a build gets wrong quietly.** The
    // sentence is put in whatever the program does, so a pane that never showed it is a pane the
    // newline was withheld from — the sentence is left in the input box for the person, and nothing
    // was answered on their behalf. A road reads that with a line the program cannot have been sent:
    // it registers a command that swallows what it is shown but still hands back any line it is
    // given, so the marked line appearing at all is a newline that went in blind. Where such a build
    // would have answered the first thing the program asked, this reading is the whole of the guard.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "handed-over", required: &["given-back"], refs: &[], strings: &["given-back"], binds: false },
    // And what the row above that pane says about it afterwards. It is the other half of the ending
    // `handed-over`'s absent side reads: the sentence was left in the input box, and the reader is
    // told so rather than left with a screen that looks exactly like one which was handed its
    // sentence properly.
    //
    // **Nothing is quoted, because the words are the interface's own.** What the row says is drawn in
    // whatever language the machine is set to, so a road naming it would hold on one machine — the
    // rule every reading here is written around. The sentence is described in the instruction instead
    // and left to an eye, the way the lamp's faces are (`dot`).
    //
    // It takes no argument for the reason `face-badge` takes none: there is one row above one pane,
    // and the whole of what is being read is whether it is saying this.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "unsent", required: &[], refs: &[], strings: &[], binds: false },
    // How many panes the page being shown draws. It is not `set-panes` read back: that one is the
    // ceiling on how many a page may hold, and this is how many are actually standing there. The two
    // part company on exactly the thing worth defending — a face that filled the ceiling with empty
    // boxes would be asking the same question four times over, and a count of the ceiling could never
    // tell that from a face with one pane on it.
    //
    // `empty` is how many of the boxes beside them are the way in — 0 or 1, since the page draws one
    // at its first gap and never a second. Left out, the reading is that there is at most one, which
    // is all a road needs while the page has room. It is worth saying exactly where the panes fill
    // the count: room the page has not got must not be offered, and "at most one" cannot tell a full
    // page from a page still offering.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "frames", required: &["count"], refs: &[], strings: &[], binds: false },
    // How many panes a page draws: 1, 2 or 4, and no other number. It is not a change of look. The
    // frames are one list cut into pages, so a new count re-pages every pane this device has — which
    // is why a road walks it at all: what has to survive the cut is the terminals running inside them.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "set-panes", required: &["count"], refs: &[], strings: &[], binds: false },
    // Which way the two panes of a two-pane page sit: `across` side by side, `down` one above the
    // other. It is asked at two and at no other count — everywhere else the rows are already spent
    // and there is nothing left to arrange (`app/src/talk/layout.ts`) — so a road walks it with two
    // panes standing, and never as a shape a page of any other count could be put into.
    //
    // **It is an op of its own rather than an argument on the count**, for the reason `split-out` and
    // `fold-back` are two: the presses are two controls, and what they do differs on exactly the
    // thing worth defending. A new count re-pages every frame this device has; a new orientation
    // moves no pane anywhere and turns no page. A road that said both in one step would be asking for
    // a re-cut it did not want, and could not tell which of the two had done what came out.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "set-orient", required: &["orient"], refs: &[], strings: &["orient"], binds: false },
    // And the shape the page came out in, read off the panes themselves. It is the whole of what the
    // press is for: `down` is asked for so that each pane keeps the window's whole width, and a build
    // that lit the control without re-laying the grid under it would draw exactly the screen the
    // reader pressed away from.
    //
    // Both shapes are walked, never the asked-for one alone. A page read only after the press has
    // nothing to say about what it was before, so a face stuck in one arrangement — the one a road
    // happened to ask for last — would come out green from end to end.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "panes-sit", required: &["orient"], refs: &[], strings: &["orient"], binds: false },
    // Which page is being shown, counted from 1. Paging is one of the two ways to a pane that is not
    // on the screen and by far the commoner, so it is the move a terminal has to be able to outlive:
    // a page is drawn rather than held, and the panes it took away are still running behind it.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "go-page", required: &["page"], refs: &[], strings: &[], binds: false },
    // The columns beside the panes, put away and brought back. `side` says which of the two: the rail
    // that lists the panes, and the file face on the other edge.
    //
    // **Which control does it is the whole reason both halves are here.** A column is folded by a
    // control of its own — the rail from the row above the panes, the file face from a cross on the
    // panel itself — and both are opened again from that same row. A road that only folded one of
    // them would leave the pairing untested on the other, and a column with no way back is the one
    // failure this face cannot recover from: nothing else on the screen says the thing is still
    // there to be asked for.
    //
    // They are two ops rather than one carrying a direction, for the reason `split-out` and
    // `fold-back` are two: the press that puts a column away and the press that brings it back are
    // in different places, and a road that named the wrong one would be pressing something else.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "hide-side", required: &["side"], refs: &[], strings: &["side"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "show-side", required: &["side"], refs: &[], strings: &["side"], binds: false },
    // The edge a column shares with the panes, dragged. `toward` is which way — `wider` takes room
    // from the panes, `narrower` gives it back — and both halves are walked, because a width that
    // only ever grows is half a control and the half a reader is left with is the one that took
    // their pane's room.
    //
    // `broad` is the third, and it is a place rather than a distance: drag until the column is about
    // as wide as the panes. It is here for the width a road has to be able to *recognise* later
    // (`side-span`), which the other two cannot hand it — a finger's width either way is a difference
    // between two shots, and nothing an eye can name on one.
    //
    // `cover` is the fourth, and a place as well: drag the wide width on until the panes are gone
    // behind it. It is separate from `broad` because the two stop in different places — the narrow
    // width stops where the panes would be squeezed away, the wide one goes on to the rail — and an
    // operator handed the wrong sentence would let go somewhere neither reading can name. What it is
    // for is `side-cover`, the way `broad` is for `side-span`.
    //
    // **It is the one step on these roads aimed at a line.** The edge carries no name, so nothing
    // reaches it the way a button is reached — the screen tool drags between two points, and working
    // those out of the screen is an operator's. So the instruction says where to put the pointer and
    // what to watch follow it, and the shot after it is what an eye closes.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "drag-side", required: &["side", "toward"], refs: &[], strings: &["side", "toward"], binds: false },
    // A press on a pane, meaning nothing but the press. It is what puts the reading column back to
    // its narrow width: the next press outside that column says where the reader is looking, and a
    // pane is the answer that says they have gone back to the work. So what it proves is on the
    // other side of the panes from what it touches.
    //
    // **It is not `type-line` with the typing left out.** That step's press is a way to reach the
    // input line and what it is about is the line; this one leaves the pane as it found it, and a
    // road that typed to get here would be reading a column beside a pane that had changed under it.
    //
    // `shows` is which pane, named the way `type-line` and `paste` name one: by the words a road
    // typed into it earlier. Left out, it is the page's one pane.
    OpSpec { kind: Kind::Action, domain: Domain::Terminal, op: "press-pane", required: &[], refs: &[], strings: &["shows"], binds: false },
    // Whether a column is beside the panes at all. `present: false` is the half the folding is proved
    // by, and it is the half worth having: a column that went away is what gives the panes the width,
    // and a face that drew it anyway would look exactly like one that had honoured the press until
    // somebody measured. It is not required, the way it is on no assert this face has: absence is
    // said out loud and presence is what a step means when it says nothing.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "side", required: &["side"], refs: &[], strings: &["side"], binds: false },
    // And how wide it is, after a drag. `wider` says which way it should have gone, against where it
    // stood on the shot before — the two pictures side by side are the reading, which is why this one
    // is left to an eye rather than to a search for words.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "side-width", required: &["side", "wider"], refs: &[], strings: &["side"], binds: false },
    // And how wide it stands, on one picture. The neighbour above answers against the shot before it,
    // which is an answer a road loses the moment the whole face changes underneath: crossing to
    // another project redraws every column on it, and "wider than it was" is then about two faces
    // rather than about one width. So this one is read against what is beside the column on its own
    // shot — the panes, whose width is whatever the column left them.
    //
    // `span` is `thin` for a column the panes are plainly wider than, `broad` for one that has taken
    // about their width or more. Two answers rather than a number, because what closes it is an eye
    // at a picture and a person at the screen has no ruler. Which is also why `broad` is reached by
    // `drag-side toward: broad` and not by the finger's width the other direction moves: a difference
    // an eye is to swear to has to be a difference an eye can see.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "side-span", required: &["side", "span"], refs: &[], strings: &["side", "span"], binds: false },
    // And how much of the panes is left beside the reading column at its wide width, which is the one
    // question the neighbour above cannot answer for that width. `span` divides the width between the
    // column and the panes, and the wide width the column ships with takes about half of it — so a
    // wide width somebody dragged out and the one nobody touched are the same answer on `span`
    // whenever the window is the size the application opens at, and on a window dragged wider the
    // shipped one turns `thin` while the build is perfectly correct. A reading resting on the size of
    // the window is a reading a road cannot carry.
    //
    // What does not rest on it is whether any of the panes is showing. The wide width lies over the
    // panes rather than beside them, and it stops at the rail: the one it ships with leaves a strip of
    // the work in view on the narrowest window the application opens, and one dragged to the stop
    // leaves none on the widest. So `cover` is `part` for a column with some of the panes still
    // showing beside it, `all` for one with none — a presence rather than a proportion, which is why
    // it stands as an op of its own instead of a third answer on `span`.
    //
    // `side` is here for the shape of the family and is `files`: the rail has one width and is never
    // drawn over anything, so there is nothing for this reading to be about on that side.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "side-cover", required: &["side", "cover"], refs: &[], strings: &["side", "cover"], binds: false },
    // Which face the lamp on a pane's label is showing. It is the one reading that says a pane is
    // *alive* rather than drawn: a terminal that ended leaves its last output where it was, so words
    // on a pane outlive the process that wrote them and a road reading only those cannot tell a
    // session that came back from the picture of one. `face` says which of the three is being read —
    // `lit` for output arriving, `calling` for a turn standing, `out` for neither. The lamp's hue is
    // not this op's and must not be read as its neighbour: hue says which pane, and the face says what
    // is happening in it — except on `calling`, which drops the hue for the warning colour because it
    // has stopped saying which pane this is and started saying come here.
    //
    // **Only the calling face moves**, and that is what decides how a road may read each of them. The
    // two still ones are a picture and can be shot; `calling` is a blink, so it is watched — at either
    // end of its turn it rests at a step a shot cannot tell from the others, and a road that judged it
    // off a picture would go red on the frame it happened to catch.
    OpSpec { kind: Kind::Assert, domain: Domain::Terminal, op: "dot", required: &["face"], refs: &[], strings: &["face"], binds: false },

    // ── the file face ─────────────────────────────────────────────────────────────────────────────
    // The folder a project is bound to, read from inside Amenbo: the folder itself, folded down, with
    // what git says about each row drawn as a colour on it. Every op is the screen's —
    // `cat` is not Amenbo doing anything — and `section` says which part of the column a row is being
    // looked for in. There is one part to name today; the arg is kept because the panel is not
    // finished growing, and a road that named none of them would have to be rewritten when it does.
    //
    // Unfolding the folder. It is a value and not two ops because it is one control that opens and
    // shuts, unlike the two windows' way out and way back, which are pressed in different places.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "tree", required: &["open"], refs: &[], strings: &[], binds: false },
    // One folder opened a level, or shut again with `open: false`. Folders are opened one at a time
    // on purpose — the level below is fetched when it is asked for — so a road reaching something
    // deep names each step of the way.
    //
    // The two halves are a value rather than two ops, the way the whole tree's are: what opens a
    // folder is the press that shuts it. Shutting one is how a road keeps a name on the screen once,
    // which is what a road pasting copies anywhere needs — a copy carries the name it came from.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "enter", required: &["name"], refs: &[], strings: &["name"], binds: false },
    // A file opened, from whichever section it is being pressed in. The row is named by the words it
    // draws, which is the file's own name.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "open", required: &["name", "section"], refs: &[], strings: &["name", "section"], binds: false },
    // And back out of it, by the way drawn on the screen. It is one of two — the other is the key
    // below — and this is the one a reader finds without knowing it is there.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "back", required: &[], refs: &[], strings: &[], binds: false },
    // A key pressed on the file face. The vocabulary is closed at the driver rather than here: what
    // a key does is the face's, and a road naming one the face has no answer for is a road nobody
    // can walk.
    //
    // It is an op of its own and not an argument on the ops above, because what a key reaches is
    // decided by where the reader is standing rather than by what is being asked for: the key that
    // leaves takes one layer per press, so the same step means the file on one press and the panel
    // on the next.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "press", required: &["key"], refs: &[], strings: &["key"], binds: false },
    // Which of its two forms a Markdown file is drawn in — what the text says (`rendered`), or the
    // text itself, hashes and all (`source`). Markdown is the only file with two, and the control is
    // drawn for it and for nothing else.
    //
    // The step names the form to end in rather than the press, because the one control is a toggle:
    // a road saying "press it" would mean the other form on a face that had already been switched,
    // and what a road is about is where the screen ends up.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "show-as", required: &["form"], refs: &[], strings: &["form"], binds: false },
    // The open file read again as the encoding the reader names, chosen from the control on its own
    // row that says what it was read as. The guess reports no confidence and breaks nothing visible
    // when it is wrong, so this door is the only thing standing between a reader and a file that
    // quietly says something else — and a door nobody walks is a door nobody knows is shut.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "reopen-with", required: &["encoding"], refs: &[], strings: &["encoding"], binds: false },
    // Whether a row is standing in a section. `present: false` is the half several of these roads are
    // about — a file the folder holds but the face must not offer, because it is ignored.
    OpSpec { kind: Kind::Assert, domain: Domain::Files, op: "listed", required: &["name", "section"], refs: &[], strings: &["name", "section"], binds: false },
    // What git says about a row, drawn on it as a colour. `mark` names which of the three
    // the row is wearing — `untracked`, `added`, `modified` — rather than the colour itself: what each
    // one is drawn in is a theme's to choose, and a road naming a colour would go red the day one moved.
    //
    // It is a `Review` on the screen, and the only assert on this face that could never be anything
    // else. A shot is read for words, and a colour is not one — the row says the same letters wearing
    // it as it does bare. `present: false` is the half an ignored row is read by: git records nothing
    // about it, so it wears no colour while standing on the tree like any other.
    OpSpec { kind: Kind::Assert, domain: Domain::Files, op: "row-mark", required: &["name", "section", "mark"], refs: &[], strings: &["name", "section", "mark"], binds: false },
    // What the open file says it was read as. The name is the one the build itself offers — a road
    // that spelt an encoding its own way would be asking for a label nothing draws — and what it
    // proves is that the row follows the reader rather than the guess.
    OpSpec { kind: Kind::Assert, domain: Domain::Files, op: "read-as", required: &["encoding"], refs: &[], strings: &["encoding"], binds: false },
    // What the column has open draws — a file, or the draft page that sits on the first of its tabs.
    // `shows` is words the road itself put there, so a reading finds them because what was written
    // reached the screen and for no other reason.
    //
    // `as` says what the words are standing in. Two of its three answers are a Markdown file's forms,
    // and asking for either hands the whole step to an eye: they carry the same words — that is what
    // makes them the same file — and what tells them apart is punctuation a reading throws away and a
    // size no reading reports, so a step judged on the words alone would pass on the very form it was
    // written to catch. The third is `picture`, and it is the opposite: the words are drawn inside the
    // file, so reading them off a shot is the whole of the question.
    OpSpec { kind: Kind::Assert, domain: Domain::Files, op: "reading", required: &["shows"], refs: &[], strings: &["shows", "as"], binds: false },
    // One of the face's standing lines, named by what it says rather than by its wording: the words are
    // the interface's own, and which language the run's machine is in is not a road's to know.
    OpSpec { kind: Kind::Assert, domain: Domain::Files, op: "says", required: &["note"], refs: &[], strings: &["note"], binds: false },
    // The way to keep what was typed, read as a control rather than pressed. A file the panel already
    // knows has moved under the reader cannot be saved from here — the mark it is holding is the one
    // the door refuses — so the control is shut, rather than taking a press that spends a round trip
    // and lands back on the screen the reader is already reading. It takes no arguments because the
    // face draws one of these, in the row above the text, and the file it belongs to is the one open.
    OpSpec { kind: Kind::Assert, domain: Domain::Files, op: "save-shut", required: &[], refs: &[], strings: &[], binds: false },

    // ── the files the column is holding ───────────────────────────────────────────────────────────
    // The reading column holds several files at once, as a row of tabs above the one on top
    // (`app/src/files/FilesPanel.tsx`), and these are the three ways a road walks that row.
    //
    // One tab pressed, bringing its file up. `name` is the file's, which is what the tab is drawn
    // with; left out, it is the draft page, the one tab with no name a road could type — its words
    // are the interface's own, and it is the only tab that is always there. The two are one op
    // because they are one row, the way `menu-on-folder`'s two are one menu.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "tab", required: &[], refs: &[], strings: &["name"], binds: false },
    // The same file reached from the control at the end of that row instead. It is a second door and
    // not a convenience: the row scrolls sideways and never wraps, so a tab far enough along it is
    // off the end, and this is the only way to one that is. It names a file and never the draft
    // page — what the control lists is what a reader opened.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "more-tabs", required: &["name"], refs: &[], strings: &["name"], binds: false },
    // And the row read back: which files it holds, in the order they were opened. `names` is all of
    // them, so a road reads what is **not** there as well — a file opened twice is one tab, and a
    // tab closed leaves the rest standing.
    //
    // A `Review`, though every word on it is the road's own. What is being asked is an order and a
    // whole: a reading finds each name wherever it stands, and finds them all with a fourth tab
    // beside them. Both are what this exists to catch.
    OpSpec { kind: Kind::Assert, domain: Domain::Files, op: "tabs", required: &["names"], refs: &[], strings: &[], binds: false },

    // ── changing what is in a file ────────────────────────────────────────────────────────────────
    // The words typed into the editor an opened file draws. They go on the end and on a line of their
    // own, which is what lets a road read the file afterwards for **both** what it already held and
    // what was added: a save that wrote only the typing would pass a reading of the new words and
    // have thrown the file away.
    //
    // It is one op and not two — the caret put where it goes, and then the typing — because on this
    // face they are one move, the same reasoning `name` is one op for.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "edit", required: &["types"], refs: &[], strings: &["types"], binds: false },
    // The same box filled from the clipboard instead of from the keyboard. It is a separate op and
    // not `edit` with the words left out, because a road cannot spell what goes in: what arrives is
    // whatever the last copy put there, and on the road this exists for that is a path the run
    // minted and nobody can write down beforehand.
    //
    // **It is an ordinary text box, and that is the whole of why it is here.** A path pasted into a
    // pane comes with the quoting a shell needs, so a road that read only the pane could not tell a
    // plain path on the clipboard from a quoted one. This is where the plain half is read, with
    // `files reading`.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "paste-into-editor", required: &[], refs: &[], strings: &[], binds: false },
    // And the typing kept. It takes no args: what is saved is the file that is open, and where the
    // bytes go is not a road's to say.
    //
    // **Nothing is asserted here**, and that is deliberate. What the panel draws once a save is
    // through is that there is nothing left to save, which is a reading of the control rather than of
    // the file — a face that drew it having written nothing would read exactly the same. So a road
    // that means to prove the bytes landed leaves the file and opens it again, which is the app
    // reading the disk, and the only reading that could not have come from what was on the screen.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "save", required: &[], refs: &[], strings: &[], binds: false },
    // The offer beside the line about the file having moved: take what is on the disk now, and lose
    // what was typed. It is the panel's whole answer to a file written under a reader — lining two
    // texts up is the pane's agent's work and not this face's — so it is one press and takes no args.
    //
    // **What it proves is the disk.** The editor holds what somebody typed until this is pressed, so
    // a reading taken after it is a reading of the bytes, and it is how a road tells a save that was
    // refused from one that went through without leaving the file and opening it again.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "read-again", required: &[], refs: &[], strings: &[], binds: false },

    // ── putting a row in the bin, and taking it back ──────────────────────────────────────────────
    // Where the file face's own settings row stands: whether the panel asks before it bins a row.
    // The face has one setting, so the row is not named — the way the tick's is not.
    //
    // **This is what lets the question be walked at all.** Whether it is asked is a habit of the
    // machine the run is on, and the checkbox that turns it off is drawn inside the question it
    // silences — so the settings row is the only thing that puts the question back. A road that can
    // move it can put the machine where it needs it and then walk the question, instead of pressing
    // the bin and covering both endings.
    OpSpec { kind: Kind::Assert, domain: Domain::Files, op: "setting", required: &["position"], refs: &[], strings: &["position"], binds: false },
    // And moving it. `asks` puts the question back, `quiet` takes it away. The positions are named by
    // what each does rather than by the word drawn on the row, since the words are the interface's own
    // and the run's language is whatever the machine is set to.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "set", required: &["position"], refs: &[], strings: &["position"], binds: false },
    // The bin pressed on the file that is open. It takes no args for the reason `save` does: what goes
    // is the file on the screen, and where the machine keeps what it deleted is not a road's to say.
    //
    // **The press is the whole of this step, and the question is the next one.** Where a road put the
    // row in `asks`, the panel puts a question here and this step leaves it standing; `answer` is what
    // decides it. Where the row is in `quiet` there is no question and the row goes on the press. Both
    // are the same press, which is why they are the same op — what differs is whether a road wrote a
    // step after it.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "trash", required: &[], refs: &[], strings: &[], binds: false },
    // The question the bin put, answered. `yes` bins the file and `no` leaves it where it is — and
    // both are a road's to walk, since what the question is for is the second one.
    //
    // **The checkbox in it is not this op's.** Ticking it would turn the question off for every run
    // walked on this machine afterwards, which is the state this pair exists to stop being permanent;
    // a road that wants the panel quiet says so with `set`.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "answer", required: &["answer"], refs: &[], strings: &["answer"], binds: false },
    // And taking it back, which on this face means the last press of the bin and nothing else. What
    // does it is the key the machine already undoes with rather than a control Amenbo drew, so the
    // line says the key — and says where to be standing, because the column beside this one hears the
    // same key as its own.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "undo", required: &[], refs: &[], strings: &[], binds: false },

    // ── picking rows out ──────────────────────────────────────────────────────────────────────────
    // Which rows the next act is about, said by pressing them rather than by naming them on the act
    // itself. What a key or a menu item does to "the files" is the question these roads ask, and a
    // step that named the rows on the act would be asking a different one — whether the face can be
    // told, one row at a time, which ones are meant.
    //
    // Two ops because the gestures are two questions. One row is the machine's own adding key, which
    // is also the way a row leaves a selection; a run of them is Shift, measured from wherever the
    // last press without it landed. Both name their row the way every row-addressed op here does.
    //
    // A screen road alone. What the binary is handed is the paths themselves, so there is nothing on
    // that side to pick out.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "pick", required: &["name", "section"], refs: &[], strings: &["name", "section"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "pick-to", required: &["name", "section"], refs: &[], strings: &["name", "section"], binds: false },

    // ── the machine's own copy and paste ──────────────────────────────────────────────────────────
    // What `⌘C` and `⌘V` mean on this face is what they mean everywhere else on the machine, so the
    // clipboard they go through is the machine's and not a pocket of Amenbo's. Where a paste lands is
    // worked out from the row the keyboard is on, the way a drop's landing is.
    //
    // Each names its row the way every other row-addressed op here does: a key reaches the row the
    // keyboard is standing on, and which row that is cannot be read off the screen afterwards — so a
    // step that left it unsaid would be handing the operator a choice the road meant to make.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "copy", required: &["name", "section"], refs: &[], strings: &["name", "section"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "paste", required: &["name", "section"], refs: &[], strings: &["name", "section"], binds: false },

    // ── bringing a file in from the machine ───────────────────────────────────────────────────────
    // A file dragged in from outside and let go over a row, which is the one way anything reaches this
    // folder that does not go through the folder itself. What is under test is the landing rather than
    // the carrying: the drop is caught by the application and not by the face, and what the face
    // decides is which folder was under the pointer when the hand opened.
    //
    // So the row is named the way every other row here is, and it is a folder's — a file's row opens a
    // file and has nothing to put anything in. The section is named beside it for the reason `open` and
    // `menu` name theirs.
    //
    // `brings` is a name rather than a path, and the operator brings the row it stands for. That is the
    // same fact `task attach` runs into on this face and for the same reason: a drop reads the disk the
    // operator is sitting at, and nothing a run lays down is anywhere a hand can reach from there. What
    // it holds is nothing this op reads — it is looked for by its name once it has landed.
    //
    // `as` says which of the two is being dragged, because the operator has to bring the right one and
    // the two prove different things: a file lands as itself, and a folder lands with everything in it,
    // which is a reading only a road that opens it can make.
    //
    // That reading is what `holding` is for, and it is the folder's alone: a road that opened the
    // folder and looked for a name would be looking for a name only the operator knows, having brought
    // the folder themselves. Named here, it is asked for at the hand-over instead — bring one with
    // *this* in it — and the row inside becomes something a shot can answer for.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "drop-in", required: &["as", "brings", "name", "section"], refs: &[], strings: &["as", "brings", "holding", "name", "section"], binds: false },

    // ── naming what is in the folder ──────────────────────────────────────────────────────────────
    // The menu again, over what a file's menu cannot be opened on. A folder's row carries no way out
    // to the machine and offers a name to make instead, and so does the heading at the top of the
    // tree — which is the folder itself, and the only way to make a name at the top level, there
    // being no row up there to point at.
    //
    // The two are one op because they are one menu, and which of them is meant travels as `name`
    // being there or not: a row is named the way every other row here is, and the heading is named by
    // nothing, having no name of its own to be told apart by. `section` is asked for either way, and
    // on the heading it is the whole of the answer — a project answering for several folders draws a
    // heading each.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "menu-on-folder", required: &["section"], refs: &[], strings: &["name", "section"], binds: false },
    // One name made, from the item on that menu through to the name being asked for. It is one op and
    // not two because the press and the typing are one move on this face: the item puts a box where a
    // row would be, and a box nobody typed into is a name nobody asked for — there is nothing in
    // between worth a road's while. `as` says which of the two items, by what it makes rather than by
    // the item's words.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "name", required: &["as", "name"], refs: &[], strings: &["as", "name"], binds: false },
    // And the same box over a name that is already there, from the item that opens it. What is typed
    // is the whole new name and not a change to the old one: the box opens holding what the row is
    // called, and what a step names is what the box is to be left holding — not the letters typed to
    // get there. The two differ, because the box picks out the part before the last dot rather than
    // the whole name, and the driver's line is what says so.
    //
    // **A name changed only in its letters' case is not a rename a road can read.** Every reading on
    // a screen road is folded to one case before the shot and the expectation meet, so a row that was
    // never renamed draws the same answer as one that was. It is the rename most worth walking — a
    // machine that reads two such names as one is the machine that would refuse it — which is why it
    // is said here: a road that named one would go green over a face that had done nothing.
    //
    // `by` says which door the box was opened by — the menu (left out, and the usual one) or the key
    // (`by: key`, where `press` with `f2` opened it a step earlier). Only the words differ: a step
    // that named the menu after a key had opened the box would send an operator hunting for a menu
    // nothing put on the screen.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "rename", required: &["name"], refs: &[], strings: &["name", "by"], binds: false },

    // ── handing a file to the machine ─────────────────────────────────────────────────────────────
    // The three ways out of this face that are not reading the file here, and all three are the
    // machine's own: the application it already opens that kind of file with, one the reader picks
    // for this file alone, and the file manager they keep their folders in. Amenbo chooses none of
    // them and remembers none of them, so the road stops at the hand-over and never follows what came
    // forward — where the file ended up is the machine's answer, not this face's.
    //
    // On a row the menu is a right-click. A folder's row opens one too, but it holds none of the
    // three: what a folder can be handed to is nothing, and what it is offered instead is a name to
    // make or to write over, which `menu-on-folder` above reaches. So this op names a file, the way
    // every other row here is named — by its name, and by the section it is standing in.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "menu", required: &["name", "section"], refs: &[], strings: &["name", "section"], binds: false },
    // The same menu, reached from the file that is open rather than from a row. It is a second door and not a
    // convenience: a file the face refuses to draw offers a way on to something built to open it, and there is no
    // row under the pointer to right-click by then — the column has been replaced by what the file turned out to
    // be. It takes no args for the same reason: one file is open, and it is the one the menu is about.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "menu-on-file", required: &[], refs: &[], strings: &[], binds: false },
    // One item on that menu pressed. `door` names which of the three rather than the words on the
    // item, for the reason `note` and `section` are named that way: the wording is the interface's
    // own, and which language the run's machine is in is not a road's to know.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "hand-over", required: &["door"], refs: &[], strings: &["door"], binds: false },
    // The one item on that menu that goes to something inside Amenbo rather than out to the machine:
    // what is running in the pane the reader is working in. It takes no args because there is nothing
    // to name — what is named is the row the menu was opened on, a folder as readily as a file, and
    // which pane is the face's own answer rather than a road's.
    //
    // It is its own op and not a fourth `door` for the reason the three above are one: those three
    // end off Amenbo's window and stop at the hand-over, and this one ends **on** it, in the pane,
    // where a shot can settle what happened. `terminal in-the-box` is what reads it.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "hand-to-pane", required: &[], refs: &[], strings: &[], binds: false },
    // The item that puts the row on the machine's own clipboard — the file itself and its plain path
    // together. It takes no args for the reason the one above it does: what is copied is the row the
    // menu was opened on, a folder as readily as a file.
    //
    // **Nothing here can be read.** A clipboard is off every window a run shoots, so this op ends in
    // the press and says so, and what settles it is the paste after it — into a pane, into the
    // editor an opened file draws, or back into the tree. It is the one step on this menu whose
    // whole meaning is in the step that follows.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "copy-path", required: &[], refs: &[], strings: &[], binds: false },
    // The same hand-over made by hand: the row taken hold of and carried onto a pane, rather than
    // named to the one the reader is already in. It is a separate op from the menu item because the
    // difference is the whole of what it proves — the menu's pane is the face's answer and this one
    // is the reader's, and a road that walked only the menu would leave the gesture untested.
    //
    // `onto` names which pane, by the words a road typed into it, the way `type-line` and
    // `remove-pane` name one. It is what the road is about — a page with one pane cannot tell "where
    // it landed" from "where the reader was" — so unlike those two it is not optional.
    OpSpec { kind: Kind::Action, domain: Domain::Files, op: "carry-to-pane", required: &["name", "section", "onto"], refs: &[], strings: &["name", "onto", "section"], binds: false },
    // And what the press left. No shot settles it, and that is the point rather than a gap: what a
    // hand-over ends in is off Amenbo's own window — an application that came forward, or an
    // operating system's chooser drawn by the system — and the run shoots the window under test. The
    // eye that closes it is the one that was standing at the screen when the item was pressed.
    OpSpec { kind: Kind::Assert, domain: Domain::Files, op: "handed-over", required: &["door"], refs: &[], strings: &["door"], binds: false },

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
    // How that work is classified: the axes a project declares, the values they offer, the filing of
    // a task under one, and whether the axis goes on the card. A screen road about what the board
    // *says* of a classification has to open on one already there — filing is a road of its own
    // (`classify-work-along-an-axis`), and walking it again on screen would prove that road twice and
    // this one not at all.
    (Domain::Dimension, "create"),
    (Domain::Dimension, "value-add"),
    (Domain::Dimension, "set"),
    (Domain::Dimension, "show-on-card"),
    // And how many of its values one record may answer with, which is the same kind of answer: the
    // axis's own, written once and read by every face. A screen road about work already filed under
    // several values has to open on an axis that admits them — raising the flag is the classification
    // manager's own road, and walking it here would prove that road rather than this one.
    (Domain::Dimension, "cardinality"),
    // And which axis is the project's time axis, with the windows its values already cover. A screen
    // road about the era is watching the era arrive on a record nobody was asked about, and that
    // reader set the axis up on some earlier day — designating it is a road of its own, on the manager,
    // and one walked here would prove that road rather than this one. A window is the payload of the
    // same role, so a value that already carries one is part of the same world; a road that means to
    // watch a window being written still writes that one itself.
    (Domain::Dimension, "time-axis"),
    (Domain::Dimension, "period"),
    // And the record on the other side, for a road that reads across both. A screen narrowed to one
    // project has to have a decision standing in another to leave out, and which project a decision
    // was filed under is nothing such a road proves — recording one is a road of its own.
    (Domain::Decision, "create"),
    // And one of those already settled, where a road opens on a corpus a reader is separating rather
    // than on one they are ruling over. Settling is a road of its own (`read-whether-a-decision-still-holds`),
    // so a screen road that walked it to arrange its world would prove that road twice and its own not
    // at all — while a store holding both a standing policy and somebody's open suggestion is exactly
    // the world anyone narrowing decisions is standing in.
    (Domain::Decision, "accept"),
    // A device that has been used for a while. It is the one premise no amount of doing reaches: what
    // it stands up is the passage of time itself — launches tallied across days written on — which a
    // road can only be given, never earn.
    (Domain::Store, "worn-in"),
    // And a device nothing has been raised on at all, which is the world every road about the screen
    // a first-time reader meets opens on. It is here for the state it leaves rather than for the act,
    // the way `folder unbind` is: what the driver does to have somewhere to file a premise is itself
    // a project, so there is no other way to arrive at a store holding none — and no road reaches it
    // either, since a road that emptied the device would be walking `retire-a-project`.
    (Domain::Store, "nothing-raised"),
    // A folder already answering for a project — what a screen showing bindings has to be looking at.
    // Taking a pointer back off is here for the state it leaves rather than for the act: a project
    // with no folder left is what one whole notice is about, and creating a project links one, so
    // there is no other way to arrive at it.
    (Domain::Folder, "init"),
    (Domain::Folder, "bind"),
    (Domain::Folder, "unbind"),
    // And one of those folders already claimed by another store. It is here for the same reason
    // `store worn-in` is: no road reaches this world, whichever face is walking it. A build stamps
    // its own name as it writes a pointer, so the one store that cannot leave another's is the one
    // under test — and on screen there is not even a command to try it with. A road that reads the
    // claim has to open on it.
    (Domain::Folder, "foreign-pointer"),
    // And one whose store is gone rather than standing beside this one. It is out of reach for the
    // same reason and one step further: a build writes the number of a project it has, so a folder
    // naming one nothing answers for is a folder some other store wrote in and then went away.
    (Domain::Folder, "lost-pointer"),
    // A file already lying in one of those folders. What a folder traces is read off its contents and
    // recorded nowhere, so a bound folder that already carries a provider's settings — the state every
    // road about wiring an AI starts from — is a world no amount of store seeding reaches.
    (Domain::Repo, "write-file"),
    // And the same for bytes a scenario cannot hold: an image is not text, so the file a road has an
    // operator choose in a picker comes off the fixtures shelf rather than out of the YAML. A screen
    // road cannot make a file at all — every move it has is a move on a record — so a road that needs
    // one lying on the disk has no other way to arrive at it.
    (Domain::Repo, "copy-fixture"),
    // And one of those names being a link instead of a file. Amenbo makes no link and has no command
    // that would — a face that refuses to follow one is all it ever does about them — so the world a
    // road about that refusal opens on is one no face reaches.
    (Domain::Repo, "symlink"),
    // And the folder being a git repository, which is the world every road about what git says has to
    // open on. Amenbo makes no repository and has no command that would — it only ever reads one — so
    // no road reaches this state whichever face is walking it. It is a step as well, on the roads where
    // making one is what is being walked: the hook slots are written into a repository, and getting
    // there is those roads' own work rather than the ground they start from.
    (Domain::Repo, "git-init"),
    // And what was lying there being recorded in it. Same reason one line up, and one state further:
    // Amenbo makes no commit either, so a folder git is quiet about while a file inside it is new is
    // a world no face can reach. It is a premise and only that — a road that recorded something
    // mid-walk would be walking git rather than Amenbo.
    (Domain::Repo, "git-commit"),
    // And a folder already wired, which is the same kind of world one step further on. The wiring is a
    // file and not a record, so nothing in the store reaches it — and writing the settings out by hand
    // would put the launch command's own name in the scenario, which is the one thing the build under
    // test is supposed to say. This asks that build for its own text and makes the edit with it.
    (Domain::Repo, "wire-ai"),
    // And a folder an app already reaches over MCP, which is the same kind of world by a different
    // road: what reaches it is an entry in a settings file, so nothing in the store arrives there
    // either. A screen road that opens on such a folder has no other way to be standing in one.
    (Domain::Repo, "mcp-reach"),
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
    // And what it asks Amenbo to *draw* between them. Same reason twice over: no published
    // plugin writes one, and what the road is about is a form that already says something before anybody
    // has typed anything — so the writing is the world and the reading is the road.
    (Domain::Plugin, "declare-part"),
    // And what it offers to *do* from that same form, with the program that answers a press. The
    // declaration is the world for the same reason a setting's is — no published plugin carries one — and
    // the program comes with it: an operation is code being run, so a road that pressed a button no
    // stand-in was answering would be reading whatever the real plugin happened to say.
    (Domain::Plugin, "declare-action"),
    (Domain::Plugin, "press-program"),
    // And the check it raises before a gate opens, with the program that answers it. Same pair and the
    // same reason: the declaration is the author's word, and a road that pressed a switch with nothing
    // standing in would be judged by whatever the real plugin thought of the values.
    (Domain::Plugin, "declare-check"),
    (Domain::Plugin, "check-program"),
    // And a value already filled in for one of them. Answering a setting is a road of its own and is
    // deliberately not one a premise walks — except that a setting the author marked `readonly` is not
    // answered by anybody a road can be: the value is the plugin's own, written back through
    // `plugin config set`, and the screen's whole promise about it is that it offers no way
    // to type one or to take one away. A form with nothing in the field would draw no button either, and
    // would prove that promise for the wrong reason.
    (Domain::Plugin, "config-set"),
    // And the layer it says it lives at, for that same reason one line further out: the layer is the
    // author's word too, every plugin the official catalog serves declares none, and a screen road
    // about reading the layer off a row has to find a row that already declares one.
    (Domain::Plugin, "declare-scope"),
    // The tick's band already put off. What it stands up is a day having passed — or not — since
    // "later" was pressed, and no run earns that: the band is judged once at launch, so the press
    // and the judgement it gates can never be in the same run. The same kind of reach as
    // `store worn-in`, one key further in.
    (Domain::Tick, "deferred"),
    // Which agents this machine can start. It is a premise and can be nothing else: the build asks
    // the question once, as it draws the frame, by running a login shell over the `PATH` it was
    // launched with — so the answer is fixed before there is a screen to press anything on. What it
    // arranges is the machine and never the app: programs in a directory of the run's own, handed to
    // the launch and to nothing else.
    (Domain::Terminal, "can-start"),
];

/// Whether this op may stand a world up (see [`PREMISE_OPS`]).
fn may_stand(domain: Domain, op: &str) -> bool {
    PREMISE_OPS.iter().any(|(d, o)| *d == domain && *o == op)
}

/// The ops whose step carries a query — the words a reader types, or the number of a record they
/// already hold. It is checked apart from `required` because neither key is required on its own:
/// exactly one of them is, and a spec can only say that a key must be there.
///
/// Why a number is not simply written as words: the store issues it, so a scenario has no way to
/// know it in advance, and `number_of` names the record instead and leaves the driver to fill it in.
fn takes_a_query(domain: Domain, op: &str) -> bool {
    matches!(
        (domain, op),
        (Domain::Task, "narrow" | "open-hit" | "found") | (Domain::Decision, "narrow" | "found")
    )
}

/// What is wrong with an `offers:` value, if anything — the rows a stood catalog publishes.
///
/// Two words are a row's floor: the `name` it is fetched and badged by, and the `desc` a row draws
/// under it. The rest is optional, and each is held to its own shape — a `claims_official` that
/// arrived as the word "true" would be a badge nobody claimed, and a `label` that arrived as a
/// number would reach a form as one.
///
/// `translated` is the same row in the author's other languages, keyed by language code, and it is
/// checked against the base row rather than on its own: translating a `label` on an entry that
/// declares no `setting` publishes a label for a field nobody will see, and translating an `about`
/// on an entry that describes itself nowhere is the text Amenbo's own manifest check refuses — both
/// being the same mistake one language along.
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
        for key in ["about", "setting", "label"] {
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
        problems.extend(translated_problems(row).into_iter().map(at_row));
    }
    problems
}

/// What is wrong with a declaration's `translated` block — the words a form's field carries in the
/// author's other languages.
///
/// It is keyed by language code, and each language holds the `label` the field is drawn under and the
/// `options` its candidates are, keyed by the value each candidate stores. Keying the candidates by
/// value rather than by position is the same rule the published form obeys: an author reordering their
/// list would otherwise silently move every language's words onto the wrong answer.
fn declared_translations_problems(block: &serde_yaml::Value) -> Vec<String> {
    let Some(langs) = block.as_mapping() else {
        return vec!["`translated` must be a mapping of language code to the words in it".to_string()];
    };
    let mut problems = Vec::new();
    for (lang, words) in langs {
        let Some(lang) = lang.as_str() else {
            problems.push("`translated` is keyed by language code, which is a string".to_string());
            continue;
        };
        let Some(words) = words.as_mapping() else {
            problems.push(format!("`translated.{lang}` must be a mapping of the words written in it"));
            continue;
        };
        if words.get("label").is_some_and(|v| v.as_str().is_none()) {
            problems.push(format!("`translated.{lang}.label` must be a string"));
        }
        match words.get("options") {
            None => {}
            Some(o) => match o.as_mapping() {
                None => problems.push(format!(
                    "`translated.{lang}.options` must be a mapping of a candidate's stored value to the words it is drawn under"
                )),
                Some(candidates) => {
                    for (value, shown) in candidates {
                        if value.as_str().is_none() || shown.as_str().is_none() {
                            problems.push(format!(
                                "`translated.{lang}.options` maps a candidate's stored value to its words, and both are strings"
                            ));
                        }
                    }
                }
            },
        }
        if words.get("label").is_none() && words.get("options").is_none() {
            problems.push(format!("`translated.{lang}` translates nothing — name a `label`, some `options`, or both"));
        }
    }
    problems
}

/// What is wrong with one row's `translated` block — the same row in the author's other languages.
///
/// It is a mapping keyed by language code, and each language holds the words a translation is made
/// of: the `desc` a row draws, the `about` an opened panel is read by, and the `label` its one
/// setting is shown under. Naming none of them is a language that translates nothing, which is a
/// document published for no reason — so it is caught here rather than serving an empty answer to a
/// reader who then sees English and cannot tell why.
fn translated_problems(row: &serde_yaml::Value) -> Vec<String> {
    let Some(block) = row.get("translated") else { return Vec::new() };
    let Some(langs) = block.as_mapping() else {
        return vec!["`translated` must be a mapping of language code to the words in it".to_string()];
    };
    let mut problems = Vec::new();
    for (lang, words) in langs {
        let Some(lang) = lang.as_str() else {
            problems.push("`translated` is keyed by language code, which is a string".to_string());
            continue;
        };
        let Some(_) = words.as_mapping() else {
            problems.push(format!("`translated.{lang}` must be a mapping of the words written in it"));
            continue;
        };
        for key in ["desc", "about", "label"] {
            if words.get(key).is_some_and(|v| v.as_str().is_none()) {
                problems.push(format!("`translated.{lang}.{key}` must be a string"));
            }
        }
        if words.get("label").is_some() && row.get("setting").is_none() {
            problems.push(format!(
                "`translated.{lang}.label` names a `setting`, so one has to be declared"
            ));
        }
        // The same rule Amenbo holds a real manifest to: there is nothing to translate where the
        // author described the plugin in no language at all.
        if words.get("about").is_some() && row.get("about").is_none() {
            problems.push(format!(
                "`translated.{lang}.about` translates an `about`, so one has to be written"
            ));
        }
        if ["desc", "about", "label"].iter().all(|key| words.get(key).is_none()) {
            problems.push(format!(
                "`translated.{lang}` translates nothing — name a `desc`, an `about`, a `label`, or any of them"
            ));
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

        // And the one rule that is about where a road has left the screen rather than about a step's
        // own text. A screen's question alone: the CLI's road has no column to draw a file in.
        for (i, op) in rendering_in_the_way(self.steps(Driver::Gui)) {
            errs.push(ValidationError {
                driver: Some(Driver::Gui),
                step: Some(i),
                message: format!(
                    "`{op}` needs the file's own text on the screen, and the Markdown file this road has open is drawn as what that text says — put the text up first with `show-as` (`form: source`)"
                ),
            });
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
            // Which window a step happens in is a screen's question. On the premise it names a
            // window nothing has drawn yet, and on the CLI's road it names one that never exists —
            // both read as a road written for the screen and filed under the wrong key, which is
            // exactly the mistake a silently ignored field would leave standing.
            if let Some(window) = step.window() {
                if driver != Some(Driver::Gui) {
                    let where_ = match driver {
                        None => "a premise stands a world up before any window is drawn",
                        _ => "the CLI has no windows",
                    };
                    errs.push(at(i, format!(
                        "`window: {window}` says which screen this happens on, and {where_} — it belongs on `steps_gui`"
                    )));
                } else if window.trim().is_empty() {
                    errs.push(at(i, "`window` names a window by the title drawn in its bar, so it cannot be empty".to_string()));
                }
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
            // cannot work without, `away` whether a word said in a pane is armed and left behind,
            // `folded` whether the ref being pressed in a pane is one the fold broke across two
            // rows, `asks` whether the press that opens a pane meets the question of which folder
            // rather than a pane, the two key questions whether a catalog serves a signing key and
            // whether one of its is pinned, and `first` whether a hit stands at the top of the
            // answer rather than merely somewhere in it.
            // The query, in whichever of its two spellings — one of them, never both and never
            // neither. `spelled` belongs to the number alone: a word is typed as it is written, so a
            // step naming a shape for one is a step that means a number and left the record out.
            if takes_a_query(step.domain(), step.op()) {
                let words = step.with().contains_key("words");
                let numbered = step.with().contains_key("number_of");
                if words && numbered {
                    errs.push(at(i, "`words` and `number_of` are the one query written two ways — name one of them".to_string()));
                } else if !words && !numbered {
                    errs.push(at(i, "the query is missing — write `words`, or `number_of` naming the record whose number is typed".to_string()));
                }
                if step.with().contains_key("spelled") && !numbered {
                    errs.push(at(i, "`spelled` is the shape a number is typed in, so it belongs with `number_of`".to_string()));
                }
                if let Some(v) = step.with().get("spelled") {
                    if !matches!(v.as_str(), Some("bare" | "hash")) {
                        errs.push(at(i, "`spelled` must be `bare` (`12`) or `hash` (`#12`)".to_string()));
                    }
                }
            }

            // A number written into text needs text to be written into. On an update that is a title
            // or a notes field, and a step that named neither would be asking for a number to be put
            // nowhere.
            if step.domain() == Domain::Task
                && step.op() == "update"
                && step.with().contains_key("mentions")
                && !step.with().contains_key("notes")
                && !step.with().contains_key("title")
            {
                errs.push(at(i, "`mentions` writes a number into text, so it needs a `notes` or a `title` to write it into".to_string()));
            }

            for key in [
                "present",
                "ok",
                "running",
                "required",
                "away",
                "folded",
                "asks",
                "publishes_key",
                "pinned_key",
                "first",
            ] {
                if let Some(v) = step.with().get(key) {
                    if v.as_bool().is_none() {
                        errs.push(at(i, format!("`{key}` must be a boolean")));
                    }
                }
            }

            // Which side of the store a step is talking about. It is a word rather than a boolean
            // because an axis has three answers to give — and the reading has two, `both` being a
            // state an axis is in and not a side anything is offered on — so the set is checked
            // against the kind of step rather than once for the key.
            //
            // Only where the word means that. `side` is also the terminal's word for which of its
            // panels is up (`files`, `memo`, `rail`), which is a different vocabulary under the same
            // key — so the domain is part of the question, not just the kind.
            if step.domain() == Domain::Dimension {
                if let Some(v) = step.with().get("side") {
                    let (ok, takes) = match step.kind() {
                        Kind::Action => (matches!(v.as_str(), Some("task" | "decision" | "both")), "`task`, `decision` or `both`"),
                        Kind::Assert => (matches!(v.as_str(), Some("task" | "decision")), "`task` or `decision`"),
                    };
                    if !ok {
                        errs.push(at(i, format!("`side` must be {takes}")));
                    }
                }
            }

            // The shelf a stood catalog serves — the one arg written as a list of rows rather than
            // as a word. Its rows are a document's fields, not Amenbo's arguments, so the loader
            // reads them here instead of through `strings`: a row is where a typo would otherwise
            // travel all the way to a catalog served with a blank line under a name.
            if let Some(v) = step.with().get("offers") {
                for problem in offers_problems(v) {
                    errs.push(at(i, problem));
                }
            }

            // The words a declaration carries in the author's other languages — the same kind of
            // arg one tier in, and read here for the same reason: what it holds is a form's fields,
            // not Amenbo's arguments, so `strings` has no shape to hold it to. A row's `offers`
            // carries its own copy of this, checked against the row it sits on; here there is no row
            // to check against, so the shape is the whole of what can be said.
            if let Some(v) = step.with().get("translated") {
                if step.domain() == Domain::Plugin && step.op().starts_with("declare-") {
                    for problem in declared_translations_problems(v) {
                        errs.push(at(i, problem));
                    }
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

/// Every step on a screen road that types into a file whose text is not on the screen.
///
/// **A Markdown file opens on what its text draws, and a rendering has no editor** — the switch
/// beside the name is what puts the text up (`app/src/files/FilesPanel.tsx`). So a road that opens a
/// `.md` and goes straight to the typing reads perfectly and stops dead on the screen: the operator
/// finds a heading where the caret should be, and every step after it is unwalkable. That is the
/// road's fault and not the panel's, which is why it is caught here rather than left for the
/// pre-distribution run to find on the day.
///
/// What the column is holding is followed through the three doors a road has for changing it — a
/// file opened from the tree, a tab pressed, and the draft page, which is written on rather than
/// read and is nobody's Markdown — and the form it is drawn in through the two that change that:
/// the switch, and reading the bytes again as another encoding, which opens the file afresh.
fn rendering_in_the_way(steps: &[Step]) -> Vec<(usize, &str)> {
    let markdown = |name: &str| {
        let lower = name.to_lowercase();
        lower.ends_with(".md") || lower.ends_with(".markdown")
    };
    let mut holding_markdown = false;
    let mut as_source = false;
    let mut found = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        if step.domain() != Domain::Files {
            continue;
        }
        let named = step.with().get("name").and_then(|v| v.as_str());
        match step.op() {
            // The column comes to hold another file, drawn the way every file opens. A tab with no
            // name is the draft page, which is not a file at all.
            "open" | "tab" | "more-tabs" => {
                holding_markdown = named.is_some_and(markdown);
                as_source = false;
            }
            // The file closes, and what stands where it was is not one.
            "back" => {
                holding_markdown = false;
                as_source = false;
            }
            // Read again from its bytes, which is the file opened afresh — the form goes back with
            // it (`FilesPanel`).
            "reopen-with" => as_source = false,
            "show-as" => {
                as_source =
                    matches!(step.with().get("form").and_then(|v| v.as_str()), Some("source"));
            }
            "edit" | "paste-into-editor" | "save" if holding_markdown && !as_source => {
                found.push((i, step.op()));
            }
            _ => {}
        }
    }
    found
}

/// Where the files a road copies into its world are kept when nobody says otherwise, resolved
/// against this crate so it is the same folder whatever the CWD.
///
/// It is answered here rather than beside the driver that reads from it, because the lint below has
/// to look in the very folder the run will: two answers to "where are the fixtures" is how a road
/// comes to name one that is not there and nothing says so until release day.
pub fn fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // verification/
        .map(|p| p.join("fixtures"))
        .unwrap_or_else(|| std::path::PathBuf::from("fixtures"))
}

/// Every fixture this scenario names that is not in `dir`.
///
/// **The one check in the lint that touches the disk**, and the reason it is not in
/// [`Scenario::validate`] with the rest: validate answers about the text of a road, and this asks
/// the filesystem. It is worth the exception because nothing else asks — `cargo test` and `--print`
/// both read a road without ever fetching what it copies, so a `from:` naming nothing is green
/// everywhere until the pre-distribution run walks it for real and stops.
fn missing_fixtures(scenario: &Scenario, dir: &Path) -> Vec<ValidationError> {
    let mut errs = Vec::new();
    let mut look = |driver: Option<Driver>, steps: &[Step]| {
        for (i, step) in steps.iter().enumerate() {
            if step.domain() != Domain::Repo || step.op() != "copy-fixture" {
                continue;
            }
            let Some(from) = step.with().get("from").and_then(|v| v.as_str()) else { continue };
            if dir.join(from).is_file() {
                continue;
            }
            errs.push(ValidationError {
                driver,
                step: Some(i),
                message: format!(
                    "there is no fixture at `{from}` — the path is read from {}",
                    dir.display()
                ),
            });
        }
    };
    look(None, &scenario.given);
    for driver in Driver::ALL {
        look(Some(driver), scenario.steps(driver));
    }
    errs
}

/// Load and validate in one call — the check a `lint` run performs on each file.
///
/// `fixtures` is the folder the run standing behind this lint will copy from; `None` is
/// [`fixtures_dir`], this repository's own. A caller that takes the folder on its own command line
/// hands the same value here, and the argument is what asks it to: the screen harness runs inside a
/// VM and copies from a folder in the guest, while this crate's compile-time path names nothing
/// there — so a lint left to answer for itself turns away every road that copies a file, and the
/// run never starts.
pub fn lint_file(path: impl AsRef<Path>, fixtures: Option<&Path>) -> Result<Scenario, Vec<String>> {
    let scenario = load_file(path).map_err(|e| vec![e.to_string()])?;
    let mut errs = scenario.validate().err().unwrap_or_default();
    // Only once the road itself reads: a `copy-fixture` whose op or args are wrong has already been
    // named, and a second line about the file it points at would be noise on top of the real fault.
    if errs.is_empty() {
        let dir = fixtures.map(Path::to_path_buf).unwrap_or_else(fixtures_dir);
        errs = missing_fixtures(&scenario, &dir);
    }
    if errs.is_empty() {
        Ok(scenario)
    } else {
        Err(errs.into_iter().map(|e| e.to_string()).collect())
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

    /// A road that says which window it is walked in — the one thing a screen driver needs once an
    /// app draws more than one, and the one thing the other drivers have no use for.
    #[test]
    fn a_screen_road_may_say_which_window_a_step_is_walked_in() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: task
    op: narrowing-shut
    window: "Amenbo — Terminal"
"#;
        let s = load_str(yaml).expect("parses");
        s.validate().expect("valid");
        assert_eq!(s.steps(Driver::Gui)[0].window(), Some("Amenbo — Terminal"));
    }

    /// Written on the wrong road it is a road filed under the wrong key, and the loader says so
    /// rather than reading past it: a terminal has no windows to stand in front of, and a premise
    /// stands its world up before anything is drawn at all.
    #[test]
    fn a_window_off_the_screen_road_is_rejected() {
        for (key, step) in [
            ("steps_cli", "  - type: assert\n    domain: task\n    op: narrowing-shut\n    window: \"Amenbo\"\n"),
            ("given", "  - type: action\n    domain: project\n    op: create\n    with: { name: P }\n    window: \"Amenbo\"\n"),
        ] {
            let yaml = format!("id: x\ntitle: y\nsteps_gui:\n  - type: assert\n    domain: task\n    op: narrowing-shut\n{key}:\n{step}");
            let s = load_str(&yaml).expect("parses");
            let errs = s.validate().expect_err("the window is refused");
            assert!(
                errs.iter().any(|e| e.message.contains("belongs on `steps_gui`")),
                "{key}: {errs:?}"
            );
        }
    }

    /// A window is named by the title drawn in its bar, and no window is called nothing.
    #[test]
    fn an_empty_window_name_is_rejected() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: task
    op: narrowing-shut
    window: "  "
"#;
        let s = load_str(yaml).expect("parses");
        let errs = s.validate().expect_err("an empty title is refused");
        assert!(errs.iter().any(|e| e.message.contains("cannot be empty")), "{errs:?}");
    }

    /// The road that reads right and cannot be walked: a Markdown file is drawn as what its text
    /// says, so there is no editor to click into until the switch beside the name is pressed.
    #[test]
    fn typing_into_a_markdown_file_still_drawn_as_markdown_is_rejected() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: action
    domain: files
    op: open
    with: { name: watering.md, section: tree }
  - type: action
    domain: files
    op: edit
    with: { types: SCENARIO a line }
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(
            errs.iter().any(|e| e.message.contains("`show-as`") && e.step == Some(1)),
            "{errs:?}"
        );
    }

    /// And the three ways it is not a fault: the switch pressed, a file that has only one form to
    /// be drawn in, and the draft page, which is written on rather than read.
    #[test]
    fn the_text_on_the_screen_leaves_the_typing_alone() {
        for road in [
            "  - type: action\n    domain: files\n    op: open\n    with: { name: watering.md, section: tree }\n  - type: action\n    domain: files\n    op: show-as\n    with: { form: source }\n",
            "  - type: action\n    domain: files\n    op: open\n    with: { name: watering.txt, section: tree }\n",
            "  - type: action\n    domain: files\n    op: tab\n",
        ] {
            let yaml = format!(
                "id: x\ntitle: y\nsteps_gui:\n{road}  - type: action\n    domain: files\n    op: edit\n    with: {{ types: SCENARIO a line }}\n"
            );
            load_str(&yaml).unwrap().validate().expect("the text is on the screen");
        }
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

    /// The query is one thing written two ways, and a step names one of them. Neither is `required`
    /// on its own, so this is the check that keeps a road from asking for nothing — or from asking
    /// for two things and leaving the driver to pick.
    #[test]
    fn a_search_step_names_one_query_and_not_two_or_none() {
        let road = |with: &str| {
            format!(
                "id: x\ntitle: y\nsteps_cli:\n  - type: action\n    domain: task\n    op: create\n    with: {{ title: T }}\n    as: seed\n  - type: assert\n    domain: task\n    op: found\n    with: {{ target: seed, {with} }}\n"
            )
        };
        let errs = |with: &str| {
            load_str(&road(with)).unwrap().validate().unwrap_err().iter().map(|e| e.message.clone()).collect::<Vec<_>>().join(" / ")
        };

        load_str(&road("words: SEED")).unwrap().validate().expect("words alone is a query");
        load_str(&road("number_of: seed")).unwrap().validate().expect("a record's number is the other one");
        load_str(&road("number_of: seed, spelled: hash")).unwrap().validate().expect("and it may say which shape");

        assert!(
            errs("words: SEED, number_of: seed").contains("one query written two ways"),
            "both at once leaves the driver to pick: {}", errs("words: SEED, number_of: seed")
        );
        assert!(errs("present: true").contains("the query is missing"), "neither asks for nothing");
        assert!(
            errs("words: SEED, spelled: hash").contains("belongs with `number_of`"),
            "a shape with no number is a step that meant one"
        );
        assert!(errs("number_of: seed, spelled: sideways").contains("`bare`"), "and the shapes are two");
        assert!(errs("number_of: ghost").contains("does not resolve"), "the record is a binding like any other");
    }

    /// A number written into one record's text is how a road shows that a number is still asked as a
    /// word: the store issues it, so there is no other way for a scenario to put one record's number
    /// inside another's. It needs text to land in, and the record it names is a binding like any other.
    #[test]
    fn a_number_written_into_text_needs_text_to_land_in() {
        let road = |with: &str| {
            format!(
                "id: x\ntitle: y\nsteps_cli:\n  - type: action\n    domain: task\n    op: create\n    with: {{ title: T }}\n    as: seed\n  - type: action\n    domain: task\n    op: update\n    with: {{ target: seed, {with} }}\n"
            )
        };
        let errs = |with: &str| {
            load_str(&road(with)).unwrap().validate().unwrap_err().iter().map(|e| e.message.clone()).collect::<Vec<_>>().join(" / ")
        };

        load_str(&road("notes: SEE, mentions: seed")).unwrap().validate().expect("notes is text to land in");
        load_str(&road("title: SEE, mentions: seed")).unwrap().validate().expect("and so is a title");

        assert!(
            errs("due: today, mentions: seed").contains("needs a `notes` or a `title`"),
            "a day is not text: {}", errs("due: today, mentions: seed")
        );
        assert!(errs("notes: SEE, mentions: ghost").contains("does not resolve"), "the record is a binding");
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

    /// The refusal vocabulary: an action may declare that Amenbo will turn it away, and the code it
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

    /// The world a road about a first-time reader's screen opens on: a device with nothing raised on
    /// it. The driver raises a project to have somewhere to file a premise, so this is the one world
    /// a scenario cannot arrive at by declaring records — it declares the absence of them instead.
    #[test]
    fn a_premise_may_take_the_device_back_to_nothing_raised_on_it() {
        let yaml = r#"
id: x
title: y
given:
  - type: action
    domain: store
    op: nothing-raised
steps_gui:
  - type: action
    domain: project
    op: create
    with: { name: Greenhouse }
"#;
        load_str(yaml).unwrap().validate().expect("a premise may empty the device");
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

        // The same row in another language is held to the same shape, and to the base row beside it:
        // a label translated onto an entry declaring no setting is a label no form will ever show.
        let errs = stand("        - { name: standup, desc: d, translated: { de: { label: Beschriftung } } }").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`translated.de.label` names a `setting`")), "{errs:?}");

        let errs = stand("        - { name: standup, desc: d, translated: { de: { desc: 12 } } }").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`translated.de.desc` must be a string")), "{errs:?}");

        // And the text an opened panel is read by, held the same way its line is — including to the
        // base row, which is where Amenbo's own manifest check holds it too.
        let errs = stand("        - { name: standup, desc: d, about: 12 }").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`about` must be a string")), "{errs:?}");

        let errs = stand("        - { name: standup, desc: d, translated: { de: { about: Beschreibung } } }").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`translated.de.about` translates an `about`")), "{errs:?}");

        assert!(stand("        - { name: standup, desc: d, about: What it does, translated: { de: { about: Was es tut } } }").is_ok());

        // A language that translates none of the words is a document published with nothing in it.
        let errs = stand("        - { name: standup, desc: d, translated: { de: {} } }").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`translated.de` translates nothing")), "{errs:?}");

        let errs = stand("        - { name: standup, desc: d, translated: de }").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`translated` must be a mapping")), "{errs:?}");

        assert!(
            stand(
                "        - { name: standup, desc: d, setting: channel, label: L, translated: { de: { desc: Beschreibung, label: Beschriftung } } }"
            )
            .is_ok()
        );
    }

    /// The words a declared field carries in another language are held to their own shape, for the
    /// reason a shelf's rows are: they are a form's fields rather than Amenbo's arguments, so nothing
    /// else would catch a typo before it reached a screen as a blank.
    #[test]
    fn a_declarations_other_languages_are_held_to_the_shape_a_form_reads() {
        let declare = |translated: &str| {
            let yaml = format!(
                r#"
id: x
title: y
steps_gui:
  - type: action
    domain: plugin
    op: declare-setting
    with:
      name: worktree
      key: base
      label: Base branch
{translated}
"#
            );
            load_str(&yaml).unwrap().validate()
        };

        let errs = declare("      translated: de").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`translated` must be a mapping")), "{errs:?}");

        let errs = declare("      translated: { de: { label: 12 } }").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`translated.de.label` must be a string")), "{errs:?}");

        // A candidate's words are keyed by the value it stores, so a list has nowhere to say which is which.
        let errs = declare("      translated: { de: { options: [Aufgabe erledigt] } }").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`translated.de.options` must be a mapping")), "{errs:?}");

        let errs = declare("      translated: { de: {} }").unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`translated.de` translates nothing")), "{errs:?}");

        assert!(declare("      translated: { de: { label: Basis-Branch } }").is_ok());
        assert!(
            declare("      translated: { de: { label: Ereignisse, options: { task.done: Aufgabe erledigt } } }")
                .is_ok()
        );
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

    /// The kind a binding carries, and the spelling it turns into. A driver that got this backwards
    /// would file a decision onto whatever task happens to carry the same digits — and be told
    /// nothing, both refs resolving to a live row. The domains that bind neither answer `None`
    /// rather than falling through to a default.
    #[test]
    fn a_binding_is_spelled_by_the_kind_the_domain_binds() {
        assert_eq!(BoundKind::of_domain(Domain::Task), Some(BoundKind::Task));
        assert_eq!(BoundKind::of_domain(Domain::Decision), Some(BoundKind::Decision));
        assert_eq!(BoundKind::of_domain(Domain::Project), None);
        assert_eq!(BoundKind::of_domain(Domain::Attachment), None);
        assert_eq!(BoundKind::Task.spell(42), "AMB-T-42");
        assert_eq!(BoundKind::Decision.spell(42), "AMB-D-42");
        assert_eq!(BoundKind::Task.noun(), "task");
        assert_eq!(BoundKind::Decision.noun(), "decision");
    }
}
