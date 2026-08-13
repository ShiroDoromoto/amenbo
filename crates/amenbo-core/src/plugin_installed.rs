//! The **installed registry** — which plugins are actually on this machine, read off
//! `<base>/plugins/` (`AMB-D-350`).
//!
//! [`plugin_subscribe`](crate::plugin_subscribe) is handed the set of installed plugins rather than
//! discovering it, and says so: *how* an install lands a binary and its manifest on disk is the install
//! lifecycle's. This module is that half — the one place that knows the on-disk shape of an installed
//! plugin, so the dispatch mount, the CLI faces and `uninstall` all read the same layout.
//!
//! **One plugin's home is `<base>/plugins/<name>/`**, holding exactly four things this layer knows about:
//!
//! - `manifest.json` — the catalog entry the plugin was installed from ([`Manifest`]), kept beside the
//!   binary so the subscription list and config schema are readable with no network and no catalog. It is
//!   also the **install marker**: an install writes it last, so a half-finished directory is simply not
//!   installed rather than half-installed.
//! - the **executable**, named after the plugin itself (`<name>`, plus the platform's `.exe` suffix). The
//!   name is a convention, not a manifest field: the catalog entry says where to *fetch* an asset, never
//!   what to run, so nothing a third party writes can point amenbo's spawn at another path.
//! - `source.json` — which catalog it came from ([`Origin`], `AMB-D-389`). A separate file because the
//!   manifest is the *catalog's* document and this is amenbo's note about it; written before the manifest,
//!   so anything that reads as installed has it.
//! - `i18n.json` — what the catalog said about it in other languages ([`translations`], `AMB-D-622`).
//!   Separate for the same reason, and here rather than fetched because the face that reads it — the
//!   settings form — opens with no network and has to follow the reader when they change language.
//!
//! **The directory name is the identity.** `Config::plugin_enabled`, the config storage key and the secret
//! file all key off the plugin's name, so a manifest whose `name` disagrees with the directory it sits in
//! is refused rather than reconciled — the two would otherwise name different plugins in the same breath.
//! The reserved `registry` directory ([`is_reserved_plugin_name`]) is not a plugin and is skipped.
//!
//! **Reading is not the door.** A manifest's *rules* are enforced fail-closed where untrusted input enters
//! — the install/intake door (`AMB-D-354`, [`crate::plugin_validate`]). What is on disk here passed it
//! *then*, which is not the same as passing it now: the rules travel with amenbo and the manifest does
//! not, so a build carrying a newer rule meets documents admitted under older ones, and the file is a
//! plain one beside the binary that the checksum never covered. So this layer still checks only what it
//! can — that the files exist, parse, and agree on the name — and the rules are re-asked where what they
//! guard is actually used (`AMB-D-573`, [`crate::plugin_agent`] for the guide an entry point relays).
//!
//! **One broken install never hides the rest.** [`installed`] warns and skips a directory it cannot read,
//! the same best-effort posture the dispatch resolver takes with a plugin whose config will not resolve
//! (`AMB-D-352`); [`read`] — the by-name path an `enable` takes — is exact and returns the error instead.

use std::path::PathBuf;

use crate::config::{is_reserved_plugin_name, Paths};
use crate::error::{Error, ErrorCode, Msg, Result};
use crate::plugin_manifest::{Manifest, Translations};
use crate::plugin_subscribe::InstalledPlugin;

/// The file in a plugin's home holding the catalog entry it was installed from, and the marker that the
/// install finished (see the module docs).
pub const MANIFEST_FILE_NAME: &str = "manifest.json";

/// The file in a plugin's home recording which catalog it was installed from (see [`Origin`]).
pub const SOURCE_FILE_NAME: &str = "source.json";

/// The file in a plugin's home holding what the catalog said about it in other languages (see
/// [`translations`]).
pub const TRANSLATIONS_FILE_NAME: &str = "i18n.json";

