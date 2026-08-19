//! Configuration and file-path resolution.
//!
//! - If the `AMENBO_HOME` environment variable is set, everything lives under that directory
//!   (for tests and explicit overrides).
//! - Otherwise the OS-standard data/config directory (`directories`) is used.
//!
//! Config lives in its own file next to the store (`config.json`). The identity lives in
//! `identity.json` directly under the base dir and **holds no secrets**. The `accounts/P0/` layout
//! written by older builds is lifted to the base dir once, on open ([`lift_legacy_identity`]).

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

/// The word a development build wears on screen. Not translated: it names a channel, the way a
/// version number does, and a reader who sees it in someone else's screenshot must read the same word.
const DEV_BADGE: &str = "DEV";

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

    /// The app-data "app name" of the **production** channel — the directory real user data lives in
    /// (`…/work.amenbo.amenbo`). Named because more than the path is keyed off it:
    /// [`crate::build_stamp`] asks whether this build is pointed at production before it lets an
    /// unreleased binary migrate anything (`AMB-D-378`).
    pub const PRODUCTION_APP_NAME: &'static str = "amenbo";

    /// The app-data "app name". Substitutable **at build time** via `AMENBO_APP_NAME`, which is how
    /// dev and prod are kept apart. The default is production
    /// ([`PRODUCTION_APP_NAME`](Self::PRODUCTION_APP_NAME) — `~/Library/Application Support/work.amenbo.amenbo`);
    /// a dev build sets `AMENBO_APP_NAME=amenbo-dev` and gets its own directory
    /// (`…/work.amenbo.amenbo-dev`), so its identity and store never collide with production data.
    pub const APP_NAME: &'static str = match option_env!("AMENBO_APP_NAME") {
        Some(name) => name,
        None => Self::PRODUCTION_APP_NAME,
    };

    /// The app-data "app name" of the **development** channel — the shared dev build's own directory
    /// (`…/work.amenbo.amenbo-dev`). It is a prefix as much as a name: a throwaway dev GUI built for
    /// one task extends it with that task's number (`amenbo-dev-<id>`, `AMB-D-390`) so two parallel
    /// sessions never share a store, which is why the channel test below is not an equality.
    pub const DEV_APP_NAME: &'static str = "amenbo-dev";

    /// Whether this build is on the development channel: the shared dev build, or a throwaway
    /// per-task instance named after it. Build-time `AMENBO_APP_NAME` picks the channel, so a given
    /// binary always answers the same. Channel defaults that should follow development rather than
    /// production — the perf log's, for one — key off this instead of matching a single name, or a
    /// task instance would quietly fall through to the production behaviour.
    pub fn is_dev_channel() -> bool {
        Self::is_dev_app_name(Self::APP_NAME)
    }

    /// The CLI this build's channel installs — the name guidance tells a human or an AI to **type**,
    /// which is not the same thing as the app-data name this build **reads**. The two coincide on
    /// production and on the shared dev build, and part company on a throwaway per-task instance:
    /// its app-data is `amenbo-dev-<id>`, but it ships no CLI of its own, so what there is to type is
    /// still the dev CLI. Naming the app-data instead is how guidance ends up pointing at a command
    /// that does not exist.
    ///
    /// Every surface that words a command for someone to run — the managed block's `{CMD}`, the hook
    /// setup notice, `init`'s closing line — takes it from here, and everything that names a
    /// directory or a channel keeps taking [`APP_NAME`](Self::APP_NAME).
    pub fn command_name() -> &'static str {
        Self::command_name_for(Self::APP_NAME)
    }

    /// The rule [`command_name`](Self::command_name) applies, taking the name as an argument for the
    /// reason [`is_dev_app_name`](Self::is_dev_app_name) does — a running binary's channel is fixed
    /// at compile time, so only a table can pin what each name maps to.
    pub(crate) fn command_name_for(app_name: &str) -> &'static str {
        if Self::is_dev_app_name(app_name) {
            Self::DEV_APP_NAME // the dev CLI's name, which the shared dev build's app-data happens to share
        } else {
            Self::PRODUCTION_APP_NAME
        }
    }

    /// The name the CLI file carries **inside the app's bundle**, without an extension. It is not a
    /// channel fact at all: Tauri bundles the sidecar under the stem `bundle.externalBin` names
    /// (`binaries/amenbo`), and that config is one file for every build, so a dev bundle ships
    /// `amenbo` beside its GUI exactly as production does. `guards/check-sidecar-name.sh` is what
    /// keeps this constant and that config saying the same word.
    pub const SIDECAR_NAME: &'static str = "amenbo";

    /// That stem as the file name this platform's bundle actually holds — the thing to look for beside
    /// the running binary when something needs the CLI as a **path** rather than as a command word
    /// (an MCP host is not a shell and has no `PATH` of the reader's to resolve one in).
    ///
    /// Distinct from [`command_name`](Self::command_name) on purpose: that answers what to **type**,
    /// which the dev channel spells `amenbo-dev`, while nothing of that name is ever written into a
    /// bundle. Asking the typing question about a file is how a dev build ends up looking for
    /// `Contents/MacOS/amenbo-dev` and finding nothing.
    pub fn sidecar_file_name() -> &'static str {
        if cfg!(windows) { "amenbo.exe" } else { Self::SIDECAR_NAME }
    }

    /// The naming rule [`is_dev_channel`](Self::is_dev_channel) applies, taking the name as an
    /// argument so the rule can be pinned by a table: the channel of a running binary is fixed at
    /// compile time, so a test cannot vary it. `amenbo-dev-ish` is not a task instance — only the
    /// separator makes one, which is what keeps a future channel name from being read as one.
    pub(crate) fn is_dev_app_name(name: &str) -> bool {
        name == Self::DEV_APP_NAME
            || name
                .strip_prefix(Self::DEV_APP_NAME)
                .is_some_and(|rest| rest.starts_with('-'))
    }

    /// What this build should call itself on screen, or `None` on production — the string the GUI puts
    /// in its header so a screenshot says which of the three same-named windows it came from
    /// (production, the shared dev build, one task's throwaway instance all run as `amenbo-app`).
    ///
    /// Production is deliberately silent: a badge that showed everywhere would be shipped chrome
    /// nobody reads, and its absence is what makes the dev one worth noticing.
    pub fn dev_badge() -> Option<String> {
        Self::dev_badge_for(Self::APP_NAME)
    }

    /// The labelling rule [`dev_badge`](Self::dev_badge) applies, taking the name as an argument for
    /// the reason [`is_dev_app_name`](Self::is_dev_app_name) does — a running binary's channel is
    /// fixed at compile time, so only a table can pin what each name reads as.
    ///
    /// A task instance carries its number, and it is spelled as the ref the task is known by, not as
    /// the raw app-data suffix: the badge is read next to the task it belongs to. A suffix that is not
    /// a number is shown as it stands rather than dropped — the build only ever writes digits there
    /// (`AMB-T-ID`), so anything else is worth seeing rather than hiding behind a bare `DEV`.
    pub(crate) fn dev_badge_for(name: &str) -> Option<String> {
        if name == Self::DEV_APP_NAME {
            return Some(DEV_BADGE.to_owned());
        }
        let instance = name
            .strip_prefix(Self::DEV_APP_NAME)
            .and_then(|rest| rest.strip_prefix('-'))?;
        Some(match instance.parse::<i64>() {
            Ok(task_id) => format!("{DEV_BADGE} {}", crate::idref::task(task_id)),
            Err(_) => format!("{DEV_BADGE} {instance}"),
        })
    }

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
    /// never in `.amenbo`, the store-agnostic, sync-safe pointer. Enablement is *not* on disk here: it is
    /// a store table, one row per project the plugin is on in (`AMB-D-434`).
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

    /// The **plugin execution log**, `<base>/plugin-runs.jsonl` ([`crate::plugin_log`], `AMB-D-361`): the
    /// last runs of each installed plugin, with the stderr its author wrote. Machine-local like the
    /// activity ledger and, like it, outside every backup and export — it is a debugging aid about *this*
    /// machine's installs, bounded by construction rather than kept as history.
    pub fn plugin_log_file(&self) -> PathBuf {
        self.base_dir.join(crate::plugin_log::FILE_NAME)
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
    /// **How dates are written**, as a BCP-47 tag (`ja-JP` / `en-US` / `sv-SE` for the ISO form).
    /// Unset means the one that goes with [`language`](Config::language), which is the answer that
    /// fits most people — this exists for the reader whose two answers differ, wanting a Japanese UI
    /// with ISO dates, or an English one with Japanese date order.
    ///
    /// **Only the GUI reads it** (`AMB-D-11`: the CLI is English-fixed, so a date it prints is part
    /// of a contract, not a presentation). The tag is stored opaquely — what is a usable locale is
    /// the formatter's judgement, and the GUI falls back to the language's when the platform does not
    /// know the tag, rather than failing to draw a date.
    #[serde(default)]
    pub date_locale: Option<String>,
    /// Whether the AI (`--actor ai`) may perform destructive or concealing project operations
    /// (`project archive` / `delete`). **Off by default**: the AI guardrail refuses archive and
    /// delete, while the reversible operations (add/update/move/unarchive) are not gated. **A local
    /// policy; never synced.** It exists to stop accidents by an honest actor — it is not a security
    /// boundary.
    #[serde(default)]
    pub ai_allow_project_ops: bool,
    /// Whether startup runs the read-only consistency check (the same orphan/dangling-reference
    /// sweep as `doctor`) and surfaces any problem as a warning. **On by default: preventive
    /// detection in production.** It only inspects — it never repairs; repair stays a manual
    /// `amenbo doctor --fix`. It can be turned off as an escape hatch when the cost is noticeable on
    /// a large store. **A local setting; never synced.**
    #[serde(default = "default_true")]
    pub startup_integrity_check: bool,
    /// Whether to query the update endpoint and thereby notice that a newer release exists.
    /// **On by default**: noticing a new release is a plain user benefit. Turning it off suppresses
    /// even this infrastructure query — the privacy escape hatch. The query is **infrastructure
    /// traffic only**; the core guarantee of zero functional traffic (no user data ever leaves the
    /// machine) is untouched. The env var `AMENBO_UPDATE_CHECK=0` overrides the config and disables
    /// it outright — a hard kill switch for CI. The query has a timeout, fails silently, and caches
    /// its result ([`crate::update_check`]). **A local setting; never synced.**
    #[serde(default = "default_true")]
    pub update_check: bool,
    /// Whether the GUI is registered to start when the user logs in to the OS (`AMB-D-541`).
    /// **Off by default**: a program that appears uninvited is one the user did not ask for, so the
    /// switch is theirs to throw. On means the registration was written to the per-user place the OS
    /// reads (a LaunchAgent plist, the `HKCU` Run key, a desktop entry); off means it was removed.
    /// **A local setting; never synced** — it names an executable path on this machine.
    ///
    /// There is no `config set` key for it, and the reason is that this field alone is not the state:
    /// the OS registration is. Only the GUI can write that (the registration goes through a Tauri
    /// plugin), so a value set from the CLI would claim a registration nothing wrote. The one face
    /// that moves it is the GUI's switch, which writes both halves — and a development build has
    /// neither the switch nor a registration (`AMB-D-547`).
    #[serde(default)]
    pub autostart: bool,
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
    /// **May amenbo have this machine's scheduler wake it once an hour?** — asked once, for the tick as
    /// a feature, and never again ([`crate::tick`]). `None` is the unanswered state, the same shape as
    /// [`Config::hook_consent`] and for the same reason. It lives here because the device is the only
    /// scale it has: one machine holds one timer, so there is nothing narrower to record it against.
    /// **A user-level setting; never synced.**
    ///
    /// There is no `config set` key for it. What moves it is the faces that state an intent — `tick
    /// install` and `tick uninstall` — and the startup pass, which only ever takes a `yes` back to
    /// unanswered after the user has removed the registration themselves (`AMB-D-718`).
    #[serde(default)]
    pub tick_consent: Option<crate::tick::TickConsent>,
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
        ));
    }
    if !value.starts_with("data:image/") {
        return Err(crate::error::Error::invalid(format!("{key} must be a data:image/… URL")));
    }
    Ok(())
}

