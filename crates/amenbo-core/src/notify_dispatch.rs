//! **Carrying a notification out** — which events a project reports, who they are worded for, and the
//! process that posts them (`AMB-D-885`, `AMB-D-416`, `AMB-D-352`).
//!
//! The events are already on the outbox: every write point appends one inside its transaction
//! (`AMB-D-367`), and the tick puts the day's due warnings there too ([`crate::due`]). So nothing new is
//! collected here — what a pass words is what the drive's one walk saw ([`crate::outbox_drive`]), handed
//! over rather than read a second time, which is what keeps one reclaim from outrunning another reader.
//!
//! **The network is never on the write's path.** A pass turns events into messages and hands them to a
//! process of its own ([`Dispatcher`]): whatever the person was doing returns without waiting on a relay
//! that may be minutes away.
//!
//! **A burst is one message.** Events are grouped by project and by target, so ten writes in a row reach a
//! channel as ten lines under one heading rather than as ten messages — and a project that reports through
//! both a channel and an inbox gets one of each.
//!
//! **Only the AI's writes are reported** (`AMB-D-416`), plus the two events nobody drove: a day arriving is
//! not a write, and there was nobody to have been present for it. What a person did themselves they were
//! there for, and reporting it back to them is noise.
//!
//! **A message that will not go is dropped** (`AMB-D-352`). Nothing is retried and nothing is queued for a
//! later attempt: a notification is worth telling now, and a channel catching up on yesterday's burst is
//! worse than one that missed it. What failed is written to the delivery log ([`crate::delivery_log`],
//! `AMB-D-361`).
//!
//! **A line names the record and not its title.** The ref is what a reader searches for, and it is a length
//! that can be reckoned with; a title is whatever somebody typed, and a subject that has to cut one says
//! something other than what it said ([`crate::notify_mail`]).

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::model::{NotifyKind, ProjectNotify};
use crate::notify_wording::{self, Said};
use crate::lifecycle::name;
use crate::store::Store;

/// How many events one pass carries into messages.
///
/// It is a mouthful and not a backlog: a burst is what this is for, and a store that has just been restored
/// or resumed can have far more standing than anybody wants posted to a channel at once. What is past the
/// cap is dropped with a line in the log, which is the same answer a failed send gets (`AMB-D-352`).
pub const PASS_LIMIT: usize = 200;

/// One event, reduced to what a line is written from.
///
/// It is taken off the outbox row rather than read back from the record, for the reason the row carries
/// these at all (`AMB-D-405`, `AMB-D-407`): by the time anybody looks, the record may have moved or gone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Happened {
    /// One of [`crate::lifecycle::V1_EVENTS`].
    pub event: String,
    /// The affected record's id.
    pub record_id: i64,
    /// The project the record was in when it fired. `None` is a real answer, and a record in no project is
    /// reported to nobody: a notification is a project's setting.
    pub project: Option<i64>,
    /// Who drove the write, as the outbox spells it — `ai`, `human`, or empty on the events nobody drove.
    pub actor: String,
    /// The record's new state, for the events an update disambiguates.
    pub new_state: Option<String>,
    /// The record this event's record hangs on — a comment's task.
    pub parent: Option<i64>,
}

/// One message: a project's burst, for one target, already worded.
///
/// It crosses a process boundary, so it carries everything the sender needs and nothing it would have to go
/// and look up — except the connection itself, which is a credential and stays in the store until the
/// moment it is used ([`crate::notify_slack::webhook_for`], [`crate::notify_mail::settings_for`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub project_id: i64,
    /// The project's name, as the heading over the lines. Empty where it could not be read back, which
    /// costs the message its heading and nothing else.
    pub project: String,
    /// The target on the device's shelf that carries it.
    pub target_id: i64,
    /// One line per event, oldest first — the order things happened in.
    pub lines: Vec<String>,
    /// The language the lines were written in, carried so the sender's own additions match them.
    pub language: String,
    /// What a message carrying **one** line says happened, ref and all — the subject a mail target needs
    /// where there is room to say what it was rather than how much there was
    /// ([`crate::notify_wording::subject_what`]). A message that grew past one line says the count
    /// instead, which is not known until the burst closes, so it is worked out at the send.
    pub one_says: String,
}