/// Which catalog a plugin was installed from — the shelf an update goes back to.
///
/// A name alone is not enough to find that shelf again. Browsing and installing resolve a name across the
/// merged catalogs (`AMB-D-389`), so a catalog earlier in the order that starts publishing a name already
/// installed would become where the next update fetched from — and because each catalog answers for its
/// own key, that update would verify, against the new publisher's key. The distributor would have changed
/// with nothing to notice it by. Recording the shelf is what makes an update ask the same catalog that
/// answered the install.
///
/// The official catalog is [`Origin::Official`] rather than its URL: it is the one shelf whose identity is
/// not an address the user typed, and writing the address down would make a build pointed at a staging
/// catalog read as a different origin from the one beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The official catalog — the shelf amenbo ships a key for, and the only one needing no registration.
    Official,
    /// A catalog the user registered, named by the URL it was registered under (the identity the
    /// registration list keys on).
    Catalog(String),
}

/// How [`Origin::Official`] is written down. A registered catalog is stored as its URL, which must start
/// with `http://` or `https://` to have been registered at all, so the two can never be read for
/// each other.
const OFFICIAL_SOURCE: &str = "official";

/// The on-disk shape of `source.json` — an envelope, so a field can be added later without an older build
/// misreading it (the same discipline as the catalog and sources files).
#[derive(serde::Serialize, serde::Deserialize)]
struct SourceFile {
    source: String,
}

impl Origin {
    /// How this origin is written down (see [`OFFICIAL_SOURCE`]).
    fn as_stored(&self) -> &str {
        match self {
            Origin::Official => OFFICIAL_SOURCE,
            Origin::Catalog(url) => url,
        }
    }
}

/// Where one installed plugin's origin record sits: `<base>/plugins/<name>/source.json`.
pub fn source_path(paths: &Paths, name: &str) -> PathBuf {
    paths.plugin_dir(name).join(SOURCE_FILE_NAME)
}

/// Which catalog this install came from, or `None` when nothing says.
///
/// `None` is an install amenbo cannot place: one laid down by hand, or one made before the record
/// existed. It is not "the official catalog" — the caller decides what an unknown origin resolves
/// against, and is the one that can say so in a message.
///
/// A record that will not parse reads as unknown rather than as an error. The unknown case is the
/// careful one anyway (see [`crate::plugin_catalog::Discovery::find_from`]), so a corrupted note lands
/// where a missing one does instead of taking the whole install down with it.
pub fn origin(paths: &Paths, name: &str) -> Option<Origin> {
    let raw = std::fs::read_to_string(source_path(paths, name)).ok()?;
    let Ok(file) = serde_json::from_str::<SourceFile>(&raw) else {
        tracing::warn!(plugin = %name, "ignoring an unreadable plugin origin record");
        return None;
    };
    Some(match file.source.as_str() {
        OFFICIAL_SOURCE => Origin::Official,
        url => Origin::Catalog(url.to_string()),
    })
}

/// Record which catalog an install came from, in the plugin's home.
///
/// Called before the manifest is written, so an install that reads as finished has its origin: the
/// manifest is the install marker, and a marker that could land without this would leave an install whose
/// shelf is unknown for no reason but the order of two writes.
pub(crate) fn record_origin(paths: &Paths, name: &str, origin: &Origin) -> Result<()> {
    let dest = source_path(paths, name);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&SourceFile { source: origin.as_stored().to_string() })
        .map_err(|e| Error::Io(std::io::Error::other(e)))?;
    std::fs::write(dest, json)?;
    Ok(())
}

/// Where one installed plugin's translations sit: `<base>/plugins/<name>/i18n.json`.
pub fn translations_path(paths: &Paths, name: &str) -> PathBuf {
    paths.plugin_dir(name).join(TRANSLATIONS_FILE_NAME)
}

