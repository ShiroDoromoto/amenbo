//! The plugin catalog as amenbo holds it — **one** fetch of the whole index, cached on disk
//! (`AMB-D-347`), and one small document per plugin someone actually opens or installs (`AMB-D-385`).
//!
//! The unit of retrieval is the catalog, never the plugin: the catalog repository's CI aggregates every
//! reviewed manifest into a single `catalog.json` on a static host, and amenbo fetches that one file.
//! Browsing, searching, sorting and paging then happen entirely on the local copy — no per-plugin request,
//! so the cost of discovery does not grow with the number of plugins.
//!
//! What comes back is an envelope — `catalog_v` (the envelope's version), `generated_at`, and `plugins` —
//! whose entries are **list entries** ([`crate::plugin_wire::ListEntry`]): what a browse view draws, plus
//! the two fields only the catalog can supply. `added_at` is the day the manifest first appeared in the
//! index, which has no other source — a client holds no git history of the catalog repository, so "new"
//! is knowable only because the CI writes it. `detail_sum` is the digest of that plugin's detail document
//! (`AMB-D-386`), which is what keeps update detection riding on this one fetch.
//!
//! **What an install needs is a second document**, `plugins/<name>.json` beside the catalog ([`detail`],
//! `AMB-D-385`): the url, checksums, signature and contract versions, fetched for the one plugin being
//! opened or installed rather than carried for all of them. The signature alone is larger than everything
//! a list draws, and every reader would otherwise pay for every plugin's.
//!
//! **The delivery path is not trusted** (`AMB-D-354`). Every entry is run through
//! [`crate::plugin_validate`] here, on the way in, and one that does not parse or does not validate is
//! dropped — the rest of the catalog stays usable. Dropping is recorded ([`Dropped`]) rather than
//! silent, so a catalog that is quietly losing entries is visible. A detail document is checked the same
//! way where it is used: it must be the one the entry asked for, and — when the entry declares a digest —
//! the bytes it declared. The envelope itself is fail-closed the other way: a `catalog_v` from the future
//! is refused whole, because amenbo cannot know what a newer envelope means.
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
//! the registry directory, not the directory itself. A registration carries the key that catalog publishes,
//! pinned on the user's consent (`AMB-D-389`, [`Source`]), which is what an install off it will verify
//! against; teaching install and update to resolve across the merged view is its own change.

use crate::config::Paths;
use crate::error::{Error, Result};
use crate::plugin_validate::{validate_list_entry, Problem};
use crate::plugin_wire::{Detail, ListEntry};
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

/// What the official catalog is called where every catalog is named ([`DiscoveredSource`]). It is not
/// registered and has no record to hold a name, but a list that names the others and leaves this one as a
/// URL reads as though it were the odd one out.
pub const OFFICIAL_CATALOG_NAME: &str = "amenbo";

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
    pub entries: Vec<ListEntry>,
    /// The entries that did not, and why.
    pub dropped: Vec<Dropped>,
}

impl Catalog {
    /// Find one entry by plugin name — what an install resolves against (`AMB-T-2050`), and the entry
    /// whose detail document it then fetches.
    pub fn find(&self, name: &str) -> Option<&ListEntry> {
        self.entries.iter().find(|e| e.name == name)
    }
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

