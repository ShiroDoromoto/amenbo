//! amenbo-verify-gui — the mac GUI harness for pre-distribution verification.
//!
//! The same scenario the CLI driver black-box-drives, this harness reads as a **screen
//! checklist**. It bakes in no command line and no pixel: each step becomes a plain-language
//! instruction of what to do or confirm on screen, the running GUI's window is located through
//! `app/scripts/uiauto/uiauto.swift`, and every step is captured with `screencapture -l <winid>`
//! into an evidence directory.
//!
//! An assert step is judged from that shot with macOS's own **Vision** OCR (`ocr.swift`): the
//! harness derives the text the step expects on screen and reads the shot back, passing when that
//! text is present (or absent, for a `present: false` assert). An assert OCR cannot mechanically
//! judge — a structured field value — is left as a `Review`: its shot is kept for an AI/human eye,
//! the run is not failed by it. tesseract stays the Linux container path
//! (`scripts/docker/gui-e2e.sh`); each driver maps the one scenario source to its own world.
//!
//! uiauto is the input primitive, called here, never moved: `window` resolves the id
//! `screencapture -l` needs and the bounds an operator uses to turn a shot's pixel into a click
//! point (uiauto's own coordinate rule), and its `click` / `type` / `key` carry out the action
//! steps the checklist names.
//!
//! The pure part — turning a step into an instruction and an expectation, and walking a scenario
//! into per-step evidence with a verdict — is separated from the side effects (running
//! `swift`/`osascript`/`screencapture`) so the walk is testable with injected capture and OCR.

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