/// What the catalog said about this plugin in languages other than the one its author wrote it in
/// (`AMB-D-622`) — the `desc` a row draws and the labels a settings form shows, keyed by language code.
///
/// A separate file for the same reason `source.json` is one: `manifest.json` is the *catalog's*
/// document, kept as it was published, and the translations are a layer beside it rather than fields
/// inside it (`AMB-D-623` — the `Manifest` type does not change). Kept on disk at all because the faces
/// that read it open with no network: a settings form is drawn from what is beside the binary, and it
/// has to follow the reader when they change language rather than send them back to the catalog.
///
/// **Nothing here is an error.** Absent is a plugin nobody translated, and a file that will not parse
/// reads the same way — what such a plugin loses is the layer, and the base values are what a reader
/// sees, which is where an untranslated plugin already stands. Selecting between the two is the GUI's
/// (`AMB-D-623`), so this hands over both languages and lines exactly as they were published.
pub fn translations(paths: &Paths, name: &str) -> Translations {
    let Ok(raw) = std::fs::read_to_string(translations_path(paths, name)) else {
        return Translations::new();
    };
    serde_json::from_str(&raw).unwrap_or_else(|_| {
        tracing::warn!(plugin = %name, "ignoring an unreadable plugin translation record");
        Translations::new()
    })
}

/// Write what the catalog said about a plugin in other languages, in its home.
///
/// Called before the manifest, like [`record_origin`] and for the same reason: the manifest is the
/// install marker, so anything that reads as installed has the record beside it rather than acquiring it
/// one write later.
///
/// **No translations removes the file** rather than writing an empty one. This is the path an update
/// takes too, and a build whose author withdrew a language must not keep answering in it — a stale layer
/// over a new manifest is the one way this file could say something the catalog does not.
pub(crate) fn record_translations(
    paths: &Paths,
    name: &str,
    translations: &Translations,
) -> Result<()> {
    let dest = translations_path(paths, name);
    if translations.is_empty() {
        match std::fs::remove_file(&dest) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(Error::from(e)),
        }
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(translations)
        .map_err(|e| Error::Io(std::io::Error::other(e)))?;
    std::fs::write(dest, json)?;
    Ok(())
}

/// The executable's file name inside a plugin's home: the plugin's own name plus the platform's
/// executable suffix (`.exe` on Windows, empty elsewhere).
pub fn program_file_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// Where one installed plugin's manifest sits: `<base>/plugins/<name>/manifest.json`.
pub fn manifest_path(paths: &Paths, name: &str) -> PathBuf {
    paths.plugin_dir(name).join(MANIFEST_FILE_NAME)
}

/// Where one installed plugin's executable sits: `<base>/plugins/<name>/<name>`.
pub fn program_path(paths: &Paths, name: &str) -> PathBuf {
    paths.plugin_dir(name).join(program_file_name(name))
}

/// Read one installed plugin by name — the exact path, for a caller naming a single plugin (`enable`,
/// `config`, `uninstall`). Errors rather than skipping: an absent manifest means *not installed*, and a
/// manifest that will not parse, disagrees with its directory name, or has lost its executable means a
/// broken install the caller must be told about, not one silently treated as absent.
pub fn read(paths: &Paths, name: &str) -> Result<InstalledPlugin> {
    if is_reserved_plugin_name(name) {
        return Err(Error::invalid(format!("'{name}' is not a plugin name (it is reserved for the registry cache)")));
    }
    let manifest_file = manifest_path(paths, name);
    let raw = match std::fs::read_to_string(&manifest_file) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::NotFound(
                Msg::new(format!("plugin '{name}' is not installed"))
                    .coded(ErrorCode::NotFoundPluginInstalled)
                    .with("name", name),
            ));
        }
        Err(e) => return Err(Error::from(e)),
    };
    let manifest: Manifest = serde_json::from_str(&raw).map_err(|e| {
        Error::Invalid(
            Msg::new(format!(
                "plugin '{name}' has a malformed manifest ({}): {e}",
                manifest_file.display()
            ))
            .coded(ErrorCode::InvalidPluginManifestMalformed)
            .with("name", name)
            .with("path", manifest_file.display())
            .with("reason", e),
        )
    })?;
    if manifest.name != name {
        return Err(Error::Invalid(
            Msg::new(format!(
                "plugin '{name}' has a manifest naming a different plugin ('{}')",
                manifest.name
            ))
            .coded(ErrorCode::InvalidPluginManifestNamesOther)
            .with("name", name)
            .with("other", &manifest.name),
        ));
    }
    let program = program_path(paths, name);
    if !program.exists() {
        return Err(Error::Invalid(
            Msg::new(format!("plugin '{name}' has no executable at {}", program.display()))
                .coded(ErrorCode::InvalidPluginProgramAbsent)
                .with("name", name)
                .with("path", program.display()),
        ));
    }
    Ok(InstalledPlugin { name: name.to_string(), program, manifest, origin: origin(paths, name) })
}

