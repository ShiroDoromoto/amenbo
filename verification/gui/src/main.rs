//! `verify-gui` — drive one verification scenario as a mac GUI checklist.
//!
//! It reads the same scenario the CLI driver black-box-drives, renders each step into a screen
//! instruction (no command line, no pixel), launches the app bundle it was pointed at against a
//! throwaway store, and hands the shooting and the reading to the screen tool
//! (`scripts/screen.swift`): the app is named by the pid that launch answered with, one shot per
//! step lands in an evidence directory, and each assert OCR can judge is decided from that shot.
//!
//! Four things can close an assert, and which one is on it follows from what the assert asks about.
//! Words on the screen are OCR's. A name the screen draws **cut** — one standing in a space
//! narrower than itself — is read off the window's own accessibility tree, where it stands whole;
//! what is on that table is [`amenbo_verify_gui::reads_the_tree`]. State **no screen draws** — the
//! original image an ingest kept — is read back out of the store the app is running against, through
//! the CLI the same bundle ships; what is on that table is
//! [`amenbo_verify_gui::reads_the_store`]. Both tables are short, and both keep the shot.
//! Everything else is left as a `Review` for a human eye, with its shot kept.
//!
//! Usage: `verify-gui <scenario.yaml> --app <bundle.app> [--evidence <dir>] [--screen <path>]
//!                    [--fixtures <dir>] [--json]`
//!        `verify-gui <scenario.yaml> --print`
//!   `--app`      the installed `.app` bundle to launch and shoot (e.g. `~/Applications/Amenbo.app`)
//!   `--evidence` where the shots + manifest land (default: a fresh dir under the temp tree)
//!   `--screen`   path to the screen tool (default: scripts/screen.swift in the repo)
//!   `--fixtures` where the files a premise copies are (default: verification/fixtures in the repo)
//!   `--json`     emit the manifest path, verdict and step count as JSON instead of the summary
//!   `--print`    print the road's instructions and stop — nothing launched, no shot, no OCR
//!
//! `--screen` and `--fixtures` exist for the same reason: both are resolved from where this harness
//! was compiled, and a run in a VM stands somewhere else entirely, where that path names nothing.
//! Leaving either out is the repository's own copy, which is what a run on this machine wants.
//!
//! What it will launch is a build the release workflow produced, and nothing else
//! ([`amenbo_verify_gui::shipped`]). The bundle is asked before the run starts; a build made here is
//! turned away at the door, since the point of shooting a screen before a release is to have shot
//! the one that ships.
//!
//! The run owns the app it shoots. It starts the bundle with `AMENBO_HOME` pointed at a store of
//! its own, so a screen road that creates projects and tasks writes them nowhere near the user's
//! backlog, and it holds the pid that launch answered with, so what is captured is that app and not
//! whichever copy of the same build happened to be open. Both go when the run ends.
//!
//! **A road can ask for the app itself to be run again** (`store run-again`), and that step is the
//! harness's rather than the operator's, for the same reason the launch is: an app opened from the
//! machine would come up on the user's own store and under no pid the run can shoot. This one goes
//! down, another comes up on the same store, and everything after it is shot against that one.
//!
//! Into that store, before the app is started, goes the world the scenario declared: `given` is
//! walked with the CLI the same bundle ships, so a road that stands on records it never makes finds
//! them there. The screen's own moves are not among them — those are the road, and the operator
//! walks them. What stood the world up is held for the length of the run, since it is also what
//! reads the store back for the asserts no screen can answer — so a road carrying one of those and
//! declaring no world is turned away at the door rather than booting a driver, which would raise a
//! project the road was written without.
//!
//! **Every step is handed over before it is taken, and there is no way to ask for otherwise.**
//! The run prints the step it is about to shoot and waits for a line on stdin;
//! whoever is driving — a person, or an AI calling the screen tool — stands the screen where that
//! step says, and answers. The first step is handed over like every other, so a road whose opening
//! move is a check is not shot against the untouched screen a launch leaves behind. Waiting on a
//! line rather than on a clock is the whole point: a run held for a fixed number of seconds shoots
//! whatever is on screen when the clock runs out, so a step that took a moment longer is filed as
//! evidence of a screen nobody stood on. Everything the wait says goes to stderr, so a `--json` run
//! is still one line of JSON on stdout.
//!
//! One thing is said between the steps without being asked about: an action whose shot came back as
//! the picture the step before it left. A step that was handed over and never carried out leaves
//! exactly that trace, and so does an action that was never going to move anything — which is why
//! the line is a remark and not a refusal. It is marked `!` and goes to stderr with the hand-over.
//!
//! `--print` is the other half of that: the road rendered into the instructions an operator would
//! read, and nothing else done with them. The sentences are written here in Rust while the road is
//! written in YAML, so what a step will say cannot be read off the file it was written in — and
//! asking a full run is asking for a GUI to be built, launched and fronted first. It prints what
//! [`amenbo_verify_gui::instructions`] returns, one to a line, which is the very text a run hands the
//! operator; a road carrying an op this harness has not mapped fails here exactly as it would there.
//!
//! Exit code is the machine signal: 0 when every assert a machine judged — off the shot, off the
//! window's tree or off the store — passed and every step was captured, non-zero on a failed assert,
//! a load failure, or a capture/reading/store-read failure. A `Review` step does not fail the run — a human closes it from
//! the evidence.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use amenbo_verify_cli::World;
use amenbo_verify_gui::{
    launch, read_menu, read_shot, read_tree, scratch, shoot, walk, write_manifest, StepBrief, StepRecord,
    Verdict,
};

