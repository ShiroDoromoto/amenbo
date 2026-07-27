//! amenbo-scenario — the schema and validating loader for pre-distribution
//! verification scenarios.
//!
//! A scenario is the single source of truth: one declarative YAML file states a domain
//! procedure plus its expected results, driver-independent. Every driver reads the SAME
//! file — the CLI driver (`verification/cli`) maps each step to a shipped-binary
//! invocation, the mac GUI harness (`verification/gui`) maps it to a screen instruction,
//! the Linux OCR harness (`scripts/docker/gui-e2e.sh`) is fed the card title its host
//! launcher (`make verify-gui-linux`) resolves through this crate's JSON face (the `emit`
//! bin), since that container carries no toolchain to read the YAML itself. Nothing here
//! knows about a command line or a pixel.
//!
//! Two layers of checking, both surfaced as clear failures:
//!   * [`load_str`] / [`load_file`] — the YAML must parse into the typed model
//!     (`deny_unknown_fields` catches misspelled keys).
//!   * [`Scenario::validate`] — the semantic pass: known ops only, required args present,
//!     each arg of the type its op takes, and every `target:` resolving to an earlier
//!     `as:` binding.

use std::collections::HashSet;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// A whole scenario: an ordered list of steps under an id and a human title.
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
    /// The drivers this scenario is written to be run through. **Absent means CLI alone**, which is
    /// where a line belongs unless it is one of the few the screen is the only place to see.
    ///
    /// One scenario source, but not every driver has to copy every line: adding one costs a driver
    /// what that driver costs. The CLI driver runs unattended and closes on an exit code, so its set
    /// aims at the whole capability list; the GUI harnesses shoot the screen, read it back with OCR
    /// and leave a `field` assert to a human eye, so theirs is a chosen few. Declaring it here keeps
    /// the choice in the source of truth rather than in each driver's idea of what it should skip.
    #[serde(default = "cli_only")]
    pub drivers: Vec<Driver>,
    /// The ordered steps. Must be non-empty.
    pub steps: Vec<Step>,
}

/// A driver a scenario can be run through — the harnesses that map the one source to their world.
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
    /// The wire token, as it is written in a scenario.
    pub fn as_str(self) -> &'static str {
        match self {
            Driver::Cli => "cli",
            Driver::Gui => "gui",
        }
    }
}

