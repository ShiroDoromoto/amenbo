//! The **config write boundary** — the single function every face passes a plugin config value through
//! (`AMB-D-356`).
//!
//! A plugin's settings are declared by its author as a flat schema of [`ConfigField`]s
//! ([`crate::plugin_manifest`]); each field carries a `secret` flag. Amenbo does **not** judge what is
//! secret — it routes by that flag alone, and both roads lead to a row of **this project's**
//! (`AMB-D-434`), addressed the same way (`(project, plugin, field_key)`):
//!
//! - **secret** ⇒ the store's `plugin_secret` table ([`crate::ops::plugin_secret`]) — carried by
//!   `backup`, and one an `export` must leave. Injected at run time as an environment variable
//!   (`AMB-T-2016`).
//! - **text** ⇒ the store's `plugin_config` table ([`crate::ops::plugin_config`]), an ordinary record.
//!   Injected on stdin as JSON.
//!
//! **There is no tier under either.** A plugin lives at one layer and its author picks which
//! ([`Layer`], `AMB-D-601`), so a value is that layer's; nothing is left for a machine-wide default to be
//! the default *of*, and a read answers with the row or with nothing.
//!
//! Both the CLI (`plugin config set/get`) and the GUI form submit go through [`set`] / [`get`] so the
//! routing rule, and the safe floor below, live in exactly one place (`AMB-D-356`). The **secret flag
//! is the author's**, read off the plugin's manifest by the caller and handed in as `field` — this
//! function never guesses whether a key is a secret.
//!
//! **The safe floor** (`AMB-D-354`) is Amenbo's "unbreakable floor": a per-value byte cap and a
//! control-character reject, whose only purpose is to stop a runaway value from bloating the store —
//! it never judges whether a value is *meaningful* (a valid URL, an email); that is the
//! plugin author's at run time. The fuller manifest-shape validation is `AMB-T-1988`'s; what lives here is
//! only the floor over the *value a user typed*, enforced at this one write boundary.
//!
//! Setting a field to the **empty string clears it** — an empty value is "not provided" (the same reading
//! `required` uses), so it removes the row rather than storing a blank. There is thus one door for both
//! set and unset.
//!
//! **A save is never judged by the author's check** (`AMB-D-664`). A plugin may name a call that says
//! whether its values are usable ([`crate::plugin_check`]), and it is raised again after a save while the
//! plugin is enabled — but by the face that has somewhere to show the answer, and never here: nothing this
//! boundary writes is held back by it, and an enabled plugin is not switched off behind the user. What a
//! failing check costs is one thing only, and it is spent at the gate ([`crate::plugin_trust::enable`]).
//!
//! **A field that offers candidates has three answers, and all three are stored here** (`AMB-D-415`): the
//! chosen [`value`](crate::plugin_manifest::ConfigOption::value)s joined by commas, the reserved
//! [`NONE_SELECTED`] for "none of them", and — by the clear path above — no row at all for "not answered
//! yet", which is the state the author's [`default`](ConfigField::default) speaks for. The word is resolved
//! away on the far side ([`crate::plugin_inject`]), so what a plugin reads is an answer and never a
//! spelling Amenbo picked.

use crate::error::{Error, ErrorCode, Msg, Result};
use crate::plugin_layer::Layer;
use crate::plugin_manifest::{ConfigField, FieldType, NONE_SELECTED};
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

