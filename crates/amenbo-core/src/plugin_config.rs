//! The **config write boundary** — the single function every face passes a plugin config value through
//! (`AMB-D-356`).
//!
//! A plugin's settings are declared by its author as a flat schema of [`ConfigField`]s
//! ([`crate::plugin_manifest`]); each field carries a `secret` flag. amenbo does **not** judge what is
//! secret — it routes by that flag alone:
//!
//! - **secret** ⇒ the user-area secret file ([`crate::plugin_secret`]), owner-only (0600), off the store
//!   and off every backup/export. Injected at run time as an environment variable (`AMB-T-2016`).
//! - **text** ⇒ the ordinary two tiers. Either the **machine default** in `config.json`
//!   ([`crate::config::Config::plugin_config`]) or the **per-project override** in the store's
//!   `plugin_config` record table, chosen by [`Scope`]. Injected on stdin as JSON.
//!
//! Both the CLI (`plugin config set/get`) and the GUI form submit go through [`set`] / [`get`] so the
//! routing rule, and the safe floor below, live in exactly one place (`AMB-D-356`). The **secret flag
//! is the author's**, read off the plugin's manifest by the caller and handed in as `field` — this
//! function never guesses whether a key is a secret.
//!
//! **The safe floor** (`AMB-D-354`) is amenbo's "unbreakable floor": a per-value byte cap and a
//! control-character reject, whose only purpose is to stop a runaway value from bloating the store or the
//! secret file — it never judges whether a value is *meaningful* (a valid URL, an email); that is the
//! plugin author's at run time. The fuller manifest-shape validation is `AMB-T-1988`'s; what lives here is
//! only the floor over the *value a user typed*, enforced at this one write boundary.
//!
//! Setting a field to the **empty string clears it** — an empty value is "not provided" (the same reading
//! `required` uses), so it removes the machine default / project override / secret rather than storing a
//! blank. There is thus one door for both set and unset.

use crate::error::{Error, Result};
use crate::plugin_manifest::ConfigField;
use crate::store::Store;

/// The largest a single config value may be, in bytes. Loose — a webhook URL or a short token fits with
/// room to spare — because the cap exists to stop a runaway value bloating the store or the secret file,
/// not to ration. Applied by [`check_value`] at the write boundary, to secret and text alike.
pub const MAX_CONFIG_VALUE_BYTES: usize = 8 * 1024;

/// The largest a plugin name or field key may be, in bytes. These arrive from the manifest (validated in
/// shape by `AMB-T-1988`), but the write boundary does not trust its caller: a key that would be a storage
/// key is length-capped here too, defense in depth.
pub const MAX_CONFIG_IDENT_BYTES: usize = 128;

/// Which text tier a value is written to (`AMB-D-356` / `AMB-D-350`). Ignored for a `secret` field, which
/// always lands in the user-area secret file regardless — there is no per-project secret in v1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The machine default, in `config.json` — the lower tier, the value used when a project has no
    /// override of its own.
    MachineDefault,
    /// The named project's override, in the store — the upper tier, taking precedence for that project.
    Project(i64),
}

/// Enforce the safe floor on a value (`AMB-D-354`): the byte cap and the control-character reject. An empty
/// value is exempt — it is the clear path, checked before this is reached.
pub fn check_value(value: &str) -> Result<()> {
    if value.len() > MAX_CONFIG_VALUE_BYTES {
        return Err(Error::invalid(
            format!("config value too large ({} bytes; max {})", value.len(), MAX_CONFIG_VALUE_BYTES),
            format!("設定値が大きすぎます（{} バイト・上限 {}）", value.len(), MAX_CONFIG_VALUE_BYTES),
        ));
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(Error::invalid(
            "config value must not contain control characters",
            "設定値に制御文字を含めることはできません",
        ));
    }
    Ok(())
}

/// Enforce the identifier floor on a plugin name or field key: non-empty and within the byte cap.
fn check_ident(kind: &str, s: &str) -> Result<()> {
    if s.is_empty() {
        return Err(Error::invalid(
            format!("plugin config {kind} must not be empty"),
            format!("プラグイン設定の{kind}は空にできません"),
        ));
    }
    if s.len() > MAX_CONFIG_IDENT_BYTES {
        return Err(Error::invalid(
            format!("plugin config {kind} too long ({} bytes; max {})", s.len(), MAX_CONFIG_IDENT_BYTES),
            format!("プラグイン設定の{kind}が長すぎます（{} バイト・上限 {}）", s.len(), MAX_CONFIG_IDENT_BYTES),
        ));
    }
    Ok(())
}

