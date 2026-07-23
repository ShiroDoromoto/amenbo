//! The real subscription resolver — which installed plugins observe a fired event (`AMB-T-2032`).
//!
//! [`plugin_dispatch`](crate::plugin_dispatch) drains the outbox and, for each event, asks a
//! [`Subscribers`] which plugins to fire. [`NoSubscribers`](crate::plugin_dispatch::NoSubscribers) is the
//! empty stand-in; this is the resolver that reads the actual install≠enable state (`AMB-D-351`) and the
//! manifests' subscription lists (`AMB-T-2032`) to answer it for real.
//!
//! **Four inputs, joined at the seam.** A plugin fires for an event only when all four hold:
//!
//! - it is **enabled** — the one gate its author declared is open (`AMB-D-351`/`AMB-D-379`; `install ≠
//!   enable`, so an installed-but-not-enabled plugin never fires). Which gate that is depends on the
//!   plugin: a `machine` one is the device's ([`Config::plugin_enabled`](crate::config::Config::plugin_enabled)), a `project` one is the gate of
//!   the project the **event happened in**, which the dispatcher resolves from the row it drained and
//!   hands over. An event whose project cannot be named — a deleted task takes its project with it —
//!   fires no project-scoped plugin at all, which is the fail-safe side: the alternative is opening a gate
//!   in a project the user never opened one in;
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
//! **Both tiered reads are at the machine tier here.** A fired observation event carries no single project
//! (the outbox spans them), so [`resolve`](EnabledSubscribers::resolve) reads each plugin's text config at
//! its machine default and its gate at the machine-global answer — the `project` a two-tier override would
//! need is not one an event has. The project-scoped tiers are the command face's, where an invocation runs
//! inside one project (`AMB-D-356` for a value, `AMB-D-350` for the gate). Resolving an event back to the
//! project of the record it names, so a per-project gate can decide an observation too, is its own work.
//!
//! **A config read that errors drops that one plugin, not the event.** Delivery is best-effort
//! (`AMB-D-352`): if a plugin's config cannot be read, the resolver warns and omits it, and the event still
//! fires for every other subscriber — one broken plugin never silences the rest.

use std::path::PathBuf;

use crate::plugin_dispatch::{Subscriber, Subscribers};
use crate::plugin_exec::PluginInvocation;
use crate::plugin_inject;
use crate::plugin_manifest::Manifest;
use crate::plugin_trust;
use crate::store::Store;

/// One installed plugin, as the resolver reads it: its catalog name, the executable to run, and its
/// manifest (the subscription list and config schema). Read off disk by
/// [`plugin_installed`](crate::plugin_installed) and handed to [`EnabledSubscribers`]; nothing is
/// discovered here (see the module docs).
#[derive(Debug, Clone)]
pub struct InstalledPlugin {
    /// The plugin's name — its identity in [`Config::plugin_enabled`](crate::config::Config::plugin_enabled) and its config storage key.
    pub name: String,
    /// The executable to run when the plugin fires.
    pub program: PathBuf,
    /// The plugin's manifest — the subscription list (`events`) and config schema this resolver reads.
    pub manifest: Manifest,
}

/// The real subscription resolver: fires the installed plugins that are enabled and subscribed to an event,
/// each with its own config injected (`AMB-T-2032`). Borrows its inputs — the installed set and the store
/// every gate and setting is read from — so constructing it is free and the mount point can build one per
/// drive.
pub struct EnabledSubscribers<'a> {
    installed: &'a [InstalledPlugin],
    store: &'a Store,
}

impl<'a> EnabledSubscribers<'a> {
    /// Build the resolver over the installed plugins and the store their gates and settings live in. The
    /// enable state is not passed separately: a project-scoped plugin's gate is a row in that same store
    /// (`AMB-D-379`), so the two halves of the answer have to come from one place.
    pub fn new(installed: &'a [InstalledPlugin], store: &'a Store) -> Self {
        Self { installed, store }
    }
}

