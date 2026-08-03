//! The CLI driver's reusable core: drive the **shipped / installed**
//! `amenbo` binary against one scenario in an isolated throwaway store, and judge the asserts
//! from that binary's `--json` output. A black-box driver — it knows the domain vocabulary, not
//! the build under test. Shipped is the whole of what it will drive, and the binary is asked to
//! prove it is one before a run starts (`shipped`).
//!
//! Two bins sit on top of this: `verify-cli` runs one scenario, `verify-all` runs a whole set
//! and aggregates. Both call [`run_scenario`] and read a [`Report`]; the isolation, the driver
//! and the reporting live here so the two share one contract.
//!
//! What this file is, and is not: the machinery every step stands on — the isolated session, the
//! one invocation every call goes through, the bindings a step names an earlier step by, and the
//! report. The steps themselves live in [`domain`], one module per domain, and are reached by
//! handing each step to the domain it names.

/// The throwaway store an amenbo run is given. Public because every bin in this crate needs it:
/// asking the shipped binary anything at all means giving it a home that is not the user's.
pub mod scratch;

mod domain;
mod judge;
mod shipped;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use amenbo_scenario::{Args, Domain, Scenario, Step};

use crate::domain::plugin::StoodCatalog;

/// Load, isolate, execute, judge — one scenario against one binary. `Err` is an execution error
/// (the scenario would not load, the binary is not one a run may drive, it would not run); a
/// scenario that ran but had a failing assert comes back as an `Ok(Report)` with `passed == false`.
///
/// The binary is asked which build it is before anything is run against it (`shipped`): this is
/// the one door both bins come through, so the line holds for a single scenario and for a whole set
/// without either bin restating it.
pub fn run_scenario(scenario: &Scenario, bin: &Path, keep: bool) -> Result<Report, String> {
    shipped::ensure_release_build(bin)?;
    let session = scratch::session(&scenario.id, keep)
        .map_err(|e| format!("could not create a throwaway store: {e}"))?;
    let mut driver = Driver::new(bin, &session)?;

    let mut report = Report::new(scenario);
    // The world the road takes for granted, stood up before the road is walked. It is the same driver
    // and the same throwaway store, so a premise is written in the one vocabulary a road is, and the
    // `as:` names it binds are the names the road calls them by.
    //
    // **A premise that does not stand ends this scenario, red, right here.** Walking on would judge
    // the road against a world half built, and every line it then wrote — pass or fail — would be
    // about the wrong thing. It is this scenario's failure and not the run's, so it comes back as a
    // report rather than an error: a set keeps going, and the line says the premise was what broke.
    //
    // Walked here rather than through `stand_world`, and the two are not the same job. That one hands
    // a screen run a world and then gets out of the way, so it takes a driver of its own; here the
    // road that follows is driven from this one, and a premise's `as:` names have to be in its hands
    // for the road to call them by name at all.
    for (i, step) in scenario.given.iter().enumerate() {
        let outcome = match driver.exec(step) {
            Ok(outcome) => outcome,
            Err(error) => Outcome::assert(false, error),
        };
        let stood = outcome.pass;
        report.push_premise(i, outcome);
        if !stood {
            return Ok(report);
        }
    }
    for (i, step) in scenario.steps(amenbo_scenario::Driver::Cli).iter().enumerate() {
        let outcome = driver.exec(step)?; // an execution error aborts this scenario's run
        report.push(i, step, outcome);
    }
    Ok(report)
}

