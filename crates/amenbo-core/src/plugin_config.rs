//! The **config write boundary** — the single function every face passes a plugin config value through
//! (`AMB-D-356`).
//!
//! A plugin's settings are declared by its author as a flat schema of [`ConfigField`]s
//! ([`crate::plugin_manifest`]); each field carries a `secret` flag. amenbo does **not** judge what is
//! secret — it routes by that flag alone, and both roads lead to a row of **this project's**
//! (`AMB-D-434`), addressed the same way (`(project, plugin, field_key)`):
//!
//! - **secret** ⇒ the store's `plugin_secret` table ([`crate::ops::plugin_secret`]) — carried by
//!   `backup`, and one an `export` must leave. Injected at run time as an environment variable
//!   (`AMB-T-2016`).
//! - **text** ⇒ the store's `plugin_config` table ([`crate::ops::plugin_config`]), an ordinary record.
//!   Injected on stdin as JSON.
//!
//! **There is no tier under either.** A plugin is a project's, so a value is a project's; nothing is left
//! for a machine-wide default to be the default *of*, and a read answers with the row or with nothing.
//!
//! Both the CLI (`plugin config set/get`) and the GUI form submit go through [`set`] / [`get`] so the
//! routing rule, and the safe floor below, live in exactly one place (`AMB-D-356`). The **secret flag
//! is the author's**, read off the plugin's manifest by the caller and handed in as `field` — this
//! function never guesses whether a key is a secret.
//!
//! **The safe floor** (`AMB-D-354`) is amenbo's "unbreakable floor": a per-value byte cap and a
//! control-character reject, whose only purpose is to stop a runaway value from bloating the store —
//! it never judges whether a value is *meaningful* (a valid URL, an email); that is the
//! plugin author's at run time. The fuller manifest-shape validation is `AMB-T-1988`'s; what lives here is
//! only the floor over the *value a user typed*, enforced at this one write boundary.
//!
//! Setting a field to the **empty string clears it** — an empty value is "not provided" (the same reading
//! `required` uses), so it removes the row rather than storing a blank. There is thus one door for both
//! set and unset.

use crate::error::{Error, ErrorCode, Msg, Result};
use crate::plugin_manifest::ConfigField;
use crate::store::Store;

/// The largest a single config value may be, in bytes. Loose — a webhook URL or a short token fits with
/// room to spare — because the cap exists to stop a runaway value bloating the store, not to ration.
/// Applied by [`check_value`] at the write boundary, to secret and text alike.
pub const MAX_CONFIG_VALUE_BYTES: usize = 8 * 1024;

/// The largest a plugin name or field key may be, in bytes. These arrive from the manifest (validated in
/// shape by `AMB-T-1988`), but the write boundary does not trust its caller: a key that would be a storage
/// key is length-capped here too, defense in depth.
pub const MAX_CONFIG_IDENT_BYTES: usize = 128;

/// Enforce the safe floor on a value (`AMB-D-354`): the byte cap and the control-character reject. An empty
/// value is exempt — it is the clear path, checked before this is reached.
pub fn check_value(value: &str) -> Result<()> {
    if value.len() > MAX_CONFIG_VALUE_BYTES {
        return Err(Error::Invalid(
            Msg::new(format!(
                "config value too large ({} bytes; max {})",
                value.len(),
                MAX_CONFIG_VALUE_BYTES
            ))
            .coded(ErrorCode::InvalidPluginConfigValueTooLarge)
            .with("size", value.len())
            .with("max", MAX_CONFIG_VALUE_BYTES),
        ));
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(Error::Invalid(
            Msg::new("config value must not contain control characters")
                .coded(ErrorCode::InvalidPluginConfigValueControlChars),
        ));
    }
    Ok(())
}

/// Enforce the identifier floor on a plugin name or field key: non-empty and within the byte cap.
fn check_ident(kind: &str, s: &str) -> Result<()> {
    if s.is_empty() {
        return Err(Error::invalid(format!("plugin config {kind} must not be empty")));
    }
    if s.len() > MAX_CONFIG_IDENT_BYTES {
        return Err(Error::invalid(
            format!("plugin config {kind} too long ({} bytes; max {})", s.len(), MAX_CONFIG_IDENT_BYTES),
        ));
    }
    Ok(())
}