/// Whose writes are reported (`AMB-D-416`) — the facet the outbox stamps on a row the AI drove.
const ACTOR_AI: &str = "ai";

/// Turn a pass's events into the messages they become.
///
/// Every event is asked of the store once: which project it happened in, whether that project notifies,
/// whether it reports this event, and which targets carry it. What comes back is one message per
/// `(project, target)` that had at least one line.
pub fn messages(store: &Store, seen: &[Happened]) -> Result<Vec<Message>> {
    let language = store.config.language.clone().unwrap_or_else(|| "en".to_string());
    let mut carried: Vec<Message> = Vec::new();
    // Read once per project rather than once per event: a burst is mostly one project, and the settings
    // cannot change underneath a pass that has already begun.
    let mut asked: Vec<(i64, Option<Reporting>)> = Vec::new();

    for happened in seen.iter().take(PASS_LIMIT) {
        let Some(project_id) = happened.project else { continue };
        if !worth_saying(happened) {
            continue;
        }
        let reporting = match asked.iter().find(|(id, _)| *id == project_id) {
            Some((_, found)) => found.clone(),
            None => {
                let found = reporting_of(store, project_id)?;
                asked.push((project_id, found.clone()));
                found
            }
        };
        let Some(reporting) = reporting else { continue };
        if !reporting.events.iter().any(|e| e == &happened.event) {
            continue;
        }
        let parts = Parts::of(store, &language, happened);
        let line = notify_wording::line(&language, &parts.said(&happened.event));
        // What a one-line message would say it was, kept with the first line rather than worked out again
        // at the send: by then the event is behind the wording and only the sentence is left.
        let one_says = format!(
            "{} {}",
            notify_wording::subject_what(&language, &happened.event, parts.state.as_deref()),
            parts.what
        );
        for target_id in &reporting.targets {
            match carried.iter_mut().find(|m| m.project_id == project_id && m.target_id == *target_id) {
                Some(message) => message.lines.push(line.clone()),
                None => carried.push(Message {
                    project_id,
                    project: reporting.name.clone(),
                    target_id: *target_id,
                    lines: vec![line.clone()],
                    language: language.clone(),
                    one_says: one_says.clone(),
                }),
            }
        }
    }

    if seen.len() > PASS_LIMIT {
        tracing::warn!(
            seen = seen.len(),
            limit = PASS_LIMIT,
            "more events than one notification pass carries; the rest are dropped"
        );
    }
    Ok(carried)
}

/// What one project reports, and where — read once per project.
#[derive(Clone, Debug)]
struct Reporting {
    name: String,
    targets: Vec<i64>,
    events: Vec<String>,
}

/// A project's notification settings, or `None` where it notifies nothing at all — off, with no target
/// selected, or never set up.
fn reporting_of(store: &Store, project_id: i64) -> Result<Option<Reporting>> {
    let Some(ProjectNotify { enabled: true, .. }) = store.project_notify(project_id)? else {
        return Ok(None);
    };
    let targets: Vec<i64> =
        store.project_notify_targets(project_id)?.iter().map(|row| row.target_id).collect();
    if targets.is_empty() {
        return Ok(None);
    }
    let events: Vec<String> =
        store.project_notify_events(project_id)?.iter().map(|row| row.event.clone()).collect();
    if events.is_empty() {
        return Ok(None);
    }
    let name = store.project(project_id)?.map(|p| p.name).unwrap_or_default();
    Ok(Some(Reporting { name, targets, events }))
}

/// **The actor gate** (`AMB-D-416`): the AI's writes are reported and a person's own are not — they were
/// there for them. What passes unasked is the two events nobody drove.
fn worth_saying(happened: &Happened) -> bool {
    unattended(&happened.event) || happened.actor == ACTOR_AI
}

/// The events a day brings rather than a person (`crate::due`). They carry no actor, so the gate above
/// would drop them.
fn unattended(event: &str) -> bool {
    event == name::TASK_DUE || event == name::TASK_DUE_TOMORROW
}

/// The three things a line is filled in from, owned — the slots [`Said`] borrows.
struct Parts {
    who: String,
    what: String,
    state: Option<String>,
}

