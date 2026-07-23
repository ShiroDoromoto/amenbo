//! The plugin catalog as amenbo holds it — **one** fetch of the whole index, cached on disk
//! (`AMB-D-347`).
//!
//! The unit of retrieval is the catalog, never the plugin: the catalog repository's CI aggregates every
//! reviewed manifest into a single `catalog.json` on a static host, and amenbo fetches that one file.
//! Browsing, searching, sorting and paging then happen entirely on the local copy — no per-plugin request,
//! so the cost of discovery does not grow with the number of plugins.
//!
//! What comes back is an envelope — `catalog_v` (the envelope's version), `generated_at`, and `plugins` —
//! whose entries are plugin manifests plus the two fields only the catalog can supply: the `signature` its
//! CI produced (verified at install by [`crate::plugin_provenance`]) and `added_at`, the day the manifest
//! first appeared in the index. `added_at` has no other source: a client holds no git history of the
//! catalog repository, so "new" is knowable only because the CI writes it.
//!
//! **The delivery path is not trusted** (`AMB-D-354`). Every entry is run through
//! [`crate::plugin_validate`] here, on the way in, and one that does not parse or does not validate is
//! dropped — the rest of the catalog stays usable. Dropping is recorded ([`Dropped`]) rather than
//! silent, so a catalog that is quietly losing entries is visible. The envelope itself is fail-closed
//! the other way: a `catalog_v` from the future is refused whole, because amenbo cannot know what a newer
//! envelope means.
//!
//! **The catalog carries no signature, and needs none.** Trust rests on each asset's signature, which
//! verifies only against the public key amenbo ships ([`crate::plugin_provenance::CATALOG_PUBLIC_KEY`]).
//! A swapped catalog buys nothing: its assets still have to pass that door at install.
//!
//! Offline is the normal case, not an error: [`load`] serves the cached copy when the fetch fails, and a
//! failed fetch never overwrites what is cached — the cache is replaced only by a catalog that parsed.
//!
//! Scope: the official catalog, one URL. Registering third-party catalogs and merging several is
//! `AMB-T-1980`, which is why the cache is a **named file** in the registry directory rather than the
//! directory itself.

use crate::config::Paths;
use crate::error::{Error, Result};
use crate::plugin_manifest::Manifest;
use crate::plugin_validate::{validate_manifest, Problem};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;

/// Where the official catalog is published — the static host the catalog repository's CI writes to on
/// every merge. Overridable for development and tests through `AMENBO_PLUGIN_CATALOG_URL`
/// ([`crate::env::plugin_catalog_url`]).
pub const OFFICIAL_CATALOG_URL: &str = "https://shirodoromoto.github.io/amenbo-plugins/catalog.json";

/// The envelope version this amenbo understands. It versions the *envelope*, not the entries: an entry
/// grows by adding fields that older clients ignore, so this number moves only if the shape around
/// `plugins` changes. A catalog declaring a higher one is refused ([`parse`]).
pub const SUPPORTED_CATALOG_V: u32 = 1;

/// The cached copy of the official catalog, under [`Paths::registry_dir`]. Named, not anonymous, because
/// third-party catalogs land beside it (`AMB-T-1980`).
pub const OFFICIAL_CACHE_FILE_NAME: &str = "official.json";

/// How long a catalog fetch may take before it is treated as a failure (and the cache answers instead).
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// The wire envelope, exactly as the catalog CI writes it.
#[derive(Debug, Deserialize)]
struct Envelope {
    catalog_v: u32,
    #[serde(default)]
    generated_at: Option<String>,
    #[serde(default)]
    plugins: Vec<serde_json::Value>,
}

/// One usable catalog entry: a manifest, plus what only the catalog knows about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The plugin's manifest — validated ([`crate::plugin_validate`]) before it got here.
    pub manifest: Manifest,
    /// When this manifest first entered the catalog, from the catalog repository's git history. `None`
    /// for a catalog built before the CI wrote the field; it is the only source for a "new" sort, so a
    /// missing value means unknown, never "old".
    pub added_at: Option<String>,
}

/// Why one entry did not make it into the catalog amenbo holds. Kept rather than discarded so a catalog
/// that is silently shedding entries can be seen (`AMB-D-354`).
#[derive(Clone, Debug)]
pub enum Dropped {
    /// The entry did not deserialize into a manifest at all — a missing required field, a wrong type.
    /// Identified by position, since a value this broken may not even have a usable name.
    Unreadable { index: usize, error: String },
    /// The entry parsed but broke the manifest rules.
    Invalid { name: String, problems: Vec<Problem> },
    /// A second entry claiming a name an earlier one already took. The first wins: a later duplicate
    /// cannot displace an entry that is already there.
    Duplicate { name: String },
}

