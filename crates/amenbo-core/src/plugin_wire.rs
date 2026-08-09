//! **Which half of the catalog a manifest field is published in** (`AMB-D-385`) — the split itself, held
//! here so nobody else has to hold a copy of it.
//!
//! The catalog is delivered in two stages: a `catalog.json` everyone fetches once to draw the list, and a
//! `plugins/<name>.json` fetched only for the one plugin someone opened or is installing. A manifest field
//! therefore belongs to exactly one of two faces — [`ListEntry`], what a browse view draws, and [`Detail`],
//! what an install needs — and [`split`] is where that assignment is written down. [`join`] is the way
//! back: amenbo's own gates all read a whole manifest, so a client that fetched both halves puts them
//! together once rather than teaching each gate to read two documents.
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
//! **Three values on [`ListEntry`] are the catalog's, not the author's**, and amenbo emits them as empty
//! slots for the CI to fill: `added_at`, which is knowable only from the catalog repository's git history;
//! `detail_sum`, the digest of the detail document (`AMB-D-386`); and `featured`, the index's hand
//! curation (`AMB-D-347`). With the checksums a document away, `detail_sum` is what keeps update detection
//! on the one list fetch everyone already makes. Carrying the slots here is what keeps the CI from naming
//! its own fields after all.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::plugin_manifest::{
    AgentGuide, Asset, ConfigField, EventSubscription, Manifest, Os, Platform, Scope,
};

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
    /// The official badge. Catalog-authoritative (`AMB-D-347`): the official index's CI decides it, and a
    /// manifest that claims it for itself is refused at intake. A catalog anyone can publish holds no such
    /// review, so the merge a browse reads clears the badge on everything a registered catalog serves
    /// ([`crate::plugin_catalog::discover`]). Absent reads as `false`, the same safe default the manifest
    /// takes — the badge is a claim a reader must find, never one it assumes.
    #[serde(default)]
    pub official: bool,
    /// **The catalog's slot, emitted unset**: the recommendation, hand-curated on the official index
    /// (`AMB-D-347`). Unlike `official` — a fact about who wrote the plugin, which the CI can read off the
    /// repository — this is a judgement about the plugin itself, so nothing in a manifest can imply it and
    /// no owner test can grant it. It is the curator's, written beside the reviewed manifests rather than
    /// inside one, and a submitter therefore has no field to tick.
    #[serde(default)]
    pub featured: bool,
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
/// correctly: what layer it lives at, what it subscribes to, what it can be configured with, and which
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
    /// What layer the plugin lives at — a project's, or the device's (`AMB-D-601`). It rides in the detail
    /// rather than the list because it is read where a plugin is taken on and run, not where a browse view
    /// draws a row.
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
    /// What the plugin says for itself at the AI's entry point (`AMB-D-437`).
    ///
    /// It rides here rather than in the list because it is read on the machine the plugin is installed and
    /// enabled on, from the copy kept beside the binary — never while browsing, where `desc` is the line
    /// a reader is shown. Putting it in the list would charge every reader for every plugin's usage notes
    /// on a fetch that only draws names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentGuide>,
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
        featured: false,
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
        agent: manifest.agent.clone(),
    };
    (entry, detail)
}