impl Parts {
    /// Read off one event, with the store asked only for the name whoever drove it goes by.
    fn of(store: &Store, language: &str, happened: &Happened) -> Parts {
        Parts { who: who(store, happened), what: what(happened), state: state(language, happened) }
    }

    fn said<'a>(&'a self, event: &'a str) -> Said<'a> {
        Said { event, who: &self.who, what: &self.what, state: self.state.as_deref() }
    }
}

/// The name whoever drove the write goes by, or nothing on the events nobody drove.
fn who(store: &Store, happened: &Happened) -> String {
    if unattended(&happened.event) {
        return String::new();
    }
    if happened.actor == ACTOR_AI {
        store.config.ai_display_name()
    } else {
        store.config.human_display_name()
    }
}

/// The record the line is about, by the ref a reader would search for.
///
/// A comment names the record it hangs on rather than itself where that is known: "added a comment on"
/// followed by the task's own ref is the sentence somebody can act on, and the comment's number is not a
/// thing anybody goes looking for.
fn what(happened: &Happened) -> String {
    use crate::idref;
    match happened.event.as_str() {
        e if e.starts_with("decision.") => idref::decision(happened.record_id),
        name::COMMENT_ADDED | name::COMMENT_REMOVED => match happened.parent {
            Some(parent) => idref::task(parent),
            None => idref::task_comment(happened.record_id),
        },
        _ => idref::task(happened.record_id),
    }
}

/// The second thing an event names, in the reader's words where it is one of Amenbo's.
///
/// Three kinds reach one slot. A **status** is a word Amenbo owns, so it is said the way Amenbo says it. An
/// **assignee** arrives as the facet and is said by the name that facet goes by — the same name the
/// sentence's subject is said by. A **project** arrives as its slug, which is the store's own value and the
/// one a reader would go and search for, so it passes through untouched.
fn state(language: &str, happened: &Happened) -> Option<String> {
    let new = happened.new_state.as_deref()?;
    match happened.event.as_str() {
        name::TASK_STATUS_CHANGED => Some(notify_wording::status_word(language, new)),
        _ => Some(new.to_string()),
    }
}

/// How a face hands a pass's messages to a process of its own.
///
/// It answers one question and returns: *post these, and do not make me wait*. An `Err` means no process
/// started, which under `AMB-D-352` is a dropped message and a line in the log — not a failed write.
pub trait Dispatcher {
    fn hand_over(&self, messages: &[Message]) -> std::io::Result<()>;
}

/// The dispatcher a real face hands a drive: **this same executable, re-run** as a sender.
///
/// Every face ships as a single binary, so a sender needs no second one; what differs between faces is only
/// how each names its own entry point, which is what `argv` carries. The store's base directory follows it,
/// because a sender reads the connection out of the store its parent drove and not whichever one its own
/// environment would resolve to.
///
/// **The messages go over stdin rather than on the command line.** A burst is a paragraph of text, and a
/// command line is a place with a length limit and a reader — every process list on the machine.
pub struct SelfDispatcher {
    argv: Vec<String>,
    base_dir: std::path::PathBuf,
}

impl SelfDispatcher {
    pub fn new(argv: &[&str], base_dir: std::path::PathBuf) -> Self {
        Self { argv: argv.iter().map(|a| (*a).to_string()).collect(), base_dir }
    }
}

impl Dispatcher for SelfDispatcher {
    fn hand_over(&self, messages: &[Message]) -> std::io::Result<()> {
        use std::io::Write as _;
        use std::process::Stdio;
        let written = serde_json::to_vec(messages)?;
        let mut child = std::process::Command::new(std::env::current_exe()?)
            .args(&self.argv)
            .arg(&self.base_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        // The parent's whole part is to hand the work over. It writes the batch, closes the pipe, and
        // leaves — the child has everything it needs by then, and a parent that waited would be the write
        // path waiting on a relay.
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(&written)?;
        }
        crate::sys::reap(child);
        Ok(())
    }
}

/// Post one pass's messages — **the sender process's whole job**.
///
/// Each message is sent on its own: a relay that will not answer costs that message and not the pass, which
/// is what keeps one broken connection from silencing the other. What failed is answered here and recorded
/// by the caller; nothing is retried (`AMB-D-352`).
pub fn deliver(store: &Store, messages: &[Message]) -> Vec<(i64, String)> {
    let mut refused = Vec::new();
    for message in messages {
        if let Err(e) = post(store, message) {
            refused.push((message.target_id, e.to_string()));
        }
    }
    refused
}

