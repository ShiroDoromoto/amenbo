//! The **installed registry** — which plugins are actually on this machine, read off
//! `<base>/plugins/` (`AMB-D-350`).
//!
//! [`plugin_subscribe`](crate::plugin_subscribe) is handed the set of installed plugins rather than
//! discovering it, and says so: *how* an install lands a binary and its manifest on disk is the install
//! lifecycle's. This module is that half — the one place that knows the on-disk shape of an installed
//! plugin, so the dispatch mount, the CLI faces and `uninstall` all read the same layout.
//!
//! **One plugin's home is `<base>/plugins/<name>/`**, holding exactly two things this layer knows about:
//!
//! - `manifest.json` — the catalog entry the plugin was installed from ([`Manifest`]), kept beside the
//!   binary so the subscription list and config schema are readable with no network and no catalog. It is
//!   also the **install marker**: an install writes it last, so a half-finished directory is simply not
//!   installed rather than half-installed.
//! - the **executable**, named after the plugin itself (`<name>`, plus the platform's `.exe` suffix). The
//!   name is a convention, not a manifest field: the catalog entry says where to *fetch* an asset, never
//!   what to run, so nothing a third party writes can point amenbo's spawn at another path.
//!
//! **The directory name is the identity.** `Config::plugin_enabled`, the config storage key and the secret
//! file all key off the plugin's name, so a manifest whose `name` disagrees with the directory it sits in
//! is refused rather than reconciled — the two would otherwise name different plugins in the same breath.
//! The reserved `registry` directory ([`is_reserved_plugin_name`]) is not a plugin and is skipped.
//!
//! **Reading is not the door.** A manifest's *rules* are enforced fail-closed where untrusted input enters
//! — the install/intake door (`AMB-D-354`, [`crate::plugin_validate`]). What is on disk here has already
//! passed it, so this layer checks only what could have rotted since: that the files exist, parse, and
//! still agree on the name.
//!
//! **One broken install never hides the rest.** [`installed`] warns and skips a directory it cannot read,
//! the same best-effort posture the dispatch resolver takes with a plugin whose config will not resolve
//! (`AMB-D-352`); [`read`] — the by-name path an `enable` takes — is exact and returns the error instead.

use std::path::PathBuf;

use crate::config::{is_reserved_plugin_name, Paths};
use crate::error::{Error, Result};
use crate::plugin_manifest::Manifest;
use crate::plugin_subscribe::InstalledPlugin;

/// The file in a plugin's home holding the catalog entry it was installed from, and the marker that the
/// install finished (see the module docs).
pub const MANIFEST_FILE_NAME: &str = "manifest.json";

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
        return Err(Error::invalid(
            format!("'{name}' is not a plugin name (it is reserved for the registry cache)"),
            format!("'{name}' はプラグイン名ではありません（目録キャッシュ用に予約されています）"),
        ));
    }
    let manifest_file = manifest_path(paths, name);
    let raw = match std::fs::read_to_string(&manifest_file) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::not_found(
                format!("plugin '{name}' is not installed"),
                format!("プラグイン '{name}' はインストールされていません"),
            ));
        }
        Err(e) => return Err(Error::from(e)),
    };
    let manifest: Manifest = serde_json::from_str(&raw).map_err(|e| {
        Error::invalid(
            format!("plugin '{name}' has a malformed manifest ({}): {e}", manifest_file.display()),
            format!(
                "プラグイン '{name}' の manifest が壊れています（{}）: {e}",
                manifest_file.display()
            ),
        )
    })?;
    if manifest.name != name {
        return Err(Error::invalid(
            format!(
                "plugin '{name}' has a manifest naming a different plugin ('{}')",
                manifest.name
            ),
            format!(
                "プラグイン '{name}' の manifest が別のプラグイン（'{}'）を名乗っています",
                manifest.name
            ),
        ));
    }
    let program = program_path(paths, name);
    if !program.exists() {
        return Err(Error::invalid(
            format!("plugin '{name}' has no executable at {}", program.display()),
            format!("プラグイン '{name}' の実行ファイルがありません（{}）", program.display()),
        ));
    }
    Ok(InstalledPlugin { name: name.to_string(), program, manifest })
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
            author: "amenbo".to_string(),
            repo: "ShiroDoromoto/amenbo".to_string(),
            os: vec![Os::Macos, Os::Linux, Os::Windows],
            category: "workflow".to_string(),
            url: "https://example.invalid/x.tar.gz".to_string(),
            checksum: "sha256:00".to_string(),
            signature: None,
            assets: Default::default(),
            official: false,
            scope: crate::plugin_manifest::Scope::Project,
            payload_v: 1,
            min_amenbo: None,
            config: Vec::new(),
            events: Vec::new(),
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
        assert_eq!(err.code(), "not_found");
    }

    /// The manifest is the install marker: a home with an executable but no manifest is not installed.
    #[test]
    fn a_manifestless_home_is_not_installed() {
        let (paths, _dir) = paths_at("no-manifest");
        let home = paths.plugin_dir("worktree");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(program_file_name("worktree")), b"x").unwrap();

        assert_eq!(read(&paths, "worktree").unwrap_err().code(), "not_found");
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
}
