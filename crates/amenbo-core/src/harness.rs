//! The AI harnesses that can be wired to run `amenbo agent` when a session starts, and the text each one
//! is wired with (`AMB-D-440`). **Read-only**: nothing here writes to a user's provider settings. Amenbo
//! asks, detects, and hands over the text — the wiring stays in the user's hands.
//!
//! **What is handed over is a request, not a file.** [`request`] is addressed to the AI the reader
//! already works with, and the [`configuration`] rides inside it: which file it goes in, and that
//! whatever is already there is kept, are sentences in the request rather than something the reader has
//! to work out. A whole settings document was the wrong thing to hand over — it reads as a file to
//! replace, and the reader most in need of this feature is exactly the one whose file is not empty.
//!
//! **What a probe can claim, and what it cannot.** [`probe`] answers whether a folder's settings for a
//! harness *say* to run the launch command when a session starts. Whether the hook then fires, and
//! whether its output reaches the model, is outside Amenbo: some providers load project-level settings
//! only under a trust prompt, some do not use a session-start hook's stdout for injection at all, and
//! versions have regressed on both. So the vocabulary here is **wired / unwired** — never "enabled", and
//! never a guarantee.
//!
//! **The catalog is a table, not a code path.** Every harness is one [`Harness`] row in [`HARNESSES`]:
//! where its settings live, how its session-start event is spelled, and the configuration it takes.
//! Listing one more settings-only provider is one more row — that a new entry costs no new branch is the
//! condition the shape is holding to (`AMB-D-440`), which is also why a provider needing plugin code or
//! an IDE setting does not belong here at all.
//!
//! **What the configuration injects is the launch instruction, not the spec.** Each template carries
//! [`crate::agents::launch_instruction`] — the same one line the managed block holds — and never the
//! output of `agent --json`, which is 40 KB an agent holding the instruction fetches for itself. The
//! block stays where it is: a wired folder is not a reason to strip it, and the hook adds reach over the
//! block, not content.
//!
//! **There are two tables, because there are two questions** (`AMB-D-791`). Being wired and being started
//! ask different things of a provider, and a row answering for both could hold neither on its own:
//!
//! | | [`HARNESSES`] — wired | [`LAUNCHES`] — started |
//! |---|---|---|
//! | what it does | a hook in the folder's own settings runs `amenbo agent` when a session starts | Amenbo opens a pane and starts the agent in it, saying the same thing as its first argument |
//! | what a row holds | [`event`](Harness::event), [`places`](Harness::places), [`home`](Harness::home), [`paste_into`](Harness::paste_into), [`template`](Harness::template), [`json_layers`](Harness::json_layers) | [`command`](Launch::command), [`prompt_flag`](Launch::prompt_flag), [`model_flag`](Launch::model_flag), [`models`](Launch::models), [`switch`](Launch::switch), [`rename`](Launch::rename), [`resume`](Launch::resume) |
//!
//! One product can stand in both, and Claude Code does — that repetition is what this costs. What it buys
//! is a row for a provider that can only be started: with no session-start hook to write, the price of a
//! seat at one table used to be a template for a wiring it does not have.
//!
//! **The same instruction travels both ways.** A pane Amenbo opens itself starts the agent with
//! [`opening`] — the instruction handed over as an argument, read before the program has drawn anything,
//! rather than printed by a hook the folder may never have been wired for. Which flag the argument goes
//! behind is the one thing the providers spell differently, so it is a column ([`Launch::prompt_flag`])
//! and not a branch, like everything else in these tables. The flag a model is named behind
//! ([`Launch::model_flag`]) is the second such column, and the model itself is not a column at all:
//! Amenbo keeps no list of model names (`AMB-D-865`).
//!
//! **A pane that is already running is moved by typing at it, and that is a third column**
//! ([`Launch::switch`]). There is no flag to pass a program that started minutes ago: the only way in
//! is the provider's own slash command, put in the input box the way a person would type it. What the
//! six differ on is not just its spelling but whether the model's name may go on that line at all —
//! two of them send an unrecognised line to the model as a prompt, which costs the reader money
//! (`AMB-T-4581`).

use std::path::{Path, PathBuf};

use serde::Serialize;

/// One AI harness Amenbo knows how to be wired into — the wiring catalog's row (`AMB-D-440`).
///
/// The fields split in two: [`places`](Harness::places) and [`event`](Harness::event) are what a probe
/// reads, and [`paste_into`](Harness::paste_into) with [`template`](Harness::template) are what a
/// [`request`] is built from. Nothing here is a schema — the config shapes have nothing in common (JSON
/// depth, event casing, which key holds the command), which is why the whole of each one is carried as
/// text.
///
/// **What starts the provider is not here.** That is the other table's ([`Launch`]), and the two are
/// joined by [`id`](Harness::id) where a product stands in both.
pub struct Harness {
    /// The stable token a face names this harness by (`claude-code`), lowercase and hyphenated. It is a
    /// key, not a rendering: a user reads [`label`](Harness::label).
    pub id: &'static str,
    /// The product's own name for itself, as it appears in its documentation.
    pub label: &'static str,
    /// How this provider spells its session-start event. Matched **case-insensitively**, because that is
    /// the one thing the providers differ on that a probe would otherwise have to know per row
    /// (`SessionStart` here, `sessionStart` there).
    pub event: &'static str,
    /// Where this provider's settings live, relative to the folder. A path that is a **directory** on
    /// disk is read as every `*.json` directly inside it — the shape a provider that takes any filename
    /// under a hooks directory needs, with no second vocabulary for the ones that name a single file.
    pub places: &'static [&'static str],
    /// The directory whose presence in a folder says **this provider is used here** — the trace a notice
    /// names a provider by (`AMB-D-440`). It is carried per row rather than derived from
    /// [`places`](Harness::places), because the directory a place sits in is not always the provider's:
    /// `.github` belongs to GitHub and is in nearly every repository, while `.github/hooks` is the one
    /// that says this provider's hooks are kept here. Every place is inside it.
    pub home: &'static str,
    /// The file [`configuration`] is written for, and the one a [`request`] names. One of
    /// [`places`](Harness::places): a configuration landing where a probe does not look would read as
    /// unwired forever.
    pub paste_into: &'static str,
    /// The configuration, with `{instruction}` standing in for the launch instruction.
    pub template: &'static str,
    /// How many JSON strings the `{instruction}` placeholder sits inside — 1 where the command is
    /// `echo '<instruction>'`, 2 where the command echoes a JSON document that carries it. The
    /// instruction is escaped that many times before it is substituted, so a provider that needs the
    /// text one layer deeper is a number, not a branch.
    pub json_layers: u8,
}

/// Every harness Amenbo can be wired into, in the order a face offers them — the wiring catalog
/// (`AMB-D-440`). The five whose session-start hook Amenbo knows how to write; what a pane starts is
/// [`LAUNCHES`].
pub static HARNESSES: &[Harness] = &[
    Harness {
        id: "claude-code",
        label: "Claude Code",
        event: "SessionStart",
        // `settings.local.json` is the same folder's settings kept out of the repository, and a user who
        // wired it there has wired it: leaving it out would ask them again forever.
        places: &[".claude/settings.json", ".claude/settings.local.json"],
        home: ".claude",
        paste_into: ".claude/settings.json",
        // Plain stdout is what this one adds to the session, so the command is the instruction itself.
        template: r#"{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "echo '{instruction}'"
          }
        ]
      }
    ]
  }
}"#,
        json_layers: 1,
    },
    Harness {
        id: "github-copilot",
        label: "GitHub Copilot CLI",
        event: "sessionStart",
        places: &[".github/hooks"],
        home: ".github/hooks",
        paste_into: ".github/hooks/amenbo.json",
        template: r#"{
  "version": 1,
  "hooks": {
    "sessionStart": [
      {
        "type": "command",
        "bash": "echo '{\"additionalContext\": \"{instruction}\"}'"
      }
    ]
  }
}"#,
        json_layers: 2,
    },
    Harness {
        id: "cursor",
        label: "Cursor",
        event: "sessionStart",
        places: &[".cursor/hooks.json"],
        home: ".cursor",
        paste_into: ".cursor/hooks.json",
        template: r#"{
  "version": 1,
  "hooks": {
    "sessionStart": [
      {
        "command": "echo '{\"additional_context\": \"{instruction}\"}'"
      }
    ]
  }
}"#,
        json_layers: 2,
    },
    Harness {
        id: "codex-cli",
        label: "Codex CLI",
        event: "SessionStart",
        // Two files, one wiring: this provider reads hooks from its own file or from inline tables in the
        // folder's `config.toml`, and either is where a user may have written it.
        places: &[".codex/hooks.json", ".codex/config.toml"],
        home: ".codex",
        paste_into: ".codex/hooks.json",
        template: r#"{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "echo '{\"hookSpecificOutput\":{\"hookEventName\":\"SessionStart\",\"additionalContext\":\"{instruction}\"}}'"
          }
        ]
      }
    ]
  }
}"#,
        json_layers: 2,
    },
    Harness {
        id: "gemini-cli",
        label: "Gemini CLI",
        event: "SessionStart",
        places: &[".gemini/settings.json"],
        home: ".gemini",
        paste_into: ".gemini/settings.json",
        template: r#"{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "echo '{\"hookSpecificOutput\":{\"additionalContext\":\"{instruction}\"}}'"
          }
        ]
      }
    ]
  }
}"#,
        json_layers: 2,
    },
];

/// The harness with this [`id`](Harness::id), or `None` when nothing lists it.
pub fn find(id: &str) -> Option<&'static Harness> {
    HARNESSES.iter().find(|harness| harness.id == id)
}

