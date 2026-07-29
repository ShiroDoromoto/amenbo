//! The **enable/disable boundary** — `install ≠ enable` (`AMB-D-351`).
//!
//! Installing a plugin puts its binary on disk; it does not run it. Enabling opens the **gate** the plugin
//! fires through. This module is the one door that moves that state, exactly as [`crate::plugin_config`] is
//! the one door for a config *value* — so the fail-closed rule below lives in a single place.
//!
//! **One switch, and it is a project's** (`AMB-D-434`). Every plugin is enabled per project: the gate is a
//! row in the store's `plugin_enable`, and the row **is** the answer — present means on in that project,
//! absent means off. Nothing declares a level, so there is no second tier to inherit from or veto, and a
//! user is never shown two switches for one plugin. A caller standing in no project has no gate to move
//! ([`require_project`]) rather than a device-wide one to fall back on.
//!
//! **Enabling is the consent** (`AMB-D-434`). Running somebody else's code is what turning a plugin on
//! means, so the act is the permission and amenbo keeps no separate answer beside it. That also makes the
//! rows safe to carry whole: they ride `export` and `backup` (a restore must not silently switch a
//! project's plugins off) and say the same thing wherever they land, which is what a device-local answer
//! beside them could never do.
//!
//! **Fail-closed on `required`** (`AMB-D-351`). A plugin whose manifest marks a setting `required` cannot be
//! enabled until that setting holds a value: [`enable`] refuses, naming the empty fields. amenbo checks
//! **presence only** — whether a value is *valid* is the plugin author's at run time (`AMB-D-356`). Where a
//! value lives is the caller's to resolve and report through `has_value`; this boundary does not reach into
//! storage itself.
//!
//! **Not the CLI, not the GUI.** Those faces call in here after they have the manifest and the resolved
//! values; the state model and its gate are here so both drive them the same way.

use crate::error::{Error, ErrorCode, Msg, Result};
use crate::plugin_manifest::ConfigField;
use crate::store::Store;

/// What stopping a plugin threw away — the receipt [`disable`] returns, so a face can report the discard
/// rather than make it silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stopped {
    /// How many events were waiting on the plugin's queue and were dropped (`AMB-D-399`).
    pub queued: usize,
}

/// The project whose gate a face is moving, or the refusal when it is standing in none (`AMB-D-434`).
///
/// Every plugin is a project's, so a context with no project has no answer to give — refused here rather
/// than silently resolved device-wide, which is the layer this removed. One wording, shared by every face
/// that has an `Option<i64>` and needs the gate.
pub fn require_project(project: Option<i64>) -> Result<i64> {
    project.ok_or_else(|| {
        Error::Invalid(
            Msg::new(
                "this plugin is enabled per project, and no project is in context — run it in a bound folder",
            )
            .coded(ErrorCode::InvalidPluginProjectRequired),
        )
    })
}

/// The `required` fields of `fields` that have no value per the caller's `has_value` probe — the reason an
/// [`enable`] would be refused. An empty result means every required field is satisfied. Presence is all
/// amenbo checks (`AMB-D-351`); the author validates meaning at run time (`AMB-D-356`).
///
/// **A field carrying a `default` is never unanswered** (`AMB-D-415`), so it is not held against an enable
/// however it is marked: the store holding nothing for it means the author's value is in force, and that is
/// what the run receives ([`plugin_inject`](crate::plugin_inject)). `required` asks whether the plugin can
/// work without an answer, and a default is one — the two are separate axes, and demanding a user retype a
/// value that is already in effect would refuse a plugin that is, in fact, fully configured.
pub fn missing_required(
    fields: &[ConfigField],
    has_value: impl Fn(&ConfigField) -> bool,
) -> Vec<&str> {
    fields
        .iter()
        .filter(|f| f.required && f.default.is_none() && !has_value(f))
        .map(|f| f.key.as_str())
        .collect()
}

/// Enable a plugin in one project: fail-closed on unsatisfied `required` settings (`AMB-D-351`), then open
/// that project's gate. `has_value` reports whether one field currently holds a value — the caller resolves
/// that; this boundary does not touch storage for it.
///
/// Idempotent: a plugin already on in that project ends where it started.
pub fn enable(
    store: &mut Store,
    plugin: &str,
    project: i64,
    fields: &[ConfigField],
    has_value: impl Fn(&ConfigField) -> bool,
) -> Result<()> {
    refuse_missing_required(plugin, fields, has_value)?;
    store.set_plugin_enabled_in_project(project, plugin, true)?;
    Ok(())
}

