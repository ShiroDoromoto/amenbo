//! **Notifying by mail** — what a message is made of, and the settings it goes out on
//! (`AMB-D-885`, ported from the `mail` plugin `AMB-D-884` retired).
//!
//! The connection is the device's ([`NotifyTarget`]) and the address is the project's
//! ([`crate::model::ProjectNotify::mail_to`]), which is `AMB-D-885`'s split: a relay is written down once
//! and every project that reports through it selects it, while who is told is each project's own
//! business. [`Settings::of`] is where the two are put together, along with everything a person left
//! for the account to answer for.
//!
//! **The words are not this module's.** What arrives is a subject phrase and a body, already in the
//! language the reader reads (`crate::config::Config::language`); what is added here is the shape a
//! message has to have — the project in front of the subject, the header encoding, the line endings,
//! the thread it is filed under. That split is what keeps one dictionary answering for both carriers
//! rather than one per carrier.
//!
//! **The body is plain text and nothing else.** What goes in it is a list of moments, which has no
//! headings, tables or links to gain anything from markup; plain text reads the same in every mail
//! client on every device, none of which Amenbo gets to choose; an HTML body is likelier to be filed as
//! spam, which is a poor trade for a notification; and one form to build is one form to check, where
//! two is a pair that can drift until only one of them is right.

use crate::error::{Error, Result};
use crate::model::{NotifyKind, NotifyTarget, SecretArea};
use crate::notify_smtp::{self, Envelope, Relay};

/// How many characters a subject may run to.
///
/// Characters, not bytes: a subject with anything outside ASCII in it travels as an encoded word,
/// which is around four times its length in bytes, and a byte limit would leave a Japanese subject a
/// quarter the length of an English one for no reason a reader would recognise.
const SUBJECT_LIMIT: usize = 60;

/// What stands in the brackets when the project's name could not be read back. A subject is never
/// empty, and the name of the thing that sent the message is closer to the truth than nothing at all.
const FALLBACK_PROJECT: &str = "Amenbo";

/// Marks a project name that was cut to fit.
const ELLIPSIS: &str = "…";

/// The domain a message is named under when the address it is sent from carries none. A `Message-ID`
/// has to look like an address, and there is no honest name left to use.
const FALLBACK_DOMAIN: &str = "localhost";

/// One field of the form, and what stands between it and a message going out.
///
/// The sentence is English, like everything else core writes (`crate::error`): the side holding the
/// nineteen dictionaries is the side that can write them, and `field` is what lets it — a screen shows
/// its own sentence against the field this names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub field: &'static str,
    pub message: &'static str,
}

/// The field keys a problem is reported against — the same spellings the target's columns carry, so a
/// screen can put a sentence under the box a person typed into.
pub mod field {
    pub const HOST: &str = "smtp_host";
    pub const USER: &str = "smtp_user";
    pub const PASSWORD: &str = "smtp_password";
    pub const FROM: &str = "mail_from";
    pub const TO: &str = "mail_to";
}

/// What a message goes out on: everything a person filled in, plus everything derived from what they
/// left out.
#[derive(Clone, Debug)]
pub struct Settings {
    pub relay: Relay,
    pub envelope: Envelope,
}

impl Settings {
    /// Put a target's connection together with a project's address.
    ///
    /// **The account is what the rest falls back on.** It is the address most providers will let a
    /// message claim to be from, and it is the mailbox its owner reads — so filling in an account is
    /// enough to be reporting to yourself. An address in `mail_to` is what changes that, and it holds
    /// as many as are separated with commas, since a notification several people read is the ordinary
    /// case rather than a second setting. A relay that wants no account leaves nothing to fall back
    /// on, and then both of those have to be filled in themselves.
    ///
    /// What comes back may still be unsendable; [`Settings::problems`] is what says so.
    pub fn of(target: &NotifyTarget, password: &str, mail_to: &str) -> Settings {
        let user = trimmed(target.smtp_user.as_deref());
        let mut from = trimmed(target.mail_from.as_deref());
        if from.is_empty() {
            from = user.clone();
        }
        let mut to = addresses(mail_to);
        if to.is_empty() && !user.is_empty() {
            to = vec![user.clone()];
        }
        Settings {
            relay: Relay {
                host: trimmed(target.smtp_host.as_deref()),
                port: target
                    .smtp_port
                    .and_then(|port| u16::try_from(port).ok())
                    .filter(|port| *port != 0)
                    .unwrap_or(notify_smtp::DEFAULT_PORT),
                user,
                password: password.trim().to_string(),
            },
            envelope: Envelope { from, to },
        }
    }

