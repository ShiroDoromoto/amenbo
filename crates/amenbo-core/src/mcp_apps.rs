//! The AI apps amenbo can be reached from over MCP, and where each one keeps the settings that would
//! reach it (`AMB-D-672`, `AMB-D-673`).
//!
//! **Read-only, and a table rather than a code path.** Every app is one [`McpApp`] row in
//! [`MCP_APPS`]: the name it calls itself, the one file its MCP servers are written in, whether that
//! file is the machine's or the folder's, what it is written in, and whether amenbo writes it. Listing
//! one more app is one more row — the same condition the harness catalog holds to
//! ([`crate::harness`]), and the reason this is a table at all: three faces stand on it (writing the
//! file out, wording the request that asks an AI to write it, and saying which apps are already set
//! up), and none of them may grow a branch per app.
//!
//! **What a row carries of the settings' shape is the one word the apps disagree on.** An entry is a
//! command and its arguments wherever it is written; what differs is where the entries hang — one app
//! calls that place `mcpServers`, another `servers`, and the one written in TOML names a table
//! `mcp_servers`. So the row says the format and that word, and the document itself is derived from
//! the two ([`crate::mcp_request`]).

use std::path::{Path, PathBuf};

/// What one app's MCP settings are written in. The two the listed apps use, and nothing else: a
/// format is what a face needs to fence a document it hands over, not a parser selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// JSON, which is what all but one of them take.
    Json,
    /// TOML — the shape a tool that keeps its whole configuration in one table uses.
    Toml,
}

impl Format {
    /// The word a fenced code block is opened with, and the extension the file carries.
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Json => "json",
            Format::Toml => "toml",
        }
    }
}

