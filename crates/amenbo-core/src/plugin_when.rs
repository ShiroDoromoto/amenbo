//! **When a declared thing is shown** (`AMB-D-727`) — the condition an author writes on a settings field,
//! on one of its candidates, or on an operation, and the reading of it.
//!
//! The settings form used to draw a manifest's fields top to bottom, all of them, always. So a Windows
//! user was offered an iCloud transport, and someone who had not chosen Cloudflare still had Cloudflare's
//! three fields to scroll past. Neither is something the author could fix from their side: there was
//! nowhere to say "only when".
//!
//! **The reading is here, in core, and not on the screen** (`AMB-D-727`). A face does not know which OS
//! this build is running on, and core does; putting the judgement anywhere else would let `plugin config
//! get` and the form answer differently about the same field. What a face receives is the list that
//! survived.
//!
//! **What a condition hides is the field, never the value.** A value saved on a Mac is still there when the
//! same store is opened on Windows, and still handed to the plugin — hiding a box is a statement about the
//! screen, not a reason to throw an answer away. The one place hiding does decide something is `required`:
//! a hidden field that is empty must not shut the enable gate, because the form has nowhere to show the
//! user what is missing ([`crate::plugin_trust::missing_required`]).
//!
//! The type lives here rather than beside the rest of the manifest's shapes because it is the one that
//! carries a reading: [`Stage`] is what a condition is judged against, and keeping the two together is what
//! keeps a second, drifting reading from being written somewhere else.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::plugin_manifest::{ConfigField, Os, SettingsAction};

/// **One condition** (`AMB-D-727`). There are two kinds and no more — the platform this build runs on, and
/// what another field currently holds:
///
/// ```yaml
/// when:
///   - { os: [macos] }
///   - { field: transport, has: cloudflare }
/// ```
///
/// **A list of them is an `and`.** Everything written has to hold for the thing to be shown. There is no
/// `or` and no `not`: the conditions an author actually reached for were conjunctions, and a language that
/// grows operators is one the form has to explain.
///
/// Both keys of a kind are written together — `field` names the key, `has` the value looked for. A clause
/// carrying half of a kind, or neither kind, is refused by the validator
/// ([`crate::plugin_validate`]) rather than read; if one reaches the reading anyway it decides nothing, so
/// a condition Amenbo cannot make sense of leaves the thing visible rather than making it disappear
/// without a way to ask why.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct When {
    /// The platforms this holds on. Empty means the clause says nothing about the platform — it is absent,
    /// not "no platform".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub os: Vec<Os>,
    /// The key of the field whose value this clause reads — a [`ConfigField::key`] of the same manifest.
    /// Written with [`has`](When::has); alone it is a mistake the validator names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// The value looked for in the field [`field`](When::field) names. A `multi` field holds several at
    /// once, so this asks whether the value is among them rather than whether it is the whole answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has: Option<String>,
}

impl When {
    /// A clause on the platform: shown on these, hidden elsewhere.
    pub fn on(os: impl IntoIterator<Item = Os>) -> Self {
        When { os: os.into_iter().collect(), ..When::default() }
    }

    /// A clause on another field's answer: shown while `field` holds `has`.
    pub fn field_has(field: impl Into<String>, has: impl Into<String>) -> Self {
        When { field: Some(field.into()), has: Some(has.into()), ..When::default() }
    }

    /// Whether this one clause holds on `stage`. A clause that names neither kind holds — see the type's
    /// note on why an unreadable condition shows rather than hides.
    fn holds(&self, stage: &Stage) -> bool {
        if !self.os.is_empty() && !stage.os.is_some_and(|here| self.os.contains(&here)) {
            return false;
        }
        if let (Some(field), Some(has)) = (&self.field, &self.has) {
            if !stage.field_has(field, has) {
                return false;
            }
        }
        true
    }
}

