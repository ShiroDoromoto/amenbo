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
//! **The app that takes a bundle is asked twice**, because a bundle it opened writes nothing into the
//! settings file (`AMB-T-3156`): what it leaves is an extension of its own, and asking only the file
//! would answer "not set up" to every reader who took the road amenbo offers. So the file is read as
//! for any other app — an entry there is one the reader wrote by hand, and theirs is the one they will
//! look for — and where it says nothing the extension answers ([`extension`]).
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

/// Where an app that takes a bundle keeps the extensions themselves, under
/// [`McpApp::extensions_dir`] — one directory per extension, holding the manifest it was installed
/// from.
const EXTENSIONS: &str = "Claude Extensions";

/// And where it keeps what the reader answered about each of them: one file per extension, named
/// after it. The two are read together, in that order (see [`extension`]).
const EXTENSION_SETTINGS: &str = "Claude Extensions Settings";

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
    let entries = server
        .settings_folder()
        .and_then(|folder| app.settings_path(folder))
        .and_then(|path| entries(app, &path))
        .unwrap_or_default();
    let current = entries.iter().find(|(filed, _)| filed == name);

    let mut stale: Vec<Stale> = entries
        .iter()
        .filter(|(filed, _)| crate::mcp::is_superseded_name(filed))
        .map(|(filed, args)| Stale { name: filed.clone(), folder: bound_folder(args) })
        .collect();
    // A settings document is a map, so the order they came out in is the parser's rather than the
    // reader's; sorted, a row that lists two of them lists them the same way twice running.
    stale.sort_by(|a, b| a.name.cmp(&b.name));

    // And what the app holds as an extension of its own, for the one app that takes a bundle. It is
    // asked second and answers only where the settings file said nothing: a reader who has both has
    // written one of them by hand, and their own entry is the one they will look for.
    let held = app.extensions_dir().and_then(|dir| extension(&dir)).filter(|_| current.is_none());

    Setup {
        id: app.id,
        label: app.label,
        set: current.is_some() || held.is_some(),
        folder: match current {
            Some((_, args)) => bound_folder(args),
            None => held.and_then(|folders| folders.into_iter().next()),
        },
        stale,
    }
}

/// The folders amenbo's own extension is set up for, or `None` where this app is not holding one.
///
/// **The answer is in two places and the order is the whole of it** (`AMB-T-3157`). What the reader
/// saved is kept beside the extension, and outranks everything: it is what the host puts on the
/// command line. Where they saved nothing that file has no answers in it — which is not "no folders"
/// but "the ones the bundle arrived with", still reaching the server, and those are in the manifest
/// the extension was installed from. Reading only the first would tell the most ordinary reader of
/// all — the one who installed it and touched nothing — that they are not set up.
///
/// An extension that is there but names no folder answers with an empty list rather than with `None`:
/// it is installed, which is the question `set` asks, and a reader who emptied the field is still a
/// reader who set the app up.
fn extension(dir: &Path) -> Option<Vec<PathBuf>> {
    let id = crate::mcp_bundle::extension_id();
    let saved = read_json(&dir.join(EXTENSION_SETTINGS).join(format!("{id}.json")));
    let installed = read_json(&dir.join(EXTENSIONS).join(&id).join("manifest.json"));
    let key = crate::mcp_bundle::FOLDERS_KEY;

    let answered = saved
        .as_ref()
        .and_then(|document| document.get("userConfig")?.get(key)?.as_array())
        .map(|folders| paths(folders));
    let arrived = installed
        .as_ref()
        .and_then(|document| document.get("user_config")?.get(key)?.get("default")?.as_array())
        .map(|folders| paths(folders));

    // Neither file there at all: whatever else is under this directory, amenbo's extension is not.
    // Uninstalling takes both away together (`AMB-T-3156`), so their absence is the answer.
    if saved.is_none() && installed.is_none() {
        return None;
    }
    Some(answered.or(arrived).unwrap_or_default())
}

