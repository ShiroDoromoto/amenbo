//! The **enable/disable boundary** — `install ≠ enable` (`AMB-D-351`).
//!
//! Installing a plugin puts its binary on disk; it does not run it. Enabling does two things at once: it
//! writes the **one-time consent** to run the plugin's arbitrary code (`AMB-D-351`, asked once and never
//! again, always machine-local in [`Config::plugin_trust`](crate::config::Config::plugin_trust)), and it opens the **gate** the plugin fires
//! through. This module is the one door that moves that state, exactly as [`crate::plugin_config`] is the
//! one door for a config *value* — so the fail-closed rule below lives in a single place.
//!
//! **One switch, and the author says which** (`AMB-D-379`). A manifest declares its
//! [`Scope`], and that decides where the gate lives:
//!
//! | declared | the gate | where it is kept |
//! |---|---|---|
//! | `project` (the default) | this project's, and no other's | a row in the store's `plugin_enable` |
//! | `machine` | the device's, once | the machine field in `config.json` |
//!
//! There is no second tier under either, and so no inheriting and no vetoing: a user is never shown two
//! switches for one plugin, or asked which of them is currently answering. [`gate_for`] is where a
//! declaration plus the caller's context becomes the one gate to move, and it is the only place that
//! judges it.
//!
//! **The consent is the device's, whichever gate is moved.** Enabling a project-scoped plugin records
//! consent on this device and opens *that project's* gate. Reading it back the same way is what makes the
//! rows safe to carry: they ride `export` and `backup` (a restore must not silently switch a project's
//! plugins off), and on a device that never consented they resolve to `false` rather than firing.
//!
//! **Fail-closed on `required`** (`AMB-D-351`). A plugin whose manifest marks a setting `required` cannot be
//! enabled until that setting holds a value: [`enable`] refuses, naming the empty fields. amenbo checks
//! **presence only** — whether a value is *valid* is the plugin author's at run time (`AMB-D-356`). Where a
//! value lives (config tier, secret file, project override) is the caller's to resolve and report through
//! `has_value`; this boundary does not reach into storage itself.
//!
//! **Not the CLI, not the GUI.** Those faces (`AMB-T-1979` / `AMB-T-1985`) call in here after they have the
//! manifest and the resolved values; the state model and its gate are here so both drive them the same way.

use crate::error::{Error, Result};
use crate::plugin_manifest::{ConfigField, Scope};
use crate::store::Store;

/// The one gate a plugin has, resolved from what its manifest declared and where the caller is standing
/// ([`gate_for`]). It is deliberately not [`plugin_config::Scope`](crate::plugin_config::Scope), which
/// names the tier a config *value* is written at: a value has two tiers by design, a gate has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    /// The device's single switch, for a plugin whose manifest declares `scope: machine`.
    Machine,
    /// One project's switch, for a plugin whose manifest declares `scope: project`.
    Project(i64),
}

/// The gate to move for a plugin declaring `declared`, from a context that is in `project` (or in none).
///
/// A `machine` plugin ignores the project entirely — that is what its author declared, and offering a
/// per-project answer would be a switch that looks like it does something and does not. A `project`
/// plugin without a project to name is refused rather than silently answered device-wide: there is no
/// device-wide answer for it to fall back to.
pub fn gate_for(declared: Scope, project: Option<i64>) -> Result<Gate> {
    match declared {
        Scope::Machine => Ok(Gate::Machine),
        Scope::Project => project.map(Gate::Project).ok_or_else(|| {
            Error::invalid(
                "this plugin is enabled per project, and no project is in context — run it in a bound folder",
                "このプラグインはプロジェクト単位で有効化します。プロジェクトの文脈がありません——バインド済みフォルダで実行してください",
            )
        }),
    }
}

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

/// Enable a plugin at the gate its manifest declared ([`gate_for`]): fail-closed on unsatisfied `required`
/// settings (`AMB-D-351`), then record the device's consent and open that one gate. `has_value` reports
/// whether one field currently holds a value — the caller resolves that across the config tiers and the
/// secret file; this boundary does not touch storage for it.
///
/// Idempotent: a plugin already on at that gate ends where it started. Re-enabling a *disabled* plugin
/// keeps its earlier consent, so the user is never asked twice (`AMB-D-351`).
///
/// Both halves persist here, consent first: an interrupted enable may leave a consent with no gate
/// (harmless — nothing fires), never a gate with no consent (which [`effective_enabled_in`] would refuse
/// anyway, but the file on disk should not claim it either).
pub fn enable(
    store: &mut Store,
    plugin: &str,
    gate: Gate,
    fields: &[ConfigField],
    has_value: impl Fn(&ConfigField) -> bool,
) -> Result<()> {
    refuse_missing_required(plugin, fields, has_value)?;
    match gate {
        Gate::Machine => {
            store.config.enable_plugin(plugin);
            store.save_config()?;
        }
        Gate::Project(project_id) => {
            store.config.consent_plugin(plugin);
            store.save_config()?;
            store.set_plugin_enabled_in_project(project_id, plugin, true)?;
        }
    }
    Ok(())
}