/// The default driver set: the CLI alone.
fn cli_only() -> Vec<Driver> {
    vec![Driver::Cli]
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
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "create", required: &["title"], refs: &[], strings: &["title"], binds: true },
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
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "create", required: &["title"], refs: &[], strings: &["title"], binds: true },
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
    // The store as a whole. `export` and `backup` write a file the run keeps, and bind it by the
    // same `as:` every other producing op uses — so `restore` names the archive it puts back the
    // way a step names any other earlier result, and a mistyped name is caught here rather than in
    // a driver.
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "export", required: &[], refs: &[], strings: &[], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "backup", required: &[], refs: &[], strings: &[], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Store, op: "restore", required: &["target"], refs: &["target"], strings: &[], binds: false },
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
    // Hanging bytes or a link on a record. Each `attach` names either a `file` the run wrote or a
    // `url`, and binds the attachment, since managing one afterwards means naming it.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "attach", required: &["target"], refs: &["target"], strings: &["file", "url", "name"], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "attach", required: &["target"], refs: &["target"], strings: &["file", "url", "name"], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Comment, op: "attach", required: &["target"], refs: &["target"], strings: &["file", "url", "name"], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Attachment, op: "rm", required: &["target"], refs: &["target"], strings: &[], binds: false },
    // The folder the run works in: the files a person already has there, the repository the hooks
    // are written into, and the two hook commands themselves.
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "write-file", required: &["path", "content"], refs: &[], strings: &["path", "content"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "copy-fixture", required: &["from", "path"], refs: &[], strings: &["from", "path"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "git-init", required: &[], refs: &[], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "hooks-install", required: &[], refs: &[], strings: &[], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Repo, op: "hooks-uninstall", required: &[], refs: &[], strings: &[], binds: false },
    // A plugin's life on this machine. `install` fetches it from the catalog and `enable` opens its
    // gate — two separate acts on purpose, since an installed plugin that never fires is the normal
    // state. `run` calls the command face: `command` is the word the plugin's own face takes, `task`
    // hands it the id of a task an earlier step created, and `args` carries anything else verbatim.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "install", required: &["name"], refs: &[], strings: &["name"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "enable", required: &["name"], refs: &[], strings: &["name"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "disable", required: &["name"], refs: &[], strings: &["name"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "uninstall", required: &["name"], refs: &[], strings: &["name"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "run", required: &["name", "command"], refs: &["task"], strings: &["name", "command"], binds: false },
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
    // An installed plugin declaring a setting its author marked secret. Which settings a plugin takes
    // is the author's word and amenbo never invents one, so the only honest way to reach this state is
    // for a plugin that declares one to be published — and no plugin in the official catalog does. The
    // secret route (off the store, off every backup, injected as an environment variable) is the half
    // of `plugin config` that fails silently and in plain text, so it is not left unwalked until one
    // is: the driver writes the declaration onto the installed manifest, the way `stale-manifest`
    // writes the disagreement it needs. Everything after it is amenbo's own doing.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "declare-secret", required: &["name", "key"], refs: &[], strings: &["name", "key", "label"], binds: false },
    // What an installed plugin is told. `key` is a setting its author declared, and `scope` picks the
    // tier the value is written at (the machine default, or this project's override); an empty value
    // is how one is taken back, which is why it is a value here and not an op of its own.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "config-set", required: &["name", "key", "value"], refs: &[], strings: &["name", "key", "value", "scope"], binds: false },
    // The catalogs a browse reads. A third-party one is named by the URL of its `catalog.json`, and
    // that URL is the handle for taking it back off again — there is nothing else to name it by.
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "catalog-add", required: &["url"], refs: &[], strings: &["url"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Plugin, op: "catalog-remove", required: &["url"], refs: &[], strings: &["url"], binds: false },
    // Asserts
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "listed", required: &["filter"], refs: &["target"], strings: &["filter", "position"], binds: false },
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
    OpSpec { kind: Kind::Assert, domain: Domain::Store, op: "snapshot", required: &["target"], refs: &["target"], strings: &["absent"], binds: false },
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
    // An attachment read back three ways: its own row, the owner's list it hangs in, and — for a
    // blob — the bytes coming out again, which is the only proof the ingest kept them.
    OpSpec { kind: Kind::Assert, domain: Domain::Attachment, op: "field", required: &["target", "field", "equals"], refs: &["target"], strings: &["field"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Attachment, op: "listed", required: &["target", "owner", "owner_kind"], refs: &["target", "owner"], strings: &["owner_kind", "position"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Attachment, op: "saved", required: &["target", "content"], refs: &["target"], strings: &["content"], binds: false },
    // The repository-side gates: what the lint found in a file, and what is in a hook slot.
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "lint", required: &["path", "hits"], refs: &[], strings: &["path"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Repo, op: "hooks", required: &["hook", "state"], refs: &[], strings: &["hook", "state"], binds: false },
    // A project as it is read back: one row's fields, and whether it is in the listing at all (and
    // where). `archived: true` asks the listing that carries the archived ones.
    OpSpec { kind: Kind::Assert, domain: Domain::Project, op: "field", required: &["target", "field", "equals"], refs: &["target"], strings: &["field"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Project, op: "listed", required: &["target"], refs: &["target"], strings: &["position"], binds: false },
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
    // What is on this machine and whose gate is open (`enabled` asks the gate; without it the
    // question is only whether the plugin is there at all), what the last call returned on its own
    // stdout, and what the execution log kept of a run.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "listed", required: &["name"], refs: &[], strings: &["name"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "returned", required: &["contains"], refs: &[], strings: &["contains"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "ran", required: &["name"], refs: &[], strings: &["name", "outcome"], binds: false },
    // Whether the catalog holds a different build of an installed plugin — the question `update --check`
    // answers, and the only way to read from outside which build a machine is on (a manifest carries no
    // version number, so there is no number to compare).
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "outdated", required: &["name", "present"], refs: &[], strings: &["name"], binds: false },
    // A setting read back at the tier it was written at — `equals` for the value, or `set: false` to
    // ask that the tier holds none. `scope` names the tier, since a read is per-tier and not the
    // precedence a run would apply. `secret: true` asks the other thing a read of a secret has to be
    // true of: that it says so, and that the value does not come out with it.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "config", required: &["name", "key"], refs: &[], strings: &["name", "key", "scope"], binds: false },
    // A catalog in the browsing view: whether it is a source at all (`present`), whether the browse
    // could reach it, and how many plugins it offers.
    OpSpec { kind: Kind::Assert, domain: Domain::Plugin, op: "catalog", required: &["url"], refs: &[], strings: &["url"], binds: false },
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

/// A single semantic problem, anchored to the step that carries it (0-based, or `None`
/// for a scenario-wide problem such as an empty id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub step: Option<usize>,
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.step {
            Some(i) => write!(f, "step {}: {}", i + 1, self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl Scenario {
    /// Whether this scenario is written to be run through `driver`.
    pub fn runs_on(&self, driver: Driver) -> bool {
        self.drivers.contains(&driver)
    }

    /// The declared drivers as their wire tokens, for a message that has to name them.
    pub fn driver_tokens(&self) -> Vec<&'static str> {
        self.drivers.iter().map(|d| d.as_str()).collect()
    }

    /// Check the semantic rules the type system cannot: non-empty id/title/steps, a known
    /// op for each step, its required args present and of the type it takes, and every
    /// `target:` resolving to an earlier `as:` binding. Returns every problem found, not
    /// just the first.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errs = Vec::new();
        let whole = |m: &str| ValidationError { step: None, message: m.to_string() };
        let at = |i: usize, m: String| ValidationError { step: Some(i), message: m };

        if self.id.trim().is_empty() {
            errs.push(whole("id is empty"));
        }
        if self.title.trim().is_empty() {
            errs.push(whole("title is empty"));
        }
        if self.steps.is_empty() {
            errs.push(whole("scenario has no steps"));
        }
        // A scenario no driver runs is one nothing keeps honest: it rots silently while the set it
        // sits in reports green.
        if self.drivers.is_empty() {
            errs.push(whole("scenario names no driver to run it"));
        }

        let mut bound: HashSet<&str> = HashSet::new();
        for (i, step) in self.steps.iter().enumerate() {
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

            // The two yes/no args are booleans wherever they appear: `present` asks whether
            // something is there, `ok` asks what verdict a check is expected to come back with.
            for key in ["present", "ok"] {
                if let Some(v) = step.with().get(key) {
                    if v.as_bool().is_none() {
                        errs.push(at(i, format!("`{key}` must be a boolean")));
                    }
                }
            }

            // A step that says its operation will be turned away. It is an action's word — an
            // assert already comes back with a verdict of its own — and what it names is the code
            // the refusal has to carry, so a step written against one guard cannot pass on another
            // guard's refusal.
            if let Some(v) = step.with().get("refused") {
                if step.kind() == Kind::Assert {
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

        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
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
steps:
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
        assert_eq!(s.steps.len(), 3);
    }

    #[test]
    fn unknown_key_is_a_parse_error() {
        let yaml = "id: x\ntitle: y\nbogus: 1\nsteps: []\n";
        assert!(load_str(yaml).is_err());
    }

    #[test]
    fn unknown_op_is_rejected() {
        let yaml = r#"
id: x
title: y
steps:
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
steps:
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
steps:
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
steps:
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
steps:
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
steps:
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
steps:
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

    /// A scenario that says nothing about drivers is a CLI scenario — the set aims at coverage there,
    /// so that is the answer for a line that does not go out of its way to ask for the screen.
    #[test]
    fn a_scenario_that_names_no_driver_is_cli_only() {
        let s = load_str(GOOD).expect("parses");
        assert!(s.runs_on(Driver::Cli));
        assert!(!s.runs_on(Driver::Gui), "the screen is asked for, never assumed");
    }

    #[test]
    fn a_declared_driver_set_is_read_back() {
        let yaml = format!("drivers: [cli, gui]{GOOD}");
        let s = load_str(&yaml).expect("parses");
        assert!(s.runs_on(Driver::Cli) && s.runs_on(Driver::Gui));
        assert_eq!(s.driver_tokens(), vec!["cli", "gui"]);
    }

    #[test]
    fn a_driver_outside_the_set_is_a_parse_error() {
        let yaml = format!("drivers: [tui]{GOOD}");
        assert!(load_str(&yaml).is_err(), "an unknown driver is a typo, not an extension point");
    }

    #[test]
    fn naming_no_driver_at_all_is_rejected() {
        let yaml = format!("drivers: []{GOOD}");
        let errs = load_str(&yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("no driver")));
    }

    /// The file a `store` action writes is named back through the one binding namespace every
    /// other result uses, so an archive named at the wrong step is caught here and not by a driver
    /// looking for a file nobody wrote.
    #[test]
    fn a_store_action_binds_the_file_it_wrote() {
        let yaml = r#"
id: x
title: y
steps:
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
steps:
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
steps:
  - type: assert
    domain: store
    op: doctor
    with: { ok: "yes" }
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("`ok` must be a boolean")));
    }

    /// The refusal vocabulary: an action may declare that amenbo will turn it away, and the code it
    /// will be turned away with. The op and its args are the ordinary ones — what is under test is
    /// the guard in front of them, not a second spelling of the command.
    #[test]
    fn an_action_may_declare_the_refusal_it_expects() {
        let yaml = r#"
id: x
title: y
steps:
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
steps:
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
steps:
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
steps:
  - type: action
    domain: task
    op: create
    with: { title: T, refused: out_of_reach }
    as: ghost
"#;
        let errs = load_str(yaml).unwrap().validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("produces nothing")));
    }

    #[test]
    fn empty_steps_is_rejected() {
        let s = load_str("id: x\ntitle: y\nsteps: []\n").unwrap();
        let errs = s.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("no steps")));
    }
}
