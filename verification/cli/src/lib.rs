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

/// The throwaway store an Amenbo run is given. Public because every bin in this crate needs it:
/// asking the shipped binary anything at all means giving it a home that is not the user's.
pub mod scratch;

mod domain;
mod judge;
mod shipped;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use amenbo_scenario::{Args, BoundKind, Domain, Scenario, Step};

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
    let mut driver = Driver::new(bin, &session, None)?;

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
///
/// `fixtures` says where the files a premise copies are, for a caller that is not standing in this
/// repository — the GUI harness runs inside a VM, where this crate's compile-time path names nothing.
/// `None` is that path, which is what the CLI road runs on.
pub fn stand_world<'a>(
    bin: &Path,
    session: &'a scratch::Session,
    given: &[Step],
    fixtures: Option<PathBuf>,
) -> Result<World<'a>, String> {
    let mut driver = Driver::new(bin, session, fixtures)?;
    let mut stood = Vec::new();
    for (i, step) in given.iter().enumerate() {
        // The premise's own numbering, the way the loader's errors read it: a world's step is not a
        // road's, and a message that said "step 2" would send a reader to the wrong list.
        let outcome = driver.exec(step).map_err(|e| format!("given step {}: {e}", i + 1))?;
        stood.push(outcome.note);
    }
    Ok(World { stood, driver })
}

/// A world that has been stood up, and is standing for as long as this is held.
pub struct World<'a> {
    stood: Vec<String>,
    /// The driver that stood the world up, kept for two reasons. It owns what a premise put on the
    /// loopback, so dropping it would close the port a registration is pinned to while the run is
    /// still pointed at it; and it is the way back into the store afterwards ([`World::read`]).
    driver: Driver<'a>,
}

impl World<'_> {
    /// What the premise did, a line per step, in the same words an action reports itself with on a
    /// road — so what a run stood on is readable beside what it then walked.
    pub fn stood(&self) -> &[String] {
        &self.stood
    }

    /// Put one assert to the store the world stands in, and hand back the verdict and the line the
    /// CLI road would report it with. `Err` is an execution failure (the binary would not run, the
    /// op is not one this driver maps); an assert that ran and came out false is `Ok((false, …))`.
    ///
    /// This is what the screen harness closes its unshowable asserts with. It is the
    /// very arm the CLI road judges that assert with, reached through the driver that stood the
    /// premise up — so the two drivers answer one question one way, and a road does not have to say
    /// which of them is asking.
    ///
    /// An action handed here would be **carried out**, since that is what `exec` does with one. The
    /// caller is the one that knows an assert from an action, and the screen harness only sends the
    /// asserts on its own closed table.
    pub fn read(&mut self, step: &Step) -> Result<(bool, String), String> {
        let outcome = self.driver.exec(step)?;
        Ok((outcome.pass, outcome.note))
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

/// Where the scenario fixtures live when nobody says otherwise — `verification/fixtures/`, beside
/// the scenarios that name them. Resolved from this crate's own location rather than from the CWD,
/// so `verify-all` finds them wherever it is invoked from.
fn default_fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("fixtures")
}

