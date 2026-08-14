//! What a reader hands the AI already sitting in their app, to have amenbo added to it or taken back
//! out (`AMB-D-672`).
//!
//! **The AI is the hand that edits the file**, and the request is worded so that stays true. amenbo
//! writes nothing here: the text travels through the reader, who decides whether to give it to anyone.
//! What it asks for is an edit to a file that already exists and already holds other servers — so it
//! says which file, that everything else in it stays, and that nothing else is to change. Handing over
//! a whole settings document instead would leave the merge to the reader, exactly when their file is
//! not empty. It is the same shape [`crate::harness::request`] takes, for the same reasons.
//!
//! **Every app on this road can run a command**, which is why it is this road rather than a file
//! ([`crate::mcp_apps`]): the AI in the app writes the settings its own app reads, and knows that
//! document better than a generator does. What the request contributes is the part nobody in the app
//! can know — where amenbo is installed, which folder this project is, and the name the entry has to
//! carry so a later read can tell it apart ([`crate::mcp::name`]).
//!
//! **Taking it back out is a request too, and not the same one.** A removal names the entry and
//! nothing else: there is no configuration to carry, and a reader who moved a project or finished with
//! one is otherwise left editing by hand the thing they were never asked to edit by hand. There are two
//! of them, because there are two entries to take out: the one amenbo writes today, and the one an
//! older amenbo left under a name it no longer uses ([`remove_stale`], `AMB-D-679`).
//!
//! English, like the request the harness hands over: the recipient is a model, and one text is one
//! text to keep in step with.

use crate::mcp::Server;
use crate::mcp_apps::{Format, McpApp};

/// The entry to be added, in the shape the app's own settings are written in — the document the
/// request carries inside it.
///
/// It is public because the two are separately true: a caller standing in for the AI that does the
/// merge needs the entry alone, and reading it back out of the request's prose would make that caller
/// the judge of prose it does not own.
pub fn entry(app: &McpApp, server: &Server) -> String {
    match app.format {
        Format::Json => {
            // Both keys are words the row and the server give, so the object is built rather than
            // written out: a literal would put one app's spelling in the one place that must not
            // carry it.
            let mut servers = serde_json::Map::new();
            servers.insert(
                crate::mcp::name().to_string(),
                serde_json::json!({ "command": server.command(), "args": server.args() }),
            );
            let mut document = serde_json::Map::new();
            document.insert(app.servers_key.to_string(), serde_json::Value::Object(servers));
            let document = serde_json::Value::Object(document);
            serde_json::to_string_pretty(&document).unwrap_or_else(|_| document.to_string())
        }
        // The same shape one tier over, through the crate that reads these files back in
        // `mcp_probe`: a folder called `C:\work` or one with a quote in its name would otherwise hand
        // a reader a document their app cannot parse, and writing the escape by hand beside a parser
        // that knows it is two implementations of one grammar.
        Format::Toml => {
            let mut entry = toml::Table::new();
            entry.insert("command".to_string(), toml::Value::String(server.command()));
            entry.insert(
                "args".to_string(),
                toml::Value::Array(server.args().into_iter().map(toml::Value::String).collect()),
            );
            let mut servers = toml::Table::new();
            servers.insert(crate::mcp::name().to_string(), toml::Value::Table(entry));
            let mut document = toml::Table::new();
            document.insert(app.servers_key.to_string(), toml::Value::Table(servers));
            toml::to_string(&document).unwrap_or_default().trim_end().to_string()
        }
    }
}

/// The request that adds amenbo to one app, for one project.
pub fn add(app: &McpApp, server: &Server) -> String {
    format!(
        "Please add the Amenbo project \"{project}\" to {label} as an MCP server, so you can work its \
         backlog from here.\n\
         \n\
         Merge the entry below into `{settings}`. Keep every other server that file already holds, and \
         create the file if it is not there. Change nothing else, and tell me what you changed.\n\
         \n\
         ```{format}\n\
         {entry}\n\
         ```\n\
         \n\
         The server is bound to the folder it names, so it answers about that project and no other.",
        project = server.project,
        label = app.label,
        settings = settings(app, server.folder),
        format = app.format.as_str(),
        entry = entry(app, server),
    )
}

