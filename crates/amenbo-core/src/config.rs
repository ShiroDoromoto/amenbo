//! Configuration and file-path resolution.
//!
//! - If the `AMENBO_HOME` environment variable is set, everything lives under that directory
//!   (for tests and explicit overrides).
//! - Otherwise the OS-standard data/config directory (`directories`) is used.
//!
//! Config lives in its own file next to the store (`config.json`). The identity lives in
//! `identity.json` directly under the base dir and **holds no secrets**. The `accounts/P0/` layout
//! written by older builds is lifted to the base dir once, on open ([`lift_legacy_identity`]).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::model::View;

/// On-disk name of the identity file. It has exactly one home: directly under the base dir
/// ([`Paths::identity_file`]).
pub const IDENTITY_FILE_NAME: &str = "identity.json";

/// On-disk name of the source-of-truth store file. Every read and write goes through this name.
pub const STORE_FILE_NAME: &str = "store.sqlite";

/// The old store file name (`oplog.sqlite`). A store under this name predates consolidation, so
/// renaming it would not make its contents readable — **open does not rename it; it detects the
/// name and refuses by name**. This constant serves that detection and the paths that need to
/// locate a store at all ([`resolve_store_file`], backup, `doctor`).
pub const LEGACY_STORE_FILE_NAME: &str = "oplog.sqlite";

/// Resolve the source-of-truth store file inside `dir`: the current name [`STORE_FILE_NAME`] if it
/// exists, else the legacy `oplog.sqlite` if only that exists, else the current name (the store
/// genesis is about to create). This only tests for existence — it never renames (no side effects).
/// Picking up the legacy name is about not losing sight of the fact that a store *is* there (backup
/// must be able to grab a store it cannot open); it does not imply open can open it.
pub fn resolve_store_file(dir: &std::path::Path) -> PathBuf {
    let current = dir.join(STORE_FILE_NAME);
    if current.exists() {
        return current;
    }
    let legacy = dir.join(LEGACY_STORE_FILE_NAME);
    if legacy.exists() {
        return legacy;
    }
    current
}

/// **Lift the identity out of the old vault layout** (`<base>/accounts/P0/identity.json`,
/// `<base>/personas/P0/identity.json`) **into the base dir, once.** Rather than growing another
/// read fallback, this **normalizes the identity down to a single location** — a fallback would
/// keep the old layout alive forever and block ever cleaning up the corpses beside it. The lift is
/// one-way and idempotent (a no-op once the file sits under base), so afterwards every path looks
/// only at the base dir. The old directory itself (`accounts/`) is left alone: this function is
/// responsible for not losing the identity, and nothing more — open does not get to delete files it
/// happens to find next door.
pub(crate) fn lift_legacy_identity(base: &std::path::Path) -> Result<()> {
    let flat = base.join(IDENTITY_FILE_NAME);
    if flat.exists() {
        return Ok(());
    }
    for legacy_vault in ["accounts", "personas"] {
        let legacy = base.join(legacy_vault).join("P0").join(IDENTITY_FILE_NAME);
        if legacy.exists() {
            std::fs::rename(&legacy, &flat)?;
            return Ok(());
        }
    }
    Ok(())
}

/// Where the data and config files live. The store is a single SQLite file
/// (`<app-data>/store.sqlite`); identity, config and the store itself all sit flat under the same
/// base (`base_dir`). **The data itself never lives in a project directory**, which may be synced
/// behind our back — all a project holds is the `.amenbo` pointer. The base is the OS's per-app
/// area (on macOS, Application Support, which iCloud does not sync) or the directory named
/// explicitly by `AMENBO_HOME`.
#[derive(Clone, Debug)]
pub struct Paths {
    /// The source-of-truth engine (SQLite) file (`store.sqlite`; during the transition it may still
    /// point at the legacy `oplog.sqlite`). It doubles as the "does this store exist?" indicator —
    /// [`resolve_store_file`] points it at whichever is actually there.
    pub store_file: PathBuf,
    /// User config (holds no secrets).
    pub config_file: PathBuf,
    /// Identity: display name plus the machine signal used to detect clones. **Holds no secrets.**
    pub identity_file: PathBuf,
    /// The base directory holding identity, config and the store itself (app-data, or `AMENBO_HOME`).
    pub base_dir: PathBuf,
    /// The system-event ledger behind activity ([`crate::activity_log`]). It sits outside the source
    /// of truth, one per machine, and `backup` / `export` do not copy it — it is a machine-local
    /// viewing stream.
    pub activity_file: PathBuf,
}

impl Paths {
    /// Paths with identity, config and the store laid out flat under `base`. The identity has
    /// exactly one home, directly under base — anything left in the old vault layout
    /// (`accounts/P0/`) is lifted by open ([`lift_legacy_identity`]), so there is no branch here.
    pub fn at(base: PathBuf) -> Paths {
        Paths {
            store_file: resolve_store_file(&base),
            config_file: base.join("config.json"),
            identity_file: base.join(IDENTITY_FILE_NAME),
            activity_file: base.join(crate::activity_log::FILE_NAME),
            base_dir: base,
        }
    }

