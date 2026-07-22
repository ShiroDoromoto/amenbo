//! amenbo-verify-gui — the mac GUI harness for pre-distribution verification (decision
//! `AMB-D-345`).
//!
//! The same scenario the CLI driver black-box-drives, this harness reads as a **screen
//! checklist**. It bakes in no command line and no pixel (`AMB-D-297`): each step becomes a
//! plain-language instruction of what to do or confirm on screen, the running GUI's window is
//! located through `app/scripts/uiauto/uiauto.swift`, and every step is captured with
//! `screencapture -l <winid>` into an evidence directory. Judging what a shot *shows* — OCR or
//! a human eye — is the sibling task (`AMB-T-1961`); this crate lays the rail those verdicts run
//! on: an ordered walk that leaves one screenshot per step.
//!
//! uiauto is the input primitive, called here, never moved: `window` resolves the id
//! `screencapture -l` needs and the bounds an operator uses to turn a shot's pixel into a click
//! point (uiauto's own coordinate rule), and its `click` / `type` / `key` carry out the action
//! steps the checklist names.
//!
//! The pure part — turning a step into an instruction, and walking a scenario into per-step
//! evidence — is separated from the side effects (running `swift`/`osascript`/`screencapture`) so
//! the walk is testable with a capture that only touches files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use amenbo_scenario::{Args, Domain, Scenario, Step};

// ---------------------------------------------------------------------------
// Locating and fronting the app (the side effects: swift / osascript)
// ---------------------------------------------------------------------------

