//! The plugin manifest: one entry in the distribution catalog, describing a plugin well enough to
//! list it, judge it, and fetch it — without hitting a central server (`AMB-D-347`).
//!
//! A plugin is distributed as a manifest in a public **catalog repository**: a third party opens a PR
//! adding `plugins/<name>.yaml`, and CI aggregates the reviewed manifests into one `catalog.json` that
//! the GUI fetches once (`AMB-D-347`). This module defines the *shape* of one such entry — the type both
//! sides share. It does not read the catalog, fetch anything, or install: it is the schema the fetch
//! (`AMB-T-1979`), the aggregation CI (`AMB-T-1978`), and the provenance check (`AMB-T-1976`) all speak.
//!
//! ```json
//! {
//!   "name": "worktree", "desc": "Isolate each task in its own git worktree",
//!   "author": "amenbo", "repo": "ShiroDoromoto/amenbo-plugin-worktree",
//!   "os": ["macos", "linux"], "category": "workflow",
//!   "url": "https://github.com/.../worktree-v1.tar.gz", "checksum": "sha256:…",
//!   "official": true
//! }
//! ```
//!
//! **Lightweight by design** (`AMB-D-347`): an entry carries only what a browse view needs to list and
//! filter — name, description, author, source repo, supported OSes, category, and the official badge —
//! plus the `url`/`checksum` an install needs. Heavy numbers (stars, download counts) are deliberately
//! *not* here: they are fetched lazily for the one plugin a user opens, never for the whole catalog.
//!
//! **`official` is catalog-authoritative, not self-declared** (`AMB-D-347`): the badge means the author is
//! the amenbo team, decided by catalog curation (the PR review / the manifest's directory), never by a
//! third party ticking a box. The field lives here because the catalog is the shape, but *who may set it
//! true* is enforced upstream by the catalog CI and the validator — the type only supplies the safe
//! default (absent ⇒ `false`).
//!
//! **Validation lives elsewhere** (`AMB-D-354`). The manifest is untrusted third-party input, checked
//! fail-closed at the door (install / catalog intake) by a single validator — `AMB-T-1988`, which also
//! backs `plugin validate` for authors. This module is the type only: it enforces the *shape* (serde
//! rejects a manifest missing a required field), while the *rules* — a well-formed checksum, a name that
//! is not the reserved `registry` ([`config::is_reserved_plugin_name`](crate::config::is_reserved_plugin_name)),
//! a non-empty OS set — are the validator's, so the one truth about them lives in one place.
//!
//! Unknown keys are ignored rather than rejected: forward compatibility is handled by the version-compat
//! declaration a manifest will carry (`AMB-D-359` — target payload `v` and min amenbo version), which
//! gates a plugin gracefully instead of failing to parse a manifest a newer amenbo wrote. Denying unknown
//! fields would preempt that path (the same reasoning as the stored-blob schema in [`blob`](crate::blob)).

use serde::{Deserialize, Serialize};

/// An operating system a plugin runs on, in wharfy's vocabulary — the same tokens
/// [`update_check`](crate::update_check) uses (`std::env::consts::OS`), so a plugin's OS set and the
/// running platform compare directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Os {
    Macos,
    Windows,
    Linux,
}

impl Os {
    /// The wire token, matching `std::env::consts::OS`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Os::Macos => "macos",
            Os::Windows => "windows",
            Os::Linux => "linux",
        }
    }

    /// Parse a wire token back to an [`Os`]; `None` for anything else.
    pub fn parse(s: &str) -> Option<Os> {
        match s {
            "macos" => Some(Os::Macos),
            "windows" => Some(Os::Windows),
            "linux" => Some(Os::Linux),
            _ => None,
        }
    }
}

