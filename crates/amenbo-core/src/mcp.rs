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