/// The request that takes it back out again.
pub fn remove(app: &McpApp, server: &Server) -> String {
    format!(
        "Please remove the Amenbo project \"{project}\" from {label}.\n\
         \n\
         In `{settings}`, delete the MCP server entry named `{name}` and nothing else — every other \
         server in that file stays. Tell me what you changed.",
        project = server.project,
        label = app.label,
        settings = settings(app, server.folder),
        name = crate::mcp::name(),
    )
}

/// The request that clears out an entry left under a name amenbo no longer writes
/// (`AMB-D-679`, found by [`crate::mcp_probe`]).
///
/// It names the entry rather than the project, because the two have come apart: the name carries the
/// slug the project had when the entry was written, and the reader may have renamed that project or
/// finished with it since. What it says beyond the name is which entry stays — the one in use reads as
/// a near neighbour of the one being taken out, and "delete the amenbo entry" would be carried out
/// either way.
pub fn remove_stale(app: &McpApp, server: &Server, name: &str) -> String {
    format!(
        "Please remove an Amenbo MCP server entry that an older Amenbo left behind, from {label}.\n\
         \n\
         In `{settings}`, delete the MCP server entry named `{name}` and nothing else — every other \
         server in that file stays, the `{current}` entry Amenbo uses now included. Tell me what you \
         changed.",
        label = app.label,
        settings = settings(app, server.folder),
        current = crate::mcp::name(),
    )
}

