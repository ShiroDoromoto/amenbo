//! In-place self-update for the **standalone CLI**.
//!
//! [`update_check`](crate::update_check) only ever *notices* a newer release; this module is the one
//! that acts on it — downloading the new CLI archive and swapping the running binary for it, with no
//! installer and no elevation. It exists for the **CLI-only** user (no desktop app): their
//! `~/.local/bin/amenbo` (or a standalone `amenbo.exe` on Windows) is a plain, user-writable file, so a
//! temp-download → verify-version → atomic-rename swap is enough. Where a GUI is installed the desktop
//! updater owns replacement of the bundled CLI — a shim into the `.app` bundle on macOS, a real copy
//! beside the GUI `amenbo-app.exe` in the NSIS `$INSTDIR` on Windows — and [`is_gui_managed`] detects
//! either shape so this module refuses, never corrupting a signed bundle or an installer-managed copy.
//!
//! A **development build** is outside this entirely: [`apply`] refuses it
//! ([`SelfUpdateError::DevChannel`]), because the archive it would fetch is the shipped CLI, and
//! installing that over `amenbo-dev` leaves a binary that reads production's app-data under the dev
//! name. [`rollback`] is deliberately *not* gated on the channel — restoring the copy an earlier
//! apply retained is the offline way back **out** of that swap, and refusing it would take the
//! recovery away from the only build that could need it.
//!
//! The archive is a gzip'd tar on mac/linux and a zip on Windows; [`extract_amenbo_binary`] reads the
//! `amenbo` / `amenbo.exe` entry out of whichever this platform ships.
//!
//! The trust model matches the first install: the archive is fetched over TLS from the same public
//! release host, and downgrades are refused by the existing version monotonicity
//! ([`LatestRelease::is_newer_than`]). The material is the `-update` copy of the CLI archive listed in
//! `latest.json` under this platform's `os-arch` key ([`cli_archive_url`]) — that manifest is unsigned
//! and consumer-owned, so integrity here rests on TLS (as the first install does); the signed
//! `latest-tauri.json` is the GUI updater's manifest, not this one's.

use crate::update_check::{current_platform_key, LatestRelease};
use std::path::{Path, PathBuf};

/// Cap on the archive download. The CLI archive is a single vendored binary (tens of MB); this ceiling
/// is generous enough to never clip a real release yet bounds a misbehaving endpoint.
const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;

/// The reason a self-update did not complete. `UpToDate`, `GuiManaged` and `DevChannel` are **not
/// failures** — they are the three "correctly declined" outcomes the CLI reports plainly; the rest
/// are genuine errors.
#[derive(Debug, thiserror::Error)]
pub enum SelfUpdateError {
    /// The running binary is already at or ahead of the latest published version — nothing to do.
    #[error("already up to date (running {running})")]
    UpToDate { running: String },
    /// The running binary is a **development build**. The release it would fetch is production's, so
    /// applying it would replace the binary under test with the shipped one — a dev build moves
    /// forward by being rebuilt, never by updating.
    #[error("{channel} is a development build; it does not update itself")]
    DevChannel { channel: String },
    /// The running binary lives inside a desktop `.app` bundle (a GUI-managed shim). Replacing it here
    /// would corrupt the bundle; the desktop updater owns replacement instead.
    #[error("this amenbo is managed by the desktop app; update it from there")]
    GuiManaged { exe: PathBuf },
    /// No CLI archive is listed for this platform in the manifest.
    #[error("no CLI archive for {platform} in the release manifest")]
    NoArchive { platform: String },
    /// A rollback was asked for but no retained previous binary (`amenbo.bak`) exists — nothing to
    /// restore. The first `--apply` writes one; a fresh install, or a store already rolled back, has none.
    #[error("no previous amenbo to roll back to (none retained at {})", .path.display())]
    NoBackup { path: PathBuf },
    /// Could not resolve the path of the running executable.
    #[error("could not locate the running executable: {0}")]
    Exe(std::io::Error),
    /// The archive download failed (network, TLS, HTTP status, size cap).
    #[error("download failed: {0}")]
    Download(String),
    /// The `amenbo` binary could not be read out of the downloaded archive.
    #[error("could not extract the amenbo binary from the archive: {0}")]
    Extract(String),
    /// Writing the extracted binary to a temp file, or swapping it into place, failed.
    #[error("could not replace the running binary: {0}")]
    Replace(std::io::Error),
}