    /// The app-data "app name". Substitutable **at build time** via `AMENBO_APP_NAME`, which is how
    /// dev and prod are kept apart. The default `amenbo` is production
    /// (`~/Library/Application Support/work.amenbo.amenbo`); a dev build sets
    /// `AMENBO_APP_NAME=amenbo-dev` and gets its own directory (`…/work.amenbo.amenbo-dev`), so its
    /// identity and store never collide with production data.
    pub const APP_NAME: &'static str = match option_env!("AMENBO_APP_NAME") {
        Some(name) => name,
        None => "amenbo",
    };

    /// The base for identity and config: `AMENBO_HOME` if set, otherwise the OS data directory.
    /// `pub(crate)` so that store discovery and vault resolution follow the environment to the same
    /// base.
    pub(crate) fn user_base() -> PathBuf {
        if let Some(home) = crate::env::home() {
            return PathBuf::from(home);
        }
        match directories::ProjectDirs::from("work", "amenbo", Paths::APP_NAME) {
            Some(d) => d.data_dir().to_path_buf(),
            // Not even a home directory available: fall back to somewhere under the CWD.
            None => PathBuf::from(".amenbo"),
        }
    }

    /// Resolve the paths for this environment. There is a single store, so there is no branching:
    /// this points at the one `store.sqlite` directly under `AMENBO_HOME` (if set) or the OS
    /// app-data base. (`.amenbo` names a project by `project_id`; it takes no part in choosing the
    /// store.)
    pub fn resolve() -> Result<Paths> {
        Ok(Paths::at(Paths::user_base()))
    }

    /// Absolute path of the app-data root, i.e. where the single store lives. This is the
    /// OS-independent entry point for "where this machine's real data sits", used by the GUI's
    /// Settings > Data screen to show the actual path instead of a hard-coded literal. When
    /// `AMENBO_HOME` is set, that directory is returned.
    pub fn data_root() -> PathBuf {
        Paths::user_base()
    }

    /// The directory name, under the base, holding installed plugins and the registry cache.
    pub const PLUGINS_DIR_NAME: &'static str = "plugins";
    /// The name reserved, under [`plugins_dir`](Self::plugins_dir), for the registry cache — it sits
    /// beside the plugins, so no plugin may claim it (see [`is_reserved_plugin_name`]).
    pub const REGISTRY_DIR_NAME: &'static str = "registry";

    /// Where installed plugins and the registry cache live: `<base>/plugins/`, machine-global under the
    /// base (`AMB-D-350`). A plugin's executable is OS/arch-specific and would collide with
    /// distribution, PII and `.gitignore` if it lived in a project directory, so it never does — and
    /// never in `.amenbo`, the store-agnostic, sync-safe pointer. Enablement is *not* on disk here: the
    /// machine default is a `config.json` field and the per-project override is a store table.
    pub fn plugins_dir(&self) -> PathBuf {
        self.base_dir.join(Self::PLUGINS_DIR_NAME)
    }

    /// One plugin's home, `<base>/plugins/<name>/`, holding its executable and files — what lives in it
    /// is [`plugin_installed`](crate::plugin_installed)'s. `name` is the plugin's manifest name;
    /// [`REGISTRY_DIR_NAME`](Self::REGISTRY_DIR_NAME) is reserved and cannot be one — the manifest
    /// validator rejects it up front, but do not hand this an unvalidated name.
    pub fn plugin_dir(&self, name: &str) -> PathBuf {
        self.plugins_dir().join(name)
    }

    /// The manifest-registry cache, `<base>/plugins/registry/`: the fetched copy of the plugin catalog.
    /// There is no central server (local-first); the catalog is a set of git-hosted manifests pulled
    /// into this directory (`AMB-D-350`).
    pub fn registry_dir(&self) -> PathBuf {
        self.plugins_dir().join(Self::REGISTRY_DIR_NAME)
    }

    /// On-disk name of the plugin secret file (see [`plugin_secrets_file`](Self::plugin_secrets_file)).
    pub const PLUGIN_SECRETS_FILE_NAME: &'static str = "plugin-secrets.json";

    /// The **plugin secret file**, `<base>/plugin-secrets.json`: the user-area home a `secret` plugin
    /// config field is stored in (`AMB-D-356`). It sits flat under the base beside `config.json` — the
    /// same home as amenbo's own identity — but unlike the store it is **outside the source of truth and
    /// outside every backup/export**: `backup` snapshots `store.sqlite` and `export` walks the record
    /// tables, and this file is neither. Written owner-only (0600) by [`crate::plugin_secret`], so amenbo
    /// holds the secret centrally and injects it at run time rather than the store ever seeing it.
    pub fn plugin_secrets_file(&self) -> PathBuf {
        self.base_dir.join(Self::PLUGIN_SECRETS_FILE_NAME)
    }
}

/// Whether `name` is reserved by the plugin disk layout and so cannot name a plugin: the registry cache
/// shares the `plugins/` directory with the installed plugins (`AMB-D-350`), so a plugin called
/// `registry` would clash with it. The manifest validator calls this so the one truth about the layout's
/// reserved names lives beside the layout.
pub fn is_reserved_plugin_name(name: &str) -> bool {
    name == Paths::REGISTRY_DIR_NAME
}

/// Logging level for the perf instrumentation. One of three values, persisted as `perf_log` in
/// `config.json`. Unset (`None`) defers to the channel/build default — see
/// [`crate::perf::resolve_directive`], which resolves env > config > channel > build.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PerfLog {
    /// Emit nothing (not even a file).
    Off,
    /// Emit only the WARNs for a blown budget (command > 50ms, or a complexity ratio where scanned
    /// ≫ returned).
    BudgetOnly,
    /// Emit every perf event, DEBUG included.
    Verbose,
}

