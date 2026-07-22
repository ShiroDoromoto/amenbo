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

/// One catalog entry: everything a browse view needs to list a plugin, plus what an install needs to
/// fetch it. See the module docs for the design (`AMB-D-347`) and the validation boundary (`AMB-D-354`).
///
/// Every field but `official` is required — a manifest omitting one does not parse, which is the
/// shape half of the fail-closed door. `official` defaults to `false` when absent, the safe default for
/// a badge no third party may self-grant.
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
    /// The asset's integrity digest, verified on download against what `url` served (`AMB-D-351`); a
    /// third-party plugin additionally requires a minisign signature at that point.
    pub checksum: String,
    /// The official badge: the author is the amenbo team. Catalog-authoritative (`AMB-D-347`), never
    /// self-declared — absent means `false`.
    #[serde(default)]
    pub official: bool,
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
            "official": true
        })
    }

    #[test]
    fn a_full_entry_round_trips() {
        let m: Manifest = serde_json::from_value(full_json()).unwrap();
        assert_eq!(m.name, "worktree");
        assert_eq!(m.os, vec![Os::Macos, Os::Linux]);
        assert!(m.official);
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
    fn an_unknown_key_is_ignored_for_forward_compatibility() {
        // A field a newer amenbo added (see `AMB-D-359`) must not make an older one refuse the manifest.
        let mut v = full_json();
        v["min_amenbo"] = serde_json::json!("2.0.0");
        let m: Manifest = serde_json::from_value(v).expect("unknown keys are tolerated");
        assert_eq!(m.name, "worktree");
    }
}