/// Enforce what a [`Multi`](FieldType::Multi) field accepts as an answer (`AMB-D-415`): the candidates its
/// author declared, joined by commas and each named once, or [`NONE_SELECTED`] on its own. A text field
/// takes any line, so it passes untouched — this is not Amenbo learning what a value *means* (`AMB-D-354`
/// leaves that to the plugin); it is the one thing Amenbo does know, because the author told it the whole
/// list.
///
/// It sits at the boundary rather than in a form so that both faces refuse alike: a checkbox group cannot
/// produce a candidate that is not on it, but `plugin config set` can, and a misspelt one stored quietly
/// would simply never match anything at run time.
fn check_choice(field: &ConfigField, value: &str) -> Result<()> {
    if field.field_type != FieldType::Multi || value == NONE_SELECTED {
        return Ok(());
    }
    let mut chosen: Vec<&str> = Vec::new();
    for one in value.split(',') {
        if !field.options.iter().any(|o| o.value == one) {
            return Err(Error::invalid(format!(
                "'{one}' is not one of the choices '{}' offers",
                field.key
            )));
        }
        if chosen.contains(&one) {
            return Err(Error::invalid(format!("'{one}' is chosen twice for '{}'", field.key)));
        }
        chosen.push(one);
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
/// into `layer`'s own row (`AMB-D-434` / `AMB-D-601`). An **empty** `value` clears the setting rather than
/// storing a blank — "not provided" is unset. Persists as it goes: either road commits its own transaction.
///
/// `field` carries the author's `secret` flag and the `key` — the caller loads it from the plugin's
/// manifest; this function never decides secrecy for itself. It carries the candidates too, which is what
/// lets a field that offers a choice refuse an answer outside it ([`check_choice`], `AMB-D-415`). The layer
/// comes from the same manifest, through [`Layer::of`] — the two roads below are the author's `secret`
/// flag's call, and *which* row either writes is the author's `scope`'s.
pub fn set(
    store: &mut Store,
    field: &ConfigField,
    plugin: &str,
    layer: Layer,
    value: &str,
) -> Result<()> {
    check_ident("plugin name", plugin)?;
    check_ident("key", &field.key)?;
    // A value is stored verbatim (no trimming): whitespace can be significant. Empty is the clear path.
    let stored: Option<&str> = if value.is_empty() {
        None
    } else {
        check_value(value)?;
        check_choice(field, value)?;
        Some(value)
    };

    if field.secret {
        store.set_plugin_secret(layer.project_id(), plugin, &field.key, stored)?;
    } else {
        store.set_plugin_config_value(layer.project_id(), plugin, &field.key, stored)?;
    }
    Ok(())
}

/// What a [`purge_undeclared`] took, counted per road (`AMB-D-456`) — the two are different kinds of
/// residue, so a receipt that added them up would hide which one a reader cares about.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Purged {
    /// How many `plugin_config` rows went, across every project.
    pub settings: usize,
    /// How many `plugin_secret` rows went, across every project.
    pub secrets: usize,
}

impl Purged {
    /// Whether anything was found to purge — `false` is the ordinary update, where the new build declares
    /// everything the old one did and no stored value is left without a declaration.
    pub fn anything(&self) -> bool {
        self.settings > 0 || self.secrets > 0
    }
}

/// Erase what this plugin's declaration no longer asks for (`AMB-D-456`): every project's stored value
/// under a key `fields` does not name — the step an update runs once the new build and its manifest are in
/// place.
///
/// **A value does not outlive the declaration that asked for it.** Nothing reads one that is no longer
/// declared — injection ([`crate::plugin_inject`]) is handed the schema, and both faces draw their forms
/// from it — so a row left behind lives on only in `backup` and `export`, which is where its owner is
/// least likely to find it.
///
/// Routed by the same flag [`set`] writes with, which is what decides the key sets: the text road keeps
/// the keys declared **not** secret, the secret road the keys declared secret. So a key that stayed but
/// stopped being a secret leaves the secret table — the new declaration is not asking for a secret under
/// that name, and its old row would otherwise sit unread in the table an `export` must leave.
///
/// Keyed on the declaration in hand rather than on a diff against the old one, so it is idempotent and
/// says the same thing whatever left the manifest, or was never in it.
pub fn purge_undeclared(store: &mut Store, plugin: &str, fields: &[ConfigField]) -> Result<Purged> {
    let text: Vec<&str> =
        fields.iter().filter(|f| !f.secret).map(|f| f.key.as_str()).collect();
    let secret: Vec<&str> = fields.iter().filter(|f| f.secret).map(|f| f.key.as_str()).collect();
    Ok(Purged {
        settings: store.purge_undeclared_plugin_config(plugin, &text)?,
        secrets: store.purge_undeclared_plugin_secrets(plugin, &secret)?,
    })
}

/// Which of the three answers a field holds right now (`AMB-D-415`) — the reading a face shows, named once
/// here so the CLI and the GUI cannot each invent their own words for the same state.
///
/// Read off what storage holds, which is why it is a function of the field and the raw value rather than a
/// flag anyone writes: the store keeps answers, never states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// Nothing is stored — nobody has answered, so the author's [`default`](ConfigField::default) is what
    /// the run receives, if the author wrote one.
    Unanswered,
    /// A value is stored: the line a text field was given, or the candidates a choice picked.
    Chosen,
    /// A choice was answered with *none of them* ([`NONE_SELECTED`]) — an answer, and not the same as
    /// having none.
    NoneOfThem,
}

