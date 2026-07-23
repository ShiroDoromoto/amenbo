//! The **enable/disable boundary** — `install ≠ enable` (`AMB-D-351`).
//!
//! Installing a plugin puts its binary on disk; it does not run it. Enabling does two things at once, both
//! recorded in [`Config::plugin_trust`]: it writes the **one-time consent** to run the plugin's arbitrary
//! code (`AMB-D-351`, asked once and never again), and it opens the **gate** so the plugin fires
//! (`AMB-D-350`, the machine-global tier the dispatch resolver reads, `AMB-T-2032`). This module is the one
//! door that moves that state, exactly as [`crate::plugin_config`] is the one door for a config *value* —
//! so the fail-closed rule below lives in a single place.
//!
//! **Two tiers, one gate** (`AMB-D-350`). The machine-global answer above is the lower tier; a project may
//! override it with a row in the store's `plugin_enable` table ([`crate::ops::plugin_enable`]), and
//! [`effective_enabled`] is the resolution — the project's answer if it declares one, the machine's
//! otherwise. The tiers are the same two a text config value lives in, so they are named by the same
//! [`Scope`].
//!
//! **The project tier moves the gate, never the consent.** Consent is the device's answer to running this
//! code at all, so it stays machine-local whichever tier is being written: a project-scoped enable records
//! it ([`Config::consent_plugin`]) and opens *that project's* gate, leaving the machine gate as it found
//! it. Reading it back the same way is what makes the override safe to carry: the row rides `export` and
//! `backup` (a restore must not reopen a gate the user closed), and on a device that never consented it
//! resolves to `false` rather than firing.
//!
//! **Fail-closed on `required`** (`AMB-D-351`). A plugin whose manifest marks a setting `required` cannot be
//! enabled until that setting holds a value: [`enable`] refuses, naming the empty fields. amenbo checks
//! **presence only** — whether a value is *valid* is the plugin author's at run time (`AMB-D-356`). Where a
//! value lives (config tier, secret file, project override) is the caller's to resolve and report through
//! `has_value`; this boundary does not reach into storage itself.
//!
//! **Not the CLI, not the GUI.** Those faces (`AMB-T-1979` / `AMB-T-1985`) call in here after they have the
//! manifest and the resolved values; the state model and its gate are here so both drive them the same way.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::plugin_config::Scope;
use crate::plugin_manifest::ConfigField;
use crate::store::Store;

/// The `required` fields of `fields` that have no value per the caller's `has_value` probe — the reason an
/// [`enable`] would be refused. An empty result means every required field is satisfied. Presence is all
/// amenbo checks (`AMB-D-351`); the author validates meaning at run time (`AMB-D-356`).
pub fn missing_required(
    fields: &[ConfigField],
    has_value: impl Fn(&ConfigField) -> bool,
) -> Vec<&str> {
    fields
        .iter()
        .filter(|f| f.required && !has_value(f))
        .map(|f| f.key.as_str())
        .collect()
}

/// Enable a plugin: fail-closed on unsatisfied `required` settings (`AMB-D-351`), then record consent and
/// open the gate ([`Config::enable_plugin`]). `has_value` reports whether one field currently holds a value
/// — the caller resolves that across the config tiers and the secret file; this boundary does not touch
/// storage. Does **not** persist: the caller saves the config through the write boundary.
///
/// Idempotent for an already-enabled plugin. Re-enabling a *disabled* plugin keeps its earlier consent, so
/// the user is never asked twice (`AMB-D-351`).
pub fn enable(
    config: &mut Config,
    plugin: &str,
    fields: &[ConfigField],
    has_value: impl Fn(&ConfigField) -> bool,
) -> Result<()> {
    refuse_missing_required(plugin, fields, has_value)?;
    config.enable_plugin(plugin);
    Ok(())
}

/// Disable a plugin, keeping its consent record (`disable ≠ uninstall`, `AMB-D-357`): the gate closes but
/// the plugin stays installed and consented, so a later [`enable`] runs no gate on consent again. A no-op
/// for a plugin with no trust record. Does not persist.
pub fn disable(config: &mut Config, plugin: &str) {
    config.disable_plugin(plugin);
}

