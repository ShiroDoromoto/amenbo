//! The real subscription resolver — which installed plugins observe a fired event (`AMB-T-2032`).
//!
//! [`plugin_dispatch`](crate::plugin_dispatch) drains the outbox and, for each event, asks a
//! [`Subscribers`] which plugins to fire. [`NoSubscribers`](crate::plugin_dispatch::NoSubscribers) is the
//! empty stand-in; this is the resolver that reads the actual install≠enable state (`AMB-D-351`) and the
//! manifests' subscription lists (`AMB-T-2032`) to answer it for real.
//!
//! **Four inputs, joined at the seam.** A plugin fires for an event only when all four hold:
//!
//! - it is **enabled** — the gate of the project the **event happened in** is open (`AMB-D-351`/`AMB-D-434`;
//!   `install ≠ enable`, so an installed-but-not-enabled plugin never fires). The dispatcher resolves that
//!   project from the row it drained and hands it over. An event whose project cannot be named — a deleted
//!   task takes its project with it — fires nothing at all, which is the fail-safe side: the alternative is
//!   opening a gate in a project the user never opened one in;
//! - it **subscribes** — the event's name is in its manifest [`events`](crate::plugin_manifest::Manifest::events);
//! - it is **compatible** — this amenbo speaks the payload contract it reads and clears the version floor
//!   it declares ([`plugin_compat::check`](crate::plugin_compat::check), `AMB-D-359`);
//! - it resolves — its config reads cleanly ([`plugin_inject::resolve`],
//!   `AMB-D-356`), splitting the plugin's own settings into secret env vars and text stdin config.
//!
//! For each such plugin the resolver builds a [`Subscriber`]: the program to run, its secret config set as
//! environment variables (off argv, off logs), the read-back path beside it ([`plugin_callback`],
//! `AMB-D-406` — the store to call `amenbo` into and the window to read it through), and its non-secret
//! config for the payload's `config` key. It never sets the event payload itself —
//! [`plugin_dispatch`](crate::plugin_dispatch) composes stdin, so the payload channel stays the
//! dispatcher's.
//!
//! **What is installed is given, not discovered here.** The resolver is handed the set of
//! [`InstalledPlugin`]s — each a name, an executable, and a manifest — rather than scanning the plugins
//! directory itself: the on-disk shape of an install is [`plugin_installed`](crate::plugin_installed)'s,
//! and keeping that out of here leaves the resolver a pure function of state it is given, as testable as
//! the dispatcher above it. The mount point (`AMB-T-2033`) assembles the list and constructs this resolver
//! once per drive.
//!
//! **The project is the event's own, not the caller's.** The dispatcher resolves each drained row back to
//! the project of the record it names and hands it here, so both project-keyed reads — the gate
//! (`AMB-D-434`) and a text setting's override (`AMB-D-356`) — are answered where the event happened rather
//! than wherever the drive was standing. An event whose project cannot be named fires nothing (above).
//!
//! **A config read that errors drops that one plugin, not the event.** Delivery is best-effort
//! (`AMB-D-352`): if a plugin's config cannot be read, the resolver warns and omits it, and the event still
//! fires for every other subscriber — one broken plugin never silences the rest.

use std::path::PathBuf;

use crate::plugin_callback;
use crate::plugin_dispatch::{Subscriber, Subscribers};
use crate::plugin_exec::PluginInvocation;
use crate::plugin_inject;
use crate::plugin_installed::Origin;
use crate::plugin_manifest::{Face, Manifest};
use crate::plugin_trust;
use crate::store::Store;

