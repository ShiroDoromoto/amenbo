//! **Install** — the one door bytes off the network become a plugin on this machine
//! (`AMB-D-347`/`AMB-D-351`/`AMB-D-360`).
//!
//! `plugin_installed` reads what is on disk and says how an install *lands*; this is the other half —
//! how it gets there. One name in, four gates, then the layout that module defines:
//!
//! 1. **Resolve** the name against the catalog ([`crate::plugin_catalog`]) — the only source an install
//!    reads. A name the catalog does not carry is not installable, and when intake *dropped* the entry
//!    that name is reported with the reason rather than as a plain "no such plugin".
//! 2. **Refuse to overwrite** (`AMB-D-360`): a name already installed is a conflict a person resolves
//!    (`plugin uninstall`, or the update path of `AMB-D-359`), never a silent replacement. A half-written
//!    home — no manifest, so [`plugin_installed::read`] reads it as absent —
//!    is not an install and is written over, which is what makes a failed install retryable.
//! 3. **Refuse a platform the manifest does not claim**, and resolve this one's distributable
//!    (`AMB-D-381`): a per-OS `assets` entry, or the single `url` of an entry that is one file everywhere.
//!    An OS outside the declared set is a binary that was never built to run here.
//! 4. **Verify provenance fail-closed** ([`crate::plugin_provenance::verify_catalog_asset`], `AMB-D-371`):
//!    the minisign signature against the key amenbo ships, then that distributable's checksum, both over
//!    the exact bytes the URL served. Unsigned, signed by another key, or a digest that does not match, and
//!    nothing is written. The key is never a parameter here — a caller cannot install against another
//!    trust root.
//!
//! **What the asset may be.** The bytes are recognised by what they start with, not by the URL's
//! extension: a gzip'd tar (the form the catalog's own example publishes), from which the entry named
//! after the plugin is taken, or the executable itself. A zip is refused rather than guessed at — naming
//! the two accepted shapes beats a half-supported third. Extraction never trusts a path: the entry is
//! matched on its *file name*, so a member called `../../etc/cron.d/x` is simply not the one being looked
//! for, and only a regular file is ever read.
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
use crate::error::{Error, Result};
use crate::plugin_catalog::{self, Catalog, Dropped, Entry};
use crate::plugin_installed;
use crate::plugin_manifest::{Manifest, Platform};
use crate::plugin_provenance;

/// Cap on the asset download, and on what is read out of an archive entry — the second is what keeps a
/// small gzip stream from expanding without bound. A plugin is one executable, so this ceiling is far
/// above any real asset while still bounding a misbehaving endpoint.
const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;

/// The first bytes of a gzip stream (`.tar.gz`).
const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

/// The first bytes of a zip archive — recognised only so it can be refused by name.
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
    // The current catalog when the network answers, the cached one when it does not: an install resolves
    // against the freshest index available, and stays possible offline once a catalog has been fetched.
    let catalog = plugin_catalog::load(paths)?;
    let entry = resolve(&catalog, name)?;
    let manifest = entry.manifest.clone();
    refuse_an_overwrite(paths, &manifest.name)?;
    let program = fetch_verified_program(&manifest)?;
    place(paths, &manifest, &program)
}

/// The plugin's executable, off the network and through the trust gates — the *only* way bytes named by a
/// manifest become bytes amenbo will write.
///
/// Platform, provenance and packaging in one call, because taking them apart is how a caller ends up
/// with a partial chain: [`crate::plugin_update`] replaces an installed binary through exactly this
/// function (`AMB-D-359` — an update re-verifies), and there is deliberately no entry point beside it
/// that fetches without verifying. The key is never a parameter (see the module docs).
pub(crate) fn fetch_verified_program(manifest: &Manifest) -> Result<Vec<u8>> {
    // What is fetched, what it must hash to and who signed it are all this platform's (`AMB-D-381`) —
    // one lookup, so provenance can never be checked against another OS's bytes.
    let here = refuse_another_platform(manifest)?;
    let published = published_for(manifest, here)?;
    let asset = download(&published.url)?;
    plugin_provenance::verify_catalog_asset(
        &asset,
        published.signature.as_deref(),
        &published.checksum,
    )?;
    unpack_program(&asset, &manifest.name)
}

