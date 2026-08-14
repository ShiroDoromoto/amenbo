//! The one file Claude Desktop takes a server from — an `.mcpb` bundle carrying the server this
//! machine already has (`AMB-D-672`).
//!
//! **Why a bundle and not a settings document.** Every other app amenbo lists can run a command, so
//! the road there is to ask the AI already sitting in it to write the settings itself
//! ([`crate::mcp_apps`]). This one cannot run anything, so there is nobody in it to ask — and it is
//! also the only one that takes a server by being handed a file (`AMB-T-3122`). The file is what is
//! left, and it is a whole road rather than a fallback: the reader saves it, opens it, and the app
//! puts the server in.
//!
//! **What is inside is a pointer, not a program.** A bundle may carry the server's own code; this one
//! carries none. amenbo is already on the machine — a whole installer put it there — so what the
//! manifest names is the binary that is standing, and the arguments that bind it to one folder
//! (`AMB-D-666`). Shipping a copy of amenbo inside would be a second amenbo to keep in step with the
//! first.
//!
//! **One bundle per project, and the name is what keeps them apart.** A server is bound to one folder,
//! so a machine working two projects wants two of these — and a host keeps its extensions under the
//! name the manifest gives. The project's slug is already unique on this machine
//! ([`crate::slug`]), so it is what the name is built from.
//!
//! **The manifest carries the required fields and nothing else.** Every optional word is one more
//! thing a host may read differently than the version of the spec it was written against, and none of
//! them changes what the reader gets. English, like the request the other apps are handed
//! ([`crate::harness::request`]): these are a document's identifiers, and the call has no reader's
//! language to write them in.

use std::io::Write;
use std::path::{Path, PathBuf};

/// The name of the one document inside the archive, at its root — the whole of the layout an `.mcpb`
/// is held to.
const MANIFEST: &str = "manifest.json";

/// The extension a host recognises the archive by. It is the file's whole affordance: the reader is
/// told to open it, and the app is the thing that opens it.
pub const EXTENSION: &str = "mcpb";

/// The spec version the manifest below is written against.
const MANIFEST_VERSION: &str = "0.3";

/// One project's server, as a bundle names it.
pub struct Bundle<'a> {
    /// The project's slug — unique on this machine, which is what makes the bundle's own name unique
    /// among the ones a host is holding.
    pub slug: &'a str,
    /// The project's name as its owner wrote it, for the line a reader sees beside the extension.
    pub project: &'a str,
    /// The folder the server is bound to. It is written into the arguments rather than asked for at
    /// install time: the reader is setting up a project they are already standing in, and a question
    /// asked here would be one they have answered already.
    pub folder: &'a Path,
    /// The installed amenbo binary the host will run. The caller resolves it — the GUI knows where the
    /// command it ships sits, and a bundle written against whatever binary happened to be running
    /// would name the wrong one from inside an app.
    pub exe: &'a Path,
}

