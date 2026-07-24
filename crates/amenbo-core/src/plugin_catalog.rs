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
//! Scope of install/update: the official catalog, one URL. **Third-party catalogs** register beside it and
//! merge into a browsing view ([`discover`], `AMB-T-1980`) — which is why the cache is a **named file** in
//! the registry directory, not the directory itself — but they never enter the install path: an asset is
//! trusted only by amenbo's catalog key (`AMB-D-371`), so install and update read the official catalog
//! alone.

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

/// How long the cached catalog answers before a read goes back to the network (`AMB-D-359`).
///
/// This is the boundary that lets a check ride along with something a user did anyway — opening a view,
/// listing what is installed — instead of needing a resident timer to poll: a trigger arriving inside the
/// window is answered from disk, and only one outside it costs a fetch. The value is deliberately the
/// same hour amenbo's own update check settles on (`AMB-D-362`), for the same reason: a fix that is
/// published is worth reaching within the day, and an hour is short enough for that without turning
/// discovery into traffic.
pub const FRESH_FOR: Duration = Duration::from_secs(60 * 60);

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
            format!("プラグインカタログを読み取れません：{e}"),
        )
    })?;
    if envelope.catalog_v > SUPPORTED_CATALOG_V {
        return Err(Error::invalid(
            format!(
                "this plugin catalog is version {} — newer than this amenbo understands ({SUPPORTED_CATALOG_V}). Update amenbo.",
                envelope.catalog_v
            ),
            format!(
                "このプラグインカタログは版 {} で、この amenbo が解釈できる版（{SUPPORTED_CATALOG_V}）より新しいです。amenbo を更新してください。",
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
    cached_at(&cache_file(paths))
}

/// Fetch the catalog and replace the cache with it — the explicit "get the current index" path.
///
/// The document is parsed **before** anything is written, so a fetch that returns garbage leaves the
/// existing cache intact. Individual entries the intake dropped are still cached: they are what the
/// catalog actually served, and re-parsing the cache reproduces the same drops.
pub fn refresh(paths: &Paths) -> Result<Catalog> {
    refresh_to(&catalog_url(), &cache_file(paths))
}

/// The catalog to work from: the current one when the network answers, the cached one when it does not
/// (`AMB-D-347` — discovery is a static file, so being offline costs freshness, not function).
///
/// Fails only when both are unavailable: nothing fetched, and nothing cached.
pub fn load(paths: &Paths) -> Result<Catalog> {
    load_to(&catalog_url(), &cache_file(paths))
}

/// The catalog for a read that is *incidental* — a check hanging off something the user did anyway
/// (`AMB-D-359`), rather than the explicit "get me the current index" [`load`] an install makes.
///
/// A cache younger than [`FRESH_FOR`] answers as it stands, with no request at all; past that boundary
/// this is exactly [`load`], so the network is asked once and a failure still falls back to the cache.
/// The distinction is the point: an install wants the newest index it can get, while a check wants to be
/// cheap enough that it can be offered often.
pub fn fresh(paths: &Paths) -> Result<Catalog> {
    fresh_to(&catalog_url(), &cache_file(paths))
}

// ---- the catalog fetch/cache mechanism, keyed on a named cache file ----
//
// The official catalog is one URL cached in one file; a registered third-party catalog is another URL
// cached in its own file beside it (`AMB-T-1980`). The mechanism is the same for both, so the functions
// that fetch, cache and fall back take the cache file as an argument — the `paths`-shaped functions above
// are the official-catalog spellings of these, and `discover` drives the third-party ones.

/// [`cached`] against a named cache file.
fn cached_at(cache_file: &std::path::Path) -> Option<Catalog> {
    let json = std::fs::read_to_string(cache_file).ok()?;
    parse(&json).ok()
}

/// [`refresh`] against a named URL and cache file — the seam a test drives, and where a registered
/// third-party catalog enters (`AMB-T-1980`).
fn refresh_to(url: &str, cache_file: &std::path::Path) -> Result<Catalog> {
    let json = fetch(url)?;
    let catalog = parse(&json)?;
    write_cache_at(cache_file, &json)?;
    Ok(catalog)
}

/// [`load`] against a named URL and cache file — see [`refresh_to`].
fn load_to(url: &str, cache_file: &std::path::Path) -> Result<Catalog> {
    match refresh_to(url, cache_file) {
        Ok(catalog) => Ok(catalog),
        Err(fetch_error) => cached_at(cache_file).ok_or(fetch_error),
    }
}

/// [`fresh`] against a named URL and cache file.
fn fresh_to(url: &str, cache_file: &std::path::Path) -> Result<Catalog> {
    if cache_age_at(cache_file).is_some_and(|age| age < FRESH_FOR) {
        if let Some(catalog) = cached_at(cache_file) {
            return Ok(catalog);
        }
    }
    load_to(url, cache_file)
}

/// How long ago the cache was last written, or `None` when there is no cache — or when the clock says it
/// was written in the future, which is no evidence of freshness and falls through to a fetch.
fn cache_age_at(cache_file: &std::path::Path) -> Option<Duration> {
    std::fs::metadata(cache_file).ok()?.modified().ok()?.elapsed().ok()
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

/// Replace a cached catalog with `json`, atomically: written to a temporary file beside the target and
/// renamed over it, so a crash mid-write cannot leave a half-catalog behind.
fn write_cache_at(cache_file: &std::path::Path, json: &str) -> Result<()> {
    if let Some(parent) = cache_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = cache_file.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, cache_file)?;
    Ok(())
}

// ---- registered third-party catalogs, and the merged view (`AMB-T-1980`) ----
//
// A third-party catalog is a second index the same shape as the official one, at the author's own URL
// (`AMB-D-347`, the "free" tier). The unit registered is the **catalog**, never the plugin: what grows is
// the number of indexes (few), not per-plugin requests (many). The list of registered URLs is a small
// file in the registry directory; each catalog is fetched and cached in its own named file beside the
// official one, and `discover` merges them for browsing.
//
// **Discovery only.** A merged catalog is what a user browses (`AMB-T-1982`); it is *not* what an install
// resolves against. Install and update keep reading the official catalog alone (`load`/`fresh`),
// because an asset is trusted only by amenbo's catalog key (`AMB-D-371`): a third-party asset does not
// verify and so cannot be installed regardless, and letting a third-party name shadow an official one in
// the install path buys an attacker nothing but confusion. So the merge lives here, apart from the
// install path, and official entries always win a name clash.

/// The name, under [`Paths::registry_dir`], of the list of registered third-party catalog URLs.
pub const SOURCES_FILE_NAME: &str = "sources.json";

/// The registered-sources file, `<base>/plugins/registry/sources.json`.
fn sources_file(paths: &Paths) -> PathBuf {
    paths.registry_dir().join(SOURCES_FILE_NAME)
}

/// The on-disk shape of the sources list — an envelope so a field can be added later without an older
/// build misreading it, the same discipline as the catalog envelope.
#[derive(Debug, Default, serde::Serialize, Deserialize)]
struct SourcesFile {
    #[serde(default)]
    sources: Vec<String>,
}

/// The registered third-party catalog URLs, in the order they were added — an unreadable or absent file
/// reads as none, never an error, the same way a missing cache does.
pub fn sources(paths: &Paths) -> Vec<String> {
    std::fs::read_to_string(sources_file(paths))
        .ok()
        .and_then(|json| serde_json::from_str::<SourcesFile>(&json).ok())
        .map(|f| f.sources)
        .unwrap_or_default()
}

/// The named cache file for a third-party catalog, derived from its URL so the same URL always maps to the
/// same file. Prefixed to sit beside — and never collide with — the official cache and the sources list.
fn source_cache_file(paths: &Paths, url: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(url.as_bytes());
    let short: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    paths.registry_dir().join(format!("source-{short}.json"))
}

/// Register a third-party catalog URL. Returns `true` if it was added, `false` if it was already
/// registered (idempotent). Refuses a URL that is not `http(s)://…`, and the official catalog's own URL —
/// the official catalog is not a third-party source and is always merged first anyway.
pub fn add_source(paths: &Paths, url: &str) -> Result<bool> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(Error::invalid(
            format!("a catalog URL must start with http:// or https://: {url}"),
            format!("カタログの URL は http:// か https:// で始まる必要があります：{url}"),
        ));
    }
    if url == catalog_url() {
        return Err(Error::invalid(
            "that is the official catalog's URL — it is always included and cannot be registered as a third-party source".to_string(),
            "それは公式カタログの URL です——常に含まれるため、サードパーティカタログとして登録できません。".to_string(),
        ));
    }
    let mut list = sources(paths);
    if list.iter().any(|u| u == url) {
        return Ok(false);
    }
    list.push(url.to_string());
    write_sources(paths, &list)?;
    Ok(true)
}

/// Unregister a third-party catalog URL, and remove its cached copy. Returns `true` if it was registered,
/// `false` if it was not (idempotent).
pub fn remove_source(paths: &Paths, url: &str) -> Result<bool> {
    let url = url.trim();
    let mut list = sources(paths);
    let before = list.len();
    list.retain(|u| u != url);
    if list.len() == before {
        return Ok(false);
    }
    write_sources(paths, &list)?;
    // Best-effort: a leftover cache is harmless (nothing merges it once unregistered), so a failed
    // removal is not worth failing the command over.
    let _ = std::fs::remove_file(source_cache_file(paths, url));
    Ok(true)
}

/// Write the sources list atomically.
fn write_sources(paths: &Paths, sources: &[String]) -> Result<()> {
    let dest = sources_file(paths);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&SourcesFile { sources: sources.to_vec() })
        .map_err(|e| Error::Io(std::io::Error::other(e)))?;
    let tmp = dest.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &dest)?;
    Ok(())
}