    /// What stands between these settings and a message going out, one sentence per field it is about.
    /// Nothing reported means a message can be sent on them.
    ///
    /// **Presence is not the question**, which is why no field is simply required. An account filled in
    /// without its password cannot authenticate, and neither can a password with no account to use it
    /// with; but a relay that asks for neither — one on the same machine, one inside a network — is a
    /// perfectly good way to send, and demanding an account would put it out of reach of anyone filling
    /// the form in. What each field needs depends on the others, and that is a sentence rather than a
    /// flag.
    pub fn problems(&self) -> Vec<Problem> {
        let mut found = Vec::new();
        if self.relay.host.is_empty() {
            found.push(Problem {
                field: field::HOST,
                message: "There is no server to hand the message to.",
            });
        }
        if !self.relay.user.is_empty() && self.relay.password.is_empty() {
            found.push(Problem {
                field: field::PASSWORD,
                message: "The account has no password. Leave both empty for a relay that asks for none.",
            });
        } else if self.relay.user.is_empty() && !self.relay.password.is_empty() {
            found.push(Problem {
                field: field::USER,
                message: "There is a password here, but no account to authenticate as.",
            });
        }
        if self.relay.password.chars().any(char::is_whitespace) {
            found.push(Problem {
                field: field::PASSWORD,
                message: "This has a space in it. An app password is entered as one run of characters — the groups it is shown in are not part of it.",
            });
        }
        // Only reachable with no account: with one, both of these fall back on it.
        if self.envelope.from.is_empty() {
            found.push(Problem {
                field: field::FROM,
                message: "With no account to fall back on, the address to send from has to be filled in.",
            });
        }
        if self.envelope.to.is_empty() {
            found.push(Problem {
                field: field::TO,
                message: "With no account to fall back on, where to send has to be filled in.",
            });
        }
        found
    }
}

/// Whether the relay is reachable and the account accepted, without a message being posted to prove it
/// (`AMB-T-4750`).
///
/// The settings are read first and the connection only after: a form that is not filled in is answered
/// from here rather than from a server, which is both faster and the only answer that can name a field.
/// What comes back on a bad form is every problem at once, so a person fixes the settings in one go
/// instead of discovering the next one on the next try.
pub fn check(settings: &Settings) -> Result<()> {
    let problems = settings.problems();
    if !problems.is_empty() {
        return Err(Error::Io(std::io::Error::other(format!(
            "no message is sent on these settings: {}",
            problems
                .iter()
                .map(|p| format!("{}: {}", p.field, p.message))
                .collect::<Vec<_>>()
                .join(" ")
        ))));
    }
    notify_smtp::probe(&settings.relay)
}

/// Send one message: the subject phrase and body as the caller worded them, filed in this project's
/// thread at this target.
///
/// The settings are checked the same way [`check`] checks them — settings that cannot send end the
/// send rather than posting something incomplete — and then the message is built and handed over.
pub fn send(settings: &Settings, thread: &Thread, project: &str, what: &str, body: &str) -> Result<()> {
    let problems = settings.problems();
    if !problems.is_empty() {
        return Err(Error::Io(std::io::Error::other(format!(
            "no message is sent on these settings: {}",
            problems
                .iter()
                .map(|p| format!("{}: {}", p.field, p.message))
                .collect::<Vec<_>>()
                .join(" ")
        ))));
    }
    let message = compose(settings, thread, project, what, body);
    notify_smtp::send(&settings.relay, &settings.envelope, &message)
}

/// Where one message sits in its project's conversation at one target.
///
/// **A project's messages belong together, and the subject cannot be what says so**: it changes shape
/// with how many events a message carries, while matching subjects is how most mail clients decide two
/// messages are one conversation. So the messages say it in headers instead.
///
/// **The message the thread hangs off is named rather than remembered.** The plugin this was ported
/// from wrote down the `Message-ID` of the first message it ever sent for a project and had every
/// later one answer that; the name is instead derived from the project and the target, so nothing has
/// to be stored, nothing is lost when it cannot be read back, and a device restored from a backup goes
/// on filing into the conversation it was filing into. What it costs is that the message being
/// answered is one that was never sent — which mail clients handle, `References` being exactly the
/// chain a client walks when what it names is not in the mailbox. Where a client insists on having the
/// message itself, the messages land as separate threads: one more thread in the mailbox and no
/// notification lost, which is the same harmless outcome the plugin had when its note could not be
/// read back.
#[derive(Clone, Debug)]
pub struct Thread {
    /// This message's own name.
    id: String,
    /// The name the conversation is filed under.
    root: String,
}

