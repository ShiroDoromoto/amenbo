//! amenbo-verify-gui — the mac GUI harness for pre-distribution verification.
//!
//! The same scenario the CLI driver black-box-drives, this harness reads as a **screen
//! checklist**. It bakes in no command line and no pixel: each step becomes a plain-language
//! instruction of what to do or confirm on screen, and every step is shot into an evidence
//! directory by the screen tool (`scripts/screen.swift`), which is named the app's pid and hands
//! back a file — which window it shot, and the id it shot by, never leave it. The pid it is named
//! is the harness's own: the app under test is started here, against a throwaway store
//! ([`launch`], [`scratch`]), and goes down with the run. The world that store holds when the app
//! opens it is the scenario's `given`, stood up beforehand with the CLI the bundle ships
//! ([`amenbo_verify_cli::stand_world`]) — never the screen's own moves, which are the road.
//!
//! An assert step is judged from that shot by asking the same tool to read it (macOS **Vision**
//! behind it): the harness derives the text the step expects on screen and matches it against the
//! reading, passing when it is present (or absent, for a `present: false` assert). An assert OCR
//! cannot mechanically judge — a structured field value — is left as a `Review`: its shot is kept
//! for an AI/human eye, the run is not failed by it. tesseract stays the Linux container path
//! (`scripts/docker/gui-e2e.sh`); each driver maps the one scenario source to its own world.
//!
//! The screen tool is the input primitive too, called by whoever drives the screen between steps:
//! its `find` / `click-named` / `click` / `dblclick` / `type` / `key` carry out the action steps
//! the checklist names.
//!
//! The pure part — turning a step into an instruction and an expectation, and walking a scenario
//! into per-step evidence with a verdict — is separated from the side effects (running the tool)
//! so the walk is testable with injected capture, reading and step-boundary wait.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use amenbo_scenario::{Args, Domain, Driver, Scenario, Step};

/// Starting the app under test and holding it — the pid every shot is aimed at comes from here.
pub mod launch;
/// The line the run stands on: only a bundle the release workflow produced is launched.
pub mod shipped;

/// The throwaway store that app is launched against — the CLI driver's own, not a second one. What
/// isolates a run is the same two things whichever driver is asking (`AMENBO_HOME` at a directory
/// the run made, and a working directory carrying no pointer to a real project), and the premise is
/// stood up in this store by the shipped CLI, so a store of this harness's own would be a second
/// implementation of one rule with two drivers walking into it.
pub use amenbo_verify_cli::scratch;

// ---------------------------------------------------------------------------
// The screen tool (the side effects: front, shoot, read)
// ---------------------------------------------------------------------------

/// What the tool read off one shot. `text` is the reading folded to its words — the correction the
/// reader behind the tool needs, applied by the tool — and `raw` is what that reader handed back
/// before it. A verdict is taken on `text`; `raw` is what a person reads when one comes out red.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub text: String,
    pub raw: String,
}

