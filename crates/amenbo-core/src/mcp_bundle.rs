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
//! manifest names is the binary that is standing, and the arguments that start it (`AMB-D-666`).
//! Shipping a copy of amenbo inside would be a second amenbo to keep in step with the first.
//!
//! **The folders are the host's to keep here, and amenbo's only to start with** (`AMB-D-681`). Every
//! other app is handed the set written out; this one keeps it in a setting of its own, so what the
//! manifest carries is the field, a placeholder standing where the folders go, and the choice the
//! reader already made as that field's opening value. It is an opening value in the strict sense:
//! the moment the reader saves the settings screen their own answer is stored and outranks it, and a
//! bundle written again cannot move a folder they have chosen (`AMB-T-3157`). So this file sets a
//! reader up once, and the screen it points at is where the set changes afterwards.
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

/// The name of the one setting this manifest declares, and the placeholder that puts what it holds on
/// the command line. A host expands the placeholder where it stands in `args`, one argv per folder —
/// which is what makes `args` the road the set travels: the same placeholder in `env` arrives as the
/// literal text, since a variable there has one string to be (`AMB-T-3156`).
///
/// The read that asks later which folders an app is set up for looks the word up here
/// ([`crate::mcp_probe`]), so the two say it once between them rather than once each.
pub(crate) const FOLDERS_KEY: &str = "folders";

/// Who the manifest says wrote this bundle. It is a word in the document, and it is also half of the
/// id a host files the extension under — which is why it is a constant rather than a literal in the
/// document below.
const AUTHOR: &str = "Amenbo";

/// What a host files this bundle's extension under: the local-bundle prefix, the author lowercased,
/// and the manifest's name (`AMB-T-3156`).
///
/// It is derived here because it is derived from this document — a reader asking later whether the app
/// is set up is asking after the extension *this* bundle became, and an id worked out anywhere else
/// would be a second opinion about a name only the manifest decides.
pub fn extension_id() -> String {
    format!("local.mcpb.{}.{}", AUTHOR.to_lowercase(), crate::mcp::name())
}

/// What to call the file the reader saves. It is the server's own name, so a bundle written again
/// lands on the file it landed on before rather than piling a second one up beside it.
pub fn file_name() -> String {
    format!("{}.{EXTENSION}", crate::mcp::name())
}

/// The folders the setting starts out holding — the ones this server was written for.
///
/// It is a list whatever it holds, because the field is one: a host reading a bare string where a
/// list was declared has a shape to reconcile that nobody meant it to have.
fn folders(server: &Server) -> Vec<String> {
    server.folders.iter().map(|folder| folder.display().to_string()).collect()
}