/// **What a condition is judged against**: the platform this build runs on, and the answer each field
/// currently has at the layer being read (`AMB-D-727`).
///
/// A stage is one layer's, because the answers are: the same plugin can hold different values for two
/// projects, so "is `transport` set to `cloudflare`" has a different answer at each of them. Build one per
/// layer being read ([`crate::plugin_config::stage`]) rather than reusing another's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stage {
    os: Option<Os>,
    values: BTreeMap<String, String>,
}

impl Stage {
    /// A stage on this build's platform, holding `values` — the resolved answer for each field that has
    /// one, keyed by [`ConfigField::key`]. This is what every caller reading a real store wants;
    /// [`on`](Stage::on) is for saying otherwise.
    pub fn here(values: BTreeMap<String, String>) -> Self {
        Stage { os: Os::here(), values }
    }

    /// A stage on a named platform. `None` is a platform Amenbo's own vocabulary cannot name, where no
    /// `os` clause can hold — nothing Amenbo ships runs there, so the case exists to be total rather than
    /// to be met.
    pub fn on(os: Option<Os>, values: BTreeMap<String, String>) -> Self {
        Stage { os, values }
    }

    /// Whether every clause in `when` holds here. An empty list is an unconditional thing, and is shown.
    pub fn shows(&self, when: &[When]) -> bool {
        when.iter().all(|clause| clause.holds(self))
    }

    /// Whether the field `key` names currently answers with `has`.
    ///
    /// A `multi` field stores its chosen candidates joined by commas (`AMB-D-415`), so the answer is read
    /// as the set it is: `has` matches when it is one of them. A text field has one part and compares
    /// whole. A field with no answer at all matches nothing — an unanswered question is not a "no", but
    /// for the purpose of showing something *else* it may as well be, and the alternative is a form whose
    /// dependent fields are all visible until the first save.
    fn field_has(&self, key: &str, has: &str) -> bool {
        self.values.get(key).is_some_and(|held| held.split(',').any(|part| part == has))
    }
}

/// **The fields a face draws at this stage** — the ones whose conditions hold, each carrying only the
/// candidates whose own conditions hold (`AMB-D-727`).
///
/// Owned rather than borrowed because a surviving field is not always the declared one: a `multi` field
/// whose candidate list was narrowed is a field the manifest does not contain.
///
/// **Nothing else may be filtered through this.** What a plugin is handed at run time is every value the
/// store holds ([`crate::plugin_inject`]), and what an undeclared-value purge compares against is every
/// declared key ([`crate::plugin_config::purge_undeclared`]) — passing a narrowed list to either would
/// throw away a value for being off screen.
pub fn visible_fields(fields: &[ConfigField], stage: &Stage) -> Vec<ConfigField> {
    fields
        .iter()
        .filter(|field| stage.shows(&field.when))
        .map(|field| {
            let mut field = field.clone();
            field.options.retain(|option| stage.shows(&option.when));
            field
        })
        .collect()
}

