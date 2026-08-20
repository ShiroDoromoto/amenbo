//! **Which half of the catalog a manifest field is published in** (`AMB-D-385`) — the split itself, held
//! here so nobody else has to hold a copy of it.
//!
//! The catalog is delivered in two stages: a `catalog.json` everyone fetches once to draw the list, and a
//! `plugins/<name>.json` fetched only for the one plugin someone opened or is installing. A manifest field
//! therefore belongs to exactly one of two faces — [`ListEntry`], what a browse view draws, and [`Detail`],
//! what an install needs — and [`split`] is where that assignment is written down. [`join`] is the way
//! back: Amenbo's own gates all read a whole manifest, so a client that fetched both halves puts them
//! together once rather than teaching each gate to read two documents.
//!
//! **The line lives in Amenbo, not in the aggregator.** The catalog repository's CI calls
//! `plugin validate --json` and publishes what comes back, so it names no manifest field of its own. A
//! field it had to name would be a field it can fail to name: a copy list in the aggregator drops whatever
//! Amenbo adds after the list was written, silently, and an aggregator deciding which half a field belongs
//! in is that same list one fork further along. So Amenbo emits the two documents and the CI publishes them.
//!
//! A field Amenbo adds later must be assigned, and `every_manifest_field_is_published_exactly_once` (below)
//! is what refuses to let it be forgotten: it reads the serialized manifest's own keys and fails unless each
//! one appears in exactly one face.
//!
//! **The translations are delivered along the same two faces, carried differently** (`AMB-D-622`). A
//! translated field rides on the face its base field rides on — that is the whole rule, and
//! `every_translatable_field_is_published_on_its_base_field_face` (below) holds it — but how it travels
//! differs, because the two faces are fetched differently. The list half comes back from [`split`] as one
//! [`ListEntryOverlay`] per language, for the CI to key by plugin name into a `catalog.<lang>.json` beside
//! the list everyone fetches: a reader then pays for their own language and no one else's. The detail half
//! rides *inside* [`Detail`] as [`Detail::i18n`], every language at once, because a detail is fetched for
//! one plugin at a time, kept beside the binary, and read offline — and because the entry carries one
//! `detail_sum` (`AMB-D-386`), which splitting the detail per language would turn into nineteen.
//!
//! **Three values on [`ListEntry`] are the catalog's, not the author's**, and Amenbo emits them as empty
//! slots for the CI to fill: `added_at`, which is knowable only from the catalog repository's git history;
//! `detail_sum`, the digest of the detail document (`AMB-D-386`); and `featured`, the index's hand
//! curation (`AMB-D-347`). With the checksums a document away, `detail_sum` is what keeps update detection
//! on the one list fetch everyone already makes. Carrying the slots here is what keeps the CI from naming
//! its own fields after all.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::plugin_manifest::{
    AgentGuide, Asset, ConfigField, ConfigFieldOverlay, EventSubscription, Manifest, Os, Platform,
    Scope, Settings, SettingsOverlay, Translations,
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
    /// document away. The CI computes it over the bytes it publishes, which is why Amenbo cannot fill it
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
    /// That asset's minisign signature, produced by the catalog CI with Amenbo's catalog key
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
    /// The minimum Amenbo version the plugin needs (`AMB-D-359`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_amenbo: Option<String>,
    /// The plugin's configuration schema (`AMB-D-356`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config: Vec<ConfigField>,
    /// Where the plugin's own code is called from the settings face (`AMB-D-664`).
    ///
    /// It rides here for the reason the schema it stands beside does: it is read where the plugin is
    /// enabled and configured, on the machine it was installed on, and a browse view draws none of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Settings>,
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
    /// What the plugin is, in the author's own words (`AMB-D-638`) — the Markdown a detail view draws.
    ///
    /// It rides here rather than in the list for the reason everything else here does: it is read on
    /// the one plugin someone opened, while a browse view draws `desc`. Charging every reader for every
    /// plugin's paragraphs on the fetch that only draws names is what `AMB-D-385` split the two
    /// documents to avoid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    /// **What this plugin says in every language its author wrote** (`AMB-D-622`), keyed by language
    /// code — the detail half of the translations, which is the description text and the configuration
    /// labels.
    ///
    /// All of them ride here rather than one per document because this document is fetched for one
    /// plugin at a time and then kept beside the binary: a settings form opened offline, or a detail
    /// view read after the user changed language, has the words already. Absent means the author wrote
    /// none, which is what a manifest with no overlay beside it splits into.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub i18n: BTreeMap<String, DetailOverlay>,
}

