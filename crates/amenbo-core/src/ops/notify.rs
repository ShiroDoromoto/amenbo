//! **Notifications**: the connections this device can send through, and what each project sends
//! (`AMB-D-885`).
//!
//! Mail and Slack are one feature here, and the thing that makes them one is where the connection
//! lives. A [`NotifyTarget`] sits on the **device** under a name, and a project **selects** from the
//! shelf — it never holds a connection of its own, so a webhook that changes is one edit rather than one
//! per project, and reading where a project's notifications go takes one screen.
//!
//! Three tables hang off the project and are written here: the row itself
//! ([`crate::model::ProjectNotify`] — the switch and the mail address), the targets it is carried by,
//! and the events it reports. Each is `CASCADE` from the project, so a project's delete takes all three
//! without this module being called; nothing here is a row a person points at the way a comment is.
//!
//! **No credential passes through this module.** A Slack target's webhook URL and a mail target's SMTP
//! password are [`crate::ops::secret`] rows under `(None, SecretArea::Notify, Some(target), field)` —
//! the table no road out of the store walks — and [`delete_target`] is what sweeps them, `owner_id`
//! being polymorphic and reachable by no constraint.

use crate::error::{Error, ErrorCode, Result};
use crate::model::{
    NotifyKind, NotifyTarget, ProjectNotify, ProjectNotifyEvent, ProjectNotifyTarget, SecretArea,
};
use crate::ops::{emit_create, emit_update, Noun};
use crate::lifecycle::{name, V1_EVENTS};
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// The word for a target. The code is the family's rather than one of its own: a finer code buys a
/// sentence in nineteen languages, and what this refusal reaches today is an operator reading the
/// English it already carries.
pub(crate) const NOUN: Noun =
    Noun { en: "notification target", code: ErrorCode::NotFound };

/// **What a project reports before anybody touches it** (`AMB-D-714`): the four writes, and the two days.
///
/// The two due events are here because a day that passed leaves nothing on a screen to catch up on — a
/// write can be read next time the board is opened, and a deadline cannot. Left off by default, almost
/// nobody would ever receive them.
pub const EVENTS_A_PROJECT_STARTS_WITH: &[&str] = &[
    name::TASK_CREATED,
    name::TASK_STATUS_CHANGED,
    name::TASK_DONE,
    name::TASK_REJECTED,
    name::TASK_DUE,
    name::TASK_DUE_TOMORROW,
];

/// Is this one of the thirteen a project may report? Every name in the catalog except
/// [`name::STORE_CHANGED`], which says only that *something* moved — a signal for a mirror to re-read
/// on, and nothing a person can be told (`AMB-D-582`).
pub fn is_reportable(event: &str) -> bool {
    event != name::STORE_CHANGED && V1_EVENTS.contains(&event)
}

fn checked_event(event: &str) -> Result<()> {
    if !is_reportable(event) {
        return Err(Error::invalid(format!(
            "'{event}' is not an event a project can report"
        )));
    }
    Ok(())
}

fn checked_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(Error::invalid("a notification target needs a name"));
    }
    Ok(trimmed.to_string())
}

/// Read a live target's `before` snapshot **from this transaction**.
fn live_target(tx: &WriteTx<'_>, id: i64) -> Result<NotifyTarget> {
    read::notify_target(tx.conn(), id)?.ok_or_else(|| NOUN.not_found(id.to_string()))
}

// ───────────────────────────── The device's shelf ─────────────────────────────

/// Add a target to the device's shelf. The connection is written afterwards — the mail fields through
/// [`set_mail_connection`], the credential through [`crate::ops::secret::set`] — so a target exists
/// before anything secret is asked for, which is what gives that secret a row to hang off.
///
/// **The first target ever raised carries the default mark**, there being nothing else it could point
/// at; after that the mark is moved deliberately ([`set_default`]).
pub fn add_target(tx: &WriteTx<'_>, kind: NotifyKind, name: &str) -> Result<NotifyTarget> {
    let name = checked_name(name)?;
    let now = Timestamp::now();
    let target = NotifyTarget {
        id: read::next_id(tx.conn(), "notify_target")?,
        kind,
        name,
        is_default: read::default_notify_target_id(tx.conn())?.is_none(),
        // A Slack target never grows these; a mail one is filled in by `set_mail_connection`.
        smtp_host: None,
        smtp_port: None,
        smtp_user: None,
        mail_from: None,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::notify_target(&target))?;
    Ok(target)
}