    let mut entries: Vec<ListEntry> = Vec::new();
    let mut dropped = Vec::new();
    for (index, value) in envelope.plugins.into_iter().enumerate() {
        let entry: ListEntry = match serde_json::from_value(value) {
            Ok(e) => e,
            Err(e) => {
                dropped.push(Dropped::Unreadable { index, error: e.to_string() });
                continue;
            }
        };
        let problems = validate_list_entry(&entry);
        if !problems.is_empty() {
            dropped.push(Dropped::Invalid { name: entry.name, problems });
            continue;
        }
        if entries.iter().any(|e| e.name == entry.name) {
            dropped.push(Dropped::Duplicate { name: entry.name });
            continue;
        }
        entries.push(entry);
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

// ---- the detail document, fetched one plugin at a time (`AMB-D-385`) ----
//
// The other half of a catalog entry: what an install needs and a list does not. It is fetched for the one
// plugin being opened or installed, which is the same lazy shape `AMB-D-347` already takes for a plugin's
// stars and download counts — no new principle, and the list stays one fetch for everyone.

/// The directory-relative name of one plugin's detail document, under the same base as `catalog.json`.
fn detail_path(name: &str) -> String {
    format!("plugins/{name}.json")
}

/// Where one plugin's detail document is published: alongside `catalog.json`, under `plugins/`. Derived
/// from the catalog URL rather than configured separately, so an override that points amenbo at a test
/// catalog (`AMENBO_PLUGIN_CATALOG_URL`) moves both documents together — one address, one publisher.
fn detail_url(catalog_url: &str, name: &str) -> String {
    match catalog_url.rsplit_once('/') {
        Some((base, _file)) => format!("{base}/{}", detail_path(name)),
        None => detail_path(name),
    }
}

/// Where one plugin's detail document is cached: `<base>/plugins/registry/detail-<name>.json`, beside the
/// catalog's own cache. Named after the plugin, because that is what it is fetched by — and prefixed so it
/// can never collide with the catalog cache, the sources list, or a third-party catalog's cache.
fn detail_cache_file(paths: &Paths, name: &str) -> PathBuf {
    paths.registry_dir().join(format!("detail-{name}.json"))
}

/// The detail document for one listed plugin — the half of its catalog entry an install needs
/// (`AMB-D-385`).
///
/// Fetched fresh, with the cached copy answering when the network does not, exactly as [`load`] does for
/// the list: being offline costs freshness, not function, and a fetch that returns garbage never replaces
/// what is cached.
///
/// **Two things are checked before it is anything but bytes** (`AMB-D-354`, the delivery path is not
/// trusted). The document must name the plugin it was fetched for — the `name` in both documents is the
/// join, and a detail that names another plugin is not an answer to what was asked. And when the entry
/// declares a `detail_sum` (`AMB-D-386`), the bytes must hash to it: it is what the entry says this
/// plugin's detail *is*, so a document that fails it is a mismatched pair, not a newer one. That check is
/// also what makes the cached copy safe to fall back on — a cache from a previous publication does not
/// answer for the entry in hand, so it is passed over as if it were not there.
///
/// This is not the trust root. The asset's signature and checksum are (`AMB-D-371`), verified at install
/// over the bytes the URL served; a swapped detail buys nothing, because its asset still has to pass that
/// door.
pub fn detail(paths: &Paths, entry: &ListEntry) -> Result<Detail> {
    detail_to(&catalog_url(), &detail_cache_file(paths, &entry.name), entry)
}

/// The detail document for one entry of the **merged** view — fetched from the catalog that served the
/// entry, not from the official one (`AMB-D-389`).
///
/// The join is the address: a detail document lives beside its own `catalog.json`, so which catalog an
/// entry came from is what says where its second half is. Asking the official catalog for a third-party
/// plugin's detail would be asking the wrong publisher — a 404 at best, and at worst another plugin's
/// document under a name that happened to match.
pub fn detail_of(paths: &Paths, found: &DiscoveredEntry) -> Result<Detail> {
    let cache = if found.listed {
        detail_cache_file(paths, &found.entry.name)
    } else {
        source_detail_cache_file(paths, &found.source, &found.entry.name)
    };
    detail_to(&found.source, &cache, &found.entry)
}

/// Where a registered catalog's detail document is cached: named after both the catalog and the plugin,
/// so two catalogs offering the same name do not overwrite each other's — and neither touches the
/// official catalog's ([`detail_cache_file`]).
fn source_detail_cache_file(paths: &Paths, source: &str, name: &str) -> PathBuf {
    paths.registry_dir().join(format!("detail-{}-{name}.json", url_tag(source)))
}

/// [`detail`] against a named catalog URL and cache file — the seam a test drives, the same shape the
/// list's [`load_to`] takes.
fn detail_to(catalog_url: &str, cache_file: &std::path::Path, entry: &ListEntry) -> Result<Detail> {
    let url = detail_url(catalog_url, &entry.name);
    match fetch(&url) {
        Ok(json) => {
            let detail = read_detail(&json, entry)?;
            write_cache_at(cache_file, &json)?;
            Ok(detail)
        }
        Err(offline) => std::fs::read_to_string(cache_file)
            .ok()
            .and_then(|json| read_detail(&json, entry).ok())
            .ok_or(offline),
    }
}

/// One detail document, checked against the entry that named it (see [`detail`]).
fn read_detail(json: &str, entry: &ListEntry) -> Result<Detail> {
    if let Some(sum) = &entry.detail_sum {
        crate::plugin_provenance::verify_checksum(json.as_bytes(), sum).map_err(|_| {
            Error::invalid(
                format!(
                    "the catalog's detail for '{}' is not the document the catalog listed ({sum})",
                    entry.name
                ),
                format!(
                    "カタログの '{}' の詳細が、一覧の記載（{sum}）と一致しません",
                    entry.name
                ),
            )
        })?;
    }
    let detail: Detail = serde_json::from_str(json).map_err(|e| {
        Error::invalid(
            format!("the catalog's detail for '{}' is not readable: {e}", entry.name),
            format!("カタログの '{}' の詳細を読み取れません：{e}", entry.name),
        )
    })?;
    if detail.name != entry.name {
        return Err(Error::invalid(
            format!(
                "the catalog's detail for '{}' names another plugin ('{}')",
                entry.name, detail.name
            ),
            format!(
                "カタログの '{}' の詳細が別のプラグイン（'{}'）を名乗っています",
                entry.name, detail.name
            ),
        ));
    }
    Ok(detail)
}

// ---- registered third-party catalogs, and the merged view (`AMB-T-1980`) ----
//
// A third-party catalog is a second index the same shape as the official one, at the author's own URL
// (`AMB-D-347`, the "free" tier). The unit registered is the **catalog**, never the plugin: what grows is
// the number of indexes (few), not per-plugin requests (many). The list of registrations is a small file
// in the registry directory; each catalog is fetched and cached in its own named file beside the
// official one, and `discover` merges them for browsing.
//
// **Registering is a trust decision, not a bookmark** (`AMB-D-389`). A registration holds the key the
// catalog published beside its `catalog.json`, taken once and pinned, with the fingerprint put in front
// of the person agreeing to it (`probe_source` works out what is being agreed to; `add_source` writes
// it). What that key is *for* is the install path: an asset off a registered catalog verifies against
// **that catalog's** key rather than the one amenbo ships, so the trust root stays "the keeper of the
// shelf vouched for what is on it" one shelf down. A catalog that publishes no key is browsable and
// installs nothing.
//
// A pin is compared only where it can mean something: at registration, where a changed key is refused
// rather than swallowed, and at install, over an asset's signature. The catalog document itself carries
// no signature (see the module note), so re-fetching the key on every browse would cost a request per
// catalog and prove nothing.
//
// Official entries always win a name clash in the merge, as they did when the merge was browsing only.

/// The name, under [`Paths::registry_dir`], of the list of registered third-party catalogs.
pub const SOURCES_FILE_NAME: &str = "sources.json";

/// What a catalog publishes its own public key under, beside its `catalog.json` (`AMB-D-389`) — the
/// document a registration reads to pin.
pub const CATALOG_KEY_FILE_NAME: &str = "catalog-key.pub";

/// The registered-sources file, `<base>/plugins/registry/sources.json`.
fn sources_file(paths: &Paths) -> PathBuf {
    paths.registry_dir().join(SOURCES_FILE_NAME)
}

/// One registered catalog: where it is, what the user calls it, and the key its assets are verified
/// against (`AMB-D-389`).
///
/// The key is what makes registering more than a bookmark. Trust for an official plugin rests on the key
/// amenbo ships (`AMB-D-371`); for a registered catalog it rests on **that catalog's** key, taken at
/// registration and pinned, so what installs later is what the same publisher signed. `key` of `None` is
/// a catalog that published none: browsable, and nothing from it installs.
///
/// The fingerprint is not stored — it is [`Source::fingerprint`], read off the key. One truth on disk,
/// and no way for a file to say two things about one key.
#[derive(Clone, Debug, serde::Serialize, Deserialize)]
pub struct Source {
    /// The URL of this catalog's `catalog.json`.
    pub url: String,
    /// What to call it on screen. Given at registration; the host of its URL when nothing was given.
    pub name: String,
    /// The pinned minisign public key, or `None` for a catalog that published none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

impl Source {
    /// The short form of the pinned key, for a human comparing it with what the publisher says
    /// ([`crate::plugin_provenance::key_fingerprint`]). `None` when nothing is pinned — and also when
    /// what is pinned will not parse, which a face shows as "no fingerprint" rather than as a key.
    pub fn fingerprint(&self) -> Option<String> {
        self.key.as_deref().and_then(|k| crate::plugin_provenance::key_fingerprint(k).ok())
    }
}

/// The on-disk shape of the sources list — an envelope so a field can be added later without an older
/// build misreading it, the same discipline as the catalog envelope.
#[derive(Debug, Default, Deserialize)]
struct SourcesFile {
    #[serde(default)]
    sources: Vec<StoredSource>,
}

/// A record as the file may hold it. amenbo wrote a bare URL before a catalog was more than an address,
/// and those registrations stay registered: they read as a source with no pinned key, which is exactly
/// what they are — the user consented to seeing the catalog, never to a key. Pinning one is a new
/// consent, taken the next time they register it.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StoredSource {
    Record {
        url: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        key: Option<String>,
    },
    Url(String),
}

impl From<StoredSource> for Source {
    fn from(stored: StoredSource) -> Source {
        match stored {
            StoredSource::Record { url, name, key } => {
                let name = name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| default_name(&url));
                Source { url, name, key }
            }
            StoredSource::Url(url) => {
                let name = default_name(&url);
                Source { url, name, key: None }
            }
        }
    }
}

/// What a catalog is called when the user names nothing: the host it is served from — the part of the
/// URL they typed that says who is answering.
fn default_name(url: &str) -> String {
    url.split_once("://")
        .and_then(|(_scheme, rest)| rest.split('/').next())
        .filter(|host| !host.is_empty())
        .unwrap_or(url)
        .to_string()
}

/// The registered third-party catalogs, in the order they were added — an unreadable or absent file
/// reads as none, never an error, the same way a missing cache does.
pub fn sources(paths: &Paths) -> Vec<Source> {
    std::fs::read_to_string(sources_file(paths))
        .ok()
        .and_then(|json| serde_json::from_str::<SourcesFile>(&json).ok())
        .map(|f| f.sources.into_iter().map(Source::from).collect())
        .unwrap_or_default()
}

/// The named cache file for a third-party catalog, derived from its URL so the same URL always maps to the
/// same file. Prefixed to sit beside — and never collide with — the official cache and the sources list.
fn source_cache_file(paths: &Paths, url: &str) -> PathBuf {
    paths.registry_dir().join(format!("source-{}.json", url_tag(url)))
}

/// A short, stable, file-name-safe tag for a URL — how a catalog's own files are told apart on disk
/// without the URL itself having to be one.
fn url_tag(url: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(url.as_bytes()).iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// What registering a catalog would mean, worked out before anything is written — the material the
/// consent is given against (`AMB-D-389`).
///
/// Registering is not a bookmark: it adds a trust root, and the fingerprint is what the person agreeing
/// is agreeing to. So the key is fetched, the URL is judged, and an existing pin is compared, all in one
/// place that writes nothing — a face shows this, asks, and only then calls [`add_source`].
#[derive(Clone, Debug)]
pub struct SourceProbe {
    /// The URL, trimmed — the form that will be registered.
    pub url: String,
    /// What to call it if the user offers no name of their own.
    pub suggested_name: String,
    /// The key this catalog publishes, or `None` when it publishes none amenbo could fetch.
    pub key: Option<String>,
    /// The short form of [`key`](SourceProbe::key) to show while asking.
    pub fingerprint: Option<String>,
    /// Whether this URL is already registered — a second registration of the same catalog changes
    /// nothing unless it is bringing a key the record does not have yet.
    pub registered: bool,
    /// What that record already pins, if anything. Never a *different* key from
    /// [`key`](SourceProbe::key): [`probe_source`] refuses before it gets here.
    pub pinned: Option<String>,
}

impl SourceProbe {
    /// Whether registering this would pin a key that is not pinned yet — the one case that needs a
    /// human's consent, because it is the one that adds a trust root.
    pub fn pins_a_new_key(&self) -> bool {
        self.key.is_some() && self.pinned.is_none()
    }
}

/// Work out what registering `url` would mean, without writing anything (see [`SourceProbe`]).
///
/// Refuses a URL that is not `http(s)://…`, and the official catalog's own URL — the official catalog is
/// not a third-party source and is always merged first anyway. Refuses, too, a catalog whose published
/// key will not parse: a broken key document is a signal, not an absence.
///
/// **A key that changed is refused here, and that is the pin doing its job** (`AMB-D-389`). A publisher
/// who rotates their key is asking for trust again, so amenbo will not swallow the new one on the
/// strength of the old consent: the way through is to unregister and register again, which puts the new
/// fingerprint in front of the person deciding.
pub fn probe_source(paths: &Paths, url: &str) -> Result<SourceProbe> {
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
    let existing = sources(paths).into_iter().find(|s| s.url == url);
    let key = published_key(url)?;
    check_pin(url, existing.as_ref().and_then(|s| s.key.as_deref()), key.as_deref())?;
    let fingerprint = key.as_deref().and_then(|k| crate::plugin_provenance::key_fingerprint(k).ok());
    Ok(SourceProbe {
        suggested_name: existing.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| default_name(url)),
        pinned: existing.as_ref().and_then(|s| s.key.clone()),
        registered: existing.is_some(),
        url: url.to_string(),
        key,
        fingerprint,
    })
}

/// Judge a served key against what is pinned — the whole fail-closed rule, in one place with no I/O
/// (`AMB-D-389`).
///
/// - nothing pinned: anything is fine, and pinning it is the consent [`SourceProbe::pins_a_new_key`]
///   asks for. This is the first-use half of trust-on-first-use.
/// - pinned, and the same key came back: the ordinary case.
/// - pinned, and **nothing** came back: the catalog is unreachable or has stopped publishing its key.
///   The pin stands — it is what an install verifies against, and dropping it because a fetch failed
///   would turn being offline into a downgrade.
/// - pinned, and a **different** key came back: refused. That is the pin doing its job.
fn check_pin(url: &str, pinned: Option<&str>, served: Option<&str>) -> Result<()> {
    let (Some(pinned), Some(served)) = (pinned, served) else { return Ok(()) };
    if pinned == served {
        return Ok(());
    }
    let (was, now) = (fingerprint_of(pinned), fingerprint_of(served));
    Err(Error::invalid(
        format!(
            "{url} now publishes a different key ({now}, pinned: {was}). amenbo will not accept it on the old consent — unregister the catalog and register it again to trust the new key."
        ),
        format!(
            "{url} の鍵が登録時と変わっています（現在 {now} / 登録時 {was}）。以前の同意のままでは受け入れません——登録を解除し、登録し直して新しい鍵を信頼してください。"
        ),
    ))
}

/// A key's fingerprint for a message, falling back to the key itself when it will not parse — a refusal
/// has to name what it is refusing even when the thing is malformed.
fn fingerprint_of(key: &str) -> String {
    crate::plugin_provenance::key_fingerprint(key).unwrap_or_else(|_| key.to_string())
}

/// The key a catalog publishes beside its `catalog.json`, or `None` when it publishes none.
///
/// Nothing served at that address is a state, not a failure: a catalog can be browsed without one, and
/// being offline at registration lands in the same place. Only a document that is *there* and is not a
/// key refuses — see [`probe_source`].
fn published_key(catalog_url: &str) -> Result<Option<String>> {
    match fetch(&key_url(catalog_url)) {
        Ok(text) => crate::plugin_provenance::read_public_key(&text).map(Some).map_err(|e| {
            Error::invalid(
                format!("{}: {e}", key_url(catalog_url)),
                format!("{}：{e}", key_url(catalog_url)),
            )
        }),
        Err(_) => Ok(None),
    }
}

/// Where a catalog publishes its key: beside its `catalog.json`, the same way a detail document sits
/// under the same base — one address, one publisher.
fn key_url(catalog_url: &str) -> String {
    match catalog_url.rsplit_once('/') {
        Some((base, _file)) => format!("{base}/{CATALOG_KEY_FILE_NAME}"),
        None => CATALOG_KEY_FILE_NAME.to_string(),
    }
}

/// Register a catalog the way [`probe_source`] found it, under `name` (its suggested name when `None`).
///
/// Returns `true` when the registration changed — a new catalog, or a key pinned onto a record that had
/// none — and `false` when it was already registered exactly like this. Both are success: registering
/// twice is a no-op, not an error.
///
/// The probe is the argument rather than the URL because the key belongs to the consent that was just
/// given: taking a URL here would mean fetching the key a second time, which is a second chance for a
/// different one to arrive between what was shown and what is pinned.
pub fn add_source(paths: &Paths, probe: &SourceProbe, name: Option<&str>) -> Result<bool> {
    let name = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| probe.suggested_name.clone());
    let mut list = sources(paths);
    match list.iter_mut().find(|s| s.url == probe.url) {
        Some(existing) => {
            // A record that never held a key takes the one the probe found; a pin already there stands,
            // because a *different* key never reaches this point (`probe_source` refuses it).
            let pinning = probe.key.is_some() && existing.key.is_none();
            if !pinning && existing.name == name {
                return Ok(false);
            }
            if pinning {
                existing.key = probe.key.clone();
            }
            existing.name = name;
        }
        None => list.push(Source { url: probe.url.clone(), name, key: probe.key.clone() }),
    }
    write_sources(paths, &list)?;
    Ok(true)
}