/// A catalog, as amenbo holds it after intake.
#[derive(Clone, Debug)]
pub struct Catalog {
    /// When the catalog CI generated this copy, verbatim from the envelope.
    pub generated_at: Option<String>,
    /// The entries that parsed and validated, in catalog order.
    pub entries: Vec<Entry>,
    /// The entries that did not, and why.
    pub dropped: Vec<Dropped>,
}

impl Catalog {
    /// Find one entry by plugin name — what an install resolves against (`AMB-T-2050`).
    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.manifest.name == name)
    }
}

/// A catalog entry on the wire: a manifest with the catalog's own `added_at` beside it. `signature` is
/// part of the manifest already, so it needs no special handling here.
#[derive(Debug, Deserialize)]
struct WireEntry {
    #[serde(flatten)]
    manifest: Manifest,
    #[serde(default)]
    added_at: Option<String>,
}

/// The URL to fetch: [`OFFICIAL_CATALOG_URL`], unless the environment overrides it.
fn catalog_url() -> String {
    crate::env::plugin_catalog_url().unwrap_or_else(|| OFFICIAL_CATALOG_URL.to_string())
}

/// Parse a catalog document, running every entry through the validator on the way in (`AMB-D-354`).
///
/// Fail-closed at two different scales. The **envelope** is all-or-nothing: unparseable JSON, or a
/// `catalog_v` newer than [`SUPPORTED_CATALOG_V`], refuses the whole document — amenbo will not guess at
/// a shape it does not know. An **entry** is dropped on its own ([`Dropped`]), leaving the rest of the
/// catalog usable, because one bad manifest is not a reason to have no catalog.
pub fn parse(json: &str) -> Result<Catalog> {
    let envelope: Envelope = serde_json::from_str(json).map_err(|e| {
        Error::invalid(
            format!("the plugin catalog is not readable: {e}"),
            format!("プラグイン目録を読み取れません：{e}"),
        )
    })?;
    if envelope.catalog_v > SUPPORTED_CATALOG_V {
        return Err(Error::invalid(
            format!(
                "this plugin catalog is version {} — newer than this amenbo understands ({SUPPORTED_CATALOG_V}). Update amenbo.",
                envelope.catalog_v
            ),
            format!(
                "このプラグイン目録は版 {} で、この amenbo が解釈できる版（{SUPPORTED_CATALOG_V}）より新しいです。amenbo を更新してください。",
                envelope.catalog_v
            ),
        ));
    }

    let mut entries: Vec<Entry> = Vec::new();
    let mut dropped = Vec::new();
    for (index, value) in envelope.plugins.into_iter().enumerate() {
        let wire: WireEntry = match serde_json::from_value(value) {
            Ok(w) => w,
            Err(e) => {
                dropped.push(Dropped::Unreadable { index, error: e.to_string() });
                continue;
            }
        };
        let problems = validate_manifest(&wire.manifest);
        if !problems.is_empty() {
            dropped.push(Dropped::Invalid { name: wire.manifest.name.clone(), problems });
            continue;
        }
        if entries.iter().any(|e| e.manifest.name == wire.manifest.name) {
            dropped.push(Dropped::Duplicate { name: wire.manifest.name });
            continue;
        }
        entries.push(Entry { manifest: wire.manifest, added_at: wire.added_at });
    }
    Ok(Catalog { generated_at: envelope.generated_at, entries, dropped })
}

/// Where the official catalog is cached: `<base>/plugins/registry/official.json`.
pub fn cache_file(paths: &Paths) -> PathBuf {
    paths.registry_dir().join(OFFICIAL_CACHE_FILE_NAME)
}

/// The cached catalog, or `None` when there is none — absent, unreadable, or written by a version whose
/// envelope this one refuses. `None` is "we have no local copy", never an error to show: the caller's
/// answer to it is to fetch.
pub fn cached(paths: &Paths) -> Option<Catalog> {
    let json = std::fs::read_to_string(cache_file(paths)).ok()?;
    parse(&json).ok()
}

/// Fetch the catalog and replace the cache with it — the explicit "get the current index" path.
///
/// The document is parsed **before** anything is written, so a fetch that returns garbage leaves the
/// existing cache intact. Individual entries the intake dropped are still cached: they are what the
/// catalog actually served, and re-parsing the cache reproduces the same drops.
pub fn refresh(paths: &Paths) -> Result<Catalog> {
    refresh_from(paths, &catalog_url())
}

/// [`refresh`] against a named URL — the seam a test drives, and where a registered third-party catalog
/// will enter (`AMB-T-1980`).
fn refresh_from(paths: &Paths, url: &str) -> Result<Catalog> {
    let json = fetch(url)?;
    let catalog = parse(&json)?;
    write_cache(paths, &json)?;
    Ok(catalog)
}