fn main() -> ExitCode {
    let opts = match Opts::parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("verify-gui: {msg}");
            eprintln!("usage: verify-gui <scenario.yaml> --app <bundle.app> [--evidence <dir>] [--screen <path>] [--fixtures <dir>] [--json]");
            eprintln!("       verify-gui <scenario.yaml> --print");
            return ExitCode::from(2);
        }
    };

    match run(&opts) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE, // the scenario ran but an assert a machine judged failed
        Err(msg) => {
            eprintln!("verify-gui: {msg}");
            ExitCode::FAILURE
        }
    }
}

/// Run one scenario. `Ok(passed)` is the verdict; `Err` is an execution failure (load, capture, or
/// OCR would not run) — distinct from a clean run whose assert came out red.
fn run(opts: &Opts) -> Result<bool, String> {
    let scenario = amenbo_scenario::lint_file(&opts.scenario, opts.fixtures.as_deref())
        .map_err(|errs| format!("scenario does not load/validate:\n  {}", errs.join("\n  ")))?;

    // The screen is asked for in the scenario, never assumed by the harness: shooting a road written
    // for the CLI would spend a human's eye on `Review` steps nobody meant to send here.
    if !scenario.runs_on(amenbo_scenario::Driver::Gui) {
        return Err(format!(
            "`{}` carries a road for {} alone — write it a `steps_gui:` one if the screen is where it belongs",
            scenario.id,
            scenario.driver_tokens().join(", ")
        ));
    }

    // Reading the road back comes before anything that needs a screen: `--print` answers with no app
    // running, no window to find and nothing to shoot, which is what makes it usable while the road is
    // still being written.
    if opts.print {
        for line in amenbo_verify_gui::instructions(&scenario)? {
            println!("{line}");
        }
        return Ok(true);
    }

    let bundle = opts
        .app
        .as_ref()
        .ok_or("need --app <bundle.app> to know which build to launch and shoot")?;

    // What a run may shoot is settled at the door, before a store is made or a window is drawn: the
    // evidence a run files is only worth what the build behind it is.
    amenbo_verify_gui::shipped::ensure_release_build(bundle)?;

    // An assert closed by reading the store needs a reader, and the reader is the premise's own
    // driver. Asked before a store is made, because standing one up for a road that declared no
    // world is not the way out: the driver raises a project as it boots, which would put a record on
    // the screen this road was written without.
    if scenario.given.is_empty() {
        if let Some(step) = scenario
            .steps(amenbo_scenario::Driver::Gui)
            .iter()
            .find(|s| amenbo_verify_gui::step_reads_the_store(s))
        {
            return Err(format!(
                "`{}` carries a step closed by reading the store ({}), and declares no `given:` — \
                 what reads the store is the premise's own driver, so give the road a world to stand on",
                scenario.id,
                step_name(step)
            ));
        }
    }

    // The store comes next, and everything after it is pointed at it: the world is stood up in it,
    // and the app is launched at it. Both are let go of before it is, which is what the order of
    // these bindings says — the app goes down, then whatever the premise was holding, then the store.
    let store = scratch::session(&scenario.id, false)
        .map_err(|e| format!("could not create a throwaway store: {e}"))?;

    // The world the scenario declared, stood up with the CLI this very bundle ships. Before the
    // launch, because a store is read as the app starts; and before there is an evidence directory,
    // because a premise that could not be stood up must leave no shots of a half-built world.
    let mut world = stand_world(&scenario, bundle, &store, opts.fixtures.clone())?;

    let mut gui = launch::launch(bundle, &store)?;
    gui.wait_until_shootable(&opts.screen)?;
    // Held where both the shooting and the restart can reach it. A road may put the app through a
    // run of its own (`store run-again`), and the pid moves when it does — so what names the app to
    // the screen tool is read off the launch at every shot rather than copied out once.
    let gui = std::cell::RefCell::new(gui);

    let evidence = opts
        .evidence
        .clone()
        .unwrap_or_else(|| default_evidence_dir(&scenario.id));

    // What the premise stood up, said before the walk rather than only in the summary afterwards.
    // Some of it is a road's to reach for — a file the operator has to find in a picker lies under a
    // throwaway path nothing in the road can name, since the instructions are rendered from the YAML
    // alone — so a line that arrives after the last step arrives too late to be of use.
    for line in world.as_ref().map(World::stood).unwrap_or_default() {
        eprintln!("world: {line}");
    }
    eprintln!(
        "{} step(s) — each one is handed over before it is shot",
        scenario.steps(amenbo_scenario::Driver::Gui).len()
    );

    let screen = opts.screen.clone();
    let stdin = std::io::stdin();
    let outcome = walk(
        &scenario,
        &evidence,
        |window, path| shoot(gui.borrow().pid, window, path, &screen),
        |image| read_shot(image, &screen),
        // The same window the shot was aimed at, listed off its accessibility tree — for the asserts
        // a picture carries but cannot be read for, a name a narrow column drew cut being the one.
        |window| read_tree(gui.borrow().pid, window, &screen),
        // The app's own menu bar, which stands above every window and is on no shot of one.
        || read_menu(gui.borrow().pid, &screen),
        |brief| hand_over(&stdin, brief),
        || gui.borrow_mut().run_again(&screen),
        // The store the app is running against, asked through the driver that stood its world up.
        // A road with no world never reaches here — the door above turned it away — so a step that
        // arrives with nothing to read is this harness having gone wrong, not the road.
        |step| match world.as_mut() {
            Some(w) => w.read(step),
            None => Err("internal: a step to read the store, on a road that stood none up".into()),
        },
        // What the walk noticed and is not asking about. To stderr with the hand-over, since it is
        // read by the same eye at the same moment and a `--json` run keeps stdout to its one line.
        |line| eprintln!("  ! {line}"),
    )?;

    let stood = world.as_ref().map(World::stood).unwrap_or_default();
    let manifest = write_manifest(&evidence, &scenario, stood, &outcome)?;

    if opts.json {
        println!(
            "{{\"scenario\":{},\"passed\":{},\"evidence\":{},\"manifest\":{},\"steps\":{}}}",
            amenbo_verify_gui::js_out(&scenario.id),
            outcome.passed,
            amenbo_verify_gui::js_out(&evidence.to_string_lossy()),
            amenbo_verify_gui::js_out(&manifest.to_string_lossy()),
            outcome.records.len()
        );
    } else {
        println!("scenario: {} — {}", scenario.id, scenario.title);
        for line in stood {
            println!("  world: {line}");
        }
        println!("evidence: {}", evidence.display());
        for r in &outcome.records {
            println!("{}", step_lines(r));
        }
        println!("manifest: {}", manifest.display());
        println!("VERDICT:  {}", if outcome.passed { "green" } else { "red" });
    }
    Ok(outcome.passed)
}