impl PerfLog {
    /// The `EnvFilter` directive for this level (a level cut against the perf target).
    pub fn directive(self) -> &'static str {
        match self {
            PerfLog::Off => "off",
            PerfLog::BudgetOnly => "perf=warn",
            PerfLog::Verbose => "perf=debug",
        }
    }

    /// The string form `config set perf_log <v>` accepts (`off` / `budget-only` / `verbose`).
    /// Exposed in the GUI snapshot to gate the front-end instrumentation by level. Distinct from
    /// the env-shaped `directive` used by [`crate::perf::resolve_directive`].
    pub fn as_config_str(self) -> &'static str {
        match self {
            PerfLog::Off => "off",
            PerfLog::BudgetOnly => "budget-only",
            PerfLog::Verbose => "verbose",
        }
    }
}

/// User configuration (`amenbo config`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Default view, used when `project add` omits `--view`.
    pub default_view: View,
    /// **The user's language** (BCP-47-ish: `ja` / `en` / `zh` …). Drives what we tell the AI, what
    /// it produces (task titles, comments), the directives in AGENTS.md, and — in future — the
    /// localization of UI labels. Unset means English. **A user-level, global setting**, not a
    /// per-project one. Set it with `amenbo config set language <code>` or
    /// `amenbo init --language <code>`.
    #[serde(default)]
    pub language: Option<String>,
    /// Date locale. Reserved; unused so far.
    #[serde(default)]
    pub date_locale: Option<String>,
    /// Whether the AI (`--actor ai`) may perform destructive or concealing project operations
    /// (`project archive` / `delete`). **Off by default**: the AI guardrail refuses archive and
    /// delete, while the reversible operations (add/update/move/unarchive) are not gated. **A local
    /// policy; never synced.** It exists to stop accidents by an honest actor — it is not a security
    /// boundary.
    #[serde(default)]
    pub ai_allow_project_ops: bool,
    /// Whether first-run setup (language → name → …) has been completed. This is the **explicit
    /// trigger** for the GUI's first-launch flow. **Defaults to false** (not done). Regardless of
    /// whether data already exists, the GUI shows first-run setup while this is false, so a store
    /// that has data but never got configured is not skipped. Set to true on completion. **A
    /// user-level setting; never synced.**
    #[serde(default)]
    pub onboarded: bool,
    /// Whether startup runs the read-only consistency check (the same orphan/dangling-reference
    /// sweep as `doctor`) and surfaces any problem as a warning. **On by default: preventive
    /// detection in production.** It only inspects — it never repairs; repair stays a manual
    /// `amenbo doctor --fix`. It can be turned off as an escape hatch when the cost is noticeable on
    /// a large store. **A local setting; never synced.**
    #[serde(default = "default_true")]
    pub startup_integrity_check: bool,
    /// Whether to query the static `latest.json` and thereby notice that a newer release exists.
    /// **On by default**: noticing a new release is a plain user benefit. Turning it off suppresses
    /// even this infrastructure query — the privacy escape hatch. The query is **infrastructure
    /// traffic only**; the core guarantee of zero functional traffic (no user data ever leaves the
    /// machine) is untouched. The env var `AMENBO_UPDATE_CHECK=0` overrides the config and disables
    /// it outright — a hard kill switch for CI. The query has a timeout, fails silently, and caches
    /// its result ([`crate::update_check`]). **A local setting; never synced.**
    #[serde(default = "default_true")]
    pub update_check: bool,
    /// Logging level for the perf instrumentation (`off` / `budget-only` / `verbose`). **Defaults to
    /// `None` (unset)**, deferring to the channel/build default: on (`budget-only`) for
    /// amenbo-dev/debug, off for amenbo/release. The `AMENBO_PERF` env var overrides everything.
    /// It can be switched at runtime through `tracing_subscriber::reload` (`config_set_perf_log`).
    /// **A local setting; never synced.**
    #[serde(default)]
    pub perf_log: Option<PerfLog>,
    /// Per-file attachment size caps, by type. The defaults are loose — they exist to stop breakage
    /// and runaways, not to ration quota — and can be overridden with `amenbo config set
    /// attachment.*` (`image_max` / `audio_max` / `video_max` / `document_max` / `other_max`; the
    /// value is a byte count). **A local setting; never synced** — capacity is a per-machine concern.
    #[serde(default)]
    pub attachment_limits: crate::blob::CapacityPolicy,
    /// Display name of the human facet. The roster of a single local store has exactly two members,
    /// "me" and "my AI"; this names the human one. Unset means the language-linked default —
    /// `Human`, or its Japanese equivalent under `ja`; see [`Config::human_display_name`].
    /// **A user-level setting; never synced.**
    #[serde(default)]
    pub human_name: Option<String>,
    /// Display name of the AI facet: what the human delegates to (`assignee:me-ai`). Unset means the
    /// default `AI`, in every language — see [`Config::ai_display_name`]. **A user-level setting;
    /// never synced.**
    #[serde(default)]
    pub ai_name: Option<String>,
    /// Optional avatar image for the human facet: a small `data:image/…` data URL, capped at
    /// [`AVATAR_MAX_BYTES`]. Together with the display name, it is the face that tells "me" from "my
    /// AI". Unset means an automatic avatar (identicon). **A user-level setting; never synced** — it
    /// is a per-machine preference.
    #[serde(default)]
    pub human_avatar: Option<String>,
    /// Optional avatar image for the AI facet. Same contract as [`Config::human_avatar`].
    #[serde(default)]
    pub ai_avatar: Option<String>,
    /// **May amenbo wire its lint into your git hooks?** — asked once, for the lint as a feature, and
    /// never again ([`crate::hooks`]). `None` is the unanswered state, which is what makes "asked and
    /// refused" different from "never asked". It lives here rather than against a project because the
    /// answer is not about a project: the same person answers the same way in every repository they have,
    /// so asking per repository would be repeating a question, not asking a new one. **A user-level
    /// setting; never synced.**
    ///
    /// There is no `config set` key for it. The faces that move it are the ones that state an intent —
    /// the first-run question, and `hooks install` / `hooks uninstall`, which act on one repository and
    /// leave this alone.
    #[serde(default)]
    pub hook_consent: Option<crate::hooks::HookConsent>,
    /// **Plugin text configuration — machine defaults** (`AMB-D-356` / `AMB-D-350`): the lower of the two
    /// tiers a *non-secret* plugin setting lives in (the per-project override is the store's
    /// `plugin_config` record table, read on top of this). Keyed plugin name → field key → value. Written
    /// only through the config write boundary ([`crate::plugin_config::set`]), never `config set`: a
    /// plugin's schema is the author's, not a fixed key set. **Secrets are never here** — a `secret` field
    /// lands in the user-area secret file ([`crate::plugin_secret`]), off the store and off every backup.
    /// Empty by default; an older config with no key is a machine with no plugin defaults, not a parse
    /// error, and an empty map does not serialize (no `{}` residue).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub plugin_config: BTreeMap<String, BTreeMap<String, String>>,
    /// **Per-plugin trust — machine-global, never synced** (`AMB-D-350`/`AMB-D-351`). A plugin's *presence*
    /// in this map is the record of its **one-time consent** to run arbitrary code (`AMB-D-351`): it is
    /// written at the first `enable`, kept across a `disable`, and removed only by `uninstall`
    /// (`AMB-D-357`). [`PluginTrust::enabled`] is the current gate — `install ≠ enable`, so a plugin that
    /// was installed but never enabled is simply absent here, and a disabled one is present with
    /// `enabled: false`. Moved only through the trust write boundary ([`crate::plugin_trust`]), never
    /// `config set`. Empty by default and does not serialize empty (no `{}` residue).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub plugin_trust: BTreeMap<String, PluginTrust>,
}

