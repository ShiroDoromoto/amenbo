//! The `workspace` domain's one premise: **the machine a pane would be opened on**.
//!
//! Everything else this domain names is a screen's — a pane is what a reader is already typing in,
//! so the moves are the operator's and this driver walks none of them (`crate::Driver::action`).
//! What is here is the world underneath them, and it is here because it is a world: which agents a
//! pane can be opened with is settled before the app comes up, the same as a project already on the
//! board.
//!
//! **The build asks the operator's own machine, so a road has to answer for it.** What a frame
//! offers to open a pane with is every agent the build could find, and it finds them by running the
//! pane's login shell over the `PATH` that shell reads (`app/src-tauri/src/launch.rs`). Left alone,
//! the row is therefore whatever the person running the gate happens to have installed — nothing on
//! it where several were found, one on where a single was, no row at all where none were, all three
//! correct — so a road reading it would pass or fail by the machine and not by the build.
//!
//! **A directory in front of the `PATH` is how it is answered**, and it is the whole of the reach:
//! the programs stand in a directory the session owns, the GUI harness hands it to the app it
//! launches and to nothing else (`amenbo_verify_gui::launch`), and it goes when the session does.
//! Nothing is installed, nothing outside the run can see it, and the build under test is the shipped
//! one being asked the question it always asks.
//!
//! **It can only add.** Taking an install away would mean handing the probe a `PATH` the operator's
//! own profile could not put back, and that profile is read every time the shell starts — so the
//! count a road asks for is a floor, and the shape that can be stood up is "more than one thing to
//! open with". That is the shape worth having: it is the one the first run is read on.
//!
//! **And a name the machine already answers for is a name the run does not get.** The directory is
//! handed over before the shell starts and the profile is read after, so one line putting the
//! operator's own `~/.local/bin` in front is the whole of it: `claude` is their install, and the
//! stand-in written under that name is never reached. Nothing on the row says so — a road that
//! counts what a pane can be opened with passes either way — and what changes is every road that
//! reads a stand-in's own behaviour, which becomes a road about whatever that operator happens to
//! have — a run measured on 2026-09-14 opened their real Claude Code and left it running. So the
//! premise asks, after it has written them, what a pane's own shell answers for each of those names
//! ([`nothing_else_answers`]) and refuses to stand where the answer is not the program it just
//! wrote. A machine these roads are walked on therefore carries its own agents *behind* the `PATH`
//! it hands a run, never in front of it (`devtool/vmclaude.go`).
//!
//! **The stand-ins answer two things, and a road says how much of the second it wants.** The first is
//! what they were always for: a program is on the `PATH` under an agent's name, so the build finds it
//! and draws the row. The second is the question the frame puts to an agent once it is chosen — which
//! models it can be started on — and `models` is how many names come back. Left off, nothing answers
//! and the row draws the shape a provider with no list gives, which is the road that was there before
//! this ([`model_names`]). Asked for, each stand-up answers **in its own provider's shape**, because
//! the shapes are what the build reads by (`amenbo_core::agent_models::Reading`): Claude Code's is a
//! paragraph of help text, Codex CLI's a JSON catalog, OpenCode's qualified lines, Cursor's `id -
//! label`, Gemini CLI's an ACP session answer, and GitHub Copilot's is nothing at all, which is that
//! provider having no door to ask at.
//!
//! **Two of them can also say which model they are on**, and `on` is which — the position of one of
//! the names they answer with. Only Cursor and Gemini CLI have anywhere in their answer to put it
//! (`amenbo_core::agent_models::Answer::current`), so the same premise stands up both readings a
//! road about a provider's own default needs: the two naming one, and the four naming none. Left
//! off, nobody says, which is the machine every road before this one stood up.
//!
//! **And a road may say what one leaves with.** A stand-in ends successfully unless `exits` names
//! another status, which is how a road stands up an ending a provider's own number is the whole
//! answer to: the pane says the program ended, and for a few numbers it says what the ending was
//! about (`app/src/talk/terminal.ts`). The number is the road's own here, and it is written as the
//! provider writes it.
//!
//! **A road may also ask them to stay and read.** Left alone a stand-in prints and ends, which is the
//! pane every road before this one read: the output stands on the screen and nothing is running. A
//! pane whose model is *moved* has to still be running — there is no control on a frame whose program
//! has gone — and what says the move arrived is the program reading it, so `then: reads` is a stand-in
//! that waits and prints back every line it is given, in brackets ([`SAID`]) so the whole of the line
//! is read and never a piece of one.
//!
//! **And `then: runs` carries the line out as well as printing it.** What it buys is a pane of an
//! agent that a road can type a command into: a plain shell is the only other pane a road can say
//! anything in, and a shell is the one pane with no way back into it. So a road that
//! needs a record made in a session somebody could come back to types the create at one of these
//! (`amenbo_core::session::PANE_RESUME_VAR`). It is the reading half's own shape with
//! the line run after it is printed, so what a road reads back is the command's own output.
//!
//! **And a road may ask them to refuse a way back.** A stand-in keeps no sessions, so every handle
//! it is ever given is one it did not issue — and what it does with one is the road's to say.
//! `comes-back: false` is the stand-in that says so and exits ([`NO_SESSION`]), which is the one
//! shape a road can reach the pane's own account of a conversation that is not there any more by
//! (`app/src/talk/terminal.ts`, `whyItStopped`). Left alone it carries on regardless, which is what
//! every road that opens the app again and finds its panes resumed reads.
//!
//! **And they print what they were started with.** A model chosen on the frame is a flag on a command
//! line and nothing else, so the only thing that says the choice arrived is the program it arrived at
//! — the line the frame writes out under the row is Amenbo's own account of what a press would do, and
//! a build that drew it right and started the pane wrong would keep that promise falsely. Each
//! argument is printed on a line of its own, marked ([`ARG`]), so a road reads back the name it wrote
//! and never a spelling belonging to one tool.

use std::ffi::OsString;
use std::path::Path;

use amenbo_scenario::{Args, Domain};

use crate::{req_i64, unmapped, Driver, Outcome};

/// The commands the build looks for, in the order it lists them (`amenbo_core::harness::LAUNCHES`).
///
/// Written down here rather than asked, because no face of the shipped binary hands the catalog out
/// — `agent-hook snippet` answers with a tool's wiring text and never with the program it is
/// started as. What that costs is drift: a command the build renamed leaves the stand-in unfound,
/// and the machine reads as whatever the operator has. The row this premise exists to stand up is
/// then not standing, and the assert that reads it (`opens-with` with `start: none`) is what says
/// so — on any machine with fewer than two agents of its own.
const COMMANDS: &[&str] = &["claude", "codex", "copilot", "gemini", "opencode", "cursor-agent"];

/// The mark every argument a stand-in was started with is printed under.
///
/// A word of the harness's own rather than the tool's, and one line per argument rather than the
/// whole line at once: what a road reads back is the model name it wrote, which is spelled the same
/// whichever agent carried it, and never the flag in front of it — `--model` on three of the six and
/// `-m` on the other three, so a road reading a flag would be a road about one tool.
const ARG: &str = "SCENARIO arg";

/// The mark a line typed at a stand-in is printed back under, and the brackets it is printed inside.
///
/// **The brackets are what make the reading exact.** A road that moves a pane to a model reads back
/// the line Amenbo put there, and the fault it is watching for is a name on a line that must not
/// carry one — so `SCENARIO said [/model]` and `SCENARIO said [/model a-name]` have to be two
/// readings and not one that contains the other.
///
/// What arrives is wrapped as a bracketed paste (`app/src/talk/terminal.ts`), so the markers are cut
/// off before the line is printed: they are the terminal's envelope and not what anybody typed.
const SAID: &str = "SCENARIO said";

