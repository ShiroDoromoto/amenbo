//! Querying the update endpoint.
//!
//! Amenbo keeps its **functional traffic at zero**: local-first, no central server, no user data
//! ever leaving the machine. That core is untouched; what this module owns is the one piece of
//! **infrastructure traffic** — noticing that a newer release exists. It queries Amenbo's own
//! update endpoint, which answers with the manifest wharfy's release step publishes as
//! `latest.json`: a version plus minimal metadata, carrying no user data.
//!
//! The policy: **on by default**, with a config knob to disable it
//! ([`crate::config::Config::update_check`]); **timed out**; **silent on failure** (a failed update
//! check must never get in the way of the real work); and **cached**, so we do not talk to the
//! network on every command. This module returns only the awareness; **acting** on it — downloading
//! and swapping the binary in place — is [`crate::self_update`]'s job (the standalone CLI), which
//! reuses the [`LatestRelease`] fetched here.
//!
//! Two kinds of build are outside all of that, and both for the same reason — the manifest names a
//! version they cannot be measured against, so every answer this module could hand them is either an
//! offer to install production over themselves or a claim to be current that they cannot make
//! ([`is_disabled`]). A **development build never queries at all**: its version is normally *behind*
//! what production's manifest names. Neither does a build the release workflow did not stamp
//! ([`crate::build_stamp::is_release_build`]) — a working tree wears the released number before the
//! release that publishes it, so it is the same mismatch in the other direction. That second arm is
//! what keeps the test suites, `make verify` and any local build off the production endpoint by
//! construction, rather than by every spawn remembering to set `AMENBO_UPDATE_CHECK=0`: a forgotten
//! one now falls to *not asking*. An unstamped build that was pointed somewhere else to ask
//! (`AMENBO_UPDATE_JSON_URL`) is not withheld — that override is how a test drives the query against
//! a manifest of its own, and it never names the production endpoint.
//!
//! The module does nothing but query and fetch. Comparing the fetched version against the running
//! binary to derive `update_available` is the caller's job (it is folded into
//! [`crate::store::VersionStatus`]).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// The endpoint this build queries: Amenbo's own update service, answering with the manifest the
/// release step publishes as `latest.json`. Asking here rather than at the release itself is what
/// keeps where a version is announced apart from where the release is hosted (`AMB-D-717`).
///
/// The address is **injected at build time**, from `AMENBO_LATEST_JSON_URL`, and is nowhere in this
/// source — writing it here publishes the production endpoint with every clone of a public
/// repository. There is no default to fall back to: an unset variable stops the compile, because a
/// default is what turns a forgotten variable into a wrong answer nobody sees, and a default that
/// *is* production puts that answer straight into a shipped binary (`AMB-D-849`). An empty value
/// stops it too — that is what an unset repository variable hands a workflow, so it is the same
/// forgetting wearing a different shape.
///
/// The query string is part of the injected value on purpose. One string is the whole address, so a
/// change of server — or a move back to a file hanging off a release — is a change of what the build
/// is handed and nothing else, rather than a base and its parameters kept in step by hand.
///
/// A shipped binary carries the address it was built with baked in, so moving the endpoint strands
/// every existing install on the old one until it updates once — which is why the release's own
/// `latest.json` goes on being published.
pub const LATEST_JSON_URL: &str = match option_env!("AMENBO_LATEST_JSON_URL") {
    Some(url) if !url.is_empty() => url,
    _ => panic!(
        "AMENBO_LATEST_JSON_URL must be set, and non-empty, when this crate is compiled: the \
         update endpoint is injected at build time and has no default. Build through the Makefile, \
         which passes it, or set it yourself."
    ),
};

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
/// with Apple silicon answers `macos-arm64` whether the Amenbo asking is the arm64 build or the
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
/// use: `aarch64` → `arm64`, `x86_64` → `x64`, anything else verbatim (an arch Amenbo does not
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
/// arch Amenbo has no token for — and a call the OS turns away — is left to the caller's fallback.
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

