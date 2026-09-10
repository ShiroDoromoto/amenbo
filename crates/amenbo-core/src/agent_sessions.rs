//! **Which sessions a provider has** — asked of the provider's own command, for the one row that
//! will not be told a handle (`AMB-D-869`).
//!
//! Four of the six take the handle Amenbo decided as they start ([`crate::harness::Resume`]), and
//! there is nothing to ask them: the id is already written down before the program is running.
//! OpenCode names its own — `ses_f7a6428cbffemn98kHpVwT8NEh` is not a shape anyone else could have
//! chosen — so the pane is started, and the id it took is read back out of its own list afterwards
//! (`AMB-T-4630`). The sixth is Gemini, which carries no handle at all: it restarts itself on the
//! same argv, and its own list holds a session only once there is something in it to resume, so
//! there is nothing to ask at the moment a pane starts (`AMB-T-4659`).
//!
//! **A public command, not a file of the provider's** (`AMB-D-747`). `opencode session list
//! --format json` is documented and prints what the provider itself reads; the alternative was
//! `~/.local/share/opencode`, which is exactly what Amenbo does not open.
//!
//! **Nothing here runs anything.** This says what to ask and reads the answer, and the running is
//! the caller's, in the reader's own login shell (`app/src-tauri/src/agent_sessions.rs`) — the same
//! division [`crate::agent_models`] holds to, and for the same reason: a command resolved against
//! this process's environment would answer for a machine nobody is using.

use std::path::Path;

/// How one provider is asked what sessions it has — a column on [`crate::harness::Resume`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ask {
    /// The arguments the provider's own command ([`crate::harness::Launch::command`]) is asked
    /// with.
    pub args: &'static [&'static str],
    /// The shape the answer comes back in.
    pub reading: Reading,
}

/// The shape one provider's answer comes back in.
///
/// One variant, because one provider names its own handle. It is a variant rather than nothing at
/// all for the reason [`crate::agent_models::Reading`] has five: what a row says is how to read the
/// answer, and a second provider that named its own would bring its own shape rather than be bent
/// into this one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// A JSON array of `{ id, directory, created }` — OpenCode's. `created` is milliseconds since
    /// the epoch, and `directory` is the folder as the filesystem spells it after resolution, which
    /// is the same spelling a pane is opened under (`app/src-tauri/src/pty.rs`).
    JsonSessions,
}

/// One session a provider is holding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// The provider's own handle for it — what goes behind [`crate::harness::Resume::back`].
    pub id: String,
    /// The folder it was opened in, as the answer spelled it.
    pub folder: String,
    /// When it was made, in milliseconds since the epoch. It is what picks between two sessions in
    /// one folder, and nothing else reads it.
    pub created: i64,
}

/// The sessions in what the provider printed — **never an error**.
///
/// An answer this cannot make sense of is an empty list, and so is an empty answer. What a caller
/// does with either is the same thing: the pane opened all the same, and what is lost is the way
/// back into it a run later, which is not a failure a person could repair.
pub fn read(ask: &Ask, printed: &str) -> Vec<Session> {
    match ask.reading {
        Reading::JsonSessions => json_sessions(printed),
    }
}

/// OpenCode's `session list --format json`: the rows that carry all three fields, in the order they
/// came.
fn json_sessions(printed: &str) -> Vec<Session> {
    let Ok(serde_json::Value::Array(rows)) = serde_json::from_str::<serde_json::Value>(printed)
    else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            Some(Session {
                id: row.get("id")?.as_str()?.to_string(),
                folder: row.get("directory")?.as_str()?.to_string(),
                created: row.get("created")?.as_i64()?,
            })
        })
        .collect()
}

/// The newest session in `folder` that this pane could be — the handle to write down for it.
///
/// **`since` is when the pane was started**, in milliseconds since the epoch, and it is what parts
/// this pane's session from the ones the folder already had. A person who works in one folder has a
/// list of sessions in it going back weeks; the row that is theirs *now* is the one that appeared
/// while the pane was coming up.
///
/// **`taken` is the handles other panes have already been written down under.** Two panes opened in
/// one folder at once are two rows that both pass `since`, and without this the second would be
/// written down under the first's session — both panes then pointing at one conversation.
///
/// `folder` is compared as text. Both spellings come from the same place — the pane resolves the
/// folder before it starts anything and the provider prints the folder it was started in — so a
/// comparison that tried to be cleverer would be reading the filesystem for an answer it already
/// has.
pub fn newest_in<'a>(
    sessions: &'a [Session],
    folder: &Path,
    since: i64,
    taken: &[String],
) -> Option<&'a Session> {
    let folder = folder.to_string_lossy();
    sessions
        .iter()
        .filter(|one| one.folder == folder && one.created >= since)
        .filter(|one| !taken.contains(&one.id))
        .max_by_key(|one| one.created)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASK: Ask = Ask { args: &["session", "list", "--format", "json"], reading: Reading::JsonSessions };

    /// The answer as OpenCode prints it, cut to the three fields that are read.
    const PRINTED: &str = r#"[
      { "id": "ses_new", "title": "New session", "created": 200, "directory": "/work/repo" },
      { "id": "ses_old", "title": "New session", "created": 100, "directory": "/work/repo" },
      { "id": "ses_else", "title": "New session", "created": 300, "directory": "/work/other" }
    ]"#;

    #[test]
    fn the_rows_are_read_as_a_handle_a_folder_and_a_time() {
        let sessions = read(&ASK, PRINTED);
        assert_eq!(sessions.len(), 3);
        assert_eq!(sessions[0].id, "ses_new");
        assert_eq!(sessions[0].folder, "/work/repo");
        assert_eq!(sessions[0].created, 200);
    }

    /// An answer in no shape this knows is no sessions — never a failure, see the module docs.
    #[test]
    fn an_answer_that_is_not_the_shape_is_no_sessions() {
        for printed in ["", "not json at all", "{}", r#"[{"id":"ses_1"}]"#] {
            assert!(read(&ASK, printed).is_empty(), "{printed}");
        }
    }

    /// The pane's own session is the newest one in its folder that appeared after it started.
    #[test]
    fn the_pane_takes_the_newest_handle_that_appeared_in_its_folder() {
        let sessions = read(&ASK, PRINTED);
        let folder = Path::new("/work/repo");
        assert_eq!(newest_in(&sessions, folder, 0, &[]).map(|one| one.id.as_str()), Some("ses_new"));
        // And a folder the provider is holding nothing for is nothing to write down.
        assert!(newest_in(&sessions, Path::new("/work/none"), 0, &[]).is_none());
    }

    /// A session the folder already had is not this pane's, however new it is — the folder somebody
    /// has been working in for weeks is the case this is here for.
    #[test]
    fn a_session_older_than_the_pane_is_not_the_panes() {
        let sessions = read(&ASK, PRINTED);
        let folder = Path::new("/work/repo");
        assert_eq!(newest_in(&sessions, folder, 150, &[]).map(|one| one.id.as_str()), Some("ses_new"));
        assert!(newest_in(&sessions, folder, 250, &[]).is_none());
    }

    /// And a handle another pane has already been written down under is that pane's — two panes
    /// opened in one folder at once are two rows, not one conversation twice.
    #[test]
    fn a_handle_another_pane_took_is_not_offered_twice() {
        let sessions = read(&ASK, PRINTED);
        let folder = Path::new("/work/repo");
        let taken = vec!["ses_new".to_string()];
        assert_eq!(newest_in(&sessions, folder, 0, &taken).map(|one| one.id.as_str()), Some("ses_old"));
    }
}