/// Stand up the world a scenario declared (`given`) in the store `session` names, using the shipped
/// binary at `bin`. The premise is walked as the plain actions it is, and an `Err` from any of them
/// stops there: a road is walked from the world it declared or not at all, since a run that started
/// on half of one would report on a screen nobody meant to stand in front of.
///
/// This is the other driver's way in. The GUI harness stands its world up here rather than through a
/// vocabulary of its own, because what a premise names are the same ops the CLI road names, and two
/// spellings of `plugin install` would drift the moment one of them learned something.
///
/// The CLI road does not come through here, and that is not an oversight: it stands its premise up in
/// the very driver that then walks it ([`run_scenario`]), which is the only way the names a premise
/// binds are still in hand when the road calls them. A screen run has no such need — the hands that
/// walk it are a person's.
///
/// What comes back is held rather than dropped, and that is not a formality: part of a world can be
/// a thing that only stands while something holds it — a catalog the premise put on the loopback
/// answers for exactly as long as this does.
pub fn stand_world<'a>(
    bin: &Path,
    session: &'a scratch::Session,
    given: &[Step],
) -> Result<World<'a>, String> {
    let mut driver = Driver::new(bin, session)?;
    let mut stood = Vec::new();
    for (i, step) in given.iter().enumerate() {
        // The premise's own numbering, the way the loader's errors read it: a world's step is not a
        // road's, and a message that said "step 2" would send a reader to the wrong list.
        let outcome = driver.exec(step).map_err(|e| format!("given step {}: {e}", i + 1))?;
        stood.push(outcome.note);
    }
    Ok(World { stood, _driver: driver })
}

/// A world that has been stood up, and is standing for as long as this is held.
pub struct World<'a> {
    stood: Vec<String>,
    /// Held, never read: the driver owns what a premise put on the loopback, and dropping it would
    /// close the port a registration is pinned to while the run is still pointed at it.
    _driver: Driver<'a>,
}

impl World<'_> {
    /// What the premise did, a line per step, in the same words an action reports itself with on a
    /// road — so what a run stood on is readable beside what it then walked.
    pub fn stood(&self) -> &[String] {
        &self.stood
    }
}

/// Pin the binary under test to where the caller named it, while we are still standing there.
/// Every run drives the binary from a throwaway cwd, so a relative path handed in on `--bin` (or
/// `$AMENBO_BIN`) would be counted from *that* directory and land on nothing — and the failure it
/// raises names the binary, never the directory it looked in, so the path itself reads as the
/// suspect. Resolving it at the door is what keeps `--bin ../dist/amenbo` — an artifact unpacked
/// beside the repository — meaning what the caller sees. A bare name carries no separator and is a
/// `PATH` lookup, so it is left alone.
pub fn anchor_bin(bin: PathBuf) -> PathBuf {
    let names_a_path = bin.parent().is_some_and(|p| !p.as_os_str().is_empty());
    if bin.is_absolute() || !names_a_path {
        return bin;
    }
    match std::env::current_dir() {
        Ok(cwd) => cwd.join(bin),
        // Nowhere to anchor to: hand back what we were given and let the run report its own failure.
        Err(_) => bin,
    }
}

/// Drives the shipped binary against one isolated store, remembering the ids that steps bind.
///
/// The store is borrowed rather than owned, because a driver is not always the only one working in
/// it: the GUI harness stands a world up through one and then launches the app under test at the
/// same store, and the borrow is what says the store outlives both.
pub(crate) struct Driver<'a> {
    bin: std::path::PathBuf,
    session: &'a scratch::Session,
    project_id: i64,
    bindings: HashMap<String, i64>,
    /// What the last `plugin run` came back with. A command face's return value is its own stdout
    /// and is deliberately kept out of the execution log, so this is the only place a later step can
    /// read it from — which is why the assert that reads it has to follow its call.
    last_run: Option<serde_json::Value>,
    /// What the last `unbind` answered. Kept for the reason the line above is: how many folders the
    /// project has left is part of that answer, and afterwards there is only the state it left —
    /// which reads the same whether the answer mentioned it or not.
    last_unbind: Option<serde_json::Value>,
    /// What the last `plugin flush` reported. Kept for the same reason as the line above: what a
    /// flush got through, and which queues it stepped around, is said once as it returns and is
    /// nowhere to be read afterwards — the store shows the state, not who declined to touch it.
    last_flush: Option<serde_json::Value>,
    /// The files the `store` actions wrote, under the same names. A scenario has one binding
    /// namespace — the loader keeps it unique across both — and which of the two maps a name lands
    /// in follows from the op that bound it: nothing in the store is a path, and no archive is an id.
    artifacts: HashMap<String, std::path::PathBuf>,
    /// The catalogs the run stood up itself, under the names their steps bound. They are held here
    /// for the length of the scenario because a host answers only while it is alive: dropping one
    /// after the step that made it would leave every later step pointed at a closed port.
    catalogs: HashMap<String, StoodCatalog>,
    /// Set while a step that declared `refused:` is running. It is read where a failed invocation
    /// is judged, so the arm issuing the command never has to know it might be turned away —
    /// [`Driver::refused`] puts it up and takes it back down around the one call.
    refusal: Option<String>,
}

