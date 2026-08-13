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
//! Both are read at the **layer this plugin lives at** and no other (`AMB-D-434`/`AMB-D-601`): the run's
//! project for a plugin declaring the project layer, the device for one declaring the machine layer. Either
//! way there is one value per field per layer and no tier to resolve on the way out. Every run still
//! happens inside a project — an event that cannot name one fires nothing.
//!
//! **Only this plugin's config is injected.** [`resolve`] is handed one plugin's schema and reads only that
//! plugin's stored values, so a plugin never sees another's settings — the central-injection promise of
//! `AMB-D-356` (a plugin reads nothing of its own; amenbo hands it exactly, and only, its own).
//!
//! **Resolution happens here, so a plugin reads answers and not amenbo's bookkeeping** (`AMB-D-415`). An
//! **unset** field falls back to the author's [`default`](ConfigField::default), and contributes nothing
//! only when there is none — the store holds no row for an unanswered field, so a manifest that changes its
//! default reaches every project that never answered. A field whose user chose *none* of its candidates is
//! injected **empty**: the reserved word that tells that answer apart from silence in storage
//! ([`NONE_SELECTED`](crate::plugin_manifest::NONE_SELECTED)) is spent by the time the child sees it, and
//! an author writes no special case for a spelling they never chose.
//!
//! This layer does not launch the plugin or build the event payload — it returns the two pieces, and the
//! hook/command wiring (`AMB-T-1972`) attaches [`env`](Injection::env) to the invocation and merges
//! [`text`](Injection::text) into the stdin document.
//!
//! **A third road, for the values with no storage behind them** ([`asked`], `AMB-D-664`): what an operation
//! on the settings face asks for at the press — a token pasted once — is handed to that one run as an
//! environment variable and written nowhere. It is the mirror image of everything above, which is why it
//! reads nothing: there is no field to look up and no layer to look it up at, only the values the face
//! collected on their way through.

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

/// The environment-variable prefix a one-time asked value is handed over under (`AMB-D-664`). Its own
/// segment, beside [`SECRET_ENV_PREFIX`]'s: a press that asks for `api_token` and a stored secret keyed
/// `api_token` would otherwise be the same variable, and they are opposites — one is kept and one is
/// never written down. The validator keeps the two key sets apart in a single manifest
/// ([`crate::plugin_validate`]); the separate prefix is what keeps them apart in the child's environment
/// even so.
pub const ASK_ENV_PREFIX: &str = "AMENBO_ASK_";

/// The environment-variable name a secret field's value is injected under: [`SECRET_ENV_PREFIX`] followed
/// by the field key upper-cased, with every character that is not an ASCII letter or digit mapped to `_`
/// (so `webhook_url` → `AMENBO_CONFIG_WEBHOOK_URL`). Keys are snake_case identifiers by convention (their
/// shape is the validator's, `AMB-T-1988`); the transform is deterministic so an author can name the
/// variable from the key alone.
pub fn secret_env_name(key: &str) -> String {
    env_name(SECRET_ENV_PREFIX, key)
}

/// The environment-variable name an asked value is handed over under (`AMB-D-664`): [`ASK_ENV_PREFIX`]
/// and the ask's key through the same transform [`secret_env_name`] spells a config key with, so
/// `api_token` → `AMENBO_ASK_API_TOKEN`. An author reads it out of the environment for that one run and
/// finds it nowhere afterwards.
pub fn ask_env_name(key: &str) -> String {
    env_name(ASK_ENV_PREFIX, key)
}