/// A completed self-update: the versions swapped and the path that now holds the new binary.
#[derive(Debug, Clone)]
pub struct Applied {
    /// The version that was running before the swap.
    pub from: String,
    /// The version now installed.
    pub to: String,
    /// The executable path that was replaced.
    pub path: PathBuf,
    /// Where the replaced (previous) binary was retained, so a bad update can be undone with
    /// [`rollback`] — no network, no re-download. One copy is kept (overwritten by the next apply).
    pub backup: PathBuf,
}

/// A completed rollback: the running version that was undone, and the path now holding the restored
/// binary. `restored` is the version the retained binary reported at apply time — `None` if the sidecar
/// that recorded it is missing (the binary is still restored; only its version label is unknown).
#[derive(Debug, Clone)]
pub struct RolledBack {
    /// The version that was running before the rollback (the one being undone).
    pub from: String,
    /// The version now restored, when it was recorded; `None` if unknown.
    pub restored: Option<String>,
    /// The executable path that was restored.
    pub path: PathBuf,
}

/// Whether `exe` is a GUI-managed binary that the desktop updater — not this module — replaces. The
/// desktop app ships the CLI in a different shape on each OS it targets:
///
/// - **macOS**: the CLI is a shim resolving into an `.app` bundle, so any `.app` component in the
///   *resolved* path means this process was reached through the desktop shim.
/// - **Windows**: the unified NSIS installer drops the CLI as `amenbo.exe` beside the GUI
///   `amenbo-app.exe` in its per-user `$INSTDIR` (under `%LOCALAPPDATA%`), and a reinstall overwrites
///   that copy — so a sibling `amenbo-app.exe` marks this binary as the bundled one. There is no
///   symlink to follow as on macOS; NSIS installs a real copy, so the marker is the sibling file.
///
/// A standalone CLI-only install — a plain, user-writable file with neither marker — is the only shape
/// this module self-replaces. The macOS check is pure over the path; the Windows check reads the
/// filesystem for the sibling.
#[must_use]
pub fn is_gui_managed(exe: &Path) -> bool {
    // macOS: a shim resolving into an `.app` bundle.
    if exe.components().any(|c| c.as_os_str().to_string_lossy().ends_with(".app")) {
        return true;
    }
    // Windows: the NSIS-bundled CLI sits beside the GUI `amenbo-app.exe` in $INSTDIR; a standalone CLI
    // has no such sibling.
    #[cfg(windows)]
    if exe.parent().is_some_and(|dir| dir.join("amenbo-app.exe").exists()) {
        return true;
    }
    false
}

/// An older *system-wide* Linux install left behind after the move to the per-user AppImage/CLI.
///
/// The retired `.deb`/`.rpm` packages placed the GUI (`amenbo-app`) and the CLI (`amenbo`) under
/// `/usr/bin` — root-owned, package-managed. The per-user build cannot retire those: it is not root, and
/// it never auto-strips them either — it advises, and the user removes them with their package manager.
/// This detects that lingering system copy so the caller can print that guidance. It is
/// self-clearing — once the packages are gone this returns `false`, so no marker or state is needed and
/// it is idempotent by construction. On the stock PATH `~/.local/bin` precedes `/usr/bin`, so the orphan
/// does no harm beyond version skew; the advice is a courtesy, not a repair.
///
/// Advise only when the *running* binary is not itself the system copy: a user still running
/// `/usr/bin/amenbo` has not migrated, and telling them to delete the binary under their feet is wrong.
/// Pure over its inputs so it is testable off Linux; [`linux_system_orphan_present`] supplies the real
/// `current_exe()` and `/usr/bin`, gated to Linux.
#[must_use]
pub fn linux_system_orphan(running_exe: &Path, system_dir: &Path) -> bool {
    if running_exe.starts_with(system_dir) {
        return false; // still running the old system copy — not migrated yet
    }
    system_dir.join("amenbo").exists() || system_dir.join("amenbo-app").exists()
}

/// The real-path, Linux-only entry: `false` off Linux, else [`linux_system_orphan`] over `current_exe()`
/// and `/usr/bin`. `cfg!` (not `#[cfg]`) so the whole thing compiles on every platform and only the
/// boolean differs — the logic above stays covered by tests that run everywhere.
#[must_use]
pub fn linux_system_orphan_present() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    linux_system_orphan(&exe, Path::new("/usr/bin"))
}