/// One installed plugin, as the resolver reads it: its catalog name, the executable to run, and its
/// manifest (the subscription list and config schema). Read off disk by
/// [`plugin_installed`](crate::plugin_installed) and handed to [`EnabledSubscribers`]; nothing is
/// discovered here (see the module docs).
#[derive(Debug, Clone)]
pub struct InstalledPlugin {
    /// The plugin's name — its identity in the store's `plugin_enable` rows and its config storage key.
    pub name: String,
    /// The executable to run when the plugin fires.
    pub program: PathBuf,
    /// The plugin's manifest — the subscription list (`events`) and config schema this resolver reads.
    pub manifest: Manifest,
    /// Which catalog it was installed from, or `None` for an install that records none — placed by hand,
    /// or made before amenbo wrote the record down. Nothing in this module reads it; it rides along
    /// because it is part of what one install *is* on disk, and the update path is where it is spent
    /// ([`plugin_update`](crate::plugin_update)).
    pub origin: Option<Origin>,
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
    /// enable state is not passed separately: a plugin's gate is a row in that same store (`AMB-D-434`).
    pub fn new(installed: &'a [InstalledPlugin], store: &'a Store) -> Self {
        Self { installed, store }
    }
}

impl Subscribers for EnabledSubscribers<'_> {
    fn resolve(&self, event: &str, project: Option<i64>, face: Face) -> Vec<Subscriber> {
        // Every gate is a project's (`AMB-D-434`), so an event that names none has no switch to measure any
        // plugin against and fires nothing — rather than being answered by a device-wide gate that no
        // longer exists.
        let Some(project) = project else {
            return Vec::new();
        };
        let mut subscribers = Vec::new();
        for plugin in self.installed {
            // Enabled in the project the event happened in (`AMB-D-351`/`AMB-D-434`) and subscribed (the
            // event is in its manifest). A gate that cannot be read drops this plugin only (`AMB-D-352`).
            match plugin_trust::effective_enabled_in(self.store, &plugin.name, project) {
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
            // Subscribed to this event, and declaring the face driving this dispatch (`AMB-D-383`): a
            // `faces:[cli]` hook stays silent on a GUI drive and vice versa. The matching subscription also
            // carries `reply`, which rides down to the fan-out — it is what tells the CLI face to run this one
            // synchronously and relay its stderr (and, since `reply:true` is pinned to `faces:[cli]`, a GUI
            // drive never resolves a replying subscriber). A plugin subscribes to one event at most once, so
            // the first match on (event, face) is the subscription.
            let Some(subscription) =
                plugin.manifest.events.iter().find(|e| e.event == event && e.faces.contains(&face))
            else {
                continue;
            };
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
            // The values are that project's own, so what the plugin is handed here is what was set
            // where the event happened (`AMB-D-434`).
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
            // The read-back path (`AMB-D-406`): the store to call `amenbo` into, and the window to read it
            // through — the same gate that let this subscriber fire, since what a plugin may observe is what
            // it may read.
            for (name, value) in
                plugin_callback::env(&self.store.paths.base_dir, plugin_callback::reach_of(project))
            {
                invocation = invocation.env(name, value);
            }
            subscribers.push(Subscriber {
                plugin: plugin.name.clone(),
                invocation,
                config: injection.text,
                reply: subscription.reply,
            });
        }
        subscribers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Paths;
    use crate::plugin_config;
    use crate::plugin_manifest::{ConfigField, EventSubscription, Manifest, Os};

    fn store_at(tag: &str) -> (Store, std::path::PathBuf) {
        let dir = amenbo_scratch::scratch(&format!("plugin-subscribe-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        (Store::open_at(Paths::at(dir.clone())).unwrap(), dir)
    }

    /// A store with one project to hang events on — the ordinary setup, since every gate is a project's
    /// (`AMB-D-434`) and an event with no project fires nothing.
    fn store_in_a_project(tag: &str) -> (Store, std::path::PathBuf, i64) {
        let (mut store, dir) = store_at(tag);
        let project = mk_project(&mut store, "p");
        (store, dir, project)
    }

    /// Open a plugin's gate in one project — what an enable does.
    fn enable_in(store: &mut Store, plugin: &str, project: i64) {
        plugin_trust::enable(store, plugin, project, &[], |_| true).unwrap();
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

    /// A minimal manifest carrying a subscription list and a config schema — the two fields this resolver
    /// reads; the rest is filler it never touches.
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
            detail_sum: None,
            scope: crate::plugin_manifest::Scope::Project,
            // The contract this build speaks: the compatibility gate reads this one, so it tracks
            // `VERSION` rather than sitting on a literal that a bump would turn into a false failure.
            payload_v: crate::plugin_payload::VERSION,
            min_amenbo: None,
            config,
            events: events.iter().map(|e| EventSubscription::new(*e)).collect(),
            agent: None,
        }
    }

    fn installed(name: &str, events: &[&str], config: Vec<ConfigField>) -> InstalledPlugin {
        InstalledPlugin {
            name: name.into(),
            program: PathBuf::from(format!("/plugins/{name}")),
            manifest: manifest(events, config),
            origin: Some(Origin::Official),
        }
    }

    fn text_field(key: &str) -> ConfigField {
        ConfigField::new(key, key)
    }
    fn secret_field(key: &str) -> ConfigField {
        ConfigField { secret: true, ..ConfigField::new(key, key) }
    }

    /// An enabled, subscribed plugin fires; the resolved invocation names its program.
    #[test]
    fn an_enabled_subscribed_plugin_fires() {
        let (mut store, _dir, p) = store_in_a_project("enabled-subscribed");
        enable_in(&mut store, "slack", p);
        let plugins = [installed("slack", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        let subs = resolver.resolve("task.created", Some(p), Face::Cli);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].invocation.program, PathBuf::from("/plugins/slack"));
    }

    /// A resolved subscriber carries the plugin's **name**, not just the program to run: it is the only
    /// place the name is still known, and everything downstream — the hook runner's warnings, the
    /// execution log — reports on plugins, not on paths.
    #[test]
    fn a_resolved_subscriber_carries_the_plugins_name() {
        let (mut store, _dir, p) = store_in_a_project("named");
        enable_in(&mut store, "slack", p);
        let plugins = [installed("slack", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        let subs = resolver.resolve("task.created", Some(p), Face::Cli);
        assert_eq!(subs[0].plugin, "slack");
    }

    /// Installed but not enabled: nothing fires — `install ≠ enable` (`AMB-D-351`).
    #[test]
    fn an_installed_but_disabled_plugin_does_not_fire() {
        let (store, _dir, p) = store_in_a_project("disabled");
        let plugins = [installed("slack", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        assert!(
            resolver.resolve("task.created", Some(p), Face::Cli).is_empty(),
            "an unenabled plugin never fires"
        );
    }

    /// Enabled but not subscribed to this event: nothing fires.
    #[test]
    fn an_enabled_plugin_not_subscribed_to_the_event_does_not_fire() {
        let (mut store, _dir, p) = store_in_a_project("unsubscribed");
        enable_in(&mut store, "slack", p);
        let plugins = [installed("slack", &["comment.added"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        assert!(
            resolver.resolve("task.created", Some(p), Face::Cli).is_empty(),
            "only the subscribed event fires it"
        );
    }

    /// Enabled and subscribed, but incompatible with this build: it is dropped with a warning rather than
    /// fired (`AMB-D-359`) — enable-time is not the only door, since amenbo can update underneath it.
    #[test]
    fn an_incompatible_plugin_does_not_fire() {
        let (mut store, _dir, p) = store_in_a_project("incompatible");
        enable_in(&mut store, "slack", p);
        let mut plugin = installed("slack", &["task.created"], vec![]);
        plugin.manifest.min_amenbo = Some("999.0.0".into());
        let plugins = [plugin];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        assert!(
            resolver.resolve("task.created", Some(p), Face::Cli).is_empty(),
            "a floor this build cannot meet"
        );
    }

    /// One incompatible plugin never silences the rest: delivery is best-effort (`AMB-D-352`).
    #[test]
    fn an_incompatible_plugin_does_not_silence_the_others() {
        let (mut store, _dir, p) = store_in_a_project("incompatible-many");
        enable_in(&mut store, "slack", p);
        enable_in(&mut store, "email", p);
        let mut stale = installed("slack", &["task.created"], vec![]);
        stale.manifest.payload_v = crate::plugin_payload::VERSION + 1;
        let plugins = [stale, installed("email", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        let fired: Vec<_> = resolver
            .resolve("task.created", Some(p), Face::Cli)
            .into_iter()
            .map(|s| s.invocation.program)
            .collect();
        assert_eq!(fired, vec![PathBuf::from("/plugins/email")]);
    }

    /// The plugin's config is injected: a secret rides env, a text field rides the `config` map — split by
    /// the author's `secret` flag (`AMB-D-356`), and only this plugin's own values.
    #[test]
    fn a_subscribers_own_config_is_injected_split_by_secret() {
        let (mut store, _dir, p) = store_in_a_project("inject");
        plugin_config::set(&mut store, &secret_field("webhook_url"), "slack", p, "https://hooks/x").unwrap();
        plugin_config::set(&mut store, &text_field("channel"), "slack", p, "#ops").unwrap();

        enable_in(&mut store, "slack", p);
        let plugins = [installed(
            "slack",
            &["task.created"],
            vec![secret_field("webhook_url"), text_field("channel")],
        )];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        let subs = resolver.resolve("task.created", Some(p), Face::Cli);
        assert_eq!(subs.len(), 1);
        // Secret → env, off the payload. (The read-back path rides the same channel — `AMB-D-406` — so this
        // asks for the one variable rather than for the whole environment.)
        assert_eq!(env_of(&subs[0], "AMENBO_CONFIG_WEBHOOK_URL"), "https://hooks/x");
        // Text → the config map the dispatcher folds onto stdin.
        assert_eq!(subs[0].config.get("channel").and_then(|v| v.as_str()), Some("#ops"));
        assert!(subs[0].config.get("webhook_url").is_none(), "a secret never rides the stdin config");
    }

    /// Several plugins subscribe to one event; every enabled subscriber fires, the disabled one does not.
    #[test]
    fn every_enabled_subscriber_to_an_event_fires() {
        let (mut store, _dir, p) = store_in_a_project("many");
        enable_in(&mut store, "slack", p);
        enable_in(&mut store, "email", p);
        // `audit` is subscribed but never enabled — it must not fire.
        let plugins = [
            installed("slack", &["task.created"], vec![]),
            installed("email", &["task.created"], vec![]),
            installed("audit", &["task.created"], vec![]),
        ];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        let fired: Vec<_> = resolver
            .resolve("task.created", Some(p), Face::Cli)
            .into_iter()
            .map(|s| s.invocation.program)
            .collect();
        assert_eq!(fired, vec![PathBuf::from("/plugins/slack"), PathBuf::from("/plugins/email")]);
    }

    // ───────────────────── the project the event happened in (`AMB-D-434`) ────────────────────────

    /// A plugin fires for an event in a project that has it on — and for nothing else.
    #[test]
    fn a_plugin_fires_only_in_the_project_that_enabled_it() {
        let (mut store, _dir) = store_at("project-gate");
        let a = mk_project(&mut store, "a");
        let b = mk_project(&mut store, "b");
        enable_in(&mut store, "slack", a);
        let plugins = [installed("slack", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        assert_eq!(resolver.resolve("task.created", Some(a), Face::Cli).len(), 1, "on in a");
        assert!(resolver.resolve("task.created", Some(b), Face::Cli).is_empty(), "off in b");
    }

    /// An event whose project cannot be named fires nothing: without a project there is no switch to read,
    /// and firing anyway would open a gate the user never opened.
    #[test]
    fn an_unplaced_event_fires_nothing() {
        let (mut store, _dir, p) = store_in_a_project("project-unplaced");
        enable_in(&mut store, "slack", p);
        let plugins = [installed("slack", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        assert!(resolver.resolve("task.created", None, Face::Cli).is_empty());
    }

    // ───────────────────── the read-back path a subscriber is handed (`AMB-D-406`) ─────────────────

    /// Every fired subscriber is told which store to call `amenbo` into: the one this resolver reads, named
    /// rather than left to the plugin's directory to imply.
    #[test]
    fn a_subscriber_is_told_which_store_to_read_back_from() {
        let (mut store, dir, p) = store_in_a_project("callback-store");
        enable_in(&mut store, "slack", p);
        let plugins = [installed("slack", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        let subs = resolver.resolve("task.created", Some(p), Face::Cli);
        let named = env_of(&subs[0], crate::plugin_callback::STORE_ENV);
        assert_eq!(std::path::PathBuf::from(named), dir);
    }

    /// The window a subscriber reads through is the gate it fired through: the project that has it on
    /// (`AMB-D-406`).
    #[test]
    fn a_subscribers_window_is_the_gate_it_fired_through() {
        let (mut store, _dir, p) = store_in_a_project("callback-reach");
        enable_in(&mut store, "slack", p);
        let plugins = [installed("slack", &["task.created"], vec![])];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        let subs = resolver.resolve("task.created", Some(p), Face::Cli);
        assert_eq!(env_of(&subs[0], crate::plugin_callback::REACH_ENV), crate::idref::project(p));
    }

    /// One variable's value off a resolved subscriber's invocation.
    fn env_of(sub: &Subscriber, key: &str) -> String {
        sub.invocation
            .env
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| panic!("{key} was not set on {}'s invocation", sub.plugin))
    }

    // ───────────────────── the face a subscription fires on (`AMB-D-383`) ──────────────────────────

    /// An enabled plugin whose subscription carries `subscription` — the seam for placing a face-narrowed or
    /// replying subscription on an installed plugin.
    fn installed_sub(name: &str, subscription: EventSubscription) -> InstalledPlugin {
        let mut plugin = installed(name, &[], vec![]);
        plugin.manifest.events = vec![subscription];
        plugin
    }

    /// A subscription fires only on a face it declares: a `faces:[cli]` hook resolves on a CLI drive and
    /// stays silent on a GUI one. This is the filter that keeps a reply off the GUI, where no caller waits.
    #[test]
    fn a_subscription_fires_only_on_a_declared_face() {
        let (mut store, _dir, p) = store_in_a_project("face-filter");
        enable_in(&mut store, "worktree", p);
        let sub = EventSubscription {
            event: "task.status_changed".into(),
            faces: vec![Face::Cli],
            reply: false,
        };
        let plugins = [installed_sub("worktree", sub)];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        assert_eq!(
            resolver.resolve("task.status_changed", Some(p), Face::Cli).len(),
            1,
            "fires on cli"
        );
        assert!(
            resolver.resolve("task.status_changed", Some(p), Face::Gui).is_empty(),
            "the same hook stays silent on the face it did not declare"
        );
    }

    /// The matching subscription's `reply` flag rides onto the resolved subscriber, so the fan-out knows to run
    /// it synchronously and relay its stderr (`AMB-D-383`). A plain subscription resolves `reply:false`.
    #[test]
    fn a_replying_subscription_resolves_a_replying_subscriber() {
        let (mut store, _dir, p) = store_in_a_project("reply-flag");
        enable_in(&mut store, "worktree", p);
        enable_in(&mut store, "slack", p);
        let advice = EventSubscription {
            event: "task.status_changed".into(),
            faces: vec![Face::Cli],
            reply: true,
        };
        let plugins = [
            installed_sub("worktree", advice),
            installed("slack", &["task.status_changed"], vec![]),
        ];

        let resolver = EnabledSubscribers::new(&plugins, &store);
        let subs = resolver.resolve("task.status_changed", Some(p), Face::Cli);
        let worktree = subs.iter().find(|s| s.plugin == "worktree").expect("the advice hook fired");
        assert!(worktree.reply, "the replying subscription resolves a replying subscriber");
        let slack = subs.iter().find(|s| s.plugin == "slack").expect("the notification fired too");
        assert!(!slack.reply, "a plain subscription resolves a non-replying subscriber");
    }
}