/// The file the request names, written out in full where the machine will say where it is. A relative
/// path would be read against wherever the AI happens to be standing, and half of these apps keep
/// their settings nowhere near the folder.
fn settings(app: &McpApp, folder: &std::path::Path) -> String {
    match app.settings_path(folder) {
        Some(path) => path.display().to_string(),
        // No home directory to resolve against — the only thing left to name is the path as the app's
        // own documentation writes it, which is better than naming nothing.
        None => match app.place {
            crate::mcp_apps::Place::Settings(p)
            | crate::mcp_apps::Place::Home(p)
            | crate::mcp_apps::Place::Folder(p) => p.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn server<'a>(folder: &'a Path, exe: &'a Path) -> Server<'a> {
        Server { project: "Shop", folder, exe }
    }

    fn app(id: &str) -> &'static McpApp {
        crate::mcp_apps::find(id).expect("listed")
    }

    /// The JSON entry is the app's own word for where servers hang, with the command and the folder
    /// under it — and the word is the app's, not the family's.
    #[test]
    fn a_json_entry_hangs_under_the_word_that_app_uses() {
        let folder = Path::new("/work/shop");
        let exe = Path::new("/usr/local/bin/amenbo");

        let cursor: serde_json::Value =
            serde_json::from_str(&entry(app("cursor"), &server(folder, exe))).expect("valid JSON");
        assert_eq!(cursor["mcpServers"]["amenbo"]["command"], "/usr/local/bin/amenbo");
        assert_eq!(
            cursor["mcpServers"]["amenbo"]["args"],
            serde_json::json!(["mcp", "--dir", "/work/shop"])
        );

        let vscode: serde_json::Value =
            serde_json::from_str(&entry(app("vscode"), &server(folder, exe))).expect("valid JSON");
        assert!(vscode["mcpServers"].is_null(), "VS Code does not call it that");
        assert_eq!(vscode["servers"]["amenbo"]["command"], "/usr/local/bin/amenbo");
    }

    /// The one app whose settings are TOML: a table named after the entry, with the same two keys. It
    /// is read back rather than compared as text — what the reader's app does with this is parse it,
    /// so that is the question, and the crate is free to lay a document out as it likes.
    #[test]
    fn a_toml_entry_is_a_table_named_after_the_server() {
        let written = entry(app("codex-cli"), &server(Path::new("/work/shop"), Path::new("/bin/a")));
        assert!(written.contains("[mcp_servers.amenbo]"), "the table is named: {written}");

        let read: toml::Value = toml::from_str(&written).expect("valid TOML");
        let entry = &read["mcp_servers"]["amenbo"];
        assert_eq!(entry["command"].as_str(), Some("/bin/a"));
        assert_eq!(
            entry["args"].as_array().expect("a list").iter().filter_map(|v| v.as_str()).collect::<Vec<_>>(),
            vec!["mcp", "--dir", "/work/shop"]
        );
    }

    /// A Windows path carries the one character TOML reads as an escape. The document has to survive
    /// it, which is why the value goes through a writer rather than being quoted by hand.
    #[test]
    fn a_path_with_a_backslash_survives_the_toml() {
        let written = entry(
            app("codex-cli"),
            &Server {
                project: "Shop",
                folder: Path::new(r"C:\work\shop"),
                exe: Path::new(r"C:\Program Files\amenbo\amenbo.exe"),
            },
        );

        let read: toml::Value = toml::from_str(&written).expect("valid TOML");
        let entry = &read["mcp_servers"]["amenbo"];
        assert_eq!(entry["command"].as_str(), Some(r"C:\Program Files\amenbo\amenbo.exe"));
        assert_eq!(entry["args"][2].as_str(), Some(r"C:\work\shop"));
    }

    /// What the request has to say to be acted on: which file, that the rest of it stays, and the
    /// entry itself — fenced in the format that file is written in.
    #[test]
    fn the_request_names_the_file_and_keeps_what_is_in_it() {
        let folder = Path::new("/work/shop");
        let exe = Path::new("/usr/local/bin/amenbo");
        let said = add(app("cursor"), &server(folder, exe));

        assert!(said.contains("/work/shop/.cursor/mcp.json"), "{said}");
        assert!(said.contains("Keep every other server"), "{said}");
        assert!(said.contains("```json"), "{said}");
        assert!(said.contains(&entry(app("cursor"), &server(folder, exe))), "{said}");
        assert!(said.contains("Shop"), "the project is named: {said}");
    }

    /// A folder's settings are named against the folder, and the machine's are named where they are —
    /// a request that pointed a reader at a path relative to nothing would name a file they do not have.
    #[test]
    fn the_machines_settings_are_named_where_they_are() {
        let folder = Path::new("/work/shop");
        let exe = Path::new("/usr/local/bin/amenbo");
        let said = add(app("codex-cli"), &server(folder, exe));

        assert!(said.contains(".codex/config.toml"), "{said}");
        assert!(!said.contains("/work/shop/.codex"), "not under the project's folder: {said}");
        assert!(said.contains("```toml"), "{said}");
    }

    /// A removal carries no configuration — only the name of the entry to take out, and the promise
    /// that the other servers in the file are left alone.
    #[test]
    fn a_removal_names_the_entry_and_carries_no_document() {
        let folder = Path::new("/work/shop");
        let exe = Path::new("/usr/local/bin/amenbo");
        let said = remove(app("cursor"), &server(folder, exe));

        assert!(said.contains("`amenbo`"), "{said}");
        assert!(said.contains("/work/shop/.cursor/mcp.json"), "{said}");
        assert!(!said.contains("```"), "there is nothing to fence: {said}");
        assert!(said.contains("stays"), "{said}");
    }

    /// Clearing out an old entry names that entry, and says which one is not to go with it. Without
    /// that second half the request reads as "delete the amenbo entry", and the AI acting on it is as
    /// likely to take the live one.
    #[test]
    fn clearing_an_old_entry_names_it_and_spares_the_one_in_use() {
        let folder = Path::new("/work/shop");
        let exe = Path::new("/usr/local/bin/amenbo");
        let said = remove_stale(app("cursor"), &server(folder, exe), "amenbo-greenhouse");

        assert!(said.contains("`amenbo-greenhouse`"), "the old entry is named: {said}");
        assert!(said.contains("`amenbo` entry Amenbo uses now"), "the live one is spared: {said}");
        assert!(said.contains("/work/shop/.cursor/mcp.json"), "{said}");
        assert!(!said.contains("```"), "there is nothing to fence: {said}");
        // The project it was written for is gone from the name's meaning, so it is not claimed here.
        assert!(!said.contains("Shop"), "no project is named: {said}");
    }

    /// Every app the catalog lists can be asked, both ways — a row nobody can word a request for would
    /// be a row a face draws a button beside that does nothing.
    #[test]
    fn every_listed_app_can_be_asked_both_ways() {
        let folder = Path::new("/work/shop");
        let exe = Path::new("/usr/local/bin/amenbo");
        for app in crate::mcp_apps::MCP_APPS {
            let added = add(app, &server(folder, exe));
            let removed = remove(app, &server(folder, exe));
            assert!(added.contains(app.label), "the add names {}", app.id);
            assert!(
                added.contains(&entry(app, &server(folder, exe))),
                "the add carries the entry for {}",
                app.id
            );
            assert!(removed.contains("`amenbo`"), "the removal names it for {}", app.id);
        }
    }
}
