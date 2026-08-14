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

/// Whether `candidate` is a name amenbo filed a server under **before** the name said the machine
/// (`AMB-D-679`): `<command>-<the project's slug>`, where [`name`] is now the command alone.
///
/// An entry under one of these keeps working, so nothing about it looks broken — and setting amenbo up
/// again writes the new name beside it rather than over it, which is the doubling `AMB-D-679` set out
/// to be rid of. Finding them is therefore a read of its own ([`crate::mcp_probe`]), and clearing one
/// is a request the reader is handed ([`crate::mcp_request::remove_stale`]).
///
/// The test is the old name's shape and nothing more: a slug is lower-case alphanumerics joined with
/// `-` ([`crate::slug`]), so whatever follows the separator that could have been one is taken for one.
/// It cannot be narrower. On production the command is `amenbo`, which leaves a dev build's old entry
/// (`amenbo-dev-shop`) reading exactly like a project actually named "dev shop" — and a rule carved to
/// exclude the first would take the second off the list of entries its owner is offered a way to clear.
pub fn is_superseded_name(candidate: &str) -> bool {
    candidate.strip_prefix(name()).and_then(|rest| rest.strip_prefix('-')).is_some_and(is_slug_shaped)
}

/// Whether a word could be a project's slug: something rather than nothing, lower-case ASCII
/// alphanumerics and `-`, and no `-` left at either end ([`crate::slug::base`]).
fn is_slug_shaped(word: &str) -> bool {
    !word.is_empty()
        && !word.starts_with('-')
        && !word.ends_with('-')
        && word.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
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

    /// The old name is the current one with a project's slug hung off it, and every shape a slug can
    /// take has to be caught: a plain word, one squeezed out of a longer name, and one that took a
    /// number to stay unique.
    #[test]
    fn a_name_with_a_slug_hung_off_it_is_the_one_that_was_superseded() {
        for old in ["amenbo-shop", "amenbo-web-site-2026", "amenbo-shop-2", "amenbo-project"] {
            assert!(is_superseded_name(old), "{old} is an old name");
        }
    }

    /// What is not one. The entry in use is the case that matters most — offering to delete it would
    /// take away the server the reader just set up — and the rest is somebody else's entry, which this
    /// is not entitled to name at all.
    #[test]
    fn neither_the_name_in_use_nor_a_strangers_is_read_as_superseded() {
        for other in ["amenbo", "amenbo-", "amenboshop", "amenbo-Shop", "amenbo-shop!", "something-else"] {
            assert!(!is_superseded_name(other), "{other} is not an old name");
        }
    }
}