/// Run one of the tool's subcommands and hand back its stdout.
fn tool(screen: &Path, cmd: &str, args: &[&OsStr]) -> Result<Vec<u8>, String> {
    let out = Command::new("swift")
        .arg(screen)
        .arg(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("could not run `swift {} {cmd}`: {e}", screen.display()))?;
    if !out.status.success() {
        return Err(format!(
            "`screen {cmd}` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}

/// Bring the app under test to the front, so the window the tool goes looking for counts as
/// on-screen (one behind another Space does not).
pub fn front(pid: i64, screen: &Path) -> Result<(), String> {
    tool(screen, "front", &[OsStr::new(&pid.to_string())]).map(|_| ())
}

/// Shoot the app's window into `path`. The harness names the app by pid and receives a file: which
/// of its windows was shot, and the id the shot was taken by, are the tool's and stay there.
pub fn shoot(pid: i64, path: &Path, screen: &Path) -> Result<(), String> {
    tool(screen, "shot", &[OsStr::new(&pid.to_string()), path.as_os_str()]).map(|_| ())
}

/// Read the words off a shot. An error is an execution failure, not a miss: a shot the reader found
/// no text in comes back as an empty [`Reading`], which is the honest answer for an assert that
/// expected words there.
pub fn read_shot(image: &Path, screen: &Path) -> Result<Reading, String> {
    let out = tool(screen, "read", &[image.as_os_str()])?;
    let v: serde_json::Value = serde_json::from_slice(&out)
        .map_err(|e| format!("could not read `screen read {}` as JSON: {e}", image.display()))?;
    let field = |k: &str| {
        v.get(k)
            .and_then(|s| s.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("`screen read {}` answered without `{k}`", image.display()))
    };
    Ok(Reading { text: field("text")?, raw: field("raw")? })
}

/// The dashes Unicode files under letters. A long vowel mark is what Vision most often returns for an
/// em dash on a Japanese screen, and it is alphanumeric where every other dash is punctuation — so it
/// would survive the fold on the read side while the dash it stands for is dropped on the expected
/// one, and the two halves of the same title would stop matching. Dropped on both sides, a title that
/// really carries one still matches itself.
const DASHES_FILED_AS_LETTERS: [char; 2] = ['\u{30FC}', '\u{FF70}'];

/// Fold an expectation to the part of it a reading can be held to: the words, not the glyphs. Vision
/// reads the words on a card reliably and the punctuation between them however it likes — an em dash
/// comes back as a hyphen, a space, a long vowel mark, or nothing — so a verbatim comparison fails on
/// a title no human would call misread. Case goes the same way, and a line break where the card
/// wrapped folds to the single space the title was written with. Alphanumerics are what survives,
/// Japanese included: the screen under test is in Japanese and is judged by this same rule.
///
/// This side of the match is the expectation's, which is this harness's own text. The reading's side
/// is folded by the same rule in the tool that read it, where a reader's habits belong
/// (`scripts/screen.swift`) — the two meet already folded.
fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    for c in s.chars() {
        if c.is_alphanumeric() && !DASHES_FILED_AS_LETTERS.contains(&c) {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.extend(c.to_lowercase());
        } else {
            pending_space = true;
        }
    }
    out
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

    /// Learn what the world a road starts from left standing, without rendering a word of it. The
    /// premise is the driver's to stand up and none of it is an instruction — but what it made is
    /// what a road then points at, and an operator has to be told to open "Greenhouse" rather than
    /// to open a binding they cannot see.
    ///
    /// It renders nothing on purpose, which is also what keeps it from failing: the premise takes
    /// ops that never reach the screen, and this harness maps the screen's.
    fn learn(&mut self, steps: &[Step]) {
        for step in steps {
            if let Step::Action { with, bind: Some(name), .. } = step {
                if let Some(label) = label(with) {
                    self.labels.insert(name.clone(), label.to_string());
                }
            }
        }
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
                let mut text = self.action(*domain, op, with)?;
                // A step that says the screen will turn it away. The code `refused:` names is what a
                // driver reading an exit status compares against, and there is no exit status here — so
                // what it does on this road is tell the operator that being refused is the step going
                // right, rather than their own hand going wrong. Which guard did the refusing is left to
                // the assert after it: what a screen offers is a sentence, not a code.
                if with.contains_key("refused") {
                    text.push_str(
                        " Expect it to be turned away rather than to go through — the refusal is what this step walks.",
                    );
                }
                if let Some(name) = bind {
                    if let Some(label) = label(with) {
                        self.labels.insert(name.clone(), label.to_string());
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
    ///
    /// `narrowed` is judged the same way and on the same text, since the card is what the reading is
    /// about either way. The words the reader typed stand in the box on that very shot, so the
    /// expectation is the whole title rather than any word of it: a card that went carries none of its
    /// title, and the half of the query still on screen cannot read as the card that left.
    ///
    /// `found` is judged on the same text again — every hit row leads with the ref and the title of the
    /// record it belongs to, whichever face the words were written on. What that settles is that the
    /// record is among the hits, which is the half a screen can be read for. The face itself is a word of
    /// the interface, and the row carrying it is drawn under a title that is on the shot either way, so
    /// the face named in a step is for the eye and the instruction says it out loud. The other side of
    /// that: a `present: false` step here has to be one where the whole *record* leaves the answer, since
    /// a hit on any of its faces puts its title back on the shot.
    ///
    /// `opened` is the step after that press, and it is judged on the phrase it names instead of on the
    /// record's title, for the reason `found` cannot be: the title is on the hit row the press was made
    /// from, so a shot where nothing opened at all would carry it — and so would one where the wrong
    /// record did.
    ///
    /// `browsed` is judged the same way when it says an entry is **not** official: the badge such a
    /// row wears is the serving catalog's name, which is a name the user gave and not a word of the
    /// interface, so it reads the same whatever language the app is in. The official badge is a word
    /// of the interface, so an `official: true` line has no text to derive and is left for a
    /// `Review`.
    ///
    /// `detail` follows that same line: what it expects is the declaration the step named, which the
    /// catalog's document carries in the author's own words — an event id, or a setting's label.
    ///
    /// `first-loop` too: what it expects is the command the handed-over request tells the reader's
    /// AI to run, which is the same words in any language the app is in. Its sibling `first-loop-order`
    /// names an order instead, and an order is not something a reading settles — which words are on a
    /// shot is all OCR answers — so that one is left for a `Review`.
    ///
    /// `none-linked` is read the same way, and for the same reason twice over: the warning's own words
    /// are the interface's, and the command it is judged on is what the two notices that would
    /// otherwise be standing there both hand over — so a reading that comes back without it says both
    /// that the loop is not up and that the wiring text is not either.
    ///
    /// `ways-in` is the one assert judged the other way round: what it names is a command, and a
    /// command is the same words in any language, so the reading has to come back without it. Its
    /// sibling `open-existing` names a project, and a reading answers which words are on a shot and
    /// never which part of the window they came from — the same name is in the list of projects
    /// down the side of every screen, so a reading of it would pass wherever the run was pointed.
    /// That one is a `Review`, closed by an eye on the shot.
    ///
    /// `fires-in` is a `Review` for that same reason, and it is the whole of that road: what every
    /// line of it names is a project, and the projects run down the side of every screen — so a
    /// reading that finds the name cannot say it came from the row, and one that fails to find it
    /// would be reading a screen the plugin's row is not even on.
    ///
    /// `project plugin-row` reads the same crossing from the other face and is a `Review` for a reason
    /// of its own: what separates a row just drawn from one turned on is the button standing in it, and
    /// a button's label is a word of the interface. Both states put the plugin's name on the shot, so
    /// finding it settles neither.
    ///
    /// `config` is a `Review` for a reason of its own: what a form says about a choice is which of
    /// its boxes are ticked and which chip the field wears. The candidates are drawn whichever answer
    /// is held, so their words are on every shot of the road, and the chip is a word of the interface
    /// — neither is something the presence of text can settle.
    ///
    /// `settings-in` is a `Review` on all three of its states. Two of them are marks the row wears, which
    /// are words of the interface; the third turns on a picker that is not there, and a reading answers
    /// which words are on a shot and never which are missing from the right part of it.
    ///
    /// The two `ai-launch` readings are judged on what is not the interface's own words either. The
    /// hand-over is judged on the file the text goes into, which is the one thing on the road that
    /// appears nowhere else on that board, so a shot taken where the report is not standing reads as the
    /// miss it is — and the same word read the other way (`present: false`) is what says the report
    /// went. `ai-launch-folder` is judged on the folder's own name, which the reader gave it and
    /// the interface has no word of its own for; the board the report stands on names no folder anywhere
    /// else, so finding one on that shot is finding it in the list.
    ///
    /// `ai-launch-answer` is the third of them and a `Review`, for the reason `plugin config`'s state
    /// is: all three of its answers — the yes, the no, and the never asked — are drawn as words of the
    /// interface, and which of them is standing is not something the presence of text can settle.
    ///
    /// `ai-launch-waiting` is a `Review` for the reason `open-existing` is: the folder it names is
    /// listed a second time on that same screen, among the ones bound to the project, so a reading of
    /// the name would pass over a build that dropped the inventory and kept the binding.
    ///
    /// `narrowing-shut` is a `Review`, and doubly so. What it is about is a box refusing the hand, which
    /// leaves no text on a shot either way — and what tells a box shut from a box merely empty is the
    /// words standing in it in place of an example, which are the interface's own and would hold this
    /// gate to whichever language the run was set up in.
    ///
    /// `nudge` is a `Review`, and the sentence it names is why: an offer is put in the interface's own
    /// words, so a reading of it would hold this gate to the one language the run happened to be set up
    /// in. What the step names is written down all the same — it is what the eye closing the shot is
    /// looking for. The road has a second reader besides that one: an offer that never came up is an
    /// offer nobody can decline, so the step after it cannot be carried out at all.
    fn expectation(&self, step: &Step) -> Option<Expectation> {
        let Step::Assert { domain, op, with } = step else { return None };
        match (*domain, op.as_str()) {
            (Domain::Task, "listed")
            | (Domain::Task, "narrowed")
            | (Domain::Task, "found")
            | (Domain::Decision, "found") => {
                Some(Expectation { text: self.target_label(with), present: present(with) })
            }
            (Domain::Task, "opened") => {
                Some(Expectation { text: arg_str(with, "shows")?.to_string(), present: present(with) })
            }
            (Domain::Plugin, "browsed") if !official(with) => {
                Some(Expectation { text: arg_str(with, "source")?.to_string(), present: true })
            }
            (Domain::Plugin, "detail") => {
                Some(Expectation { text: arg_str(with, "declares")?.to_string(), present: true })
            }
            (Domain::Folder, "first-loop") => {
                Some(Expectation { text: arg_str(with, "hands_over")?.to_string(), present: true })
            }
            (Domain::Folder, "ways-in") | (Domain::Folder, "none-linked") => {
                Some(Expectation { text: arg_str(with, "absent")?.to_string(), present: false })
            }
            (Domain::Repo, "ai-launch-notice") => {
                Some(Expectation { text: arg_str(with, "paste_into")?.to_string(), present: present(with) })
            }
            (Domain::Repo, "ai-launch-folder") => {
                Some(Expectation { text: arg_str(with, "dir")?.to_string(), present: true })
            }
            _ => None,
        }
    }

    fn action(&self, domain: Domain, op: &str, with: &Args) -> Result<String, String> {
        Ok(match (domain, op) {
            (Domain::Task, "create") => {
                format!("Create a task titled \"{}\" on the board.", req(with, "title")?)
            }
            // The one premise a reader settles where it is reported: the row that says the creation is
            // still open is the row carrying the button that ends it, so the move is opening the task
            // and pressing it rather than going anywhere else for it.
            (Domain::Task, "finish-creating") => format!(
                "Open the task \"{}\" and press the button that finishes creating it.",
                self.target_label(with)
            ),
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
            // The op names whichever of a task's own fields the step is setting, so the instruction names
            // those and no others: a line that recited the whole form would have the operator wondering
            // what to put in the fields the road never mentioned.
            (Domain::Task, "update") => {
                let set: Vec<String> = ["title", "notes", "due", "start", "priority"]
                    .iter()
                    .filter_map(|k| arg_str(with, k).map(|v| format!("its {k} to \"{v}\"")))
                    .collect();
                if set.is_empty() {
                    return Err("action `update` names no field to set".to_string());
                }
                format!("Open the task \"{}\" and set {}.", self.target_label(with), set.join(", "))
            }
            (Domain::Decision, "create") => {
                format!("Create a decision titled \"{}\".", req(with, "title")?)
            }
            // The same move on the other side. It is written out rather than shared with the task's,
            // because the pane it is made in is the decision's own and the road has to say which screen
            // the operator is standing on.
            (Domain::Decision, "comment") => format!(
                "Open the decision \"{}\" and add the comment \"{}\".",
                self.target_label(with),
                req(with, "text")?
            ),
            // The words go into the box, and what they do to the board is the shot after this one. It is
            // written as a move rather than folded into the assert for the reason the other moves here
            // are: the screen it arrives at is the whole of the road, so it is photographed rather than
            // taken on trust.
            (Domain::Task, "narrow") => format!(
                "On the board, type \"{}\" into the search box over the columns.",
                req(with, "words")?
            ),
            // Onto the face that searches across the records, and through the hit standing on it. The
            // asking is part of the move rather than a step of its own: a hit cannot be pressed before
            // it is drawn, and what the shot after this catches is where the press landed.
            (Domain::Task, "open-hit") => format!(
                "Take the face that searches across every record, search it for \"{}\", and press the ref on the hit for \"{}\".",
                req(with, "words")?,
                self.target_label(with)
            ),
            // The moves that carry the screen from one shot to the next. They read as what to do and
            // not as what to confirm, because that is what they are — the shot they leave behind is
            // the screen after the move, which is how a road across screens is proven walked rather
            // than assumed.
            (Domain::Folder, "open-existing-card") => {
                "Open the card that links this folder to a project this device already holds."
                    .to_string()
            }
            (Domain::Folder, "choose-project") => format!(
                "In that card, choose \"{}\" among the projects this device holds.",
                req(with, "project")?
            ),
            // Two ops the CLI drives as one command apiece, and the screen as one form apiece. They
            // are the domain's own ops and not moves invented for the screen: what a road on screen
            // needs of its own is a card to open, never a project to raise.
            (Domain::Project, "create") => format!(
                "Take the way in that raises a project, name it \"{}\", choose a folder for it, and create it.",
                req(with, "name")?
            ),
            // Moving onto the face a project keeps for itself. It is written as a step rather than left
            // to a note in front of the road, for the reason every other move here is: the screen it
            // arrives at is shot, so a road that reads a row on this face has the arrival on it as
            // evidence rather than as something taken on trust.
            (Domain::Project, "open-settings") => format!(
                "Open the settings the project \"{}\" keeps for itself.",
                req(with, "project")?
            ),
            // Back onto the board, and it says "again" because that is what is under test: what a screen
            // draws on arrival is not what it was holding before the road walked away from it.
            (Domain::Project, "open") => format!(
                "Open the project \"{}\" again, from the list of projects.",
                req(with, "project")?
            ),
            // `dir` is a name and not a path here as it is everywhere else: which folder is linked is
            // the run's to decide, and what the scenario writes down is what to call the one it picked.
            (Domain::Folder, "bind") => format!(
                "In the folder picker that opens, choose a folder for this project — the road calls it \"{}\" — and open it.",
                req(with, "dir")?
            ),
            // The answer given to an offer nobody asked for. It names the button by what it does and
            // not by its wording: the two sit side by side in the same modal, and pressing the other
            // one is exactly the mistake this road exists to catch, so a line a reader could satisfy
            // either way would be no line at all.
            //
            // The refusal is the only answer the driver takes, and the reason is what an acceptance
            // leaves behind: a login registration with the OS, outside the throwaway store and
            // outside anything the run can hand back. That half is walked on real machines.
            (Domain::Store, "nudge-answer") => match req(with, "answer")? {
                "no" => "In the offer standing on screen, press the button that declines it — the one that changes no setting."
                    .to_string(),
                other => {
                    return Err(format!("action `nudge-answer` does not know the answer `{other}`"))
                }
            },
            (Domain::Plugin, "open-entry") => format!(
                "Open the row for \"{}\", the one served by the catalog \"{}\".",
                req(with, "name")?,
                req(with, "source")?
            ),
            // The switch, moved one project at a time — and it lives in that crossing's row, so the
            // picker only draws the row. A crossing that already has a row standing is not offered by
            // the picker again; the instruction covers both by naming where the switch is rather than
            // how the row got there.
            //
            // Neither line names a face. The crossing is the row on both of them and the step is the
            // same move either way — which face the road is standing on is said by the step that
            // carried it there, and repeating it here would be the one place the two could disagree.
            (Domain::Plugin, "enable-in") => format!(
                "Find the row where \"{}\" crosses \"{}\" — drawing it from the picker if the screen has none yet — and turn the plugin on there.",
                req(with, "name")?,
                req(with, "project")?
            ),
            (Domain::Plugin, "disable-in") => format!(
                "Turn \"{}\" off in the row where it crosses \"{}\".",
                req(with, "name")?,
                req(with, "project")?
            ),
            // The picker on its own. What it leaves behind is the row and nothing more, so the
            // instruction stops where the picker does — pressing anything in the row is the next step's,
            // and the shot between them is the evidence that drawing a row is not turning one on.
            (Domain::Plugin, "draw-crossing") => format!(
                "Draw the row where \"{}\" crosses \"{}\" from the picker beside the rows, and press nothing in it.",
                req(with, "name")?,
                req(with, "project")?
            ),
            // The settings, opened from inside the row. Where they are opened from is the whole of this
            // step: a form reached from the crossing needs no project answered, and one reached anywhere
            // else would be asking the question the row has already answered.
            (Domain::Plugin, "open-config-in-row") => format!(
                "In the row where \"{}\" crosses \"{}\", open the settings kept there — from inside that row and nowhere else on the screen.",
                req(with, "name")?,
                req(with, "project")?
            ),
            // A line the reader types, in the form standing open. Saving belongs to the move for the
            // reason it does on a choice: a shot of a filled box nobody committed would be evidence of a
            // value the store never received. An empty value is the clear, and on a form that is the
            // button under the field rather than something to type.
            (Domain::Plugin, "config-set") => {
                let name = req(with, "name")?;
                let key = req(with, "key")?;
                match req(with, "value")? {
                    "" => format!(
                        "In the settings for \"{name}\", press the button under \"{key}\" that empties it."
                    ),
                    value => format!(
                        "In the settings for \"{name}\", type \"{value}\" into \"{key}\" and save."
                    ),
                }
            }
            // Answering a choice on the form it is drawn on. Saving belongs to the move rather than
            // standing as a step of its own: the form holds an answer nobody has committed yet, so a
            // shot of ticked boxes left unsaved would be evidence of an answer the store never
            // received. The button is the one exception — it writes on its own, and there is nothing
            // to save after it.
            (Domain::Plugin, "config-choose") => format!(
                "In the settings for \"{}\", tick \"{}\" among the candidates offered for \"{}\", leave every other one clear, and save.",
                req(with, "name")?,
                req(with, "options")?,
                req(with, "key")?
            ),
            (Domain::Plugin, "config-choose-none") => format!(
                "In the settings for \"{}\", clear every candidate ticked for \"{}\" and save.",
                req(with, "name")?,
                req(with, "key")?
            ),
            (Domain::Plugin, "config-restore-default") => format!(
                "In the settings for \"{}\", press the button under \"{}\" that takes it back to what its author put behind it.",
                req(with, "name")?,
                req(with, "key")?
            ),
            // The one answer the report takes, and the only button on it that writes anything. Closing the
            // report is not among them: it records nothing and is spent the moment the project is opened
            // again, which is a road that ends where it started rather than one this scenario walks.
            (Domain::Repo, "ai-launch-consent") => match req(with, "answer")? {
                "no" => "On the report about this project's folders, press the button that declines having their AI started on amenbo.".to_string(),
                other => {
                    return Err(format!("action `ai-launch-consent` does not know the answer `{other}`"))
                }
            },
            // The button beside it that answers nothing. The instruction names it by what it does rather
            // than by its label, since the label is a word of the interface and the two buttons sit side
            // by side — pressing the wrong one is the mistake this road is written to catch, so the line
            // must not be one a reader can satisfy either way.
            (Domain::Repo, "ai-launch-close") => {
                "On the report about this project's folders, press the button that only takes it off the screen — the one that answers nothing."
                    .to_string()
            }
            // Which of the tools on offer the text is for. Only a folder that points at none is offered
            // more than one, so this is where that road parts from the traced one — and picking a tool
            // the folder shows no trace of is what proves the catalog is standing behind the picker,
            // since the text that follows is that tool's and no other's.
            (Domain::Repo, "ai-launch-pick") => format!(
                "On the report about this project's folders, choose \"{}\" among the tools it offers text for.",
                req(with, "tool")?
            ),
            (Domain::Repo, "ai-launch-copy") => format!(
                "On the report about this project's folders, press the button that takes the text for \"{}\".",
                req(with, "tool")?
            ),
            // Dropping the answer, from the project's own face. It names no answer, since it does not put
            // one: what it leaves behind is the project as it was before it was ever asked, and which
            // answer was there is said by the assert in front of it.
            (Domain::Repo, "ai-launch-consent-clear") => {
                "In this project's own settings, press the button that clears its answer about starting its AI on amenbo."
                    .to_string()
            }
            _ => return Err(unmapped(domain, op)),
        })
    }

    fn assert(&self, domain: Domain, op: &str, with: &Args) -> Result<String, String> {
        Ok(match (domain, op) {
            // A nudge came up by itself, or has gone. Nothing is pressed here and nothing is opened —
            // what is under test is that the offer arrives unasked, on a device that has been used
            // enough for it, so the line is the screen as the app left it.
            (Domain::Store, "nudge") => match present(with) {
                true => format!(
                    "Confirm the offer that came up by itself asks \"{}\" — nothing was pressed to bring it up.",
                    req(with, "shows")?
                ),
                false => format!(
                    "Confirm the offer is off the screen: \"{}\" is nowhere on it.",
                    req(with, "shows")?
                ),
            },
            (Domain::Task, "listed") => format!(
                "Confirm the task \"{}\" is {} the listing filtered by `{}`.",
                self.target_label(with),
                if present(with) { "present in" } else { "absent from" },
                req(with, "filter")?
            ),
            (Domain::Task, "narrowed") => format!(
                "Confirm the card \"{}\" is {} the board the search left.",
                self.target_label(with),
                if present(with) { "still on" } else { "gone from" }
            ),
            // Which record the press opened. Both halves name the phrase rather than the record's title:
            // the title is standing on the hit row as well, so a line read on it would pass over a press
            // that opened nothing, and over one that opened the wrong record just as quietly.
            (Domain::Task, "opened") => match present(with) {
                true => format!(
                    "Confirm the record \"{}\" is standing open beside the hits, showing \"{}\" — words the hits themselves do not carry.",
                    self.target_label(with),
                    req(with, "shows")?
                ),
                false => format!(
                    "Confirm the record standing open is not \"{}\": \"{}\", which only that record's own face carries, is nowhere on the screen.",
                    self.target_label(with),
                    req(with, "shows")?
                ),
            },
            // The asking is inside the confirming, the way `listed`'s filter is: the cross-cutting search
            // answers one question per question put to it, so there is no standing screen for a separate
            // move to arrive at — the words, the narrowing and the reading are one thing a reader does.
            (Domain::Task, "found") | (Domain::Decision, "found") => {
                let side = if domain == Domain::Task { "task" } else { "decision" };
                let mut line = format!("Ask the cross-cutting search for \"{}\"", req(with, "words")?);
                if let Some(kind) = arg_str(with, "kind") {
                    line.push_str(&format!(", narrowed to {kind}"));
                }
                // The other knob, said separately because it is the other axis: the screen carries one
                // for which record and one for which of its faces, and either can be set without giving
                // up the other. A step naming both is asking for the pairing.
                if let Some(face) = arg_str(with, "only_face") {
                    line.push_str(&format!(", narrowed to what is written on a {face}"));
                }
                if let Some(filter) = arg_str(with, "filter") {
                    line.push_str(&format!(", narrowed by `{filter}`"));
                }
                // The scope, and the one narrowing on this screen that is chosen rather than written.
                // It is said as a pull-down because that is the move: a step that read like the box
                // beside it would have the operator typing a project into the grammar the project is
                // deliberately kept out of.
                if let Some(project) = with.get("project").and_then(|v| v.as_str()) {
                    let name = self.labels.get(project).cloned().unwrap_or_else(|| format!("<{project}>"));
                    line.push_str(&format!(", scoped to the project \"{name}\" in the pull-down"));
                }
                line.push_str(&format!(
                    ", and confirm the {side} \"{}\" is {} the hits",
                    self.target_label(with),
                    if present(with) { "among" } else { "not among" }
                ));
                if let Some(face) = arg_str(with, "face") {
                    line.push_str(&format!(", on its {face}"));
                }
                // What the row says about the record it points at, which the eye closing the shot reads
                // off the same line as the ref.
                if let Some(standing) = arg_str(with, "standing") {
                    line.push_str(&format!(", and that the row says it stands at {standing}"));
                }
                // Which of the four places the row calls it. The words are the interface's own, so this
                // is the eye's to close and never the reading's: a match on them would hold the gate to
                // whichever language the run happened to be set up in.
                if let Some(landed) = arg_str(with, "landed_on") {
                    line.push_str(&format!(", and that the row calls the place {}", place(landed)?));
                }
                // Where the words landed inside the excerpt. The eye's for a reason of its own: what is
                // under test is paint, and a reading gives back characters with nothing on them.
                if let Some(marked) = arg_str(with, "marked") {
                    line.push_str(&format!(", with \"{marked}\" marked inside its excerpt"));
                }
                line.push('.');
                line
            }
            // The same box, asked whether it can be used at all. The line stands the screen up itself, as
            // `found` does: what puts the box in this state is a side left unchosen, so a step that only
            // said "confirm" would be read against whichever side the step before it had picked. What it
            // names in the box is the words standing in place of an example, since a box shut and a box
            // merely empty look alike until those are read.
            (Domain::Task, "narrowing-shut") => {
                "On the cross-cutting search, leave the side unchosen — the chips back on the arm that \
                 narrows nothing — and confirm the box that narrows by a side's own grammar is standing \
                 there and cannot be typed into, holding in place of an example the words that say to \
                 choose a side first."
                    .to_string()
            }
            (Domain::Task, "field") => format!(
                "Confirm the task \"{}\" shows {} = {}.",
                self.target_label(with),
                req(with, "field")?,
                show(with.get("equals").ok_or("assert `field` needs `equals`")?)
            ),
            (Domain::Plugin, "browsed") => {
                let name = req(with, "name")?;
                let source = req(with, "source")?;
                match official(with) {
                    true => format!(
                        "Confirm the market's row for \"{name}\", off the catalog \"{source}\", wears the official badge."
                    ),
                    false => format!(
                        "Confirm the market's row for \"{name}\" is badged \"{source}\" — the catalog that served it — and not as official."
                    ),
                }
            }
            (Domain::Plugin, "detail") => format!(
                "Confirm the panel open under \"{}\" — the row the catalog \"{}\" served — says installing it would mean \"{}\".",
                req(with, "name")?,
                req(with, "source")?,
                req(with, "declares")?
            ),
            (Domain::Plugin, "fires-in") => {
                let name = req(with, "name")?;
                let project = req(with, "project")?;
                match present(with) {
                    true => format!(
                        "Confirm \"{name}\" has a row for \"{project}\", and that row says the plugin is on there."
                    ),
                    false => format!(
                        "Confirm \"{name}\" is not on in \"{project}\": either it has no row for that project, or the row it has offers to turn the plugin on rather than off."
                    ),
                }
            }
            // The same crossing as `fires-in`, read where a project's own settings draw it. Three
            // states and not two: what the picker leaves behind is a row with the plugin off in it, and
            // a line that could only say "not on" would read the same over a screen where nothing was
            // added at all.
            (Domain::Project, "plugin-row") => {
                let project = req(with, "project")?;
                let plugin = req(with, "plugin")?;
                match req(with, "state")? {
                    "absent" => format!(
                        "Confirm \"{project}\"'s own settings draw no row for \"{plugin}\": the plugin is only among what the picker there offers to add."
                    ),
                    "drawn" => format!(
                        "Confirm \"{project}\"'s own settings draw a row for \"{plugin}\", and that row offers to turn the plugin on — drawing it turned nothing on."
                    ),
                    "firing" => format!(
                        "Confirm \"{project}\"'s own settings draw a row for \"{plugin}\", and that row says the plugin is on in this project."
                    ),
                    other => {
                        return Err(format!("assert `plugin-row` does not know the state `{other}`"))
                    }
                }
            }
            (Domain::Folder, "none-linked") => format!(
                "Confirm this project's board warns that it has no folder linked, with the one move that ends it — linking a folder — offered in the warning, and the board's own cards still standing under it: \"{}\" is nowhere on the screen.",
                req(with, "absent")?
            ),
            (Domain::Folder, "first-loop") => format!(
                "Confirm the first loop is offered here: its first move opens a terminal already inside the linked folder, its second hands over the request to paste — which names \"{}\" — and its third says the tasks will appear on the board.",
                req(with, "hands_over")?
            ),
            (Domain::Folder, "first-loop-order") => format!(
                "Confirm the screen is arranged in this order: {}.",
                req(with, "order")?
            ),
            (Domain::Folder, "ways-in") => format!(
                "Confirm the screen offers two ways in — raise a project, and open one this device already holds — and that each is carried out here: \"{}\" is nowhere on the screen.",
                req(with, "absent")?
            ),
            (Domain::Folder, "open-existing") => format!(
                "Confirm the open card asks which project to link the folder to — with \"{}\", one of the projects on this device, chosen in it.",
                req(with, "project")?
            ),
            (Domain::Repo, "ai-launch-notice") => match present(with) {
                true => format!(
                    "Confirm the project's board carries the report about starting its folders' AI on amenbo, with nothing asked and nothing over it: \"{}\" is named, with \"{}\" as the file its text goes into.",
                    req(with, "tool")?,
                    req(with, "paste_into")?
                ),
                false => format!(
                    "Confirm this project's board is standing with no such report on it: nothing here names \"{}\", and \"{}\" is nowhere on the screen.",
                    req(with, "tool")?,
                    req(with, "paste_into")?
                ),
            },
            // The record, read on the project's own face. Each state is read together with what the way
            // back out of it is doing, since that is the half a reader acts on: an answer is there to be
            // taken back, and where there is none the button that would take it back must be shut.
            (Domain::Repo, "ai-launch-answer") => match req(with, "answer")? {
                "yes" => "Confirm this project's own settings say it answered yes to having its AI started on amenbo, with the way to clear that answer open.".to_string(),
                "no" => "Confirm this project's own settings say it answered no to having its AI started on amenbo, with the way to clear that answer open — a refusal takes the report away for good, so this is the only way back.".to_string(),
                "unanswered" => "Confirm this project's own settings say it has not been answered for — neither a yes nor a no — and that there is nothing left to clear.".to_string(),
                other => {
                    return Err(format!("assert `ai-launch-answer` does not know the answer `{other}`"))
                }
            },
            // Which folders that one text is still waiting on. The line says "under" on purpose: what is
            // under test is a list standing beneath a single request, so a screen carrying the request
            // once per folder is the miss it catches — and a road naming several folders writes a step
            // apiece, each one read on the same standing screen.
            (Domain::Repo, "ai-launch-folder") => format!(
                "Confirm the folder \"{}\" is listed under \"{}\"'s text, among the folders that text is still waiting on — with the text itself standing once above the list.",
                req(with, "dir")?,
                req(with, "tool")?
            ),
            // The same folder where it is listed whichever notice the board is carrying. What the eye is
            // shown is the inventory itself: the list is on the project's own settings, and the folder is
            // in it under its own heading rather than among the ones bound to the project.
            (Domain::Repo, "ai-launch-waiting") => format!(
                "Confirm this project's own settings list the folder \"{}\" among the ones still starting their AI without amenbo — in that list, not the one of folders bound to the project, and standing there whatever notice the board was carrying.",
                req(with, "dir")?
            ),
            // Which of the three answers the form is holding. The state is the whole question here:
            // the value a screen shows is its ticks, and two of the three answers leave every box
            // clear — so a line naming only the value would pass over the one difference this reads.
            (Domain::Plugin, "config") => {
                let name = req(with, "name")?;
                let key = req(with, "key")?;
                let state = arg_str(with, "state").ok_or_else(|| {
                    "assert `config` on screen needs `state`: a form draws its answer as ticks, and which answer that is cannot be read off them alone".to_string()
                })?;
                match state {
                    "unanswered" => format!(
                        "Confirm the setting \"{key}\" in \"{name}\"'s settings is one nobody has answered: its author's candidates are drawn a box apiece, the ones the author's default names are the ticked ones, and the field is chipped as standing on that default."
                    ),
                    "chosen" => format!(
                        "Confirm the setting \"{key}\" in \"{name}\"'s settings has \"{}\" ticked and every other candidate clear, wearing neither the default's chip nor the declined one's.",
                        req(with, "equals")?
                    ),
                    "none" => format!(
                        "Confirm the setting \"{key}\" in \"{name}\"'s settings has every candidate clear, and is chipped as having declined them all — which is the answer an unanswered field is not."
                    ),
                    other => {
                        return Err(format!("assert `config` on screen does not know the state `{other}`"))
                    }
                }
            }
            // What the crossing's row says about the settings kept there. The mark is read on the row and
            // not on a field, because that is where a person meets it: they are about to press a switch,
            // and what stops them is the row saying the press would be refused.
            (Domain::Plugin, "settings-in") => {
                let name = req(with, "name")?;
                let project = req(with, "project")?;
                match req(with, "state")? {
                    "required-empty" => format!(
                        "Confirm the row where \"{name}\" crosses \"{project}\" is marked as owing a setting the plugin cannot be enabled without."
                    ),
                    "open" => format!(
                        "Confirm the settings for \"{name}\" are standing open inside that same row — the one crossing \"{project}\" — and that nothing in them asks which project they are for."
                    ),
                    "filled" => format!(
                        "Confirm the row where \"{name}\" crosses \"{project}\" says the settings there are filled in, and that nothing in it still says a required one is empty or is keeping the plugin off."
                    ),
                    other => {
                        return Err(format!("assert `settings-in` does not know the state `{other}`"))
                    }
                }
            }
            _ => return Err(unmapped(domain, op)),
        })
    }
}

/// A scenario's screen road as the instructions it renders into — what [`walk`] derives step by
/// step, without the shooting. A road nobody can render is a road nobody can walk, and that answer
/// should not have to wait for a person to be standing in front of a screen.
pub fn instructions(scenario: &Scenario) -> Result<Vec<String>, String> {
    let mut instructor = Instructor::new();
    instructor.learn(&scenario.given);
    scenario.steps(Driver::Gui).iter().map(|s| instructor.render(s)).collect()
}

fn unmapped(domain: Domain, op: &str) -> String {
    format!(
        "op `{op}` for domain `{domain:?}` is in the scenario registry but not yet mapped in the GUI harness"
    )
}

fn arg_str<'a>(with: &'a Args, key: &str) -> Option<&'a str> {
    with.get(key).and_then(|v| v.as_str())
}

/// The words on screen for what an action made — what a later step's `target:` has to be read back
/// as. A record is written under a `title`, and the two things that hold records (a project, a
/// dimension's axis) under a `name`; an action that made neither has no label to offer.
fn label(with: &Args) -> Option<&str> {
    arg_str(with, "title").or_else(|| arg_str(with, "name"))
}

fn req<'a>(with: &'a Args, key: &str) -> Result<&'a str, String> {
    arg_str(with, key).ok_or_else(|| format!("arg `{key}` must be a string"))
}

/// The four places a hit can be on, in words an operator reads off the row. The set is closed and a
/// name outside it is refused here rather than passed through: this is what the reader is asked to
/// confirm, and a line inviting them to look for a phrase the screen has no word for would be closed
/// on nothing.
fn place(landed_on: &str) -> Result<&'static str, String> {
    Ok(match landed_on {
        "task" => "a task",
        "task-comment" => "a comment on a task",
        "decision" => "a decision",
        "decision-comment" => "a comment on a decision",
        other => return Err(format!(
            "`landed_on: {other}` is not one of the four places a hit is on (task / task-comment / decision / decision-comment)"
        )),
    })
}

/// Whether a step says the entry wears the official badge. The op requires the key, so the default
/// is only what an unlinted step falls back to — and it falls back to the half with something to
/// prove, since "not official" is the reading a badge has to earn.
fn official(with: &Args) -> bool {
    with.get("official").and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Whether a step asks for what it names to be on screen, rather than gone from it. Absence is
/// always said out loud, so an unsaid one is a step asking that something is there.
fn present(with: &Args) -> bool {
    with.get("present").and_then(|v| v.as_bool()).unwrap_or(true)
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

/// A step as it is handed over — everything there is to know about it *before* it is carried out.
/// It is the record's own front half ([`StepRecord`]), minus everything only the shot can say.
#[derive(Debug, Clone, Copy)]
pub struct StepBrief<'a> {
    /// Zero-based, as [`StepRecord::index`] is; a reader counting steps adds one.
    pub index: usize,
    /// `action` or `assert` — what is being asked of whoever is standing at the screen.
    pub kind: &'static str,
    /// The step rendered as the sentence an operator reads.
    pub instruction: &'a str,
    /// What OCR will be asked to find on the shot, for an assert that named it. `None` on an action,
    /// and on an assert whose check no reading can settle — the shot goes to a human eye instead.
    pub expected: Option<&'a Expectation>,
}

/// Walk a scenario step by step: capture one screenshot per step into `evidence_dir`, and for an
/// assert OCR can judge, read the shot back and decide `Pass`/`Fail` against the expected text.
/// Every side effect is injected — `capture` and `read_text` shell out to the screen tool,
/// `hand_over` gives the step away before it is taken; a test passes closures that only
/// touch/return fixtures — so the walk is verifiable without a GUI. A capture failure aborts the
/// walk (a missing shot is missing evidence); each judged step's reading is written next to its
/// shot as it came back from the reader, before the fold the match is taken on.
///
/// `hand_over` is what puts somebody at the screen for every shot. It is called with
/// the step about to be taken, **before** anything is captured, and it is called for the first step
/// as well as the rest — so the screen a shot is taken of is one somebody was asked to stand up,
/// and no shot is filed as evidence of a step nobody carried out. A hand-over that fails aborts the
/// walk, since a run nobody is holding would shoot whatever screen was left standing.
pub fn walk<C, O, H>(
    scenario: &Scenario,
    evidence_dir: &Path,
    mut capture: C,
    mut read_text: O,
    mut hand_over: H,
) -> Result<WalkOutcome, String>
where
    C: FnMut(&Path) -> Result<(), String>,
    O: FnMut(&Path) -> Result<Reading, String>,
    H: FnMut(&StepBrief<'_>) -> Result<(), String>,
{
    std::fs::create_dir_all(evidence_dir)
        .map_err(|e| format!("could not create evidence dir {}: {e}", evidence_dir.display()))?;

    let mut instructor = Instructor::new();
    instructor.learn(&scenario.given);
    let mut records = Vec::new();
    let mut passed = true;

    let steps = scenario.steps(Driver::Gui);
    for (i, step) in steps.iter().enumerate() {
        let (kind, domain, op) = match step {
            Step::Action { domain, op, .. } => ("action", *domain, op.clone()),
            Step::Assert { domain, op, .. } => ("assert", *domain, op.clone()),
        };
        let instruction = instructor.render(step)?;
        let expected = instructor.expectation(step);
        let domain = domain_str(domain);
        let screenshot = format!("{:02}-{kind}-{domain}-{op}.png", i + 1);
        let shot_path = evidence_dir.join(&screenshot);

        // Handed over first, shot second. The screen is nobody's until somebody has been asked to
        // stand it up, and a shot taken before that is a photograph of the step before this one.
        hand_over(&StepBrief {
            index: i,
            kind,
            instruction: &instruction,
            expected: expected.as_ref(),
        })
        .map_err(|e| format!("step {}: handing the step over failed: {e}", i + 1))?;

        capture(&shot_path)
            .map_err(|e| format!("step {}: capturing `{screenshot}` failed: {e}", i + 1))?;

        // Judge an assert that named an expectation; keep the reading as evidence.
        let (verdict, found) = match (kind, &expected) {
            ("assert", Some(exp)) => {
                let reading = read_text(&shot_path)
                    .map_err(|e| format!("step {}: reading `{screenshot}` failed: {e}", i + 1))?;
                let hit = reading.text.contains(&fold(&exp.text));
                let _ = std::fs::write(
                    evidence_dir.join(format!("{:02}-{kind}-{domain}-{op}.txt", i + 1)),
                    &reading.raw,
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

        let record = StepRecord {
            index: i,
            kind,
            domain: domain.to_string(),
            op,
            instruction,
            screenshot,
            verdict,
            expected,
            found,
        };
        records.push(record);
    }
    Ok(WalkOutcome { records, passed })
}

fn domain_str(d: Domain) -> &'static str {
    match d {
        Domain::Task => "task",
        Domain::Decision => "decision",
        Domain::Comment => "comment",
        Domain::Project => "project",
        Domain::Dimension => "dimension",
        Domain::Store => "store",
        Domain::Folder => "folder",
        Domain::Attachment => "attachment",
        Domain::Repo => "repo",
        Domain::Plugin => "plugin",
    }
}

/// Write the run's manifest — the scenario, the world it was walked from, the roll-up, and every
/// step's instruction, verdict and evidence — as JSON into the evidence dir, so a later pass (a
/// human closing the `Review`s, or a release gate) reads the checklist and its verdicts back
/// without re-walking the scenario.
///
/// `stood` is what the premise put in the store before the first shot, a line per step. It belongs
/// in the evidence for the reason the shots do: what a screen showed is only as good as the world it
/// was showing, and a reader coming back to a manifest cannot ask the store — it went out with the
/// run.
pub fn write_manifest(
    dir: &Path,
    scenario: &Scenario,
    stood: &[String],
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
    let world: Vec<String> = stood.iter().map(|s| js(s)).collect();
    let json = format!(
        "{{\"scenario\":{},\"title\":{},\"passed\":{},\"world\":[{}],\"steps\":[{}]}}",
        js(&scenario.id),
        js(&scenario.title),
        outcome.passed,
        world.join(","),
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
steps_gui:
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

    /// A reading as the tool hands one back: what the reader returned, and that folded. The fold is
    /// the tool's own in a real run, so a test standing in for it applies the same rule here rather
    /// than writing a folded string by hand that nothing would keep honest.
    fn reading(raw: &str) -> Reading {
        Reading { text: fold(raw), raw: raw.to_string() }
    }

    fn load(yaml: &str) -> Scenario {
        let s = amenbo_scenario::load_str(yaml).expect("parses");
        s.validate().expect("valid");
        s
    }

    #[test]
    fn instructions_read_a_bound_target_by_its_title() {
        let s = load(SCENARIO);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("Create a task titled \"SEED\""));
        assert!(lines[1].contains("\"SEED\"") && lines[1].contains("me-ai"));
        assert!(lines[2].contains("\"SEED\"") && lines[2].contains("present in"));
    }

    #[test]
    fn a_listed_assert_expects_the_bound_title_present() {
        let s = load(SCENARIO);
        let mut ins = Instructor::new();
        for st in s.steps(Driver::Gui) {
            ins.render(st).unwrap();
        }
        let exp = ins.expectation(&s.steps(Driver::Gui)[2]).expect("listed has an expectation");
        assert_eq!(exp, Expectation { text: "SEED".to_string(), present: true });
    }

    #[test]
    fn a_field_assert_is_left_for_review() {
        let yaml = r#"
id: x
title: y
steps_gui:
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
        ins.render(&s.steps(Driver::Gui)[0]).unwrap();
        assert!(ins.expectation(&s.steps(Driver::Gui)[1]).is_none(), "a field assert is not OCR-judged");
    }

    /// The press through a hit: the move names the word asked for and the record pressed, and what the
    /// reading after it is sent looking for is the phrase, never the title. The title stands on the hit
    /// row too, so an expectation derived from it would be satisfied by the screen the press was made
    /// from — which is the failure this road exists to catch.
    #[test]
    fn an_opened_assert_expects_the_phrase_and_not_the_title() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: Retire the nightly reconciliation job }
    as: nightly
  - type: action
    domain: task
    op: open-hit
    with: { words: reconciliation, target: nightly }
  - type: assert
    domain: task
    op: opened
    with: { target: nightly, shows: Closed by hand every Sunday }
  - type: assert
    domain: task
    op: opened
    with: { target: nightly, shows: Untouched since the migration, present: false }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let steps = s.steps(Driver::Gui);
        let lines: Vec<String> = steps.iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(
            lines[1].contains("\"reconciliation\"")
                && lines[1].contains("\"Retire the nightly reconciliation job\""),
            "got: {}",
            lines[1]
        );
        assert!(lines[2].contains("standing open"), "got: {}", lines[2]);
        assert!(lines[3].contains("is not"), "got: {}", lines[3]);

        assert_eq!(
            ins.expectation(&steps[2]).expect("an opened assert is OCR-judged"),
            Expectation { text: "Closed by hand every Sunday".to_string(), present: true }
        );
        assert_eq!(
            ins.expectation(&steps[3]).expect("and so is its absent half"),
            Expectation { text: "Untouched since the migration".to_string(), present: false }
        );
    }

    /// The badge line: an entry off a registered catalog reads as that catalog, and the name is what
    /// OCR is sent looking for — a name the user gave, so it is the same word in any language the
    /// app is in. The official badge is a word of the interface, so that half is left for a `Review`.
    #[test]
    fn a_browsed_assert_expects_the_serving_catalogs_name() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: plugin
    op: browsed
    with: { name: standup, source: In-house catalog, official: false }
  - type: assert
    domain: plugin
    op: browsed
    with: { name: worktree, source: amenbo, official: true }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("\"standup\"") && lines[0].contains("\"In-house catalog\""));
        assert!(lines[1].contains("wears the official badge"), "got: {}", lines[1]);

        let exp = ins.expectation(&s.steps(Driver::Gui)[0]).expect("a not-official row names its shelf");
        assert_eq!(exp, Expectation { text: "In-house catalog".to_string(), present: true });
        assert!(ins.expectation(&s.steps(Driver::Gui)[1]).is_none(), "the official badge is an interface word");
    }

    /// The detail line: opening a row off a registered catalog fetches that catalog's own document,
    /// and what is sent to OCR is the declaration the step named — the author's words, so the reading
    /// does not turn on which language the app is in. Opening the row is the step between the two
    /// readings, and it names the shelf as well as the plugin, since which row was opened is the
    /// whole question the panel answers.
    #[test]
    fn a_detail_assert_expects_the_declaration_it_names() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: action
    domain: plugin
    op: open-entry
    with: { name: standup, source: In-house catalog }
  - type: assert
    domain: plugin
    op: detail
    with: { name: standup, source: In-house catalog, declares: Channel webhook }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("Open the row") && lines[0].contains("\"In-house catalog\""), "got: {}", lines[0]);
        assert!(lines[1].contains("\"standup\"") && lines[1].contains("\"In-house catalog\""), "got: {}", lines[1]);

        let exp = ins.expectation(&s.steps(Driver::Gui)[1]).expect("a detail assert is OCR-judged");
        assert_eq!(exp, Expectation { text: "Channel webhook".to_string(), present: true });
    }

    /// The gate line: the switch is moved a project at a time — in that project's own row — and read
    /// back the same way, including the reading that matters most, the project left firing after
    /// another one's gate was shut. Every line of it is a `Review`: what it names is a project, and the
    /// projects are down the side of the screen whether or not a row names one.
    #[test]
    fn the_gate_moves_one_project_at_a_time_and_is_read_back_by_an_eye() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: action
    domain: plugin
    op: enable-in
    with: { name: worktree, project: Greenhouse }
  - type: assert
    domain: plugin
    op: fires-in
    with: { name: worktree, project: Greenhouse, present: true }
  - type: action
    domain: plugin
    op: disable-in
    with: { name: worktree, project: Greenhouse }
  - type: assert
    domain: plugin
    op: fires-in
    with: { name: worktree, project: Greenhouse, present: false }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("crosses \"Greenhouse\""), "got: {}", lines[0]);
        assert!(lines[1].contains("a row for \"Greenhouse\""), "got: {}", lines[1]);
        assert!(lines[2].contains("off in the row where it crosses \"Greenhouse\""), "got: {}", lines[2]);
        assert!(lines[3].contains("not on in \"Greenhouse\""), "got: {}", lines[3]);

        for (i, st) in s.steps(Driver::Gui).iter().enumerate() {
            assert!(ins.expectation(st).is_none(), "step {i} names a project, which no reading settles");
        }
    }

    /// The same crossing from the project's own face: the road moves onto that screen, draws the row
    /// from the picker there, and reads the three states apart. The middle one is why the state is a
    /// word and not a yes/no — a row standing with the plugin off is what the picker leaves behind, and
    /// a line that could only say "not on" would pass over a screen where nothing had been added.
    #[test]
    fn a_projects_own_face_draws_the_crossing_and_reads_its_three_states() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: action
    domain: project
    op: open-settings
    with: { project: Greenhouse }
  - type: assert
    domain: project
    op: plugin-row
    with: { project: Greenhouse, plugin: worktree, state: absent }
  - type: action
    domain: plugin
    op: draw-crossing
    with: { name: worktree, project: Greenhouse }
  - type: assert
    domain: project
    op: plugin-row
    with: { project: Greenhouse, plugin: worktree, state: drawn }
  - type: action
    domain: plugin
    op: enable-in
    with: { name: worktree, project: Greenhouse }
  - type: assert
    domain: project
    op: plugin-row
    with: { project: Greenhouse, plugin: worktree, state: firing }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("settings the project \"Greenhouse\" keeps"), "got: {}", lines[0]);
        assert!(lines[1].contains("no row for \"worktree\""), "got: {}", lines[1]);
        assert!(lines[2].contains("from the picker") && lines[2].contains("press nothing"), "got: {}", lines[2]);
        assert!(lines[3].contains("offers to turn the plugin on"), "got: {}", lines[3]);
        // The switch is the same op on either face, so its line names the crossing and no screen.
        assert!(lines[4].contains("crosses \"Greenhouse\"") && !lines[4].contains("Under"), "got: {}", lines[4]);
        assert!(lines[5].contains("is on in this project"), "got: {}", lines[5]);

        for (i, st) in s.steps(Driver::Gui).iter().enumerate() {
            assert!(ins.expectation(st).is_none(), "step {i} turns on a button's label, which no reading settles");
        }
    }

    /// A state the face does not have is refused by name rather than rendered as an empty line.
    #[test]
    fn an_unknown_plugin_row_state_is_refused() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: project
    op: plugin-row
    with: { project: Greenhouse, plugin: worktree, state: half-on }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let err = ins.render(&s.steps(Driver::Gui)[0]).expect_err("an unknown state has no instruction");
        assert!(err.contains("half-on"), "got: {err}");
    }

    /// What a hit row draws, which is the half only a screen has: which of the four places it calls
    /// itself, and the run of characters marked inside its excerpt. Both ride the same step as the
    /// reading that finds the row, so the line an eye closes says everything it is being asked at once.
    #[test]
    fn a_hit_row_is_read_for_the_place_it_names_and_the_run_it_marks() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: SEED }
    as: seed
  - type: assert
    domain: task
    op: found
    with: { words: sweep, target: seed, face: comment, landed_on: task-comment, marked: sweep }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let steps = s.steps(Driver::Gui);
        ins.render(&steps[0]).unwrap();
        let line = ins.render(&steps[1]).unwrap();
        assert!(line.contains("on its comment"), "got: {line}");
        assert!(line.contains("calls the place a comment on a task"), "got: {line}");
        assert!(line.ends_with("with \"sweep\" marked inside its excerpt."), "got: {line}");
    }

    /// The control a reader can see, reach and not use. The line has to stand the screen up as well as
    /// read it — the box is shut by the side being unchosen, and the step before this one on a real road
    /// has chosen one — and it has to name what is standing in the box, since shut and empty look alike
    /// until those words are read. It is closed by an eye: neither half leaves anything on a shot that a
    /// reading could match without being held to one language.
    #[test]
    fn the_narrowing_box_is_read_for_refusing_the_hand_while_no_side_is_chosen() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: task
    op: narrowing-shut
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let step = &s.steps(Driver::Gui)[0];
        let line = ins.render(step).unwrap();
        assert!(line.contains("leave the side unchosen"), "got: {line}");
        assert!(line.contains("cannot be typed into"), "got: {line}");
        assert!(line.contains("choose a side first"), "got: {line}");
        assert!(ins.expectation(step).is_none(), "the shut box is closed by an eye, not by a reading");
    }

    #[test]
    fn a_place_outside_the_four_a_hit_can_be_on_is_refused() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: SEED }
    as: seed
  - type: assert
    domain: task
    op: found
    with: { words: sweep, target: seed, landed_on: project }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let steps = s.steps(Driver::Gui);
        ins.render(&steps[0]).unwrap();
        let err = ins.render(&steps[1]).expect_err("a place off the list has no instruction");
        assert!(err.contains("project"), "got: {err}");
    }

    /// The settings form: a choice is answered by ticking and saving, declined by clearing every box,
    /// and taken back by the button under the field. Each of the three answers is read back by its
    /// state, and every one of those readings is a `Review` — the candidates are on the shot whichever
    /// answer is held, and the chip that tells the answers apart is a word of the interface.
    #[test]
    fn a_choice_is_answered_on_the_form_and_read_back_by_which_answer_it_holds() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: plugin
    op: config
    with: { name: worktree, key: events, state: unanswered }
  - type: action
    domain: plugin
    op: config-choose
    with: { name: worktree, key: events, options: "task.done,task.rejected" }
  - type: assert
    domain: plugin
    op: config
    with: { name: worktree, key: events, state: chosen, equals: "task.done,task.rejected" }
  - type: action
    domain: plugin
    op: config-choose-none
    with: { name: worktree, key: events }
  - type: assert
    domain: plugin
    op: config
    with: { name: worktree, key: events, state: "none" }
  - type: action
    domain: plugin
    op: config-restore-default
    with: { name: worktree, key: events }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("nobody has answered") && lines[0].contains("default"), "got: {}", lines[0]);
        assert!(lines[1].contains("tick \"task.done,task.rejected\"") && lines[1].contains("save"), "got: {}", lines[1]);
        assert!(lines[2].contains("\"task.done,task.rejected\" ticked"), "got: {}", lines[2]);
        assert!(lines[3].contains("clear every candidate"), "got: {}", lines[3]);
        assert!(lines[4].contains("declined them all"), "got: {}", lines[4]);
        assert!(lines[5].contains("back to what its author put behind it"), "got: {}", lines[5]);

        for (i, st) in s.steps(Driver::Gui).iter().enumerate() {
            assert!(ins.expectation(st).is_none(), "step {i} is closed by an eye, not by a reading");
        }
    }

    /// The refusal road: the row is marked before the switch is pressed, the press is expected to be
    /// turned away, the settings open inside that same row and ask for no project, the value goes in
    /// there, and the mark gives way. The refused press is the one line whose instruction says so — a
    /// screen has no code to compare, so the word reaches the operator instead of a comparison.
    #[test]
    fn a_refused_switch_is_answered_in_the_row_that_refused_it() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: plugin
    op: settings-in
    with: { name: worktree, project: Greenhouse, state: required-empty }
  - type: action
    domain: plugin
    op: enable-in
    with: { name: worktree, project: Greenhouse, refused: invalid_plugin_settings_required }
  - type: action
    domain: plugin
    op: open-config-in-row
    with: { name: worktree, project: Greenhouse }
  - type: assert
    domain: plugin
    op: settings-in
    with: { name: worktree, project: Greenhouse, state: open }
  - type: action
    domain: plugin
    op: config-set
    with: { name: worktree, key: base, value: main }
  - type: assert
    domain: plugin
    op: settings-in
    with: { name: worktree, project: Greenhouse, state: filled }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("owing a setting the plugin cannot be enabled without"), "got: {}", lines[0]);
        assert!(lines[1].contains("turned away rather than to go through"), "got: {}", lines[1]);
        assert!(lines[2].contains("open the settings kept there"), "got: {}", lines[2]);
        assert!(lines[3].contains("nothing in them asks which project"), "got: {}", lines[3]);
        assert!(lines[4].contains("type \"main\" into \"base\"") && lines[4].contains("save"), "got: {}", lines[4]);
        assert!(lines[5].contains("filled in"), "got: {}", lines[5]);

        for (i, st) in s.steps(Driver::Gui).iter().enumerate() {
            assert!(ins.expectation(st).is_none(), "step {i} is closed by an eye, not by a reading");
        }
    }

    /// A state the row cannot be in is refused by name, the way the row's other reading refuses one.
    #[test]
    fn an_unknown_settings_state_is_refused() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: plugin
    op: settings-in
    with: { name: worktree, project: Greenhouse, state: half-filled }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let err = ins.render(&s.steps(Driver::Gui)[0]).expect_err("an unknown state has no instruction");
        assert!(err.contains("half-filled"), "got: {err}");
    }

    /// An empty value is the clear, and a form clears a field with the button under it rather than with
    /// something typed — so the instruction has to name the button, not an empty pair of quotes.
    #[test]
    fn clearing_a_setting_on_a_form_is_the_button_under_the_field() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: action
    domain: plugin
    op: config-set
    with: { name: worktree, key: base, value: "" }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let line = ins.render(&s.steps(Driver::Gui)[0]).unwrap();
        assert!(line.contains("press the button under \"base\" that empties it"), "got: {line}");
    }

    /// A screen road that says which answer a choice holds without saying which of the three it is has
    /// asked for nothing: two of them leave every box clear. The harness says so rather than rendering
    /// a line an operator would read as a reading of the value.
    #[test]
    fn a_choice_read_back_without_naming_its_answer_is_refused() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: plugin
    op: config
    with: { name: worktree, key: events, equals: task.done }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let err = ins.render(&s.steps(Driver::Gui)[0]).unwrap_err();
        assert!(err.contains("needs `state`"), "got: {err}");
    }

    /// Where the screen has a form for what the CLI does with a command, the road takes the domain's
    /// own op and the harness renders it as that form. Nothing is invented for the screen here — what
    /// a screen road needs of its own is a card to open, never a project to raise.
    #[test]
    fn the_screen_renders_a_domain_op_as_the_form_it_is_carried_out_on() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: action
    domain: project
    op: create
    with: { name: Seedbed }
  - type: action
    domain: folder
    op: bind
    with: { dir: greenhouse }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("\"Seedbed\"") && lines[0].contains("raises a project"), "got: {}", lines[0]);
        assert!(lines[1].contains("\"greenhouse\"") && lines[1].contains("folder picker"), "got: {}", lines[1]);
    }

    /// The first loop: what OCR is sent looking for is the command the handed-over request tells
    /// the reader's AI to run — a command, so the reading does not turn on the app's language.
    /// The order the same screen puts its moves in is not something a reading settles, so that step
    /// is a `Review` and its instruction is what an eye closes it by.
    #[test]
    fn a_first_loop_assert_expects_the_command_its_request_names() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: folder
    op: first-loop
    with: { hands_over: agent --json }
  - type: assert
    domain: folder
    op: first-loop-order
    with: { order: "the first loop, then the other moves, then the way on to the board" }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("\"agent --json\"") && lines[0].contains("linked folder"), "got: {}", lines[0]);
        assert!(lines[1].contains("then the way on to the board"), "got: {}", lines[1]);

        let exp = ins.expectation(&s.steps(Driver::Gui)[0]).expect("the request's command is OCR-judged");
        assert_eq!(exp, Expectation { text: "agent --json".to_string(), present: true });
        assert!(ins.expectation(&s.steps(Driver::Gui)[1]).is_none(), "an order is not something a reading settles");
    }

    /// A title carrying an em dash is what the scenarios are actually written with, and Vision hands
    /// it back as a hyphen. Judged verbatim, such a title can never match however plainly it is on
    /// screen — so the reading and the expectation meet folded.
    #[test]
    fn a_reading_meets_its_expectation_through_the_words_alone() {
        assert_eq!(fold("SCENARIO SEED — handed to me-ai"), "scenario seed handed to me ai");
        assert!(fold("… SCENARIO SEED - handed to me-ai\nAMB-T-1 …")
            .contains(&fold("SCENARIO SEED — handed to me-ai")));
        // The reading Vision hands back for that same card on a Japanese screen: the em dash as a long
        // vowel mark, which is a letter to Unicode and would otherwise be the one glyph left standing.
        assert!(fold("… SCENARIO SEED ー handed to me-ai\nAMB-T-9 …")
            .contains(&fold("SCENARIO SEED — handed to me-ai")));
        // And a word that carries the mark as itself still meets the title it was written into.
        assert!(fold("メニューバーの表示").contains(&fold("メニューバー")));
        // A card that wrapped mid-title reads as two lines, and folds back to the one it was written as.
        assert!(fold("SCENARIO SEED — handed to\nme-ai").contains(&fold("handed to me-ai")));
        // Japanese is words too, so a screen in Japanese is judged by the same rule.
        assert_eq!(fold("入れたあとに設定するもの: Channel webhook (必須)"), "入れたあとに設定するもの channel webhook 必須");
        // What is not there is still not there.
        assert!(!fold("some other card").contains(&fold("SEED")));
    }

    /// The ways in: the judged one is judged by what must **not** be read back — a command is the
    /// same words in any language, so a screen carrying one is the failure. The project the card
    /// asks for is a name the side of every screen carries too, so that one is left for an eye.
    ///
    /// Between the two asserts sit the moves that get from the first screen to the second, written
    /// as steps of their own: each is an instruction to carry out, and the shot each leaves behind
    /// is the screen it arrived at.
    #[test]
    fn a_ways_in_assert_expects_the_command_to_be_absent() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: folder
    op: ways-in
    with: { absent: "bind --project" }
  - type: action
    domain: folder
    op: open-existing-card
  - type: action
    domain: folder
    op: choose-project
    with: { project: Greenhouse }
  - type: assert
    domain: folder
    op: open-existing
    with: { project: Greenhouse }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("\"bind --project\"") && lines[0].contains("two ways in"), "got: {}", lines[0]);
        assert!(lines[1].contains("Open the card") && lines[1].contains("already holds"), "got: {}", lines[1]);
        assert!(lines[2].contains("\"Greenhouse\"") && lines[2].contains("choose"), "got: {}", lines[2]);
        assert!(lines[3].contains("\"Greenhouse\"") && lines[3].contains("which project"), "got: {}", lines[3]);

        let exp = ins.expectation(&s.steps(Driver::Gui)[0]).expect("the command is what must not be read back");
        assert_eq!(exp, Expectation { text: "bind --project".to_string(), present: false });
        assert!(ins.expectation(&s.steps(Driver::Gui)[3]).is_none(), "a name the whole window carries is not a reading");
    }

    /// The board's own warning is read the same way round, and the words it names buy two readings for
    /// one: they are what both of the notices it stands ahead of hand over, so a shot they are missing
    /// from says neither of those is up in its place.
    #[test]
    fn a_none_linked_assert_expects_the_other_notices_command_to_be_absent() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: folder
    op: none-linked
    with: { absent: "agent --json" }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let line = ins.render(&s.steps(Driver::Gui)[0]).unwrap();
        assert!(line.contains("\"agent --json\"") && line.contains("no folder linked"), "got: {line}");

        let exp = ins.expectation(&s.steps(Driver::Gui)[0]).expect("the command is what must not be read back");
        assert_eq!(exp, Expectation { text: "agent --json".to_string(), present: false });
    }

    /// Starting a folder's AI on amenbo, as the screen walks it: the report standing on the board, and
    /// the button that takes a copy of the text. The file it names is the reading — it appears nowhere
    /// else on that board — so a shot taken where the report is not standing is a red and not a shot of
    /// the same words somewhere else.
    #[test]
    fn the_screen_road_that_starts_a_folders_ai_reads_the_file_the_text_goes_into() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: repo
    op: ai-launch-notice
    with: { tool: claude-code, paste_into: .claude/settings.json }
  - type: action
    domain: repo
    op: ai-launch-copy
    with: { tool: claude-code }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("\".claude/settings.json\"") && lines[0].contains("report"), "got: {}", lines[0]);
        assert!(lines[1].contains("\"claude-code\"") && lines[1].contains("takes the text"), "got: {}", lines[1]);

        let notice = ins.expectation(&s.steps(Driver::Gui)[0]).expect("the file is what only the report names");
        assert_eq!(notice, Expectation { text: ".claude/settings.json".to_string(), present: true });
        // The tool's name is written the catalog's way and drawn the reader's way; the fold is what
        // leaves those the same words.
        assert!(fold("This folder looks like Claude Code's.").contains(&fold("claude-code")));
    }

    /// The same road in a folder that names no tool: the pick is what carries the screen from one
    /// tool's text to another's, and the two readings on either side of it are the two files. A picker
    /// that moved its own label and left the text alone leaves both shots reading the first file, which
    /// is the red this pair is written to catch.
    #[test]
    fn picking_a_tool_moves_the_reading_to_that_tools_own_file() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: repo
    op: ai-launch-notice
    with: { tool: claude-code, paste_into: .claude/settings.json }
  - type: action
    domain: repo
    op: ai-launch-pick
    with: { tool: gemini-cli }
  - type: assert
    domain: repo
    op: ai-launch-notice
    with: { tool: gemini-cli, paste_into: .gemini/settings.json }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[1].contains("\"gemini-cli\"") && lines[1].contains("choose"), "got: {}", lines[1]);

        let before = ins.expectation(&s.steps(Driver::Gui)[0]).expect("the file is the reading");
        let after = ins.expectation(&s.steps(Driver::Gui)[2]).expect("the file is the reading");
        assert_eq!(before, Expectation { text: ".claude/settings.json".to_string(), present: true });
        assert_eq!(after, Expectation { text: ".gemini/settings.json".to_string(), present: true });
    }

    /// The report a project keeps standing: one text, and the folders it is still waiting on listed under
    /// it. The folder names are the readings — they are the reader's own words, and the board names no
    /// folder anywhere else — so a report that had shrunk to one folder leaves the second shot red.
    /// A road that points at what the premise stood up reads by the card's own title. The world is
    /// the driver's to stand up and never an instruction — but the operator is sent to a card, and
    /// a binding is a name only the file uses.
    #[test]
    fn a_road_reads_what_the_premise_stood_up_by_its_title() {
        let yaml = r#"
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
  - type: action
    domain: task
    op: assign
    with: { target: seed, assignee: me-ai }
  - type: assert
    domain: task
    op: listed
    with: { filter: "status:todo", target: seed, present: true }
"#;
        let s = load(yaml);
        let lines = instructions(&s).expect("the road renders");
        assert_eq!(lines.len(), s.steps(Driver::Gui).len(), "the premise is no instruction of its own");
        assert!(lines[0].contains("\"SEED\""), "got: {}", lines[0]);

        // And the same name is what the shot is read for — a run judged against `<seed>` would fail
        // every time, on a screen that was right.
        let mut ins = Instructor::new();
        ins.learn(&s.given);
        assert_eq!(
            ins.expectation(&s.steps(Driver::Gui)[1]),
            Some(Expectation { text: "SEED".to_string(), present: true })
        );
    }

    #[test]
    fn the_standing_report_reads_each_folder_the_one_text_is_waiting_on() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: repo
    op: ai-launch-notice
    with: { tool: claude-code, paste_into: .claude/settings.json }
  - type: assert
    domain: repo
    op: ai-launch-folder
    with: { tool: claude-code, dir: frontend }
  - type: assert
    domain: repo
    op: ai-launch-folder
    with: { tool: claude-code, dir: worker }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[1].contains("\"frontend\"") && lines[1].contains("standing once"), "got: {}", lines[1]);
        assert!(lines[2].contains("\"worker\""), "got: {}", lines[2]);

        let first = ins.expectation(&s.steps(Driver::Gui)[1]).expect("the folder's own name is the reading");
        let second = ins.expectation(&s.steps(Driver::Gui)[2]).expect("the folder's own name is the reading");
        assert_eq!(first, Expectation { text: "frontend".to_string(), present: true });
        assert_eq!(second, Expectation { text: "worker".to_string(), present: true });
        // The list carries whole paths and the scenario names the folder alone; the fold leaves the one
        // inside the other.
        assert!(fold("/Users/reader/work/frontend").contains(&fold("frontend")));
    }

    /// The same folder read where it is listed whatever the board is carrying. Here the name settles
    /// nothing — the project's own settings list it a second time, among the folders bound to it — so
    /// the step is an eye's, and the instruction has to say which of the two lists is meant.
    #[test]
    fn the_settings_inventory_names_which_list_the_folder_is_read_in() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: repo
    op: ai-launch-waiting
    with: { dir: frontend }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let line = ins.render(&s.steps(Driver::Gui)[0]).unwrap();
        assert!(line.contains("\"frontend\"") && line.contains("not the one of folders bound"), "got: {line}");
        assert!(
            ins.expectation(&s.steps(Driver::Gui)[0]).is_none(),
            "a name the same screen carries twice is not a reading"
        );
    }

    /// Putting the report aside and finding it again: the two readings are the same file, read one way
    /// and then the other, with the walk off the board and back onto it between them. What they have to tell apart is
    /// the button that answers nothing from the one beside it that answers for good — so the instruction
    /// for the press names what it does rather than what it says, and the last read is the whole claim.
    #[test]
    fn closing_the_report_reads_the_board_without_it_then_the_board_with_it_again() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: repo
    op: ai-launch-notice
    with: { tool: claude-code, paste_into: .claude/settings.json }
  - type: action
    domain: repo
    op: ai-launch-close
  - type: assert
    domain: repo
    op: ai-launch-notice
    with: { tool: claude-code, paste_into: .claude/settings.json, present: false }
  - type: action
    domain: project
    op: open-settings
    with: { project: Greenhouse }
  - type: action
    domain: project
    op: open
    with: { project: Greenhouse }
  - type: assert
    domain: repo
    op: ai-launch-notice
    with: { tool: claude-code, paste_into: .claude/settings.json }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[1].contains("answers nothing"), "got: {}", lines[1]);
        assert!(lines[2].contains("nowhere on the screen"), "got: {}", lines[2]);
        assert!(lines[4].contains("\"Greenhouse\"") && lines[4].contains("again"), "got: {}", lines[4]);

        let gone = ins.expectation(&s.steps(Driver::Gui)[2]).expect("the file is the reading either way");
        assert_eq!(gone, Expectation { text: ".claude/settings.json".to_string(), present: false });
        let back = ins.expectation(&s.steps(Driver::Gui)[5]).expect("and it is the reading on the way back");
        assert_eq!(back, Expectation { text: ".claude/settings.json".to_string(), present: true });
    }

    /// The way back out of a refusal, read where the record is kept: the no standing, the press, and the
    /// project back to never having been answered for. Neither read is a reading — all three answers are
    /// words of the interface — so what the pair has to carry is the difference between them in the
    /// instruction, which is what an eye closes them on.
    #[test]
    fn clearing_the_answer_reads_the_refusal_then_a_project_never_answered_for() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: repo
    op: ai-launch-answer
    with: { answer: "no" }
  - type: action
    domain: repo
    op: ai-launch-consent-clear
  - type: assert
    domain: repo
    op: ai-launch-answer
    with: { answer: unanswered }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("answered no") && lines[0].contains("only way back"), "got: {}", lines[0]);
        assert!(lines[1].contains("clears its answer"), "got: {}", lines[1]);
        assert!(lines[2].contains("not been answered for") && lines[2].contains("nothing left to clear"), "got: {}", lines[2]);
        assert!(
            s.steps(Driver::Gui).iter().all(|st| ins.expectation(st).is_none()),
            "an answer drawn in the interface's own words is not a reading"
        );
    }

    /// An answer the record cannot hold is refused where it is written, the same way a consent answered
    /// with neither yes nor no is.
    #[test]
    fn an_answer_the_record_does_not_hold_is_refused() {
        let step = Step::Assert {
            domain: Domain::Repo,
            op: "ai-launch-answer".to_string(),
            with: [("answer".to_string(), serde_yaml::Value::from("maybe"))].into_iter().collect(),
        };
        let err = Instructor::new().render(&step).unwrap_err();
        assert!(err.contains("maybe"), "got: {err}");
    }

    /// An answer the road does not have is refused where it is written, not carried to a screen as an
    /// instruction nobody can act on — the same way an unmapped op is. A yes is one of them here: the
    /// screen no longer asks, so there is no button on it that gives one.
    #[test]
    fn a_consent_answered_with_anything_but_the_refusal_is_refused() {
        for answer in ["later", "yes"] {
            let step = Step::Action {
                domain: Domain::Repo,
                op: "ai-launch-consent".to_string(),
                with: [("answer".to_string(), serde_yaml::Value::from(answer))].into_iter().collect(),
                bind: None,
            };
            let err = Instructor::new().render(&step).unwrap_err();
            assert!(err.contains(answer), "got: {err}");
        }
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
            |_| Ok(reading("me-ai board\nSEED\nsome other card")),
            |_| Ok(()),
        )
        .expect("walk");

        assert!(outcome.passed, "the listed assert is green when SEED is on screen");
        assert_eq!(*shots.borrow(), s.steps(Driver::Gui).len(), "one shot per step");
        let assert_rec = outcome.records.iter().find(|r| r.kind == "assert").unwrap();
        assert_eq!(assert_rec.verdict, Verdict::Pass);
        assert_eq!(assert_rec.found, Some(true));
        // The reading is kept next to the shot as evidence, as the reader gave it.
        let kept = std::fs::read_to_string(dir.join("03-assert-task-listed.txt")).expect("the reading");
        assert!(kept.contains("me-ai board"), "got: {kept}");

        let stood = ["raised the project `Greenhouse`".to_string()];
        let manifest = write_manifest(&dir, &s, &stood, &outcome).expect("manifest");
        let text = std::fs::read_to_string(&manifest).unwrap();
        assert!(text.contains("\"passed\":true"));
        assert!(text.contains("\"verdict\":\"pass\""));
        // What the shots were taken in front of is written down with them: the store the run stood
        // on is gone by the time anyone reads this back.
        assert!(text.contains("\"world\":[\"raised the project `Greenhouse`\"]"), "got: {text}");
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
            |_| Ok(reading("an empty board with no such card")),
            |_| Ok(()),
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
        let err = walk(&s, &dir, |_| Err("no screen".to_string()), |_| Ok(reading("")), |_| Ok(()))
            .unwrap_err();
        assert!(err.contains("step 1") && err.contains("no screen"), "got: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every step is handed over, the first one included, and each hand-over comes before that
    /// step's shot exists — so nothing is ever captured off a screen nobody was asked to stand up.
    #[test]
    fn every_step_is_handed_over_before_it_is_shot() {
        let s = load(SCENARIO);
        let dir = std::env::temp_dir().join(format!("amenbo-verify-gui-step-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let handed: RefCell<Vec<(usize, &'static str, usize)>> = RefCell::new(Vec::new());
        let shots: RefCell<usize> = RefCell::new(0);
        let outcome = walk(
            &s,
            &dir,
            |p| {
                *shots.borrow_mut() += 1;
                std::fs::write(p, b"fake-png").map_err(|e| e.to_string())
            },
            |_| Ok(reading("me-ai board\nSEED")),
            |b| {
                // The shot count taken at the hand-over says which side of the capture it fell on:
                // step `i` is handed over with `i` shots on disk, never `i + 1`.
                handed.borrow_mut().push((b.index, b.kind, *shots.borrow()));
                assert!(!b.instruction.is_empty(), "a step is handed over as a sentence to carry out");
                Ok(())
            },
        )
        .expect("walk");

        assert_eq!(outcome.records.len(), 3);
        assert_eq!(
            *handed.borrow(),
            vec![(0, "action", 0), (1, "action", 1), (2, "assert", 2)],
            "one hand-over per step, from the first, each before its own shot"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A hand-over that cannot be made is an execution failure, not a nod — and it fails at the
    /// first step, before any shot: were it swallowed, the run would shoot whatever screen was left
    /// standing and file it as evidence of steps nobody carried out.
    #[test]
    fn a_hand_over_failure_aborts_the_walk_before_the_first_shot() {
        let s = load(SCENARIO);
        let dir = std::env::temp_dir().join(format!("amenbo-verify-gui-step-red-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let shots: RefCell<usize> = RefCell::new(0);
        let err = walk(
            &s,
            &dir,
            |p| {
                *shots.borrow_mut() += 1;
                std::fs::write(p, b"fake-png").map_err(|e| e.to_string())
            },
            |_| Ok(reading("")),
            |_| Err("nobody is watching".to_string()),
        )
        .unwrap_err();
        assert!(err.contains("step 1") && err.contains("nobody is watching"), "got: {err}");
        assert_eq!(*shots.borrow(), 0, "nothing was shot");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