/// **The list half of one language's overlay** — what a browse view draws, translated (`AMB-D-622`).
///
/// It is published outside [`ListEntry`], not inside it: the CI keys one of these per plugin into a
/// `catalog.<lang>.json` beside `catalog.json`, so a reader fetches their own language and nobody pays
/// for the other eighteen. A language nobody translated the list half of has no document, and the 404 a
/// fetch gets is the answer rather than an error.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListEntryOverlay {
    /// The one-line description in this language. Optional for the same reason it is optional on the
    /// overlay the author wrote: what is not translated falls back to the base line (`AMB-D-623`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
}

/// **The detail half of one language's overlay** — what a detail view and a settings form read,
/// translated (`AMB-D-622`). It rides inside [`Detail::i18n`], one entry per language.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetailOverlay {
    /// The description text in this language (`AMB-D-638`). Optional for the same reason the line is:
    /// what is not translated falls back to the base text (`AMB-D-623`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    /// The configuration form's labels in this language, keyed by field key as the author wrote them
    /// (`AMB-D-621`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config: BTreeMap<String, ConfigFieldOverlay>,
    /// The settings block's buttons in this language, keyed by the call each raises (`AMB-D-664`). It
    /// rides here because the block it translates does: both are read on the form of a plugin someone
    /// installed, never while browsing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<SettingsOverlay>,
}

/// Publish one validated manifest, and what its author wrote it as in other languages, as the documents
/// the catalog serves (`AMB-D-385`, `AMB-D-622`).
///
/// `name` is the only field in both: the list entry is fetched by it and the detail document is verified to
/// be the one it was asked for, so leaving it out of either would break the join. Every other field goes to
/// exactly one side.
///
/// The translations come back split the same way, each language's fields following their base fields: the
/// list halves as a map the CI keys into `catalog.<lang>.json` by plugin name, the detail halves already
/// inside [`Detail::i18n`]. **A language whose list half translates nothing gets no entry in that map** —
/// there is nothing to publish for it, and an empty object in nineteen documents is nineteen fetches that
/// answer nothing.
///
/// The manifest is expected to have passed [`crate::plugin_validate`] first, its overlays included
/// ([`crate::plugin_validate::validate_overlays`] is what has established that these language codes name
/// languages, and that every key they translate exists). Splitting does not judge — it moves values — so
/// an invalid manifest would split just as happily into invalid documents; the door is what keeps one from
/// being published, not this.
pub fn split(
    manifest: &Manifest,
    translations: &Translations,
) -> (ListEntry, BTreeMap<String, ListEntryOverlay>, Detail) {
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
        settings: manifest.settings.clone(),
        events: manifest.events.clone(),
        agent: manifest.agent.clone(),
        about: manifest.about.clone(),
        i18n: translations
            .iter()
            .filter(|(_, o)| o.about.is_some() || !o.config.is_empty() || o.settings.is_some())
            .map(|(lang, o)| {
                (
                    lang.clone(),
                    DetailOverlay {
                        about: o.about.clone(),
                        config: o.config.clone(),
                        settings: o.settings.clone(),
                    },
                )
            })
            .collect(),
    };
    let entries = translations
        .iter()
        .filter(|(_, o)| o.desc.is_some())
        .map(|(lang, o)| (lang.clone(), ListEntryOverlay { desc: o.desc.clone() }))
        .collect();
    (entry, entries, detail)
}

