//! In-place self-update for the **standalone CLI**.
//!
//! [`update_check`](crate::update_check) only ever *notices* a newer release; this module is the one
//! that acts on it — downloading the new CLI archive and swapping the running binary for it, with no
//! installer and no elevation. It exists for the **CLI-only** user (no desktop app): their
//! `~/.local/bin/amenbo` is a plain, user-writable file, so a temp-download → verify-version →
//! atomic-rename swap is enough. Where a GUI is installed the CLI is a shim into the `.app` bundle
//! and the desktop updater owns replacement — [`is_gui_managed`] detects that and this module
//! refuses, so it can never corrupt a signed bundle.
//!
//! The trust model matches the first install: the archive is fetched over TLS from the same public
//! release host, and downgrades are refused by the existing version monotonicity
//! ([`LatestRelease::is_newer_than`]). The material is the CLI archive already listed in `latest.json`
//! under this platform's `os-arch` key — that manifest is unsigned and consumer-owned, so integrity
//! here rests on TLS (as the first install does); the signed `latest-tauri.json` is the GUI updater's
//! manifest, not this one's.

use crate::update_check::{current_platform_key, LatestRelease};
use std::path::{Path, PathBuf};

/// Cap on the archive download. The CLI archive is a single vendored binary (tens of MB); this ceiling
/// is generous enough to never clip a real release yet bounds a misbehaving endpoint.
const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;

/// The reason a self-update did not complete. `UpToDate` and `GuiManaged` are **not failures** — they
/// are the two "correctly declined" outcomes the CLI reports plainly; the rest are genuine errors.
#[derive(Debug, thiserror::Error)]
pub enum SelfUpdateError {
    /// The running binary is already at or ahead of the latest published version — nothing to do.
    #[error("already up to date (running {running})")]
    UpToDate { running: String },
    /// The running binary lives inside a desktop `.app` bundle (a GUI-managed shim). Replacing it here
    /// would corrupt the bundle; the desktop updater owns replacement instead.
    #[error("this amenbo is managed by the desktop app; update it from there")]
    GuiManaged { exe: PathBuf },
    /// No CLI archive is listed for this platform in the manifest.
    #[error("no CLI archive for {platform} in the release manifest")]
    NoArchive { platform: String },
    /// Self-update over an in-place binary swap is not wired for this platform yet (Windows: needs the
    /// GUI-shim layout to guard against replacing the bundled binary through a symlink).
    #[error("self-update is not available on this platform yet — run `amenbo update` for the installer")]
    Unsupported,
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
}

/// Whether `exe` is a GUI-managed binary: it resolves to somewhere inside a macOS `.app` bundle. The
/// CLI-only install is a plain file under `~/.local/bin`, so any `.app` component in the *resolved*
/// path means this process was reached through the desktop shim — where the desktop updater, not this
/// one, does replacement. Pure over the path so it can be tested without a real install.
#[must_use]
pub fn is_gui_managed(exe: &Path) -> bool {
    exe.components()
        .any(|c| c.as_os_str().to_string_lossy().ends_with(".app"))
}

/// The CLI archive URL for the running platform: the `os-arch` (suffix-less) key in the manifest's
/// `assets` — the same tar.gz/zip the first install came from. `None` when this platform is not listed.
#[must_use]
pub fn cli_archive_url(latest: &LatestRelease) -> Option<&str> {
    latest.assets.get(&current_platform_key()).map(String::as_str)
}

/// Download the CLI archive, verify it is newer, extract the `amenbo` binary, and swap it into the
/// running executable's place. Returns [`Applied`] on success; the caller reports the two declined
/// outcomes ([`SelfUpdateError::UpToDate`], [`SelfUpdateError::GuiManaged`]) as plain messages and the
/// rest as errors. Touches no store — a CLI-only user updates without a binding.
///
/// The order matters: the cheap, offline guards (downgrade, GUI-managed, platform) run **before** any
/// network I/O, so a no-op update stays silent and traffic-free.
pub fn apply(latest: &LatestRelease) -> Result<Applied, SelfUpdateError> {
    let running = crate::agent::VERSION;

    // Windows self-update waits on the GUI-shim layout (task C) to guard a symlinked CLI from
    // replacing the bundled binary; until then Windows falls back to the installer.
    if cfg!(windows) {
        return Err(SelfUpdateError::Unsupported);
    }

    // Downgrade guard first — the whole point is monotonic versions, and it costs nothing.
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

    let archive = download(url).map_err(SelfUpdateError::Download)?;
    let binary = extract_amenbo_binary(&archive).map_err(SelfUpdateError::Extract)?;
    swap_running_binary(&exe, &binary).map_err(SelfUpdateError::Replace)?;

    Ok(Applied { from: running.to_string(), to: latest.version.clone(), path: exe })
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

/// Read the `amenbo` executable out of a gzip'd tar (the mac/linux CLI archive). We match the entry by
/// file name (`amenbo`), so extra files (LICENSE, README) and a leading directory both parse. The
/// bytes are returned rather than unpacked to disk, so the caller owns where they land.
fn extract_amenbo_binary(archive: &[u8]) -> Result<Vec<u8>, String> {
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

    /// The archive URL is the platform's suffix-less `os-arch` key — the CLI archive, never an installer.
    #[test]
    fn archive_url_picks_current_platform_cli_key() {
        let mut assets = BTreeMap::new();
        assets.insert(current_platform_key(), "https://example/amenbo-cli.tar.gz".to_string());
        // An installer key for the same platform must not be picked.
        assets.insert(format!("{}-pkg", current_platform_key()), "https://example/installer.pkg".to_string());
        let r = LatestRelease { version: "9.9.9".into(), notes_url: None, assets };
        assert_eq!(cli_archive_url(&r), Some("https://example/amenbo-cli.tar.gz"));

        let empty = LatestRelease { version: "9.9.9".into(), notes_url: None, assets: BTreeMap::new() };
        assert_eq!(cli_archive_url(&empty), None);
    }

    /// A version that is not newer than the running build declines as `UpToDate` — before any network
    /// or filesystem touch, so this runs offline.
    #[test]
    fn apply_declines_when_not_newer() {
        let same = LatestRelease {
            version: crate::agent::VERSION.to_string(),
            notes_url: None,
            assets: BTreeMap::new(),
        };
        // On Windows the platform guard fires first; either way `apply` never reaches the network.
        match apply(&same) {
            Err(SelfUpdateError::UpToDate { .. }) | Err(SelfUpdateError::Unsupported) => {}
            other => panic!("expected a no-op decline, got {other:?}"),
        }
    }

    /// The `amenbo` binary is read out of a gzip'd tar even with a leading directory and sibling files.
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
}