/// The mark a stand-in prints when it is stopped where it stands, before it goes.
///
/// **A program that dies quietly is a press nobody can read.** What a stop does is end the thing
/// running in the pane, and a shell prompt answers that with a mark and a fresh prompt — the same
/// two it was already drawing, which a shot cannot tell from the ones before the press. So a
/// stand-in that reads says so in a word of its own, and the road reads that word: the press
/// arrived, and what answered it is the program it was aimed at.
///
/// It is printed from a trap on the interrupt rather than left to the shell, so what a road reads is
/// the program's own last word rather than the absence of one.
const STOPPED: &str = "SCENARIO stopped";

/// The mark a stand-in prints when it is handed a way back it will not honour, before it goes.
///
/// A word of the harness's own rather than a provider's sentence: what each of the six says when it
/// is given a session id it never issued is its own, and a road that read one would be a road about
/// that tool. What a road reads is the pane's answer to the ending rather than this line — the line
/// is here so that a pane which stopped in moments is not a pane that stopped in silence.
const NO_SESSION: &str = "SCENARIO no session";

/// The question a stand-in holding one draws, and the choices under it.
///
/// **It is a question and not a prompt, which is the whole of what a road stands this shape up for.**
/// What a program draws while it waits for an answer is its own — a dialogue, a list, a box — and a
/// newline arriving there picks whatever is first. So the stand-in draws something a reader would
/// have to answer, and the road reads whether anything answered it for them.
///
/// The words are the harness's own, for the reason every quoted word here is: what a provider's own
/// question says differs per product and per version, and a road that read one would be a road about
/// that tool.
const ASKING: &str = "SCENARIO which way out of here";
const CHOICES: &str = "SCENARIO 1) the north channel   2) the south channel";

/// The mark a stand-in holding a question prints for every line it is given — the whole of it, with
/// nothing of what the line said.
///
/// **What is being read is that a line arrived at all, so the line's own words would only get in the
/// way.** A rename that was never sent is sitting in the pane's input box, and it goes in on the next
/// press a person makes; a stand-in that printed what it was given would put those words on the
/// screen, where the loop handing them over reads its own text back and takes it for the paste having
/// landed (`app/src-tauri/src/handover.rs`, `echoed`). The mark says the same thing without teaching
/// the screen anything.
const ARRIVED: &str = "SCENARIO a line arrived";

/// What every model a stand-in answers with is called, before its number.
///
/// The road's own word, for the reason a road never writes a real model name: the names on that row
/// belong to the providers and to the account a run happens to be signed in to.
const MODEL: &str = "scenario-model-";

/// The most models a stand-in will answer with. The number is written two digits wide so that no
/// name is a piece of another — a road narrowing the row to `-17` means one of them — and three
/// digits is a row nobody would read to the end of anyway.
const MOST_MODELS: i64 = 99;

/// The names `count` models are answered under, in the order they are printed.
fn model_names(count: i64) -> Vec<String> {
    (1..=count).map(|n| format!("{MODEL}{n:02}")).collect()
}

/// How one agent is asked what it can be started on, and what it answers — the command line the ask
/// arrives as, and the text printed back.
///
/// **One shape per provider, because the providers do not agree there is one**
/// (`amenbo_core::agent_models::Reading`). They are written down here rather than asked for, which is
/// the same bargain the catalog of commands above is written under: what it costs is drift, and what
/// a drifted shape reads as is an agent with no models to offer — the row a road walked for a list
/// then draws a box to type in, and the step that pressed a name is what says so.
///
/// `None` is a provider with no door to ask at, and GitHub Copilot is the one: nothing is written for
/// it, so it answers the way it does on a real machine.
///
/// **OpenCode's names carry a qualifier** — its spelling is `provider/model` and the whole of it is
/// what its flag takes — so a road that picked that one names it with the qualifier on.
///
/// `on` is the model the provider says it is standing on, and it reaches the answer only where that
/// provider's own shape has room for it: Cursor writes it into the label of the row, Gemini CLI
/// beside the list, and the other four have nowhere to put it and say nothing.
fn list_door(command: &str, models: &[String], on: Option<&str>) -> Option<(String, String)> {
    if models.is_empty() {
        return None;
    }
    match command {
        // The aliases quoted in the `--model` paragraph of its own help, which is where they are on
        // the real one: this provider has no list command at all. The paragraph ends where the next
        // flag begins, and what follows the words that introduce a full name is not an alias.
        "claude" => Some((
            "--help".to_string(),
            format!(
                "Usage: claude [options] [prompt]\n\nOptions:\n  \
                 --model <model>  Model for the current session. Provide an alias for the latest \
                 model ({aliases}) or a full name, spelled the way the provider writes it.\n  \
                 --version        Output the version number\n",
                aliases = models.iter().map(|one| format!("'{one}'")).collect::<Vec<_>>().join(" or "),
            ),
        )),
        // A JSON catalog, one row per model, with the spelling the flag takes on `slug`. A hidden row
        // is written in with them: the reader drops it, and a stand-in that never printed one would
        // leave that side of the reading standing on nothing.
        "codex" => Some((
            "debug models".to_string(),
            serde_json::json!({
                "models": models
                    .iter()
                    .map(|one| serde_json::json!({
                        "slug": one,
                        "display_name": one,
                        "visibility": "show",
                    }))
                    .chain(std::iter::once(serde_json::json!({
                        "slug": format!("{MODEL}kept-back"),
                        "display_name": "kept back",
                        "visibility": "hide",
                    })))
                    .collect::<Vec<_>>(),
            })
            .to_string(),
        )),
        // An ACP session's answer, on one line. It is printed without waiting to be spoken to: the
        // caller writes its hello and its request and then reads until it recognises an answer, so an
        // answer already there is one it recognises on the first line it reads.
        "gemini" => Some(("--acp".to_string(), {
            let mut said = serde_json::Map::new();
            said.insert(
                "availableModels".to_string(),
                models.iter().map(|one| serde_json::json!({ "modelId": one, "name": one })).collect(),
            );
            // Where this provider says which one it is on, it says it beside the list rather than
            // inside a row — and where it does not say, the key is not there at all, which is the
            // shape a provider nobody has told gives.
            if let Some(one) = on {
                said.insert("currentModelId".to_string(), serde_json::json!(one));
            }
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "result": { "sessionId": "scenario-session", "models": said },
            })
            .to_string()
        })),
        // One qualified name per line and nothing else on it.
        "opencode" => Some((
            "models".to_string(),
            models.iter().map(|one| format!("scenario/{one}\n")).collect(),
        )),
        // `id - label` per line, under a heading and above a tip — both of which are printed, because
        // both are what the reader has to get past on the real one. The tip is written as a sentence
        // with the separator inside it, which is the shape that keeps it off the row: a tip whose
        // first half was one word would be read as a model called that.
        "cursor-agent" => Some((
            "--list-models".to_string(),
            format!(
                "Available models:\n{rows}\nTip: name one when you start - or leave it and pick in the tool.\n",
                rows = models
                    .iter()
                    .map(|one| match on == Some(one.as_str()) {
                        // Where this provider says which one it is on, it says it in the label of the
                        // row itself — read off the label and taken as state rather than as name
                        // (`amenbo_core::agent_models`).
                        true => format!("{one} - {one} (current, default)\n"),
                        false => format!("{one} - {one}\n"),
                    })
                    .collect::<String>(),
            ),
        )),
        // GitHub Copilot, which has nowhere to be asked.
        _ => None,
    }
}