impl Bundle<'_> {
    /// The machine-readable name the host files this extension under. Two projects give two names, and
    /// the same project gives the same one — so writing a bundle again replaces the extension rather
    /// than standing a second one beside it.
    ///
    /// It opens with this build's own command name rather than the product's, for the same reason the
    /// channels are three different commands: a bundle written by a dev build names the dev binary,
    /// and one landing on the production build's name would put that binary behind the extension the
    /// reader already had.
    pub fn name(&self) -> String {
        format!("{}-{}", crate::config::Paths::command_name(), self.slug)
    }

    /// What to call the file the reader saves.
    pub fn file_name(&self) -> String {
        format!("{}.{EXTENSION}", self.name())
    }

    /// The manifest document, as it sits at the archive's root.
    pub fn manifest(&self) -> String {
        let document = serde_json::json!({
            "manifest_version": MANIFEST_VERSION,
            "name": self.name(),
            "version": crate::agent::VERSION,
            // The product's name as a person reads it (`AMB-D-633`) — this line is the one part of the
            // document a reader sees, and the lowercase spelling is the command's and the identifier's.
            "description": format!(
                "Work the Amenbo project \"{}\" from this app — its backlog, its decisions and the way \
                 to use them, for the folder {}.",
                self.project,
                self.folder.display()
            ),
            "author": { "name": "Amenbo" },
            "server": {
                "type": "binary",
                // The entry point of a bundle that carries no code is the binary it points at. It is
                // the same path the command below names, and it is absolute for the same reason: there
                // is nothing inside the archive for a relative one to reach.
                "entry_point": self.exe.display().to_string(),
                "mcp_config": {
                    "command": self.exe.display().to_string(),
                    "args": ["mcp", "--dir", self.folder.display().to_string()],
                },
            },
        });
        serde_json::to_string_pretty(&document).unwrap_or_else(|_| document.to_string())
    }

    /// Write the bundle into `dir`, and hand back the file that was written.
    ///
    /// The archive holds the manifest and nothing else, which is what a bundle carrying no code is.
    pub fn write_into(&self, dir: &Path) -> std::io::Result<PathBuf> {
        let path = dir.join(self.file_name());
        let mut archive = zip::ZipWriter::new(std::fs::File::create(&path)?);
        archive
            .start_file(MANIFEST, zip::write::SimpleFileOptions::default())
            .map_err(std::io::Error::other)?;
        archive.write_all(self.manifest().as_bytes())?;
        archive.finish().map_err(std::io::Error::other)?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle<'a>(folder: &'a Path, exe: &'a Path) -> Bundle<'a> {
        Bundle { slug: "shop", project: "Shop", folder, exe }
    }

    /// The fields a host refuses a manifest for want of, and the arguments that bind the server to one
    /// folder — the whole of what this document is for.
    #[test]
    fn the_manifest_carries_what_a_host_asks_for_and_the_folder_it_is_bound_to() {
        let folder = Path::new("/work/shop");
        let exe = Path::new("/usr/local/bin/amenbo");
        let read: serde_json::Value =
            serde_json::from_str(&bundle(folder, exe).manifest()).expect("valid JSON");

        for field in ["manifest_version", "name", "version", "description", "author", "server"] {
            assert!(!read[field].is_null(), "the manifest says nothing about `{field}`");
        }
        assert_eq!(read["name"], "amenbo-shop");
        assert_eq!(read["author"]["name"], "Amenbo");
        assert_eq!(read["server"]["type"], "binary");
        assert_eq!(read["server"]["mcp_config"]["command"], "/usr/local/bin/amenbo");
        assert_eq!(
            read["server"]["mcp_config"]["args"],
            serde_json::json!(["mcp", "--dir", "/work/shop"])
        );
    }

    /// Two projects on one machine are two extensions, and a host keeps them apart by the name the
    /// manifest gives. Writing the same project again is the same name, which is how a bundle written
    /// twice replaces itself rather than piling up.
    #[test]
    fn a_projects_name_is_its_own_and_stays_the_same() {
        let folder = Path::new("/work/shop");
        let exe = Path::new("/usr/local/bin/amenbo");
        let shop = bundle(folder, exe);
        let other = Bundle { slug: "greenhouse", ..bundle(folder, exe) };

        assert_ne!(shop.name(), other.name());
        assert_eq!(shop.name(), bundle(folder, exe).name());
        assert_eq!(shop.file_name(), "amenbo-shop.mcpb");
    }

    /// What a host opens: an archive with the manifest at its root, and the same document that was
    /// derived. The layout is the one thing a bundle is held to, so it is read back rather than
    /// assumed.
    #[test]
    fn the_archive_holds_the_manifest_at_its_root() {
        let dir = std::env::temp_dir().join(format!("amenbo-mcpb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a directory to write into");
        let folder = Path::new("/work/shop");
        let exe = Path::new("/usr/local/bin/amenbo");
        let written = bundle(folder, exe).write_into(&dir).expect("the bundle is written");

        assert_eq!(written.file_name().and_then(|n| n.to_str()), Some("amenbo-shop.mcpb"));
        let mut archive =
            zip::ZipArchive::new(std::fs::File::open(&written).expect("the file is there"))
                .expect("a zip");
        assert_eq!(archive.len(), 1, "the archive carries the manifest and nothing else");
        let mut held = String::new();
        std::io::Read::read_to_string(
            &mut archive.by_name(MANIFEST).expect("the manifest is at the root"),
            &mut held,
        )
        .expect("readable");
        assert_eq!(held, bundle(folder, exe).manifest());

        std::fs::remove_dir_all(&dir).ok();
    }
}