/// The app window `screencapture` targets. `id` feeds `screencapture -l`; the bounds let an
/// operator translate a pixel in the shot to a screen click point, the way uiauto documents.
#[derive(Debug, Clone)]
pub struct Window {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Ask uiauto for the running app's window (`swift uiauto.swift window <pid>` → `id x y w h`).
/// The first substantial window wins — uiauto has already dropped the shadows and tooltips. An
/// empty answer means the app is not running or is behind another Space (see uiauto's own notes).
pub fn resolve_window(pid: i64, uiauto: &Path) -> Result<Window, String> {
    let out = Command::new("swift")
        .arg(uiauto)
        .arg("window")
        .arg(pid.to_string())
        .output()
        .map_err(|e| format!("could not run `swift {}`: {e}", uiauto.display()))?;
    if !out.status.success() {
        return Err(format!(
            "uiauto could not find a window for pid {pid}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout
        .lines()
        .next()
        .ok_or_else(|| format!("uiauto reported no window for pid {pid}"))?;
    parse_window(line)
}

/// Parse one `id x y w h` line from uiauto into a [`Window`].
fn parse_window(line: &str) -> Result<Window, String> {
    let f: Vec<&str> = line.split_whitespace().collect();
    if f.len() != 5 {
        return Err(format!("could not read a window from uiauto output `{line}`"));
    }
    let num = |s: &str, what: &str| s.parse::<f64>().map_err(|_| format!("uiauto gave a non-number {what} in `{line}`"));
    Ok(Window {
        id: f[0].to_string(),
        x: num(f[1], "x")?,
        y: num(f[2], "y")?,
        w: num(f[3], "width")?,
        h: num(f[4], "height")?,
    })
}

/// Bring the named app to the front, so its window counts as on-screen before a shot is taken
/// (uiauto's `window` skips a window behind another Space). A no-op the caller may skip when the
/// operator has already fronted the app.
pub fn activate(app: &str) -> Result<(), String> {
    let status = Command::new("osascript")
        .arg("-e")
        .arg(format!("tell application \"{app}\" to activate"))
        .status()
        .map_err(|e| format!("could not run osascript to activate `{app}`: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("osascript could not activate `{app}`"))
    }
}

// ---------------------------------------------------------------------------
// Turning a step into a screen instruction (the pure part)
// ---------------------------------------------------------------------------

/// Renders each step into a plain-language screen instruction, remembering the human label a
/// binding stands for so a later step that refers back by `target:` reads by name, not by id.
struct Instructor {
    labels: HashMap<String, String>,
}

impl Instructor {
    fn new() -> Instructor {
        Instructor { labels: HashMap::new() }
    }

    /// The human label a step's `target:` points at — the title the bound action created, or a
    /// `<name>` placeholder if it never bound one (the loader has already proven it resolves).
    fn target_label(&self, with: &Args) -> String {
        match with.get("target").and_then(|v| v.as_str()) {
            Some(name) => self.labels.get(name).cloned().unwrap_or_else(|| format!("<{name}>")),
            None => "<the target>".to_string(),
        }
    }

    /// One step → one instruction. Fails closed on a registry op this harness has not mapped yet
    /// — the same contract the CLI driver keeps, so a new op surfaces loudly here too rather than
    /// walking past with a blank instruction.
    fn render(&mut self, step: &Step) -> Result<String, String> {
        match step {
            Step::Action { domain, op, with, bind } => {
                let text = self.action(*domain, op, with)?;
                // A create binds its title as the label later steps read by.
                if let Some(name) = bind {
                    if let Some(title) = arg_str(with, "title") {
                        self.labels.insert(name.clone(), title.to_string());
                    }
                }
                Ok(text)
            }
            Step::Assert { domain, op, with } => self.assert(*domain, op, with),
        }
    }

    fn action(&self, domain: Domain, op: &str, with: &Args) -> Result<String, String> {
        Ok(match (domain, op) {
            (Domain::Task, "create") => {
                format!("Create a task titled \"{}\" on the board.", req(with, "title")?)
            }
            (Domain::Task, "assign") => format!(
                "Open the task \"{}\" and set its assignee to \"{}\".",
                self.target_label(with),
                req(with, "assignee")?
            ),
            (Domain::Task, "comment") => format!(
                "Open the task \"{}\" and add the comment \"{}\".",
                self.target_label(with),
                req(with, "text")?
            ),
            (Domain::Decision, "create") => {
                format!("Create a decision titled \"{}\".", req(with, "title")?)
            }
            _ => return Err(unmapped(domain, op)),
        })
    }

    fn assert(&self, domain: Domain, op: &str, with: &Args) -> Result<String, String> {
        Ok(match (domain, op) {
            (Domain::Task, "listed") => {
                let present = with.get("present").and_then(|v| v.as_bool()).unwrap_or(true);
                format!(
                    "Confirm the task \"{}\" is {} the listing filtered by `{}`.",
                    self.target_label(with),
                    if present { "present in" } else { "absent from" },
                    req(with, "filter")?
                )
            }
            (Domain::Task, "field") => format!(
                "Confirm the task \"{}\" shows {} = {}.",
                self.target_label(with),
                req(with, "field")?,
                show(with.get("equals").ok_or("assert `field` needs `equals`")?)
            ),
            _ => return Err(unmapped(domain, op)),
        })
    }
}

fn unmapped(domain: Domain, op: &str) -> String {
    format!(
        "op `{op}` for domain `{domain:?}` is in the scenario registry but not yet mapped in the GUI harness"
    )
}

fn arg_str<'a>(with: &'a Args, key: &str) -> Option<&'a str> {
    with.get(key).and_then(|v| v.as_str())
}

fn req<'a>(with: &'a Args, key: &str) -> Result<&'a str, String> {
    arg_str(with, key).ok_or_else(|| format!("arg `{key}` must be a string"))
}

/// Render an arbitrary scalar arg for display: a string as itself, anything else through YAML so
/// `equals: false` reads `false` and `equals: 3` reads `3`.
fn show(v: &serde_yaml::Value) -> String {
    match v.as_str() {
        Some(s) => s.to_string(),
        None => serde_yaml::to_string(v).unwrap_or_default().trim().to_string(),
    }
}

// ---------------------------------------------------------------------------
// Walking a scenario into per-step evidence
// ---------------------------------------------------------------------------

/// What one step left behind: its instruction and the screenshot filename (relative to the
/// evidence dir) that proves the operator stood at that step.
#[derive(Debug, Clone)]
pub struct StepRecord {
    pub index: usize,
    pub kind: &'static str,
    pub domain: String,
    pub op: String,
    pub instruction: String,
    pub screenshot: String,
}

/// Walk a scenario step by step, capturing one screenshot per step into `evidence_dir`. The
/// capture is injected — the caller passes a closure that shells out to `screencapture -l`, a
/// test passes one that only touches the file — so the ordered walk is verifiable without a GUI.
/// A capture that fails aborts the walk: a missing shot is missing evidence, not a soft miss.
pub fn walk<C>(scenario: &Scenario, evidence_dir: &Path, mut capture: C) -> Result<Vec<StepRecord>, String>
where
    C: FnMut(&Path) -> Result<(), String>,
{
    std::fs::create_dir_all(evidence_dir)
        .map_err(|e| format!("could not create evidence dir {}: {e}", evidence_dir.display()))?;

    let mut instructor = Instructor::new();
    let mut records = Vec::new();
    for (i, step) in scenario.steps.iter().enumerate() {
        let (kind, domain, op) = match step {
            Step::Action { domain, op, .. } => ("action", *domain, op.clone()),
            Step::Assert { domain, op, .. } => ("assert", *domain, op.clone()),
        };
        let instruction = instructor.render(step)?;
        let domain = domain_str(domain);
        let screenshot = format!("{:02}-{kind}-{domain}-{op}.png", i + 1);
        capture(&evidence_dir.join(&screenshot))
            .map_err(|e| format!("step {}: capturing `{screenshot}` failed: {e}", i + 1))?;
        records.push(StepRecord { index: i, kind, domain: domain.to_string(), op, instruction, screenshot });
    }
    Ok(records)
}

fn domain_str(d: Domain) -> &'static str {
    match d {
        Domain::Task => "task",
        Domain::Decision => "decision",
        Domain::Comment => "comment",
        Domain::Project => "project",
    }
}