/// Write one plugin config value through the boundary (`AMB-D-356`). Routes by `field.secret`, enforcing
/// the safe floor first. An **empty** `value` clears the setting (removes the machine default / project
/// override / secret) rather than storing a blank — "not provided" is unset. Persists as it goes: a text
/// machine default saves `config.json`, a project override commits its own transaction, a secret rewrites
/// the secret file (0600).
///
/// `field` carries the author's `secret` flag and the `key` — the caller loads it from the plugin's
/// manifest; this function never decides secrecy for itself.
pub fn set(store: &mut Store, field: &ConfigField, plugin: &str, value: &str, scope: Scope) -> Result<()> {
    check_ident("plugin name", plugin)?;
    check_ident("key", &field.key)?;
    // A value is stored verbatim (no trimming): whitespace can be significant. Empty is the clear path.
    let stored: Option<&str> = if value.is_empty() {
        None
    } else {
        check_value(value)?;
        Some(value)
    };

    if field.secret {
        // Secret: the user-area file, off the store and off every backup. Scope does not apply — there is
        // one secret per (plugin, key) in v1.
        let path = store.paths.plugin_secrets_file();
        let mut secrets = crate::plugin_secret::Secrets::load(&path)?;
        secrets.set(plugin, &field.key, stored);
        secrets.save(&path)?;
        return Ok(());
    }

    match scope {
        Scope::MachineDefault => {
            store.config.set_plugin_text_default(plugin, &field.key, stored);
            store.save_config()?;
        }
        Scope::Project(project_id) => {
            store.set_plugin_config_override(project_id, plugin, &field.key, stored)?;
        }
    }
    Ok(())
}

/// Read back one plugin config value **at the requested scope** (`AMB-D-356`). For a text field this is the
/// machine default or the project override, exactly as [`set`] wrote it — *not* the effective value with
/// precedence applied (that resolution, and secret injection, are the run-time injection layer's,
/// `AMB-T-2016`). For a secret it is the value from the secret file (scope ignored). Returns `None` when
/// the setting is unset at that scope.
///
/// The value is returned raw; a secret is not masked here — the CLI face masks it on the way to the
/// terminal (`plugin config get` never echoes a secret), while injection reads it whole.
pub fn get(store: &Store, field: &ConfigField, plugin: &str, scope: Scope) -> Result<Option<String>> {
    if field.secret {
        let path = store.paths.plugin_secrets_file();
        let secrets = crate::plugin_secret::Secrets::load(&path)?;
        return Ok(secrets.get(plugin, &field.key).map(str::to_string));
    }
    match scope {
        Scope::MachineDefault => Ok(store.config.plugin_text_default(plugin, &field.key).map(str::to_string)),
        Scope::Project(project_id) => store.plugin_config_override(project_id, plugin, &field.key),
    }
}