/// The trust state amenbo keeps for one plugin — machine-global, never synced (`AMB-D-350`). The struct's
/// *presence* in [`Config::plugin_trust`] is the consent record (`AMB-D-351`, given once and never asked
/// for again); [`PluginTrust::enabled`] is whether the plugin currently fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginTrust {
    /// Whether the plugin currently fires (`AMB-D-351`, the machine-global gate). `install ≠ enable`:
    /// consent (this record's very existence) does not by itself run anything — `enable` sets this, and
    /// `disable` clears it while keeping the consent so a later re-`enable` never asks again.
    pub enabled: bool,
}

impl Config {
    /// The machine-default value of one plugin text field, if set (`AMB-D-356`, the lower tier). The
    /// per-project override that may sit on top of it lives in the store, not here.
    pub fn plugin_text_default(&self, plugin: &str, key: &str) -> Option<&str> {
        self.plugin_config.get(plugin)?.get(key).map(String::as_str)
    }

    /// Whether this plugin currently fires (`AMB-D-351`, the machine-global gate). `false` both for a plugin
    /// that was never enabled (absent) and for one explicitly disabled. Read by the dispatch resolver so
    /// only an enabled plugin observes an event (`AMB-T-2032`).
    pub fn plugin_enabled(&self, plugin: &str) -> bool {
        self.plugin_trust.get(plugin).is_some_and(|t| t.enabled)
    }

    /// Whether consent to run this plugin's arbitrary code has been recorded (`AMB-D-351`, given once). It
    /// stays `true` across a `disable`, so a re-`enable` never re-asks; only `uninstall` clears it
    /// ([`Config::forget_plugin_trust`]).
    pub fn plugin_consented(&self, plugin: &str) -> bool {
        self.plugin_trust.contains_key(plugin)
    }

    /// Record consent (if not already) and open the gate so the plugin fires (`AMB-D-351`). Idempotent —
    /// re-enabling an already-enabled plugin is a no-op, and re-enabling a disabled one keeps its consent.
    /// Does **not** persist; the caller saves the config through the write boundary, as with
    /// [`Config::set_plugin_text_default`]. Prefer the boundary [`crate::plugin_trust::enable`], which is
    /// fail-closed on required settings.
    pub fn enable_plugin(&mut self, plugin: &str) {
        self.plugin_trust.insert(plugin.to_string(), PluginTrust { enabled: true });
    }

    /// Close the gate, keeping the consent record (`disable ≠ uninstall`, `AMB-D-357`). A no-op for a plugin
    /// with no trust record. Does not persist.
    pub fn disable_plugin(&mut self, plugin: &str) {
        if let Some(t) = self.plugin_trust.get_mut(plugin) {
            t.enabled = false;
        }
    }

    /// Erase this plugin's trust entirely — its consent and its enabled state both (`AMB-D-357`, the
    /// `uninstall` after-clean). Does not persist.
    pub fn forget_plugin_trust(&mut self, plugin: &str) {
        self.plugin_trust.remove(plugin);
    }