/// Write the run's manifest — the scenario, the window it was shot against, and every step's
/// instruction and screenshot — as JSON into the evidence dir, so a later pass (OCR or human)
/// reads the checklist and its evidence back without re-walking the scenario.
pub fn write_manifest(
    dir: &Path,
    scenario: &Scenario,
    window: &Window,
    records: &[StepRecord],
) -> Result<PathBuf, String> {
    let steps: Vec<String> = records
        .iter()
        .map(|r| {
            format!(
                "{{\"step\":{},\"kind\":{},\"domain\":{},\"op\":{},\"instruction\":{},\"screenshot\":{}}}",
                r.index + 1,
                js(r.kind),
                js(&r.domain),
                js(&r.op),
                js(&r.instruction),
                js(&r.screenshot)
            )
        })
        .collect();
    let json = format!(
        "{{\"scenario\":{},\"title\":{},\"window\":{{\"id\":{},\"x\":{},\"y\":{},\"w\":{},\"h\":{}}},\"steps\":[{}]}}",
        js(&scenario.id),
        js(&scenario.title),
        js(&window.id),
        window.x,
        window.y,
        window.w,
        window.h,
        steps.join(",")
    );
    let path = dir.join("manifest.json");
    std::fs::write(&path, json).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(path)
}

/// Encode a string as a JSON string literal (correct escaping, no bespoke struct just for output).
fn js(s: &str) -> String {
    serde_json::Value::String(s.to_string()).to_string()
}

/// The same JSON-string encoding, for the bin's own `--json` line.
pub fn js_out(s: &str) -> String {
    js(s)
}

// ---------------------------------------------------------------------------
// Tests — the pure walk and the instruction rendering, no GUI required
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const SCENARIO: &str = r#"
id: sample
title: A task assigned to me-ai surfaces in the listing
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

    fn load(yaml: &str) -> Scenario {
        let s = amenbo_scenario::load_str(yaml).expect("parses");
        s.validate().expect("valid");
        s
    }

    #[test]
    fn instructions_read_a_bound_target_by_its_title() {
        let s = load(SCENARIO);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps.iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("Create a task titled \"SEED\""));
        // The assign and the assert refer back to the create by title, not by binding name.
        assert!(lines[1].contains("\"SEED\"") && lines[1].contains("me-ai"));
        assert!(lines[2].contains("\"SEED\"") && lines[2].contains("present in"));
    }

    #[test]
    fn an_absent_assert_reads_absent() {
        let yaml = r#"
id: x
title: y
steps:
  - type: action
    domain: task
    op: create
    with: { title: T }
    as: a
  - type: assert
    domain: task
    op: listed
    with: { filter: "assignee:none", target: a, present: false }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let line = ins.render(&s.steps[1]).unwrap();
        assert!(line.contains("absent from"), "got: {line}");
    }

    #[test]
    fn an_unmapped_op_fails_closed() {
        // A step the loader would reject cannot reach a real walk, but the renderer must still
        // fail loudly rather than emit a blank instruction if the registry grows past the map.
        let step = Step::Action {
            domain: Domain::Task,
            op: "frobnicate".to_string(),
            with: Args::new(),
            bind: None,
        };
        let err = Instructor::new().render(&step).unwrap_err();
        assert!(err.contains("not yet mapped"), "got: {err}");
    }

    #[test]
    fn walk_leaves_one_screenshot_per_step_in_order() {
        let s = load(SCENARIO);
        let dir = std::env::temp_dir().join(format!("amenbo-verify-gui-selftest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let seen: RefCell<Vec<PathBuf>> = RefCell::new(Vec::new());
        let records = walk(&s, &dir, |p| {
            std::fs::write(p, b"fake-png").map_err(|e| e.to_string())?;
            seen.borrow_mut().push(p.to_path_buf());
            Ok(())
        })
        .expect("walk");

        assert_eq!(records.len(), s.steps.len());
        assert_eq!(seen.borrow().len(), s.steps.len(), "one capture per step");
        // Screenshots are numbered in step order and all landed on disk.
        assert!(records[0].screenshot.starts_with("01-action-task-create"));
        assert!(records[2].screenshot.starts_with("03-assert-task-listed"));
        for r in &records {
            assert!(dir.join(&r.screenshot).is_file(), "{} exists", r.screenshot);
        }

        let win = Window { id: "42".into(), x: 0.0, y: 0.0, w: 800.0, h: 600.0 };
        let manifest = write_manifest(&dir, &s, &win, &records).expect("manifest");
        let text = std::fs::read_to_string(&manifest).unwrap();
        assert!(text.contains("\"scenario\":\"sample\""));
        assert!(text.contains("\"screenshot\":\"01-action-task-create.png\""));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_capture_failure_aborts_the_walk() {
        let s = load(SCENARIO);
        let dir = std::env::temp_dir().join(format!("amenbo-verify-gui-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let err = walk(&s, &dir, |_| Err("no screen".to_string())).unwrap_err();
        assert!(err.contains("step 1") && err.contains("no screen"), "got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_window_reads_id_and_bounds() {
        let w = parse_window("7 100 200 1440 900").unwrap();
        assert_eq!(w.id, "7");
        assert_eq!((w.x, w.y, w.w, w.h), (100.0, 200.0, 1440.0, 900.0));
        assert!(parse_window("garbage").is_err());
    }
}