/// The CLI archive URL for the running platform, aimed at the copy an update downloads.
///
/// The manifest's `os-arch` (suffix-less) key lists what a **first install** fetches. The release
/// carries a second copy of those same bytes under that name plus `-update`, and this is the side an
/// update takes: GitHub reports one download count per asset, so one asset serving both audiences
/// reports a sum, and a sum cannot be split back into its parts. `None` when this platform is not
/// listed.
#[must_use]
pub fn cli_archive_url(latest: &LatestRelease) -> Option<String> {
    latest.assets.get(&current_platform_key()).map(|url| update_named(url))
}

/// Put the `-update` suffix ahead of a URL's archive extension. The two forms the CLI archive takes
/// are matched whole rather than by hunting for a `.`: `.tar.gz` is one extension, and the version
/// sits in the file name carrying dots of its own (`amenbo_2.0.1_linux_amd64.tar.gz`), so neither the
/// first `.` nor the last one finds the right place. Anything else takes the suffix at its end.
fn update_named(url: &str) -> String {
    for ext in [".tar.gz", ".zip"] {
        if let Some(stem) = url.strip_suffix(ext) {
            return format!("{stem}-update{ext}");
        }
    }
    format!("{url}-update")
}

/// Download the CLI archive, verify it is newer, extract the `amenbo` binary, and swap it into the
/// running executable's place. Returns [`Applied`] on success; the caller reports the three declined
/// outcomes ([`SelfUpdateError::DevChannel`], [`SelfUpdateError::UpToDate`],
/// [`SelfUpdateError::GuiManaged`]) as plain messages and the rest as errors. Touches no store — a
/// CLI-only user updates without a binding.
///
/// The order matters: the cheap, offline guards (channel, downgrade, GUI-managed, platform) run
/// **before** any network I/O, so a no-op update stays silent and traffic-free.
pub fn apply(latest: &LatestRelease) -> Result<Applied, SelfUpdateError> {
    let running = crate::agent::VERSION;

    // The channel before the versions: a development build is behind production as a matter of
    // course, so "newer" here means production itself, and swapping it in would leave a binary that
    // opens production's app-data under the dev build's name. Nothing further down can tell the two
    // apart — the archive is the shipped one either way — so this is refused rather than compared.
    if crate::config::Paths::is_dev_channel() {
        return Err(SelfUpdateError::DevChannel {
            channel: crate::config::Paths::APP_NAME.to_string(),
        });
    }

    // Downgrade guard next — the whole point is monotonic versions, and it costs nothing.
    if !latest.is_newer_than(running) {
        return Err(SelfUpdateError::UpToDate { running: running.to_string() });
    }

    // Resolve (following symlinks) so a shim into an `.app` is seen for what it is.
    let exe = std::env::current_exe().map_err(SelfUpdateError::Exe)?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    if is_gui_managed(&exe) {
        return Err(SelfUpdateError::GuiManaged { exe });
    }

    let url = cli_archive_url(latest)
        .ok_or_else(|| SelfUpdateError::NoArchive { platform: current_platform_key() })?;

    let archive = download(&url).map_err(SelfUpdateError::Download)?;
    let binary = extract_amenbo_binary(&archive).map_err(SelfUpdateError::Extract)?;

    // Retain the current binary before replacing it, so a bad update can be undone offline (see
    // `rollback`). This runs before the swap: if it fails, the live binary is untouched and we abort
    // rather than update with no way back. One copy is kept — the next apply overwrites it.
    let backup = backup_path(&exe);
    std::fs::copy(&exe, &backup).map_err(SelfUpdateError::Replace)?;
    // Record the retained version for a useful rollback message. Best-effort: a missing sidecar only
    // costs the version label, never the restore itself.
    let _ = std::fs::write(backup_version_path(&exe), running);

    swap_running_binary(&exe, &binary).map_err(SelfUpdateError::Replace)?;

    Ok(Applied { from: running.to_string(), to: latest.version.clone(), path: exe, backup })
}