/// Put the two documents the catalog serves back together as the one manifest Amenbo works from
/// (`AMB-D-385`) — the reverse of [`split`], and the shape every gate downstream already speaks.
///
/// An install reads the list once and the detail for the one plugin it is installing; from there on the
/// platform resolution, the provenance check, the compatibility gate and the record written beside the
/// binary all want a whole manifest, so the join happens once here rather than each of them learning to
/// read two documents. `detail_sum` rides along from the entry, which is what makes the record beside the
/// binary say which detail it came from (`AMB-D-386`).
///
/// The translations come back the same way ([`Translations`]), from wherever each half of them was
/// fetched: the detail carries every language's form labels, while the list half is however many
/// `catalog.<lang>.json` documents the caller went and got — usually the one the reader's language names,
/// and none at all offline. A language present in one half and not the other is not a gap to fill: an
/// overlay is optional field by field (`AMB-D-623`), so what was not fetched reads exactly like what was
/// never translated, and both fall back to the base value.
///
/// **The join is not a door.** The two halves are untrusted delivery, and joining them judges nothing:
/// the caller runs [`crate::plugin_validate::validate_manifest`] over the result before anything is
/// fetched or written. That the detail is the one the entry asked for — same `name` — is checked where it
/// is fetched ([`crate::plugin_catalog::detail`]), since that is where the question is asked.
pub fn join(
    entry: &ListEntry,
    entries: &BTreeMap<String, ListEntryOverlay>,
    detail: &Detail,
) -> (Manifest, Translations) {
    let mut translations = Translations::new();
    for (lang, o) in entries {
        translations.entry(lang.clone()).or_default().desc = o.desc.clone();
    }
    for (lang, o) in &detail.i18n {
        let overlay = translations.entry(lang.clone()).or_default();
        overlay.about = o.about.clone();
        overlay.config = o.config.clone();
        overlay.settings = o.settings.clone();
    }
    let manifest = Manifest {
        name: entry.name.clone(),
        desc: entry.desc.clone(),
        about: detail.about.clone(),
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
        settings: detail.settings.clone(),
        events: detail.events.clone(),
        agent: detail.agent.clone(),
    };
    (manifest, translations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_manifest::{Face, ManifestOverlay, SettingsActionOverlay};

    /// A manifest exercising every field, so a split that drops one is visible rather than merely
    /// unrepresented. Both distributable forms are written at once — which is not a shape the door accepts,
    /// but this is about where a value lands, not whether the document is legal.
    fn full() -> Manifest {
        serde_json::from_value(serde_json::json!({
            "name": "worktree",
            "desc": "Isolate each task in its own git worktree",
            "about": "Cuts a worktree per task, and folds it once the work is merged.",
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
            "settings": {
                "check": "config check",
                "actions": [{
                    "cmd": "config test",
                    "label": "Send a test message",
                    "ask": [{ "key": "api_token", "label": "API token", "secret": true }],
                }],
            },
            "events": [{ "event": "task.status_changed", "faces": ["cli"], "reply": true }],
            "agent": {
                "when": "Starting work on a task that will produce commits",
                "commands": [{ "cmd": "start <task-id>", "does": "Cuts a worktree and returns the cd line" }],
            },
        }))
        .expect("the fixture is a manifest")
    }

    /// What that manifest's author wrote it as elsewhere, exercising both faces at once: a language that
    /// translates the line a browse view draws, and the text, the labels and the buttons a detail view
    /// and a settings form show.
    fn translated() -> Translations {
        Translations::from([(
            "ja".to_string(),
            ManifestOverlay {
                desc: Some("タスクごとに git worktree を切り分ける".into()),
                about: Some("タスクごとに worktree を切り、マージが済んだら畳む。".into()),
                config: BTreeMap::from([(
                    "base".to_string(),
                    ConfigFieldOverlay {
                        label: Some("基点にするブランチ".into()),
                        ..ConfigFieldOverlay::default()
                    },
                )]),
                settings: Some(SettingsOverlay {
                    actions: BTreeMap::from([(
                        "config test".to_string(),
                        SettingsActionOverlay {
                            label: Some("テスト送信".into()),
                            ask: BTreeMap::from([(
                                "api_token".to_string(),
                                "API トークン".to_string(),
                            )]),
                            ..SettingsActionOverlay::default()
                        },
                    )]),
                    ..SettingsOverlay::default()
                }),
                ..ManifestOverlay::default()
            },
        )])
    }

    /// **The guard on the split.** Every field Amenbo serializes on a manifest is published in exactly one
    /// face, so a field added later can reach neither document nor both only over this test's dead body.
    /// The keys are read off the serialized shapes rather than listed here, because a list here would be
    /// one more copy to forget — which is the failure the split exists to close.
    #[test]
    fn every_manifest_field_is_published_exactly_once() {
        let manifest = full();
        let (entry, _, detail) = split(&manifest, &translated());

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

        // And nothing is invented on the way out beyond the slots the catalog fills and the translated
        // layer, which is the author's other languages rather than a field of the manifest itself.
        for field in entry_keys.iter().chain(detail_keys.iter()) {
            assert!(
                manifest_keys.contains(field)
                    || ["featured", "added_at", "detail_sum", "i18n"].contains(&field.as_str()),
                "published field {field:?} is neither a manifest field nor a catalog slot",
            );
        }
    }

    /// **The guard on the language axis** (`AMB-D-622`). A translated field is published on the face its
    /// base field is published on, so a reader who has one document in hand has that document's words in
    /// their own language — and never has to fetch the other half to read the half they already drew.
    /// Like the guard above, the keys are read off the serialized shapes: a translatable field added
    /// later reaches its face or fails here.
    #[test]
    fn every_translatable_field_is_published_on_its_base_field_face() {
        let (entry, entries, detail) = split(&full(), &translated());

        let keys = |v: serde_json::Value| {
            v.as_object().expect("a document serializes as an object").keys().cloned().collect::<Vec<_>>()
        };
        let base_face = |field: &String| match (
            keys(serde_json::to_value(&entry).unwrap()).contains(field),
            keys(serde_json::to_value(&detail).unwrap()).contains(field),
        ) {
            (true, false) => "list",
            (false, true) => "detail",
            _ => panic!("{field:?} is translated but is not a field of exactly one face"),
        };
        let translated_face = |field: &String| {
            let in_list = entries
                .values()
                .any(|o| keys(serde_json::to_value(o).unwrap()).contains(field));
            let in_detail = detail
                .i18n
                .values()
                .any(|o| keys(serde_json::to_value(o).unwrap()).contains(field));
            match (in_list, in_detail) {
                (true, false) => "list",
                (false, true) => "detail",
                (true, true) => panic!("{field:?} is translated on both faces — a reader pays twice"),
                (false, false) => panic!("{field:?} is translatable but reaches neither face"),
            }
        };

        let overlay = translated()["ja"].clone();
        let translatable = keys(serde_json::to_value(&overlay).unwrap());
        assert!(
            translatable.contains(&"desc".to_string())
                && translatable.contains(&"config".to_string())
                && translatable.contains(&"settings".to_string()),
            "the fixture translates both faces, or this proves nothing: {translatable:?}",
        );
        for field in &translatable {
            assert_eq!(
                translated_face(field),
                base_face(field),
                "{field:?} is translated on one face and published on the other",
            );
        }
    }

    /// The values land where they belong, and the catalog's own slots come out empty for the CI to fill.
    #[test]
    fn the_list_entry_draws_and_the_detail_installs() {
        let (entry, _, detail) = split(&full(), &Translations::new());

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
        assert_eq!(
            detail.about.as_deref(),
            Some("Cuts a worktree per task, and folds it once the work is merged."),
            "the author's own words are read on the one plugin someone opened, not while browsing",
        );
        assert_eq!(detail.checksum, "sha256:".to_string() + &"0".repeat(64));
        assert!(detail.signature.is_some());
        assert_eq!(detail.assets.len(), 1);
        assert_eq!(detail.scope, Scope::Machine, "the layer installs, and is not a row a list draws");
        assert_eq!(detail.payload_v, 1);
        assert_eq!(detail.min_amenbo.as_deref(), Some("1.8.0"));
        assert_eq!(detail.config.len(), 1);
        let settings = detail.settings.as_ref().expect("where the form raises a call installs too");
        assert_eq!(settings.check.as_deref(), Some("config check"));
        assert_eq!(settings.actions[0].ask[0].key, "api_token", "the press's one-time input with it");
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
        let translations = translated();
        let (entry, entries, detail) = split(&manifest, &translations);

        assert_eq!(
            join(&entry, &entries, &detail),
            (Manifest { detail_sum: None, ..manifest }, translations),
            "the author's other languages survive the round trip as whole overlays, not per face",
        );
    }

    /// **What a reader who fetched only their own language ends up with.** The list half comes back one
    /// document per language and the detail half comes back all at once, so the everyday join has one
    /// list overlay in hand and nineteen detail ones. Nothing is filled in for the languages that were
    /// not fetched: an overlay is optional field by field, so an untranslated line and an unfetched one
    /// look the same and both fall back to the base (`AMB-D-623`).
    #[test]
    fn a_language_nobody_fetched_the_list_half_of_still_carries_what_the_detail_held() {
        let (entry, _, detail) = split(&full(), &translated());

        let (_, translations) = join(&entry, &BTreeMap::new(), &detail);

        let ja = &translations["ja"];
        assert_eq!(ja.desc, None, "the list document for ja was never fetched, so its line is the base's");
        assert_eq!(
            ja.about.as_deref(),
            Some("タスクごとに worktree を切り、マージが済んだら畳む。"),
            "the description text rode in the detail, which an install always has",
        );
        assert_eq!(
            ja.config["base"].label.as_deref(),
            Some("基点にするブランチ"),
            "and the form labels rode there with it",
        );
        assert_eq!(
            ja.settings.as_ref().unwrap().actions["config test"].label.as_deref(),
            Some("テスト送信"),
            "and the buttons on that same form, which is where the block they translate is read",
        );
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
            let (_, _, detail) = split(&manifest, &Translations::new());
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

            let mut d = serde_json::to_value(split(&full(), &Translations::new()).2).unwrap();
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
        let (mut entry, entries, detail) = split(&full(), &Translations::new());
        entry.detail_sum = Some(format!("sha256:{}", "4".repeat(64)));

        assert_eq!(join(&entry, &entries, &detail).0.detail_sum, entry.detail_sum);
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
        let (entry, entries, detail) = split(&bare, &Translations::new());

        assert!(entries.is_empty(), "nobody translated it, so there is no language document to publish");
        let detail_json = serde_json::to_value(&detail).unwrap();
        for absent in
            ["signature", "assets", "min_amenbo", "config", "settings", "events", "agent", "about", "i18n"]
        {
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
    /// manifest is writing a key Amenbo does not read, so the entry the catalog publishes still says
    /// `false` — the recommendation is the curator's, and there is no field on this path for anyone else
    /// to set. `official` needs the CI to refuse a claim because the manifest carries it; this one is
    /// unreachable from a manifest at all.
    #[test]
    fn a_manifest_claiming_the_recommendation_is_published_without_it() {
        let mut claimed = serde_json::to_value(full()).unwrap();
        claimed["featured"] = serde_json::json!(true);
        let manifest: Manifest = serde_json::from_value(claimed).expect("unknown keys are ignored");

        let (entry, _, _) = split(&manifest, &Translations::new());
        assert!(!entry.featured, "what the author wrote never reached the entry");
    }

    /// **A language is published on a face only when it has something to say there** (`AMB-D-622`). An
    /// author who translated the one line a browse view draws and left the detail alone gets a
    /// `catalog.<lang>.json` entry and no detail entry, and one who did the reverse gets the reverse —
    /// so no reader fetches a document that turns out to hold an empty object. The detail face has three
    /// translatable fields, and any one of them alone is something to say there.
    #[test]
    fn a_language_reaches_only_the_faces_it_translates() {
        let translations = Translations::from([
            (
                "ja".to_string(),
                ManifestOverlay { desc: Some("一行".into()), ..ManifestOverlay::default() },
            ),
            (
                "de".to_string(),
                ManifestOverlay {
                    config: BTreeMap::from([(
                        "base".to_string(),
                        ConfigFieldOverlay {
                            label: Some("Basis-Branch".into()),
                            ..ConfigFieldOverlay::default()
                        },
                    )]),
                    ..ManifestOverlay::default()
                },
            ),
            (
                "es".to_string(),
                ManifestOverlay {
                    settings: Some(SettingsOverlay {
                        actions: BTreeMap::from([(
                            "config test".to_string(),
                            SettingsActionOverlay {
                                label: Some("Enviar una prueba".into()),
                                ..SettingsActionOverlay::default()
                            },
                        )]),
                        ..SettingsOverlay::default()
                    }),
                    ..ManifestOverlay::default()
                },
            ),
            (
                "fr".to_string(),
                ManifestOverlay { about: Some("Quelques mots.".into()), ..ManifestOverlay::default() },
            ),
        ]);

        let (_, entries, detail) = split(&full(), &translations);

        assert_eq!(entries.keys().collect::<Vec<_>>(), ["ja"], "only ja translated the line");
        assert_eq!(
            detail.i18n.keys().collect::<Vec<_>>(),
            ["de", "es", "fr"],
            "de translated the form, es its buttons and fr the text — each is the detail face on its own",
        );
    }
}
