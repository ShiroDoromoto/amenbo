//! Querying the static `latest.json`.
//!
//! amenbo keeps its **functional traffic at zero**: local-first, no central server, no user data
//! ever leaving the machine. That core is untouched; what this module owns is the one piece of
//! **infrastructure traffic** — noticing that a newer release exists. It queries the static
//! `latest.json` hanging off the newest release of the project's own repository: a version plus
//! minimal metadata, carrying no user data. (It is wharfy's release step that publishes it.)
//!
//! The policy: **on by default**, with a config knob to disable it
//! ([`crate::config::Config::update_check`]); **timed out**; **silent on failure** (a failed update
//! check must never get in the way of the real work); and **cached**, so we do not talk to the
//! network on every command. This module returns only the awareness; **acting** on it — downloading
//! and swapping the binary in place — is [`crate::self_update`]'s job (the standalone CLI), which
//! reuses the [`LatestRelease`] fetched here.
//!
//! One channel is outside all of that: a **development build never queries at all**
//! ([`is_disabled`]). Its version is normally *behind* what production's manifest names, so every
//! answer this module could hand it is either an offer to install production over itself or a claim
//! to be current that it cannot make. Withholding the material closes both, for every caller, with
//! no traffic.
//!
//! The module does nothing but query and fetch. Comparing the fetched version against the running
//! binary to derive `update_available` is the caller's job (it is folded into
//! [`crate::store::VersionStatus`]).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// The default endpoint: the static manifest hanging off the **newest** release of the project's
/// own repository. `releases/latest/download/…` is a stable redirect (302 to an object CDN) that
/// does not hit the GitHub Releases API, so we can fetch it as a plain, cacheable static file. It is
/// the counterpart of the URL the publisher writes to.
///
/// A shipped binary carries this URL baked in, so moving the releases elsewhere strands every
/// existing install on the old address until it updates once.
pub const LATEST_JSON_URL: &str =
    "https://github.com/ShiroDoromoto/amenbo/releases/latest/download/latest.json";

/// Where the "apply the update" affordance falls back to: the download page of the latest release.
/// We land here whenever the unified-installer URL for the current OS cannot be read out of
/// `latest.json` — the query failed, the asset is not listed, or the env var disabled the check —
/// and the user can pick from the list by hand. This installer affordance only ever *opens* a page;
/// the in-place swap is [`crate::self_update`]'s separate path (`amenbo update --apply`).
pub const LATEST_RELEASE_PAGE: &str = "https://github.com/ShiroDoromoto/amenbo/releases/latest";

/// Timeout on the query. We give up silently on failure, so keep it short enough that it never
/// needlessly stalls an interactive command.
const FETCH_TIMEOUT: Duration = Duration::from_secs(3);

/// How long a cached result stays valid, in seconds. Within this window we answer from the cache
/// without re-querying, keeping the request rate about as modest as Homebrew's.
const CACHE_TTL_SECS: i64 = 24 * 60 * 60;

/// Schema of `latest.json` (published by wharfy's release step). Everything but `version` is
/// optional metadata for display and affordances; the query still succeeds without it, because the
/// fields carry `#[serde(default)]` and unknown fields are ignored — forwards- and
/// backwards-compatible.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LatestRelease {
    /// The newest published version (e.g. `"0.1.5"`). The basis for deciding an update exists.
    pub version: String,
    /// URL of the release-notes page (the GitHub Release tag). The fallback for when we cannot work
    /// out the direct affordance for a given OS.
    #[serde(default)]
    pub notes_url: Option<String>,
    /// `os-arch[-kind]` → distribution asset URL (wharfy's `assets` map). The key vocabulary follows
    /// wharfy: os is `macos` / `windows` / `linux`, arch is `x64` / `arm64`. A unified installer
    /// carries a kind suffix (`macos-arm64-pkg`, `windows-x64-exe`, `linux-x64-appimage`), while a
    /// suffix-less key (`macos-arm64`, …) is the CLI archive (tar.gz/zip). The "open the latest
    /// installer" affordance looks up the installer key for the current OS.
    #[serde(default)]
    pub assets: std::collections::BTreeMap<String, String>,
}

