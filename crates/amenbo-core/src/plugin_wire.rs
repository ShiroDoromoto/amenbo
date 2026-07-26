//! **Which half of the catalog a manifest field is published in** (`AMB-D-385`) — the split itself, held
//! here so nobody else has to hold a copy of it.
//!
//! The catalog is delivered in two stages: a `catalog.json` everyone fetches once to draw the list, and a
//! `plugins/<name>.json` fetched only for the one plugin someone opened or is installing. A manifest field
//! therefore belongs to exactly one of two faces — [`ListEntry`], what a browse view draws, and [`Detail`],
//! what an install needs — and [`split`] is where that assignment is written down.
//!
//! **The line lives in amenbo, not in the aggregator.** The catalog repository's CI calls
//! `plugin validate --json` and publishes what comes back, so it names no manifest field of its own. A
//! field it had to name would be a field it can fail to name: a copy list in the aggregator drops whatever
//! amenbo adds after the list was written, silently, and an aggregator deciding which half a field belongs
//! in is that same list one fork further along. So amenbo emits the two documents and the CI publishes them.
//!
//! A field amenbo adds later must be assigned, and `every_manifest_field_is_published_exactly_once` (below)
//! is what refuses to let it be forgotten: it reads the serialized manifest's own keys and fails unless each
//! one appears in exactly one face.
//!
//! **Two values on [`ListEntry`] are the catalog's, not the author's**, and amenbo emits them as empty slots
//! for the CI to fill: `added_at`, which is knowable only from the catalog repository's git history, and
//! `detail_sum`, the digest of the detail document (`AMB-D-386`). With the checksums a document away,
//! `detail_sum` is what keeps update detection on the one list fetch everyone already makes. Carrying the
//! slots here is what keeps the CI from naming its own fields after all.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::plugin_manifest::{Asset, ConfigField, EventSubscription, Manifest, Os, Platform, Scope};

/// **What a browse view draws** — the half of a manifest that rides in `catalog.json`, which everyone
/// fetches whole (`AMB-D-385`). Nothing an install needs is here, and that is the point: the signature
/// alone is larger than all of this, and every reader would pay for it on every plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListEntry {
    /// The plugin's name — its identity, and the key the detail document is fetched by.
    pub name: String,
    /// The one-line description the list shows.
    pub desc: String,
    /// Who wrote the plugin, as display text.
    pub author: String,
    /// The plugin's source repository, `owner/name` — the coordinates a detail view reads stars and
    /// README from, lazily.
    pub repo: String,
    /// The operating systems the plugin supports, which a list filters and greys out by.
    pub os: Vec<Os>,
    /// The plugin's category, which a list filters by.
    pub category: String,
    /// The official badge. Catalog-authoritative (`AMB-D-347`): the CI decides it, and a manifest that
    /// claims it for itself is refused at intake.
    pub official: bool,
    /// **The catalog's slot, emitted empty**: the day this manifest first appeared in the index, from the
    /// catalog repository's git history. A client holds no such history, so the CI is the only thing that
    /// can answer, and a missing value means unknown rather than old.
    pub added_at: Option<String>,
    /// **The catalog's slot, emitted empty**: the digest of this plugin's [`Detail`] document
    /// (`AMB-D-386`). Update detection compares it against what the installed copy recorded, so a changed
    /// install stays detectable from the one list fetch even though the checksums themselves now live a
    /// document away. The CI computes it over the bytes it publishes, which is why amenbo cannot fill it
    /// here — the detail document is not yet written when a manifest is validated.
    pub detail_sum: Option<String>,
}