/// Whether the query is **disabled**: the build is on the development channel, or it carries no
/// release stamp and was pointed at no manifest of its own, or the config is off (`enabled = false`),
/// or the `AMENBO_UPDATE_CHECK` env var asks for it to be off. The env var wins over the config — it
/// is the hard kill switch — and the two facts of the build win over both, being facts rather than
/// preferences: there is no answer either build could act on (see the module header). Kept pure by
/// taking all five as arguments; [`is_disabled`] below is what reads the environment, the channel and
/// the stamp.
fn disabled(
    enabled: bool,
    env_off: bool,
    dev_channel: bool,
    release_build: bool,
    url_overridden: bool,
) -> bool {
    dev_channel || withheld_from_build(release_build, url_overridden) || !enabled || env_off
}

/// Whether the query is withheld because the binary is not a release artifact: no stamp, and no
/// `AMENBO_UPDATE_JSON_URL` naming somewhere else to ask. The override is what lets a test point the
/// query at its own manifest — the production endpoint is only ever reached by a build that shipped.
fn withheld_from_build(release_build: bool, url_overridden: bool) -> bool {
    !release_build && !url_overridden
}

/// Whether the query is disabled, effectively — the channel, the release stamp, the config toggle and
/// the env override combined.
pub fn is_disabled(enabled: bool) -> bool {
    disabled(
        enabled,
        crate::env::update_check_disabled(),
        crate::config::Paths::is_dev_channel(),
        crate::build_stamp::is_release_build(),
        crate::env::update_json_url().is_some(),
    )
}

