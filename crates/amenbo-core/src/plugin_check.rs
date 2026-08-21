//! **The check the settings face raises** (`AMB-D-664`) — the author's own code, run to say whether the
//! values it has been given are usable, and read fail-closed.
//!
//! `required` asks whether a field holds *something* ([`plugin_trust`](crate::plugin_trust)); nobody but
//! the author's code can say whether what it holds is a webhook that exists, a password that goes with its
//! user, or a pair of fields that contradict each other. So a manifest may name one call for it
//! ([`Settings::check`](crate::plugin_manifest::Settings::check)) and Amenbo raises it at the two moments
//! the answer can still be acted on:
//!
//! - **when the plugin is enabled** — a verdict that is not a yes leaves the gate shut
//!   ([`plugin_trust::enable`](crate::plugin_trust::enable));
//! - **after a save while it is enabled** — the same call, read the same way, except that nothing is
//!   undone by it: a save is never stopped and an enabled plugin is never switched off behind the user's
//!   back (`AMB-D-664`). What it is for there is the sentence on the screen.
//!
//! **It runs before the gate, and it is the one call that does.** Running somebody else's code is what
//! enabling *means* (`AMB-D-351`), so a check raised by an enable has the consent of the hand that pressed
//! it — which is why it is assembled by [`prepare_check`](crate::plugin_invoke::prepare_check) rather than
//! by the settings face's ordinary road ([`prepare_declared`](crate::plugin_invoke::prepare_declared)),
//! which raises what a user presses and holds the gate the way every other call does. Everything else about
//! the run is that road's: the same command face (`AMB-D-353`), the same injected settings (`AMB-D-356`),
//! the same read-back path (`AMB-D-406`), the same execution log (`AMB-D-361`).
//!
//! **Fail-closed** (`AMB-D-354`). The verdict is a document ([`Verdict`]), and a run that does not produce
//! one — will not start, exits non-zero, overruns [`TIMEOUT`], or writes something this build cannot read
//! — has not checked anything. It is [`Silence`], and a silence never opens a gate. The alternative would
//! be to enable on the strength of a plugin that crashed, which is the one reading the check exists to
//! prevent.
//!
//! **The author's sentences stop here and at the screen** (`AMB-D-664`). A verdict carries the author's
//! `message` and its per-field lines, and those are the GUI settings form's alone. Nothing on a face an AI
//! reads takes them: the refusal an enable raises names the *keys* the check spoke about and no more, and
//! `agent --json` has never carried a plugin's settings at all. That is why the two travel separately — the
//! verdict to whoever draws it, the refusal to whoever is told no.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::Value;

use crate::error::Result;
use crate::plugin_exec::PluginOutput;
use crate::plugin_log::{self, Outcome, Run};
use crate::plugin_manifest::ConfigField;
use crate::plugin_payload::VERSION;
use crate::plugin_subscribe::InstalledPlugin;
use crate::store::Store;

/// How long a check may run before it is given up on and killed (`AMB-D-664`). The same bound a
/// `reply:true` hook is held to ([`REPLY_TIMEOUT`](crate::plugin_hooks::REPLY_TIMEOUT)), and for the same
/// reason: somebody is waiting in front of a screen for the answer, so a wedged check must cost a refusal
/// rather than a frozen window. Overrunning it is a [`Silence::TimedOut`] — the run said nothing, which is
/// not the same as saying no.
pub const TIMEOUT: Duration = Duration::from_secs(2);

/// The most a sentence in a verdict may be, in bytes — the author's `message`, and each of the lines it
/// puts beside a field (`AMB-D-664`). One line under a text box, and the cap says so; a check with a
/// paragraph to deliver has the execution log for it (`AMB-D-361`).
pub const MAX_VERDICT_TEXT_BYTES: usize = 200;