impl LatestRelease {
    /// Whether this upstream version is **newer** than `current`, the version of the running binary
    /// (e.g. [`crate::agent::VERSION`]). The comparison shares the logic used for cross-surface
    /// checks: pre-release and build metadata are ignored, and anything unparseable is `false` on
    /// the safe side. This is the predicate the CLI and GUI use to surface "there is a newer release
    /// upstream" — a thin public API that keeps raw version comparison out of the callers.
    #[must_use]
    pub fn is_newer_than(&self, current: &str) -> bool {
        crate::store::version_is_newer(&self.version, current)
    }

    /// The unified-installer URL for the current OS/arch: the URL listed in `assets` under this
    /// platform's installer key ([`installer_asset_key`], e.g. `macos-arm64-pkg`), or `None` when
    /// the manifest lists nothing under it. `None` is the honest answer for a platform the release
    /// published no installer for, and for a manifest published before it did.
    ///
    /// This is the manifest as published: the name a **first install** fetches. The update side of it
    /// is [`update_named`], which [`update_url`](Self::update_url) applies.
    #[must_use]
    pub fn installer_for_current_platform(&self) -> Option<&str> {
        self.assets.get(&installer_asset_key()).map(String::as_str)
    }

    /// The URL the "apply the update" affordance should open: the current OS's unified installer if
    /// there is one — under its **update-download name** ([`update_named`]) — else the release-notes
    /// page (`notes_url`), else the latest-release page ([`LATEST_RELEASE_PAGE`]). This is the
    /// **installer** affordance — the CLI and GUI merely **open** this URL in the OS's default
    /// browser; the standalone CLI's in-place swap is a separate path ([`crate::self_update`],
    /// `amenbo update --apply`).
    ///
    /// Everyone who arrives here is updating, so the installer is taken from the update side
    /// (`AMB-D-441`): whoever opens this already has amenbo. Only the installer is renamed — the two
    /// fallbacks are pages, not assets, and carry no count to keep apart.
    #[must_use]
    pub fn update_url(&self) -> String {
        self.installer_for_current_platform()
            .map(update_named)
            .unwrap_or_else(|| self.notes_url.as_deref().unwrap_or(LATEST_RELEASE_PAGE).to_string())
    }
}

/// Map the machine onto wharfy's `assets` platform key (`os-arch`). We follow wharfy's vocabulary:
/// the OS matches `std::env::consts::OS` verbatim (macOS → `macos`, plus `windows` and `linux`), and
/// the arch is [`native_arch`] — the machine's own, not the running binary's (`AMB-D-551`). So a Mac
/// with Apple silicon answers `macos-arm64` whether the amenbo asking is the arm64 build or the
/// Intel one under Rosetta. This is also the key of the CLI archive (the suffix-less one);
/// [`installer_asset_key`] appends the kind to get the installer key.
#[must_use]
pub fn current_platform_key() -> String {
    format!("{}-{}", std::env::consts::OS, native_arch())
}

/// The architecture of the **machine**, in wharfy's tokens (`arm64` / `x64`) — what an update is
/// aimed at (`AMB-D-551`).
///
/// It is asked of the OS rather than read off the build, because the two disagree exactly where it
/// matters: a binary built for one arch and running on another under emulation carries the
/// emulated arch in `std::env::consts::ARCH` for its whole life, so it would go on fetching the
/// emulated build forever and the machine would never come off the translation layer. Whoever
/// applies an update lands on the build their machine runs natively.
///
/// Where the OS has no answer — it does not emulate, it was not asked in a way it understands, or
/// this is a platform with no ask at all — the running binary's own arch is the answer, which is
/// what this returned before there was a question. Mapping it is the same rewrite wharfy's names
/// use: `aarch64` → `arm64`, `x86_64` → `x64`, anything else verbatim (an arch amenbo does not
/// distribute simply keys nothing).
#[must_use]
pub fn native_arch() -> &'static str {
    machine_arch().unwrap_or(match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    })
}

