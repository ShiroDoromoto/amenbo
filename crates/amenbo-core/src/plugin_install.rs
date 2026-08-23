//! **Install** — the one door bytes off the network become a plugin on this machine
//! (`AMB-D-347`/`AMB-D-351`/`AMB-D-360`).
//!
//! `plugin_installed` reads what is on disk and says how an install *lands*; this is the other half —
//! how it gets there. One name in, five gates, then the layout that module defines:
//!
//! 1. **Resolve** the name across every catalog ([`crate::plugin_catalog::for_install`], `AMB-D-389`):
//!    the official one, then each catalog the user registered, official winning a name clash. A name no
//!    catalog carries is not installable, and when intake *dropped* the entry that name is reported with
//!    the reason rather than as a plain "no such plugin".
//! 2. **Refuse to overwrite** (`AMB-D-360`): a name already installed is a conflict a person resolves
//!    (`plugin uninstall`, or the update path of `AMB-D-359`), never a silent replacement. A half-written
//!    home — no manifest, so [`plugin_installed::read`] reads it as absent —
//!    is not an install and is written over, which is what makes a failed install retryable.
//! 3. **Fetch the detail and validate the whole entry** (`AMB-D-385`, `AMB-D-354`): the list carries what
//!    a browse view draws, so where to fetch this plugin from is a second document, taken for this one
//!    plugin ([`crate::plugin_catalog::detail`]) and joined with the entry ([`crate::plugin_wire::join`]).
//!    The join is untrusted delivery like everything else on this path, so it goes through the validator
//!    before a byte is fetched — that is where the rules a list entry could not answer for are answered.
//!    The translations ride along the same join (`AMB-D-622`) and meet their own door, which drops a
//!    language rather than refusing the plugin ([`catalog_manifest`]).
//! 4. **Refuse a platform the manifest does not claim**, and resolve this one's distributable
//!    (`AMB-D-381`): a per-OS `assets` entry, or the single `url` of an entry that is one file everywhere.
//!    An OS outside the declared set is a binary that was never built to run here.
//! 5. **Verify provenance fail-closed** ([`crate::plugin_provenance::verify_against`], `AMB-D-371`):
//!    the minisign signature against **the key that catalog answers for** — Amenbo's own for the official
//!    index, the pinned key for a registered one (`AMB-D-389`) — then that distributable's checksum, both
//!    over the exact bytes the URL served. Unsigned, signed by another key, or a digest that does not
//!    match, and nothing is written. Which key is not a caller's to pick: the root travels with the
//!    resolution ([`crate::plugin_provenance::TrustRoot`]), and a catalog that published none has no root
//!    at all, so nothing installs from it.
//!
//! **What the asset may be.** The bytes are recognised by what they start with, not by the URL's
//! extension, and there are three shapes: a gzip'd tar (the form the catalog's own example publishes),
//! from which the entry named after the plugin is taken; the executable itself; and — **on Windows
//! only** — a zip, because that is the shape wharfy writes there. Off Windows a zip is named and
//! refused rather than guessed at, since nothing ships one. Extraction never trusts a path: the entry
//! is matched on its *file name*, so a member called `../../etc/cron.d/x` is simply not the one being
//! looked for, and only a regular file is ever read.
//!
//! **Which catalog answered is written down** ([`plugin_installed::Origin`], `AMB-D-389`). Resolution
//! here is by name across the merged view, official first — an install is asked for by name and that is
//! the shelf order the user was browsing. An *update* is a different question: it replaces bytes that are
//! already here, and it must go back to the catalog that served them rather than to whoever holds the
//! name now. So the shelf is recorded beside the install, before the manifest that marks it finished.
//!
//! **Install is not enable** (`AMB-D-351`). Nothing here opens a gate, records consent, or fires
//! anything: the plugin lands on disk inert, and `plugin enable` is the separate, explicit act — which is
//! also where compatibility is judged ([`crate::plugin_compat`]), since an install that is merely
//! premature is not one to refuse.
//!
//! **The manifest is written last**, because it is the install marker (see
//! [`plugin_installed`]): an install interrupted anywhere before that reads as
//! not installed, never as half-installed.

use std::path::PathBuf;

use crate::config::{is_reserved_plugin_name, Paths};
use crate::error::{Error, ErrorCode, Msg, Result};
use crate::plugin_catalog::{self, DiscoveredEntry, Discovery, Dropped};
use crate::plugin_installed;
use crate::plugin_manifest::{Manifest, Platform, Translations};
use crate::plugin_provenance;
use crate::plugin_validate::validate_manifest;

/// Cap on the asset download, and on what is read out of an archive entry — the second is what keeps a
/// small gzip stream from expanding without bound. A plugin is one executable, so this ceiling is far
/// above any real asset while still bounding a misbehaving endpoint.
const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;

/// The first bytes of a gzip stream (`.tar.gz`).
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

/// The first bytes of a zip archive — read on Windows, and elsewhere recognised so it can be refused
/// by name rather than taken for the executable itself.
const ZIP_MAGIC: [u8; 4] = [b'P', b'K', 0x03, 0x04];

/// What an [`install`] put on this machine — the receipt a caller reports from, rather than re-reading
/// the disk to describe what it just wrote.
#[derive(Clone, Debug)]
pub struct Installed {
    /// The plugin's name, which is also its directory name (the identity, per `plugin_installed`).
    pub name: String,
    /// The catalog manifest that was installed, as written to `manifest.json`.
    pub manifest: Manifest,
    /// The plugin's home: `<base>/plugins/<name>/`.
    pub home: PathBuf,
    /// The executable inside that home.
    pub program: PathBuf,
    /// How large the installed executable is, in bytes.
    pub program_bytes: usize,
}