/// Undo the last [`apply`] by restoring the binary it retained at [`backup_path`]. Offline and instant —
/// no download, no version check (a rollback is a deliberate downgrade). Refuses the same GUI-managed case
/// as [`apply`] (the desktop updater owns a bundled CLI), and reports [`SelfUpdateError::NoBackup`] when no
/// retained binary exists. On success the retained copy is consumed (there is nothing further to roll back
/// to).
pub fn rollback() -> Result<RolledBack, SelfUpdateError> {
    // Same shim guard as apply: a GUI-managed CLI is owned by the desktop updater, so it does not
    // self-replace here. A standalone CLI (any OS) does.
    let exe = std::env::current_exe().map_err(SelfUpdateError::Exe)?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    if is_gui_managed(&exe) {
        return Err(SelfUpdateError::GuiManaged { exe });
    }

    let backup = backup_path(&exe);
    if !backup.exists() {
        return Err(SelfUpdateError::NoBackup { path: backup });
    }
    let bytes = std::fs::read(&backup).map_err(SelfUpdateError::Replace)?;
    let restored = std::fs::read_to_string(backup_version_path(&exe))
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    swap_running_binary(&exe, &bytes).map_err(SelfUpdateError::Replace)?;
    // The retained copy is now the running binary — nothing is left to roll back to. Clear both so a
    // stale `.bak` never masquerades as a fresh one.
    let _ = std::fs::remove_file(&backup);
    let _ = std::fs::remove_file(backup_version_path(&exe));

    Ok(RolledBack { from: crate::agent::VERSION.to_string(), restored, path: exe })
}

/// Where the previous binary is retained beside the running one: `amenbo` → `amenbo.bak` (and
/// `amenbo.exe` → `amenbo.bak` on Windows). A sibling on the same filesystem, so retaining it is a cheap
/// copy and restoring it is the same atomic swap.
#[must_use]
pub fn backup_path(exe: &Path) -> PathBuf {
    exe.with_extension("bak")
}

/// The sidecar recording the retained binary's version (`amenbo.bak.version`), so a rollback can name
/// what it restored. Purely advisory — its absence never blocks a restore.
#[must_use]
fn backup_version_path(exe: &Path) -> PathBuf {
    let mut p = backup_path(exe).into_os_string();
    p.push(".version");
    PathBuf::from(p)
}

/// Fetch the archive bytes over TLS, with the same short-lived agent as the update check but a
/// download-sized read cap. HTTP errors and the size cap both surface as `Err(String)`.
fn download(url: &str) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().build().into();
    let mut resp = agent.get(url).call().map_err(|e| e.to_string())?;
    resp.body_mut()
        .with_config()
        .limit(MAX_ARCHIVE_BYTES)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

/// Read the `amenbo` executable out of the downloaded CLI archive. The format is the one wharfy writes
/// for this platform — a gzip'd tar on mac/linux, a zip on Windows — so extraction dispatches on the
/// target. Either backend matches the entry by file name (`amenbo` / `amenbo.exe`), so extra files
/// (LICENSE, README) and a leading directory both parse. The bytes are returned rather than unpacked
/// to disk, so the caller owns where they land.
fn extract_amenbo_binary(archive: &[u8]) -> Result<Vec<u8>, String> {
    #[cfg(not(windows))]
    {
        extract_from_tar_gz(archive)
    }
    #[cfg(windows)]
    {
        extract_from_zip(archive)
    }
}

/// The mac/linux path: un-gzip the tar and stream out the `amenbo` entry.
#[cfg(not(windows))]
fn extract_from_tar_gz(archive: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let gz = flate2::read::GzDecoder::new(archive);
    let mut tar = tar::Archive::new(gz);
    let entries = tar.entries().map_err(|e| e.to_string())?;
    for entry in entries {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let is_amenbo = entry
            .path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_os_string()))
            .is_some_and(|n| n == "amenbo");
        if is_amenbo {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            return Ok(buf);
        }
    }
    Err("archive contains no `amenbo` entry".to_string())
}

/// The Windows path: read the `amenbo.exe` entry out of the zip (wharfy writes a single Deflate entry).
/// `enclosed_name` sanitizes the path (no zip-slip), and we match by file name so a leading directory
/// still resolves — mirroring the tar reader's tolerance.
#[cfg(windows)]
fn extract_from_zip(archive: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive)).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let is_amenbo = entry
            .enclosed_name()
            .and_then(|p| p.file_name().map(|n| n.to_os_string()))
            .is_some_and(|n| n == "amenbo.exe");
        if is_amenbo {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            return Ok(buf);
        }
    }
    Err("archive contains no `amenbo.exe` entry".to_string())
}