/// Ask macOS whether this process is being translated. Rosetta 2 is the only translation the OS
/// runs and it runs x86_64 on Apple silicon, so "translated" names the machine on its own: arm64.
/// A process that is not translated is running natively, and `sysctl.proc_translated` is absent
/// altogether on a Mac with no Rosetta — both are "nothing to say", and the build's own arch is
/// then the machine's.
#[cfg(target_os = "macos")]
fn machine_arch() -> Option<&'static str> {
    let mut translated: libc::c_int = 0;
    let mut size = std::mem::size_of::<libc::c_int>();
    // SAFETY: the name is a NUL-terminated C string, and the out-buffer is one `c_int` with its
    // length passed alongside — sysctl writes at most that many bytes and reports what it wrote.
    let asked = unsafe {
        libc::sysctlbyname(
            c"sysctl.proc_translated".as_ptr(),
            std::ptr::addr_of_mut!(translated).cast(),
            std::ptr::addr_of_mut!(size),
            std::ptr::null_mut(),
            0,
        )
    };
    (asked == 0 && translated == 1).then_some("arm64")
}

/// Ask Windows what machine it is, which is the one question `IsWow64Process2` answers directly:
/// its second out-parameter is the native machine, whatever the process itself was built for. An
/// arch amenbo has no token for — and a call the OS turns away — is left to the caller's fallback.
#[cfg(windows)]
fn machine_arch() -> Option<&'static str> {
    use windows_sys::Win32::System::SystemInformation::{
        IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, IsWow64Process2};

    let mut process: IMAGE_FILE_MACHINE = 0;
    let mut native: IMAGE_FILE_MACHINE = 0;
    // SAFETY: a pseudo-handle to this process, and two out-parameters the call fills in; both are
    // live for the duration of it.
    let answered = unsafe {
        IsWow64Process2(GetCurrentProcess(), std::ptr::addr_of_mut!(process), std::ptr::addr_of_mut!(native))
    };
    if answered == 0 {
        return None;
    }
    match native {
        IMAGE_FILE_MACHINE_ARM64 => Some("arm64"),
        IMAGE_FILE_MACHINE_AMD64 => Some("x64"),
        _ => None,
    }
}

/// Nowhere else has an ask. Linux runs a foreign binary through a user-mode emulator that answers
/// as the emulated machine from the kernel down, so there is no question whose answer would differ
/// from the build's own arch.
#[cfg(not(any(target_os = "macos", windows)))]
fn machine_arch() -> Option<&'static str> {
    None
}

/// The wharfy `assets` key (`os-arch-kind`) of the **unified installer** for the current OS. The
/// kind follows from the OS: macOS → `pkg`, Windows → `exe`, Linux → `appimage` — the per-user
/// AppImage is the whole of the Linux GUI distribution (`AMB-D-428`). This composite key is
/// what picks the installer rather than the CLI archive (the suffix-less `os-arch` tar.gz/zip);
/// every OS ships one per architecture it is published for (`AMB-D-550`), so the arch rides in the
/// key. Where `assets` carries no installer under this key,
/// [`LatestRelease::installer_for_current_platform`] yields `None`, falling back to the
/// release-notes page.
fn installer_asset_key() -> String {
    let kind = match std::env::consts::OS {
        "macos" => "pkg",
        "windows" => "exe",
        // linux (any other OS tries the appimage key too, but an absent key falls back to None).
        _ => "appimage",
    };
    format!("{}-{kind}", current_platform_key())
}

