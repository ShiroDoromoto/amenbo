//! The **enable/disable boundary** — `install ≠ enable` (`AMB-D-351`).
//!
//! Installing a plugin puts its binary on disk; it does not run it. Enabling does two things at once, both
//! recorded in [`Config::plugin_trust`]: it writes the **one-time consent** to run the plugin's arbitrary
//! code (`AMB-D-351`, asked once and never again), and it opens the **gate** so the plugin fires
//! (`AMB-D-350`, the machine-global tier the dispatch resolver reads, `AMB-T-2032`). This module is the one
//! door that moves that state, exactly as [`crate::plugin_config`] is the one door for a config *value* —
//! so the fail-closed rule below lives in a single place.
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
use crate::plugin_manifest::ConfigField;

/// The `required` fields of `fields` that have no value per the caller's `has_value` probe — the reason an
/// [`enable`] would be refused. An empty result means every required field is satisfied. Presence is all
/// amenbo checks (`AMB-D-351`); the author validates meaning at run time (`AMB-D-356`).
pub fn missing_required<'a>(
    fields: &'a [ConfigField],
    has_value: impl Fn(&ConfigField) -> bool,
) -> Vec<&'a str> {
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
    let missing = missing_required(fields, has_value);
    if !missing.is_empty() {
        return Err(Error::invalid(
            format!(
                "plugin '{plugin}' cannot be enabled: required setting(s) not provided: {}",
                missing.join(", ")
            ),
            format!(
                "プラグイン '{plugin}' を有効化できません：必須設定が未入力です（{}）",
                missing.join("、")
            ),
        ));
    }
    config.enable_plugin(plugin);
    Ok(())
}

/// Disable a plugin, keeping its consent record (`disable ≠ uninstall`, `AMB-D-357`): the gate closes but
/// the plugin stays installed and consented, so a later [`enable`] runs no gate on consent again. A no-op
/// for a plugin with no trust record. Does not persist.
pub fn disable(config: &mut Config, plugin: &str) {
    config.disable_plugin(plugin);
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
}