/// Rename a target. The name is the target's whole identity on a project's screen, so it is the one
/// field both kinds have and the only one this door writes.
pub fn rename_target(tx: &WriteTx<'_>, id: i64, name: &str) -> Result<NotifyTarget> {
    let name = checked_name(name)?;
    let before = live_target(tx, id)?;
    let after = NotifyTarget { name, updated_at: Timestamp::now(), ..before.clone() };
    emit_update(tx, record::notify_target(&before), record::notify_target(&after))?;
    Ok(after)
}

/// The parts of a mail target's connection that are not the password — the form is filled in and saved
/// whole, so they are written whole. The password is [`crate::ops::secret`]'s.
#[derive(Clone, Debug, Default)]
pub struct MailConnection {
    /// The relay the message is handed to.
    pub smtp_host: Option<String>,
    /// The port it listens on (587 nearly everywhere).
    pub smtp_port: Option<i64>,
    /// The account to authenticate as, in full. `None` where the relay asks for none.
    pub smtp_user: Option<String>,
    /// The address the message is sent from. `None` falls back to the account.
    pub mail_from: Option<String>,
}

/// Write a mail target's connection. **Refused on a Slack target**, whose whole connection is the
/// webhook URL and so is a secret — the columns exist on the shared table, and refusing here is what
/// keeps them from holding a value that means nothing.
pub fn set_mail_connection(
    tx: &WriteTx<'_>,
    id: i64,
    conn: MailConnection,
) -> Result<NotifyTarget> {
    let before = live_target(tx, id)?;
    if before.kind != NotifyKind::Mail {
        return Err(Error::invalid(format!(
            "'{}' sends through Slack, which has no SMTP connection",
            before.name
        )));
    }
    let after = NotifyTarget {
        smtp_host: conn.smtp_host,
        smtp_port: conn.smtp_port,
        smtp_user: conn.smtp_user,
        mail_from: conn.mail_from,
        updated_at: Timestamp::now(),
        ..before.clone()
    };
    emit_update(tx, record::notify_target(&before), record::notify_target(&after))?;
    Ok(after)
}

/// Move the default mark onto this target — where a **newly created** project starts out pointing
/// (`AMB-D-885`). It changes nothing about the projects already standing: each of those answers with
/// its own selection, which is what keeps the mark from becoming a tier.
///
/// The mark is moved, not added: every other target carrying it is cleared in the same transaction, so
/// "at most one" holds without a constraint that could say it.
pub fn set_default(tx: &WriteTx<'_>, id: i64) -> Result<NotifyTarget> {
    let before = live_target(tx, id)?;
    for other in read::other_default_notify_target_ids(tx.conn(), id)? {
        let was = live_target(tx, other)?;
        let now = NotifyTarget { is_default: false, updated_at: Timestamp::now(), ..was.clone() };
        emit_update(tx, record::notify_target(&was), record::notify_target(&now))?;
    }
    if before.is_default {
        return Ok(before);
    }
    let after = NotifyTarget { is_default: true, updated_at: Timestamp::now(), ..before.clone() };
    emit_update(tx, record::notify_target(&before), record::notify_target(&after))?;
    Ok(after)
}

/// Which projects this target carries the notifications of — what a screen puts in front of a delete
/// ("two projects use this, and both lose it"). Read before anything goes, since after the delete
/// nobody can be asked.
pub fn projects_using(tx: &WriteTx<'_>, id: i64) -> Result<Vec<i64>> {
    Ok(read::projects_using_notify_target(tx.conn(), id)?)
}