/// The update-download name of a distribution asset: the same bytes, published a second time under
/// the first-install name plus `-update`. GitHub reports one download count per asset, so one asset
/// serving both audiences reports a sum, and a sum cannot be split back into its parts
/// (`AMB-D-426`); every path that fetches or opens an asset **because there is an update** takes this
/// side of it (`AMB-D-441`).
///
/// The suffix lands ahead of the whole extension, which is matched from a list rather than found by
/// hunting for a `.`: `.tar.gz` is one extension, and a file name carries dots of its own (the
/// version, in `amenbo_2.0.1_linux_amd64.tar.gz`), so neither the first `.` nor the last one finds
/// the right place. The list is every shape the release publishes an update copy of — the two CLI
/// archives and the three installers. Anything else takes the suffix at its end.
pub(crate) fn update_named(url: &str) -> String {
    for ext in [".tar.gz", ".zip", ".AppImage", ".exe", ".pkg"] {
        if let Some(stem) = url.strip_suffix(ext) {
            return format!("{stem}-update{ext}");
        }
    }
    format!("{url}-update")
}

/// Resolve the update URL that an explicit user action (`amenbo update`, or the GUI's "open the
/// installer") should open. We try regardless of the config toggle — the user asked for it, so we go
/// and fetch — but the env kill switch (`AMENBO_UPDATE_CHECK=0`), and a failed query, both fall back
/// to the latest-release page ([`LATEST_RELEASE_PAGE`]).
#[must_use]
pub fn resolve_update_url() -> String {
    check(true).map(|r| r.update_url()).unwrap_or_else(|| LATEST_RELEASE_PAGE.to_string())
}

/// The on-disk cache envelope: when we fetched, and what we got. It carries the fetch time so the
/// TTL can be judged.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct CacheEnvelope {
    /// UNIX time of the fetch, in seconds.
    fetched_at: i64,
    /// The latest release as it was fetched then.
    release: LatestRelease,
}

/// Whether the query is **disabled**: the build is on the development channel, or the config is off
/// (`enabled = false`), or the `AMENBO_UPDATE_CHECK` env var asks for it to be off. The env var wins
/// over the config — it is the hard kill switch — and the channel wins over both, being a fact of
/// the build rather than a preference: there is no answer a dev build could act on (see the module
/// header). Kept pure by taking all three as arguments; [`is_disabled`] below is what reads the
/// environment and the channel.
fn disabled(enabled: bool, env_off: bool, dev_channel: bool) -> bool {
    dev_channel || !enabled || env_off
}

/// Whether the query is disabled, effectively — the channel, the config toggle and the env override
/// combined.
pub fn is_disabled(enabled: bool) -> bool {
    disabled(
        enabled,
        crate::env::update_check_disabled(),
        crate::config::Paths::is_dev_channel(),
    )
}

/// Whether a cache entry is fresh: it is, while less than [`CACHE_TTL_SECS`] has passed since the
/// fetch. A fetch time in the future (a clock rolled back, say) counts as stale on the safe side,
/// prompting a re-fetch.
fn is_fresh(fetched_at: i64, now: i64) -> bool {
    let age = now - fetched_at;
    (0..CACHE_TTL_SECS).contains(&age)
}

/// The current UNIX time, in seconds.
fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

/// The URL to query: [`LATEST_JSON_URL`] in production, overridable through the
/// `AMENBO_UPDATE_JSON_URL` env var.
fn latest_json_url() -> String {
    crate::env::update_json_url().unwrap_or_else(|| LATEST_JSON_URL.to_string())
}

/// Path of the cache file, under the app-data cache dir — one per channel, so dev and prod never mix.
/// In the rare environment where `directories` cannot produce a cache dir, this is `None`: no cache,
/// so we query every time. We lose the TTL's benefit, but we still work.
fn cache_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("work", "amenbo", crate::config::Paths::APP_NAME)
        .map(|d| d.cache_dir().join("update_check.json"))
}

/// Read the cache. A corrupt file reads as `None`, which simply means we fetch again.
fn read_cache(path: &std::path::Path) -> Option<CacheEnvelope> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Write the cache. Failure is silent: the cache is only ever an optimization.
fn write_cache(path: &std::path::Path, env: &CacheEnvelope) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(env) {
        let _ = std::fs::write(path, bytes);
    }
}

