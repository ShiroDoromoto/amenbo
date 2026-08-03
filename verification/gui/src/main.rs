//! `verify-gui` — drive one verification scenario as a mac GUI checklist.
//!
//! It reads the same scenario the CLI driver black-box-drives, renders each step into a screen
//! instruction (no command line, no pixel), launches the app bundle it was pointed at against a
//! throwaway store, locates that process's window through `app/scripts/uiauto/uiauto.swift`, and
//! captures one `screencapture -l <winid>` per step into an evidence directory. Each assert OCR can
//! judge is decided from its shot with macOS Vision (`ocr.swift`); an assert it cannot judge is left
//! as a `Review` for a human eye, and its shot is kept.
//!
//! Usage: `verify-gui <scenario.yaml> --app <bundle.app>
//!                    [--evidence <dir>] [--uiauto <path>] [--ocr <path>] [--step] [--json]`
//!        `verify-gui <scenario.yaml> --print`
//!   `--app`      the `.app` bundle to launch and shoot (e.g. `/Applications/amenbo.app`)
//!   `--evidence` where the shots + manifest land (default: a fresh dir under the temp tree)
//!   `--uiauto`   path to uiauto.swift (default: app/scripts/uiauto/uiauto.swift in the repo)
//!   `--ocr`      path to ocr.swift (default: the ocr.swift beside this crate)
//!   `--step`     stop after each step's shot and wait for a line on stdin before the next
//!   `--json`     emit the manifest path, verdict and step count as JSON instead of the summary
//!   `--print`    print the road's instructions and stop — nothing launched, no shot, no OCR
//!
//! The run owns the app it shoots. It starts the bundle with `AMENBO_HOME` pointed at a store of
//! its own, so a screen road that creates projects and tasks writes them nowhere near the user's
//! backlog, and it holds the pid that launch answered with, so what is captured is that app and not
//! whichever copy of the same build happened to be open. Both go when the run ends.
//!
//! Without `--step` the run shoots every step back to back, which is one screen photographed as
//! many times as the scenario is long. `--step` is what lets a scenario carry a screen that moves:
//! it hands the run back after each shot, and whoever is driving — a person, or an AI calling
//! uiauto — carries out the next step and sends a line to say the screen is standing where the
//! scenario says it should. Waiting on a line rather than on a clock is the whole point: a run held
//! for a fixed number of seconds shoots whatever is on screen when the clock runs out, so a step
//! that took a moment longer is filed as evidence of a screen nobody stood on. Everything the wait
//! says goes to stderr, so a `--json` run is still one line of JSON on stdout.
//!
//! `--print` is the other half of that: the road rendered into the instructions an operator would
//! read, and nothing else done with them. The sentences are written here in Rust while the road is
//! written in YAML, so what a step will say cannot be read off the file it was written in — and
//! asking a full run is asking for a GUI to be built, launched and fronted first. It prints what
//! [`amenbo_verify_gui::instructions`] returns, one to a line, which is the very text a run hands the
//! operator; a road carrying an op this harness has not mapped fails here exactly as it would there.
//!
//! Exit code is the machine signal: 0 when every OCR-judged assert passed and every step was
//! captured, non-zero on a failed assert, a load failure, or a capture/OCR failure. A `Review`
//! step (an assert OCR cannot judge) does not fail the run — a human closes it from the evidence.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

use amenbo_verify_gui::{launch, ocr, scratch, walk, write_manifest, StepRecord, Verdict};

fn main() -> ExitCode {
    let opts = match Opts::parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("verify-gui: {msg}");
            eprintln!("usage: verify-gui <scenario.yaml> --app <bundle.app> [--evidence <dir>] [--uiauto <path>] [--ocr <path>] [--step] [--json]");
            eprintln!("       verify-gui <scenario.yaml> --print");
            return ExitCode::from(2);
        }
    };

    match run(&opts) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE, // the scenario ran but an OCR-judged assert failed
        Err(msg) => {
            eprintln!("verify-gui: {msg}");
            ExitCode::FAILURE
        }
    }
}

