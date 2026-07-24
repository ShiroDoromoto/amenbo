//! **Uninstall** — take a plugin out and leave nothing of it behind (`AMB-D-357`).
//!
//! `disable ≠ uninstall`, the mirror of `install ≠ enable`: [`plugin_trust::disable`](crate::plugin_trust::disable) stops a plugin
//! firing and keeps everything (the binary, the settings, the consent) so re-enabling costs nothing.
//! This is the other end — the plugin goes, and so does every trace it left in the five places one can
//! accumulate:
//!
//! | what | where | how it goes |
//! |---|---|---|
//! | the gate and the consent | `config.json` | [`Config::forget_plugin_trust`](crate::config::Config::forget_plugin_trust) |
//! | machine-default settings | `config.json` | [`Config::forget_plugin_config`](crate::config::Config::forget_plugin_config) |
//! | secrets | the user-area secret file | [`Secrets::forget_plugin`](crate::plugin_secret::Secrets::forget_plugin) + save (**always**, `AMB-D-357`) |
//! | per-project settings, every project | the store's `plugin_config` rows | [`Store::forget_plugin_config`](crate::store::Store::forget_plugin_config) |
//! | per-project gate answers, every project | the store's `plugin_enable` rows | [`Store::forget_plugin_enable`](crate::store::Store::forget_plugin_enable) |
//! | the binary and its home | `<base>/plugins/<name>/` | the directory is removed |
//! | the plugin's runs in the execution log | `<base>/plugin-runs.jsonl` | [`plugin_log::forget`](crate::plugin_log::forget) |
//!
//! **A re-install is therefore clean**, deliberately: the settings of the copy that was removed do not
//! come back, and the consent is asked again — a plugin's second life is a first life.
//!
//! **The order is chosen for what a failure leaves.** No filesystem sequence is atomic, so the steps run
//! from the most dangerous residue to the least: the gate closes and the consent goes *first* (an
//! interrupted uninstall can never leave a plugin that still fires), the secrets are purged next (bytes
//! that must not outlive the plugin), then the store rows, the binary, and the execution-log lines last —
//! secret-free debugging text is the least dangerous residue of all. Stopping anywhere leaves at most an
//! inert directory or a few stale log lines — the safe residue, and one a re-run of the same command
//! finishes off, since every step is idempotent and none requires the plugin to still read as installed.
//!
//! **Nothing points at a plugin from the backlog** (`AMB-D-357`): tasks and decisions do not reference
//! plugins, so there is no dangling reference to repair and no cascade to run beyond this list.

use crate::config::is_reserved_plugin_name;
use crate::error::{Error, Result};
use crate::plugin_secret::Secrets;
use crate::store::Store;

/// What an [`uninstall`] actually found and removed — the receipt, so a caller can report the removal
/// truthfully rather than claiming a uniform "done". Every field is what *went*, not what was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Removed {
    /// The plugin was enabled, and the gate has been closed.
    pub was_enabled: bool,
    /// A consent record existed and is gone.
    pub consent: bool,
    /// Machine-default settings existed in `config.json` and are gone.
    pub machine_defaults: bool,
    /// Secrets existed in the secret file and have been purged.
    pub secrets: bool,
    /// How many per-project setting rows were deleted, across every project.
    pub project_overrides: usize,
    /// How many per-project gate answers were deleted, across every project (`AMB-D-350`'s upper tier).
    pub project_gates: usize,
    /// The plugin's home under `plugins/` existed and has been removed.
    pub directory: bool,
    /// The plugin had runs in the execution log and they have been purged (`AMB-D-357`, `AMB-T-2098`).
    pub runs_log: bool,
}

impl Removed {
    /// Whether anything at all was found — `false` means the name held nothing on this machine, which a
    /// caller reports as "not installed" rather than as a removal.
    pub fn anything(&self) -> bool {
        self.consent
            || self.machine_defaults
            || self.secrets
            || self.project_overrides > 0
            || self.project_gates > 0
            || self.directory
            || self.runs_log
    }
}