/// **The operations a face offers at this stage** (`AMB-D-727`) — someone who chose only iCloud has no use
/// for a button that raises a Cloudflare tunnel, and a form that hides the fields but keeps the button
/// leaves them a step they cannot read.
pub fn visible_actions<'a>(actions: &'a [SettingsAction], stage: &Stage) -> Vec<&'a SettingsAction> {
    actions.iter().filter(|action| stage.shows(&action.when)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_manifest::{ConfigOption, FieldType};

    fn stage(os: Option<Os>, values: &[(&str, &str)]) -> Stage {
        Stage::on(os, values.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    /// Nothing declared is unconditional: a field written before the key existed is shown everywhere.
    #[test]
    fn no_condition_is_always_shown() {
        assert!(stage(Some(Os::Macos), &[]).shows(&[]));
    }

    /// The platform clause is the platform this build runs on, and nothing else.
    #[test]
    fn an_os_clause_holds_only_on_the_platforms_it_names() {
        let when = [When::on([Os::Macos])];
        assert!(stage(Some(Os::Macos), &[]).shows(&when));
        assert!(!stage(Some(Os::Windows), &[]).shows(&when));
        assert!(!stage(None, &[]).shows(&when), "a platform we cannot name matches no os clause");
    }

    /// A `multi` field holds its candidates joined by commas (`AMB-D-415`), so the clause asks whether the
    /// value is among them — not whether it is the whole answer.
    #[test]
    fn a_field_clause_reads_a_multi_answer_as_the_set_it_is() {
        let when = [When::field_has("transport", "cloudflare")];
        assert!(stage(Some(Os::Macos), &[("transport", "icloud,cloudflare")]).shows(&when));
        assert!(stage(Some(Os::Macos), &[("transport", "cloudflare")]).shows(&when));
        assert!(!stage(Some(Os::Macos), &[("transport", "icloud")]).shows(&when));
        assert!(!stage(Some(Os::Macos), &[]).shows(&when), "an unanswered field matches nothing");
    }

    /// Several clauses are an `and` — every one of them has to hold.
    #[test]
    fn clauses_are_read_together() {
        let when = [When::on([Os::Macos]), When::field_has("transport", "cloudflare")];
        assert!(stage(Some(Os::Macos), &[("transport", "cloudflare")]).shows(&when));
        assert!(!stage(Some(Os::Windows), &[("transport", "cloudflare")]).shows(&when));
        assert!(!stage(Some(Os::Macos), &[("transport", "icloud")]).shows(&when));
    }

    /// A clause naming neither kind decides nothing, so the thing it is written on stays visible. The
    /// validator refuses the manifest that carries one; a reading that hid it instead would make an
    /// author's mistake look like a field that was never declared.
    #[test]
    fn a_clause_that_names_neither_kind_leaves_the_thing_visible() {
        let half = When { field: Some("transport".into()), has: None, ..When::default() };
        assert!(stage(Some(Os::Macos), &[]).shows(&[When::default()]));
        assert!(stage(Some(Os::Macos), &[]).shows(&[half]));
    }

    /// A hidden field is dropped whole; a shown one keeps only the candidates that are themselves shown.
    #[test]
    fn the_visible_list_drops_hidden_fields_and_hidden_candidates() {
        let mut transport = ConfigField::new("transport", "経路");
        transport.field_type = FieldType::Multi;
        transport.options = vec![
            ConfigOption {
                value: "icloud".into(),
                label: "iCloud".into(),
                when: vec![When::on([Os::Macos])],
            },
            ConfigOption { value: "cloudflare".into(), label: "Cloudflare".into(), when: Vec::new() },
        ];
        let mut worker = ConfigField::new("worker_url", "Worker の URL");
        worker.when = vec![When::field_has("transport", "cloudflare")];

        let fields = [transport, worker];
        let on_windows = stage(Some(Os::Windows), &[("transport", "icloud")]);
        let shown = visible_fields(&fields, &on_windows);
        assert_eq!(shown.len(), 1, "the Cloudflare field is hidden while iCloud is the answer");
        assert_eq!(shown[0].key, "transport");
        assert_eq!(
            shown[0].options.iter().map(|o| o.value.as_str()).collect::<Vec<_>>(),
            ["cloudflare"],
            "the iCloud candidate is Mac-only and this is Windows",
        );

        let on_mac = stage(Some(Os::Macos), &[("transport", "cloudflare")]);
        let shown = visible_fields(&fields, &on_mac);
        assert_eq!(shown.len(), 2, "choosing Cloudflare brings its field out");
        assert_eq!(shown[0].options.len(), 2, "both candidates stand on a Mac");
    }

    /// An operation is filtered by the same reading its fields are.
    #[test]
    fn the_visible_operations_are_filtered_the_same_way() {
        let tunnel = SettingsAction {
            cmd: "tunnel".into(),
            label: "Cloudflare 経路を立てる".into(),
            ask: Vec::new(),
            when: vec![When::field_has("transport", "cloudflare")],
        };
        let actions = [tunnel];
        assert!(visible_actions(&actions, &stage(Some(Os::Macos), &[("transport", "icloud")])).is_empty());
        assert_eq!(
            visible_actions(&actions, &stage(Some(Os::Macos), &[("transport", "cloudflare")])).len(),
            1,
        );
    }
}
