//! amenbo-verify-gui — the mac GUI harness for pre-distribution verification.
//!
//! The same scenario the CLI driver black-box-drives, this harness reads as a **screen
//! checklist**. It bakes in no command line and no pixel: each step becomes a plain-language
//! instruction of what to do or confirm on screen, and every step is shot into an evidence
//! directory by the screen tool (`scripts/screen.swift`), which is named the app's pid — and, where
//! a road says so, the title of the window within it — and hands back a file: the id it shot by
//! never leaves the tool. A window is named by a road only when the app draws more than one, and
//! then it has to be: the tool refuses to pick between two windows, because a shot of the wrong one
//! is evidence of a screen nobody stood at and reads on the manifest exactly like the right one.
//! The pid it is named
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
//! its `find` / `click-named` / `right-click-named` / `click` / `right-click` / `dblclick` / `drag` /
//! `type` / `key` / `scroll` carry out the action steps the checklist names.
//!
//! One step is nobody's to carry out at the screen: `store run-again` ends this run of the app and
//! brings another up on the same store ([`launch::Gui::run_again`]), which is how a road reads what
//! Amenbo keeps of a run against what goes out with one. It is the harness's because the app is —
//! the store it is pointed at and the pid it is shot by are both the run's own.
//!
//! The pure part — turning a step into an instruction and an expectation, and walking a scenario
//! into per-step evidence with a verdict — is separated from the side effects (running the tool,
//! starting the app again) so the walk is testable with injected capture, reading, step-boundary
//! wait and restart.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use amenbo_scenario::{Args, BoundKind, Domain, Driver, Scenario, Step};

/// Starting the app under test and holding it — the pid every shot is aimed at comes from here.
pub mod launch;
/// The line the run stands on: only a bundle the release workflow produced is launched.
pub mod shipped;