impl Thread {
    /// Name this message, and the conversation it belongs to.
    ///
    /// The domain is the sender's, that being the one name in the message belonging to whoever is
    /// sending it.
    pub fn of(settings: &Settings, project_id: i64, target_id: i64) -> Thread {
        let domain = domain_of(&settings.envelope.from);
        Thread {
            id: format!("<{}@{domain}>", hex(&random_bytes())),
            root: format!("<amenbo-p{project_id}-n{target_id}@{domain}>"),
        }
    }

    /// What the message says about its thread.
    ///
    /// `In-Reply-To` is what a client files a message under, and `References` is the chain it walks
    /// when what that names is not in the mailbox. Both carry the one name the conversation is filed
    /// under: the messages under it are not answers to one another, they are all reports on the same
    /// project.
    fn headers(&self) -> Vec<String> {
        vec![
            format!("Message-ID: {}", self.id),
            format!("In-Reply-To: {}", self.root),
            format!("References: {}", self.root),
        ]
    }
}

/// Write the message: the headers a mail client needs to file it, then the body.
fn compose(settings: &Settings, thread: &Thread, project: &str, what: &str, body: &str) -> String {
    let mut headers = vec![
        format!("From: {}", header_value(&settings.envelope.from)),
        format!("To: {}", header_value(&settings.envelope.to.join(", "))),
        format!("Subject: {}", subject(project, what)),
        format!("Date: {}", crate::time::Timestamp::now().0.to_rfc2822()),
    ];
    headers.extend(thread.headers());
    headers.push("MIME-Version: 1.0".to_string());
    headers.push("Content-Type: text/plain; charset=\"utf-8\"".to_string());
    format!("{}\r\n\r\n{}", headers.join("\r\n"), crlf(&message_body(project, body)))
}

/// The body of one message: the project's name, then what happened.
///
/// The heading is the project's name on its own. The subject carries it too, but the subject is cut to
/// [`SUBJECT_LIMIT`] and the project is what gives way there, so this is where a long name is read in
/// full. It is written once rather than on every line, because everything a message carries belongs to
/// one project.
///
/// A project whose name could not be read back gets no heading and what happened starts straight away.
/// The lines already say everything that did; not knowing what the project is called is no reason to
/// hold a message back.
fn message_body(project: &str, body: &str) -> String {
    let name = project.trim();
    // Blank lines at either end are the join's leftovers rather than anything a reader asked for; what
    // is between them is left exactly as it arrived, the order lines come in being the order things
    // happened in.
    let body = body.trim_matches(|ch: char| ch == '\n' || ch == '\r' || ch == ' ' || ch == '\t');
    match (name.is_empty(), body.is_empty()) {
        (true, _) => format!("{body}\n"),
        (false, true) => format!("{name}\n"),
        (false, false) => format!("{name}\n\n{body}\n"),
    }
}

/// The subject, ready to be a header: the project in front of what happened, cut to fit, and encoded.
///
/// **The project is the only part that gives way.** It is the one part whose length cannot be reckoned
/// with beforehand — what happened and which record it happened to are Amenbo's own vocabulary — and
/// they are what the subject is for: a subject that runs a few characters long says more than one
/// trimmed into saying nothing.
pub fn subject(project: &str, what: &str) -> String {
    header_ready(&subject_text(project, what))
}

/// The subject before it is encoded — the part the limit is counted in, and the part a test can read.
fn subject_text(project: &str, what: &str) -> String {
    // Both halves are made header-safe here rather than at the encoding, because the cut below is
    // counted in characters and a line break left in would be counted as one of them.
    let what = header_value(what);
    let mut name = header_value(project);
    if name.is_empty() {
        name = FALLBACK_PROJECT.to_string();
    }
    let full = format!("[{name}] {what}");
    if full.chars().count() <= SUBJECT_LIMIT {
        return full;
    }
    let fixed = format!("[] {what}").chars().count() + ELLIPSIS.chars().count();
    let room = SUBJECT_LIMIT.saturating_sub(fixed);
    if room < 1 {
        return format!("[{ELLIPSIS}] {what}");
    }
    let cut: String = name.chars().take(room).collect();
    format!("[{cut}{ELLIPSIS}] {what}")
}