/// One AI Amenbo knows how to **start** — the launch catalog's row (`AMB-D-791`).
///
/// A row here holds what opening a pane needs and nothing else: the program to run, and the flag its
/// opening prompt goes behind. Being able to write the provider's session-start hook is not asked of it,
/// which is the whole reason this table is its own — a provider with no hook to write is still one a
/// person opens a terminal on.
pub struct Launch {
    /// The stable token a face names this provider by (`claude-code`), lowercase and hyphenated. Where a
    /// product stands in both tables it is the same token as [`Harness::id`], which is what joins them.
    pub id: &'static str,
    /// The product's own name for itself, as it appears in its documentation.
    pub label: &'static str,
    /// What this provider is started as in a terminal — the program name, resolved against the user's
    /// own `PATH` rather than a path of Amenbo's ([`crate::wake`]).
    pub command: &'static str,
    /// The flag this provider takes an opening prompt behind, or `None` where the prompt is simply its
    /// first argument — the one spelling these differ on ([`opening`]).
    ///
    /// What is named here is always the **interactive** one. Several of them also take a prompt behind
    /// `-p`, which runs it and exits: a pane started that way holds a program that is already gone by
    /// the time the person looks at it.
    pub prompt_flag: Option<&'static str>,
    /// The flag this provider takes a model name behind ([`opening`]). All six spell one and they do
    /// not agree on which — `--model` here, `-m` there — so it is a column and not a branch, the same
    /// as [`prompt_flag`](Launch::prompt_flag).
    ///
    /// **What goes behind it is the provider's own name for a model, and Amenbo holds no list of
    /// those** (`AMB-D-865`). Nothing here checks the name: a model the provider does not know, or one
    /// the account cannot reach, is refused by the provider in its own words on the pane's screen —
    /// which is the only place that answer exists.
    pub model_flag: &'static str,
    /// How this provider is asked what models it can be started on, or `None` where it cannot be
    /// asked at all ([`crate::agent_models`], `AMB-D-865`).
    ///
    /// It is a column here rather than a table of its own for the reason the prompt flag is one: the
    /// question is per provider, and the answer is a property of the row that says how to start it.
    /// What is **not** here is any model name — Amenbo holds none, and this says only how to go and
    /// ask.
    pub models: Option<crate::agent_models::Ask>,
    /// How a pane already running this provider is asked to change model ([`Switch`], `AMB-D-865`).
    ///
    /// A column for the same reason the flags are: every one of the six has a way, they spell it
    /// differently, and what a row of the table cannot become is a branch in the code.
    pub switch: Switch,
    /// How a pane already running this provider is asked to change its own session name, or `None`
    /// where the provider has no such command ([`Rename`], `AMB-D-872`).
    ///
    /// A column for the same reason [`switch`](Launch::switch) is one: four of the six have a way,
    /// and the two that do not are rows rather than an exception written into the code that types it.
    pub rename: Option<Rename>,
    /// How a pane running this provider comes back into the session it was running, or `None`
    /// where the way back is not a flag on the line ([`Resume`], `AMB-D-869`).
    ///
    /// A column for the same reason [`rename`](Launch::rename) is one: four of the six take a
    /// handle beside the opening prompt, and the two that do not are rows rather than an exception —
    /// Codex carries a home instead (`AMB-T-4640`), and Gemini carries nothing that survives its own
    /// restart (`AMB-T-4659`).
    pub resume: Option<Resume>,
    /// Whether this row has been watched starting the provider on a real machine (`AMB-T-3819`).
    ///
    /// Every row is written from the product's own documentation, and that is not the same as having
    /// seen a pane open on it and take the instruction. The mark is carried rather than the row left
    /// out: an unconfirmed row is still the best answer Amenbo has for somebody who works with that
    /// tool, and what it must not do is read as tried when nobody has tried it.
    pub confirmed: bool,
}

/// What a running pane is asked to change model with — the provider's own slash command, typed into
/// it the way a person would type it (`AMB-D-865`).
///
/// **There is no other road.** A model named on the launch line is settled for a program that has not
/// started yet; a pane a person has been working in for an hour started long ago, and nothing outside
/// it can reach the model it is answering on. What Amenbo can do is put the provider's own command in
/// the box and let the provider do the rest, which is the pane passing this through the way it passes
/// everything else through (`AMB-D-747`).
pub struct Switch {
    /// What is typed — `/model` for five of the six, `/models` for OpenCode.
    pub command: &'static str,
    /// Whether the model's name may go on that line ([`Carries`]).
    pub carries: Carries,
    /// Where this machine keeps the change once it is made, past the session — or `None` where the
    /// provider changes only the session in front of the reader.
    ///
    /// **It is here to be said before the press, not after** (`AMB-T-4581`). Three of these rewrite a
    /// file the person edits by hand, and a fourth keeps it in a database of its own; a control that
    /// moved somebody's default without saying so would be Amenbo writing a provider's settings by
    /// the back door, which `AMB-D-440` refuses at the front.
    pub keeps: Option<&'static str>,
}

/// Whether a provider's model command takes the name on its line — the one thing the six differ on
/// that costs money to get wrong (`AMB-T-4581`).
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum Carries {
    /// The name goes on the line and the return settles it: `/model sonnet`, and the change is done.
    Named,
    /// The line is the command alone. A name behind it is **sent to the model as a prompt** — Codex
    /// and OpenCode were both watched billing for one — so what the press does is open the provider's
    /// own picker, and the choosing is the person's.
    Picker,
    /// The command alone opens the picker, and the name is then put in the picker's own search box
    /// rather than on the command's line. Nothing is submitted behind it: what the box narrows to is
    /// for the reader to confirm, the way every other paste into a pane is (`AMB-D-793`).
    Filter,
}

/// The command's line for a running pane, and what follows it — [`switching`]'s answer.
#[derive(PartialEq, Eq, Debug)]
pub struct Switching {
    /// What goes in the input box and is submitted, as a person typing it would.
    pub line: String,
    /// What is pasted after that line has gone, submitting nothing — the picker's search text, or
    /// `None` where there is no second half.
    pub then: Option<String>,
    /// Whether the line settles the model on its own. False is the provider's picker standing open
    /// with the choosing still to do, which is a thing a face has to say rather than claim the model
    /// has moved.
    pub settles: bool,
}

/// What to put into a pane running `launch` to move it to `model` (`AMB-D-865`).
///
/// **The name is left off the line wherever the provider would read it as a prompt.** That is the
/// whole of the branch, and it is why this is written once here rather than composed by whoever is
/// drawing the control: Codex and OpenCode were both measured sending `/model <name>` to the model
/// and being billed for the answer (`AMB-T-4581`).
///
/// A name that is blank or only spaces is no name at all, the same reading [`opening`] gives one: it
/// arrives from a text box, and `/model ` with nothing behind it is a line the reader did not mean.
pub fn switching(launch: &Launch, model: Option<&str>) -> Switching {
    let name = model.map(str::trim).filter(|one| !one.is_empty());
    match (launch.switch.carries, name) {
        (Carries::Named, Some(name)) => Switching {
            line: format!("{} {name}", launch.switch.command),
            then: None,
            settles: true,
        },
        (Carries::Filter, Some(name)) => Switching {
            line: launch.switch.command.to_string(),
            then: Some(name.to_string()),
            settles: false,
        },
        _ => Switching {
            line: launch.switch.command.to_string(),
            then: None,
            settles: false,
        },
    }
}

/// What a running pane is asked to change its own session name with — the provider's own slash
/// command, typed into it the way a person would type it (`AMB-D-872`).
///
/// **Amenbo's card is the name; this is the copy.** A provider that titles its own sessions takes the
/// title from what it was told, and every pane Amenbo opens is told the same opening instruction — so
/// that provider's own list of sessions comes back reading the same title down the column. Typing the
/// card in as a rename is what parts them there.
///
/// Unlike [`Switch`] there is no branch on where the name goes: all four were watched taking it on
/// the command's own line (`AMB-T-4652`), so nothing here answers [`Carries`].
pub struct Rename {
    /// What is typed — `/rename` for all four of the ones that have it.
    pub command: &'static str,
    /// How long a name this provider takes, in characters, or `None` where none was found.
    ///
    /// It is measured rather than documented: Copilot refuses at 101 and says so, and the other
    /// three took 127 characters without a word (`AMB-T-4652`). Amenbo's own names are cut to
    /// [`crate::frames::NAME_LIMIT`] before they get here, which is under every bound in the table —
    /// the column is what keeps that true when a row is added.
    pub limit: Option<usize>,
    /// Whether the pane's terminal title (OSC 0) moves with the name, or only the provider's own
    /// list of sessions does.
    ///
    /// Codex is the row this is false on: it renames the thread and leaves the title on the folder's
    /// name (`AMB-T-4652`). It is said here because the two faces of one rename are what a reader
    /// compares, and a face that promised both would be wrong on that pane.
    pub titles: bool,
}

/// How a pane comes back into the session it was running — the resume catalog's column
/// (`AMB-D-869`).
///
/// **Five of the six have a way back, and no two of them spell it alike** (`AMB-T-4630`). What
/// parts them is not the flag but who decides the handle: two take one Amenbo hands them as the
/// session starts, one takes an unknown one and creates it, one names its own and has to be asked
/// afterwards, and one is not a session id at all. So the column carries the two halves separately —
/// [`back`](Resume::back) is the way in, [`issue`](Resume::issue) is who decides — and a provider
/// with no way back at all is a row with `None` on it rather than a flag nothing takes.
///
/// **`None` here is two different answers, and the rows say which.** Codex's handle is a home of its
/// own rather than an id on a line — an environment variable and a directory to keep
/// (`AMB-T-4640`), which a flag column could hold neither of. Gemini has the flags and cannot use
/// them: it restarts itself on the same argv, so a handle on the line kills the second start
/// (`AMB-T-4659`).
pub struct Resume {
    /// The flag the handle goes behind to come back into that session — `--resume` for three of
    /// them, `-s` for OpenCode.
    pub back: &'static str,
    /// How a handle Amenbo decided is put on the line as the session **starts**, or `None` where
    /// the provider names its own and it has to be read back afterwards ([`ask`](Resume::ask)).
    pub issue: Option<Issue>,
    /// How this provider is asked which sessions it has, for the row that names its own — or `None`
    /// where Amenbo decides the handle and has nothing to ask.
    pub ask: Option<crate::agent_sessions::Ask>,
}

/// Who decides the handle a session starts under (`AMB-T-4630`).
///
/// It is an enum rather than a second flag column because the two spellings mean different things
/// to the provider: one is being told the id of a session it is about to create, and the other is
/// being asked to come back into a session that does not exist yet.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum Issue {
    /// A flag of the provider's own for it — `--session-id <uuid>`, which the two that have one
    /// take beside the opening prompt.
    Flag(&'static str),
    /// The same flag as coming back. Cursor was watched creating a session under a handle it had
    /// never seen, so the way in and the way it starts are one line (`AMB-T-4630`).
    Back,
}