/// Unregister a third-party catalog URL, and remove its cached copy. Returns `true` if it was registered,
/// `false` if it was not (idempotent). This is also how a pinned key is let go of: the pin is part of the
/// registration, so trusting a new one is registering again.
pub fn remove_source(paths: &Paths, url: &str) -> Result<bool> {
    let url = url.trim();
    let mut list = sources(paths);
    let before = list.len();
    list.retain(|s| s.url != url);
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
fn write_sources(paths: &Paths, sources: &[Source]) -> Result<()> {
    let dest = sources_file(paths);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&serde_json::json!({ "sources": sources }))
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
    /// What to call it: the name given at registration, or [`OFFICIAL_CATALOG_NAME`] for the official
    /// catalog.
    pub name: String,
    /// The fingerprint of the key this catalog's assets verify against (`AMB-D-389`) — pinned at
    /// registration, or amenbo's own embedded key for the official catalog. `None` is a catalog that
    /// published none: browsable, and nothing from it installs.
    pub fingerprint: Option<String>,
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredEntry {
    /// The entry as its catalog published it.
    pub entry: ListEntry,
    /// The URL of the catalog that served it.
    pub source: String,
    /// What that catalog is called ([`DiscoveredSource::name`]) — carried on the entry so a refusal or a
    /// receipt can name the publisher without the caller holding the source list open beside it.
    pub source_name: String,
    /// Whether [`source`](DiscoveredEntry::source) is the official catalog — the entry passed review
    /// onto the official index. Said once here so no caller has to re-derive it by comparing URLs.
    /// An official plugin is always listed too; a listed one is not necessarily official.
    pub listed: bool,
    /// The key this catalog's assets are verified against, when it has one. Private, because the point
    /// of it is that nobody chooses it: it is reached through [`DiscoveredEntry::trust_root`].
    key: Option<String>,
}

impl DiscoveredEntry {
    /// The trust root this entry's asset must verify against (`AMB-D-389`): amenbo's own key for the
    /// official catalog, the pinned key for a registered one.
    ///
    /// A registered catalog that published no key has no root, and that is a refusal rather than a
    /// fallback to amenbo's — falling back would mean an asset nobody's key signed passing the door
    /// (`AMB-D-351`). What such a catalog offers can be browsed and read about; installing from it takes
    /// the publisher publishing a key, and the user registering it again.
    pub fn trust_root(&self) -> Result<crate::plugin_provenance::TrustRoot> {
        if self.listed {
            return Ok(crate::plugin_provenance::TrustRoot::official());
        }
        match &self.key {
            Some(key) => Ok(crate::plugin_provenance::TrustRoot::pinned(key.clone())),
            None => Err(Error::invalid(
                format!(
                    "'{}' comes from {} ({}), which publishes no signing key — nothing from it can be installed. Ask its publisher for a catalog-key.pub, then register the catalog again.",
                    self.entry.name, self.source_name, self.source
                ),
                format!(
                    "'{}' の配布元 {}（{}）は署名鍵を公開していないため、そこからは install できません。配布元に catalog-key.pub の公開を求め、登録し直してください。",
                    self.entry.name, self.source_name, self.source
                ),
            )),
        }
    }
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

impl Discovery {
    /// A view standing for what one catalog served — the seam the install and update tests build on,
    /// since the fold that makes one in production needs a network.
    ///
    /// `key` is what that catalog is pinned with, and `None` on a third-party catalog is the one that
    /// publishes none. The official catalog answers to its own root and ignores it.
    #[cfg(test)]
    pub(crate) fn served_by(source: &str, official: bool, key: Option<&str>, catalog: Catalog) -> Discovery {
        Discovery {
            entries: catalog
                .entries
                .into_iter()
                .map(|entry| DiscoveredEntry {
                    entry,
                    source: source.to_string(),
                    source_name: source.to_string(),
                    listed: official,
                    key: key.map(str::to_string),
                })
                .collect(),
            sources: Vec::new(),
            dropped: catalog.dropped,
        }
    }

    /// Find one entry by plugin name across the merged view — what an install resolves against
    /// (`AMB-D-389`).
    ///
    /// There is nothing to choose between here: the fold already put the official catalog first and the
    /// registered ones in registration order, and dropped every repeat of a name it had seen. So the
    /// first match *is* the winner, and a name the official catalog carries can never resolve to a
    /// third-party entry.
    pub fn find(&self, name: &str) -> Option<&DiscoveredEntry> {
        self.entries.iter().find(|e| e.entry.name == name)
    }
}

/// Merge the official catalog with every registered third-party catalog for browsing (`AMB-T-1980`).
///
/// Each catalog is read the incidental way ([`fresh`]-style): a cache inside the freshness window answers
/// without a request, so listing many sources does not mean many fetches. A source that cannot be reached
/// and has no cache contributes nothing and is marked unreachable — one dead URL does not cost the view.
/// The official catalog is merged first and wins every name clash, so a third-party catalog cannot shadow
/// an official plugin in what the user sees. Each entry keeps the catalog it came from
/// ([`DiscoveredEntry`]), which the fold is the only place that still knows.
///
/// **The badge and the recommendation survive the merge only from the official index.** Both are the
/// index's word rather than the author's (`AMB-D-347`): `official` says the amenbo team wrote the plugin,
/// which the official index's CI establishes and refuses to take on a manifest's say-so, and `featured` is
/// that index's hand curation. A catalog anyone can publish is not where either question is answered —
/// letting it answer them for its own entries would hand out the strongest mark a reader trusts, and
/// self-promotion into the one ordering that is supposed to be a judgement. So both are cleared for every
/// third-party entry here, once, rather than left for each face to remember: what a face reads is already
/// the answer.
pub fn discover(paths: &Paths) -> Discovery {
    merge(paths, fresh_to)
}

/// The same merged view, read the way an install must read it (`AMB-D-389`).
///
/// [`discover`] is for a browse, so it answers from a cache inside the freshness window; this asks each
/// catalog the way [`load`] does — the network first, its cache when that fails. Installing is the
/// explicit act, and fetching a binary on the strength of what a cache said an hour ago is acting on
/// stale evidence.
///
/// Fails only when **no** catalog answered at all: nothing fetched and nothing cached anywhere. One
/// unreachable catalog is not a failure — its entries are simply not among the ones that can be resolved,
/// which is the same deal a browse gets.
pub fn for_install(paths: &Paths) -> Result<Discovery> {
    let merged = merge(paths, load_to);
    if merged.sources.iter().any(|s| s.reachable) {
        return Ok(merged);
    }
    Err(Error::Io(std::io::Error::other(
        "no plugin catalog could be read: none answered, and none is cached",
    )))
}

/// The merged view from what is already on disk — no network at all, and no failure either.
///
/// This is what a listing reads (`AMB-D-359`): a mark beside an installed plugin must cost nothing and
/// must work offline, so a catalog with no cache simply contributes nothing rather than being fetched or
/// reported. Every catalog is read this way, so a plugin installed from a registered catalog gets the
/// same mark as an official one instead of quietly never being flagged.
#[must_use]
pub fn cached_view(paths: &Paths) -> Discovery {
    merge(paths, |_url, cache| {
        cached_at(cache).ok_or_else(|| Error::Io(std::io::Error::other("no cached catalog")))
    })
}

/// Fold the official catalog and every registered one into a single view, each catalog read by `read`.
///
/// The two callers differ only in that function ([`fresh_to`] for a browse, [`load_to`] for an install),
/// and in nothing else: same order, same official-wins rule, same clearing of the marks only the official
/// index grants. Keeping the fold in one place is what stops the view an install resolves against from
/// drifting away from the one the user was looking at when they chose.
fn merge(paths: &Paths, read: impl Fn(&str, &std::path::Path) -> Result<Catalog>) -> Discovery {
    let mut entries: Vec<DiscoveredEntry> = Vec::new();
    let mut sources_meta: Vec<DiscoveredSource> = Vec::new();
    let mut dropped: Vec<Dropped> = Vec::new();

    let official_url = catalog_url();
    let mut fold = |url: String,
                    name: String,
                    fingerprint: Option<String>,
                    key: Option<String>,
                    official: bool,
                    catalog: Result<Catalog>| {
        let (reachable, offered) = match catalog {
            Ok(catalog) => {
                let offered = catalog.entries.len();
                dropped.extend(catalog.dropped);
                for mut entry in catalog.entries {
                    if entries.iter().any(|e| e.entry.name == entry.name) {
                        dropped.push(Dropped::Duplicate { name: entry.name });
                    } else {
                        entry.official &= official;
                        entry.featured &= official;
                        entries.push(DiscoveredEntry {
                            entry,
                            source: url.clone(),
                            source_name: name.clone(),
                            listed: official,
                            key: key.clone(),
                        });
                    }
                }
                (true, offered)
            }
            Err(_) => (false, 0),
        };
        sources_meta.push(DiscoveredSource { url, name, fingerprint, official, reachable, offered });
    };

    fold(
        official_url.clone(),
        OFFICIAL_CATALOG_NAME.to_string(),
        crate::plugin_provenance::key_fingerprint(crate::plugin_provenance::CATALOG_PUBLIC_KEY).ok(),
        None, // the official root is the embedded key, and is never read off a record
        true,
        read(&official_url, &cache_file(paths)),
    );
    for source in sources(paths) {
        let catalog = read(&source.url, &source_cache_file(paths, &source.url));
        fold(
            source.url.clone(),
            source.name.clone(),
            source.fingerprint(),
            source.key.clone(),
            false,
            catalog,
        );
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
            "added_at": "2026-07-23T04:23:48Z",
            "detail_sum": format!("sha256:{}", "a".repeat(64)),
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

    /// The values on an entry that no author writes: the day it entered the index, the digest of the
    /// document an install goes on to fetch (`AMB-D-386`), and the index's own recommendation.
    #[test]
    fn an_entry_keeps_the_fields_only_the_catalog_supplies() {
        let mut json = entry_json("worktree");
        json["featured"] = serde_json::json!(true);
        let catalog = parse(&catalog_json(vec![json])).unwrap();
        let entry = catalog.find("worktree").expect("the entry is there");
        assert_eq!(entry.added_at.as_deref(), Some("2026-07-23T04:23:48Z"), "the 'new' axis");
        assert_eq!(
            entry.detail_sum,
            Some(format!("sha256:{}", "a".repeat(64))),
            "the comparison material update detection is left with"
        );
        assert!(entry.featured, "the 'featured' axis");
    }

    /// An entry carries nothing an install needs, and intake does not ask it for any: a list entry with
    /// no url, checksum or signature anywhere on it is exactly what the catalog now publishes
    /// (`AMB-D-385`), and refusing it would empty the catalog.
    #[test]
    fn an_entry_without_a_distributable_is_a_perfectly_good_entry() {
        let catalog = parse(&catalog_json(vec![entry_json("worktree")])).unwrap();
        assert!(catalog.dropped.is_empty(), "nothing was dropped: {:?}", catalog.dropped);
        assert!(catalog.find("worktree").is_some());
    }

    /// The digest is the one field intake checks beyond what a browse view draws, because a malformed one
    /// is a comparison that could never be made.
    #[test]
    fn an_entry_whose_digest_is_not_a_digest_is_dropped() {
        let mut entry = entry_json("worktree");
        entry["detail_sum"] = serde_json::json!("whatever-the-ci-felt-like");
        let catalog = parse(&catalog_json(vec![entry])).unwrap();
        assert!(
            matches!(catalog.dropped.as_slice(), [Dropped::Invalid { name, problems }]
                if name == "worktree" && problems.iter().any(|p| p.location == "detail_sum")),
            "the field is named: {:?}",
            catalog.dropped
        );
    }

    /// A catalog built before the field existed says nothing about it, and nothing is what that means:
    /// an entry with no `featured` key is simply not recommended, not unreadable.
    #[test]
    fn an_entry_without_the_recommendation_reads_as_not_recommended() {
        let catalog = parse(&catalog_json(vec![entry_json("worktree")])).unwrap();
        assert!(!catalog.find("worktree").expect("the entry is there").featured);
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
        // `repo` is a field a list entry owes outright, so serde cannot read the entry at all — which is a
        // different drop from one that reads and then breaks a rule.
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
        assert_eq!(catalog.find("worktree").unwrap().desc, "a plugin", "the first one holds");
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
        let names: Vec<_> = catalog.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["only-on-disk"]);
    }

    /// With no cache at all there is no age to read, so the boundary cannot vouch for anything and the
    /// read falls through to a fetch.
    #[test]
    fn no_cache_has_no_age() {
        assert!(cache_age_at(&cache_file(&paths_at("ageless"))).is_none());
    }

    // ---- the detail document (`AMB-D-385`) ----

    fn detail_json(name: &str) -> String {
        serde_json::json!({
            "name": name,
            "url": "https://example.invalid/x.tar.gz",
            "checksum": format!("sha256:{}", "b".repeat(64)),
            "payload_v": 1,
        })
        .to_string()
    }

    /// The digest the catalog CI computes: sha256 over the bytes it publishes (`AMB-D-386`).
    fn sum_of(json: &str) -> String {
        use sha2::{Digest, Sha256};
        let hex: String = Sha256::digest(json.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
        format!("sha256:{hex}")
    }

    fn listed(name: &str, detail_sum: Option<String>) -> ListEntry {
        let mut entry: ListEntry = serde_json::from_value(entry_json(name)).unwrap();
        entry.detail_sum = detail_sum;
        entry
    }

    /// The two documents are published together, so the address of one is the address of the other: an
    /// override that points amenbo at a test catalog moves the details with it.
    #[test]
    fn a_detail_sits_beside_the_catalog_that_listed_it() {
        assert_eq!(
            detail_url("https://example.invalid/amenbo-plugins/catalog.json", "worktree"),
            "https://example.invalid/amenbo-plugins/plugins/worktree.json"
        );
    }

    #[test]
    fn a_detail_that_names_another_plugin_is_refused() {
        let entry = listed("worktree", None);
        let err = read_detail(&detail_json("slack"), &entry).unwrap_err();
        assert!(format!("{err:?}").contains("slack"), "the name it did carry is shown: {err:?}");
    }

    /// The entry says what this plugin's detail is; a document that does not hash to it is a mismatched
    /// pair, and taking it would install from bytes the list never vouched for.
    #[test]
    fn a_detail_that_is_not_the_one_listed_is_refused() {
        let json = detail_json("worktree");
        assert!(read_detail(&json, &listed("worktree", Some(sum_of(&json)))).is_ok(), "the pair agrees");

        let entry = listed("worktree", Some(sum_of("some other document")));
        assert!(read_detail(&json, &entry).is_err(), "and a pair that does not, does not install");
    }

    /// An entry with no digest is not refused for it — the check is over what the catalog declared, and a
    /// catalog that declares nothing is one whose plugins simply never report an update.
    #[test]
    fn an_entry_with_no_digest_takes_its_detail_as_it_comes() {
        assert!(read_detail(&detail_json("worktree"), &listed("worktree", None)).is_ok());
    }

    #[test]
    fn the_cached_detail_answers_when_the_fetch_fails() {
        let paths = paths_at("detail-offline");
        let json = detail_json("worktree");
        let entry = listed("worktree", Some(sum_of(&json)));
        write_cache_at(&detail_cache_file(&paths, "worktree"), &json).unwrap();

        let detail =
            detail_to(UNREACHABLE, &detail_cache_file(&paths, "worktree"), &entry).expect("cached");
        assert_eq!(detail.checksum, format!("sha256:{}", "b".repeat(64)));
    }

    /// A cached detail from a previous publication does not answer for the entry in hand: it is passed
    /// over as if there were no cache, rather than installed from as though it were current.
    #[test]
    fn a_cached_detail_the_entry_no_longer_lists_is_passed_over() {
        let paths = paths_at("detail-stale");
        write_cache_at(&detail_cache_file(&paths, "worktree"), &detail_json("worktree")).unwrap();
        let moved_on = listed("worktree", Some(sum_of("what the catalog publishes now")));

        assert!(detail_to(UNREACHABLE, &detail_cache_file(&paths, "worktree"), &moved_on).is_err());
    }

    /// The detail cache is named per plugin and never collides with the catalog's own cache, the sources
    /// list, or a third-party catalog's cache.
    #[test]
    fn a_detail_cache_file_is_distinct_from_every_other_file_in_the_registry() {
        let paths = paths_at("detail-cache-name");
        let a = detail_cache_file(&paths, "worktree");
        assert_ne!(a, detail_cache_file(&paths, "slack"));
        assert_ne!(a, cache_file(&paths));
        assert_ne!(a, sources_file(&paths));
        assert_ne!(a, source_cache_file(&paths, "https://example.invalid/a/catalog.json"));

        // Two catalogs may publish the same plugin name, and each one's detail document is its own
        // (`AMB-D-389`) — filing them under one name would serve one catalog's document for the other's.
        let (x, y) = (
            source_detail_cache_file(&paths, "https://example.invalid/a/catalog.json", "worktree"),
            source_detail_cache_file(&paths, "https://example.invalid/b/catalog.json", "worktree"),
        );
        assert_ne!(x, y, "same plugin, two catalogs, two caches");
        assert_ne!(x, a, "and neither is the official catalog's");
        assert_ne!(x, source_cache_file(&paths, "https://example.invalid/a/catalog.json"));
    }

    // ---- registered third-party catalogs, and the merged discovery view (`AMB-T-1980`) ----

    /// A minisign public key, real enough to have a fingerprint. Registration is what is under test
    /// here, never a signature, so any two distinct keys do.
    const KEY_A: &str = "RWSgV8uCt8tyYg74JbwBblWoE+g7bxSGvK8blkKW7gUo3EuBXaqy5oMR";
    const KEY_B: &str = "RWSw3wZ34b1PMyHu4KajlLhV0SdlMAgQGefo4pFIxv7MgRoWSVpCVXSE";

    /// A probe as [`probe_source`] would have built it, without the fetch. What the network does is its
    /// own question ([`published_key`]); what a registration *writes* is this one, and a test of it has
    /// no business resolving a hostname.
    fn probe_for(url: &str, key: Option<&str>) -> SourceProbe {
        SourceProbe {
            url: url.to_string(),
            suggested_name: default_name(url),
            key: key.map(str::to_string),
            fingerprint: key.and_then(|k| crate::plugin_provenance::key_fingerprint(k).ok()),
            registered: false,
            pinned: None,
        }
    }

    /// Register as a face would: probe (here, without the network), then write.
    fn register(paths: &Paths, url: &str, key: Option<&str>, name: Option<&str>) -> bool {
        add_source(paths, &probe_for(url, key), name).unwrap()
    }

    fn urls(paths: &Paths) -> Vec<String> {
        sources(paths).into_iter().map(|s| s.url).collect()
    }

    #[test]
    fn sources_round_trip_and_are_idempotent() {
        let paths = paths_at("sources-round-trip");
        assert!(sources(&paths).is_empty(), "none registered yet");

        assert!(register(&paths, "https://example.invalid/a/catalog.json", None, None), "added");
        assert!(register(&paths, "https://example.invalid/b/catalog.json", None, None), "added");
        assert!(
            !register(&paths, "https://example.invalid/a/catalog.json", None, None),
            "a second add of the same URL is a no-op"
        );
        assert_eq!(
            urls(&paths),
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
        assert_eq!(urls(&paths), vec!["https://example.invalid/b/catalog.json".to_string()]);
    }

    #[test]
    fn probe_refuses_a_non_http_url_and_the_official_catalog() {
        let paths = paths_at("sources-refuse");
        // Refused on the URL alone, before anything is fetched — so these never touch the network.
        assert!(probe_source(&paths, "ftp://example.invalid/catalog.json").is_err(), "not http(s)");
        assert!(probe_source(&paths, "just-a-name").is_err(), "not a URL");
        assert!(
            probe_source(&paths, OFFICIAL_CATALOG_URL).is_err(),
            "the official catalog is not a source"
        );
        assert!(sources(&paths).is_empty(), "nothing refused was registered");
    }

    /// The pin is what a registration is *for* (`AMB-D-389`): the key travels onto the record, and the
    /// fingerprint a face shows is read back off it rather than stored beside it.
    #[test]
    fn a_registration_pins_the_key_the_catalog_published() {
        let paths = paths_at("sources-pin");
        let url = "https://example.invalid/third/catalog.json";
        register(&paths, url, Some(KEY_A), Some("社内カタログ"));

        let source = sources(&paths).into_iter().find(|s| s.url == url).expect("registered");
        assert_eq!(source.key.as_deref(), Some(KEY_A), "the whole key is what is pinned");
        assert_eq!(source.name, "社内カタログ", "and the name it was registered under");
        assert_eq!(source.fingerprint().as_deref(), Some("6272CBB782CB57A0"), "the short form is derived");
    }

    /// A catalog that publishes no key registers all the same — it is browsable, and nothing from it
    /// installs. Registering it again once it does publish one is how the pin arrives, and that
    /// registration is a change, not a no-op.
    #[test]
    fn a_catalog_with_no_key_registers_and_can_be_pinned_later() {
        let paths = paths_at("sources-pin-later");
        let url = "https://example.invalid/late/catalog.json";
        assert!(register(&paths, url, None, None), "registered with nothing pinned");
        assert!(sources(&paths)[0].key.is_none(), "browse-only");
        assert_eq!(sources(&paths)[0].name, "example.invalid", "named after its host by default");

        let mut probe = probe_for(url, Some(KEY_A));
        probe.registered = true;
        assert!(add_source(&paths, &probe, None).unwrap(), "pinning a key is a change");
        assert_eq!(sources(&paths)[0].key.as_deref(), Some(KEY_A));
        assert_eq!(sources(&paths).len(), 1, "still one registration, not two");
    }

    /// The fail-closed rule, as a truth table. A different key is the only refusal — and being offline
    /// is not one, or a lost network would quietly cost a pin.
    #[test]
    fn a_changed_key_is_refused_and_nothing_else_is() {
        let url = "https://example.invalid/third/catalog.json";
        assert!(check_pin(url, None, None).is_ok(), "nothing pinned, nothing served");
        assert!(check_pin(url, None, Some(KEY_A)).is_ok(), "first use pins");
        assert!(check_pin(url, Some(KEY_A), Some(KEY_A)).is_ok(), "the same key is the ordinary case");
        assert!(check_pin(url, Some(KEY_A), None).is_ok(), "unreachable does not drop the pin");

        let err = check_pin(url, Some(KEY_A), Some(KEY_B)).unwrap_err();
        let text = format!("{err:?}");
        assert!(text.contains("6272CBB782CB57A0"), "the pinned fingerprint is named: {text}");
        assert!(text.contains("register it again"), "and the way through is said: {text}");
    }

    /// A registration written before a catalog was more than an address still reads, as a source with
    /// nothing pinned. Losing those on upgrade would silently empty a user's browsing view.
    #[test]
    fn a_bare_url_from_an_older_build_still_reads() {
        let paths = paths_at("sources-legacy");
        std::fs::create_dir_all(paths.registry_dir()).unwrap();
        std::fs::write(
            sources_file(&paths),
            r#"{"sources": ["https://example.invalid/old/catalog.json"]}"#,
        )
        .unwrap();

        let list = sources(&paths);
        assert_eq!(list.len(), 1, "the old shape reads");
        assert_eq!(list[0].url, "https://example.invalid/old/catalog.json");
        assert!(list[0].key.is_none(), "it consented to a catalog, never to a key");
        assert_eq!(list[0].name, "example.invalid", "and gets the host as its name");
    }

    /// Where a catalog's key is looked for: beside its own `catalog.json`, so one address is one
    /// publisher (the same rule a detail document follows).
    #[test]
    fn a_catalogs_key_sits_beside_it() {
        assert_eq!(
            key_url("https://example.invalid/plugins/catalog.json"),
            "https://example.invalid/plugins/catalog-key.pub"
        );
    }

    #[test]
    fn removing_a_source_drops_its_cached_copy() {
        let paths = paths_at("sources-drop-cache");
        let url = "https://example.invalid/x/catalog.json";
        register(&paths, url, None, None);
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
        register(&paths, src, None, None);
        let mut impostor = entry_json("worktree");
        impostor["desc"] = serde_json::json!("a third-party impostor");
        write_cache_at(
            &source_cache_file(&paths, src),
            &catalog_json(vec![impostor, entry_json("extra")]),
        )
        .unwrap();

        let discovery = discover(&paths);
        let names: Vec<_> = discovery.entries.iter().map(|e| e.entry.name.as_str()).collect();
        assert_eq!(names, vec!["worktree", "extra"], "official first, then the source's fresh entry");
        assert_eq!(
            discovery.entries[0].entry.desc, "a plugin",
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

    /// A third-party catalog cannot recommend its own entries into the browse view: `featured` is the
    /// official index's curation, so the merge keeps it only from there.
    #[test]
    fn discover_keeps_a_recommendation_only_from_the_official_catalog() {
        let paths = paths_at("discover-featured");
        let mut official = entry_json("worktree");
        official["featured"] = serde_json::json!(true);
        write_cache_at(&cache_file(&paths), &catalog_json(vec![official])).unwrap();
        let src = "https://example.invalid/third/catalog.json";
        register(&paths, src, None, None);
        let mut boasting = entry_json("extra");
        boasting["featured"] = serde_json::json!(true);
        write_cache_at(&source_cache_file(&paths, src), &catalog_json(vec![boasting])).unwrap();

        let discovery = discover(&paths);
        assert!(discovery.entries[0].entry.featured, "the official index's curation stands");
        assert!(
            !discovery.entries[1].entry.featured,
            "a catalog anyone can publish does not get to recommend itself"
        );
    }

    /// Nor can it call its own entries official: the badge says the amenbo team wrote the plugin, which
    /// is the official index's to establish (`AMB-D-347`), so the merge keeps it only from there.
    #[test]
    fn discover_keeps_the_official_badge_only_from_the_official_catalog() {
        let paths = paths_at("discover-official");
        let mut official = entry_json("worktree");
        official["official"] = serde_json::json!(true);
        write_cache_at(&cache_file(&paths), &catalog_json(vec![official])).unwrap();
        let src = "https://example.invalid/third/catalog.json";
        register(&paths, src, None, None);
        let mut boasting = entry_json("extra");
        boasting["official"] = serde_json::json!(true);
        write_cache_at(&source_cache_file(&paths, src), &catalog_json(vec![boasting])).unwrap();

        let discovery = discover(&paths);
        assert!(discovery.entries[0].entry.official, "the official index's own badge stands");
        assert!(
            !discovery.entries[1].entry.official,
            "a catalog anyone can publish does not get to wear the badge"
        );
    }

    /// What an install resolves against is the same fold a browse shows, so the entry a user chose is
    /// the entry that gets installed — official first, then registration order, and the winner of a name
    /// clash decided once (`AMB-D-389`).
    #[test]
    fn the_view_an_install_resolves_against_finds_the_official_entry_first() {
        let paths = paths_at("install-view-order");
        write_cache_at(&cache_file(&paths), &catalog_json(vec![entry_json("worktree")])).unwrap();
        let src = "https://example.invalid/third/catalog.json";
        register(&paths, src, Some(KEY_A), None);
        let mut impostor = entry_json("worktree");
        impostor["desc"] = serde_json::json!("a third-party impostor");
        write_cache_at(
            &source_cache_file(&paths, src),
            &catalog_json(vec![impostor, entry_json("extra")]),
        )
        .unwrap();

        let view = cached_view(&paths);
        let worktree = view.find("worktree").expect("resolvable");
        assert!(worktree.listed, "the official catalog's entry won the name");
        assert_eq!(
            worktree.trust_root().unwrap().fingerprint(),
            crate::plugin_provenance::TrustRoot::official().fingerprint(),
            "and it is checked against amenbo's own key"
        );

        let extra = view.find("extra").expect("the registered catalog's own entry resolves");
        assert_eq!(extra.source, src);
        assert_eq!(
            extra.trust_root().unwrap().fingerprint().as_deref(),
            Some("6272CBB782CB57A0"),
            "checked against the key that catalog was pinned with"
        );
        assert!(view.find("nothing-here").is_none());
    }

    /// A listing reads every catalog's cache, not just the official one: a plugin installed from a
    /// registered catalog is due the same "there is a newer build" mark as an official one.
    #[test]
    fn the_cached_view_reads_registered_catalogs_too() {
        let paths = paths_at("cached-view");
        let src = "https://example.invalid/third/catalog.json";
        register(&paths, src, Some(KEY_A), None);
        write_cache_at(&source_cache_file(&paths, src), &catalog_json(vec![entry_json("extra")]))
            .unwrap();

        // No official cache at all, and no network: the registered catalog still answers.
        let view = cached_view(&paths);
        assert!(view.find("extra").is_some(), "read from the registered catalog's cache");
        assert!(view.sources.iter().any(|s| s.official && !s.reachable), "the official one had none");
    }

    /// A registered source that cannot be reached and holds no cache contributes nothing and is marked
    /// unreachable — one dead URL does not cost the view. (`UNREACHABLE` refuses fast, no timeout wait.)
    #[test]
    fn discover_marks_an_unreachable_source_without_losing_the_rest() {
        let paths = paths_at("discover-unreachable");
        write_cache_at(&cache_file(&paths), &catalog_json(vec![entry_json("worktree")])).unwrap();
        register(&paths, UNREACHABLE, None, None);

        let discovery = discover(&paths);
        assert_eq!(discovery.entries.len(), 1, "the official catalog still answers");
        let dead = discovery.sources.iter().find(|s| s.url == UNREACHABLE).expect("listed");
        assert!(!dead.reachable && dead.offered == 0, "the dead source contributes nothing");
    }
}