/// Where an app keeps the file its MCP servers are written in.
///
/// The distinction that matters to a reader is the first one below against the last: settings held
/// once for the whole machine name the folder they are about *inside* the entry, while settings held
/// beside the work already say which folder they mean by sitting in it. Which root a machine-wide
/// file sits under is not a second question — it is the same answer written the way each app's
/// platform expects, and no caller has to know which is which ([`McpApp::settings_path`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// One file for the whole machine, under the OS's own settings directory (`~/Library/Application
    /// Support` on macOS, `%APPDATA%` on Windows, `~/.config` on Linux) — where an app that keeps its
    /// settings the way its platform says to puts them. The path is relative to that directory.
    Settings(&'static str),
    /// One file for the whole machine, in a dot-directory of the user's home — where a tool that
    /// keeps its settings the way a unix tool does puts them, at the same path on every OS. The path
    /// is relative to the home directory.
    Home(&'static str),
    /// One file per folder, sitting in the folder it is about. The path is relative to that folder.
    Folder(&'static str),
}

impl Place {
    /// Whether this is the machine's one file rather than a folder's own — the half of the answer a
    /// reader needs before anything else, since a machine-wide file is shared by every project on the
    /// device and a folder's is not.
    pub fn device_wide(self) -> bool {
        !matches!(self, Place::Folder(_))
    }

    /// The path this place stands for, for the folder being set up. `None` only where the OS will not
    /// say where the user's home is, which is the same answer amenbo gives about its own store.
    fn resolve(self, folder: &Path) -> Option<PathBuf> {
        match self {
            Place::Folder(relative) => Some(folder.join(relative)),
            Place::Settings(relative) => {
                Some(directories::BaseDirs::new()?.config_dir().join(relative))
            }
            Place::Home(relative) => Some(directories::BaseDirs::new()?.home_dir().join(relative)),
        }
    }
}

/// One AI app amenbo knows how to be reached from — the catalog row.
pub struct McpApp {
    /// The stable token a face names this app by (`claude-desktop`), lowercase and hyphenated. It is
    /// a key, not a rendering: a user reads [`label`](McpApp::label). Where an app is already listed
    /// as a harness ([`crate::harness`]) the token is the same one, so a face holding both catalogs
    /// is holding one vocabulary.
    pub id: &'static str,
    /// The product's own name for itself, as it appears in its documentation.
    pub label: &'static str,
    /// Where this app keeps the file its MCP servers are written in.
    pub place: Place,
    /// What that file is written in.
    pub format: Format,
    /// What this app calls the place its MCP servers are kept, inside that file. In a JSON document it
    /// is the object the entries hang under; in a TOML one it is the table each entry is named beneath
    /// (`[mcp_servers.<name>]`).
    ///
    /// The apps agree on nearly everything about that entry — a command and its arguments — and
    /// disagree about this one word, which is why it is the one part of the document a row carries.
    pub servers_key: &'static str,
    /// Whether amenbo writes the settings out for this app, rather than wording a request that asks
    /// the reader's AI to write them (`AMB-D-672`).
    ///
    /// It is one app, and the reason is not a preference: an app that can run a command has an AI in
    /// it that edits its own settings far better than a generator can — it merges, it keeps what is
    /// there, and it says what it changed. An app that cannot run one has nobody to ask, so the file
    /// is the only road left.
    pub amenbo_writes: bool,
    /// Where this app keeps what it was handed as a bundle, for the one app handed one — the directory
    /// its extensions live under, relative to the OS's own settings directory. `None` for every app
    /// that takes a settings file instead.
    ///
    /// It sits beside [`place`](McpApp::place) rather than in it because both are true of that app at
    /// once, and a reader can have set it up either way. A bundle it opened writes nothing into the
    /// settings file (`AMB-T-3156`) — an entry there is one the reader wrote by hand — so an answer
    /// that asked only the file would miss every reader who took the road amenbo offers, and one that
    /// asked only the extensions would miss the reader who wrote their own.
    pub extensions: Option<&'static str>,
}

impl McpApp {
    /// The file this app's MCP servers are written in, for the folder being set up. A machine-wide
    /// place ignores the folder, which is exactly what makes it machine-wide.
    ///
    /// `None` only where the OS will not say where the user's home is.
    pub fn settings_path(&self, folder: &Path) -> Option<PathBuf> {
        self.place.resolve(folder)
    }

    /// The directory this app keeps its extensions under, for the one app that takes a bundle.
    ///
    /// `None` where the app takes a settings file instead — and, as everywhere else here, where the OS
    /// will not say what the user's settings directory is.
    pub fn extensions_dir(&self) -> Option<PathBuf> {
        let held = self.extensions?;
        Some(directories::BaseDirs::new()?.config_dir().join(held))
    }
}

/// Every app amenbo lists, in the order a face offers them: the one it writes a file for first, and
/// the rest — which all have an AI of their own to ask — after it.
pub static MCP_APPS: &[McpApp] = &[
    McpApp {
        id: "claude-desktop",
        label: "Claude Desktop",
        // The one app on this list that cannot run a command, which is why it is also the one amenbo
        // writes a file for. `BaseDirs::config_dir()` is the same directory this app's own
        // documentation names on each of the three platforms, so the row carries one path rather
        // than three.
        place: Place::Settings("Claude/claude_desktop_config.json"),
        format: Format::Json,
        servers_key: "mcpServers",
        amenbo_writes: true,
        // The bundle it is handed becomes an extension of its own, kept under this directory beside
        // the settings file above (`AMB-T-3156`).
        extensions: Some("Claude"),
    },
    McpApp {
        id: "claude-code",
        label: "Claude Code",
        place: Place::Folder(".mcp.json"),
        format: Format::Json,
        servers_key: "mcpServers",
        amenbo_writes: false,
        extensions: None,
    },
    McpApp {
        id: "cursor",
        label: "Cursor",
        place: Place::Folder(".cursor/mcp.json"),
        format: Format::Json,
        servers_key: "mcpServers",
        amenbo_writes: false,
        extensions: None,
    },
    McpApp {
        id: "vscode",
        label: "VS Code",
        place: Place::Folder(".vscode/mcp.json"),
        format: Format::Json,
        // The one JSON app that does not call it `mcpServers`. Its own documentation writes `servers`
        // and names no second spelling, so this is the word rather than the family's.
        servers_key: "servers",
        amenbo_writes: false,
        extensions: None,
    },
    McpApp {
        id: "codex-cli",
        // The app a reader most often meets as the coding side of ChatGPT's desktop app; both read
        // the one configuration below, so they are one row rather than two.
        label: "Codex CLI",
        place: Place::Home(".codex/config.toml"),
        format: Format::Toml,
        // A table rather than an object: an entry is written `[mcp_servers.<name>]`.
        servers_key: "mcp_servers",
        amenbo_writes: false,
        extensions: None,
    },
    McpApp {
        id: "gemini-cli",
        label: "Gemini CLI",
        place: Place::Folder(".gemini/settings.json"),
        format: Format::Json,
        servers_key: "mcpServers",
        amenbo_writes: false,
        extensions: None,
    },
    McpApp {
        id: "antigravity",
        // Where a reader who moved off Gemini CLI arrives: the same house, and one file the editor and
        // the command both read, so it is one row for two faces the way `codex-cli` is.
        label: "Antigravity",
        // The machine's own, and only that. The workspace file its documentation names is read and
        // never started — the entry is found, no child comes up — so a row pointing there would offer
        // a road that goes nowhere. One file per machine is no trouble for a folder-per-server rule:
        // the folder rides in the entry's own arguments, and projects show up as entries beside each
        // other.
        place: Place::Home(".gemini/config/mcp_config.json"),
        format: Format::Json,
        servers_key: "mcpServers",
        amenbo_writes: false,
        extensions: None,
    },
    McpApp {
        id: "github-copilot",
        // The same token and the same name the harness catalog carries, which is the point of both
        // being one vocabulary: a reader who set their hooks up here meets the same word when they
        // come to connect it.
        label: "GitHub Copilot CLI",
        // The machine's, out of the two this app reads. Its other road is the folder's own `.mcp.json`
        // — the very file `claude-code` above already names, under the very same word — and a row
        // pointing there would put two apps on one entry: setting either up would have the read that
        // asks which apps are configured answer both, and taking one back out would take the other
        // with it. The home file is this app's alone, so it is the one the row names (`AMB-T-3178`).
        place: Place::Home(".copilot/mcp-config.json"),
        format: Format::Json,
        servers_key: "mcpServers",
        amenbo_writes: false,
        extensions: None,
    },
];

/// The app with this [`id`](McpApp::id), or `None` when nothing lists it.
pub fn find(id: &str) -> Option<&'static McpApp> {
    MCP_APPS.iter().find(|app| app.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_findable() {
        let mut seen = std::collections::HashSet::new();
        for app in MCP_APPS {
            assert!(seen.insert(app.id), "`{}` is listed twice", app.id);
            assert!(std::ptr::eq(find(app.id).expect("listed"), app), "`{}` is findable", app.id);
        }
        assert!(find("nothing-lists-this").is_none());
    }

    /// A key is a key, not a rendering: a face writing one into a URL, a flag or a JSON field should
    /// never have to quote it.
    #[test]
    fn ids_are_lowercase_and_hyphenated() {
        for app in MCP_APPS {
            assert!(
                app.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "`{}` is not a plain token",
                app.id
            );
            assert!(!app.label.is_empty(), "`{}` has no name to show", app.id);
        }
    }

    /// The format is what a face fences a document with, so it has to agree with the file it names —
    /// a row saying TOML about a `.json` would hand a reader a document their app cannot read.
    #[test]
    fn the_format_agrees_with_the_file_the_row_names() {
        for app in MCP_APPS {
            let path = match app.place {
                Place::Settings(p) | Place::Home(p) | Place::Folder(p) => p,
            };
            assert!(
                path.ends_with(&format!(".{}", app.format.as_str())),
                "`{}` says {:?} and names `{path}`",
                app.id,
                app.format
            );
            assert!(!path.starts_with('/'), "`{}` names an absolute path: `{path}`", app.id);
        }
    }

    /// A folder's own settings sit in the folder, and the machine's do not — which is the whole of
    /// what the distinction buys a reader, so it is worth holding rather than assuming.
    #[test]
    fn a_folders_place_lands_in_that_folder_and_the_machines_does_not() {
        let folder = Path::new("/tmp/amenbo-mcp-apps-test");
        for app in MCP_APPS {
            let Some(path) = app.settings_path(folder) else {
                continue; // No home directory on this machine; there is nothing to place against.
            };
            assert_eq!(
                path.starts_with(folder),
                !app.place.device_wide(),
                "`{}` placed its settings at {}",
                app.id,
                path.display()
            );
        }
    }

    /// The one app amenbo writes a file for (`AMB-D-672`). A second one appearing means either a new
    /// app that cannot run a command, or the flag being read as a preference — and the second is what
    /// this catches.
    #[test]
    fn amenbo_writes_the_settings_for_one_app() {
        let written: Vec<&str> =
            MCP_APPS.iter().filter(|app| app.amenbo_writes).map(|app| app.id).collect();
        assert_eq!(written, vec!["claude-desktop"]);
    }
}
