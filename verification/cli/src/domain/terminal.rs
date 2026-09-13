//! The `terminal` domain's one premise: **the machine a pane would be opened on**.
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
//! **And they print what they were started with.** A model chosen on the frame is a flag on a command
//! line and nothing else, so the only thing that says the choice arrived is the program it arrived at
//! — the line the frame writes out under the row is Amenbo's own account of what a press would do, and
//! a build that drew it right and started the pane wrong would keep that promise falsely. Each
//! argument is printed on a line of its own, marked ([`ARG`]), so a road reads back the name it wrote
//! and never a spelling belonging to one tool.

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

/// The program one stand-in is: what it says it is, what it was started with, and — where the road
/// asked for models — the answer its own provider's list door gives.
///
/// The order is the order it prints in, and the list door is first because it ends there: a run that
/// answered the ask has been asked a question, not started as a pane, and saying it is a stand-in
/// afterwards would put a line of English in the middle of a JSON document.
fn program(command: &str, models: &[String], on: Option<&str>, exits: i64, reads: bool) -> String {
    let mut body = "#!/bin/sh\n".to_string();
    if let Some((asked, answer)) = list_door(command, models, on) {
        // The answer is written between the marks with its own last newline taken off: the heredoc
        // gives every line one, and an answer that kept its would print a blank line no provider does.
        body.push_str(&format!(
            "if [ \"$*\" = '{asked}' ]; then\ncat <<'SCENARIO_ANSWER'\n{answer}\nSCENARIO_ANSWER\nexit 0\nfi\n",
            answer = answer.trim_end(),
        ));
    }
    body.push_str(&format!(
        "echo 'this is the verification harness standing in for {command}'\n"
    ));
    body.push_str(&format!("for arg in \"$@\"; do printf '{ARG} %s\\n' \"$arg\"; done\n"));
    if reads {
        // And what it says on the way out, where a road stops it (`STOPPED`). The trap is set before
        // the loop that reads: an interrupt arriving while a line is being waited for ends the wait,
        // and the trap is what runs next.
        body.push_str(&format!("trap \"printf '{STOPPED}\\n'; exit 130\" INT\n"));
        // The envelope comes off with `tr` and `sed` rather than a shell replacement, because what is
        // being cut is an escape byte: `tr -d` takes the escape itself and the two `sed` clauses take
        // what is left of the pair of markers.
        body.push_str(&format!(
            "while IFS= read -r line; do\n               said=$(printf '%s' \"$line\" | tr -d '\\033' | sed -e 's/\\[200~//g' -e 's/\\[201~//g')\n               printf '{SAID} [%s]\\n' \"$said\"\ndone\n"
        ));
    }
    // Last, and written even for the plain ending: a script that fell off its end would leave
    // whatever the line before it did, which is the shell's answer rather than the road's.
    body.push_str(&format!("exit {exits}\n"));
    body
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
    reads: bool,
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
    let named = &COMMANDS[..want];
    let names = model_names(models);
    let standing = usize::try_from(on).ok().filter(|at| *at > 0).and_then(|at| names.get(at - 1));
    for command in named {
        let body = program(command, &names, standing.map(String::as_str), exits, reads);
        write_program(&tools.join(command), &body)?;
    }
    Ok(format!(
        "this machine can start {want} of the agents Amenbo knows ({}), each answering with {models} \
         models it can be started on{}{}{}",
        named.join(", "),
        match standing {
            Some(one) => format!(" and the two that can say so saying they are on {one}"),
            None => String::new(),
        },
        match reads {
            true => " and staying open to read what is typed at it",
            false => "",
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

impl Driver<'_> {
    /// The terminal face's one action here, and it is a premise's: everything else in this domain is
    /// a move on a screen this driver has not got.
    pub(crate) fn terminal_action(&self, op: &str, with: &Args) -> Result<Outcome, String> {
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
                let reads = match with.get("then").and_then(serde_yaml::Value::as_str) {
                    None | Some("ends") => false,
                    Some("reads") => true,
                    Some(other) => {
                        return Err(format!(
                            "`can-start` does not know what `{other}` means for what a stand-in does \
                             once it has printed — it either `ends` (the default) or `reads` what is \
                             typed at it"
                        ))
                    }
                };
                Ok(Outcome::action(stand_up(
                    &self.session.tools,
                    req_i64(with, "count")?,
                    models,
                    on,
                    exits,
                    reads,
                )?))
            }
            _ => Err(unmapped(Domain::Terminal, op)),
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
        let said = stand_up(&session.tools, 2, 0, 0, 0, false).expect("two is a machine it can stand up");

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
            let err = stand_up(&session.tools, count, 0, 0, 0, false).expect_err("a floor, never a ceiling");
            assert!(err.contains("2 or more"), "{err}");
        }
    }

    /// And more agents than Amenbo knows how to start, which is a machine nobody could be at.
    #[test]
    fn more_agents_than_there_are_is_refused() {
        let session = crate::scratch::session("can-start-over-test", false).expect("a session");
        let asked = COMMANDS.len() as i64 + 1;
        let err = stand_up(&session.tools, asked, 0, 0, 0, false).expect_err("there are only so many");
        assert!(err.contains(&COMMANDS.len().to_string()), "{err}");
    }

    /// A row of models longer than the names can be told apart in, refused for the same reason.
    #[test]
    fn more_models_than_the_names_can_carry_is_refused() {
        let session = crate::scratch::session("can-start-models-over-test", false).expect("a session");
        let err = stand_up(&session.tools, 2, MOST_MODELS + 1, 0, 0, false).expect_err("two digits is the width");
        assert!(err.contains(&MOST_MODELS.to_string()), "{err}");
    }

    /// No models asked for is the machine every road before this one stood up: the ask is answered
    /// with the same sentence as anything else, which is a provider with nothing to list.
    #[test]
    fn an_agent_asked_for_no_models_answers_its_list_door_with_nothing_of_the_kind() {
        let session = crate::scratch::session("can-start-no-models-test", false).expect("a session");
        stand_up(&session.tools, 2, 0, 0, 0, false).expect("a machine with two agents on it");

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
        stand_up(&session.tools, COMMANDS.len() as i64, 2, 0, 0, false).expect("the whole catalog");
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
        stand_up(&session.tools, 2, 3, 0, 0, true).expect("a machine whose stand-ins stay and read");

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
        stand_up(&session.tools, 2, 0, 0, 0, true).expect("a machine whose stand-ins stay and read");

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

    /// Left alone it ends, which is the pane every road before this one read.
    #[test]
    fn a_stand_in_left_alone_ends_rather_than_waiting_to_be_typed_at() {
        let session = crate::scratch::session("can-start-ends-test", false).expect("a session");
        stand_up(&session.tools, 2, 0, 0, 0, false).expect("a machine with two agents on it");
        let printed = said_to(&session.tools.join("claude"), "a line nobody reads\n");
        assert!(!printed.contains(SAID), "{printed}");
    }

    /// Which model a stand-in says it is on, where its own provider's answer has room to say it —
    /// and nothing for the four whose answer has none, which is the same premise read from the other
    /// side.
    #[test]
    fn the_two_that_can_say_which_model_they_are_on_say_it_and_the_rest_do_not() {
        let session = crate::scratch::session("can-start-on-test", false).expect("a session");
        stand_up(&session.tools, COMMANDS.len() as i64, 3, 2, 0, false).expect("the whole catalog");
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
        stand_up(&session.tools, COMMANDS.len() as i64, 3, 0, 0, false).expect("the whole catalog");

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
        let err = stand_up(&session.tools, 2, 3, 4, 0, false).expect_err("three is the whole row");
        assert!(err.contains("model 4 of the 3"), "{err}");
    }

    /// What a stand-in leaves with, where a road said so — and the list door answering all the same,
    /// because being asked a question is not being started as a pane.
    #[test]
    fn a_stand_in_leaves_with_the_status_the_road_named() {
        let session = crate::scratch::session("can-start-exits-test", false).expect("a session");
        stand_up(&session.tools, 2, 3, 0, 41, false).expect("a machine whose stand-ins die");

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
        stand_up(&session.tools, 2, 0, 0, 0, false).expect("a machine with two agents on it");

        let out = std::process::Command::new(session.tools.join("claude"))
            .output()
            .expect("the stand-in runs");
        assert_eq!(out.status.code(), Some(0));
    }

    /// A status no process can leave with is a road writing something it will never read back.
    #[test]
    fn a_status_outside_what_a_process_leaves_with_is_refused() {
        let session = crate::scratch::session("can-start-exits-over-test", false).expect("a session");
        let err = stand_up(&session.tools, 2, 0, 0, 256, false).expect_err("0 to 255");
        assert!(err.contains("0 to 255"), "{err}");
    }

    /// Started rather than asked, a stand-in prints what it was started with — one argument to a
    /// line, so a name a road wrote is read back whole and on its own.
    #[test]
    fn a_stand_in_prints_what_it_was_started_with_one_argument_to_a_line() {
        let session = crate::scratch::session("can-start-args-test", false).expect("a session");
        stand_up(&session.tools, 2, 3, 0, 0, false).expect("a machine with a row of models on it");

        let name = format!("{MODEL}02");
        let printed = ran(&session.tools.join("claude"), &["--model", &name, "an opening sentence"]);
        let lines: Vec<&str> = printed.lines().collect();
        assert_eq!(lines[0], "this is the verification harness standing in for claude");
        assert_eq!(lines[1], format!("{ARG} --model"));
        assert_eq!(lines[2], format!("{ARG} {name}"));
        assert_eq!(lines[3], format!("{ARG} an opening sentence"), "a whole argument, spaces and all");
    }
}


