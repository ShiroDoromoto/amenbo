//! amenbo as an app on the other side of MCP reaches it: the command a host starts, and the folder it
//! is given (`AMB-D-666`, amended by `AMB-D-679`). The server itself takes as many folders as it is
//! given and a call names one of them, so what is written from here is a set of one.
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

use std::path::Path;

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
/// The flag takes **one or more** folders on the server's side (`AMB-D-679`), and a call names the one
/// it is for. What is written from here is still a single folder: the set an entry carries is the
/// person's choice, and the faces where that choice is made are not these.
pub const DIR_FLAG: &str = "--dir";

/// amenbo, as an MCP server: what a host is told to run, and where. The name it is filed under is the
/// machine's rather than this struct's ([`name`]).
pub struct Server<'a> {
    /// The project's name as its owner wrote it, for the line a reader sees beside the server.
    pub project: &'a str,
    /// The folder the server is bound to. It is settled here rather than asked for at install time:
    /// the reader is setting up a project they are already standing in, and a question asked on the
    /// other side would be one they have answered already.
    pub folder: &'a Path,
    /// The installed amenbo binary a host will run. The caller resolves it — the GUI knows where the
    /// command it ships sits, and a path taken from whatever binary happened to be running would name
    /// the app rather than the command from inside one.
    pub exe: &'a Path,
}

impl Server<'_> {
    /// The arguments that start it, after the command itself: the server, bound to its folder.
    pub fn args(&self) -> Vec<String> {
        vec!["mcp".to_string(), DIR_FLAG.to_string(), self.folder.display().to_string()]
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
    fn a_server_is_a_command_and_the_folder_it_is_bound_to() {
        let server = Server {
            project: "Shop",
            folder: Path::new("/work/shop"),
            exe: Path::new("/usr/local/bin/amenbo"),
        };
        assert_eq!(server.command(), "/usr/local/bin/amenbo");
        assert_eq!(server.args(), vec!["mcp", "--dir", "/work/shop"]);
    }

    /// The name is the machine's, so a second project set up on it lands on the entry the first one
    /// left rather than standing a second beside it — which is the whole of what `AMB-D-679` asks the
    /// name for.
    #[test]
    fn the_name_says_the_machine_and_not_the_project() {
        assert_eq!(name(), "amenbo");
    }
}