/// Run one scenario. `Ok(passed)` is the verdict; `Err` is an execution failure (load, capture, or
/// OCR would not run) — distinct from a clean run whose assert came out red.
fn run(opts: &Opts) -> Result<bool, String> {
    let scenario = amenbo_scenario::lint_file(&opts.scenario)
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

    // The store is made first and handed to the launch, which owns both from here: the app is up
    // for exactly as long as this binding lives, and the store goes down with it.
    let store = scratch::store(&scenario.id)
        .map_err(|e| format!("could not create a throwaway store: {e}"))?;
    let mut gui = launch::launch(bundle, store)?;
    let window = gui.window(&opts.uiauto)?;

    let evidence = opts
        .evidence
        .clone()
        .unwrap_or_else(|| default_evidence_dir(&scenario.id));

    if opts.step {
        eprintln!(
            "stepping: {} step(s) — the run stops after each shot",
            scenario.steps(amenbo_scenario::Driver::Gui).len()
        );
    }

    let capture_bin =
        std::env::var_os("AMENBO_GUI_CAPTURE_BIN").unwrap_or_else(|| "screencapture".into());
    let winid = window.id.clone();
    let ocr_swift = opts.ocr.clone();
    let stepping = opts.step;
    let stdin = std::io::stdin();
    let outcome = walk(
        &scenario,
        &evidence,
        |path| {
            // `screencapture -x -l <winid> <path>`: -x is silent, -l shoots one window, path last.
            let status = Command::new(&capture_bin)
                .arg("-x")
                .arg("-l")
                .arg(&winid)
                .arg(path)
                .status()
                .map_err(|e| format!("could not run `{}`: {e}", capture_bin.to_string_lossy()))?;
            if status.success() {
                Ok(())
            } else {
                Err(format!("screencapture exited with {status}"))
            }
        },
        |image| ocr(image, &ocr_swift),
        |record| if stepping { hand_back(&stdin, record) } else { Ok(()) },
    )?;

    let manifest = write_manifest(&evidence, &scenario, &window, &outcome)?;

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
        println!("window:   {} ({}x{})", window.id, window.w, window.h);
        println!("evidence: {}", evidence.display());
        for r in &outcome.records {
            println!("{}", step_lines(r));
        }
        println!("manifest: {}", manifest.display());
        println!("VERDICT:  {}", if outcome.passed { "green" } else { "red" });
    }
    Ok(outcome.passed)
}

/// One step as the summary prints it: its verdict mark, number, kind and instruction, and the shot
/// it left behind. The same two lines a stepped run hands back at each boundary, so what an operator
/// reads mid-run and what the summary reports afterwards are the one rendering.
fn step_lines(r: &StepRecord) -> String {
    let mark = match r.verdict {
        Verdict::Action => "·",
        Verdict::Pass => "✓",
        Verdict::Fail => "✗",
        Verdict::Review => "?",
    };
    format!(
        "  {mark} {:02} [{}] {}\n        → {}",
        r.index + 1,
        r.kind,
        r.instruction,
        r.screenshot
    )
}

/// Hold the run at a step boundary: show the step whose shot is now on disk, then wait for one line
/// on stdin saying the screen has been moved on to the next one. It writes to stderr so a `--json`
/// run keeps stdout to its one machine-readable line, and a line is all it asks for — the content is
/// the operator's to use as a note to themselves.
///
/// End of input is a failure rather than a nod. A run asked to step with nothing left to hold it
/// would walk the rest of the scenario off whichever screen was up, and file those shots as evidence
/// of steps nobody carried out.
fn hand_back(stdin: &std::io::Stdin, record: &StepRecord) -> Result<(), String> {
    eprintln!("{}", step_lines(record));
    eprint!("  … carry out the next step on screen, then press Enter: ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    match stdin.lock().read_line(&mut line) {
        Ok(0) => Err("stdin reached end of input — a stepped run needs one line per step".into()),
        Ok(_) => Ok(()),
        Err(e) => Err(format!("could not read the go-ahead from stdin: {e}")),
    }
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
    uiauto: PathBuf,
    ocr: PathBuf,
    step: bool,
    json: bool,
    print: bool,
}

impl Opts {
    fn parse(args: impl Iterator<Item = String>) -> Result<Opts, String> {
        let mut scenario = None;
        let mut app = None;
        let mut evidence = None;
        let mut uiauto = None;
        let mut ocr = None;
        let mut step = false;
        let mut json = false;
        let mut print = false;
        let mut it = args.peekable();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--json" => json = true,
                "--step" => step = true,
                "--print" => print = true,
                "--app" => app = Some(PathBuf::from(it.next().ok_or("--app needs a bundle path")?)),
                "--evidence" => evidence = Some(PathBuf::from(it.next().ok_or("--evidence needs a path")?)),
                "--uiauto" => uiauto = Some(PathBuf::from(it.next().ok_or("--uiauto needs a path")?)),
                "--ocr" => ocr = Some(PathBuf::from(it.next().ok_or("--ocr needs a path")?)),
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
            uiauto: uiauto.unwrap_or_else(default_uiauto),
            ocr: ocr.unwrap_or_else(default_ocr),
            step,
            json,
            print,
        })
    }
}

/// uiauto.swift's place in the repo, resolved from this crate so it is found whatever the CWD:
/// `verification/gui` → `app/scripts/uiauto/uiauto.swift`.
fn default_uiauto() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // verification/
        .and_then(|p| p.parent()) // repo root
        .map(|p| p.join("app/scripts/uiauto/uiauto.swift"))
        .unwrap_or_else(|| PathBuf::from("app/scripts/uiauto/uiauto.swift"))
}

/// ocr.swift sits beside this crate (`verification/gui/ocr.swift`), resolved from the manifest dir
/// so it is found whatever the CWD.
fn default_ocr() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("ocr.swift")
}