/// A key as an environment variable's stem under `prefix`: upper-cased, with every character that is not
/// an ASCII letter or digit mapped to `_`.
fn env_name(prefix: &str, key: &str) -> String {
    let mut name = String::with_capacity(prefix.len() + key.len());
    name.push_str(prefix);
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

/// What one field is worth to a run, from what the store holds for it (`AMB-D-415`) — the three answers a
/// field can carry, resolved into the one value a plugin receives:
///
/// | state ([`plugin_config::answer`]) | injected |
/// |---|---|
/// | [`Chosen`](plugin_config::Answer::Chosen) | the value itself |
/// | [`NoneOfThem`](plugin_config::Answer::NoneOfThem) | the empty string — an answer, and nothing in it |
/// | [`Unanswered`](plugin_config::Answer::Unanswered) | the author's [`default`](ConfigField::default), or nothing at all |
///
/// The state is read where it is named, so a face saying "none of them" and a run receiving nothing are
/// reading the same thing rather than two spellings of it.
fn resolved(field: &ConfigField, held: Option<String>) -> Option<String> {
    match plugin_config::answer(field, held.as_deref()) {
        plugin_config::Answer::Chosen => held,
        plugin_config::Answer::NoneOfThem => Some(String::new()),
        plugin_config::Answer::Unanswered => field.default.clone(),
    }
}

/// Resolve a plugin's config for one run and split it into env (secret) and stdin-JSON (text) pieces
/// (`AMB-D-356` / `AMB-T-2016`), reading only *this* plugin's stored values.
///
/// `fields` is the plugin's manifest config schema; each field carries the author's `secret` flag, which
/// decides both where the value was stored and how it is injected, and the `default` and candidates
/// [`resolved`] reads. `layer` is where this plugin's values live — the run's project for a `scope: project`
/// plugin, the device for a `scope: machine` one (`AMB-D-601`), and the only rows this run may see either
/// way. A field that resolves to nothing — unanswered, with no default behind it — contributes nothing.
pub fn resolve(
    store: &Store,
    plugin: &str,
    fields: &[ConfigField],
    layer: crate::plugin_layer::Layer,
) -> Result<Injection> {
    let mut injection = Injection::default();
    for field in fields {
        let Some(value) = resolved(field, plugin_config::get(store, field, plugin, layer)?) else {
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

/// The environment variables one press's **asked** values ride on (`AMB-D-664`) — the road beside
/// [`resolve`]'s, for the values that are never stored and so are never read back from anywhere.
///
/// `fields` is what the pressed operation declares it asks for, `supplied` what the face collected at the
/// press. The declaration is what is handed over, not the collection: every declared key becomes a
/// variable ([`ask_env_name`]), and a key the manifest did not ask for is **refused** rather than dropped
/// — the caller is amenbo's own settings face, so a name that reaches here unasked-for is a fault in the
/// face and not something a plugin should be left to notice. A declared key the user left blank is handed
/// over empty: an ask has no `required` (`AMB-D-664`), so an empty box is an answer, and the author reads
/// one variable per declared key either way.
///
/// Everything travels on the environment, `secret: false` included: the flag says what the screen shows,
/// and there is no other road here — stdin is the config document's ([`resolve`]), and argv is the
/// declared call's words.
pub fn asked(
    fields: &[crate::plugin_manifest::AskField],
    supplied: &std::collections::BTreeMap<String, String>,
) -> Result<Vec<(String, String)>> {
    if let Some(stray) = supplied.keys().find(|k| !fields.iter().any(|f| &&f.key == k)) {
        return Err(crate::error::Error::invalid(format!(
            "this call asks for no value named '{stray}' — only what its `ask` declares is handed over"
        )));
    }
    Ok(fields
        .iter()
        .map(|field| {
            let value = supplied.get(&field.key).cloned().unwrap_or_default();
            (ask_env_name(&field.key), value)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_layer::Layer;
    use crate::config::Paths;
    use crate::plugin_manifest::{ConfigField, ConfigOption, FieldType, NONE_SELECTED};

    fn text_field(key: &str) -> ConfigField {
        ConfigField::new(key, key)
    }
    fn secret_field(key: &str) -> ConfigField {
        ConfigField { secret: true, ..ConfigField::new(key, key) }
    }
    /// A field offering two candidates, with the author's `default` behind them (`AMB-D-415`).
    fn multi_field(key: &str, default: Option<&str>) -> ConfigField {
        ConfigField {
            field_type: FieldType::Multi,
            options: vec![
                ConfigOption { value: "task.done".into(), label: "完了した".into() },
                ConfigOption { value: "task.rejected".into(), label: "見送った".into() },
            ],
            default: default.map(str::to_string),
            ..ConfigField::new(key, key)
        }
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

    /// An asked value has its own prefix, so a press asking for `token` and a stored secret keyed `token`
    /// are two variables and not one (`AMB-D-664`).
    #[test]
    fn an_asked_value_is_named_under_its_own_prefix() {
        assert_eq!(ask_env_name("api_token"), "AMENBO_ASK_API_TOKEN");
        assert_ne!(ask_env_name("token"), secret_env_name("token"));
    }

    fn ask(key: &str) -> crate::plugin_manifest::AskField {
        crate::plugin_manifest::AskField {
            key: key.to_string(),
            label: key.to_string(),
            secret: true,
            extra: Default::default(),
        }
    }

    /// Every declared key is handed over — the one that was typed into, and the one that was left blank
    /// (an ask has no `required`, so an empty box is an answer).
    #[test]
    fn every_declared_ask_is_handed_over_and_a_blank_one_goes_over_empty() {
        let supplied = std::collections::BTreeMap::from([("code".to_string(), "1234".to_string())]);
        let env = asked(&[ask("code"), ask("note")], &supplied).unwrap();
        assert_eq!(
            env,
            vec![
                ("AMENBO_ASK_CODE".to_string(), "1234".to_string()),
                ("AMENBO_ASK_NOTE".to_string(), String::new()),
            ]
        );
    }

    /// A value the operation never asked for is refused, not dropped: the face collected it, so a name
    /// nobody declared is that face's fault and worth saying out loud.
    #[test]
    fn a_value_nobody_asked_for_is_refused() {
        let supplied = std::collections::BTreeMap::from([("code".to_string(), "1234".to_string())]);
        let err = asked(&[ask("note")], &supplied).unwrap_err();
        assert!(err.message_en().contains("code"), "{}", err.message_en());
    }

    #[test]
    fn a_secret_rides_env_and_text_rides_stdin() {
        let (mut store, _dir) = store_at("split");
        let p = new_project(&mut store, "proj");
        plugin_config::set(&mut store, &secret_field("webhook_url"), "slack", Layer::Project(p), "https://hooks/x").unwrap();
        plugin_config::set(&mut store, &text_field("events"), "slack", Layer::Project(p), "push,merge").unwrap();

        let fields = [secret_field("webhook_url"), text_field("events")];
        let inj = resolve(&store, "slack", &fields, Layer::Project(p)).unwrap();

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
        plugin_config::set(&mut store, &text_field("events"), "slack", Layer::Project(here), "for-here").unwrap();
        plugin_config::set(&mut store, &text_field("events"), "slack", Layer::Project(elsewhere), "for-elsewhere").unwrap();

        let inj = resolve(&store, "slack", &[text_field("events")], Layer::Project(here)).unwrap();
        assert_eq!(inj.text.get("events"), Some(&Value::String("for-here".to_string())));

        // A project that set nothing is handed nothing — there is no tier under it to fall back to.
        let bare = new_project(&mut store, "bare");
        let inj = resolve(&store, "slack", &[text_field("events")], Layer::Project(bare)).unwrap();
        assert!(inj.text.is_empty());
    }

    #[test]
    fn an_unset_field_contributes_nothing() {
        let (mut store, _dir) = store_at("unset");
        let p = new_project(&mut store, "proj");
        let fields = [secret_field("webhook_url"), text_field("events")];
        let inj = resolve(&store, "slack", &fields, Layer::Project(p)).unwrap();
        assert!(inj.env.is_empty(), "an unset secret sets no env var");
        assert!(inj.text.is_empty(), "an unset text field adds no stdin key");
    }

    /// An unanswered field is handed the author's `default`, on both roads — nothing is stored for it, so
    /// the manifest is where the answer comes from until a user writes one.
    #[test]
    fn an_unanswered_field_is_handed_the_authors_default() {
        let (mut store, _dir) = store_at("default");
        let p = new_project(&mut store, "proj");
        let events = multi_field("events", Some("task.done"));
        let token = ConfigField { default: Some("anonymous".into()), ..secret_field("token") };

        let inj = resolve(&store, "slack", &[events.clone(), token], Layer::Project(p)).unwrap();
        assert_eq!(inj.text.get("events"), Some(&Value::String("task.done".to_string())));
        assert_eq!(inj.env, vec![("AMENBO_CONFIG_TOKEN".to_string(), "anonymous".to_string())]);

        // An answer takes over from the default, and clearing it hands the default back.
        plugin_config::set(&mut store, &events, "slack", Layer::Project(p), "task.rejected").unwrap();
        let inj = resolve(&store, "slack", std::slice::from_ref(&events), Layer::Project(p)).unwrap();
        assert_eq!(inj.text.get("events"), Some(&Value::String("task.rejected".to_string())));

        plugin_config::set(&mut store, &events, "slack", Layer::Project(p), "").unwrap();
        let inj = resolve(&store, "slack", std::slice::from_ref(&events), Layer::Project(p)).unwrap();
        assert_eq!(inj.text.get("events"), Some(&Value::String("task.done".to_string())));
    }

    /// Wanting none of the candidates is an answer of its own, and it reaches the plugin **empty** — the
    /// word the store keeps it under is amenbo's bookkeeping and stops here (`AMB-D-415`). A default behind
    /// it does not come back: it was declined, not left unanswered.
    #[test]
    fn choosing_none_of_them_is_injected_empty() {
        let (mut store, _dir) = store_at("none");
        let p = new_project(&mut store, "proj");
        let events = multi_field("events", Some("task.done"));
        plugin_config::set(&mut store, &events, "slack", Layer::Project(p), NONE_SELECTED).unwrap();

        let inj = resolve(&store, "slack", std::slice::from_ref(&events), Layer::Project(p)).unwrap();
        assert_eq!(
            inj.text.get("events"),
            Some(&Value::String(String::new())),
            "the key is there — the answer is 'none of them', not 'nothing said'",
        );
    }

    /// The word is a choice's alone: a text field holding the line `none` hands over that line.
    #[test]
    fn a_text_field_hands_over_the_reserved_word_as_a_line() {
        let (mut store, _dir) = store_at("word-as-line");
        let p = new_project(&mut store, "proj");
        plugin_config::set(&mut store, &text_field("greeting"), "slack", Layer::Project(p), NONE_SELECTED).unwrap();

        let inj = resolve(&store, "slack", &[text_field("greeting")], Layer::Project(p)).unwrap();
        assert_eq!(inj.text.get("greeting"), Some(&Value::String(NONE_SELECTED.to_string())));
    }

    #[test]
    fn only_this_plugins_config_is_injected() {
        let (mut store, _dir) = store_at("scoped");
        let p = new_project(&mut store, "proj");
        // Another plugin's config must not leak into this one's injection.
        plugin_config::set(&mut store, &secret_field("token"), "github", Layer::Project(p), "gh-secret").unwrap();
        plugin_config::set(&mut store, &text_field("events"), "slack", Layer::Project(p), "push").unwrap();

        let inj = resolve(&store, "slack", &[text_field("events")], Layer::Project(p)).unwrap();
        assert_eq!(inj.text.get("events"), Some(&Value::String("push".to_string())));
        assert!(inj.env.is_empty(), "the other plugin's secret is not injected here");
        assert!(inj.text.get("token").is_none());
    }
}
