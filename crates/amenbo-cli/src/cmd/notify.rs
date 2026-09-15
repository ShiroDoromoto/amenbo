//! `notify`: the device's shelf of connections, and what this project reports through it (`AMB-D-885`).
//!
//! **The shelf is the device's and the selection is the project's.** A connection is written once under a
//! name and every project that wants it selects it, so a webhook that changes is one edit — which is why
//! the two halves are one command group with two vocabularies rather than two groups: what a person is
//! doing is one thing, and where each half is kept is the decision, not the spelling.
//!
//! **No credential is ever read back.** A row says whether one is held and nothing more, and the one place
//! a value goes in takes it from stdin — a webhook URL or a password on the command line is visible in the
//! process list and lands in shell history.

use std::io::IsTerminal as _;

use serde_json::json;

use amenbo_core::config::Paths;
use amenbo_core::model::{NotifyKind, NotifyTarget};
use amenbo_core::Store;

use crate::cli::{NotifyCmd, NotifyTargetCmd};
use crate::cmd::place::bound_project;
use crate::output::{confirm, human, print_json, write_envelope, CliError, Flags};

pub(crate) fn notify(
    store: &mut Store,
    flags: &Flags,
    sub: Option<NotifyCmd>,
) -> Result<i32, CliError> {
    match sub {
        None => show(store, flags),
        Some(NotifyCmd::Target { sub }) => target(store, flags, sub),
        Some(NotifyCmd::On) => switch(store, flags, true),
        Some(NotifyCmd::Off) => switch(store, flags, false),
        Some(NotifyCmd::Use { target }) => select(store, flags, target, true),
        Some(NotifyCmd::Unuse { target }) => select(store, flags, target, false),
        Some(NotifyCmd::To { addresses }) => addressed(store, flags, &addresses),
        Some(NotifyCmd::Event { name, off }) => event(store, flags, &name, !off),
    }
}

/// Both halves at once: the shelf, and what the bound project does with it.
///
/// It is one answer rather than two commands because that is the question — *where do this project's
/// notifications go* — and reading it in two places is what `AMB-D-885` turned the per-project connection
/// down to avoid.
fn show(store: &Store, flags: &Flags) -> Result<i32, CliError> {
    let shelf = shelf_json(store)?;
    let project = bound_project(store);
    let mine = match project {
        Some(id) => project_json(store, id)?,
        None => serde_json::Value::Null,
    };
    let value = json!({ "targets": shelf, "project": mine });
    if flags.json {
        print_json(&value);
        return Ok(0);
    }
    if shelf.as_array().is_some_and(|rows| rows.is_empty()) {
        human(flags, format!("No notification targets yet. Raise one with `{} notify target add --kind slack <name>`.", Paths::command_name()));
    }
    for row in shelf.as_array().into_iter().flatten() {
        human(flags, format!("  {}", target_line(row)));
    }
    match &mine {
        serde_json::Value::Null => {
            human(flags, "This folder is not bound to a project, so there is nothing it reports.");
        }
        mine => {
            human(
                flags,
                format!(
                    "This project: {}, through {}, reporting {}",
                    if mine["enabled"].as_bool() == Some(true) { "on" } else { "off" },
                    joined(&mine["targets"]),
                    joined(&mine["events"]),
                ),
            );
            if let Some(to) = mine["mail_to"].as_str().filter(|to| !to.is_empty()) {
                human(flags, format!("Mail is addressed to: {to}"));
            }
        }
    }
    Ok(0)
}