/// **What an install needs** — the half of a manifest that rides in `plugins/<name>.json`, fetched for one
/// plugin at a time (`AMB-D-385`). The signature and checksums live here because they are needed *before*
/// the asset is downloaded and so can never travel inside it, and the rest is what the plugin has to be run
/// correctly: which switch enables it, what it subscribes to, what it can be configured with, and which
/// contract versions it speaks.
///
/// Every field is optional or defaulted, exactly as on [`Manifest`], so a detail document round-trips what
/// the author wrote rather than spelling out defaults they omitted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detail {
    /// The name the list entry pairs with — the join between the two documents, and the check that a
    /// detail fetched by name is the one that was asked for.
    pub name: String,
    /// The single distributable that serves every OS the entry lists, for a plugin that is one file
    /// everywhere (`AMB-D-381`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// That asset's integrity digest.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub checksum: String,
    /// That asset's minisign signature, produced by the catalog CI with amenbo's catalog key
    /// (`AMB-D-371`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// One distributable per platform, for a plugin built per platform (`AMB-D-381`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assets: BTreeMap<Platform, Asset>,
    /// Which switch enables the plugin — per project, or once for the device (`AMB-D-379`).
    #[serde(default)]
    pub scope: Scope,
    /// The payload contract version the plugin reads (`AMB-D-349`).
    pub payload_v: u32,
    /// The minimum amenbo version the plugin needs (`AMB-D-359`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_amenbo: Option<String>,
    /// The plugin's configuration schema (`AMB-D-356`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config: Vec<ConfigField>,
    /// The observation events the plugin subscribes to (`AMB-D-383`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<EventSubscription>,
}