/// What came back when the settings face raised the author's check — the whole of what a gate is judged on
/// (`AMB-D-664`).
///
/// Three states, not two, because "the values are wrong" and "nobody said anything" are different facts and
/// only one of them has a sentence to show. They cost the same at the gate ([`Self::opens_the_gate`]) and
/// read differently on the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Checked {
    /// The manifest names no check. Nothing was run, and the gate is the presence check it always was
    /// (`required`, [`plugin_trust::missing_required`](crate::plugin_trust::missing_required)) — which is
    /// every plugin written before this block existed.
    NotDeclared,
    /// The author's code ran and answered. Whether the answer is a yes is [`Verdict::ok`]'s.
    Answered(Verdict),
    /// The author's code was raised and said nothing this build can act on. Fail-closed (`AMB-D-354`).
    Silent(Silence),
}

impl Checked {
    /// Whether a gate may open on this (`AMB-D-664`) — the one reading [`enable`](crate::plugin_trust::enable)
    /// takes.
    ///
    /// A declared check that did not answer *yes* holds the gate shut, whichever way it failed to. Only two
    /// things open it: a verdict saying so, and a plugin that declared no check to begin with.
    pub fn opens_the_gate(&self) -> bool {
        match self {
            Checked::NotDeclared => true,
            Checked::Answered(verdict) => verdict.ok,
            Checked::Silent(_) => false,
        }
    }

    /// The verdict, when the check answered — what the settings form draws. `None` covers both silences:
    /// the check that was never declared, and the one that said nothing readable.
    pub fn verdict(&self) -> Option<&Verdict> {
        match self {
            Checked::Answered(verdict) => Some(verdict),
            _ => None,
        }
    }

    /// Why nothing was said, when nothing was. `None` for a check that answered, and for one no manifest
    /// asked for.
    pub fn silence(&self) -> Option<Silence> {
        match self {
            Checked::Silent(silence) => Some(*silence),
            _ => None,
        }
    }
}

/// What the author's check said about the values it was handed (`AMB-D-664`) — the document a run writes on
/// stdout, read into the three things Amenbo does anything with.
///
/// ```json
/// { "v": 1, "ok": false, "fields": { "smtp_password": "there is a space in it" }, "message": "…" }
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Verdict {
    /// Whether the values are usable. The gate turns on this alone: the sentences below it are for the
    /// reader, and Amenbo does not read them (`AMB-D-356` — judging a value is the author's).
    pub ok: bool,
    /// One sentence about the settings as a whole, for the head of the form. Absent when the check wrote
    /// none, and an empty one is none.
    pub message: Option<String>,
    /// One sentence per setting the check spoke about, keyed by the setting's own key.
    ///
    /// **Only keys the manifest declares survive** (`AMB-D-664`): a check naming something the form has no
    /// box for is talking about a field that cannot be drawn, so the line is dropped and the rest of the
    /// verdict stands. Ordered by key, since the form draws it beside its box rather than in the order the
    /// check happened to write.
    pub fields: BTreeMap<String, String>,
    /// What the check asked to have drawn on the form (`AMB-D-727`), in the order it wrote them — the
    /// same vocabulary an operation answers with ([`plugin_show`](crate::plugin_show)).
    ///
    /// A check is the one run that happens before anybody has filled anything in, which is where a way
    /// to the page that issues the token is worth the most; empty is every check written before the
    /// vocabulary existed.
    pub show: Vec<crate::plugin_show::Part>,
}

impl Verdict {
    /// The declared settings this verdict spoke about, by key alone.
    ///
    /// This is the whole of what a refusal may repeat on a face an AI reads (`AMB-D-664`): a key is the
    /// form's own word for a box, while the sentence beside it is the author's text and stays on the screen
    /// that shows the form.
    pub fn field_keys(&self) -> Vec<&str> {
        self.fields.keys().map(String::as_str).collect()
    }
}