/// Enable a plugin **for one project** (`AMB-D-350`, the upper tier): fail-closed on unsatisfied `required`
/// settings exactly as [`enable`] is, then record the device's consent and open this project's gate — the
/// machine-global gate is left as it was, so the plugin fires here and nowhere else. `has_value` reports
/// whether one field currently holds a value; the caller resolves that across the tiers it means to count
/// (for a project-scoped enable, the project's override on top of the machine default).
///
/// Persists both halves: the consent through the config write boundary, the gate as its own transaction.
/// Idempotent — a project already enabled ends where it started.
pub fn enable_for_project(
    store: &mut Store,
    plugin: &str,
    project_id: i64,
    fields: &[ConfigField],
    has_value: impl Fn(&ConfigField) -> bool,
) -> Result<()> {
    refuse_missing_required(plugin, fields, has_value)?;
    // Consent first, and saved before the gate opens: an interrupted enable may leave a consent with no
    // gate (harmless — nothing fires), never a gate with no consent (which `effective_enabled` would
    // refuse anyway, but the file on disk should not claim it either).
    store.config.consent_plugin(plugin);
    store.save_config()?;
    store.set_plugin_enable_override(project_id, plugin, Some(true))?;
    Ok(())
}

/// Close one project's gate (`AMB-D-350`), keeping the consent: the override says `false` here, which
/// stands even while the machine-global gate is open. Idempotent. To go back to following the machine
/// answer, clear the override instead ([`inherit_in_project`]) — "off here" and "whatever the machine
/// says" are different states.
pub fn disable_for_project(store: &mut Store, plugin: &str, project_id: i64) -> Result<()> {
    store.set_plugin_enable_override(project_id, plugin, Some(false))?;
    Ok(())
}

/// Drop this project's override so the machine-global gate answers for it again (`AMB-D-350`). Returns
/// whether there was one to drop.
pub fn inherit_in_project(store: &mut Store, plugin: &str, project_id: i64) -> Result<bool> {
    store.set_plugin_enable_override(project_id, plugin, None)
}

/// Whether the plugin fires, given the project's answer (`AMB-D-350`): the project's override if it
/// declares one, the machine-global gate otherwise. `project_override` is `None` both for a project that
/// declares nothing and for a context that has no project at all.
///
/// **An override cannot grant consent.** A `true` override only fires on a device that has consented to
/// this plugin (`AMB-D-351`), which is what keeps an exported override from opening a gate on a machine
/// that never answered the question. The machine tier needs no such guard — an `enabled` trust record
/// cannot exist without the consent that wrote it.
pub fn effective_enabled(config: &Config, plugin: &str, project_override: Option<bool>) -> bool {
    match project_override {
        Some(on) => on && config.plugin_consented(plugin),
        None => config.plugin_enabled(plugin),
    }
}