/// Drives the shipped binary against one isolated store, remembering the ids that steps bind.
///
/// The store is borrowed rather than owned, because a driver is not always the only one working in
/// it: the GUI harness stands a world up through one and then launches the app under test at the
/// same store, and the borrow is what says the store outlives both.
pub(crate) struct Driver<'a> {
    bin: std::path::PathBuf,
    session: &'a scratch::Session,
    /// The project a step files under when it names none, raised by the `init` this driver boots
    /// with. Zero once a premise has taken the device back to nothing raised on it
    /// (`store nothing-raised`) — read through [`Driver::standing_project`], which turns that into
    /// a refusal by name rather than a write to a project that is gone.
    project_id: i64,
    bindings: HashMap<String, i64>,
    /// Which number space each binding's id lives in, for the ops that hand an id back to Amenbo as a
    /// reference rather than as a number. Recorded beside the binding rather than on it: every other
    /// op takes the id itself, and only the classification doors ask which kind it is.
    bound_kinds: HashMap<String, BoundKind>,
    /// What the last `plugin run` came back with. A command face's return value is its own stdout
    /// and is deliberately kept out of the execution log, so this is the only place a later step can
    /// read it from — which is why the assert that reads it has to follow its call.
    last_run: Option<serde_json::Value>,
    /// What the last `unbind` answered. Kept for the reason the line above is: how many folders the
    /// project has left is part of that answer, and afterwards there is only the state it left —
    /// which reads the same whether the answer mentioned it or not.
    last_unbind: Option<serde_json::Value>,
    /// What the last `bind --rebind` answered. Kept for the same reason again, and here it is the id:
    /// no read Amenbo offers publishes a binding's, so which row moved is said once, as it moves.
    last_rebind: Option<serde_json::Value>,
    /// Where each folder a step moved used to stand, under the name the road calls it by. A moved
    /// folder is the one thing a later step cannot ask the session for — asking places it, and a path
    /// placed again is a path that leads somewhere, which is the very state the road took away.
    moved: HashMap<String, std::path::PathBuf>,
    /// What the last `plugin flush` reported. Kept for the same reason as the line above: what a
    /// flush got through, and which queues it stepped around, is said once as it returns and is
    /// nowhere to be read afterwards — the store shows the state, not who declined to touch it.
    last_flush: Option<serde_json::Value>,
    /// The files the `store` actions wrote, under the same names. A scenario has one binding
    /// namespace — the loader keeps it unique across all of them — and which map a name lands in
    /// follows from the op that bound it: nothing in the store is a path, and no archive is an id.
    artifacts: HashMap<String, std::path::PathBuf>,
    /// The numbers a `store` action read back, under the names their steps bound. The third thing an
    /// `as:` can hold, and apart from the other two for the reason they are apart from each other:
    /// which map a name lands in follows from the op that bound it, and a version is neither a row in
    /// the store nor a file on disk.
    numbers: HashMap<String, i64>,
    /// The catalogs the run stood up itself, under the names their steps bound. They are held here
    /// for the length of the scenario because a host answers only while it is alive: dropping one
    /// after the step that made it would leave every later step pointed at a closed port.
    catalogs: HashMap<String, StoodCatalog>,
    /// The MCP server a road stood up, held for the length of that road. It is one because a server
    /// serves one folder and a road walks one — and it is held rather than started per step because a
    /// conversation is what the protocol has: dropping it between steps would leave every later one
    /// talking to a closed pipe.
    server: Option<crate::domain::mcp::Standing>,
    /// What the machine's own scheduler was holding when this run began. Read once, before anything
    /// walks: nothing in this harness registers a timer, so the only reading an assert can honestly
    /// make is the difference — whether the run left the machine as it found it. The absolute state
    /// belongs to the machine and not to the run, and reading it as one would fail every road about
    /// leaving nothing behind on the very machines where somebody uses the hourly tick.
    tick_at_start: bool,
    /// Set while a step that declared `refused:` is running. It is read where a failed invocation
    /// is judged, so the arm issuing the command never has to know it might be turned away —
    /// [`Driver::refused`] puts it up and takes it back down around the one call.
    refusal: Option<String>,
    /// Where the files a `copy-fixture` step reaches for are. Held on the driver rather than read
    /// off this crate's own location, because a run does not always stand in the tree it was
    /// compiled in: the harness that walks a screen road is sent to a VM, and the compile-time path
    /// names nothing there.
    fixtures: PathBuf,
    /// The pointer the run's own folder was holding when this driver booted, kept as its text.
    ///
    /// The two ops that leave a folder holding a pointer no build under test would write
    /// (`folder foreign-pointer`, `folder lost-pointer`) copy this one rather than write one from
    /// parts, so that everything but the field they move agrees. It is read here rather than at the
    /// step, because a premise can take the folder's own away first: `store nothing-raised` deletes
    /// the project the boot raised, and deleting a project releases every folder pointing at it —
    /// the run's own included. Those two ops are exactly the ones a road walks on that world.
    own_pointer: String,
}

/// What an expected refusal travels back on. A refusal has to reach [`Driver::refused`] from
/// wherever the command was issued, and the way out of an arm that every one of them already has is
/// the `?` on its invocation — so it goes as an `Err`, and a byte no message of ours carries keeps
/// it apart from a real failure. The code that came back is spliced on after it.
const REFUSED: &str = "\u{1}refused:";