/// Why a raised check said nothing this build can act on (`AMB-D-354`) — Amenbo's own reading of the run,
/// with no word of the plugin's in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Silence {
    /// The program would not start.
    NotLaunched,
    /// It ran and exited non-zero (or died on a signal). Under the command contract a failed run's stdout
    /// is not consumed, so whatever it wrote is not a verdict (`AMB-D-354`).
    Failed,
    /// It overran [`TIMEOUT`] and was killed.
    TimedOut,
    /// It exited cleanly and wrote something that is not a verdict this build can read — not JSON, not this
    /// payload version, no `ok`, or a sentence past [`MAX_VERDICT_TEXT_BYTES`] or carrying control
    /// characters.
    Unreadable,
}

impl Silence {
    /// The stable word for it — what a face puts in its own sentence, and what the reason reads as in a log
    /// line's stderr when Amenbo is the one who wrote it.
    pub fn as_str(self) -> &'static str {
        match self {
            Silence::NotLaunched => "it would not start",
            Silence::Failed => "it exited without success",
            Silence::TimedOut => "it overran the two-second bound and was killed",
            Silence::Unreadable => "its answer could not be read",
        }
    }
}

/// Raise the check this plugin declares, and read the verdict (`AMB-D-664`).
///
/// `project` is the project the face is standing in, as every other run of this plugin is assembled from
/// (the layer its gate and settings sit at follows the author's declaration, `AMB-D-601`). A plugin that
/// declares no check is [`Checked::NotDeclared`] and nothing is spawned.
///
/// `bound` is how long the check may take: [`TIMEOUT`] is the rule (`AMB-D-664`) and what every face hands
/// in. It is an argument for the reason [`run_reply`](crate::plugin_hooks::run_reply)'s is — the bound
/// belongs to whoever is waiting on the answer — and it is what lets a test drive both sides of it without
/// spending the wall clock to do so.
///
/// The run is on the execution log whichever way it ends (`AMB-D-361`), filed with the rest of what the
/// settings face raises ([`SETTINGS_LOG_EVENT`](crate::plugin_invoke::SETTINGS_LOG_EVENT)): *why was I not
/// allowed to enable this* is exactly the question that log answers.
///
/// `Err` is for the assembly failing — a plugin whose settings cannot be resolved, or a project-scoped
/// plugin asked for outside a project. A plugin that ran and failed is not an error of ours: it is a
/// [`Silence`], which is the fail-closed answer the gate wants.
pub fn run(
    store: &Store,
    plugin: &InstalledPlugin,
    project: Option<i64>,
    bound: Duration,
) -> Result<Checked> {
    let Some(cmd) = plugin.manifest.settings.as_ref().and_then(|s| s.check.as_deref()) else {
        return Ok(Checked::NotDeclared);
    };
    let invocation =
        crate::plugin_invoke::prepare_check(store, &plugin.name, cmd, project)?;

    let waited = invocation.spawn().and_then(|running| running.wait_timeout(bound));
    let (checked, recorded) =
        read(&plugin.name, &plugin.manifest.fields(), plugin.manifest.official, waited, bound);
    plugin_log::record(&store.paths.plugin_log_file(), &recorded);
    Ok(checked)
}

