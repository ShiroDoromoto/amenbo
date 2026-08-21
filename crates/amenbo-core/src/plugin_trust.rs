//! The **enable/disable boundary** — `install ≠ enable` (`AMB-D-351`).
//!
//! Installing a plugin puts its binary on disk; it does not run it. Enabling opens the **gate** the plugin
//! fires through. This module is the one door that moves that state, exactly as [`crate::plugin_config`] is
//! the one door for a config *value* — so the fail-closed rule below lives in a single place.
//!
//! **One switch, at the layer its author declared** (`AMB-D-434` / `AMB-D-601`). The gate is a row in the
//! store's `plugin_enable`, and the row **is** the answer — present means on here, absent means off.
//! *Which* "here" is [`Layer`]'s to say, derived from the manifest's `scope` and never from where the
//! caller happens to stand: a `scope: project` plugin has one gate per project, and a `scope: machine`
//! plugin one for the device. Either way there is a single tier, so a user is never shown two switches for
//! one plugin, and a caller standing in no project has no *project* gate to move ([`require_project`])
//! rather than a device-wide one to fall back on.
//!
//! **Enabling is the consent** (`AMB-D-434`). Running somebody else's code is what turning a plugin on
//! means, so the act is the permission and Amenbo keeps no separate answer beside it. That also makes the
//! rows safe to carry whole: they ride `export` and `backup` (a restore must not silently switch a
//! project's plugins off) and say the same thing wherever they land, which is what a device-local answer
//! beside them could never do.
//!
//! **Fail-closed on `required`** (`AMB-D-351`). A plugin whose manifest marks a setting `required` cannot be
//! enabled until that setting holds a value: [`enable`] refuses, naming the empty fields. Amenbo checks
//! **presence only** — whether a value is *valid* is the plugin author's at run time (`AMB-D-356`). Where a
//! value lives is the caller's to resolve and report through `has_value`; this boundary does not reach into
//! storage itself.
//!
//! **And fail-closed on the author's own check** (`AMB-D-664`). Presence is all Amenbo can judge, so a
//! manifest may name a call that judges the rest ([`crate::plugin_check`]), and [`enable`] takes the
//! verdict it produced: anything but a yes leaves the gate shut, a check nobody declared changes nothing.
//! The verdict is raised by the caller and refused here, which is what keeps the two halves where they
//! belong — the author's sentences with the face that shows them, the gate with the door that moves it.
//!
//! **Not the CLI, not the GUI.** Those faces call in here after they have the manifest and the resolved
//! values; the state model and its gate are here so both drive them the same way.

use crate::error::{Error, ErrorCode, Msg, Result};
use crate::plugin_check::Checked;
use crate::plugin_layer::Layer;
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
/// A `scope: project` plugin is a project's, so a context with no project has no answer to give — refused
/// here rather than silently resolved device-wide, a fallback no declaration asked for. Asked from one
/// place only: [`Layer::of`](crate::plugin_layer::Layer::of), where the declaration decides whether the
/// question arises at all — a face that asked it beside the layer would put the project plugin's refusal in
/// front of a device-wide run that never needed a project.
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
/// Amenbo checks (`AMB-D-351`); the author validates meaning at run time (`AMB-D-356`).
///
/// **A field carrying a `default` is never unanswered** (`AMB-D-415`), so it is not held against an enable
/// however it is marked: the store holding nothing for it means the author's value is in force, and that is
/// what the run receives ([`plugin_inject`](crate::plugin_inject)). `required` asks whether the plugin can
/// work without an answer, and a default is one — the two are separate axes, and demanding a user retype a
/// value that is already in effect would refuse a plugin that is, in fact, fully configured.
///
/// **Neither is a field the author's conditions currently hide** (`AMB-D-727`). A hidden `required` field
/// that is empty would shut this gate over a box the form does not draw: the user is told a setting is
/// missing, goes to fill it in, and there is nothing on the screen to fill in. So `stage` is asked here
/// rather than by each caller — it is what makes forgetting it impossible, which is the only reason this
/// takes a parameter it could have been handed a filtered list instead of
/// ([`crate::plugin_when::visible_fields`]).
pub fn missing_required<'a>(
    fields: &'a [ConfigField],
    stage: &crate::plugin_when::Stage,
    has_value: impl Fn(&ConfigField) -> bool,
) -> Vec<&'a str> {
    fields
        .iter()
        .filter(|f| stage.shows(&f.when))
        .filter(|f| f.required && f.default.is_none() && !has_value(f))
        .map(|f| f.key.as_str())
        .collect()
}

