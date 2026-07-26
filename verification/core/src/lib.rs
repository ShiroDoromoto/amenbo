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
//!     every `target:` resolves to an earlier `as:` binding.

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
    /// A domain operation that changes state.
    Action {
        domain: Domain,
        op: String,
        /// Named arguments for the op. A string value under the key `target` is a
        /// reference to an earlier step's `as:` binding.
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
/// only inspects the few keys it validates (`target`, `present`).
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
    /// Whether this op may carry an `as:` binding (true only for ops that produce an object).
    binds: bool,
}

const REGISTRY: &[OpSpec] = &[
    // Actions
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "create", required: &["title"], binds: true },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "assign", required: &["target", "assignee"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "comment", required: &["target", "text"], binds: false },
    // The progress states, each by the command a user reaches for: `status` is the explicit move
    // (and the reserve), `done` / `reopen` / `block` are the three the CLI gives their own verb.
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "status", required: &["target", "status"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "done", required: &["target"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "reopen", required: &["target"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Task, op: "block", required: &["target", "reason"], binds: false },
    OpSpec { kind: Kind::Action, domain: Domain::Decision, op: "create", required: &["title"], binds: true },
    // Asserts
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "listed", required: &["filter"], binds: false },
    OpSpec { kind: Kind::Assert, domain: Domain::Task, op: "field", required: &["target", "field", "equals"], binds: false },
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
    /// op for each step, its required args present, and every `target:` resolving to an
    /// earlier `as:` binding. Returns every problem found, not just the first.
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

            // A `target:` must name a binding introduced by an earlier action's `as:`.
            if let Some(v) = step.with().get("target") {
                match v.as_str() {
                    Some(name) if bound.contains(name) => {}
                    Some(name) => errs.push(at(
                        i,
                        format!("`target: {name}` does not resolve to an earlier `as:` binding"),
                    )),
                    None => errs.push(at(i, "`target` must be a string binding name".to_string())),
                }
            }

            // `present`, when given, is a boolean.
            if let Some(v) = step.with().get("present") {
                if v.as_bool().is_none() {
                    errs.push(at(i, "`present` must be a boolean".to_string()));
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

    #[test]
    fn empty_steps_is_rejected() {
        let s = load_str("id: x\ntitle: y\nsteps: []\n").unwrap();
        let errs = s.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("no steps")));
    }
}