/// What an expected refusal travels back on. A refusal has to reach [`Driver::refused`] from
/// wherever the command was issued, and the way out of an arm that every one of them already has is
/// the `?` on its invocation — so it goes as an `Err`, and a byte no message of ours carries keeps
/// it apart from a real failure. The code that came back is spliced on after it.
const REFUSED: &str = "\u{1}refused:";

impl<'a> Driver<'a> {
    /// Boot a fresh store: `init` creates it and hands back the project every `task add` needs.
    fn new(bin: &Path, session: &'a scratch::Session) -> Result<Driver<'a>, String> {
        let mut d = Driver {
            bin: bin.to_path_buf(),
            session,
            project_id: 0,
            bindings: HashMap::new(),
            last_run: None,
            last_unbind: None,
            last_flush: None,
            artifacts: HashMap::new(),
            catalogs: HashMap::new(),
            refusal: None,
        };
        let v = d.run_json(&["init", "--name", "verify", "--json"])?;
        d.project_id = v["identity"]["project_id"]
            .as_i64()
            .ok_or("init did not report a project_id")?;
        Ok(d)
    }

    /// Spawn the shipped binary in the isolated store, from a chosen folder. Every call goes
    /// through here, so the isolation is stated once and cannot be forgotten by an arm that builds
    /// its own command. Where the command stands is itself an input for anything to do with binding
    /// — the pointer that decides what a run reaches is found by walking up from the CWD — so those
    /// steps ask their question from inside the folder they are asking about.
    fn invoke_in(&self, cwd: &Path, args: &[&str]) -> Result<std::process::Output, String> {
        // The facet goes on the command line, which is the one input amenbo is to take it by; a call
        // that names its own is left alone.
        let mut with_facet = args.to_vec();
        if !args.contains(&"--actor") {
            with_facet.extend_from_slice(&["--actor", "human"]);
        }
        Command::new(&self.bin)
            .args(&with_facet)
            .current_dir(cwd)
            .env("AMENBO_HOME", &self.session.home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .env("NO_COLOR", "1")
            .output()
            .map_err(|e| format!("could not run `{}`: {e}", self.bin.display()))
    }

    /// The same, from the run's own CWD — where every step that is not asking about a folder stands.
    fn invoke(&self, args: &[&str]) -> Result<std::process::Output, String> {
        self.invoke_in(&self.session.cwd, args)
    }

    /// Run `amenbo <args>` in the isolated store and parse its `--json` output. An `Err` is an
    /// execution failure (spawn failed, non-JSON output, non-zero exit, or an `error` object) —
    /// distinct from an assert that ran cleanly and came out false.
    fn run_json(&self, args: &[&str]) -> Result<serde_json::Value, String> {
        self.run_json_in(&self.session.cwd, args)
    }

    /// The same, from a chosen folder.
    fn run_json_in(&self, cwd: &Path, args: &[&str]) -> Result<serde_json::Value, String> {
        let out = self.invoke_in(cwd, args)?;
        if let Some(code) = self.refused_code(&out) {
            return Err(format!("{REFUSED}{code}"));
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let v = parse_json(args, &stdout)?;
        if !out.status.success() || v.get("error").is_some() {
            return Err(format!("`amenbo {}` failed: {}", args.join(" "), stdout.trim()));
        }
        Ok(v)
    }

    /// The same, for a command whose exit code is its **verdict** rather than a report on whether
    /// it ran: `doctor` and `validate` come back non-zero when what they found is bad news, and
    /// that is a value to judge, not a driver failure. A command that could not run at all still
    /// says so in an `error` object, which is an `Err` here as everywhere.
    fn run_check(&self, args: &[&str]) -> Result<serde_json::Value, String> {
        let out = self.invoke(args)?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let v = parse_json(args, &stdout)?;
        if v.get("error").is_some() {
            return Err(format!("`amenbo {}` failed: {}", args.join(" "), stdout.trim()));
        }
        Ok(v)
    }

    /// Run for the file it leaves behind rather than for what it prints — `export --out` answers
    /// with a directory on disk and prose on stdout, so there is no JSON to read and the exit code
    /// is the whole signal. stderr carries the reason when it fails.
    fn run_bare(&self, args: &[&str]) -> Result<(), String> {
        let out = self.invoke(args)?;
        if let Some(code) = self.refused_code(&out) {
            return Err(format!("{REFUSED}{code}"));
        }
        if !out.status.success() {
            return Err(format!(
                "`amenbo {}` failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(())
    }

    /// Resolve a path a step named against the session's own folder, refusing anything that would
    /// reach outside it. A scenario writes and lints files in the throwaway CWD and nowhere else —
    /// this driver is handed real machines to run on, so an absolute path or a `..` in a scenario is
    /// refused rather than followed.
    fn in_session(&self, path: &str) -> Result<std::path::PathBuf, String> {
        let p = Path::new(path);
        if p.is_absolute() || p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err(format!("`path: {path}` must stay inside the run's own folder"));
        }
        Ok(self.session.cwd.join(p))
    }

    /// The code a refusal came back with, but only while a step is expecting one. amenbo prints the
    /// error object on **stderr** and leaves stdout empty, so a refusal is read off the stream it is
    /// actually on rather than the one a result would come back on. `None` when no refusal is
    /// expected, when the command succeeded, or when the failure carries no error object at all —
    /// that last is a binary that could not run, and it stays an execution error.
    fn refused_code(&self, out: &std::process::Output) -> Option<String> {
        self.refusal.as_ref()?;
        if out.status.success() {
            return None;
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        let v: serde_json::Value = serde_json::from_str(stderr.trim()).ok()?;
        Some(v["error"]["code"].as_str()?.to_string())
    }

    /// Map one step to a binary call. Returns whether the step passed — an assert's verdict, or a
    /// `refused:` action's — plus a human note. An action with nothing to prove passes unless it
    /// errors.
    fn exec(&mut self, step: &Step) -> Result<Outcome, String> {
        match step {
            Step::Action { domain, op, with, bind } => match with.get("refused") {
                Some(code) => {
                    let want = code
                        .as_str()
                        .ok_or("`refused` must be the error code the operation is expected to be rejected with")?
                        .to_string();
                    self.refused(*domain, op, with, &want)
                }
                None => self.action(*domain, op, with, bind.as_deref()),
            },
            Step::Assert { domain, op, with } => self.assert(*domain, op, with),
        }
    }

    /// Run an operation the step says amenbo will turn away, and judge the refusal — the guard in
    /// front of an operation is only proven by meeting it, and a driver that reads every non-zero
    /// exit as its own failure can never write that line down.
    ///
    /// The op arms are left exactly as they are: the invocation recognises the refusal and unwinds
    /// the arm through the `?` it already has on its command, which lands back here. So the verdict
    /// is: refused with the code the step named → pass; refused with another code → fail, since the
    /// step is about *that* guard; went through → fail, which is the regression this exists to catch.
    fn refused(&mut self, domain: Domain, op: &str, with: &Args, want: &str) -> Result<Outcome, String> {
        self.refusal = Some(want.to_string());
        // A refused op binds nothing, so no binding is offered to the arm (the loader refuses `as:`
        // on one, and there would be no id to put under the name anyway).
        let outcome = self.action(domain, op, with, None);
        self.refusal = None;
        match outcome {
            Err(e) => match e.strip_prefix(REFUSED) {
                Some(code) => Ok(Outcome::assert(
                    code == want,
                    format!(
                        "`{op}` was refused with `{code}` ({})",
                        if code == want {
                            "as expected".to_string()
                        } else {
                            format!("expected `{want}`, MISMATCH")
                        }
                    ),
                )),
                None => Err(e), // it did not get as far as a refusal — an execution error as usual
            },
            Ok(done) => Ok(Outcome::assert(
                false,
                format!("`{op}` went through where `{want}` was expected to refuse it — {} (MISMATCH)", done.note),
            )),
        }
    }

    /// Run an action by handing it to the domain that answers for it. One op is named for what it
    /// acts on rather than for a domain of its own: attaching makes an attachment whether it is hung
    /// on a task, a decision or a comment, and it is the attachment side that knows how.
    fn action(&mut self, domain: Domain, op: &str, with: &Args, bind: Option<&str>) -> Result<Outcome, String> {
        if op == "attach" {
            return self.attachment_action(domain, op, with, bind);
        }
        match domain {
            Domain::Task => self.task_action(op, with, bind),
            Domain::Decision => self.decision_action(op, with, bind),
            Domain::Comment => self.comment_action(op, with, bind),
            Domain::Project => self.project_action(op, with, bind),
            Domain::Dimension => self.dimension_action(op, with),
            Domain::Store => self.store_action(op, with, bind),
            Domain::Folder => self.folder_action(op, with, bind),
            Domain::Attachment => self.attachment_action(domain, op, with, bind),
            Domain::Repo => self.repo_action(op, with),
            Domain::Plugin => self.plugin_action(op, with, bind),
        }
    }

    /// Judge an assert by handing it to the domain that answers for it. One op is named for what it
    /// is about rather than for a domain of its own: an export is read out of the archive whatever
    /// kind of row is being looked for in it.
    fn assert(&self, domain: Domain, op: &str, with: &Args) -> Result<Outcome, String> {
        if op == "exported" {
            return self.judge_exported(domain, with);
        }
        match domain {
            Domain::Task => self.task_assert(op, with),
            Domain::Decision => self.decision_assert(op, with),
            // A comment is read on the timeline of whatever holds it, so the domain answers for no
            // assert of its own — `exported` above is the one question asked about a comment itself.
            Domain::Comment => Err(unmapped(domain, op)),
            Domain::Project => self.project_assert(op, with),
            Domain::Dimension => self.dimension_assert(op, with),
            Domain::Store => self.store_assert(op, with),
            Domain::Folder => self.folder_assert(op, with),
            Domain::Attachment => self.attachment_assert(op, with),
            Domain::Repo => self.repo_assert(op, with),
            Domain::Plugin => self.plugin_assert(op, with),
        }
    }

    /// Where a `store` action's file goes: under the session's own scratch space, named after the
    /// binding that will name it back. A step that binds nothing still gets a slot of its own, so
    /// two unnamed backups never collide on one path — which `backup` would refuse outright.
    fn artifact(&self, bind: Option<&str>, kind: &str, ext: &str) -> std::path::PathBuf {
        let stem = match bind {
            Some(name) => name.to_string(),
            None => format!("{kind}-{}", self.artifacts.len()),
        };
        self.session.artifacts.join(format!("{stem}{ext}"))
    }

    /// Record what an action wrote, under the name a later step will ask for it by.
    fn remember(&mut self, bind: Option<&str>, kind: &str, path: std::path::PathBuf) {
        let name = match bind {
            Some(name) => name.to_string(),
            None => format!("{kind}-{}", self.artifacts.len()),
        };
        self.artifacts.insert(name, path);
    }

    /// Resolve a name to the file an earlier `store` action wrote. The loader proved the name
    /// resolves to an earlier `as:`, so what is left to catch here is a name bound by an op that
    /// produced an object rather than a file.
    fn artifact_ref(&self, with: &Args, key: &str) -> Result<std::path::PathBuf, String> {
        let name = req_str(with, key)?;
        self.artifacts.get(name).cloned().ok_or_else(|| {
            format!("`{key}: {name}` names no file a `store` action wrote in this run")
        })
    }

    /// The folder a step names. A scenario says *which* folder, never where it is: the driver places
    /// it, because a binding is answered by where a folder sits and only the driver knows where its
    /// own isolated run lives.
    fn folder(&self, with: &Args) -> Result<PathBuf, String> {
        let name = req_str(with, "dir")?;
        self.session
            .folder(name)
            .map_err(|e| format!("could not make the folder `{name}`: {e}"))
    }

    /// Resolve a step's `target:` to the id an earlier action bound. The loader already proved
    /// the name resolves to an earlier `as:`, so a miss here is an internal error, not user input.
    fn resolve(&self, with: &Args) -> Result<i64, String> {
        self.resolve_key(with, "target")
    }

    /// The same, for an op that names a second object under its own key (`decision link`'s `task`).
    fn resolve_key(&self, with: &Args, key: &str) -> Result<i64, String> {
        let name = req_str(with, key)?;
        self.bindings
            .get(name)
            .copied()
            .ok_or_else(|| format!("internal: binding `{name}` was never produced"))
    }
}

/// The outcome of one step.
pub(crate) struct Outcome {
    /// An assert's verdict — and an action's, when it declared `refused:` and is judged on whether
    /// it really was turned away. An ordinary action is `true` unless it errored out of `exec`.
    pass: bool,
    note: String,
}

impl Outcome {
    pub(crate) fn action(note: String) -> Outcome {
        Outcome { pass: true, note }
    }
    pub(crate) fn assert(pass: bool, note: String) -> Outcome {
        Outcome { pass, note }
    }
}

/// Read a binary's stdout as JSON, naming the call in the failure so a command that printed prose
/// (or nothing) is recognisable without re-running it by hand.
pub(crate) fn parse_json(args: &[&str], stdout: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(stdout.trim()).map_err(|e| {
        format!("`amenbo {}` did not print JSON ({e}); output was:\n{}", args.join(" "), stdout.trim())
    })
}

pub(crate) fn unmapped(domain: Domain, op: &str) -> String {
    format!(
        "op `{op}` for domain `{domain:?}` is in the scenario registry but not yet mapped in the CLI driver"
    )
}

/// A path as the binary takes it. Non-UTF-8 never comes up — these paths are the driver's own —
/// but it is refused rather than mangled into one that names something else.
pub(crate) fn path_str(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("path {} is not valid UTF-8", path.display()))
}

pub(crate) fn req_str<'a>(with: &'a Args, key: &str) -> Result<&'a str, String> {
    with.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("arg `{key}` must be a string"))
}

pub(crate) fn req_i64(with: &Args, key: &str) -> Result<i64, String> {
    with.get(key)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| format!("arg `{key}` must be a whole number"))
}

pub(crate) fn req_bool(with: &Args, key: &str) -> Result<bool, String> {
    opt_bool(with, key).ok_or_else(|| format!("arg `{key}` must be a boolean"))
}

pub(crate) fn opt_bool(with: &Args, key: &str) -> Option<bool> {
    with.get(key).and_then(|v| v.as_bool())
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

/// The verdict of one scenario run: the ordered per-step lines plus the roll-up. `passed` is the
/// AND of every step, so a runner reads it directly to aggregate across scenarios.
pub struct Report {
    scenario_id: String,
    title: String,
    steps: Vec<Line>,
    passed: bool,
}

struct Line {
    index: usize,
    kind: &'static str,
    pass: bool,
    note: String,
}

impl Report {
    fn new(s: &Scenario) -> Report {
        Report { scenario_id: s.id.clone(), title: s.title.clone(), steps: Vec::new(), passed: true }
    }

    /// A line for a step of the premise, numbered in its own sequence. It is named apart from the
    /// road because it answers a different question: a red one says the world could not be stood up,
    /// which is not the road failing — and a reader who cannot tell the two apart goes looking for a
    /// regression in what was never walked.
    fn push_premise(&mut self, index: usize, outcome: Outcome) {
        if !outcome.pass {
            self.passed = false;
        }
        self.steps.push(Line { index, kind: "premise", pass: outcome.pass, note: outcome.note });
    }

    fn push(&mut self, index: usize, step: &Step, outcome: Outcome) {
        let kind = match step {
            // A step that declared `refused:` is an action in the schema, but it comes back with a
            // verdict the way an assert does. Naming it apart says which, so a red line under
            // `action` never reads as the driver having tripped over its own command.
            Step::Action { with, .. } if with.contains_key("refused") => "refused",
            Step::Action { .. } => "action",
            Step::Assert { .. } => "assert",
        };
        if !outcome.pass {
            self.passed = false;
        }
        self.steps.push(Line { index, kind, pass: outcome.pass, note: outcome.note });
    }

    /// Did every step pass?
    pub fn passed(&self) -> bool {
        self.passed
    }

    /// The scenario's stable id.
    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    /// The scenario's human title.
    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn print_human(&self) {
        println!("scenario: {} — {}", self.scenario_id, self.title);
        for l in &self.steps {
            // A step that carries no verdict of its own is marked apart, but a failure is a failure
            // whichever kind it came from — an action can fail too, now that one can be judged.
            let mark = if !l.pass {
                "✗"
            } else if l.kind == "action" || l.kind == "premise" {
                "·"
            } else {
                "✓"
            };
            // The premise is numbered in its own sequence, so it is named in its own words too —
            // "step 1" appearing twice in one report is a reader counting the road wrong.
            let what = if l.kind == "premise" { "premise" } else { "step" };
            println!("  {mark} {what} {} [{}] {}", l.index + 1, l.kind, l.note);
        }
        println!("VERDICT: {}", if self.passed { "green" } else { "red" });
    }

    pub fn to_json(&self) -> String {
        let steps: Vec<String> = self
            .steps
            .iter()
            .map(|l| {
                format!(
                    "{{\"step\":{},\"kind\":{},\"pass\":{},\"note\":{}}}",
                    l.index + 1,
                    json_string(l.kind),
                    l.pass,
                    json_string(&l.note)
                )
            })
            .collect();
        format!(
            "{{\"scenario\":{},\"title\":{},\"passed\":{},\"steps\":[{}]}}",
            json_string(&self.scenario_id),
            json_string(&self.title),
            self.passed,
            steps.join(",")
        )
    }
}

/// Encode a string as a JSON string literal via serde_json (correct escaping without a dep on
/// a bespoke struct just for output).
pub fn json_string(s: &str) -> String {
    serde_json::Value::String(s.to_string()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one that the throwaway cwd would otherwise break: a path the caller counted from their
    /// own directory is resolved there, not where the scenario ends up running.
    #[test]
    fn a_relative_binary_is_anchored_where_it_was_typed() {
        let here = std::env::current_dir().expect("a cwd to anchor to");
        assert_eq!(anchor_bin(PathBuf::from("../dist/amenbo")), here.join("../dist/amenbo"));
        assert_eq!(anchor_bin(PathBuf::from("./amenbo")), here.join("./amenbo"));
    }

    /// A name with no separator is a `PATH` lookup, and an absolute path is already answered —
    /// anchoring either would turn a working invocation into a miss.
    #[test]
    fn a_bare_name_and_an_absolute_path_are_left_as_they_are() {
        assert_eq!(anchor_bin(PathBuf::from("amenbo")), PathBuf::from("amenbo"));
        let abs = std::env::current_dir().expect("a cwd").join("amenbo");
        assert_eq!(anchor_bin(abs.clone()), abs);
    }

    fn scenario(id: &str) -> Scenario {
        Scenario {
            id: id.to_string(),
            title: "a road and the world it stands on".to_string(),
            description: None,
            given: Vec::new(),
            steps_cli: Vec::new(),
            steps_gui: Vec::new(),
        }
    }

    /// A premise that did not stand takes the scenario down with it. The road is what the reader came
    /// for, so the line that failed has to say it was the world and not the road — otherwise a report
    /// sends them looking for a regression in something that was never walked.
    #[test]
    fn a_premise_that_did_not_stand_makes_the_scenario_red_and_says_it_was_the_premise() {
        let mut report = Report::new(&scenario("premise-red"));
        report.push_premise(0, Outcome::action("a project is already there".to_string()));
        assert!(report.passed(), "a premise that stood is not a failure");

        report.push_premise(1, Outcome::assert(false, "the catalog would not register".to_string()));
        assert!(!report.passed(), "one that did not stand is");
        assert!(
            report.to_json().contains("\"kind\":\"premise\""),
            "and it is named apart from the road: {}",
            report.to_json(),
        );
    }

    /// The two sequences are numbered on their own, so a report never carries two lines both calling
    /// themselves the first step.
    #[test]
    fn the_premise_and_the_road_are_numbered_apart() {
        let mut report = Report::new(&scenario("premise-numbering"));
        report.push_premise(0, Outcome::action("a project is already there".to_string()));
        report.push(0, &Step::Assert { domain: Domain::Task, op: "field".into(), with: Args::new() },
            Outcome::assert(true, "the board holds it".to_string()));

        let lines = report.to_json();
        assert_eq!(lines.matches("\"step\":1").count(), 2, "both are their sequence's first: {lines}");
        assert_eq!(lines.matches("\"kind\":\"premise\"").count(), 1, "only one of them is the world: {lines}");
    }
}