/// Delete a target, and with it every project's selection of it and every credential it held. Returns
/// the projects that lost it, in id order — the same list [`projects_using`] answers with, read here
/// before the rows go so a caller need not ask twice.
///
/// Three things go in one transaction, and the order is the schema's: the selections first, because
/// `project_notify_target.target_id` is `RESTRICT` and the target's own delete would be stopped by any
/// one of them left standing; then the secrets, which no constraint reaches at all (`owner_id` is
/// polymorphic — which table it names is the area's to say); then the row.
pub fn delete_target(tx: &WriteTx<'_>, id: i64) -> Result<Vec<i64>> {
    let target = live_target(tx, id)?;
    let projects = read::projects_using_notify_target(tx.conn(), id)?;
    for link in read::project_notify_target_ids_for_target(tx.conn(), id)? {
        tx.delete_record("project_notify_target", link)?;
    }
    crate::ops::secret::forget_owner(tx, SecretArea::Notify, Some(id))?;
    tx.delete_record("notify_target", target.id)?;
    Ok(projects)
}

// ───────────────────────────── One project's row ─────────────────────────────

/// Give a project its notification row — called once, when the project is created
/// ([`crate::ops::project::add`]).
///
/// It is materialised at birth rather than read as a fallback later, and that is the whole of
/// `AMB-D-885`'s "default": the mark says where a project **starts**, and from that moment the
/// project's own rows are the entire answer. Were the defaults read at send time instead, marking a
/// different target tomorrow would silently redirect every project that had never been touched.
pub fn init_project(tx: &WriteTx<'_>, project_id: i64) -> Result<ProjectNotify> {
    let row = ensure_row(tx, project_id)?;
    for event in EVENTS_A_PROJECT_STARTS_WITH {
        set_event(tx, project_id, event, true)?;
    }
    if let Some(target) = read::default_notify_target_id(tx.conn())? {
        select_target(tx, project_id, target)?;
    }
    Ok(row)
}

/// This project's notification row, created if it is not there yet. A row born here is **on** with no
/// targets and no events, which sends nothing — the settings a project starts with are
/// [`init_project`]'s, and a row raised by a later write is one whose project predates the feature.
fn ensure_row(tx: &WriteTx<'_>, project_id: i64) -> Result<ProjectNotify> {
    if let Some(id) = read::project_notify_row_id(tx.conn(), project_id)? {
        return Ok(read::project_notify_row_by_id(tx.conn(), id)?
            .expect("the row id was just read from the same transaction"));
    }
    if read::project(tx.conn(), project_id)?.is_none() {
        return Err(crate::ops::project::NOUN.not_found(project_id.to_string()));
    }
    let now = Timestamp::now();
    let row = ProjectNotify {
        id: read::next_id(tx.conn(), "project_notify")?,
        project_id,
        // On, because the switch is here to *stop* notifications; with nothing selected it sends
        // nothing anyway, and a project that has chosen a target has said what it wants.
        enabled: true,
        mail_to: String::new(),
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::project_notify(&row))?;
    Ok(row)
}

/// Turn this project's notifications on or off. Off keeps the targets and the events where they are —
/// the two are apart so that a fortnight away costs one switch rather than a settings screen rebuilt on
/// the way back. Returns whether anything changed.
pub fn set_enabled(tx: &WriteTx<'_>, project_id: i64, enabled: bool) -> Result<bool> {
    let before = ensure_row(tx, project_id)?;
    if before.enabled == enabled {
        return Ok(false);
    }
    let after = ProjectNotify { enabled, updated_at: Timestamp::now(), ..before.clone() };
    emit_update(tx, record::project_notify(&before), record::project_notify(&after))?;
    Ok(true)
}

/// Where this project's mail is addressed — several addresses on one line, separated by commas, as the
/// person typed them. Empty falls back to the mail target's own account. Returns whether anything
/// changed.
///
/// It is the project's and not the target's because it answers *who is told*, while the target answers
/// what carries it — so it is not the same setting kept in two tiers (`AMB-D-434`), there being no
/// second copy of it anywhere.
pub fn set_mail_to(tx: &WriteTx<'_>, project_id: i64, mail_to: &str) -> Result<bool> {
    let before = ensure_row(tx, project_id)?;
    if before.mail_to == mail_to {
        return Ok(false);
    }
    let after =
        ProjectNotify { mail_to: mail_to.to_string(), updated_at: Timestamp::now(), ..before.clone() };
    emit_update(tx, record::project_notify(&before), record::project_notify(&after))?;
    Ok(true)
}