/// What a stand-in does once it has printed what it was started with.
///
/// The three are one question and not two, because they are one loop: a stand-in either leaves the
/// pane with nothing running in it, or stays at it — and what it does with a line that arrives is
/// the whole of what parts the two that stay.
#[derive(Clone, Copy, PartialEq, Eq)]
enum After {
    /// Print and go, which is the pane every road before `then` existed read: the output standing on
    /// the screen and nothing running.
    Ends,
    /// Stay, and print back every line it is given. A pane whose model is moved needs one — there is
    /// no control on a frame whose program has gone, and what says the move arrived is the program
    /// reading it.
    Reads,
    /// Stay, print the line back, and then carry it out. It is what lets a road say something *to*
    /// the machine from inside an agent's pane, which is the one pane a plain shell cannot stand in
    /// for: a shell has no way back into it, so a record made at one names no session to return to.
    Runs,
    /// Stay holding a question, show nothing of what is written in, and say only that a line arrived.
    ///
    /// It is the pane a rename is the dangerous thing to carry into: a program drawing a dialogue
    /// swallows what is pasted at it and draws not one character differently, so a newline sent on
    /// the strength of the screen having moved answers the dialogue instead. Two things make that pane
    /// on a machine with no such program installed — the terminal's own echo turned off, so nothing
    /// written in comes back, and a mark printed for each line given, so a line arriving is readable
    /// where its words must not be. The third, asking for a bracketed paste, every shape that stays
    /// says.
    Asks,
}

impl After {
    /// Whether the pane still has something running in it once the printing is done.
    fn stays(self) -> bool {
        self != After::Ends
    }

    /// How the premise says what was stood up, for the sentence it hands back.
    fn said(self) -> &'static str {
        match self {
            After::Ends => "",
            After::Reads => " and staying open to read what is typed at it",
            After::Runs => " and staying open to run what is typed at it",
            After::Asks =>
                " and staying open on a question of their own, showing nothing of what is written into them",
        }
    }
}

/// The program one stand-in is: what it says it is, what it was started with, and — where the road
/// asked for models — the answer its own provider's list door gives.
///
/// The order is the order it prints in, and the list door is first because it ends there: a run that
/// answered the ask has been asked a question, not started as a pane, and saying it is a stand-in
/// afterwards would put a line of English in the middle of a JSON document.
fn program(
    command: &str,
    models: &[String],
    on: Option<&str>,
    exits: i64,
    after: After,
    comes_back: bool,
) -> String {
    let mut body = "#!/bin/sh\n".to_string();
    if let Some((asked, answer)) = list_door(command, models, on) {
        // The answer is written between the marks with its own last newline taken off: the heredoc
        // gives every line one, and an answer that kept its would print a blank line no provider does.
        body.push_str(&format!(
            "if [ \"$*\" = '{asked}' ]; then\ncat <<'SCENARIO_ANSWER'\n{answer}\nSCENARIO_ANSWER\nexit 0\nfi\n",
            answer = answer.trim_end(),
        ));
    }
    if !comes_back {
        // Every spelling the catalog comes back by, in one reading: a flag with a handle behind it on
        // four of the rows, and a word at the head of the line on the other two
        // (`amenbo_core::harness::Back`). Which of them this command uses is the catalog's answer and
        // not this script's, and a stand-in that answered for one of them would be a stand-in a road
        // could only stand up under one name.
        //
        // Before the printing rather than after it, and well inside the window a handle is believed in
        // (`app/src-tauri/src/pty.rs`, `BELIEVED_AFTER`): what is being stood up is a provider saying
        // at once that it has no such session.
        body.push_str(&format!(
            "for arg in \"$@\"; do\n  case \"$arg\" in\n    --resume|-s|resume) printf '{NO_SESSION}\\n'; exit 1 ;;\n  esac\ndone\n"
        ));
    }
    body.push_str(&format!(
        "echo 'this is the verification harness standing in for {command}'\n"
    ));
    body.push_str(&format!("for arg in \"$@\"; do printf '{ARG} %s\\n' \"$arg\"; done\n"));
    if after == After::Asks {
        // The echo off, so that nothing written in from here on comes back by the terminal's own hand
        // — which is what a program drawing its own interface leaves the pane doing.
        body.push_str("stty -echo 2>/dev/null\n");
    }
    if after.stays() {
        // **Said by every shape that stays.** A pane that has not asked for a bracketed paste is one
        // Amenbo writes nothing into — the markers would arrive as keys and the escape that opens
        // them as cancel — so a road reading what the app pasted into one of these would be reading a
        // paste that never happened. Every provider measured asks within a second of being able to
        // take one; a shell script reading lines asks for nothing, so the stand-in says it itself.
        body.push_str("printf '\\033[?2004h'\n");
    }
    if after == After::Asks {
        // Last of what is printed, so the question is what the pane is left holding.
        body.push_str(&format!("echo '{ASKING}'\n"));
        body.push_str(&format!("echo '{CHOICES}'\n"));
    }
    if after.stays() {
        // And what it says on the way out, where a road stops it (`STOPPED`). The trap is set before
        // the loop that reads: an interrupt arriving while a line is being waited for ends the wait,
        // and the trap is what runs next.
        body.push_str(&format!("trap \"printf '{STOPPED}\\n'; exit 130\" INT\n"));
        match after {
            // A question is answered by a line arriving, and by nothing the line says — so the mark
            // goes out on its own. What it buys is the screen staying clear of the words that were
            // written in, which is the state the hand-over loop is being read in.
            After::Asks => body.push_str(&format!(
                "while IFS= read -r line; do printf '{ARRIVED}\\n'; done\n"
            )),
            // The envelope comes off with `tr` and `sed` rather than a shell replacement, because
            // what is being cut is an escape byte: `tr -d` takes the escape itself and the two `sed`
            // clauses take what is left of the pair of markers.
            _ => body.push_str(&format!(
                "while IFS= read -r line; do\n               said=$(printf '%s' \"$line\" | tr -d '\\033' | sed -e 's/\\[200~//g' -e 's/\\[201~//g')\n               printf '{SAID} [%s]\\n' \"$said\"\n{carried}done\n",
                // And carried out, where the road asked for the shape that runs what it is given. It
                // is after the printing, so the line is on the screen whether or not the machine has
                // anything to run it with — the reading that says it arrived is the same one either
                // way.
                carried = match after {
                    After::Runs => "               eval \"$said\"\n",
                    _ => "",
                },
            )),
        }
    }
    // Last, and written even for the plain ending: a script that fell off its end would leave
    // whatever the line before it did, which is the shell's answer rather than the road's.
    body.push_str(&format!("exit {exits}\n"));
    body
}

/// The front of the catalog `count` of them is taken off, as far as the catalog goes.
///
/// One reading for both halves of the premise: what [`stand_up`] writes and what
/// [`nothing_else_answers`] asks the machine about have to be the same names, or the guard would be
/// watching a row nobody stood up.
fn named(count: i64) -> &'static [&'static str] {
    let want = usize::try_from(count).unwrap_or(0).min(COMMANDS.len());
    &COMMANDS[..want]
}

