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

use std::path::{Path, PathBuf};

use serde::Serialize;

/// One AI harness Amenbo knows how to be wired into — the catalog row (`AMB-D-440`).
///
/// The fields split in two: [`places`](Harness::places) and [`event`](Harness::event) are what a probe
/// reads, and [`paste_into`](Harness::paste_into) with [`template`](Harness::template) are what a
/// [`request`] is built from. Nothing here is a schema — the config shapes have nothing in common (JSON
/// depth, event casing, which key holds the command), which is why the whole of each one is carried as
/// text.
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

/// Every harness Amenbo lists, in the order a face offers them. The five settings-only providers of the
/// first catalog (`AMB-D-440`).
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