/// The handle a pane is opened on — one Amenbo has just decided, or the one the pane came back
/// with (`AMB-D-869`).
///
/// The two are separate because the flag they go behind can be, and the caller is the only one that
/// knows which it is holding: a handle read off a pane's row is a session that exists, and one
/// [`issue`] has just minted is a name for a session about to be made.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum Handle<'a> {
    /// A handle Amenbo decided, for a session the provider is about to create under it.
    New(&'a str),
    /// The handle a pane came back with, for the session it was running.
    Back(&'a str),
}

/// A handle for a session about to start on `launch`, where Amenbo is the one that decides it.
///
/// `None` for the three rows it is not Amenbo's to decide: OpenCode names its own and is asked
/// afterwards ([`Resume::ask`]), Codex carries a home rather than an id (`AMB-T-4640`), and Gemini
/// takes no handle at all (`AMB-T-4659`).
///
/// **A version 4 UUID, because that is the shape the providers were watched taking** — Claude Code
/// refuses `--session-id` anything else, and the other two were only ever handed one
/// (`AMB-T-4630`). The randomness is the operating system's, the same source a pane's own session
/// id is drawn from: a handle is written down and read back a run later, so a counter would name
/// one session this run and a different one the next.
pub fn issue(launch: &Launch) -> Option<String> {
    launch.resume.as_ref()?.issue.map(|_| uuid_v4())
}

/// Sixteen bytes of the operating system's randomness, spelled as a version 4 UUID.
fn uuid_v4() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("failed to draw OS randomness");
    // The two fields a version 4 UUID is read by: the version in the high nibble of byte 6, and the
    // variant in the top bits of byte 8. A provider that parses the text checks both.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

/// Every AI Amenbo knows how to start, in the order a face offers them — the launch catalog
/// (`AMB-D-791`). Wider than [`HARNESSES`]: a provider earns a row here by being startable, whether or
/// not its session-start hook is one Amenbo can write.
pub static LAUNCHES: &[Launch] = &[
    Launch {
        id: "claude-code",
        label: "Claude Code",
        command: "claude",
        // The prompt is this one's first argument; `-p` is the form that prints and exits.
        prompt_flag: None,
        // Aliases (`opus`, `haiku`) and full names are both taken here.
        model_flag: "--model",
        // No list door at all: `claude models` is read as a prompt and answered by the model itself
        // (`AMB-T-4581`). What names the aliases is the help text, so the help text is what is read.
        models: Some(crate::agent_models::Ask {
            args: &["--help"],
            reading: crate::agent_models::Reading::HelpAliases,
        }),
        // `/model sonnet` settles it in one line, and settles it as the reader's own default: the
        // session-only form is a key pressed inside the picker and there is no line that spells it
        // (`AMB-T-4581`).
        switch: Switch {
            command: "/model",
            carries: Carries::Named,
            keeps: Some("~/.claude/settings.json"),
        },
        // Watched renaming the thread and the title both, and taking the name on the line
        // (`AMB-T-4652`).
        rename: Some(Rename { command: "/rename", limit: None, titles: true }),
        // Takes the id of the session it is about to make, and refuses anything that is not a
        // UUID. Coming back on one it has no record of is an error and exit 1 (`AMB-T-4630`).
        resume: Some(Resume {
            back: "--resume",
            issue: Some(Issue::Flag("--session-id")),
            ask: None,
        }),
        confirmed: true,
    },
    Launch {
        id: "codex-cli",
        label: "Codex CLI",
        command: "codex",
        prompt_flag: None,
        model_flag: "-m",
        // Answers without a key — the same list with `CODEX_HOME` empty — and its `slug` is the
        // spelling `-m` takes (`AMB-T-4576`).
        models: Some(crate::agent_models::Ask {
            args: &["debug", "models"],
            reading: crate::agent_models::Reading::CodexCatalog,
        }),
        // A name behind `/model` here is sent to the model as a prompt: the measurement reached the
        // API and came back with a usage limit (`AMB-T-4581`). The picker it opens is two steps —
        // model, then reasoning — and both of them are the reader's.
        switch: Switch {
            command: "/model",
            carries: Carries::Picker,
            keeps: Some("~/.codex/config.toml"),
        },
        // The one row the rename is half-seen on: the thread takes the name, the terminal title
        // stays on the folder it was started in (`AMB-T-4652`).
        rename: Some(Rename { command: "/rename", limit: None, titles: false }),
        // The way back here is a home of its own rather than a handle on the line — an environment
        // variable and a directory to keep, which is `AMB-T-4640`'s and not a flag column's.
        resume: None,
        confirmed: true,
    },
    Launch {
        id: "github-copilot",
        label: "GitHub Copilot CLI",
        command: "copilot",
        // `-p` runs a prompt and exits here, so the interactive one is spelled separately.
        prompt_flag: Some("-i"),
        model_flag: "--model",
        // The one provider with nowhere to ask: no list command, and its ACP session offers a mode
        // and a permission and no model at all (`AMB-T-4581`). What `copilot help config` prints is
        // a paragraph of documentation, which is a table Amenbo would be copying rather than asking
        // for — and the measured one was wrong for this account in all 26 rows (`AMB-T-4576`).
        models: None,
        // The one provider whose switch is session-only by design: its own picker says so, and the
        // default is moved by a different command (`/config model`) that Amenbo does not send.
        switch: Switch { command: "/model", carries: Carries::Named, keeps: None },
        // The only one that answers with a bound: 101 characters is refused in its own words
        // (`AMB-T-4652`), counted in characters rather than bytes.
        rename: Some(Rename { command: "/rename", limit: Some(100), titles: true }),
        // Takes the id it is about to make, the same as Claude Code. Coming back on one it does not
        // know is the one row that neither stops nor carries on quietly: it says so in the pane and
        // then stands there as a new session (`AMB-T-4630`).
        resume: Some(Resume {
            back: "--resume",
            issue: Some(Issue::Flag("--session-id")),
            ask: None,
        }),
        confirmed: true,
    },
    Launch {
        id: "gemini-cli",
        label: "Gemini CLI",
        command: "gemini",
        // A bare query is interactive by default, but the default is a setting: the flag that says
        // "run this and stay" is not.
        prompt_flag: Some("-i"),
        model_flag: "-m",
        // Asked over ACP, which is this one's only list door (`AMB-T-4581`). The pane itself is
        // still a terminal running the ordinary TUI — what `AMB-D-747` refused is filling a pane
        // with ACP, and this is a second process started to ask one question and killed.
        models: Some(crate::agent_models::Ask {
            args: &["--acp"],
            reading: crate::agent_models::Reading::AcpSession,
        }),
        // An argument here is dropped without a word, so the line would look like it worked and
        // change nothing. What opens is a two-step picker, and remembering the answer past this
        // session is a toggle inside it that starts off.
        switch: Switch { command: "/model", carries: Carries::Picker, keeps: None },
        // No such command at all — typed in, it goes to the model as a sentence and is answered as
        // one (`AMB-T-4652`), which is why this is `None` rather than a row with a spelling.
        rename: None,
        // **The one row with both flags and no way back.** A UUID handed to `--session-id` was
        // watched making the round trip (`AMB-T-4630`), and the flag is still unusable here: this
        // provider runs as a supervisor over its own work process and re-spawns it on the *same*
        // argv whenever it asks to restart — the trust prompt being answered, the auth type
        // changing, a setting being written. The second start finds the session the first one
        // already made and exits on it (`Session ID "…" already exists`, `AMB-T-4659`), taking the
        // pane with it. Nothing Amenbo puts on that line survives, and standing the pane back up
        // afterwards is the covering-for-a-provider `AMB-D-869` refuses.
        //
        // **`--resume` is no way in either.** A handle it has never seen is refused rather than
        // created (Cursor's road), and a session it has just made is not in `--list-sessions` until
        // there is something in it to resume — so the handle cannot be read back as the pane starts
        // the way OpenCode's is. Reading it back later, once the conversation has content, is a
        // road that stays open and is not this row's (`AMB-T-4659`).
        resume: None,
        confirmed: true,
    },
    Launch {
        id: "opencode",
        label: "OpenCode",
        command: "opencode",
        // The base command's own flag. `opencode run` takes a prompt too and is the non-interactive
        // form, so a pane started that way would hold a program that has already printed and gone.
        prompt_flag: Some("--prompt"),
        // A name here is `provider/model`, not a model on its own. The flag is the base command's, like
        // the prompt's above it — `opencode run` spells it the same, and that is the form to stay off.
        model_flag: "-m",
        // Answers without a key, and the list is whatever this machine's providers come to — add a
        // key and the same command answers with more (`AMB-T-4576`).
        models: Some(crate::agent_models::Ask {
            args: &["models"],
            reading: crate::agent_models::Reading::QualifiedLines,
        }),
        // Spelled with the plural, and a name on its line is billed the way Codex's is. What the
        // picker offers instead is a search box that takes the name as text, which is why this is
        // the one row whose second half goes anywhere (`AMB-T-4581`).
        switch: Switch {
            command: "/models",
            carries: Carries::Filter,
            keeps: Some("~/.local/share/opencode/opencode.db"),
        },
        // The second one with no rename: the command is not offered, and the title it shows comes
        // from whatever it was first said (`AMB-T-4652`).
        rename: None,
        // The one row that names its own handle: there is no flag to be told one, so the pane is
        // started and the id is read back out of the provider's own list afterwards
        // (`AMB-T-4630`).
        resume: Some(Resume {
            back: "-s",
            issue: None,
            ask: Some(crate::agent_sessions::Ask {
                args: &["session", "list", "--format", "json"],
                reading: crate::agent_sessions::Reading::JsonSessions,
            }),
        }),
        confirmed: true,
    },
    Launch {
        id: "cursor",
        label: "Cursor",
        command: "cursor-agent",
        // Not `agent`, the shorter name this was also given: it is a general enough word that a script
        // of the reader's own could answer to it, and starting that would open a pane on somebody
        // else's program.
        prompt_flag: None,
        model_flag: "--model",
        // The one that needs the reader signed in: unauthenticated it exits 1 and names the ways to
        // sign in, which reaches a face as no models rather than as a message (`AMB-T-4576`).
        models: Some(crate::agent_models::Ask {
            args: &["--list-models"],
            reading: crate::agent_models::Reading::IdThenLabel,
        }),
        // Takes the name as a filter on its line and settles on the return, which is the same one
        // press as the two above it.
        switch: Switch {
            command: "/model",
            carries: Carries::Named,
            keeps: Some("~/.cursor/cli-config.json"),
        },
        // Takes the name on the line and moves both faces, saying nothing on the screen about it
        // (`AMB-T-4652`).
        rename: Some(Rename { command: "/rename", limit: None, titles: true }),
        // One flag both ways: a handle it has never seen is created under that handle, so the line
        // that starts a session and the line that comes back into one are the same
        // (`AMB-T-4630`). **A handle it cannot find is a new session and no word about it** — the
        // one row that fails silently, and `AMB-D-869` leaves it there rather than covering for it.
        resume: Some(Resume { back: "--resume", issue: Some(Issue::Back), ask: None }),
        // Written from the documentation and never run — the tool is not on the machine the other five
        // were tried on (`AMB-T-3838`).
        confirmed: false,
    },
];

/// The launch row with this [`id`](Launch::id), or `None` when nothing lists it.
pub fn find_launch(id: &str) -> Option<&'static Launch> {
    LAUNCHES.iter().find(|launch| launch.id == id)
}


/// The configuration for one harness, with the launch instruction in place. `cmd` is the launch command
/// name ([`crate::config::Paths::command_name`]), so a dev-channel build describes wiring for the binary
/// the user is actually running.
///
/// This is the payload, not the hand-over: what a face gives a reader is [`request`], which carries this
/// inside it. It is public because the two are separately true — a caller standing in for the AI that
/// does the merge needs the settings alone, and reading them out of the request's prose would make that
/// caller the judge of prose it does not own.
pub fn configuration(harness: &Harness, cmd: &str) -> String {
    let instruction = json_escaped(&crate::agents::launch_instruction(cmd), harness.json_layers);
    harness.template.replace("{instruction}", &instruction)
}

