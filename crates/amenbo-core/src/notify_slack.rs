//! **Notifying a Slack channel** — one incoming webhook, posted to (`AMB-D-885`, ported from the
//! `slack` plugin `AMB-D-884` retired).
//!
//! **The URL is the whole connection, and it is a credential.** Whoever has it can post to that
//! channel, so it is kept in the table no road out of the store walks ([`crate::ops::secret`]) rather
//! than on the target's row, and nothing here writes it into a failure — a line in a log is read by
//! whoever is helping.
//!
//! **The words are not this module's.** What arrives is a set of lines already in the language the
//! reader reads ([`crate::notify_wording`]); what is added here is the project's name over them and
//! the POST under them. That split is what keeps one dictionary answering for both carriers.
//!
//! **The message is sent once.** Nothing is retried: a message this fails to deliver is one the
//! caller still holds, and two sides retrying the same send is how one message becomes three with
//! nobody able to say why.

use std::time::Duration;

use crate::error::{Error, Result};
use crate::model::{NotifyKind, NotifyTarget, SecretArea};

/// How long one post gets. A notification is never hurried — nothing waits behind it — so this is
/// not a deadline for the work, only the point past which a connection that will never answer stops
/// holding a run open.
const POST_TIMEOUT: Duration = Duration::from_secs(30);

/// How much of a refusal's body is quoted back. Slack answers a rejected post with a short reason
/// (`invalid_payload`, `no_service`); anything longer is an error page nobody needs in a log line.
const DIAGNOSTIC_LIMIT: usize = 200;

/// Where a Slack incoming webhook lives. Slack hands the whole URL over in one piece and a person
/// pastes it, so what can go wrong is pasting something else — the workspace's home page, a bot
/// token's URL, half of one. All three are visible in the shape.
const SCHEME: &str = "https://";
const HOST: &str = "hooks.slack.com";
const PATH_PREFIX: &str = "/services/";
const PATH_PARTS: usize = 3;

/// The field key a problem is reported against — the same spelling the credential is stored under, so
/// a screen can put a sentence under the box a person pasted into.
pub const FIELD: &str = NotifyTarget::SLACK_WEBHOOK_URL;

/// What a check that found nothing wrong says. It is careful about what it claims: the shape is all
/// it looked at, and a webhook revoked yesterday still has it.
pub const SHAPE_IS_RIGHT: &str =
    "The URL has the shape of a Slack incoming webhook. Whether it still works is what a test message answers.";

/// Read the webhook a target posts through, out of the table no road out of the store walks.
///
/// The only caller that wants the plaintext is the thing about to post with it; a screen asks whether
/// it is set and stops there.
pub fn webhook_for(store: &crate::store::Store, target_id: i64) -> Result<String> {
    let target = store
        .notify_target(target_id)?
        .ok_or_else(|| Error::not_found(format!("notification target {target_id} was not found")))?;
    if !matches!(target.kind, NotifyKind::Slack) {
        return Err(Error::invalid(format!(
            "notification target {target_id} is a {} target, which posts to no channel",
            target.kind.as_str()
        )));
    }
    Ok(store
        .secret_value(None, SecretArea::Notify, Some(target_id), NotifyTarget::SLACK_WEBHOOK_URL)?
        .unwrap_or_default())
}

/// What is wrong with a webhook URL, in the one sentence the box gets — or nothing, when it looks
/// like one.
///
/// **It quotes none of what was pasted.** The sentence is drawn on a settings screen, which is the
/// one place the URL is already visible, but a sentence also travels through logs and reports — and a
/// credential carried in one is a credential in a place nobody thought to guard. So every answer here
/// describes the shape and names nothing.
pub fn shape_problem(webhook: &str) -> Option<&'static str> {
    let raw = webhook.trim();
    if raw.is_empty() {
        return Some("There is no webhook here to post to.");
    }
    let Some(rest) = raw.strip_prefix(SCHEME) else {
        return Some("A Slack incoming webhook is an https URL, and this is not.");
    };
    let (host, path) = match rest.find('/') {
        Some(cut) => (&rest[..cut], &rest[cut..]),
        None => (rest, ""),
    };
    if host != HOST {
        return Some("A Slack incoming webhook is at hooks.slack.com, and this URL is not.");
    }
    let Some(tail) = path.strip_prefix(PATH_PREFIX) else {
        return Some(MALFORMED_PATH);
    };
    let parts: Vec<&str> = tail.split('/').collect();
    if parts.len() != PATH_PARTS || parts.iter().any(|part| part.is_empty()) {
        return Some(MALFORMED_PATH);
    }
    None
}

