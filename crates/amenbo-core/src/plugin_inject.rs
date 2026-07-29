//! **Run-time config injection** — resolve a plugin's configured values for one run and split them by how
//! each is delivered to the process (`AMB-D-356` / `AMB-T-2016`).
//!
//! The write boundary ([`crate::plugin_config`]) routes each value to *storage* by the author's `secret`
//! flag; this is the mirror on the read side, at the moment a plugin is about to run. Given the plugin's
//! config schema (its manifest [`ConfigField`]s), it reads each field from wherever the boundary put it and
//! splits the results the same way the flag says they travel to the child:
//!
//! - **secret** ⇒ an **environment variable** ([`Injection::env`]), off argv and off logs, named
//!   [`secret_env_name`].
//! - **text** ⇒ a JSON object ([`Injection::text`]) the caller places on the child's **stdin**, under the
//!   payload's config key.
//!
//! Both are read for the **project the run is for** and no other (`AMB-D-434`): a plugin is a project's,
//! so there is one value per field per project and no tier to resolve on the way out. Every run has a
//! project — an event that cannot name one fires nothing.
//!
//! **Only this plugin's config is injected.** [`resolve`] is handed one plugin's schema and reads only that
//! plugin's stored values, so a plugin never sees another's settings — the central-injection promise of
//! `AMB-D-356` (a plugin reads nothing of its own; amenbo hands it exactly, and only, its own).
//!
//! An **unset** field contributes nothing: no env var, no stdin key. Only a field with a value set (the
//! same "not provided is unset" reading the write boundary uses) is injected. This layer does not launch
//! the plugin or build the event payload — it returns the two pieces, and the hook/command wiring
//! (`AMB-T-1972`) attaches [`env`](Injection::env) to the invocation and merges [`text`](Injection::text)
//! into the stdin document.

use serde_json::{Map, Value};

use crate::error::Result;
use crate::plugin_config;
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
/// decides both where the value was stored and how it is injected. `project` is the run's project — the
/// only one whose values this run may see. A field with no value set contributes nothing.
pub fn resolve(
    store: &Store,
    plugin: &str,
    fields: &[ConfigField],
    project: i64,
) -> Result<Injection> {
    let mut injection = Injection::default();
    for field in fields {
        let Some(value) = plugin_config::get(store, field, plugin, project)? else {
            continue;
        };
        if field.secret {
            // Off argv and off logs — an environment variable on the child process.
            injection.env.push((secret_env_name(&field.key), value));
        } else {
            injection.text.insert(field.key.clone(), Value::String(value));
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
        let p = new_project(&mut store, "proj");
        plugin_config::set(&mut store, &secret_field("webhook_url"), "slack", p, "https://hooks/x").unwrap();
        plugin_config::set(&mut store, &text_field("events"), "slack", p, "push,merge").unwrap();

        let fields = [secret_field("webhook_url"), text_field("events")];
        let inj = resolve(&store, "slack", &fields, p).unwrap();

        // Secret → env, never text.
        assert_eq!(inj.env, vec![("AMENBO_CONFIG_WEBHOOK_URL".to_string(), "https://hooks/x".to_string())]);
        // Text → stdin JSON, never env.
        assert_eq!(inj.text.get("events"), Some(&Value::String("push,merge".to_string())));
        assert!(inj.text.get("webhook_url").is_none(), "a secret never appears in the stdin JSON");
    }

    /// A run sees the values of the project it is for, and no other project's (`AMB-D-434`).
    #[test]
    fn a_field_reads_the_value_of_the_project_the_run_is_for() {
        let (mut store, _dir) = store_at("per-project");
        let (here, elsewhere) = (new_project(&mut store, "here"), new_project(&mut store, "elsewhere"));
        plugin_config::set(&mut store, &text_field("events"), "slack", here, "for-here").unwrap();
        plugin_config::set(&mut store, &text_field("events"), "slack", elsewhere, "for-elsewhere").unwrap();

        let inj = resolve(&store, "slack", &[text_field("events")], here).unwrap();
        assert_eq!(inj.text.get("events"), Some(&Value::String("for-here".to_string())));

        // A project that set nothing is handed nothing — there is no tier under it to fall back to.
        let bare = new_project(&mut store, "bare");
        let inj = resolve(&store, "slack", &[text_field("events")], bare).unwrap();
        assert!(inj.text.is_empty());
    }

    #[test]
    fn an_unset_field_contributes_nothing() {
        let (mut store, _dir) = store_at("unset");
        let p = new_project(&mut store, "proj");
        let fields = [secret_field("webhook_url"), text_field("events")];
        let inj = resolve(&store, "slack", &fields, p).unwrap();
        assert!(inj.env.is_empty(), "an unset secret sets no env var");
        assert!(inj.text.is_empty(), "an unset text field adds no stdin key");
    }

    #[test]
    fn only_this_plugins_config_is_injected() {
        let (mut store, _dir) = store_at("scoped");
        let p = new_project(&mut store, "proj");
        // Another plugin's config must not leak into this one's injection.
        plugin_config::set(&mut store, &secret_field("token"), "github", p, "gh-secret").unwrap();
        plugin_config::set(&mut store, &text_field("events"), "slack", p, "push").unwrap();

        let inj = resolve(&store, "slack", &[text_field("events")], p).unwrap();
        assert_eq!(inj.text.get("events"), Some(&Value::String("push".to_string())));
        assert!(inj.env.is_empty(), "the other plugin's secret is not injected here");
        assert!(inj.text.get("token").is_none());
    }
}