/// Parse the value of `config set attachment.*`: a byte count, as a non-negative decimal integer.
fn parse_bytes(key: &str, value: &str) -> crate::error::Result<u64> {
    value.trim().parse::<u64>().map_err(|_| {
        crate::error::Error::invalid(format!("{key} must be a non-negative byte count; '{value}' is invalid"))
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
            startup_integrity_check: true,
            update_check: true,
            autostart: false,
            perf_log: None,
            attachment_limits: crate::blob::CapacityPolicy::default(),
            human_name: None,
            ai_name: None,
            human_avatar: None,
            ai_avatar: None,
            hook_consent: None,
            tick_consent: None,
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

/// **The languages amenbo is read in** (`AMB-D-394`) — Tier 2 and up, spelled the way every document
/// keyed by one spells it. `en` leads because it is where everything falls back to: an unset setting,
/// a code from outside this list, and a value nobody translated all end there.
///
/// The list is data rather than a match arm because two roads ask it a question a `match` cannot
/// answer: which codes exist at all. A plugin's translation overlay is named by its code
/// (`plugins/<name>.<lang>.yaml`, `AMB-D-621`) and refused when the code is not one of these
/// ([`crate::plugin_validate::validate_overlays`]), and the GUI carries the same list as `LANGS`
/// (`app/src/core/i18n/lang.ts`) — a code amenbo accepted that the GUI cannot read would be a
/// translation nobody ever sees.
///
/// Chinese and Portuguese are carried by script and region rather than by language alone: Simplified
/// and Traditional are separate writing systems and Brazilian Portuguese a separate vocabulary, so
/// each is its own code rather than one narrowed later.
pub const LANGUAGES: [&str; 19] = [
    "en", "ja", "zh-Hans", "zh-Hant", "ko", "es", "pt-BR", "fr", "de", "it", "ru", "hi", "id", "vi",
    "th", "tr", "pl", "nl", "uk",
];

/// Turn a language code (`ja`, …) into its English name, for both AI and human readers; an unknown
/// code is returned as-is. Used when embedding the language into AGENTS.md.
///
/// Every supported code has a name here, because the label is what tells an AI which language to
/// write in — a code that falls through reaches the reader as `Communicate … in hi`. Chinese and
/// Portuguese are named down to the script / region subtag: the label picks between writing systems
/// (`zh-Hans` vs `zh-Hant`) and between vocabularies (`pt-BR` vs European Portuguese), which a
/// reader cannot re-derive from `Chinese` alone. A code carrying no such subtag (`zh`, `pt`) gets
/// the plain name instead, since there is nothing to narrow it by. Subtags are matched
/// case-insensitively, as BCP 47 defines them.
pub fn language_label(code: &str) -> String {
    let mut subtags = code.split(['-', '_']);
    let primary = subtags.next().unwrap_or(code).to_ascii_lowercase();
    let secondary = subtags.next().unwrap_or_default().to_ascii_lowercase();
    match (primary.as_str(), secondary.as_str()) {
        ("ja", _) => "Japanese",
        ("en", _) => "English",
        ("zh", "hans") => "Simplified Chinese",
        ("zh", "hant") => "Traditional Chinese",
        ("zh", _) => "Chinese",
        ("ko", _) => "Korean",
        ("es", _) => "Spanish",
        ("pt", "br") => "Brazilian Portuguese",
        ("pt", _) => "Portuguese",
        ("fr", _) => "French",
        ("de", _) => "German",
        ("it", _) => "Italian",
        ("ru", _) => "Russian",
        ("hi", _) => "Hindi",
        ("id", _) => "Indonesian",
        ("vi", _) => "Vietnamese",
        ("th", _) => "Thai",
        ("tr", _) => "Turkish",
        ("pl", _) => "Polish",
        ("nl", _) => "Dutch",
        ("uk", _) => "Ukrainian",
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
                        ))
                    }
                };
            }
            "language" => {
                self.language = Some(value.to_string());
            }
            // An empty string clears it, restoring the language-linked default — the same "empty
            // means take the override away" the display names and avatars below use.
            "date_locale" => {
                let trimmed = value.trim();
                self.date_locale = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
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
                    format!("unknown config key '{other}' (known: default_view / language / date_locale / human_name / ai_name / human_avatar / ai_avatar / ai_allow_project_ops / startup_integrity_check / update_check / perf_log / attachment.image_max / attachment.audio_max / attachment.video_max / attachment.document_max / attachment.other_max)"),
                ))
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every supported language code reaches the managed block as a name, never as the code itself
    /// — the label is the whole instruction an AI gets about which language to write in.
    #[test]
    fn every_supported_language_code_has_a_name() {
        let mut names = std::collections::HashSet::new();
        for code in LANGUAGES {
            let label = language_label(code);
            assert_ne!(label, code, "{code} must resolve to a name, not fall through");
            assert!(names.insert(label.clone()), "{code} shares the name {label} with another code");
        }
    }

    /// The list is what other roads key documents by — a plugin's overlay file, the GUI's dictionaries
    /// — so a code written twice, or an `en` that stopped leading the fallback, is a fault here rather
    /// than a puzzle wherever it is read.
    #[test]
    fn the_supported_languages_are_distinct_and_fall_back_to_english() {
        let mut seen = std::collections::HashSet::new();
        for code in LANGUAGES {
            assert!(seen.insert(code), "{code} is listed twice");
        }
        assert_eq!(LANGUAGES[0], "en", "everything falls back to English, so it leads the list");
    }

    /// The two codes that carry a script or region subtag name it, because the label is what picks
    /// the writing system and the vocabulary; the bare code gets the plain name.
    #[test]
    fn a_script_or_region_subtag_narrows_the_name() {
        assert_eq!(language_label("zh-Hans"), "Simplified Chinese");
        assert_eq!(language_label("zh-Hant"), "Traditional Chinese");
        assert_eq!(language_label("zh"), "Chinese");
        assert_eq!(language_label("pt-BR"), "Brazilian Portuguese");
        assert_eq!(language_label("pt"), "Portuguese");

        // Subtags are case-insensitive, and `_` separates as `-` does.
        assert_eq!(language_label("ZH_hant"), "Traditional Chinese");
        // A region we do not name falls back to the language's plain name, never to the code.
        assert_eq!(language_label("de-AT"), "German");
        // An unsupported language is returned as-is.
        assert_eq!(language_label("xx"), "xx");
    }

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

    /// `date_locale` is stored as written, and cleared by an empty value — the same "empty takes the
    /// override away" the display names use. It is deliberately **not** judged here: whether a tag is
    /// a usable locale is the formatter's answer, and the GUI falls back to the language's when the
    /// platform does not know it. Refusing a tag this store cannot evaluate would only be guessing.
    #[test]
    fn date_locale_is_stored_as_written_and_cleared_by_an_empty_value() {
        let mut c = Config::default();
        assert!(c.date_locale.is_none(), "unset means the language's own locale");

        c.set("date_locale", "  sv-SE  ").unwrap();
        assert_eq!(c.date_locale.as_deref(), Some("sv-SE"), "stored trimmed");

        c.set("date_locale", "").unwrap();
        assert!(c.date_locale.is_none(), "empty clears it, back to following the language");

        c.set("date_locale", "not a locale").expect("the store does not judge the tag");
        assert_eq!(c.date_locale.as_deref(), Some("not a locale"));
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

    /// The dev channel covers the shared dev build and every throwaway per-task instance, and
    /// nothing else — production least of all, since a channel default that leaked onto it would be
    /// a development behaviour running against real user data.
    #[test]
    fn dev_channel_covers_the_shared_build_and_its_task_instances() {
        for name in ["amenbo-dev", "amenbo-dev-2131", "amenbo-dev-0"] {
            assert!(Paths::is_dev_app_name(name), "{name} is the dev channel");
        }
        for name in ["amenbo", "amenbo-devish", "dev", "amenbo-staging", ""] {
            assert!(!Paths::is_dev_app_name(name), "{name} is not the dev channel");
        }
    }

    /// What there is to type, per channel. The case that matters is the task instance: it reads its
    /// own app-data but installs no CLI, so guidance that named the app-data would send someone to a
    /// command that is not on the machine.
    #[test]
    fn the_command_to_type_is_the_channel_s_cli_not_the_app_data() {
        assert_eq!(Paths::command_name_for("amenbo"), "amenbo");
        assert_eq!(Paths::command_name_for("amenbo-dev"), "amenbo-dev");
        assert_eq!(Paths::command_name_for("amenbo-dev-2134"), "amenbo-dev");
        // Not the dev channel, so production's CLI — the same fallback `APP_NAME` itself takes.
        assert_eq!(Paths::command_name_for("amenbo-devish"), "amenbo");
    }

    /// The file in the bundle keeps its stem whatever extension the platform hangs off it, so the two
    /// cannot drift into naming different files. The word itself is held against the bundle config by
    /// `guards/check-sidecar-name.sh`; what a test can see from in here is only that the pair agree.
    #[test]
    fn the_bundled_file_is_the_sidecar_name_with_this_platform_s_extension() {
        let file = Paths::sidecar_file_name();
        assert_eq!(file.trim_end_matches(".exe"), Paths::SIDECAR_NAME);
        assert_eq!(file.ends_with(".exe"), cfg!(windows), "only Windows carries the extension");
    }

    /// What each channel calls itself on screen. Production says nothing at all — the one case that
    /// would ship a development marker to a user — and a task instance names its task, so a
    /// screenshot of one window is telling apart from a screenshot of the other.
    #[test]
    fn the_badge_names_the_channel_and_production_wears_none() {
        assert_eq!(Paths::dev_badge_for("amenbo"), None);
        assert_eq!(Paths::dev_badge_for("amenbo-dev"), Some("DEV".to_owned()));
        assert_eq!(Paths::dev_badge_for("amenbo-dev-2133"), Some("DEV AMB-T-2133".to_owned()));
        // Not a dev name at all: no badge, for the same reason production has none.
        assert_eq!(Paths::dev_badge_for("amenbo-devish"), None);
        // The build writes digits and nothing else there, so an unreadable suffix is shown, not swallowed.
        assert_eq!(Paths::dev_badge_for("amenbo-dev-wip"), Some("DEV wip".to_owned()));
    }
}