/// Write one plugin config value through the boundary (`AMB-D-356`). Routes by `field.secret` — a secret
/// to `plugin_secret`, everything else to `plugin_config` — enforcing the safe floor first, and always
/// into `project`'s own row (`AMB-D-434`). An **empty** `value` clears the setting rather than storing a
/// blank — "not provided" is unset. Persists as it goes: either road commits its own transaction.
///
/// `field` carries the author's `secret` flag and the `key` — the caller loads it from the plugin's
/// manifest; this function never decides secrecy for itself.
pub fn set(
    store: &mut Store,
    field: &ConfigField,
    plugin: &str,
    project: i64,
    value: &str,
) -> Result<()> {
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
        store.set_plugin_secret(project, plugin, &field.key, stored)?;
    } else {
        store.set_plugin_config_value(project, plugin, &field.key, stored)?;
    }
    Ok(())
}

/// Read back one plugin config value in `project`, exactly as [`set`] wrote it (`AMB-D-434`) — the same
/// two roads, taken by the same flag. Returns `None` when the setting is unset there.
///
/// The value is returned raw; a secret is not masked here — the CLI face masks it on the way to the
/// terminal (`plugin config get` never echoes a secret), while injection reads it whole.
pub fn get(
    store: &Store,
    field: &ConfigField,
    plugin: &str,
    project: i64,
) -> Result<Option<String>> {
    if field.secret {
        store.plugin_secret_value(project, plugin, &field.key)
    } else {
        store.plugin_config_value(project, plugin, &field.key)
    }
}

/// Which of the author's declared `fields` this project currently holds a value for (`AMB-D-434`). This is
/// the `has_value` probe [`plugin_trust::enable`](crate::plugin_trust::enable) asks its caller for — that
/// boundary judges `required` and deliberately does not read storage, so the resolution lives here with
/// the rest of the value routing, and both faces (CLI `plugin enable`, the GUI's) run the same one.
///
/// Probed into a list first because the probe reads the store while the enable writes inside it, so the
/// two cannot borrow it at once.
pub fn satisfied_keys(
    store: &Store,
    plugin: &str,
    fields: &[ConfigField],
    project: i64,
) -> Result<Vec<String>> {
    let mut satisfied = Vec::new();
    for field in fields {
        if get(store, field, plugin, project)?.is_some() {
            satisfied.push(field.key.clone());
        }
    }
    Ok(satisfied)
}