/// The shell a pane is opened as, read the way the app reads it.
///
/// The account database rather than `SHELL`, because that is what the app asks
/// (`app/src-tauri/src/launch.rs`): `SHELL` describes the shell of whatever session set it, and a
/// harness reached over ssh is a session that may have set none. Asking the wrong shell would be
/// asking about a different profile than the pane reads, and a guard that reads a different machine
/// than the road walks is worse than no guard. `SHELL` and `/bin/sh` are what is left where the
/// database will not answer.
fn pane_shell() -> OsString {
    let asked = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(
            r#"u=$(id -un)
s=$(getent passwd "$u" 2>/dev/null | cut -d: -f7)
[ -n "$s" ] || s=$(dscl . -read /Users/"$u" UserShell 2>/dev/null | sed 's/^UserShell: //')
printf '%s' "$s""#,
        )
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|said| !said.is_empty());
    match asked {
        Some(shell) => OsString::from(shell),
        None => std::env::var_os("SHELL").unwrap_or_else(|| OsString::from("/bin/sh")),
    }
}

/// Where this machine answers each of `named` from, asked the way a pane asks.
///
/// The `PATH` handed over is not the one a program in a pane runs under: a pane is a login *and*
/// interactive shell (`app/src-tauri/src/launch.rs`), and the profile that shell reads runs after
/// the handover. So the same shell is started here with the run's own directory in front, and what
/// it answers is what a pane would find. Each name comes back with what `command -v` said, which is
/// empty where nothing answered at all.
fn answered_from(tools: &Path, named: &[&str]) -> Result<Vec<(String, String)>, String> {
    let mut path = OsString::from(tools);
    if let Some(inherited) = std::env::var_os("PATH") {
        path.push(":");
        path.push(inherited);
    }
    let asks: Vec<String> = named
        .iter()
        .map(|name| format!("printf '%s\\t%s\\n' '{name}' \"$(command -v '{name}' 2>/dev/null)\""))
        .collect();
    let shell = pane_shell();
    let out = std::process::Command::new(&shell)
        .args(["-l", "-i", "-c"])
        .arg(asks.join("\n"))
        .env("PATH", &path)
        .output()
        .map_err(|e| {
            format!("could not ask {} what this machine answers for: {e}", shell.to_string_lossy())
        })?;
    let said = String::from_utf8_lossy(&out.stdout);
    Ok(said
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(name, at)| (name.to_string(), at.to_string()))
        .collect())
}

/// The names among `answers` the run did not get — what answered was written somewhere other than
/// `tools`, or nothing answered and the run's own directory is not on the `PATH` at all.
fn taken_from(tools: &Path, answers: &[(String, String)]) -> Vec<String> {
    answers
        .iter()
        .filter(|(_, at)| !Path::new(at).starts_with(tools))
        .map(|(name, at)| match at.is_empty() {
            true => format!("{name}: nothing answered"),
            false => format!("{name}: {at}"),
        })
        .collect()
}

/// Whether the programs just written under `tools` are the ones a pane would reach by those names.
///
/// This is the premise's reach, and it is a question about the machine rather than about what was
/// written: a road reading a stand-in's own behaviour reads the operator's install instead where one
/// stands under the same name, and reads it without anything going wrong on the way. So the failure
/// is put here, where it names the program that won.
fn nothing_else_answers(tools: &Path, named: &[&str]) -> Result<(), String> {
    let taken = taken_from(tools, &answered_from(tools, named)?);
    if taken.is_empty() {
        return Ok(());
    }
    Err(format!(
        "`can-start` wrote its stand-ins in {}, and a pane's own shell answers for {} of those \
         names from somewhere else — {}. The profile that shell reads runs after the directory is \
         handed over, so a road opening one of these would open what the operator installed: the \
         row reads right and the program behind it is a real agent. Walk these roads on a machine \
         that carries its own agents behind the `PATH` it hands a run, never in front of it.",
        tools.display(),
        taken.len(),
        taken.join("; ")
    ))
}

/// Put `count` of them in `tools`, each answering with `models` of them, and say what was stood up.
///
/// The first of the catalog rather than a road's pick: which agents are on the row is nothing this
/// premise is about — what it is about is how many — and a road naming one would be a road about a
/// tool.
fn stand_up(
    tools: &Path,
    count: i64,
    models: i64,
    on: i64,
    exits: i64,
    after: After,
    comes_back: bool,
) -> Result<String, String> {
    if count < 2 {
        return Err(format!(
            "`can-start` takes a count of 2 or more, not {count} — it puts programs in front of the \
             machine's own `PATH` and can only add to what the operator has installed, so a row with \
             fewer things on it is not a machine this can stand up"
        ));
    }
    let want = usize::try_from(count).unwrap_or(usize::MAX);
    if want > COMMANDS.len() {
        return Err(format!(
            "`can-start` was asked for {want} agents and Amenbo knows how to start {} — a machine \
             with more of them on it than there are is not one a reader could ever be at",
            COMMANDS.len()
        ));
    }
    if !(0..=MOST_MODELS).contains(&models) {
        return Err(format!(
            "`can-start` was asked for {models} models an agent can be started on, and it answers \
             with 0 to {MOST_MODELS} — none of them is the row a provider that cannot be asked \
             draws, and a row longer than this is one whose names would stop being one word each"
        ));
    }
    if !(0..=models).contains(&on) {
        return Err(format!(
            "`can-start` was asked to have the stand-ins say they are on model {on} of the {models} \
             they answer with — the position is one of those, and 0 (the default) is nobody saying \
             which they are on"
        ));
    }
    if !(0..=255).contains(&exits) {
        return Err(format!(
            "`can-start` was asked to have the stand-ins leave with {exits} — a status is 0 to 255, \
             and 0 (the default) is a program that ended having done what it was started for"
        ));
    }
    let named = named(count);
    let names = model_names(models);
    let standing = usize::try_from(on).ok().filter(|at| *at > 0).and_then(|at| names.get(at - 1));
    for command in named {
        let body = program(command, &names, standing.map(String::as_str), exits, after, comes_back);
        write_program(&tools.join(command), &body)?;
    }
    Ok(format!(
        "this machine can start {want} of the agents Amenbo knows ({}), each answering with {models} \
         models it can be started on{}{}{}{}",
        named.join(", "),
        match standing {
            Some(one) => format!(" and the two that can say so saying they are on {one}"),
            None => String::new(),
        },
        after.said(),
        match comes_back {
            true => "",
            false => " and refusing any way back it is handed",
        },
        match exits {
            0 => String::new(),
            n => format!(" and leaving with {n}"),
        }
    ))
}