impl Answer {
    /// The stable word a face uses for this state — `--json` prints it, and a human line is worded from it.
    pub fn as_str(&self) -> &'static str {
        match self {
            Answer::Unanswered => "unanswered",
            Answer::Chosen => "chosen",
            Answer::NoneOfThem => "none",
        }
    }
}

/// Read the state of one field from what the store holds for it (`AMB-D-415`) — `held` is [`get`]'s answer.
pub fn answer(field: &ConfigField, held: Option<&str>) -> Answer {
    match held {
        None => Answer::Unanswered,
        Some(v) if field.field_type == FieldType::Multi && v == NONE_SELECTED => Answer::NoneOfThem,
        Some(_) => Answer::Chosen,
    }
}

/// Read back one plugin config value at `layer`, exactly as [`set`] wrote it (`AMB-D-434` / `AMB-D-601`) —
/// the same two roads, taken by the same flag. Returns `None` when the setting is unset there.
///
/// The value is returned raw; a secret is not masked here — the CLI face masks it on the way to the
/// terminal (`plugin config get` never echoes a secret), while injection reads it whole.
pub fn get(
    store: &Store,
    field: &ConfigField,
    plugin: &str,
    layer: Layer,
) -> Result<Option<String>> {
    if field.secret {
        store.plugin_secret_value(layer.project_id(), plugin, &field.key)
    } else {
        store.plugin_config_value(layer.project_id(), plugin, &field.key)
    }
}

/// Which of the author's declared `fields` this layer currently holds a value for (`AMB-D-434`). This is
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
    layer: Layer,
) -> Result<Vec<String>> {
    let mut satisfied = Vec::new();
    for field in fields {
        if get(store, field, plugin, layer)?.is_some() {
            satisfied.push(field.key.clone());
        }
    }
    Ok(satisfied)
}

/// One "project × plugin" intersection, as the faces draw a row for it (`AMB-D-447`) — the whole state of
/// that one crossing, so a row is drawn from a single answer rather than from one read per project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Intersection {
    /// The project this row is for.
    pub project: i64,
    /// Whether the plugin fires in it — that project's gate, and nothing else (`AMB-D-434`).
    pub enabled: bool,
    /// Whether it holds a value for any setting the author declares. Off with values is an ordinary state
    /// (`AMB-D-434`), which is why it is a fact of its own and not a reading of the gate.
    pub has_value: bool,
    /// Whether a `required` setting is empty here — an enable would be refused at this crossing
    /// (`AMB-D-351`), which is the one thing a face wants to say *before* the switch is pressed.
    pub required_unset: bool,
}

/// What one layer holds for a plugin's declared settings — the two readings a row draws beside its gate.
///
/// The gate is not in here on purpose: a list of crossings reads every gate in one go, while a single
/// layer's is one read, so folding them together would cost a read per project to say something already
/// known. What is folded together is the pair that has to mean the same thing on every face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Held {
    /// Whether it holds a value for any setting the author declares. Off with values is an ordinary state
    /// (`AMB-D-434`), which is why it is a fact of its own and not a reading of the gate.
    pub has_value: bool,
    /// Whether a `required` setting is empty here — an enable at this layer would be refused
    /// (`AMB-D-351`), which is the one thing a face wants to say *before* the switch is pressed.
    pub required_unset: bool,
}

/// Read one layer's settings whole — one project's, or the device's (`AMB-D-601`).
///
/// [`intersections`] is this made once per project that has a row at all. The device layer has exactly one
/// row and no list to walk, so a face drawing it asks here directly rather than deciding for itself what
/// "has a value" and "would be refused" mean — which is the drift this being one function prevents.
pub fn held_at(
    store: &Store,
    plugin: &str,
    fields: &[ConfigField],
    layer: Layer,
) -> Result<Held> {
    Ok(held_from(fields, &satisfied_keys(store, plugin, fields, layer)?))
}

/// The same two readings off a satisfied set already in hand — what [`intersections`] has after its own
/// probe, and what keeps it from paying for a second one.
fn held_from(fields: &[ConfigField], satisfied: &[String]) -> Held {
    Held {
        has_value: !satisfied.is_empty(),
        required_unset: !crate::plugin_trust::missing_required(fields, |f| {
            satisfied.iter().any(|k| k == &f.key)
        })
        .is_empty(),
    }
}

