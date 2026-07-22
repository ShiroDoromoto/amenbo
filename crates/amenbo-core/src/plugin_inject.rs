//! **Run-time config injection** — resolve a plugin's configured values for one run and split them by how
//! each is delivered to the process (`AMB-D-356` / `AMB-T-2016`).
//!
//! The write boundary ([`crate::plugin_config`]) routes each value to *storage* by the author's `secret`
//! flag; this is the mirror on the read side, at the moment a plugin is about to run. Given the plugin's
//! config schema (its manifest [`ConfigField`]s), it reads each field from wherever the boundary put it and
//! splits the results the same way the flag says they travel to the child:
//!
//! - **secret** ⇒ an **environment variable** ([`Injection::env`]), off argv and off logs, named
//!   [`secret_env_name`]. The value is pulled from the user-area secret file
//!   ([`crate::plugin_secret`]) — never the store.
//! - **text** ⇒ a JSON object ([`Injection::text`]) the caller places on the child's **stdin**, under the
//!   payload's config key. The value is the **effective** one: a project override
//!   ([`Scope::Project`]) when the run has a project and one is set, otherwise the machine default
//!   ([`Scope::MachineDefault`]).
//!   Reading the effective value — applying the two-tier precedence — is this layer's, not the boundary's
//!   (the boundary reads one tier verbatim; see [`crate::plugin_config::get`]).
//!
//! **Only this plugin's config is injected.** [`resolve`] is handed one plugin's schema and reads only that
//! plugin's stored values, so a plugin never sees another's settings — the central-injection promise of
//! `AMB-D-356` (a plugin reads no secret file of its own; amenbo hands it exactly, and only, its own).
//!
//! An **unset** field contributes nothing: no env var, no stdin key. Only a field with a value set (the
//! same "not provided is unset" reading the write boundary uses) is injected. This layer does not launch
//! the plugin or build the event payload — it returns the two pieces, and the hook/command wiring
//! (`AMB-T-1972`) attaches [`env`](Injection::env) to the invocation and merges [`text`](Injection::text)
//! into the stdin document.

use serde_json::{Map, Value};

use crate::error::Result;
use crate::plugin_config::{self, Scope};
use crate::plugin_manifest::ConfigField;
use crate::store::Store;

/// The environment-variable prefix a secret config value is injected under. Namespaced under `AMENBO_`
/// like amenbo's own variables, but with its own `CONFIG_` segment so a secret keyed, say, `home` becomes
/// `AMENBO_CONFIG_HOME` and never collides with amenbo's reserved `AMENBO_HOME` and its kin. A plugin
/// author reads the value at `$AMENBO_CONFIG_<KEY>` (see [`secret_env_name`] for the exact transform).
pub const SECRET_ENV_PREFIX: &str = "AMENBO_CONFIG_";

/// The environment-variable name a secret field's value is injected under: [`SECRET_ENV_PREFIX`] followed
/// by the field key upper-cased, with every character that is not an ASCII letter or digit mapped to `_`
/// (so `webhook_url` → `AMENBO_CONFIG_WEBHOOK_URL`). Keys are snake_case identifiers by convention (their
/// shape is the validator's, `AMB-T-1988`); the transform is deterministic so an author can name the
/// variable from the key alone.
pub fn secret_env_name(key: &str) -> String {
    let mut name = String::with_capacity(SECRET_ENV_PREFIX.len() + key.len());
    name.push_str(SECRET_ENV_PREFIX);
    for c in key.chars() {
        if c.is_ascii_alphanumeric() {
            name.push(c.to_ascii_uppercase());
        } else {
            name.push('_');
        }
    }
    name
}

/// The two ways a plugin's config reaches the child process, resolved for one run (`AMB-D-356`). The
/// caller sets [`env`](Self::env) on the [`PluginInvocation`](crate::plugin_exec::PluginInvocation) and
/// merges [`text`](Self::text) into the JSON it writes to stdin.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Injection {
    /// Secret fields as environment variables — `(name, value)`, the name from [`secret_env_name`]. In
    /// the same `(String, String)` shape [`PluginInvocation::env`](crate::plugin_exec::PluginInvocation)
    /// takes, so the caller sets each verbatim.
    pub env: Vec<(String, String)>,
    /// Text (non-secret) fields as a JSON object — key → value — for the child's stdin. The caller places
    /// it under the payload's config key. Keys with no value set are absent.
    pub text: Map<String, Value>,
}