/// Send this project's notifications through one more target. Idempotent: selecting the same target
/// twice is a noop. Returns (row, created).
pub fn select_target(
    tx: &WriteTx<'_>,
    project_id: i64,
    target_id: i64,
) -> Result<(ProjectNotifyTarget, bool)> {
    ensure_row(tx, project_id)?;
    live_target(tx, target_id)?;
    if let Some(id) = read::project_notify_target_id(tx.conn(), project_id, target_id)? {
        let existing = read::project_notify_targets(tx.conn(), project_id)?
            .into_iter()
            .find(|r| r.id == id)
            .expect("the row id was just read from the same transaction");
        return Ok((existing, false));
    }
    let now = Timestamp::now();
    let row = ProjectNotifyTarget {
        id: read::next_id(tx.conn(), "project_notify_target")?,
        project_id,
        target_id,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::project_notify_target(&row))?;
    Ok((row, true))
}

/// Stop sending this project's notifications through one target. A noop when it was not selected.
/// Returns whether anything changed. The target itself is untouched — it stays on the shelf for the
/// other projects that chose it.
pub fn deselect_target(tx: &WriteTx<'_>, project_id: i64, target_id: i64) -> Result<bool> {
    let Some(id) = read::project_notify_target_id(tx.conn(), project_id, target_id)? else {
        return Ok(false);
    };
    tx.delete_record("project_notify_target", id)?;
    Ok(true)
}