/// Enable a plugin at one layer: fail-closed on unsatisfied `required` settings (`AMB-D-351`) and on the
/// author's own check (`AMB-D-664`), then open that layer's gate. `has_value` reports whether one field
/// currently holds a value — the caller resolves that; this boundary does not touch storage for it.
///
/// **Opening the device gate is the consent to let the plugin read the whole machine** (`AMB-D-601`), so
/// nothing asks a second time: a `scope: machine` plugin's [`Layer::Device`] is that answer, and there is no
/// separate one stored beside it.
///
/// `stage` is what this layer's answers and this platform make of the author's conditions
/// ([`crate::plugin_config::stage`]) — the gate is judged on the fields a user can actually see
/// ([`missing_required`]).
///
/// `checked` is what the author's check said about the values ([`crate::plugin_check::run`]), and the
/// caller raises it rather than this door: the run is the caller's to hold, because a verdict carries
/// sentences meant for the screen and only the face knows whether it has one
/// ([`Verdict`](crate::plugin_check::Verdict)). What is *not* the caller's is what a verdict costs — a
/// check that did not say yes leaves the gate shut here, so no face can enable a plugin without having
/// asked. A plugin declaring no check hands in [`Checked::NotDeclared`], which is every plugin written
/// before the block existed.
///
/// Idempotent: a plugin already on at that layer ends where it started.
pub fn enable(
    store: &mut Store,
    plugin: &str,
    layer: Layer,
    fields: &[ConfigField],
    stage: &crate::plugin_when::Stage,
    has_value: impl Fn(&ConfigField) -> bool,
    checked: &Checked,
) -> Result<()> {
    // Presence first: it costs nothing to read, and a plugin with an empty `required` field is refused in
    // the words a user can act on, rather than in whatever the author's code made of the emptiness.
    refuse_missing_required(plugin, fields, stage, has_value)?;
    refuse_failed_check(plugin, checked)?;
    store.set_plugin_enabled_in_project(layer.project_id(), plugin, true)?;
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
/// is left to them. Closing the **device** gate is the whole plugin stopping, so everything queued for it
/// goes, whichever project the event came from (`AMB-D-601`) — there is no other gate left to carry it out.
///
/// [`Stopped`] carries what that cost was on this call, so a face can say it out loud instead of leaving a
/// silent discard.
pub fn disable(store: &mut Store, plugin: &str, layer: Layer) -> Result<Stopped> {
    store.set_plugin_enabled_in_project(layer.project_id(), plugin, false)?;
    // The gate is closed first, so nothing is queued behind the drop: the fan-out asks this same gate
    // (`plugin_subscribe::EnabledSubscribers`) and a write racing this one either sees the gate still open
    // and queues what the drop then takes, or sees it shut and queues nothing.
    let queued = store.drop_plugin_delivery(plugin, layer.project_id())?;
    Ok(Stopped { queued })
}

/// Whether the plugin fires at `layer` — the row, and nothing beside it (`AMB-D-434` / `AMB-D-601`).
pub fn effective_enabled_in(store: &Store, plugin: &str, layer: Layer) -> Result<bool> {
    store.plugin_enabled_in_project(layer.project_id(), plugin)
}

/// The fail-closed reading of the author's own check (`AMB-D-664`), as the refusal every face shares.
///
/// **What it says is the field keys, and never the author's sentences.** A verdict's `message` and its
/// per-field lines are the GUI settings form's — the face that has a person in front of it — while this
/// refusal travels to whoever called `enable`, the CLI and its `--json` included. So the refusal repeats
/// the keys the check spoke about, which are the form's own words, and the rest of the verdict stays with
/// the caller that is going to draw it. A check that said nothing at all names no keys: it is Amenbo's own
/// sentence about a run that did not answer, and there is nothing of the plugin's in it.
///
/// **And the two are told apart by code**, which is the half a reader of `--json` has instead of the
/// sentence: [`ErrorCode::InvalidPluginCheckRefused`] is the check having looked and said no, and
/// [`ErrorCode::InvalidPluginCheckSilent`] is a run that answered nothing this build can act on. Both are
/// CLI-only — the GUI takes the verdict and the shut gate rather than this refusal (`AMB-D-664`).
fn refuse_failed_check(plugin: &str, checked: &Checked) -> Result<()> {
    if checked.opens_the_gate() {
        return Ok(());
    }
    let named = checked.verdict().map(|verdict| verdict.field_keys().join(", ")).unwrap_or_default();
    // The code says which of the two happened, because they are different facts and a caller reading
    // `--json` acts on them differently: a refusal is about the values in front of the user, a silence is
    // about the plugin (`AMB-D-354`). It is a CLI-only code — the GUI is handed the verdict and the shut
    // gate instead of this refusal (`AMB-D-664`), so no screen ever puts this sentence in front of anyone.
    let (code, refusal) = match (checked.silence(), named.is_empty()) {
        (Some(silence), _) => (
            ErrorCode::InvalidPluginCheckSilent,
            format!(
                "plugin '{plugin}' cannot be enabled: its own check did not answer — {}",
                silence.as_str()
            ),
        ),
        (None, true) => (
            ErrorCode::InvalidPluginCheckRefused,
            format!("plugin '{plugin}' cannot be enabled: its own check refused the values it was given"),
        ),
        (None, false) => (
            ErrorCode::InvalidPluginCheckRefused,
            format!("plugin '{plugin}' cannot be enabled: its own check refused the setting(s): {named}"),
        ),
    };
    Err(Error::Invalid(
        Msg::new(refusal).coded(code).with("name", plugin).with("settings", named),
    ))
}

/// The fail-closed `required` check both enable doors run (`AMB-D-351`), as the refusal they share.
fn refuse_missing_required(
    plugin: &str,
    fields: &[ConfigField],
    stage: &crate::plugin_when::Stage,
    has_value: impl Fn(&ConfigField) -> bool,
) -> Result<()> {
    let missing = missing_required(fields, stage, has_value);
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

    /// A stage in which nothing the author conditioned is showing — the platform is one we do not name and
    /// no field has an answer, so every clause fails. What a hidden `required` field is judged against.
    fn nothing_shown() -> crate::plugin_when::Stage {
        crate::plugin_when::Stage::on(None, Default::default())
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

    // ──────────────────── the gate sits at one declared layer, and only one ───────────────────────

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
        assert!(!effective_enabled_in(&store, "slack", Layer::Project(p)).unwrap());
    }

    /// The gate end to end: this project fires, and no other is touched.
    #[test]
    fn an_enable_opens_only_that_project() {
        let mut store = store_at("project-enable");
        let a = mk_project(&mut store, "a");
        let b = mk_project(&mut store, "b");

        enable(&mut store, "slack", Layer::Project(a), &[], &crate::plugin_when::Stage::default(), |_| true, &Checked::NotDeclared).unwrap();

        assert!(effective_enabled_in(&store, "slack", Layer::Project(a)).unwrap());
        assert!(!effective_enabled_in(&store, "slack", Layer::Project(b)).unwrap());
    }

    /// Turning it off in one project leaves the others alone, and turning it back on asks nothing.
    #[test]
    fn a_disable_is_that_project_alone() {
        let mut store = store_at("project-disable");
        let a = mk_project(&mut store, "a");
        let b = mk_project(&mut store, "b");
        enable(&mut store, "slack", Layer::Project(a), &[], &crate::plugin_when::Stage::default(), |_| true, &Checked::NotDeclared).unwrap();
        enable(&mut store, "slack", Layer::Project(b), &[], &crate::plugin_when::Stage::default(), |_| true, &Checked::NotDeclared).unwrap();

        disable(&mut store, "slack", Layer::Project(a)).unwrap();
        assert!(!effective_enabled_in(&store, "slack", Layer::Project(a)).unwrap());
        assert!(effective_enabled_in(&store, "slack", Layer::Project(b)).unwrap(), "b is untouched");

        enable(&mut store, "slack", Layer::Project(a), &[], &crate::plugin_when::Stage::default(), |_| true, &Checked::NotDeclared).unwrap();
        assert!(effective_enabled_in(&store, "slack", Layer::Project(a)).unwrap());
    }

    /// A restored row fires where it lands (`AMB-D-434`): the state is all in the store, so a backup
    /// carried onto another machine brings the plugin back on with it.
    #[test]
    fn a_carried_row_fires_on_its_own() {
        let mut store = store_at("carried-row");
        let p = mk_project(&mut store, "p");
        // The row as a restore would leave it: written straight to the store.
        store.set_plugin_enabled_in_project(Some(p), "slack", true).unwrap();
        assert!(effective_enabled_in(&store, "slack", Layer::Project(p)).unwrap());
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
        enable(&mut store, "slack", Layer::Project(a), &[], &crate::plugin_when::Stage::default(), |_| true, &Checked::NotDeclared).unwrap();
        enable(&mut store, "slack", Layer::Project(b), &[], &crate::plugin_when::Stage::default(), |_| true, &Checked::NotDeclared).unwrap();
        queue_for(&store, "slack", Some(a), 1);
        queue_for(&store, "slack", Some(b), 2);
        let tx = store.read_model().write().unwrap();
        tx.claim_runner("slack", "runner-1", "2999-01-01T00:00:00Z", "2026-07-25T09:00:00Z").unwrap();
        tx.commit().unwrap();

        let stopped = disable(&mut store, "slack", Layer::Project(a)).unwrap();

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
        enable(&mut store, "slack", Layer::Project(p), &[], &crate::plugin_when::Stage::default(), |_| true, &Checked::NotDeclared).unwrap();
        queue_for(&store, "slack", Some(p), 1);
        let tx = store.read_model().write().unwrap();
        tx.claim_runner("slack", "runner-1", "2999-01-01T00:00:00Z", "2026-07-25T09:00:00Z").unwrap();
        tx.commit().unwrap();

        let stopped = disable(&mut store, "slack", Layer::Project(p)).unwrap();

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

        let err = enable(&mut store, "slack", Layer::Project(p), &fields, &crate::plugin_when::Stage::default(), |_| false, &Checked::NotDeclared)
            .unwrap_err();
        assert!(format!("{err:?}").contains("webhook_url"), "the empty field is named");
        assert!(!effective_enabled_in(&store, "slack", Layer::Project(p)).unwrap());
    }

    /// The same required field, now holding a value, no longer blocks enable.
    #[test]
    fn a_satisfied_required_field_allows_enable() {
        let mut store = store_at("satisfied");
        let p = mk_project(&mut store, "p");
        let fields = [field("webhook_url", true)];
        enable(&mut store, "slack", Layer::Project(p), &fields, &crate::plugin_when::Stage::default(), |f| f.key == "webhook_url", &Checked::NotDeclared)
            .unwrap();
        assert!(effective_enabled_in(&store, "slack", Layer::Project(p)).unwrap());
    }

    /// **A required field the author's conditions hide does not shut the gate** (`AMB-D-727`). The refusal
    /// names a setting, and the form the user is sent to has no box for it — so the plugin could never be
    /// turned on, and the screen could never say why.
    #[test]
    fn a_required_field_that_is_hidden_does_not_refuse_enable() {
        let mut store = store_at("hidden-required");
        let p = mk_project(&mut store, "p");
        let hidden = ConfigField {
            when: vec![crate::plugin_when::When::field_has("transport", "cloudflare")],
            ..field("worker_url", true)
        };
        let fields = [field("transport", false), hidden];

        // Nothing answers the condition, so the field is off screen: the gate opens with it empty.
        enable(
            &mut store,
            "viewer",
            Layer::Project(p),
            &fields,
            &nothing_shown(),
            |f| f.key == "transport",
            &Checked::NotDeclared,
        )
        .unwrap();
        assert!(effective_enabled_in(&store, "viewer", Layer::Project(p)).unwrap());

        // Once it is on screen it is a setting like any other, and an empty one shuts the gate.
        let shown = crate::plugin_when::Stage::on(
            None,
            [("transport".to_string(), "cloudflare".to_string())].into_iter().collect(),
        );
        let err = enable(
            &mut store,
            "viewer2",
            Layer::Project(p),
            &fields,
            &shown,
            |f| f.key == "transport",
            &Checked::NotDeclared,
        )
        .unwrap_err();
        assert!(format!("{err:?}").contains("worker_url"), "the setting is named: {err:?}");
    }

    /// `missing_required` reports only the empty required fields — optional and satisfied ones are not
    /// blockers.
    #[test]
    fn missing_required_lists_only_the_empty_required_fields() {
        let fields = [field("a", true), field("b", false), field("c", true)];
        let missing = missing_required(&fields, &crate::plugin_when::Stage::default(), |f| f.key == "a");
        assert_eq!(missing, vec!["c"]);
    }

    // ─────────────── fail-closed on the author's own check (`AMB-D-664`) ───────────────

    /// A verdict that returns [`Checked::Answered`] with `ok`.
    fn said(ok: bool, about: &[(&str, &str)]) -> Checked {
        Checked::Answered(crate::plugin_check::Verdict {
            ok,
            message: Some("cannot sign in".into()),
            fields: about.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect(),
        })
    }

    /// A check that said no holds the gate shut, and the refusal names the settings it spoke about — never
    /// the sentences beside them, which travel to the form that draws them.
    #[test]
    fn a_check_that_said_no_refuses_enable_and_names_only_the_settings() {
        let mut store = store_at("check-no");
        let p = mk_project(&mut store, "p");

        let err = enable(
            &mut store,
            "mail",
            Layer::Project(p),
            &[],
            &crate::plugin_when::Stage::default(),
            |_| true,
            &said(false, &[("smtp_user", "no such mailbox")]),
        )
        .unwrap_err();

        assert!(format!("{err:?}").contains("smtp_user"), "the setting is named: {err:?}");
        assert!(
            !format!("{err:?}").contains("no such mailbox")
                && !format!("{err:?}").contains("cannot sign in"),
            "the author's own sentences stay off the refusal: {err:?}"
        );
        // What a reader of `--json` has instead of the sentence. It is the finer code and not the family's,
        // because "your values are wrong" and "a key the manifest never declared" are both `invalid_value`
        // and nothing downstream could tell them apart.
        assert_eq!(err.code(), "invalid_plugin_check_refused", "{err:?}");
        assert!(!effective_enabled_in(&store, "mail", Layer::Project(p)).unwrap());
    }

    /// A check that said nothing costs the same as one that said no — and says so in Amenbo's own words,
    /// since there is no answer of the plugin's to repeat.
    #[test]
    fn a_check_that_said_nothing_refuses_enable_too() {
        let mut store = store_at("check-silent");
        let p = mk_project(&mut store, "p");

        for silence in [
            crate::plugin_check::Silence::NotLaunched,
            crate::plugin_check::Silence::Failed,
            crate::plugin_check::Silence::TimedOut,
            crate::plugin_check::Silence::Unreadable,
        ] {
            let err =
                enable(&mut store, "mail", Layer::Project(p), &[], &crate::plugin_when::Stage::default(), |_| true, &Checked::Silent(silence))
                    .unwrap_err();
            assert!(format!("{err:?}").contains("did not answer"), "{err:?}");
            // A code of its own, whichever way the run failed to answer: the four silences are one fact to
            // a caller — the check is not usable — and the reason they differ is on the execution log.
            assert_eq!(err.code(), "invalid_plugin_check_silent", "{err:?}");
            assert!(!effective_enabled_in(&store, "mail", Layer::Project(p)).unwrap());
        }
    }

    /// A check that said yes opens the gate, whatever else it had to say.
    #[test]
    fn a_check_that_said_yes_opens_the_gate() {
        let mut store = store_at("check-yes");
        let p = mk_project(&mut store, "p");

        enable(&mut store, "mail", Layer::Project(p), &[], &crate::plugin_when::Stage::default(), |_| true, &said(true, &[])).unwrap();

        assert!(effective_enabled_in(&store, "mail", Layer::Project(p)).unwrap());
    }

    /// Presence is judged first: a plugin with an empty `required` field is refused in the words a user
    /// can act on, rather than in whatever the author's code made of the emptiness.
    #[test]
    fn an_empty_required_field_is_the_refusal_even_when_the_check_also_said_no() {
        let mut store = store_at("check-and-required");
        let p = mk_project(&mut store, "p");
        let fields = [field("webhook_url", true)];

        let err = enable(
            &mut store,
            "mail",
            Layer::Project(p),
            &fields,
            &crate::plugin_when::Stage::default(),
            |_| false,
            &said(false, &[("webhook_url", "not a url")]),
        )
        .unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidPluginSettingsRequired.as_str(), "{err:?}");
    }

    /// A required field with a `default` is answered by its author (`AMB-D-415`): the run receives that
    /// value, so an empty store is not a reason to hold the plugin off.
    #[test]
    fn a_required_field_with_a_default_does_not_block_enable() {
        let fields = [
            ConfigField { default: Some("task.done".into()), ..field("events", true) },
            field("webhook_url", true),
        ];
        assert_eq!(missing_required(&fields, &crate::plugin_when::Stage::default(), |_| false), vec!["webhook_url"]);

        let mut store = store_at("defaulted");
        let p = mk_project(&mut store, "p");
        enable(&mut store, "slack", Layer::Project(p), &fields[..1], &crate::plugin_when::Stage::default(), |_| false, &Checked::NotDeclared).unwrap();
        assert!(effective_enabled_in(&store, "slack", Layer::Project(p)).unwrap());
    }
}
