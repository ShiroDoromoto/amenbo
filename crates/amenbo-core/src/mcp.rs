//! amenbo as an app on the other side of MCP reaches it: one server per folder (`AMB-D-666`).
//!
//! A server is nothing but the command that starts it and the folder it is bound to. Which project
//! that folder belongs to is worked out by the amenbo the command starts, standing there — so what a
//! host is told is a path and an argument, never a project id.
//!
//! **The name is the whole of what keeps two of them apart.** A machine working two projects wants two
//! servers, and every host — the one handed a bundle ([`crate::mcp_bundle`]) and the ones handed a
//! request ([`crate::mcp_request`]) — files what it is given under the name it was given. So the name
//! is derived once, here, and read back by whatever asks later whether a project is already set up
//! (`AMB-D-673`).

use std::path::Path;

/// The word that binds a server to its folder. It is written into an entry by whatever sets one up and
/// read back out of one by whatever asks whether a project is already set up ([`crate::mcp_probe`]), so
/// the two say it once between them rather than once each.
pub const DIR_FLAG: &str = "--dir";

/// amenbo, as one project's MCP server.
pub struct Server<'a> {
    /// The project's slug — unique on this machine (`crate::slug`), which is what makes the server's
    /// name unique among the ones a host is holding.
    pub slug: &'a str,
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
    /// What a host files this server under. Two projects give two names, and the same project gives
    /// the same one — so setting a project up twice replaces the entry rather than standing a second
    /// one beside it.
    ///
    /// It opens with this build's own command name rather than the product's, for the same reason the
    /// channels are three different commands: a dev build names the dev binary, and an entry landing
    /// on the production build's name would put that binary behind the server the reader already had.
    pub fn name(&self) -> String {
        format!("{}-{}", crate::config::Paths::command_name(), self.slug)
    }

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
            slug: "shop",
            project: "Shop",
            folder: Path::new("/work/shop"),
            exe: Path::new("/usr/local/bin/amenbo"),
        };
        assert_eq!(server.name(), "amenbo-shop");
        assert_eq!(server.command(), "/usr/local/bin/amenbo");
        assert_eq!(server.args(), vec!["mcp", "--dir", "/work/shop"]);
    }

    /// Two projects are two servers, and one project is one — which is what a host needs of the name
    /// for a second setup to replace the first rather than pile on it.
    #[test]
    fn the_name_follows_the_project_and_nothing_else() {
        let folder = Path::new("/work/shop");
        let exe = Path::new("/usr/local/bin/amenbo");
        let shop = Server { slug: "shop", project: "Shop", folder, exe };
        let again = Server { slug: "shop", project: "Shop renamed", folder, exe };
        let other = Server { slug: "greenhouse", project: "Greenhouse", folder, exe };

        assert_eq!(shop.name(), again.name());
        assert_ne!(shop.name(), other.name());
    }
}