/// Publish one validated manifest as the two documents the catalog serves (`AMB-D-385`).
///
/// `name` is the only field in both: the list entry is fetched by it and the detail document is verified to
/// be the one it was asked for, so leaving it out of either would break the join. Every other field goes to
/// exactly one side.
///
/// The manifest is expected to have passed [`crate::plugin_validate`] first. Splitting does not judge —
/// it moves values — so an invalid manifest would split just as happily into two invalid documents; the
/// door is what keeps one from being published, not this.
pub fn split(manifest: &Manifest) -> (ListEntry, Detail) {
    let entry = ListEntry {
        name: manifest.name.clone(),
        desc: manifest.desc.clone(),
        author: manifest.author.clone(),
        repo: manifest.repo.clone(),
        os: manifest.os.clone(),
        category: manifest.category.clone(),
        official: manifest.official,
        added_at: None,
        detail_sum: None,
    };
    let detail = Detail {
        name: manifest.name.clone(),
        url: manifest.url.clone(),
        checksum: manifest.checksum.clone(),
        signature: manifest.signature.clone(),
        assets: manifest.assets.clone(),
        scope: manifest.scope,
        payload_v: manifest.payload_v,
        min_amenbo: manifest.min_amenbo.clone(),
        config: manifest.config.clone(),
        events: manifest.events.clone(),
    };
    (entry, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_manifest::Face;

    /// A manifest exercising every field, so a split that drops one is visible rather than merely
    /// unrepresented. Both distributable forms are written at once — which is not a shape the door accepts,
    /// but this is about where a value lands, not whether the document is legal.
    fn full() -> Manifest {
        serde_json::from_value(serde_json::json!({
            "name": "worktree",
            "desc": "Isolate each task in its own git worktree",
            "author": "amenbo",
            "repo": "alice/amenbo-plugin-worktree",
            "os": ["macos", "linux"],
            "category": "workflow",
            "url": "https://example.test/worktree.tar.gz",
            "checksum": format!("sha256:{}", "0".repeat(64)),
            "signature": "untrusted comment: signature\nRWQ=\n",
            "assets": {
                "macos": {
                    "url": "https://example.test/worktree-macos.tar.gz",
                    "checksum": format!("sha256:{}", "1".repeat(64)),
                }
            },
            "official": true,
            "scope": "machine",
            "payload_v": 1,
            "min_amenbo": "1.8.0",
            "config": [{ "key": "base", "label": "Base branch", "secret": false, "required": false }],
            "events": [{ "event": "task.status_changed", "faces": ["cli"], "reply": true }],
        }))
        .expect("the fixture is a manifest")
    }

    /// **The guard on the split.** Every field amenbo serializes on a manifest is published in exactly one
    /// face, so a field added later can reach neither document nor both only over this test's dead body.
    /// The keys are read off the serialized shapes rather than listed here, because a list here would be
    /// one more copy to forget — which is the failure the split exists to close.
    #[test]
    fn every_manifest_field_is_published_exactly_once() {
        let manifest = full();
        let (entry, detail) = split(&manifest);

        let keys = |v: serde_json::Value| {
            v.as_object().expect("a manifest serializes as an object").keys().cloned().collect::<Vec<_>>()
        };
        let entry_keys = keys(serde_json::to_value(&entry).unwrap());
        let detail_keys = keys(serde_json::to_value(&detail).unwrap());

        let manifest_keys = keys(serde_json::to_value(&manifest).unwrap());
        for field in &manifest_keys {
            let in_entry = entry_keys.contains(field);
            let in_detail = detail_keys.contains(field);
            if field == "name" {
                assert!(in_entry && in_detail, "`name` is the join and belongs to both documents");
                continue;
            }
            assert!(in_entry || in_detail, "manifest field {field:?} reaches neither — assign it in split()");
            assert!(!(in_entry && in_detail), "manifest field {field:?} reaches both — only `name` joins them");
        }

        // And nothing is invented on the way out beyond the two slots the catalog fills.
        for field in entry_keys.iter().chain(detail_keys.iter()) {
            assert!(
                manifest_keys.contains(field) || ["added_at", "detail_sum"].contains(&field.as_str()),
                "published field {field:?} is neither a manifest field nor a catalog slot",
            );
        }
    }

    /// The values land where they belong, and the catalog's two slots come out empty for the CI to fill.
    #[test]
    fn the_list_entry_draws_and_the_detail_installs() {
        let (entry, detail) = split(&full());

        assert_eq!(entry.name, "worktree");
        assert_eq!(entry.desc, "Isolate each task in its own git worktree");
        assert_eq!(entry.os, vec![Os::Macos, Os::Linux]);
        assert!(entry.official, "the badge rides in the list, where the badge is drawn");
        assert_eq!(entry.added_at, None, "the catalog's, from git history");
        assert_eq!(entry.detail_sum, None, "the catalog's, over the bytes it publishes");

        assert_eq!(detail.name, "worktree", "the join back to the entry that named it");
        assert_eq!(detail.checksum, "sha256:".to_string() + &"0".repeat(64));
        assert!(detail.signature.is_some());
        assert_eq!(detail.assets.len(), 1);
        assert_eq!(detail.scope, Scope::Machine);
        assert_eq!(detail.payload_v, 1);
        assert_eq!(detail.min_amenbo.as_deref(), Some("1.8.0"));
        assert_eq!(detail.config.len(), 1);
        assert_eq!(detail.events.len(), 1);
        assert_eq!(detail.events[0].faces, vec![Face::Cli]);
        assert!(detail.events[0].reply, "and the reply the subscription asked for survives the split");
    }

    /// A manifest that omits its optional fields splits into documents that omit them too, so what the
    /// catalog publishes is what the author wrote rather than defaults spelled out on their behalf. The two
    /// slots are the exception: they are emitted as `null`, because a key the CI can see is what keeps it
    /// from having to know the names itself.
    #[test]
    fn what_the_author_left_out_stays_out_and_the_slots_stay_visible() {
        let bare: Manifest = serde_json::from_value(serde_json::json!({
            "name": "minimal",
            "desc": "A plugin with nothing optional",
            "author": "Alice",
            "repo": "alice/minimal",
            "os": ["linux"],
            "category": "workflow",
            "url": "https://example.test/minimal.tar.gz",
            "checksum": format!("sha256:{}", "2".repeat(64)),
        }))
        .unwrap();
        let (entry, detail) = split(&bare);

        let detail_json = serde_json::to_value(&detail).unwrap();
        for absent in ["signature", "assets", "min_amenbo", "config", "events"] {
            assert!(detail_json.get(absent).is_none(), "{absent} was not written, so it is not emitted");
        }
        assert_eq!(detail_json["scope"], "project", "the default the author relied on is still stated");
        assert_eq!(detail_json["payload_v"], 1);

        let entry_json = serde_json::to_value(&entry).unwrap();
        assert!(entry_json["added_at"].is_null(), "the slot is present and empty");
        assert!(entry_json["detail_sum"].is_null());
    }
}