/// The catalog to work from: the current one when the network answers, the cached one when it does not
/// (`AMB-D-347` — discovery is a static file, so being offline costs freshness, not function).
///
/// Fails only when both are unavailable: nothing fetched, and nothing cached.
pub fn load(paths: &Paths) -> Result<Catalog> {
    load_from(paths, &catalog_url())
}

/// [`load`] against a named URL — see [`refresh_from`].
fn load_from(paths: &Paths, url: &str) -> Result<Catalog> {
    match refresh_from(paths, url) {
        Ok(catalog) => Ok(catalog),
        Err(fetch_error) => cached(paths).ok_or(fetch_error),
    }
}

/// Fetch the catalog document, with a timeout. The only network I/O in this module.
fn fetch(url: &str) -> Result<String> {
    let agent: ureq::Agent =
        ureq::Agent::config_builder().timeout_global(Some(FETCH_TIMEOUT)).build().into();
    let mut response = agent.get(url).call().map_err(|e| {
        Error::Io(std::io::Error::other(format!("could not reach the plugin catalog at {url}: {e}")))
    })?;
    response.body_mut().read_to_string().map_err(|e| {
        Error::Io(std::io::Error::other(format!("could not read the plugin catalog from {url}: {e}")))
    })
}

/// Replace the cached catalog with `json`, atomically: written to a temporary file beside the target and
/// renamed over it, so a crash mid-write cannot leave a half-catalog behind.
fn write_cache(paths: &Paths, json: &str) -> Result<()> {
    let dest = cache_file(paths);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &dest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The envelope the catalog repository's CI actually publishes, copied verbatim from
    /// `https://shirodoromoto.github.io/amenbo-plugins/catalog.json`. Pinned here to fix the contract:
    /// the producer is a separate repository, so a rename on its side would otherwise reach this
    /// consumer only as an empty catalog on a user's machine.
    const REAL_EMPTY_CATALOG: &str = r#"{
  "catalog_v": 1,
  "generated_at": "2026-07-23T04:57:10Z",
  "plugins": []
}
"#;

    fn entry_json(name: &str) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "desc": "a plugin",
            "author": "amenbo",
            "repo": "ShiroDoromoto/amenbo",
            "os": ["macos", "linux", "windows"],
            "category": "workflow",
            "url": "https://example.invalid/x.tar.gz",
            "checksum": format!("sha256:{}", "a".repeat(64)),
            "signature": "untrusted comment: x\nsig\ntrusted comment: y\nglobal\n",
            "added_at": "2026-07-23T04:23:48Z",
        })
    }

    fn catalog_json(entries: Vec<serde_json::Value>) -> String {
        serde_json::json!({ "catalog_v": 1, "generated_at": "2026-07-23T04:57:10Z", "plugins": entries })
            .to_string()
    }

    fn paths_at(tag: &str) -> Paths {
        let dir = amenbo_scratch::scratch(&format!("plugin-catalog-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        Paths::at(dir)
    }

    #[test]
    fn the_published_catalog_parses() {
        let catalog = parse(REAL_EMPTY_CATALOG).unwrap();
        assert!(catalog.entries.is_empty(), "an empty catalog is a catalog, not a failure");
        assert_eq!(catalog.generated_at.as_deref(), Some("2026-07-23T04:57:10Z"));
    }

    #[test]
    fn an_entry_keeps_the_two_fields_only_the_catalog_supplies() {
        let catalog = parse(&catalog_json(vec![entry_json("worktree")])).unwrap();
        let entry = catalog.find("worktree").expect("the entry is there");
        assert!(entry.manifest.signature.is_some(), "the CI's signature rides on the manifest");
        assert_eq!(entry.added_at.as_deref(), Some("2026-07-23T04:23:48Z"), "the 'new' axis");
    }

    #[test]
    fn a_newer_envelope_is_refused_whole() {
        let json = r#"{"catalog_v": 2, "plugins": []}"#;
        let err = parse(json).unwrap_err();
        assert!(format!("{err:?}").contains("newer"), "the version is the reason");
    }

    #[test]
    fn a_malformed_document_is_refused_whole() {
        assert!(parse("not json").is_err());
        assert!(parse(r#"{"generated_at": "now"}"#).is_err(), "no catalog_v is not a catalog");
    }

    #[test]
    fn a_broken_entry_is_dropped_and_the_rest_survives() {
        // The delivery path is not trusted: one entry missing a required field must not cost the catalog.
        let mut broken = entry_json("broken");
        broken.as_object_mut().unwrap().remove("checksum");
        let json = catalog_json(vec![entry_json("first"), broken, entry_json("last")]);
        let catalog = parse(&json).unwrap();
        assert_eq!(catalog.entries.len(), 2, "the readable entries are kept");
        assert!(catalog.find("first").is_some() && catalog.find("last").is_some());
        assert!(
            matches!(catalog.dropped.as_slice(), [Dropped::Unreadable { index: 1, .. }]),
            "the drop is recorded, with where it was: {:?}",
            catalog.dropped
        );
    }

    #[test]
    fn an_entry_that_fails_the_validator_is_dropped() {
        // Well-formed JSON, but a name the on-disk layout reserves. The catalog CI should have caught
        // it; intake checks anyway, because the delivery path is not what amenbo trusts.
        let json = catalog_json(vec![entry_json("registry"), entry_json("good")]);
        let catalog = parse(&json).unwrap();
        assert_eq!(catalog.entries.len(), 1);
        assert!(
            matches!(catalog.dropped.as_slice(), [Dropped::Invalid { name, .. }] if name == "registry"),
            "the validator's refusal is recorded: {:?}",
            catalog.dropped
        );
    }

    #[test]
    fn a_duplicate_name_cannot_displace_the_first_entry() {
        let mut second = entry_json("worktree");
        second["desc"] = serde_json::json!("an impostor");
        let json = catalog_json(vec![entry_json("worktree"), second]);
        let catalog = parse(&json).unwrap();
        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(catalog.find("worktree").unwrap().manifest.desc, "a plugin", "the first one holds");
        assert!(matches!(catalog.dropped.as_slice(), [Dropped::Duplicate { .. }]));
    }

    #[test]
    fn an_unknown_entry_field_is_ignored_rather_than_fatal() {
        // The catalog grows by adding fields; an older amenbo must keep reading it.
        let mut entry = entry_json("worktree");
        entry["stars_next_version"] = serde_json::json!(42);
        let catalog = parse(&catalog_json(vec![entry])).unwrap();
        assert_eq!(catalog.entries.len(), 1);
    }

    // ---- the cache ----

    #[test]
    fn the_cache_round_trips_through_the_registry_dir() {
        let paths = paths_at("round-trip");
        assert!(cached(&paths).is_none(), "nothing cached yet");
        write_cache(&paths, &catalog_json(vec![entry_json("worktree")])).unwrap();
        assert_eq!(cache_file(&paths), paths.registry_dir().join("official.json"));
        let catalog = cached(&paths).expect("the cache reads back");
        assert!(catalog.find("worktree").is_some());
    }

    #[test]
    fn a_corrupt_cache_reads_as_no_cache() {
        let paths = paths_at("corrupt");
        write_cache(&paths, "half a file").unwrap();
        assert!(cached(&paths).is_none(), "unreadable is 'fetch again', not an error to show");
    }

    #[test]
    fn writing_the_cache_leaves_no_temporary_behind() {
        let paths = paths_at("no-temp");
        write_cache(&paths, &catalog_json(vec![])).unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(paths.registry_dir())
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "the staging file is renamed, not left: {leftovers:?}");
    }

    /// An address nothing answers on — what being offline looks like from inside the fetch.
    const UNREACHABLE: &str = "http://127.0.0.1:1/catalog.json";

    #[test]
    fn load_falls_back_to_the_cache_when_the_fetch_fails() {
        let paths = paths_at("offline");
        write_cache(&paths, &catalog_json(vec![entry_json("worktree")])).unwrap();
        let catalog = load_from(&paths, UNREACHABLE).expect("the cache answers");
        assert!(catalog.find("worktree").is_some());
        assert!(refresh_from(&paths, UNREACHABLE).is_err(), "a refresh still reports the failure");
        assert!(cached(&paths).is_some(), "and a failed fetch never clears the cache");
    }

    #[test]
    fn load_fails_only_when_there_is_neither_network_nor_cache() {
        let paths = paths_at("nothing");
        assert!(load_from(&paths, UNREACHABLE).is_err());
    }

    #[test]
    #[ignore = "reaches the published catalog over the network"]
    fn the_live_catalog_answers_and_comes_through_intake_whole() {
        // The end-to-end this module cannot assert in CI: the real URL, the real envelope, the real
        // intake. Run it by hand (`cargo nextest run -p amenbo-core plugin_catalog -- --ignored`) when
        // the catalog's producer changes.
        let paths = paths_at("live");
        let catalog = refresh_from(&paths, OFFICIAL_CATALOG_URL).expect("the published catalog answers");
        assert!(catalog.dropped.is_empty(), "nothing published had to be dropped: {:?}", catalog.dropped);
        assert!(cached(&paths).is_some(), "and the fetch replaced the cache");
    }
}