/// What `launch` is started with so that the launch instruction is the first thing said to it: the
/// arguments that follow the program ([`Launch::command`]), in the order they are written — and,
/// where the line comes back into a conversation that has already had one, the arguments without it
/// (`handle` below).
///
/// **An argument rather than something typed at the program**, because it is the certain route: the
/// agent is handed this before it starts, so nothing about what it draws first can eat it, and it is
/// done in one move. A terminal cannot know when the agent in it is ready to be typed at, and there is
/// no signal that says so — a program turns bracketed paste on within a tenth of a second of starting,
/// long before it has drawn an input box (`AMB-T-3819`).
///
/// **That is a reason to prefer this route, and no longer a reason there is only one.** Typing at the
/// program was refused here on two grounds — that readiness cannot be known, and that a trust prompt
/// standing in front would eat what was typed — and both were grounds against typing *blind*. Amenbo
/// holds the reading side of the terminal as well, so the sentence can be pasted, watched for on the
/// screen, and submitted only once it is there; neither ground survives that (`AMB-D-793`, walked in
/// `app/src-tauri/src/handover.rs`). It stays the second route rather than the first: it is a loop with
/// a patience and a way of running out of one, and this is an argument.
///
/// `cmd` is the launch command name ([`crate::config::Paths::command_name`]), so a dev-channel build
/// tells the agent to run the binary the person is actually running — the same text, and the same
/// reason, as [`configuration`].
///
/// **Nothing here is quoted or escaped.** What comes back is an argument list, and the shell it will be
/// written into belongs to the caller (`app/src-tauri/src/launch.rs`): a quoting rule chosen in core
/// would be one written for a shell core cannot see.
///
/// What is said is [`crate::agents::pane_instruction`] rather than the launch instruction alone: this
/// route opens panes and nothing else, and a pane is where the talk vocabulary can be used.
///
/// **`model` is the provider's own name for a model, or `None` for however the provider is already
/// set up.** Nothing is named where nobody chose one: an unchosen model leaves the flag off the line
/// entirely, so the CLI's own setting — which is where the answer lived before Amenbo asked — keeps
/// deciding (`AMB-D-865`). A name that is blank or only spaces is read as no choice for the same
/// reason: it reaches here from a text field, and passing it on would open the pane on
/// `--model ''`, which every one of these providers refuses.
///
/// It goes in front of the prompt, which is where all six were watched taking it (`AMB-T-4576`), and
/// in front of [`prompt_flag`](Launch::prompt_flag) because that flag takes the argument straight
/// after it. **It is named on a line that comes back as well**: which model answers is as true of a
/// conversation carried on as of one started.
///
/// **`handle` is the session this pane is opened on, and `None` is a pane opened on none**
/// ([`Handle`], `AMB-D-869`). It rides on the same line as the opening prompt rather than in a
/// second move: every row that takes a handle takes the two together.
///
/// **A line that comes back into a conversation says nothing** ([`Handle::Back`], `AMB-T-4663`).
/// The instruction is a first thing to say, and a conversation being resumed has already been said
/// one: said again it arrives as a turn the person never took, and the agent spends it looking up
/// what it was told the run before. The flag goes with it — [`prompt_flag`](Launch::prompt_flag)
/// exists to take a prompt, and with no prompt behind it it would take whatever came next.
///
/// **A handle Amenbo has just decided is not that**, even on the row that starts a session under
/// the flag it comes back on ([`Issue::Back`]): the session is being made here, so it is told where
/// it is working. Nor are the two rows that take no handle on their line ([`Resume`] `None`):
/// nothing on either line resumes anything, so every line they have is a session starting, and both
/// are told where they are working.
///
/// **What a provider does with a handle it cannot find is still the provider's**: Cursor opens a new
/// session and says nothing about it, and that session now starts unsaid as well. `AMB-D-869` left
/// that row where it stands rather than covering for it, and this changes only what it is said.
pub fn opening(
    launch: &Launch,
    cmd: &str,
    model: Option<&str>,
    handle: Option<Handle<'_>>,
) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    if let Some(model) = model.map(str::trim).filter(|name| !name.is_empty()) {
        args.push(launch.model_flag.to_string());
        args.push(model.to_string());
    }
    let way_back = resuming(launch, handle);
    let carrying_on = way_back.is_some() && matches!(handle, Some(Handle::Back(_)));
    if let Some((flag, id)) = way_back {
        args.push(flag.to_string());
        args.push(id.to_string());
    }
    if !carrying_on {
        args.extend(launch.prompt_flag.map(str::to_string));
        args.push(crate::agents::pane_instruction(cmd));
    }
    args
}

/// The flag a handle goes behind on this row's launch line, and the handle — or nothing where this
/// row puts none there.
///
/// Three ways to have nothing, and they are different things: the pane is being opened on no
/// session at all, the row has no way back on a line ([`Launch::resume`]), or the row names its own
/// handle and the one it will name is not known yet ([`Resume::ask`]). None of them is a flag with
/// an empty value, which is what every provider would refuse.
fn resuming<'a>(launch: &Launch, handle: Option<Handle<'a>>) -> Option<(&'static str, &'a str)> {
    let resume = launch.resume.as_ref()?;
    match handle? {
        Handle::Back(id) => Some((resume.back, id)),
        Handle::New(id) => match resume.issue? {
            Issue::Flag(flag) => Some((flag, id)),
            Issue::Back => Some((resume.back, id)),
        },
    }
}

/// What a face hands the reader for one harness: a request addressed to the AI they work with, carrying
/// the [`configuration`] (`AMB-D-440`).
///
/// **The reader's AI is the hand that edits the file**, which is what the wording has to make true.
/// Amenbo still writes nothing — the request travels through the reader, who decides whether to give it
/// to anyone — but the work it asks for is an edit to an existing file, so it says so: which file, that
/// what is already in there stays, and that nothing else is to change. Handing over a whole settings
/// document instead left that judgment with the reader, who then had to merge by hand exactly when their
/// file was not empty.
///
/// English, like the launch instruction and the managed block: the recipient is a model, and one text is
/// one text to keep in step with the wording those two carry.
pub fn request(harness: &Harness, cmd: &str) -> String {
    format!(
        "Please start this folder's AI on Amenbo, by wiring {label}'s session-start hook.\n\
         \n\
         Merge the configuration below into `{paste_into}` in this folder. Keep everything that file \
         already holds — add to its hooks rather than replacing them — and create it if it is not \
         there. Change nothing else, and tell me what you changed.\n\
         \n\
         ```json\n\
         {configuration}\n\
         ```\n\
         \n\
         Once it is in place, every session in this folder opens by running `{cmd} agent --json`.",
        label = harness.label,
        paste_into = harness.paste_into,
        configuration = configuration(harness, cmd),
    )
}

/// `text` as the body of a JSON string, `layers` deep. Nothing in the instruction needs escaping today —
/// which is exactly why this is here: the escape is what keeps that from being a property of the
/// sentence, so rewording it can never quietly emit a configuration that will not parse.
fn json_escaped(text: &str, layers: u8) -> String {
    let mut out = text.to_string();
    for _ in 0..layers {
        // serde_json writes the surrounding quotes; the body is what goes inside the template's own.
        let quoted = serde_json::to_string(&out).unwrap_or_else(|_| format!("\"{out}\""));
        out = quoted[1..quoted.len() - 1].to_string();
    }
    out
}

/// What a folder says about one harness: wired, and where it says so (`AMB-D-440`). `wired_at` is the
/// file the wiring was read from — a user told "unwired" while a file of theirs says otherwise needs to
/// know which file Amenbo looked at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Wiring {
    /// The harness this answers for ([`Harness::id`]).
    pub id: &'static str,
    /// The harness's own name, so a face can render this row without a second lookup.
    pub label: &'static str,
    /// Where the wiring was found, relative to the probed folder — `None` when there is none.
    pub wired_at: Option<PathBuf>,
    /// Whether the folder shows a trace of this provider being used here — its [`home`](Harness::home)
    /// directory exists. It is what lets a notice name the provider this user actually has instead of
    /// reciting the catalog, and it is independent of the wiring: a traced provider is usually the
    /// unwired one worth mentioning, and an untraced one is not evidence of anything either way.
    pub traced: bool,
}

impl Wiring {
    /// Whether this folder is wired for the harness.
    pub fn wired(&self) -> bool {
        self.wired_at.is_some()
    }
}

/// Every harness's answer for `dir`, in [`HARNESSES`] order. `cmd` is the launch command name, so the
/// dev channel is not read as the production binary's wiring and the other way round.
///
/// The judgment is deliberately shallow: a settings file counts as wired when it names both the launch
/// command's `agent` call and the provider's session-start event. Neither on its own is the wiring — a
/// folder can call `agent` from some other hook, and a session-start hook can be doing something else
/// entirely — and parsing five schemas to say more would still not answer the question anybody actually
/// has, which is whether the hook fires. See the module docs.
pub fn probe(dir: &Path, cmd: &str) -> Vec<Wiring> {
    HARNESSES
        .iter()
        .map(|harness| Wiring {
            id: harness.id,
            label: harness.label,
            wired_at: wired_at(dir, harness, cmd),
            traced: dir.join(harness.home).is_dir(),
        })
        .collect()
}

/// The files a folder leaves standing instructions for an AI in, whichever one opens it — the second
/// sign that an AI is worked with here (`AMB-D-680`).
///
/// They are kept out of the catalog on purpose. A [`Harness`] row's [`home`](Harness::home) says *which*
/// provider a folder uses, because that is what the request it is handed is written for, and `AGENTS.md`
/// is shared by several vendors: folding it in would leave a notice unable to say whose text to offer.
/// Here the question is only whether anybody is working with an AI in this folder, and for that the file
/// answers without naming anyone.
const INSTRUCTIONS: &[&str] = &["CLAUDE.md", "AGENTS.md"];

/// Whether this folder shows any sign of an AI being worked with in it (`AMB-D-680`): a provider's own
/// directory ([`Wiring::traced`], read off `found` rather than the disk a second time), or standing
/// instructions of the reader's own ([`instructed`]).
///
/// Either is enough — a folder holding `.claude` and nothing written down is worked in, and so is one
/// holding the reverse — so this is the two read together and never a list of who.
///
/// It exists for one judgment: a folder reached over MCP, showing none of this, is one nothing opens a
/// shell in, and a session-start hook there would never fire (`AMB-D-680`). Nothing else asks it, and
/// the answer is deliberately generous — a report withheld is a setup the reader never learns is
/// unfinished, so a sign that is only half a sign still counts.
pub fn ai_in_use(dir: &Path, found: &[Wiring]) -> bool {
    found.iter().any(|one| one.traced) || INSTRUCTIONS.iter().any(|name| instructed(&dir.join(name)))
}

/// Whether an instruction file holds a word the **reader** put there.
///
/// Its being on disk says nothing on its own: binding a folder writes Amenbo's own managed block into
/// both of these files ([`crate::agents::upsert_into_dir`]), and every folder this is asked about is
/// bound — so a rule reading their presence would never once fire. What counts is content outside that
/// block, or a file Amenbo never wrote into at all.
fn instructed(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    match crate::agents::strip_managed(&text) {
        // Nothing of Amenbo's in it, so the whole file is the reader's.
        None => !text.trim().is_empty(),
        Some(theirs) => !theirs.trim().is_empty(),
    }
}

/// The first of a harness's [`places`](Harness::places) whose text carries the wiring, as a path relative
/// to `dir`.
fn wired_at(dir: &Path, harness: &Harness, cmd: &str) -> Option<PathBuf> {
    let call = format!("{cmd} agent");
    harness.places.iter().find_map(|place| {
        settings_files(dir, place).into_iter().find(|relative| {
            std::fs::read_to_string(dir.join(relative))
                .is_ok_and(|text| text.contains(&call) && contains_ignoring_case(&text, harness.event))
        })
    })
}

