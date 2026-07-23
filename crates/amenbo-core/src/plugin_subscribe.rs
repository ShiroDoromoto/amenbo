//! The real subscription resolver — which installed plugins observe a fired event (`AMB-T-2032`).
//!
//! [`plugin_dispatch`](crate::plugin_dispatch) drains the outbox and, for each event, asks a
//! [`Subscribers`] which plugins to fire. [`NoSubscribers`](crate::plugin_dispatch::NoSubscribers) is the
//! empty stand-in; this is the resolver that reads the actual install≠enable state (`AMB-D-351`) and the
//! manifests' subscription lists (`AMB-T-2032`) to answer it for real.
//!
//! **Four inputs, joined at the seam.** A plugin fires for an event only when all four hold:
//!
//! - it is **enabled** — its machine-global gate is open ([`Config::plugin_enabled`], `AMB-D-351`;
//!   `install ≠ enable`, so an installed-but-not-enabled plugin never fires);
//! - it **subscribes** — the event's name is in its manifest [`events`](crate::plugin_manifest::Manifest::events);
//! - it is **compatible** — this amenbo speaks the payload contract it reads and clears the version floor
//!   it declares ([`plugin_compat::check`](crate::plugin_compat::check), `AMB-D-359`);
//! - it resolves — its config reads cleanly ([`plugin_inject::resolve`],
//!   `AMB-D-356`), splitting the plugin's own settings into secret env vars and text stdin config.
//!
//! For each such plugin the resolver builds a [`Subscriber`]: the program to run, its secret config set as
//! environment variables (off argv, off logs), and its non-secret config for the payload's `config` key.
//! It never sets the event payload itself — [`deliver`](crate::plugin_dispatch::deliver) composes stdin, so
//! the payload channel stays the dispatcher's.
//!
//! **What is installed is given, not discovered here.** The resolver is handed the set of
//! [`InstalledPlugin`]s — each a name, an executable, and a manifest — rather than scanning the plugins
//! directory itself: the on-disk shape of an install is [`plugin_installed`](crate::plugin_installed)'s,
//! and keeping that out of here leaves the resolver a pure function of state it is given, as testable as
//! the dispatcher above it. The mount point (`AMB-T-2033`) assembles the list and constructs this resolver
//! once per drive.
//!
//! **Config reads at the machine-default tier.** A fired observation event carries no single project (the
//! outbox spans them), so [`resolve`](EnabledSubscribers::resolve) reads each plugin's text config at its
//! machine default — the `project` a two-tier override would need is not one an event has. The
//! project-scoped tier is the command face's, where an invocation runs inside one project (`AMB-D-356`).
//!
//! **A config read that errors drops that one plugin, not the event.** Delivery is best-effort
//! (`AMB-D-352`): if a plugin's config cannot be read, the resolver warns and omits it, and the event still
//! fires for every other subscriber — one broken plugin never silences the rest.

use std::path::PathBuf;

use crate::config::Config;
use crate::plugin_dispatch::{Subscriber, Subscribers};
use crate::plugin_exec::PluginInvocation;
use crate::plugin_inject;
use crate::plugin_manifest::Manifest;
use crate::store::Store;

/// One installed plugin, as the resolver reads it: its catalog name, the executable to run, and its
/// manifest (the subscription list and config schema). Read off disk by
/// [`plugin_installed`](crate::plugin_installed) and handed to [`EnabledSubscribers`]; nothing is
/// discovered here (see the module docs).
#[derive(Debug, Clone)]
pub struct InstalledPlugin {
    /// The plugin's name — its identity in [`Config::plugin_enabled`] and its config storage key.
    pub name: String,
    /// The executable to run when the plugin fires.
    pub program: PathBuf,
    /// The plugin's manifest — the subscription list (`events`) and config schema this resolver reads.
    pub manifest: Manifest,
}

/// The real subscription resolver: fires the installed plugins that are enabled and subscribed to an event,
/// each with its own config injected (`AMB-T-2032`). Borrows its inputs — the installed set, the enable
/// state, and the store the config is read from — so constructing it is free and the mount point can build
/// one per drive.
pub struct EnabledSubscribers<'a> {
    installed: &'a [InstalledPlugin],
    config: &'a Config,
    store: &'a Store,
}

impl<'a> EnabledSubscribers<'a> {
    /// Build the resolver over the installed plugins, the current enable state, and the store the plugins'
    /// config is read from.
    pub fn new(installed: &'a [InstalledPlugin], config: &'a Config, store: &'a Store) -> Self {
        Self { installed, config, store }
    }
}