/// Which of the author's declared `fields` currently hold a value, probed at the tier an enable is for
/// (`AMB-D-356`): the machine defaults, plus this project's overrides on top when the gate being opened is
/// a project's. This is the `has_value` probe
/// [`plugin_trust::enable`](crate::plugin_trust::enable) asks its caller for — that boundary judges
/// `required` and deliberately does not read storage, so the resolution lives here with the rest of the
/// value routing, and both faces (CLI `plugin enable`, the GUI's) run the same one.
///
/// Probed into a list first because the probe reads the store while the enable writes inside it, so the
/// two cannot borrow it at once.
pub fn satisfied_keys(
    store: &Store,
    plugin: &str,
    fields: &[ConfigField],
    tier: Scope,
) -> Result<Vec<String>> {
    let mut satisfied = Vec::new();
    for field in fields {
        let held = get(store, field, plugin, Scope::MachineDefault)?.is_some()
            || match tier {
                Scope::MachineDefault => false,
                Scope::Project(_) => get(store, field, plugin, tier)?.is_some(),
            };
        if held {
            satisfied.push(field.key.clone());
        }
    }
    Ok(satisfied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Paths;
    use crate::plugin_manifest::ConfigField;

    fn text_field(key: &str) -> ConfigField {
        ConfigField { key: key.to_string(), label: key.to_string(), secret: false, required: false }
    }
    fn secret_field(key: &str) -> ConfigField {
        ConfigField { key: key.to_string(), label: key.to_string(), secret: true, required: false }
    }

    /// Open a real store under a scratch AMENBO_HOME so config.json and the secret file resolve under it.
    fn store_at(tag: &str) -> (Store, std::path::PathBuf) {
        let dir = amenbo_scratch::scratch(&format!("plugin-config-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open_at(Paths::at(dir.clone())).unwrap();
        (store, dir)
    }

    #[test]
    fn a_text_machine_default_lands_in_config_and_not_the_store() {
        let (mut store, dir) = store_at("text-machine");
        set(&mut store, &text_field("events"), "slack", "push,merge", Scope::MachineDefault).unwrap();

        // Read back at machine scope.
        assert_eq!(
            get(&store, &text_field("events"), "slack", Scope::MachineDefault).unwrap().as_deref(),
            Some("push,merge"),
        );
        // It is in config.json on disk (persisted), and the config field carries it.
        assert_eq!(store.config.plugin_text_default("slack", "events"), Some("push,merge"));
        let raw = std::fs::read_to_string(dir.join("config.json")).unwrap();
        assert!(raw.contains("push,merge"), "the machine default is persisted to config.json");
    }

    #[test]
    fn a_secret_lands_in_the_secret_file_never_the_store_or_config() {
        let (mut store, dir) = store_at("secret");
        set(&mut store, &secret_field("webhook_url"), "slack", "https://hooks/x", Scope::MachineDefault)
            .unwrap();

        // Read back through the boundary.
        assert_eq!(
            get(&store, &secret_field("webhook_url"), "slack", Scope::MachineDefault).unwrap().as_deref(),
            Some("https://hooks/x"),
        );
        // It is in the secret file...
        let secret_raw = std::fs::read_to_string(dir.join(Paths::PLUGIN_SECRETS_FILE_NAME)).unwrap();
        assert!(secret_raw.contains("https://hooks/x"), "the secret is in the secret file");
        // ...and NOT in config.json (a re-read of config from disk shows nothing).
        let cfg = std::fs::read_to_string(dir.join("config.json")).unwrap_or_default();
        assert!(!cfg.contains("https://hooks/x"), "a secret must never reach config.json");
        assert!(store.config.plugin_text_default("slack", "webhook_url").is_none());
    }

    #[test]
    fn empty_value_clears_at_each_kind() {
        let (mut store, _dir) = store_at("clear");
        // machine text
        set(&mut store, &text_field("k"), "p", "v", Scope::MachineDefault).unwrap();
        set(&mut store, &text_field("k"), "p", "", Scope::MachineDefault).unwrap();
        assert_eq!(get(&store, &text_field("k"), "p", Scope::MachineDefault).unwrap(), None);
        // secret
        set(&mut store, &secret_field("s"), "p", "v", Scope::MachineDefault).unwrap();
        set(&mut store, &secret_field("s"), "p", "", Scope::MachineDefault).unwrap();
        assert_eq!(get(&store, &secret_field("s"), "p", Scope::MachineDefault).unwrap(), None);
    }

    #[test]
    fn a_project_override_sits_on_top_of_the_machine_default() {
        let (mut store, _dir) = store_at("project");
        let project = store.project_add(crate::ops::project::NewProject {
            name: "proj".into(),
            view: crate::model::View::List,
            notes: String::new(),
            color: None,
        }).unwrap();

        set(&mut store, &text_field("events"), "slack", "default", Scope::MachineDefault).unwrap();
        set(&mut store, &text_field("events"), "slack", "for-proj", Scope::Project(project.id)).unwrap();

        // Each scope reads back its own tier.
        assert_eq!(
            get(&store, &text_field("events"), "slack", Scope::MachineDefault).unwrap().as_deref(),
            Some("default"),
        );
        assert_eq!(
            get(&store, &text_field("events"), "slack", Scope::Project(project.id)).unwrap().as_deref(),
            Some("for-proj"),
        );
    }

    #[test]
    fn the_floor_rejects_an_oversize_or_control_char_value() {
        let (mut store, _dir) = store_at("floor");
        let big = "x".repeat(MAX_CONFIG_VALUE_BYTES + 1);
        assert!(set(&mut store, &text_field("k"), "p", &big, Scope::MachineDefault).is_err());
        assert!(set(&mut store, &text_field("k"), "p", "a\u{0}b", Scope::MachineDefault).is_err());
        // Nothing landed from the rejected writes.
        assert_eq!(get(&store, &text_field("k"), "p", Scope::MachineDefault).unwrap(), None);
    }
}