/// Every plugin installed on this machine, name-sorted so a listing and a dispatch see the same order.
/// An absent `plugins/` directory is the ordinary empty state (nothing installed yet), not an error. A
/// directory that cannot be read as an install is warned about and skipped — one broken plugin never
/// hides the others (see the module docs).
pub fn installed(paths: &Paths) -> Result<Vec<InstalledPlugin>> {
    let dir = paths.plugins_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::from(e)),
    };
    let mut plugins = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        // A name that is not valid UTF-8 is not a name any manifest could carry, and the registry cache
        // is not a plugin: neither is an install, and neither is worth a warning.
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        if is_reserved_plugin_name(&name) {
            continue;
        }
        match read(paths, &name) {
            Ok(plugin) => plugins.push(plugin),
            Err(error) => {
                tracing::warn!(plugin = %name, error = %error, "skipping an unreadable plugin install")
            }
        }
    }
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(plugins)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_manifest::Os;

    /// A scratch base, and the `Paths` rooted at it.
    fn paths_at(tag: &str) -> (Paths, PathBuf) {
        let dir = amenbo_scratch::scratch(&format!("plugin-installed-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        (Paths::at(dir.clone()), dir)
    }

    fn manifest(name: &str) -> Manifest {
        Manifest {
            name: name.to_string(),
            desc: "a test plugin".to_string(),
            about: None,
            author: "amenbo".to_string(),
            repo: "ShiroDoromoto/amenbo".to_string(),
            os: vec![Os::Macos, Os::Linux, Os::Windows],
            category: "workflow".to_string(),
            url: "https://example.invalid/x.tar.gz".to_string(),
            checksum: "sha256:00".to_string(),
            signature: None,
            assets: Default::default(),
            official: false,
            detail_sum: None,
            scope: crate::plugin_manifest::Scope::Project,
            payload_v: 1,
            min_amenbo: None,
            config: Vec::new(),
            events: Vec::new(),
            agent: None,
            settings: None,
        }
    }

    /// Lay a well-formed install down: the home, the executable, and the manifest naming `manifest_name`
    /// (which the identity test bends away from the directory).
    fn install(paths: &Paths, dir_name: &str, manifest_name: &str) {
        let home = paths.plugin_dir(dir_name);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(program_file_name(dir_name)), b"#!/bin/sh\n").unwrap();
        std::fs::write(
            home.join(MANIFEST_FILE_NAME),
            serde_json::to_string(&manifest(manifest_name)).unwrap(),
        )
        .unwrap();
    }

    /// A complete install reads back as one `InstalledPlugin`: the name, the executable inside its home,
    /// and the manifest.
    #[test]
    fn a_complete_install_reads_back() {
        let (paths, _dir) = paths_at("complete");
        install(&paths, "worktree", "worktree");

        let plugin = read(&paths, "worktree").unwrap();
        assert_eq!(plugin.name, "worktree");
        assert_eq!(plugin.program, program_path(&paths, "worktree"));
        assert_eq!(plugin.manifest.desc, "a test plugin");
    }

    /// Nothing installed at all — the plugins directory does not even exist — is the empty state, not an
    /// error (the ordinary first run).
    #[test]
    fn no_plugins_directory_is_the_empty_state() {
        let (paths, _dir) = paths_at("empty");
        assert!(installed(&paths).unwrap().is_empty());
    }

    /// An absent plugin is `not_found` by name — "not installed", distinct from a broken install.
    #[test]
    fn an_absent_plugin_is_not_found() {
        let (paths, _dir) = paths_at("absent");
        let err = read(&paths, "worktree").unwrap_err();
        assert_eq!(err.code(), "not_found_plugin_installed");
    }

    /// The manifest is the install marker: a home with an executable but no manifest is not installed.
    #[test]
    fn a_manifestless_home_is_not_installed() {
        let (paths, _dir) = paths_at("no-manifest");
        let home = paths.plugin_dir("worktree");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(program_file_name("worktree")), b"x").unwrap();

        assert_eq!(read(&paths, "worktree").unwrap_err().code(), "not_found_plugin_installed");
        assert!(installed(&paths).unwrap().is_empty(), "and it is skipped by the scan");
    }

    /// A manifest with no executable beside it is a broken install, refused by name rather than run.
    #[test]
    fn a_manifest_without_its_executable_is_refused() {
        let (paths, _dir) = paths_at("no-exe");
        let home = paths.plugin_dir("worktree");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join(MANIFEST_FILE_NAME),
            serde_json::to_string(&manifest("worktree")).unwrap(),
        )
        .unwrap();

        let err = read(&paths, "worktree").unwrap_err();
        assert!(format!("{err:?}").contains("executable"), "the missing piece is named");
    }

    /// The directory name is the identity: a manifest naming another plugin is refused, never reconciled.
    #[test]
    fn a_manifest_naming_another_plugin_is_refused() {
        let (paths, _dir) = paths_at("identity");
        install(&paths, "worktree", "slack");

        let err = read(&paths, "worktree").unwrap_err();
        assert!(format!("{err:?}").contains("slack"), "the disagreeing name is reported");
    }

    /// The registry cache sits beside the plugins and is not one: neither by name nor in the scan.
    #[test]
    fn the_registry_cache_is_not_a_plugin() {
        let (paths, _dir) = paths_at("registry");
        std::fs::create_dir_all(paths.registry_dir()).unwrap();
        install(&paths, "worktree", "worktree");

        assert_eq!(read(&paths, Paths::REGISTRY_DIR_NAME).unwrap_err().code(), "invalid_value");
        let names: Vec<_> = installed(&paths).unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["worktree"]);
    }

    /// The origin round-trips, in both shapes: the official shelf, and a registered catalog named by the
    /// URL it was registered under.
    #[test]
    fn the_catalog_an_install_came_from_is_read_back() {
        let (paths, _dir) = paths_at("origin");
        install(&paths, "worktree", "worktree");

        record_origin(&paths, "worktree", &Origin::Official).unwrap();
        assert_eq!(origin(&paths, "worktree"), Some(Origin::Official));
        assert_eq!(read(&paths, "worktree").unwrap().origin, Some(Origin::Official));

        let url = "https://catalog.example.invalid/catalog.json";
        record_origin(&paths, "worktree", &Origin::Catalog(url.to_string())).unwrap();
        assert_eq!(origin(&paths, "worktree"), Some(Origin::Catalog(url.to_string())));
    }

    /// An install that says nothing about where it came from is unknown, not official: what to do about
    /// that is the update path's call, and it cannot make it if this layer has already guessed.
    #[test]
    fn an_install_with_no_record_has_no_origin() {
        let (paths, _dir) = paths_at("origin-absent");
        install(&paths, "worktree", "worktree");

        assert_eq!(origin(&paths, "worktree"), None);
        assert_eq!(read(&paths, "worktree").unwrap().origin, None, "and the install still reads");
    }

    /// A record that will not parse reads as unknown rather than taking the install down with it — the
    /// unknown case is the careful one, so landing there is safe.
    #[test]
    fn an_unreadable_origin_record_reads_as_unknown() {
        let (paths, _dir) = paths_at("origin-broken");
        install(&paths, "worktree", "worktree");
        std::fs::write(source_path(&paths, "worktree"), b"{ not json").unwrap();

        assert_eq!(origin(&paths, "worktree"), None);
        assert!(read(&paths, "worktree").is_ok(), "the install itself is fine");
    }

    /// The scan is name-sorted, and one broken install does not hide the good ones.
    #[test]
    fn the_scan_is_sorted_and_survives_a_broken_install() {
        let (paths, _dir) = paths_at("scan");
        install(&paths, "worktree", "worktree");
        install(&paths, "slack", "slack");
        // A broken one: a home holding a manifest that does not parse.
        let broken = paths.plugin_dir("broken");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join(program_file_name("broken")), b"x").unwrap();
        std::fs::write(broken.join(MANIFEST_FILE_NAME), b"{ not json").unwrap();

        let names: Vec<_> = installed(&paths).unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["slack", "worktree"]);
    }

    /// One language's overlay, as the catalog published it.
    fn translated(lang: &str, desc: &str) -> Translations {
        Translations::from([(
            lang.to_string(),
            crate::plugin_manifest::ManifestOverlay {
                desc: Some(desc.to_string()),
                ..Default::default()
            },
        )])
    }

    /// **The translations round-trip beside the binary** (`AMB-D-622`) — every language as it was
    /// published, so a settings form opened with no network has the labels and a reader who changes
    /// language is followed without going back to the catalog.
    #[test]
    fn the_translations_round_trip_beside_the_plugin() {
        let (paths, _dir) = paths_at("i18n-round-trip");
        install(&paths, "worktree", "worktree");

        let published = translated("ja", "タスクごとに git worktree を切り分ける");
        record_translations(&paths, "worktree", &published).unwrap();

        assert_eq!(translations(&paths, "worktree"), published);
        assert_eq!(
            translations_path(&paths, "worktree"),
            paths.plugin_dir("worktree").join("i18n.json"),
        );
    }

    /// **No translations removes the file.** This is the path an update takes, and a layer left over
    /// from the build before it would label the new one in the old one's words — the one way this file
    /// could say something the catalog does not.
    #[test]
    fn recording_no_translations_clears_what_was_there() {
        let (paths, _dir) = paths_at("i18n-cleared");
        install(&paths, "worktree", "worktree");
        record_translations(&paths, "worktree", &translated("ja", "一行")).unwrap();

        record_translations(&paths, "worktree", &Translations::new()).unwrap();

        assert!(!translations_path(&paths, "worktree").exists());
        assert!(translations(&paths, "worktree").is_empty());
        // And doing it again, with nothing there, is not a failure.
        record_translations(&paths, "worktree", &Translations::new()).unwrap();
    }

    /// Absent and unreadable both read as untranslated, never as an error: what such a plugin loses is
    /// the layer, and the base values are what a reader falls back to anyway (`AMB-D-623`).
    #[test]
    fn a_plugin_with_no_readable_translations_is_simply_untranslated() {
        let (paths, _dir) = paths_at("i18n-broken");
        install(&paths, "worktree", "worktree");

        assert!(translations(&paths, "worktree").is_empty(), "nothing written yet");
        std::fs::write(translations_path(&paths, "worktree"), b"{ not json").unwrap();
        assert!(translations(&paths, "worktree").is_empty());
        assert!(read(&paths, "worktree").is_ok(), "the install itself is fine");
    }
}