/// The `required` settings a **candidate** build would leave unset, judged at **every** gate the plugin is
/// enabled at right now (`AMB-D-359`) — the resolution behind an update's `approve` gate, so a face only has
/// to word the refusal it already knows how to word.
///
/// [`plugin_update`](crate::plugin_update) owns the timing (after "is it a different build", before the
/// network) and deliberately leaves the values and the enabled state to its caller; this is that caller's
/// half, shared so the CLI's `plugin update` and the GUI's cannot drift on *which* build is held back — only
/// on how they say so. Empty means nothing is in the way.
///
/// **Where the caller happens to be standing is not part of the judgement.** A plugin has one gate per
/// project (`AMB-D-434`), and an update replaces the build for all of them at once, so asking only about
/// the project on screen would let a schema through that leaves some *other* project's enabled plugin
/// without a value its author marked `required` — and the GUI can be asked from screens that are in no
/// project at all. A field counts as held only when every enabled gate holds it; the keys come back in the
/// author's declared order, once each, whichever gates want them.
///
/// It answers empty, letting the update through, in every case where there is nothing to break: a build this
/// amenbo cannot run anyway (the write path refuses it with its own wording), a plugin that is not installed,
/// and — the common case — a plugin enabled nowhere, which fires nothing and whose own enable gate will catch
/// an empty `required` when it is next turned on.
pub fn required_unset_for_update(
    store: &Store,
    available: &crate::plugin_manifest::Manifest,
) -> Result<Vec<String>> {
    let name = available.name.as_str();
    if crate::plugin_compat::check(available).is_err() {
        return Ok(Vec::new());
    }
    // A plugin that is not installed has no gate to re-judge; the new schema is what we judge the ones it
    // does have against.
    if crate::plugin_installed::read(&store.paths, name).is_err() {
        return Ok(Vec::new());
    }
    // One satisfied set per project that actually fires: each project holds its own values, so two of
    // them can hold different halves of the same schema.
    let mut per_gate = Vec::new();
    for project in store.projects_with_plugin_enabled(name)? {
        if !crate::plugin_trust::effective_enabled_in(store, name, project)? {
            continue;
        }
        per_gate.push(satisfied_keys(store, name, &available.config, project)?);
    }
    if per_gate.is_empty() {
        return Ok(Vec::new());
    }
    Ok(crate::plugin_trust::missing_required(&available.config, |f| {
        per_gate.iter().all(|satisfied| satisfied.iter().any(|k| k == &f.key))
    })
    .into_iter()
    .map(str::to_string)
    .collect())
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

    /// Open a real store under a scratch AMENBO_HOME so the store file resolves under it.
    fn store_at(tag: &str) -> (Store, std::path::PathBuf) {
        let dir = amenbo_scratch::scratch(&format!("plugin-config-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open_at(Paths::at(dir.clone())).unwrap();
        (store, dir)
    }

    #[test]
    fn a_text_value_lands_in_the_projects_row_and_reads_back() {
        let (mut store, _dir) = store_at("text");
        let p = mk_project(&mut store, "proj");
        set(&mut store, &text_field("events"), "slack", p, "push,merge").unwrap();

        assert_eq!(
            get(&store, &text_field("events"), "slack", p).unwrap().as_deref(),
            Some("push,merge"),
        );
        assert_eq!(
            store.plugin_config_value(p, "slack", "events").unwrap().as_deref(),
            Some("push,merge"),
            "the text road is the `plugin_config` table",
        );
    }

    /// The routing rule, from the author's flag alone: a secret goes to the table of its own, and nothing
    /// of it appears on the text road.
    #[test]
    fn a_secret_lands_in_the_secret_table_and_not_the_text_one() {
        let (mut store, _dir) = store_at("secret");
        let p = mk_project(&mut store, "proj");
        set(&mut store, &secret_field("webhook_url"), "slack", p, "https://hooks/x").unwrap();

        assert_eq!(
            get(&store, &secret_field("webhook_url"), "slack", p).unwrap().as_deref(),
            Some("https://hooks/x"),
        );
        assert_eq!(
            store.plugin_secret_value(p, "slack", "webhook_url").unwrap().as_deref(),
            Some("https://hooks/x"),
        );
        assert_eq!(
            store.plugin_config_value(p, "slack", "webhook_url").unwrap(),
            None,
            "a secret never reaches the text table",
        );
    }

    #[test]
    fn empty_value_clears_at_each_kind() {
        let (mut store, _dir) = store_at("clear");
        let p = mk_project(&mut store, "proj");
        // text
        set(&mut store, &text_field("k"), "plug", p, "v").unwrap();
        set(&mut store, &text_field("k"), "plug", p, "").unwrap();
        assert_eq!(get(&store, &text_field("k"), "plug", p).unwrap(), None);
        // secret
        set(&mut store, &secret_field("s"), "plug", p, "v").unwrap();
        set(&mut store, &secret_field("s"), "plug", p, "").unwrap();
        assert_eq!(get(&store, &secret_field("s"), "plug", p).unwrap(), None);
    }

    /// A value is one project's and reaches no other — there is no tier under it that both would see
    /// (`AMB-D-434`).
    #[test]
    fn a_value_is_one_projects_own() {
        let (mut store, _dir) = store_at("project");
        let (a, b) = (mk_project(&mut store, "a"), mk_project(&mut store, "b"));

        set(&mut store, &text_field("events"), "slack", a, "for-a").unwrap();

        assert_eq!(get(&store, &text_field("events"), "slack", a).unwrap().as_deref(), Some("for-a"));
        assert_eq!(get(&store, &text_field("events"), "slack", b).unwrap(), None);
    }

    /// An install on disk, with the manifest an update would be judged against.
    fn install_plugin(
        paths: &Paths,
        name: &str,
        config: Vec<ConfigField>,
    ) -> crate::plugin_manifest::Manifest {
        let manifest = crate::plugin_manifest::Manifest {
            name: name.to_string(),
            desc: "a test plugin".to_string(),
            author: "amenbo".to_string(),
            repo: "ShiroDoromoto/amenbo".to_string(),
            os: vec![crate::plugin_manifest::Os::Macos, crate::plugin_manifest::Os::Linux, crate::plugin_manifest::Os::Windows],
            category: "workflow".to_string(),
            url: "https://example.invalid/x.tar.gz".to_string(),
            checksum: "sha256:00".to_string(),
            signature: None,
            assets: Default::default(),
            official: false,
            detail_sum: None,
            payload_v: 1,
            min_amenbo: None,
            config,
            events: Vec::new(),
        };
        let home = paths.plugin_dir(name);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(crate::plugin_installed::program_file_name(name)), b"#!/bin/sh\n").unwrap();
        std::fs::write(
            home.join(crate::plugin_installed::MANIFEST_FILE_NAME),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
        manifest
    }

    fn required_field(key: &str) -> ConfigField {
        ConfigField { key: key.to_string(), label: key.to_string(), secret: false, required: true }
    }

    /// A project to enable a plugin in.
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

    /// The gate an update runs (`AMB-D-359`): a schema that grew a `required` field the project has no
    /// value for holds the new build back — an *enabled* plugin must not be left missing what its author
    /// requires.
    #[test]
    fn a_new_required_setting_with_no_value_holds_an_enabled_plugin_back() {
        let (mut store, _dir) = store_at("update-required");
        let p = mk_project(&mut store, "p");
        install_plugin(&store.paths.clone(), "slack", Vec::new());
        crate::plugin_trust::enable(&mut store, "slack", p, &[], |_| true).unwrap();
        let available =
            install_plugin(&store.paths.clone(), "slack", vec![required_field("webhook_url")]);

        assert_eq!(
            required_unset_for_update(&store, &available).unwrap(),
            vec!["webhook_url".to_string()],
        );

        // Set it, and the same build goes through: presence is all this judges (`AMB-D-356`).
        set(&mut store, &required_field("webhook_url"), "slack", p, "https://hooks/x").unwrap();
        assert!(required_unset_for_update(&store, &available).unwrap().is_empty());
    }

    /// A **disabled** plugin fires nothing, so there is nothing to keep working: its own enable gate is
    /// where the empty `required` is caught, the next time anyone turns it on.
    #[test]
    fn a_disabled_plugin_is_never_held_back() {
        let (store, _dir) = store_at("update-disabled");
        let available =
            install_plugin(&store.paths.clone(), "slack", vec![required_field("webhook_url")]);

        assert!(required_unset_for_update(&store, &available).unwrap().is_empty());
    }

    /// Nothing installed under that name is not this gate's business — `plugin update` refuses it upstream,
    /// and answering "held back" here would name the wrong reason.
    #[test]
    fn a_plugin_that_is_not_installed_is_not_held_back() {
        let (store, _dir) = store_at("update-absent");
        let elsewhere = amenbo_scratch::scratch("plugin-config-update-absent-src");
        std::fs::create_dir_all(&elsewhere).unwrap();
        let available =
            install_plugin(&Paths::at(elsewhere), "slack", vec![required_field("webhook_url")]);

        assert!(required_unset_for_update(&store, &available).unwrap().is_empty());
    }

    /// The whole point of judging every gate (`AMB-D-434`): a plugin is enabled in two projects, only one
    /// of which set the new `required` field. The update is held back — nobody had to be standing in the
    /// project that is short of a value for it to count.
    #[test]
    fn a_project_that_is_short_of_a_value_holds_the_build_back_from_anywhere() {
        let (mut store, _dir) = store_at("update-across-projects");
        let (set_up, short) = (mk_project(&mut store, "set-up"), mk_project(&mut store, "short"));

        install_plugin(&store.paths.clone(), "slack", Vec::new());
        for id in [set_up, short] {
            crate::plugin_trust::enable(&mut store, "slack", id, &[], |_| true).unwrap();
        }
        let available =
            install_plugin(&store.paths.clone(), "slack", vec![required_field("webhook_url")]);
        set(&mut store, &required_field("webhook_url"), "slack", set_up, "https://hooks/x").unwrap();

        assert_eq!(
            required_unset_for_update(&store, &available).unwrap(),
            vec!["webhook_url".to_string()],
            "the project still short of the value is judged too",
        );

        // The last gate that wanted it is satisfied, so the same build goes through.
        set(&mut store, &required_field("webhook_url"), "slack", short, "https://hooks/y").unwrap();
        assert!(required_unset_for_update(&store, &available).unwrap().is_empty());
    }

    #[test]
    fn the_floor_rejects_an_oversize_or_control_char_value() {
        let (mut store, _dir) = store_at("floor");
        let p = mk_project(&mut store, "proj");
        let big = "x".repeat(MAX_CONFIG_VALUE_BYTES + 1);
        assert!(set(&mut store, &text_field("k"), "plug", p, &big).is_err());
        assert!(set(&mut store, &text_field("k"), "plug", p, "a\u{0}b").is_err());
        // Nothing landed from the rejected writes.
        assert_eq!(get(&store, &text_field("k"), "plug", p).unwrap(), None);
    }
}