/// Said in full rather than built, so the shape it describes and the constants above cannot drift
/// into two different claims.
const MALFORMED_PATH: &str =
    "A Slack incoming webhook's path is /services/ and 3 parts after it, like /T…/B…/… — this one is not.";

/// Whether the settings are usable, without a message being posted to prove it.
///
/// It reads the value and stops there, because it runs unasked — whenever a screen saves — and a
/// check that posted would put a line in the channel on every save. Whether the webhook still works
/// is [`send_test`]'s to answer, which is a button somebody pressed on purpose.
pub fn check(webhook: &str) -> Result<()> {
    match shape_problem(webhook) {
        Some(wrong) => Err(Error::invalid(format!("{FIELD}: {wrong}"))),
        None => Ok(()),
    }
}

/// Post one message: the project's name over the lines, as the caller worded them.
///
/// **The project's name is the heading and nothing else is.** Everything a message carries belongs to
/// one project, so a name on each line would only repeat it; a project whose name could not be read
/// back gets no heading and the lines start straight away.
pub fn send(webhook: &str, project: &str, lines: &[String]) -> Result<()> {
    check(webhook)?;
    post(webhook, &message(project, lines))
}

/// Post the one line no event produces: somebody pressed a button. It lands in the same channel as
/// every other, so it is worded with them rather than left in the language this was written in.
pub fn send_test(webhook: &str, project: &str, language: &str) -> Result<()> {
    check(webhook)?;
    post(webhook, &message(project, &[crate::notify_wording::test_line(language)]))
}

/// The text of one message: the heading, then a line per event in the order they arrived.
///
/// That order is the order things happened in — lines are appended as events happen — so sorting them
/// again here would only be this function deciding it knows better.
fn message(project: &str, lines: &[String]) -> String {
    let said: Vec<&str> =
        lines.iter().map(|line| line.trim()).filter(|line| !line.is_empty()).collect();
    let name = project.trim();
    if name.is_empty() {
        return said.join("\n");
    }
    if said.is_empty() {
        return format!("*{name}*");
    }
    format!("*{name}*\n{}", said.join("\n"))
}

/// Hand one message to the webhook.
fn post(webhook: &str, text: &str) -> Result<()> {
    let body = serde_json::json!({ "text": text }).to_string();
    // A refused post is read rather than raised. Slack answers one with a short reason of its own
    // (`invalid_payload`, `no_service`), and that reason is the whole of what a person can act on —
    // left as the client's own error, the answer says a number and nothing about why.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(POST_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let answered = agent
        .post(webhook.trim())
        .header("content-type", "application/json")
        .send(&body);
    match answered {
        Ok(response) if response.status().is_success() => Ok(()),
        Ok(mut response) => {
            let status = response.status();
            let said = response
                .body_mut()
                .read_to_string()
                .unwrap_or_default()
                .chars()
                .take(DIAGNOSTIC_LIMIT)
                .collect::<String>();
            Err(Error::Io(std::io::Error::other(format!(
                "the webhook refused the message: {status} {}",
                said.trim()
            ))))
        }
        // A transport failure names the URL it failed on, and this one is the credential — a channel
        // anyone reading the log could then post to.
        Err(e) => Err(Error::Io(std::io::Error::other(format!(
            "posting to the webhook: {}",
            scrub(&e.to_string(), webhook.trim())
        )))),
    }
}