/// Close one project's gate (`disable ≠ uninstall`, `AMB-D-357`): the plugin stays installed, so a later
/// [`enable`] costs nothing again. Idempotent, and it does not ask whether the plugin still reads as
/// installed — stopping a half-broken install is exactly when this matters most.
///
/// **And it throws the plugin's waiting work away** (`AMB-D-399`, [`Store::drop_plugin_delivery`]): what is
/// queued for that project goes, and the runner working it ends. Disabling says *do not run this now*, not
/// *save it all up for me* — a queue kept while a plugin is off grows for as long as it stays off and then
/// arrives at once, describing a world that has since moved. The cost is admitted rather than mitigated: an
/// event that happened while a plugin was off never reaches it, and a deletion is the one kind that cannot
/// be caught up on afterwards by re-reading the current state. A plugin whose author would rather decide
/// for itself stays enabled and does nothing.
///
/// Another project's queued rows stay: the switch that closed answers for one project only, so the runner
/// is left to them.
///
/// [`Stopped`] carries what that cost was on this call, so a face can say it out loud instead of leaving a
/// silent discard.
pub fn disable(store: &mut Store, plugin: &str, project: i64) -> Result<Stopped> {
    store.set_plugin_enabled_in_project(project, plugin, false)?;
    // The gate is closed first, so nothing is queued behind the drop: the fan-out asks this same gate
    // (`plugin_subscribe::EnabledSubscribers`) and a write racing this one either sees the gate still open
    // and queues what the drop then takes, or sees it shut and queues nothing.
    let queued = store.drop_plugin_delivery(plugin, Some(project))?;
    Ok(Stopped { queued })
}

/// Whether the plugin fires in `project` — the row, and nothing beside it (`AMB-D-434`).
pub fn effective_enabled_in(store: &Store, plugin: &str, project: i64) -> Result<bool> {
    store.plugin_enabled_in_project(project, plugin)
}