/// Install one plugin by name: resolve it in the catalog, fetch its asset, verify its provenance, and
/// lay it down under `plugins/<name>/` (see the module docs for the four gates).
///
/// The only network path in this module, and the only entry point: everything below it is reachable
/// solely through this function, so an asset can never be written without having passed the door.
/// Installing does **not** enable — the plugin is inert until `plugin enable` (`AMB-D-351`).
pub fn install(paths: &Paths, name: &str) -> Result<Installed> {
    // Every catalog, each read the way an install must read it — the network when it answers, the cache
    // when it does not (`AMB-D-389`). An install resolves against the freshest index available, and stays
    // possible offline once a catalog has been fetched.
    let view = plugin_catalog::for_install(paths)?;
    let found = resolve(&view, name)?;
    // Before the second fetch, so a name this machine already holds costs no request at all.
    refuse_an_overwrite(paths, &found.entry.name)?;
    let (manifest, translations) = catalog_manifest(paths, found)?;
    let program = fetch_verified_program(&manifest, &found.trust_root()?)?;
    // Which shelf this came off, before the marker that says it is installed at all: an update reads it
    // to go back to the same one (see `plugin_installed::Origin`).
    plugin_installed::record_origin(paths, &manifest.name, &found.origin())?;
    // And what the catalog said about it in other languages, beside it for the same reason: the faces
    // that read it open with no network (`AMB-D-622`).
    plugin_installed::record_translations(paths, &manifest.name, &translations)?;
    place(paths, &manifest, &program)
}

/// The whole catalog entry for one listed plugin: the list half already in hand, joined with the detail
/// document fetched for this one plugin (`AMB-D-385`) and put through the validator (`AMB-D-354`).
///
/// The one place the two halves become a manifest. An update goes through it too
/// ([`crate::plugin_update`]), because "what the catalog publishes for this plugin" is the same question
/// whether it is being installed for the first time or replacing a build — and answering it in two places
/// is how one of them ends up skipping the door.
///
/// Validating the join is what keeps the split from loosening anything: a list entry cannot be asked for a
/// checksum it does not carry, so the rules that live on the detail's fields are checked here, with both
/// halves in hand, before a byte of the asset is fetched.
///
/// **The translations come back with it** (`AMB-D-622`), joined from both halves: the form labels the
/// detail document just carried, and the lines whichever `catalog.<lang>.json` documents are already on
/// this machine carried ([`plugin_catalog::cached_list_overlays`]). They go through their own door on the
/// way out ([`sound_languages`]), which — unlike the manifest's — drops a language rather than refusing
/// the plugin.
pub(crate) fn catalog_manifest(
    paths: &Paths,
    found: &DiscoveredEntry,
) -> Result<(Manifest, Translations)> {
    let entry = &found.entry;
    let detail = plugin_catalog::detail_of(paths, found)?;
    let (manifest, translations) = crate::plugin_wire::join(
        entry,
        &plugin_catalog::cached_list_overlays(paths, found),
        &detail,
    );
    let problems = validate_manifest(&manifest);
    if let Some(first) = problems.first() {
        let first = format!("{}: {}", first.location, first.message.en());
        return Err(Error::Invalid(
            Msg::new(format!("the catalog's entry for '{}' is not valid ({first})", entry.name))
                .coded(ErrorCode::InvalidPluginEntry)
                .with("name", &entry.name)
                .with("problem", &first),
        ));
    }
    let translations = sound_languages(&manifest, translations);
    Ok((manifest, translations))
}

/// The languages that line up with the manifest they translate —
/// [`validate_overlays`](crate::plugin_validate::validate_overlays) asked one language at a time,
/// because the answer here is per language (`AMB-D-621`).
///
/// **A language that does not line up is dropped, and the install goes on.** The delivery path is not
/// trusted (`AMB-D-354`), so what arrives is checked like everything else on it; but a translation is a
/// layer over values that are already there, and what dropping one costs a reader is the base line they
/// would have seen had nobody translated it (`AMB-D-623`). Refusing the install instead would be
/// refusing a plugin whose binary, checksum and signature are all sound over a label — and would hand
/// whoever publishes the catalog a way to make a plugin uninstallable by mistranslating it.
///
/// Dropped languages are warned about rather than swallowed, for the same reason a dropped catalog entry
/// is: a catalog quietly shedding its translations should be visible somewhere.
fn sound_languages(m: &Manifest, translations: Translations) -> Translations {
    translations
        .into_iter()
        .filter(|(lang, overlay)| {
            let one = Translations::from([(lang.clone(), overlay.clone())]);
            let problems = crate::plugin_validate::validate_overlays(m, &one);
            if let Some(first) = problems.first() {
                tracing::warn!(
                    plugin = %m.name,
                    language = %lang,
                    problem = %format!("{}: {}", first.location, first.message.en()),
                    "dropping a plugin translation that does not line up with the manifest it translates",
                );
            }
            problems.is_empty()
        })
        .collect()
}

/// The plugin's executable, off the network and through the trust gates — the *only* way bytes named by a
/// manifest become bytes Amenbo will write.
///
/// Platform, provenance and packaging in one call, because taking them apart is how a caller ends up
/// with a partial chain: [`crate::plugin_update`] replaces an installed binary through exactly this
/// function (`AMB-D-359` — an update re-verifies), and there is deliberately no entry point beside it
/// that fetches without verifying. The key is never a parameter (see the module docs).
pub(crate) fn fetch_verified_program(
    manifest: &Manifest,
    root: &plugin_provenance::TrustRoot,
) -> Result<Vec<u8>> {
    // What is fetched, what it must hash to and who signed it are all this platform's (`AMB-D-381`) —
    // one lookup, so provenance can never be checked against another OS's bytes.
    let here = refuse_another_platform(manifest)?;
    let published = published_for(manifest, here)?;
    let asset = download(&published.url)?;
    plugin_provenance::verify_against(
        &asset,
        published.signature.as_deref(),
        &published.checksum,
        root,
    )?;
    unpack_program(&asset, &manifest.name)
}