impl<'a> Driver<'a> {
    /// Boot a fresh store: `init` creates it and hands back the project every `task add` needs. A
    /// premise that wants a device with nothing raised on it takes this one away again as it opens
    /// (`store nothing-raised`) — the store has to have somewhere to file what a premise stands up
    /// before it can have nowhere.
    ///
    /// `fixtures` names the shelf a `copy-fixture` step reads from; `None` takes the one beside the
    /// scenarios in this repository.
    fn new(
        bin: &Path,
        session: &'a scratch::Session,
        fixtures: Option<PathBuf>,
    ) -> Result<Driver<'a>, String> {
        let mut d = Driver {
            bin: bin.to_path_buf(),
            session,
            project_id: 0,
            bindings: HashMap::new(),
            bound_kinds: HashMap::new(),
            last_run: None,
            last_unbind: None,
            last_rebind: None,
            moved: HashMap::new(),
            last_flush: None,
            artifacts: HashMap::new(),
            numbers: HashMap::new(),
            catalogs: HashMap::new(),
            server: None,
            tick_at_start: false,
            refusal: None,
            fixtures: fixtures.unwrap_or_else(default_fixtures_dir),
            own_pointer: String::new(),
        };
        let v = d.run_json(&["init", "--name", "verify", "--json"])?;
        d.project_id = v["identity"]["project_id"]
            .as_i64()
            .ok_or("init did not report a project_id")?;
        let ours = session.cwd.join(".amenbo");
        d.own_pointer = std::fs::read_to_string(&ours)
            .map_err(|e| format!("could not read the run's own pointer at {}: {e}", ours.display()))?;
        d.tick_at_start = d.tick_registered()?;
        Ok(d)
    }

    /// The project a step files under when it names none — the one `init` raised as this driver
    /// booted. An `Err` once a premise has taken the device back to nothing raised on it
    /// (`store nothing-raised`), which is a world where filing something without saying where has
    /// no answer: better to say so by name than to send the row to a project that was deleted.
    fn standing_project(&self) -> Result<i64, String> {
        match self.project_id {
            0 => Err("nothing is raised on this device — a step that files something has to name \
                      the project it goes in (`store nothing-raised` took the last one away)"
                .to_string()),
            id => Ok(id),
        }
    }

    /// Spawn the shipped binary in the isolated store, from a chosen folder. Every call goes
    /// through here, so the isolation is stated once and cannot be forgotten by an arm that builds
    /// its own command. Where the command stands is itself an input for anything to do with binding
    /// — the pointer that decides what a run reaches is found by walking up from the CWD — so those
    /// steps ask their question from inside the folder they are asking about.
    fn invoke_in(&self, cwd: &Path, args: &[&str]) -> Result<std::process::Output, String> {
        // The facet goes on the command line, which is the one input Amenbo is to take it by; a call
        // that names its own is left alone.
        let mut with_facet = args.to_vec();
        if !args.contains(&"--actor") {
            with_facet.extend_from_slice(&["--actor", "human"]);
        }
        let mut cmd = Command::new(&self.bin);
        cmd.args(&with_facet)
            .current_dir(cwd)
            .env("AMENBO_HOME", &self.session.home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .env("NO_COLOR", "1");
        cmd.output().map_err(|e| format!("could not run `{}`: {e}", self.bin.display()))
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
        self.judge_json(args, out)
    }

    /// What every run of a `--json` command comes to: a refusal, an execution failure, or the answer.
    fn judge_json(
        &self,
        args: &[&str],
        out: std::process::Output,
    ) -> Result<serde_json::Value, String> {
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

    /// Run a command the caller expects to be **turned away**, and hand back the error object it
    /// printed. Apart from a step's own `refused:`, which judges a refusal and stops there: this is
    /// for the road that has to *read* one, because what it needs is published nowhere else — the
    /// ids `bind` lines up when a folder of the project has vanished are in that answer's hint and in
    /// no read at all. The error object goes to stderr, so it is taken off the stream it is on.
    fn refusal_in(&self, cwd: &Path, args: &[&str]) -> Result<serde_json::Value, String> {
        let out = self.invoke_in(cwd, args)?;
        if out.status.success() {
            return Err(format!("`amenbo {}` went through where it had to be turned away", args.join(" ")));
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        let v: serde_json::Value = serde_json::from_str(stderr.trim()).map_err(|e| {
            format!("`amenbo {}` was turned away without an error object ({e}): {}", args.join(" "), stderr.trim())
        })?;
        v.get("error")
            .cloned()
            .ok_or_else(|| format!("`amenbo {}` was turned away without an error object: {}", args.join(" "), stderr.trim()))
    }

    /// Judge a **read** the scenario says Amenbo will turn away. `refused:` is an action's word,
    /// since an assert already carries a verdict of its own — but "refused" is not one of the
    /// verdicts a listing can come back with, and a listing turned away is not a listing with
    /// nobody in it. So the verdict is read here: turned away with the code the step named → pass;
    /// turned away with another → fail, since the step is about *that* guard; answered at all →
    /// fail, which is the regression the line exists to catch.
    fn refused_read(&self, args: &[&str], want: &str) -> Result<Outcome, String> {
        let out = self.invoke(args)?;
        if out.status.success() {
            return Ok(Outcome::assert(
                false,
                format!(
                    "`amenbo {}` answered where `{want}` was expected to refuse it (MISMATCH)",
                    args.join(" ")
                ),
            ));
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        let v: serde_json::Value = serde_json::from_str(stderr.trim()).map_err(|e| {
            format!(
                "`amenbo {}` was turned away without an error object ({e}): {}",
                args.join(" "),
                stderr.trim()
            )
        })?;
        let code = v["error"]["code"].as_str().ok_or_else(|| {
            format!("`amenbo {}` was turned away without an error code: {}", args.join(" "), stderr.trim())
        })?;
        let pass = code == want;
        Ok(Outcome::assert(
            pass,
            format!(
                "`amenbo {}` was refused with `{code}` (expected `{want}`, {})",
                args.join(" "),
                if pass { "as expected" } else { "MISMATCH" }
            ),
        ))
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

    /// Run for what it *wrote to stdout* — the shape a command takes when its stdout is the document
    /// rather than a report about one (`sync snapshot`). The bytes come back unread: what they hold is
    /// the step's business, and treating a document as text here would put an encoding between a
    /// carrier's file and the file this run judges.
    fn run_stdout(&self, args: &[&str]) -> Result<Vec<u8>, String> {
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
        Ok(out.stdout)
    }

    /// Resolve a path a step named against the session's own folder, refusing anything that would
    /// reach outside it. A scenario writes and lints files in the throwaway CWD and nowhere else —
    /// this driver is handed real machines to run on, so an absolute path or a `..` in a scenario is
    /// refused rather than followed.
    fn in_session(&self, path: &str) -> Result<std::path::PathBuf, String> {
        Ok(self.session.cwd.join(self.inside(path)?))
    }

    /// The same check on its own, for a step that has a folder of its own to hang the path off. A
    /// path is refused by its shape rather than by where it was about to be joined, so every folder
    /// a scenario writes into is closed the one way.
    fn inside<'p>(&self, path: &'p str) -> Result<&'p Path, String> {
        let p = Path::new(path);
        if p.is_absolute() || p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err(format!("`path: {path}` must stay inside the folder it is written into"));
        }
        Ok(p)
    }

    /// The code a refusal came back with, but only while a step is expecting one. Amenbo prints the
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
            Step::Action { domain, op, with, bind, .. } => match with.get("refused") {
                Some(code) => {
                    let want = code
                        .as_str()
                        .ok_or("`refused` must be the error code the operation is expected to be rejected with")?
                        .to_string();
                    self.refused(*domain, op, with, &want)
                }
                None => self.action(*domain, op, with, bind.as_deref()),
            },
            Step::Assert { domain, op, with, .. } => self.assert(*domain, op, with),
        }
    }

    /// Run an operation the step says Amenbo will turn away, and judge the refusal — the guard in
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
        // What kind the binding this step is about to make stands for. Noted here, off the domain, so
        // the nine places that insert a binding stay as they are and there is one line to keep true.
        if let (Some(name), Some(kind)) = (bind, BoundKind::of_domain(domain)) {
            self.bound_kinds.insert(name.to_string(), kind);
        }
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
            Domain::Mcp => self.mcp_action(op, with),
            // Nothing here writes a registration into the machine the gate is running on — see
            // `domain::tick`. The one action the wake-up carries is a premise's reach into the
            // run's own store, and nothing further.
            Domain::Tick => self.tick_action(op, with),
            // Where a terminal is drawn is a question about a screen, and this driver has none —
            // bar the one premise that stands the machine up underneath it (`domain::terminal`),
            // which is settled before any app comes up and is nobody's screen.
            Domain::Terminal => self.terminal_action(op, with),
            // The file face is a screen too. Reading a file at a shell is `cat`, which is not Amenbo
            // doing anything, so there is nothing here to walk and no gap in the road.
            Domain::Files => Err(unmapped(domain, op)),
        }
    }

    /// Judge an assert by handing it to the domain that answers for it. Two ops are named for what
    /// they are about rather than for a domain of their own: a document Amenbo handed out — an
    /// export's archive, a carrier's snapshot — is read for whatever kind of row is looked for in it.
    fn assert(&self, domain: Domain, op: &str, with: &Args) -> Result<Outcome, String> {
        if op == "exported" || op == "synced" {
            return self.judge_carried(domain, op, with);
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
            Domain::Mcp => self.mcp_assert(op, with),
            Domain::Tick => self.tick_assert(op, with),
            // The screen's alone, the same way its actions are.
            Domain::Terminal => Err(unmapped(domain, op)),
            Domain::Files => Err(unmapped(domain, op)),
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

    /// Record a number an action read back, under the name a later step will ask for it by. A step
    /// that binds nothing still gets a slot, so nothing is silently dropped on the way.
    fn remember_number(&mut self, bind: Option<&str>, kind: &str, value: i64) {
        let name = match bind {
            Some(name) => name.to_string(),
            None => format!("{kind}-{}", self.numbers.len()),
        };
        self.numbers.insert(name, value);
    }

    /// Resolve a name to the number an earlier `store` action read. As with a file, the loader proved
    /// the name resolves to an earlier `as:`, so what is left here is a name bound by an op that
    /// produced something else.
    fn number_ref(&self, with: &Args, key: &str) -> Result<i64, String> {
        let name = req_str(with, key)?;
        self.numbers.get(name).copied().ok_or_else(|| {
            format!("`{key}: {name}` names no number a `store` action read in this run")
        })
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
        self.folder_named(req_str(with, "dir")?)
    }

    /// The same, by the name itself — for an op that names a second folder under its own key (`folder
    /// move`'s `to:`), the way an op that joins two objects names the second side under its own.
    fn folder_named(&self, name: &str) -> Result<PathBuf, String> {
        self.session
            .folder(name)
            .map_err(|e| format!("could not make the folder `{name}`: {e}"))
    }

    /// Where to stand for a step that names a `project:` — the folder this run linked to it when the
    /// premise raised it. Some of what Amenbo holds is held per project, and a terminal says which
    /// project it means by standing in a folder bound to that one; there is no flag for it. A step
    /// that names none gets `None`, which is the run's own working directory: bound to nothing, and
    /// so answering to the store's default project.
    ///
    /// The binding is read back before the folder is handed over. A folder that answers with another
    /// project — or with none, which is what a name no `project create` ever raised leaves behind —
    /// would take the command somewhere quietly and leave the road reading the wrong crossing, which
    /// is the whole reason a step gets to name one.
    fn project_folder(&self, with: &Args) -> Result<Option<PathBuf>, String> {
        let Some(name) = with.get("project") else { return Ok(None) };
        let name = name.as_str().ok_or("arg `project` must be a string")?;
        let dir = self.folder_named(&crate::domain::project::folder_name(name))?;
        let v = self.run_json_in(&dir, &["bind", "--json"])?;
        match v["binding"]["project_name"].as_str() {
            Some(bound) if bound == name => Ok(Some(dir)),
            other => Err(format!(
                "`project: {name}` names no folder this run stands in — {} is bound to {}",
                dir.display(),
                match other {
                    Some(bound) => format!("`{bound}`"),
                    None => "no project".to_string(),
                }
            )),
        }
    }

    /// Resolve a step's `target:` to the id an earlier action bound. The loader already proved
    /// the name resolves to an earlier `as:`, so a miss here is an internal error, not user input.
    fn resolve(&self, with: &Args) -> Result<i64, String> {
        self.resolve_key(with, "target")
    }

    /// Resolve a step's `target:` to the **reference** Amenbo reads it back by, kind code and all. What
    /// [`Self::resolve`] answers is a number, and a number no longer says which of the two spaces it
    /// belongs to at the doors that take either — so those doors ask here instead.
    fn resolve_ref(&self, with: &Args) -> Result<String, String> {
        let id = self.resolve(with)?;
        let name = req_str(with, "target")?;
        let kind = self.bound_kinds.get(name).copied().ok_or_else(|| {
            format!("`target: {name}` is neither a task nor a decision, so it cannot be classified")
        })?;
        Ok(kind.spell(id))
    }

    /// The text a step writes, with the number of the record its `mentions` names put on the end of it
    /// as a word of its own. Left out, the text is what the step wrote and nothing more.
    ///
    /// A number goes in this way rather than being typed into the text because the store issues it: a
    /// road knows which record it means and never what that record was numbered. What it is for is a
    /// search — a number is asked as a word underneath the record it pins, which is only visible where
    /// another record has it written in it.
    fn mentioning(&self, with: &Args, text: &str) -> Result<String, String> {
        if !with.contains_key("mentions") {
            return Ok(text.to_string());
        }
        let id = self.resolve_key(with, "mentions")?;
        Ok(format!("{text} {id}"))
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
        report.push(0, &Step::Assert { domain: Domain::Task, op: "field".into(), with: Args::new(), window: None },
            Outcome::assert(true, "the board holds it".to_string()));

        let lines = report.to_json();
        assert_eq!(lines.matches("\"step\":1").count(), 2, "both are their sequence's first: {lines}");
        assert_eq!(lines.matches("\"kind\":\"premise\"").count(), 1, "only one of them is the world: {lines}");
    }
}
