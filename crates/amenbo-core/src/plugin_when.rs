//! **When a declared thing is shown** (`AMB-D-727`) — the condition an author writes on a settings field,
//! on one of its candidates, or on an operation, and the reading of it.
//!
//! The settings form used to draw a manifest's fields top to bottom, all of them, always. So a Windows
//! user was offered an iCloud transport, and someone who had not chosen Cloudflare still had Cloudflare's
//! three fields to scroll past. Neither is something the author could fix from their side: there was
//! nowhere to say "only when".
//!
//! **The reading splits where the facts live** (`AMB-D-727`). A condition asks two kinds of question, and
//! they are answered in different places:
//!
//! - **the platform** is core's, because a face does not know which OS this build runs on and core does,
//!   and because the answer cannot change while the program runs. [`after_platform`] settles it once, and
//!   what it hides is gone before any face sees it;
//! - **another setting's answer** is settled wherever the answers are. At the gate that is the store
//!   ([`Stage`], [`crate::plugin_config::stage`]); while a form is open it is the form, whose boxes hold
//!   answers the store has not been told about yet — someone ticking Cloudflare expects its fields the
//!   same moment, not after a save.
//!
//! So a face is handed the platform's verdict already applied and [`OnAnswer`] for the rest, rather than a
//! list it has to judge whole or a question it has to ask again on every keystroke.
//!
//! **What a condition hides is the field, never the value.** A value saved on a Mac is still there when the
//! same store is opened on Windows, and still handed to the plugin — hiding a box is a statement about the
//! screen, not a reason to throw an answer away. The one place hiding does decide something is `required`:
//! a hidden field that is empty must not shut the enable gate, because the form has nowhere to show the
//! user what is missing ([`crate::plugin_trust::missing_required`]).
//!
//! The type lives here rather than beside the rest of the manifest's shapes because it is the one that
//! carries a reading: [`Stage`] is what a condition is judged against, and keeping the two together is what
//! keeps the store-side reading from drifting from the declaration it reads.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::plugin_manifest::Os;

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
    /// The key of the field whose value this clause reads — a [`crate::plugin_manifest::ConfigField::key`] of the same manifest.
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
    /// one, keyed by [`crate::plugin_manifest::ConfigField::key`]. This is what every caller reading a real store wants;
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

/// **One condition on another setting's answer** (`AMB-D-727`) — what is left of a `when` once the
/// platform has answered its half ([`after_platform`]).
///
/// It is the half that keeps moving. A platform is settled for as long as the program runs, and an answer
/// changes under the user's fingers: someone ticking Cloudflare on a settings form expects its three
/// fields the same moment, before anything is saved and before the store has heard about it. So this is
/// the shape a face is handed, to re-read against the answers it is holding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnAnswer {
    /// The key of the setting whose answer this reads.
    pub field: String,
    /// The value looked for among that setting's answers.
    pub has: String,
}

/// **The platform's half of the reading, settled once** (`AMB-D-727`).
///
/// `None` is a thing this build's platform hides outright — a Windows machine has no use for an iCloud
/// candidate, and nothing that happens on the form will change that. `Some(rest)` is what is left to judge:
/// the conditions that read another setting, handed on for whoever holds the answers.
///
/// **This is what "core does the filtering" means here.** A face does not know which OS it is running on,
/// and core does, so the platform is decided where the fact lives and a face never learns an OS name. The
/// half that is left is decided where the answers live — which, while a form is open, is the form: the
/// store has not been told yet, so asking core would be asking about the answers of a moment ago.
///
/// A clause naming neither kind is dropped rather than obeyed, for the reason [`When`] gives: it decides
/// nothing, and what a face cannot judge it must not hide something over. The validator refuses it at the
/// author's desk ([`crate::plugin_validate`]).
pub fn after_platform(when: &[When]) -> Option<Vec<OnAnswer>> {
    let here = Os::here();
    let mut rest = Vec::new();
    for clause in when {
        if !clause.os.is_empty() && !here.is_some_and(|os| clause.os.contains(&os)) {
            return None;
        }
        if let (Some(field), Some(has)) = (&clause.field, &clause.has) {
            rest.push(OnAnswer { field: field.clone(), has: has.clone() });
        }
    }
    Some(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The platform decides its half here and hands the rest on: a thing this platform hides is gone, and
    /// what survives carries only the conditions that read an answer.
    #[test]
    fn the_platform_is_settled_and_the_answer_half_is_handed_on() {
        let here = Os::here().expect("this build runs on a platform Amenbo names");
        let elsewhere = if here == Os::Windows { Os::Macos } else { Os::Windows };

        assert_eq!(after_platform(&[]), Some(Vec::new()), "nothing declared survives with nothing left");
        assert_eq!(after_platform(&[When::on([elsewhere])]), None, "another platform's is gone");
        assert_eq!(
            after_platform(&[When::on([here])]),
            Some(Vec::new()),
            "this platform's held, and left nothing behind to judge",
        );
        assert_eq!(
            after_platform(&[When::on([here]), When::field_has("transport", "cloudflare")]),
            Some(vec![OnAnswer { field: "transport".into(), has: "cloudflare".into() }]),
            "the answer half is what a face is handed",
        );
    }

    /// A clause a face could not judge is dropped rather than obeyed — the same fail-open the reading
    /// keeps, so an author's mistake never looks like a field that was never declared.
    #[test]
    fn a_clause_that_names_neither_kind_is_not_handed_on() {
        let half = When { field: Some("transport".into()), has: None, ..When::default() };
        assert_eq!(after_platform(&[When::default(), half]), Some(Vec::new()));
    }
}