/// The catalog entry this name resolves to. A name the catalog does not carry is `not_found` — unless
/// intake dropped an entry under exactly that name, in which case the drop is the answer: "the catalog
/// has it, and it did not pass the door" is a different problem from "no such plugin", and only one of
/// them is the user's typo.
fn resolve<'a>(view: &'a Discovery, name: &str) -> Result<&'a DiscoveredEntry> {
    if let Some(found) = view.find(name) {
        return Ok(found);
    }
    for dropped in &view.dropped {
        match dropped {
            Dropped::Invalid { name: dropped_name, problems } if dropped_name == name => {
                let first = problems
                    .first()
                    .map(|p| format!("{}: {}", p.location, p.message.en()))
                    .unwrap_or_default();
                return Err(Error::Invalid(
                    Msg::new(format!(
                        "the catalog's entry for '{name}' is not valid and was dropped ({first})"
                    ))
                    .coded(ErrorCode::InvalidPluginEntryDropped)
                    .with("name", name)
                    .with("problem", &first),
                ));
            }
            Dropped::Duplicate { name: dropped_name } if dropped_name == name => {
                return Err(Error::Invalid(
                    Msg::new(format!("the catalog carries more than one entry named '{name}'"))
                        .coded(ErrorCode::InvalidPluginEntryDuplicate)
                        .with("name", name),
                ));
            }
            _ => {}
        }
    }
    Err(Error::NotFound(
        Msg::new(format!("no plugin named '{name}' in the catalog"))
            .coded(ErrorCode::NotFoundPluginInCatalog)
            .with("name", name),
    ))
}

/// Refuse to install over a name this machine already holds (`AMB-D-360`) — no silent overwrite, in
/// either of the two shapes it could take: a working install (which is `plugin update`'s to move, not
/// install's), and a broken one (which is `plugin uninstall`'s to clear). A home with no manifest is
/// neither: it is the residue of an install that did not finish, so it is written over rather than
/// stood in the way of a retry.
fn refuse_an_overwrite(paths: &Paths, name: &str) -> Result<()> {
    match plugin_installed::read(paths, name) {
        Ok(_) => Err(Error::Conflict(
            Msg::new(format!("plugin '{name}' is already installed on this machine"))
                .coded(ErrorCode::ConflictPluginInstalled)
                .with("name", name),
        )),
        // The one refusal that means "nothing is there": the home holds no manifest. It is matched on
        // the code rather than the sentence, and on the code that names *this* refusal — the family
        // `not_found` would also swallow "the catalog does not list it", which is not this question.
        Err(e) if e.error_code() == ErrorCode::NotFoundPluginInstalled => Ok(()),
        Err(e) => Err(Error::Conflict(
            Msg::new(format!(
                "a broken install of '{name}' is in the way ({}) — uninstall it first",
                e.message_en()
            ))
            .coded(ErrorCode::ConflictPluginInstallBroken)
            .with("name", name)
            .with("reason", e.message_en()),
        )),
    }
}

/// Refuse an OS the manifest does not list, and hand back the one it does — the platform every later step
/// resolves against. A plugin that never claimed this platform has nothing built to run here, whichever
/// form its distributables take.
fn refuse_another_platform(manifest: &Manifest) -> Result<Platform> {
    let here = std::env::consts::OS;
    if let Some(platform) = Platform::here().filter(|p| manifest.os.contains(&p.os)) {
        return Ok(platform);
    }
    let supported: Vec<&str> = manifest.os.iter().map(|os| os.as_str()).collect();
    let supported = supported.join(", ");
    Err(Error::Invalid(
        Msg::new(format!(
            "plugin '{}' does not support {here} (it supports: {supported})",
            manifest.name
        ))
        .coded(ErrorCode::InvalidPluginOsUnsupported)
        .with("name", &manifest.name)
        .with("os", here)
        .with("supported", &supported),
    ))
}

/// This platform's distributable, or the refusal that the entry claims the OS yet publishes nothing this
/// machine can run (`AMB-D-381`, `AMB-D-384`). Reached when neither the exact `<os>-<arch>` nor the
/// arch-agnostic `<os>` key answers — the fail-open `AMB-D-384` closes, refused at the door instead of a
/// mismatched binary at run time. The catalog door keeps `os` answered by a key, so a bare-OS refusal here
/// is a manifest that did not come through it — a hand-placed one, most likely — and the honest answer is
/// to stop rather than reach for another platform's bytes.
fn published_for(manifest: &Manifest, here: Platform) -> Result<crate::plugin_manifest::Asset> {
    manifest.asset_for(here).ok_or_else(|| {
        Error::Invalid(
            Msg::new(format!(
                "plugin '{}' lists {} but publishes no asset for {}",
                manifest.name,
                here.os.as_str(),
                here.token()
            ))
            .coded(ErrorCode::InvalidPluginAssetAbsent)
            .with("name", &manifest.name)
            .with("os", here.os.as_str())
            .with("platform", here.token()),
        )
    })
}

/// Fetch the asset, with the same short-lived agent the rest of Amenbo's downloads use and a
/// download-sized read cap. The bytes are returned rather than streamed to disk: nothing is written
/// anywhere until provenance has passed over the whole asset.
fn download(url: &str) -> Result<Vec<u8>> {
    let agent: ureq::Agent = ureq::Agent::config_builder().build().into();
    let mut response = agent.get(url).call().map_err(|e| {
        Error::Io(std::io::Error::other(format!("could not fetch the plugin asset at {url}: {e}")))
    })?;
    response.body_mut().with_config().limit(MAX_ASSET_BYTES).read_to_vec().map_err(|e| {
        Error::Io(std::io::Error::other(format!("could not read the plugin asset from {url}: {e}")))
    })
}