/// Stand up the world the scenario says the screen starts from, in the store the app is about to be
/// pointed at, with the CLI the bundle under test ships.
///
/// A scenario that declares none gets none: an empty store is exactly the world a road written
/// without a premise was written against, and booting one to be tidy would put records on screen no
/// line of that road asked for.
fn stand_world<'a>(
    scenario: &amenbo_scenario::Scenario,
    bundle: &Path,
    store: &'a scratch::Session,
    fixtures: Option<PathBuf>,
) -> Result<Option<World<'a>>, String> {
    if scenario.given.is_empty() {
        return Ok(None);
    }
    let cli = amenbo_verify_gui::shipped::sidecar(bundle)?;
    amenbo_verify_cli::stand_world(&cli, store, &scenario.given, fixtures)
        .map(Some)
        .map_err(|e| format!("the world `{}` starts from could not be stood up: {e}", scenario.id))
}

/// One step as the summary prints it: its verdict mark, number, kind and instruction, and the shot
/// it left behind. The hand-over says the same of a step it has no shot for yet, so what a driver
/// reads mid-run and what the summary reports afterwards line up.
///
/// A step whose words met only once a misread character was forgiven says so on the same line. It
/// passed, and the run is green on it — but a tolerance is not a reading, and the one place a person
/// would look for that is the summary they are already reading.
fn step_lines(r: &StepRecord) -> String {
    let mark = match r.verdict {
        Verdict::Action => "·",
        Verdict::Pass => "✓",
        Verdict::Fail => "✗",
        Verdict::Review => "?",
        Verdict::Read => "☑",
    };
    let slip = if r.slipped { "  (the words met on a forgiven glyph — worth an eye)" } else { "" };
    // What the store said, on a step whose shot says nothing about it. It goes under the shot rather
    // than in place of it: the screen it was read beside is part of the evidence either way.
    let told = match &r.told {
        Some(t) => format!("\n        store: {t}"),
        None => String::new(),
    };
    format!(
        "  {mark} {:02} [{}] {}\n        → {}{slip}{told}",
        r.index + 1,
        r.kind,
        r.instruction,
        r.screenshot
    )
}