/// Tick (`true`) or untick (`false`) one of the thirteen events this project reports. Idempotent; an
/// unknown name — `store.changed` among them — is refused rather than stored. Returns whether anything
/// changed.
///
/// **Unticking them all is an answer**, and it is kept: the project's row is what says the project has
/// been set up, so a project reporting nothing stays on and reports nothing.
pub fn set_event(tx: &WriteTx<'_>, project_id: i64, event: &str, on: bool) -> Result<bool> {
    checked_event(event)?;
    ensure_row(tx, project_id)?;
    let existing = read::project_notify_event_id(tx.conn(), project_id, event)?;
    match (existing, on) {
        (Some(_), true) | (None, false) => Ok(false),
        (Some(id), false) => {
            tx.delete_record("project_notify_event", id)?;
            Ok(true)
        }
        (None, true) => {
            let now = Timestamp::now();
            let row = ProjectNotifyEvent {
                id: read::next_id(tx.conn(), "project_notify_event")?,
                project_id,
                event: event.to_string(),
                created_at: now,
                updated_at: now,
            };
            emit_create(tx, record::project_notify_event(&row))?;
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{mk_project, with_tx};

    fn mail(tx: &WriteTx<'_>, name: &str) -> i64 {
        add_target(tx, NotifyKind::Mail, name).unwrap().id
    }

    fn slack(tx: &WriteTx<'_>, name: &str) -> i64 {
        add_target(tx, NotifyKind::Slack, name).unwrap().id
    }

    /// The shelf is the device's: a target is raised once, under a name, and carries no project.
    #[test]
    fn the_first_target_raised_is_the_one_a_new_project_points_at() {
        with_tx(|tx| {
            let first = slack(tx, "開発チーム");
            let second = slack(tx, "個人メモ");
            assert!(read::notify_target(tx.conn(), first).unwrap().unwrap().is_default);
            assert!(!read::notify_target(tx.conn(), second).unwrap().unwrap().is_default);

            // The mark moves rather than accumulating.
            set_default(tx, second).unwrap();
            assert!(!read::notify_target(tx.conn(), first).unwrap().unwrap().is_default);
            assert_eq!(read::default_notify_target_id(tx.conn()).unwrap(), Some(second));
        });
    }

    /// A project is born pointing at the marked target and reporting the six (`AMB-D-714`).
    #[test]
    fn a_new_project_starts_at_the_default_target_with_the_six_events() {
        with_tx(|tx| {
            let target = slack(tx, "開発チーム");
            let p = mk_project(tx, "通知のあるPJ");

            let row = read::project_notify(tx.conn(), p).unwrap().unwrap();
            assert!(row.enabled);
            assert_eq!(row.mail_to, "");
            let picked: Vec<i64> = read::project_notify_targets(tx.conn(), p)
                .unwrap()
                .into_iter()
                .map(|r| r.target_id)
                .collect();
            assert_eq!(picked, vec![target]);
            let events: Vec<String> = read::project_notify_events(tx.conn(), p)
                .unwrap()
                .into_iter()
                .map(|r| r.event)
                .collect();
            assert_eq!(events, EVENTS_A_PROJECT_STARTS_WITH.to_vec());
        });
    }

    /// Marking a different target later leaves the projects already standing where they were — the mark
    /// says where a project starts, and is not a tier anything falls back to.
    #[test]
    fn moving_the_default_leaves_the_projects_already_standing_alone() {
        with_tx(|tx| {
            let first = slack(tx, "開発チーム");
            let p = mk_project(tx, "先にできたPJ");

            let second = slack(tx, "別チーム");
            set_default(tx, second).unwrap();

            let picked: Vec<i64> = read::project_notify_targets(tx.conn(), p)
                .unwrap()
                .into_iter()
                .map(|r| r.target_id)
                .collect();
            assert_eq!(picked, vec![first]);
        });
    }

    /// One project reports to Slack and to mail at once — the selection is a set, which is what makes
    /// the two carriers one feature (`AMB-D-885`).
    #[test]
    fn a_project_sends_through_as_many_targets_as_it_likes() {
        with_tx(|tx| {
            // The project is raised before the shelf, so it starts out selecting nothing.
            let p = mk_project(tx, "両方に送るPJ");
            let chat = slack(tx, "開発チーム");
            let inbox = mail(tx, "自分のメール");

            assert!(select_target(tx, p, chat).unwrap().1);
            assert!(select_target(tx, p, inbox).unwrap().1);
            assert!(!select_target(tx, p, chat).unwrap().1, "selecting the same target again is a noop");

            let picked: Vec<i64> = read::project_notify_targets(tx.conn(), p)
                .unwrap()
                .into_iter()
                .map(|r| r.target_id)
                .collect();
            assert_eq!(picked, vec![chat, inbox]);

            assert!(deselect_target(tx, p, chat).unwrap());
            assert!(!deselect_target(tx, p, chat).unwrap());
            assert!(read::notify_target(tx.conn(), chat).unwrap().is_some(), "the target stays on the shelf");
        });
    }

    /// Switching off keeps the settings standing, which is the whole reason the switch is separate from
    /// the selection.
    #[test]
    fn off_keeps_the_targets_and_the_events() {
        with_tx(|tx| {
            let target = slack(tx, "開発チーム");
            let p = mk_project(tx, "休むPJ");

            assert!(set_enabled(tx, p, false).unwrap());
            assert!(!set_enabled(tx, p, false).unwrap());
            assert!(!read::project_notify(tx.conn(), p).unwrap().unwrap().enabled);
            assert_eq!(read::project_notify_targets(tx.conn(), p).unwrap().len(), 1);
            assert_eq!(read::project_notify_events(tx.conn(), p).unwrap().len(), 6);
            assert_eq!(read::default_notify_target_id(tx.conn()).unwrap(), Some(target));
        });
    }

    /// Reporting nothing is an answer a person can give, and it is not the same as never having been
    /// set up: the project's row is still there and still on.
    #[test]
    fn a_project_that_reports_nothing_stays_a_project_that_is_set_up() {
        with_tx(|tx| {
            let p = mk_project(tx, "何も報告しないPJ");
            for event in EVENTS_A_PROJECT_STARTS_WITH {
                assert!(set_event(tx, p, event, false).unwrap());
            }
            assert!(read::project_notify_events(tx.conn(), p).unwrap().is_empty());
            assert!(read::project_notify(tx.conn(), p).unwrap().unwrap().enabled);
        });
    }

    /// The ledger's own signal is not something a person is told, so it cannot be subscribed to; nor can
    /// a name the catalog does not carry.
    #[test]
    fn the_store_changed_signal_is_not_an_event_a_project_reports() {
        with_tx(|tx| {
            let p = mk_project(tx, "PJ");
            set_event(tx, p, name::STORE_CHANGED, true).unwrap_err();
            set_event(tx, p, "task.renamed", true).unwrap_err();
            let events: Vec<String> = read::project_notify_events(tx.conn(), p)
                .unwrap()
                .into_iter()
                .map(|r| r.event)
                .collect();
            assert_eq!(events, EVENTS_A_PROJECT_STARTS_WITH.to_vec(), "the six it was born with, and no more");
        });
    }

    /// A Slack target has no SMTP connection at all — its whole connection is the webhook, which is a
    /// secret and never reaches this table.
    #[test]
    fn only_a_mail_target_takes_an_smtp_connection() {
        with_tx(|tx| {
            let inbox = mail(tx, "自分のメール");
            let chat = slack(tx, "開発チーム");
            let conn = MailConnection {
                smtp_host: Some("smtp.example.com".into()),
                smtp_port: Some(587),
                smtp_user: Some("alice@example.com".into()),
                mail_from: None,
            };
            set_mail_connection(tx, inbox, conn.clone()).unwrap();
            set_mail_connection(tx, chat, conn).unwrap_err();

            let row = read::notify_target(tx.conn(), inbox).unwrap().unwrap();
            assert_eq!(row.smtp_host.as_deref(), Some("smtp.example.com"));
            assert_eq!(row.smtp_port, Some(587));
            assert_eq!(read::notify_target(tx.conn(), chat).unwrap().unwrap().smtp_host, None);
        });
    }

    /// Deleting a target says which projects lose it, then takes the selections that hold it and the
    /// credential it kept. Leave either behind and the row cannot go — `RESTRICT` on the one, and
    /// nothing at all on the other.
    #[test]
    fn deleting_a_target_takes_its_selections_and_its_secret_with_it() {
        with_tx(|tx| {
            let chat = slack(tx, "開発チーム");
            let one = mk_project(tx, "PJ1");
            let two = mk_project(tx, "PJ2");
            select_target(tx, one, chat).unwrap();
            select_target(tx, two, chat).unwrap();
            crate::ops::secret::set(
                tx,
                None,
                SecretArea::Notify,
                Some(chat),
                NotifyTarget::SLACK_WEBHOOK_URL,
                Some("https://hooks.slack.com/services/T/B/x"),
            )
            .unwrap();

            assert_eq!(projects_using(tx, chat).unwrap(), vec![one, two]);
            assert_eq!(delete_target(tx, chat).unwrap(), vec![one, two]);

            assert!(read::notify_target(tx.conn(), chat).unwrap().is_none());
            assert!(read::project_notify_targets(tx.conn(), one).unwrap().is_empty());
            assert!(read::project_notify_targets(tx.conn(), two).unwrap().is_empty());
            assert_eq!(
                read::secret_value(
                    tx.conn(),
                    None,
                    SecretArea::Notify,
                    Some(chat),
                    NotifyTarget::SLACK_WEBHOOK_URL,
                )
                .unwrap(),
                None,
            );
        });
    }

    /// The thirteen this module admits are the catalog's own, minus the one signal that says nothing
    /// about what happened — so the column's `CHECK` and `plugin_payload` cannot drift apart.
    #[test]
    fn the_reportable_events_are_the_catalog_minus_the_ledgers_signal() {
        let reportable: Vec<&str> =
            V1_EVENTS.iter().copied().filter(|e| is_reportable(e)).collect();
        assert_eq!(reportable.len(), 13);
        assert!(!reportable.contains(&name::STORE_CHANGED));
        for event in EVENTS_A_PROJECT_STARTS_WITH {
            assert!(reportable.contains(event));
        }
    }
}