/// Make one value fit to be a header.
///
/// Anything outside ASCII travels as an encoded word (RFC 2047), base64 being the compact way to carry
/// a subject that is mostly not ASCII; a value that is plain ASCII comes back untouched, which is what
/// a reader's mail client would rather see.
///
/// **An encoded word is cut to length and folded**, because the standard gives one 75 characters and a
/// longer one is refused by the stricter readers rather than shown. The cut is made at character
/// boundaries: half a character encoded on one line and half on the next comes out as neither.
fn header_ready(value: &str) -> String {
    let value = header_value(value);
    if value.is_ascii() {
        return value;
    }
    // `=?UTF-8?B?` and `?=` are twelve characters, and base64 is written in groups of four — so the
    // payload of one word is sixty characters, which is forty-five bytes of what it carries.
    const CHUNK: usize = 45;
    let mut words = Vec::new();
    let mut chunk = String::new();
    for ch in value.chars() {
        if chunk.len() + ch.len_utf8() > CHUNK {
            words.push(encoded_word(&chunk));
            chunk.clear();
        }
        chunk.push(ch);
    }
    if !chunk.is_empty() {
        words.push(encoded_word(&chunk));
    }
    words.join("\r\n ")
}

fn encoded_word(part: &str) -> String {
    use base64::Engine as _;
    format!("=?UTF-8?B?{}?=", base64::engine::general_purpose::STANDARD.encode(part))
}

/// Make one value safe to put on a header line.
///
/// A line break inside it would end the header and start whatever came after it as a new one — an
/// address nobody asked for, on a message assembled partly out of text read back from the store.
fn header_value(value: &str) -> String {
    value.trim().replace("\r\n", " ").replace(['\r', '\n'], " ")
}

/// Give the body the line ending mail is written in.
fn crlf(body: &str) -> String {
    body.replace("\r\n", "\n").replace('\n', "\r\n")
}