/// Take the webhook back out of what a failure says about it.
fn scrub(said: &str, webhook: &str) -> String {
    if webhook.is_empty() {
        return said.to_string();
    }
    said.replace(webhook, "<webhook_url>")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A URL of the right shape, **built rather than written down**. A literal one is what a secret
    /// scanner is looking for, and it refuses a push carrying it whether or not the thing is real —
    /// so the shape is assembled out of the constants above, which also holds the test to them.
    fn good() -> String {
        format!("{SCHEME}{HOST}{PATH_PREFIX}T0/B0/{}", "x".repeat(24))
    }

    /// The three ways a pasted URL is the wrong one, and the one way it is right.
    #[test]
    fn what_was_pasted_is_judged_by_its_shape() {
        assert_eq!(shape_problem(&good()), None);
        assert!(shape_problem("").is_some());
        assert!(shape_problem("http://hooks.slack.com/services/a/b/c").is_some());
        assert!(shape_problem("https://example.com/services/a/b/c").is_some());
        assert!(shape_problem("https://hooks.slack.com/T0/B0/x").is_some());
        assert!(shape_problem("https://hooks.slack.com/services/T0/B0").is_some());
        assert!(shape_problem("https://hooks.slack.com/services/T0//x").is_some());
        assert!(shape_problem("https://hooks.slack.com/services/T0/B0/x/y").is_some());
    }

    /// **No sentence quotes what was pasted.** A refusal travels further than the box it is drawn in.
    #[test]
    fn no_refusal_quotes_the_credential() {
        for pasted in ["https://example.com/secret-path", "not-a-webhook", &good()] {
            let said = shape_problem(pasted).unwrap_or(SHAPE_IS_RIGHT);
            assert!(!said.contains(pasted), "{said}");
            let refused = check(pasted).err().map(|e| e.to_string()).unwrap_or_default();
            assert!(!refused.contains(pasted), "{refused}");
        }
        // An empty box is the one case with nothing to quote, and it still says what is missing.
        assert!(shape_problem("  ").is_some());
    }

    /// The heading is the project, once, and the lines keep the order they arrived in.
    #[test]
    fn the_project_heads_the_message_once() {
        let lines = ["AI created AMB-T-1".to_string(), "AI finished AMB-T-1".to_string()];
        assert_eq!(message("work", &lines), "*work*\nAI created AMB-T-1\nAI finished AMB-T-1");
        assert_eq!(message("  ", &lines), "AI created AMB-T-1\nAI finished AMB-T-1");
        assert_eq!(message("work", &[]), "*work*");
    }

    /// Blank lines are the join's leftovers rather than anything a reader asked for.
    #[test]
    fn a_blank_line_is_not_a_line() {
        let lines = ["".to_string(), "  ".to_string(), "AI created AMB-T-1".to_string()];
        assert_eq!(message("work", &lines), "*work*\nAI created AMB-T-1");
    }

    /// A URL that is not a webhook never reaches the network: the shape is answered here, which is
    /// both faster and the only answer that can name the field.
    #[test]
    fn a_url_that_is_not_a_webhook_never_reaches_the_network() {
        let refused = send("https://example.com/x", "work", &["line".to_string()])
            .expect_err("not a webhook");
        assert!(refused.to_string().contains(FIELD), "{refused}");
    }

    /// **A refusal is quoted back in the words the far side used.** Slack answers a rejected post with
    /// a short reason of its own, and that reason is the whole of what a person can act on: a webhook
    /// deleted from the workspace and one that never existed are both `404`, and only the body tells
    /// them apart.
    #[test]
    fn a_refusal_is_answered_in_the_words_it_came_in() {
        let refusing = refusing_with("HTTP/1.1 404 Not Found", "no_service");
        let refused = post(&refusing, "anything").expect_err("the post was refused");
        let said = refused.to_string();
        assert!(said.contains("no_service"), "{said}");
        assert!(said.contains("404"), "{said}");
    }

    /// What comes back past the cap is left behind: a refusal is a short reason, and anything longer is
    /// an error page nobody needs in a log line.
    #[test]
    fn an_error_page_is_not_carried_into_a_log() {
        let refusing = refusing_with("HTTP/1.1 500 Internal Server Error", &"x".repeat(4000));
        let said = post(&refusing, "anything").expect_err("refused").to_string();
        assert!(said.len() < DIAGNOSTIC_LIMIT + 200, "{} characters came back", said.len());
    }

    /// One connection, answered with the status and body a test chose. It is here rather than behind
    /// the static host because what these two are about is the refusal's *body*, and that host answers
    /// a miss with none.
    fn refusing_with(status: &str, body: &str) -> String {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("a port to answer on");
        let port = listener.local_addr().expect("the port").port();
        let answer = format!(
            "{status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else { return };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(answer.as_bytes());
        });
        format!("http://127.0.0.1:{port}/services/T0/B0/x")
    }

    /// A transport failure that names the URL is not written down word for word.
    #[test]
    fn a_failure_that_names_the_webhook_is_not_repeated() {
        let webhook = good();
        let said = scrub(&format!("dns error for {webhook}"), &webhook);
        assert!(!said.contains(&webhook), "{said}");
        assert!(said.contains("<webhook_url>"), "{said}");
    }
}