/// The executable inside a verified asset, recognised by the asset's own leading bytes: a gzip'd tar, the
/// executable itself, or — **on Windows only** — a zip. The zip path exists because that is the shape
/// wharfy writes for Windows (mirroring [`crate::self_update`]); off Windows nothing ships one, so a zip
/// is named and refused rather than guessed at.
///
/// This runs **after** provenance, so the bytes are the ones the catalog blessed — the shapes here are a
/// packaging question, not a trust one. The size cap still applies to what comes out of the archive,
/// which is the part a signature cannot bound.
fn unpack_program(asset: &[u8], name: &str) -> Result<Vec<u8>> {
    if asset.is_empty() {
        return Err(Error::Invalid(
            Msg::new(format!("the asset for plugin '{name}' is empty"))
                .coded(ErrorCode::InvalidPluginAssetEmpty)
                .with("name", name),
        ));
    }
    if asset.starts_with(&GZIP_MAGIC) {
        return from_tar_gz(asset, name);
    }
    if asset.starts_with(&ZIP_MAGIC) {
        #[cfg(windows)]
        {
            return from_zip(asset, name);
        }
        #[cfg(not(windows))]
        {
            return Err(Error::Invalid(
                Msg::new(format!(
                    "the asset for plugin '{name}' is a zip — a zip is only taken on Windows; publish it as a .tar.gz, or as the executable itself"
                ))
                .coded(ErrorCode::InvalidPluginAssetZipOffWindows)
                .with("name", name),
            ));
        }
    }
    Ok(asset.to_vec())
}

/// Read the plugin's executable out of a gzip'd tar: the entry whose **file name** is the plugin's own
/// (plus this platform's executable suffix), which is the same convention `plugin_installed` writes it
/// under. Matching on the file name alone means a leading directory still resolves and a member path
/// that tries to climb out of the archive is simply never the one being looked for; only a regular file
/// is read, so a symlink cannot stand in for the binary.
fn from_tar_gz(asset: &[u8], name: &str) -> Result<Vec<u8>> {
    use std::io::Read;

    let wanted = plugin_installed::program_file_name(name);
    let unreadable = |e: std::io::Error| {
        Error::Invalid(
            Msg::new(format!("the asset for plugin '{name}' is not a readable .tar.gz: {e}"))
                .coded(ErrorCode::InvalidPluginAssetTarUnreadable)
                .with("name", name)
                .with("reason", e),
        )
    };

    let gz = flate2::read::GzDecoder::new(asset);
    let mut archive = tar::Archive::new(gz);
    for entry in archive.entries().map_err(unreadable)? {
        let entry = entry.map_err(unreadable)?;
        let is_wanted = entry
            .path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_os_string()))
            .is_some_and(|n| n == wanted.as_str());
        if !is_wanted || !entry.header().entry_type().is_file() {
            continue;
        }
        let mut program = Vec::new();
        entry.take(MAX_ASSET_BYTES).read_to_end(&mut program).map_err(unreadable)?;
        return Ok(program);
    }
    Err(Error::Invalid(
        Msg::new(format!("the asset for plugin '{name}' holds no '{wanted}' entry"))
            .coded(ErrorCode::InvalidPluginAssetWithoutProgram)
            .with("name", name)
            .with("program", &wanted),
    ))
}

/// The Windows path: read the plugin's executable out of a zip — the entry whose **file name** is the
/// plugin's own ([`plugin_installed::program_file_name`], i.e. `<name>.exe` here). Matching on the file
/// name alone means a leading directory still resolves and a member path that climbs out of the archive
/// is never the one looked for; `enclosed_name` sanitizes against zip-slip and only a regular file is
/// read. The size cap applies to the **decompressed** bytes — the part a signature cannot bound. This is
/// the counterpart to [`from_tar_gz`], mirroring [`crate::self_update`]'s zip reader, and — like the
/// `zip` dependency itself — Windows-only, because no other platform ships a zip.
#[cfg(windows)]
fn from_zip(asset: &[u8], name: &str) -> Result<Vec<u8>> {
    use std::io::Read;

    let wanted = plugin_installed::program_file_name(name);
    let unreadable = |e: String| {
        Error::Invalid(
            Msg::new(format!("the asset for plugin '{name}' is not a readable zip: {e}"))
                .coded(ErrorCode::InvalidPluginAssetZipUnreadable)
                .with("name", name)
                .with("reason", &e),
        )
    };

    let mut zip =
        zip::ZipArchive::new(std::io::Cursor::new(asset)).map_err(|e| unreadable(e.to_string()))?;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| unreadable(e.to_string()))?;
        let is_wanted = entry
            .enclosed_name()
            .and_then(|p| p.file_name().map(|n| n.to_os_string()))
            .is_some_and(|n| n == wanted.as_str());
        if !is_wanted || !entry.is_file() {
            continue;
        }
        let mut program = Vec::new();
        entry
            .take(MAX_ASSET_BYTES)
            .read_to_end(&mut program)
            .map_err(|e| unreadable(e.to_string()))?;
        return Ok(program);
    }
    Err(Error::Invalid(
        Msg::new(format!("the asset for plugin '{name}' holds no '{wanted}' entry"))
            .coded(ErrorCode::InvalidPluginAssetWithoutProgram)
            .with("name", name)
            .with("program", &wanted),
    ))
}