fn target(store: &mut Store, flags: &Flags, sub: NotifyTargetCmd) -> Result<i32, CliError> {
    match sub {
        NotifyTargetCmd::List => {
            let shelf = shelf_json(store)?;
            if flags.json {
                print_json(&json!({ "targets": shelf }));
                return Ok(0);
            }
            for row in shelf.as_array().into_iter().flatten() {
                human(flags, target_line(row));
            }
            Ok(0)
        }
        NotifyTargetCmd::Add { kind, name } => {
            let kind = NotifyKind::parse(&kind).ok_or_else(|| {
                CliError::from(amenbo_core::Error::invalid(format!(
                    "'{kind}' is not a kind a target can be (slack, mail)"
                )))
            })?;
            let raised = store.notify_target_add(kind, &name).map_err(CliError::from)?;
            write_envelope(
                flags,
                "notify.target.add",
                "target",
                one_json(store, &raised)?,
                None,
                false,
                format!("Raised notification target {} ({})", raised.id, raised.name),
            );
            Ok(0)
        }
        NotifyTargetCmd::Set {
            target,
            name,
            smtp_host,
            smtp_port,
            smtp_user,
            mail_from,
            secret,
        } => {
            let before = live(store, target)?;
            let mut changed: Vec<String> = Vec::new();
            if let Some(name) = &name {
                store.notify_target_rename(target, name).map_err(CliError::from)?;
                changed.push("name".to_string());
            }
            // The four are written whole, as the form that fills them in is: a connection half-written is
            // one nobody can send on, and reading the row back to patch one field would make a save depend
            // on what somebody else had just written.
            if before.kind == NotifyKind::Mail
                && [&smtp_host, &smtp_user, &mail_from].iter().any(|v| v.is_some())
                || smtp_port.is_some()
            {
                store
                    .notify_target_set_mail_connection(
                        target,
                        amenbo_core::ops::notify::MailConnection {
                            smtp_host: given(smtp_host, before.smtp_host.clone()),
                            smtp_port: smtp_port.or(before.smtp_port),
                            smtp_user: given(smtp_user, before.smtp_user.clone()),
                            mail_from: given(mail_from, before.mail_from.clone()),
                        },
                    )
                    .map_err(CliError::from)?;
                changed.push("connection".to_string());
            }
            if let Some(secret) = secret {
                let value = read_secret(secret)?;
                let write = (!value.is_empty()).then_some(value.as_str());
                store
                    .set_secret(
                        None,
                        amenbo_core::model::SecretArea::Notify,
                        Some(target),
                        secret_key(before.kind),
                        write,
                    )
                    .map_err(CliError::from)?;
                changed.push("secret".to_string());
            }
            let after = live(store, target)?;
            write_envelope(
                flags,
                "notify.target.set",
                "target",
                one_json(store, &after)?,
                Some(changed.clone()),
                changed.is_empty(),
                format!("Wrote notification target {target} ({})", after.name),
            );
            Ok(0)
        }
        NotifyTargetCmd::Default { target } => {
            let marked = store.notify_target_set_default(target).map_err(CliError::from)?;
            write_envelope(
                flags,
                "notify.target.default",
                "target",
                one_json(store, &marked)?,
                None,
                false,
                format!("New projects now start out pointing at {} ({})", marked.id, marked.name),
            );
            Ok(0)
        }
        NotifyTargetCmd::Rm { target } => {
            let row = live(store, target)?;
            // What the press costs is said before it is made: after the delete there is nobody left to ask
            // which projects were sending through it.
            let using = store.projects_using_notify_target(target).map_err(CliError::from)?.len();
            let what = match using {
                0 => format!("remove notification target {target} ({})", row.name),
                1 => format!("remove notification target {target} ({}), which 1 project sends through", row.name),
                n => format!("remove notification target {target} ({}), which {n} projects send through", row.name),
            };
            if !confirm(flags, &what)? {
                return Ok(1);
            }
            let lost = store.notify_target_delete(target).map_err(CliError::from)?;
            write_envelope(
                flags,
                "notify.target.rm",
                "target",
                json!({ "id": target, "name": row.name, "projects_lost": lost }),
                None,
                false,
                match lost.len() {
                    0 => format!("Removed notification target {target} ({})", row.name),
                    1 => format!("Removed notification target {target} ({}); 1 project lost it", row.name),
                    n => format!("Removed notification target {target} ({}); {n} projects lost it", row.name),
                },
            );
            Ok(0)
        }
        NotifyTargetCmd::Check { target } => {
            let row = live(store, target)?;
            let reached = match row.kind {
                NotifyKind::Slack => {
                    let webhook = amenbo_core::notify_slack::webhook_for(store, target)
                        .map_err(CliError::from)?;
                    amenbo_core::notify_slack::check(&webhook).map_err(CliError::from)?;
                    false
                }
                NotifyKind::Mail => {
                    let settings = amenbo_core::notify_mail::settings_for(store, target, None)
                        .map_err(CliError::from)?;
                    amenbo_core::notify_mail::check(&settings).map_err(CliError::from)?;
                    true
                }
            };
            let said = if reached {
                "The server accepted the account."
            } else {
                amenbo_core::notify_slack::SHAPE_IS_RIGHT
            };
            if flags.json {
                print_json(&json!({ "ok": true, "target": target, "reached": reached, "message": said }));
            } else {
                human(flags, said);
            }
            Ok(0)
        }
        NotifyTargetCmd::Test { target } => {
            let row = live(store, target)?;
            let language = store.config.language.clone().unwrap_or_else(|| "en".to_string());
            let said = amenbo_core::notify_wording::test_line(&language);
            match row.kind {
                NotifyKind::Slack => {
                    let webhook = amenbo_core::notify_slack::webhook_for(store, target)
                        .map_err(CliError::from)?;
                    amenbo_core::notify_slack::send_test(&webhook, "", &language)
                        .map_err(CliError::from)?;
                }
                NotifyKind::Mail => {
                    let settings = amenbo_core::notify_mail::settings_for(store, target, None)
                        .map_err(CliError::from)?;
                    let thread = amenbo_core::notify_mail::Thread::of(&settings, 0, target);
                    amenbo_core::notify_mail::send(&settings, &thread, "", &said, &said)
                        .map_err(CliError::from)?;
                }
            }
            if flags.json {
                print_json(&json!({ "ok": true, "target": target, "sent": true }));
            } else {
                human(flags, "It went out — look for it where this target sends.");
            }
            Ok(0)
        }
    }
}