/// The settings files one `place` stands for, relative to `dir`: the path itself, or — when it is a
/// directory — every `*.json` directly inside it, name-sorted so a probe's answer does not depend on the
/// filesystem's order.
fn settings_files(dir: &Path, place: &str) -> Vec<PathBuf> {
    let path = dir.join(place);
    if !path.is_dir() {
        return vec![PathBuf::from(place)];
    }
    let Ok(entries) = std::fs::read_dir(&path) else { return Vec::new() };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.file_name())
        .filter(|name| Path::new(name).extension().is_some_and(|ext| ext == "json"))
        .map(|name| Path::new(place).join(name))
        .collect();
    found.sort();
    found
}

/// Whether `text` holds `needle`, ignoring case. Only the event token goes through here, and it is ASCII
/// in every provider's vocabulary.
fn contains_ignoring_case(text: &str, needle: &str) -> bool {
    text.to_ascii_lowercase().contains(&needle.to_ascii_lowercase())
}

/// A project's answer to being asked whether Amenbo may have its folder start an AI on `amenbo agent`
/// (`AMB-D-440`) — the row of `harness_consent`, read and written through [`crate::overview`].
///
/// There is no `Unanswered` variant: never having answered is the *absence* of a row (`Option::None`),
/// which is what keeps "asked and refused" apart from "never asked" — the first must never be asked
/// again, the second must. The same shape as the lint's [`crate::hooks::HookConsent`], and for the same
/// reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Consent {
    /// Whether the offer was accepted. A `false` is "don't ask again", and forbids nothing: the text
    /// stays there for the asking.
    pub allowed: bool,
    /// Whether the standing yes has already been put again after its wiring went missing. It is the
    /// memory that makes "once more" once: without it a yes with nothing wired asks at every startup
    /// forever, since answering it changes nothing on disk.
    pub asked_again: bool,
}

impl Consent {
    /// The answer to the question as first put.
    pub fn answered(allowed: bool) -> Consent {
        Consent { allowed, asked_again: false }
    }

    /// The answer to [`ConsentAction::AskAgain`] — the one re-ask, recorded as spent whichever way it
    /// went, so it is not put a third time.
    pub fn answered_again(allowed: bool) -> Consent {
        Consent { allowed, asked_again: true }
    }
}

/// The two facts [`reconcile`] weighs, plus what the surface asking is able to do.
pub struct ConsentContext {
    /// The answer on record for this project, `None` when it has never been asked.
    pub consent: Option<Consent>,
    /// Whether **any** harness is wired in this folder ([`probe`]). One bit, because the question is
    /// about the feature: a second provider appearing later is not a second question.
    pub wired: bool,
    /// Whether this surface can put a question to a person. False for `--json`, an AI and a script,
    /// where a prompt would hang on a terminal nobody is watching.
    pub can_ask: bool,
}

/// What to do about the consent, once the record and the folder have been read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentAction {
    /// Put the question, and record the answer with [`Consent::answered`].
    Ask,
    /// Nothing was ever answered here, but a harness is already wired: the user's own hand is the
    /// answer, so record [`Consent::answered(true)`](Consent::answered) without asking anyone anything.
    AdoptWired,
    /// A standing yes with nothing wired — deleted, or a clone that never carried the settings. Put the
    /// question one more time and record the answer with [`Consent::answered_again`].
    AskAgain,
    /// Say nothing about the consent. It does **not** mean the folder is wired: an unwired provider is
    /// still reported, which is a separate duty from this question.
    Nothing,
}

/// Read the answer and the folder against each other and say what to do — the drift table (`AMB-D-440`),
/// which the tests below walk row by row.
///
/// The rungs, in order:
///
/// 1. **A refusal** is silent from there on, whatever the folder holds.
/// 2. **Never asked, and already wired**: somebody wired this by hand, and that is the answer. Recording
///    it rather than asking is what stops Amenbo putting a question whose answer is on disk in front of
///    it.
/// 3. **Never asked, nothing wired**: the one question — and only where it can be answered.
/// 4. **A standing yes with nothing wired**: asked once more, and only once. The wiring can go for
///    reasons that are not a change of mind (a fresh clone, a settings file a team rewrote), so it is
///    worth one question — and no more than one, because the answer cannot fix it.
///
/// **Once ever, not once per absence.** The re-ask is spent when it is answered and never comes back:
/// this question's failure mode is nagging about a file Amenbo will not write, and the user who wants it
/// again can ask for the text whenever they like.
///
/// A machine caller is never asked (`can_ask`), and nothing is recorded when it is not — the unanswered
/// state carries intact to the next surface that can ask, and what the machine gets instead is the
/// unwired harnesses in its output.
pub fn reconcile(ctx: &ConsentContext) -> ConsentAction {
    match ctx.consent {
        Some(Consent { allowed: false, .. }) => ConsentAction::Nothing,
        None if ctx.wired => ConsentAction::AdoptWired,
        None if ctx.can_ask => ConsentAction::Ask,
        None => ConsentAction::Nothing,
        Some(Consent { asked_again, .. }) if !ctx.wired && !asked_again && ctx.can_ask => {
            ConsentAction::AskAgain
        }
        Some(_) => ConsentAction::Nothing,
    }
}

/// What a folder still has to say about its session-start wiring (`AMB-D-440`) — the standing signal,
/// where [`reconcile`] is a question asked once.
///
/// It is a warning and never a refusal: nothing here is a reason to fail a command, and a folder whose AI
/// is not wired works exactly as it always did — it just reads the instruction only if it reads the
/// managed block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Notice {
    /// The providers this folder shows a trace of that are not wired — the ones to name, in catalog
    /// order. Empty where the folder traces none, which is its own state rather than a shorter list:
    /// there the reader is the one who knows which harness they are, and the catalog is what they pick
    /// from ([`HARNESSES`]).
    ///
    /// A face built for a person shows only what this list can point at: a standing warning about a tool
    /// the folder shows no sign of is one they cannot act on, and it arrives on every command. A face read
    /// by the harness itself has the catalog too — it knows which one it is (`AMB-D-440`).
    pub unwired: Vec<Wiring>,
    /// Whether anything at all is wired here. A folder with one provider wired and another traced and
    /// unwired still has something to say, but it is not the same thing as a folder where nothing starts
    /// its AI on Amenbo at all.
    pub any_wired: bool,
}

/// What to report about `found`, or `None` when there is nothing to report.
///
/// Two things silence it, and neither is "the question was answered":
///
/// - **A refusal** (`allowed: false`). The report exists to finish a setup, and a reader who said no has
///   no setup pending. The text stays there for the asking.
/// - **A folder that is wired, with no traced provider left out.** Something here starts the AI on
///   Amenbo, which is the whole of what this was about.
///
/// A standing yes does **not** silence it: consent is not wiring, and Amenbo writes no settings file, so
/// the only thing that ends this report is the configuration actually landing in the file.
pub fn setup_notice(found: &[Wiring], consent: Option<Consent>) -> Option<Notice> {
    if consent.is_some_and(|answer| !answer.allowed) {
        return None;
    }
    let any_wired = found.iter().any(Wiring::wired);
    let unwired: Vec<Wiring> =
        found.iter().filter(|one| one.traced && !one.wired()).cloned().collect();
    (!unwired.is_empty() || !any_wired).then_some(Notice { unwired, any_wired })
}

/// Whether the setup this folder's session-start wiring is part of is still unfinished **for a reader that
/// names its own harness** (`AMB-D-440`) — the machine face's silence, where [`setup_notice`] is a person's.
///
/// The same folder answers the two differently, because the two are told which provider by different
/// things. A person is told by the folder, so one provider wired with nothing else traced leaves them
/// nothing to act on and [`setup_notice`] goes quiet. The reader here is the harness itself: a provider
/// that left no trace is not absent from this folder, it is the one parsing this. So what ends the report
/// is the whole catalog being wired, and not any row of it — until then some row is unwired, and the
/// reader may be that row.
///
/// The case this is the difference on: a folder wired for Claude Code, read by Codex CLI. Nothing is
/// traced unwired, so a person is told nothing and rightly; the report the harness reads must still stand,
/// or the one AI that could hand a human the text never learns it is unwired.
///
/// A refusal ends it either way, and for [`setup_notice`]'s reason: a reader who said no has no setup
/// pending.
pub fn setup_incomplete(found: &[Wiring], consent: Option<Consent>) -> bool {
    if consent.is_some_and(|answer| !answer.allowed) {
        return false;
    }
    !found.iter().all(Wiring::wired)
}