/// Lay the plugin down in the layout [`plugin_installed`] reads: the executable (marked runnable on
/// unix, since nothing else will), and **then** `manifest.json`, the install marker. The order is the
/// whole failure story — stop before the last write and the directory reads as not installed.
///
/// An update writes through here too ([`crate::plugin_update`]), so the same order holds when the home
/// already exists: the new binary lands first and the manifest describing it last.
pub(crate) fn place(paths: &Paths, manifest: &Manifest, program: &[u8]) -> Result<Installed> {
    let name = &manifest.name;
    // The registry cache lives beside the plugins, so a name that resolved to it would install *over*
    // the catalog. The catalog's own intake refuses this name and so does the validator; this is the
    // write boundary saying so too, because it is the one place where being wrong costs the cache.
    if is_reserved_plugin_name(name) {
        return Err(Error::invalid(format!("'{name}' is not a plugin name (it is reserved for the registry cache)")));
    }
    let home = paths.plugin_dir(name);
    std::fs::create_dir_all(&home)?;

    let program_path = plugin_installed::program_path(paths, name);
    std::fs::write(&program_path, program)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program_path, std::fs::Permissions::from_mode(0o755))?;
    }

    let json = serde_json::to_string_pretty(manifest).map_err(|e| {
        Error::Invalid(
            Msg::new(format!("the manifest for plugin '{name}' cannot be written out: {e}"))
                .coded(ErrorCode::InvalidPluginManifestUnwritable)
                .with("name", name)
                .with("reason", e),
        )
    })?;
    std::fs::write(plugin_installed::manifest_path(paths, name), json)?;

    Ok(Installed {
        name: name.clone(),
        manifest: manifest.clone(),
        home,
        program: program_path,
        program_bytes: program.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_manifest::{Arch, Os};

    /// An arch-agnostic platform key (`<os>`).
    fn plat(os: Os) -> Platform {
        Platform { os, arch: None }
    }

    fn paths_at(tag: &str) -> Paths {
        let dir = amenbo_scratch::scratch(&format!("plugin-install-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        Paths::at(dir)
    }

    fn manifest(name: &str) -> Manifest {
        Manifest {
            name: name.to_string(),
            title: None,
            desc: "a test plugin".to_string(),
            about: None,
            author: "amenbo".to_string(),
            repo: "ShiroDoromoto/amenbo".to_string(),
            os: vec![Os::Macos, Os::Linux, Os::Windows],
            category: "workflow".to_string(),
            url: "https://example.invalid/x.tar.gz".to_string(),
            checksum: format!("sha256:{}", "a".repeat(64)),
            signature: Some("sig".to_string()),
            assets: Default::default(),
            official: false,
            detail_sum: None,
            scope: crate::plugin_manifest::Scope::Project,
            payload_v: 1,
            min_amenbo: None,
            config: Vec::new(),
            events: Vec::new(),
            agent: None,
            settings: None,
        }
    }

    /// The merged view holding these entries, built the way a real one is — through intake, then the
    /// fold. `official` says which catalog served them, which is what decides the trust root.
    fn view_of(entries: Vec<serde_json::Value>) -> Discovery {
        served_view(entries, true, None)
    }

    /// The same, from a registered catalog pinned with `key` (`None` = one that published none).
    fn served_view(entries: Vec<serde_json::Value>, official: bool, key: Option<&str>) -> Discovery {
        let json =
            serde_json::json!({ "catalog_v": 1, "generated_at": "2026-07-23T04:57:10Z", "plugins": entries })
                .to_string();
        let catalog = plugin_catalog::parse(&json).unwrap();
        let source = if official {
            plugin_catalog::OFFICIAL_CATALOG_URL
        } else {
            "https://example.invalid/works/catalog.json"
        };
        Discovery::served_by(source, official, key, catalog)
    }

    /// One **list** entry, as `catalog.json` now serves it (`AMB-D-385`) — what an install resolves a
    /// name against before it fetches anything.
    fn entry_json(name: &str) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "desc": "a plugin",
            "author": "amenbo",
            "repo": "ShiroDoromoto/amenbo",
            "os": ["macos", "linux", "windows"],
            "category": "workflow",
            "detail_sum": format!("sha256:{}", "a".repeat(64)),
        })
    }

    /// A gzip'd tar holding one file per `(path, bytes)`.
    fn tar_gz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        for (path, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, path, *bytes).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    // ---- resolving a name against the catalog ----

    #[test]
    fn a_listed_name_resolves_to_its_entry() {
        let view = view_of(vec![entry_json("worktree"), entry_json("slack")]);
        assert_eq!(resolve(&view, "slack").unwrap().entry.desc, "a plugin");
    }

    #[test]
    fn an_unlisted_name_is_not_found() {
        let view = view_of(vec![entry_json("worktree")]);
        assert_eq!(resolve(&view, "slack").unwrap_err().code(), "not_found_plugin_in_catalog");
    }

    /// The trust root follows the catalog the entry came from (`AMB-D-389`): Amenbo's own key for the
    /// official index, the catalog's pinned key for a registered one.
    #[test]
    fn an_entry_is_verified_against_its_own_catalogs_key() {
        const PINNED: &str = "RWSw3wZ34b1PMyHu4KajlLhV0SdlMAgQGefo4pFIxv7MgRoWSVpCVXSE";
        let official = view_of(vec![entry_json("worktree")]);
        assert_eq!(
            resolve(&official, "worktree").unwrap().trust_root().unwrap().fingerprint(),
            plugin_provenance::TrustRoot::official().fingerprint(),
        );

        let third = served_view(vec![entry_json("worktree")], false, Some(PINNED));
        assert_eq!(
            resolve(&third, "worktree").unwrap().trust_root().unwrap().fingerprint().as_deref(),
            Some("334FBDE17706DFB0"),
            "the catalog's own key, not Amenbo's"
        );
    }

    /// A registered catalog that published no key has no root, and an install off it is refused rather
    /// than falling back to Amenbo's — that fallback would be an unsigned-by-anyone asset going in.
    #[test]
    fn a_catalog_with_no_pinned_key_cannot_be_installed_from() {
        let view = served_view(vec![entry_json("worktree")], false, None);
        let err = resolve(&view, "worktree").unwrap().trust_root().unwrap_err();
        assert_eq!(err.code(), "invalid_catalog_key_absent");
        assert!(format!("{err:?}").contains("no signing key"), "the reason is said: {err:?}");
    }

    /// A name intake dropped answers with the drop, not with "no such plugin" — the catalog does carry
    /// it, and the reason is what the user needs.
    #[test]
    fn a_name_the_catalog_dropped_is_reported_with_the_reason() {
        let mut invalid = entry_json("slack");
        invalid["repo"] = serde_json::json!("not-github-coordinates");
        let view = view_of(vec![invalid]);

        let err = resolve(&view, "slack").unwrap_err();
        assert_eq!(err.code(), "invalid_plugin_entry_dropped");
        assert!(format!("{err:?}").contains("dropped"), "the drop is the answer: {err:?}");
    }

    // ---- the gates that run before anything is fetched ----

    #[test]
    fn an_installed_name_is_not_overwritten() {
        let paths = paths_at("overwrite");
        place(&paths, &manifest("worktree"), b"#!/bin/sh\n").unwrap();

        let err = refuse_an_overwrite(&paths, "worktree").unwrap_err();
        assert_eq!(err.code(), "conflict_plugin_installed");
    }

    /// A home left behind by an install that did not finish (no manifest) is not an install: the retry
    /// goes through and writes over it.
    #[test]
    fn a_half_written_home_does_not_block_a_retry() {
        let paths = paths_at("retry");
        let home = paths.plugin_dir("worktree");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(plugin_installed::program_file_name("worktree")), b"old").unwrap();

        refuse_an_overwrite(&paths, "worktree").expect("a manifest-less home is not an install");
        place(&paths, &manifest("worktree"), b"new").unwrap();
        assert_eq!(
            std::fs::read(plugin_installed::program_path(&paths, "worktree")).unwrap(),
            b"new",
        );
    }

    /// A broken install (a manifest naming another plugin) is refused rather than replaced — clearing it
    /// is `uninstall`'s job, and the message says so.
    #[test]
    fn a_broken_install_is_refused_rather_than_replaced() {
        let paths = paths_at("broken");
        place(&paths, &manifest("slack"), b"x").unwrap();
        std::fs::rename(paths.plugin_dir("slack"), paths.plugin_dir("worktree")).unwrap();
        std::fs::write(
            plugin_installed::program_path(&paths, "worktree"),
            b"x",
        )
        .unwrap();

        let err = refuse_an_overwrite(&paths, "worktree").unwrap_err();
        assert_eq!(err.code(), "conflict_plugin_install_broken");
        assert!(format!("{err:?}").contains("uninstall"), "the way out is named: {err:?}");
    }

    #[test]
    fn a_platform_the_manifest_does_not_claim_is_refused() {
        let mut m = manifest("worktree");
        m.os = vec![Os::parse(std::env::consts::OS).unwrap()];
        refuse_another_platform(&m).expect("this platform is listed");

        m.os = vec![Os::Macos, Os::Windows, Os::Linux]
            .into_iter()
            .filter(|os| os.as_str() != std::env::consts::OS)
            .collect();
        let err = refuse_another_platform(&m).unwrap_err();
        assert!(
            format!("{err:?}").contains(std::env::consts::OS),
            "the platform that has no asset is named: {err:?}",
        );
    }

    /// What gets fetched and what it is checked against are this platform's (`AMB-D-381`) — never
    /// another's, and never the single fields once a map is declared.
    #[test]
    fn the_distributable_fetched_is_the_one_published_for_this_platform() {
        use crate::plugin_manifest::Asset;

        let here = Os::here().expect("Amenbo runs on an OS its manifests can name");
        let other = [Os::Macos, Os::Windows, Os::Linux].into_iter().find(|os| *os != here).unwrap();

        let mut m = manifest("worktree");
        m.os = vec![here, other];
        m.url = String::new();
        m.checksum = String::new();
        m.assets = [
            (
                plat(here),
                Asset {
                    url: "https://example.invalid/here.tar.gz".into(),
                    checksum: "sha256:here".into(),
                    signature: Some("sig-here".into()),
                },
            ),
            (
                plat(other),
                Asset {
                    url: "https://example.invalid/other.tar.gz".into(),
                    checksum: "sha256:other".into(),
                    signature: Some("sig-other".into()),
                },
            ),
        ]
        .into_iter()
        .collect();

        let picked = published_for(&m, refuse_another_platform(&m).unwrap()).unwrap();
        assert_eq!(picked.url, "https://example.invalid/here.tar.gz");
        assert_eq!(picked.checksum, "sha256:here", "provenance is checked against these bytes");
        assert_eq!(picked.signature.as_deref(), Some("sig-here"));

        // An entry claiming this platform and publishing nothing for it stops, rather than reaching for
        // the other one's binary. Only a manifest that skipped the door can be in this state.
        m.assets.remove(&plat(here));
        let err = published_for(&m, Platform::here().unwrap()).unwrap_err();
        assert!(format!("{err:?}").contains(here.as_str()), "{err:?}");
    }

    /// An arch-specific `assets` map serves this machine's arch, and refuses a machine whose arch it does
    /// not publish — the fail-open `AMB-D-384` closes, at the door.
    #[test]
    fn an_os_arch_map_is_resolved_and_refuses_an_unpublished_arch() {
        use crate::plugin_manifest::Asset;

        let Some(here) = Platform::here() else { return };
        let Some(arch) = here.arch else { return };
        let other_arch = if arch == Arch::Arm64 { Arch::X64 } else { Arch::Arm64 };

        let mut m = manifest("worktree");
        m.os = vec![here.os];
        m.url = String::new();
        m.checksum = String::new();

        // Only this machine's exact os-arch is published: it resolves, on the exact key.
        m.assets = [(
            Platform { os: here.os, arch: Some(arch) },
            Asset { url: "https://example.invalid/exact.tar.gz".into(), checksum: "sha256:exact".into(), signature: None },
        )]
        .into_iter()
        .collect();
        assert_eq!(published_for(&m, here).unwrap().checksum, "sha256:exact");

        // Only the *other* arch is published: no arch-agnostic key to fall back to, so it is refused rather
        // than handed a binary built for a different arch.
        m.assets = [(
            Platform { os: here.os, arch: Some(other_arch) },
            Asset { url: "https://example.invalid/other.tar.gz".into(), checksum: "sha256:other".into(), signature: None },
        )]
        .into_iter()
        .collect();
        assert!(published_for(&m, here).is_err(), "a build for another arch is not this machine's");
    }

    // ---- what the asset may be ----

    #[test]
    fn a_bare_executable_asset_is_taken_as_the_program() {
        assert_eq!(unpack_program(b"#!/bin/sh\necho hi\n", "worktree").unwrap(), b"#!/bin/sh\necho hi\n");
    }

    #[test]
    fn an_empty_asset_is_refused() {
        assert!(unpack_program(b"", "worktree").is_err());
    }

    /// Off Windows nothing ships a zip, so one is named and refused by its shape (never parsed).
    #[cfg(not(windows))]
    #[test]
    fn a_zip_asset_is_refused_off_windows() {
        let mut zip = ZIP_MAGIC.to_vec();
        zip.extend_from_slice(b"...the rest of a zip...");
        let err = unpack_program(&zip, "worktree").unwrap_err();
        assert!(format!("{err:?}").contains("zip"), "the shape is named: {err:?}");
    }

    /// A Deflate zip holding one file per `(path, bytes)`. Windows-only, matching where zips are taken.
    #[cfg(windows)]
    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        use zip::write::{SimpleFileOptions, ZipWriter};
        use zip::CompressionMethod;
        let mut buf = Vec::new();
        {
            let mut zw = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            for (path, bytes) in files {
                zw.start_file(*path, opts).unwrap();
                zw.write_all(bytes).unwrap();
            }
            zw.finish().unwrap();
        }
        buf
    }

    /// On Windows the zip path mirrors the tar one: the entry named after the plugin is the program, and
    /// the extra files a release archive carries — plus a leading directory — are ignored.
    #[cfg(windows)]
    #[test]
    fn the_named_entry_is_taken_out_of_a_zip() {
        let program = plugin_installed::program_file_name("worktree");
        let asset = zip_of(&[
            ("worktree-v1/README.md", b"docs" as &[u8]),
            (&format!("worktree-v1/{program}"), b"MZ-ish"),
        ]);
        assert_eq!(unpack_program(&asset, "worktree").unwrap(), b"MZ-ish");
    }

    #[cfg(windows)]
    #[test]
    fn a_zip_without_the_named_entry_is_refused() {
        let asset = zip_of(&[("worktree-v1/some-other-binary.exe", b"x" as &[u8])]);
        let err = unpack_program(&asset, "worktree").unwrap_err();
        assert!(format!("{err:?}").contains("worktree"), "the entry looked for is named: {err:?}");
    }

    #[cfg(windows)]
    #[test]
    fn a_corrupt_zip_asset_is_refused() {
        let mut corrupt = ZIP_MAGIC.to_vec();
        corrupt.extend_from_slice(b"not really a zip stream");
        assert!(unpack_program(&corrupt, "worktree").is_err());
    }

    /// The tar path: the entry named after the plugin is the program, and the extra files a release
    /// archive carries are ignored — including a leading directory on the path.
    #[test]
    fn the_named_entry_is_taken_out_of_a_tar_gz() {
        let program = plugin_installed::program_file_name("worktree");
        let asset = tar_gz(&[
            ("worktree-v1/README.md", b"docs" as &[u8]),
            (&format!("worktree-v1/{program}"), b"ELF-ish"),
        ]);
        assert_eq!(unpack_program(&asset, "worktree").unwrap(), b"ELF-ish");
    }

    #[test]
    fn a_tar_gz_without_the_named_entry_is_refused() {
        let asset = tar_gz(&[("worktree-v1/some-other-binary", b"x" as &[u8])]);
        let err = unpack_program(&asset, "worktree").unwrap_err();
        assert!(format!("{err:?}").contains("worktree"), "the entry looked for is named: {err:?}");
    }

    /// Where a member sits in the archive decides nothing: the bytes are read out and written to the
    /// path Amenbo computes, so an entry buried under directories lands in the same home as any other.
    /// This is what makes the archive's own paths unable to steer a write.
    #[test]
    fn the_archives_own_path_never_decides_where_the_program_lands() {
        let paths = paths_at("archive-path");
        let program = plugin_installed::program_file_name("worktree");
        let asset = tar_gz(&[(&format!("a/deeply/nested/{program}"), b"ELF-ish" as &[u8])]);

        let unpacked = unpack_program(&asset, "worktree").unwrap();
        let placed = place(&paths, &manifest("worktree"), &unpacked).unwrap();
        assert_eq!(placed.program, plugin_installed::program_path(&paths, "worktree"));
        assert!(!paths.plugins_dir().join("a").exists(), "nothing followed the archive's path");
    }

    #[test]
    fn a_corrupt_gzip_asset_is_refused() {
        let mut corrupt = GZIP_MAGIC.to_vec();
        corrupt.extend_from_slice(b"not really a gzip stream");
        assert!(unpack_program(&corrupt, "worktree").is_err());
    }

    // ---- the layout that lands on disk ----

    #[test]
    fn what_is_placed_reads_back_as_an_install() {
        let paths = paths_at("layout");
        let placed = place(&paths, &manifest("worktree"), b"#!/bin/sh\n").unwrap();

        assert_eq!(placed.home, paths.plugin_dir("worktree"));
        assert_eq!(placed.program, plugin_installed::program_path(&paths, "worktree"));
        assert_eq!(placed.program_bytes, b"#!/bin/sh\n".len());

        // The point of the layout: the reader agrees this is an install.
        let read = plugin_installed::read(&paths, "worktree").unwrap();
        assert_eq!(read.manifest, manifest("worktree"));
        assert_eq!(read.program, placed.program);
    }

    #[cfg(unix)]
    #[test]
    fn the_program_is_marked_runnable() {
        use std::os::unix::fs::PermissionsExt;
        let paths = paths_at("mode");
        let placed = place(&paths, &manifest("worktree"), b"#!/bin/sh\n").unwrap();
        let mode = std::fs::metadata(&placed.program).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "nothing else would make it executable");
    }

    /// The registry cache is not a plugin, and the write boundary refuses it too — installing over it
    /// would take the catalog with it.
    #[test]
    fn the_registry_cache_is_never_installed_over() {
        let paths = paths_at("registry");
        let err = place(&paths, &manifest(Paths::REGISTRY_DIR_NAME), b"x").unwrap_err();
        assert_eq!(err.code(), "invalid_value");
        assert!(!paths.registry_dir().exists(), "and nothing was written");
    }

    /// A manifest with a form to translate, for the door below.
    fn manifest_with_a_form(name: &str) -> Manifest {
        Manifest {
            config: vec![serde_json::from_value(serde_json::json!({
                "key": "base",
                "label": "Base branch",
            }))
            .expect("a config field")],
            ..manifest(name)
        }
    }

    /// One language's overlay of that form.
    fn overlay(desc: Option<&str>, label: Option<&str>) -> crate::plugin_manifest::ManifestOverlay {
        crate::plugin_manifest::ManifestOverlay {
            desc: desc.map(str::to_string),
            config: label
                .map(|label| {
                    std::collections::BTreeMap::from([(
                        "base".to_string(),
                        crate::plugin_manifest::ConfigFieldOverlay {
                            label: Some(label.to_string()),
                            ..Default::default()
                        },
                    )])
                })
                .unwrap_or_default(),
            ..Default::default()
        }
    }

    /// **The translations meet the same door the manifest does** (`AMB-D-354`) — the delivery path is
    /// not trusted, so a language that does not line up with what it translates is not shown.
    #[test]
    fn a_translation_that_does_not_line_up_is_dropped() {
        let m = manifest_with_a_form("worktree");
        let published = Translations::from([
            ("ja".to_string(), overlay(Some("一行"), Some("基点にするブランチ"))),
            // A language Amenbo is not read in — a document nothing would ever fetch.
            ("xx".to_string(), overlay(Some("a line"), None)),
            // A form field this manifest does not declare: text nobody would ever see.
            ("de".to_string(), {
                let mut o = overlay(Some("eine Zeile"), None);
                o.config.insert("nothing".into(), Default::default());
                o
            }),
        ]);

        let kept = sound_languages(&m, published);

        assert_eq!(kept.keys().collect::<Vec<_>>(), ["ja"]);
        assert_eq!(kept["ja"].config["base"].label.as_deref(), Some("基点にするブランチ"));
    }

    /// **And the plugin still installs** (`AMB-D-623`). What a dropped language costs a reader is the
    /// base line they would have seen had nobody translated it — refusing over it would refuse a plugin
    /// whose binary, checksum and signature are all sound, and hand a catalog a way to make one
    /// uninstallable by mistranslating it.
    #[test]
    fn a_plugin_whose_every_translation_is_refused_is_still_a_plugin() {
        let m = manifest_with_a_form("worktree");
        let published = Translations::from([("xx".to_string(), overlay(Some("a line"), None))]);

        assert!(sound_languages(&m, published).is_empty());
        assert!(validate_manifest(&m).is_empty(), "and the manifest itself was never in question");
    }

    /// **What the install leaves behind carries the author's own words, in every language**
    /// (`AMB-D-638`, `AMB-D-640`) — the base text inside `manifest.json`, the translations beside it in
    /// `i18n.json`, written by the same two calls an install ends with and in the same order.
    ///
    /// The detail view is drawn from what is on disk, with no network and no catalog, and it follows a
    /// reader who changes language without going back for anything. A text that did not land here is
    /// one nobody ever reads.
    #[test]
    fn the_install_leaves_the_description_text_behind_in_every_language() {
        let paths = paths_at("about");
        let m = Manifest {
            about: Some("Cuts a worktree per task, and folds it once the work is merged.".into()),
            ..manifest("worktree")
        };
        let published = Translations::from([(
            "ja".to_string(),
            crate::plugin_manifest::ManifestOverlay {
                about: Some("タスクごとに worktree を切り、マージが済んだら畳む。".into()),
                ..Default::default()
            },
        )]);

        plugin_installed::record_translations(&paths, &m.name, &published).unwrap();
        place(&paths, &m, b"#!/bin/sh\n").unwrap();

        let kept = plugin_installed::read(&paths, "worktree").unwrap();
        assert_eq!(kept.manifest.about, m.about, "the base text is inside the manifest kept on disk");
        assert_eq!(
            plugin_installed::translations(&paths, "worktree")["ja"].about.as_deref(),
            Some("タスクごとに worktree を切り、マージが済んだら畳む。"),
            "and every language the catalog published is beside it",
        );
    }
}