/// The fail-closed `required` check both enable doors run (`AMB-D-351`), as the refusal they share.
fn refuse_missing_required(
    plugin: &str,
    fields: &[ConfigField],
    has_value: impl Fn(&ConfigField) -> bool,
) -> Result<()> {
    let missing = missing_required(fields, has_value);
    if missing.is_empty() {
        return Ok(());
    }
    Err(Error::Invalid(
        Msg::new(format!(
            "plugin '{plugin}' cannot be enabled: required setting(s) not provided: {}",
            missing.join(", ")
        ))
        .coded(ErrorCode::InvalidPluginSettingsRequired)
        .with("name", plugin)
        .with("settings", missing.join(", ")),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(key: &str, required: bool) -> ConfigField {
        ConfigField { required, ..ConfigField::new(key, key) }
    }

    /// A real store on a scratch base, so the enable rows land somewhere.
    fn store_at(tag: &str) -> Store {
        let dir = amenbo_scratch::scratch(&format!("plugin-trust-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        Store::open_at(crate::config::Paths::at(dir)).unwrap()
    }

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

    // ───────────────────────── the gate is a project's, and only one ──────────────────────────────

    /// A context standing in no project is refused, not answered device-wide: there is no device-wide
    /// answer for it to fall back to (`AMB-D-434`).
    #[test]
    fn a_plugin_without_a_project_has_no_gate_to_move() {
        assert_eq!(require_project(Some(7)).unwrap(), 7);
        let err = require_project(None).unwrap_err();
        assert!(format!("{err:?}").contains("per project"), "the reason is named: {err:?}");
    }

    /// A fresh store knows nothing of a plugin — `install ≠ enable`.
    #[test]
    fn an_installed_but_never_enabled_plugin_is_absent() {
        let mut store = store_at("absent");
        let p = mk_project(&mut store, "p");
        assert!(!effective_enabled_in(&store, "slack", p).unwrap());
    }

    /// The gate end to end: this project fires, and no other is touched.
    #[test]
    fn an_enable_opens_only_that_project() {
        let mut store = store_at("project-enable");
        let a = mk_project(&mut store, "a");
        let b = mk_project(&mut store, "b");

        enable(&mut store, "slack", a, &[], |_| true).unwrap();

        assert!(effective_enabled_in(&store, "slack", a).unwrap());
        assert!(!effective_enabled_in(&store, "slack", b).unwrap());
    }

    /// Turning it off in one project leaves the others alone, and turning it back on asks nothing.
    #[test]
    fn a_disable_is_that_project_alone() {
        let mut store = store_at("project-disable");
        let a = mk_project(&mut store, "a");
        let b = mk_project(&mut store, "b");
        enable(&mut store, "slack", a, &[], |_| true).unwrap();
        enable(&mut store, "slack", b, &[], |_| true).unwrap();

        disable(&mut store, "slack", a).unwrap();
        assert!(!effective_enabled_in(&store, "slack", a).unwrap());
        assert!(effective_enabled_in(&store, "slack", b).unwrap(), "b is untouched");

        enable(&mut store, "slack", a, &[], |_| true).unwrap();
        assert!(effective_enabled_in(&store, "slack", a).unwrap());
    }

    /// A restored row fires where it lands (`AMB-D-434`): the state is all in the store, so a backup
    /// carried onto another machine brings the plugin back on with it.
    #[test]
    fn a_carried_row_fires_on_its_own() {
        let mut store = store_at("carried-row");
        let p = mk_project(&mut store, "p");
        // The row as a restore would leave it: written straight to the store.
        store.set_plugin_enabled_in_project(p, "slack", true).unwrap();
        assert!(effective_enabled_in(&store, "slack", p).unwrap());
    }

    // ───────────────────────── what a stop throws away (`AMB-D-399`) ──────────────────────────────

    /// Put one row on a plugin's queue, stamped with `project` — the fan-out's leavings, for the tests
    /// about what a stop does to them.
    fn queue_for(store: &Store, plugin: &str, project: Option<i64>, id: i64) {
        let tx = store.read_model().write().unwrap();
        tx.queue_event(&crate::store_engine::QueuedEvent {
            plugin,
            face: "cli",
            event: "task.created",
            record_id: id,
            actor: "ai",
            at: "2026-07-25T09:00:00Z",
            new_state: None,
            project,
            record: None,
            parent: None,
        })
        .unwrap();
        tx.commit().unwrap();
    }

    fn still_queued(store: &Store, plugin: &str) -> usize {
        crate::store_engine::queued_for(store.read_model().conn(), plugin, 100).unwrap().len()
    }

    /// A plugin turned off in one project keeps what it has waiting in another — the switch that closed
    /// answers for one project only, so the runner is left to it.
    #[test]
    fn disabling_in_one_project_keeps_the_other_projects_queue() {
        let mut store = store_at("project-disable-queue");
        let a = mk_project(&mut store, "A");
        let b = mk_project(&mut store, "B");
        enable(&mut store, "slack", a, &[], |_| true).unwrap();
        enable(&mut store, "slack", b, &[], |_| true).unwrap();
        queue_for(&store, "slack", Some(a), 1);
        queue_for(&store, "slack", Some(b), 2);
        let tx = store.read_model().write().unwrap();
        tx.claim_runner("slack", "runner-1", "2999-01-01T00:00:00Z", "2026-07-25T09:00:00Z").unwrap();
        tx.commit().unwrap();

        let stopped = disable(&mut store, "slack", a).unwrap();

        assert_eq!(stopped.queued, 1, "only the project that was switched off loses its rows");
        assert_eq!(still_queued(&store, "slack"), 1);
        assert!(
            crate::store_engine::lease_of(store.read_model().conn(), "slack").unwrap().is_some(),
            "the runner keeps the lease: it still has the other project's row to carry out"
        );
    }

    /// The last project switching off ends the runner too: nothing is left for it to carry out.
    #[test]
    fn disabling_the_only_project_drops_the_queue_and_the_runners_lease() {
        let mut store = store_at("only-project-disable-queue");
        let p = mk_project(&mut store, "p");
        enable(&mut store, "slack", p, &[], |_| true).unwrap();
        queue_for(&store, "slack", Some(p), 1);
        let tx = store.read_model().write().unwrap();
        tx.claim_runner("slack", "runner-1", "2999-01-01T00:00:00Z", "2026-07-25T09:00:00Z").unwrap();
        tx.commit().unwrap();

        let stopped = disable(&mut store, "slack", p).unwrap();

        assert_eq!(stopped.queued, 1);
        assert_eq!(still_queued(&store, "slack"), 0);
        assert_eq!(
            crate::store_engine::lease_of(store.read_model().conn(), "slack").unwrap(),
            None,
            "and the runner working that queue is ended, not left holding a claim"
        );
    }

    // ───────────────────────── fail-closed on `required` (`AMB-D-351`) ────────────────────────────

    /// A required field with no value is fail-closed, and nothing is recorded.
    #[test]
    fn a_missing_required_field_refuses_enable() {
        let fields = [field("webhook_url", true)];
        let mut store = store_at("required");
        let p = mk_project(&mut store, "p");

        let err = enable(&mut store, "slack", p, &fields, |_| false).unwrap_err();
        assert!(format!("{err:?}").contains("webhook_url"), "the empty field is named");
        assert!(!effective_enabled_in(&store, "slack", p).unwrap());
    }

    /// The same required field, now holding a value, no longer blocks enable.
    #[test]
    fn a_satisfied_required_field_allows_enable() {
        let mut store = store_at("satisfied");
        let p = mk_project(&mut store, "p");
        let fields = [field("webhook_url", true)];
        enable(&mut store, "slack", p, &fields, |f| f.key == "webhook_url").unwrap();
        assert!(effective_enabled_in(&store, "slack", p).unwrap());
    }

    /// `missing_required` reports only the empty required fields — optional and satisfied ones are not
    /// blockers.
    #[test]
    fn missing_required_lists_only_the_empty_required_fields() {
        let fields = [field("a", true), field("b", false), field("c", true)];
        let missing = missing_required(&fields, |f| f.key == "a");
        assert_eq!(missing, vec!["c"]);
    }

    /// A required field with a `default` is answered by its author (`AMB-D-415`): the run receives that
    /// value, so an empty store is not a reason to hold the plugin off.
    #[test]
    fn a_required_field_with_a_default_does_not_block_enable() {
        let fields = [
            ConfigField { default: Some("task.done".into()), ..field("events", true) },
            field("webhook_url", true),
        ];
        assert_eq!(missing_required(&fields, |_| false), vec!["webhook_url"]);

        let mut store = store_at("defaulted");
        let p = mk_project(&mut store, "p");
        enable(&mut store, "slack", p, &fields[..1], |_| false).unwrap();
        assert!(effective_enabled_in(&store, "slack", p).unwrap());
    }
}