impl Subscribers for EnabledSubscribers<'_> {
    fn resolve(&self, event: &str, project: Option<i64>) -> Vec<Subscriber> {
        let mut subscribers = Vec::new();
        for plugin in self.installed {
            // Enabled (the one gate its author declared is open, `AMB-D-351`/`AMB-D-379`) and subscribed
            // (the event is in its manifest). A project-scoped plugin is answered by the project the event
            // happened in; with none to name, it is skipped rather than measured against a switch it does
            // not have. A gate that cannot be read drops this plugin only (`AMB-D-352`).
            let Ok(gate) = plugin_trust::gate_for(plugin.manifest.scope, project) else {
                continue;
            };
            match plugin_trust::effective_enabled_in(self.store, &plugin.name, gate) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    tracing::warn!(
                        plugin = %plugin.name,
                        event = %event,
                        error = %error,
                        "could not read the plugin's gate; skipping this subscriber"
                    );
                    continue;
                }
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
            // The same project answers the config tiers, so a value it overrides is what the plugin is
            // handed here (`AMB-D-356`).
            let injection = match plugin_inject::resolve(
                self.store,
                &plugin.name,
                &plugin.manifest.config,
                project,
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

    /// Open a plugin's device-wide gate — what a `scope: machine` plugin's enable does.
    fn enable_machine(store: &mut Store, plugin: &str) {
        plugin_trust::enable(store, plugin, plugin_trust::Gate::Machine, &[], |_| true).unwrap();
    }

    /// Open a plugin's gate in one project — what a `scope: project` plugin's enable does.
    fn enable_in(store: &mut Store, plugin: &str, project: i64) {
        plugin_trust::enable(store, plugin, plugin_trust::Gate::Project(project), &[], |_| true)
            .unwrap();
    }

    /// A project to hang an event on.
    fn mk_project(store: &mut Store, name: &str) -> i64 {
        store
            .project_add(crate::ops::project::NewProject {
                name: name.into(),
                view: crate::model::View::List,
                notes: String::new(),
                color: None,
            })
            .unwrap()
            .id
    }

    /// The same manifest, declaring the project switch instead of the device one (`AMB-D-379`).
    fn project_scoped(name: &str, events: &[&str]) -> InstalledPlugin {
        let mut plugin = installed(name, events, vec![]);
        plugin.manifest.scope = crate::plugin_manifest::Scope::Project;
        plugin
    }

    /// A minimal manifest carrying a subscription list and a config schema — the two fields this resolver
    /// reads, plus the `scope` that says its switch is the device's (the only one this path can read —
    /// `AMB-D-379`); the rest is filler the resolver never touches.
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
            assets: Default::default(),
            official: false,
            scope: crate::plugin_manifest::Scope::Machine,
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
        let (mut store, _dir) = store_at("enabled-subscribed");
        enable_machine(&mut store, "slack");
        let plugins = [installed("slack", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        let subs = resolver.resolve("task.created", None);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].invocation.program, PathBuf::from("/plugins/slack"));
    }

    /// A resolved subscriber carries the plugin's **name**, not just the program to run: it is the only
    /// place the name is still known, and everything downstream — the hook runner's warnings, the
    /// execution log — reports on plugins, not on paths.
    #[test]
    fn a_resolved_subscriber_carries_the_plugins_name() {
        let (mut store, _dir) = store_at("named");
        enable_machine(&mut store, "slack");
        let plugins = [installed("slack", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        let subs = resolver.resolve("task.created", None);
        assert_eq!(subs[0].plugin, "slack");
    }

    /// Installed but not enabled: nothing fires — `install ≠ enable` (`AMB-D-351`).
    #[test]
    fn an_installed_but_disabled_plugin_does_not_fire() {
        let (store, _dir) = store_at("disabled");
        let plugins = [installed("slack", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        assert!(resolver.resolve("task.created", None).is_empty(), "an unenabled plugin never fires");
    }

    /// Enabled but not subscribed to this event: nothing fires.
    #[test]
    fn an_enabled_plugin_not_subscribed_to_the_event_does_not_fire() {
        let (mut store, _dir) = store_at("unsubscribed");
        enable_machine(&mut store, "slack");
        let plugins = [installed("slack", &["comment.added"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        assert!(resolver.resolve("task.created", None).is_empty(), "only the subscribed event fires it");
    }

    /// Enabled and subscribed, but incompatible with this build: it is dropped with a warning rather than
    /// fired (`AMB-D-359`) — enable-time is not the only door, since amenbo can update underneath it.
    #[test]
    fn an_incompatible_plugin_does_not_fire() {
        let (mut store, _dir) = store_at("incompatible");
        enable_machine(&mut store, "slack");
        let mut plugin = installed("slack", &["task.created"], vec![]);
        plugin.manifest.min_amenbo = Some("999.0.0".into());
        let plugins = [plugin];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        assert!(resolver.resolve("task.created", None).is_empty(), "a floor this build cannot meet");
    }

    /// One incompatible plugin never silences the rest: delivery is best-effort (`AMB-D-352`).
    #[test]
    fn an_incompatible_plugin_does_not_silence_the_others() {
        let (mut store, _dir) = store_at("incompatible-many");
        enable_machine(&mut store, "slack");
        enable_machine(&mut store, "email");
        let mut stale = installed("slack", &["task.created"], vec![]);
        stale.manifest.payload_v = crate::plugin_payload::VERSION + 1;
        let plugins = [stale, installed("email", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        let fired: Vec<_> = resolver
            .resolve("task.created", None)
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

        enable_machine(&mut store, "slack");
        let plugins = [installed(
            "slack",
            &["task.created"],
            vec![secret_field("webhook_url"), text_field("channel")],
        )];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        let subs = resolver.resolve("task.created", None);
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
        let (mut store, _dir) = store_at("many");
        enable_machine(&mut store, "slack");
        enable_machine(&mut store, "email");
        // `audit` is subscribed but never enabled — it must not fire.
        let plugins = [
            installed("slack", &["task.created"], vec![]),
            installed("email", &["task.created"], vec![]),
            installed("audit", &["task.created"], vec![]),
        ];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        let fired: Vec<_> = resolver
            .resolve("task.created", None)
            .into_iter()
            .map(|s| s.invocation.program)
            .collect();
        assert_eq!(fired, vec![PathBuf::from("/plugins/slack"), PathBuf::from("/plugins/email")]);
    }

    // ───────────────────── the project the event happened in (`AMB-D-379`) ────────────────────────

    /// A project-scoped plugin fires for an event in a project that has it on — and for nothing else.
    #[test]
    fn a_project_scoped_plugin_fires_only_in_the_project_that_enabled_it() {
        let (mut store, _dir) = store_at("project-gate");
        let a = mk_project(&mut store, "a");
        let b = mk_project(&mut store, "b");
        enable_in(&mut store, "slack", a);
        let plugins = [project_scoped("slack", &["task.created"])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        assert_eq!(resolver.resolve("task.created", Some(a)).len(), 1, "on in a");
        assert!(resolver.resolve("task.created", Some(b)).is_empty(), "off in b");
    }

    /// An event whose project cannot be named fires no project-scoped plugin: without a project there is
    /// no switch to read, and firing anyway would open a gate the user never opened.
    #[test]
    fn a_project_scoped_plugin_does_not_fire_for_an_unplaced_event() {
        let (mut store, _dir) = store_at("project-unplaced");
        let p = mk_project(&mut store, "p");
        enable_in(&mut store, "slack", p);
        let plugins = [project_scoped("slack", &["task.created"])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        assert!(resolver.resolve("task.created", None).is_empty());
    }

    /// A machine-scoped plugin is the device's answer wherever the event happened — the project it
    /// carries changes nothing.
    #[test]
    fn a_machine_scoped_plugin_ignores_the_events_project() {
        let (mut store, _dir) = store_at("machine-anywhere");
        let p = mk_project(&mut store, "p");
        enable_machine(&mut store, "slack");
        let plugins = [installed("slack", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        assert_eq!(resolver.resolve("task.created", Some(p)).len(), 1);
        assert_eq!(resolver.resolve("task.created", None).len(), 1);
    }
}