/// **What one switch turns this plugin on** (`AMB-D-379`) — declared by the author, because only the
/// author knows which one is meaningful for their plugin.
///
/// A user is never shown two enable switches for the same plugin. A notifier is answered per project ("do
/// I want this here"), while a plugin that watches the whole device has nothing a project could usefully
/// say about it — so amenbo asks for one answer, at the level this field names, and refuses the other. The
/// per-project *differences* a plugin needs beyond on/off are its **settings**, which have their own tiers
/// (`AMB-D-356`): "notify at all" is one switch, "to which channel" is a value a project may override.
///
/// Not to be confused with [`plugin_config::Scope`](crate::plugin_config::Scope), which names the tier one
/// config *value* is written at. This names what the *gate* is per.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Enabled per project — the default, and what most plugins want. A project that has not enabled it
    /// does not run it, and there is no device-wide answer to inherit.
    #[default]
    Project,
    /// Enabled once for the device. A project cannot override it: for a plugin whose work is not a
    /// project's (it watches the machine, or the store as a whole), a per-project answer would be a switch
    /// that looks like it does something and does not.
    Machine,
}

impl Scope {
    /// The wire token.
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Project => "project",
            Scope::Machine => "machine",
        }
    }
}

/// The payload contract version a manifest targets when it declares none: the v1 baseline (`AMB-D-349`).
/// A fixed literal, deliberately *not* [`crate::plugin_payload::VERSION`] — an omitted `payload_v` means
/// the plugin was written against the original contract, so it must not drift upward as amenbo bumps its
/// own `v`.
fn default_payload_v() -> u32 {
    1
}

/// One catalog entry: everything a browse view needs to list a plugin, plus what an install needs to
/// fetch it. See the module docs for the design (`AMB-D-347`) and the validation boundary (`AMB-D-354`).
///
/// The core descriptive fields are required — a manifest omitting one does not parse, which is the
/// shape half of the fail-closed door. The rest carry safe defaults for a manifest that omits them:
/// `official` ⇒ `false` (a badge no third party may self-grant), `payload_v` ⇒ the v1 baseline,
/// `min_amenbo` ⇒ no floor, and `config` ⇒ no settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The plugin's name — its identity in the catalog and its directory under `plugins/`. Must not be
    /// the reserved `registry` (the validator enforces this; see the module docs).
    pub name: String,
    /// A one-line description, for the list view.
    pub desc: String,
    /// Who wrote the plugin. For an official plugin this is the amenbo team; it is display text, not the
    /// authority on the official badge (that is `official`, set by the catalog).
    pub author: String,
    /// The plugin's source repository, `owner/name` — the GitHub coordinates a detail view reads stars
    /// and README from, lazily.
    pub repo: String,
    /// The operating systems the plugin supports. The validator requires this to be non-empty.
    pub os: Vec<Os>,
    /// The plugin's category, for filtering the catalog (e.g. `workflow`). A free label, not a closed
    /// set — the catalog curates the vocabulary.
    pub category: String,
    /// Where the plugin asset is fetched from on install.
    pub url: String,
    /// The asset's integrity digest (`sha256:<hex>`), verified on download against what `url` served and
    /// re-checked cheaply on every use of the on-disk asset (`AMB-D-351`). See [`crate::plugin_provenance`].
    pub checksum: String,
    /// The asset's minisign signature (the full `.minisig` text), produced by the catalog CI with the
    /// amenbo **catalog key** when the manifest is aggregated (`AMB-D-371`). Verified once on download
    /// against amenbo's embedded catalog public key ([`crate::plugin_provenance`]) — the origin half of
    /// provenance, next to `checksum`'s integrity half. Absent means unsigned: a third-party asset with no
    /// signature cannot be installed or enabled (`AMB-D-351`). An official plugin is signed too; its extra
    /// GitHub build-provenance attestation is a separate check, not this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// The official badge: the author is the amenbo team. Catalog-authoritative (`AMB-D-347`), never
    /// self-declared — absent means `false`.
    #[serde(default)]
    pub official: bool,
    /// Which switch enables this plugin — per project, or once for the device ([`Scope`], `AMB-D-379`).
    /// Absent means [`Scope::Project`], the answer that fits most plugins and the safe one: a project that
    /// has said nothing runs nothing. Declaring `machine` is the author saying a per-project answer would
    /// be meaningless for their plugin, and the faces then offer only the device-wide switch.
    ///
    /// A value outside the two is refused where every other shape error is (`AMB-D-354`): the manifest does
    /// not parse, so it never reaches the rules or the catalog.
    #[serde(default)]
    pub scope: Scope,
    /// The event-payload contract version this plugin reads (`AMB-D-349` — a single integer `v` for the
    /// whole contract, evolving additively). It lets amenbo notice when its own `v` has moved past what a
    /// plugin understands and warn or refuse rather than silently feed it a payload it cannot parse
    /// (`AMB-D-359`). Absent means the v1 baseline — a manifest written before this field targets the
    /// original contract, not whatever version the reading amenbo happens to be at. This module only
    /// *carries* the number; the enable/run-time comparison is [`crate::plugin_compat`]'s, not the type's.
    #[serde(default = "default_payload_v")]
    pub payload_v: u32,
    /// The minimum amenbo version this plugin needs, as a semver string — below it, amenbo warns or
    /// refuses to enable/run the plugin (`AMB-D-359`). Absent means no floor: the plugin declares no
    /// version requirement. Stored opaquely, like `checksum` — this module neither parses nor compares
    /// it; reading it is [`crate::plugin_compat`]'s, so the one truth about version ordering lives with
    /// the gate that acts on it (a string it cannot parse is a floor amenbo will not claim to meet).
    /// A value that reads as no version at all is refused earlier, at the manifest door
    /// ([`crate::plugin_validate`]), so it does not reach that gate through a fresh install.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_amenbo: Option<String>,
    /// The plugin's configuration schema: a flat list of fields the author declares so amenbo can
    /// render a form, store the values, and inject them at run time (`AMB-D-356`). Absent means the
    /// plugin takes no configuration — the safe default is an empty schema, so an older manifest with
    /// no `config` key is a plugin with no settings, not a parse error.
    ///
    /// The list is **the whole schema**: no types, no validation rules. amenbo does not judge what a
    /// value means (a URL, an email) — that is the author's job at run time. What amenbo reads here is
    /// only which fields exist, which are secret (so the store never sees them — `AMB-D-356`), and
    /// which are required (so `enable` is blocked until they are filled — `AMB-D-351`).
    ///
    /// An empty schema does not serialize (`skip_serializing_if`), so a re-emitted manifest for a plugin
    /// with no settings is byte-for-byte what an author who omitted `config` wrote — the absent and the
    /// empty forms stay the same document.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config: Vec<ConfigField>,
    /// The observation events this plugin subscribes to — the v1 event names
    /// ([`plugin_payload::V1_EVENTS`](crate::plugin_payload::V1_EVENTS)) it wants to be fired for. The
    /// subscription resolver (`AMB-D-367`, `AMB-T-2032`) fires an enabled plugin only for an event whose
    /// name appears here, so a plugin with no `events` observes nothing — a command-only plugin declares an
    /// empty list.
    ///
    /// **This module only carries the strings**; that each names a real v1 event is the validator's to
    /// enforce (`AMB-D-354` / `AMB-T-1988`) — the one home for the rules — and an unrecognised name is
    /// simply inert here, since only catalog events are ever fired. Absent means no subscription, and an
    /// empty list does not serialize, so a re-emitted manifest for a command-only plugin is byte-for-byte
    /// what an author who omitted `events` wrote (the same absent-equals-empty rule as `config`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
}