/// The harnesses to put in front of a reader who has asked for the text, in catalog order: the ones the
/// folder points at, or — where it points at none — the catalog (`AMB-D-440`).
///
/// **A reader who says yes must be handed something.** [`Notice::unwired`] is what Amenbo can *name*, and
/// a folder that traces no provider names none: the offer there is not a shorter list but the whole
/// catalog, out of which the reader picks the tool they know they use. Anything else takes a yes and gives
/// nothing back, which is the state a folder nobody has run an AI in yet is always in.
///
/// **What is offered stays inside the catalog**, even when nothing is traced. Amenbo can only see wiring
/// it knows the shape of ([`probe`]), so a text written for some provider outside it would land somewhere
/// no probe reads — and the report, which only the wiring ends, would go on saying this folder is unwired
/// for ever.
///
/// Whether a face uses this at all is the face's own call: a line printed on every command has to be one
/// the reader can act on, so the CLI's person-facing warning stays with what it can name. A surface that
/// asks once, and can let the reader choose, is where the catalog belongs.
pub fn offered(notice: &Notice) -> Vec<&'static Harness> {
    if notice.unwired.is_empty() {
        return HARNESSES.iter().collect();
    }
    notice.unwired.iter().filter_map(|one| find(one.id)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder with one settings file in it.
    fn folder(tag: &str, place: &str, contents: &str) -> PathBuf {
        let dir = amenbo_scratch::scratch(&format!("harness-{tag}"));
        let file = dir.join(place);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, contents).unwrap();
        dir
    }

    fn wiring<'a>(found: &'a [Wiring], id: &str) -> &'a Wiring {
        found.iter().find(|w| w.id == id).expect("every harness answers")
    }

    /// The catalog's own shape: what a new row has to hold for the table to stay a table.
    #[test]
    fn every_row_is_addressable_configurable_and_probed_where_it_lands() {
        for harness in HARNESSES {
            assert_eq!(find(harness.id).map(|h| h.id), Some(harness.id), "{} is not findable", harness.id);
            assert!(
                harness.template.contains("{instruction}"),
                "{} has nowhere to put the instruction",
                harness.id
            );
            assert!((1..=2).contains(&harness.json_layers), "{} nests the instruction oddly", harness.id);
            // Landing where the probe does not read would leave the user wired and told otherwise. A
            // directory place answers for the file the request names inside it.
            assert!(
                harness.places.iter().any(|place| harness.paste_into.starts_with(place)),
                "{} lands somewhere it is not probed",
                harness.id
            );
            // The trace has to be this provider's own directory, which is what makes its presence say
            // anything: a home some other place sits outside of is a home holding the wrong thing.
            assert!(
                harness.places.iter().all(|place| place.starts_with(harness.home)),
                "{} keeps settings outside the home it is traced by",
                harness.id
            );
        }
        let ids: std::collections::BTreeSet<_> = HARNESSES.iter().map(|h| h.id).collect();
        assert_eq!(ids.len(), HARNESSES.len(), "two rows answer to one id");
    }

    /// The launch catalog's own shape: every row is addressable and names the program it starts. The
    /// mark on an unconfirmed row is part of that shape — a row written from a product's documentation
    /// and never watched opening a pane is worth saying so about, and flipping the mark without running
    /// the tool is exactly the quiet change this catches (`AMB-T-3838`).
    #[test]
    fn every_launch_row_is_addressable_and_says_whether_anyone_has_run_it() {
        for launch in LAUNCHES {
            assert_eq!(
                find_launch(launch.id).map(|one| one.id),
                Some(launch.id),
                "{} is not findable",
                launch.id
            );
            assert!(!launch.command.is_empty(), "{} names no program", launch.id);
        }
        let ids: std::collections::BTreeSet<_> = LAUNCHES.iter().map(|one| one.id).collect();
        assert_eq!(ids.len(), LAUNCHES.len(), "two rows answer to one id");
        assert_eq!(
            LAUNCHES.iter().filter(|one| !one.confirmed).map(|one| one.id).collect::<Vec<_>>(),
            ["cursor"],
            "the rows nobody has watched start",
        );
        for launch in LAUNCHES {
            assert!(
                launch.model_flag.starts_with('-'),
                "{}: {} is not a flag",
                launch.id,
                launch.model_flag
            );
        }
        // The tables part in one direction only. A provider whose hook Amenbo writes is one it tells a
        // folder to run `amenbo agent` in — and a pane unable to open on that provider would leave the
        // wiring pointing at an agent this machine has no way to start.
        for harness in HARNESSES {
            assert!(find_launch(harness.id).is_some(), "{} is wired and not startable", harness.id);
        }
    }

    /// The opening prompt, row by row: the instruction is what is said, it is said last, and a row that
    /// spells a flag puts that in front of it. This is the shape a new row has to hold to as much as the
    /// template is — a row that opened on nothing would start an agent that was never told where it is
    /// working, and nothing on the screen would look wrong.
    ///
    /// It is one argument and not two. A prompt flag takes the prompt after it, so a second argument
    /// would be a second prompt — which is why the pane's two sentences travel as one string.
    #[test]
    fn every_row_opens_by_saying_the_launch_instruction() {
        let said = crate::agents::pane_instruction("amenbo");
        assert!(said.starts_with(&crate::agents::launch_instruction("amenbo")), "{said}");
        for launch in LAUNCHES {
            let args = opening(launch, "amenbo", None, None);
            assert_eq!(
                args.last().map(String::as_str),
                Some(said.as_str()),
                "{} says something else first",
                launch.id
            );
            match launch.prompt_flag {
                None => assert_eq!(args.len(), 1, "{} passes an argument it never declared", launch.id),
                Some(flag) => {
                    assert!(flag.starts_with('-'), "{}: {flag} is not a flag", launch.id);
                    assert_eq!(args, vec![flag.to_string(), said.clone()], "{}", launch.id);
                }
            }
        }
    }

    /// A chosen model rides in front of the prompt, and no choice leaves the line as it was.
    ///
    /// The second half is the one worth a test: `--model ''` is not "however the CLI is set up", it is
    /// a model name every one of these providers refuses, and the name arrives here from a text field
    /// (`AMB-D-865`). Nothing downstream would look wrong — the pane opens, on a program that has
    /// already given up.
    #[test]
    fn a_chosen_model_is_named_in_front_of_the_prompt_and_an_unchosen_one_is_not() {
        let said = crate::agents::pane_instruction("amenbo");
        for launch in LAUNCHES {
            let mut want = vec![launch.model_flag.to_string(), "a-model".to_string()];
            want.extend(launch.prompt_flag.map(str::to_string));
            want.push(said.clone());
            assert_eq!(opening(launch, "amenbo", Some("a-model"), None), want, "{}", launch.id);

            let bare = opening(launch, "amenbo", None, None);
            for empty in [Some(""), Some("   ")] {
                assert_eq!(opening(launch, "amenbo", empty, None), bare, "{}: {empty:?}", launch.id);
            }
            assert!(!bare.contains(&launch.model_flag.to_string()), "{}", launch.id);
        }
    }

    /// Every row that has a way back spells it as a flag, and the row that names its own handle is
    /// the only one with something to ask.
    ///
    /// The pairing is what makes the column readable without a branch: a row Amenbo decides the
    /// handle for has nothing to ask, and a row it does not has no way to be told one.
    #[test]
    fn every_row_with_a_way_back_spells_it_as_a_flag() {
        for launch in LAUNCHES {
            let Some(resume) = launch.resume.as_ref() else { continue };
            assert!(resume.back.starts_with('-'), "{}: {} is not a flag", launch.id, resume.back);
            if let Some(Issue::Flag(flag)) = resume.issue {
                assert!(flag.starts_with('-'), "{}: {flag} is not a flag", launch.id);
            }
            assert_eq!(
                resume.issue.is_none(),
                resume.ask.is_some(),
                "{}: who decides the handle and what is asked disagree",
                launch.id
            );
        }
        // And the two rows with no way back on a line, for the two different reasons: Codex's is a
        // home of its own rather than a handle (`AMB-T-4640`), and Gemini has the flags but restarts
        // itself on the same argv, so a handle on the line kills the second start (`AMB-T-4659`).
        assert!(find_launch("codex-cli").unwrap().resume.is_none());
        assert!(find_launch("gemini-cli").unwrap().resume.is_none());
    }

    /// A handle rides on the same line the pane is opened with, in front of whatever follows it,
    /// and a pane opened on no session puts none there.
    ///
    /// The empty case is the one worth a test: a flag with nothing behind it would be read as the
    /// prompt flag taking the instruction's place, so the pane would come up on a provider that was
    /// never told where it is working.
    #[test]
    fn a_handle_rides_in_front_of_the_prompt_and_no_session_puts_none_there() {
        for launch in LAUNCHES {
            let bare = opening(launch, "amenbo", None, None);
            assert!(!bare.iter().any(|arg| arg == "a-handle"), "{}", launch.id);

            let Some(resume) = launch.resume.as_ref() else {
                // A row with no way back is opened the same way whatever it is handed.
                assert_eq!(opening(launch, "amenbo", None, Some(Handle::Back("a-handle"))), bare);
                continue;
            };
            assert_eq!(
                opening(launch, "amenbo", None, Some(Handle::Back("a-handle"))),
                vec![resume.back.to_string(), "a-handle".to_string()],
                "{}",
                launch.id
            );

            // And a handle Amenbo has just decided goes behind whichever flag this row starts a
            // session under — its own, or the same one it comes back on.
            let started = opening(launch, "amenbo", None, Some(Handle::New("a-handle")));
            match resume.issue {
                Some(Issue::Flag(flag)) => assert_eq!(started[0], flag, "{}", launch.id),
                Some(Issue::Back) => assert_eq!(started[0], resume.back, "{}", launch.id),
                // The row that names its own is started with nothing on the line: the handle it
                // takes is read back afterwards (`crate::agent_sessions`).
                None => assert_eq!(started, bare, "{}", launch.id),
            }
        }
    }

    /// A line that comes back into a conversation says nothing, and a line that starts a session
    /// says where it is working.
    ///
    /// The instruction is a first thing to say. Handed to a conversation already running it is a
    /// turn the person never took, and the agent answers it — reading `agent --json` and naming the
    /// pane again, every time the app comes up (`AMB-T-4663`). The prompt flag goes with it: left
    /// on a line with no prompt behind it, it would take the handle or the model name as one.
    #[test]
    fn a_line_coming_back_says_nothing_and_one_starting_a_session_says_where_it_is_working() {
        let said = crate::agents::pane_instruction("amenbo");
        for launch in LAUNCHES {
            let back = opening(launch, "amenbo", Some("a-model"), Some(Handle::Back("a-handle")));
            let started = opening(launch, "amenbo", Some("a-model"), Some(Handle::New("a-handle")));
            assert_eq!(started.last().map(String::as_str), Some(said.as_str()), "{}", launch.id);
            // The model is named either way (`AMB-D-865`).
            assert_eq!(back[0], launch.model_flag, "{}", launch.id);
            assert_eq!(back[1], "a-model", "{}", launch.id);

            let Some(resume) = launch.resume.as_ref() else {
                // The two rows that take no handle on their line resume nothing on it, so every
                // line they have is a session starting — Codex comes back by the directory it runs
                // in (`app/src-tauri/src/codex_home.rs`) and Gemini does not come back at all
                // (`AMB-T-4659`).
                assert_eq!(back, started, "{}", launch.id);
                continue;
            };
            assert!(!back.contains(&said), "{} says it again on the way back", launch.id);
            if let Some(flag) = launch.prompt_flag {
                assert!(
                    !back.iter().any(|arg| arg == flag),
                    "{} keeps a prompt flag with no prompt",
                    launch.id
                );
            }
            assert_eq!(back[2..], [resume.back.to_string(), "a-handle".to_string()], "{}", launch.id);
        }
    }

    /// A handle is minted for every row Amenbo decides one for, and for no other — and it is a
    /// version 4 UUID, which is the shape the providers were watched taking.
    #[test]
    fn a_handle_is_minted_only_where_amenbo_is_the_one_that_decides_it() {
        for launch in LAUNCHES {
            let decides = launch.resume.as_ref().is_some_and(|resume| resume.issue.is_some());
            let Some(handle) = issue(launch) else {
                assert!(!decides, "{} decides a handle and was given none", launch.id);
                continue;
            };
            assert!(decides, "{} minted a handle it does not decide", launch.id);
            assert_eq!(handle.len(), 36, "{}: {handle}", launch.id);
            let parts: Vec<&str> = handle.split('-').collect();
            assert_eq!(parts.iter().map(|one| one.len()).collect::<Vec<_>>(), [8, 4, 4, 4, 12]);
            assert!(handle.chars().all(|c| c.is_ascii_hexdigit() || c == '-'), "{handle}");
            assert!(parts[2].starts_with('4'), "{handle} is not version 4");
            assert!(matches!(&parts[3][0..1], "8" | "9" | "a" | "b"), "{handle} has no variant");
            // Two panes are two sessions: a handle that repeated would put both on one conversation.
            assert_ne!(issue(launch), Some(handle), "{} minted the same handle twice", launch.id);
        }
    }

    /// Every row says how a running pane is moved, and says it as a slash command.
    ///
    /// The shape is worth holding to because the alternative reads as working: a row that spelled its
    /// command without the slash would put a plain word in an agent's input box, which is a prompt.
    #[test]
    fn every_launch_row_says_how_a_running_pane_is_moved() {
        for launch in LAUNCHES {
            assert!(
                launch.switch.command.starts_with('/'),
                "{}: {} is not a command a provider's input box takes",
                launch.id,
                launch.switch.command
            );
            if let Some(keeps) = launch.switch.keeps {
                assert!(keeps.starts_with('~'), "{}: {keeps} is not a place in the reader's home", launch.id);
            }
        }
    }

    /// The name goes on the line only where the provider takes it there.
    ///
    /// **This is the test the money is on.** Codex and OpenCode read `/model <name>` as a prompt and
    /// were both billed for the answer (`AMB-T-4581`), so a row moved to `Named` by somebody tidying
    /// the table has to fail here rather than on a reader's account.
    #[test]
    fn a_name_reaches_the_line_only_where_the_provider_takes_one() {
        for launch in LAUNCHES {
            let with = switching(launch, Some("a-model"));
            match launch.switch.carries {
                Carries::Named => assert_eq!(
                    with,
                    Switching {
                        line: format!("{} a-model", launch.switch.command),
                        then: None,
                        settles: true,
                    },
                    "{}",
                    launch.id
                ),
                Carries::Picker => assert_eq!(
                    with,
                    Switching {
                        line: launch.switch.command.to_string(),
                        then: None,
                        settles: false,
                    },
                    "{}",
                    launch.id
                ),
                Carries::Filter => assert_eq!(
                    with,
                    Switching {
                        line: launch.switch.command.to_string(),
                        then: Some("a-model".to_string()),
                        settles: false,
                    },
                    "{}",
                    launch.id
                ),
            }
            // Nothing chosen, and a box somebody typed spaces into, are the same thing: the command
            // alone, which opens the provider's own picker and settles nothing.
            let bare = switching(launch, None);
            assert_eq!(bare.line, launch.switch.command, "{}", launch.id);
            assert_eq!(bare.then, None, "{}", launch.id);
            assert!(!bare.settles, "{}", launch.id);
            for empty in [Some(""), Some("   ")] {
                assert_eq!(switching(launch, empty), bare, "{}: {empty:?}", launch.id);
            }
        }
    }

    /// The two the measurement caught billing for a prompt, named on their own.
    ///
    /// The test above holds whatever the table says; this one holds what the table says, so that
    /// moving Codex or OpenCode to `Named` fails here as well as reading wrong.
    #[test]
    fn the_two_that_bill_for_a_named_line_never_get_one() {
        for id in ["codex-cli", "opencode"] {
            let launch = find_launch(id).unwrap();
            assert!(
                !switching(launch, Some("a-model")).line.contains("a-model"),
                "{id} puts the name where the provider reads it as a prompt",
            );
        }
    }

    /// A row that offers a rename offers a command a pane takes, and a bound Amenbo's own names clear.
    ///
    /// The bound is the half worth holding. Amenbo cuts a name to [`crate::frames::NAME_LIMIT`] and
    /// hands it over without measuring it again, so a row added with a shorter bound than that would
    /// be refused on a reader's pane rather than here (`AMB-T-4652`).
    #[test]
    fn a_rename_that_is_offered_takes_a_slash_and_a_name_cut_to_the_label_s_length() {
        for launch in LAUNCHES {
            let Some(rename) = &launch.rename else { continue };
            assert!(
                rename.command.starts_with('/'),
                "{}: {} is not a command a provider's input box takes",
                launch.id,
                rename.command
            );
            if let Some(limit) = rename.limit {
                assert!(
                    limit >= crate::frames::NAME_LIMIT,
                    "{}: a name of {} characters is handed over unmeasured and this row cuts at {limit}",
                    launch.id,
                    crate::frames::NAME_LIMIT
                );
            }
        }
    }

    /// The four that were watched renaming, and the two that have no such command, named on their own.
    ///
    /// The one above holds whatever the table says; this one holds what the table says, so that
    /// giving OpenCode or Gemini a spelling nobody measured fails here rather than sending a sentence
    /// to a model (`AMB-T-4652`).
    #[test]
    fn only_the_providers_measured_taking_a_rename_are_ever_typed_at() {
        for id in ["claude-code", "codex-cli", "github-copilot", "cursor"] {
            let rename = find_launch(id).unwrap().rename.as_ref().expect(id);
            assert_eq!(rename.command, "/rename", "{id}");
        }
        for id in ["opencode", "gemini-cli"] {
            assert!(find_launch(id).unwrap().rename.is_none(), "{id} has no rename to type");
        }
        // The measured differences, each on the row it was measured on: Copilot is the only bound,
        // and Codex the only one whose terminal title stays where it was.
        assert_eq!(find_launch("github-copilot").unwrap().rename.as_ref().unwrap().limit, Some(100));
        assert!(!find_launch("codex-cli").unwrap().rename.as_ref().unwrap().titles);
        for id in ["claude-code", "codex-cli", "cursor"] {
            assert_eq!(find_launch(id).unwrap().rename.as_ref().unwrap().limit, None, "{id}");
        }
        for id in ["claude-code", "github-copilot", "cursor"] {
            assert!(find_launch(id).unwrap().rename.as_ref().unwrap().titles, "{id}");
        }
    }

    /// The opening prompt points the agent at the binary the user is running, for the reason the
    /// configuration does: a dev-channel window telling an agent to run `amenbo` names a command the
    /// reader may not have.
    #[test]
    fn the_opening_prompt_names_the_running_command() {
        let said = opening(find_launch("claude-code").unwrap(), "amenbo-dev", None, None).pop().unwrap();
        assert!(said.contains("amenbo-dev agent --json"), "{said}");
        assert!(!said.contains("`amenbo agent"), "{said}");
        // Both canons the pane names, and the second one for the same reason as the first.
        assert!(said.contains("amenbo-dev talk --json"), "{said}");
        assert!(!said.contains("`amenbo talk"), "{said}");
    }

    /// The two signs that an AI is worked with in a folder, each enough on its own (`AMB-D-680`) — and a
    /// folder holding neither, which is the one a session-start hook would never fire in.
    #[test]
    fn a_folder_is_in_use_where_it_traces_a_provider_or_instructs_an_ai() {
        let dir = folder("in-use", "notes.txt", "nothing here is addressed to an AI");
        assert!(!ai_in_use(&dir, &probe(&dir, "amenbo")), "a folder with only a reader's own files");

        // What binding wrote, and nothing else. It is on disk in every folder this is ever asked about,
        // so counting it would leave the rule unable to fire at all.
        let block = crate::agents::upsert_managed(None, &crate::agents::managed_block_body("English", "amenbo"));
        std::fs::write(dir.join("AGENTS.md"), &block).unwrap();
        assert!(!ai_in_use(&dir, &probe(&dir, "amenbo")), "Amenbo's own block is not a sign of anyone");

        // A word of the reader's beside it is. `AGENTS.md` is the case the catalog cannot carry —
        // several vendors read it — and the case this has to answer.
        std::fs::write(dir.join("AGENTS.md"), format!("# how to work here\n\n{block}")).unwrap();
        assert!(ai_in_use(&dir, &probe(&dir, "amenbo")), "instructions are a sign, whoever reads them");

        // And the trace on its own, once they are gone.
        std::fs::remove_file(dir.join("AGENTS.md")).unwrap();
        assert!(!ai_in_use(&dir, &probe(&dir, "amenbo")), "back to nothing");
        std::fs::create_dir_all(dir.join(find("codex-cli").unwrap().home)).unwrap();
        assert!(ai_in_use(&dir, &probe(&dir, "amenbo")), "a provider's own directory is the other");
    }

    /// The first string in `value` that runs a command, whatever key its provider hangs it on.
    fn echoed_command(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::String(text) => text.starts_with("echo ").then(|| text.clone()),
            serde_json::Value::Array(items) => items.iter().find_map(echoed_command),
            serde_json::Value::Object(fields) => fields.values().find_map(echoed_command),
            _ => None,
        }
    }

    /// The whole configuration, all the way down: the file parses, the command it holds runs a
    /// single-quoted `echo`, and what that echo prints is the instruction — as text where the provider
    /// takes text, and as a JSON document carrying it where the provider takes one. This is the one test
    /// that would catch a template escaped one layer too few, which parses as a file and prints nonsense.
    #[test]
    fn every_configuration_is_json_whose_echo_prints_the_instruction() {
        let instruction = crate::agents::launch_instruction("amenbo");
        for harness in HARNESSES {
            let text = configuration(harness, "amenbo");
            assert!(!text.contains("{instruction}"), "{} left the placeholder in", harness.id);
            let parsed: serde_json::Value =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}\n{text}", harness.id));

            let command = echoed_command(&parsed).unwrap_or_else(|| panic!("{} echoes nothing", harness.id));
            let printed = command
                .strip_prefix("echo '")
                .and_then(|rest| rest.strip_suffix('\''))
                .unwrap_or_else(|| panic!("{}: not a single-quoted echo: {command}", harness.id));
            assert!(!printed.contains('\''), "{}: the echo would end early", harness.id);

            if harness.json_layers == 1 {
                assert_eq!(printed, instruction, "{}", harness.id);
                continue;
            }
            let payload: serde_json::Value = serde_json::from_str(printed)
                .unwrap_or_else(|e| panic!("{} prints no document: {e}\n{printed}", harness.id));
            assert!(
                payload.to_string().contains(&instruction),
                "{} prints a document without the instruction: {payload}",
                harness.id
            );
        }
    }

    /// The configuration calls the binary the user is running, not the product's name.
    #[test]
    fn the_configuration_names_the_running_command() {
        let text = configuration(find("claude-code").unwrap(), "amenbo-dev");
        assert!(text.contains("amenbo-dev agent --json"), "{text}");
    }

    /// What is handed over is a request an AI can carry out, and the two things it must not leave the
    /// reader to work out are the file and the merge: a request that reads as a whole settings document
    /// is exactly what this replaced, and it fails the reader whose file is not empty.
    #[test]
    fn every_request_carries_its_configuration_and_asks_for_a_merge_into_a_named_file() {
        for harness in HARNESSES {
            let text = request(harness, "amenbo");
            assert!(
                text.contains(&configuration(harness, "amenbo")),
                "{} hands over no configuration: {text}",
                harness.id
            );
            assert!(text.contains(harness.paste_into), "{} names no file: {text}", harness.id);
            assert!(text.contains(harness.label), "{} names no tool: {text}", harness.id);
            // The one sentence the old shape could not say. It is asserted on the words a reader would
            // look for, because a request that carries the settings and not this is the failure being
            // guarded against, not a wording preference.
            assert!(text.contains("Merge"), "{} does not ask for a merge: {text}", harness.id);
            assert!(
                text.contains("Keep everything that file already holds"),
                "{} does not say the existing settings stay: {text}",
                harness.id
            );
            // A request is prose and not a settings file, which is the whole change: pasted into one it
            // would not parse, and nothing here should read as though it could be.
            assert!(
                serde_json::from_str::<serde_json::Value>(&text).is_err(),
                "{} hands over something that reads as a settings file",
                harness.id
            );
        }
    }

    /// Both halves are the wiring: the call, and the event it is wired to.
    #[test]
    fn a_call_and_a_session_start_event_in_one_file_is_the_wiring() {
        let dir = folder(
            "wired",
            ".claude/settings.json",
            &configuration(find("claude-code").unwrap(), "amenbo"),
        );
        let found = probe(&dir, "amenbo");
        assert_eq!(
            wiring(&found, "claude-code").wired_at,
            Some(PathBuf::from(".claude/settings.json"))
        );
        // The other four are not wired by Claude Code's file.
        assert!(found.iter().filter(|w| w.wired()).count() == 1, "{found:?}");
    }

    #[test]
    fn half_the_wiring_is_not_the_wiring() {
        // The event, with something else wired to it.
        let dir = folder("event-only", ".claude/settings.json", r#"{"hooks":{"SessionStart":[]}}"#);
        assert!(!wiring(&probe(&dir, "amenbo"), "claude-code").wired());

        // The call, from a hook that is not session start.
        let dir = folder("call-only", ".claude/settings.json", r#"{"hooks":{"PreToolUse":["amenbo agent"]}}"#);
        assert!(!wiring(&probe(&dir, "amenbo"), "claude-code").wired());
    }

    /// A folder nobody has wired answers for every harness, and answers no.
    #[test]
    fn an_untouched_folder_is_unwired_everywhere() {
        let dir = amenbo_scratch::scratch("harness-bare");
        let found = probe(&dir, "amenbo");
        assert_eq!(found.len(), HARNESSES.len());
        assert!(found.iter().all(|w| !w.wired()), "{found:?}");
    }

    /// A dev-channel wiring is not the production binary's, and the other way round.
    #[test]
    fn the_launch_command_has_to_match() {
        let dir =
            folder("channel", ".claude/settings.json", &configuration(find("claude-code").unwrap(), "amenbo-dev"));
        assert!(wiring(&probe(&dir, "amenbo-dev"), "claude-code").wired());
        assert!(!wiring(&probe(&dir, "amenbo"), "claude-code").wired());
    }

    /// A hooks directory answers for whatever the user named the file in it.
    #[test]
    fn a_directory_place_reads_every_json_the_user_put_there() {
        let dir = folder(
            "copilot",
            ".github/hooks/whatever-they-called-it.json",
            &configuration(find("github-copilot").unwrap(), "amenbo"),
        );
        assert_eq!(
            wiring(&probe(&dir, "amenbo"), "github-copilot").wired_at,
            Some(PathBuf::from(".github/hooks/whatever-they-called-it.json"))
        );
    }

    /// A provider reading its wiring from either of two files is wired by either.
    #[test]
    fn a_second_place_is_read_when_the_first_holds_nothing() {
        let toml = "[[hooks.SessionStart]]\nmatcher = \"*\"\n\n[[hooks.SessionStart.hooks]]\ntype = \"command\"\ncommand = 'echo amenbo agent'\n";
        let dir = folder("codex", ".codex/config.toml", toml);
        assert_eq!(
            wiring(&probe(&dir, "amenbo"), "codex-cli").wired_at,
            Some(PathBuf::from(".codex/config.toml"))
        );
    }

    /// The drift table, walked row by row (`AMB-D-440`). The record decides whether to ask; the folder
    /// is read every time; neither stands in for the other.
    #[test]
    fn the_record_and_the_folder_meet_row_by_row() {
        let ask = |consent, wired| reconcile(&ConsentContext { consent, wired, can_ask: true });

        // Never asked, nothing wired: the one question.
        assert_eq!(ask(None, false), ConsentAction::Ask);
        // Never asked, but wired by hand: the disk is the answer, so take it and stay quiet.
        assert_eq!(ask(None, true), ConsentAction::AdoptWired);
        // A yes that is wired has nothing left to say.
        assert_eq!(ask(Some(Consent::answered(true)), true), ConsentAction::Nothing);
        // A yes whose wiring went missing: once more.
        assert_eq!(ask(Some(Consent::answered(true)), false), ConsentAction::AskAgain);
        // And once is once.
        assert_eq!(ask(Some(Consent::answered_again(true)), false), ConsentAction::Nothing);
        // A refusal is silent either way, and a spent re-ask does not revive it.
        for consent in [Consent::answered(false), Consent::answered_again(false)] {
            for wired in [true, false] {
                assert_eq!(ask(Some(consent), wired), ConsentAction::Nothing, "{consent:?} {wired}");
            }
        }
    }

    /// A surface that cannot put a question asks none — and records nothing, so the unanswered state
    /// reaches the next surface that can. Adopting a wiring already on disk is not asking, so it still
    /// happens.
    #[test]
    fn a_machine_caller_is_never_asked() {
        let machine = |consent, wired| reconcile(&ConsentContext { consent, wired, can_ask: false });

        assert_eq!(machine(None, false), ConsentAction::Nothing);
        assert_eq!(machine(Some(Consent::answered(true)), false), ConsentAction::Nothing);
        assert_eq!(machine(None, true), ConsentAction::AdoptWired);
    }

    /// The event token is matched however the provider spells it.
    #[test]
    fn the_event_is_matched_whatever_its_casing() {
        let dir = folder("casing", ".cursor/hooks.json", r#"{"hooks":{"SESSIONSTART":[{"command":"amenbo agent --json"}]}}"#);
        assert!(wiring(&probe(&dir, "amenbo"), "cursor").wired());
    }

    /// A trace is the provider's **own** directory. `.github` is in nearly every repository and says
    /// nothing about whether this provider is used here, which is why the row carries `.github/hooks`
    /// instead — get this wrong and every repository on earth is told to wire a tool it does not have.
    #[test]
    fn a_trace_is_the_providers_own_directory_and_not_a_shared_one() {
        let dir = amenbo_scratch::scratch("harness-trace");
        std::fs::create_dir_all(dir.join(".github/workflows")).unwrap();
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        let found = probe(&dir, "amenbo");

        assert!(wiring(&found, "claude-code").traced, "a folder with .claude uses Claude Code");
        assert!(!wiring(&found, "github-copilot").traced, "a .github is not this provider's hooks");
        assert!(!wiring(&found, "cursor").traced);
        // Traced is not wired: the settings file is not even there.
        assert!(!wiring(&found, "claude-code").wired());
    }

    /// What the standing report says, state by state. It is the wiring that ends it, never the answer:
    /// Amenbo writes no settings file, so a yes leaves the setup exactly as unfinished as it found it.
    #[test]
    fn the_report_ends_when_the_wiring_lands_or_the_reader_says_no() {
        let bare = |id: &'static str, traced| Wiring { id, label: id, wired_at: None, traced };
        let wired = |id: &'static str| Wiring {
            id,
            label: id,
            wired_at: Some(PathBuf::from("somewhere")),
            traced: true,
        };

        // Nothing traced and nothing wired: still worth saying, since the reader knows their own harness
        // even where the folder shows none.
        let quiet_folder = [bare("claude-code", false), bare("cursor", false)];
        let notice = setup_notice(&quiet_folder, None).expect("a folder wired to nothing has something to say");
        assert!(notice.unwired.is_empty(), "there is nothing to name: {notice:?}");
        assert!(!notice.any_wired);

        // A traced provider that is not wired is named.
        let traced = [bare("claude-code", true), bare("cursor", false)];
        let notice = setup_notice(&traced, None).expect("a traced provider unwired is the case for saying so");
        assert_eq!(notice.unwired.iter().map(|w| w.id).collect::<Vec<_>>(), ["claude-code"]);

        // Wired, with nothing else traced: nothing left to finish.
        assert_eq!(setup_notice(&[wired("claude-code"), bare("cursor", false)], None), None);

        // Wired, and another provider traced and unwired: that one is still named.
        let mixed = [wired("claude-code"), bare("cursor", true)];
        let notice = setup_notice(&mixed, Some(Consent::answered(true))).expect("the other one is unwired");
        assert_eq!(notice.unwired.iter().map(|w| w.id).collect::<Vec<_>>(), ["cursor"]);
        assert!(notice.any_wired, "one wired provider is not none");

        // A standing yes does not end it — only the wiring landing does.
        assert!(setup_notice(&traced, Some(Consent::answered(true))).is_some());
        assert!(setup_notice(&quiet_folder, Some(Consent::answered_again(true))).is_some());

        // A refusal ends it, whatever the folder holds.
        for consent in [Consent::answered(false), Consent::answered_again(false)] {
            assert_eq!(setup_notice(&traced, Some(consent)), None, "{consent:?}");
            assert_eq!(setup_notice(&quiet_folder, Some(consent)), None, "{consent:?}");
        }
    }

    /// What the machine face's report answers to, against the person's. The case it exists for is the
    /// folder wired for one tool and read by another: a person has nothing left to act on there, and the
    /// harness reading it is unwired and learns it nowhere else.
    #[test]
    fn the_machine_face_stands_until_the_whole_catalog_is_wired() {
        let bare = |id: &'static str, traced| Wiring { id, label: id, wired_at: None, traced };
        let wired = |id: &'static str| Wiring {
            id,
            label: id,
            wired_at: Some(PathBuf::from("somewhere")),
            traced: true,
        };

        // One wired, nothing else traced: the person's face is done, this one is not.
        let one_of_two = [wired("claude-code"), bare("codex-cli", false)];
        assert_eq!(setup_notice(&one_of_two, Some(Consent::answered(true))), None);
        assert!(setup_incomplete(&one_of_two, Some(Consent::answered(true))));

        // Every row wired: no reader of this can be the unwired one, so there is nothing left to say.
        assert!(!setup_incomplete(&[wired("claude-code"), wired("codex-cli")], Some(Consent::answered(true))));

        // A refusal ends it here too, whatever the folder holds.
        for consent in [Consent::answered(false), Consent::answered_again(false)] {
            assert!(!setup_incomplete(&one_of_two, Some(consent)), "{consent:?}");
        }
        // An unanswered question does not: consent is not wiring.
        assert!(setup_incomplete(&one_of_two, None));
    }

    /// What a reader who asked for the text is handed. The case this exists for is the folder that traces
    /// nothing — a new one, where nobody has run an AI yet — which named no provider and so, on a surface
    /// that showed only named ones, handed over nothing at all.
    #[test]
    fn a_folder_that_names_no_provider_is_offered_the_catalog() {
        let bare = |id: &'static str, traced| Wiring { id, label: id, wired_at: None, traced };

        let quiet_folder = [bare("claude-code", false), bare("cursor", false)];
        let notice = setup_notice(&quiet_folder, None).unwrap();
        assert_eq!(
            offered(&notice).iter().map(|h| h.id).collect::<Vec<_>>(),
            HARNESSES.iter().map(|h| h.id).collect::<Vec<_>>(),
            "a folder pointing at nothing is offered everything, in catalog order",
        );

        // Where the folder does point somewhere, that is the offer: the reader is not asked to pick out of
        // five when Amenbo can already see which one they use.
        let traced = [bare("claude-code", true), bare("cursor", false)];
        let notice = setup_notice(&traced, None).unwrap();
        assert_eq!(offered(&notice).iter().map(|h| h.id).collect::<Vec<_>>(), ["claude-code"]);

        // Everything offered is a row of the catalog — Amenbo only sees wiring it knows the shape of, so a
        // text for anything else would land where no probe reads and the report would never end.
        for harness in offered(&notice) {
            assert!(find(harness.id).is_some(), "{} is offered and not listed", harness.id);
        }
    }
}