    /// Drop every machine default this plugin holds — the `config.json` half of `uninstall`
    /// (`AMB-D-357`), beside [`forget_plugin_trust`](Self::forget_plugin_trust)'s consent half, so a
    /// re-install starts clean rather than inheriting the settings of the copy that was removed. Returns
    /// whether anything was there. Does not persist.
    pub fn forget_plugin_config(&mut self, plugin: &str) -> bool {
        self.plugin_config.remove(plugin).is_some()
    }

    /// Set (`Some`) or clear (`None`) the machine default of one plugin text field. Clearing removes the
    /// key, and the plugin's map with it once empty, so an unset field leaves no `{}` residue. Does **not**
    /// persist — the caller saves the config (through the write boundary).
    pub fn set_plugin_text_default(&mut self, plugin: &str, key: &str, value: Option<&str>) {
        match value {
            Some(v) => {
                self.plugin_config
                    .entry(plugin.to_string())
                    .or_default()
                    .insert(key.to_string(), v.to_string());
            }
            None => {
                if let Some(fields) = self.plugin_config.get_mut(plugin) {
                    fields.remove(key);
                    if fields.is_empty() {
                        self.plugin_config.remove(plugin);
                    }
                }
            }
        }
    }
}

/// Cap on the size of an avatar data URL, in bytes. A loose limit to stop breakage and runaways —
/// a downscaled (96px) PNG fits comfortably. Both `config set human_avatar/ai_avatar` (CLI) and the
/// GUI's `set_facet_avatars` go through this guard.
pub const AVATAR_MAX_BYTES: usize = 256 * 1024;

/// Validate the shape of an avatar value (a data URL). Only pass a non-empty value; clearing (empty)
/// is the caller's branch. Rejects anything over the cap or not starting with `data:image/`. The CLI
/// (`config set`) and the GUI (`set_facet_avatars`) go through the same rule.
pub fn validate_avatar(key: &str, value: &str) -> Result<()> {
    if value.len() > AVATAR_MAX_BYTES {
        return Err(crate::error::Error::invalid(
            format!("{key} image too large ({} KB; max {} KB)", value.len() / 1024, AVATAR_MAX_BYTES / 1024),
            format!("{key} の画像が大きすぎます（{} KB・上限 {} KB）", value.len() / 1024, AVATAR_MAX_BYTES / 1024),
        ));
    }
    if !value.starts_with("data:image/") {
        return Err(crate::error::Error::invalid(
            format!("{key} must be a data:image/… URL"),
            format!("{key} は data:image/… 形式の画像データが必要です"),
        ));
    }
    Ok(())
}

/// Parse the value of `config set attachment.*`: a byte count, as a non-negative decimal integer.
fn parse_bytes(key: &str, value: &str) -> crate::error::Result<u64> {
    value.trim().parse::<u64>().map_err(|_| {
        crate::error::Error::invalid(
            format!("{key} must be a non-negative byte count; '{value}' is invalid"),
            format!("{key} はバイト数（非負整数）。'{value}' は不正"),
        )
    })
}

/// For `serde(default)`: the value (on) an existing config gets when it predates the field.
fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config {
            default_view: View::Board,
            language: None,
            date_locale: None,
            ai_allow_project_ops: false,
            onboarded: false,
            startup_integrity_check: true,
            update_check: true,
            perf_log: None,
            attachment_limits: crate::blob::CapacityPolicy::default(),
            human_name: None,
            ai_name: None,
            human_avatar: None,
            ai_avatar: None,
            hook_consent: None,
            plugin_config: BTreeMap::new(),
            plugin_trust: BTreeMap::new(),
        }
    }
}

/// Whether a language code is in the Japanese family (`ja`, `ja-JP`, …). Used to make the default
/// display names follow the language.
fn is_japanese(lang: Option<&str>) -> bool {
    lang.map(|l| l.split(['-', '_']).next().unwrap_or(l) == "ja").unwrap_or(false)
}

/// The **language-linked default** display name of the human facet, when `config.human_name` is
/// unset: the Japanese word for "human" under `ja`, and `Human` otherwise.
pub fn default_human_name(lang: Option<&str>) -> String {
    if is_japanese(lang) { "人間" } else { "Human" }.to_string()
}

/// The **default** display name of the AI facet, when `config.ai_name` is unset: `AI`, in every
/// language.
pub fn default_ai_name(_lang: Option<&str>) -> String {
    "AI".to_string()
}

/// The **language-linked default** project name, used when a project is created without one: the
/// Japanese word for "project" under `ja`, and `Project` otherwise. It fills the blank when the user
/// submits the GUI's project-creation form with an empty name.
pub fn default_project_name(lang: Option<&str>) -> String {
    if is_japanese(lang) { "プロジェクト" } else { "Project" }.to_string()
}

/// Turn a language code (`ja`, …) into its English name, for both AI and human readers; an unknown
/// code is returned as-is. Used when embedding the language into AGENTS.md.
pub fn language_label(code: &str) -> String {
    match code.split(['-', '_']).next().unwrap_or(code) {
        "ja" => "Japanese",
        "en" => "English",
        "zh" => "Chinese",
        "ko" => "Korean",
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "pt" => "Portuguese",
        "it" => "Italian",
        "ru" => "Russian",
        _ => return code.to_string(),
    }
    .to_string()
}