/// One field of a plugin's configuration schema (`AMB-D-356`). The author declares a flat list of these
/// in the manifest; amenbo renders each as one form field, routes its value to storage by the `secret`
/// flag, and injects it into the plugin at run time. **amenbo carries no notion of the field's type or
/// meaning** — there is no `type`, no pattern, no validation rule here. The only semantics amenbo acts on
/// are `secret` (where the value is stored and how it is injected) and `required` (whether an empty value
/// blocks `enable`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigField {
    /// The field's stable key — its identity in storage and the name it is injected under (the env var
    /// for a secret, the JSON key on stdin for the rest). Not shown to the user; `label` is.
    pub key: String,
    /// The human-readable label the form shows beside the field. Display text only.
    pub label: String,
    /// Whether the value is a secret. **The author declares this; amenbo does not judge it** (`AMB-D-356`)
    /// — amenbo cannot know a webhook URL is sensitive, so it trusts the flag. A secret is stored in the
    /// user-area secret file (never the store, never a backup) and injected as an environment variable
    /// (off argv, off logs); a non-secret is stored in the ordinary two tiers and injected on stdin.
    /// Absent means `false` — the safe-for-storage default is *not* secret only for a field the author
    /// left unmarked, which is a plain-text field by construction.
    #[serde(default)]
    pub secret: bool,
    /// Whether the field must hold a value before the plugin may be enabled (`AMB-D-351`, fail-closed).
    /// amenbo only checks presence (a non-empty value); it does not check the value is *valid*. Absent
    /// means `false`.
    #[serde(default)]
    pub required: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_json() -> serde_json::Value {
        serde_json::json!({
            "name": "worktree",
            "desc": "Isolate each task in its own git worktree",
            "author": "amenbo",
            "repo": "ShiroDoromoto/amenbo-plugin-worktree",
            "os": ["macos", "linux"],
            "category": "workflow",
            "url": "https://example.com/worktree-v1.tar.gz",
            "checksum": "sha256:deadbeef",
            "official": true,
            "scope": "project",
            "payload_v": 1,
            "min_amenbo": "1.8.0"
        })
    }

    #[test]
    fn a_full_entry_round_trips() {
        let m: Manifest = serde_json::from_value(full_json()).unwrap();
        assert_eq!(m.name, "worktree");
        assert_eq!(m.os, vec![Os::Macos, Os::Linux]);
        assert!(m.official);
        assert_eq!(m.payload_v, 1);
        assert_eq!(m.min_amenbo.as_deref(), Some("1.8.0"));
        // Re-serializing yields the same document.
        assert_eq!(serde_json::to_value(&m).unwrap(), full_json());
    }

    #[test]
    fn os_tokens_are_wharfy_vocabulary() {
        assert_eq!(Os::Macos.as_str(), "macos");
        assert_eq!(Os::Windows.as_str(), "windows");
        assert_eq!(Os::Linux.as_str(), "linux");
        assert_eq!(serde_json::to_value(Os::Macos).unwrap(), serde_json::json!("macos"));
        assert_eq!(Os::parse("linux"), Some(Os::Linux));
        assert_eq!(Os::parse("bsd"), None);
    }

    /// The enable scope is the author's declaration (`AMB-D-379`): absent means per project — the safe
    /// answer, since a project that has said nothing then runs nothing — and a value outside the two is a
    /// manifest that does not parse, which is where every other shape error is caught.
    #[test]
    fn the_enable_scope_defaults_to_project_and_rejects_anything_else() {
        let mut v = full_json();
        v.as_object_mut().unwrap().remove("scope");
        let m: Manifest = serde_json::from_value(v).unwrap();
        assert_eq!(m.scope, Scope::Project, "an undeclared scope is per project");

        let mut machine = full_json();
        machine["scope"] = serde_json::json!("machine");
        let m: Manifest = serde_json::from_value(machine).unwrap();
        assert_eq!(m.scope, Scope::Machine);
        assert_eq!(serde_json::to_value(&m).unwrap()["scope"], serde_json::json!("machine"));

        for bad in ["global", "workspace", "Project", ""] {
            let mut v = full_json();
            v["scope"] = serde_json::json!(bad);
            assert!(
                serde_json::from_value::<Manifest>(v).is_err(),
                "a scope outside the vocabulary must not parse: {bad}"
            );
        }
        assert_eq!(Scope::Project.as_str(), "project");
        assert_eq!(Scope::Machine.as_str(), "machine");
    }

    #[test]
    fn official_defaults_to_false_when_absent() {
        let mut v = full_json();
        v.as_object_mut().unwrap().remove("official");
        let m: Manifest = serde_json::from_value(v).unwrap();
        assert!(!m.official, "a manifest that does not claim official is not official");
    }

    #[test]
    fn a_missing_required_field_does_not_parse() {
        // The shape half of the fail-closed door: drop a required field and it fails to deserialize.
        for field in ["name", "desc", "author", "repo", "os", "category", "url", "checksum"] {
            let mut v = full_json();
            v.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<Manifest>(v).is_err(),
                "a manifest missing `{field}` must not parse"
            );
        }
    }

    #[test]
    fn an_unknown_os_does_not_parse() {
        let mut v = full_json();
        v["os"] = serde_json::json!(["macos", "haiku"]);
        assert!(serde_json::from_value::<Manifest>(v).is_err(), "an OS outside the vocabulary is rejected");
    }

    #[test]
    fn config_defaults_to_an_empty_schema_when_absent() {
        // A manifest with no `config` key is a plugin that takes no settings, not a parse error.
        let m: Manifest = serde_json::from_value(full_json()).unwrap();
        assert!(m.config.is_empty(), "no `config` key ⇒ no configuration schema");
    }

    #[test]
    fn a_config_schema_round_trips() {
        let mut v = full_json();
        v["config"] = serde_json::json!([
            { "key": "webhook_url", "label": "Slack Webhook URL", "secret": true, "required": true },
            { "key": "events", "label": "通知するイベント" }
        ]);
        let m: Manifest = serde_json::from_value(v.clone()).unwrap();
        assert_eq!(m.config.len(), 2);
        assert_eq!(m.config[0].key, "webhook_url");
        assert!(m.config[0].secret && m.config[0].required);
        // The second field declares neither flag: both default to false.
        assert_eq!(m.config[1].key, "events");
        assert!(!m.config[1].secret, "an unmarked field is not a secret");
        assert!(!m.config[1].required, "an unmarked field is not required");
        // Re-serializing a schema built from the parsed form yields the same document.
        assert_eq!(serde_json::to_value(&m).unwrap()["config"], serde_json::to_value(&m.config).unwrap());
    }

    #[test]
    fn a_config_field_missing_key_or_label_does_not_parse() {
        // key and label are the required half of a field's shape (secret/required default).
        for field in ["key", "label"] {
            let full = serde_json::json!({ "key": "k", "label": "L", "secret": true, "required": true });
            let mut one = full.clone();
            one.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<ConfigField>(one).is_err(),
                "a config field missing `{field}` must not parse"
            );
        }
    }

    #[test]
    fn events_default_to_no_subscription_when_absent_and_round_trip() {
        // A manifest with no `events` key subscribes to nothing — a command-only plugin, not a parse error.
        let m: Manifest = serde_json::from_value(full_json()).unwrap();
        assert!(m.events.is_empty(), "no `events` key ⇒ no subscription");
        // An empty list does not re-serialize, mirroring `config` (absent equals empty).
        assert!(serde_json::to_value(&m).unwrap().get("events").is_none());

        // A declared subscription round-trips verbatim.
        let mut v = full_json();
        v["events"] = serde_json::json!(["task.created", "comment.added"]);
        let m: Manifest = serde_json::from_value(v).unwrap();
        assert_eq!(m.events, vec!["task.created".to_string(), "comment.added".to_string()]);
        assert_eq!(
            serde_json::to_value(&m).unwrap()["events"],
            serde_json::json!(["task.created", "comment.added"])
        );
    }

    #[test]
    fn an_unknown_key_is_ignored_for_forward_compatibility() {
        // A field a newer amenbo added must not make an older one refuse the manifest.
        let mut v = full_json();
        v["some_future_field"] = serde_json::json!("whatever a later version wrote");
        let m: Manifest = serde_json::from_value(v).expect("unknown keys are tolerated");
        assert_eq!(m.name, "worktree");
    }

    #[test]
    fn the_compat_declaration_defaults_when_absent() {
        // A manifest written before the compat fields existed still parses: it targets the v1 payload
        // baseline and declares no amenbo-version floor. The default must be the fixed baseline, not the
        // reading amenbo's current `v` — an old plugin does not silently start claiming a newer contract.
        let mut v = full_json();
        v.as_object_mut().unwrap().remove("payload_v");
        v.as_object_mut().unwrap().remove("min_amenbo");
        let m: Manifest = serde_json::from_value(v).unwrap();
        assert_eq!(m.payload_v, 1, "an omitted payload_v is the v1 baseline");
        assert!(m.min_amenbo.is_none(), "an omitted min_amenbo is no version floor");
    }

    #[test]
    fn an_absent_min_amenbo_does_not_serialize() {
        // Absent and present-but-none stay one document: a plugin with no version floor re-emits without
        // the key, mirroring how an empty config schema does not serialize.
        let mut v = full_json();
        v.as_object_mut().unwrap().remove("min_amenbo");
        let m: Manifest = serde_json::from_value(v).unwrap();
        let out = serde_json::to_value(&m).unwrap();
        assert!(out.get("min_amenbo").is_none(), "no floor ⇒ no min_amenbo key on the way out");
    }
}