/// [`effective_enabled`] over a store: reads the project's override for itself, at the tier `scope` names.
/// `Scope::MachineDefault` asks for the machine answer alone (no project is consulted), which is what a
/// context with no project — the dispatch resolver's drained event — has to use.
pub fn effective_enabled_in(store: &Store, plugin: &str, scope: Scope) -> Result<bool> {
    let project_override = match scope {
        Scope::MachineDefault => None,
        Scope::Project(project_id) => store.plugin_enable_override(project_id, plugin)?,
    };
    Ok(effective_enabled(&store.config, plugin, project_override))
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
    Err(Error::invalid(
        format!(
            "plugin '{plugin}' cannot be enabled: required setting(s) not provided: {}",
            missing.join(", ")
        ),
        format!(
            "プラグイン '{plugin}' を有効化できません：必須設定が未入力です（{}）",
            missing.join("、")
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(key: &str, required: bool) -> ConfigField {
        ConfigField { key: key.to_string(), label: key.to_string(), secret: false, required }
    }

    /// A fresh config knows nothing of a plugin: not consented, not enabled — `install ≠ enable`.
    #[test]
    fn an_installed_but_never_enabled_plugin_is_absent() {
        let config = Config::default();
        assert!(!config.plugin_consented("slack"));
        assert!(!config.plugin_enabled("slack"));
    }

    /// Enabling with no required fields records consent and opens the gate.
    #[test]
    fn enable_records_consent_and_opens_the_gate() {
        let mut config = Config::default();
        enable(&mut config, "slack", &[], |_| true).unwrap();
        assert!(config.plugin_consented("slack"));
        assert!(config.plugin_enabled("slack"));
    }

    /// A required field with no value is fail-closed: enable is refused and nothing is recorded (no consent
    /// leaks from a refused enable).
    #[test]
    fn a_missing_required_field_refuses_enable() {
        let mut config = Config::default();
        let fields = [field("webhook_url", true)];
        let err = enable(&mut config, "slack", &fields, |_| false).unwrap_err();
        assert!(format!("{err:?}").contains("webhook_url"), "the empty field is named");
        assert!(!config.plugin_consented("slack"), "a refused enable records no consent");
        assert!(!config.plugin_enabled("slack"));
    }

    /// The same required field, now holding a value, no longer blocks enable.
    #[test]
    fn a_satisfied_required_field_allows_enable() {
        let mut config = Config::default();
        let fields = [field("webhook_url", true)];
        enable(&mut config, "slack", &fields, |f| f.key == "webhook_url").unwrap();
        assert!(config.plugin_enabled("slack"));
    }

    /// `missing_required` reports only the empty required fields — optional ones and satisfied ones are not
    /// blockers.
    #[test]
    fn missing_required_lists_only_the_empty_required_fields() {
        let fields = [field("a", true), field("b", false), field("c", true)];
        // Only `a` has a value; `b` is optional; `c` is required and empty.
        let missing = missing_required(&fields, |f| f.key == "a");
        assert_eq!(missing, vec!["c"]);
    }

    /// Disable closes the gate but keeps the consent: a re-enable does not re-run the consent path, and here
    /// re-enable needs no required check because none is declared.
    #[test]
    fn disable_keeps_consent_and_re_enable_needs_no_reconsent() {
        let mut config = Config::default();
        enable(&mut config, "slack", &[], |_| true).unwrap();
        disable(&mut config, "slack");
        assert!(!config.plugin_enabled("slack"), "the gate is closed");
        assert!(config.plugin_consented("slack"), "consent survives a disable (disable ≠ uninstall)");

        enable(&mut config, "slack", &[], |_| true).unwrap();
        assert!(config.plugin_enabled("slack"), "re-enable reopens the gate");
    }

    /// Uninstall's after-clean erases the consent record entirely (`AMB-D-357`).
    #[test]
    fn forgetting_trust_erases_consent() {
        let mut config = Config::default();
        enable(&mut config, "slack", &[], |_| true).unwrap();
        config.forget_plugin_trust("slack");
        assert!(!config.plugin_consented("slack"));
        assert!(!config.plugin_enabled("slack"));
    }

    // ───────────────────────── the project tier (`AMB-D-350`) ─────────────────────────

    /// With no project override, the effective answer *is* the machine gate — the tier is inert until a
    /// project declares something.
    #[test]
    fn with_no_override_the_machine_gate_answers() {
        let mut config = Config::default();
        assert!(!effective_enabled(&config, "slack", None));
        enable(&mut config, "slack", &[], |_| true).unwrap();
        assert!(effective_enabled(&config, "slack", None));
    }

    /// Either answer overrides the machine gate, in either direction.
    #[test]
    fn an_override_answers_over_the_machine_gate() {
        let mut config = Config::default();
        enable(&mut config, "slack", &[], |_| true).unwrap();
        assert!(!effective_enabled(&config, "slack", Some(false)), "off here, over an open gate");

        disable(&mut config, "slack");
        assert!(effective_enabled(&config, "slack", Some(true)), "on here, over a closed gate");
    }

    /// The guard that makes the override safe to export: a `true` row on a device that never consented
    /// fires nothing (`AMB-D-351` — consent is the device's, and no row can grant it).
    #[test]
    fn an_override_cannot_fire_without_consent() {
        let config = Config::default(); // never consented on this device
        assert!(!effective_enabled(&config, "slack", Some(true)));
    }

    /// A disabled-but-consented plugin keeps its consent, so a project override may reopen it there —
    /// the state a `disable` then a project-scoped `enable` leaves.
    #[test]
    fn a_disabled_plugin_keeps_the_consent_an_override_needs() {
        let mut config = Config::default();
        enable(&mut config, "slack", &[], |_| true).unwrap();
        disable(&mut config, "slack");
        assert!(config.plugin_consented("slack"));
        assert!(effective_enabled(&config, "slack", Some(true)));
    }

    /// Recording consent on its own never opens the machine gate — that is what a project-scoped enable
    /// leaves behind, and the difference is the whole point of the two tiers.
    #[test]
    fn consent_alone_opens_no_gate() {
        let mut config = Config::default();
        config.consent_plugin("slack");
        assert!(config.plugin_consented("slack"));
        assert!(!config.plugin_enabled("slack"), "consent is not a gate");
        assert!(!effective_enabled(&config, "slack", None));
    }

    /// A real store on a scratch base, so the config file and the override rows both land somewhere.
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

    /// The project tier end to end: consent is recorded, this project fires, the machine gate stays shut
    /// — so every other project (and every context with no project) is unchanged.
    #[test]
    fn a_project_scoped_enable_opens_only_that_project() {
        let mut store = store_at("project-enable");
        let a = mk_project(&mut store, "a");
        let b = mk_project(&mut store, "b");

        enable_for_project(&mut store, "slack", a, &[], |_| true).unwrap();

        assert!(store.config.plugin_consented("slack"), "the device has consented");
        assert!(!store.config.plugin_enabled("slack"), "the machine gate is untouched");
        assert!(effective_enabled_in(&store, "slack", Scope::Project(a)).unwrap());
        assert!(!effective_enabled_in(&store, "slack", Scope::Project(b)).unwrap());
        assert!(!effective_enabled_in(&store, "slack", Scope::MachineDefault).unwrap());
    }

    /// The project tier is fail-closed on `required` exactly as the machine tier is — and a refused
    /// enable records nothing at all, consent included.
    #[test]
    fn a_project_scoped_enable_is_fail_closed_on_required() {
        let mut store = store_at("project-required");
        let p = mk_project(&mut store, "p");
        let fields = [field("webhook_url", true)];

        let err = enable_for_project(&mut store, "slack", p, &fields, |_| false).unwrap_err();
        assert!(format!("{err:?}").contains("webhook_url"), "the empty field is named");
        assert!(!store.config.plugin_consented("slack"), "a refused enable records no consent");
        assert!(!effective_enabled_in(&store, "slack", Scope::Project(p)).unwrap());
    }

    /// One project can veto a machine-wide enable, and `inherit` puts it back under the machine answer —
    /// the third state a stored `false` is not.
    #[test]
    fn a_project_veto_survives_a_machine_enable_until_it_is_inherited() {
        let mut store = store_at("project-veto");
        let p = mk_project(&mut store, "p");
        enable(&mut store.config, "slack", &[], |_| true).unwrap();
        store.save_config().unwrap();

        disable_for_project(&mut store, "slack", p).unwrap();
        assert!(!effective_enabled_in(&store, "slack", Scope::Project(p)).unwrap(), "off here");
        assert!(effective_enabled_in(&store, "slack", Scope::MachineDefault).unwrap(), "on elsewhere");

        assert!(inherit_in_project(&mut store, "slack", p).unwrap(), "there was an override to drop");
        assert!(effective_enabled_in(&store, "slack", Scope::Project(p)).unwrap());
        assert!(!inherit_in_project(&mut store, "slack", p).unwrap(), "dropping nothing is a no-op");
    }
}