/// Whether *this* build is the one the query is withheld from for want of a release stamp
/// ([`withheld_from_build`]). The CLI asks so it can word the refusal — a local build that says
/// "no newer version detected" is claiming something it never went and looked at (`AMB-D-378`
/// names the same stamp, for migrations).
#[must_use]
pub fn is_withheld_from_build() -> bool {
    withheld_from_build(
        crate::build_stamp::is_release_build(),
        crate::env::update_json_url().is_some(),
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
///   kill switch, by being a development build, or by carrying no release stamp ([`is_disabled`]) —
///   means `None` immediately, with no traffic.
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

/// [`check`], bypassing the cache: query upstream once **even if** a fresh entry exists. For the one
/// reading that must not answer from an entry up to a TTL old: a check somebody **typed or clicked**
/// (`AMB-D-463`) — `amenbo update`, and the app menu's "check for updates" — where what is being
/// asked for is the state now. Every reading nobody asked for goes through [`check`], process start
/// included (`AMB-D-710`), which is what keeps the traffic to the cache's terms.
///
/// The contract (honours the disable knob, times out, silent on failure, falls back to stale) is
/// identical to [`check`], and **a successful fetch updates the cache too**, so the entry that later ticks read
/// through [`check`] is swapped for the new version as well. A failed query falls back to the stale
/// entry, which is what makes it tolerate being offline.
pub fn check_fresh(enabled: bool) -> Option<LatestRelease> {
    check_inner(enabled, true)
}

/// The body behind [`check`] and [`check_fresh`]: the disable knob, and then the read itself against
/// this machine's endpoint, cache file and clock.
fn check_inner(enabled: bool, force_fresh: bool) -> Option<LatestRelease> {
    if is_disabled(enabled) {
        return None;
    }
    check_at(&latest_json_url(), cache_path().as_deref(), now_unix(), force_fresh)
}

/// The read, against a named URL, cache file and instant — the seam the tests drive, so nothing in
/// this module's test run reaches the real manifest, and the TTL can be crossed without waiting a day
/// (the shape [`crate::plugin_github::facts`] and [`crate::plugin_catalog::load`] take for the same
/// reason).
///
/// When `force_fresh` is set, the early return on a fresh cache entry is skipped and upstream is
/// always queried — updating the cache on success, and falling back to the stale entry on failure.
///
/// `cache_file` is `None` where the machine has no cache directory to offer ([`cache_path`]): the
/// query then happens every time and nothing is written, which costs the TTL's benefit and no
/// function.
fn check_at(
    url: &str,
    cache_file: Option<&std::path::Path>,
    now: i64,
    force_fresh: bool,
) -> Option<LatestRelease> {
    let cached = cache_file.and_then(read_cache);
    if !force_fresh {
        if let Some(env) = &cached {
            if is_fresh(env.fetched_at, now) {
                return Some(env.release.clone());
            }
        }
    }
    match fetch(url) {
        Some(release) => {
            if let Some(path) = cache_file {
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

    /// The arches Amenbo distributes are the two it has tokens for, and a machine answering as one
    /// of them is what every asset key rests on.
    #[test]
    fn native_arch_is_one_amenbo_distributes() {
        assert!(
            ["arm64", "x64"].contains(&native_arch()),
            "the machine answered `{}`, which keys nothing Amenbo publishes",
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
    /// against the names `_release.yml` actually copies to. The suffix lands ahead of the whole
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

    /// A stamped production build, which is where the preferences are the only thing deciding.
    const SHIPPED: bool = true;
    /// No `AMENBO_UPDATE_JSON_URL`: the query would go to the production endpoint.
    const NO_URL: bool = false;

    #[test]
    fn disabled_by_channel_config_or_env() {
        assert!(disabled(false, false, false, SHIPPED, NO_URL), "config off means disabled");
        assert!(disabled(true, true, false, SHIPPED, NO_URL), "the env override wins over config, so disabled");
        assert!(disabled(false, true, false, SHIPPED, NO_URL), "both off, disabled");
        assert!(!disabled(true, false, false, SHIPPED, NO_URL), "config on with no env override means enabled");
        // The channel is a fact of the build, so it outranks every preference: a dev build is
        // disabled however the toggle and the env var are set.
        assert!(disabled(true, false, true, SHIPPED, NO_URL), "a dev build is disabled with everything else on");
        assert!(disabled(false, true, true, SHIPPED, NO_URL), "a dev build stays disabled when the rest is off too");
    }

    /// The stamp decides the same way the channel does, and the override is the one way past it: the
    /// production endpoint is reached by a shipped build and by nothing else, so a spawn that forgets
    /// `AMENBO_UPDATE_CHECK=0` falls to not asking rather than to asking production.
    #[test]
    fn only_a_stamped_build_reaches_the_production_endpoint() {
        assert!(!disabled(true, false, false, true, NO_URL), "a shipped build asks the default endpoint");
        assert!(disabled(true, false, false, false, NO_URL), "an unstamped build asks nothing");
        assert!(!disabled(true, false, false, false, true), "pointed elsewhere, it asks that instead");
        // The preferences still hold over the override: it names where to ask, not whether to.
        assert!(disabled(false, false, false, false, true), "config off still means disabled");
        assert!(disabled(true, true, false, false, true), "the env kill switch still wins");
        // And the channel outranks the override too — a dev build has no manifest to be measured
        // against whatever it is pointed at.
        assert!(disabled(true, false, true, false, true), "a dev build stays disabled when pointed elsewhere");
    }

    /// The predicate the CLI words its refusal from is the stamp arm alone — not the channel, not the
    /// preferences.
    #[test]
    fn the_withheld_predicate_is_the_stamp_arm_alone() {
        assert!(withheld_from_build(false, false), "unstamped and unpointed is what is withheld");
        assert!(!withheld_from_build(true, false), "a shipped build is not");
        assert!(!withheld_from_build(false, true), "nor is one pointed at its own manifest");
        // The test binary is itself unstamped, so the live reading agrees with the rule.
        assert!(!crate::build_stamp::is_release_build(), "a test binary is never a release artifact");
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

    // ---- the read itself: the window, the check somebody typed, and being offline ----

    /// An address nothing answers on — what being offline looks like from inside the query.
    const UNREACHABLE: &str = "http://127.0.0.1:1/latest.json";

    /// A fixed instant to hang the tests off, so the TTL is crossed by arithmetic rather than by
    /// waiting a day.
    const NOW: i64 = 1_700_000_000;

    /// A cache file of this test's own, never the machine's ([`cache_path`]).
    fn cache_at(tag: &str) -> PathBuf {
        amenbo_scratch::scratch(&format!("update-check-{tag}")).join("update_check.json")
    }

    /// The smallest manifest that parses: a version, and the optional metadata left out.
    fn manifest(version: &str) -> String {
        format!(r#"{{"version": "{version}"}}"#)
    }

    /// An entry naming `version`, written `fetched_at`.
    fn entry(path: &std::path::Path, version: &str, fetched_at: i64) {
        let release =
            LatestRelease { version: version.into(), notes_url: None, assets: Default::default() };
        write_cache(path, &CacheEnvelope { fetched_at, release });
    }

    /// Inside the TTL the entry answers and **no request is made at all** — which is what keeps the
    /// readings that ride along with real work off the network (`AMB-D-520`). The proof is the address:
    /// nothing answers on it, so a query would have come back with nothing.
    #[test]
    fn an_entry_inside_the_ttl_answers_without_the_network() {
        let path = cache_at("inside-the-ttl");
        entry(&path, "1.0.0", NOW - 60);

        let got = check_at(UNREACHABLE, Some(&path), NOW, false).expect("the entry answers");
        assert_eq!(got.version, "1.0.0");
    }

    /// **A check somebody typed or clicked goes past a fresh entry** (`AMB-D-463`): what is being asked
    /// for is the state now, and answering it out of a copy up to a day old is declining the question.
    /// The answer replaces the entry, so the readings that ride along afterwards see the new version too.
    #[test]
    fn a_check_somebody_asked_for_goes_past_a_fresh_entry() {
        let path = cache_at("asked-for");
        entry(&path, "1.0.0", NOW - 60);
        let host = amenbo_static_host::StaticHost::serve([("/latest.json", manifest("2.0.0"))]);
        let url = host.url("/latest.json");

        let riding_along = check_at(&url, Some(&path), NOW, false).expect("the entry answers");
        assert_eq!(riding_along.version, "1.0.0", "a reading that came along with something else");

        let asked_for = check_at(&url, Some(&path), NOW, true).expect("upstream answers");
        assert_eq!(asked_for.version, "2.0.0", "and one somebody actually asked for");
        assert_eq!(read_cache(&path).unwrap().release.version, "2.0.0", "which is the entry now");
    }

    /// Past the TTL the query happens, and what comes back is written down — so the next reading inside
    /// the window is answered from it rather than from upstream again.
    #[test]
    fn a_stale_entry_is_replaced_by_what_the_query_brings_back() {
        let path = cache_at("stale-then-queried");
        entry(&path, "1.0.0", NOW - CACHE_TTL_SECS - 1);
        let host = amenbo_static_host::StaticHost::serve([("/latest.json", manifest("2.0.0"))]);

        let got = check_at(&host.url("/latest.json"), Some(&path), NOW, false).expect("upstream");
        assert_eq!(got.version, "2.0.0", "the window had closed, so the query happened");
        assert_eq!(
            check_at(UNREACHABLE, Some(&path), NOW, false).unwrap().version,
            "2.0.0",
            "and the fresh entry answers the next reading with no request",
        );
    }

    /// A query that fails answers with the entry it has, however old: being offline costs freshness,
    /// never the awareness we last had. It holds for the check somebody asked for as well — crossing the
    /// TTL is asking upstream, not requiring it — and with nothing on either side there is simply no
    /// answer to give.
    #[test]
    fn a_failed_query_falls_back_to_the_entry_it_has() {
        let path = cache_at("offline");
        entry(&path, "1.0.0", NOW - CACHE_TTL_SECS - 1);

        assert_eq!(check_at(UNREACHABLE, Some(&path), NOW, false).unwrap().version, "1.0.0");
        assert_eq!(check_at(UNREACHABLE, Some(&path), NOW, true).unwrap().version, "1.0.0");
        assert_eq!(read_cache(&path).unwrap().release.version, "1.0.0", "and never cleared it");

        assert!(
            check_at(UNREACHABLE, Some(&cache_at("nothing-either")), NOW, false).is_none(),
            "nothing fetched and nothing cached is the one reading with no answer",
        );
    }

    /// With nowhere to write — a machine whose cache directory cannot be worked out ([`cache_path`]) —
    /// every reading queries and nothing is kept. What is lost is the window, not the answer.
    #[test]
    fn with_nowhere_to_cache_the_query_still_answers() {
        let host = amenbo_static_host::StaticHost::serve([("/latest.json", manifest("3.0.0"))]);

        let got = check_at(&host.url("/latest.json"), None, NOW, false).expect("upstream answers");
        assert_eq!(got.version, "3.0.0");
    }
}