/// Read one finished check run: the verdict it is, and the log line it leaves. Split from [`run`] so both
/// halves are decided in one place off a captured run, with nothing spawned. `bound` is what a killed run
/// records as its duration — it ran exactly as long as it was allowed to.
fn read(
    plugin: &str,
    declared: &[ConfigField],
    official: bool,
    waited: std::io::Result<Option<PluginOutput>>,
    bound: Duration,
) -> (Checked, Run) {
    let line = |outcome, code, elapsed, stderr: &str| Run {
        plugin: plugin.to_string(),
        event: crate::plugin_invoke::SETTINGS_LOG_EVENT,
        outcome,
        code,
        elapsed,
        stderr: stderr.to_string(),
    };
    match waited {
        // The launch failure is Amenbo's own sentence: there was no child to write one.
        Err(error) => (
            Checked::Silent(Silence::NotLaunched),
            line(Outcome::NotLaunched, None, Duration::ZERO, &error.to_string()),
        ),
        // Killed at the bound, so there is no code and no stderr — the child was reaped, not read.
        Ok(None) => {
            (Checked::Silent(Silence::TimedOut), line(Outcome::TimedOut, None, bound, ""))
        }
        Ok(Some(output)) if !output.succeeded() => {
            let recorded =
                line(Outcome::Failed, output.code, output.elapsed, &output.stderr);
            (Checked::Silent(Silence::Failed), recorded)
        }
        Ok(Some(output)) => {
            let recorded = line(Outcome::Ok, output.code, output.elapsed, &output.stderr);
            match verdict(&output.stdout, declared, official) {
                Some(verdict) => (Checked::Answered(verdict), recorded),
                None => {
                    // A clean exit is what the log records — the run itself was fine — so the reason the
                    // gate stayed shut is said here, where the return value is the thing at fault.
                    tracing::warn!(
                        plugin = %plugin,
                        "a plugin's settings check exited cleanly and wrote no verdict this build can read"
                    );
                    (Checked::Silent(Silence::Unreadable), recorded)
                }
            }
        }
    }
}

/// Read a check's stdout as a verdict, or `None` for one this build cannot act on (`AMB-D-354`).
///
/// Strict where the shape is Amenbo's business and forgiving where it is the author's: the version marker,
/// `ok`, and the type of every value are held to the contract, while a line about a setting the manifest
/// does not declare is dropped and the rest of the verdict stands. A sentence past the cap or carrying
/// control characters is not dropped but refused whole — the same floor a stored value is held to
/// ([`plugin_config::check_value`](crate::plugin_config::check_value)) — because a verdict Amenbo has
/// edited is no longer the author's answer, and this one is about to be put in front of a person. The
/// parts it asks to have drawn (`AMB-D-727`) are read the same way, by
/// [`plugin_show::read`](crate::plugin_show::read): a third party's `qr` is dropped and the rest stands,
/// while a shape this build cannot draw takes the verdict with it. `official` is the catalog's badge off
/// the installed manifest, which is what settles that (`AMB-D-347`).
fn verdict(stdout: &str, declared: &[ConfigField], official: bool) -> Option<Verdict> {
    let document: Value = serde_json::from_str(stdout.trim()).ok()?;
    let document = document.as_object()?;
    if document.get("v").and_then(Value::as_u64) != Some(u64::from(VERSION)) {
        return None;
    }
    let ok = document.get("ok")?.as_bool()?;

    let message = match document.get("message") {
        None | Some(Value::Null) => None,
        Some(said) => match sentence(said)? {
            // Nothing said is nothing to draw: an empty line under a form is a box with no words in it.
            said if said.is_empty() => None,
            said => Some(said),
        },
    };

    let mut fields = BTreeMap::new();
    match document.get("fields") {
        None | Some(Value::Null) => {}
        Some(said) => {
            for (key, value) in said.as_object()? {
                if !declared.iter().any(|f| &f.key == key) {
                    continue;
                }
                fields.insert(key.clone(), sentence(value)?);
            }
        }
    }
    // Fail-closed on the parts as well (`AMB-D-354`): an answer this build cannot draw is one it cannot
    // act on, and a verdict half-drawn in front of somebody is worse than the silence.
    let show = crate::plugin_show::read(document.get("show"), official)?;
    Some(Verdict { ok, message, fields, show })
}