fn switch(store: &mut Store, flags: &Flags, on: bool) -> Result<i32, CliError> {
    let project = here(store)?;
    let changed = store.project_notify_set_enabled(project, on).map_err(CliError::from)?;
    write_envelope(
        flags,
        if on { "notify.on" } else { "notify.off" },
        "project",
        project_json(store, project)?,
        None,
        !changed,
        if on { "This project now reports." } else { "This project reports nothing until it is switched on again." },
    );
    Ok(0)
}

fn select(store: &mut Store, flags: &Flags, target: i64, on: bool) -> Result<i32, CliError> {
    let project = here(store)?;
    let row = live(store, target)?;
    let changed = if on {
        store.project_notify_select_target(project, target).map_err(CliError::from)?
    } else {
        store.project_notify_deselect_target(project, target).map_err(CliError::from)?
    };
    write_envelope(
        flags,
        if on { "notify.use" } else { "notify.unuse" },
        "project",
        project_json(store, project)?,
        None,
        !changed,
        format!(
            "This project {} {} ({})",
            if on { "now sends through" } else { "no longer sends through" },
            row.id,
            row.name
        ),
    );
    Ok(0)
}

fn addressed(store: &mut Store, flags: &Flags, addresses: &str) -> Result<i32, CliError> {
    let project = here(store)?;
    let changed = store.project_notify_set_mail_to(project, addresses).map_err(CliError::from)?;
    write_envelope(
        flags,
        "notify.to",
        "project",
        project_json(store, project)?,
        None,
        !changed,
        if addresses.trim().is_empty() {
            "Mail now goes to the account each relay authenticates as.".to_string()
        } else {
            format!("Mail is now addressed to: {addresses}")
        },
    );
    Ok(0)
}

fn event(store: &mut Store, flags: &Flags, name: &str, on: bool) -> Result<i32, CliError> {
    let project = here(store)?;
    let changed = store.project_notify_set_event(project, name, on).map_err(CliError::from)?;
    write_envelope(
        flags,
        "notify.event",
        "project",
        project_json(store, project)?,
        None,
        !changed,
        format!("This project {} {name}", if on { "reports" } else { "no longer reports" }),
    );
    Ok(0)
}

/// The project this folder is bound to, or the refusal that says there is none. The shelf is the device's
/// and needs no project; everything a project selects does.
fn here(store: &Store) -> Result<i64, CliError> {
    bound_project(store).ok_or_else(|| {
        CliError::from(amenbo_core::Error::invalid(format!(
            "this folder is not bound to a project, so there is nothing here to report — bind it with `{} bind`",
            Paths::command_name()
        )))
    })
}

/// One target as it stands, or the refusal that says there is no such row.
fn live(store: &Store, id: i64) -> Result<NotifyTarget, CliError> {
    store
        .notify_target(id)
        .map_err(CliError::from)?
        .ok_or_else(|| CliError::from(amenbo_core::Error::not_found(format!("notification target {id}"))))
}

/// Which `secret` field a target of this kind keeps its credential under. The kind decides it, so nobody
/// passes a key in.
fn secret_key(kind: NotifyKind) -> &'static str {
    match kind {
        NotifyKind::Slack => NotifyTarget::SLACK_WEBHOOK_URL,
        NotifyKind::Mail => NotifyTarget::SMTP_PASSWORD,
    }
}

/// A field that was named, or what the row already held. A box left out of the command is a field nobody
/// touched, which is not the same as one emptied on purpose — that is `--field ""`.
fn given(named: Option<String>, held: Option<String>) -> Option<String> {
    match named {
        Some(value) if value.trim().is_empty() => None,
        Some(value) => Some(value.trim().to_string()),
        None => held,
    }
}

