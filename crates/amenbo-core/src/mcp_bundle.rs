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
//! **One bundle per machine, because the name is the machine's** (`AMB-D-679`). A host keeps its
//! extensions under the name the manifest gives, and that name is [`crate::mcp::name`] — shared with
//! every other road a server is set up by, so a bundle and a request that named one thing differently
//! would leave one machine set up twice under two names. A reader who saves this file again therefore
//! replaces the extension they already had rather than adding a second one beside it.
//!
//! **The manifest carries the required fields and nothing else.** Every optional word is one more
//! thing a host may read differently than the version of the spec it was written against, and none of
//! them changes what the reader gets. English, like the request the other apps are handed
//! ([`crate::harness::request`]): these are a document's identifiers, and the call has no reader's
//! language to write them in.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::mcp::Server;

/// The name of the one document inside the archive, at its root — the whole of the layout an `.mcpb`
/// is held to.
const MANIFEST: &str = "manifest.json";

/// The extension a host recognises the archive by. It is the file's whole affordance: the reader is
/// told to open it, and the app is the thing that opens it.
pub const EXTENSION: &str = "mcpb";

/// The spec version the manifest below is written against.
const MANIFEST_VERSION: &str = "0.3";

/// What to call the file the reader saves. It is the server's own name, so a bundle written again
/// lands on the file it landed on before rather than piling a second one up beside it.
pub fn file_name() -> String {
    format!("{}.{EXTENSION}", crate::mcp::name())
}

/// The manifest document, as it sits at the archive's root.
pub fn manifest(server: &Server) -> String {
    let document = serde_json::json!({
        "manifest_version": MANIFEST_VERSION,
        "name": crate::mcp::name(),
        "version": crate::agent::VERSION,
        // The product's name as a person reads it (`AMB-D-633`) — this line is the one part of the
        // document a reader sees, and the lowercase spelling is the command's and the identifier's.
        "description": format!(
            "Work the Amenbo project \"{}\" from this app — its backlog, its decisions and the way \
             to use them, for the folder {}.",
            server.project,
            server.folder.display()
        ),
        "author": { "name": "Amenbo" },
        "server": {
            "type": "binary",
            // The entry point of a bundle that carries no code is the binary it points at. It is
            // the same path the command below names, and it is absolute for the same reason: there
            // is nothing inside the archive for a relative one to reach.
            "entry_point": server.command(),
            "mcp_config": {
                "command": server.command(),
                "args": server.args(),
            },
        },
    });
    serde_json::to_string_pretty(&document).unwrap_or_else(|_| document.to_string())
}

/// Write the bundle into `dir`, and hand back the file that was written.
///
/// The archive holds the manifest and nothing else, which is what a bundle carrying no code is.
pub fn write_into(server: &Server, dir: &Path) -> std::io::Result<PathBuf> {
    let path = dir.join(file_name());
    let mut archive = zip::ZipWriter::new(std::fs::File::create(&path)?);
    archive
        .start_file(MANIFEST, zip::write::SimpleFileOptions::default())
        .map_err(std::io::Error::other)?;
    archive.write_all(manifest(server).as_bytes())?;
    archive.finish().map_err(std::io::Error::other)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server<'a>(folder: &'a Path, exe: &'a Path) -> Server<'a> {
        Server { project: "Shop", folder, exe }
    }

    /// The fields a host refuses a manifest for want of, and the arguments that bind the server to one
    /// folder — the whole of what this document is for.
    #[test]
    fn the_manifest_carries_what_a_host_asks_for_and_the_folder_it_is_bound_to() {
        let folder = Path::new("/work/shop");
        let exe = Path::new("/usr/local/bin/amenbo");
        let read: serde_json::Value =
            serde_json::from_str(&manifest(&server(folder, exe))).expect("valid JSON");

        for field in ["manifest_version", "name", "version", "description", "author", "server"] {
            assert!(!read[field].is_null(), "the manifest says nothing about `{field}`");
        }
        assert_eq!(read["name"], "amenbo");
        assert_eq!(read["author"]["name"], "Amenbo");
        assert_eq!(read["server"]["type"], "binary");
        assert_eq!(read["server"]["mcp_config"]["command"], "/usr/local/bin/amenbo");
        assert_eq!(
            read["server"]["mcp_config"]["args"],
            serde_json::json!(["mcp", "--dir", "/work/shop"])
        );
    }

    /// The file the reader saves is named after the server, and the server's name is the machine's —
    /// so a bundle written twice lands where the first one did rather than piling a second file up
    /// beside it.
    #[test]
    fn the_file_is_named_after_the_server() {
        assert_eq!(file_name(), "amenbo.mcpb");
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
        let written = write_into(&server(folder, exe), &dir).expect("the bundle is written");

        assert_eq!(written.file_name().and_then(|n| n.to_str()), Some("amenbo.mcpb"));
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
        assert_eq!(held, manifest(&server(folder, exe)));

        std::fs::remove_dir_all(&dir).ok();
    }
}