/// Hand the step about to be shot to whoever is driving, then wait for one line on stdin saying the
/// screen is standing where that step says. It writes to stderr so a `--json` run keeps stdout to
/// its one machine-readable line, and a line is all it asks for — the content is the driver's to use
/// as a note to themselves.
///
/// What an assert expects to find is shown with it, and where it will be looked for: on the shot, or
/// on the window's accessibility tree, for the asserts a picture carries but cannot be read for. The
/// reading is a search for those words and no more — one misread character inside them is forgiven
/// and nothing else is — so a driver who can see the screen is the one who can tell a check that
/// genuinely passed from one the words happened to satisfy.
///
/// End of input is a failure rather than a nod. A run with nothing left to hold it would walk the
/// rest of the scenario off whichever screen was up, and file those shots as evidence of steps
/// nobody carried out.
fn hand_over(stdin: &std::io::Stdin, brief: &StepBrief<'_>) -> Result<(), String> {
    eprintln!("  → {:02} [{}] {}", brief.index + 1, brief.kind, brief.instruction);
    if let Some(exp) = brief.expected {
        let side = match (brief.from_the_tree, exp.present) {
            (false, true) => "the shot reads",
            (false, false) => "the shot does not read",
            (true, true) => "the screen names",
            (true, false) => "the screen does not name",
        };
        eprintln!("        {side}: {}", exp.text);
    }
    eprint!("  … stand the screen where this step says, then press Enter: ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    match stdin.lock().read_line(&mut line) {
        Ok(0) => Err("stdin reached end of input — a run needs one line per step".into()),
        Ok(_) => Ok(()),
        Err(e) => Err(format!("could not read the go-ahead from stdin: {e}")),
    }
}

/// A step named the way a road writes it (`store blobs`), for a message about the road itself.
fn step_name(step: &amenbo_scenario::Step) -> String {
    let (domain, op) = match step {
        amenbo_scenario::Step::Action { domain, op, .. }
        | amenbo_scenario::Step::Assert { domain, op, .. } => (domain, op),
    };
    format!("{} {op}", amenbo_verify_gui::domain_str(*domain))
}

/// A fresh evidence dir under the temp tree, named for the scenario and the wall clock so two
/// runs never share one (the manifest and shots of one run must not land on another's).
fn default_evidence_dir(id: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    std::env::temp_dir()
        .join("amenbo-verify-gui")
        .join(format!("{id}-{nanos:x}"))
}

/// Parsed command line.
struct Opts {
    scenario: PathBuf,
    /// The `.app` bundle to launch. Optional here and required in [`run`], since `--print` reads a
    /// road back without anything running.
    app: Option<PathBuf>,
    evidence: Option<PathBuf>,
    screen: PathBuf,
    /// Where the files a premise copies are. `None` is the shelf beside the scenarios in this
    /// repository, which is where the harness was compiled and so where a run on this machine finds
    /// them; a run in the VM is handed the copy that was sent in there.
    fixtures: Option<PathBuf>,
    json: bool,
    print: bool,
}

impl Opts {
    fn parse(args: impl Iterator<Item = String>) -> Result<Opts, String> {
        let mut scenario = None;
        let mut app = None;
        let mut evidence = None;
        let mut screen = None;
        let mut fixtures = None;
        let mut json = false;
        let mut print = false;
        let mut it = args.peekable();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--json" => json = true,
                "--print" => print = true,
                "--app" => app = Some(PathBuf::from(it.next().ok_or("--app needs a bundle path")?)),
                "--evidence" => evidence = Some(PathBuf::from(it.next().ok_or("--evidence needs a path")?)),
                "--screen" => screen = Some(PathBuf::from(it.next().ok_or("--screen needs a path")?)),
                "--fixtures" => {
                    fixtures = Some(PathBuf::from(it.next().ok_or("--fixtures needs a path")?))
                }
                s if s.starts_with("--") => return Err(format!("unknown flag `{s}`")),
                _ => {
                    if scenario.replace(PathBuf::from(a)).is_some() {
                        return Err("more than one scenario given".into());
                    }
                }
            }
        }
        Ok(Opts {
            scenario: scenario.ok_or("no scenario file given")?,
            app,
            evidence,
            screen: screen.unwrap_or_else(default_screen),
            fixtures,
            json,
            print,
        })
    }
}

/// The screen tool's place in the repo, resolved from this crate so it is found whatever the CWD:
/// `verification/gui` → `scripts/screen.swift`.
fn default_screen() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // verification/
        .and_then(|p| p.parent()) // repo root
        .map(|p| p.join("scripts/screen.swift"))
        .unwrap_or_else(|| PathBuf::from("scripts/screen.swift"))
}
