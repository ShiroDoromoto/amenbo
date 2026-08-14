//! `activity`: the shared timeline of system events and comments.

use amenbo_core::model::ActorKind;
use amenbo_core::{query, Store};

use crate::cmd::arg::parse_date_opt;
use crate::cmd::task::resolve_task;
use crate::output::{human, print_json, CliError, Flags};

/// `amenbo activity`: the unified timeline (system events plus comments), newest first.
#[allow(clippy::too_many_arguments)]
pub(crate) fn activity_cmd(
    store: &Store,
    flags: &Flags,
    task: Option<String>,
    project: Option<String>,
    since: Option<String>,
    kind: Option<String>,
    by: Option<String>,
    for_scope: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<i32, CliError> {
    let task_id = task.map(|t| resolve_task(store, &t)).transpose().map_err(CliError::from)?;
    let project_id = match project {
        Some(p) => Some(store.resolve_project_ref(&p).map_err(CliError::from)?),
        None => None,
    };
    // `--since`: an opaque cursor from a previous response means incremental mode (walking forward in time);
    // anything else is a date.
    let (since_date, since_cursor) = match since.as_deref() {
        None => (None, None),
        Some(s) if query::looks_like_activity_cursor(s) => {
            let cur = query::parse_activity_cursor(s).ok_or_else(|| CliError {
                code: "invalid_value",
                message: "--since cursor is malformed".to_string(),
                hint: Some("pass a cursor from a previous activity response, or a date (today / +3d / YYYY-MM-DD)".to_string()),
                exit: 2,
            })?;
            (None, Some(cur))
        }
        Some(_) => (parse_date_opt(&since)?, None),
    };
    let cursor_mode = since_cursor.is_some();
    // `--for`: the audience scope. `me` is this invocation's own facet (the one `--actor` declared);
    // human/ai name a facet outright.
    let for_facet = match for_scope.as_deref() {
        None => None,
        Some("me") => Some(flags.facet()?),
        Some(s) => Some(ActorKind::parse(s).ok_or_else(|| CliError {
            code: "invalid_value",
            message: format!("--for must be me / human / ai ('{s}' is invalid)"),
            hint: None,
            exit: 2,
        })?),
    };
    let kind = match kind.as_deref() {
        None => None,
        Some("system") => Some(amenbo_core::activity::Kind::System),
        Some("comment") => Some(amenbo_core::activity::Kind::Comment),
        Some(other) => return Err(CliError { code: "invalid_value", message: format!("--kind must be system / comment ('{other}' is invalid)"), hint: None, exit: 2 }),
    };
    let actor = match by.as_deref() {
        None => None,
        Some(s) => Some(ActorKind::parse(s).ok_or_else(|| CliError { code: "invalid_value", message: format!("--by must be human / ai ('{s}' is invalid)"), hint: None, exit: 2 })?),
    };
    let result = store.activity(query::ActivityParams {
        task_id, project_id, since: since_date, since_cursor, kind, actor, for_facet, limit, offset,
    }).map_err(CliError::from)?;
    if flags.json {
        print_json(&result);
    } else {
        // Incremental mode runs oldest first (it moves forward); history mode runs newest first.
        let order = if cursor_mode { "oldest first" } else { "newest first" };
        human(flags, format!("{} activity item(s) ({order})", result.count));
        for it in &result.items {
            let who = match it.author.kind {
                Some(amenbo_core::model::ActorKind::Ai) => format!("🤖{}", it.author.name),
                _ => it.author.name.clone(),
            };
            let body = if it.kind == "comment" {
                // Say when a comment was edited, in the same words `comment list` uses.
                let edited = it
                    .edited_at
                    .map(|t| format!(" · edited {}", t.to_rfc3339_z()))
                    .unwrap_or_default();
                format!("💬 {}{edited}", it.text.clone().unwrap_or_default())
            } else {
                let kind = it.event.as_ref().and_then(|e| e.get("kind")).and_then(|k| k.as_str()).unwrap_or("event");
                format!("⚙ {kind}")
            };
            // A target whose name could not be recovered — the row carrying it was compacted away, or lies
            // beyond the lookback budget — comes back as an empty string. To a human that would just be a
            // blank after the " — ", so say here that the target is gone (`--json` passes it through raw).
            let title = if it.target.title.is_empty() { "(deleted)" } else { &it.target.title };
            // Some rows still have the name while the target itself is gone: past rows about something later
            // deleted. Printing the name alone makes it indistinguishable from a live target, and `task show`
            // comes back empty — so if it cannot be followed, say so. `--json` does not paraphrase; it hands
            // over `target.live` raw, which is what a machine reads.
            let gone = if it.target.live || it.target.title.is_empty() { "" } else { " (deleted)" };
            human(flags, format!("  [{}] {} {} — {}{gone}", it.at.to_rfc3339_z(), who, body, title));
        }
        // Hand back an opaque cursor that can be passed straight to `--since <cursor>` next time — the seam
        // an incremental subscription is stitched from.
        if let Some(c) = &result.cursor {
            let more = if result.has_more { " (more)" } else { "" };
            human(flags, format!("  ↪ cursor: {c}{more}"));
        }
    }
    Ok(0)
}