/// Remove a plugin and everything it left behind (`AMB-D-357`), returning what was actually found.
///
/// Deliberately **does not require the plugin to still read as installed**: uninstall is the way to clean
/// up after a half-broken install too, so it works from the name alone and treats every missing piece as
/// one less thing to remove. Removing a name that holds nothing is a no-op reported through
/// [`Removed::anything`], not an error — only a *failure to remove* something that is there is.
pub fn uninstall(store: &mut Store, plugin: &str) -> Result<Removed> {
    if is_reserved_plugin_name(plugin) {
        return Err(Error::invalid(
            format!("'{plugin}' is not a plugin name (it is reserved for the registry cache)"),
            format!("'{plugin}' はプラグイン名ではありません（カタログのキャッシュ用に予約されています）"),
        ));
    }
    let mut removed = Removed {
        was_enabled: store.config.plugin_enabled(plugin),
        consent: store.config.plugin_consented(plugin),
        ..Removed::default()
    };

    // 1. The gate and the consent, then the machine defaults — one config write, so an interrupted
    //    uninstall can never leave a plugin that still fires.
    store.config.forget_plugin_trust(plugin);
    removed.machine_defaults = store.config.forget_plugin_config(plugin);
    store.save_config()?;

    // 2. The secrets, purged unconditionally — the bytes that must not outlive the plugin.
    let secrets_file = store.paths.plugin_secrets_file();
    let mut secrets = Secrets::load(&secrets_file)?;
    if secrets.forget_plugin(plugin) {
        secrets.save(&secrets_file)?;
        removed.secrets = true;
    }

    // 3. Every project's settings and gate answers, in one pass each over the single device-wide store.
    //    The gates go with them: a row left behind would be a project still saying "on here" when the
    //    plugin comes back under the same name, which is exactly the inheritance a re-install must not get.
    removed.project_overrides = store.forget_plugin_config(plugin)?;
    removed.project_gates = store.forget_plugin_enable(plugin)?;

    // 4. The binary and its home: what is left if anything above failed is an inert directory.
    let home = store.paths.plugin_dir(plugin);
    match std::fs::remove_dir_all(&home) {
        Ok(()) => removed.directory = true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::from(e)),
    }

    // 5. The plugin's runs in the execution log, last and safest: secret-free debugging lines, machine-local
    //    and outside the store. Its per-plugin ring never trims a removed plugin's lines out on its own
    //    (nothing of it runs again to push them), so uninstall clears them or they stay for good
    //    (`AMB-T-2098`). A failure here is a warn inside `forget`, never a failure of the uninstall.
    removed.runs_log = crate::plugin_log::forget(&store.paths.plugin_log_file(), plugin);

    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Paths;
    use crate::plugin_config::{self, Scope};
    use crate::plugin_manifest::ConfigField;

    fn field(key: &str, secret: bool) -> ConfigField {
        ConfigField { key: key.to_string(), label: key.to_string(), secret, required: false }
    }

    /// A store on a scratch base, plus the base path so the on-disk residue can be inspected.
    fn store_at(tag: &str) -> (Store, std::path::PathBuf) {
        let dir = amenbo_scratch::scratch(&format!("plugin-uninstall-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open_at(Paths::at(dir.clone())).unwrap();
        (store, dir)
    }

    /// Give the plugin something in all four places: a home with a binary, a machine default, a secret,
    /// a per-project override, and an open gate.
    fn install_and_configure(store: &mut Store, plugin: &str) -> i64 {
        let home = store.paths.plugin_dir(plugin);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(plugin), b"#!/bin/sh\n").unwrap();

        let project = store
            .project_add(crate::ops::project::NewProject {
                name: format!("{plugin}-proj"),
                view: crate::model::View::Board,
                notes: String::new(),
                color: None,
            })
            .unwrap()
            .id;
        plugin_config::set(store, &field("events", false), plugin, "push", Scope::MachineDefault)
            .unwrap();
        plugin_config::set(store, &field("events", false), plugin, "merge", Scope::Project(project))
            .unwrap();
        plugin_config::set(store, &field("token", true), plugin, "s3cret", Scope::MachineDefault)
            .unwrap();
        crate::plugin_trust::enable(store, plugin, crate::plugin_trust::Gate::Machine, &[], |_| true)
            .unwrap();
        // ...and a project that has the plugin on, the project tier's residue (`AMB-D-379`).
        crate::plugin_trust::enable(
            store,
            plugin,
            crate::plugin_trust::Gate::Project(project),
            &[],
            |_| true,
        )
        .unwrap();
        // ...and a line in the execution log, the fifth trace an uninstall must clear (`AMB-T-2098`).
        crate::plugin_log::record(
            &store.paths.plugin_log_file(),
            &crate::plugin_log::Run {
                plugin: plugin.to_string(),
                event: "task.created",
                outcome: crate::plugin_log::Outcome::Ok,
                code: Some(0),
                elapsed: std::time::Duration::from_millis(1),
                stderr: String::new(),
            },
        );
        project
    }

    /// The whole point: after an uninstall nothing of the plugin is left in any of the four places.
    #[test]
    fn uninstall_leaves_nothing_behind() {
        let (mut store, dir) = store_at("everything");
        let project = install_and_configure(&mut store, "slack");

        let removed = uninstall(&mut store, "slack").unwrap();
        assert!(removed.was_enabled && removed.consent && removed.machine_defaults);
        assert!(removed.secrets && removed.directory && removed.runs_log);
        assert_eq!(removed.project_overrides, 1);
        assert_eq!(removed.project_gates, 1);
        assert!(
            crate::plugin_log::recent(&store.paths.plugin_log_file(), "slack").is_empty(),
            "the plugin's runs are purged from the execution log",
        );

        assert!(!store.config.plugin_enabled("slack"), "the gate is gone");
        assert!(!store.config.plugin_consented("slack"), "the consent is gone");
        assert_eq!(store.config.plugin_text_default("slack", "events"), None);
        assert_eq!(
            store.plugin_config_override(project, "slack", "events").unwrap(),
            None,
            "the project override is gone",
        );
        assert!(
            !store.plugin_enabled_in_project(project, "slack").unwrap(),
            "the project's gate row is gone",
        );
        assert_eq!(
            plugin_config::get(&store, &field("token", true), "slack", Scope::MachineDefault)
                .unwrap(),
            None,
            "the secret is purged",
        );
        assert!(!dir.join("plugins").join("slack").exists(), "the home is gone");
        // And the purge really left the file — not just the reader's view of it.
        let raw = std::fs::read_to_string(dir.join(Paths::PLUGIN_SECRETS_FILE_NAME)).unwrap();
        assert!(!raw.contains("s3cret"), "the secret bytes are out of the file");
    }

    /// A re-install starts clean: nothing survives to be inherited by the next copy of the same name.
    #[test]
    fn a_reinstall_inherits_nothing() {
        let (mut store, _dir) = store_at("reinstall");
        install_and_configure(&mut store, "slack");
        uninstall(&mut store, "slack").unwrap();

        // The same name, installed again: no consent, no gate, no settings.
        assert!(!store.config.plugin_consented("slack"));
        assert!(!store.config.plugin_enabled("slack"));
        assert_eq!(store.config.plugin_text_default("slack", "events"), None);
    }

    /// Uninstall works from the name alone — a half-broken install (a home with no manifest) is exactly
    /// what it is for, and it is not asked whether the plugin reads as installed.
    #[test]
    fn a_broken_install_can_still_be_uninstalled() {
        let (mut store, dir) = store_at("broken");
        let home = store.paths.plugin_dir("slack");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("slack"), b"x").unwrap();

        let removed = uninstall(&mut store, "slack").unwrap();
        assert!(removed.directory);
        assert!(!dir.join("plugins").join("slack").exists());
    }

    /// A name holding nothing is a no-op, not an error — and running uninstall twice is safe.
    #[test]
    fn uninstalling_nothing_is_a_no_op() {
        let (mut store, _dir) = store_at("nothing");
        install_and_configure(&mut store, "slack");
        uninstall(&mut store, "slack").unwrap();

        let again = uninstall(&mut store, "slack").unwrap();
        assert!(!again.anything(), "the second run finds nothing left");
        assert!(!uninstall(&mut store, "never-installed").unwrap().anything());
    }

    /// One plugin's uninstall does not touch another's — the blast radius is the name.
    #[test]
    fn another_plugin_is_untouched() {
        let (mut store, dir) = store_at("neighbour");
        let project = install_and_configure(&mut store, "slack");
        install_and_configure(&mut store, "worktree");

        uninstall(&mut store, "slack").unwrap();

        assert!(store.config.plugin_enabled("worktree"), "the neighbour keeps its gate");
        assert_eq!(store.config.plugin_text_default("worktree", "events"), Some("push"));
        assert!(dir.join("plugins").join("worktree").exists(), "the neighbour keeps its home");
        assert_eq!(
            crate::plugin_log::recent(&store.paths.plugin_log_file(), "worktree").len(),
            1,
            "the neighbour keeps its execution-log runs",
        );
        // ...including its own project override and gate answer, which share the store with the erased ones.
        assert!(store.plugin_config_override(project, "slack", "events").unwrap().is_none());
        assert!(!store.plugin_enabled_in_project(project, "slack").unwrap());
    }

    /// The registry cache is not a plugin, so it cannot be uninstalled — the directory that holds the
    /// catalog is never a removal target.
    #[test]
    fn the_registry_cache_cannot_be_uninstalled() {
        let (mut store, _dir) = store_at("registry");
        let err = uninstall(&mut store, Paths::REGISTRY_DIR_NAME).unwrap_err();
        assert_eq!(err.code(), "invalid_value");
    }
}
