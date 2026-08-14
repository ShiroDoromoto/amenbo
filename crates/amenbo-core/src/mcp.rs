//! amenbo as an app on the other side of MCP reaches it: the command a host starts, and the folders it
//! is given (`AMB-D-666`, amended by `AMB-D-679`). A call names one of them, so what an AI chooses is
//! which folder this call is for — never which folders there are.
//!
//! A server is nothing but the command that starts it and the folders it is bound to. Which project
//! each folder belongs to is worked out by the amenbo the command starts, standing there — so what a
//! host is told is a path and an argument, never a project id.
//!
//! **The name says the machine, not the project** (`AMB-D-679`). One server can be given several
//! folders, so there is no one project whose slug could name it — and a name that still divided by
//! project would keep a host showing a list as long as the reader's backlog. So there is a single name
//! here, and every road writes it: the bundle a host is handed ([`crate::mcp_bundle`]), the request an
//! app's AI is handed ([`crate::mcp_request`]), and the read that asks later whether an app is already
//! set up (`AMB-D-673`, [`crate::mcp_probe`]). Setting amenbo up a second time therefore lands on the
//! entry that is already there rather than beside it.

use std::path::{Path, PathBuf};

/// What a host files amenbo under — one name for this machine, whatever folders the server is given
/// (`AMB-D-679`).
///
/// It is this build's own command name rather than the product's, for the same reason the channels are
/// three different commands: a dev build names the dev binary, and an entry landing on the production
/// build's name would put that binary behind the server the reader already had.
pub fn name() -> &'static str {
    crate::config::Paths::command_name()
}

/// The word that binds a server to its folders. It is written into an entry by whatever sets one up and
/// read back out of one by whatever asks whether a project is already set up ([`crate::mcp_probe`]), so
/// the two say it once between them rather than once each.
///
/// The flag takes **one or more** folders (`AMB-D-679`), all of them under the one flag, and a call
/// names the one it is for. One flag rather than one per folder is not a preference: it is the shape
/// every settings format a host reads can express (`AMB-T-3156`).
pub const DIR_FLAG: &str = "--dir";

/// amenbo, as an MCP server: what a host is told to run, and where. The name it is filed under is the
/// machine's rather than this struct's ([`name`]).
pub struct Server<'a> {
    /// The folders the server is given, in the order the person chose them (`AMB-D-679`). They are
    /// settled here rather than asked for at install time: the reader is setting up projects they have
    /// already told amenbo about, and a question asked on the other side would be one they have
    /// answered already.
    pub folders: &'a [PathBuf],
    /// The installed amenbo binary a host will run. The caller resolves it — the GUI knows where the
    /// command it ships sits, and a path taken from whatever binary happened to be running would name
    /// the app rather than the command from inside one.
    pub exe: &'a Path,
}

impl Server<'_> {
    /// The arguments that start it, after the command itself: the server, and every folder it is
    /// given. One flag carries them all, because that is the one shape a host can write — a flag
    /// repeated per folder is not something every settings format can express (`AMB-T-3156`).
    pub fn args(&self) -> Vec<String> {
        let mut args = vec!["mcp".to_string(), DIR_FLAG.to_string()];
        args.extend(self.folders.iter().map(|folder| folder.display().to_string()));
        args
    }

    /// The folder an app that keeps its settings *inside* one is set up from, where there is one.
    ///
    /// A server spans folders and a workspace-scoped settings file does not, so one of them has to be
    /// the file's home. It is the first the person named, which is not a tie broken by chance: the
    /// order is theirs, and the folder they started from is the one they are standing in.
    pub fn settings_folder(&self) -> Option<&Path> {
        self.folders.first().map(PathBuf::as_path)
    }

    /// The command, as a host writes it down — an absolute path, because the host is not a shell and
    /// has no `PATH` of the reader's to look the name up in.
    pub fn command(&self) -> String {
        self.exe.display().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_is_a_command_and_the_folders_it_is_given() {
        let one = vec![PathBuf::from("/work/shop")];
        let server = Server { folders: &one, exe: Path::new("/usr/local/bin/amenbo") };
        assert_eq!(server.command(), "/usr/local/bin/amenbo");
        assert_eq!(server.args(), vec!["mcp", "--dir", "/work/shop"]);
        assert_eq!(server.settings_folder(), Some(Path::new("/work/shop")));
    }

    /// Several folders ride one flag, in the order they were given — the shape a host can write, and
    /// the order the person chose.
    #[test]
    fn every_folder_rides_the_one_flag_in_the_order_it_was_given() {
        let several =
            vec![PathBuf::from("/work/shop"), PathBuf::from("/work/greenhouse")];
        let server = Server { folders: &several, exe: Path::new("/bin/amenbo") };
        assert_eq!(server.args(), vec!["mcp", "--dir", "/work/shop", "/work/greenhouse"]);
        assert_eq!(
            server.settings_folder(),
            Some(Path::new("/work/shop")),
            "a settings file that lives in a folder lives in the first one",
        );

        let none: Vec<PathBuf> = Vec::new();
        let empty = Server { folders: &none, exe: Path::new("/bin/amenbo") };
        assert_eq!(empty.args(), vec!["mcp", "--dir"], "a server with no folder is one nobody set up");
        assert_eq!(empty.settings_folder(), None);
    }

    /// The name is the machine's, so a second project set up on it lands on the entry the first one
    /// left rather than standing a second beside it — which is the whole of what `AMB-D-679` asks the
    /// name for.
    #[test]
    fn the_name_says_the_machine_and_not_the_project() {
        assert_eq!(name(), "amenbo");
    }
}