/// One message, through whichever kind of target carries it.
fn post(store: &Store, message: &Message) -> Result<()> {
    let target = store.notify_target(message.target_id)?.ok_or_else(|| {
        crate::Error::not_found(format!("notification target {} was not found", message.target_id))
    })?;
    match target.kind {
        NotifyKind::Slack => {
            let webhook = crate::notify_slack::webhook_for(store, message.target_id)?;
            crate::notify_slack::send(&webhook, &message.project, &message.lines)
        }
        NotifyKind::Mail => {
            let settings =
                crate::notify_mail::settings_for(store, message.target_id, Some(message.project_id))?;
            let thread =
                crate::notify_mail::Thread::of(&settings, message.project_id, message.target_id);
            // One line has room to say what happened; a burst has room only to say how much of it there
            // was, the events having nothing left in common by then but the project.
            let what = if message.lines.len() == 1 {
                message.one_says.clone()
            } else {
                crate::notify_wording::subject_count(&message.language, message.lines.len())
            };
            crate::notify_mail::send(
                &settings,
                &thread,
                &message.project,
                &what,
                &message.lines.join("\n"),
            )
        }
    }
}

/// **The sender process's whole life** — open the store it was handed, read the messages off stdin, post
/// them, exit.
///
/// The store is named rather than resolved: it must post through the connections of the store its parent
/// drove, not whichever one its own working directory would resolve to.
///
/// Nothing here is reported to a caller — there is none. A store that will not open, a stdin that is not
/// messages, a relay that refuses: each is a line in the log and a message nobody receives (`AMB-D-352`).
pub fn send_process(base_dir: std::path::PathBuf) {
    use std::io::Read as _;
    let store = match crate::Store::open_at(crate::config::Paths::at(base_dir)) {
        Ok(store) => store,
        Err(e) => {
            tracing::warn!(error = %e, "a notification sender could not open the store; nothing is sent");
            return;
        }
    };
    let mut written = Vec::new();
    if let Err(e) = std::io::stdin().read_to_end(&mut written) {
        tracing::warn!(error = %e, "a notification sender could not read what it was handed");
        return;
    }
    let messages: Vec<Message> = match serde_json::from_slice(&written) {
        Ok(messages) => messages,
        Err(e) => {
            tracing::warn!(error = %e, "a notification sender was handed something that is not messages");
            return;
        }
    };
    store.send_notifications(&messages);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn happened(event: &str, record_id: i64, project: Option<i64>, actor: &str) -> Happened {
        Happened {
            event: event.to_string(),
            record_id,
            project,
            actor: actor.to_string(),
            ..Default::default()
        }
    }

    /// **A person's own writes are not reported back to them** (`AMB-D-416`) — they were there for them.
    #[test]
    fn only_the_ais_writes_and_the_days_own_events_pass_the_gate() {
        assert!(worth_saying(&happened(name::TASK_CREATED, 1, Some(1), "ai")));
        assert!(!worth_saying(&happened(name::TASK_CREATED, 1, Some(1), "human")));
        // A day arriving carries no actor, and there is nobody it could have been.
        assert!(worth_saying(&happened(name::TASK_DUE, 1, Some(1), "")));
        assert!(worth_saying(&happened(name::TASK_DUE_TOMORROW, 1, Some(1), "")));
    }

    /// A comment names the record it hangs on: that is the sentence somebody can act on.
    #[test]
    fn a_comment_names_the_task_it_was_posted_to() {
        let mut on_a_task = happened(name::COMMENT_ADDED, 55, Some(1), "ai");
        on_a_task.parent = Some(12);
        assert_eq!(what(&on_a_task), "AMB-T-12");
        // An older store's row carries no parent; the comment then names itself rather than nothing.
        let orphan = happened(name::COMMENT_ADDED, 55, Some(1), "ai");
        assert_eq!(what(&orphan), "AMB-TC-55");
    }

    /// Which family a ref comes from is the event's to say.
    #[test]
    fn a_decision_is_named_as_a_decision() {
        assert_eq!(what(&happened(name::DECISION_ACCEPTED, 9, Some(1), "ai")), "AMB-D-9");
        assert_eq!(what(&happened(name::TASK_DONE, 9, Some(1), "ai")), "AMB-T-9");
    }

    /// A store with a project that reports two events through one Slack target.
    fn store_that_reports() -> (Store, i64, i64) {
        use crate::config::Paths;
        use crate::model::{NotifyKind, View};
        use crate::ops::project::NewProject;

        let base = amenbo_scratch::scratch("notify-dispatch");
        let mut store = Store::open_at(Paths::at(base)).unwrap();
        let project = store
            .project_add(NewProject {
                name: "shelf".to_string(),
                view: View::Board,
                notes: String::new(),
                color: None,
            })
            .unwrap();
        let target = store.notify_target_add(NotifyKind::Slack, "team").unwrap();
        store.project_notify_set_enabled(project.id, true).unwrap();
        store.project_notify_select_target(project.id, target.id).unwrap();
        for event in crate::lifecycle::V1_EVENTS {
            if event != name::STORE_CHANGED {
                store.project_notify_set_event(project.id, event, true).unwrap();
            }
        }
        (store, project.id, target.id)
    }

    /// **A burst is one message.** Ten writes in a row reach a channel as ten lines under one heading, in
    /// the order they happened.
    #[test]
    fn a_burst_becomes_one_message_per_target() {
        let (store, project, target) = store_that_reports();
        let seen = vec![
            happened(name::TASK_CREATED, 1, Some(project), "ai"),
            happened(name::TASK_DONE, 1, Some(project), "ai"),
        ];
        let messages = messages(&store, &seen).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].target_id, target);
        assert_eq!(messages[0].project, "shelf");
        assert_eq!(messages[0].lines.len(), 2);
        assert!(messages[0].lines[0].contains("AMB-T-1"), "{:?}", messages[0].lines);
    }

    /// A project that reports through two targets gets one message each — which is what keeps a channel
    /// and an inbox from being one setting.
    #[test]
    fn two_targets_are_two_messages_of_the_same_burst() {
        let (mut store, project, slack) = store_that_reports();
        let inbox = store.notify_target_add(crate::model::NotifyKind::Mail, "inbox").unwrap();
        store.project_notify_select_target(project, inbox.id).unwrap();

        let seen = vec![happened(name::TASK_CREATED, 1, Some(project), "ai")];
        let messages = messages(&store, &seen).unwrap();
        let mut carried: Vec<i64> = messages.iter().map(|m| m.target_id).collect();
        carried.sort_unstable();
        assert_eq!(carried, vec![slack.min(inbox.id), slack.max(inbox.id)]);
        assert!(messages.iter().all(|m| m.lines.len() == 1));
    }

    /// Off keeps the targets and the events where they are, and reports nothing (`AMB-D-885`).
    #[test]
    fn a_project_switched_off_reports_nothing() {
        let (mut store, project, _) = store_that_reports();
        store.project_notify_set_enabled(project, false).unwrap();
        let seen = vec![happened(name::TASK_CREATED, 1, Some(project), "ai")];
        assert!(messages(&store, &seen).unwrap().is_empty());
    }

    /// An event the project ticked off is not reported, and the rest of the burst still is.
    #[test]
    fn an_event_nobody_asked_for_is_left_out() {
        let (mut store, project, _) = store_that_reports();
        store.project_notify_set_event(project, name::TASK_CREATED, false).unwrap();
        let seen = vec![
            happened(name::TASK_CREATED, 1, Some(project), "ai"),
            happened(name::TASK_DONE, 2, Some(project), "ai"),
        ];
        let messages = messages(&store, &seen).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].lines.len(), 1);
        assert!(messages[0].lines[0].contains("AMB-T-2"));
    }

    /// A record in no project is reported to nobody: a notification is a project's setting.
    #[test]
    fn an_event_in_no_project_reaches_nobody() {
        let (store, _, _) = store_that_reports();
        let seen = vec![happened(name::TASK_CREATED, 1, None, "ai")];
        assert!(messages(&store, &seen).unwrap().is_empty());
    }

    /// What a person did themselves is not reported back to them (`AMB-D-416`), even where the project
    /// reports that event.
    #[test]
    fn a_persons_own_writes_make_no_message() {
        let (store, project, _) = store_that_reports();
        let seen = vec![happened(name::TASK_CREATED, 1, Some(project), "human")];
        assert!(messages(&store, &seen).unwrap().is_empty());
    }

    /// **A pass is a mouthful and not a backlog.** A store that has just been restored can have far more
    /// standing than anybody wants posted at once.
    #[test]
    fn more_than_a_pass_carries_is_dropped() {
        let (store, project, _) = store_that_reports();
        let seen: Vec<Happened> = (1..=(PASS_LIMIT as i64 + 50))
            .map(|id| happened(name::TASK_CREATED, id, Some(project), "ai"))
            .collect();
        let messages = messages(&store, &seen).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].lines.len(), PASS_LIMIT);
    }

    /// **A message nobody could take is written down.** A sender is a detached process with no stdio, so
    /// this line is the only trace a person chasing "why did nothing arrive" can find (`AMB-D-352`).
    #[test]
    fn a_refused_message_leaves_a_line_in_the_execution_log() {
        let (store, project, target) = store_that_reports();
        // The target holds no webhook, so nothing can go through it — which is the ordinary shape of a
        // connection that was never finished.
        let message = Message {
            project_id: project,
            project: "shelf".into(),
            target_id: target,
            lines: vec!["AI created AMB-T-1".into()],
            language: "en".into(),
            one_says: "created AMB-T-1".into(),
        };
        store.send_notifications(&[message]);

        let lines = crate::delivery_log::read(&store.paths.delivery_log_file());
        let refused: Vec<_> = lines
            .iter()
            .filter(|l| l.outcome == crate::delivery_log::Outcome::Refused)
            .collect();
        assert_eq!(refused.len(), 1, "{lines:?}");
        assert!(refused[0].why.contains(&target.to_string()), "{:?}", refused[0].why);
    }

    /// **The whole chain, from a write to a line in the log.** The pieces are tested apart above; this is
    /// the one that says they are joined — that a drive walks the outbox, words what it saw, and posts it.
    ///
    /// The flush is the mount used because it posts here rather than starting a process: what a detached
    /// sender did is exactly what a test cannot see.
    #[test]
    fn a_write_reaches_the_shelf_through_the_drive() {
        use crate::model::ActorKind;
        use crate::ops::task::NewTask;
        use crate::outbox_drive::Face;

        let (mut store, project, target) = store_that_reports();
        let task = store
            .add_task(NewTask {
                title: "見つかるはず".to_string(),
                project_id: Some(project),
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: Some(ActorKind::Ai),
                at_binding_id: None,
            })
            .unwrap();
        // A task is born when its creation ends, not when the row appears (`AMB-D-557`) — that is the
        // moment `task.created` reaches anybody.
        store.finish_task_creation(task.id, ActorKind::Ai).unwrap();

        let walked = store.flush_delivery(Face::Cli).unwrap();
        assert!(!walked.seen.is_empty(), "the drive walked nothing");

        // The target holds no webhook, so the post is refused — and a refusal is the trace that says the
        // message was built, addressed and handed over.
        let refused: Vec<_> = crate::delivery_log::read(&store.paths.delivery_log_file())
            .into_iter()
            .filter(|l| l.outcome == crate::delivery_log::Outcome::Refused)
            .collect();
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert!(refused[0].why.contains(&target.to_string()), "{:?}", refused[0].why);
    }

    /// A status is said the way Amenbo says it; a project's slug is the store's own value and passes
    /// through untouched.
    #[test]
    fn the_second_thing_is_translated_only_where_it_is_amenbos_word() {
        let mut moved = happened(name::TASK_STATUS_CHANGED, 1, Some(1), "ai");
        moved.new_state = Some("done".into());
        assert_eq!(state("ja", &moved).as_deref(), Some("完了"));

        let mut rehomed = happened(name::TASK_MOVED, 1, Some(1), "ai");
        rehomed.new_state = Some("other-project".into());
        assert_eq!(state("ja", &rehomed).as_deref(), Some("other-project"));

        // No second thing is not a blank to fill in: the bare sentence is said instead.
        assert_eq!(state("ja", &happened(name::TASK_DONE, 1, Some(1), "ai")), None);
    }
}