/// The credential, as given or read whole from stdin when it is `-`.
///
/// The stdin route is the point: a webhook URL and a password are both credentials, and one on argv is
/// visible in the process list and lands in shell history. The trailing newline a pipe adds is dropped and
/// nothing else.
fn read_secret(value: String) -> Result<String, CliError> {
    if value != "-" {
        return Ok(value);
    }
    if std::io::stdin().is_terminal() {
        return Err(CliError {
            code: "invalid_value",
            message: "`-` says the credential comes in on stdin, but stdin is a terminal".to_string(),
            hint: Some(format!(
                "Pipe it in (`… | {} notify target set <id> --secret -`).",
                Paths::command_name()
            )),
            exit: 2,
        });
    }
    use std::io::Read as _;
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).map_err(|e| CliError {
        code: "io_error",
        message: format!("Cannot read the credential from stdin: {e}"),
        hint: None,
        exit: 1,
    })?;
    Ok(s.strip_suffix('\n').map(|t| t.strip_suffix('\r').unwrap_or(t)).unwrap_or(&s).to_string())
}

/// The whole shelf, as the rows a face draws.
fn shelf_json(store: &Store) -> Result<serde_json::Value, CliError> {
    let rows: Result<Vec<serde_json::Value>, CliError> =
        store.notify_targets().map_err(CliError::from)?.iter().map(|t| one_json(store, t)).collect();
    Ok(json!(rows?))
}

/// One target, with the two things the row itself does not carry: whether the credential is held, and how
/// many projects have selected it. **Never the credential** — the value stays in core.
fn one_json(store: &Store, target: &NotifyTarget) -> Result<serde_json::Value, CliError> {
    let held = store
        .secret_value(
            None,
            amenbo_core::model::SecretArea::Notify,
            Some(target.id),
            secret_key(target.kind),
        )
        .map_err(CliError::from)?
        .is_some();
    Ok(json!({
        "id": target.id,
        "kind": target.kind.as_str(),
        "name": target.name,
        "is_default": target.is_default,
        "smtp_host": target.smtp_host,
        "smtp_port": target.smtp_port,
        "smtp_user": target.smtp_user,
        "mail_from": target.mail_from,
        "secret_set": held,
        "projects_using": store.projects_using_notify_target(target.id).map_err(CliError::from)?.len(),
    }))
}

/// What one project does with the shelf, and the events it may choose among — core's list rather than a
/// second copy here, so a name added there reaches this without this being told.
fn project_json(store: &Store, project: i64) -> Result<serde_json::Value, CliError> {
    let row = store.project_notify(project).map_err(CliError::from)?;
    let targets: Vec<i64> = store
        .project_notify_targets(project)
        .map_err(CliError::from)?
        .iter()
        .map(|r| r.target_id)
        .collect();
    let events: Vec<String> = store
        .project_notify_events(project)
        .map_err(CliError::from)?
        .iter()
        .map(|r| r.event.clone())
        .collect();
    let reportable: Vec<&str> = amenbo_core::lifecycle::V1_EVENTS
        .iter()
        .copied()
        .filter(|e| amenbo_core::ops::notify::is_reportable(e))
        .collect();
    Ok(json!({
        "project_id": project,
        "enabled": row.as_ref().is_some_and(|r| r.enabled),
        "mail_to": row.map(|r| r.mail_to).unwrap_or_default(),
        "targets": targets,
        "events": events,
        "reportable": reportable,
    }))
}

/// One shelf row as a line: what carries it, its name, and the part of the connection that is not a
/// credential.
fn target_line(row: &serde_json::Value) -> String {
    let mark = if row["is_default"].as_bool() == Some(true) { " (default)" } else { "" };
    let held = if row["secret_set"].as_bool() == Some(true) { "set" } else { "not set" };
    let connection = match row["kind"].as_str() {
        Some("mail") => match row["smtp_host"].as_str() {
            Some(host) => format!("{host} · credential {held}"),
            None => format!("no server yet · credential {held}"),
        },
        _ => format!("credential {held}"),
    };
    format!(
        "{} {}{} — {}",
        row["id"].as_i64().unwrap_or_default(),
        row["name"].as_str().unwrap_or_default(),
        mark,
        connection
    )
}

/// A JSON array as a comma-separated line, or a word saying it is empty.
fn joined(value: &serde_json::Value) -> String {
    let said: Vec<String> = value
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))
        .collect();
    if said.is_empty() {
        "nothing".to_string()
    } else {
        said.join(", ")
    }
}