/// One catalog's contribution to a [`Discovery`] — what a `plugin catalog list` reports per index.
#[derive(Clone, Debug)]
pub struct DiscoveredSource {
    /// The catalog's URL, or [`OFFICIAL_CATALOG_URL`] for the official one.
    pub url: String,
    /// Whether this is the official catalog (always merged first, always wins a name clash).
    pub official: bool,
    /// Whether the catalog answered at all — from the network or, failing that, its cache. `false` is a
    /// registered source that has never been reached and holds no cache: it contributes nothing, but it
    /// stays registered.
    pub reachable: bool,
    /// How many entries this catalog offered (before cross-catalog de-duplication) — `0` when it was not
    /// reachable.
    pub offered: usize,
}

/// One entry as browsing sees it: the catalog entry, plus which catalog served it.
///
/// Provenance belongs to the merge, not to the document: a catalog file says nothing about where it
/// itself came from, and once the fold is done every entry sits in one list with that fact otherwise
/// lost. It is not decoration — the trust layers are two independent axes (`AMB-D-347`), and only one
/// of them rides on the manifest. Whether the author is the amenbo team is
/// [`Manifest::official`](crate::plugin_manifest::Manifest::official); whether an entry was reviewed
/// into the official index is this.
#[derive(Clone, Debug)]
pub struct DiscoveredEntry {
    /// The entry as its catalog published it.
    pub entry: Entry,
    /// The URL of the catalog that served it.
    pub source: String,
    /// Whether [`source`](DiscoveredEntry::source) is the official catalog — the entry passed review
    /// onto the official index. Said once here so no caller has to re-derive it by comparing URLs.
    /// An official plugin is always listed too; a listed one is not necessarily official.
    pub listed: bool,
}