/// Resolve a plugin's config for one run and split it into env (secret) and stdin-JSON (text) pieces
/// (`AMB-D-356` / `AMB-T-2016`), reading only *this* plugin's stored values.
///
/// `fields` is the plugin's manifest config schema; each field carries the author's `secret` flag, which
/// decides both where the value was stored and how it is injected. `project` is the run's project context
/// (the write path always has one; `None` falls back to the machine default alone). A field with no value
/// set contributes nothing.
pub fn resolve(
    store: &Store,
    plugin: &str,
    fields: &[ConfigField],
    project: Option<i64>,
) -> Result<Injection> {
    let mut injection = Injection::default();
    for field in fields {
        if field.secret {
            // Secret: the user-area secret file (scope is ignored for a secret). Injected off argv/logs.
            if let Some(value) = plugin_config::get(store, field, plugin, Scope::MachineDefault)? {
                injection.env.push((secret_env_name(&field.key), value));
            }
        } else {
            // Text: the effective value — a project override on top of the machine default.
            let value = match project {
                Some(project_id) => match plugin_config::get(store, field, plugin, Scope::Project(project_id))? {
                    Some(v) => Some(v),
                    None => plugin_config::get(store, field, plugin, Scope::MachineDefault)?,
                },
                None => plugin_config::get(store, field, plugin, Scope::MachineDefault)?,
            };
            if let Some(value) = value {
                injection.text.insert(field.key.clone(), Value::String(value));
            }
        }
    }
    Ok(injection)
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

    fn store_at(tag: &str) -> (Store, std::path::PathBuf) {
        let dir = amenbo_scratch::scratch(&format!("plugin-inject-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open_at(Paths::at(dir.clone())).unwrap();
        (store, dir)
    }

    fn new_project(store: &mut Store, name: &str) -> i64 {
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

    #[test]
    fn the_env_name_prefixes_and_upper_cases_the_key() {
        assert_eq!(secret_env_name("webhook_url"), "AMENBO_CONFIG_WEBHOOK_URL");
        // A char outside [A-Za-z0-9] maps to underscore, and the prefix guards amenbo's own vars.
        assert_eq!(secret_env_name("api.key"), "AMENBO_CONFIG_API_KEY");
        assert_eq!(secret_env_name("home"), "AMENBO_CONFIG_HOME");
    }

    #[test]
    fn a_secret_rides_env_and_text_rides_stdin() {
        let (mut store, _dir) = store_at("split");
        plugin_config::set(&mut store, &secret_field("webhook_url"), "slack", "https://hooks/x", Scope::MachineDefault).unwrap();
        plugin_config::set(&mut store, &text_field("events"), "slack", "push,merge", Scope::MachineDefault).unwrap();

        let fields = [secret_field("webhook_url"), text_field("events")];
        let inj = resolve(&store, "slack", &fields, None).unwrap();

        // Secret → env, never text.
        assert_eq!(inj.env, vec![("AMENBO_CONFIG_WEBHOOK_URL".to_string(), "https://hooks/x".to_string())]);
        // Text → stdin JSON, never env.
        assert_eq!(inj.text.get("events"), Some(&Value::String("push,merge".to_string())));
        assert!(inj.text.get("webhook_url").is_none(), "a secret never appears in the stdin JSON");
    }

    #[test]
    fn a_text_field_reads_its_effective_value_project_over_machine() {
        let (mut store, _dir) = store_at("effective");
        let project = new_project(&mut store, "proj");
        plugin_config::set(&mut store, &text_field("events"), "slack", "default", Scope::MachineDefault).unwrap();
        plugin_config::set(&mut store, &text_field("events"), "slack", "for-proj", Scope::Project(project)).unwrap();

        // With the project context, the override wins.
        let with_project = resolve(&store, "slack", &[text_field("events")], Some(project)).unwrap();
        assert_eq!(with_project.text.get("events"), Some(&Value::String("for-proj".to_string())));

        // Without it, the machine default stands.
        let no_project = resolve(&store, "slack", &[text_field("events")], None).unwrap();
        assert_eq!(no_project.text.get("events"), Some(&Value::String("default".to_string())));
    }

    #[test]
    fn a_text_field_falls_back_to_the_machine_default_when_the_project_has_no_override() {
        let (mut store, _dir) = store_at("fallback");
        let project = new_project(&mut store, "proj");
        // Only a machine default is set; the project has no override of its own.
        plugin_config::set(&mut store, &text_field("events"), "slack", "default", Scope::MachineDefault).unwrap();

        let inj = resolve(&store, "slack", &[text_field("events")], Some(project)).unwrap();
        assert_eq!(inj.text.get("events"), Some(&Value::String("default".to_string())));
    }

    #[test]
    fn an_unset_field_contributes_nothing() {
        let (store, _dir) = store_at("unset");
        let fields = [secret_field("webhook_url"), text_field("events")];
        let inj = resolve(&store, "slack", &fields, None).unwrap();
        assert!(inj.env.is_empty(), "an unset secret sets no env var");
        assert!(inj.text.is_empty(), "an unset text field adds no stdin key");
    }

    #[test]
    fn only_this_plugins_config_is_injected() {
        let (mut store, _dir) = store_at("scoped");
        // Another plugin's config must not leak into this one's injection.
        plugin_config::set(&mut store, &secret_field("token"), "github", "gh-secret", Scope::MachineDefault).unwrap();
        plugin_config::set(&mut store, &text_field("events"), "slack", "push", Scope::MachineDefault).unwrap();

        let inj = resolve(&store, "slack", &[text_field("events")], None).unwrap();
        assert_eq!(inj.text.get("events"), Some(&Value::String("push".to_string())));
        assert!(inj.env.is_empty(), "the other plugin's secret is not injected here");
        assert!(inj.text.get("token").is_none());
    }
}