/// Actually fetch `latest.json`, with a timeout; failure is `None`. This is the only place that
/// performs network I/O.
fn fetch(url: &str) -> Option<LatestRelease> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(FETCH_TIMEOUT))
        .build()
        .into();
    let body = agent.get(url).call().ok()?.body_mut().read_to_string().ok()?;
    serde_json::from_str(&body).ok()
}

/// Return the newest published release. **On by default; honours the config's disable knob; timed
/// out; silent on failure; caches its result.**
///
/// - `enabled` is [`crate::config::Config::update_check`]. Disabled — by that toggle, by the env
///   kill switch, or by being a development build ([`is_disabled`]) — means `None` immediately, with
///   no traffic.
/// - A fresh cache entry (within the TTL) is returned as-is, again with no traffic.
/// - If the entry is stale or absent, we query upstream exactly once, with a timeout, and update the
///   cache on success.
/// - If that query fails, a stale cache entry is still returned — offline, we keep the most recent
///   awareness we had — and `None` only if there is none.
///
/// So `None` means disabled, never fetched, or failed. **Callers may only use this as material for
/// what they display; they must not surface the failure** — an update check does not get in the way
/// of the real work.
pub fn check(enabled: bool) -> Option<LatestRelease> {
    check_inner(enabled, false)
}

/// [`check`], bypassing the cache: query upstream once **even if** a fresh entry exists. For the two
/// readings that must not answer from an entry up to a TTL old: the **first** tick after process
/// start, so that right after a release we do not sit behind a fresh cache entry for the old version
/// and fail to mention the new one, and a check somebody **typed or clicked** (`AMB-D-463`), where
/// what is being asked for is the state now. Every other reading — the ones that ride along with work
/// the user came to do — goes through [`check`], which is what keeps the traffic to the cache's terms.
///
/// The contract (honours the disable knob, times out, silent on failure, falls back to stale) is
/// identical to [`check`], and **a successful fetch updates the cache too**, so the entry that later ticks read
/// through [`check`] is swapped for the new version as well. A failed query falls back to the stale
/// entry, which is what makes it tolerate being offline.
pub fn check_fresh(enabled: bool) -> Option<LatestRelease> {
    check_inner(enabled, true)
}