/// One sentence out of a verdict, held to the floor Amenbo puts under every author string it shows
/// (`AMB-D-664`): a string, within [`MAX_VERDICT_TEXT_BYTES`], with no control characters in it. `None` is
/// what makes the whole verdict unreadable.
fn sentence(said: &Value) -> Option<String> {
    let said = said.as_str()?;
    if said.len() > MAX_VERDICT_TEXT_BYTES || said.chars().any(char::is_control) {
        return None;
    }
    Some(said.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(keys: &[&str]) -> Vec<ConfigField> {
        keys.iter().map(|k| ConfigField::new(*k, *k)).collect()
    }

    fn read_verdict(stdout: &str) -> Option<Verdict> {
        verdict(stdout, &declared(&["smtp_user", "smtp_password"]), true)
    }

    // ───────────────────────── what a verdict says (`AMB-D-664`) ─────────────────────────

    #[test]
    fn a_yes_is_the_one_answer_that_opens_the_gate() {
        assert!(Checked::NotDeclared.opens_the_gate(), "a plugin that declares none is unchanged");
        assert!(Checked::Answered(Verdict { ok: true, ..Verdict::default() }).opens_the_gate());
        assert!(!Checked::Answered(Verdict::default()).opens_the_gate(), "ok: false is a no");
        for silence in
            [Silence::NotLaunched, Silence::Failed, Silence::TimedOut, Silence::Unreadable]
        {
            assert!(
                !Checked::Silent(silence).opens_the_gate(),
                "a check that said nothing has checked nothing: {silence:?}"
            );
        }
    }

    #[test]
    fn a_verdict_is_read_whole() {
        let read = read_verdict(
            r#"{"v":1,"ok":false,"fields":{"smtp_password":"there is a space in it"},"message":"cannot sign in"}"#,
        )
        .expect("the document is a verdict");
        assert!(!read.ok);
        assert_eq!(read.message.as_deref(), Some("cannot sign in"));
        assert_eq!(read.fields["smtp_password"], "there is a space in it");
        assert_eq!(read.field_keys(), vec!["smtp_password"], "the keys are what a refusal may name");
    }

    /// A verdict is `ok` and nothing else at its thinnest — the passing check most authors write.
    #[test]
    fn a_bare_yes_is_a_verdict() {
        let read = read_verdict(r#"{"v":1,"ok":true}"#).expect("a yes with nothing to say");
        assert_eq!(
            read,
            Verdict { ok: true, message: None, fields: BTreeMap::new(), show: Vec::new() }
        );
    }

    /// A line about a setting the manifest does not declare is dropped, and the verdict stands: there is no
    /// box on the form to draw it beside (`AMB-D-664`).
    #[test]
    fn a_line_about_an_undeclared_setting_is_dropped_rather_than_refused() {
        let read = read_verdict(
            r#"{"v":1,"ok":false,"fields":{"smtp_user":"unknown","api_token":"expired"}}"#,
        )
        .expect("the rest of the verdict stands");
        assert_eq!(read.field_keys(), vec!["smtp_user"]);
    }

    #[test]
    fn a_trailing_newline_does_not_stop_a_verdict_being_read() {
        assert!(read_verdict("{\"v\":1,\"ok\":true}\n").is_some());
    }

    // ────────────────── what is not a verdict is a silence (`AMB-D-354`) ──────────────────

    #[test]
    fn a_document_this_build_cannot_act_on_is_no_verdict() {
        for stdout in [
            "",
            "ok",
            "[]",
            r#"{"ok":true}"#,                       // no version marker
            r#"{"v":2,"ok":true}"#,                 // a version this build does not speak
            r#"{"v":1}"#,                           // nothing said about the values
            r#"{"v":1,"ok":"yes"}"#,                // the verdict is a bool, not a word
            r#"{"v":1,"ok":true,"fields":[]}"#,     // the lines are keyed by setting
            r#"{"v":1,"ok":true,"message":7}"#,     // a sentence is a string
            r#"{"v":1,"ok":false,"fields":{"smtp_user":7}}"#,
        ] {
            assert!(
                verdict(stdout, &declared(&["smtp_user"]), true).is_none(),
                "read as a verdict: {stdout}"
            );
        }
    }

    /// A sentence past the floor refuses the whole verdict rather than being trimmed: what would be shown
    /// otherwise is Amenbo's edit of the author's answer.
    #[test]
    fn a_sentence_past_the_floor_refuses_the_verdict() {
        let long = "x".repeat(MAX_VERDICT_TEXT_BYTES + 1);
        assert!(read_verdict(&format!(r#"{{"v":1,"ok":true,"message":"{long}"}}"#)).is_none());
        assert!(read_verdict(&format!(
            r#"{{"v":1,"ok":false,"fields":{{"smtp_user":"{long}"}}}}"#
        ))
        .is_none());
        assert!(
            read_verdict(r#"{"v":1,"ok":true,"message":"one\ntwo"}"#).is_none(),
            "a control character is refused, as it is in a stored value"
        );
        assert!(
            read_verdict(&format!(r#"{{"v":1,"ok":true,"message":"{}"}}"#, "x".repeat(MAX_VERDICT_TEXT_BYTES)))
                .is_some(),
            "the cap itself is allowed"
        );
    }

    /// An empty `message` is nothing said, not a blank line to draw.
    #[test]
    fn an_empty_sentence_is_nothing_said() {
        assert_eq!(read_verdict(r#"{"v":1,"ok":true,"message":""}"#).unwrap().message, None);
    }

    // ─────────────────────── what one run leaves on the log (`AMB-D-361`) ───────────────────────

    fn output(code: Option<i32>, stdout: &str, stderr: &str) -> std::io::Result<Option<PluginOutput>> {
        Ok(Some(PluginOutput {
            code,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            elapsed: Duration::ZERO,
        }))
    }

    #[test]
    fn an_answered_check_logs_as_a_clean_run_and_keeps_its_diagnostic() {
        let (checked, recorded) = read(
            "mail",
            &declared(&["smtp_user"]),
            true,
            output(Some(0), r#"{"v":1,"ok":true}"#, "signed in"),
            TIMEOUT,
        );
        assert!(checked.opens_the_gate());
        assert_eq!(recorded.event, crate::plugin_invoke::SETTINGS_LOG_EVENT);
        assert_eq!(recorded.outcome, Outcome::Ok);
        assert_eq!(recorded.stderr, "signed in", "the author's stderr is what the log is for");
    }

    #[test]
    fn a_check_that_exits_non_zero_has_checked_nothing() {
        let (checked, recorded) = read(
            "mail",
            &declared(&["smtp_user"]),
            true,
            output(Some(3), r#"{"v":1,"ok":true}"#, "boom"),
            TIMEOUT,
        );
        assert_eq!(checked.silence(), Some(Silence::Failed), "its stdout is not consumed");
        assert_eq!(recorded.outcome, Outcome::Failed);
        assert_eq!(recorded.code, Some(3));
    }

    #[test]
    fn a_check_that_writes_nonsense_has_checked_nothing() {
        let (checked, recorded) =
            read("mail", &declared(&["smtp_user"]), true, output(Some(0), "fine!", ""), TIMEOUT);
        assert_eq!(checked.silence(), Some(Silence::Unreadable));
        assert_eq!(recorded.outcome, Outcome::Ok, "the run itself was clean; the answer was not");
    }

    #[test]
    fn a_check_that_overran_is_logged_at_the_bound_it_was_killed_at() {
        let (checked, recorded) = read("mail", &[], true, Ok(None), TIMEOUT);
        assert_eq!(checked.silence(), Some(Silence::TimedOut));
        assert_eq!(recorded.outcome, Outcome::TimedOut);
        assert_eq!(recorded.elapsed, TIMEOUT);
        assert_eq!(recorded.code, None);
    }

    #[test]
    fn a_check_that_would_not_start_says_so_in_amenbos_own_words() {
        let (checked, recorded) =
            read("mail", &[], true, Err(std::io::Error::other("no such file")), TIMEOUT);
        assert_eq!(checked.silence(), Some(Silence::NotLaunched));
        assert_eq!(recorded.outcome, Outcome::NotLaunched);
        assert!(recorded.stderr.contains("no such file"), "there was no child to write one");
    }
}