/// Read the text off a screenshot with `ocr.swift` (macOS Vision). Returns the recognized text as
/// one string (Vision's per-region lines joined by newlines), which the caller
/// judges an expected string against by substring. An error is an execution failure, not a miss:
/// a shot Vision read but found no text in comes back as `Ok("")`.
pub fn ocr(image: &Path, ocr_swift: &Path) -> Result<String, String> {
    let out = Command::new("swift")
        .arg(ocr_swift)
        .arg(image)
        .output()
        .map_err(|e| format!("could not run `swift {}`: {e}", ocr_swift.display()))?;
    if !out.status.success() {
        return Err(format!(
            "ocr.swift failed on {}: {}",
            image.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// ---------------------------------------------------------------------------
// Turning a step into a screen instruction and an expectation (the pure part)
// ---------------------------------------------------------------------------

/// What an assert step expects on screen: a `text` that must be `present` (or, when `present` is
/// false, absent). The harness judges it by reading the step's shot back with OCR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expectation {
    pub text: String,
    pub present: bool,
}

/// Renders each step into a plain-language screen instruction and, for an assert OCR can judge, an
/// [`Expectation`]. It remembers the human label a binding stands for so a later step that refers
/// back by `target:` reads — and is judged — by name, not by id.
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
    /// walking past with a blank instruction. An action also records the label later steps read by.
    fn render(&mut self, step: &Step) -> Result<String, String> {
        match step {
            Step::Action { domain, op, with, bind } => {
                let text = self.action(*domain, op, with)?;
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

    /// The text an assert step expects on screen, when OCR can judge it. `listed` expects the
    /// bound title present (or absent); a `field` value is not something OCR reads off a card
    /// reliably, so it returns `None` and the step is left for a visual `Review`.
    fn expectation(&self, step: &Step) -> Option<Expectation> {
        if let Step::Assert { domain: Domain::Task, op, with } = step {
            if op == "listed" {
                let present = with.get("present").and_then(|v| v.as_bool()).unwrap_or(true);
                return Some(Expectation { text: self.target_label(with), present });
            }
        }
        None
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
// Walking a scenario into per-step evidence with a verdict
// ---------------------------------------------------------------------------

/// One step's verdict. An action carries no screen judgment; an OCR-judged assert is `Pass` or
/// `Fail`; an assert OCR cannot mechanically judge is `Review` — kept for an AI/human eye, never a
/// run failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Action,
    Pass,
    Fail,
    Review,
}

impl Verdict {
    fn as_str(self) -> &'static str {
        match self {
            Verdict::Action => "action",
            Verdict::Pass => "pass",
            Verdict::Fail => "fail",
            Verdict::Review => "review",
        }
    }
}

/// What one step left behind: its instruction, the screenshot proving the operator stood at it,
/// the verdict, and — for a judged assert — the expected text and whether OCR found it.
#[derive(Debug, Clone)]
pub struct StepRecord {
    pub index: usize,
    pub kind: &'static str,
    pub domain: String,
    pub op: String,
    pub instruction: String,
    pub screenshot: String,
    pub verdict: Verdict,
    pub expected: Option<Expectation>,
    pub found: Option<bool>,
}

/// The whole walk: the per-step records and the roll-up. `passed` is the AND of every OCR-judged
/// assert (actions and `Review` steps never fail it), so a release gate reads it directly.
#[derive(Debug)]
pub struct WalkOutcome {
    pub records: Vec<StepRecord>,
    pub passed: bool,
}

/// Walk a scenario step by step: capture one screenshot per step into `evidence_dir`, and for an
/// assert OCR can judge, read the shot back and decide `Pass`/`Fail` against the expected text.
/// Both side effects are injected — `capture` shells out to `screencapture`, `read_text` to
/// `ocr.swift`; a test passes closures that only touch/return fixtures — so the walk is verifiable
/// without a GUI. A capture failure aborts the walk (a missing shot is missing evidence); the
/// recognized text of each judged step is written next to its shot as evidence of the reading.
pub fn walk<C, O>(
    scenario: &Scenario,
    evidence_dir: &Path,
    mut capture: C,
    mut read_text: O,
) -> Result<WalkOutcome, String>
where
    C: FnMut(&Path) -> Result<(), String>,
    O: FnMut(&Path) -> Result<String, String>,
{
    std::fs::create_dir_all(evidence_dir)
        .map_err(|e| format!("could not create evidence dir {}: {e}", evidence_dir.display()))?;

    let mut instructor = Instructor::new();
    let mut records = Vec::new();
    let mut passed = true;

    for (i, step) in scenario.steps.iter().enumerate() {
        let (kind, domain, op) = match step {
            Step::Action { domain, op, .. } => ("action", *domain, op.clone()),
            Step::Assert { domain, op, .. } => ("assert", *domain, op.clone()),
        };
        let instruction = instructor.render(step)?;
        let expected = instructor.expectation(step);
        let domain = domain_str(domain);
        let screenshot = format!("{:02}-{kind}-{domain}-{op}.png", i + 1);
        let shot_path = evidence_dir.join(&screenshot);
        capture(&shot_path)
            .map_err(|e| format!("step {}: capturing `{screenshot}` failed: {e}", i + 1))?;

        // Judge an assert that named an expectation; keep the reading as evidence.
        let (verdict, found) = match (kind, &expected) {
            ("assert", Some(exp)) => {
                let text = read_text(&shot_path)
                    .map_err(|e| format!("step {}: reading `{screenshot}` failed: {e}", i + 1))?;
                let hit = text.contains(&exp.text);
                let _ = std::fs::write(
                    evidence_dir.join(format!("{:02}-{kind}-{domain}-{op}.txt", i + 1)),
                    &text,
                );
                let pass = hit == exp.present;
                if !pass {
                    passed = false;
                }
                (if pass { Verdict::Pass } else { Verdict::Fail }, Some(hit))
            }
            ("assert", None) => (Verdict::Review, None),
            _ => (Verdict::Action, None),
        };

        records.push(StepRecord {
            index: i,
            kind,
            domain: domain.to_string(),
            op,
            instruction,
            screenshot,
            verdict,
            expected,
            found,
        });
    }
    Ok(WalkOutcome { records, passed })
}

fn domain_str(d: Domain) -> &'static str {
    match d {
        Domain::Task => "task",
        Domain::Decision => "decision",
        Domain::Comment => "comment",
        Domain::Project => "project",
        Domain::Store => "store",
        Domain::Folder => "folder",
    }
}

/// Write the run's manifest — the scenario, the window it was shot against, the roll-up, and every
/// step's instruction, verdict and evidence — as JSON into the evidence dir, so a later pass (a
/// human closing the `Review`s, or a release gate) reads the checklist and its verdicts back
/// without re-walking the scenario.
pub fn write_manifest(
    dir: &Path,
    scenario: &Scenario,
    window: &Window,
    outcome: &WalkOutcome,
) -> Result<PathBuf, String> {
    let steps: Vec<String> = outcome
        .records
        .iter()
        .map(|r| {
            let expect = match (&r.expected, r.found) {
                (Some(e), Some(found)) => format!(
                    ",\"expected\":{},\"present\":{},\"found\":{}",
                    js(&e.text),
                    e.present,
                    found
                ),
                _ => String::new(),
            };
            format!(
                "{{\"step\":{},\"kind\":{},\"domain\":{},\"op\":{},\"verdict\":{},\"instruction\":{},\"screenshot\":{}{}}}",
                r.index + 1,
                js(r.kind),
                js(&r.domain),
                js(&r.op),
                js(r.verdict.as_str()),
                js(&r.instruction),
                js(&r.screenshot),
                expect
            )
        })
        .collect();
    let json = format!(
        "{{\"scenario\":{},\"title\":{},\"passed\":{},\"window\":{{\"id\":{},\"x\":{},\"y\":{},\"w\":{},\"h\":{}}},\"steps\":[{}]}}",
        js(&scenario.id),
        js(&scenario.title),
        outcome.passed,
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
// Tests — the pure walk, the instruction/expectation rendering, and the OCR
// verdict, all with injected side effects (no GUI, no Vision required)
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
        assert!(lines[1].contains("\"SEED\"") && lines[1].contains("me-ai"));
        assert!(lines[2].contains("\"SEED\"") && lines[2].contains("present in"));
    }

    #[test]
    fn a_listed_assert_expects_the_bound_title_present() {
        let s = load(SCENARIO);
        let mut ins = Instructor::new();
        for st in &s.steps {
            ins.render(st).unwrap();
        }
        let exp = ins.expectation(&s.steps[2]).expect("listed has an expectation");
        assert_eq!(exp, Expectation { text: "SEED".to_string(), present: true });
    }

    #[test]
    fn a_field_assert_is_left_for_review() {
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
    op: field
    with: { target: a, field: status, equals: todo }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        ins.render(&s.steps[0]).unwrap();
        assert!(ins.expectation(&s.steps[1]).is_none(), "a field assert is not OCR-judged");
    }

    #[test]
    fn an_unmapped_op_fails_closed() {
        let step = Step::Action {
            domain: Domain::Task,
            op: "frobnicate".to_string(),
            with: Args::new(),
            bind: None,
        };
        let err = Instructor::new().render(&step).unwrap_err();
        assert!(err.contains("not yet mapped"), "got: {err}");
    }

    /// The whole walk with fakes: every step shot, the `listed` assert OCR-judged green when the
    /// title is on screen, and one screenshot + one manifest per run.
    #[test]
    fn walk_captures_every_step_and_judges_the_assert_green() {
        let s = load(SCENARIO);
        let dir = std::env::temp_dir().join(format!("amenbo-verify-gui-green-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let shots: RefCell<usize> = RefCell::new(0);
        let outcome = walk(
            &s,
            &dir,
            |p| {
                *shots.borrow_mut() += 1;
                std::fs::write(p, b"fake-png").map_err(|e| e.to_string())
            },
            // The board OCRs to text that contains the seed title.
            |_| Ok("me-ai board\nSEED\nsome other card".to_string()),
        )
        .expect("walk");

        assert!(outcome.passed, "the listed assert is green when SEED is on screen");
        assert_eq!(*shots.borrow(), s.steps.len(), "one shot per step");
        let assert_rec = outcome.records.iter().find(|r| r.kind == "assert").unwrap();
        assert_eq!(assert_rec.verdict, Verdict::Pass);
        assert_eq!(assert_rec.found, Some(true));
        // The reading is kept next to the shot as evidence.
        assert!(dir.join("03-assert-task-listed.txt").is_file());

        let win = Window { id: "42".into(), x: 0.0, y: 0.0, w: 800.0, h: 600.0 };
        let manifest = write_manifest(&dir, &s, &win, &outcome).expect("manifest");
        let text = std::fs::read_to_string(&manifest).unwrap();
        assert!(text.contains("\"passed\":true"));
        assert!(text.contains("\"verdict\":\"pass\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// When the expected title is NOT on the shot, a `present: true` assert fails and reddens the
    /// run — the reading is the whole verdict, not a pixel diff.
    #[test]
    fn walk_reds_the_run_when_the_expected_text_is_missing() {
        let s = load(SCENARIO);
        let dir = std::env::temp_dir().join(format!("amenbo-verify-gui-red-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let outcome = walk(
            &s,
            &dir,
            |p| std::fs::write(p, b"fake-png").map_err(|e| e.to_string()),
            |_| Ok("an empty board with no such card".to_string()),
        )
        .expect("walk");

        assert!(!outcome.passed, "SEED absent ⇒ the present-assert fails");
        let assert_rec = outcome.records.iter().find(|r| r.kind == "assert").unwrap();
        assert_eq!(assert_rec.verdict, Verdict::Fail);
        assert_eq!(assert_rec.found, Some(false));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_capture_failure_aborts_the_walk() {
        let s = load(SCENARIO);
        let dir = std::env::temp_dir().join(format!("amenbo-verify-gui-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let err = walk(&s, &dir, |_| Err("no screen".to_string()), |_| Ok(String::new()))
            .unwrap_err();
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
