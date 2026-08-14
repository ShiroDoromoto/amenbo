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
//! under ([`crate::mcp::Server::name`]). An app holding some other MCP server is not holding this one,
//! and a reader who wrote their own entry by hand under that name is a reader who set it up.
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
}

/// Every listed app's answer for this project, in catalog order ([`MCP_APPS`]).
pub fn probe(server: &Server) -> Vec<Setup> {
    MCP_APPS.iter().map(|app| read(app, server)).collect()
}

/// One app's answer, read from its own settings.
pub fn read(app: &McpApp, server: &Server) -> Setup {
    let entry = app.settings_path(server.folder).and_then(|path| args_of(app, &path, &server.name()));
    Setup {
        id: app.id,
        label: app.label,
        set: entry.is_some(),
        folder: entry.as_deref().and_then(bound_folder),
    }
}

/// The arguments the named entry starts its server with, or `None` where this app holds no such entry
/// — which is also what every unreadable file answers.
fn args_of(app: &McpApp, path: &Path, name: &str) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(path).ok()?;
    match app.format {
        Format::Json => {
            let document: serde_json::Value = serde_json::from_str(&text).ok()?;
            let entry = document.get(app.servers_key)?.get(name)?;
            Some(words(entry.get("args")?.as_array()?, |v| v.as_str()))
        }
        Format::Toml => {
            let document: toml::Value = toml::from_str(&text).ok()?;
            let entry = document.get(app.servers_key)?.get(name)?;
            Some(words(entry.get("args")?.as_array()?, |v| v.as_str()))
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
        Server { slug: "shop", project: "Shop", folder, exe }
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
            r#"{"mcpServers":{"amenbo-shop":{"command":"/usr/local/bin/amenbo",
               "args":["mcp","--dir","/work/elsewhere"]}}}"#,
        );

        let found = read(app("cursor"), &server(&dir, exe));
        assert!(found.set);
        // The folder the entry names, which is not the folder the settings were found beside — the
        // difference is the whole reason it is read back rather than assumed.
        assert_eq!(found.folder.as_deref(), Some(Path::new("/work/elsewhere")));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Another app's entry is not this one's, and neither is another project's — the name is what the
    /// answer turns on.
    #[test]
    fn an_entry_under_another_name_is_not_this_project() {
        let dir = scratch("names");
        let exe = Path::new("/usr/local/bin/amenbo");
        write(
            &dir.join(".cursor/mcp.json"),
            r#"{"mcpServers":{"amenbo-greenhouse":{"command":"a","args":["mcp","--dir","/work/g"]},
               "something-else":{"command":"b","args":[]}}}"#,
        );

        assert!(!read(app("cursor"), &server(&dir, exe)).set);

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
            r#"{"servers":{"amenbo-shop":{"command":"a","args":["mcp","--dir","/work/shop"]}},
               "mcpServers":{"amenbo-shop":{"command":"a","args":["mcp","--dir","/nowhere"]}}}"#,
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
             [mcp_servers.\"amenbo-shop\"]\n\
             command = \"/usr/local/bin/amenbo\"\n\
             args = [\n  \"mcp\",\n  \"--dir\",\n  \"/work/shop\",   # the folder\n]\n",
        );

        let found = args_of(app("codex-cli"), &settings, "amenbo-shop").expect("the entry is found");
        assert_eq!(found, vec!["mcp", "--dir", "/work/shop"]);
        assert_eq!(bound_folder(&found).as_deref(), Some(Path::new("/work/shop")));

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
        write(&dir.join(".cursor/mcp.json"), r#"{"mcpServers":{"amenbo-shop":{"command":"a","args":["mcp"]}}}"#);

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