/// The catalog entry this name resolves to. A name the catalog does not carry is `not_found` — unless
/// intake dropped an entry under exactly that name, in which case the drop is the answer: "the catalog
/// has it, and it did not pass the door" is a different problem from "no such plugin", and only one of
/// them is the user's typo.
fn resolve<'a>(catalog: &'a Catalog, name: &str) -> Result<&'a Entry> {
    if let Some(entry) = catalog.find(name) {
        return Ok(entry);
    }
    for dropped in &catalog.dropped {
        match dropped {
            Dropped::Invalid { name: dropped_name, problems } if dropped_name == name => {
                let first = problems
                    .first()
                    .map(|p| format!("{}: {}", p.location, p.message.en()))
                    .unwrap_or_default();
                return Err(Error::invalid(
                    format!("the catalog's entry for '{name}' is not valid and was dropped ({first})"),
                    format!("目録の '{name}' は検証に通らず取り込み時に落とされました（{first}）"),
                ));
            }
            Dropped::Duplicate { name: dropped_name } if dropped_name == name => {
                return Err(Error::invalid(
                    format!("the catalog carries more than one entry named '{name}'"),
                    format!("目録に '{name}' という名前のエントリが複数あります"),
                ));
            }
            _ => {}
        }
    }
    Err(Error::not_found(
        format!("no plugin named '{name}' in the catalog"),
        format!("目録にプラグイン '{name}' はありません"),
    ))
}