/// Close the gate `gate` names, keeping the consent (`disable ≠ uninstall`, `AMB-D-357`): the plugin stays
/// installed and stays consented, so a later [`enable`] asks for nothing again. Idempotent, and it does not
/// ask whether the plugin still reads as installed — stopping a half-broken install is exactly when this
/// matters most.
pub fn disable(store: &mut Store, plugin: &str, gate: Gate) -> Result<()> {
    match gate {
        Gate::Machine => {
            store.config.disable_plugin(plugin);
            store.save_config()?;
        }
        Gate::Project(project_id) => {
            store.set_plugin_enabled_in_project(project_id, plugin, false)?;
        }
    }
    Ok(())
}

/// Whether the plugin fires at the gate `gate` names.
///
/// For a project gate that is **the row and the consent together** (`AMB-D-379`/`AMB-D-351`): a row
/// carried onto a device that never answered the consent question opens nothing. The machine gate needs no
/// such guard — an enabled trust record cannot exist without the consent that wrote it.
pub fn effective_enabled_in(store: &Store, plugin: &str, gate: Gate) -> Result<bool> {
    Ok(match gate {
        Gate::Machine => store.config.plugin_enabled(plugin),
        Gate::Project(project_id) => {
            store.plugin_enabled_in_project(project_id, plugin)?
                && store.config.plugin_consented(plugin)
        }
    })
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

    /// A real store on a scratch base, so the config file and the enable rows both land somewhere.
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

    // ───────────────────────── which gate the declaration names (`AMB-D-379`) ─────────────────────

    #[test]
    fn the_declaration_picks_the_gate() {
        assert_eq!(gate_for(Scope::Machine, None).unwrap(), Gate::Machine);
        assert_eq!(gate_for(Scope::Machine, Some(7)).unwrap(), Gate::Machine, "a project says nothing here");
        assert_eq!(gate_for(Scope::Project, Some(7)).unwrap(), Gate::Project(7));
    }

    /// A project-scoped plugin outside any project is refused, not answered device-wide: there is no
    /// device-wide answer for it to fall back to.
    #[test]
    fn a_project_scoped_plugin_needs_a_project() {
        let err = gate_for(Scope::Project, None).unwrap_err();
        assert!(format!("{err:?}").contains("per project"), "the reason is named: {err:?}");
    }

    // ───────────────────────── the machine gate ───────────────────────────────────────────────────

    /// A fresh store knows nothing of a plugin: not consented, not enabled — `install ≠ enable`.
    #[test]
    fn an_installed_but_never_enabled_plugin_is_absent() {
        let store = store_at("absent");
        assert!(!store.config.plugin_consented("slack"));
        assert!(!effective_enabled_in(&store, "slack", Gate::Machine).unwrap());
    }

    #[test]
    fn enable_records_consent_and_opens_the_gate() {
        let mut store = store_at("machine-enable");
        enable(&mut store, "slack", Gate::Machine, &[], |_| true).unwrap();
        assert!(store.config.plugin_consented("slack"));
        assert!(effective_enabled_in(&store, "slack", Gate::Machine).unwrap());
    }

    /// Disable closes the gate but keeps the consent, so a re-enable never re-asks.
    #[test]
    fn disable_keeps_consent_and_re_enable_needs_no_reconsent() {
        let mut store = store_at("machine-disable");
        enable(&mut store, "slack", Gate::Machine, &[], |_| true).unwrap();
        disable(&mut store, "slack", Gate::Machine).unwrap();
        assert!(!effective_enabled_in(&store, "slack", Gate::Machine).unwrap(), "the gate is closed");
        assert!(store.config.plugin_consented("slack"), "consent survives (disable ≠ uninstall)");

        enable(&mut store, "slack", Gate::Machine, &[], |_| true).unwrap();
        assert!(effective_enabled_in(&store, "slack", Gate::Machine).unwrap());
    }

    /// Uninstall's after-clean erases the consent record entirely (`AMB-D-357`).
    #[test]
    fn forgetting_trust_erases_consent() {
        let mut store = store_at("forget");
        enable(&mut store, "slack", Gate::Machine, &[], |_| true).unwrap();
        store.config.forget_plugin_trust("slack");
        assert!(!store.config.plugin_consented("slack"));
        assert!(!effective_enabled_in(&store, "slack", Gate::Machine).unwrap());
    }

    // ───────────────────────── fail-closed on `required` (`AMB-D-351`) ────────────────────────────

    /// A required field with no value is fail-closed at either gate, and nothing is recorded — no consent
    /// leaks from a refused enable.
    #[test]
    fn a_missing_required_field_refuses_enable_at_either_gate() {
        let fields = [field("webhook_url", true)];
        let mut store = store_at("required");
        let p = mk_project(&mut store, "p");

        let err = enable(&mut store, "slack", Gate::Machine, &fields, |_| false).unwrap_err();
        assert!(format!("{err:?}").contains("webhook_url"), "the empty field is named");
        let err = enable(&mut store, "slack", Gate::Project(p), &fields, |_| false).unwrap_err();
        assert!(format!("{err:?}").contains("webhook_url"));

        assert!(!store.config.plugin_consented("slack"), "a refused enable records no consent");
        assert!(!effective_enabled_in(&store, "slack", Gate::Project(p)).unwrap());
    }

    /// The same required field, now holding a value, no longer blocks enable.
    #[test]
    fn a_satisfied_required_field_allows_enable() {
        let mut store = store_at("satisfied");
        let fields = [field("webhook_url", true)];
        enable(&mut store, "slack", Gate::Machine, &fields, |f| f.key == "webhook_url").unwrap();
        assert!(effective_enabled_in(&store, "slack", Gate::Machine).unwrap());
    }

    /// `missing_required` reports only the empty required fields — optional and satisfied ones are not
    /// blockers.
    #[test]
    fn missing_required_lists_only_the_empty_required_fields() {
        let fields = [field("a", true), field("b", false), field("c", true)];
        let missing = missing_required(&fields, |f| f.key == "a");
        assert_eq!(missing, vec!["c"]);
    }

    // ───────────────────────── the project gate (`AMB-D-379`) ─────────────────────────────────────

    /// The project gate end to end: consent is recorded, this project fires, and no other project — and no
    /// machine answer — is touched.
    #[test]
    fn a_project_enable_opens_only_that_project() {
        let mut store = store_at("project-enable");
        let a = mk_project(&mut store, "a");
        let b = mk_project(&mut store, "b");

        enable(&mut store, "slack", Gate::Project(a), &[], |_| true).unwrap();

        assert!(store.config.plugin_consented("slack"), "the device has consented");
        assert!(!store.config.plugin_enabled("slack"), "no machine gate was opened");
        assert!(effective_enabled_in(&store, "slack", Gate::Project(a)).unwrap());
        assert!(!effective_enabled_in(&store, "slack", Gate::Project(b)).unwrap());
    }

    /// Turning it off in one project leaves the others alone, and turning it back on asks nothing.
    #[test]
    fn a_project_disable_is_that_project_alone() {
        let mut store = store_at("project-disable");
        let a = mk_project(&mut store, "a");
        let b = mk_project(&mut store, "b");
        enable(&mut store, "slack", Gate::Project(a), &[], |_| true).unwrap();
        enable(&mut store, "slack", Gate::Project(b), &[], |_| true).unwrap();

        disable(&mut store, "slack", Gate::Project(a)).unwrap();
        assert!(!effective_enabled_in(&store, "slack", Gate::Project(a)).unwrap());
        assert!(effective_enabled_in(&store, "slack", Gate::Project(b)).unwrap(), "b is untouched");
        assert!(store.config.plugin_consented("slack"), "the consent is the device's, and stays");
    }

    /// The guard that makes the rows safe to carry: a row on a device that never consented fires nothing
    /// (`AMB-D-351` — consent is the device's, and no row can grant it).
    #[test]
    fn a_carried_row_cannot_fire_without_consent() {
        let mut store = store_at("no-consent");
        let p = mk_project(&mut store, "p");
        // The row as a restore would leave it: written straight to the store, with no consent beside it.
        store.set_plugin_enabled_in_project(p, "slack", true).unwrap();
        assert!(store.plugin_enabled_in_project(p, "slack").unwrap(), "the row is there");
        assert!(!effective_enabled_in(&store, "slack", Gate::Project(p)).unwrap(), "and it fires nothing");
    }

    /// The two gates do not answer for each other: a machine-wide enable does not open a project-scoped
    /// plugin's gate anywhere, and a project's row is not a device-wide answer.
    #[test]
    fn neither_gate_answers_for_the_other() {
        let mut store = store_at("no-crossover");
        let p = mk_project(&mut store, "p");

        enable(&mut store, "slack", Gate::Machine, &[], |_| true).unwrap();
        assert!(!effective_enabled_in(&store, "slack", Gate::Project(p)).unwrap());

        enable(&mut store, "worktree", Gate::Project(p), &[], |_| true).unwrap();
        assert!(!effective_enabled_in(&store, "worktree", Gate::Machine).unwrap());
    }
}