impl Subscribers for EnabledSubscribers<'_> {
    fn resolve(&self, event: &str) -> Vec<Subscriber> {
        let mut subscribers = Vec::new();
        for plugin in self.installed {
            // Enabled (the gate is open, `AMB-D-351`) and subscribed (the event is in its manifest).
            if !self.config.plugin_enabled(&plugin.name) {
                continue;
            }
            if !plugin.manifest.events.iter().any(|e| e == event) {
                continue;
            }
            // Compatible with this build (`AMB-D-359`). Checked here and not only at `enable`, because
            // amenbo updates underneath an install: a plugin enabled against the old payload contract is
            // dropped rather than fed one it cannot read — best-effort like a config that will not
            // resolve, so the event still fires for everyone else (`AMB-D-352`).
            if let Err(incompatible) = crate::plugin_compat::check(&plugin.manifest) {
                tracing::warn!(
                    plugin = %plugin.name,
                    event = %event,
                    reason = %incompatible,
                    "skipping an incompatible plugin"
                );
                continue;
            }
            // Resolve this plugin's own config: secret → env, text → the payload's `config` key. A read
            // that errors drops this plugin only — the event still fires for the rest (`AMB-D-352`).
            let injection = match plugin_inject::resolve(
                self.store,
                &plugin.name,
                &plugin.manifest.config,
                None,
            ) {
                Ok(injection) => injection,
                Err(error) => {
                    tracing::warn!(
                        plugin = %plugin.name,
                        event = %event,
                        error = %error,
                        "could not resolve plugin config; skipping this subscriber"
                    );
                    continue;
                }
            };
            let mut invocation = PluginInvocation::new(&plugin.program);
            for (name, value) in injection.env {
                invocation = invocation.env(name, value);
            }
            subscribers.push(Subscriber {
                plugin: plugin.name.clone(),
                invocation,
                config: injection.text,
            });
        }
        subscribers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Paths;
    use crate::plugin_config::{self, Scope};
    use crate::plugin_manifest::{ConfigField, Manifest, Os};

    fn store_at(tag: &str) -> (Store, std::path::PathBuf) {
        let dir = amenbo_scratch::scratch(&format!("plugin-subscribe-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        (Store::open_at(Paths::at(dir.clone())).unwrap(), dir)
    }

    /// A minimal manifest carrying a subscription list and a config schema — the two fields this resolver
    /// reads; the rest are filler the resolver never touches.
    fn manifest(events: &[&str], config: Vec<ConfigField>) -> Manifest {
        Manifest {
            name: "unused".into(),
            desc: String::new(),
            author: String::new(),
            repo: String::new(),
            os: vec![Os::Linux],
            category: String::new(),
            url: String::new(),
            checksum: String::new(),
            signature: None,
            official: false,
            // The contract this build speaks: the compatibility gate reads this one, so it tracks
            // `VERSION` rather than sitting on a literal that a bump would turn into a false failure.
            payload_v: crate::plugin_payload::VERSION,
            min_amenbo: None,
            config,
            events: events.iter().map(|e| e.to_string()).collect(),
        }
    }

    fn installed(name: &str, events: &[&str], config: Vec<ConfigField>) -> InstalledPlugin {
        InstalledPlugin {
            name: name.into(),
            program: PathBuf::from(format!("/plugins/{name}")),
            manifest: manifest(events, config),
        }
    }

    fn text_field(key: &str) -> ConfigField {
        ConfigField { key: key.into(), label: key.into(), secret: false, required: false }
    }
    fn secret_field(key: &str) -> ConfigField {
        ConfigField { key: key.into(), label: key.into(), secret: true, required: false }
    }

    /// An enabled, subscribed plugin fires; the resolved invocation names its program.
    #[test]
    fn an_enabled_subscribed_plugin_fires() {
        let (store, _dir) = store_at("enabled-subscribed");
        let mut config = Config::default();
        config.enable_plugin("slack");
        let plugins = [installed("slack", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &config, &store);
        let subs = resolver.resolve("task.created");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].invocation.program, PathBuf::from("/plugins/slack"));
    }

    /// A resolved subscriber carries the plugin's **name**, not just the program to run: it is the only
    /// place the name is still known, and everything downstream — the hook runner's warnings, the
    /// execution log — reports on plugins, not on paths.
    #[test]
    fn a_resolved_subscriber_carries_the_plugins_name() {
        let (store, _dir) = store_at("named");
        let mut config = Config::default();
        config.enable_plugin("slack");
        let plugins = [installed("slack", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &config, &store);
        let subs = resolver.resolve("task.created");
        assert_eq!(subs[0].plugin, "slack");
    }

    /// Installed but not enabled: nothing fires — `install ≠ enable` (`AMB-D-351`).
    #[test]
    fn an_installed_but_disabled_plugin_does_not_fire() {
        let (store, _dir) = store_at("disabled");
        let config = Config::default(); // never enabled
        let plugins = [installed("slack", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &config, &store);
        assert!(resolver.resolve("task.created").is_empty(), "an unenabled plugin never fires");
    }

    /// Enabled but not subscribed to this event: nothing fires.
    #[test]
    fn an_enabled_plugin_not_subscribed_to_the_event_does_not_fire() {
        let (store, _dir) = store_at("unsubscribed");
        let mut config = Config::default();
        config.enable_plugin("slack");
        let plugins = [installed("slack", &["comment.added"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &config, &store);
        assert!(resolver.resolve("task.created").is_empty(), "only the subscribed event fires it");
    }

    /// Enabled and subscribed, but incompatible with this build: it is dropped with a warning rather than
    /// fired (`AMB-D-359`) — enable-time is not the only door, since amenbo can update underneath it.
    #[test]
    fn an_incompatible_plugin_does_not_fire() {
        let (store, _dir) = store_at("incompatible");
        let mut config = Config::default();
        config.enable_plugin("slack");
        let mut plugin = installed("slack", &["task.created"], vec![]);
        plugin.manifest.min_amenbo = Some("999.0.0".into());
        let plugins = [plugin];

        let resolver = EnabledSubscribers::new(&plugins, &config, &store);
        assert!(resolver.resolve("task.created").is_empty(), "a floor this build cannot meet");
    }

    /// One incompatible plugin never silences the rest: delivery is best-effort (`AMB-D-352`).
    #[test]
    fn an_incompatible_plugin_does_not_silence_the_others() {
        let (store, _dir) = store_at("incompatible-many");
        let mut config = Config::default();
        config.enable_plugin("slack");
        config.enable_plugin("email");
        let mut stale = installed("slack", &["task.created"], vec![]);
        stale.manifest.payload_v = crate::plugin_payload::VERSION + 1;
        let plugins = [stale, installed("email", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &config, &store);
        let fired: Vec<_> = resolver
            .resolve("task.created")
            .into_iter()
            .map(|s| s.invocation.program)
            .collect();
        assert_eq!(fired, vec![PathBuf::from("/plugins/email")]);
    }

    /// The plugin's config is injected: a secret rides env, a text field rides the `config` map — split by
    /// the author's `secret` flag (`AMB-D-356`), and only this plugin's own values.
    #[test]
    fn a_subscribers_own_config_is_injected_split_by_secret() {
        let (mut store, _dir) = store_at("inject");
        plugin_config::set(&mut store, &secret_field("webhook_url"), "slack", "https://hooks/x", Scope::MachineDefault).unwrap();
        plugin_config::set(&mut store, &text_field("channel"), "slack", "#ops", Scope::MachineDefault).unwrap();

        let mut config = Config::default();
        config.enable_plugin("slack");
        let plugins = [installed(
            "slack",
            &["task.created"],
            vec![secret_field("webhook_url"), text_field("channel")],
        )];

        let resolver = EnabledSubscribers::new(&plugins, &config, &store);
        let subs = resolver.resolve("task.created");
        assert_eq!(subs.len(), 1);
        // Secret → env, off the payload.
        assert_eq!(
            subs[0].invocation.env,
            vec![("AMENBO_CONFIG_WEBHOOK_URL".to_string(), "https://hooks/x".to_string())]
        );
        // Text → the config map deliver folds onto stdin.
        assert_eq!(subs[0].config.get("channel").and_then(|v| v.as_str()), Some("#ops"));
        assert!(subs[0].config.get("webhook_url").is_none(), "a secret never rides the stdin config");
    }

    /// Several plugins subscribe to one event; every enabled subscriber fires, the disabled one does not.
    #[test]
    fn every_enabled_subscriber_to_an_event_fires() {
        let (store, _dir) = store_at("many");
        let mut config = Config::default();
        config.enable_plugin("slack");
        config.enable_plugin("email");
        // `audit` is subscribed but never enabled — it must not fire.
        let plugins = [
            installed("slack", &["task.created"], vec![]),
            installed("email", &["task.created"], vec![]),
            installed("audit", &["task.created"], vec![]),
        ];

        let resolver = EnabledSubscribers::new(&plugins, &config, &store);
        let fired: Vec<_> = resolver
            .resolve("task.created")
            .into_iter()
            .map(|s| s.invocation.program)
            .collect();
        assert_eq!(fired, vec![PathBuf::from("/plugins/slack"), PathBuf::from("/plugins/email")]);
    }
}
