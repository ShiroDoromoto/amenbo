//! Which apps are already set up to reach a project, and which folder each one says it is
//! (`AMB-D-673`).
//!
//! **Read-only.** Nothing here writes to a reader's settings. The two roads that set an app up are a
//! file the reader opens ([`crate::mcp_bundle`]) and a request they hand their AI
//! ([`crate::mcp_request`]); this is the third question, asked of what came of either.
//!
//! **Why the folder and not only the answer.** "Set up" on its own leaves the one thing a reader most
//! needs unsaid: set up *for which folder*. An entry binds the server to the folder written in it, and
//! a project is worked in whichever folder it is bound to today — so an app pointed at a folder nobody
//! works in reads exactly like one pointed at the right folder. What is read back is therefore the
//! folder the entry names, and the answer is that folder or nothing.
//!
//! **What is read is the entry amenbo would have written**, found by the name every road writes it
//! under ([`crate::mcp::name`]). An app holding some other MCP server is not holding this one,
//! and a reader who wrote their own entry by hand under that name is a reader who set it up.
//!
//! **What amenbo used to write is read too, and answered apart.** The name moved from the project's to
//! the machine's (`AMB-D-679`), and an entry under the old one goes on working — so it is invisible to
//! a reader and to a read that only asks after the name in use, while a fresh setup lands beside it
//! rather than on it. Those are gathered as [`Setup::stale`], never folded into
//! [`set`](Setup::set): the question "is this app set up" is about the entry amenbo writes today, and
//! an old one is the separate question of what to clear away ([`crate::mcp_request::remove_stale`]).
//!
//! **Every failure is "not set up".** A settings file that is missing, unreadable, or does not parse
//! says nothing about this project — it is not a fault to report, and a reader told "something is
//! wrong with your Cursor settings" by a backlog tool is being told about the wrong thing. The one
//! thing worth being careful about is which of those a near-miss lands in, which is why the TOML side
//! is parsed rather than scanned (see the crate's `toml` dependency).

use std::path::{Path, PathBuf};

use crate::mcp::{Server, DIR_FLAG};
use crate::mcp_apps::{Format, McpApp, MCP_APPS};

/// What one app's settings say about one project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setup {
    /// The app this answers for ([`McpApp::id`]).
    pub id: &'static str,
    /// The app's own name, so a face can draw this row without a second lookup.
    pub label: &'static str,
    /// Whether that app holds an entry for this project's server at all.
    pub set: bool,
    /// The folder that entry binds the server to. It is asked apart from [`set`](Setup::set) because
    /// the two can disagree: an entry someone edited by hand may name no folder, and a row saying
    /// "set up" with nothing after it is still the truth.
    pub folder: Option<PathBuf>,
    /// The entries this app still holds under a name amenbo used to write, by name
    /// ([`crate::mcp::is_superseded_name`]). Empty for a reader who never set amenbo up under the old
    /// scheme, which is every reader who arrived after `AMB-D-679`.
    pub stale: Vec<Stale>,
}

/// An entry left behind under a superseded name — what a reader has to be shown before they can be
/// asked whether to clear it ([`crate::mcp_request::remove_stale`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stale {
    /// The name it is filed under. A removal has to name it, and it is also the only thing telling one
    /// of these apart from the entry in use.
    pub name: String,
    /// The folder it binds its server to, where it names one at all — read for the same reason
    /// [`Setup::folder`] is, since which project a reader is being offered to clear away is a question
    /// only the folder answers.
    pub folder: Option<PathBuf>,
}

/// Every listed app's answer for this project, in catalog order ([`MCP_APPS`]).
pub fn probe(server: &Server) -> Vec<Setup> {
    MCP_APPS.iter().map(|app| read(app, server)).collect()
}

/// One app's answer, read from its own settings.
pub fn read(app: &McpApp, server: &Server) -> Setup {
    let name = crate::mcp::name();
    let entries = app.settings_path(server.folder).and_then(|path| entries(app, &path)).unwrap_or_default();
    let current = entries.iter().find(|(filed, _)| filed == name);

    let mut stale: Vec<Stale> = entries
        .iter()
        .filter(|(filed, _)| crate::mcp::is_superseded_name(filed))
        .map(|(filed, args)| Stale { name: filed.clone(), folder: bound_folder(args) })
        .collect();
    // A settings document is a map, so the order they came out in is the parser's rather than the
    // reader's; sorted, a row that lists two of them lists them the same way twice running.
    stale.sort_by(|a, b| a.name.cmp(&b.name));

    Setup {
        id: app.id,
        label: app.label,
        set: current.is_some(),
        folder: current.and_then(|(_, args)| bound_folder(args)),
        stale,
    }
}