/// The manifest document, as it sits at the archive's root.
pub fn manifest(server: &Server) -> String {
    let document = serde_json::json!({
        "manifest_version": MANIFEST_VERSION,
        "name": crate::mcp::name(),
        "version": crate::agent::VERSION,
        // The product's name as a person reads it (`AMB-D-633`) — this line is the one part of the
        // document a reader sees, and the lowercase spelling is the command's and the identifier's.
        // The folders are named below rather than here: they are a setting the reader can change, and
        // a description repeating them would go stale the first time they do.
        "description": "Work your Amenbo projects from this app — their backlogs, their decisions \
                        and the way to use them, for the folders set below.",
        "author": { "name": AUTHOR },
        // The one place this app differs from every other: the folders the server works in are the
        // host's to keep, not amenbo's (`AMB-D-681`). What is declared here is the field they live
        // in, and what is written into it is the choice the reader already made in amenbo — so the
        // reader who installs this and does nothing else is already set up for that folder.
        //
        // **`required` is not written, deliberately.** With it, the host counts the field as
        // unanswered however full it looks, refuses to enable the extension, and sends the reader to
        // the settings screen to press save on a value they never typed (`AMB-T-3157`). Without it,
        // installing is the whole of what they do. Nothing is lost by leaving it out: the field is
        // never empty unless a person empties it, and a person emptying it is the set being theirs.
        "user_config": {
            FOLDERS_KEY: {
                "type": "directory",
                "title": "Project folders",
                "description": "The folders this server works in. Each call from the AI names one of them, and each folder's project is whatever amenbo is set up for there.",
                "multiple": true,
                // What the reader gets on a first install — and only a first install. Once they save
                // the settings screen, their own answer is stored and outranks this, so a bundle
                // written again cannot move a folder they have chosen (`AMB-T-3157`); the way to
                // change it from then on is that screen.
                "default": folders(server),
            },
        },
        "server": {
            "type": "binary",
            // The entry point of a bundle that carries no code is the binary it points at. It is
            // the same path the command below names, and it is absolute for the same reason: there
            // is nothing inside the archive for a relative one to reach.
            "entry_point": server.command(),
            "mcp_config": {
                "command": server.command(),
                // The folders are named by the placeholder rather than written out, because the
                // field above is what holds them from here on. It stands where a folder would, and
                // the host puts as many argv there as the field has.
                "args": [
                    "mcp",
                    crate::mcp::DIR_FLAG,
                    format!("${{user_config.{FOLDERS_KEY}}}"),
                ],
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

    fn one(folder: &str) -> Vec<PathBuf> {
        vec![PathBuf::from(folder)]
    }

    fn server<'a>(folders: &'a [PathBuf], exe: &'a Path) -> Server<'a> {
        Server { folders, exe }
    }

    /// The fields a host refuses a manifest for want of, and the arguments that start the server —
    /// the whole of what this document is for.
    #[test]
    fn the_manifest_carries_what_a_host_asks_for_and_the_words_that_start_the_server() {
        let folder = one("/work/shop");
        let exe = Path::new("/usr/local/bin/amenbo");
        let read: serde_json::Value =
            serde_json::from_str(&manifest(&server(&folder, exe))).expect("valid JSON");

        for field in ["manifest_version", "name", "version", "description", "author", "server"] {
            assert!(!read[field].is_null(), "the manifest says nothing about `{field}`");
        }
        assert_eq!(read["name"], "amenbo");
        assert_eq!(read["author"]["name"], "Amenbo");
        assert_eq!(read["server"]["type"], "binary");
        assert_eq!(read["server"]["mcp_config"]["command"], "/usr/local/bin/amenbo");
        // The folder is not written out here: the setting holds it, and the placeholder is where the
        // host puts what the setting holds — one argv per folder, in this position.
        assert_eq!(
            read["server"]["mcp_config"]["args"],
            serde_json::json!(["mcp", "--dir", "${user_config.folders}"])
        );
    }

    /// The setting the folders live in, and the value it opens with. The shape is what a host reads
    /// the field's affordance off — a folder picker that takes several — and the opening value is the
    /// choice the reader already made, so installing is the whole of what they do.
    #[test]
    fn the_folders_are_a_setting_that_opens_on_the_project_the_reader_chose() {
        let folder = one("/work/shop");
        let exe = Path::new("/usr/local/bin/amenbo");
        let read: serde_json::Value =
            serde_json::from_str(&manifest(&server(&folder, exe))).expect("valid JSON");
        let declared = &read["user_config"]["folders"];

        assert_eq!(declared["type"], "directory");
        assert_eq!(declared["multiple"], true, "a set, not a folder");
        assert_eq!(declared["default"], serde_json::json!(["/work/shop"]));
        assert!(
            declared["title"].as_str().is_some_and(|t| !t.is_empty())
                && declared["description"].as_str().is_some_and(|d| !d.is_empty()),
            "the reader edits this field on a screen, so it is named there: {declared}",
        );
        // Writing `required` is what stops the host enabling the extension on its own, however full
        // the field looks — and the reader is then sent to press save on a value they never typed
        // (`AMB-T-3157`).
        assert!(declared["required"].is_null(), "nothing here asks the reader to answer twice");
    }

    /// The id a host files this bundle under, built from the two words the manifest carries. It is
    /// what the read that asks later whether the app is set up looks for ([`crate::mcp_probe`]), so a
    /// change to either word here is a change to what that read finds.
    #[test]
    fn the_extension_is_filed_under_the_author_and_the_name_the_manifest_carries() {
        let read: serde_json::Value =
            serde_json::from_str(&manifest(&server(&one("/work/shop"), Path::new("/bin/a"))))
                .expect("valid JSON");
        assert_eq!(extension_id(), "local.mcpb.amenbo.amenbo");
        assert_eq!(read["author"]["name"], "Amenbo", "the author, before the host lowercases it");
        assert_eq!(read["name"], "amenbo", "and the name, as it stands");
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
        let folder = one("/work/shop");
        let exe = Path::new("/usr/local/bin/amenbo");
        let written = write_into(&server(&folder, exe), &dir).expect("the bundle is written");

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
        assert_eq!(held, manifest(&server(&folder, exe)));

        std::fs::remove_dir_all(&dir).ok();
    }
}