/// Write `body` at `path` as a program these tests will run, leaving no descriptor on it here.
///
/// A file written in this process and exec'd a moment later is the classic ETXTBSY race: these
/// tests run beside each other, and a process forked while this one's file was still open for
/// writing carries that descriptor until it execs — which is what makes Linux refuse to exec the
/// file it points at. The refusal lands on whichever test was unlucky, has nothing to do with the
/// change under review, and goes away on a re-run: a red nobody reads, holding up a merge.
///
/// So the writing is handed to a child. The only descriptor on the file lives in a process nothing
/// here can fork from, and by the time the path is exec'd that process is gone — there is no
/// descriptor left to inherit, whatever else the harness is doing at the time. (The CLI driver's
/// MCP stand-in dodges the same race from the other side, by writing no file at all.)
#[cfg(all(test, unix))]
pub(crate) fn stand_in_program(path: &Path, body: &str) {
    use std::io::Write;
    use std::process::Stdio;

    let mut writer = Command::new("/bin/sh")
        .arg("-c")
        .arg(r#"cat > "$0" && chmod 755 "$0""#)
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .expect("a shell to write the stand-in with");
    let mut input = writer.stdin.take().expect("its input");
    input.write_all(body.as_bytes()).expect("the stand-in's body is written");
    drop(input);
    assert!(writer.wait().expect("the writer finishes").success(), "the stand-in is runnable");
}

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

/// Run one of the tool's subcommands and hand back its stdout. A window named by the step is passed
/// on as the tool's own qualifier on the aim — one more thing said about *where*, never an argument
/// of the subcommand.
fn tool(screen: &Path, cmd: &str, args: &[&OsStr], window: Option<&str>) -> Result<Vec<u8>, String> {
    let mut command = Command::new("swift");
    command.arg(screen).arg(cmd).args(args);
    if let Some(window) = window {
        command.arg("--window").arg(window);
    }
    let out = command
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
/// on-screen (one behind another Space does not). The app and not one of its windows: what being
/// frontmost decides is whether any of them can be shot at all.
pub fn front(pid: i64, screen: &Path) -> Result<(), String> {
    tool(screen, "front", &[OsStr::new(&pid.to_string())], None).map(|_| ())
}

/// Shoot the window `window` names into `path` — or the app's one window, when a road named none.
/// The harness names the app by pid and the window by its title, and receives a file: the id the
/// shot was taken by is the tool's and stays there.
///
/// A road that names no window against an app drawing two is refused by the tool rather than
/// answered with whichever was in front. That refusal is the point of naming: a shot of the wrong
/// window is evidence of a screen nobody was standing at, and it reads on the manifest exactly like
/// evidence of the right one.
pub fn shoot(pid: i64, window: Option<&str>, path: &Path, screen: &Path) -> Result<(), String> {
    tool(screen, "shot", &[OsStr::new(&pid.to_string()), path.as_os_str()], window).map(|_| ())
}

/// Read the words off a shot. An error is an execution failure, not a miss: a shot the reader found
/// no text in comes back as an empty [`Reading`], which is the honest answer for an assert that
/// expected words there.
pub fn read_shot(image: &Path, screen: &Path) -> Result<Reading, String> {
    let out = tool(screen, "read", &[image.as_os_str()], None)?;
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

/// Glyph pairs a screen draws alike and a reader has to guess between: the digit `1` against the
/// letter `l`, and the digit `0` against the letter `o`. Vision guesses them wrong on the one face
/// that writes short ASCII words in a monospace font — the key beside a category's name — where
/// `channel` comes back as `channe1` off a shot the key is plainly legible on.
///
/// Each pair is folded onto one of its two members, on the reading and on the expectation both, so
/// the two meet whichever way the guess went. What it costs is the ability to tell `route1` from
/// `routel`: two keys a character apart in this pair are not distinguishable from a photograph, so
/// declaring them the same is the honest answer rather than a widened tolerance. Everything else on
/// the shot keeps its glyph — a lowercase `i` is left out on purpose, since the monospace face this
/// serves draws it with a dot and folding it onto `l` would give away discrimination against a
/// misreading this face does not produce.
const CONFUSED_GLYPHS: [(char, char); 2] = [('1', 'l'), ('0', 'o')];

/// Fold the confusable glyphs onto their representative, so a reading and an expectation that differ
/// only by which member of a pair was guessed meet as the same word.
fn unconfuse(s: &str) -> String {
    s.chars()
        .map(|c| CONFUSED_GLYPHS.iter().find(|(from, _)| *from == c).map_or(c, |(_, to)| *to))
        .collect()
}

/// How short an expectation has to be before a slipped character is no longer forgiven.
///
/// One edit inside eight characters is at most an eighth of what was asked for, and a card's title or
/// an author's label is far longer than that. Under it, one edit is most of the word: `core` and
/// `gore` are a value apart, and a tolerance that cannot tell them apart is one that reads a wrong
/// screen as the right one. The line is on the **expectation**, which is this harness's own text, so
/// which side of it a step falls on is knowable from the scenario rather than from what came back.
const SLIP_FLOOR: usize = 8;

/// Whether a reading holds an expectation, and whether holding it took forgiving a character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Held {
    found: bool,
    /// True only where the words did **not** meet verbatim and one edit brought them together. A
    /// green earned this way is worth saying out loud, so it rides to the manifest rather than being
    /// folded into `found`.
    slipped: bool,
}

/// Match a folded expectation against a folded reading, forgiving the glyphs a screen draws alike
/// ([`CONFUSED_GLYPHS`]) and, on top of that, **one** misread character.
///
/// Vision reads the words on a screen well and the glyphs inside them not always: `day's` came back
/// as `dav's` on a title that was otherwise perfect, and the fold keeps alphanumerics, so `days` and
/// `davs` stay two different words and a verbatim search finds nothing. What is wanted is the
/// reader's own tolerance — a person reading that shot sees the title — held to a budget small enough
/// that a *different* title cannot be read as this one.
///
/// The budget is one character, over the whole expectation, however long it is. Two misreads in one
/// title is not what this is for: that shot goes red, and a person reads it. And it is characters
/// rather than words on purpose — the screen under test is also read in Japanese, where the fold
/// leaves a title with no spaces in it at all, so a tolerance counted in words would be no tolerance
/// there.
///
/// The confusable fold sits outside that budget and outside the floor, because it is not a
/// forgiveness of the reader: `1` and `l` are one drawing, so a key spelt with either is the same key
/// on any shot. That is what carries the short expectations the budget cannot reach — a category's
/// key is a word of five or six characters, well under [`SLIP_FLOOR`].
///
/// **Which way the looseness leans is worth knowing.** On a `present: true` step it can only turn a
/// red green, and on a `present: false` step only a green red — the same tolerance that finds a
/// misread title also finds it when a step says it should be gone. So the risk it carries is a step
/// that fails, never a step that passes on a screen nobody stood up.
fn held(reading: &str, expected: &str) -> Held {
    if expected.is_empty() || reading.contains(expected) {
        return Held { found: true, slipped: false };
    }
    // The confusable pairs go first and are not spent out of the budget below: a glyph the screen
    // draws the same for two characters is not a character the reader got wrong, it is one the shot
    // never told apart. Folding them is what lets a key too short for the budget be read at all.
    let reading = unconfuse(reading);
    let expected = unconfuse(expected);
    if reading.contains(&expected) {
        return Held { found: true, slipped: true };
    }
    let needle: Vec<char> = expected.chars().collect();
    if needle.len() < SLIP_FLOOR {
        return Held { found: false, slipped: false };
    }
    let haystack: Vec<char> = reading.chars().collect();
    Held { found: within_one_edit(&haystack, &needle), slipped: true }
}

/// Whether `needle` stands anywhere inside `haystack` once one character is allowed to be wrong,
/// missing, or extra. It is the ordinary edit distance with a free start and a free end — every place
/// the needle could begin is begun at, and the answer is the cheapest way any of them ends — which is
/// what "somewhere in this reading" means when the reading is a whole screen and the needle is a
/// title on it.
fn within_one_edit(haystack: &[char], needle: &[char]) -> bool {
    // One row of the distance table: `prev[j]` is the cost of the best alignment of the needle's
    // first `j` characters ending at the haystack position just walked past. The first row is zero
    // across, which is the free start; deleting from the needle is what the column costs.
    let mut prev: Vec<usize> = (0..=needle.len()).collect();
    prev[0] = 0;
    let mut best = prev[needle.len()];
    for &h in haystack {
        let mut cur = vec![0usize; needle.len() + 1];
        for (j, &n) in needle.iter().enumerate() {
            let substitute = prev[j] + usize::from(h != n);
            let insert = cur[j] + 1;
            let delete = prev[j + 1] + 1;
            cur[j + 1] = substitute.min(insert).min(delete);
        }
        best = best.min(cur[needle.len()]);
        if best <= 1 {
            return true;
        }
        prev = cur;
    }
    best <= 1
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
    /// Which kind each binding stands for. An instruction has to name the page the operator is to
    /// open, and "the task" is the wrong page for a decision — one axis is set from either.
    kinds: HashMap<String, BoundKind>,
    /// The bindings whose task has **ended** — done or rejected — as of the step being rendered.
    /// The app draws such a title with a line through it wherever it draws it (`card--closed`,
    /// `row__title--closed`), and that line is what takes the title out of OCR's reach, so this is
    /// the whole of what says a step is an eye's to close rather than a reading's.
    ///
    /// It is a state that moves, not a flag that is set once: a world may take a task into a terminal
    /// state and back out again, and the steps on either side of that are judged differently.
    ended: HashSet<String>,
}

impl Instructor {
    fn new() -> Instructor {
        Instructor { labels: HashMap::new(), kinds: HashMap::new(), ended: HashSet::new() }
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
            if let Step::Action { domain, op, with, bind, .. } = step {
                if let Some(name) = bind {
                    if let Some(label) = label(with) {
                        self.labels.insert(name.clone(), label.to_string());
                    }
                    if let Some(kind) = BoundKind::of_domain(*domain) {
                        self.kinds.insert(name.clone(), kind);
                    }
                }
                self.note_end(*domain, op, with);
            }
        }
    }

    /// Follow a task into and out of its terminal states, which is what decides whether the title a
    /// later step names is drawn with a line through it.
    ///
    /// The four ops here are every one the registry has for moving a task across that line, and the
    /// world is where all of it happens today: a premise takes `status` and none of the other three,
    /// and a screen road can end nothing at all, since this harness maps no op that would. It is
    /// walked on the road as well anyway — the rule is that an action noted is an action walked, so
    /// mapping one of them later needs nothing remembered here.
    ///
    fn note_end(&mut self, domain: Domain, op: &str, with: &Args) {
        if domain != Domain::Task {
            return;
        }
        let Some(target) = with.get("target").and_then(|v| v.as_str()) else { return };
        let ended = match op {
            "done" | "reject" => true,
            "reopen" => false,
            // The one op that moves either way, so the state is read off the value rather than off
            // the op: `blocked` and `in_progress` are as much a way back out as `reopen` is.
            "status" => {
                matches!(with.get("status").and_then(|v| v.as_str()), Some("done") | Some("rejected"))
            }
            _ => return,
        };
        if ended {
            self.ended.insert(target.to_string());
        } else {
            self.ended.remove(target);
        }
    }

    /// Whether the title this step names is on screen with a line through it.
    fn struck_through(&self, with: &Args) -> bool {
        with.get("target").and_then(|v| v.as_str()).is_some_and(|name| self.ended.contains(name))
    }

    /// The sentence a step whose title is struck through carries, and nothing at all for the rest.
    /// A `Review` says on the run's roll-up that an eye is owed, but not why one is — and "read this
    /// yourself" without the reason reads as the harness having given up.
    fn struck_note(&self, with: &Args) -> &'static str {
        if self.struck_through(with) {
            " This task has ended, so its title is drawn with a line through it. Vision reads the \
             glyphs under that line as other letters, and no folding of the two sides puts them back \
             together — so this one is closed by an eye on the shot rather than by the reading."
        } else {
            ""
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

    /// What to call the thing a step's `target:` points at — "task" or "decision". They are opened on
    /// different pages, so an instruction that named the wrong one would send the operator to a screen
    /// the step cannot be walked on. Unbound, it falls back to "task", which is what every road that
    /// existed before a decision could be classified was already saying.
    fn target_noun(&self, with: &Args) -> &'static str {
        with.get("target")
            .and_then(|v| v.as_str())
            .and_then(|name| self.kinds.get(name))
            .map(|k| k.noun())
            .unwrap_or("task")
    }

    /// What a step has the operator put into a box: the `words` it wrote, or — where it wrote
    /// `number_of` instead — a number, said by the record that carries it rather than by the number
    /// itself. The store issues that number and these lines are rendered from the YAML alone, so the
    /// operator is sent to read it off the screen; `spelled` is the shape it goes in as.
    fn typed(&self, with: &Args) -> Result<String, String> {
        let Some(name) = with.get("number_of").and_then(|v| v.as_str()) else {
            return Ok(format!("\"{}\"", req(with, "words")?));
        };
        let label = self.labels.get(name).cloned().unwrap_or_else(|| format!("<{name}>"));
        let noun = self.kinds.get(name).map(|k| k.noun()).unwrap_or("task");
        Ok(match arg_str(with, "spelled") {
            Some("hash") => format!("the number of the {noun} \"{label}\" with a `#` in front of it"),
            _ => format!("the number of the {noun} \"{label}\" and nothing else"),
        })
    }

    /// What a step's `mentions` adds to the end of a text it writes: the number of a record, said by
    /// the record rather than by the number, which is what the operator reads off the screen. The store
    /// issues that number and these lines are rendered before any world stands up, so there is nothing
    /// else to name it by. Left out, nothing is added.
    fn mentioning(&self, with: &Args) -> String {
        let Some(name) = with.get("mentions").and_then(|v| v.as_str()) else { return String::new() };
        let label = self.labels.get(name).cloned().unwrap_or_else(|| format!("<{name}>"));
        let noun = self.kinds.get(name).map(|k| k.noun()).unwrap_or("task");
        format!(", followed by the number of the {noun} \"{label}\"")
    }

    /// One step → one instruction. Fails closed on a registry op this harness has not mapped yet
    /// — the same contract the CLI driver keeps, so a new op surfaces loudly here too rather than
    /// walking past with a blank instruction. An action also records the label later steps read by.
    ///
    /// A window the road named is written into the sentence rather than kept beside it, so it
    /// reaches everywhere the sentence does — what `--print` shows while a road is being written,
    /// what the operator is handed mid-run, and what the manifest keeps afterwards — and so an
    /// operator is told which screen to stand at *before* being told what to do there.
    fn render(&mut self, step: &Step) -> Result<String, String> {
        let sentence = self.sentence(step)?;
        Ok(match step.window() {
            Some(window) => format!("In the window called \"{window}\": {sentence}"),
            None => sentence,
        })
    }

    /// The step itself, said without reference to which window it is said of.
    fn sentence(&mut self, step: &Step) -> Result<String, String> {
        match step {
            Step::Action { domain, op, with, bind, .. } => {
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
                    if let Some(kind) = BoundKind::of_domain(*domain) {
                        self.kinds.insert(name.clone(), kind);
                    }
                }
                self.note_end(*domain, op, with);
                Ok(text)
            }
            Step::Assert { domain, op, with, .. } => self.assert(*domain, op, with),
        }
    }

    /// The text an assert step expects on screen, when OCR can judge it. `listed` expects the
    /// bound title present (or absent); a `field` value is not something OCR reads off a card
    /// reliably, so it returns `None` and the step is left for a visual `Review`.
    ///
    /// `narrowed` is judged the same way and on the same text, since the card is what the reading is
    /// about either way. What did the narrowing is still standing on that very shot — the words in the
    /// box, or the values chosen on the axes — so the expectation is the whole title rather than any
    /// word of it: a card that went carries none of its title, and neither the half of a query nor the
    /// name of a value can read as the card that left.
    ///
    /// Both of them stop being OCR's the moment the task has **ended**. A done or rejected title is
    /// drawn with a line through it, and Vision reads the glyphs under that line as other letters —
    /// `SCENARIO — work is over` came back as `SCENARIOwotk is eveF` — so the two sides never meet
    /// however they are folded. What is worth saying is that the `present: false` half is the more
    /// dangerous one: a reading that cannot find a title it is looking at passes an absence step, so
    /// such a line reads green while proving nothing. Both halves go to a `Review`, which is the same
    /// answer this harness gives every assert OCR cannot judge.
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
    /// `line` is the same again, one row earlier: the sentence drawn under a plugin's name is the
    /// author's, in whichever language the catalog published it and the reader is in. It is judged on
    /// the sentence itself and not on "is this translated", since the fallback draws no mark of its
    /// own — a row in English says nothing about why it is in English.
    ///
    /// `asks` is that pair's third: the words a settings form draws a field, or one of a choice's
    /// answers, under. Author's words again, read the same way and for the same reason.
    ///
    /// `checked` reads the same kind of sentence one door earlier — what the author's check said about the
    /// values, over the form or beside the box it named. Their words again, and told from Amenbo's own
    /// sentence in that same place by which of the two is standing there.
    ///
    /// `press-said` is the fourth of that family, one press along: the line an operation left on the
    /// form. It is the author's own sentence, and what a build would draw in its place where the program
    /// said nothing is Amenbo's — so the two are told apart by which of them is standing there, and a
    /// reading settles that.
    ///
    /// Its two neighbours are `Review`s. `press-asks` reads a box that is *empty*, which is what a value
    /// handed to one run and kept nowhere looks like from outside — the words it is asked under are on the
    /// shot whether or not anything was kept in it, so a reading of them would go green over the very
    /// build this is written to catch. `press-shut` is the same shape as `ai-launch-consent-clear-shut`:
    /// what it is about is a button refusing the hand, and paint is not something a reading gives back.
    ///
    /// `body` is the fourth, and the one read in both directions. What an opened panel is read by is
    /// prose somebody wrote — the author's description, or the README off the repository — so a
    /// reading can be held to it either way: found, for the one that should be standing there, and
    /// not found, for the one that must not be standing beside it. It is the whole of what that step
    /// can say, since the panel names neither of them.
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
    /// `plugin fires-on-device` and `plugin settings-on-device` are `Review`s beside them, for the reasons
    /// their crossing-shaped siblings are: what tells an open gate from a shut one is the word on a button,
    /// the marks a row wears are words of the interface, and what `open` turns on is a picker that is *not*
    /// there.
    ///
    /// `plugin layer` is a `Review` for the same reason once more, and on both of its states: the
    /// sentence a declared row carries is a word of the interface, and the state beside it is a row
    /// carrying no sentence — which is an absence, and a reading answers which words are on a shot and
    /// never which are missing from the right part of it.
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
    /// The three `ai-launch` readings are judged on what is not the interface's own words either. The
    /// hand-over is judged on the file the text goes into, which is the one thing on the road that
    /// appears nowhere else on that board, so a shot taken where the report is not standing reads as the
    /// miss it is — and the same word read the other way (`present: false`) is what says the report
    /// went. `ai-launch-request` is the same word read on the project's own settings, where it is again
    /// the only thing that says which tool's text is up; what makes it a road of its own is where it is
    /// read, on a folder wired and nothing being reported. `ai-launch-folder` is judged on the folder's
    /// own name, which the reader gave it and the interface has no word of its own for; the board the
    /// report stands on names no folder anywhere else, so finding one on that shot is finding it in the
    /// list.
    ///
    /// `mcp-app` is judged on the folder its entry names, and only where the step names one: a path is
    /// the reader's own and the fold draws no other, so a row that lost it reads as the miss it is. The
    /// row read without one is a `Review`, and so is `mcp-road` beside it — "set up", "not set up" and
    /// the label on a button are all words of the interface, and which of them is standing is not
    /// something the presence of text can settle.
    ///
    /// `mcp-in-app` is a `Review` further out than either. What settles it is on another program's
    /// screen, and every reading here is taken off a shot of the build under test — so there is no
    /// wording of the expectation that would be read against the right window. The instruction asks the
    /// attending AI for that shot instead, and an eye closes the step from it.
    ///
    /// `ai-launch-answer` is the third of them and a `Review`, for the reason `plugin config`'s state
    /// is: all three of its answers — the yes, the no, and the never asked — are drawn as words of the
    /// interface, and which of them is standing is not something the presence of text can settle.
    ///
    /// `ai-launch-consent-clear-shut` is a `Review` beside it, and for a reason of its own: what it is
    /// about is a button refusing the hand, which puts no words on a shot either way. The one thing that
    /// tells it from a button that can be pressed is how it is drawn, and paint is not something a reading
    /// gives back.
    ///
    /// `ai-launch-waiting` is a `Review` for the reason `open-existing` is: the folder it names is
    /// listed a second time on that same screen, among the ones bound to the project, so a reading of
    /// the name would pass over a build that dropped the inventory and kept the binding.
    ///
    /// `found` is a `Review` on the one reading of it that is about order — a step carrying `first`,
    /// which asks that the record a number names leads the answer rather than sitting anywhere in it.
    /// A reading answers which words are on a shot and never which line they were on, so passing such a
    /// step on its presence would read green off a build that had stopped putting the pin on top. Every
    /// other `found` keeps its expectation.
    ///
    /// `narrowing-shut` is a `Review`, and doubly so. What it is about is a box refusing the hand, which
    /// leaves no text on a shot either way — and what tells a box shut from a box merely empty is the
    /// words standing in it in place of an example, which are the interface's own and would hold this
    /// gate to whichever language the run was set up in.
    ///
    /// `filters-folded` is a `Review` on both of the things it reads. That the values are gone is an
    /// absence, and a reading answers which words are on a shot and never which are missing from the
    /// right part of it; the count beside them is a bare number, and a board draws bare numbers all over
    /// itself — one on every column head — so a reading of it would pass wherever the run was pointed.
    ///
    /// `carded` is read except on the one step that carries `grouping: true`, where it is a `Review`:
    /// the axis named is the one the columns are cut along, so its value is written on a heading over
    /// the very card being read. Both answers put that word on the shot, and what separates them is
    /// which part of the board it came from — which a reading never says.
    ///
    /// `view-warns` is a `Review`. What it names is a bare number, and the sidebar draws bare numbers
    /// down its whole length — one beside every project — so a reading of it would pass wherever the
    /// run was pointed. What tells this one from those is the colour it is drawn in, and a reading
    /// gives back characters with nothing on them. The zero side is worse still: it is an absence, and
    /// a reading answers which words are on a shot, never which are missing from the right part of it.
    ///
    /// `nudge` is a `Review`, and the sentence it names is why: an offer is put in the interface's own
    /// words, so a reading of it would hold this gate to the one language the run happened to be set up
    /// in. What the step names is written down all the same — it is what the eye closing the shot is
    /// looking for. The road has a second reader besides that one: an offer that never came up is an
    /// offer nobody can decline, so the step after it cannot be carried out at all.
    ///
    /// `tick banner` is a `Review` for the reason `nudge` is: the band puts its offer in the
    /// interface's own words, so a reading of them would hold this gate to the one language the run
    /// was set up in — and its `present: false` half is an absence, which a reading answers nothing
    /// about. It keeps `nudge`'s second reader too: a band that never came up is a band nobody can
    /// answer, so the step after it cannot be carried out at all.
    ///
    /// `tick setting` is a `Review` for the reason `plugin config`'s state is: the two positions the
    /// row can stand in are drawn as words of the interface, and which of them is standing is not
    /// something the presence of text can settle.
    ///
    /// `terminal dot` is a `Review`, and one that is not about words at all. What it reads is a mark
    /// with no text on it, and what tells its three faces apart is a glow, a colour and a blink. The
    /// blink is the one that settles it: at either end of its turn it rests where a still lamp holds,
    /// so a picture of the two can be the same picture. The eye that closes it is therefore watching
    /// the screen rather than the shot, and the instruction says so and says for how long. The shot is
    /// still kept, for the half of the row a picture does carry — which pane the mark belongs to.
    ///
    /// `files row-mark` is a `Review`, and the plainest one here: what it reads is a colour, and a
    /// reading answers with words. The row wearing one says the same letters as the row beside it that
    /// wears none, so no folding of the two sides can tell them apart. The instruction therefore names
    /// the state git is in rather than the colour it is drawn in — which colour that is belongs to the
    /// theme, and an eye at the screen can see that two rows differ without being told what to expect.
    ///
    /// `files handed-over` is a `Review` on all three of its doors, and further out than most: what
    /// settles it is not on Amenbo's window at all. A file handed to the machine leaves through an
    /// application that came forward, or an operating system's own chooser drawn by the system, and
    /// the run shoots the window under test. The eye that closes it is the operator's at the moment
    /// they pressed the item, which is why each of the three lines asks them to say what they saw.
    ///
    /// `terminal frames` is a `Review` for a reason close to the dot's: what it reads is a count of
    /// boxes, and a box on this face is a box whether it holds a terminal or a question. Nothing on
    /// it is the road's own words — the panes have not been typed into yet, and the empty ones this
    /// step exists to rule out would carry the interface's — so there is nothing for a reading to
    /// look for, and its absence would settle nothing either way. Its neighbour `asking-folder` is
    /// read, and the two are worth having side by side: one says what is standing, and the other
    /// says how many.
    ///
    /// `terminal side` and `terminal side-width` are `Review`s, and for the reason `frames` is: what
    /// they read is a region of the screen rather than anything written in one. Every word a column
    /// carries is drawn elsewhere on the same face — a pane's name is on the row above the pane as
    /// well as on the list, and the file panel's two halves are named on the top row whether the
    /// panel is up or not — so a reading finds those words on the shot either way and settles
    /// nothing. `side-width` is further out still: what it asks is where an edge stands compared with
    /// the shot before it, which is two pictures rather than one.
    ///
    /// `dimension key` is read, and it is the cleanest reading on these roads. A key is neither a word
    /// of the interface nor a title drawn twice over — it is what a reader types for somewhere outside
    /// Amenbo — so it stands on the shot in the one field it was typed into, and nowhere else.
    ///
    /// `dimension listed` is read for nearly that reason: what its control on a pane is labelled with is
    /// the category's own name, which a reader gave it. The shot is the record's pane and nothing else,
    /// so the name is on it only where the control is — which is what makes the `present: false` half
    /// worth reading rather than an absence nobody can answer for.
    ///
    /// `decision field` is a `Review` for the reason the task's own is: what a pane says of a state is
    /// a word of the interface's, so an eye closes it.
    fn expectation(&self, step: &Step) -> Option<Expectation> {
        let Step::Assert { domain, op, with, .. } = step else { return None };
        match (*domain, op.as_str()) {
            // The two that read a task's own title off a card or a row, where a title that has ended
            // is drawn through. Nothing derived from it can be matched, in either direction: a reading
            // of a struck title misses the words that are there, and the same miss on a `present:
            // false` step is a pass nobody earned. A hit row is not one of these — a search draws its
            // titles plain, ended or not — so `found` keeps its expectation.
            (Domain::Task, "listed") | (Domain::Task, "narrowed") | (Domain::Task, "view-lists")
                if self.struck_through(with) =>
            {
                None
            }
            // Where a hit stands in the answer is not something a reading gives back — it answers
            // which words are on the shot, and every one of them is on it whichever line it took. A
            // step asking for the top is left for an eye rather than passed on its presence, which
            // would read green off a build that had stopped pinning anything.
            (Domain::Task, "found") | (Domain::Decision, "found") if first(with) => None,
            (Domain::Task, "listed")
            | (Domain::Task, "narrowed")
            | (Domain::Task, "view-lists")
            | (Domain::Task, "found")
            | (Domain::Decision, "narrowed")
            | (Domain::Decision, "found") => {
                Some(Expectation { text: self.target_label(with), present: present(with) })
            }
            (Domain::Task, "opened") => {
                Some(Expectation { text: arg_str(with, "shows")?.to_string(), present: present(with) })
            }
            // The pane's own name, on the row the task draws for it. It is the line a road typed into
            // that pane, so it is the road's own words and no part of the interface — which is what
            // lets the absent half be read: a task with no such row has those words nowhere on it.
            (Domain::Task, "pane") => {
                Some(Expectation { text: arg_str(with, "shows")?.to_string(), present: present(with) })
            }
            // The category's own name, which is what its control on the pane is labelled with. The
            // reader gave that name, so it is not a word of the interface's — and on the `present:
            // false` side its absence from the pane is the whole of the claim.
            //
            // Except where the step names a value: what is being asked then is what the control offers,
            // and the answers are inside a list a reader opens. A shot of the pane holds the field and
            // not the list, so the word would be missing on the `present: true` side as often as it is
            // there — an eye closes that one.
            (Domain::Dimension, "listed") if arg_str(with, "value").is_none() => {
                Some(Expectation { text: arg_str(with, "dimension")?.to_string(), present: present(with) })
            }
            // The value on the column heading, which is a word the reader gave. A column that is not
            // drawn holds nothing that would carry the name elsewhere on the board — the cards under it
            // went with it, and a card draws a category only where the axis is marked for it.
            (Domain::Project, "column") => {
                Some(Expectation { text: arg_str(with, "value")?.to_string(), present: present(with) })
            }
            // The value, not the card's title: what is being asked is whether the classification is
            // drawn, and a title is on the board either way. Which card carries it is the driver's to
            // see — the instruction names it — so the reading answers the half a reading can.
            //
            // Except where the axis is the one splitting the columns (`grouping: true`), which is a
            // `Review`: the value is written on the column heading whether or not the card repeats it,
            // so a reading finds the word on every shot of that board and settles nothing.
            (Domain::Task, "carded") if !grouping(with) => {
                Some(Expectation { text: arg_str(with, "value")?.to_string(), present: present(with) })
            }
            (Domain::Plugin, "browsed") if !official(with) => {
                Some(Expectation { text: arg_str(with, "source")?.to_string(), present: true })
            }
            (Domain::Plugin, "line") => {
                Some(Expectation { text: arg_str(with, "desc")?.to_string(), present: true })
            }
            // `present: false` reads the same words for their absence: a setting a condition
            // does not let through is not drawn greyed or drawn empty, so what proves the condition held
            // is that its label is nowhere on the screen.
            (Domain::Plugin, "asks") | (Domain::Plugin, "offers") => {
                Some(Expectation { text: arg_str(with, "label")?.to_string(), present: present(with) })
            }
            (Domain::Plugin, "checked") | (Domain::Plugin, "press-said") => {
                Some(Expectation { text: arg_str(with, "text")?.to_string(), present: true })
            }
            // A part is read by its own string, where it has one on the shot: the words on a `link`'s
            // button, the line a `text` is, the address beside a `copy`. They are the author's, which is
            // what makes them worth reading back — Amenbo wrote none of them.
            //
            // A `qr` is not one of those. What is on the screen is a picture, and the claim is about who
            // drew it, so that one is left to an eye.
            (Domain::Plugin, "drawn") if arg_str(with, "kind")? != "qr" => {
                Some(Expectation { text: arg_str(with, "value")?.to_string(), present: true })
            }
            (Domain::Plugin, "detail") => {
                Some(Expectation { text: arg_str(with, "declares")?.to_string(), present: true })
            }
            (Domain::Plugin, "body") => {
                Some(Expectation { text: arg_str(with, "text")?.to_string(), present: present(with) })
            }
            (Domain::Folder, "first-loop") => {
                Some(Expectation { text: arg_str(with, "hands_over")?.to_string(), present: true })
            }
            (Domain::Folder, "ways-in") | (Domain::Folder, "none-linked") => {
                Some(Expectation { text: arg_str(with, "absent")?.to_string(), present: false })
            }
            // The store the folder belongs to, read off the row that lists it. It is the one word on
            // that row a reader can act on — where the terminal names it in a refusal, the screen has
            // no refusal to name it in — and it is a store's name rather than a word of the
            // interface's, so a reading finds it on this row alone whatever language the screen is in.
            (Domain::Folder, "claimed") => {
                Some(Expectation { text: arg_str(with, "store")?.to_string(), present: true })
            }
            (Domain::Repo, "ai-launch-notice") | (Domain::Repo, "ai-launch-request") => {
                Some(Expectation { text: arg_str(with, "paste_into")?.to_string(), present: present(with) })
            }
            (Domain::Repo, "ai-launch-folder") => {
                Some(Expectation { text: arg_str(with, "dir")?.to_string(), present: true })
            }
            // The folder an app's entry names, where the step names one. It is the reader's own path and
            // the fold draws no other, so a row that had lost it reads as the miss it is — while the
            // words on either side of it ("set up", "not set up") are the interface's own and settle
            // nothing.
            (Domain::Repo, "mcp-app") => {
                Some(Expectation { text: arg_str(with, "dir")?.to_string(), present: true })
            }
            // The key held in a field, judged by reading it. What is being looked for is a word nobody
            // else on that screen carries — not a name, not a label, but the key a reader typed for use
            // outside Amenbo — so a reading that finds it found it where it was typed. A refusal is read
            // the same way, on the key that was there before: the field is put back rather than left
            // holding a key nothing was saved under, so what says the guard bit is the old key standing.
            (Domain::Dimension, "key") => {
                Some(Expectation { text: arg_str(with, "equals")?.to_string(), present: true })
            }
            // The line a road typed into a terminal, followed from one window to the other. It is the
            // reader's own words and no part of the interface, so a reading finds it on the pane
            // drawing that session and on no other screen — which is what lets the absent half be
            // read as well: with the ledger up, the pane is hidden, and words that are hidden are
            // words that are not on the shot.
            (Domain::Terminal, "pane")
            | (Domain::Terminal, "label")
            // What is standing in the input line is on the same screen as what a program printed:
            // one shot, one reading, and the sentence is where the difference between them lives.
            | (Domain::Terminal, "in-the-box") => {
                Some(Expectation { text: arg_str(with, "shows")?.to_string(), present: present(with) })
            }
            // The folder the question offers, read by the name the road gave it. The buttons carry the
            // folders' own paths, and the last part of one is the word the world was told to make it
            // under — so a reading finds it on the question and nowhere else on this face, and finds
            // it gone once the question has been left.
            (Domain::Terminal, "asking-folder") => {
                Some(Expectation { text: arg_str(with, "dir")?.to_string(), present: present(with) })
            }
            // A row on the file face, and the words an opened file draws. Both are read off the shot
            // as the road wrote them: a file's name is a name the road gave it, and what is inside is
            // what the road put there.
            (Domain::Files, "listed") => {
                Some(Expectation { text: arg_str(with, "name")?.to_string(), present: present(with) })
            }
            // The encoding the row names. It is drawn as words and read as words, and the fold takes
            // the punctuation with it — `Shift_JIS` and `UTF-8` come back as their letters and digits
            // either way, which is the whole of what is being compared.
            (Domain::Files, "read-as") => {
                Some(Expectation { text: arg_str(with, "encoding")?.to_string(), present: present(with) })
            }
            // A form named takes this away from the reading: both forms carry the same words, and
            // what separates them is punctuation the fold throws away and a size no reading reports.
            (Domain::Files, "reading") if picture(with) => {
                Some(Expectation { text: arg_str(with, "shows")?.to_string(), present: present(with) })
            }
            (Domain::Files, "reading") if with.contains_key("as") => None,
            (Domain::Files, "reading") => {
                Some(Expectation { text: arg_str(with, "shows")?.to_string(), present: present(with) })
            }
            // The axis's own name, which is what its button in that row is drawn with. Nothing else on
            // a board opened plain carries it — a card draws values and not the axis they are on, and
            // the filter chips are behind a control this road does not press — so the name being
            // nowhere is the picker not offering it.
            (Domain::Project, "groupable") => {
                Some(Expectation { text: arg_str(with, "axis")?.to_string(), present: present(with) })
            }
            _ => None,
        }
    }

    fn action(&self, domain: Domain, op: &str, with: &Args) -> Result<String, String> {
        Ok(match (domain, op) {
            // Classifying at creation is a flag, and this face has no form field for it: the board's
            // new-card control takes a title. A road that means to walk it belongs on the terminal's
            // side, so this fails closed rather than quietly instructing a reader to file the task and
            // classify it afterwards — which is a different road, and one `classify-work-along-an-axis`
            // already walks.
            (Domain::Task, "create") if with.contains_key("dimension") => {
                return Err(
                    "the board's create takes a title alone — classifying at creation is the terminal's road"
                        .to_string(),
                )
            }
            (Domain::Task, "create") => {
                format!("Create a task titled \"{}\" on the board.", req(with, "title")?)
            }
            // The one premise a reader settles where it is reported: the row that says the creation is
            // still open is the row carrying the button that ends it, so the move is opening the task
            // and pressing it rather than going anywhere else for it.
            // A creation this face will not let end is held rather than refused: the button is shut and
            // the axes still to answer are named beside it, so a line telling a reader to press it would
            // have them hunting for a press that was never on offer.
            (Domain::Task, "finish-creating") if with.contains_key("refused") => format!(
                "Open the task \"{}\" and find the button that finishes creating it shut, with the categories still to answer named beside it.",
                self.target_label(with)
            ),
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
                "Open the task \"{}\" and add the comment \"{}\"{}.",
                self.target_label(with),
                req(with, "text")?,
                self.mentioning(with)
            ),
            // The op names whichever of a task's own fields the step is setting, so the instruction names
            // those and no others: a line that recited the whole form would have the operator wondering
            // what to put in the fields the road never mentioned.
            (Domain::Task, "update") => {
                // The number a `mentions` asks for lands in the two fields that carry text, and the
                // loader has already held the step to naming one of them.
                let mentioned = self.mentioning(with);
                let set: Vec<String> = ["title", "notes", "due", "start", "priority"]
                    .iter()
                    .filter_map(|k| {
                        arg_str(with, k).map(|v| match *k {
                            "title" | "notes" => format!("its {k} to \"{v}\"{mentioned}"),
                            _ => format!("its {k} to \"{v}\""),
                        })
                    })
                    .collect();
                if set.is_empty() {
                    return Err("action `update` names no field to set".to_string());
                }
                format!("Open the task \"{}\" and set {}.", self.target_label(with), set.join(", "))
            }
            // The other half of writing a day: taking it back off. It is a move of its own on this face
            // and not an empty value handed to the same form — an emptied picker draws a day of its own,
            // so the screen keeps a button beside each day for the taking-off, and that button is what a
            // reader who wants the day gone has to find. The field is named by the CLI's own word for it,
            // the pair the two roads share, and the two days are the whole of what this face can take
            // back: a step naming anything else is turned away here rather than sending an operator to
            // look for a button the screen never draws.
            (Domain::Task, "clear") => {
                let day = match req(with, "field")? {
                    "due" => "due date",
                    "start" => "start date",
                    other => {
                        return Err(format!(
                            "action `clear` names `{other}`, and the screen offers no way to take that back"
                        ))
                    }
                };
                format!(
                    "Open the task \"{}\" and press the button beside its {day} that takes the day off.",
                    self.target_label(with)
                )
            }
            // Hanging a file on a record. Where it goes is the whole of the instruction, because the
            // screen keeps two ways in and they are not the same place: a record's own attachments have
            // a section of their own on its pane, and a remark's fold into the button under it.
            (Domain::Task, "attach") => format!(
                "Open the task \"{}\" and attach a file named \"{}\" to it, from the attachments section on its pane.",
                self.target_label(with),
                file_named(with)?
            ),
            // The form's own selects, answered before the record goes in. It is one move rather than
            // two: what is chosen is written with the decision, so the line sends a reader to the form
            // and not to the pane afterwards.
            (Domain::Decision, "create") if with.contains_key("dimension") => format!(
                "Begin recording a decision titled \"{}\", choose \"{}\" for the category \"{}\" in the same form, and record it.",
                req(with, "title")?,
                req(with, "value")?,
                req(with, "dimension")?
            ),
            // The form the demand bites at, where a project will not have a decision left blank on an
            // axis. The task side's twin is the button that ends a creation: both are controls the
            // screen holds shut, so what the reader is sent to find is a shut control and the axes
            // named beside it — not a refusal to press for.
            (Domain::Decision, "create") if with.contains_key("refused") => format!(
                "Begin recording a decision titled \"{}\" and find the button that records it shut, with the categories still to answer named beside it.",
                req(with, "title")?
            ),
            (Domain::Decision, "create") => {
                format!("Create a decision titled \"{}\".", req(with, "title")?)
            }
            // Settling a decision, from its own pane. Unlike the creation the task pane holds shut, this
            // button is live and the refusal comes back from the press — so the line sends a reader to
            // press it, and what the road reads is the sentence that comes back and the decision left
            // where it was. A line that had them hunting for a shut button would describe a screen that
            // is not there.
            (Domain::Decision, "accept") if with.contains_key("refused") => format!(
                "Open the decision \"{}\", press the button that settles it, and confirm. The pane refuses the confirmation and names the categories still to answer, in the box the confirmation was made in.",
                self.target_label(with)
            ),
            (Domain::Decision, "accept") => format!(
                "Open the decision \"{}\", press the button that settles it, and confirm.",
                self.target_label(with)
            ),
            // The same move on the other side. It is written out rather than shared with the task's,
            // because the pane it is made in is the decision's own and the road has to say which screen
            // the operator is standing on.
            (Domain::Decision, "comment") => format!(
                "Open the decision \"{}\" and add the comment \"{}\".",
                self.target_label(with),
                req(with, "text")?
            ),
            // The same bytes hung one level down, and the reason the two moves are written apart: a
            // remark carries no section, so its way in is the button standing under it. The remark is
            // named by what it says, which is what an operator has to read it off the timeline by —
            // nothing else on screen tells one remark from another.
            (Domain::Comment, "attach") => format!(
                "Find the comment \"{}\" and attach a file named \"{}\" to it, with the button for that under the remark itself.",
                self.target_label(with),
                file_named(with)?
            ),
            // The words go into the box, and what they do to the board is the shot after this one. It is
            // written as a move rather than folded into the assert for the reason the other moves here
            // are: the screen it arrives at is the whole of the road, so it is photographed rather than
            // taken on trust.
            (Domain::Task, "narrow") => format!(
                "On the board, type {} into the search box over the columns.",
                self.typed(with)?
            ),
            // The same box on the decisions tab. It is written out rather than shared with the board's,
            // for the reason that tab's other moves are: the box sits over the decisions and not over the
            // columns, and a line that did not say which of the two an operator is standing at could be
            // walked on either.
            (Domain::Decision, "narrow") => format!(
                "On the decisions tab, type {} into the search box over the rows.",
                self.typed(with)?
            ),
            // The other narrowing on that board, opened and folded from the one control it has. The line
            // names that control by what it does rather than by its wording, the way every button here is
            // named: it is the only thing above the board that speaks about narrowing, and while the
            // values are folded away it is also the only thing that says any narrowing is on.
            (Domain::Task, "open-filters") => {
                "On the board, open the values to narrow by, from the control above the columns that says how many axes are narrowing."
                    .to_string()
            }
            (Domain::Task, "close-filters") => {
                "Fold the values away again from that same control, so the tasks have back the room they were taking."
                    .to_string()
            }
            // One press on one axis. The pair is named as the CLI writes it and not as the chips read,
            // because the chips read in whichever language the app was started in — the grammar is the
            // one name the screen and the terminal share, and an operator standing in front of the axis
            // can see which value it names. What is already chosen there is said out loud: each value is
            // a switch, so a press that cleared its neighbours would be a different move.
            (Domain::Task, "choose-filter") => format!(
                "In the values now open, press the one the CLI writes as `{}`, and leave whatever is already chosen on that axis chosen.",
                filter_pair(req(with, "axis")?, req(with, "value")?)
            ),
            // The same three moves on the decisions tab. They are written out rather than shared with the
            // board's, for the reason the decision's own comment is (above): the control sits over the
            // decisions and not over the columns, and a road that did not say which of the two tabs the
            // operator is standing on could be walked on either. What the lines ask for is the same act —
            // the panel is one panel — so the wording is kept parallel and only the screen differs.
            (Domain::Decision, "open-filters") => {
                "On the decisions tab, open the values to narrow by, from the control beside the search box that says how many axes are narrowing."
                    .to_string()
            }
            (Domain::Decision, "close-filters") => {
                "Fold the values away again from that same control, so the decisions have back the room they were taking."
                    .to_string()
            }
            (Domain::Decision, "choose-filter") => format!(
                "In the values now open, press the one the CLI writes as `{}`, and leave whatever is already chosen on that axis chosen.",
                filter_pair(req(with, "axis")?, req(with, "value")?)
            ),
            // Onto the face that searches across the records, and through the hit standing on it. The
            // asking is part of the move rather than a step of its own: a hit cannot be pressed before
            // it is drawn, and what the shot after this catches is where the press landed.
            (Domain::Task, "open-hit") => format!(
                "Take the face that searches across every record, search it for {}, and press the ref on the hit for \"{}\".",
                self.typed(with)?,
                self.target_label(with)
            ),
            // Onto a smart view. The row is named by what it stands for and not by its label: the
            // sidebar is drawn in whichever language the app was started in, so a line written on the
            // wording would send the operator looking for a word this run never puts on screen.
            (Domain::Task, "open-view") => {
                format!("In the sidebar, press {}.", view_row(req(with, "view")?)?)
            }
            // The card carried across the board and let go in a column, which is how work is filed
            // where it is standing. The column is named by the value on its heading — a word the reader
            // gave — and the axis beside it says which cut of the board the operator is looking at, so a
            // line is walkable on the board it was written for and on no other.
            //
            // The half a closed value turns away is written out rather than left to the sentence every
            // refused step ends with, for the reason the held creation is: nothing comes back and no
            // sentence is shown, the column simply not taking the card — so an operator told only to
            // expect a refusal would be watching for something that never appears.
            (Domain::Task, "drop-into-column") if with.contains_key("refused") => format!(
                "On the board cut along \"{}\", carry the card \"{}\" over the column headed \"{}\" and let it go. The column takes no card — it is nowhere a drop can land — so the card stays in the column it came from, with nothing said about it.",
                req(with, "axis")?,
                self.target_label(with),
                req(with, "value")?
            ),
            (Domain::Task, "drop-into-column") => format!(
                "On the board cut along \"{}\", drag the card \"{}\" into the column headed \"{}\" and let it go there.",
                req(with, "axis")?,
                self.target_label(with),
                req(with, "value")?
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
            // The first loop's one press. It is named by what it does rather than by what is written
            // on it, the way every other control on these roads is, and the folder it opens in is
            // deliberately not said: the card names that folder above the press, so an operator told
            // which one to expect could not tell a card that named the wrong one from a card that
            // named the right one.
            //
            // What follows the press is said, because it is the press landing: the screen goes to the
            // terminal by itself and a pane is already open there. An operator left to find the
            // terminal for themselves would walk a road that passed whether or not the press did
            // anything at all.
            (Domain::Folder, "start-terminal") => {
                "In the first loop, press the one move it offers — the one that starts a terminal in the folder the card names above it. The screen goes to the terminal face on its own, with a pane already open there and nothing asked."
                    .to_string()
            }
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
            (Domain::Decision, "open-face") => {
                "Press the control that opens this project's decision records, beside the views its tasks are read on."
                    .to_string()
            }
            (Domain::Project, "open") => format!(
                "Open the project \"{}\" again, from the list of projects.",
                req(with, "project")?
            ),
            // The board recut. The row is named by what it is for rather than by its label, which is a
            // word of the interface — and the axis's own name is on its button, which the reader gave it.
            (Domain::Project, "group-by") => format!(
                "Above the board, in the row of buttons that choose what its columns are cut along, press \"{}\". The columns become that axis's values.",
                req(with, "axis")?
            ),
            // The two moves the classification side of this face has, and the manager is opened inside the
            // first of them: it is reached from the same row above the board that cuts the columns, so a
            // step that sent a reader anywhere else would be describing a screen that is not there.
            //
            // Neither line names the box by its label. What is drawn on it is each reader's own language,
            // and what the road means is the answer the box carries — so it is named by what it does, the
            // way every other control on these roads is.
            (Domain::Dimension, "required") => {
                let dimension = req(with, "dimension")?;
                match with.get("required").and_then(|v| v.as_bool()).unwrap_or(true) {
                    true => format!(
                        "Above the board, open the way into managing the project's categories, find the row for \"{dimension}\", and turn on the box that makes the category demand an answer.",
                    ),
                    false => format!(
                        "Above the board, open the way into managing the project's categories, find the row for \"{dimension}\", and turn off the box that makes the category demand an answer.",
                    ),
                }
            }
            // How many values one record may answer the category with, on the box beside the one above.
            // Turning it off is the direction the store can refuse — a record still answering with
            // several is not quietly emptied out — so a road walking that way is walking toward a
            // refusal rather than toward a screen that changed.
            (Domain::Dimension, "cardinality") => {
                let dimension = req(with, "dimension")?;
                match with.get("multi").and_then(|v| v.as_bool()).unwrap_or(true) {
                    true => format!(
                        "Above the board, open the way into managing the project's categories, find the row for \"{dimension}\", and turn on the box that lets one task or decision answer it with several values.",
                    ),
                    false => format!(
                        "Above the board, open the way into managing the project's categories, find the row for \"{dimension}\", and turn off the box that lets one task or decision answer it with several values.",
                    ),
                }
            }
            // Which side of the store the category classifies, set in that same manager. It is a pick
            // and not a box, unlike the two flags beside it: there are three answers, and the one every
            // category starts on is the wide one — so the line names the answer by what it leaves the
            // category classifying rather than by the word drawn on the control.
            (Domain::Dimension, "applies-to") => {
                let dimension = req(with, "dimension")?;
                let tail = match req(with, "side")? {
                    "task" => "to tasks alone",
                    "decision" => "to decisions alone",
                    "both" => "to both tasks and decisions",
                    other => return Err(format!("action `applies-to` does not know the side `{other}`")),
                };
                format!(
                    "Above the board, open the way into managing the project's categories, find the row for \"{dimension}\", and set the control that says what it classifies {tail}.",
                )
            }
            // The value taken out of that same manager. A required category asks where the tasks
            // classified as it are to go before it lets one go, so the line says that too where a road
            // names somewhere — and it is named as an answer the screen asks for, not as a second move
            // the reader has to think of.
            (Domain::Dimension, "value-rm") => {
                let dimension = req(with, "dimension")?;
                let value = req(with, "value")?;
                match arg_str(with, "to") {
                    Some(to) => format!(
                        "Above the board, open the way into managing the project's categories, take the way that removes the value \"{value}\" from \"{dimension}\", answer that the tasks classified as it move to \"{to}\", and confirm.",
                    ),
                    None => format!(
                        "Above the board, open the way into managing the project's categories, take the way that removes the value \"{value}\" from \"{dimension}\", and confirm.",
                    ),
                }
            }
            // The key a category or one of its values answers to, renamed where a reader renames it. The
            // field sits beside the name in the same manager the box above is in, so the line walks in
            // the same way — and it says the key is typed rather than chosen, since what a reader is
            // being asked for is a word and not a pick.
            //
            // This face has no other door onto a key: a category raised on screen takes the one its id
            // gives it, and there is no field to name another in. So the road here is renaming, and a
            // step that named a key at a row's birth would be describing a screen that is not there.
            (Domain::Dimension, "rekey") => {
                let dimension = req(with, "dimension")?;
                let slug = req(with, "slug")?;
                match arg_str(with, "value") {
                    Some(value) => format!(
                        "Above the board, open the way into managing the project's categories, find the value \"{value}\" under \"{dimension}\", and type \"{slug}\" into the field beside its name that holds its key.",
                    ),
                    None => format!(
                        "Above the board, open the way into managing the project's categories, find the row for \"{dimension}\", and type \"{slug}\" into the field beside its name that holds its key.",
                    ),
                }
            }
            // Whether the axis retires its values by closing them instead of deleting them. The box sits
            // beside the one that names the time axis because the two are the same field — an axis holds
            // one role — and it is named by what it does rather than by its label, the way every other
            // control on these roads is.
            (Domain::Dimension, "closable") => {
                let dimension = req(with, "dimension")?;
                match with.get("closable").and_then(|v| v.as_bool()).unwrap_or(true) {
                    true => format!(
                        "Above the board, open the way into managing the project's categories, find the row for \"{dimension}\", and turn on the box that lets its values be closed instead of deleted.",
                    ),
                    false => format!(
                        "Above the board, open the way into managing the project's categories, find the row for \"{dimension}\", and turn off the box that lets its values be closed instead of deleted.",
                    ),
                }
            }
            // Retiring one of that axis's values, and bringing it back. It is one button on the value's
            // own row rather than two side by side — it flips once it has been pressed — so the two
            // lines send a reader to the same place and name the direction. The button stands only under
            // an axis carrying the role, which is what the step before either of these puts there, and
            // it is shut on the last value a required axis still offers, which is the road's other half.
            (Domain::Dimension, "value-close") => format!(
                "Above the board, open the way into managing the project's categories, find the value \"{}\" under \"{}\", and press the button on its row that closes it.",
                req(with, "value")?,
                req(with, "dimension")?
            ),
            (Domain::Dimension, "value-reopen") => format!(
                "Above the board, open the way into managing the project's categories, find the value \"{}\" under \"{}\", and press the button on its row that opens it again. This panel is the only face that draws a closed value at all, so it is the only one the way back is on.",
                req(with, "value")?,
                req(with, "dimension")?
            ),
            // The window one value covers, written where a reader writes it: a pair of date controls on
            // the value's own row in that same manager, drawn only under an axis carrying the time-axis
            // role. An end nobody has written yet is a button saying so rather than an empty date field
            // — the webview draws an empty one with today faint inside it, which reads as a period
            // ending today — so the line says to press that side open before the date goes in. The end a
            // step leaves out is left standing: writing one end is what an open period is written as.
            (Domain::Dimension, "period") => {
                let dimension = req(with, "dimension")?;
                let value = req(with, "value")?;
                let tail = match (arg_str(with, "start"), arg_str(with, "end")) {
                    (Some(start), Some(end)) => {
                        format!("set the day it starts to {start} and the day it ends to {end}")
                    }
                    (Some(start), None) => {
                        format!("set the day it starts to {start}, leaving the other end as it is")
                    }
                    (None, Some(end)) => {
                        format!("set the day it ends to {end}, leaving the other end as it is")
                    }
                    (None, None) => {
                        return Err("action `period` names neither `start` nor `end`".to_string())
                    }
                };
                format!(
                    "Above the board, open the way into managing the project's categories, find the value \"{value}\" under \"{dimension}\", and in the pair of date controls on its row {tail}. An end carrying no date yet is a button saying so, which turns into a date field once pressed.",
                )
            }
            // Filing a task or a decision under one of the axis's values, from its own pane. The screen
            // keeps one control per axis there, so the axis is named as well as the value: a line naming
            // the value alone would leave a reader hunting the pane for which control carries it.
            (Domain::Dimension, "set") => format!(
                "Open the {} \"{}\" and, in the control its pane keeps for the category \"{}\", choose \"{}\".",
                self.target_noun(with),
                self.target_label(with),
                req(with, "dimension")?,
                req(with, "value")?
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
            // The language the interface is read in, changed where a reader changes it. The step names
            // the code the store keeps rather than the word standing in the list, which is each
            // language's own name for itself — a table the harness would then hold a second copy of,
            // and be wrong about the day one of the nineteen is renamed.
            //
            // Coming back is part of the step and not a note beside it: the setting is on a screen of
            // its own, and the assert after this one is about a listing somewhere else.
            (Domain::Store, "set-language") => format!(
                "In Amenbo's own settings, set the language the interface is read in to the one whose code is \"{}\", then return to the screen the road was on.",
                req(with, "language")?
            ),
            // The view a project raised without one of its own comes up in, changed where a reader
            // changes it — the same screen the language above sits on. The step names the word the
            // store keeps rather than the one standing in the pull-down, for the reason the language
            // step does: the four are drawn in the reader's own language, and a table of those here
            // would go wrong the day one of them is reworded.
            //
            // Returning is part of the step for the same reason too: what this setting decides is on
            // another screen entirely, and the road walks there next.
            (Domain::Store, "set-default-view") => format!(
                "In Amenbo's own settings, set the view a newly created project opens in to the one stored as \"{}\", then return to the screen the road was on.",
                req(with, "view")?
            ),
            // The run of the app ending and another coming up. The only step nobody at the screen
            // carries out: the run owns the app it shoots, so ending this one and starting the next
            // on the same store is the harness's, done in the walk itself, and it is already over by
            // the time the step is handed over. What the operator is asked for is what they alone
            // can say — that the window in front of them is a new one, drawn by an app that came up
            // on its own, rather than the one they had been working in all along.
            (Domain::Store, "run-again") =>
                "Nothing to press: the run has ended Amenbo and started it again on the same store, and the window on the screen is the one the new run drew. Confirm the app you were working in has gone and this one came up in its place — it opens where a fresh launch opens, with nothing of the last run's doing carried out again in front of you — and bring it forward if anything else is standing over it."
                    .to_string(),
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
            // The one switch a machine-wide plugin has, in the one row it has. The line
            // says where the row is *not* as well: a reader who went looking for it in a project's own
            // settings would find the plugin named there and nothing to press, which is the state the
            // step after this one reads.
            (Domain::Plugin, "enable-on-device") => format!(
                "On the installed plugins screen, find \"{}\"'s own row — the one for this device, which names no project — and turn the plugin on there.",
                req(with, "name")?
            ),
            // Its settings, opened inside that same row. Where they are opened from is the whole of the
            // step, as it is for a crossing: a form reached from the row asks for no layer, and one
            // reached anywhere else would be asking what the row has already answered.
            (Domain::Plugin, "open-config-on-device") => format!(
                "In \"{}\"'s own row for this device, open the settings kept there — from inside that row and nowhere else on the screen.",
                req(with, "name")?
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
                refuse_named_crossing(with, "config-set")?;
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
            // The button an author put on that form, pressed. Where the operation asks for something the
            // press does not run it — the boxes come up first — so the line stops at the press, and what
            // the shot after it holds is those boxes as they were opened.
            (Domain::Plugin, "press") => format!(
                "In the settings for \"{}\", press the operation drawn as \"{}\", and do nothing further with what it opens.",
                req(with, "name")?,
                req(with, "label")?
            ),
            // The second half of that press. Typing and running belong to one move: a shot of a filled
            // box nobody ran would be evidence of a value the author's code was never handed.
            (Domain::Plugin, "press-answer") => format!(
                "In the settings for \"{}\", type \"{}\" into the box the press opened under \"{}\", and run it.",
                req(with, "name")?,
                req(with, "value")?,
                req(with, "label")?
            ),
            // The one answer the report takes, and the only button on it that writes anything. Closing the
            // report is not among them: it records nothing and is spent the moment the project is opened
            // again, which is a road that ends where it started rather than one this scenario walks.
            (Domain::Repo, "ai-launch-consent") => match req(with, "answer")? {
                "no" => "On the report about this project's folders, press the button that declines having their AI started on Amenbo.".to_string(),
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
                "In this project's own settings, press the button that clears its answer about starting its AI on Amenbo."
                    .to_string()
            }
            // The same two moves on the settings screen. They name that screen rather than a report,
            // because where they are walked there is none: the folder is wired, so nothing is being
            // reported and the operator sent to a report would find no such thing. What they are for is
            // the reader who wired one tool and moved to another, and the pick is the half that proves
            // the catalog is standing behind it — a tool the folder shows no trace of is reachable only
            // if every one Amenbo knows is on offer.
            (Domain::Repo, "ai-launch-request-pick") => format!(
                "In this project's own settings, choose \"{}\" among the tools it offers the text for.",
                req(with, "tool")?
            ),
            (Domain::Repo, "ai-launch-request-copy") => format!(
                "In this project's own settings, press the button that takes the text for \"{}\".",
                req(with, "tool")?
            ),
            // The fold that holds the other way in. It is named by what is behind it rather than by its
            // label, the label being a word of the interface — and the line says the apps come up with
            // it, since a fold that opened onto nothing is the state every assert after this reads
            // against.
            (Domain::Repo, "mcp-open") => {
                "Open the screen where an AI is connected — the one that lists the apps Amenbo knows, \
                 each row folded to its name, whether it is set up, and the folders it reaches. Open it \
                 again where the road has already been on it: what a row says has to be read now, not \
                 remembered from before the road left, and it comes back with every row folded."
                    .to_string()
            }
            // Ticking a row and taking its road. The row is said to be open, the rows standing folded
            // and the ticks being what is under one — said as a state rather than as a press, because
            // the step before it may have opened that row already and a second press on the head would
            // fold it shut. Every project that is to be ticked is named, and the rest are said to be
            // left clear, because what goes out is the whole selection and not the difference — an
            // operator who added to what was already ticked would be walking the move this screen
            // exists to have taken away.
            (Domain::Repo, "mcp-choose") => format!(
                "With the row for \"{}\" open, tick exactly these projects on it and leave every other \
                 one clear:{}. Then take the one road that row offers — press its button, and where it \
                 asks where to put a file, put it somewhere you can find again.",
                req(with, "app")?,
                names(with, "projects")?
            ),
            // The three answers the tick's band takes, each button named by what pressing it leaves
            // behind rather than by its wording: the labels are words of the
            // interface, and the three sit side by side on the one band — which of them was pressed
            // is exactly what the assert after this has to be able to trust.
            //
            // All three are walked, where the login nudge takes only the refusal: a yes there
            // registers this machine's login with no way back on the road, while a yes here is
            // taken back by moving the settings row to off before the run ends.
            (Domain::Tick, "banner-answer") => match req(with, "answer")? {
                "start" => "In the band offering to watch due dates, press the button that starts the hourly check — the one that answers yes and registers the timer."
                    .to_string(),
                "never" => "In the band offering to watch due dates, press the button that declines it for good — the one whose answer is a no that only the settings row takes back."
                    .to_string(),
                "later" => "In the band offering to watch due dates, press the button that only puts the question off — the one that answers nothing and leaves it to come back tomorrow."
                    .to_string(),
                other => {
                    return Err(format!("action `banner-answer` does not know the answer `{other}`"))
                }
            },
            // The settings row moved. The move and what it writes are one instruction, the way a
            // form's save is: a row read in its new position with nothing written behind it would be
            // evidence of an answer the store never received. The row has two positions, so the one
            // to leave it in is named by what it does rather than by the word drawn on it.
            (Domain::Tick, "set") => match req(with, "position")? {
                "on" => "In Amenbo's own settings, move the hourly check's row to the position that turns the check on — the answer becomes a yes and the timer is registered."
                    .to_string(),
                "off" => "In Amenbo's own settings, move the hourly check's row to the position that turns the check off — the answer becomes a no and the registration is taken away."
                    .to_string(),
                other => {
                    return Err(format!("action `set` does not know the position `{other}`"))
                }
            },
            // ── the terminal face ─────────────────────────────────────────────────────────────
            // Which face the one window is showing. The segments are named by what each shows rather
            // than by the word drawn on them, since those words are the interface's own and the run's
            // language is whatever the machine is set to.
            // The press that goes from a task to the pane its work is happening in. What it lands on
            // is deliberately not promised here: the terminal face may be behind the other segment of
            // this window or in a window of its own, and which of those is the run's business. So the
            // step is the press and the reading of the row before it, and where it landed is read by
            // the step after (`terminal pane`).
            (Domain::Task, "go-to-pane") => format!(
                "On the task \"{}\" standing open, press the row saying where the work is happening — the one carrying the pane's name \"{}\". The screen goes to the terminal, on the page that pane is on, with that pane the one being worked in. Nothing is typed into it: it is somebody's terminal, and being sent to it is not being given it.",
                self.target_label(with),
                req(with, "shows")?
            ),
            (Domain::Terminal, "show-face") => match req(with, "face")? {
                "tasks" => "In the pair of segments at the top of the window, press the one that shows the ledger — the tasks, the projects and the board."
                    .to_string(),
                "terminal" => "In the pair of segments at the top of the window, press the one that shows the terminal — the pane a terminal runs in."
                    .to_string(),
                other => {
                    return Err(format!("action `show-face` does not know the face `{other}`"))
                }
            },
            // The way in, and the one control a face with no folder yet has on it. The step says what
            // choosing does rather than what the button reads, for the reason `show-face`'s does: the
            // words are the interface's own and the run's language is whatever the machine is set to.
            // What is worth confirming while walking it is that nothing else is asked — no name, no
            // submit — since the whole of this road is one press and a folder.
            (Domain::Terminal, "open-folder") => format!(
                "On the terminal face, press the one control it offers — the way in in the middle — and in the picker that opens choose a folder the road calls \"{}\". The folder is bound to the project the face is on, and the face moves on by itself as soon as the picker closes — a pane opens on the agent this folder starts with, or the face offers the ones it found, or it says it found none — and nothing is named and nothing is submitted.",
                req(with, "dir")?
            ),
            // Getting to a plain shell, which is what a road that speaks in a pane speaks to. It is
            // written for the three shapes the face can be in rather than for one press, because
            // which of them is on screen is the run's machine's business and not the road's — an
            // operator told only "choose the plain shell" would be hunting for a control that is not
            // on theirs.
            (Domain::Terminal, "open-shell") =>
                "Open a plain shell in the pane — the terminal with no agent started in it. Where a pane is already running one, end that first (run: exit): the row under a pane whose program has ended is where what to open with is chosen, and the plain shell is on the list. On an empty frame the same list is on the frame itself, above the press that opens it: choose the plain shell there and then press to open. Where the face is offering several agents, or saying it found none it can start, the plain shell is a button on what it is showing. Every way round, a prompt comes up in the pane."
                    .to_string(),
            // Writing a command of the reader's own down on the frame. Two fields and a
            // press, and the reading that matters is taken before the press: what is registered runs
            // in a terminal exactly as it stands, so the frame has to say so while there is still
            // something to change.
            (Domain::Terminal, "register-start") => format!(
                "On the terminal face, find the empty frame — the box on the page that is not a terminal — and under the row of things a pane can be opened with, press the control that registers a command of your own. Fill the two fields it opens: the name \"{}\", and the command line `{}` written exactly as it stands here, spaces and quotes and all. Before saving, confirm the frame shows that same line back as what will run in the terminal. Then save.",
                req(with, "name")?,
                req(with, "line")?,
            ),
            // Opening a pane on one. It is the same two moves the plain shell is opened by — choose,
            // then press — and it is written as its own step because what is being proved is that a
            // row nobody catalogued is pressable at all.
            (Domain::Terminal, "open-registered") => format!(
                "On the empty frame, choose \"{}\" from the row of things a pane can be opened with — it stands among them after the ones Amenbo lists and before the plain shell — and press to open. A pane comes up and the command runs in it.",
                req(with, "name")?
            ),
            // A line typed into the pane and sent. It is typed rather than pasted because what is
            // under test is a terminal: keys are what a terminal is driven by, and a line that
            // arrived some other way would be evidence of a path nobody walks.
            //
            // *Which* pane has to be said. A face can have several panes on it, and beside them it can
            // be asking about work nothing is doing any more — a reader who clicked the wrong box
            // would type this line nowhere at all.
            //
            // Where the road named it, it is named by its own line, which is the only thing on a page
            // of panes that is the road's own. Where it did not, the pane is the one the step before
            // opened — the box on the page with a prompt and nothing else on it. That is said by what
            // is on it rather than by what it was opened for: on a page with a second pane already
            // carrying the road's lines, "the pane the folder was opened in" names the wrong box.
            (Domain::Terminal, "type-line") => {
                let pane = match arg_str(with, "shows") {
                    Some(shows) => format!("the pane showing \"{shows}\" — the one the road typed that line into, and not any of the others"),
                    None => "the pane the step before opened — the box on the page with a prompt and nothing else on it, not the empty frame beside it".to_string(),
                };
                format!(
                    "Click into {pane} — then type \"{}\" and press return. The shell will not know the command — what the line is for is being on the screen, and, where the pane has not been named yet, being the name it takes.",
                    req(with, "text")?
                )
            }
            // A file let go over a pane. It comes from outside for the reason the file face's drop does:
            // a drop reads the disk the operator is sitting at, and nothing the run laid down is
            // anywhere a hand can reach from there. What it lands as is not said here — where it goes
            // is Amenbo's own answer, and the step that reads the input line is where that is settled.
            (Domain::Terminal, "drop-in") => {
                // Which pane it is let go over. A page with one has nothing to say here; a page with
                // two is being told where the drop goes, because where it goes is what the steps
                // after it read.
                let pane = match arg_str(with, "onto") {
                    Some(onto) => format!("the pane showing \"{onto}\""),
                    None => "the pane that has a terminal running in it".to_string(),
                };
                match with.get("beside").and_then(|v| v.as_str()) {
                    None => format!(
                        "From outside Amenbo — a file manager, the desktop, anywhere on this machine — drag a file named \"{}\" over {pane}, and let it go there.",
                        req(with, "brings")?
                    ),
                    // One hand and one movement, said twice over, because a pair let go one after the
                    // other is two drops and proves something else entirely.
                    Some(beside) => format!(
                        "From outside Amenbo — a file manager, the desktop, anywhere on this machine — select a file named \"{}\" and a file named \"{beside}\" together, drag the pair over {pane} in one movement, and let them both go there.",
                        req(with, "brings")?
                    ),
                }
            }
            // What the last copy left, put into a pane's line. The pane is named the way the drop
            // above names one, and the line ends where the drop's does: nothing is sent, because what
            // a hand-over owes is a line the person still has to send themselves.
            (Domain::Terminal, "paste") => {
                let pane = match arg_str(with, "onto") {
                    Some(onto) => format!("the pane showing \"{onto}\""),
                    None => "the pane that has a terminal running in it".to_string(),
                };
                format!(
                    "Click into {pane}, then press the key this machine pastes with. What the last copy put on the clipboard appears in that pane's input line — leave it there, and press nothing else."
                )
            }
            // A command run for its output, which is what the steps after it read. The clearing is
            // said first because it is what makes "the ref" a place on the screen rather than one of
            // several, and the waiting is said last because a press on a half-drawn line is a press
            // on nothing.
            (Domain::Terminal, "run") => {
                let standing_in = match with.contains_key("target") {
                    true => format!(
                        ", putting the ref of the task \"{}\" — the `AMB-T-…` it is drawn by, which the pane had on it a step ago — where the command says `<ref>`",
                        self.target_label(with)
                    ),
                    false => String::new(),
                };
                format!(
                    "Click into the pane and clear what is on it — `clear` at the prompt does that — so nothing an earlier step left is still on the screen. Then type `{}` and press return{standing_in}, and wait until it has finished and the prompt is back.",
                    req(with, "command")?
                )
            }
            // Pressing one of those refs. The folded half asks for the width to be moved rather than
            // for a particular width: where the fold lands is the run machine's business — its font,
            // its screen, how wide the window will go — so what the road can ask for is the state,
            // and the operator is the one who can see when it has arrived.
            (Domain::Terminal, "press-ref") => match flagged(with, "folded") {
                false => format!(
                    "In the pane, press the ref of the task \"{}\" where the output drew it — the `AMB-T-…` characters themselves, which the pane offers as a link.",
                    self.target_label(with)
                ),
                true => format!(
                    "Drag the window's side edge in or out until the ref of the task \"{}\" is broken across two rows — part of it at the end of one row, the rest at the start of the next — and press it there. The fold is the whole of this step: pressed while it sits whole on one row, it proves what the step above already did.",
                    self.target_label(with)
                ),
            },
            // Something set running in the pane and left there, which is what parts it from `run`
            // above: that step ends when the prompt is back, and this one is walked away from with
            // the output still arriving.
            //
            // Something set running in the pane and left there. The command is written out in full
            // for the reason `say`'s is — the whole of what the step is for is that something goes
            // on coming out, and an operator improvising that would be improvising the step. It is
            // written twice because the two shells a pane comes up in count their pings with
            // different words and the rest of the line is the same in both. What it ends with is the
            // road's own line: a step that waited for the interface to say it had finished would be
            // waiting on words drawn in whatever language the run's machine is set to.
            (Domain::Terminal, "keep-printing") => {
                let text = req(with, "text")?;
                format!(
                    "In the pane that has a terminal running in it, run: ping -c {KEEP_PRINTING_SECONDS} 127.0.0.1; echo \"{text}\" — where that pane's shell is PowerShell, the count is written -n {KEEP_PRINTING_SECONDS} and the rest of the line is the same. A line arrives about once a second for the {KEEP_PRINTING_SECONDS} seconds that takes, which is a terminal putting something out with nobody typing at it. Leave the pane alone once it is going: what every step after this reads is a pane nobody is working in, and the step looking for \"{text}\" is the road waiting for the printing to stop."
                )
            }
            // The two moves between one window and two. Named by what each does rather than by the
            // words on them, and each said with where it is: the way out is on the face, the way back
            // is in the window it made, and a road that pressed the wrong one would be in the wrong
            // window for every step after it.
            (Domain::Terminal, "split-out") =>
                "On the terminal face, press the control that opens the terminal in a separate window. A second window appears, offset from this one."
                    .to_string(),
            (Domain::Terminal, "fold-back") =>
                "Press the control that puts the terminal back into one window. This window closes and the terminal returns to the other one."
                    .to_string(),
            // The surface layer, said from inside the pane it is about. The command is written out in
            // full rather than described, because it is the one instruction on these roads that is
            // literally what an agent types — and because the layer refuses to be said anywhere else,
            // so an operator improvising it from a description would be turned away.
            //
            // The layer's four verbs are the four accepted here. An unknown word is refused loudly.
            (Domain::Terminal, "say") => {
                let text = req(with, "text")?;
                let verb = req(with, "verb")?;
                let (command, what) = match verb {
                    "name" => ("name", "the agent naming the pane it is running in"),
                    "note" => ("note", "the agent saying what it is doing now"),
                    "waiting" => ("waiting", "the agent handing the turn over, and saying why"),
                    "finished" => ("finished", "the agent saying what came of the work"),
                    other => return Err(format!("action `say` does not know the verb `{other}`")),
                };
                // Said and stood at, or said and walked away from. The second is the only way to a
                // word that arrives while the ledger is the face up: the layer is spoken inside a
                // pane and read on the other side of the switch, so the operator arms it and
                // crosses over. The wait is here rather than in the road because how long a person
                // needs to press one segment is the driver's business, not the goal's.
                if flagged(with, "away") {
                    format!(
                        "In the pane that has a terminal running in it, run: sleep {SAY_AWAY_SECONDS} && amenbo talk {command} \"{text}\" — then press the segment that shows the ledger before those seconds are up. What lands is {what}, and it lands while the terminal is the face nobody is looking at, which is the only shape it ever reaches the other face in."
                    )
                } else {
                    format!(
                        "In the pane that has a terminal running in it, run: amenbo talk {command} \"{text}\" — this is {what}."
                    )
                }
            }
            // Ending the terminal in a pane, which is done from inside it. **The one control the pane
            // has takes the place away and is not this**, so there is nothing to press here: what
            // ends a program is the program being told to end. The pane is left standing with its
            // last output on it, which is the half these roads go on to read.
            (Domain::Terminal, "end-pane") =>
                "In the pane that has a terminal running in it, run: exit — the program ends. The pane stays where it is with what it printed still on it, and nothing is running in it any more."
                    .to_string(),
            // Getting rid of the place. The pane is named by the words the road typed into it, since
            // that is the only thing on a page of panes that is the road's own — and being sure which
            // one is pressed is the whole point on the one move nothing takes back.
            //
            // The question is half the step, so the operator is told to read it before answering: what
            // is being defended is that it stands there at all. What the answer costs is said with it,
            // because an operator who did not know would have no way to tell an app that lost a pane
            // from one doing exactly as it said.
            //
            // Which question comes up is the pane's own business and not the road's: a session holding
            // a reservation is asked about it by name and offered three ways out, and one holding
            // nothing is asked the plain thing. So `answer` is what a road says about the question it
            // expects — left out for the plain one, and naming one of the three otherwise.
            (Domain::Terminal, "remove-pane") => {
                let press = format!(
                    "On the pane showing \"{}\", press the cross at the end of its own row — the control beside what is said about that pane, and not the one on any other.",
                    req(with, "shows")?
                );
                match arg_str(with, "answer") {
                    // Nothing held, so nothing to choose between. The plain question, and yes.
                    None => format!(
                        "{press} A question comes up before anything happens: read it, then answer it yes. The terminal in that pane ends, the pane goes, and the page closes up behind it."
                    ),
                    Some(answer) => {
                        // The reading is half the step here, and it is the half the three answers
                        // exist for: a question that named the wrong pane's work, or named none, is
                        // the loss it stands in front of. So the task is named before any of them is
                        // pressed, and a step that forgot to name one would send the operator to read
                        // a question against nothing.
                        if !with.contains_key("target") {
                            return Err(
                                "action `remove-pane` answering the three-way question has to name the task the question must name — give it a `target`"
                                    .to_string(),
                            );
                        }
                        let question = format!(
                            "{press} A question comes up before anything happens, and it names what this pane's session is holding: read it, and confirm what it lists is the ref of the task \"{}\" — the `AMB-T-…` it is drawn by — and no other. Three answers stand under it.",
                            self.target_label(with)
                        );
                        // Each is said by what it does rather than by what it reads, the way
                        // `show-face`'s presses are: the words are the interface's own and the run's
                        // language is whatever the machine is set to. Where they stand is said too,
                        // so an operator has two ways to find the one they were sent to.
                        match answer {
                            "hand-back" => format!(
                                "{question} Press the first of them — the one that hands the work back before going. The task named goes back to waiting for somebody to take it, the terminal in that pane ends, the pane goes, and the page closes up behind it."
                            ),
                            "leave" => format!(
                                "{question} Press the second — the one that goes and leaves the work where it is. Nothing on the ledger is moved: the terminal in that pane ends, the pane goes, the page closes up behind it, and the task named is still held."
                            ),
                            "cancel" => format!(
                                "{question} Press the third — the one that stays. Nothing happens at all: the question goes, the pane is still there with what was on it, and the task named is still held."
                            ),
                            other => {
                                return Err(format!(
                                    "action `remove-pane` does not know the answer `{other}` — it is hand-back, leave or cancel"
                                ))
                            }
                        }
                    }
                }
            }
            // Moving the whole face to another project. The row is named by the project's own name,
            // and the step says where the press leaves the screen rather than what the press looks
            // like: the rail carries two lists, the projects and then the panes of the one being
            // shown, and an operator who took a pane's row would be going to a pane instead of to a
            // project, which is a different move with a different screen after it.
            //
            // What it says is the state arrived at rather than the change, and deliberately: which
            // project the face came up on is the run's business, so a road may well press the one
            // already shown, and a line promising the screen would change would read as a failure on
            // the one press that is allowed to do nothing.
            (Domain::Terminal, "go-project") => format!(
                "In the list beside the panes, under its first heading, press the name of the project \"{}\" — a row in that upper list and not one in the list of panes below it. The face is that project's from here on: its panes are the ones drawn, on its own first page, and no other project's pane is on the screen.",
                req(with, "project")?
            ),
            // Opening a pane where there is not one yet. **A pane is opened from the empty frame**,
            // by the press on it that opens one — the row above that press is what it opens *with*,
            // and a road that said nothing about the row would be walked with whatever was on it. So
            // the step says to leave it alone: what this is about is the pane appearing, and which
            // agent it runs is another road's (`open-shell`). That is the whole of it — the two ways in differ only in what happens
            // before the press. From the face there is nothing before it: the empty frame is already
            // on the page. From the strip there is one press first, which opens nothing — it goes to
            // where a pane would land, and the empty frame is what is waiting there.
            //
            // The strip is what a full page has instead of an empty frame, so a road only reaches it
            // where the page it stands on is full: on a page with room the way in is the frame, and
            // there is no second control to press.
            //
            // Neither names a page: where a pane lands is the project's arithmetic and not the road's,
            // and a step that named one would be a road asserting it from the wrong end.
            //
            // The asking half is where the ways in stop differing: a project bound to several folders
            // answers the press on the empty frame with the same question either way, and nothing
            // opens until it is answered. So what comes before is said per way in and the press on
            // the empty frame is said once.
            (Domain::Terminal, "open-pane") => {
                let press = match req(with, "from")? {
                    "face" => "On the terminal face, find the empty frame — the one box on the page that is not a terminal, at the first gap in it — and press what opens a terminal in it, leaving the row above that press as it came up.",
                    "strip" => "On the terminal face, which is full, press the thin strip standing beside the panes at the edge of the page — the one control there that is not a pane. Nothing opens: the screen goes to the page a pane would land on, which is the page with room in it, brought into being where every one of them is full. On the empty frame waiting there, press what opens a terminal in it, leaving the row above that press as it came up.",
                    other => {
                        return Err(format!(
                            "action `open-pane` does not know the way in `{other}` — it is face or strip"
                        ))
                    }
                };
                // The one time the row is not left as it came up. Before anybody on this machine has
                // opened a pane there is nothing on it, and the press says so rather than opening on
                // a guess — so the step has to say what to do about a control that will not answer.
                // Which one is chosen is still another road's, hence "any of them".
                let first_run = "If nothing on that row is on and the press will not answer — the first run on a machine, before anybody has opened a pane — choose any of them first, and the press comes alive; that is the only time this step chooses.";
                let lands = match flagged(with, "asks") {
                    true => "Nothing opens: this project is bound to more than one folder, so what comes up where the pane would have been is the question of which of them it works in.",
                    false => "A pane opens there, in the folder the project is bound to, and nothing is asked.",
                };
                format!("{press} {first_run} {lands}")
            }
            // Answering that question. The row is found by the folder's name at the end of the path
            // it draws, since what the face writes on it is where the folder is and the road knows
            // only what it calls it.
            //
            // The list itself is the goal, so the operator is told what may be on it: a pane belongs
            // to a project and the folders it can work in are that project's, which is the one thing
            // the rail promises. A question offering a way to anywhere else is this road's failure
            // even where the press that follows lands correctly.
            (Domain::Terminal, "pick-folder") => format!(
                "In the question standing where the pane will be, press the folder the road calls \"{}\" — the row whose path ends in that name. Confirm as you press that what it offers is this project's folders and nothing besides: no picker, and no way out to a folder the project is not bound to. The pane opens in the one pressed.",
                req(with, "dir")?
            ),
            // Walking away from the question about where a pane runs. The press is named by what it
            // is *not* — not one of the folders, and not the way in again — because what is under
            // test is the leaving and not the place it was left from. The count already in force is
            // offered as the one control that is on the face whatever else is: a project with
            // nothing open has no panes to press and no second project to cross to, the page digits
            // are not drawn at all where there is only one page, and an operator told only "press
            // somewhere else" on a screen holding one question would be hunting for somewhere to
            // press. Pressing the count that is on asks for the split the face is already at, so
            // nothing about the page moves with it.
            (Domain::Terminal, "leave-question") =>
                "Without answering it, leave the question about which folder the pane works in: press somewhere else on the face — at the top, the pane count that is already in force is on every screen this question can come up on, and pressing the one already on leaves the page as it is. Answer nothing and press none of the folders it offers."
                    .to_string(),
            // How many panes a page draws. The control is named by what it holds rather than by
            // where it sits, and the number is pressed as it is written: every count is drawn at
            // once, so there is nothing to open first. What the step does not say is what
            // becomes of the panes — the frames are one list re-cut by the new count, so a pane can
            // land on another page, and which of that is right is the assert after it.
            //
            // Where the *screen* ends up is said, and it is a different kind of thing: an operator
            // who was not told may read a page they did not ask for as the app having lost their
            // place, and go looking for a pane that is exactly where it should be. The screen stays
            // with the pane being worked in, which is the last one opened or typed at.
            (Domain::Terminal, "set-panes") => format!(
                "At the top of the terminal face, in the row of pane counts, press the one that says {}. It is words rather than a bare digit — the page numbers beside it are the digits — and the one in force is the one that is not dimmed. The page redraws at that split whether or not there are panes to fill it, and the screen stays with the pane being worked in, so it may end up on a different page from the one it was on.",
                count(with, "count")?
            ),
            // Paging. The digits are the pages, so the step names the one it presses and says the
            // whole screen moves: a pane that was on the page being left is not on the screen after
            // this, which is the state half these roads are about.
            (Domain::Terminal, "go-page") => format!(
                "At the top of the terminal face, in the row of page digits, press {}. The whole screen moves to that page.",
                count(with, "page")?
            ),
            // Putting a column away, and asking for it again. Each is said with *where* its control
            // is, because the two are not in the same place: the rail is folded from the row above
            // the panes and the file face from a cross on its own panel, while both are asked for
            // again from that same row. An operator told only "close it" would go looking for one
            // control on a face that has two.
            //
            // The controls are named by what they do rather than by what they read, for the reason
            // `show-face`'s are: the words are the interface's own and the run's language is
            // whatever the machine is set to.
            (Domain::Terminal, "hide-side") => match side(with)? {
                Side::Rail => "At the top of the terminal face, press the control that folds the list of panes away — the small one just after the way out to a separate window. The list goes, and the panes take the width it was using.".to_string(),
                Side::Files => "On the panel beside the panes — whichever of its two halves is up — press the cross at the end of its own top row. The panel goes, and the panes take the width it was using.".to_string(),
            },
            (Domain::Terminal, "show-side") => match side(with)? {
                Side::Rail => "At the top of the terminal face, press that same control again. The list of panes comes back where it was.".to_string(),
                Side::Files => "At the top of the terminal face, at the far end of the row, press the one that shows the folder's files — the second of the two, the first being the page written on. The panel comes up on that half, whether it was closed or showing the other one.".to_string(),
            },
            // The one gesture on these roads, and the one step aimed at something the screen does
            // not name: the edge is a line rather than a button, so nothing reaches it the way a
            // button is reached and what the operator is told is where the edge is. The screen tool
            // drags between two points, but working those points out of the screen is an operator's.
            (Domain::Terminal, "drag-side") => {
                let which = side(with)?;
                // Said by what it does to the column rather than by left and right: which way is
                // wider depends on which edge of the face the column is on, and an operator handed a
                // direction would drag the wrong way on one of the two.
                let toward = match req(with, "toward")? {
                    "wider" => "so the column grows by something like a finger's width, and the panes give up that much",
                    "narrower" => "back to about where the edge started, so the panes take that width again",
                    other => return Err(format!("action `drag-side` does not know the direction `{other}`")),
                };
                format!(
                    "Put the pointer on the line between {} and the panes — the cursor turns into the one that says a thing can be dragged sideways — and drag it {}. The column follows the pointer while you hold it and stays where you let go.",
                    which.phrase(),
                    toward
                )
            }
            // A file put in the folder from outside Amenbo, while the app is up. It is written as an
            // instruction and not left to the premise because *when* it happens is the whole of what
            // is under test: what the face draws has to move without anybody touching the app, so the
            // operator is told plainly to leave it alone.
            (Domain::Repo, "write-file") => format!(
                "Outside Amenbo — in a file manager or another terminal — make the file \"{}\" inside the folder the road calls \"{}\", with \"{}\" in it, making any folder on the way that is not there. Do not touch Amenbo while you do.",
                req(with, "path")?,
                req(with, "dir")?,
                req(with, "content")?
            ),
            // The same, for a file whose bytes a road cannot hold in a line of YAML. What the operator
            // is asked for is a copy over the top: the name stays, so nothing about the panel's own
            // list changes, and the only thing that could reach the screen is what is inside the file.
            //
            // Where the fixture lies is said as the road says it, and where to find it is on stderr
            // with the rest of the world the run stood up: the run places the folders, and this YAML
            // never sees a path.
            (Domain::Repo, "copy-fixture") => format!(
                "Outside Amenbo — in a file manager or another terminal — copy this run's fixture \"{}\" over the file \"{}\" inside the folder the road calls \"{}\", keeping the name it has. Do not touch Amenbo while you do.",
                req(with, "from")?,
                req(with, "path")?,
                req(with, "dir")?
            ),
            // The two halves of the clipboard that happen outside Amenbo. They are the operator's own
            // file manager, named as loosely as the rest of the outside is: what matters is that the
            // clipboard was used by something that is not this window, not which application it was.
            (Domain::Repo, "copy-outside") => match with.get("dir").and_then(|v| v.as_str()) {
                Some(dir) => format!(
                    "Outside Amenbo — in a file manager — copy the file \"{}\" inside the folder the road calls \"{dir}\" the way that machine copies a file, so it is on the clipboard.",
                    req(with, "path")?
                ),
                // No folder named is the run's own, which is where the line on stderr says to look:
                // it is outside every folder the road binds, which is the point of a file kept there.
                None => format!(
                    "Outside Amenbo — in a file manager — copy the file \"{}\" from the folder this run works in, the way that machine copies a file, so it is on the clipboard. The run said where that folder is before the first step.",
                    req(with, "path")?
                ),
            },
            (Domain::Repo, "paste-outside") => format!(
                "Outside Amenbo — in a file manager — open \"{}\" inside the folder the road calls \"{}\" and paste there, the way that machine pastes a file.",
                req(with, "path")?,
                req(with, "dir")?
            ),
            // A bound folder taken away from under the app, the same way and for the same reason: a
            // folder is moved by whoever moves folders, and what Amenbo holds only becomes wrong
            // afterwards. The screen road wants the move to land *while the app is watching*, which
            // is what a premise could not do — the section it is about is drawn before it happens.
            //
            // Where it goes is named the way it is named everywhere else on these roads: by what the
            // road calls it, since the run places the folders and the YAML never sees a path.
            (Domain::Folder, "move") => format!(
                "Outside Amenbo — in a file manager or another terminal — take the folder the road calls \"{}\" away from where it stands, moving it beside itself under the name \"{}\". Do not touch Amenbo while you do.",
                req(with, "dir")?,
                req(with, "to")?
            ),
            // ── the file face ─────────────────────────────────────────────────────────────────
            // The column beside the panes. Its sections are named by what each is about rather than
            // by their headings, for the reason the segments are: the headings are the interface's
            // own words and the run's language is whatever the machine is set to.
            (Domain::Files, "tree") => match flag(with, "open")? {
                // Every one of them: a project bound to several folders draws a section each, and a
                // row can only be read in the section it belongs to now that the tree is the only
                // place rows are.
                //
                // **The state, not the press.** The section is drawn unfolded, so on most roads
                // there is nothing here for an operator to do — and a step that told them to unfold
                // what is already unfolded would have them fold it. Saying where the screen has to
                // stand leaves the road true wherever it is walked from, a section a step above
                // folded included.
                true => "In the column beside the panes, the section that draws the folder itself is to be standing unfolded — each of them where there is more than one. It is drawn that way, so this is usually nothing to do; unfold any that is folded.".to_string(),
                false => "Fold those sections back up.".to_string(),
            },
            // A folder opened a level, or shut again. Opening is what a road asks for nearly every
            // time, so it is what the step means when it says nothing.
            (Domain::Files, "enter") if !unfolds(with) => format!(
                "In the folder's section, fold the folder \"{}\" shut. What was under it goes off the screen; the folder's own row stays.",
                req(with, "name")?
            ),
            (Domain::Files, "enter") => format!(
                "In the folder's section, open the folder \"{}\" one level.",
                req(with, "name")?
            ),
            // Twice, because one press picks the row out and does no more: the file lies over the
            // tree while it is being read, so a row opened on the way past is the row that hides
            // what a reader was reaching for next.
            (Domain::Files, "open") => format!(
                "In {}, press \"{}\" twice. The column is replaced by what is in that file.",
                section(with)?,
                req(with, "name")?
            ),
            // The one control with two ends and one press, so the step names the end. What it is
            // called on screen is the form it is *not* in — a switch says where it goes — which is
            // why the instruction describes the offer rather than quoting the word on it.
            (Domain::Files, "show-as") => match form(with, "form")? {
                "source" => "On the row the open file is named on, have it drawn as the text it is rather than as what that text says: press the control offering to edit it, if it is not already showing the text. The hashes and the brackets are then on the screen as characters."
                    .to_string(),
                _ => "On the row the open file is named on, have it drawn as what its text says rather than as the text itself: press the control offering to read it, if it is not already drawn that way. The hashes and the brackets are then gone, and what they marked is drawn."
                    .to_string(),
            },
            // The control is named by what it says rather than by where it is: it draws the encoding
            // and the newline with a dot between them, and those words are the file's rather than the
            // interface's — so an operator can find it on a screen in any language.
            (Domain::Files, "reopen-with") => format!(
                "On the row the open file is named on, press what says how it was read — an encoding and a newline with a dot between them — and choose \"{}\" from the list that comes up. The file is read again from its bytes as that.",
                req(with, "encoding")?
            ),
            (Domain::Files, "back") =>
                "Press the way back out of the file. The column returns to its two sections."
                    .to_string(),
            // The keys. What each one reaches is decided by where the reader is standing, so every
            // line here says where that has to be — and says the click is only for when it is not,
            // because a road that opened or pressed a row a step ago has left the keyboard where
            // the key wants it already, and an operator told to click every time would be walking
            // a road no reader walks.
            //
            // They stand at three different heights. The key that leaves is the panel's and takes
            // one layer per press; F2 and a letter are the row the keyboard is on and do nothing at
            // all unless it is on one; the bin and the copy are what the reader has picked out,
            // which is the row they are standing on and every other row picked out with it.
            //
            // **The last two are why no line here tells anybody to click first.** A click without a
            // key held puts the selection back to one row, so a step that opened with one would
            // undo the picking the steps before it did — and the road would go green over a face
            // that had never acted on more than a row.
            //
            // The vocabulary is closed here rather than in the registry — a key the face has no
            // answer for is a road nobody can walk, and it fails on the way in rather than on a
            // screen.
            (Domain::Files, "press") => match req(with, "key")? {
                "escape" => "With the keyboard standing on the panel — which is where a row just pressed, or a file just opened, has left it — press the key this machine leaves things with. It is one layer per press: what is drawn over the tree goes first, and the panel itself only once the tree is what is showing. Click a row of the folder's section first only if something outside the panel has been clicked since, because the terminal beside this column hears the same key as meaning something of its own."
                    .to_string(),
                // The key that renames. It is the second door onto the box the menu opens — the
                // typing is `rename` either way — and which row it opens the box on is decided by
                // where the keyboard is standing, so the line says how a row comes to be stood on
                // rather than naming one: telling an operator to click the row would move the
                // keyboard off whatever the step before had walked it to.
                "f2" => "With the keyboard standing on a row of the folder's section — the click that opens a folder leaves it there, and so does a letter typed on the tree — press F2. A box takes that row's place, holding the name the row has. Type nothing into it here — the row it opened on is what this step is for, and what goes into the box is the step after it."
                    .to_string(),
                // A letter, which walks the tree rather than doing anything to a row. Any single
                // character is the vocabulary, because the face answers every one of them the same
                // way; the space is the one it hands back, so it is out.
                other if other.chars().count() == 1 && other != " " => format!(
                    "With the keyboard standing on a row of the folder's section — the click that opens a folder leaves it there — press \"{other}\". The keyboard moves down to the next row whose name begins with that letter, wrapping past the last row round to the first. Nothing is opened and no name changes: what moves is which row the keyboard is standing on."
                ),
                // The bin, reached from the tree rather than from the row above an opened file:
                // that one is the file on the screen and leaves what is picked out alone, and this
                // one is what the reader picked. The question is left standing for `answer`, the
                // way the other bin leaves it.
                "delete" => "With the keyboard standing on a row of the folder's section — the click that picked one out leaves it there — press the key this machine deletes with. Everything picked out goes to the bin together. If the panel asks first, leave the question standing and answer nothing; leave the box about not asking again unticked."
                    .to_string(),
                // And the copy. It is not `files copy` with the click left out: that one stands the
                // keyboard on a named row, which is a copy of one row, and this is a copy of what
                // was picked.
                "copy" => "With the keyboard standing on a row of the folder's section — the click that picked one out leaves it there — press the key this machine copies with. Everything picked out goes on the clipboard in one copy, and nothing on the screen changes to say so."
                    .to_string(),
                other => {
                    return Err(format!("action `press` does not know the key `{other}`"))
                }
            },
            // The typing, put where it cannot land on top of what is already there: a road that reads the
            // file afterwards reads it for both, and an operator who typed over the middle of it would
            // leave one of the two readings answering for nothing.
            (Domain::Files, "edit") => format!(
                "Click into the text of the file, put the caret at the very end of it, press Enter and type \"{}\" on the new line.",
                req(with, "types")?
            ),
            // The same box filled from the clipboard. Where the caret goes is said the way the typing
            // says it, and for the same reason: what is already in the file is half of what the road
            // reads afterwards. What arrives is not named, because a road cannot name it — that is the
            // whole of why this is a step rather than a longer `edit`.
            (Domain::Files, "paste-into-editor") =>
                "Click into the text of the file, put the caret at the very end of it, press Enter, then press the key this machine pastes with. What the last copy put on the clipboard goes in on the new line."
                    .to_string(),
            // And the keeping. The control is named by what it does rather than quoted, the same as
            // every other item on this face — and the line says where it is, since a reader who has just
            // been typing is looking at the text and not at the row above it.
            (Domain::Files, "save") =>
                "In the row above the text — beside the file's name — press the way the panel offers to keep what was typed."
                    .to_string(),
            // And the offer that comes with the line about the file having moved. It is named by what
            // it does — take the disk, drop the typing — because the words on it are the interface's
            // own, and it is said where it is, since it stands with that line rather than in the row
            // the saving is in.
            (Domain::Files, "read-again") =>
                "Beside the line saying somebody wrote to this file after it was opened, press the offer to read it again — what is on the disk now replaces what is in the editor."
                    .to_string(),
            // The file face's own settings row moved. The move and what it writes are one instruction,
            // the way the tick's is: a row read in its new position with nothing written behind it
            // would be evidence of an answer nothing kept.
            (Domain::Files, "set") => match req(with, "position")? {
                "asks" => "In Amenbo's own settings, under the section about files, move the row for the question before binning to the position that has the panel ask."
                    .to_string(),
                "quiet" => "In Amenbo's own settings, under the section about files, move the row for the question before binning to the position that has the panel not ask."
                    .to_string(),
                other => {
                    return Err(format!("action `set` does not know the position `{other}`"))
                }
            },
            // The bin. The press and nothing after it: where a road put the row in `asks`, the panel
            // puts its question here and the step leaves it standing, because deciding it is
            // `answer`'s. The line says to leave the checkbox alone whatever happens — ticking it
            // would turn the question off on this machine for every run walked here afterwards.
            (Domain::Files, "trash") =>
                "In the row above the file — at its right-hand end, past the file's name — press the bin. If the panel asks whether to move the file to the bin, leave the question standing and answer nothing; leave the box about not asking again unticked."
                    .to_string(),
            // And answering it. The two are named by what each does rather than by the words on the
            // buttons, which are the interface's own.
            (Domain::Files, "answer") => match req(with, "answer")? {
                "yes" => "In the question the panel put about binning the file, press the answer that goes ahead and bins it. Leave the box about not asking again unticked."
                    .to_string(),
                "no" => "In the question the panel put about binning the file, press the answer that keeps the file where it is. Leave the box about not asking again unticked."
                    .to_string(),
                other => {
                    return Err(format!("action `answer` does not know the answer `{other}`"))
                }
            },
            // And taking it back. The key is the machine's own, and the line says where to be standing:
            // the terminal beside this column hears the same key as meaning something of its own.
            // The copy, on the row named. The line says to stand on the row first, because the key
            // reaches nothing where nothing is standing.
            // Picking rows out, one press at a time. The key is named by what it does and not by its
            // glyph, the way every key on this face is: which one adds a row is the machine's answer,
            // and an operator on a Mac and one on Windows press different keys to say the same thing.
            //
            // What was picked before is said to stay, because that is the whole of what separates this
            // press from an ordinary one — and an operator who saw the earlier row go plain would be
            // looking at the bug this step exists to catch.
            //
            // **Both halves are said, because the press has two.** On a row already picked out the
            // same key takes it back out, which is how a reader corrects a slip — and a line that
            // promised only the adding would read as a failure to an operator walking the other half.
            (Domain::Files, "pick") => format!(
                "In {}, hold down the key this machine adds to a selection with and click the row \"{}\". If it was not picked out it joins whatever was, which stays as it is; if it was, that press takes it back out and leaves the rest.",
                section(with)?,
                req(with, "name")?
            ),
            // And a run of them in one press. Where the run starts is not named: it is wherever the
            // last press without Shift landed, which the road put there a step ago and the operator can
            // see on the screen. Which way it runs is not said either — a reader reaches back up a tree
            // as often as down it, and a line that said "down" would send them looking the wrong way.
            (Domain::Files, "pick-to") => format!(
                "In {}, hold down Shift and click the row \"{}\". Every row between it and the one picked out last comes in with them, whichever way round the two stand.",
                section(with)?,
                req(with, "name")?
            ),
            (Domain::Files, "copy") => format!(
                "In {}, click once on the row \"{}\" so the keyboard is standing on it, then press the key this machine copies with.",
                section(with)?,
                req(with, "name")?
            ),
            // The paste. Where it lands is the row the keyboard is on — the folder that row is, or the
            // folder holding it where the row is a file — which is the rule a drop lands by. The line
            // says to stand on the row first, because the key reaches nothing where nothing is
            // standing.
            (Domain::Files, "paste") => format!(
                "In {}, click once on the row \"{}\" so the keyboard is standing on it, then press the key this machine pastes with.",
                section(with)?,
                req(with, "name")?
            ),
            (Domain::Files, "undo") =>
                "With the file column in front of you — click once on an empty part of it if something else has the keyboard — press the key this machine undoes with."
                    .to_string(),
            // A file brought in from outside and let go over a folder's row. The instruction names where it
            // is dragged from as loosely as it can — anywhere on the machine that is not this folder —
            // because what would go wrong is dragging a row out of the panel and back into it, which is a
            // move of its own and not this one.
            (Domain::Files, "drop-in") => format!(
                "From outside Amenbo — a file manager, the desktop, anywhere on this machine that is not this folder — drag a {} named \"{}\"{} over the row \"{}\" in {}, and let it go there.",
                made(with)?,
                req(with, "brings")?,
                holding(with),
                req(with, "name")?,
                section(with)?
            ),
            // The menu over a folder, which is where a name is made. The heading is named by nothing, so the
            // line says which of the two the operator is pointing at rather than leaving them to read it off
            // an arg that is not there.
            (Domain::Files, "menu-on-folder") => match with.get("name").and_then(|v| v.as_str()) {
                Some(name) => format!(
                    "Come back to Amenbo if something else is in front of it. Then in {}, right-click the folder's row \"{name}\": a short menu of what can be done in that folder comes up where the pointer is.",
                    section(with)?
                ),
                None => format!(
                    "Come back to Amenbo if something else is in front of it. Then right-click the heading over {} — the folder's own row, at the top of the tree: the same short menu comes up where the pointer is.",
                    section(with)?
                ),
            },
            // The item pressed and the name typed, which is one move. The refusal is described here rather
            // than at the reading, because what the operator does next depends on the box still being open.
            (Domain::Files, "name") => format!(
                "In that menu, press the item that makes a new {}. A box takes the place of a row: type \"{}\" into it and press Enter. If the machine will not take that name it says so under the box, and the box stays open — leave it as it is.",
                made(with)?,
                req(with, "name")?
            ),
            // The same box over a name already on the row, which is why the line says the old one goes: a
            // box that opened holding the old name would otherwise be read as one to add to.
            //
            // **What the box selects is not the whole name.** It opens with the part before the last
            // dot picked out, so that renaming `archive.tar.gz` is a matter of typing — and an
            // operator who typed a whole name over that selection would leave the extension standing
            // twice. So the line asks for the selection to be widened before anything is typed.
            (Domain::Files, "rename") => match with.get("by").and_then(|v| v.as_str()) {
                // The box the key opened. The menu is not named, because there is none: `press`
                // with `f2` left the box standing on the row the keyboard was on, and this is the
                // typing that half of the move stops short of.
                Some("key") => format!(
                    "The box standing where the row was is holding the name that is there, with the part before the last dot selected. Select the whole of it, type \"{}\" so the box holds that and nothing else, and press Enter.",
                    req(with, "name")?
                ),
                Some("menu") | None => format!(
                    "In that menu, press the item that changes the name. The box opens holding the name that is there, with the part before the last dot selected. Select the whole of it, type \"{}\" so the box holds that and nothing else, and press Enter.",
                    req(with, "name")?
                ),
                Some(other) => {
                    return Err(format!("`by` does not know `{other}` — it is menu or key"))
                }
            },
            // Handing the file to the machine. On a row the menu is a right-click, and it is drawn on files alone
            // — a folder's row opens a level — so the step names a row the way every other one here does, and says
            // where the menu comes up, since nothing else on this face does.
            (Domain::Files, "menu") => format!(
                "Come back to Amenbo if a hand-over left something else in front of it. Then in {}, right-click the row \"{}\": a short menu of what can be done with that file comes up where the pointer is.",
                section(with)?,
                req(with, "name")?
            ),
            // The same menu from the other side. What a refused file offers is a way on rather than a way back,
            // and the pointer is nowhere near a row by then.
            (Domain::Files, "menu-on-file") =>
                "Under what the column says about the file, press the way on it offers — the one about opening the file in another application. The same short menu of what can be done with the file comes up where the pointer is."
                    .to_string(),
            // One item pressed. The item is described rather than quoted: its words are the
            // interface's, and the run's language is whatever the machine is set to.
            (Domain::Files, "hand-over") => match door(with)? {
                "usual" => "In that menu, press the item that opens the file with the application this machine already opens that kind of file with."
                    .to_string(),
                "pick" => "In that menu, press the item that opens the file with an application you pick."
                    .to_string(),
                // `door` has already turned away anything that is not one of the three.
                _ => "In that menu, press the item that shows the file in the file manager.".to_string(),
            },
            // The item that goes the other way — into Amenbo rather than out of it. It is described
            // by where the path goes, the same as the three above, and the line says what is to be
            // left alone: the path arrives where a person types, and sending it is theirs.
            //
            // **The row may be a folder as much as a file**, and the item says which it is in the
            // interface's own words — so it is described here by what it does rather than named, the
            // way every other item on this menu is.
            (Domain::Files, "hand-to-pane") =>
                "In that menu, press the item that pastes the row's path into the pane being worked in. The menu goes, and the path the row is at appears in the pane's input line — leave it there, and press nothing else."
                    .to_string(),
            // The item that copies. Nothing on the screen answers it, so the line says that too: an
            // operator watching for something to happen would otherwise read a working press as a
            // failed one and stop the run on the step before the one that reads it.
            (Domain::Files, "copy-path") =>
                "In that menu, press the item that copies the row's path. The menu goes and nothing else on the screen changes — what the press left is on this machine's clipboard, and the step that pastes it is where it is read."
                    .to_string(),
            // The same hand-over made by hand. No menu is opened: the row itself is taken hold of and
            // carried, which is what the step says twice over — the press, and the pointer being over
            // the pane before the hand lets go. Where it is let go is the whole of what this proves,
            // so the pane is named and the surface it draws is read on the way past.
            (Domain::Files, "carry-to-pane") => format!(
                "In {}, press and hold on the row \"{}\" and drag it — without letting go — onto the pane showing \"{}\": that pane says it would take it. Let go there. The path the row is at appears in that pane's input line — leave it there, and press nothing else.",
                section(with)?,
                req(with, "name")?,
                req(with, "onto")?
            ),
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
                "Confirm the task \"{}\" is {} the listing filtered by `{}`.{}",
                self.target_label(with),
                if present(with) { "present in" } else { "absent from" },
                req(with, "filter")?,
                self.struck_note(with)
            ),
            (Domain::Task, "narrowed") => format!(
                "Confirm the card \"{}\" is {} the board the narrowing left.{}",
                self.target_label(with),
                if present(with) { "still on" } else { "gone from" },
                self.struck_note(with)
            ),
            // Read on the board with nothing opened, which is the whole claim: the classification is
            // legible where the work is. The axis is named as well as the value, since what is under
            // test on the absent side is the axis being left off — a card that simply carries no value
            // on it would satisfy the words and not the question.
            (Domain::Task, "carded") => match present(with) {
                true => format!(
                    "Confirm the card \"{}\" carries \"{}\" on it — its value on the axis `{}` — with nothing opened.",
                    self.target_label(with),
                    req(with, "value")?,
                    req(with, "dimension")?
                ),
                false if grouping(with) => format!(
                    "Confirm the card \"{}\" does not repeat \"{}\" — the axis `{}` is the one the columns are cut along, so that word is on the heading above the card and must not be on the card itself.",
                    self.target_label(with),
                    req(with, "value")?,
                    req(with, "dimension")?
                ),
                false => format!(
                    "Confirm the card \"{}\" carries nothing from the axis `{}`: \"{}\" is nowhere on the board.",
                    self.target_label(with),
                    req(with, "dimension")?,
                    req(with, "value")?
                ),
            },
            // The row of buttons that cut the columns, read for which axes it offers. It is the same row
            // `group-by` presses, and the line says where to look rather than what the row is called:
            // the label above it is a word of the interface, and the axis's name is the reader's own.
            (Domain::Project, "groupable") => match present(with) {
                true => format!(
                    "Above the board, in the row of buttons that choose what its columns are cut along, confirm \"{}\" is one of them.",
                    req(with, "axis")?
                ),
                false => format!(
                    "Above the board, in the row of buttons that choose what its columns are cut along, confirm \"{}\" is not one of them — the name is nowhere on the board.",
                    req(with, "axis")?
                ),
            },
            // One column of the board that row cut, read for whether it is drawn at all. A closed value
            // is where the answer stops following from the value being defined: the column stands while
            // cards are still in it — hiding it would take those tasks off the board — and goes once the
            // last one leaves, so an axis that keeps closing values does not grow columns nobody can
            // drop into.
            //
            // What is standing in the column is no part of the reading, which the line says out loud: an
            // open value is drawn a column before anything is filed under it, and that empty column is
            // what a road files the first card through. A line that asked for the cards would send the
            // reader of such a road looking for a failure the board is not having.
            (Domain::Project, "column") => match present(with) {
                true => format!(
                    "On the board cut along \"{}\", confirm there is a column headed \"{}\", with or without cards standing in it.",
                    req(with, "axis")?,
                    req(with, "value")?
                ),
                false => format!(
                    "On the board cut along \"{}\", confirm there is no column headed \"{}\" — the value is still on the category, and nothing is filed under it for the board to hold.",
                    req(with, "axis")?,
                    req(with, "value")?
                ),
            },
            // The fold, read for both of the things it does. Neither half is a reading's to close, so the
            // line says out loud what an eye on the shot is looking for.
            (Domain::Task, "filters-folded") => format!(
                "Confirm the values are off the screen — the tasks have their room back — and that the control they folded into says {} axes are narrowing.",
                with.get("axes")
                    .map(show)
                    .ok_or_else(|| "arg `axes` must say how many axes are narrowing".to_string())?
            ),
            (Domain::Decision, "filters-folded") => format!(
                "Confirm the values are off the screen — the decisions have their room back — and that the control they folded into says {} axes are narrowing.",
                with.get("axes")
                    .map(show)
                    .ok_or_else(|| "arg `axes` must say how many axes are narrowing".to_string())?
            ),
            // The row the narrowing left, on the decisions tab. It names no narrowing of its own for the
            // reason the board's twin does not: the move in front of it is what did the narrowing, and
            // saying it twice is what would let the two disagree.
            (Domain::Decision, "narrowed") => format!(
                "On the list the narrowing left, confirm the decision \"{}\" is {} the rows.",
                self.target_label(with),
                if present(with) { "among" } else { "not among" }
            ),
            // Which record the press opened. Both halves name the phrase rather than the record's title:
            // whatever row led here is carrying the title too — a hit, or the question the terminal face
            // puts — so a line read on it would pass over a press that opened nothing, and over one that
            // opened the wrong record just as quietly.
            // The row that says where the work is happening. It is read on the task's own face, and
            // what it carries is the pane's name rather than anything about the reservation — the
            // status beside it already says that, and this says whose terminal it stands in.
            //
            // The absent half is not "no reservation". A move made outside a pane leaves no row, and a
            // pane that has closed takes its row with it while the reservation stands — so the line says what is being looked for and never what it would mean.
            (Domain::Task, "pane") => match present(with) {
                true => format!(
                    "On the ledger, open the task \"{}\" and confirm it draws a row saying where the work is happening, carrying the pane's own name \"{}\" — the line typed into that pane, which nothing else on this face says.",
                    self.target_label(with),
                    req(with, "shows")?
                ),
                false => format!(
                    "On the ledger, open the task \"{}\" and confirm nothing on it says the work is happening in the pane \"{}\" — that row is not drawn at all. What the task says about itself otherwise is unchanged; this is about the row and nothing else.",
                    self.target_label(with),
                    req(with, "shows")?
                ),
            },
            (Domain::Task, "opened") => match present(with) {
                true => format!(
                    "Confirm the record \"{}\" is the one standing open in the pane, showing \"{}\" — words the row that led here does not carry.",
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
                let mut line = format!("Ask the cross-cutting search for {}", self.typed(with)?);
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
                // Where in the answer it stands. The eye's, for the reason every other ordering here is:
                // a reading says which words are on the shot and never which line they were on.
                if first(with) {
                    line.push_str(", and that its row is the first of the answer, ahead of whatever the words matched");
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
            // The absence, said as an absence. A field written as `null` is read on this face by there
            // being nothing in its place, so the line asks for that rather than for the word — an
            // operator sent looking for "null" on the pane would find it nowhere and have nothing to
            // answer with. What a build gets wrong here is the second half: a field that empties into a
            // value of its own reads as a day somebody set.
            (Domain::Task, "field") if matches!(with.get("equals"), Some(v) if v.is_null()) => {
                format!(
                    "Confirm the task \"{}\" shows no {} — the pane says it has none, and draws nothing of its own in its place.",
                    self.target_label(with),
                    req(with, "field")?
                )
            }
            (Domain::Task, "field") => format!(
                "Confirm the task \"{}\" shows {} = {}.",
                self.target_label(with),
                req(with, "field")?,
                show(with.get("equals").ok_or("assert `field` needs `equals`")?)
            ),
            // The same reading on the record the other side of the store keeps. A `Review` like the
            // task's own, and for the same reason once more: what a decision's pane says of its state is
            // a word of the interface's, so an eye closes it. It is also the one thing a road can say
            // about a proposal after asking about it: that asking did not settle it.
            (Domain::Decision, "field") => format!(
                "Confirm the decision \"{}\" shows {} = {}.",
                self.target_label(with),
                req(with, "field")?,
                show(with.get("equals").ok_or("assert `field` needs `equals`")?)
            ),
            // Whether a side is offered the category at all, read where the offer actually stands: the
            // control a record's own pane keeps per category. The manager lists a narrowed category
            // like any other — being defined is not being offered — so the manager is the one screen
            // this cannot be read on, and the road names the record whose pane is opened instead.
            (Domain::Dimension, "listed") => {
                let dimension = req(with, "dimension")?;
                let noun = self.target_noun(with);
                let label = self.target_label(with);
                // A `value` narrows the reading from the control to what is inside it: whether the
                // record can be newly filed under that value. The answers are in a list that has to be
                // opened, except the one the record already carries — that one is drawn as the field's
                // answer, which is the half of the claim a closed value is about.
                match (arg_str(with, "value"), present(with)) {
                    (Some(value), true) => format!(
                        "Open the {noun} \"{label}\", open the control its pane keeps for the category \"{dimension}\", and confirm \"{value}\" is among the answers it offers. A value the record already carries stands there as the field's own answer, whether or not it is one the record could newly be filed under."
                    ),
                    (Some(value), false) => format!(
                        "Open the {noun} \"{label}\", open the control its pane keeps for the category \"{dimension}\", and confirm \"{value}\" is not among the answers it offers. The value is still on the category — the classification panel draws it — it is simply not one this record can be newly filed under."
                    ),
                    (None, true) => format!(
                        "Open the {noun} \"{label}\" and confirm its pane keeps a control for the category \"{dimension}\"."
                    ),
                    (None, false) => format!(
                        "Open the {noun} \"{label}\" and confirm its pane keeps no control for the category \"{dimension}\" — the category is still in the manager, it is simply not offered here."
                    ),
                }
            }
            // Whether a value is closed, read on the one face that draws a closed value at all. The
            // panel says it twice over — the row is drawn struck through, and the button on it offers
            // the way back rather than the way out — and neither is a word on a shot: the marking is a
            // style and the button's label is the interface's own, so an eye closes this one.
            (Domain::Dimension, "closed") => {
                let dimension = req(with, "dimension")?;
                let value = req(with, "value")?;
                match closed_equals(with) {
                    true => format!(
                        "Above the board, open the way into managing the project's categories and confirm the value \"{value}\" under \"{dimension}\" is drawn as retired — struck through, with the button on its row now offering to open it again rather than to close it."
                    ),
                    false => format!(
                        "Above the board, open the way into managing the project's categories and confirm the value \"{value}\" under \"{dimension}\" is drawn like the category's other values — not struck through, with the button on its row offering to close it."
                    ),
                }
            }
            // The same reading, one level up: a field a project keeps for itself, read off the face it
            // keeps it on. It is a `Review` like the task's own — what stands on that face is a
            // pull-down, and which of four is standing in it is a thing an eye settles and OCR does not.
            (Domain::Project, "field") => format!(
                "Confirm the project \"{}\" shows {} = {}.",
                self.target_label(with),
                req(with, "field")?,
                show(with.get("equals").ok_or("assert `field` needs `equals`")?)
            ),
            // The warning a smart view's row carries, read with nothing opened — which is the claim:
            // the reader is told before they go looking. The colour is named beside the step because
            // the colour is what an eye actually finds on the shot, and unlike a label it says the
            // same thing in every language.
            (Domain::Task, "view-warns") => {
                let view = view_row(req(with, "view")?)?;
                let step = warn_step(req(with, "step")?)?;
                match count(with, "count")? {
                    0 => format!(
                        "With nothing opened, confirm the sidebar row for {view} carries no badge on {step}: nothing stands on that step, so nothing is drawn for it."
                    ),
                    n => format!(
                        "With nothing opened, confirm the sidebar row for {view} carries the badge {n} on {step}."
                    ),
                }
            }
            // And what the press landed on. The view is named again rather than left to the step
            // before it: a shot of the wrong listing and a shot of the right one both hold rows, and
            // the line has to say which listing the eye is standing in front of.
            (Domain::Task, "view-lists") => format!(
                "Confirm the task \"{}\" is {} the rows of the listing opened from {}.{}",
                self.target_label(with),
                if present(with) { "among" } else { "nowhere among" },
                view_row(req(with, "view")?)?,
                self.struck_note(with)
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
            // The line under a row, and which language it came out in. The sentence is quoted whole
            // rather than described, because what is being read is one of two sentences a build could
            // have drawn there and nothing on the screen says which — the fallback to the base line
            // is silent by design.
            (Domain::Plugin, "line") => format!(
                "Confirm the market's row for \"{}\" draws the line \"{}\" under it.",
                req(with, "name")?,
                req(with, "desc")?
            ),
            // The words a form draws a field under, and — one level down — the words one of a choice's
            // answers is drawn under. The candidate is named by the value it stores rather than by what
            // it says, since what it says is the very thing under test.
            (Domain::Plugin, "asks") => {
                let name = req(with, "name")?;
                let key = req(with, "key")?;
                let label = req(with, "label")?;
                match (arg_str(with, "candidate"), present(with)) {
                    (Some(value), true) => format!(
                        "Confirm the settings form for \"{name}\" draws the answer stored as \"{value}\", under the setting \"{key}\", with the words \"{label}\"."
                    ),
                    // The condition's half: a candidate its author ruled out here is off the
                    // form, not drawn and refusing, so the words are the whole of what is being looked for.
                    (Some(value), false) => format!(
                        "Confirm the settings form for \"{name}\" offers no answer stored as \"{value}\" under the setting \"{key}\" — the words \"{label}\" are nowhere on it."
                    ),
                    (None, true) => format!(
                        "Confirm the settings form for \"{name}\" asks for the setting \"{key}\" under the words \"{label}\"."
                    ),
                    (None, false) => format!(
                        "Confirm the settings form for \"{name}\" does not ask for the setting \"{key}\" at all — the words \"{label}\" are nowhere on it."
                    ),
                }
            }
            // Whether the form is offering one of the author's operations. Apart from
            // `press-shut`, which reads a button that is drawn and will not be pressed: a condition that
            // does not hold takes the button off the form, and a reader has to be able to tell "there, and
            // refusing" from "not there".
            (Domain::Plugin, "offers") => {
                let name = req(with, "name")?;
                let label = req(with, "label")?;
                if present(with) {
                    format!("Confirm the settings form for \"{name}\" offers the operation drawn as \"{label}\".")
                } else {
                    format!("Confirm the settings form for \"{name}\" offers no operation drawn as \"{label}\" — those words are nowhere on it.")
                }
            }
            // What the author's check said, in whichever of its two places the step names. The line beside
            // a box and the sentence over the form are different readings of one verdict — a check may
            // speak about the settings as a whole, about one of them, or about both — so the step says
            // which one it is looking at rather than leaving an eye to find the words anywhere on screen.
            (Domain::Plugin, "checked") => {
                let name = req(with, "name")?;
                let text = req(with, "text")?;
                match arg_str(with, "key") {
                    Some(key) => format!(
                        "Confirm the settings form for \"{name}\" draws \"{text}\" beside the setting \"{key}\", which is where its own check spoke about that box."
                    ),
                    None => format!(
                        "Confirm the settings form for \"{name}\" carries \"{text}\" at its head, which is what its own check said about these settings as a whole."
                    ),
                }
            }
            // The line a press left on the form. It is quoted whole for the reason a row's line is: where
            // the author's program said nothing, Amenbo draws a sentence of its own in that same place,
            // and nothing on the screen says which of the two is standing there.
            (Domain::Plugin, "press-said") => format!(
                "Confirm the settings form for \"{}\" draws \"{}\" beside the operation that was pressed.",
                req(with, "name")?,
                req(with, "text")?
            ),
            // The box that press opened, read for what it is holding. Both halves are the step: the words
            // it asks under are the author's, and its being empty is the whole of what a value handed to
            // one run and kept nowhere looks like from the outside.
            (Domain::Plugin, "press-asks") => {
                let name = req(with, "name")?;
                let label = req(with, "label")?;
                // The credential half is a line of its own rather than a clause on the other: an eye told
                // to check two things about one box checks the first and reads past the second, and which
                // of the two boxes is standing there is exactly what a build gets wrong.
                match with.get("secret").and_then(|v| v.as_bool()) == Some(true) {
                    true => format!(
                        "Confirm the press on \"{name}\" is asking for a value under the words \"{label}\", that the box is empty, and that what is typed into it is not drawn back — it is the box a credential goes in."
                    ),
                    false => format!(
                        "Confirm the press on \"{name}\" is asking for a value under the words \"{label}\", and that the box is empty rather than carrying anything typed into it before."
                    ),
                }
            }
            // What the author asked to have drawn, read off the form. A `qr` is its own
            // line: what is on the screen is a picture, and the claim is that Amenbo drew it from a
            // string the author handed over — an image the plugin supplied is exactly what this
            // vocabulary exists instead of, and no reading of words settles which one is standing there.
            (Domain::Plugin, "drawn") => {
                let name = req(with, "name")?;
                let kind = req(with, "kind")?;
                // Where it stands, when the road named a setting it has to stand over. It is a clause on
                // the same line rather than a step of its own: what is being read is one thing on the
                // screen, and an eye given two lines about it reads the first and skims the second.
                let over = match with.get("above").and_then(|v| v.as_str()) {
                    Some(key) => format!(
                        " Confirm it stands above the setting \"{key}\", where its author put it, and not in a block of its own."
                    ),
                    None => String::new(),
                };
                let line = match (kind, with.get("value").and_then(|v| v.as_str())) {
                    ("qr", _) => format!(
                        "Confirm the settings form for \"{name}\" draws a QR code — squares Amenbo has drawn, sharp and square-on, not a picture sitting at some size of its own."
                    ),
                    ("copy", Some(value)) => format!(
                        "Confirm the settings form for \"{name}\" draws \"{value}\" with a button beside it that copies it."
                    ),
                    ("link", Some(value)) => format!(
                        "Confirm the settings form for \"{name}\" draws a button reading \"{value}\", and that it is a button rather than a line of text."
                    ),
                    (kind, Some(value)) => format!(
                        "Confirm the settings form for \"{name}\" draws \"{value}\" as a {kind}, in plain text with no markup of the author's showing through."
                    ),
                    (kind, None) => format!(
                        "Confirm the settings form for \"{name}\" draws a {kind} the plugin asked for."
                    ),
                };
                format!("{line}{over}")
            }
            // The button before the gate is open. What is under test is a control a reader can see and
            // cannot use, so the absence of the button would pass this for the wrong reason — the line
            // says both halves out loud.
            (Domain::Plugin, "press-shut") => format!(
                "Confirm the settings form for \"{}\" draws the operation \"{}\" and will not let it be pressed while the plugin is off, saying as much rather than leaving a reader to find out.",
                req(with, "name")?,
                req(with, "label")?
            ),
            (Domain::Plugin, "detail") => format!(
                "Confirm the panel open under \"{}\" — the row the catalog \"{}\" served — says installing it would mean \"{}\".",
                req(with, "name")?,
                req(with, "source")?,
                req(with, "declares")?
            ),
            // The prose that panel is read by, which is the author's description or the repository's
            // README and never both. The words are quoted whole for the reason a row's line is: the
            // panel says nowhere which of the two it drew, nor which language it drew it in, so the
            // only thing that tells them apart is which sentence is standing there. An absence is
            // written as a line of its own, since an eye sent to confirm one has to be told what it
            // would have been looking at.
            (Domain::Plugin, "body") => {
                let name = req(with, "name")?;
                let source = req(with, "source")?;
                let text = req(with, "text")?;
                match present(with) {
                    true => format!(
                        "Confirm the panel open under \"{name}\" — the row the catalog \"{source}\" served — is read by the words \"{text}\"."
                    ),
                    false => format!(
                        "Confirm the panel open under \"{name}\" — the row the catalog \"{source}\" served — does not carry the words \"{text}\" anywhere in what it is read by."
                    ),
                }
            }
            // The layer, read where a reader meets it: a sentence on the row, or no sentence at all. The
            // two states are written as lines of their own rather than as one with a `not` in it, because
            // what a build gets wrong here is drawing the sentence for everybody — and an eye sent to
            // confirm an absence has to be told what the absence is an absence of.
            (Domain::Plugin, "layer") => {
                let name = req(with, "name")?;
                match req(with, "scope")? {
                    "machine" => format!(
                        "Confirm the row for \"{name}\" among the installed plugins says, in so many words, that it reads every project on this device rather than one — put as a sentence about the one gate that row carries, and not as a second thing to set."
                    ),
                    "project" => format!(
                        "Confirm the row for \"{name}\" among the same installed plugins says no such thing: nothing on it claims to read the device, this being the ordinary plugin whose reach is one project at a time."
                    ),
                    other => return Err(format!("assert `layer` does not know the layer `{other}`")),
                }
            }
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
            // The one gate a machine-wide plugin has, read in its own row. It names no project on
            // purpose: what is open is the device's, and a line naming one would be reading the wrong
            // kind of row.
            (Domain::Plugin, "fires-on-device") => {
                let name = req(with, "name")?;
                match present(with) {
                    true => format!(
                        "Confirm \"{name}\"'s own row for this device says the plugin is on — one row, and no project named anywhere in it."
                    ),
                    false => format!(
                        "Confirm \"{name}\"'s own row for this device offers to turn the plugin on rather than off, which is the gate still shut."
                    ),
                }
            }
            // What that row says about the settings kept there — the same three states `settings-in`
            // reads on a crossing, and asked apart from the gate for the same reason: a row refused an
            // enable over a missing value is marked and off, and one word could not say both halves.
            (Domain::Plugin, "settings-on-device") => {
                let name = req(with, "name")?;
                match req(with, "state")? {
                    "required-empty" => format!(
                        "Confirm \"{name}\"'s own row for this device is marked as owing a setting the plugin cannot be enabled without."
                    ),
                    "open" => format!(
                        "Confirm the settings for \"{name}\" are standing open inside that same row, and that nothing in them asks which project they are for."
                    ),
                    "filled" => format!(
                        "Confirm \"{name}\"'s own row for this device says the settings there are filled in, and that nothing in it still says a required one is empty."
                    ),
                    other => {
                        return Err(format!("assert `settings-on-device` does not know the state `{other}`"))
                    }
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
                    "device" => format!(
                        "Confirm \"{project}\"'s own settings name \"{plugin}\" as the device's own — said apart from the crossings, with no switch of this project's on it, and not among what the picker there offers to add."
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
                "Confirm the first loop is offered here: one move starts a terminal already inside the linked folder, and under it the loop says the tasks will appear on the board. Then open the way out to your own terminal, folded beside that move, and confirm the request it hands over names \"{}\".",
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
            // What the terminal road meets as a refusal, the screen meets as a row. There is no
            // command to turn away here — the interface holds the store open the whole time — so the
            // line the two roads share is the folder being named as another store's, and the row
            // saying it is what stands in for the refusal. The half worth spelling out is the badge:
            // a row that went on calling itself ready to be worked in would be saying the opposite of
            // what happens to anyone who walks in there.
            (Domain::Folder, "claimed") => format!(
                "Confirm the linked-folder list carries the folder the road calls \"{}\" as another store's — it names \"{}\" as the store the folder belongs to, and does not call the folder ready for an AI to work in.",
                req(with, "dir")?,
                req(with, "store")?
            ),
            (Domain::Repo, "ai-launch-notice") => match present(with) {
                true => format!(
                    "Confirm the project's board carries the report about starting its folders' AI on Amenbo, with nothing asked and nothing over it: \"{}\" is named, with \"{}\" as the file its text goes into.",
                    req(with, "tool")?,
                    req(with, "paste_into")?
                ),
                false => format!(
                    "Confirm this project's board is standing with no such report on it: nothing here names \"{}\", and \"{}\" is nowhere on the screen.",
                    req(with, "tool")?,
                    req(with, "paste_into")?
                ),
            },
            // The text on the project's own settings, standing there with nothing being reported. The
            // file is the reading for the reason it is on the report: it is that tool's own and appears
            // nowhere else on the screen, so a picker that changed its label and not the text reads as
            // the miss it is. The line says the board is quiet, because that is the state under test —
            // a way to the text that only opens while something is being reported is the one this road
            // was written against.
            (Domain::Repo, "ai-launch-request") => format!(
                "Confirm this project's own settings hand over the text for \"{}\", with \"{}\" as the file it goes into — standing there while nothing on this project's board is reporting anything to wire.",
                req(with, "tool")?,
                req(with, "paste_into")?
            ),
            // The record, read on the project's own face. Each state is read together with what the way
            // back out of it is doing, since that is the half a reader acts on: an answer is there to be
            // taken back, and where there is none the button that would take it back must be shut.
            (Domain::Repo, "ai-launch-answer") => match req(with, "answer")? {
                "yes" => "Confirm this project's own settings say it answered yes to having its AI started on Amenbo, with the way to clear that answer open.".to_string(),
                "no" => "Confirm this project's own settings say it answered no to having its AI started on Amenbo, with the way to clear that answer open — a refusal takes the report away for good, so this is the only way back.".to_string(),
                "unanswered" => "Confirm this project's own settings say it has not been answered for — neither a yes nor a no — and that there is nothing left to clear.".to_string(),
                other => {
                    return Err(format!("assert `ai-launch-answer` does not know the answer `{other}`"))
                }
            },
            // The way back out, read while there is no answer to take back. The line names how the button
            // is drawn rather than what it does when pressed: a press that goes nowhere is what the state
            // row beside it already says, and the fault this stands against is a button that is shut and
            // says nothing of it — so what the eye is sent to look for is the fade and the pointer the
            // pressable ones on the same screen are wearing, held up against them.
            (Domain::Repo, "ai-launch-consent-clear-shut") => {
                "Confirm the button that clears this project's answer is drawn as one that cannot be \
                 pressed: faded beside the buttons on this screen that can be, and answering the pointer \
                 with neither a hand cursor nor a colour of its own. That it does nothing when pressed is \
                 not the reading — a button shut and drawn like a live one is the state this closes."
                    .to_string()
            }
            // One app's row in the fold. The two halves are said in one line because a reader reads them
            // in one glance — the app is set up, and for this folder — and the folder is where the eye
            // is sent: it is the reader's own path, and the screen has no other reason to draw one.
            (Domain::Repo, "mcp-app") => match (set(with), with.get("dir")) {
                (true, Some(_)) => format!(
                    "Confirm the row for \"{}\" reads as already set up, naming \"{}\" as the folder its entry reaches.{}",
                    req(with, "app")?,
                    req(with, "dir")?,
                    ticked(with)?
                ),
                (true, None) => format!(
                    "Confirm the row for \"{}\" reads as already set up.{}",
                    req(with, "app")?,
                    ticked(with)?
                ),
                // Nothing is named after it: what a row not set up says about a folder is nothing, and a
                // line naming one would send the eye looking for a path that is right to be missing.
                (false, _) => format!(
                    "Confirm the row for \"{}\" reads as not set up, with no folder named beside it.",
                    req(with, "app")?
                ),
            },
            // Which road that row offers. The road is under the fold, so the line asks for the row
            // open — an operator reading a folded one would close this on an absence the screen is
            // right to have. It names the road by what it does rather than by the words on the button,
            // so an eye closing it is held to the two being different offers and not to one build's
            // wording of them.
            (Domain::Repo, "mcp-road") => match req(with, "road")? {
                "file" => format!(
                    "With the row for \"{}\" open, confirm it offers a file to be written and opened — Amenbo's own, not this app's settings — and offers no request to hand an AI.",
                    req(with, "app")?
                ),
                "request" => format!(
                    "With the row for \"{}\" open, confirm it offers a request to hand this app's own AI, which makes the edit — and offers no file to be written.",
                    req(with, "app")?
                ),
                other => return Err(format!("assert `mcp-road` does not know the road `{other}`")),
            },
            // That road with nothing ticked to send down it. The line names how the button is drawn
            // rather than what a press does, the way the other shut way out is read: a press that goes
            // nowhere is not the reading, and a button shut while drawn like a live one is the state
            // this closes. It says which button by what it does, since which one a row carries is the
            // catalog's word.
            (Domain::Repo, "mcp-road-shut") => {
                let which = match req(with, "road")? {
                    "file" => "writes the file",
                    "request" => "takes the request",
                    other => {
                        return Err(format!("assert `mcp-road-shut` does not know the road `{other}`"))
                    }
                };
                format!(
                    "With the row for \"{}\" open and no project ticked on it, confirm the button that \
                     {which} is drawn as one that cannot be pressed: faded beside the buttons on this \
                     screen that can be, and answering the pointer with neither a hand cursor nor a \
                     colour of its own. What is ticked is what goes over, so an empty row has nothing \
                     to hand anybody — and a live button here hands one on.",
                    req(with, "app")?
                )
            }
            // The file taken the rest of the way: into the app it was written for, and read there. The
            // line carries what the operator has to have in place before the step can be walked at all,
            // because none of it is Amenbo's to stand up — an app that has not been updated, or has
            // nobody signed in, draws no servers whatever the file says, and the step would read as a
            // format Amenbo got wrong.
            //
            // It also asks for the shot, which no other step has to: every other one is closed by the
            // run's own shot of the build under test, and the window that settles this one belongs to
            // another program.
            (Domain::Repo, "mcp-in-app") => format!(
                "Take the file this road just offered into \"{}\"'s own settings — merging it with what \
                 is already there — and start that app again. Answer nothing it does not ask you to: \
                 the folders arrive with the file, and a build that made you fill them in is the miss \
                 this step is watching for. With it updated and signed in beforehand, confirm Amenbo \
                 stands among the servers it lists and the tool \"{}\" is under it. Shoot that app's \
                 window yourself and keep the picture with this run: the shot taken here is of Amenbo, \
                 which is not where the answer is.",
                req(with, "app")?,
                req(with, "tool")?
            ),
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
                "Confirm this project's own settings list the folder \"{}\" among the ones still starting their AI without Amenbo — in that list, not the one of folders bound to the project, and standing there whatever notice the board was carrying.",
                req(with, "dir")?
            ),
            // Which of the three answers the form is holding. The state is the whole question here:
            // the value a screen shows is its ticks, and two of the three answers leave every box
            // clear — so a line naming only the value would pass over the one difference this reads.
            (Domain::Plugin, "config") => {
                let name = req(with, "name")?;
                let key = req(with, "key")?;
                refuse_named_crossing(with, "config")?;
                // A field whose value the plugin wrote back. What is read here is an
                // absence — no box, no button — so the value has to be standing there while it is read:
                // an empty field would draw neither of them either, and would say nothing about whether
                // this one is out of reach.
                if with.get("readonly").and_then(|v| v.as_bool()) == Some(true) {
                    return Ok(format!(
                        "Confirm the setting \"{key}\" in \"{name}\"'s settings shows \"{}\" as words on the screen, with no box to type into and no button beside it that empties it.",
                        req(with, "equals")?
                    ));
                }
                // A plain line, read back as the box holds it. It is asked with a word of its own rather
                // than with the `equals` a choice takes, because the two are different readings: a
                // choice's answer is ticks and needs the state to say which of the three it is, while a
                // typed line is either standing in its box or it is not. What a road wants this for is a
                // value that had to *survive* something — a check that refused it after the save, a form
                // redrawn — so what is named is the value, not merely that something is there.
                if let Some(value) = arg_str(with, "holds") {
                    return Ok(format!(
                        "Confirm the box for the setting \"{key}\" in \"{name}\"'s settings holds \"{value}\"."
                    ));
                }
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
            // The tick's band, standing or gone. Nothing is pressed here and nothing is opened —
            // the band comes up on its own while its three conditions hold, so the line is the
            // screen as the app left it, and the absent half is read after one of them has gone.
            (Domain::Tick, "banner") => match present(with) {
                true => "Confirm the band offering to watch due dates is standing across the app — it came up by itself, and it carries three buttons: one that starts the checking, one that declines it, one that puts it off."
                    .to_string(),
                false => "Confirm no band offering to watch due dates is anywhere on the screen."
                    .to_string(),
            },
            // Where the hourly check's own row stands. The two positions are named by what each
            // holds rather than by the word drawn on it, since the words are the interface's own —
            // and off is also where a device nobody asked stands, the row having two positions over
            // the answer's three states.
            (Domain::Tick, "setting") => match req(with, "position")? {
                "on" => "Confirm the hourly check's row in Amenbo's own settings stands in the position that has the check on — the one that holds a yes and a registered timer."
                    .to_string(),
                "off" => "Confirm the hourly check's row in Amenbo's own settings stands in the position that has the check off — the one that holds no registration."
                    .to_string(),
                other => {
                    return Err(format!("assert `setting` does not know the position `{other}`"))
                }
            },
            // Where the file face's own settings row stands. The positions are named by what each
            // does rather than by the word drawn on the row, since the words are the interface's own.
            (Domain::Files, "setting") => match req(with, "position")? {
                "asks" => "In Amenbo's own settings, under the section about files, confirm the row for the question before binning stands in the position that has the panel ask."
                    .to_string(),
                "quiet" => "In Amenbo's own settings, under the section about files, confirm the row for the question before binning stands in the position that has the panel not ask."
                    .to_string(),
                other => {
                    return Err(format!("assert `setting` does not know the position `{other}`"))
                }
            },
            // The key that field is holding, read back. It is the one line on these roads whose word is
            // neither the interface's nor a title — a key is what a reader types for somewhere outside
            // Amenbo — so it is on the shot exactly once, in the field it was typed into.
            (Domain::Dimension, "key") => {
                let dimension = req(with, "dimension")?;
                let want = req(with, "equals")?;
                match arg_str(with, "value") {
                    Some(value) => format!(
                        "In the categories manager, confirm the field beside the value \"{value}\" under \"{dimension}\" holds the key \"{want}\"."
                    ),
                    None => format!(
                        "In the categories manager, confirm the field beside the name of \"{dimension}\" holds the key \"{want}\"."
                    ),
                }
            }
            // What the pane is showing. The words are the road's own, typed into that terminal by an
            // earlier step, so a reading that finds them found the pane drawing that session — and
            // the absent half is read with the ledger up, where the pane is hidden rather than gone.
            // What the pane's label carries. It is the row above the pane rather than the pane itself:
            // the words got there because the app read them out of the drop box the agent wrote to,
            // never because they were printed in the terminal.
            (Domain::Terminal, "label") => match present(with) {
                true => format!(
                    "On the row above the pane running a terminal, confirm the label carries \"{}\" — said in the pane, read off the screen.",
                    req(with, "shows")?
                ),
                false => format!(
                    "On the row above the pane running a terminal, confirm the label carries nothing saying \"{}\".",
                    req(with, "shows")?
                ),
            },
            // The dot on the terminal's own segment. It is read from the ledger, which is the only
            // face it is ever drawn on, and it carries nothing to quote: a road says it is there, or
            // that crossing over has spent it.
            (Domain::Terminal, "face-badge") => match present(with) {
                true => "In the pair of segments at the top of the window, confirm the one that shows the terminal is wearing a small mark — a dot, with no number and no words on it. It says a turn came up behind the face you are not looking at."
                    .to_string(),
                false => "In the pair of segments at the top of the window, confirm the one that shows the terminal is wearing no mark at all."
                    .to_string(),
            },
            // What is standing where a person types, with nothing run. Both halves are said: the words
            // being there, and the line not having gone — a build that sent the newline would draw
            // whatever the program did with it, and the input line would be empty again.
            (Domain::Terminal, "in-the-box") => {
                let pane = match arg_str(with, "on") {
                    Some(on) => format!("the pane showing \"{on}\""),
                    None => "the pane that has a terminal running in it".to_string(),
                };
                match present(with) {
                    true => format!(
                        "On {pane}, confirm \"{}\" is standing in the line you would type into — and that it has not been sent: nothing ran, and the words are still there to be edited.",
                        req(with, "shows")?
                    ),
                    false => format!(
                        "On {pane}, confirm \"{}\" is not in the line you would type into.",
                        req(with, "shows")?
                    ),
                }
            }
            // Which pane the reader is in, read as one thing off two marks. The frame says the face's
            // answer and the cursor says the browser's, and a road that read only the first would go
            // green on a pane that is drawn picked out and takes none of the typing.
            (Domain::Terminal, "worked-in") => format!(
                "Confirm the pane showing \"{}\" is the one being worked in: its frame is the one drawn picked out from the rest, and the block where you would type in it is filled in — the pane the keyboard is not in draws that block as an outline.",
                req(with, "shows")?
            ),
            (Domain::Terminal, "pane") => match present(with) {
                true => format!(
                    "Confirm the line \"{}\" is on the screen, on the pane that printed it — the same pane, drawn here. What is on a pane stays where it was printed, so a pane whose terminal has since ended still carries it.",
                    req(with, "shows")?
                ),
                false => format!(
                    "Confirm the line \"{}\" is nowhere on the screen — the pane that drew it is not being shown, the ledger being up, another page being the one on screen, the face being on another project's panes, or that pane having been taken away for good.",
                    req(with, "shows")?
                ),
            },
            // The lamp on a pane's label, on one of its three faces. Two of them hold still and are a
            // picture; the third blinks, and that one is watched rather than shot — both ends of its
            // turn rest at a step a photograph cannot tell from the others.
            //
            // Each half of the instruction says what to look for on a machine set to play no
            // animation as well, because motion turned down holds the calling face at its brightest
            // instead of moving it: the fact survives and the word for it does not, and an operator
            // told only to watch for a blink would fail a lamp reporting exactly what was asked.
            (Domain::Terminal, "dot") => match face(with)? {
                Face::Lit => format!("{LAMP_ROW} look at the lamp to the left of the name: confirm it is lit and holding still — a soft glow around it, in that pane's own colour, which is that terminal putting something out. It does not fade in and out: the one face that moves is the one calling for a person, and this is not it."),
                Face::Calling => format!("{LAMP_ROW} watch the lamp to the left of the name for a few seconds: confirm it is blinking, and in the warning colour rather than that pane's own — which is that pane asking for a person. It falls to the same beat as the mark at the other end of the same row, and the two go together. Judge it by watching rather than by the shot: a still picture of a blink can be caught at the moment it rests. Where the machine is set to play no animation the lamp does not blink at all, and what to confirm there is the warning colour, held at its brightest."),
                Face::Out => format!("{LAMP_ROW} look at the lamp to the left of the name: confirm it is sunk — dim, in that pane's own colour, with no glow around it and no blinking. Out is the pane's resting state, not the pane having gone: the lamp is drawn either way."),
            },
            // The question about where a pane runs, read by a folder it offers. The absent half is the
            // one the walking-away is proved by, and it is written to say what a screen with nothing
            // on it means here: the question is the box, so a face drawing neither it nor a pane is a
            // question that took its box with it.
            (Domain::Terminal, "asking-folder") => match present(with) {
                true => format!(
                    "Confirm the question about which folder this pane works in is standing where the pane would be, and that \"{}\" is one of the folders it offers.",
                    req(with, "dir")?
                ),
                false => format!(
                    "Confirm the question about which folder a pane works in is nowhere on this screen — no box offering \"{}\", and nothing half-made standing where it was.",
                    req(with, "dir")?
                ),
            },
            // Whether a column is beside the panes. The absent half says what the width went to, so an
            // operator reading it knows a screen that merely drew the column narrower would be a fail.
            (Domain::Terminal, "side") => {
                let which = side(with)?;
                match present(with) {
                    true => format!(
                        "On the terminal face, confirm {} is beside the panes, taking width of its own.",
                        which.phrase()
                    ),
                    false => format!(
                        "On the terminal face, confirm {} is nowhere on the screen — not narrowed, not emptied, gone — and that the panes have spread into the width it was using.",
                        which.phrase()
                    ),
                }
            }
            // And where its edge stands now. It is read against the shot before it rather than
            // against a number: what the drag is asked to have done is move this edge and nothing
            // else, and the two pictures side by side are what say so.
            (Domain::Terminal, "side-width") => {
                let which = side(with)?;
                let (moved, gave) = match flag(with, "wider")? {
                    true => ("wider than it was on the shot before this one", "narrower by that much"),
                    false => ("narrower than it was on the shot before this one", "wider by that much"),
                };
                format!(
                    "Confirm {} is {}, and that the panes beside it are {} — the edge moved, and nothing else on the face did.",
                    which.phrase(),
                    moved,
                    gave
                )
            }
            // What the empty frame is set to open with, read on the row above its press. The row is
            // every agent Amenbo knows how to start, and which of them this machine has is the
            // machine's own business: the ones it has not got are folded away behind a
            // press that says how many, and nothing behind that press can be chosen. So the reading
            // is about what is **on**, never about what is on the row — an operator told to expect
            // one shape of row would mark a working face red on the machine that draws the other.
            //
            // The button is read beside it. Nothing on the row being on is the one state that stops
            // the press, and it is what a build that forgot the choice would fall back to — so a step
            // that looked only at which name is lit could pass on a frame that opens nothing.
            (Domain::Terminal, "opens-with") => match req(with, "start")? {
                "shell" => "On the terminal face, look at the empty frame — the box on the page that is not a terminal — and at the row above the press that opens a pane in it: confirm the plain shell is the one that is on, and that the press is live rather than asking to be told what to open with. What else is on the row is nothing to this reading: a press saying how many agents this machine has not got is left folded, and on a machine that can start none the plain shell may be the only thing there is to choose."
                    .to_string(),
                // The first run, which is a state and not a program: nobody has said, so the frame
                // says so instead of guessing. Both halves are read because either alone passes on
                // a build with the other fault — a row with nothing lit above a press that opens
                // anyway is a build guessing quietly, and a press that asks with a name already lit
                // is a build asking about an answer it has.
                //
                // The row has to be there to be read blank, and that is said out loud: this is the
                // one reading here a machine cannot be relied on to be able to give, and the road
                // that asks for it stood the machine up first (`can-start`).
                "none" => "On the terminal face, look at the empty frame — the box on the page that is not a terminal — and at the row above the press that opens a pane in it: confirm the row is drawn, with several things on it to open with, and that **none of them is on**. The press below it does not open a pane: it asks to be told what to open with, and will not answer until one of the row is chosen. Leave folded any press saying how many agents this machine has not got — what is behind it cannot be chosen and is not part of this reading. Nothing is chosen here — that is the next step's — and if a name on that row is already on, this step has failed."
                    .to_string(),
                other => return Err(format!(
                    "assert `opens-with` cannot name `{other}` — the plain shell is the one thing every machine's row has, `none` is nobody having chosen yet, and which agents are on the row is that machine's own"
                )),
            },
            // A registered command as the frame draws it, name and line together. The line is read
            // character for character rather than recognised: a build that tidied it — trimmed the
            // quotes, dropped the arguments, rebuilt it from the first word — would draw something a
            // reader would still call the right one, and it is the one thing this reading exists to
            // catch.
            (Domain::Terminal, "registered") => format!(
                "On the empty frame, look under the row of things a pane can be opened with, at the list of the commands registered on this machine. Confirm one of them is called \"{}\" and that the line drawn beside it reads `{}` — the same characters in the same order, with nothing added, tidied or left out.",
                req(with, "name")?,
                req(with, "line")?,
            ),
            // The opening sentence, arrived in a pane whose launch line Amenbo did not compose, and
            // sent. It is written as a wait as much as a reading: the sentence goes in after the
            // program has drawn something and is submitted a moment later, so an operator who looked
            // the instant the pane opened would be reading a screen the app has not finished with.
            //
            // The marked line is the whole of it, and the instruction says why: what the pane echoes
            // proves only that Amenbo wrote into it, and the road's registered line is what puts the
            // sentence back on the screen with a word of the road's own in front of it.
            //
            // The absent half is a wait and nothing else, and how long is said out loud: the app
            // gives up after a minute, and an operator who looked away sooner would be reading a
            // screen that is still being tried. What it must not find is written as a line rather
            // than as a state, because the fault it catches leaves a mark and does not take one
            // away.
            (Domain::Terminal, "handed-over") => match present(with) {
                true => format!(
                    "In the pane the registered command is running in, wait a few seconds and then confirm Amenbo's opening sentence — the fixed English one, beginning \"Before you act on any request in this directory\" — is on a line the program gave back: the one marked \"{}\". Nothing is typed here; the sentence is put in and sent by Amenbo itself. The marked line is the reading — the pane shows the sentence as it goes in whether or not it was ever sent, and only the program giving it back says it was.",
                    req(with, "given-back")?
                ),
                false => format!(
                    "In the pane the registered command is running in, wait a full minute — that is how long Amenbo goes on trying — and then confirm no line marked \"{}\" is anywhere on it. The program hands back every line it is given, so a marked line would be a newline Amenbo sent into a pane that had shown it nothing, which is the one thing being read here. Nothing is typed during the wait, and the pane looking untouched apart from what the command printed at the start is the pass.",
                    req(with, "given-back")?
                ),
            },
            // What the row says once the hand-over has given up. It is described rather than quoted:
            // these are the interface's own words, drawn in the machine's language, and the operator
            // is told what the row means rather than which letters to find.
            (Domain::Terminal, "unsent") =>
                "On the row above that pane, confirm it is now saying the opening sentence was not sent — words to the effect that it has not been sent yet and that pressing return sends it, in the language the machine is set to, with a pause mark in front of them. It is the row above the pane and never the pane itself: nothing Amenbo says goes into a terminal it is reading. Confirm too that the row is still naming the pane and what it is on, and has not been given over to this alone."
                    .to_string(),
            // How many panes are standing on the page. Counted rather than read: the boxes carry no
            // words of the road's, and the whole of what this asks is how many of them there are.
            //
            // What is ruled out is a page that filled its count with boxes, so the wording says what
            // may stand beside the panes and what may not: **one** empty frame where the page has
            // room, never a second, and nothing at all where the panes fill the count. An operator
            // who was told only to count the panes would pass on the screen this exists to catch.
            (Domain::Terminal, "frames") => {
                // What may be standing beside the panes. Said exactly where the road says so, and as
                // "at most one" where it does not: a page with room draws its one way in at the first
                // gap, and a page whose panes fill the count has no room to offer and must draw none.
                //
                // A full page is not bare all the same. With no gap to draw a frame in, the way in is
                // a strip on the right edge instead — thin enough to cost no pane its place — and an
                // operator told the page must hold nothing but panes would mark a working face red.
                let beside = |them: &str| match with.get("empty").and_then(|v| v.as_u64()) {
                    None => Ok(format!(
                        "Beside {them} there is at most one empty frame — never a second — and the rest of the page is bare."
                    )),
                    Some(0) => Ok(format!(
                        "Beside {them} there is no empty frame at all: the panes fill what this page draws, so it has no room to offer. What stands beside them is the thin way in down the right edge of the page — the strip that opens another pane — and nothing else."
                    )),
                    Some(1) => Ok(format!(
                        "Beside {them} there is exactly one empty frame, at the first gap in the page — never a second — and the rest of the page is bare."
                    )),
                    Some(n) => Err(format!(
                        "assert `frames` cannot ask for {n} empty frames — a page draws one at its first gap or none at all"
                    )),
                };
                match count(with, "count")? {
                    // A page with nothing standing on it always has room, so its way in is always
                    // drawn: asking for a page with neither is asking for a screen there is no way to
                    // reach, and an operator who went looking would find the empty frame and be right.
                    0 if with.get("empty").and_then(|v| v.as_u64()) == Some(0) => return Err(
                        "assert `frames` cannot ask for a page with no panes and no empty frame — a page with room always draws its one way in".to_string()
                    ),
                    0 => "On the terminal face, confirm no pane is standing on the page at all — no terminal anywhere on it. What is on it is one empty frame, or, while it is standing, the question about where a pane runs — one box and no more, with the rest of the page bare."
                        .to_string(),
                    1 => format!(
                        "On the terminal face, confirm exactly one pane is standing on the page. {}",
                        beside("it")?
                    ),
                    n => format!(
                        "On the terminal face, count the panes standing on the page: confirm there are exactly {n}. {}",
                        beside("them")?
                    ),
                }
            }
            // ── the file face ─────────────────────────────────────────────────────────────────
            (Domain::Files, "listed") => match present(with) {
                true => format!(
                    "In {}, confirm \"{}\" is one of the rows.",
                    section(with)?,
                    req(with, "name")?
                ),
                false => format!(
                    "In {}, confirm \"{}\" is not among the rows — the section is drawn, and this is not on it.",
                    section(with)?,
                    req(with, "name")?
                ),
            },
            (Domain::Files, "read-as") => match present(with) {
                true => format!(
                    "On the row the open file is named on, confirm what says how it was read now names \"{}\".",
                    req(with, "encoding")?
                ),
                false => format!(
                    "On the row the open file is named on, confirm what says how it was read does not name \"{}\".",
                    req(with, "encoding")?
                ),
            },
            // A picture is read for the words drawn in it, which is the one thing about a redrawn
            // picture a shot can carry: what a road wants to know is that the bytes on screen are the
            // new bytes, and two pictures that say different words answer that where two that differ
            // only in their pixels would need an eye.
            (Domain::Files, "reading") if picture(with) => match present(with) {
                true => format!(
                    "Confirm the picture drawn in the panel has \"{}\" written across it.",
                    req(with, "shows")?
                ),
                false => format!(
                    "Confirm the picture drawn in the panel does **not** have \"{}\" written across it.",
                    req(with, "shows")?
                ),
            },
            (Domain::Files, "reading") if with.contains_key("as") => match form(with, "as")? {
                "source" => format!(
                    "Confirm the opened file shows \"{}\" as the text it is: the words stand in one plain size with the marks around them — a hash before a heading, and whatever else the file was written with — visible as characters.",
                    req(with, "shows")?
                ),
                _ => format!(
                    "Confirm the opened file shows \"{}\" as what the text says: no hash before it, and drawn as the heading it marks rather than in the size of the lines below.",
                    req(with, "shows")?
                ),
            },
            (Domain::Files, "reading") => match present(with) {
                true => format!(
                    "Confirm the opened file shows \"{}\" — the words that are in it.",
                    req(with, "shows")?
                ),
                false => format!(
                    "Confirm \"{}\" is nowhere on the screen — what is in the file did not reach it.",
                    req(with, "shows")?
                ),
            },
            // What git says about a row. The mark is named by the state rather than by the colour, so
            // the eye is told what to look for in words that outlive a palette.
            (Domain::Files, "row-mark") => match present(with) {
                true => format!(
                    "In {}, confirm the row \"{}\" is drawn in the colour this build gives to {} — a colour and not a word, so read it against the rows beside it that git says nothing about.",
                    section(with)?,
                    req(with, "name")?,
                    mark(with)?
                ),
                false => format!(
                    "In {}, confirm the row \"{}\" wears no colour of its own — it is drawn like the rows git says nothing about, and not in the colour that would say it is {}.",
                    section(with)?,
                    req(with, "name")?,
                    mark(with)?
                ),
            },
            (Domain::Files, "says") => match present(with) {
                true => format!("Confirm the column says {}.", note(with)?),
                false => format!("Confirm the column does not say {}.", note(with)?),
            },
            // What the hand-over left. Every one of the three is a `Review`, and for the same reason:
            // what settles it is not on Amenbo's window, which is the window the run shoots. The
            // operator standing at the screen is the one who saw it, so the line asks them for it.
            (Domain::Files, "handed-over") => match door(with)? {
                "usual" => "Confirm the machine took the file: the menu has gone, something came forward with the file open in it, and Amenbo drew nothing about the hand-over. Which application came forward, and what it does with the file, is not this road's — go no further than something having opened. The shot is of Amenbo's own window rather than of what came forward, so say what you saw."
                    .to_string(),
                "pick" => "Confirm something to choose an application from is on the screen. Which shape it takes belongs to the machine — on some it is a list of applications drawn where the menu was, on others the operating system's own chooser — and either one is right: what is read here is that a choice arrived, never who drew it. Leave it without choosing, and say which of the two you saw."
                    .to_string(),
                _ => "Confirm the file manager came forward with the file standing out in the folder it is in. Go no further into it — that the file reached it is the whole of this reading — and, the shot being of Amenbo's own window, say what you saw."
                    .to_string(),
            },
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

/// One axis and one value, written the way the `--filter` grammar writes them — which is the one
/// name a chip and a terminal share, since the chip itself reads in whatever language the app was
/// started in. The two builtin keys join with a colon (`status:todo`); a classification axis is
/// already a `dim:`-prefixed key and takes its value after `=` (`dim:theme=main`), because the axis
/// there is the user's own word and the grammar needs the two halves told apart.
fn filter_pair(axis: &str, value: &str) -> String {
    let sep = if axis.starts_with("dim:") { '=' } else { ':' };
    format!("{axis}{sep}{value}")
}

fn unmapped(domain: Domain, op: &str) -> String {
    format!(
        "op `{op}` for domain `{domain:?}` is in the scenario registry but not yet mapped in the GUI harness"
    )
}

fn arg_str<'a>(with: &'a Args, key: &str) -> Option<&'a str> {
    with.get(key).and_then(|v| v.as_str())
}

/// The word a settings step down the other pipe has to carry, turned away here rather than quietly
/// passing over.
///
/// A setting is held per crossing, and a terminal says which crossing by standing in a folder bound to
/// it — so `project` is how a road there names one. On screen the settings are opened inside the row
/// where the plugin crosses the project, and that row has already answered the question; a step naming
/// it again would either be asking for a second picker there is none of, or telling a form to write
/// somewhere the operator was never sent. Ignoring the word would hide both.
fn refuse_named_crossing(with: &Args, op: &str) -> Result<(), String> {
    match arg_str(with, "project") {
        None => Ok(()),
        Some(project) => Err(format!(
            "`project: {project}` is how a terminal says which crossing a setting is held at, so it \
             belongs on a `steps_cli` road — on screen the row the settings are opened inside has \
             already answered it, and `{op}` here is about the form standing in that row"
        )),
    }
}

/// The words on screen for what an action made — what a later step's `target:` has to be read back
/// as. A record is written under a `title`, and the two things that hold records (a project, a
/// dimension's axis) under a `name`; a remark is written under neither, and what it says is the whole
/// of how an operator picks it out of a timeline, so its `text` answers for it. An action that wrote
/// none of the three has no label to offer.
fn label(with: &Args) -> Option<&str> {
    arg_str(with, "title").or_else(|| arg_str(with, "name")).or_else(|| arg_str(with, "text"))
}

/// The name the file an attach hangs has to carry, which is the whole of what the road needs of it:
/// what a search reaches of an attachment is what it is called, never its bytes.
///
/// The file itself is the operator's to bring, and that is the one thing this move cannot be handed.
/// Both ways in on screen — the picker and the drop — read the disk the operator is sitting at, and
/// nothing a run lays down is anywhere either of them is pointed. A link is turned away for the other
/// half of the same fact: those two ways in take bytes, and the screen offers no face that takes an
/// address.
fn file_named(with: &Args) -> Result<&str, String> {
    if with.contains_key("url") {
        return Err(
            "`url` hangs a link, and the screen has no way in for one — the picker and the drop both take a file"
                .to_string(),
        );
    }
    req(with, "file")
}

fn req<'a>(with: &'a Args, key: &str) -> Result<&'a str, String> {
    arg_str(with, key).ok_or_else(|| format!("arg `{key}` must be a string"))
}

/// The names a step lists under `key`, as one clause an operator reads off. It is a list because what
/// it stands for is a selection, and the words are the reader's own — project names, which the screen
/// draws and nothing here can shorten.
fn names(with: &Args, key: &str) -> Result<String, String> {
    let listed = with
        .get(key)
        .and_then(|v| v.as_sequence())
        .ok_or_else(|| format!("arg `{key}` is a selection, so it is a list"))?;
    let words: Vec<&str> = listed
        .iter()
        .map(|one| one.as_str().ok_or_else(|| format!("every entry under `{key}` must be a name")))
        .collect::<Result<_, _>>()?;
    match words.as_slice() {
        [] => Ok(" none of them".to_string()),
        words => Ok(format!(" {}", words.join(", "))),
    }
}

/// The projects a row is read as having ticked, where the road says which — an empty tail where it
/// does not, since a row's selection is not what every reading of it is about.
fn ticked(with: &Args) -> Result<String, String> {
    match with.get("projects") {
        None => Ok(String::new()),
        // The ticks are under the fold, unlike the two halves the row says folded — so a step asking
        // for them asks for that row open. Nothing is said about shutting it again: the screen comes
        // back folded whenever it is opened afresh, which `mcp-open` already carries.
        Some(_) => Ok(format!(
            " With that row open, confirm the projects ticked on it are exactly these, and no others:{}.",
            names(with, "projects")?
        )),
    }
}

/// What a smart view's row stands for, said without its label. The sidebar is translated, so a road
/// that named the wording would be held to whichever language the run was started in — while what the
/// row is for is the same in all of them.
fn view_row(id: &str) -> Result<&'static str, String> {
    Ok(match id {
        "inbox" => "the smart view that gathers what is waiting on the reader",
        "due" => "the smart view that stands for the days work is due on",
        "activity" => "the smart view that runs everything that has happened",
        other => {
            return Err(format!("`view: {other}` is not a smart view the sidebar draws"))
        }
    })
}

/// The step a warning is drawn on, in the ladder's own words and in the colour that carries them. Both
/// halves are said: the step is what the road means, and the colour is what the eye closing the shot
/// has to find.
fn warn_step(step: &str) -> Result<&'static str, String> {
    Ok(match step {
        "stop" => "the stop step, drawn in red — its day has gone, or its day is today",
        "heed" => "the heed step, drawn in amber — its day is tomorrow",
        other => return Err(format!(
            "`step: {other}` is not a step a row warns on (stop / heed)"
        )),
    })
}

/// A count a step names, which is a number and not a word — YAML types it, and a road that wrote it
/// quoted would arrive here as a string nothing could compare.
fn count(with: &Args, key: &str) -> Result<u64, String> {
    with.get(key)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| format!("arg `{key}` must be a whole number"))
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

/// How long an armed word waits before it is said, in seconds.
///
/// It is a number a person has to beat with one press, so it is neither tight nor generous: long
/// enough to cross a switch without hurrying, short enough that a road does not stand still. The road
/// does not name it — what a road says is that the word arrives from behind the other face.
const SAY_AWAY_SECONDS: u32 = 15;

/// How long a pane set printing goes on putting something out.
///
/// It answers to both ends of the one road it is on. Long enough that the lamp is still lit after a
/// second pane has been opened beside it and the row has been looked at; short enough that the step
/// waiting for it to stop is a pause inside a road rather than a break from one. The road does not
/// name it — what a road says is that the pane keeps printing and then does not.
const KEEP_PRINTING_SECONDS: u32 = 30;

/// An optional yes-or-no argument, false where it was not written. Unlike [`present`], whose default
/// is the half most asserts want, these ask for a shape a step takes only when it says so.
fn flagged(with: &Args, key: &str) -> bool {
    with.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Which way a step about a folder is going. Opening is the ordinary ask — a road reaches down a
/// tree far more often than it puts one away — so a step that says nothing is opening one.
fn unfolds(with: &Args) -> bool {
    with.get("open").and_then(|v| v.as_bool()).unwrap_or(true)
}

/// A required yes-or-no argument. It is not `present`'s neighbour: that one has a default because most
/// asserts want one, and a value the op requires has none to fall back to.
fn flag(with: &Args, key: &str) -> Result<bool, String> {
    with.get(key)
        .and_then(|v| v.as_bool())
        .ok_or_else(|| format!("arg `{key}` must be true or false"))
}

/// Which of the two columns beside the panes a step is about.
///
/// They are named by what each holds rather than by any heading, for the reason the sections below
/// are: what is written on them is the interface's own words, and the run's language is whatever the
/// machine is set to.
#[derive(Clone, Copy)]
enum Side {
    Rail,
    Files,
}

impl Side {
    /// The words an instruction is built around, written to fit after "confirm" and after "between".
    fn phrase(self) -> &'static str {
        match self {
            Side::Rail => "the list of panes down one side",
            Side::Files => "the panel beside the panes",
        }
    }
}

/// Which pane's row the lamp is read on, said the same way for all three faces.
///
/// A road can have more than one pane on the screen by the time it reads one, and the lamp says
/// nothing about which pane it belongs to that an operator could use to pick it out — the hue does,
/// but only against the other lamps. So the step points at the pane by what the road has been doing
/// rather than by anything on the screen: the one the steps before it were about.
const LAMP_ROW: &str = "On the row above the pane the steps before this one were about — where another pane has been opened since, it is still that first one —";

/// Which of the lamp's three faces a step is reading (`app/src/talk/nameplate.ts`).
#[derive(Clone, Copy)]
enum Face {
    Lit,
    Calling,
    Out,
}

fn face(with: &Args) -> Result<Face, String> {
    match with.get("face").and_then(|v| v.as_str()) {
        Some("lit") => Ok(Face::Lit),
        Some("calling") => Ok(Face::Calling),
        Some("out") => Ok(Face::Out),
        Some(other) => Err(format!("`face` does not know `{other}` — it is lit, calling or out")),
        None => Err("arg `face` must say which of the three faces the lamp is on".to_string()),
    }
}

fn side(with: &Args) -> Result<Side, String> {
    match with.get("side").and_then(|v| v.as_str()) {
        Some("rail") => Ok(Side::Rail),
        Some("files") => Ok(Side::Files),
        Some(other) => Err(format!("`side` does not know `{other}` — it is rail or files")),
        None => Err("arg `side` must say which of the two columns".to_string()),
    }
}

/// Which of the file face's sections a row is being looked for in, as a phrase an instruction can be
/// built around. It is named by what it is about because its heading is the interface's own words,
/// and the run's language is whatever the machine is set to.
///
/// **There is one left.** The section for what had changed lately is gone — what it answered was
/// "yesterday", and what git says now goes on the tree's own rows instead. The arg stays because it
/// is the one place a road says which part of the panel it means, and the panel is not finished
/// growing.
fn section(with: &Args) -> Result<&'static str, String> {
    match with.get("section").and_then(|v| v.as_str()) {
        Some("tree") => Ok("the folder's own section"),
        Some(other) => Err(format!("`section` does not know `{other}` — it is tree")),
        None => Err("arg `section` must say which section".to_string()),
    }
}

/// Which of a Markdown file's two forms a step is about — what the text says, or the text itself.
/// Named by the form and not by the word on the control, for the reason `section` and `note` are: the
/// switch says where it goes rather than where it is, and the run's language is the machine's.
///
/// The key is the caller's because the two sides of the same question read differently: the move says
/// the `form` to end in, and the reading says what the words are standing `as`.
/// Whether a reading is of a picture rather than of a file's text. It is `as` like the other two
/// forms and not an op of its own, because what is being asked is still "what does the opened file
/// show" — but it is the only one of the three a shot can judge, so it is told apart here rather
/// than in [`form`], which answers for the pair that are about text.
fn picture(with: &Args) -> bool {
    with.get("as").and_then(|v| v.as_str()) == Some("picture")
}

fn form(with: &Args, key: &str) -> Result<&'static str, String> {
    match with.get(key).and_then(|v| v.as_str()) {
        Some("rendered") => Ok("rendered"),
        Some("source") => Ok("source"),
        Some(other) => Err(format!("`{key}` does not know `{other}` — it is rendered or source")),
        None => Err(format!("arg `{key}` must say which of the two forms")),
    }
}

/// Which of the three things git's answer is folded into a step is about. Named by the state and not
/// by the colour drawn for it, for the reason `section` and `note` are named as they are: what a row
/// is drawn in belongs to the theme, and a road naming a colour would go red the day one moved.
fn mark(with: &Args) -> Result<&'static str, String> {
    match with.get("mark").and_then(|v| v.as_str()) {
        Some("untracked") => Ok("something git has never seen"),
        Some("added") => Ok("something staged as new"),
        Some("modified") => Ok("something that has changed since git last recorded it"),
        Some(other) => {
            Err(format!("`mark` does not know `{other}` — it is untracked, added or modified"))
        }
        None => Err("arg `mark` must say what git says about the row".to_string()),
    }
}

/// Which of the three ways out of a file's menu a step is about. They are named by what each hands the
/// file to rather than by the item's words, for the reason `section` and `note` are: the words are the
/// interface's own, and the run's language is whatever the machine is set to.
fn door(with: &Args) -> Result<&'static str, String> {
    match with.get("door").and_then(|v| v.as_str()) {
        Some("usual") => Ok("usual"),
        Some("pick") => Ok("pick"),
        Some("manager") => Ok("manager"),
        Some(other) => Err(format!("`door` does not know `{other}` — it is usual, pick or manager")),
        None => Err("arg `door` must say which of the three ways out of the menu".to_string()),
    }
}

/// Which of the two items that open a naming box a step is about, named by what it makes rather than
/// by the item's words, for the reason `door` and `section` are named that way.
fn made(with: &Args) -> Result<&'static str, String> {
    match with.get("as").and_then(|v| v.as_str()) {
        Some("file") => Ok("file"),
        Some("folder") => Ok("folder"),
        Some(other) => Err(format!("`as` does not know `{other}` — it is file or folder")),
        None => Err("arg `as` must say which of the two the item makes".to_string()),
    }
}

/// What a folder being dragged in has to have in it, said at the hand-over so the operator brings one
/// that does. It is the folder case's alone — a file is read by its own name — and it renders to
/// nothing when a step names none, which is every drop of a single file.
fn holding(with: &Args) -> String {
    match with.get("holding").and_then(|v| v.as_str()) {
        Some(name) => format!(", with a file named \"{name}\" in it,"),
        None => String::new(),
    }
}

/// One of the file face's standing lines, named by what it says. The wording is the interface's, so an
/// instruction describes the line rather than quoting it.
fn note(with: &Args) -> Result<&'static str, String> {
    match with.get("note").and_then(|v| v.as_str()) {
        Some("not-text") => Ok("that this is not text and cannot be shown here"),
        Some("too-big") => Ok(
            "that this picture is too large to show here, followed by how large it was measured to be",
        ),
        Some("cut") => Ok("that only the beginning of the file is shown"),
        Some("unreadable") => Ok("that the file could not be read"),
        // Told apart from the line above on purpose: a link is refused because it is a link, and
        // saying the file could not be read sends the one reader most likely to meet it — somebody
        // keeping one file in one place and pointing several projects at it — looking for damage.
        Some("link") => Ok("that this name is a link, which is not followed out of the project's folders"),
        Some("partial") => Ok("that some of the folder is not being watched"),
        Some("nothing-changed") => Ok("that nothing has changed yet"),
        Some("no-folder") => Ok("that this project has no folder yet"),
        Some("folder-gone") => Ok("that this folder is not there any more"),
        // The file written under a reader who was typing in it. One line covers both ways it is
        // reached — the watch noticing while they type, and a save turned away for the same reason —
        // because the panel draws one state for them: what it says is the fact, and beside it is the
        // one thing it can do about it.
        Some("changed-underneath") => {
            Ok("that somebody wrote to this file after it was opened here")
        }
        // The two refusals a name comes back with, read under the box the name was typed into rather
        // than among the lines the column stands with. They are named by what the machine answered —
        // the name is in the folder already, or it is not a name this machine will hold — and not by
        // the sentence drawn for it, which is the interface's own.
        Some("taken") => Ok("that the name typed into the box is already in that folder"),
        Some("unnameable") => Ok("that this machine will not take what was typed as the name of one file"),
        Some(other) => Err(format!("`note` does not know `{other}`")),
        None => Err("arg `note` must say which of the face's lines".to_string()),
    }
}