/// Every intersection this plugin has a row at (`AMB-D-447`): the projects it fires in, and the projects
/// that hold a value for it while it is off. Ordered by project id, so two reads draw the rows in the same
/// order.
///
/// It is the read behind a face that lists crossings rather than projects. The alternative — asking
/// [`get`] per project, per field — is the same walk done once per row by the caller, and leaves each face
/// to decide for itself what "has a value" and "would be refused" mean; both readings are made here, off
/// the author's `fields`, so the two faces cannot drift.
///
/// A project whose only rows are for keys the author no longer declares is not one of these: there is
/// nothing on the schema to draw for it, and a row that offered nothing to fill in would be a project the
/// user cannot get rid of.
///
/// **Project crossings only.** A `scope: machine` plugin's one gate and its settings are the device's, and
/// no project crosses it (`AMB-D-601`), so this answers empty for one — rightly, since there is no project
/// row to draw. The device's own row is read by [`held_at`] beside the gate, and drawing it in place of a
/// project list is the faces' work.
pub fn intersections(
    store: &Store,
    plugin: &str,
    fields: &[ConfigField],
) -> Result<Vec<Intersection>> {
    let enabled: Vec<i64> =
        store.layers_with_plugin_enabled(plugin)?.into_iter().filter_map(Layer::project_id).collect();
    let mut projects = enabled.clone();
    projects.extend(store.projects_with_plugin_values(plugin)?);
    projects.sort_unstable();
    projects.dedup();

    let mut rows = Vec::new();
    for project in projects {
        let satisfied = satisfied_keys(store, plugin, fields, Layer::Project(project))?;
        let on = enabled.contains(&project);
        if !on && satisfied.is_empty() {
            continue;
        }
        let held = held_from(fields, &satisfied);
        rows.push(Intersection {
            project,
            enabled: on,
            has_value: held.has_value,
            required_unset: held.required_unset,
        });
    }
    Ok(rows)
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
/// layer it can be enabled at (`AMB-D-434` / `AMB-D-601`), and an update replaces the build for all of them
/// at once, so asking only about the project on screen would let a schema through that leaves some *other*
/// gate's enabled plugin without a value its author marked `required` — and the GUI can be asked from
/// screens that are in no project at all. The device gate is judged with the project ones for the same
/// reason: it is a gate the plugin fires through, and a list of projects would have left it out. A field
/// counts as held only when every enabled gate holds it; the keys come back in the author's declared order,
/// once each, whichever gates want them.
///
/// It answers empty, letting the update through, in every case where there is nothing to break: a build this
/// Amenbo cannot run anyway (the write path refuses it with its own wording), a plugin that is not installed,
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
    // One satisfied set per gate that actually fires: each layer holds its own values, so two of
    // them can hold different halves of the same schema.
    let declared = available.fields();
    let mut per_gate = Vec::new();
    for layer in store.layers_with_plugin_enabled(name)? {
        if !crate::plugin_trust::effective_enabled_in(store, name, layer)? {
            continue;
        }
        per_gate.push(satisfied_keys(store, name, &declared, layer)?);
    }
    if per_gate.is_empty() {
        return Ok(Vec::new());
    }
    Ok(crate::plugin_trust::missing_required(&declared, |f| {
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
    use crate::plugin_check::Checked;
    use crate::plugin_manifest::{ConfigField, ConfigOption};

    fn text_field(key: &str) -> ConfigField {
        ConfigField::new(key, key)
    }
    fn secret_field(key: &str) -> ConfigField {
        ConfigField { secret: true, ..ConfigField::new(key, key) }
    }
    /// A field offering two candidates — the shape whose answers this boundary judges (`AMB-D-415`).
    fn multi_field(key: &str) -> ConfigField {
        ConfigField {
            field_type: FieldType::Multi,
            options: vec![
                ConfigOption { value: "task.done".into(), label: "完了した".into() },
                ConfigOption { value: "task.rejected".into(), label: "見送った".into() },
            ],
            ..ConfigField::new(key, key)
        }
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
        set(&mut store, &text_field("events"), "slack", Layer::Project(p), "push,merge").unwrap();

        assert_eq!(
            get(&store, &text_field("events"), "slack", Layer::Project(p)).unwrap().as_deref(),
            Some("push,merge"),
        );
        assert_eq!(
            store.plugin_config_value(Some(p), "slack", "events").unwrap().as_deref(),
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
        set(&mut store, &secret_field("webhook_url"), "slack", Layer::Project(p), "https://hooks/x").unwrap();

        assert_eq!(
            get(&store, &secret_field("webhook_url"), "slack", Layer::Project(p)).unwrap().as_deref(),
            Some("https://hooks/x"),
        );
        assert_eq!(
            store.plugin_secret_value(Some(p), "slack", "webhook_url").unwrap().as_deref(),
            Some("https://hooks/x"),
        );
        assert_eq!(
            store.plugin_config_value(Some(p), "slack", "webhook_url").unwrap(),
            None,
            "a secret never reaches the text table",
        );
    }

    #[test]
    fn empty_value_clears_at_each_kind() {
        let (mut store, _dir) = store_at("clear");
        let p = mk_project(&mut store, "proj");
        // text
        set(&mut store, &text_field("k"), "plug", Layer::Project(p), "v").unwrap();
        set(&mut store, &text_field("k"), "plug", Layer::Project(p), "").unwrap();
        assert_eq!(get(&store, &text_field("k"), "plug", Layer::Project(p)).unwrap(), None);
        // secret
        set(&mut store, &secret_field("s"), "plug", Layer::Project(p), "v").unwrap();
        set(&mut store, &secret_field("s"), "plug", Layer::Project(p), "").unwrap();
        assert_eq!(get(&store, &secret_field("s"), "plug", Layer::Project(p)).unwrap(), None);
    }

    /// A value is one project's and reaches no other — there is no tier under it that both would see
    /// (`AMB-D-434`).
    #[test]
    fn a_value_is_one_projects_own() {
        let (mut store, _dir) = store_at("project");
        let (a, b) = (mk_project(&mut store, "a"), mk_project(&mut store, "b"));

        set(&mut store, &text_field("events"), "slack", Layer::Project(a), "for-a").unwrap();

        assert_eq!(get(&store, &text_field("events"), "slack", Layer::Project(a)).unwrap().as_deref(), Some("for-a"));
        assert_eq!(get(&store, &text_field("events"), "slack", Layer::Project(b)).unwrap(), None);
    }

    /// The three answers a choice can carry, each stored as itself (`AMB-D-415`): the chosen candidates
    /// joined by commas, the reserved word for wanting none of them, and — down the clear path every field
    /// shares — no row at all, which is the unanswered state the author's `default` speaks for.
    #[test]
    fn a_choice_keeps_its_three_answers_apart() {
        let (mut store, _dir) = store_at("choice");
        let p = mk_project(&mut store, "proj");
        let events = multi_field("events");

        set(&mut store, &events, "slack", Layer::Project(p), "task.done,task.rejected").unwrap();
        assert_eq!(
            get(&store, &events, "slack", Layer::Project(p)).unwrap().as_deref(),
            Some("task.done,task.rejected"),
        );

        set(&mut store, &events, "slack", Layer::Project(p), NONE_SELECTED).unwrap();
        assert_eq!(
            get(&store, &events, "slack", Layer::Project(p)).unwrap().as_deref(),
            Some(NONE_SELECTED),
            "wanting none of them is an answer, and it is stored",
        );

        set(&mut store, &events, "slack", Layer::Project(p), "").unwrap();
        assert_eq!(get(&store, &events, "slack", Layer::Project(p)).unwrap(), None, "empty is still the clear path");
    }

    /// An answer a choice does not offer never lands — the whole point of the author declaring candidates
    /// is that a value outside them could only ever be a mistake.
    #[test]
    fn an_answer_outside_the_choice_is_refused() {
        let (mut store, _dir) = store_at("not-a-choice");
        let p = mk_project(&mut store, "proj");
        let events = multi_field("events");

        for bad in [
            "task.created",                // never declared
            "Task.Done",                   // declared, spelt otherwise
            "task.done,task.created",      // one of each
            "task.done,",                  // a trailing comma is an empty candidate
            "task.done,task.done",         // the same one twice
            "task.done,none",              // the reserved word is not a candidate to mix in
        ] {
            assert!(set(&mut store, &events, "slack", Layer::Project(p), bad).is_err(), "'{bad}' must not be stored");
        }
        assert_eq!(get(&store, &events, "slack", Layer::Project(p)).unwrap(), None, "nothing landed from the refusals");
    }

    /// The state each answer reads as (`AMB-D-415`) — one naming, for every face that has to say which of
    /// the three a field is in.
    #[test]
    fn each_answer_reads_as_its_own_state() {
        let events = multi_field("events");
        assert_eq!(answer(&events, None), Answer::Unanswered);
        assert_eq!(answer(&events, Some("task.done")), Answer::Chosen);
        assert_eq!(answer(&events, Some(NONE_SELECTED)), Answer::NoneOfThem);
        // A default changes what an unanswered field is *worth*, never that nobody answered it.
        let with_default = ConfigField { default: Some("task.done".into()), ..multi_field("events") };
        assert_eq!(answer(&with_default, None), Answer::Unanswered);
        // The word is a choice's: on a text field it is a line like any other.
        assert_eq!(answer(&text_field("greeting"), Some(NONE_SELECTED)), Answer::Chosen);
    }

    /// The reserved word belongs to a choice, not to every field: a text field takes `none` as the line it
    /// is, because there is no "unticked every box" state there for it to stand for.
    #[test]
    fn a_text_field_takes_the_reserved_word_as_a_line() {
        let (mut store, _dir) = store_at("word-as-line");
        let p = mk_project(&mut store, "proj");
        set(&mut store, &text_field("greeting"), "plug", Layer::Project(p), NONE_SELECTED).unwrap();
        assert_eq!(
            get(&store, &text_field("greeting"), "plug", Layer::Project(p)).unwrap().as_deref(),
            Some(NONE_SELECTED),
        );
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
            about: None,
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
            scope: crate::plugin_manifest::Scope::Project,
            payload_v: 1,
            min_amenbo: None,
            config: crate::plugin_manifest::ConfigEntry::schema(config),
            events: Vec::new(),
            agent: None,
            settings: None,
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
        ConfigField { required: true, ..ConfigField::new(key, key) }
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
        crate::plugin_trust::enable(&mut store, "slack", Layer::Project(p), &[], |_| true, &Checked::NotDeclared)
            .unwrap();
        let available =
            install_plugin(&store.paths.clone(), "slack", vec![required_field("webhook_url")]);

        assert_eq!(
            required_unset_for_update(&store, &available).unwrap(),
            vec!["webhook_url".to_string()],
        );

        // Set it, and the same build goes through: presence is all this judges (`AMB-D-356`).
        set(&mut store, &required_field("webhook_url"), "slack", Layer::Project(p), "https://hooks/x").unwrap();
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
            crate::plugin_trust::enable(&mut store, "slack", Layer::Project(id), &[], |_| true, &Checked::NotDeclared)
                .unwrap();
        }
        let available =
            install_plugin(&store.paths.clone(), "slack", vec![required_field("webhook_url")]);
        set(&mut store, &required_field("webhook_url"), "slack", Layer::Project(set_up), "https://hooks/x").unwrap();

        assert_eq!(
            required_unset_for_update(&store, &available).unwrap(),
            vec!["webhook_url".to_string()],
            "the project still short of the value is judged too",
        );

        // The last gate that wanted it is satisfied, so the same build goes through.
        set(&mut store, &required_field("webhook_url"), "slack", Layer::Project(short), "https://hooks/y").unwrap();
        assert!(required_unset_for_update(&store, &available).unwrap().is_empty());
    }

    /// The rows a face draws (`AMB-D-447`): every project the plugin fires in, and every project holding a
    /// value while it is off — a project with neither is not a crossing anyone is looking at.
    #[test]
    fn the_rows_are_what_fires_and_what_holds_a_value() {
        let (mut store, _dir) = store_at("intersections");
        let firing = mk_project(&mut store, "firing");
        let off_with_value = mk_project(&mut store, "off-with-value");
        mk_project(&mut store, "untouched");
        let fields = vec![text_field("channel")];

        crate::plugin_trust::enable(&mut store, "slack", Layer::Project(firing), &fields, |_| true, &Checked::NotDeclared)
            .unwrap();
        set(&mut store, &text_field("channel"), "slack", Layer::Project(off_with_value), "#general").unwrap();

        assert_eq!(
            intersections(&store, "slack", &fields).unwrap(),
            vec![
                Intersection {
                    project: firing,
                    enabled: true,
                    has_value: false,
                    required_unset: false,
                },
                Intersection {
                    project: off_with_value,
                    enabled: false,
                    has_value: true,
                    required_unset: false,
                },
            ],
        );
    }

    /// The mark a row carries before anyone presses its switch (`AMB-D-351`): a `required` setting with no
    /// value at this crossing is exactly why an enable there would be refused.
    #[test]
    fn a_row_says_when_a_required_setting_is_empty_at_that_crossing() {
        let (mut store, _dir) = store_at("intersections-required");
        let short = mk_project(&mut store, "short");
        let filled = mk_project(&mut store, "filled");
        let fields = vec![required_field("webhook_url")];

        crate::plugin_trust::enable(&mut store, "slack", Layer::Project(short), &fields, |_| true, &Checked::NotDeclared)
            .unwrap();
        set(&mut store, &required_field("webhook_url"), "slack", Layer::Project(filled), "https://hooks/x").unwrap();

        let rows = intersections(&store, "slack", &fields).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].required_unset, "the gate that is short of the value is marked");
        assert!(!rows[1].required_unset, "the project that filled it in is not");
    }

    /// The device's row is read the same way and by the same function, which is the whole point of it
    /// being one: a machine-wide plugin has no crossing to be in [`intersections`], so a face drawing it
    /// would otherwise have to work out for itself what "has a value" and "would be refused" mean
    /// (`AMB-D-601`).
    #[test]
    fn the_device_layer_is_read_by_the_same_two_readings_a_crossing_is() {
        let (mut store, _dir) = store_at("held-at-device");
        let elsewhere = mk_project(&mut store, "elsewhere");
        let fields = vec![required_field("webhook_url")];

        let empty = held_at(&store, "slack", &fields, Layer::Device).unwrap();
        assert_eq!(
            (empty.has_value, empty.required_unset),
            (false, true),
            "nothing held, and the required setting is what an enable here would be refused over",
        );

        // A project holding it is not the device holding it: the two layers are separate rows.
        set(&mut store, &required_field("webhook_url"), "slack", Layer::Project(elsewhere), "https://hooks/x")
            .unwrap();
        assert!(held_at(&store, "slack", &fields, Layer::Device).unwrap().required_unset);

        set(&mut store, &required_field("webhook_url"), "slack", Layer::Device, "https://hooks/y").unwrap();
        let filled = held_at(&store, "slack", &fields, Layer::Device).unwrap();
        assert_eq!((filled.has_value, filled.required_unset), (true, false));

        // And it is nowhere in the crossings, which name projects and only projects.
        let rows = intersections(&store, "slack", &fields).unwrap();
        assert_eq!(rows.len(), 1, "only the project that filled it in");
        assert_eq!(rows[0].project, elsewhere);
    }

    /// A secret is a value like any other here: which table the author's flag sent it to (`AMB-D-356`)
    /// does not decide whether the project has a row.
    #[test]
    fn a_secret_alone_puts_a_project_on_the_list() {
        let (mut store, _dir) = store_at("intersections-secret");
        let p = mk_project(&mut store, "p");
        let fields = vec![secret_field("webhook_url")];
        set(&mut store, &secret_field("webhook_url"), "slack", Layer::Project(p), "https://hooks/x").unwrap();

        assert_eq!(
            intersections(&store, "slack", &fields).unwrap(),
            vec![Intersection {
                project: p,
                enabled: false,
                has_value: true,
                required_unset: false,
            }],
        );
    }

    /// A value left behind for a key the author has since dropped draws no row — there would be nothing on
    /// the schema to fill in there.
    #[test]
    fn a_value_for_an_undeclared_key_draws_no_row() {
        let (mut store, _dir) = store_at("intersections-undeclared");
        let p = mk_project(&mut store, "p");
        set(&mut store, &text_field("dropped"), "slack", Layer::Project(p), "x").unwrap();

        assert!(intersections(&store, "slack", &[text_field("channel")]).unwrap().is_empty());
    }

    /// The purge an update ends with (`AMB-D-456`): a key the new schema no longer names loses its value
    /// in every project, and the keys it still names keep theirs.
    #[test]
    fn a_key_the_new_schema_dropped_loses_its_value_everywhere() {
        let (mut store, _dir) = store_at("purge-dropped");
        let a = mk_project(&mut store, "a");
        let b = mk_project(&mut store, "b");
        set(&mut store, &text_field("channel"), "slack", Layer::Project(a), "#a").unwrap();
        set(&mut store, &text_field("webhook"), "slack", Layer::Project(a), "https://a.invalid").unwrap();
        set(&mut store, &text_field("webhook"), "slack", Layer::Project(b), "https://b.invalid").unwrap();
        set(&mut store, &secret_field("token"), "slack", Layer::Project(a), "s3cret").unwrap();
        // Another plugin, holding a key of the same name — the purge is one plugin's.
        set(&mut store, &text_field("webhook"), "worktree", Layer::Project(a), "kept").unwrap();

        let now = [text_field("channel"), secret_field("token")];
        assert_eq!(
            purge_undeclared(&mut store, "slack", &now).unwrap(),
            Purged { settings: 2, secrets: 0 },
        );

        assert_eq!(get(&store, &text_field("webhook"), "slack", Layer::Project(a)).unwrap(), None);
        assert_eq!(get(&store, &text_field("webhook"), "slack", Layer::Project(b)).unwrap(), None, "every project");
        assert_eq!(get(&store, &text_field("channel"), "slack", Layer::Project(a)).unwrap().as_deref(), Some("#a"));
        assert_eq!(get(&store, &secret_field("token"), "slack", Layer::Project(a)).unwrap().as_deref(), Some("s3cret"));
        assert_eq!(
            get(&store, &text_field("webhook"), "worktree", Layer::Project(a)).unwrap().as_deref(),
            Some("kept"),
            "another plugin's key of the same name is not this plugin's residue",
        );
    }

    /// A schema that declares everything it did before — or more — takes nothing, and running it twice
    /// takes nothing the second time either: the purge is keyed on what is declared, not on a diff.
    #[test]
    fn a_schema_that_still_declares_everything_purges_nothing() {
        let (mut store, _dir) = store_at("purge-nothing");
        let p = mk_project(&mut store, "p");
        set(&mut store, &text_field("channel"), "slack", Layer::Project(p), "#a").unwrap();
        set(&mut store, &secret_field("token"), "slack", Layer::Project(p), "s3cret").unwrap();

        let grown = [text_field("channel"), secret_field("token"), text_field("added")];
        assert_eq!(purge_undeclared(&mut store, "slack", &grown).unwrap(), Purged::default());
        assert!(!purge_undeclared(&mut store, "slack", &grown).unwrap().anything());
        assert_eq!(get(&store, &text_field("channel"), "slack", Layer::Project(p)).unwrap().as_deref(), Some("#a"));
        assert_eq!(get(&store, &secret_field("token"), "slack", Layer::Project(p)).unwrap().as_deref(), Some("s3cret"));
    }

    /// A key that stayed but stopped being a secret leaves the secret table: the new declaration is not
    /// asking for a secret under that name, and the bytes in the table an `export` must leave are the
    /// ones with least reason to stay (`AMB-D-456`).
    #[test]
    fn a_key_that_stopped_being_a_secret_leaves_the_secret_table() {
        let (mut store, _dir) = store_at("purge-flip");
        let p = mk_project(&mut store, "p");
        set(&mut store, &secret_field("token"), "slack", Layer::Project(p), "s3cret").unwrap();

        let now = [text_field("token")];
        assert_eq!(
            purge_undeclared(&mut store, "slack", &now).unwrap(),
            Purged { settings: 0, secrets: 1 },
        );
        assert_eq!(get(&store, &secret_field("token"), "slack", Layer::Project(p)).unwrap(), None);
        assert_eq!(get(&store, &text_field("token"), "slack", Layer::Project(p)).unwrap(), None, "and nothing moved");
    }

    #[test]
    fn the_floor_rejects_an_oversize_or_control_char_value() {
        let (mut store, _dir) = store_at("floor");
        let p = mk_project(&mut store, "proj");
        let big = "x".repeat(MAX_CONFIG_VALUE_BYTES + 1);
        assert!(set(&mut store, &text_field("k"), "plug", Layer::Project(p), &big).is_err());
        assert!(set(&mut store, &text_field("k"), "plug", Layer::Project(p), "a\u{0}b").is_err());
        // Nothing landed from the rejected writes.
        assert_eq!(get(&store, &text_field("k"), "plug", Layer::Project(p)).unwrap(), None);
    }
}
