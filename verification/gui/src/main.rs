//! `verify-gui` — drive one verification scenario as a mac GUI checklist.
//!
//! It reads the same scenario the CLI driver black-box-drives, renders each step into a screen
//! instruction (no command line, no pixel), locates the running GUI's window through
//! `app/scripts/uiauto/uiauto.swift`, and captures one `screencapture -l <winid>` per step into an
//! evidence directory. Each assert OCR can judge is decided from its shot with macOS Vision
//! (`ocr.swift`); an assert it cannot judge is left as a `Review` for a human eye, and its shot is
//! kept.
//!
//! Usage: `verify-gui <scenario.yaml> (--pid <pid> | --winid <id>) [--app <name>]
//!                    [--evidence <dir>] [--uiauto <path>] [--ocr <path>] [--json]`
//!   `--pid`      pid of the running GUI app; its window is resolved via uiauto (gives bounds too)
//!   `--winid`    a window id to shoot directly, skipping uiauto (bounds unknown)
//!   `--app`      bring this app to the front before shooting (e.g. `amenbo (dev)`)
//!   `--evidence` where the shots + manifest land (default: a fresh dir under the temp tree)
//!   `--uiauto`   path to uiauto.swift (default: app/scripts/uiauto/uiauto.swift in the repo)
//!   `--ocr`      path to ocr.swift (default: the ocr.swift beside this crate)
//!   `--json`     emit the manifest path, verdict and step count as JSON instead of the summary
//!
//! Exit code is the machine signal: 0 when every OCR-judged assert passed and every step was
//! captured, non-zero on a failed assert, a load failure, or a capture/OCR failure. A `Review`
//! step (an assert OCR cannot judge) does not fail the run — a human closes it from the evidence.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

use amenbo_verify_gui::{activate, ocr, resolve_window, walk, write_manifest, Verdict, Window};

fn main() -> ExitCode {
    let opts = match Opts::parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(msg) => {
            eprintln!("verify-gui: {msg}");
            eprintln!("usage: verify-gui <scenario.yaml> (--pid <pid> | --winid <id>) [--app <name>] [--evidence <dir>] [--uiauto <path>] [--ocr <path>] [--json]");
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

    // Front the app first so its window counts as on-screen (uiauto skips one behind a Space).
    if let Some(app) = &opts.app {
        activate(app)?;
    }

    let window = match (&opts.winid, opts.pid) {
        (Some(id), _) => Window { id: id.clone(), x: 0.0, y: 0.0, w: 0.0, h: 0.0 },
        (None, Some(pid)) => resolve_window(pid, &opts.uiauto)?,
        (None, None) => return Err("need one of --pid or --winid to know which window to shoot".into()),
    };

    let evidence = opts
        .evidence
        .clone()
        .unwrap_or_else(|| default_evidence_dir(&scenario.id));

    let capture_bin =
        std::env::var_os("AMENBO_GUI_CAPTURE_BIN").unwrap_or_else(|| "screencapture".into());
    let winid = window.id.clone();
    let ocr_swift = opts.ocr.clone();
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
            let mark = match r.verdict {
                Verdict::Action => "·",
                Verdict::Pass => "✓",
                Verdict::Fail => "✗",
                Verdict::Review => "?",
            };
            println!("  {mark} {:02} [{}] {}", r.index + 1, r.kind, r.instruction);
            println!("        → {}", r.screenshot);
        }
        println!("manifest: {}", manifest.display());
        println!("VERDICT:  {}", if outcome.passed { "green" } else { "red" });
    }
    Ok(outcome.passed)
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
    pid: Option<i64>,
    winid: Option<String>,
    app: Option<String>,
    evidence: Option<PathBuf>,
    uiauto: PathBuf,
    ocr: PathBuf,
    json: bool,
}

impl Opts {
    fn parse(args: impl Iterator<Item = String>) -> Result<Opts, String> {
        let mut scenario = None;
        let mut pid = None;
        let mut winid = None;
        let mut app = None;
        let mut evidence = None;
        let mut uiauto = None;
        let mut ocr = None;
        let mut json = false;
        let mut it = args.peekable();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--json" => json = true,
                "--pid" => {
                    let v = it.next().ok_or("--pid needs a number")?;
                    pid = Some(v.parse::<i64>().map_err(|_| format!("--pid `{v}` is not a number"))?);
                }
                "--winid" => winid = Some(it.next().ok_or("--winid needs an id")?),
                "--app" => app = Some(it.next().ok_or("--app needs a name")?),
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
            pid,
            winid,
            app,
            evidence,
            uiauto: uiauto.unwrap_or_else(default_uiauto),
            ocr: ocr.unwrap_or_else(default_ocr),
            json,
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