/// Which way round a `dimension closed` step reads its value. Closed is what a road walks this assert
/// for, so it is the default, and `equals: false` is the reading after the way back was taken.
fn closed_equals(with: &Args) -> bool {
    with.get("equals").and_then(|v| v.as_bool()).unwrap_or(true)
}

/// Whether a step says the app it names already reaches this project. The op requires the key, so the
/// default is only what an unlinted step falls back to — and it falls back to the half a fresh machine
/// is in, nothing being set up until something sets it up.
fn set(with: &Args) -> bool {
    with.get("set").and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Whether the axis a step names is the one the board is cut along. Said out loud by the road, since
/// nothing in a step says what the board was left grouped by — and it is what turns a reading into a
/// `Review`, the column heading carrying the value whatever the cards under it do.
fn grouping(with: &Args) -> bool {
    with.get("grouping").and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Whether a `found` step asks for the top of the answer rather than merely a place in it. Off unless
/// written, the way every other reading of the shape a step wants is.
fn first(with: &Args) -> bool {
    with.get("first").and_then(|v| v.as_bool()).unwrap_or(false)
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
    /// The window this step was carried out in, as the road named it — `None` for the app's one
    /// window. It is kept on the record because a shot cannot say which window it is of: two windows
    /// of one app are the same app at the same size, and a manifest that did not name the window
    /// would leave a reader to tell them apart by what is drawn in them.
    pub window: Option<String>,
    pub instruction: String,
    pub screenshot: String,
    pub verdict: Verdict,
    pub expected: Option<Expectation>,
    pub found: Option<bool>,
    /// Whether the words met only once a misread character was forgiven ([`held`]). It is worth its
    /// own field rather than being folded into `found`: a step that passed on a tolerance is a step
    /// whose shot is worth an eye, and a run where several of them do is a reader going wrong rather
    /// than a screen.
    pub slipped: bool,
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
    /// The window the step is carried out in, as the road named it — `None` for the app's one
    /// window. Whoever is driving stands at that window before answering.
    pub window: Option<&'a str>,
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
///
/// `run_again` is the one step nobody at the screen can take ([`ends_the_run`]): the app ends and
/// another comes up on the same store. It is called **before** that step is handed over rather than
/// after, which is what lets the operator be asked about a window that is already there — they are
/// the one who can say it is a new one, and there is nothing for them to do to bring it about. A
/// failure to start the app again aborts the walk: every step after it would be shot against an
/// app that is not running.
pub fn walk<C, O, H, R>(
    scenario: &Scenario,
    evidence_dir: &Path,
    mut capture: C,
    mut read_text: O,
    mut hand_over: H,
    mut run_again: R,
) -> Result<WalkOutcome, String>
where
    C: FnMut(Option<&str>, &Path) -> Result<(), String>,
    O: FnMut(&Path) -> Result<Reading, String>,
    H: FnMut(&StepBrief<'_>) -> Result<(), String>,
    R: FnMut() -> Result<(), String>,
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
        let window = step.window();
        let domain = domain_str(domain);
        let screenshot = format!("{:02}-{kind}-{domain}-{op}.png", i + 1);
        let shot_path = evidence_dir.join(&screenshot);

        // The app put through a run of its own, where the road asks for one. It happens before the
        // hand-over so that what the operator is asked to confirm is a window already standing:
        // there is nothing for them to press, and the app they had is gone by the time they read
        // the line.
        if ends_the_run(step) {
            run_again()
                .map_err(|e| format!("step {}: starting the app again failed: {e}", i + 1))?;
        }

        // Handed over first, shot second. The screen is nobody's until somebody has been asked to
        // stand it up, and a shot taken before that is a photograph of the step before this one.
        hand_over(&StepBrief {
            index: i,
            kind,
            window,
            instruction: &instruction,
            expected: expected.as_ref(),
        })
        .map_err(|e| format!("step {}: handing the step over failed: {e}", i + 1))?;

        capture(window, &shot_path)
            .map_err(|e| format!("step {}: capturing `{screenshot}` failed: {e}", i + 1))?;

        // Judge an assert that named an expectation; keep the reading as evidence.
        let (verdict, found, slipped) = match (kind, &expected) {
            ("assert", Some(exp)) => {
                let reading = read_text(&shot_path)
                    .map_err(|e| format!("step {}: reading `{screenshot}` failed: {e}", i + 1))?;
                let hit = held(&reading.text, &fold(&exp.text));
                let _ = std::fs::write(
                    evidence_dir.join(format!("{:02}-{kind}-{domain}-{op}.txt", i + 1)),
                    &reading.raw,
                );
                let pass = hit.found == exp.present;
                if !pass {
                    passed = false;
                }
                (
                    if pass { Verdict::Pass } else { Verdict::Fail },
                    Some(hit.found),
                    hit.found && hit.slipped,
                )
            }
            ("assert", None) => (Verdict::Review, None, false),
            _ => (Verdict::Action, None, false),
        };

        let record = StepRecord {
            index: i,
            kind,
            domain: domain.to_string(),
            op,
            window: window.map(str::to_string),
            instruction,
            screenshot,
            verdict,
            expected,
            found,
            slipped,
        };
        records.push(record);
    }
    Ok(WalkOutcome { records, passed })
}

/// Whether this step is the app itself being run again — the one move on a road that is carried out
/// by the harness rather than by whoever is standing at the screen.
///
/// It is asked of the step rather than declared in the scenario, because it is not a thing a road
/// chooses: an op that ends the run ends it, and a road that could ask for the step without the
/// restart would be reading a window nothing had put in front of it.
fn ends_the_run(step: &Step) -> bool {
    matches!(step, Step::Action { domain: Domain::Store, op, .. } if op == "run-again")
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
        Domain::Mcp => "mcp",
        Domain::Tick => "tick",
        Domain::Terminal => "terminal",
        Domain::Files => "files",
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
                    ",\"expected\":{},\"present\":{},\"found\":{},\"slipped\":{}",
                    js(&e.text),
                    e.present,
                    found,
                    r.slipped
                ),
                _ => String::new(),
            };
            // The window only when the road named one: an app with a single window has nothing to
            // say here, and a key carrying `null` on every step of every road would read as a
            // question each of them had been asked.
            let window = match &r.window {
                Some(w) => format!(",\"window\":{}", js(w)),
                None => String::new(),
            };
            format!(
                "{{\"step\":{},\"kind\":{},\"domain\":{},\"op\":{},\"verdict\":{},\"instruction\":{},\"screenshot\":{}{}{}}}",
                r.index + 1,
                js(r.kind),
                js(&r.domain),
                js(&r.op),
                js(r.verdict.as_str()),
                js(&r.instruction),
                js(&r.screenshot),
                window,
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

    /// A number the store issues is nothing a road could write, and nothing these lines could print:
    /// they are rendered from the YAML alone, before any world stands up. So the operator is sent to
    /// the record instead and reads the number off the screen, and the shape it goes in as is said.
    #[test]
    fn a_number_is_named_by_the_record_that_carries_it() {
        let s = load(r#"
id: x
title: y
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: SEED }
    as: seed
  - type: action
    domain: task
    op: narrow
    with: { number_of: seed }
  - type: assert
    domain: task
    op: found
    with: { number_of: seed, spelled: hash, target: seed, first: true }
"#);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(
            lines[1].contains("the number of the task \"SEED\"") && !lines[1].contains("`#`"),
            "got: {}", lines[1]
        );
        assert!(lines[2].contains("the number of the task \"SEED\"") && lines[2].contains("`#`"), "got: {}", lines[2]);
        assert!(lines[2].contains("first of the answer"), "and the top is what the step asks for: {}", lines[2]);
    }

    /// The two boxes that read one side each. A number carrying no type code is whichever side the box
    /// it was typed into reads, so the line has to say which of the two tabs the operator is standing
    /// at — one that named neither could be walked on either.
    #[test]
    fn the_two_boxes_that_read_one_side_each_are_told_apart() {
        let s = load(r#"
id: x
title: y
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: SEED }
    as: seed
  - type: action
    domain: decision
    op: create
    with: { title: WHY }
    as: why
  - type: action
    domain: task
    op: narrow
    with: { number_of: seed }
  - type: action
    domain: decision
    op: narrow
    with: { number_of: why }
"#);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[2].contains("On the board") && lines[2].contains("over the columns"), "got: {}", lines[2]);
        assert!(
            lines[3].contains("On the decisions tab") && lines[3].contains("the number of the decision \"WHY\""),
            "got: {}", lines[3]
        );
    }

    /// A number written into another record's text is said the same way a number typed into a box is:
    /// by the record that carries it, the operator reading it off the screen.
    #[test]
    fn a_number_written_into_text_is_named_by_the_record_that_carries_it() {
        let s = load(r#"
id: x
title: y
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: SEED }
    as: seed
  - type: action
    domain: task
    op: create
    with: { title: OTHER }
    as: other
  - type: action
    domain: task
    op: update
    with: { target: other, notes: SEE, mentions: seed }
  - type: action
    domain: task
    op: comment
    with: { target: other, text: SEE, mentions: seed }
"#);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        for line in &lines[2..] {
            assert!(
                line.contains("followed by the number of the task \"SEED\""),
                "got: {line}"
            );
        }
    }

    /// Where a hit stands is not something a reading gives back, so that step is left for an eye —
    /// while every other `found` keeps the expectation the reader closes it with.
    #[test]
    fn the_top_of_an_answer_is_left_for_an_eye() {
        let s = load(r#"
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
    with: { number_of: seed, target: seed, first: true }
  - type: assert
    domain: task
    op: found
    with: { number_of: seed, target: seed }
"#);
        let mut ins = Instructor::new();
        let steps = s.steps(Driver::Gui);
        for st in steps {
            ins.render(st).unwrap();
        }
        assert!(ins.expectation(&steps[1]).is_none(), "the top of the answer is the eye's to close");
        assert!(ins.expectation(&steps[2]).is_some(), "and merely being in it is still read");
    }

    /// A setting the plugin fills in for itself: what the operator is asked to confirm is an absence,
    /// so the instruction names the value that has to be standing there while they read it — an empty
    /// field would carry no box and no button either, and would prove nothing about this one.
    #[test]
    fn a_readonly_setting_is_read_as_a_value_with_nothing_to_press() {
        let s = load(r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: plugin
    op: config
    with: { name: viewer, key: worker_url, readonly: true, equals: https://example.test/board }
"#);
        let mut ins = Instructor::new();
        let line = ins.render(&s.steps(Driver::Gui)[0]).unwrap();
        assert!(line.contains("worker_url") && line.contains("https://example.test/board"), "got: {line}");
        assert!(line.contains("no box") && line.contains("no button"), "got: {line}");
    }

    /// **The absent half of a form reading is the half a condition needs.** A setting, a
    /// candidate or an operation its author ruled out is not drawn greyed and not drawn empty — it is off
    /// the form — so the step is a reading of words that must be nowhere, and the sentence handed to an
    /// operator has to say so rather than asking them to confirm something is there.
    #[test]
    fn a_form_reading_says_which_way_it_leans_and_expects_the_words_that_way() {
        let s = load(r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: plugin
    op: asks
    with: { name: viewer, key: worker_url, label: Worker URL, present: false }
  - type: assert
    domain: plugin
    op: asks
    with: { name: viewer, key: transport, candidate: cloudflare, label: Cloudflare, present: false }
  - type: assert
    domain: plugin
    op: offers
    with: { name: viewer, label: Raise the tunnel, present: false }
  - type: assert
    domain: plugin
    op: offers
    with: { name: viewer, label: Raise the tunnel, present: true }
"#);
        let steps = s.steps(Driver::Gui);
        let mut ins = Instructor::new();
        let lines: Vec<String> = steps.iter().map(|st| ins.render(st).unwrap()).collect();

        assert!(lines[0].contains("does not ask for the setting"), "got: {}", lines[0]);
        assert!(lines[1].contains("offers no answer stored as"), "got: {}", lines[1]);
        assert!(lines[2].contains("offers no operation drawn as"), "got: {}", lines[2]);
        assert!(lines[3].contains("offers the operation drawn as"), "got: {}", lines[3]);

        // What OCR is sent looking for is the author's words either way; which way the step leans is the
        // whole of the difference, and getting that backwards would green a run on a form nobody drew.
        assert_eq!(
            ins.expectation(&steps[0]),
            Some(Expectation { text: "Worker URL".to_string(), present: false })
        );
        assert_eq!(
            ins.expectation(&steps[2]),
            Some(Expectation { text: "Raise the tunnel".to_string(), present: false })
        );
        assert_eq!(
            ins.expectation(&steps[3]),
            Some(Expectation { text: "Raise the tunnel".to_string(), present: true })
        );
    }

    /// **What is standing in the input line and what a program printed are one screen and two
    /// readings.** Both are read off the same shot, so nothing but the sentence tells the operator
    /// which they are being asked for — and the difference is the whole of what a hand-over owes: a
    /// build that sent the newline would have drawn what the program did with it, leaving the line
    /// empty, and a road that read either as the other would go green over exactly that.
    #[test]
    fn what_is_standing_unsent_is_asked_for_as_unsent() {
        let s = load(r#"
id: x
title: y
steps_gui:
  - type: action
    domain: files
    op: hand-to-pane
  - type: assert
    domain: terminal
    op: in-the-box
    with: { shows: /work/notes.md }
  - type: assert
    domain: terminal
    op: pane
    with: { shows: /work/notes.md }
"#);
        let steps = s.steps(Driver::Gui);
        let mut ins = Instructor::new();
        let lines: Vec<String> = steps.iter().map(|st| ins.render(st).unwrap()).collect();

        assert!(lines[0].contains("pastes the row's path into the pane"), "got: {}", lines[0]);
        // The item leaves the path for the person, so the line says to leave it alone.
        assert!(lines[0].contains("leave it there"), "got: {}", lines[0]);
        assert!(lines[1].contains("not been sent"), "got: {}", lines[1]);
        assert_ne!(lines[1], lines[2], "the two readings are one screen and must not be one line");

        // And it is read off the shot, the same as what a program printed: the words are on it.
        assert_eq!(
            ins.expectation(&steps[1]),
            Some(Expectation { text: "/work/notes.md".to_string(), present: true })
        );
    }

    /// **A hand full and a hand with one thing in it are two gestures, and the line has to say which.**
    /// Two files let go one after the other end on the same input line as a pair let go together, so
    /// an instruction that named them one at a time would be walked as two drops and read as proof of
    /// something nobody asked about. The pair form says the selecting, the one movement and the one
    /// release, because each of the three is a place an operator would otherwise split it.
    #[test]
    fn a_pair_dragged_in_together_is_one_movement_and_says_so() {
        let s = load(r#"
id: x
title: y
steps_gui:
  - type: action
    domain: terminal
    op: drop-in
    with: { brings: seeds.csv }
  - type: action
    domain: terminal
    op: drop-in
    with: { brings: seeds.csv, beside: labels.txt }
"#);
        let steps = s.steps(Driver::Gui);
        let mut ins = Instructor::new();
        let lines: Vec<String> = steps.iter().map(|st| ins.render(st).unwrap()).collect();

        assert!(lines[0].contains("drag a file named \"seeds.csv\""), "got: {}", lines[0]);
        assert!(!lines[0].contains("labels.txt"), "got: {}", lines[0]);

        assert!(lines[1].contains("\"seeds.csv\""), "got: {}", lines[1]);
        assert!(lines[1].contains("\"labels.txt\""), "got: {}", lines[1]);
        assert!(lines[1].contains("together"), "the selecting is what makes it one hand: {}", lines[1]);
        assert!(lines[1].contains("one movement"), "got: {}", lines[1]);
    }

    /// **The three ways out of a file's menu are three different sentences, and none of them is shot.**
    /// What the operator is told to press has to name one item and not another — the three sit next to
    /// each other on one menu — and what they are then asked to confirm cannot be read off the picture
    /// the run takes: an application that came forward, or the operating system's own chooser, is not on
    /// Amenbo's window. An expectation appearing on any of the three would send OCR hunting Amenbo's
    /// window for words that were never going to be there, and fail the road over the harness.
    #[test]
    fn each_way_out_of_a_files_menu_is_its_own_line_and_none_of_them_is_read_off_the_shot() {
        let s = load(r#"
id: x
title: y
steps_gui:
  - type: action
    domain: files
    op: menu
    with: { name: watering.md, section: tree }
  - type: action
    domain: files
    op: hand-over
    with: { door: usual }
  - type: assert
    domain: files
    op: handed-over
    with: { door: usual }
  - type: action
    domain: files
    op: hand-over
    with: { door: pick }
  - type: assert
    domain: files
    op: handed-over
    with: { door: pick }
  - type: action
    domain: files
    op: hand-over
    with: { door: manager }
  - type: assert
    domain: files
    op: handed-over
    with: { door: manager }
"#);
        let steps = s.steps(Driver::Gui);
        let mut ins = Instructor::new();
        let lines: Vec<String> = steps.iter().map(|st| ins.render(st).unwrap()).collect();

        assert!(lines[0].contains("right-click the row \"watering.md\""), "got: {}", lines[0]);
        assert!(lines[1].contains("already opens that kind of file with"), "got: {}", lines[1]);
        assert!(lines[3].contains("an application you pick"), "got: {}", lines[3]);
        assert!(lines[5].contains("shows the file in the file manager"), "got: {}", lines[5]);

        for i in [2, 4, 6] {
            assert_eq!(ins.expectation(&steps[i]), None, "step {i} is not a reading of the shot");
        }
    }

    /// Absent, a form reading leans the way every one of these leaned before there was anything to hide.
    #[test]
    fn a_form_reading_that_says_nothing_still_expects_the_words_present() {
        let s = load(r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: plugin
    op: asks
    with: { name: viewer, key: worker_url, label: Worker URL }
"#);
        let steps = s.steps(Driver::Gui);
        let mut ins = Instructor::new();
        let line = ins.render(&steps[0]).unwrap();
        assert!(line.contains("asks for the setting"), "got: {line}");
        assert_eq!(
            ins.expectation(&steps[0]),
            Some(Expectation { text: "Worker URL".to_string(), present: true })
        );
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

    /// A card past the states a road is watching is a card that has ended, and its title is on screen
    /// with a line through it. Both halves are checked, since the absent one is the half that would
    /// otherwise read green off a reading that found nothing because it could read nothing.
    #[test]
    fn a_step_naming_an_ended_task_is_left_for_review() {
        let yaml = r#"
id: x
title: y
given:
  - type: action
    domain: task
    op: create
    with: { title: SCENARIO — work is over }
    as: finished
  - type: action
    domain: task
    op: status
    with: { target: finished, status: done }
  - type: action
    domain: task
    op: create
    with: { title: SCENARIO — still to be taken }
    as: waiting
steps_gui:
  - type: assert
    domain: task
    op: narrowed
    with: { target: finished, present: true }
  - type: assert
    domain: task
    op: narrowed
    with: { target: finished, present: false }
  - type: assert
    domain: task
    op: narrowed
    with: { target: waiting, present: true }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        ins.learn(&s.given);
        let steps = s.steps(Driver::Gui);
        let lines: Vec<String> = steps.iter().map(|st| ins.render(st).unwrap()).collect();

        assert!(ins.expectation(&steps[0]).is_none(), "a struck title is not OCR's to read");
        assert!(ins.expectation(&steps[1]).is_none(), "and its absent half is not either");
        assert!(lines[0].contains("line through it"), "the line says why an eye is owed: {}", lines[0]);

        // The card beside it is untouched: this is about the one state that draws the line, not about
        // every card on a board that happens to hold one.
        assert_eq!(
            ins.expectation(&steps[2]).expect("a task still open is read as ever"),
            Expectation { text: "SCENARIO — still to be taken".to_string(), present: true }
        );
        assert!(!lines[2].contains("line through it"));
    }

    /// The state moves rather than being set once: a world that took a task through a terminal state
    /// and back out leaves a title an eye is not owed. Written as a premise because that is where it
    /// can be written — the harness maps no op that ends a task onto the screen, so a road has no way
    /// to walk one.
    #[test]
    fn a_task_taken_back_out_of_a_terminal_state_is_readable_again() {
        let yaml = r#"
id: x
title: y
given:
  - type: action
    domain: task
    op: create
    with: { title: SEED }
    as: seed
  - type: action
    domain: task
    op: status
    with: { target: seed, status: done }
  - type: action
    domain: task
    op: status
    with: { target: seed, status: in_progress }
steps_gui:
  - type: assert
    domain: task
    op: listed
    with: { filter: "status:in_progress", target: seed, present: true }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        ins.learn(&s.given);
        let steps = s.steps(Driver::Gui);
        let line = ins.render(&steps[0]).unwrap();
        assert_eq!(
            ins.expectation(&steps[0]).expect("no line is drawn through it any more"),
            Expectation { text: "SEED".to_string(), present: true }
        );
        assert!(!line.contains("line through it"));
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

    /// The tick's band: each button is named by what pressing it leaves behind, the answer travels
    /// as a value, and every step of the road is closed by an eye — the band's offer is put in the
    /// interface's own words, and its absent half is an absence no reading settles.
    #[test]
    fn the_ticks_band_is_answered_by_what_each_press_leaves_and_read_by_an_eye() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: tick
    op: banner
    with: { present: true }
  - type: action
    domain: tick
    op: banner-answer
    with: { answer: later }
  - type: action
    domain: tick
    op: banner-answer
    with: { answer: start }
  - type: action
    domain: tick
    op: banner-answer
    with: { answer: never }
  - type: assert
    domain: tick
    op: banner
    with: { present: false }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> =
            s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("standing across the app") && lines[0].contains("three buttons"), "got: {}", lines[0]);
        assert!(lines[1].contains("puts the question off"), "got: {}", lines[1]);
        assert!(lines[2].contains("answers yes") && lines[2].contains("registers the timer"), "got: {}", lines[2]);
        assert!(lines[3].contains("declines it for good"), "got: {}", lines[3]);
        assert!(lines[4].contains("no band") && lines[4].contains("anywhere on the screen"), "got: {}", lines[4]);
        for st in s.steps(Driver::Gui) {
            assert!(ins.expectation(st).is_none(), "the band's words are the interface's — an eye closes these");
        }
    }

    /// The row that holds the answer afterwards: moved by what the position does rather than by the
    /// word drawn on it, and read the same way — which of two drawn words is standing is nothing a
    /// reading settles, so the assert is left for an eye.
    #[test]
    fn the_ticks_settings_row_is_moved_and_read_by_its_position() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: tick
    op: setting
    with: { position: off }
  - type: action
    domain: tick
    op: set
    with: { position: on }
  - type: assert
    domain: tick
    op: setting
    with: { position: on }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> =
            s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("check off") && lines[0].contains("no registration"), "got: {}", lines[0]);
        assert!(lines[1].contains("turns the check on") && lines[1].contains("timer is registered"), "got: {}", lines[1]);
        assert!(lines[2].contains("check on") && lines[2].contains("registered timer"), "got: {}", lines[2]);
        for st in s.steps(Driver::Gui) {
            assert!(ins.expectation(st).is_none(), "the row's positions are words of the interface — an eye closes these");
        }
    }

    /// An answer or a position outside the ones the screen offers is a scenario bug, and it is met
    /// at render time rather than in front of a screen.
    #[test]
    fn an_unknown_tick_answer_or_position_is_refused() {
        let s = load(r#"
id: x
title: y
steps_gui:
  - type: action
    domain: tick
    op: banner-answer
    with: { answer: maybe }
  - type: action
    domain: tick
    op: set
    with: { position: sideways }
  - type: assert
    domain: tick
    op: setting
    with: { position: sideways }
"#);
        let mut ins = Instructor::new();
        let err = ins.render(&s.steps(Driver::Gui)[0]).unwrap_err();
        assert!(err.contains("does not know the answer `maybe`"), "got: {err}");
        let err = ins.render(&s.steps(Driver::Gui)[1]).unwrap_err();
        assert!(err.contains("does not know the position `sideways`"), "got: {err}");
        let err = ins.render(&s.steps(Driver::Gui)[2]).unwrap_err();
        assert!(err.contains("does not know the position `sideways`"), "got: {err}");
    }

    /// The way out of a pane asks one of two questions, and which one is the pane's business: a
    /// session holding a reservation is asked about it by name and offered three ways out, and one
    /// holding nothing is asked the plain thing. So the plain instruction has to survive `answer`
    /// being added, each of the three has to say a different outcome, and the two ways of writing
    /// the step wrong — an answer nobody offers, and a three-way answer with nothing named — have to
    /// be turned away rather than rendered into a line an operator could not act on.
    #[test]
    fn the_way_out_of_a_pane_is_answered_by_name_and_reads_what_it_is_leaving() {
        let s = load(r#"
id: x
title: y
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: Re-line the quench tank }
    as: tank
  - type: action
    domain: terminal
    op: remove-pane
    with: { shows: SCENARIO the pane }
  - type: action
    domain: terminal
    op: remove-pane
    with: { shows: SCENARIO the pane, target: tank, answer: hand-back }
  - type: action
    domain: terminal
    op: remove-pane
    with: { shows: SCENARIO the pane, target: tank, answer: leave }
  - type: action
    domain: terminal
    op: remove-pane
    with: { shows: SCENARIO the pane, target: tank, answer: cancel }
  - type: action
    domain: terminal
    op: remove-pane
    with: { shows: SCENARIO the pane, target: tank, answer: think-about-it }
  - type: action
    domain: terminal
    op: remove-pane
    with: { shows: SCENARIO the pane, answer: hand-back }
"#);
        let mut ins = Instructor::new();
        let steps = s.steps(Driver::Gui);
        // The binding, so the three answers below have a title to name.
        ins.render(&steps[0]).expect("the world's task renders");

        // Nothing held: the plain question, and nothing about a reservation on it.
        let plain = ins.render(&steps[1]).expect("the plain question renders");
        assert!(plain.contains("answer it yes"), "got: {plain}");
        assert!(!plain.contains("Three answers"), "got: {plain}");

        // Each of the three names what stands to be lost, and then parts company on the outcome.
        for (i, step) in steps.iter().enumerate().take(5).skip(2) {
            let line = ins.render(step).unwrap_or_else(|e| panic!("step {i}: {e}"));
            assert!(line.contains("Re-line the quench tank"), "step {i} got: {line}");
            assert!(line.contains("and no other"), "step {i} got: {line}");
        }
        let back = ins.render(&steps[2]).expect("renders");
        assert!(back.contains("hands the work back"), "got: {back}");
        let leave = ins.render(&steps[3]).expect("renders");
        assert!(leave.contains("leaves the work where it is"), "got: {leave}");
        assert!(leave.contains("still held"), "got: {leave}");
        let stay = ins.render(&steps[4]).expect("renders");
        assert!(stay.contains("the pane is still there"), "got: {stay}");

        let err = ins.render(&steps[5]).unwrap_err();
        assert!(err.contains("does not know the answer `think-about-it`"), "got: {err}");
        let err = ins.render(&steps[6]).unwrap_err();
        assert!(err.contains("give it a `target`"), "got: {err}");
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

    /// What the author's own check said, in both of the places a verdict reaches a reader: the sentence
    /// over the form, and the line beside the box it named. Each is sent to OCR on its own words, since
    /// what a build would draw there instead is Amenbo's sentence — and the two are told apart by nothing
    /// else. The instruction says which of the two places is being read, because an eye handed only the
    /// words would close the step off either.
    #[test]
    fn a_checks_two_sentences_are_read_where_each_of_them_is_drawn() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: plugin
    op: checked
    with: { name: standup, text: The webhook is not one Slack answers on }
  - type: assert
    domain: plugin
    op: checked
    with: { name: standup, key: webhook, text: There is a space in it }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("at its head") && !lines[0].contains("beside"), "got: {}", lines[0]);
        assert!(lines[1].contains("beside the setting \"webhook\""), "got: {}", lines[1]);

        for (i, text) in ["The webhook is not one Slack answers on", "There is a space in it"].iter().enumerate() {
            let exp = ins.expectation(&s.steps(Driver::Gui)[i]).expect("a check's own sentence is OCR-judged");
            assert_eq!(exp, Expectation { text: text.to_string(), present: true });
        }
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

    /// The one row a machine-wide plugin has, walked the way the road walks it: the mark before the
    /// switch, the settings opened inside it, the press, and the project's own face offering no second
    /// switch. Not one line of it names a project — a device row has none, and a step that named one
    /// would be sending the operator to the wrong kind of row.
    #[test]
    fn the_device_row_is_marked_opened_and_pressed_without_naming_a_project() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: plugin
    op: settings-on-device
    with: { name: worktree, state: required-empty }
  - type: action
    domain: plugin
    op: open-config-on-device
    with: { name: worktree }
  - type: assert
    domain: plugin
    op: settings-on-device
    with: { name: worktree, state: open }
  - type: action
    domain: plugin
    op: enable-on-device
    with: { name: worktree }
  - type: assert
    domain: plugin
    op: fires-on-device
    with: { name: worktree, present: true }
  - type: assert
    domain: project
    op: plugin-row
    with: { project: Greenhouse, plugin: worktree, state: device }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("owing a setting"), "got: {}", lines[0]);
        assert!(lines[1].contains("open the settings kept there"), "got: {}", lines[1]);
        assert!(lines[2].contains("asks which project"), "got: {}", lines[2]);
        assert!(lines[3].contains("turn the plugin on there"), "got: {}", lines[3]);
        assert!(lines[4].contains("says the plugin is on"), "got: {}", lines[4]);
        // The last line is the one that does name a project, since it is read on that project's face —
        // and what it says about the plugin is that the face has nothing of its own to press.
        assert!(lines[5].contains("as the device's own"), "got: {}", lines[5]);
        for line in &lines[..5] {
            assert!(!line.contains("Greenhouse"), "a device row has no project to name: {line}");
        }

        for (i, st) in s.steps(Driver::Gui).iter().enumerate() {
            assert!(ins.expectation(st).is_none(), "step {i} reads a word of the interface's own");
        }
    }

    /// A device-row state the face does not have is refused by name, rather than rendered as a line
    /// telling an operator to confirm nothing.
    #[test]
    fn an_unknown_device_row_state_is_refused() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: plugin
    op: settings-on-device
    with: { name: worktree, state: half-filled }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let err = ins.render(&s.steps(Driver::Gui)[0]).expect_err("an unknown state has no instruction");
        assert!(err.contains("half-filled"), "got: {err}");
    }

    /// The layer read off the row: the declared one carries a sentence about the device, and the one
    /// beside it carries none. The two lines have to be different sentences rather than one negated,
    /// since the miss they stand against is a build drawing the sentence for every row — and both are a
    /// `Review`, the words being the interface's own and the absence being nothing a reading gives back.
    #[test]
    fn the_layer_is_read_off_the_row_that_declared_it_and_off_the_one_that_did_not() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: plugin
    op: layer
    with: { name: slack, scope: machine }
  - type: assert
    domain: plugin
    op: layer
    with: { name: worktree, scope: project }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("reads every project on this device"), "got: {}", lines[0]);
        assert!(lines[1].contains("says no such thing"), "got: {}", lines[1]);
        assert!(!lines[1].contains("reads every project on this device"), "got: {}", lines[1]);

        for (i, st) in s.steps(Driver::Gui).iter().enumerate() {
            assert!(ins.expectation(st).is_none(), "step {i} reads a sentence of the interface's own");
        }
    }

    /// A layer the manifest vocabulary does not hold is refused by name, rather than rendered as a line
    /// telling an operator to confirm nothing.
    #[test]
    fn an_unknown_plugin_layer_is_refused() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: plugin
    op: layer
    with: { name: slack, scope: household }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let err = ins.render(&s.steps(Driver::Gui)[0]).expect_err("an unknown layer has no instruction");
        assert!(err.contains("household"), "got: {err}");
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

    /// The other narrowing that board has. A press is named by the pair the CLI writes, since the chips
    /// carrying it are in the reader's own language, and it says the values already chosen stay chosen —
    /// an axis takes a set, so a line a reader could satisfy by clearing its neighbours would be walking
    /// a different road. What the fold leaves is read by an eye on both of its halves.
    #[test]
    fn the_values_are_opened_pressed_by_the_pair_the_cli_writes_and_folded_away() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: action
    domain: task
    op: open-filters
  - type: action
    domain: task
    op: choose-filter
    with: { axis: status, value: in_progress }
  - type: action
    domain: task
    op: close-filters
  - type: assert
    domain: task
    op: filters-folded
    with: { axes: 2 }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let steps = s.steps(Driver::Gui);
        let lines: Vec<String> = steps.iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("how many axes are narrowing"), "got: {}", lines[0]);
        assert!(lines[1].contains("`status:in_progress`"), "got: {}", lines[1]);
        assert!(lines[1].contains("already chosen on that axis chosen"), "got: {}", lines[1]);
        assert!(lines[2].contains("room they were taking"), "got: {}", lines[2]);
        assert!(lines[3].contains("says 2 axes are narrowing"), "got: {}", lines[3]);
        assert!(
            ins.expectation(&steps[3]).is_none(),
            "an absence and a bare number are closed by an eye, not by a reading"
        );
    }

    /// The same panel on the other tab. What is asked of it is that the road says which of the two the
    /// operator is standing on: the board's line and this one describe the same act, so a line that did
    /// not name its screen could be walked on either tab and would prove neither.
    #[test]
    fn the_decisions_tab_has_the_same_values_and_says_it_is_the_decisions_tab() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: action
    domain: decision
    op: open-filters
  - type: action
    domain: decision
    op: choose-filter
    with: { axis: dim:theme, value: main }
  - type: action
    domain: decision
    op: close-filters
  - type: assert
    domain: decision
    op: filters-folded
    with: { axes: 1 }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let steps = s.steps(Driver::Gui);
        let lines: Vec<String> = steps.iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("On the decisions tab"), "got: {}", lines[0]);
        assert!(lines[0].contains("how many axes are narrowing"), "got: {}", lines[0]);
        // The pair as the grammar writes it: a classification axis is a `dim:` key and takes `=`,
        // where the two builtin keys take a colon. A chip pressed off the wrong spelling is a chip
        // an operator has to guess at.
        assert!(lines[1].contains("`dim:theme=main`"), "got: {}", lines[1]);
        assert!(lines[2].contains("the decisions have back the room"), "got: {}", lines[2]);
        assert!(lines[3].contains("says 1 axes are narrowing"), "got: {}", lines[3]);
        assert!(
            ins.expectation(&steps[3]).is_none(),
            "an absence and a bare number are closed by an eye, not by a reading"
        );
    }

    /// The row the narrowing left. It is read off the shot by the decision's own title, both ways round:
    /// a row that should have gone and one that should have stayed are the same screen until the title
    /// is looked for, and the line names no narrowing of its own because the press before it did that.
    #[test]
    fn a_decision_row_is_read_against_the_list_the_narrowing_left() {
        let yaml = r#"
id: x
title: y
given:
  - type: action
    domain: decision
    op: create
    with: { title: SCENARIO — the retention window }
    as: retention
steps_gui:
  - type: action
    domain: decision
    op: choose-filter
    with: { axis: status, value: accepted }
  - type: assert
    domain: decision
    op: narrowed
    with: { target: retention, present: false }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        ins.learn(&s.given);
        let steps = s.steps(Driver::Gui);
        let line = ins.render(&steps[1]).unwrap();
        assert!(line.contains("the list the narrowing left"), "got: {line}");
        assert!(line.contains("not among"), "got: {line}");
        assert!(!line.contains("search"), "got: {line}");
        let e = ins.expectation(&steps[1]).expect("a row that went is read off the shot");
        assert_eq!(
            e,
            Expectation { text: "SCENARIO — the retention window".into(), present: false }
        );
    }

    /// The card the narrowing left, read the same way whichever narrowing left it — the line names none,
    /// because the move in front of it is what did the narrowing and saying it twice is what would let
    /// the two disagree.
    #[test]
    fn a_card_is_read_against_the_board_the_narrowing_left() {
        let yaml = r#"
id: x
title: y
given:
  - type: action
    domain: task
    op: create
    with: { title: SCENARIO — still to be taken }
    as: waiting
steps_gui:
  - type: action
    domain: task
    op: choose-filter
    with: { axis: assignee, value: me-ai }
  - type: assert
    domain: task
    op: narrowed
    with: { target: waiting, present: false }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        ins.learn(&s.given);
        let steps = s.steps(Driver::Gui);
        let line = ins.render(&steps[1]).unwrap();
        assert!(line.contains("the board the narrowing left"), "got: {line}");
        assert!(!line.contains("search"), "got: {line}");
        let e = ins.expectation(&steps[1]).expect("a card that went is read off the shot");
        assert_eq!(e, Expectation { text: "SCENARIO — still to be taken".into(), present: false });
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

    /// Hanging a file on a record and on a remark on one. Both lines say where on screen the file goes,
    /// because that is the whole of what separates them — and the remark is named by what it says,
    /// since a timeline offers nothing else to pick one out by.
    #[test]
    fn a_file_is_hung_where_the_screen_keeps_the_way_in_for_it() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: SEED }
    as: seed
  - type: action
    domain: task
    op: comment
    with: { target: seed, text: the sweep runs nightly }
    as: remark
  - type: action
    domain: task
    op: attach
    with: { target: seed, file: throughput.log }
  - type: action
    domain: comment
    op: attach
    with: { target: remark, file: handover.log }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let steps = s.steps(Driver::Gui);
        for step in &steps[..2] {
            ins.render(step).unwrap();
        }
        let on_record = ins.render(&steps[2]).unwrap();
        assert!(on_record.contains("the task \"SEED\""), "got: {on_record}");
        assert!(on_record.contains("attachments section on its pane"), "got: {on_record}");
        let on_remark = ins.render(&steps[3]).unwrap();
        assert!(on_remark.contains("the comment \"the sweep runs nightly\""), "got: {on_remark}");
        assert!(on_remark.contains("under the remark itself"), "got: {on_remark}");
    }

    /// A link named where a file belongs. The screen's two ways in both take bytes off a disk, so there
    /// is nothing to instruct — and an operator handed a line about a URL would go looking for a face
    /// the app does not have.
    #[test]
    fn a_link_has_no_way_in_on_the_screen_and_is_refused() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: SEED }
    as: seed
  - type: action
    domain: task
    op: attach
    with: { target: seed, url: "https://example.com/spec" }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let steps = s.steps(Driver::Gui);
        ins.render(&steps[0]).unwrap();
        let err = ins.render(&steps[1]).expect_err("a link is not something this screen hangs");
        assert!(err.contains("url") && err.contains("picker"), "got: {err}");
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

    /// The box a press asks in, in both of the shapes it comes in. The credential one is a line of its own
    /// rather than a clause added to the other, since an eye handed two things to check about one box
    /// checks the first — and which of the two boxes is standing there is what a build gets wrong. Neither
    /// is a reading: an empty box puts no words on a shot, and nor does one drawing dots.
    #[test]
    fn a_box_a_press_asks_in_is_read_for_being_empty_and_for_what_it_draws_back() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: plugin
    op: press-asks
    with: { name: worktree, label: API token }
  - type: assert
    domain: plugin
    op: press-asks
    with: { name: worktree, label: Access token, secret: true }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("box is empty") && !lines[0].contains("credential"), "got: {}", lines[0]);
        assert!(lines[1].contains("not drawn back") && lines[1].contains("credential"), "got: {}", lines[1]);
        for step in s.steps(Driver::Gui) {
            assert!(ins.expectation(step).is_none(), "an empty box is closed by an eye");
        }
    }

    /// The other kind of field, read by the word that means a box rather than ticks: what is asked is that
    /// a typed line is standing where it was left. It takes no state, which is the whole difference — a
    /// choice's three answers are told apart by a chip, and a box is either holding the value or it is not.
    #[test]
    fn a_typed_setting_is_read_back_as_the_box_holding_it() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: plugin
    op: config
    with: { name: worktree, key: webhook, holds: https://example.com/hooks/9f2a41 }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let line = ins.render(&s.steps(Driver::Gui)[0]).unwrap();
        assert!(
            line.contains("box for the setting \"webhook\"")
                && line.contains("https://example.com/hooks/9f2a41"),
            "got: {line}"
        );
        assert!(
            ins.expectation(&s.steps(Driver::Gui)[0]).is_none(),
            "what a box holds is closed by an eye, like every other reading of this form"
        );
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

    /// Starting a folder's AI on Amenbo, as the screen walks it: the report standing on the board, and
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

    /// The other way in, read off the screen that lists the apps: the folder an app's entry names is the
    /// reading, and the two rows either side of it are the two roads. A row read with no folder named is
    /// a `Review` — the words that say whether an app is set up are the interface's own, and so is the
    /// label on the button that says which road it offers.
    #[test]
    fn the_folder_an_apps_entry_names_is_what_the_mcp_row_is_read_by() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: action
    domain: repo
    op: mcp-open
  - type: assert
    domain: repo
    op: mcp-app
    with: { app: claude-code, set: true, dir: /Users/reader/greenhouse }
  - type: assert
    domain: repo
    op: mcp-app
    with: { app: claude-desktop, set: false }
  - type: assert
    domain: repo
    op: mcp-road
    with: { app: claude-desktop, road: file }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let steps = s.steps(Driver::Gui);
        let lines: Vec<String> = steps.iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("where an AI is connected"), "got: {}", lines[0]);
        assert!(
            lines[1].contains("\"/Users/reader/greenhouse\"") && lines[1].contains("already set up"),
            "got: {}",
            lines[1]
        );
        assert!(lines[2].contains("not set up") && lines[2].contains("no folder"), "got: {}", lines[2]);
        assert!(lines[3].contains("file") && lines[3].contains("no request"), "got: {}", lines[3]);

        let folder = ins.expectation(&steps[1]).expect("the folder is what only this row draws");
        assert_eq!(folder, Expectation { text: "/Users/reader/greenhouse".to_string(), present: true });
        // The row with no folder to read, and the road beside it: both are the interface's own words.
        assert!(ins.expectation(&steps[2]).is_none(), "a row with no folder named is an eye's to close");
        assert!(ins.expectation(&steps[3]).is_none(), "a button's label is an eye's to close");
    }

    /// A road Amenbo has no such thing as is refused where it is written, not met halfway through a run.
    #[test]
    fn an_mcp_road_the_screen_does_not_offer_is_refused() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: assert
    domain: repo
    op: mcp-road
    with: { app: claude-desktop, road: whistle }
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let err = ins.render(&s.steps(Driver::Gui)[0]).expect_err("no such road");
        assert!(err.contains("whistle"), "got: {err}");
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
  - type: assert
    domain: repo
    op: ai-launch-consent-clear-shut