/// Refuse to install over a name this machine already holds (`AMB-D-360`) — no silent overwrite, in
/// either of the two shapes it could take: a working install (which is `plugin update`'s to move, not
/// install's), and a broken one (which is `plugin uninstall`'s to clear). A home with no manifest is
/// neither: it is the residue of an install that did not finish, so it is written over rather than
/// stood in the way of a retry.
fn refuse_an_overwrite(paths: &Paths, name: &str) -> Result<()> {
    match plugin_installed::read(paths, name) {
        Ok(_) => Err(Error::conflict(
            format!("plugin '{name}' is already installed on this machine"),
            format!("プラグイン '{name}' はこのマシンに既にインストールされています"),
        )),
        Err(e) if e.code() == "not_found" => Ok(()),
        Err(e) => Err(Error::conflict(
            format!("a broken install of '{name}' is in the way ({}) — uninstall it first", e.message_en()),
            format!(
                "'{name}' の壊れたインストールが残っています（{}）——先に uninstall してください",
                e.message_en()
            ),
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
    Err(Error::invalid(
        format!("plugin '{}' does not support {here} (it supports: {supported})", manifest.name),
        format!(
            "プラグイン '{}' は {here} に対応していません（対応: {supported}）",
            manifest.name
        ),
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
        Error::invalid(
            format!(
                "plugin '{}' lists {} but publishes no asset for {}",
                manifest.name,
                here.os.as_str(),
                here.token()
            ),
            format!(
                "プラグイン '{}' は {} を挙げていますが、{} 向けの配布物がありません",
                manifest.name,
                here.os.as_str(),
                here.token()
            ),
        )
    })
}

/// Fetch the asset, with the same short-lived agent the rest of amenbo's downloads use and a
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

/// The executable inside a verified asset, recognised by the asset's own leading bytes: a gzip'd tar, or
/// the executable itself. A zip is named and refused rather than guessed at.
///
/// This runs **after** provenance, so the bytes are the ones the catalog blessed — the shapes here are a
/// packaging question, not a trust one. The size cap still applies to what comes out of the archive,
/// which is the part a signature cannot bound.
fn unpack_program(asset: &[u8], name: &str) -> Result<Vec<u8>> {
    if asset.is_empty() {
        return Err(Error::invalid(
            format!("the asset for plugin '{name}' is empty"),
            format!("プラグイン '{name}' の asset が空です"),
        ));
    }
    if asset.starts_with(&GZIP_MAGIC) {
        return from_tar_gz(asset, name);
    }
    if asset.starts_with(&ZIP_MAGIC) {
        return Err(Error::invalid(
            format!(
                "the asset for plugin '{name}' is a zip — publish it as a .tar.gz, or as the executable itself"
            ),
            format!(
                "プラグイン '{name}' の asset が zip です——.tar.gz か実行ファイルそのものとして配布してください"
            ),
        ));
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
        Error::invalid(
            format!("the asset for plugin '{name}' is not a readable .tar.gz: {e}"),
            format!("プラグイン '{name}' の asset を .tar.gz として読めません：{e}"),
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
    Err(Error::invalid(
        format!("the asset for plugin '{name}' holds no '{wanted}' entry"),
        format!("プラグイン '{name}' の asset に '{wanted}' が入っていません"),
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
        return Err(Error::invalid(
            format!("'{name}' is not a plugin name (it is reserved for the registry cache)"),
            format!("'{name}' はプラグイン名ではありません（目録キャッシュ用に予約されています）"),
        ));
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
        Error::invalid(
            format!("the manifest for plugin '{name}' cannot be written out: {e}"),
            format!("プラグイン '{name}' の manifest を書き出せません：{e}"),
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
            desc: "a test plugin".to_string(),
            author: "amenbo".to_string(),
            repo: "ShiroDoromoto/amenbo".to_string(),
            os: vec![Os::Macos, Os::Linux, Os::Windows],
            category: "workflow".to_string(),
            url: "https://example.invalid/x.tar.gz".to_string(),
            checksum: format!("sha256:{}", "a".repeat(64)),
            signature: Some("sig".to_string()),
            assets: Default::default(),
            official: false,
            scope: crate::plugin_manifest::Scope::Project,
            payload_v: 1,
            min_amenbo: None,
            config: Vec::new(),
            events: Vec::new(),
        }
    }

    /// A catalog holding these entries, built the way a real one is — through intake.
    fn catalog_of(entries: Vec<serde_json::Value>) -> Catalog {
        let json =
            serde_json::json!({ "catalog_v": 1, "generated_at": "2026-07-23T04:57:10Z", "plugins": entries })
                .to_string();
        plugin_catalog::parse(&json).unwrap()
    }

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
        let catalog = catalog_of(vec![entry_json("worktree"), entry_json("slack")]);
        assert_eq!(resolve(&catalog, "slack").unwrap().manifest.desc, "a plugin");
    }

    #[test]
    fn an_unlisted_name_is_not_found() {
        let catalog = catalog_of(vec![entry_json("worktree")]);
        assert_eq!(resolve(&catalog, "slack").unwrap_err().code(), "not_found");
    }

    /// A name intake dropped answers with the drop, not with "no such plugin" — the catalog does carry
    /// it, and the reason is what the user needs.
    #[test]
    fn a_name_the_catalog_dropped_is_reported_with_the_reason() {
        let mut invalid = entry_json("slack");
        invalid["url"] = serde_json::json!("http://example.invalid/x.tar.gz"); // not https
        let catalog = catalog_of(vec![invalid]);

        let err = resolve(&catalog, "slack").unwrap_err();
        assert_eq!(err.code(), "invalid_value");
        assert!(format!("{err:?}").contains("dropped"), "the drop is the answer: {err:?}");
    }

    // ---- the gates that run before anything is fetched ----

    #[test]
    fn an_installed_name_is_not_overwritten() {
        let paths = paths_at("overwrite");
        place(&paths, &manifest("worktree"), b"#!/bin/sh\n").unwrap();

        let err = refuse_an_overwrite(&paths, "worktree").unwrap_err();
        assert_eq!(err.code(), "conflict");
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
        assert_eq!(err.code(), "conflict");
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

        let here = Os::here().expect("amenbo runs on an OS its manifests can name");
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

    #[test]
    fn a_zip_asset_is_refused_by_name() {
        let mut zip = ZIP_MAGIC.to_vec();
        zip.extend_from_slice(b"...the rest of a zip...");
        let err = unpack_program(&zip, "worktree").unwrap_err();
        assert!(format!("{err:?}").contains("zip"), "the shape is named: {err:?}");
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
    /// path amenbo computes, so an entry buried under directories lands in the same home as any other.
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
}