/// Every server this app's settings hold, as the name it is filed under and the arguments it starts
/// with, or `None` where there is nothing to read — which is also what every unreadable file answers.
///
/// An entry whose arguments are missing, or are not a list, is read as an entry that carries none
/// rather than as no entry at all: what makes one an entry is that a name is filed there, and the
/// alternative would tell a reader who hand-edited theirs into that state that they had never set
/// amenbo up.
fn entries(app: &McpApp, path: &Path) -> Option<Vec<(String, Vec<String>)>> {
    let text = std::fs::read_to_string(path).ok()?;
    match app.format {
        Format::Json => {
            let document: serde_json::Value = serde_json::from_str(&text).ok()?;
            let servers = document.get(app.servers_key)?.as_object()?;
            Some(
                servers
                    .iter()
                    .map(|(name, entry)| {
                        let args = entry.get("args").and_then(|v| v.as_array());
                        (name.clone(), args.map(|list| words(list, |v| v.as_str())).unwrap_or_default())
                    })
                    .collect(),
            )
        }
        Format::Toml => {
            let document: toml::Value = toml::from_str(&text).ok()?;
            let servers = document.get(app.servers_key)?.as_table()?;
            Some(
                servers
                    .iter()
                    .map(|(name, entry)| {
                        let args = entry.get("args").and_then(|v| v.as_array());
                        (name.clone(), args.map(|list| words(list, |v| v.as_str())).unwrap_or_default())
                    })
                    .collect(),
            )
        }
    }
}

/// A list of values as the words it holds, dropping whatever is not one. An argument that arrived as a
/// number or a table is not an argument the server was started with, and reading it as an empty string
/// would put a word where there was none.
fn words<T>(values: &[T], word: impl Fn(&T) -> Option<&str>) -> Vec<String> {
    values.iter().filter_map(|v| word(v).map(str::to_string)).collect()
}