"#;
        let s = load(yaml);
        let mut ins = Instructor::new();
        let lines: Vec<String> = s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("answered no") && lines[0].contains("only way back"), "got: {}", lines[0]);
        assert!(lines[1].contains("clears its answer"), "got: {}", lines[1]);
        assert!(lines[2].contains("not been answered for") && lines[2].contains("nothing left to clear"), "got: {}", lines[2]);
        // And the button, sent to the eye as something drawn rather than something wired: the line names
        // the fade and the pointer, and says out loud that a press going nowhere is not what closes it.
        assert!(lines[3].contains("faded") && lines[3].contains("cursor"), "got: {}", lines[3]);
        assert!(lines[3].contains("does nothing when pressed is not the reading"), "got: {}", lines[3]);
        assert!(
            s.steps(Driver::Gui).iter().all(|st| ins.expectation(st).is_none()),
            "an answer drawn in the interface's own words is not a reading, and neither is paint"
        );
    }

    /// The two claims a road makes about a smart view, and the one of them a reading cannot close. The
    /// row is read before anything is opened and the listing after, so what proves the pair is that the
    /// badge goes to an eye — it is a bare number in a sidebar full of bare numbers, told apart from
    /// them by a colour — while the rows under it are read off the shot like any other listing.
    #[test]
    fn the_badge_on_a_view_is_an_eyes_and_the_rows_under_it_are_read() {
        let warns = Step::Assert {
            domain: Domain::Task,
            op: "view-warns".to_string(),
            with: [
                ("view".to_string(), serde_yaml::Value::from("due")),
                ("step".to_string(), serde_yaml::Value::from("stop")),
                ("count".to_string(), serde_yaml::Value::from(3)),
            ]
            .into_iter()
            .collect(),
            window: None,
        };
        let said = Instructor::new().render(&warns).unwrap();
        assert!(said.contains("nothing opened"), "the row is read before the press: {said}");
        assert!(said.contains("red"), "and the eye is told which colour to find: {said}");
        assert!(
            Instructor::new().expectation(&warns).is_none(),
            "a bare number told apart by its colour is a Review",
        );

        let lists = Step::Assert {
            domain: Domain::Task,
            op: "view-lists".to_string(),
            with: [
                ("target".to_string(), serde_yaml::Value::from("t")),
                ("view".to_string(), serde_yaml::Value::from("due")),
            ]
            .into_iter()
            .collect(),
            window: None,
        };
        assert!(
            Instructor::new().expectation(&lists).is_some(),
            "a title standing in a listing is read off the shot",
        );
    }

    /// A step with nothing on the step it names says so as an absence, and the line has to carry the
    /// reason: a badge that is simply not drawn and a badge an eye skipped over look alike on a shot.
    #[test]
    fn a_step_with_nothing_on_it_asks_for_no_badge_at_all() {
        let step = Step::Assert {
            domain: Domain::Task,
            op: "view-warns".to_string(),
            with: [
                ("view".to_string(), serde_yaml::Value::from("due")),
                ("step".to_string(), serde_yaml::Value::from("heed")),
                ("count".to_string(), serde_yaml::Value::from(0)),
            ]
            .into_iter()
            .collect(),
            window: None,
        };
        let said = Instructor::new().render(&step).unwrap();
        assert!(said.contains("no badge"), "got: {said}");
        assert!(said.contains("nothing stands on that step"), "and why there is none: {said}");
    }

    /// A view the sidebar does not draw, and a step the ladder does not have, are refused where they
    /// are written rather than carried to a screen as an instruction nobody can act on.
    #[test]
    fn a_view_or_a_step_the_screen_has_no_such_thing_of_is_refused() {
        let open = Step::Action {
            domain: Domain::Task,
            op: "open-view".to_string(),
            with: [("view".to_string(), serde_yaml::Value::from("overdue"))].into_iter().collect(),
            bind: None,
            window: None,
        };
        let err = Instructor::new().render(&open).unwrap_err();
        assert!(err.contains("overdue"), "got: {err}");

        let warns = Step::Assert {
            domain: Domain::Task,
            op: "view-warns".to_string(),
            with: [
                ("view".to_string(), serde_yaml::Value::from("due")),
                ("step".to_string(), serde_yaml::Value::from("plain")),
                ("count".to_string(), serde_yaml::Value::from(1)),
            ]
            .into_iter()
            .collect(),
            window: None,
        };
        let err = Instructor::new().render(&warns).unwrap_err();
        assert!(err.contains("plain"), "got: {err}");
    }

    /// A count written as words rather than as a number is refused here, not compared at the far end of
    /// a run. YAML types an unquoted scalar by its shape, so a quoted one arrives as a string — and a
    /// string is what the badge can never be.
    #[test]
    fn a_count_that_is_not_a_number_is_refused() {
        let step = Step::Assert {
            domain: Domain::Task,
            op: "view-warns".to_string(),
            with: [
                ("view".to_string(), serde_yaml::Value::from("due")),
                ("step".to_string(), serde_yaml::Value::from("stop")),
                ("count".to_string(), serde_yaml::Value::from("three")),
            ]
            .into_iter()
            .collect(),
            window: None,
        };
        let err = Instructor::new().render(&step).unwrap_err();
        assert!(err.contains("count"), "got: {err}");
    }

    /// An answer the record cannot hold is refused where it is written, the same way a consent answered
    /// with neither yes nor no is.
    #[test]
    fn an_answer_the_record_does_not_hold_is_refused() {
        let step = Step::Assert {
            domain: Domain::Repo,
            op: "ai-launch-answer".to_string(),
            with: [("answer".to_string(), serde_yaml::Value::from("maybe"))].into_iter().collect(),
            window: None,
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
                window: None,
            };
            let err = Instructor::new().render(&step).unwrap_err();
            assert!(err.contains(answer), "got: {err}");
        }
    }

    /// The board recut, and the one `carded` step that recut leaves to an eye. On any other axis the
    /// value is read off the shot as before; on the axis the columns are cut along, the heading over
    /// the card carries that word whichever way the card answers, so a reading would pass either way.
    #[test]
    fn the_axis_the_board_is_cut_along_leaves_its_card_to_review() {
        let regroup = Step::Action {
            domain: Domain::Project,
            op: "group-by".to_string(),
            with: [("axis".to_string(), serde_yaml::Value::from("Medium"))].into_iter().collect(),
            bind: None,
            window: None,
        };
        let said = Instructor::new().render(&regroup).unwrap();
        assert!(said.contains("Medium"), "the move names the axis to press: {said}");

        let carded = |grouping: bool| {
            let mut with: Args = [
                ("target".to_string(), serde_yaml::Value::from("t")),
                ("dimension".to_string(), serde_yaml::Value::from("Medium")),
                ("value".to_string(), serde_yaml::Value::from("print")),
                ("present".to_string(), serde_yaml::Value::from(false)),
            ]
            .into_iter()
            .collect();
            if grouping {
                with.insert("grouping".to_string(), serde_yaml::Value::from(true));
            }
            Step::Assert { domain: Domain::Task, op: "carded".to_string(), with, window: None }
        };
        assert!(
            Instructor::new().expectation(&carded(false)).is_some(),
            "an axis the board is not cut along is read off the shot",
        );
        assert!(
            Instructor::new().expectation(&carded(true)).is_none(),
            "the axis it is cut along is a Review",
        );
        let said = Instructor::new().render(&carded(true)).unwrap();
        assert!(said.contains("heading"), "and the line says where the word will be standing: {said}");
    }

    /// Filing work by carrying its card into a column, and the one column that will not take it. Both
    /// halves are the same move and the same three words — the axis, the card, the value on the heading
    /// — so what tells them apart is what the line says happens at the end of the drag. The refused one
    /// says the card comes back and nothing is said about it, because that is all a column which is no
    /// drop target ever does: an operator waiting for a sentence to appear would be waiting for a screen
    /// this build never draws.
    #[test]
    fn a_card_carried_into_a_column_says_when_the_column_will_not_take_it() {
        let drop = |value: &str, refused: bool| {
            let mut with: Args = [
                ("target".to_string(), serde_yaml::Value::from("fresh")),
                ("axis".to_string(), serde_yaml::Value::from("Release")),
                ("value".to_string(), serde_yaml::Value::from(value)),
            ]
            .into_iter()
            .collect();
            if refused {
                with.insert(
                    "refused".to_string(),
                    serde_yaml::Value::from("invalid_dimension_set_closed_value"),
                );
            }
            Step::Action {
                domain: Domain::Task,
                op: "drop-into-column".to_string(),
                with,
                bind: None,
                window: None,
            }
        };

        let landed = Instructor::new().render(&drop("v19", false)).unwrap();
        assert!(landed.contains("Release") && landed.contains("v19"), "got: {landed}");
        assert!(landed.contains("fresh"), "the card is named: {landed}");
        assert!(!landed.contains("turned away"), "a drop that lands walks no refusal: {landed}");

        let turned = Instructor::new().render(&drop("v18", true)).unwrap();
        assert!(turned.contains("takes no card"), "got: {turned}");
        assert!(
            turned.contains("turned away rather than to go through"),
            "and it is still a refused step: {turned}"
        );
    }

    /// The demand an axis can carry, and the two controls this face answers it with. A terminal meets
    /// both refusals as codes on an exit status; here there is no code to compare, so what stands in
    /// for each one is a control held shut — the box that would raise a demand nobody could answer,
    /// and the button that would end a creation the demand is not met on. The held creation is the one
    /// line whose instruction changes with `refused:`: a reader told to press a button that is shut
    /// would be hunting for a press that was never on offer.
    #[test]
    fn a_demand_an_axis_carries_is_answered_by_the_controls_it_holds_shut() {
        let raise = |dimension: &str, on: Option<bool>, refused: bool| {
            let mut with: Args =
                [("dimension".to_string(), serde_yaml::Value::from(dimension))].into_iter().collect();
            if let Some(on) = on {
                with.insert("required".to_string(), serde_yaml::Value::from(on));
            }
            if refused {
                with.insert(
                    "refused".to_string(),
                    serde_yaml::Value::from("invalid_dimension_required_without_values"),
                );
            }
            Step::Action { domain: Domain::Dimension, op: "required".to_string(), with, bind: None, window: None }
        };
        let said = Instructor::new().render(&raise("Focus", None, true)).unwrap();
        assert!(said.contains("Focus") && said.contains("turn on"), "got: {said}");
        assert!(said.contains("turned away rather than to go through"), "got: {said}");
        let lowered = Instructor::new().render(&raise("Medium", Some(false), false)).unwrap();
        assert!(lowered.contains("turn off"), "the other half of the same box: {lowered}");

        // The creation the demand holds, and the same step once it is answered. Both name the task, and
        // only the held one names the button as shut.
        let finish = |refused: bool| {
            let mut with: Args =
                [("target".to_string(), serde_yaml::Value::from("seed"))].into_iter().collect();
            if refused {
                with.insert(
                    "refused".to_string(),
                    serde_yaml::Value::from("invalid_task_required_dimension"),
                );
            }
            Step::Action { domain: Domain::Task, op: "finish-creating".to_string(), with, bind: None, window: None }
        };
        let held = Instructor::new().render(&finish(true)).unwrap();
        assert!(held.contains("shut") && held.contains("named beside it"), "got: {held}");
        let ended = Instructor::new().render(&finish(false)).unwrap();
        assert!(ended.contains("press the button"), "got: {ended}");

        // And the answer itself, put on from the task's own pane — the place the held button sends a
        // reader, so the line names the axis as well as the value.
        let set = Step::Action {
            domain: Domain::Dimension,
            op: "set".to_string(),
            with: [
                ("target".to_string(), serde_yaml::Value::from("seed")),
                ("dimension".to_string(), serde_yaml::Value::from("Medium")),
                ("value".to_string(), serde_yaml::Value::from("print")),
            ]
            .into_iter()
            .collect(),
            bind: None,
            window: None,
        };
        let said = Instructor::new().render(&set).unwrap();
        assert!(said.contains("Medium") && said.contains("print"), "got: {said}");
    }

    /// The key a category answers to, renamed on the one face that can rename it and read back off the
    /// field it was typed into. Both rows are walked — the axis's own key and one of its values' — since
    /// the manager draws a field beside each name and a line naming only the category would leave a
    /// reader typing into the wrong one. A refusal here is read on the key that was there before: the
    /// field is put back, so the old key standing is what says the guard bit.
    #[test]
    fn a_key_is_typed_beside_the_name_it_belongs_to_and_read_back_off_that_field() {
        let rekey = |value: Option<&str>, slug: &str, refused: Option<&str>| {
            let mut with: Args = [
                ("dimension".to_string(), serde_yaml::Value::from("Medium")),
                ("slug".to_string(), serde_yaml::Value::from(slug)),
            ]
            .into_iter()
            .collect();
            if let Some(value) = value {
                with.insert("value".to_string(), serde_yaml::Value::from(value));
            }
            if let Some(code) = refused {
                with.insert("refused".to_string(), serde_yaml::Value::from(code));
            }
            Step::Action { domain: Domain::Dimension, op: "rekey".to_string(), with, bind: None, window: None }
        };
        let axis = Instructor::new().render(&rekey(None, "channel", None)).unwrap();
        assert!(axis.contains("Medium") && axis.contains("channel"), "got: {axis}");
        assert!(axis.contains("categories"), "the field is in the manager: {axis}");
        let value = Instructor::new().render(&rekey(Some("print"), "paper", None)).unwrap();
        assert!(value.contains("print") && value.contains("paper"), "got: {value}");
        let turned =
            Instructor::new().render(&rekey(None, "focus", Some("invalid_dimension_slug_taken"))).unwrap();
        assert!(turned.contains("turned away rather than to go through"), "got: {turned}");

        // And the reading. A key is on the shot in the field it was typed into and nowhere else, so
        // both rows are judged rather than left to an eye.
        let read = |value: Option<&str>| {
            let mut with: Args = [
                ("dimension".to_string(), serde_yaml::Value::from("Medium")),
                ("equals".to_string(), serde_yaml::Value::from("channel")),
            ]
            .into_iter()
            .collect();
            if let Some(value) = value {
                with.insert("value".to_string(), serde_yaml::Value::from(value));
            }
            Step::Assert { domain: Domain::Dimension, op: "key".to_string(), with, window: None }
        };
        let exp = Instructor::new().expectation(&read(None)).expect("a key is read, not reviewed");
        assert_eq!(exp.text, "channel");
        assert!(exp.present, "the key is looked for rather than looked past");
        let said = Instructor::new().render(&read(Some("print"))).unwrap();
        assert!(said.contains("print") && said.contains("channel"), "got: {said}");
    }

    /// The crossing a setting is held at, named on a screen road — refused rather than passed over. A
    /// terminal answers that question by where it is typed and has to be told; a form is opened inside
    /// the row that already answered it, so the word can only mean a second picker there is none of.
    /// Silence here would leave a road looking green while writing and reading somewhere else.
    #[test]
    fn a_screen_road_naming_the_crossing_a_setting_sits_at_is_refused() {
        let with = || -> Args {
            [
                ("name".to_string(), serde_yaml::Value::from("worktree")),
                ("key".to_string(), serde_yaml::Value::from("worker_url")),
                ("project".to_string(), serde_yaml::Value::from("Greenhouse")),
                ("value".to_string(), serde_yaml::Value::from("https://example.test/board")),
                ("equals".to_string(), serde_yaml::Value::from("https://example.test/board")),
                ("readonly".to_string(), serde_yaml::Value::from(true)),
            ]
            .into_iter()
            .collect()
        };
        let wrote = Step::Action {
            domain: Domain::Plugin,
            op: "config-set".to_string(),
            with: with(),
            bind: None,
            window: None,
        };
        let read = Step::Assert { domain: Domain::Plugin, op: "config".to_string(), with: with(), window: None };
        for step in [wrote, read] {
            let err = Instructor::new().render(&step).unwrap_err();
            assert!(err.contains("Greenhouse"), "the refusal names what was written: {err}");
            assert!(err.contains("steps_cli"), "and where it belongs instead: {err}");
        }
    }

    /// Taking a day back off names the button beside that day, since the picker's own way of emptying
    /// itself is not drawn on every platform — and a field the screen keeps no such button for is
    /// refused where it is written rather than sent to an operator who would go looking for one.
    #[test]
    fn taking_a_day_off_names_the_button_beside_it_and_no_other_field_has_one() {
        let clear = |field: &str| Step::Action {
            domain: Domain::Task,
            op: "clear".to_string(),
            with: [
                ("target".to_string(), serde_yaml::Value::from("seed")),
                ("field".to_string(), serde_yaml::Value::from(field)),
            ]
            .into_iter()
            .collect(),
            bind: None,
            window: None,
        };

        for (field, day) in [("due", "due date"), ("start", "start date")] {
            let said = Instructor::new().render(&clear(field)).unwrap();
            assert!(said.contains(day), "the line names which day: {said}");
            assert!(said.contains("button beside"), "and where the button stands: {said}");
        }

        let err = Instructor::new().render(&clear("priority")).unwrap_err();
        assert!(err.contains("priority"), "the refusal names what was written: {err}");
    }

    /// And the reading that closes it: a field written as `null` is asked for as an absence on the
    /// pane, never as the word — there is nothing on screen for an operator to match that against.
    #[test]
    fn a_field_written_as_null_is_read_as_nothing_standing_there() {
        let step = Step::Assert {
            domain: Domain::Task,
            op: "field".to_string(),
            with: [
                ("target".to_string(), serde_yaml::Value::from("seed")),
                ("field".to_string(), serde_yaml::Value::from("due_on")),
                ("equals".to_string(), serde_yaml::Value::Null),
            ]
            .into_iter()
            .collect(),
            window: None,
        };
        let said = Instructor::new().render(&step).unwrap();
        assert!(said.contains("shows no due_on"), "the absence is what is asked for: {said}");
        assert!(!said.contains("null"), "and never the word itself: {said}");
        assert!(
            Instructor::new().expectation(&step).is_none(),
            "nothing standing there is not a reading",
        );
    }

    /// The moves a page is worked with, and the two ways a pane is opened. What the road has to be
    /// able to say is which control was pressed: a pane is opened by pressing the empty frame either
    /// way, and the strip a full page draws is a press before it that only moves the screen, so a
    /// step that named neither would leave an operator to pick between them.
    #[test]
    fn the_page_moves_name_the_control_they_press() {
        let s = load(r#"
id: x
title: A page is re-cut, paged and opened on
steps_gui:
  - type: action
    domain: terminal
    op: set-panes
    with: { count: 1 }
  - type: action
    domain: terminal
    op: go-page
    with: { page: 2 }
  - type: action
    domain: terminal
    op: open-pane
    with: { from: face }
  - type: action
    domain: terminal
    op: open-pane
    with: { from: strip }
"#);
        let mut ins = Instructor::new();
        let lines: Vec<String> =
            s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(lines[0].contains("pane counts, press the one that says 1"), "got: {}", lines[0]);
        assert!(lines[1].contains("page digits, press 2"), "got: {}", lines[1]);
        assert!(
            lines[2].contains("the empty frame") && !lines[2].contains("beside the name"),
            "the face's way in is the empty frame and nothing before it: {}",
            lines[2]
        );
        assert!(
            lines[3].contains("the thin strip")
                && lines[3].contains("Nothing opens")
                && lines[3].contains("the empty frame waiting there"),
            "the strip moves the screen, and the pane opens at the empty frame: {}",
            lines[3]
        );
        // Both say to leave the row above the press alone: which agent a pane opens with is another
        // road's, and a step silent about it would be walked with whatever happened to be on. Both
        // also say what to do on the machine where nothing on it is on yet, since there the press
        // does not answer and an operator left to work that out would mark a working face red.
        for one in [&lines[2], &lines[3]] {
            assert!(one.contains("leaving the row above that press as it came up"), "got: {one}");
            assert!(one.contains("choose any of them first"), "got: {one}");
        }
    }

    /// Where the project keeps several folders, both ways in meet the same question and neither opens
    /// anything — so the press is said per control and what follows it is said once. An operator told
    /// "nothing is asked" while a question stood in front of them would mark a working face red.
    #[test]
    fn a_press_that_meets_the_folder_question_does_not_promise_a_pane() {
        let open = |from: &str, asks: bool| Step::Action {
            domain: Domain::Terminal,
            op: "open-pane".to_string(),
            with: [
                ("from".to_string(), serde_yaml::Value::from(from)),
                ("asks".to_string(), serde_yaml::Value::from(asks)),
            ]
            .into_iter()
            .collect(),
            bind: None,
            window: None,
        };

        for from in ["face", "strip"] {
            let said = Instructor::new().render(&open(from, true)).unwrap();
            assert!(said.contains("Nothing opens"), "the press opens no pane: {said}");
            assert!(said.contains("which of them it works in"), "and what it meets instead: {said}");
            assert!(!said.contains("nothing is asked"), "a question is standing: {said}");
        }

        let quiet = Instructor::new().render(&open("face", false)).unwrap();
        assert!(quiet.contains("nothing is asked"), "one folder is not a question: {quiet}");
    }

    /// Answering it. The row is found by the folder's name at the end of the path it draws — a road
    /// knows what it calls a folder and not where the run put it — and the list the answer is picked
    /// out of is itself what the step is about, so the operator is told what may be on it.
    #[test]
    fn the_folder_a_pane_works_in_is_picked_out_of_this_projects_own() {
        let step = Step::Action {
            domain: Domain::Terminal,
            op: "pick-folder".to_string(),
            with: [("dir".to_string(), serde_yaml::Value::from("greenhouse-benches"))]
                .into_iter()
                .collect(),
            bind: None,
            window: None,
        };
        let said = Instructor::new().render(&step).unwrap();
        assert!(said.contains("greenhouse-benches"), "got: {said}");
        assert!(said.contains("path ends in"), "a road names a folder, not a place: {said}");
        assert!(said.contains("no picker"), "the list is the goal: {said}");
    }

    /// Going to a project. What the step says is where the screen is afterwards rather than that it
    /// changed: which project the face came up on is the run's business, so a road may press the one
    /// already shown, and that press is allowed to do nothing.
    #[test]
    fn going_to_a_project_says_where_the_face_lands_rather_than_that_it_moved() {
        let step = Step::Action {
            domain: Domain::Terminal,
            op: "go-project".to_string(),
            with: [("project".to_string(), serde_yaml::Value::from("Greenhouse"))]
                .into_iter()
                .collect(),
            bind: None,
            window: None,
        };
        let said = Instructor::new().render(&step).unwrap();
        assert!(said.contains("\"Greenhouse\""), "got: {said}");
        assert!(
            said.contains("not one in the list of panes below it"),
            "a pane's row is a different move: {said}"
        );
        assert!(
            !said.contains("changes") && !said.contains("swaps"),
            "the press may land where the face already was: {said}"
        );
    }

    /// A way in the face does not have is refused here rather than rendered into a sentence nobody
    /// could carry out — the same fail-closed contract an unmapped op keeps.
    #[test]
    fn a_way_into_a_pane_the_face_does_not_offer_is_refused() {
        let step = Step::Action {
            domain: Domain::Terminal,
            op: "open-pane".to_string(),
            with: [("from".to_string(), serde_yaml::Value::from("keyboard"))]
                .into_iter()
                .collect(),
            bind: None,
            window: None,
        };
        let err = Instructor::new().render(&step).unwrap_err();
        assert!(err.contains("face or strip"), "got: {err}");
    }

    /// What the empty frame will open with is read on the frame rather than on the pane it has not
    /// made yet, and the press is read beside the row: nothing on it being on is the one state that
    /// stops that press, so a step that only looked at which name is lit could pass on a frame that
    /// opens nothing.
    #[test]
    fn the_empty_frame_is_read_for_what_it_will_open_with_and_for_a_live_press() {
        let step = Step::Assert {
            domain: Domain::Terminal,
            op: "opens-with".to_string(),
            with: [("start".to_string(), serde_yaml::Value::from("shell"))]
                .into_iter()
                .collect(),
            window: None,
        };
        let said = Instructor::new().render(&step).unwrap();
        assert!(said.contains("plain shell is the one that is on"), "got: {said}");
        assert!(said.contains("press is live"), "the press is half the reading: {said}");
        assert!(
            said.contains("nothing to this reading"),
            "the row carries what this machine has not got as well, so the step has to put those out \
             of the reading rather than leave an operator counting them: {said}"
        );
        assert!(
            Instructor::new().expectation(&step).is_none(),
            "the names on the row are the interface's own words, so no reading is expected off the shot"
        );
    }

    /// The first run, which is the one reading on that frame that is not a program: the row is read
    /// for being drawn and blank, and the press for refusing to open. Both halves, because either
    /// alone passes on a build carrying the other fault.
    #[test]
    fn the_first_run_is_read_as_a_row_with_nothing_on_it_and_a_press_that_asks() {
        let step = Step::Assert {
            domain: Domain::Terminal,
            op: "opens-with".to_string(),
            with: [("start".to_string(), serde_yaml::Value::from("none"))]
                .into_iter()
                .collect(),
            window: None,
        };
        let said = Instructor::new().render(&step).unwrap();
        assert!(said.contains("none of them is on"), "got: {said}");
        assert!(
            said.contains("does not open a pane"),
            "the press refusing is half the reading: {said}"
        );
        assert!(
            said.contains("several things on it"),
            "a blank row has to be a row that is there: {said}"
        );
        assert!(
            Instructor::new().expectation(&step).is_none(),
            "the names on the row are the interface's own words, so no reading is expected off the shot"
        );
    }

    /// An agent named on that row is refused rather than rendered. Which agents are on it is a probe
    /// of the run machine's own `PATH`, so a road that named one would be a road that runs where that
    /// tool happens to be installed and nowhere else.
    #[test]
    fn a_start_the_row_cannot_be_named_by_is_refused() {
        let step = Step::Assert {
            domain: Domain::Terminal,
            op: "opens-with".to_string(),
            with: [("start".to_string(), serde_yaml::Value::from("claude-code"))]
                .into_iter()
                .collect(),
            window: None,
        };
        let err = Instructor::new().render(&step).unwrap_err();
        assert!(err.contains("that machine's own"), "got: {err}");
    }

    /// What the lit face of the lamp rests on: a pane set printing carries on putting lines out while
    /// the road walks around it, and the line it ends with is what the road waits on. The command is
    /// spelt out because an improvised one is the step itself being improvised, and both counts are
    /// there because a pane comes up in one of two shells.
    #[test]
    fn a_pane_is_set_printing_with_a_line_that_says_when_it_stopped() {
        let step = Step::Action {
            domain: Domain::Terminal,
            op: "keep-printing".to_string(),
            with: [("text".to_string(), serde_yaml::Value::from("SCENARIO that is all"))]
                .into_iter()
                .collect(),
            bind: None,
            window: None,
        };
        let said = Instructor::new().render(&step).unwrap();
        assert!(said.contains("ping -c 30"), "the run is bounded, and spelt out: {said}");
        assert!(said.contains("-n 30"), "the other shell counts its pings differently: {said}");
        assert!(
            said.matches("SCENARIO that is all").count() == 2,
            "the road's own line is what it ends with and what the road waits on: {said}"
        );
        assert!(
            said.contains("Leave the pane alone"),
            "a pane stopped by hand is a pane the road put out itself: {said}"
        );
    }

    /// The lamp's three faces, and the one of them that is watched rather than shot. The two still
    /// ones are a picture; the blink rests, twice a turn, at a step a photograph cannot tell from
    /// them — so only that half tells the operator to watch, and none of the three is a reading.
    #[test]
    fn the_lamp_is_read_by_its_face_and_the_blinking_one_is_watched() {
        let dot = |face: &str| Step::Assert {
            domain: Domain::Terminal,
            op: "dot".to_string(),
            with: [("face".to_string(), serde_yaml::Value::from(face))].into_iter().collect(),
            window: None,
        };
        let lit = Instructor::new().render(&dot("lit")).unwrap();
        assert!(lit.contains("glow") && lit.contains("holding still"), "got: {lit}");
        let calling = Instructor::new().render(&dot("calling")).unwrap();
        assert!(calling.contains("blinking"), "got: {calling}");
        assert!(
            calling.contains("watch") && calling.contains("warning colour"),
            "the one face that moves, and the one that leaves the pane's own hue: {calling}"
        );
        let out = Instructor::new().render(&dot("out")).unwrap();
        assert!(out.contains("sunk"), "got: {out}");
        assert!(
            out.contains("not the pane having gone"),
            "out is the resting state, and a lamp that vanished would say the pane had: {out}"
        );
        let err = Instructor::new().render(&dot("pulsing")).unwrap_err();
        assert!(err.contains("lit, calling or out"), "got: {err}");
        assert!(
            Instructor::new().expectation(&dot("calling")).is_none(),
            "a mark with no words on it is not a reading",
        );
    }

    /// Running a command for its output, and pressing a ref out of what it drew. Three things are
    /// worth holding: the pane is cleared before a command, since "the ref" is otherwise one of
    /// several places on the screen; a command needing a record's own number says so, because a road
    /// cannot spell a number the run will mint; and the folded press asks for the fold rather than
    /// for a width, which is the run machine's to settle.
    #[test]
    fn a_ref_is_pressed_out_of_what_a_command_drew() {
        let s = load(r#"
id: x
title: A ref drawn in a pane is pressed
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: SEED }
    as: seed
  - type: action
    domain: terminal
    op: run
    with: { command: amenbo task list --actor ai }
  - type: action
    domain: terminal
    op: press-ref
    with: { target: seed }
  - type: action
    domain: terminal
    op: run
    with: { command: 'echo "... <ref>"', target: seed }
  - type: action
    domain: terminal
    op: press-ref
    with: { target: seed, folded: true }
"#);
        let mut ins = Instructor::new();
        let lines: Vec<String> =
            s.steps(Driver::Gui).iter().map(|st| ins.render(st).unwrap()).collect();
        assert!(
            lines[1].contains("clear what is on it") && lines[1].contains("amenbo task list"),
            "a command is run on a cleared pane: {}",
            lines[1]
        );
        assert!(!lines[1].contains("<ref>"), "a command needing no ref says nothing of one: {}", lines[1]);
        assert!(
            lines[2].contains("\"SEED\"") && !lines[2].contains("broken across"),
            "an ordinary press names the record and no fold: {}",
            lines[2]
        );
        assert!(
            lines[3].contains("<ref>") && lines[3].contains("\"SEED\""),
            "a command that needs the number names the record it belongs to: {}",
            lines[3]
        );
        assert!(
            lines[4].contains("broken across two rows") && lines[4].contains("Drag"),
            "the folded press asks for the fold, not for a width: {}",
            lines[4]
        );
    }

    #[test]
    fn an_unmapped_op_fails_closed() {
        let step = Step::Action {
            domain: Domain::Task,
            op: "frobnicate".to_string(),
            with: Args::new(),
            bind: None,
            window: None,
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
            |_, p| {
                *shots.borrow_mut() += 1;
                std::fs::write(p, b"fake-png").map_err(|e| e.to_string())
            },
            // The board OCRs to text that contains the seed title.
            |_| Ok(reading("me-ai board\nSEED\nsome other card")),
            |_| Ok(()),
            || Ok(()),
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

    /// A road that names a window says it to all three: the operator standing at the screen, the tool
    /// taking the shot, and whoever reads the manifest afterwards. The tool's half is what nothing
    /// else can recover — two windows of one app are the same app at the same size, so a shot cannot
    /// be traced back to the window it is of.
    #[test]
    fn a_window_a_road_names_reaches_the_operator_the_tool_and_the_manifest() {
        let s = load(&SCENARIO.replace(
            "    with: { filter: \"assignee:me-ai status:todo\", target: seed, present: true }",
            "    with: { filter: \"assignee:me-ai status:todo\", target: seed, present: true }\n    window: \"Amenbo — Terminal\"",
        ));
        let dir = std::env::temp_dir().join(format!("amenbo-verify-gui-window-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let mut aimed: Vec<Option<String>> = Vec::new();
        let mut handed: Vec<String> = Vec::new();
        let outcome = walk(
            &s,
            &dir,
            |window, p| {
                aimed.push(window.map(str::to_string));
                std::fs::write(p, b"fake-png").map_err(|e| e.to_string())
            },
            |_| Ok(reading("SEED — a card on the me-ai board")),
            |brief| {
                handed.push(brief.instruction.to_string());
                Ok(())
            },
            || Ok(()),
        )
        .expect("walk");

        // The tool is aimed at the named window on the step that named one, and at the app's one
        // window on the steps that did not — a road says which screen per step, not per run.
        assert_eq!(aimed, vec![None, None, Some("Amenbo — Terminal".to_string())]);
        // The operator is told where to stand before being told what to do there.
        assert!(
            handed[2].starts_with("In the window called \"Amenbo — Terminal\": "),
            "got: {}",
            handed[2]
        );
        assert_eq!(outcome.records[2].window.as_deref(), Some("Amenbo — Terminal"));

        let manifest = write_manifest(&dir, &s, &[], &outcome).expect("manifest");
        let text = std::fs::read_to_string(&manifest).unwrap();
        assert!(text.contains("\"window\":\"Amenbo — Terminal\""), "got: {text}");
        // And a step that named none carries no window at all, rather than a null saying it was
        // asked and left blank.
        assert_eq!(text.matches("\"window\"").count(), 1, "got: {text}");
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
            |_, p| std::fs::write(p, b"fake-png").map_err(|e| e.to_string()),
            |_| Ok(reading("an empty board with no such card")),
            |_| Ok(()),
            || Ok(()),
        )
        .expect("walk");

        assert!(!outcome.passed, "SEED absent ⇒ the present-assert fails");
        let assert_rec = outcome.records.iter().find(|r| r.kind == "assert").unwrap();
        assert_eq!(assert_rec.verdict, Verdict::Fail);
        assert_eq!(assert_rec.found, Some(false));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The reading that started this: one glyph inside a word Vision otherwise read perfectly. The
    /// fold cannot help — `days` and `davs` are two words to it — so the tolerance is what carries it.
    #[test]
    fn one_misread_character_still_meets_the_words() {
        let title = "Post the day's finished tasks to the team channel";
        let misread = "Post the dav's finished tasks to the team channel";
        let hit = held(&fold(misread), &fold(title));
        assert_eq!(hit, Held { found: true, slipped: true });

        // And the reading that needed nothing forgiven says so, which is what keeps the two greens
        // apart in the evidence.
        assert_eq!(held(&fold(title), &fold(title)), Held { found: true, slipped: false });
    }

    /// The reading that this one came from: a category's key drawn in a monospace face, where the
    /// `l` came back as a `1`. It is seven characters, so the budget below cannot reach it — the
    /// confusable fold is the whole of what makes it green.
    #[test]
    fn a_key_misread_on_a_glyph_the_screen_draws_alike_still_meets_it() {
        assert_eq!(held("medium channe1", "channel"), Held { found: true, slipped: true });
        // The other pair, and the fold going the other way — the expectation carrying the digit and
        // the reading the letter.
        assert_eq!(held("the r0ute", "route"), Held { found: true, slipped: true });
        assert_eq!(held("the route", "r0ute"), Held { found: true, slipped: true });
    }

    /// What the fold does not buy: a key that differs anywhere else is still a different key, however
    /// short it is.
    #[test]
    fn a_key_that_differs_elsewhere_stays_red() {
        assert_eq!(held("medium channe1", "channet"), Held { found: false, slipped: false });
        assert_eq!(held("medium focus", "channel"), Held { found: false, slipped: false });
        // And a lowercase `i` is not in the pairs, so it keeps its own glyph.
        assert_eq!(held("medium channei", "channel"), Held { found: false, slipped: false });
    }

    /// The floor. A short expectation is a word where one character is most of the meaning, and two
    /// values a scenario really tells apart are exactly that far from each other.
    #[test]
    fn a_short_expectation_forgives_nothing() {
        assert_eq!(held("gore", "core"), Held { found: false, slipped: false });
        // Long enough, and the same single slip is met.
        assert_eq!(
            held("still to be taken", "still to be token"),
            Held { found: true, slipped: true }
        );
    }

    /// The budget is one character over the whole expectation, so a reading that went wrong twice is
    /// a shot for a person rather than a green.
    #[test]
    fn two_slips_are_not_forgiven() {
        assert_eq!(
            held("the boavd is navrowed", "the board is narrowed"),
            Held { found: false, slipped: true }
        );
    }

    /// Counted in characters and not in words, because the screen under test is also read in
    /// Japanese, where the fold leaves a title with no spaces to count.
    #[test]
    fn the_tolerance_reaches_a_language_the_fold_leaves_unspaced() {
        let title = fold("絞り込んだあとに板へ残る仕事");
        let misread = fold("絞り込んだあとに版へ残る仕事");
        assert!(!title.contains(' '), "the fold leaves this one word");
        assert_eq!(held(&misread, &title), Held { found: true, slipped: true });
    }

    /// Which way the looseness leans. The same tolerance that finds a misread title on a step saying
    /// it should be there finds it on a step saying it should be gone — so it can red a run and never
    /// green one on a screen nobody stood up.
    #[test]
    fn a_forgiven_reading_counts_against_a_step_that_wanted_the_card_gone() {
        let yaml = r#"
id: x
title: y
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: SCENARIO — nobody holds it }
    as: nobodys
  - type: assert
    domain: task
    op: narrowed
    with: { target: nobodys, present: false }
"#;
        let s = load(yaml);
        let dir = std::env::temp_dir().join(format!("amenbo-verify-gui-slip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let outcome = walk(
            &s,
            &dir,
            |_, p| std::fs::write(p, b"fake-png").map_err(|e| e.to_string()),
            |_| Ok(reading("SCENARIO — nobodv holds it")),
            |_| Ok(()),
            || Ok(()),
        )
        .expect("walk");

        assert!(!outcome.passed, "the card is still on screen, misread or not");
        let rec = outcome.records.iter().find(|r| r.kind == "assert").unwrap();
        assert_eq!(rec.found, Some(true));
        assert!(rec.slipped, "and the evidence says the words met on a forgiven character");

        let manifest = write_manifest(&dir, &s, &[], &outcome).expect("manifest");
        let text = std::fs::read_to_string(manifest).unwrap();
        assert!(text.contains("\"slipped\":true"), "got: {text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_capture_failure_aborts_the_walk() {
        let s = load(SCENARIO);
        let dir = std::env::temp_dir().join(format!("amenbo-verify-gui-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let err = walk(&s, &dir, |_, _| Err("no screen".to_string()), |_| Ok(reading("")), |_| Ok(()), || Ok(()))
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
            |_, p| {
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
            || Ok(()),
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

    /// The one step nobody at the screen carries out. The app is put through a run of its own before
    /// the step is handed over, and the order is the whole of it: handed the step first, the operator
    /// would be asked to confirm a new window while still standing in front of the old one.
    #[test]
    fn the_app_is_run_again_before_that_step_is_handed_over() {
        let s = load(r#"
id: x
title: y
steps_gui:
  - type: action
    domain: task
    op: create
    with: { title: SEED }
    as: seed
  - type: action
    domain: store
    op: run-again
"#);
        let dir = std::env::temp_dir().join(format!("amenbo-verify-gui-again-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let done: RefCell<Vec<String>> = RefCell::new(Vec::new());
        let outcome = walk(
            &s,
            &dir,
            |_, p| {
                done.borrow_mut().push("shot".to_string());
                std::fs::write(p, b"fake-png").map_err(|e| e.to_string())
            },
            |_| Ok(reading("")),
            |b| {
                done.borrow_mut().push(format!("handed {}", b.index));
                Ok(())
            },
            || {
                done.borrow_mut().push("ran again".to_string());
                Ok(())
            },
        )
        .expect("walk");

        assert_eq!(
            *done.borrow(),
            vec!["handed 0", "shot", "ran again", "handed 1", "shot"],
            "the app is started again for that step alone, and before it is handed over"
        );
        assert_eq!(outcome.records[1].op, "run-again", "and the step is recorded like any other");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An app that would not come up again ends the walk there. Every step after it would be shot
    /// against nothing running, and those shots would read on the manifest like any others.
    #[test]
    fn an_app_that_will_not_start_again_aborts_the_walk() {
        let s = load(r#"
id: x
title: y
steps_gui:
  - type: action
    domain: store
    op: run-again
  - type: assert
    domain: terminal
    op: frames
    with: { count: 0, empty: 1 }
"#);
        let dir = std::env::temp_dir().join(format!("amenbo-verify-gui-again-red-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let shots: RefCell<usize> = RefCell::new(0);
        let err = walk(
            &s,
            &dir,
            |_, p| {
                *shots.borrow_mut() += 1;
                std::fs::write(p, b"fake-png").map_err(|e| e.to_string())
            },
            |_| Ok(reading("")),
            |_| Ok(()),
            || Err("no window came up".to_string()),
        )
        .unwrap_err();

        assert!(err.contains("step 1") && err.contains("no window came up"), "got: {err}");
        assert_eq!(*shots.borrow(), 0, "nothing was shot");
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
            |_, p| {
                *shots.borrow_mut() += 1;
                std::fs::write(p, b"fake-png").map_err(|e| e.to_string())
            },
            |_| Ok(reading("")),
            |_| Err("nobody is watching".to_string()),
            || Ok(()),
        )
        .unwrap_err();
        assert!(err.contains("step 1") && err.contains("nobody is watching"), "got: {err}");
        assert_eq!(*shots.borrow(), 0, "nothing was shot");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