/// Break a comma-separated list of addresses apart, dropping what a trailing comma or a stray
/// separator leaves behind.
fn addresses(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn trimmed(value: Option<&str>) -> String {
    value.unwrap_or_default().trim().to_string()
}

/// The domain half of an address, or the name that stands in for one.
fn domain_of(address: &str) -> String {
    match address.rsplit_once('@') {
        Some((_, domain)) if !domain.is_empty() => domain.to_string(),
        _ => FALLBACK_DOMAIN.to_string(),
    }
}

fn random_bytes() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("failed to draw OS randomness");
    bytes
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Put together what one target sends on, for one project — or for none, which is the shape a person
/// filling the shelf in is answered from before any project has selected it.
///
/// **The credential is read here and nowhere above.** It comes out of the table no road out of the
/// store walks ([`crate::ops::secret`]), and the only caller that wants the plaintext is the thing
/// about to connect with it; a screen asks whether it is set and stops there.
pub fn settings_for(
    store: &crate::store::Store,
    target_id: i64,
    project_id: Option<i64>,
) -> Result<Settings> {
    let target = store
        .notify_target(target_id)?
        .ok_or_else(|| Error::not_found(format!("notification target {target_id} was not found")))?;
    if !carries_mail(&target) {
        return Err(Error::invalid(format!(
            "notification target {target_id} is a {} target, which sends no mail",
            target.kind.as_str()
        )));
    }
    let password = store
        .secret_value(None, SecretArea::Notify, Some(target_id), NotifyTarget::SMTP_PASSWORD)?
        .unwrap_or_default();
    let mail_to = match project_id {
        Some(project_id) => {
            store.project_notify(project_id)?.map(|row| row.mail_to).unwrap_or_default()
        }
        None => String::new(),
    };
    Ok(Settings::of(&target, &password, &mail_to))
}

/// Is this target one this module can send through at all? A Slack target's whole connection is its
/// webhook, and none of the columns read here mean anything on it.
pub fn carries_mail(target: &NotifyTarget) -> bool {
    matches!(target.kind, NotifyKind::Mail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NotifyKind;

    fn mail_target() -> NotifyTarget {
        NotifyTarget {
            id: 7,
            kind: NotifyKind::Mail,
            name: "work".into(),
            is_default: true,
            smtp_host: Some("smtp.example.com".into()),
            smtp_port: None,
            smtp_user: Some("someone@example.com".into()),
            mail_from: None,
            ..Default::default()
        }
    }

    /// The account is what everything a person left blank falls back on: the port is the submission
    /// one, the sender is the account, and so is the only recipient.
    #[test]
    fn what_is_left_blank_falls_back_on_the_account() {
        let settings = Settings::of(&mail_target(), "  app-password  ", "  ");
        assert_eq!(settings.relay.port, notify_smtp::DEFAULT_PORT);
        assert_eq!(settings.relay.password, "app-password");
        assert_eq!(settings.envelope.from, "someone@example.com");
        assert_eq!(settings.envelope.to, vec!["someone@example.com".to_string()]);
        assert!(settings.problems().is_empty(), "{:?}", settings.problems());
    }

    /// A project's address is the project's, and it holds as many as it separates with commas — the
    /// ordinary case being a notification several people read.
    #[test]
    fn the_projects_address_holds_as_many_as_it_separates() {
        let settings = Settings::of(&mail_target(), "pw", " a@example.com ,, b@example.com,");
        assert_eq!(settings.envelope.to, vec!["a@example.com".to_string(), "b@example.com".to_string()]);
    }

    /// A relay that asks for neither account nor password is a perfectly good way to send — as long as
    /// the two fields the account would have answered for are filled in themselves.
    #[test]
    fn a_relay_that_asks_for_nothing_needs_the_two_it_cannot_answer_for() {
        let mut target = mail_target();
        target.smtp_user = Some(String::new());
        let bare = Settings::of(&target, "", "");
        let fields: Vec<_> = bare.problems().iter().map(|p| p.field).collect();
        assert_eq!(fields, vec![field::FROM, field::TO]);

        target.mail_from = Some("robot@example.com".into());
        let filled = Settings::of(&target, "", "ops@example.com");
        assert!(filled.problems().is_empty(), "{:?}", filled.problems());
    }

    /// Half an account is what a form is complained about for — either half, and the sentence names
    /// which half is missing.
    #[test]
    fn half_an_account_is_no_account() {
        let mut target = mail_target();
        let no_password = Settings::of(&target, "", "");
        assert_eq!(no_password.problems().first().map(|p| p.field), Some(field::PASSWORD));

        target.smtp_user = Some(String::new());
        target.mail_from = Some("robot@example.com".into());
        let no_account = Settings::of(&target, "pw", "ops@example.com");
        assert_eq!(no_account.problems().first().map(|p| p.field), Some(field::USER));
    }

    /// An app password pasted in the groups it is shown in is the commonest way a correct one is
    /// wrong, and the relay answers that with the same refusal as a wrong password.
    #[test]
    fn an_app_password_with_its_spaces_still_in_is_named() {
        let settings = Settings::of(&mail_target(), "abcd efgh ijkl mnop", "");
        assert_eq!(settings.problems().first().map(|p| p.field), Some(field::PASSWORD));
    }

    /// A host nobody filled in is the one thing no other field can stand in for.
    #[test]
    fn with_no_server_there_is_nowhere_to_hand_it() {
        let mut target = mail_target();
        target.smtp_host = None;
        let settings = Settings::of(&target, "pw", "");
        assert_eq!(settings.problems().first().map(|p| p.field), Some(field::HOST));
    }

    /// The project is the only part of a subject that gives way, and what happened is never cut.
    #[test]
    fn a_long_project_is_cut_and_what_happened_is_not() {
        let what = "created a task AMB-T-1";
        let long = "a project with a name somebody really did type all of";
        let cut = subject_text(long, what);
        assert!(cut.ends_with(&format!("…] {what}")), "{cut}");
        assert_eq!(cut.chars().count(), SUBJECT_LIMIT);
        // Cut or not, what goes on the line is an encoded word — the ellipsis alone puts it outside
        // ASCII.
        assert!(subject(long, what).starts_with("=?UTF-8?B?"));
    }

    /// A name that fits is left exactly as it is, and a project with no name at all still gets a
    /// subject.
    #[test]
    fn a_subject_is_never_empty_and_never_padded() {
        assert_eq!(subject("work", "created a task"), "[work] created a task");
        assert_eq!(subject("   ", "created a task"), "[Amenbo] created a task");
    }

    /// Anything outside ASCII travels as an encoded word, and a long one is cut into several rather
    /// than run past the length the standard gives it.
    #[test]
    fn a_subject_outside_ascii_travels_encoded_and_folded() {
        let encoded = subject("仕事", "タスクを作成しました AMB-T-1");
        assert!(encoded.starts_with("=?UTF-8?B?"), "{encoded}");
        for word in encoded.split("\r\n ") {
            assert!(word.chars().count() <= 75, "an encoded word ran long: {word}");
            assert!(word.starts_with("=?UTF-8?B?") && word.ends_with("?="), "{word}");
        }
        // What was encoded is what comes back out of it.
        let decoded: String = encoded
            .split("\r\n ")
            .map(|word| {
                use base64::Engine as _;
                let payload = word.trim_start_matches("=?UTF-8?B?").trim_end_matches("?=");
                String::from_utf8(
                    base64::engine::general_purpose::STANDARD.decode(payload).expect("base64"),
                )
                .expect("utf-8")
            })
            .collect();
        assert_eq!(decoded, "[仕事] タスクを作成しました AMB-T-1");
    }

    /// A line break in a value would end the header and start whatever came after it as a new one —
    /// on a message assembled partly out of text read back from the store.
    #[test]
    fn a_header_value_cannot_start_a_header_of_its_own() {
        let settings = Settings::of(&mail_target(), "pw", "");
        let thread = Thread::of(&settings, 1, 7);
        let message = compose(&settings, &thread, "work\r\nBcc: nobody@example.com", "created a task", "line");
        let headers = message.split("\r\n\r\n").next().expect("headers");
        assert!(!headers.lines().any(|line| line.starts_with("Bcc:")), "{headers}");
        assert_eq!(headers.lines().filter(|line| line.starts_with("Subject:")).count(), 1);
        // The name is still said, on the line it belongs to.
        assert!(subject_text("work\r\nBcc: nobody@example.com", "x").contains("work Bcc:"));
    }

    /// Every message a project sends through one target names the same conversation, and names itself
    /// with something no other message carries.
    #[test]
    fn messages_from_one_project_name_one_conversation() {
        let settings = Settings::of(&mail_target(), "pw", "");
        let first = Thread::of(&settings, 3, 7);
        let second = Thread::of(&settings, 3, 7);
        assert_eq!(first.root, second.root);
        assert_ne!(first.id, second.id);
        assert!(first.root.ends_with("@example.com>"), "{}", first.root);

        // Another project, and the same project at another target, are other conversations.
        assert_ne!(Thread::of(&settings, 4, 7).root, first.root);
        assert_ne!(Thread::of(&settings, 3, 8).root, first.root);
    }

    /// The project's name heads the body, where it is read in full — the subject is where it is cut.
    #[test]
    fn the_body_carries_the_project_in_full() {
        assert_eq!(message_body("work", "one\ntwo"), "work\n\none\ntwo\n");
        assert_eq!(message_body("  ", "one\n"), "one\n");
        assert_eq!(message_body("work", "  "), "work\n");
    }

    /// The whole message is written in the line ending mail is written in — including the blank line
    /// that ends the headers.
    #[test]
    fn a_message_is_written_in_crlf() {
        let settings = Settings::of(&mail_target(), "pw", "");
        let thread = Thread::of(&settings, 1, 7);
        let message = compose(&settings, &thread, "work", "created a task", "one\ntwo");
        assert!(!message.replace("\r\n", "").contains('\n'), "a bare newline survived: {message:?}");
        assert!(message.contains("\r\n\r\nwork\r\n\r\none\r\ntwo\r\n"), "{message:?}");
        assert!(message.contains("MIME-Version: 1.0\r\n"));
        assert!(message.contains("Content-Type: text/plain; charset=\"utf-8\""));
    }

    /// Settings that cannot send are answered here rather than at a server, and the answer names every
    /// field at once so a person fixes the form in one go.
    #[test]
    fn settings_that_cannot_send_never_reach_a_server() {
        let mut target = mail_target();
        target.smtp_host = None;
        let settings = Settings::of(&target, "", "");
        let said = check(&settings).expect_err("a form this empty cannot send").to_string();
        assert!(said.contains(field::HOST), "{said}");
        assert!(said.contains(field::PASSWORD), "{said}");
    }
}