/// The merged catalog a user browses: the official catalog plus every registered third-party one, folded
/// into a single de-duplicated list (`AMB-T-1980`). This is the discovery view only — see the module note
/// on why install and update do not use it.
#[derive(Clone, Debug)]
pub struct Discovery {
    /// Every entry across the merged catalogs, official first then each source in registration order, with
    /// a name that already appeared dropped in favour of the earlier one.
    pub entries: Vec<DiscoveredEntry>,
    /// Each catalog that went into the merge, and what it contributed.
    pub sources: Vec<DiscoveredSource>,
    /// Entries dropped during the merge: each catalog's own intake drops ([`Dropped`]), plus a
    /// [`Dropped::Duplicate`] for a name a later catalog repeated.
    pub dropped: Vec<Dropped>,
}

/// Merge the official catalog with every registered third-party catalog for browsing (`AMB-T-1980`).
///
/// Each catalog is read the incidental way ([`fresh`]-style): a cache inside the freshness window answers
/// without a request, so listing many sources does not mean many fetches. A source that cannot be reached
/// and has no cache contributes nothing and is marked unreachable — one dead URL does not cost the view.
/// The official catalog is merged first and wins every name clash, so a third-party catalog cannot shadow
/// an official plugin in what the user sees. Each entry keeps the catalog it came from
/// ([`DiscoveredEntry`]), which the fold is the only place that still knows.
pub fn discover(paths: &Paths) -> Discovery {
    let mut entries: Vec<DiscoveredEntry> = Vec::new();
    let mut sources_meta: Vec<DiscoveredSource> = Vec::new();
    let mut dropped: Vec<Dropped> = Vec::new();

    let official_url = catalog_url();
    let mut fold = |url: String, official: bool, catalog: Result<Catalog>| {
        match catalog {
            Ok(catalog) => {
                let offered = catalog.entries.len();
                dropped.extend(catalog.dropped);
                for entry in catalog.entries {
                    if entries.iter().any(|e| e.entry.manifest.name == entry.manifest.name) {
                        dropped.push(Dropped::Duplicate { name: entry.manifest.name });
                    } else {
                        entries.push(DiscoveredEntry {
                            entry,
                            source: url.clone(),
                            listed: official,
                        });
                    }
                }
                sources_meta.push(DiscoveredSource { url, official, reachable: true, offered });
            }
            Err(_) => {
                sources_meta.push(DiscoveredSource { url, official, reachable: false, offered: 0 });
            }
        }
    };

    fold(official_url.clone(), true, fresh_to(&official_url, &cache_file(paths)));
    for url in sources(paths) {
        let catalog = fresh_to(&url, &source_cache_file(paths, &url));
        fold(url, false, catalog);
    }

    Discovery { entries, sources: sources_meta, dropped }
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
        // `repo` and not `checksum`: a manifest may legitimately carry no top-level checksum now that it
        // can publish one per OS (`AMB-D-381`), so a missing one is a rule the validator breaks it on
        // (dropped as Invalid), not a shape serde cannot read.
        let mut broken = entry_json("broken");
        broken.as_object_mut().unwrap().remove("repo");
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
        write_cache_at(&cache_file(&paths), &catalog_json(vec![entry_json("worktree")])).unwrap();
        assert_eq!(cache_file(&paths), paths.registry_dir().join("official.json"));
        let catalog = cached(&paths).expect("the cache reads back");
        assert!(catalog.find("worktree").is_some());
    }

    #[test]
    fn a_corrupt_cache_reads_as_no_cache() {
        let paths = paths_at("corrupt");
        write_cache_at(&cache_file(&paths), "half a file").unwrap();
        assert!(cached(&paths).is_none(), "unreadable is 'fetch again', not an error to show");
    }

    #[test]
    fn writing_the_cache_leaves_no_temporary_behind() {
        let paths = paths_at("no-temp");
        write_cache_at(&cache_file(&paths), &catalog_json(vec![])).unwrap();
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
        write_cache_at(&cache_file(&paths), &catalog_json(vec![entry_json("worktree")])).unwrap();
        let catalog = load_to(UNREACHABLE, &cache_file(&paths)).expect("the cache answers");
        assert!(catalog.find("worktree").is_some());
        assert!(
            refresh_to(UNREACHABLE, &cache_file(&paths)).is_err(),
            "a refresh still reports the failure"
        );
        assert!(cached(&paths).is_some(), "and a failed fetch never clears the cache");
    }

    #[test]
    fn load_fails_only_when_there_is_neither_network_nor_cache() {
        let paths = paths_at("nothing");
        assert!(load_to(UNREACHABLE, &cache_file(&paths)).is_err());
    }

    #[test]
    #[ignore = "reaches the published catalog over the network"]
    fn the_live_catalog_answers_and_comes_through_intake_whole() {
        // The end-to-end this module cannot assert in CI: the real URL, the real envelope, the real
        // intake. Run it by hand (`cargo nextest run -p amenbo-core plugin_catalog -- --ignored`) when
        // the catalog's producer changes.
        let paths = paths_at("live");
        let catalog =
            refresh_to(OFFICIAL_CATALOG_URL, &cache_file(&paths)).expect("the published catalog answers");
        assert!(catalog.dropped.is_empty(), "nothing published had to be dropped: {:?}", catalog.dropped);
        assert!(cached(&paths).is_some(), "and the fetch replaced the cache");
    }

    /// A cache younger than the freshness boundary answers on its own — no request is made, which is what
    /// makes a check cheap enough to offer alongside a listing (`AMB-D-359`). The proof is that what comes
    /// back is the copy on disk: a fetch would have replaced it with what the real catalog serves.
    #[test]
    fn a_cache_inside_the_freshness_boundary_answers_without_the_network() {
        let paths = paths_at("fresh");
        write_cache_at(&cache_file(&paths), &catalog_json(vec![entry_json("only-on-disk")])).unwrap();

        let catalog = fresh(&paths).expect("the cache answers");
        let names: Vec<_> = catalog.entries.iter().map(|e| e.manifest.name.as_str()).collect();
        assert_eq!(names, vec!["only-on-disk"]);
    }

    /// With no cache at all there is no age to read, so the boundary cannot vouch for anything and the
    /// read falls through to a fetch.
    #[test]
    fn no_cache_has_no_age() {
        assert!(cache_age_at(&cache_file(&paths_at("ageless"))).is_none());
    }

    // ---- registered third-party catalogs, and the merged discovery view (`AMB-T-1980`) ----

    #[test]
    fn sources_round_trip_and_are_idempotent() {
        let paths = paths_at("sources-round-trip");
        assert!(sources(&paths).is_empty(), "none registered yet");

        assert!(add_source(&paths, "https://example.invalid/a/catalog.json").unwrap(), "added");
        assert!(add_source(&paths, "https://example.invalid/b/catalog.json").unwrap(), "added");
        assert!(
            !add_source(&paths, "https://example.invalid/a/catalog.json").unwrap(),
            "a second add of the same URL is a no-op"
        );
        assert_eq!(
            sources(&paths),
            vec![
                "https://example.invalid/a/catalog.json".to_string(),
                "https://example.invalid/b/catalog.json".to_string(),
            ],
            "kept in registration order"
        );

        assert!(remove_source(&paths, "https://example.invalid/a/catalog.json").unwrap(), "removed");
        assert!(
            !remove_source(&paths, "https://example.invalid/a/catalog.json").unwrap(),
            "removing what is not registered is a no-op"
        );
        assert_eq!(sources(&paths), vec!["https://example.invalid/b/catalog.json".to_string()]);
    }

    #[test]
    fn add_source_refuses_a_non_http_url_and_the_official_catalog() {
        let paths = paths_at("sources-refuse");
        assert!(add_source(&paths, "ftp://example.invalid/catalog.json").is_err(), "not http(s)");
        assert!(add_source(&paths, "just-a-name").is_err(), "not a URL");
        assert!(add_source(&paths, OFFICIAL_CATALOG_URL).is_err(), "the official catalog is not a source");
        assert!(sources(&paths).is_empty(), "nothing refused was registered");
    }

    #[test]
    fn removing_a_source_drops_its_cached_copy() {
        let paths = paths_at("sources-drop-cache");
        let url = "https://example.invalid/x/catalog.json";
        add_source(&paths, url).unwrap();
        write_cache_at(&source_cache_file(&paths, url), &catalog_json(vec![entry_json("x")])).unwrap();
        assert!(source_cache_file(&paths, url).exists());
        remove_source(&paths, url).unwrap();
        assert!(!source_cache_file(&paths, url).exists(), "the cache goes with the registration");
    }

    /// A source's cache file is derived from its URL, so the same URL always maps to the same file and two
    /// different URLs do not collide — nor with the official cache or the sources list.
    #[test]
    fn a_source_cache_file_is_stable_per_url_and_distinct() {
        let paths = paths_at("sources-cache-name");
        let a = source_cache_file(&paths, "https://example.invalid/a/catalog.json");
        let b = source_cache_file(&paths, "https://example.invalid/b/catalog.json");
        assert_eq!(a, source_cache_file(&paths, "https://example.invalid/a/catalog.json"), "stable");
        assert_ne!(a, b, "different URLs, different files");
        assert_ne!(a, cache_file(&paths), "never the official cache");
        assert_ne!(a, sources_file(&paths), "never the sources list");
    }

    /// `discover` merges the official catalog with every registered source, official first, and an official
    /// name wins a clash with a third-party one — a third-party catalog cannot shadow an official plugin.
    #[test]
    fn discover_merges_with_the_official_catalog_winning_a_name_clash() {
        let paths = paths_at("discover-merge");
        // Fresh caches so the merge reads from disk and never touches the network.
        write_cache_at(&cache_file(&paths), &catalog_json(vec![entry_json("worktree")])).unwrap();
        let src = "https://example.invalid/third/catalog.json";
        add_source(&paths, src).unwrap();
        let mut impostor = entry_json("worktree");
        impostor["desc"] = serde_json::json!("a third-party impostor");
        write_cache_at(
            &source_cache_file(&paths, src),
            &catalog_json(vec![impostor, entry_json("extra")]),
        )
        .unwrap();

        let discovery = discover(&paths);
        let names: Vec<_> =
            discovery.entries.iter().map(|e| e.entry.manifest.name.as_str()).collect();
        assert_eq!(names, vec!["worktree", "extra"], "official first, then the source's fresh entry");
        assert_eq!(
            discovery.entries[0].entry.manifest.desc, "a plugin",
            "the official 'worktree' held; the impostor did not displace it"
        );
        assert!(discovery.entries[0].listed, "the official catalog served it");
        assert!(!discovery.entries[1].listed, "the third-party source served this one");
        assert_eq!(discovery.entries[1].source, src, "and it says which catalog that was");
        assert!(
            matches!(discovery.dropped.as_slice(), [Dropped::Duplicate { name }] if name == "worktree"),
            "the shadowed third-party entry is recorded as a drop: {:?}",
            discovery.dropped
        );
        assert_eq!(discovery.sources.len(), 2, "official plus the one source");
        assert!(discovery.sources[0].official && discovery.sources[0].reachable);
        assert_eq!(discovery.sources[1].offered, 2, "the source offered two before de-duplication");
    }

    /// A registered source that cannot be reached and holds no cache contributes nothing and is marked
    /// unreachable — one dead URL does not cost the view. (`UNREACHABLE` refuses fast, no timeout wait.)
    #[test]
    fn discover_marks_an_unreachable_source_without_losing_the_rest() {
        let paths = paths_at("discover-unreachable");
        write_cache_at(&cache_file(&paths), &catalog_json(vec![entry_json("worktree")])).unwrap();
        add_source(&paths, UNREACHABLE).unwrap();

        let discovery = discover(&paths);
        assert_eq!(discovery.entries.len(), 1, "the official catalog still answers");
        let dead = discovery.sources.iter().find(|s| s.url == UNREACHABLE).expect("listed");
        assert!(!dead.reachable && dead.offered == 0, "the dead source contributes nothing");
    }
}