/// A document read off disk, or `None` for one that is not there, cannot be read, or does not parse —
/// the same answer a settings file gives, and for the same reason.
fn read_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// A list of values as the folders it holds, dropping whatever is not a word — the same rule the
/// arguments of an entry are read by.
fn paths(values: &[serde_json::Value]) -> Vec<PathBuf> {
    values.iter().filter_map(|v| v.as_str()).map(PathBuf::from).collect()
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

    fn server<'a>(folders: &'a [PathBuf], exe: &'a Path) -> Server<'a> {
        Server { folders, exe }
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the parent is made");
        std::fs::write(path, text).expect("written");
    }

    fn app(id: &str) -> &'static McpApp {
        crate::mcp_apps::find(id).expect("listed")
    }

    /// What an app that took a bundle holds afterwards, written the way it writes it: the manifest the
    /// extension was installed from, and — where the reader saved the settings screen — their own
    /// answers beside it.
    fn extension_files(dir: &Path, arrived: Option<&str>, saved: Option<&str>) {
        let id = crate::mcp_bundle::extension_id();
        if let Some(folders) = arrived {
            write(
                &dir.join(EXTENSIONS).join(&id).join("manifest.json"),
                &format!(r#"{{"user_config":{{"folders":{{"multiple":true,"default":{folders}}}}}}}"#),
            );
        }
        if let Some(folders) = saved {
            write(
                &dir.join(EXTENSION_SETTINGS).join(format!("{id}.json")),
                &format!(r#"{{"isEnabled":true,"userConfig":{{"folders":{folders}}}}}"#),
            );
        }
    }

    /// The reader who installed the bundle and touched nothing is set up, for the folders it arrived
    /// with. Their own answers are not written down until they save that screen, so a read that asked
    /// only for those would tell the most ordinary reader of all that they were not set up.
    #[test]
    fn an_extension_nobody_has_answered_for_is_set_up_for_the_folders_it_arrived_with() {
        let dir = scratch("arrived");
        extension_files(&dir, Some(r#"["/work/shop","/work/greenhouse"]"#), None);

        assert_eq!(
            extension(&dir),
            Some(vec![PathBuf::from("/work/shop"), PathBuf::from("/work/greenhouse")]),
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// And once they have answered, their answer is the one that reaches the server — so it is the one
    /// read back, whatever the bundle arrived with.
    #[test]
    fn what_the_reader_saved_outranks_what_the_bundle_arrived_with() {
        let dir = scratch("saved");
        extension_files(&dir, Some(r#"["/work/shop"]"#), Some(r#"["/work/elsewhere"]"#));

        assert_eq!(extension(&dir), Some(vec![PathBuf::from("/work/elsewhere")]));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// An extension a reader emptied the field of is still an extension they installed. Answering
    /// "not set up" would send them to install a second one beside the first.
    #[test]
    fn an_extension_with_no_folder_left_in_it_is_still_installed() {
        let dir = scratch("emptied");
        extension_files(&dir, Some(r#"["/work/shop"]"#), Some("[]"));

        assert_eq!(extension(&dir), Some(Vec::new()));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Nothing there is not set up — and uninstalling takes both files away together, so their absence
    /// is the whole of the answer.
    #[test]
    fn an_app_holding_no_extension_of_amenbos_is_not_set_up() {
        let dir = scratch("none");
        assert_eq!(extension(&dir), None, "nothing on disk");

        // Somebody else's extension is not this one: the id is the manifest amenbo wrote.
        write(
            &dir.join(EXTENSIONS).join("local.mcpb.someone.else").join("manifest.json"),
            r#"{"user_config":{"folders":{"default":["/work/theirs"]}}}"#,
        );
        assert_eq!(extension(&dir), None, "another extension is not amenbo's");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The one app that takes a bundle is the one asked about extensions at all; every other row is a
    /// settings file and nothing else, so nothing goes looking beside them.
    #[test]
    fn only_the_app_that_takes_a_bundle_is_asked_what_it_holds() {
        let held: Vec<&str> =
            MCP_APPS.iter().filter(|app| app.extensions.is_some()).map(|app| app.id).collect();
        assert_eq!(held, vec!["claude-desktop"]);
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

        let found = read(app("cursor"), &server(std::slice::from_ref(&dir), exe));
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

        let found = read(app("cursor"), &server(std::slice::from_ref(&dir), exe));
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

        let found = read(app("cursor"), &server(std::slice::from_ref(&dir), exe));
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

        let found = read(app("cursor"), &server(std::slice::from_ref(&dir), exe));
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

        let found = read(app("vscode"), &server(std::slice::from_ref(&dir), exe));
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

        let nothing = read(app("cursor"), &server(std::slice::from_ref(&dir), exe));
        assert!(!nothing.set && nothing.folder.is_none(), "nothing on disk");

        write(&dir.join(".cursor/mcp.json"), "{ this is not JSON");
        let broken = read(app("cursor"), &server(std::slice::from_ref(&dir), exe));
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

        let found = read(app("cursor"), &server(std::slice::from_ref(&dir), exe));
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
        let answers = probe(&server(std::slice::from_ref(&dir), exe));

        assert_eq!(answers.len(), MCP_APPS.len());
        for (answer, app) in answers.iter().zip(MCP_APPS) {
            assert_eq!(answer.id, app.id);
            assert_eq!(answer.label, app.label);
        }

        std::fs::remove_dir_all(&dir).ok();
    }
}