impl Config {
    /// The effective display name of the human facet: `human_name` when non-empty, otherwise the
    /// language-linked default from [`default_human_name`].
    pub fn human_display_name(&self) -> String {
        self.human_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| default_human_name(self.language.as_deref()))
    }

    /// The effective display name of the AI facet: `ai_name` when non-empty, otherwise the default
    /// `AI`.
    pub fn ai_display_name(&self) -> String {
        self.ai_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| default_ai_name(self.language.as_deref()))
    }

    /// Resolve an assignee/author token to a facet. A single local store has exactly two actors, the
    /// human and the AI facet, and a token matches either a reserved word (`me` / `self` / `human` →
    /// Human; `me-ai` / `ai` → Ai) or a configured display name (`human_display_name` /
    /// `ai_display_name`, case-insensitively). `None` if it matches neither.
    pub fn resolve_facet(&self, token: &str) -> Option<crate::model::ActorKind> {
        use crate::model::ActorKind;
        let t = token.trim();
        if t.is_empty() {
            return None;
        }
        let eq = |a: &str| a.eq_ignore_ascii_case(t);
        if eq("me") || eq("self") || eq("human") || eq(&self.human_display_name()) {
            Some(ActorKind::Human)
        } else if eq("me-ai") || eq("ai") || eq(&self.ai_display_name()) {
            Some(ActorKind::Ai)
        } else {
            None
        }
    }

    /// The human facet's avatar: the data URL when non-empty, otherwise `None` — in which case the
    /// GUI draws an identicon.
    pub fn human_avatar(&self) -> Option<String> {
        self.human_avatar.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
    }

    /// The AI facet's avatar. Same contract as [`Config::human_avatar`].
    pub fn ai_avatar(&self) -> Option<String> {
        self.ai_avatar.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
    }

    /// Look up the avatar for a facet — what the roster and the snapshot draw each facet's face
    /// from.
    pub fn avatar_for(&self, kind: crate::model::ActorKind) -> Option<String> {
        match kind {
            crate::model::ActorKind::Human => self.human_avatar(),
            crate::model::ActorKind::Ai => self.ai_avatar(),
        }
    }

    /// The roster behind `user list` and the GUI's members: the two people in the config, as
    /// `(facet, display name)` for human and ai.
    pub fn roster(&self) -> [(crate::model::ActorKind, String); 2] {
        use crate::model::ActorKind;
        [
            (ActorKind::Human, self.human_display_name()),
            (ActorKind::Ai, self.ai_display_name()),
        ]
    }

    /// Read the config file, falling back to the defaults when it is absent — for the cases that
    /// want a peek at the config without opening the Store. A corrupt file also falls back to the
    /// defaults, so it can never block the main path.
    pub fn load(path: &std::path::Path) -> Config {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    /// Write the config file, creating the parent directory as needed. This is for the callers that
    /// want to update the config without opening the store — GUI first-run setup, the facet setters,
    /// and so on; once the store is open, use [`crate::Store::save_config`]. The GUI and the CLI can
    /// write the config concurrently, so a non-atomic full rewrite invites a torn write: this takes
    /// the same **atomic write** (temp → rename, [`crate::store::write_atomic`]) as the
    /// `Store::save_config` path, ruling corruption out structurally.
    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        crate::store::write_atomic(path, serde_json::to_string_pretty(self)?.as_bytes())?;
        Ok(())
    }

    /// Set a config key, validating it and its value on behalf of `config set`.
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "default_view" => {
                self.default_view = match value {
                    "list" => View::List,
                    "board" => View::Board,
                    "calendar" => View::Calendar,
                    "timeline" => View::Timeline,
                    other => {
                        return Err(crate::error::Error::invalid(
                            format!("default_view must be list|board|calendar|timeline; '{other}' is invalid"),
                            format!("default_view は list|board|calendar|timeline。'{other}' は不正"),
                        ))
                    }
                };
            }
            // A store owns a single workspace, so there is no field behind this key; it is still accepted
            // as a silent no-op so scripts calling `config set default_workspace` don't error.
            "default_workspace" => {}
            "language" => {
                self.language = Some(value.to_string());
            }
            "date_locale" => {
                self.date_locale = Some(value.to_string());
            }
            // Display names. An empty string clears the override, restoring the language-linked
            // default. Surrounding whitespace is trimmed.
            "human_name" => {
                let trimmed = value.trim();
                self.human_name = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
            }
            "ai_name" => {
                let trimmed = value.trim();
                self.ai_name = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
            }
            // Facet avatars. An empty string clears the avatar, restoring the identicon. A non-empty
            // value is validated for data-URL shape and size.
            "human_avatar" | "ai_avatar" => {
                let trimmed = value.trim();
                let val = if trimmed.is_empty() {
                    None
                } else {
                    validate_avatar(key, trimmed)?;
                    Some(trimmed.to_string())
                };
                if key == "human_avatar" {
                    self.human_avatar = val;
                } else {
                    self.ai_avatar = val;
                }
            }
            "ai_allow_project_ops" => {
                self.ai_allow_project_ops = match value {
                    "true" | "on" | "1" => true,
                    "false" | "off" | "0" => false,
                    other => {
                        return Err(crate::error::Error::invalid(
                            format!("ai_allow_project_ops must be true|false; '{other}' is invalid"),
                            format!("ai_allow_project_ops は true|false。'{other}' は不正"),
                        ))
                    }
                };
            }
            "onboarded" => {
                self.onboarded = match value {
                    "true" | "on" | "1" => true,
                    "false" | "off" | "0" => false,
                    other => {
                        return Err(crate::error::Error::invalid(
                            format!("onboarded must be true|false; '{other}' is invalid"),
                            format!("onboarded は true|false。'{other}' は不正"),
                        ))
                    }
                };
            }
            "startup_integrity_check" => {
                self.startup_integrity_check = match value {
                    "true" | "on" | "1" => true,
                    "false" | "off" | "0" => false,
                    other => {
                        return Err(crate::error::Error::invalid(
                            format!("startup_integrity_check must be true|false; '{other}' is invalid"),
                            format!("startup_integrity_check は true|false。'{other}' は不正"),
                        ))
                    }
                };
            }
            "update_check" => {
                self.update_check = match value {
                    "true" | "on" | "1" => true,
                    "false" | "off" | "0" => false,
                    other => {
                        return Err(crate::error::Error::invalid(
                            format!("update_check must be true|false; '{other}' is invalid"),
                            format!("update_check は true|false。'{other}' は不正"),
                        ))
                    }
                };
            }
            "perf_log" => {
                self.perf_log = match value {
                    "off" => Some(PerfLog::Off),
                    "budget-only" | "budget_only" => Some(PerfLog::BudgetOnly),
                    "verbose" => Some(PerfLog::Verbose),
                    other => {
                        return Err(crate::error::Error::invalid(
                            format!("perf_log must be off|budget-only|verbose; '{other}' is invalid"),
                            format!("perf_log は off|budget-only|verbose。'{other}' は不正"),
                        ))
                    }
                };
            }
            "attachment.image_max" => self.attachment_limits.image_max = parse_bytes("attachment.image_max", value)?,
            "attachment.audio_max" => self.attachment_limits.audio_max = parse_bytes("attachment.audio_max", value)?,
            "attachment.video_max" => self.attachment_limits.video_max = parse_bytes("attachment.video_max", value)?,
            "attachment.document_max" => self.attachment_limits.document_max = parse_bytes("attachment.document_max", value)?,
            "attachment.other_max" => self.attachment_limits.other_max = parse_bytes("attachment.other_max", value)?,
            other => {
                return Err(crate::error::Error::invalid(
                    format!("unknown config key '{other}' (known: default_view / language / date_locale / human_name / ai_name / human_avatar / ai_avatar / ai_allow_project_ops / onboarded / startup_integrity_check / update_check / perf_log / attachment.image_max / attachment.audio_max / attachment.video_max / attachment.document_max / attachment.other_max)"),
                    format!("未知の設定キー '{other}'（既知: default_view / language / date_locale / human_name / ai_name / human_avatar / ai_avatar / ai_allow_project_ops / onboarded / startup_integrity_check / update_check / perf_log / attachment.image_max / attachment.audio_max / attachment.video_max / attachment.document_max / attachment.other_max）"),
                ))
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plugin disk layout hangs off the base: bodies and the registry cache under `plugins/`, a
    /// plugin's home under its name, and nothing in `.amenbo` (`AMB-D-350`).
    #[test]
    fn plugin_layout_hangs_off_the_base_never_the_project() {
        let base = PathBuf::from("/home/base");
        let paths = Paths::at(base.clone());

        assert_eq!(paths.plugins_dir(), base.join("plugins"));
        assert_eq!(paths.plugin_dir("worktree"), base.join("plugins").join("worktree"));
        assert_eq!(paths.registry_dir(), base.join("plugins").join("registry"));

        // Every plugin path is under the base_dir — the app-data area, not a synced project dir.
        for p in [paths.plugins_dir(), paths.plugin_dir("x"), paths.registry_dir()] {
            assert!(p.starts_with(&paths.base_dir), "{} sits under the base", p.display());
        }
    }

    /// The registry cache shares `plugins/` with the installed plugins, so `registry` is not a name a
    /// plugin may take — otherwise `plugin_dir("registry")` and `registry_dir` would be the same path.
    #[test]
    fn the_registry_name_is_reserved_against_a_plugin_clash() {
        assert!(is_reserved_plugin_name("registry"));
        assert!(!is_reserved_plugin_name("worktree"));

        let paths = Paths::at(PathBuf::from("/home/base"));
        assert_eq!(
            paths.plugin_dir(Paths::REGISTRY_DIR_NAME),
            paths.registry_dir(),
            "the reason registry is reserved: the paths would collide"
        );
    }

    /// `config set attachment.*` overrides a capacity limit (byte count) and rejects non-numeric
    /// values, leaving the prior value untouched.
    #[test]
    fn attachment_limits_set_overrides_bytes_and_rejects_garbage() {
        let mut c = Config::default();

        c.set("attachment.video_max", "1048576").unwrap();
        assert_eq!(c.attachment_limits.video_max, 1_048_576);
        c.set("attachment.image_max", "  2048  ").unwrap();
        assert_eq!(c.attachment_limits.image_max, 2048, "trimmed and parsed");

        // A non-numeric value is rejected and the prior value is kept.
        let err = c.set("attachment.image_max", "10MiB").unwrap_err();
        assert_eq!(err.code(), "invalid_value");
        assert_eq!(c.attachment_limits.image_max, 2048);
    }


    /// `config set human_name/ai_name` stores a trimmed name and an empty value clears it back
    /// to the language-linked default; `*_display_name()` falls back to the default when unset.
    #[test]
    fn display_names_set_clear_and_default_by_language() {
        let mut c = Config::default();
        // Unset → language-linked defaults (English when language is None).
        assert!(c.human_name.is_none() && c.ai_name.is_none());
        assert_eq!(c.human_display_name(), "Human");
        assert_eq!(c.ai_display_name(), "AI");

        // Japanese default when language=ja.
        c.set("language", "ja").unwrap();
        assert_eq!(c.human_display_name(), "人間");
        assert_eq!(c.ai_display_name(), "AI");

        // Set (trimmed) and read back.
        c.set("human_name", "  山田  ").unwrap();
        c.set("ai_name", "さくら").unwrap();
        assert_eq!(c.human_name.as_deref(), Some("山田"), "stored trimmed");
        assert_eq!(c.human_display_name(), "山田");
        assert_eq!(c.ai_display_name(), "さくら");

        // Empty clears back to the default.
        c.set("human_name", "   ").unwrap();
        assert!(c.human_name.is_none(), "empty clears the override");
        assert_eq!(c.human_display_name(), "人間", "back to language default");
    }

    /// A config persisted without the `human_name` / `ai_name` keys still loads, the fields defaulting to
    /// `None` so display names fall back to their defaults.
    #[test]
    fn config_without_display_name_keys_loads_with_defaults() {
        let mut json = serde_json::to_value(Config::default()).unwrap();
        let obj = json.as_object_mut().unwrap();
        obj.remove("human_name");
        obj.remove("ai_name");
        let loaded: Config = serde_json::from_value(json).expect("legacy config without display names loads");
        assert!(loaded.human_name.is_none() && loaded.ai_name.is_none());
        assert_eq!(loaded.human_display_name(), "Human");
    }

    /// `config set human_avatar/ai_avatar` stores a validated data URL, an empty value clears it,
    /// and non-image / oversized values are rejected (the prior value kept). `avatar_for` reads per facet.
    #[test]
    fn facet_avatars_set_clear_and_validate() {
        use crate::model::ActorKind;
        let mut c = Config::default();
        assert!(c.human_avatar().is_none() && c.ai_avatar().is_none());

        // Set (trimmed) and read back per facet.
        c.set("human_avatar", "  data:image/png;base64,AAAA  ").unwrap();
        c.set("ai_avatar", "data:image/png;base64,BBBB").unwrap();
        assert_eq!(c.human_avatar().as_deref(), Some("data:image/png;base64,AAAA"), "stored trimmed");
        assert_eq!(c.avatar_for(ActorKind::Human).as_deref(), Some("data:image/png;base64,AAAA"));
        assert_eq!(c.avatar_for(ActorKind::Ai).as_deref(), Some("data:image/png;base64,BBBB"));

        // A non-image data URL is rejected and the prior value is kept.
        let err = c.set("human_avatar", "data:text/plain;base64,AAAA").unwrap_err();
        assert_eq!(err.code(), "invalid_value");
        assert_eq!(c.human_avatar().as_deref(), Some("data:image/png;base64,AAAA"), "kept on reject");

        // An oversized value is rejected too.
        let big = format!("data:image/png;base64,{}", "A".repeat(AVATAR_MAX_BYTES));
        let err = c.set("ai_avatar", &big).unwrap_err();
        assert_eq!(err.code(), "invalid_value");

        // Empty clears it back to identicon (None).
        c.set("human_avatar", "   ").unwrap();
        assert!(c.human_avatar().is_none(), "empty clears the avatar");
    }

    /// A config persisted without the `human_avatar` / `ai_avatar` keys still loads, the fields defaulting
    /// to `None`.
    #[test]
    fn config_without_avatar_keys_loads_with_defaults() {
        let mut json = serde_json::to_value(Config::default()).unwrap();
        let obj = json.as_object_mut().unwrap();
        obj.remove("human_avatar");
        obj.remove("ai_avatar");
        let loaded: Config = serde_json::from_value(json).expect("legacy config without avatars loads");
        assert!(loaded.human_avatar().is_none() && loaded.ai_avatar().is_none());
    }

    /// `Config::save` writes the config file atomically (temp→rename) and leaves no `.tmp` residue, so a
    /// concurrent GUI+CLI write can never leave a torn/half-written config. The round-trip through `load`
    /// recovers the same values.
    #[test]
    fn save_is_atomic_round_trips_and_leaves_no_tmp() {
        let dir = amenbo_scratch::scratch("cfg");
        let path = dir.join("config.json");

        let mut c = Config::default();
        c.set("language", "ja").unwrap();
        c.set("human_name", "山田").unwrap();
        c.save(&path).expect("save writes the config");

        // The rename target exists; the temp file does not linger.
        assert!(path.exists(), "config file is written");
        assert!(!path.with_extension("tmp").exists(), "no .tmp residue after atomic rename");

        let loaded = Config::load(&path);
        assert_eq!(loaded.language.as_deref(), Some("ja"));
        assert_eq!(loaded.human_name.as_deref(), Some("山田"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A config carrying the removed `keychain` / `identity_keychain_only` knobs still loads (the fields
    /// are gone and their JSON keys ignored), so an existing store's config is never bricked.
    #[test]
    fn legacy_keychain_config_keys_are_ignored() {
        let mut json = serde_json::to_value(Config::default()).unwrap();
        let obj = json.as_object_mut().unwrap();
        obj.insert("keychain".into(), serde_json::json!(true));
        obj.insert("identity_keychain_only".into(), serde_json::json!(true));
        let loaded: Config = serde_json::from_value(json).expect("legacy keychain keys load");
        // The knobs are gone; the rest of the config is intact.
        assert_eq!(loaded.default_view, Config::default().default_view);
    }
}