/// The folder the arguments bind the server to — whatever follows [`DIR_FLAG`].
fn bound_folder(args: &[String]) -> Option<PathBuf> {
    let flag = args.iter().position(|arg| arg == DIR_FLAG)?;
    args.get(flag + 1).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of the test's own to stand settings files in, named after the test that asked for one
    /// so two running at once do not read each other's.
    fn scratch(named: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("amenbo-mcp-probe-{}-{named}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a directory to write into");
        dir
    }

    fn server<'a>(folder: &'a Path, exe: &'a Path) -> Server<'a> {
        Server { project: "Shop", folder, exe }
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the parent is made");
        std::fs::write(path, text).expect("written");
    }

    fn app(id: &str) -> &'static McpApp {
        crate::mcp_apps::find(id).expect("listed")
    }

    /// The whole answer, off a folder's own settings: the entry is there, and it names the folder it
    /// binds the server to.
    #[test]
    fn an_entry_in_a_folders_settings_answers_with_the_folder_it_names() {
        let dir = scratch("json");
        let exe = Path::new("/usr/local/bin/amenbo");
        write(
            &dir.join(".cursor/mcp.json"),
            r#"{"mcpServers":{"amenbo":{"command":"/usr/local/bin/amenbo",
               "args":["mcp","--dir","/work/elsewhere"]}}}"#,
        );

        let found = read(app("cursor"), &server(&dir, exe));
        assert!(found.set);
        // The folder the entry names, which is not the folder the settings were found beside — the
        // difference is the whole reason it is read back rather than assumed.
        assert_eq!(found.folder.as_deref(), Some(Path::new("/work/elsewhere")));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Somebody else's server is not this one, however close its name reads — the name is what the
    /// answer turns on, and it is matched whole. Neither is it something to offer to delete: an entry
    /// amenbo never wrote is not amenbo's to name.
    #[test]
    fn an_entry_under_another_name_is_not_this_one() {
        let dir = scratch("names");
        let exe = Path::new("/usr/local/bin/amenbo");
        write(
            &dir.join(".cursor/mcp.json"),
            r#"{"mcpServers":{"amenboard":{"command":"a","args":["mcp","--dir","/work/g"]},
               "something-else":{"command":"b","args":[]}}}"#,
        );

        let found = read(app("cursor"), &server(&dir, exe));
        assert!(!found.set);
        assert!(found.stale.is_empty(), "{:?}", found.stale);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The entry in use and the ones an older amenbo left, read apart in one pass. A reader who set two
    /// projects up under the old scheme has two to clear, and each names the folder it was bound to —
    /// which is the only thing telling them apart on screen.
    #[test]
    fn the_names_amenbo_used_to_write_are_read_beside_the_one_it_writes_now() {
        let dir = scratch("stale");
        let exe = Path::new("/usr/local/bin/amenbo");
        write(
            &dir.join(".cursor/mcp.json"),
            r#"{"mcpServers":{"amenbo":{"command":"a","args":["mcp","--dir","/work/shop"]},
               "amenbo-shop":{"command":"a","args":["mcp","--dir","/work/shop"]},
               "amenbo-greenhouse":{"command":"a","args":["mcp","--dir","/work/g"]},
               "something-else":{"command":"b","args":[]}}}"#,
        );

        let found = read(app("cursor"), &server(&dir, exe));
        assert!(found.set, "the entry in use is there");
        assert_eq!(found.folder.as_deref(), Some(Path::new("/work/shop")));
        assert_eq!(
            found.stale,
            vec![
                Stale { name: "amenbo-greenhouse".into(), folder: Some(PathBuf::from("/work/g")) },
                Stale { name: "amenbo-shop".into(), folder: Some(PathBuf::from("/work/shop")) },
            ],
            "both old entries, in a settled order"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// An old entry on its own leaves the app not set up: it is the name amenbo writes today that the
    /// question is about, and answering "set up" off an entry the reader is being asked to delete would
    /// leave them with nothing.
    #[test]
    fn an_old_entry_alone_does_not_make_an_app_set_up() {
        let dir = scratch("staleonly");
        let exe = Path::new("/usr/local/bin/amenbo");
        write(
            &dir.join(".cursor/mcp.json"),
            r#"{"mcpServers":{"amenbo-shop":{"command":"a","args":["mcp","--dir","/work/shop"]}}}"#,
        );

        let found = read(app("cursor"), &server(&dir, exe));
        assert!(!found.set, "not set up");
        assert_eq!(found.folder, None);
        assert_eq!(found.stale.len(), 1, "and one entry to clear");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The one app that does not call it `mcpServers`. Its settings are read under its own word, so an
    /// entry written where VS Code keeps them is found — and one written under the other word is not.
    #[test]
    fn the_word_the_entries_hang_under_is_the_apps_own() {
        let dir = scratch("vscode");
        let exe = Path::new("/usr/local/bin/amenbo");
        write(
            &dir.join(".vscode/mcp.json"),
            r#"{"servers":{"amenbo":{"command":"a","args":["mcp","--dir","/work/shop"]}},
               "mcpServers":{"amenbo":{"command":"a","args":["mcp","--dir","/nowhere"]}}}"#,
        );

        let found = read(app("vscode"), &server(&dir, exe));
        assert_eq!(found.folder.as_deref(), Some(Path::new("/work/shop")));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The TOML side, written the way a person writes one: a quoted key, a comment, a wrapped array,
    /// and another table beside it. Every one of those is why this is parsed rather than scanned.
    #[test]
    fn a_hand_written_toml_is_read_as_toml() {
        let dir = scratch("toml");
        let settings = dir.join("config.toml");
        write(
            &settings,
            "# my own notes\n\
             model = \"something\"\n\
             \n\
             [mcp_servers.other]\n\
             command = \"elsewhere\"\n\
             \n\
             [mcp_servers.\"amenbo\"]\n\
             command = \"/usr/local/bin/amenbo\"\n\
             args = [\n  \"mcp\",\n  \"--dir\",\n  \"/work/shop\",   # the folder\n]\n",
        );

        let read = entries(app("codex-cli"), &settings).expect("the document parses");
        let found = &read.iter().find(|(name, _)| name == "amenbo").expect("the entry is found").1;
        assert_eq!(found, &["mcp", "--dir", "/work/shop"]);
        assert_eq!(bound_folder(found).as_deref(), Some(Path::new("/work/shop")));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Nothing to read is not set up, and neither is anything that will not parse. A reader is not
    /// told their settings are broken by a backlog tool — the question asked was about one project.
    #[test]
    fn a_file_that_is_missing_or_broken_answers_not_set_up() {
        let dir = scratch("broken");
        let exe = Path::new("/usr/local/bin/amenbo");

        let nothing = read(app("cursor"), &server(&dir, exe));
        assert!(!nothing.set && nothing.folder.is_none(), "nothing on disk");

        write(&dir.join(".cursor/mcp.json"), "{ this is not JSON");
        let broken = read(app("cursor"), &server(&dir, exe));
        assert!(!broken.set && broken.folder.is_none(), "nothing that parses");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// An entry someone edited into naming no folder is still an entry. Saying "not set up" about it
    /// would send a reader to add a second one beside the first.
    #[test]
    fn an_entry_that_names_no_folder_is_still_set_up() {
        let dir = scratch("nodir");
        let exe = Path::new("/usr/local/bin/amenbo");
        write(&dir.join(".cursor/mcp.json"), r#"{"mcpServers":{"amenbo":{"command":"a","args":["mcp"]}}}"#);

        let found = read(app("cursor"), &server(&dir, exe));
        assert!(found.set, "the entry is there");
        assert_eq!(found.folder, None, "and it names no folder");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Every listed app is asked, in the order the catalog names them — a face draws one row per app
    /// whatever the answers are, so a probe that dropped the ones it found nothing for would leave the
    /// reader a shorter list than the one they were offered.
    #[test]
    fn every_listed_app_is_answered_for() {
        let dir = scratch("all");
        let exe = Path::new("/usr/local/bin/amenbo");
        let answers = probe(&server(&dir, exe));

        assert_eq!(answers.len(), MCP_APPS.len());
        for (answer, app) in answers.iter().zip(MCP_APPS) {
            assert_eq!(answer.id, app.id);
            assert_eq!(answer.label, app.label);
        }

        std::fs::remove_dir_all(&dir).ok();
    }
}