/// Write `body` at `path` as a runnable program, leaving no descriptor on it in this process.
///
/// The writing is handed to a child for the reason the GUI harness's own stand-ins are: a file
/// written here and exec'd a moment later is the ETXTBSY race, and a process forked while this
/// one's descriptor was still open carries it until it execs. A premise runs beside whatever else
/// the harness is doing, so the descriptor is put somewhere nothing here can fork from.
fn write_program(path: &Path, body: &str) -> Result<(), String> {
    use std::io::Write as _;

    let mut writer = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(r#"cat > "$0" && chmod 755 "$0""#)
        .arg(path)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    let mut input = writer.stdin.take().ok_or("the writer took no input")?;
    input.write_all(body.as_bytes()).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    drop(input);
    let done = writer.wait().map_err(|e| format!("could not write {}: {e}", path.display()))?;
    match done.success() {
        true => Ok(()),
        false => Err(format!("{} was not left runnable ({done})", path.display())),
    }
}

/// Take `command` back off the machine `can-start` stood up, part way along a road, and say what was
/// done.
///
/// **Only a stand-in this run wrote can be taken**, for the reason the premise can only add: what the
/// operator installed is theirs, and nothing handed to a process can take it away. So the name has to
/// be one of the catalog's and its program has to be lying in the run's own directory.
///
/// **And gone is asked, not assumed.** Deleting the stand-in uncovers whatever the machine answers
/// for that name behind it — on a machine carrying its own agents behind the `PATH`, as the ones these
/// roads are walked on do, that is a real install. A road reading an agent gone would then open the
/// operator's program instead, and read as the build having forgotten to look. So what a pane's own
/// shell answers for the name is asked afterwards, and anything at all is refused by name.
pub(crate) fn take_away(tools: &Path, command: &str) -> Result<String, String> {
    if !COMMANDS.contains(&command) {
        return Err(format!(
            "`take-away` names `{command}`, which is not a command Amenbo starts an agent as — it is \
             one of {}",
            COMMANDS.join(", ")
        ));
    }
    let program = tools.join(command);
    if !program.is_file() {
        return Err(format!(
            "`take-away` was asked to take `{command}` off this machine, and this run never stood it \
             up — only what `can-start` wrote can be taken, since what the operator installed is \
             theirs"
        ));
    }
    std::fs::remove_file(&program)
        .map_err(|e| format!("could not take {} away: {e}", program.display()))?;
    let answered = answered_from(tools, &[command])?;
    if let Some((_, at)) = answered.iter().find(|(_, at)| !at.is_empty()) {
        return Err(format!(
            "`take-away` took the stand-in for `{command}` away, and a pane's own shell still answers \
             for that name from {at} — the operator's install, now in front. A road reading the agent \
             gone would open that program instead. Walk this road on a machine that has no `{command}` \
             of its own."
        ));
    }
    Ok(format!("this machine can no longer start `{command}`"))
}

impl Driver<'_> {
    /// The workspace's one action here, and it is a premise's: everything else in this domain is
    /// a move on a screen this driver has not got.
    pub(crate) fn workspace_action(&self, op: &str, with: &Args) -> Result<Outcome, String> {
        match op {
            // `models` is how many names each of them answers with when the frame asks what it can
            // be started on. Left off it is none, which is the machine every road before this one
            // stood up: a stand-in that says what it is and stops answers no list, and the row draws
            // the shape a provider with no list door gives.
            "can-start" => {
                let models = match with.get("models") {
                    None => 0,
                    Some(_) => req_i64(with, "models")?,
                };
                // Which of those names the stand-ins say they are on, by its position in the row.
                // Only two of the six have anywhere in their answer to say it, so this is also how a
                // road reads the other four saying nothing — one premise, both readings.
                let on = match with.get("on") {
                    None => 0,
                    Some(_) => req_i64(with, "on")?,
                };
                // What one leaves with. A road says it where the ending itself is what is being read:
                // the pane says a program ended whatever the status, and a few statuses are a
                // provider's whole answer to why.
                let exits = match with.get("exits") {
                    None => 0,
                    Some(_) => req_i64(with, "exits")?,
                };
                // What a stand-in does once it has printed. `ends` is what every road before this one
                // stood up — the output on the screen and nothing running — and `reads` is the pane a
                // road moves to another model, which needs a program still there to move.
                let after = match with.get("then").and_then(serde_yaml::Value::as_str) {
                    None | Some("ends") => After::Ends,
                    Some("reads") => After::Reads,
                    Some("runs") => After::Runs,
                    Some("asks") => After::Asks,
                    Some(other) => {
                        return Err(format!(
                            "`can-start` does not know what `{other}` means for what a stand-in does \
                             once it has printed — it `ends` (the default), `reads` what is typed at \
                             it, `runs` it, or `asks` a question and shows nothing of what is written \
                             in answer to it"
                        ))
                    }
                };
                // Whether one honours a way back it is handed. A stand-in issues no sessions, so
                // every handle it ever sees is one it did not make: `false` is the stand-in that says
                // so and goes, which is how a road reaches what a pane says when the conversation
                // behind a way back is not there any more.
                let comes_back = match with.get("comes-back") {
                    None => true,
                    Some(said) => said.as_bool().ok_or(
                        "`can-start` reads `comes-back` as true or false — whether a stand-in \
                         honours a way back it is handed",
                    )?,
                };
                let count = req_i64(with, "count")?;
                let said =
                    stand_up(&self.session.tools, count, models, on, exits, after, comes_back)?;
                // Written is not reached. What a road opens under one of these names is whatever the
                // pane's own shell answers for it, and an install of the same name answers first.
                nothing_else_answers(&self.session.tools, named(count))?;
                Ok(Outcome::action(said))
            }
            _ => Err(unmapped(Domain::Workspace, op)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What one of them printed when it was started with nothing and given `input` to read.
    fn said_to(at: &Path, input: &str) -> String {
        use std::io::Write as _;

        let mut child = std::process::Command::new(at)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("the stand-in runs");
        // A stand-in that ends is allowed to be gone before the line reaches it, and then the write
        // is a broken pipe rather than a fault: what is being read here is what it printed, and one
        // that ended without reading is the answer, not a failure to deliver.
        match child.stdin.take().expect("it takes input").write_all(input.as_bytes()) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {}
            Err(e) => panic!("written: {e:?}"),
        }
        let out = child.wait_with_output().expect("it ends when its input does");
        String::from_utf8(out.stdout).expect("what it printed is text")
    }

    /// What one of them printed when it was run with these arguments.
    fn ran(at: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new(at).args(args).output().expect("the stand-in runs");
        assert!(out.status.success(), "{} answered", at.display());
        String::from_utf8(out.stdout).expect("what it printed is text")
    }

    /// What the premise claims: a program per agent asked for, taken off the front of the catalog,
    /// and each one runnable — an unrunnable file is not something `command -v` would answer for.
    #[test]
    fn the_asked_for_agents_are_standing_and_runnable() {
        let session = crate::scratch::session("can-start-test", false).expect("a throwaway session");
        let said = stand_up(&session.tools, 2, 0, 0, 0, After::Ends, true).expect("two is a machine it can stand up");

        for command in &COMMANDS[..2] {
            let at = session.tools.join(command);
            assert!(at.is_file(), "{command} is standing: {said}");
            let out = std::process::Command::new(&at).output().expect("the stand-in runs");
            assert!(out.status.success(), "{command} is runnable");
        }
        assert!(!session.tools.join(COMMANDS[2]).exists(), "and nothing beyond what was asked for");
    }

    /// A count this cannot honestly deliver is refused rather than half-done. The reach is additive,
    /// so "one" and "none" are machines the operator's own installs decide, and a premise that
    /// shrugged would leave a road reading a row it never stood up.
    #[test]
    fn a_count_below_two_is_refused_because_nothing_here_can_take_an_install_away() {
        let session = crate::scratch::session("can-start-floor-test", false).expect("a session");
        for count in [-1, 0, 1] {
            let err = stand_up(&session.tools, count, 0, 0, 0, After::Ends, true).expect_err("a floor, never a ceiling");
            assert!(err.contains("2 or more"), "{err}");
        }
    }

    /// And more agents than Amenbo knows how to start, which is a machine nobody could be at.
    #[test]
    fn more_agents_than_there_are_is_refused() {
        let session = crate::scratch::session("can-start-over-test", false).expect("a session");
        let asked = COMMANDS.len() as i64 + 1;
        let err = stand_up(&session.tools, asked, 0, 0, 0, After::Ends, true).expect_err("there are only so many");
        assert!(err.contains(&COMMANDS.len().to_string()), "{err}");
    }

    /// A row of models longer than the names can be told apart in, refused for the same reason.
    #[test]
    fn more_models_than_the_names_can_carry_is_refused() {
        let session = crate::scratch::session("can-start-models-over-test", false).expect("a session");
        let err = stand_up(&session.tools, 2, MOST_MODELS + 1, 0, 0, After::Ends, true).expect_err("two digits is the width");
        assert!(err.contains(&MOST_MODELS.to_string()), "{err}");
    }

    /// No models asked for is the machine every road before this one stood up: the ask is answered
    /// with the same sentence as anything else, which is a provider with nothing to list.
    #[test]
    fn an_agent_asked_for_no_models_answers_its_list_door_with_nothing_of_the_kind() {
        let session = crate::scratch::session("can-start-no-models-test", false).expect("a session");
        stand_up(&session.tools, 2, 0, 0, 0, After::Ends, true).expect("a machine with two agents on it");

        let said = ran(&session.tools.join("claude"), &["--help"]);
        assert!(said.contains("standing in for claude"), "{said}");
        assert!(!said.contains(MODEL), "and no model name anywhere in it: {said}");
    }

    /// Each provider answers its own question in its own shape, and every name the road asked for is
    /// in the answer. The shapes are read from the outside here — what turns them into a row is the
    /// build's own reader, which is the thing under test and not something to be borrowed.
    #[test]
    fn each_agent_answers_its_own_list_door_in_its_own_shape() {
        let session = crate::scratch::session("can-start-models-test", false).expect("a session");
        stand_up(&session.tools, COMMANDS.len() as i64, 2, 0, 0, After::Ends, true).expect("the whole catalog");
        let first = format!("{MODEL}01");
        let second = format!("{MODEL}02");

        // Claude Code: the aliases quoted in the `--model` paragraph of its own help.
        let help = ran(&session.tools.join("claude"), &["--help"]);
        assert!(help.contains(&format!("'{first}' or '{second}'")), "{help}");

        // Codex CLI: a JSON catalog, the spelling on `slug`, and a hidden row printed with them.
        let catalog = ran(&session.tools.join("codex"), &["debug", "models"]);
        let read: serde_json::Value = serde_json::from_str(&catalog).expect("a JSON catalog");
        let rows = read["models"].as_array().expect("rows");
        assert_eq!(rows.len(), 3, "two shown and one hidden: {catalog}");
        assert_eq!(rows[0]["slug"], serde_json::json!(first));
        assert_eq!(rows[2]["visibility"], serde_json::json!("hide"));

        // Gemini CLI: an ACP session's answer, printed without waiting to be spoken to.
        let session_answer = ran(&session.tools.join("gemini"), &["--acp"]);
        let read: serde_json::Value = serde_json::from_str(session_answer.trim()).expect("one line of JSON");
        assert_eq!(read["result"]["models"]["availableModels"][1]["modelId"], serde_json::json!(second));

        // OpenCode: one qualified name per line, the qualifier being part of the spelling.
        let lines = ran(&session.tools.join("opencode"), &["models"]);
        assert_eq!(lines.lines().collect::<Vec<_>>(), vec![format!("scenario/{first}"), format!("scenario/{second}")]);

        // Cursor: `id - label` rows, with the heading and the tip they stand between.
        let listed = ran(&session.tools.join("cursor-agent"), &["--list-models"]);
        assert!(listed.contains(&format!("\n{first} - {first}\n")), "{listed}");

        // GitHub Copilot has no door to ask at, so being asked is being started.
        let asked = ran(&session.tools.join("copilot"), &["--list-models"]);
        assert!(asked.contains("standing in for copilot"), "{asked}");
        assert!(asked.contains(&format!("{ARG} --list-models")), "{asked}");
    }

    /// Asked to stay, a stand-in prints back every line it is given, and prints it whole.
    ///
    /// **The brackets are the point.** What a road watching a running pane reads back is the line
    /// Amenbo typed into it, and the fault it is watching for is a model name on a line that must not
    /// carry one — two of the six read `/model <name>` as a prompt and bill for the answer.
    /// Without the closing bracket the reading for the bare line is a piece of the
    /// reading for the named one, and the road would pass on exactly the build that costs money.
    ///
    /// The bracketed-paste envelope comes off, because it is the terminal's and not what anybody
    /// typed.
    #[test]
    fn a_stand_in_asked_to_stay_prints_back_every_line_it_is_given_whole() {
        let session = crate::scratch::session("can-start-reads-test", false).expect("a session");
        stand_up(&session.tools, 2, 3, 0, 0, After::Reads, true).expect("a machine whose stand-ins stay and read");

        let name = format!("{MODEL}02");
        let typed = format!("\u{1b}[200~/model {name}\u{1b}[201~\n\u{1b}[200~/model\u{1b}[201~\n");
        let printed = said_to(&session.tools.join("claude"), &typed);
        assert!(printed.contains(&format!("{SAID} [/model {name}]")), "{printed}");
        assert!(printed.contains(&format!("{SAID} [/model]")), "{printed}");
        // And the bare line is not a piece of the named one, which is the whole of why the brackets
        // are there.
        assert!(!format!("{SAID} [/model {name}]").contains(&format!("{SAID} [/model]")));
    }

    /// And stopped where it stands, it says so before it goes.
    ///
    /// **A program that died quietly would leave the press unreadable.** What a stop does is end the
    /// thing running in the pane, and what a shell prompt draws for one is a mark and a fresh prompt
    /// — the two it was already drawing. So the word is the stand-in's own, printed from a trap, and
    /// a road that pressed the key reads it off the pane.
    ///
    /// The signal is sent after a line has been read back, which is the moment the trap is known to
    /// be standing: a signal that arrived while the program was still printing its hello would meet
    /// the shell's own default and end it without a word.
    #[test]
    fn a_stand_in_that_reads_says_it_was_stopped_before_it_goes() {
        use std::io::{BufRead as _, BufReader, Read as _, Write as _};

        let session = crate::scratch::session("can-start-stopped-test", false).expect("a session");
        stand_up(&session.tools, 2, 0, 0, 0, After::Reads, true).expect("a machine whose stand-ins stay and read");

        let mut child = std::process::Command::new(session.tools.join("claude"))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("the stand-in runs");
        let mut writing = child.stdin.take().expect("it takes input");
        writing.write_all(b"a line to be read back\n").expect("written");
        let mut reading = BufReader::new(child.stdout.take().expect("it prints"));
        let mut said = String::new();
        while !said.contains(SAID) {
            let mut line = String::new();
            let read = reading.read_line(&mut line).expect("it goes on printing");
            assert!(read > 0, "the stand-in ended before it read a line: {said}");
            said.push_str(&line);
        }

        // The interrupt a person's key makes, sent the way anything outside a terminal sends one.
        let sent = std::process::Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .expect("the signal goes");
        assert!(sent.success(), "the stand-in was signalled");

        let mut last = String::new();
        reading.read_to_string(&mut last).expect("what it said on the way out");
        child.wait().expect("it goes");
        assert!(last.contains(STOPPED), "{said}{last}");
    }

    /// A stand-in asked to run what it is given carries the line out, and not only back.
    ///
    /// **It is what lets a road say something to the machine from inside an agent's pane.** A plain
    /// shell is the only other pane a road can type into, and a shell is the one pane with no way
    /// back into it — so a record made at one names no session anybody could return to.
    ///
    /// The line is printed before it is run, so the reading that says it arrived is the same one the
    /// reading half gives — what is added is the command's own output under it.
    #[test]
    fn a_stand_in_that_runs_carries_the_line_out() {
        let session = crate::scratch::session("can-start-runs-test", false).expect("a session");
        stand_up(&session.tools, 2, 0, 0, 0, After::Runs, true)
            .expect("a machine whose stand-ins stay and run");

        let printed = said_to(&session.tools.join("claude"), "echo SCENARIO carried out\n");

        assert!(printed.contains(&format!("{SAID} [echo SCENARIO carried out]")), "{printed}");
        assert!(printed.contains("SCENARIO carried out"), "and it was run: {printed}");
    }

    /// Every stand-in that stays asks for a bracketed paste.
    ///
    /// **It is what makes one of these a pane Amenbo writes into at all.** Nothing is pasted into a
    /// pane that has not asked — the markers would arrive as keys and the escape that opens them as
    /// cancel — so a road reading what the app put into one of these would be reading a paste that
    /// never happened, and reading it as the app having withheld something. Every provider measured
    /// asks within a second of being able to take one; a shell script reading lines asks for nothing,
    /// which is the gap this closes.
    ///
    /// The one that ends is not asked for it: nothing is running in that pane to be written to.
    #[test]
    fn every_stand_in_that_stays_asks_for_a_bracketed_paste() {
        for (tag, after, stays) in [
            ("ends", After::Ends, false),
            ("reads", After::Reads, true),
            ("runs", After::Runs, true),
            ("asks", After::Asks, true),
        ] {
            let body = program("claude", &[], None, 0, after, true);
            assert_eq!(
                body.contains("[?2004h"),
                stays,
                "the {tag} shape and the paste it does or does not ask for",
            );
        }
    }

    /// A stand-in holding a question asks it, asks for a paste, and says only that a line arrived.
    ///
    /// **The words of the line are what must not come back.** The name a rename could not submit is
    /// sitting in the pane's input box and goes in on the next press a person makes; a stand-in that
    /// printed it would put the hand-over loop's own text on the screen, where the loop reads it back
    /// and takes it for the paste having landed (`app/src-tauri/src/handover.rs`, `echoed`).
    ///
    /// The declaration is read for as well, because it is the one of the shape's three parts that a
    /// road cannot see the absence of: a pane that has not asked for a bracketed paste is one nothing
    /// is pasted into at all, and the road would read a paste that never happened as a newline that
    /// was withheld.
    #[test]
    fn a_stand_in_holding_a_question_says_a_line_arrived_and_never_what_it_said() {
        let session = crate::scratch::session("can-start-asks-test", false).expect("a session");
        stand_up(&session.tools, 2, 0, 0, 0, After::Asks, true)
            .expect("a machine whose stand-ins hold a question");

        let typed = "\u{1b}[200~/rename SCENARIO the north channel\u{1b}[201~\n";
        let printed = said_to(&session.tools.join("claude"), typed);

        assert!(printed.contains(ASKING), "the question is up: {printed}");
        assert!(printed.contains(CHOICES), "with the choices under it: {printed}");
        assert!(printed.contains("\u{1b}[?2004h"), "and a paste asked for: {printed}");
        assert!(printed.contains(ARRIVED), "a line arrived: {printed}");
        assert!(!printed.contains("/rename"), "and none of what it said: {printed}");
    }

    /// A stand-in that reads prints the line and stops there. The two shapes part on this and on
    /// nothing else.
    #[test]
    fn a_stand_in_that_only_reads_does_not_carry_the_line_out() {
        let session = crate::scratch::session("can-start-reads-only-test", false).expect("a session");
        stand_up(&session.tools, 2, 0, 0, 0, After::Reads, true)
            .expect("a machine whose stand-ins stay and read");

        let printed = said_to(&session.tools.join("claude"), "echo SCENARIO carried out\n");

        assert!(printed.contains(&format!("{SAID} [echo SCENARIO carried out]")), "{printed}");
        assert!(!printed.contains("\nSCENARIO carried out"), "{printed}");
    }

    /// Asked to refuse a way back, it says so and goes — whichever spelling the handle came in under.
    ///
    /// **All the spellings, because which one a command comes back by is the catalog's answer**
    /// (`amenbo_core::harness::Back`): a flag with a handle behind it on four of the rows, and a word
    /// at the head of the line on the other two. A stand-in that answered for one of them would be
    /// one a road could only stand up under a single name.
    #[test]
    fn a_stand_in_that_refuses_a_way_back_says_so_and_goes() {
        let session = crate::scratch::session("can-start-no-way-back-test", false).expect("a session");
        stand_up(&session.tools, 2, 0, 0, 0, After::Runs, false)
            .expect("a machine whose stand-ins keep no sessions");

        for handed in [
            vec!["--resume", "a-handle-it-never-issued"],
            vec!["-s", "a-handle-it-never-issued"],
            vec!["resume", "--last"],
        ] {
            // Run rather than `ran`, which asks for a status of 0: the whole of what this stand-in
            // does is leave with one a provider refusing a session id leaves with.
            let out = std::process::Command::new(session.tools.join("claude"))
                .args(&handed)
                .output()
                .expect("the stand-in runs");
            let printed = String::from_utf8(out.stdout).expect("what it printed is text");
            assert!(printed.contains(NO_SESSION), "{handed:?}: {printed}");
            assert!(!out.status.success(), "{handed:?}: it left as though nothing was wrong");
        }
    }

    /// And left alone it honours one, which is what every road that opens the app again and finds its
    /// panes resumed reads: the pane comes back running, on the line it was given.
    #[test]
    fn a_stand_in_handed_a_way_back_carries_on_unless_a_road_says_otherwise() {
        let session = crate::scratch::session("can-start-comes-back-test", false).expect("a session");
        stand_up(&session.tools, 2, 0, 0, 0, After::Ends, true)
            .expect("a machine with two agents on it");

        let printed = ran(&session.tools.join("claude"), &["--resume", "a-handle-from-the-run-before"]);

        assert!(!printed.contains(NO_SESSION), "{printed}");
        assert!(printed.contains(&format!("{ARG} --resume")), "{printed}");
    }

    /// Left alone it ends, which is the pane every road before this one read.
    #[test]
    fn a_stand_in_left_alone_ends_rather_than_waiting_to_be_typed_at() {
        let session = crate::scratch::session("can-start-ends-test", false).expect("a session");
        stand_up(&session.tools, 2, 0, 0, 0, After::Ends, true).expect("a machine with two agents on it");
        let printed = said_to(&session.tools.join("claude"), "a line nobody reads\n");
        assert!(!printed.contains(SAID), "{printed}");
    }

    /// Which model a stand-in says it is on, where its own provider's answer has room to say it —
    /// and nothing for the four whose answer has none, which is the same premise read from the other
    /// side.
    #[test]
    fn the_two_that_can_say_which_model_they_are_on_say_it_and_the_rest_do_not() {
        let session = crate::scratch::session("can-start-on-test", false).expect("a session");
        stand_up(&session.tools, COMMANDS.len() as i64, 3, 2, 0, After::Ends, true).expect("the whole catalog");
        let second = format!("{MODEL}02");

        // Cursor says it in the label of the row itself, beside the name.
        let rows = ran(&session.tools.join("cursor-agent"), &["--list-models"]);
        assert!(rows.contains(&format!("{second} - {second} (current, default)")), "{rows}");
        assert_eq!(rows.matches("(current, default)").count(), 1, "one row and no other: {rows}");

        // Gemini CLI says it beside the list rather than inside a row.
        let answer = ran(&session.tools.join("gemini"), &["--acp"]);
        let read: serde_json::Value = serde_json::from_str(answer.trim()).expect("one line of JSON");
        assert_eq!(read["result"]["models"]["currentModelId"], serde_json::json!(second));

        // And the shapes with nowhere to say it say nothing. Read as the build reads them — the
        // state Cursor writes into a label, and the key Gemini CLI writes beside a list — because
        // the word itself turns up in prose neither of them is: this provider's own help text calls
        // the session it is started for the current one.
        let help = ran(&session.tools.join("claude"), &["--help"]);
        assert!(!help.contains("(current, default)"), "{help}");
        let catalog = ran(&session.tools.join("codex"), &["debug", "models"]);
        assert!(!catalog.contains("currentModelId"), "{catalog}");
    }

    /// Nobody saying which is the default, and it is what every road before this one stood up: the
    /// key is not in the answer at all rather than in it holding nothing.
    #[test]
    fn a_machine_nobody_was_told_about_says_which_model_nowhere() {
        let session = crate::scratch::session("can-start-not-on-test", false).expect("a session");
        stand_up(&session.tools, COMMANDS.len() as i64, 3, 0, 0, After::Ends, true).expect("the whole catalog");

        let rows = ran(&session.tools.join("cursor-agent"), &["--list-models"]);
        assert!(!rows.contains("(current, default)"), "{rows}");
        let answer = ran(&session.tools.join("gemini"), &["--acp"]);
        assert!(!answer.contains("currentModelId"), "{answer}");
    }

    /// A position outside the row is a road naming a model nobody answered with, and it is refused
    /// where it is written rather than read back as a name that never appears.
    #[test]
    fn being_on_a_model_that_is_not_in_the_row_is_refused() {
        let session = crate::scratch::session("can-start-on-over-test", false).expect("a session");
        let err = stand_up(&session.tools, 2, 3, 4, 0, After::Ends, true).expect_err("three is the whole row");
        assert!(err.contains("model 4 of the 3"), "{err}");
    }

    /// What a stand-in leaves with, where a road said so — and the list door answering all the same,
    /// because being asked a question is not being started as a pane.
    #[test]
    fn a_stand_in_leaves_with_the_status_the_road_named() {
        let session = crate::scratch::session("can-start-exits-test", false).expect("a session");
        stand_up(&session.tools, 2, 3, 0, 41, After::Ends, true).expect("a machine whose stand-ins die");

        let out = std::process::Command::new(session.tools.join("claude"))
            .output()
            .expect("the stand-in runs");
        assert_eq!(out.status.code(), Some(41));

        // Asked rather than started, it answers and leaves with nothing wrong: a road reads the row
        // of models on a machine whose panes die, which is the shape this premise is for.
        let help = ran(&session.tools.join("claude"), &["--help"]);
        assert!(help.contains(&format!("{MODEL}01")), "{help}");
    }

    /// Left unsaid, a stand-in ends the way every road before this one read it ending.
    #[test]
    fn a_stand_in_nobody_asked_about_leaves_with_nothing_wrong() {
        let session = crate::scratch::session("can-start-exits-none-test", false).expect("a session");
        stand_up(&session.tools, 2, 0, 0, 0, After::Ends, true).expect("a machine with two agents on it");

        let out = std::process::Command::new(session.tools.join("claude"))
            .output()
            .expect("the stand-in runs");
        assert_eq!(out.status.code(), Some(0));
    }

    /// A status no process can leave with is a road writing something it will never read back.
    #[test]
    fn a_status_outside_what_a_process_leaves_with_is_refused() {
        let session = crate::scratch::session("can-start-exits-over-test", false).expect("a session");
        let err = stand_up(&session.tools, 2, 0, 0, 256, After::Ends, true).expect_err("0 to 255");
        assert!(err.contains("0 to 255"), "{err}");
    }

    /// The reach the premise claims: a name written in the run's own directory is the one a pane's
    /// own shell answers for. Asked about a name no machine carries, so what is read is the route
    /// rather than what the operator installed.
    #[test]
    fn a_pane_shell_answers_from_the_run_for_a_name_nothing_installs() {
        let session = crate::scratch::session("can-start-reach-test", false).expect("a session");
        let name = "scenario-no-such-agent";
        write_program(&session.tools.join(name), "#!/bin/sh\nexit 0\n").expect("it is written");

        let answers = answered_from(&session.tools, &[name]).expect("the shell answers");

        assert_eq!(answers.len(), 1, "one name asked, one answered: {answers:?}");
        assert_eq!(answers[0].0, name);
        assert_eq!(taken_from(&session.tools, &answers), Vec::<String>::new(), "{answers:?}");
    }

    /// An install of the same name is what a road would open, so it is what the premise refuses on:
    /// the one written elsewhere and the one nothing answered for are both names the run lost.
    #[test]
    fn a_name_answered_from_outside_the_run_is_a_name_the_run_lost() {
        let tools = Path::new("/tmp/a-run-of-its-own/tools");
        let answers = vec![
            ("claude".to_string(), "/opt/an-install-of-their-own/bin/claude".to_string()),
            ("codex".to_string(), tools.join("codex").display().to_string()),
            ("copilot".to_string(), String::new()),
        ];

        let taken = taken_from(tools, &answers);

        let won = "claude: /opt/an-install-of-their-own/bin/claude";
        assert_eq!(taken, vec![won, "copilot: nothing answered"]);
    }

    /// Taking an agent away takes the program the premise wrote, and nothing it left beside it.
    /// Whether the machine then answers for the name from an install of its own is the machine's, so
    /// both answers are read here: gone, or refused naming what answers instead.
    #[test]
    fn taking_an_agent_away_takes_the_stand_in_and_leaves_its_neighbour() {
        let session = crate::scratch::session("take-away-test", false).expect("a session");
        stand_up(&session.tools, 2, 0, 0, 0, After::Reads, true).expect("a machine with two agents");

        let said = take_away(&session.tools, "codex");

        assert!(!session.tools.join("codex").exists(), "the stand-in is gone: {said:?}");
        assert!(session.tools.join("claude").is_file(), "and the one beside it is still standing");
        match said {
            Ok(said) => assert!(said.contains("codex"), "{said}"),
            Err(err) => assert!(err.contains("the operator's install"), "{err}"),
        }
    }

    /// A name the run never stood up is not one it can take: what answers for it is the operator's.
    #[test]
    fn an_agent_this_run_never_stood_up_cannot_be_taken_away() {
        let session = crate::scratch::session("take-away-never-test", false).expect("a session");
        stand_up(&session.tools, 2, 0, 0, 0, After::Ends, true).expect("a machine with two agents");

        let err = take_away(&session.tools, "copilot").expect_err("never stood up");

        assert!(err.contains("never stood it up"), "{err}");
    }

    /// And a name outside the catalog is not an agent at all.
    #[test]
    fn a_name_that_is_no_agent_cannot_be_taken_away() {
        let session = crate::scratch::session("take-away-unknown-test", false).expect("a session");

        let err = take_away(&session.tools, "vim").expect_err("not an agent");

        assert!(err.contains("not a command Amenbo starts an agent as"), "{err}");
    }

    /// Started rather than asked, a stand-in prints what it was started with — one argument to a
    /// line, so a name a road wrote is read back whole and on its own.
    #[test]
    fn a_stand_in_prints_what_it_was_started_with_one_argument_to_a_line() {
        let session = crate::scratch::session("can-start-args-test", false).expect("a session");
        stand_up(&session.tools, 2, 3, 0, 0, After::Ends, true).expect("a machine with a row of models on it");

        let name = format!("{MODEL}02");
        let printed = ran(&session.tools.join("claude"), &["--model", &name, "an opening sentence"]);
        let lines: Vec<&str> = printed.lines().collect();
        assert_eq!(lines[0], "this is the verification harness standing in for claude");
        assert_eq!(lines[1], format!("{ARG} --model"));
        assert_eq!(lines[2], format!("{ARG} {name}"));
        assert_eq!(lines[3], format!("{ARG} an opening sentence"), "a whole argument, spaces and all");
    }
}