/// Write the new binary beside the running one (same filesystem, so the swap is an atomic rename) with
/// executable permission, then hand it to `self_replace`, which moves the live executable aside and
/// renames the new file into its place.
fn swap_running_binary(exe: &Path, binary: &[u8]) -> std::io::Result<()> {
    let tmp = exe.with_file_name(format!(".amenbo-update-{}", std::process::id()));
    std::fs::write(&tmp, binary)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    let result = self_replace::self_replace(&tmp);
    // `self_replace` consumes the temp on success; on failure clear it so a retry is clean.
    let _ = std::fs::remove_file(&tmp);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::io::Write;

    /// A plain CLI-only install path is not GUI-managed; a shim resolving into an `.app` bundle is.
    #[test]
    fn gui_managed_detects_app_bundle_only() {
        assert!(!is_gui_managed(Path::new("/Users/alice/.local/bin/amenbo")));
        assert!(!is_gui_managed(Path::new("/usr/local/bin/amenbo")));
        assert!(is_gui_managed(Path::new(
            "/Users/alice/Applications/amenbo.app/Contents/MacOS/amenbo"
        )));
        assert!(is_gui_managed(Path::new("/Applications/amenbo.app/Contents/MacOS/amenbo")));
    }

    /// The Linux system-wide orphan is detected only when a per-user build is what ran and a `/usr/bin`
    /// copy (CLI or GUI) is still present. Running the old `/usr/bin` copy itself is not a migration, so
    /// it never advises deleting the binary under the user's feet. Touches a real scratch dir standing in
    /// for `/usr/bin`, so it runs on every platform.
    #[test]
    fn linux_system_orphan_wants_a_per_user_run_and_a_lingering_system_copy() {
        let sys = amenbo_scratch::scratch("self-update-orphan");
        let per_user = std::path::Path::new("/home/alice/.local/bin/amenbo");

        // No system copy at all → nothing to retire.
        assert!(!linux_system_orphan(per_user, &sys));

        // A lingering system CLI → advise the per-user run.
        std::fs::write(sys.join("amenbo"), b"cli").unwrap();
        assert!(linux_system_orphan(per_user, &sys));

        // Still running the old system copy → not migrated yet, so no advice.
        assert!(!linux_system_orphan(&sys.join("amenbo"), &sys));

        // The GUI sidecar alone is enough to trigger it (the `.deb`/`.rpm` shipped both).
        std::fs::remove_file(sys.join("amenbo")).unwrap();
        std::fs::write(sys.join("amenbo-app"), b"gui").unwrap();
        assert!(linux_system_orphan(per_user, &sys));
    }

    /// The retained-binary path is the executable's own name with a `.bak` extension, and its version
    /// sidecar sits right beside it — both plain siblings on the same filesystem as the running binary.
    #[test]
    fn backup_paths_sit_beside_the_running_binary() {
        let exe = Path::new("/Users/alice/.local/bin/amenbo");
        assert_eq!(backup_path(exe), Path::new("/Users/alice/.local/bin/amenbo.bak"));
        assert_eq!(
            backup_version_path(exe),
            Path::new("/Users/alice/.local/bin/amenbo.bak.version")
        );
    }

    /// The archive URL is the platform's suffix-less `os-arch` key — the CLI archive, never an
    /// installer — aimed at the `-update` copy of it.
    #[test]
    fn archive_url_picks_current_platform_cli_key() {
        let mut assets = BTreeMap::new();
        assets.insert(current_platform_key(), "https://example/amenbo-cli.tar.gz".to_string());
        // An installer key for the same platform must not be picked.
        assets.insert(format!("{}-pkg", current_platform_key()), "https://example/installer.pkg".to_string());
        let r = LatestRelease { version: "9.9.9".into(), notes_url: None, assets };
        assert_eq!(cli_archive_url(&r).as_deref(), Some("https://example/amenbo-cli-update.tar.gz"));

        let empty = LatestRelease { version: "9.9.9".into(), notes_url: None, assets: BTreeMap::new() };
        assert_eq!(cli_archive_url(&empty), None);
    }

    /// The suffix lands ahead of the whole extension: the version's own dots sit in the same file
    /// name, so a split hunting for a `.` would cut inside it.
    #[test]
    fn update_name_goes_before_the_extension() {
        assert_eq!(
            update_named("https://github.com/o/r/releases/download/v2.0.1/amenbo_2.0.1_linux_amd64.tar.gz"),
            "https://github.com/o/r/releases/download/v2.0.1/amenbo_2.0.1_linux_amd64-update.tar.gz"
        );
        assert_eq!(
            update_named("https://github.com/o/r/releases/download/v2.0.1/amenbo_2.0.1_windows_amd64.zip"),
            "https://github.com/o/r/releases/download/v2.0.1/amenbo_2.0.1_windows_amd64-update.zip"
        );
        // A URL in neither archive form takes the suffix at its end.
        assert_eq!(update_named("https://example.com/v1.2.3/amenbo"), "https://example.com/v1.2.3/amenbo-update");
    }

    /// A version that is not newer than the running build declines as `UpToDate` — the downgrade guard
    /// runs first on every platform, before any network or filesystem touch, so this runs offline.
    #[test]
    fn apply_declines_when_not_newer() {
        let same = LatestRelease {
            version: crate::agent::VERSION.to_string(),
            notes_url: None,
            assets: BTreeMap::new(),
        };
        match apply(&same) {
            Err(SelfUpdateError::UpToDate { .. }) => {}
            other => panic!("expected a no-op decline, got {other:?}"),
        }
    }

    /// The `amenbo` binary is read out of a gzip'd tar even with a leading directory and sibling files.
    #[cfg(not(windows))]
    #[test]
    fn extract_finds_amenbo_among_siblings() {
        let mut tar_buf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);
            let payload = b"#!/bin/true\namenbo-binary\n";
            let mut header = tar::Header::new_gnu();
            header.set_size(payload.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, "amenbo_1.0.3/amenbo", &payload[..]).unwrap();

            let readme = b"docs";
            let mut h2 = tar::Header::new_gnu();
            h2.set_size(readme.len() as u64);
            h2.set_cksum();
            builder.append_data(&mut h2, "amenbo_1.0.3/README.md", &readme[..]).unwrap();
            builder.finish().unwrap();
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&tar_buf).unwrap();
        let archive = gz.finish().unwrap();

        let got = extract_amenbo_binary(&archive).expect("finds the amenbo entry");
        assert_eq!(got, b"#!/bin/true\namenbo-binary\n");
    }

    /// An archive with no `amenbo` entry is an error, not a silent empty binary.
    #[cfg(not(windows))]
    #[test]
    fn extract_errors_without_amenbo_entry() {
        let mut tar_buf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);
            let other = b"nope";
            let mut header = tar::Header::new_gnu();
            header.set_size(other.len() as u64);
            header.set_cksum();
            builder.append_data(&mut header, "README.md", &other[..]).unwrap();
            builder.finish().unwrap();
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&tar_buf).unwrap();
        let archive = gz.finish().unwrap();

        assert!(extract_amenbo_binary(&archive).is_err());
    }

    /// On Windows the bundled CLI sits beside the GUI `amenbo-app.exe` in the NSIS $INSTDIR; a
    /// standalone CLI has no such sibling. The guard reads the filesystem, so it is exercised against a
    /// real scratch directory rather than a bare path.
    #[cfg(windows)]
    #[test]
    fn gui_managed_detects_bundled_sibling_on_windows() {
        let dir = amenbo_scratch::scratch("self-update-guard");
        let exe = dir.join("amenbo.exe");
        std::fs::write(&exe, b"cli").unwrap();
        // Standalone: no GUI sibling → self-update allowed.
        assert!(!is_gui_managed(&exe));
        // Bundled: the installer's GUI `amenbo-app.exe` sits beside it → refused.
        std::fs::write(dir.join("amenbo-app.exe"), b"gui").unwrap();
        assert!(is_gui_managed(&exe));
    }

    /// The `amenbo.exe` binary is read out of a Deflate zip even with a leading directory and siblings.
    #[cfg(windows)]
    #[test]
    fn extract_finds_amenbo_exe_in_zip() {
        use zip::write::{SimpleFileOptions, ZipWriter};
        use zip::CompressionMethod;
        let payload: &[u8] = b"MZ\x90\x00amenbo-exe-bytes";
        let mut buf = Vec::new();
        {
            let mut zw = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zw.start_file("amenbo_1.0.3/amenbo.exe", opts).unwrap();
            zw.write_all(payload).unwrap();
            zw.start_file("amenbo_1.0.3/README.md", opts).unwrap();
            zw.write_all(b"docs").unwrap();
            zw.finish().unwrap();
        }
        let got = extract_amenbo_binary(&buf).expect("finds the amenbo.exe entry");
        assert_eq!(got, payload);
    }

    /// A zip with no `amenbo.exe` entry is an error, not a silent empty binary.
    #[cfg(windows)]
    #[test]
    fn extract_errors_without_amenbo_exe_in_zip() {
        use zip::write::{SimpleFileOptions, ZipWriter};
        use zip::CompressionMethod;
        let mut buf = Vec::new();
        {
            let mut zw = ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zw.start_file("README.md", opts).unwrap();
            zw.write_all(b"nope").unwrap();
            zw.finish().unwrap();
        }
        assert!(extract_amenbo_binary(&buf).is_err());
    }
}