/// Put the two documents the catalog serves back together as the one manifest amenbo works from
/// (`AMB-D-385`) — the reverse of [`split`], and the shape every gate downstream already speaks.
///
/// An install reads the list once and the detail for the one plugin it is installing; from there on the
/// platform resolution, the provenance check, the compatibility gate and the record written beside the
/// binary all want a whole manifest, so the join happens once here rather than each of them learning to
/// read two documents. `detail_sum` rides along from the entry, which is what makes the record beside the
/// binary say which detail it came from (`AMB-D-386`).
///
/// **The join is not a door.** The two halves are untrusted delivery, and joining them judges nothing:
/// the caller runs [`crate::plugin_validate::validate_manifest`] over the result before anything is
/// fetched or written. That the detail is the one the entry asked for — same `name` — is checked where it
/// is fetched ([`crate::plugin_catalog::detail`]), since that is where the question is asked.
pub fn join(entry: &ListEntry, detail: &Detail) -> Manifest {
    Manifest {
        name: entry.name.clone(),
        desc: entry.desc.clone(),
        author: entry.author.clone(),
        repo: entry.repo.clone(),
        os: entry.os.clone(),
        category: entry.category.clone(),
        official: entry.official,
        detail_sum: entry.detail_sum.clone(),
        url: detail.url.clone(),
        checksum: detail.checksum.clone(),
        signature: detail.signature.clone(),
        assets: detail.assets.clone(),
        scope: detail.scope,
        payload_v: detail.payload_v,
        min_amenbo: detail.min_amenbo.clone(),
        config: detail.config.clone(),
        events: detail.events.clone(),
        agent: detail.agent.clone(),
    }
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
            "detail_sum": format!("sha256:{}", "3".repeat(64)),
            "scope": "machine",
            "payload_v": 1,
            "min_amenbo": "1.8.0",
            "config": [{ "key": "base", "label": "Base branch", "secret": false, "required": false }],
            "events": [{ "event": "task.status_changed", "faces": ["cli"], "reply": true }],
            "agent": {
                "when": "Starting work on a task that will produce commits",
                "commands": [{ "cmd": "start <task-id>", "does": "Cuts a worktree and returns the cd line" }],
            },
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

        // And nothing is invented on the way out beyond the slots the catalog fills.
        for field in entry_keys.iter().chain(detail_keys.iter()) {
            assert!(
                manifest_keys.contains(field)
                    || ["featured", "added_at", "detail_sum"].contains(&field.as_str()),
                "published field {field:?} is neither a manifest field nor a catalog slot",
            );
        }
    }

    /// The values land where they belong, and the catalog's own slots come out empty for the CI to fill.
    #[test]
    fn the_list_entry_draws_and_the_detail_installs() {
        let (entry, detail) = split(&full());

        assert_eq!(entry.name, "worktree");
        assert_eq!(entry.desc, "Isolate each task in its own git worktree");
        assert_eq!(entry.os, vec![Os::Macos, Os::Linux]);
        assert!(entry.official, "the badge rides in the list, where the badge is drawn");
        assert!(!entry.featured, "the catalog's, curated by hand — never claimed by a manifest");
        assert_eq!(entry.added_at, None, "the catalog's, from git history");
        assert_eq!(
            entry.detail_sum, None,
            "the catalog's, over the bytes it publishes — a value on the way in is not published"
        );

        assert_eq!(detail.name, "worktree", "the join back to the entry that named it");
        assert_eq!(detail.checksum, "sha256:".to_string() + &"0".repeat(64));
        assert!(detail.signature.is_some());
        assert_eq!(detail.assets.len(), 1);
        assert_eq!(detail.scope, Scope::Machine, "the layer installs, and is not a row a list draws");
        assert_eq!(detail.payload_v, 1);
        assert_eq!(detail.min_amenbo.as_deref(), Some("1.8.0"));
        assert_eq!(detail.config.len(), 1);
        assert_eq!(detail.events.len(), 1);
        assert_eq!(detail.events[0].faces, vec![Face::Cli]);
        assert!(detail.events[0].reply, "and the reply the subscription asked for survives the split");
        let agent = detail.agent.as_ref().expect("what the plugin says for itself installs, not browses");
        assert_eq!(agent.commands[0].cmd, "start <task-id>", "the author's face, prefix-free");
    }

    /// **The two documents put back together are the manifest they came from.** What an install works
    /// from is the join, so a value that survives the split and is then dropped by the join would be lost
    /// exactly where it is needed. The one deliberate difference is the catalog's own slot: `detail_sum`
    /// comes back as the entry carried it, which is empty until the CI fills it in.
    #[test]
    fn joining_the_two_documents_gives_back_the_manifest() {
        let manifest = full();
        let (entry, detail) = split(&manifest);

        assert_eq!(join(&entry, &detail), Manifest { detail_sum: None, ..manifest });
    }

    /// **The door and the author's tool answer the same about the layer** (`AMB-D-601`). They meet the
    /// declaration in different documents — an install reads a [`Detail`] off the catalog, `plugin validate`
    /// reads a whole [`Manifest`] off the author's disk — but both read it through the one `Scope` type, so
    /// there is no second vocabulary to drift. An author whose manifest passes therefore knows an install
    /// will not refuse it over this, and a token outside the two is refused on both roads before any rule
    /// is ever asked.
    #[test]
    fn the_two_roads_in_read_the_layer_the_same_way() {
        for (written, expected) in [("project", Scope::Project), ("machine", Scope::Machine)] {
            let mut m = serde_json::to_value(full()).unwrap();
            m["scope"] = serde_json::json!(written);
            let manifest: Manifest = serde_json::from_value(m).expect("the author's road");
            let (_, detail) = split(&manifest);
            let detail: Detail =
                serde_json::from_value(serde_json::to_value(&detail).unwrap()).expect("the door's");
            assert_eq!((manifest.scope, detail.scope), (expected, expected));
        }

        for unknown in ["global", "workspace", "device", ""] {
            let mut m = serde_json::to_value(full()).unwrap();
            m["scope"] = serde_json::json!(unknown);
            assert!(
                serde_json::from_value::<Manifest>(m).is_err(),
                "`plugin validate` refuses '{unknown}'"
            );

            let mut d = serde_json::to_value(split(&full()).1).unwrap();
            d["scope"] = serde_json::json!(unknown);
            assert!(
                serde_json::from_value::<Detail>(d).is_err(),
                "and the door refuses '{unknown}' too, for the same reason"
            );
        }
    }

    /// The digest the catalog put on the entry is what the joined manifest records, which is what makes an
    /// install able to say later which detail document it was installed from (`AMB-D-386`).
    #[test]
    fn the_joined_manifest_records_the_detail_it_was_joined_with() {
        let (mut entry, detail) = split(&full());
        entry.detail_sum = Some(format!("sha256:{}", "4".repeat(64)));

        assert_eq!(join(&entry, &detail).detail_sum, entry.detail_sum);
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
        for absent in ["signature", "assets", "min_amenbo", "config", "events", "agent"] {
            assert!(detail_json.get(absent).is_none(), "{absent} was not written, so it is not emitted");
        }
        assert_eq!(detail_json["scope"], "project", "the default the author relied on is still stated");
        assert_eq!(detail_json["payload_v"], 1);

        let entry_json = serde_json::to_value(&entry).unwrap();
        assert!(entry_json["added_at"].is_null(), "the slot is present and empty");
        assert!(entry_json["detail_sum"].is_null());
        assert_eq!(entry_json["featured"], false, "the slot is present and unset");
    }

    /// **The curation cannot be self-granted.** A submitter who writes `featured: true` into their own
    /// manifest is writing a key amenbo does not read, so the entry the catalog publishes still says
    /// `false` — the recommendation is the curator's, and there is no field on this path for anyone else
    /// to set. `official` needs the CI to refuse a claim because the manifest carries it; this one is
    /// unreachable from a manifest at all.
    #[test]
    fn a_manifest_claiming_the_recommendation_is_published_without_it() {
        let mut claimed = serde_json::to_value(full()).unwrap();
        claimed["featured"] = serde_json::json!(true);
        let manifest: Manifest = serde_json::from_value(claimed).expect("unknown keys are ignored");

        let (entry, _) = split(&manifest);
        assert!(!entry.featured, "what the author wrote never reached the entry");
    }
}