/// The body behind [`check`] and [`check_fresh`]. When `force_fresh` is set, the early return on a
/// fresh cache entry is skipped and upstream is always queried — updating the cache on success, and
/// falling back to the stale entry on failure.
fn check_inner(enabled: bool, force_fresh: bool) -> Option<LatestRelease> {
    if is_disabled(enabled) {
        return None;
    }
    let now = now_unix();
    let path = cache_path();
    let cached = path.as_deref().and_then(read_cache);
    if !force_fresh {
        if let Some(env) = &cached {
            if is_fresh(env.fetched_at, now) {
                return Some(env.release.clone());
            }
        }
    }
    match fetch(&latest_json_url()) {
        Some(release) => {
            if let Some(path) = &path {
                write_cache(path, &CacheEnvelope { fetched_at: now, release: release.clone() });
            }
            Some(release)
        }
        // The query failed: return the most recent cache entry even if stale — silently, and so that
        // being offline still tells the user what we last knew.
        None => cached.map(|c| c.release),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A verbatim copy of the latest.json that wharfy's release step actually publishes (the
    /// `.wharfy/latest.json`), pinned here to **fix the contract**. Because the query is silent on
    /// failure by design, a schema change on wharfy's side — the key vocabulary, a field name —
    /// would break this consumer quietly; this sample is the anchor that makes CI catch it. It pins
    /// both parsing and the choice of installer for the current OS. The `.deb` / `.rpm` keys are on
    /// their way out (`AMB-D-428` retires them), and they are kept in the sample on purpose: while
    /// they are still listed, Linux must pick the AppImage over them.
    const WHARFY_LATEST_JSON: &str = r#"{
      "version": "2.2.0",
      "notes_url": "https://github.com/ShiroDoromoto/amenbo/releases/tag/v2.2.0",
      "assets": {
        "linux-arm64": "https://github.com/ShiroDoromoto/amenbo/releases/download/v2.2.0/amenbo_2.2.0_linux_arm64.tar.gz",
        "linux-x64": "https://github.com/ShiroDoromoto/amenbo/releases/download/v2.2.0/amenbo_2.2.0_linux_amd64.tar.gz",
        "linux-x64-appimage": "https://github.com/ShiroDoromoto/amenbo/releases/download/v2.2.0/amenbo-app-linux-x86_64.AppImage",
        "linux-x64-deb": "https://github.com/ShiroDoromoto/amenbo/releases/download/v2.2.0/amenbo-app-linux-amd64.deb",
        "linux-x64-rpm": "https://github.com/ShiroDoromoto/amenbo/releases/download/v2.2.0/amenbo-app-linux-x86_64.rpm",
        "macos-arm64": "https://github.com/ShiroDoromoto/amenbo/releases/download/v2.2.0/amenbo_2.2.0_darwin_arm64.tar.gz",
        "macos-arm64-pkg": "https://github.com/ShiroDoromoto/amenbo/releases/download/v2.2.0/amenbo-darwin-arm64.pkg",
        "macos-x64": "https://github.com/ShiroDoromoto/amenbo/releases/download/v2.2.0/amenbo_2.2.0_darwin_amd64.tar.gz",
        "macos-x64-pkg": "https://github.com/ShiroDoromoto/amenbo/releases/download/v2.2.0/amenbo-darwin-amd64.pkg",
        "windows-x64": "https://github.com/ShiroDoromoto/amenbo/releases/download/v2.2.0/amenbo_2.2.0_windows_amd64.zip",
        "windows-x64-exe": "https://github.com/ShiroDoromoto/amenbo/releases/download/v2.2.0/amenbo-app-windows-x64-setup.exe"
      }
    }"#;

    /// wharfy's real latest.json parses exactly, with the expected key vocabulary and notes_url.
    #[test]
    fn parses_wharfy_manifest() {
        let r: LatestRelease = serde_json::from_str(WHARFY_LATEST_JSON).unwrap();
        assert_eq!(r.version, "2.2.0");
        assert_eq!(
            r.notes_url.as_deref(),
            Some("https://github.com/ShiroDoromoto/amenbo/releases/tag/v2.2.0")
        );
        // Both the unified installers (kind suffix) and the CLI archives (no suffix) are listed.
        assert_eq!(
            r.assets.get("macos-arm64-pkg").map(String::as_str),
            Some("https://github.com/ShiroDoromoto/amenbo/releases/download/v2.2.0/amenbo-darwin-arm64.pkg")
        );
        assert_eq!(
            r.assets.get("windows-x64-exe").map(String::as_str),
            Some("https://github.com/ShiroDoromoto/amenbo/releases/download/v2.2.0/amenbo-app-windows-x64-setup.exe")
        );
        assert!(r.assets.contains_key("linux-x64-appimage"));
        assert!(r.assets.contains_key("macos-arm64"), "the CLI archive keys are listed too, with no suffix");
    }

    /// From wharfy's real sample, the running OS's **unified installer** is what gets picked — not
    /// the CLI archive.
    #[test]
    fn selects_current_os_installer_from_wharfy_manifest() {
        let r: LatestRelease = serde_json::from_str(WHARFY_LATEST_JSON).unwrap();
        // The installer asset for the running OS is chosen, and it is not a CLI archive
        // (.tar.gz/.zip).
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        assert_eq!(r.installer_for_current_platform(), r.assets.get("macos-arm64-pkg").map(String::as_str));
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        assert_eq!(r.installer_for_current_platform(), r.assets.get("windows-x64-exe").map(String::as_str));
        // Linux picks the AppImage even though the sample still lists the `.deb` / `.rpm` next to it.
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        assert_eq!(r.installer_for_current_platform(), r.assets.get("linux-x64-appimage").map(String::as_str));
        // Whatever gets picked, it never carries a CLI-archive extension: the contract is to pick an
        // installer.
        if let Some(url) = r.installer_for_current_platform() {
            assert!(!url.ends_with(".tar.gz") && !url.ends_with(".zip"), "an installer is what gets picked: {url}");
        }
    }

    /// With every optional field missing, a manifest still parses off `version` alone (forwards- and
    /// backwards-compatible).
    #[test]
    fn parses_minimal_manifest() {
        let r: LatestRelease = serde_json::from_str(r#"{"version":"9.9.9"}"#).unwrap();
        assert_eq!(r.version, "9.9.9");
        assert!(r.assets.is_empty());
        assert!(r.notes_url.is_none());
    }

    /// Unknown fields are ignored, so the publisher adding metadata later cannot break us.
    #[test]
    fn ignores_unknown_fields() {
        let r: LatestRelease = serde_json::from_str(r#"{"version":"1.0.0","future":"meta"}"#).unwrap();
        assert_eq!(r.version, "1.0.0");
    }

    /// The key for the current platform comes out in wharfy's `os-arch` shape.
    #[test]
    fn platform_key_shape() {
        let key = current_platform_key();
        let (os, arch) = key.split_once('-').expect("os-arch");
        assert!(["macos", "windows", "linux"].contains(&os) || !os.is_empty());
        assert!(["arm64", "x64"].contains(&arch) || !arch.is_empty());
        // The key matches the running OS, and the machine's own architecture. Nothing translates a
        // test run — a developer's checkout and a CI runner both build for the machine they are on
        // — so here the machine's arch is the build's, which is what makes this assertable at all.
        //
        // Which is also to say: aim a test run at the *other* arch on purpose (`cargo test --target
        // x86_64-apple-darwin` on Apple silicon) and the arch line below fails, because the key
        // comes back naming the machine rather than the build. That is the change working, not a
        // regression — it is the shortest way to see the ask answer at all.
        #[cfg(target_os = "macos")]
        assert!(key.starts_with("macos-"));
        #[cfg(target_arch = "aarch64")]
        assert!(key.ends_with("-arm64"));
        #[cfg(target_arch = "x86_64")]
        assert!(key.ends_with("-x64"));
        assert!(key.ends_with(&format!("-{}", native_arch())));
    }

    /// The arches amenbo distributes are the two it has tokens for, and a machine answering as one
    /// of them is what every asset key rests on.
    #[test]
    fn native_arch_is_one_amenbo_distributes() {
        assert!(
            ["arm64", "x64"].contains(&native_arch()),
            "the machine answered `{}`, which keys nothing amenbo publishes",
            native_arch()
        );
    }

    /// `update_url` falls back in order: the current OS's installer, then notes_url, then the
    /// release page.
    #[test]
    fn update_url_prefers_current_installer_then_falls_back() {
        let installer_key = installer_asset_key();
        // An installer listed for the current platform is what we return — under its update name,
        // while the manifest's own listing stays the first-install one.
        let mut assets = std::collections::BTreeMap::new();
        assets.insert(installer_key.clone(), "https://example/installer-for-me.pkg".to_string());
        let r = LatestRelease {
            version: "1.0.0".into(),
            notes_url: Some("https://example/releases".into()),
            assets,
        };
        assert_eq!(r.update_url(), "https://example/installer-for-me-update.pkg");
        assert_eq!(r.installer_for_current_platform(), Some("https://example/installer-for-me.pkg"));

        // With no installer for the current platform we go to notes_url — a CLI archive key alone is
        // never picked.
        let mut assets = std::collections::BTreeMap::new();
        assets.insert(current_platform_key(), "https://example/cli-archive.tar.gz".to_string());
        let r = LatestRelease {
            version: "1.0.0".into(),
            notes_url: Some("https://example/releases".into()),
            assets,
        };
        assert_eq!(r.update_url(), "https://example/releases");
        assert!(r.installer_for_current_platform().is_none(), "a CLI archive is not an installer");

        // With no notes_url either, we go to the latest-release page.
        let r = LatestRelease {
            version: "1.0.0".into(),
            notes_url: None,
            assets: Default::default(),
        };
        assert_eq!(r.update_url(), LATEST_RELEASE_PAGE);
    }

    /// The update name of every asset the release publishes a second copy of, pinned as a table
    /// against the names `release.yml` actually copies to. The suffix lands ahead of the whole
    /// extension, dots inside the file name notwithstanding.
    #[test]
    fn update_name_goes_before_the_extension() {
        const BASE: &str = "https://github.com/ShiroDoromoto/amenbo/releases/download/v2.2.0";
        for (published, update) in [
            // The three unified installers, one per OS.
            ("amenbo-darwin-arm64.pkg", "amenbo-darwin-arm64-update.pkg"),
            ("amenbo-app-windows-x64-setup.exe", "amenbo-app-windows-x64-setup-update.exe"),
            ("amenbo-app-linux-x86_64.AppImage", "amenbo-app-linux-x86_64-update.AppImage"),
            // The CLI archives, whose file name carries the version's own dots.
            ("amenbo_2.2.0_linux_amd64.tar.gz", "amenbo_2.2.0_linux_amd64-update.tar.gz"),
            ("amenbo_2.2.0_windows_amd64.zip", "amenbo_2.2.0_windows_amd64-update.zip"),
        ] {
            assert_eq!(update_named(&format!("{BASE}/{published}")), format!("{BASE}/{update}"));
        }
        // A URL in none of those shapes takes the suffix at its end.
        assert_eq!(update_named("https://example.com/v1.2.3/amenbo"), "https://example.com/v1.2.3/amenbo-update");
    }

    /// Over wharfy's real manifest, the running OS's affordance opens the update copy — whichever OS
    /// runs the test, and never the name a first install fetches.
    #[test]
    fn update_url_opens_the_update_copy_on_every_os() {
        let r: LatestRelease = serde_json::from_str(WHARFY_LATEST_JSON).unwrap();
        let Some(published) = r.installer_for_current_platform() else {
            return; // no installer for this OS/arch: the fallbacks are pages, covered elsewhere
        };
        let opened = r.update_url();
        assert_ne!(opened, published, "the first-install name is not what an update opens");
        assert!(opened.contains("-update."), "the update copy is what gets opened: {opened}");
    }

    #[test]
    fn disabled_by_channel_config_or_env() {
        assert!(disabled(false, false, false), "config off means disabled");
        assert!(disabled(true, true, false), "the env override wins over config, so disabled");
        assert!(disabled(false, true, false), "both off, disabled");
        assert!(!disabled(true, false, false), "config on with no env override means enabled");
        // The channel is a fact of the build, so it outranks every preference: a dev build is
        // disabled however the toggle and the env var are set.
        assert!(disabled(true, false, true), "a dev build is disabled with everything else on");
        assert!(disabled(false, true, true), "a dev build stays disabled when the rest is off too");
    }

    #[test]
    fn freshness_boundaries() {
        let base = 1_000_000;
        assert!(is_fresh(base, base), "the same instant is fresh");
        assert!(is_fresh(base, base + CACHE_TTL_SECS - 1), "just short of the TTL is fresh");
        assert!(!is_fresh(base, base + CACHE_TTL_SECS), "exactly at the TTL is stale");
        assert!(!is_fresh(base, base + CACHE_TTL_SECS + 100), "past the TTL is stale");
        assert!(!is_fresh(base, base - 10), "a fetch in the future (clock rolled back) is stale, erring on the safe side");
    }

    /// The cache round-trips: what we write is what we read back.
    #[test]
    fn cache_round_trip() {
        let dir = amenbo_scratch::scratch("update-test");
        let path = dir.join("update_check.json");
        let env = CacheEnvelope {
            fetched_at: 42,
            release: LatestRelease {
                version: "1.2.3".into(),
                notes_url: None,
                assets: Default::default(),
            },
        };
        write_cache(&path, &env);
        let back = read_cache(&path).expect("cache reads back");
        assert_eq!(back.fetched_at, 42);
        assert_eq!(back.release.version, "1.2.3");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
