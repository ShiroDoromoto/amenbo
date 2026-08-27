//! Configuration and file-path resolution.
//!
//! - If the `AMENBO_HOME` environment variable is set, everything lives under that directory
//!   (for tests and explicit overrides).
//! - Otherwise the OS-standard data/config directory (`directories`) is used.
//!
//! Config lives in its own file next to the store (`config.json`). The identity lives in
//! `identity.json` directly under the base dir and **holds no secrets**. The `accounts/P0/` layout
//! written by older builds is lifted to the base dir once, on open ([`lift_legacy_identity`]).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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

/// The commit a development build was made from, and when it was made — substituted at build time,
/// and absent from every build that is not one CI made. They exist for one reader: a member holding
/// two previews of the *same* theme, whose badges are otherwise identical word for word
/// (`AMB-D-732`). The channel and the instance are already in the badge; what is not is which of
/// two bakes of that instance this window is.
///
/// A local build deliberately leaves them unset. The timestamp changes on every invocation, and
/// `option_env!` is a rebuild trigger, so passing it locally would recompile the crate on every
/// build to change a caption nobody is comparing across two windows.
const BUILD_SHA: Option<&str> = option_env!("AMENBO_BUILD_SHA");
const BUILD_TIME: Option<&str> = option_env!("AMENBO_BUILD_TIME");

/// How much of a commit hash the badge carries. Long enough to be a name in this repository, short
/// enough to sit in a header beside two other fields.
const BADGE_SHA_LEN: usize = 8;

/// The commit as the badge shows it, or `None` if what the build was handed is not a hash. Nothing
/// downstream can tell a wrong caption from a right one, so a value that does not look like one is
/// dropped rather than shown.
fn badge_sha(sha: &str) -> Option<String> {
    let sha = sha.trim();
    if sha.len() < BADGE_SHA_LEN || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(sha[..BADGE_SHA_LEN].to_ascii_lowercase())
}

/// The build's instant as the badge shows it — month, day and minute, on the reader's own clock.
///
/// The year is left out because the two builds being told apart are days or weeks apart, and the
/// seconds because two bakes a minute apart are the same build to anyone comparing them. Local
/// rather than UTC for the reason every other instant is (`AMB-D-429`): the reader is comparing it
/// with when they remember installing the other one.
fn badge_built_at(rfc3339: &str) -> Option<String> {
    crate::time::Timestamp::parse_rfc3339(rfc3339.trim())
        .map(|t| t.0.with_timezone(&chrono::Local).format("%m-%d %H:%M").to_string())
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
    /// The volatile area ([`crate::session_work`]): which talk-window session is holding which task,
    /// for as long as that window is running. It sits beside the store for the one reason app-data
    /// does — a directory per identifier, so a dev build and production never read each other's — and
    /// it is nothing like the store otherwise. Nothing in it is true past the run that wrote it, and
    /// the window that owns it empties it as it comes up (`AMB-D-758`).
    pub sessions_dir: PathBuf,
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
            sessions_dir: base.join(crate::session_work::DIR_NAME),
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

    /// What guidance tells a human or an AI to **run** — this build's CLI, worded so that whoever
    /// reads it can reach it. Usually that is this build's own name: the same word as the app-data
    /// ([`APP_NAME`](Self::APP_NAME)) and as the file the bundle carries the CLI under
    /// ([`sidecar_file_name`](Self::sidecar_file_name)) — `amenbo`, `amenbo-dev`, `amenbo-dev-<id>`.
    ///
    /// Every build ships a CLI of its own — a theme's development preview is a whole bundle a team
    /// member installs (`AMB-D-732`), CLI included — and two things break the moment a name is shared
    /// across builds rather than split with them:
    ///
    /// - **On Windows the installer puts this CLI on `PATH`.** One name there means production, the
    ///   shared dev build and every preview a member has taken all answer to `amenbo`, and which of
    ///   them a shell resolves is decided by the order they were installed in — so a member can be
    ///   left typing `amenbo` at an unsigned, unstamped build.
    /// - **Guidance would name the wrong store.** A task instance reads `amenbo-dev-<id>`, and a
    ///   screen of its own telling someone to type `amenbo-dev` sends them to the *shared* dev store:
    ///   an answer that runs and is wrong, which is worse than one that is not found.
    ///
    /// Where no installer does that, the name is not the answer at all and a **path** is — which is
    /// why this is worded rather than merely named: what comes back is what someone can run, so a
    /// surface that hands over a command does not have to know which of the two it got.
    /// [`command_to_run`](Self::command_to_run) is the same answer with the third case kept apart,
    /// and this is that answer with the third case worded as the name anyway.
    ///
    /// Every surface that words a command for someone to run — the managed block's `{CMD}`, the hook
    /// setup notice, `init`'s closing line — takes it from here.
    pub fn command_name() -> &'static str {
        Self::command_to_run().unwrap_or(Self::APP_NAME)
    }

    /// How this build's CLI is reached from a shell, or `None` where nothing on the machine reaches
    /// it. Three answers, and which one a build gets is decided by who puts its CLI anywhere:
    ///
    /// | build | reached as |
    /// |---|---|
    /// | Windows, any channel | the name — the NSIS installer puts it on `PATH` |
    /// | production, any OS | the name — the unified installer puts it on `PATH` |
    /// | the shared dev build | the name — `make install-dev` puts it in `~/.cargo/bin` |
    /// | a task or theme instance on macOS | the path to the copy in the bundle |
    /// | a task or theme instance on Linux | the name, once the member has put that file on `PATH` |
    ///
    /// The last two rows are the same build on two OSes, and they part company because of what a
    /// member is handed (`AMB-D-732`). macOS hands them a `.app` they drag into `/Applications`,
    /// where it stays and the CLI inside it has an address anyone can paste. Linux hands them one
    /// `.AppImage`, whose contents are a squashfs mounted under `/tmp` for as long as the GUI runs
    /// and gone after: a path there is right for a few minutes and wrong for good. So the preview
    /// ships the CLI beside the AppImage as its own file, under the name this build answers to, and
    /// the member copies it somewhere on their `PATH`.
    ///
    /// That copying is theirs to do, so this asks rather than assumes: the name comes back only when
    /// a file by that name is actually reachable, and until then there is still no command. Both
    /// halves matter — naming a command nobody installed sends a reader to `not found` with no idea
    /// why, and staying silent after they installed it hides the one thing they went and got. The
    /// answer is settled once per process, so installing the CLI while the app is open is read on
    /// the next start — the same as the bundle's path on macOS, and the member has just been at a
    /// terminal, which is where they can try it immediately anyway.
    ///
    /// The path is escaped for a shell, not quoted: the bundle's name carries spaces and brackets
    /// (`amenbo (dev 3519).app`), and the wording it lands in is as often inside quotes already —
    /// the hook configuration's `echo '<instruction>'` — as it is on a line of its own.
    pub fn command_to_run() -> Option<&'static str> {
        static WORDED: OnceLock<Option<String>> = OnceLock::new();
        WORDED
            .get_or_init(|| {
                Self::command_to_run_for(
                    Self::APP_NAME,
                    std::env::consts::OS,
                    Self::bundled_cli().as_deref(),
                    || Self::found_on_path(Self::APP_NAME),
                )
            })
            .as_deref()
    }

    /// The rule [`command_to_run`](Self::command_to_run) applies, with all four facts it stands on
    /// said out loud — the build's name, the OS, where the bundle's copy of the CLI is, and whether
    /// a command by that name is reachable from a shell — so a test can stand on an OS, a channel
    /// and a machine other than the ones it is running on.
    ///
    /// The fourth arrives as a question rather than an answer, because it is the one that costs
    /// something to settle: every other build is decided before the Linux branch is reached, and a
    /// walk of `PATH` those builds never look at is a walk none of them should pay for.
    pub(crate) fn command_to_run_for(
        app_name: &str,
        os: &str,
        bundled: Option<&Path>,
        on_path: impl FnOnce() -> bool,
    ) -> Option<String> {
        let installed_on_path =
            os == "windows" || !Self::is_dev_app_name(app_name) || app_name == Self::DEV_APP_NAME;
        if installed_on_path {
            return Some(app_name.to_owned());
        }
        if os == "macos" {
            return bundled.map(Self::shell_escaped);
        }
        // A preview's Linux member installs the CLI by hand, so the answer is whether they did.
        on_path().then(|| app_name.to_owned())
    }

    /// Whether a shell here would find a runnable command by that name — the one fact a Linux
    /// preview's answer turns on, since nothing on that machine installs its CLI but the member.
    ///
    /// The executable bit is part of the question, not a detail: a copy made without it is found by
    /// name and then refused by the kernel, which is the same "runs and is wrong" the naming rule
    /// exists to avoid. Off Unix this never runs — every other build is answered before the Linux
    /// branch is reached — so there the file being there is the whole of it.
    fn found_on_path(name: &str) -> bool {
        let Some(paths) = crate::env::path() else { return false };
        std::env::split_paths(&paths).any(|dir| Self::is_runnable(&dir.join(name)))
    }

    #[cfg(unix)]
    fn is_runnable(path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }

    #[cfg(not(unix))]
    fn is_runnable(path: &Path) -> bool {
        path.is_file()
    }

    /// The copy of this build's CLI the bundle carries, where the running binary is standing beside
    /// it. `None` out of a build tree, where the two are not neighbours and no path would be one a
    /// reader could keep.
    fn bundled_cli() -> Option<PathBuf> {
        let beside = std::env::current_exe().ok()?.parent()?.join(Self::sidecar_file_name());
        beside.is_file().then_some(beside)
    }

    /// A path as a POSIX shell reads it in one word: everything a shell would act on is preceded by a
    /// backslash. Only this branch needs it (Windows is never worded as a path here), and only a
    /// backslash will do — a quoted path would have to survive being pasted into text that is itself
    /// quoted, and the hook configuration's is exactly that.
    fn shell_escaped(path: &Path) -> String {
        path.to_string_lossy()
            .chars()
            .flat_map(|c| {
                let plain = c.is_ascii_alphanumeric() || "/._-+,:=@%".contains(c);
                (!plain).then_some('\\').into_iter().chain(std::iter::once(c))
            })
            .collect()
    }

    /// That same name as the file this platform's bundle actually holds — the thing to look for beside
    /// the running binary when something needs the CLI as a **path** rather than as a command word
    /// (an MCP host is not a shell and has no `PATH` of the reader's to resolve one in).
    ///
    /// Tauri bundles the sidecar under the stem `bundle.externalBin` names, and the build splits that
    /// stem by channel exactly as it splits the bundle identifier, the product name and the GUI
    /// executable (the `Makefile`'s `GUI_DEV_CONFIG`). So the stem, the app-data and
    /// [`command_name`](Self::command_name) are one word, and the only thing this adds is the
    /// extension Windows hangs off it. `guards/check-sidecar-name.sh` holds the build config to it.
    pub fn sidecar_file_name() -> String {
        if cfg!(windows) { format!("{}.exe", Self::APP_NAME) } else { Self::APP_NAME.to_owned() }
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
        Self::dev_badge_with(Self::APP_NAME, BUILD_SHA, BUILD_TIME)
    }

    /// [`dev_badge`](Self::dev_badge) with all three build-time facts said out loud, so a test can
    /// stand somewhere other than the build it is running in.
    ///
    /// The two build fields only ever extend a badge that is already there: production returns
    /// `None` before either is read, which is what keeps a stamp of the commit out of a shipped
    /// window even if a release build were one day handed one.
    pub(crate) fn dev_badge_with(
        name: &str,
        sha: Option<&str>,
        built_at: Option<&str>,
    ) -> Option<String> {
        let mut badge = Self::dev_badge_for(name)?;
        for field in [sha.and_then(badge_sha), built_at.and_then(badge_built_at)]
            .into_iter()
            .flatten()
        {
            badge.push_str(" · ");
            badge.push_str(&field);
        }
        Some(badge)
    }

    /// The labelling rule [`dev_badge`](Self::dev_badge) applies, taking the name as an argument for
    /// the reason [`is_dev_app_name`](Self::is_dev_app_name) does — a running binary's channel is
    /// fixed at compile time, so only a table can pin what each name reads as.
    ///
    /// A task instance carries its number, and it is spelled as the ref the task is known by, not as
    /// the raw app-data suffix: the badge is read next to the task it belongs to. A suffix that is not
    /// a number is shown as it stands — that is a theme's preview, whose suffix is the slug it is
    /// known by (`AMB-THEME`), and the member reading the badge knows the theme under that word.
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
    /// it outright — a hard kill switch for CI, though a build CI produced is unstamped and so does
    /// not ask in the first place ([`crate::update_check::is_disabled`]). The query has a timeout,
    /// fails silently, and caches its result ([`crate::update_check`]). **A local setting; never
    /// synced.**
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
    /// **Where this app ran, the last time the login registration was settled** — the absolute path
    /// of its executable, written by the GUI's reconciliation pass on every start.
    ///
    /// It exists to tell two absences apart that look identical to the OS: a registration the user
    /// took away, and one that went with the app itself when the app moved (`AMB-D-720`). A macOS
    /// `.pkg` replaces the bundle whole, so a rename moves the executable and the login row does not
    /// survive it; reading that as the user having said no would switch [`Config::autostart`] off
    /// under everyone who updated. The comparison against this path is what separates the two.
    ///
    /// Unset means no pass has ever recorded one, which is not a move: the first pass reads an
    /// absence the way `AMB-D-546` always did. **A local setting; never synced** — it names a path on
    /// this machine.
    #[serde(default)]
    pub autostart_exe: Option<String>,
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
    /// **May Amenbo wire its lint into your git hooks?** — asked once, for the lint as a feature, and
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
    /// **May Amenbo have this machine's scheduler wake it once an hour?** — asked once, for the tick as
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
    /// **Which agent a project's panes are opened with**, by project id ([`crate::wake`]). Written
    /// where the person first opened a pane and left unchanged after that — the answer is theirs to
    /// change on the project's own settings, and a pane opened with something else for one turn is
    /// not them changing it.
    ///
    /// **The project is the scale, not the folder.** One project can bind several folders, so a
    /// folder-shaped answer splits into as many answers as the project has folders and the reader
    /// gets a different agent depending which one a pane happens to open in.
    ///
    /// It is the device's all the same: what can be started is what is installed on this machine,
    /// and the same project on a second machine may have nothing of the sort on it. **Never
    /// synced**, for the same reason — which is also why the store's own project row is the wrong
    /// place for it.
    ///
    /// A row whose agent has since gone is left alone rather than pruned. It costs a line, and the
    /// day the tool comes back is the day the project's answer is right again — [`crate::wake::settle`]
    /// already ignores one that does not hold.
    #[serde(default)]
    pub project_agent: std::collections::BTreeMap<String, String>,
    /// **The last thing this person opened a pane with** ([`crate::wake`]) — the rank under the
    /// project's own answer, and what makes a choice made once hold in every project after it.
    ///
    /// **The person is the scale, not the project.** Which agent somebody works with is their own
    /// habit rather than a property of a directory, so a project-shaped answer alone would ask the
    /// question again on every project they ever make — an endless friction for the reader who works
    /// with one agent everywhere. What a project keeps ([`Config::project_agent`]) is the answer
    /// somebody pinned on purpose, and it still wins.
    ///
    /// It holds [`crate::wake::SHELL`] as well as a catalogued agent id: "I opened a plain prompt
    /// last time" is an answer to *this* question, though it is not one to the project's.
    ///
    /// A value whose tool has since gone is left alone rather than pruned, the same as a project's —
    /// [`crate::wake::settle`] already passes over one that does not hold. **A user-level setting;
    /// never synced**, because what can be started is a fact about this machine.
    #[serde(default)]
    pub last_agent: Option<String>,
    /// **The commands this person registered themselves** ([`CustomAgent`], `AMB-D-794`), in the
    /// order they were added — the rows a face offers after the catalog's.
    ///
    /// The catalog is a shortcut, not a census: this field is what covers the tool it does not
    /// list, and the tool it lists under flags the reader does not want.
    ///
    /// **A user-level setting; never synced**, and deliberately not in the store. What a registered
    /// line does is run in a terminal on this machine, so carrying it to somebody else's would be
    /// carrying a command they never wrote — which is also why registering one asks for no
    /// permission Amenbo did not already have: the pane is a real terminal (`AMB-D-747`), and what
    /// can be written here is what the reader can already type into it.
    #[serde(default)]
    pub custom_agents: Vec<CustomAgent>,
    /// How many commands have ever been registered on this device — the counter [`Config::register_agent`]
    /// takes the next id off.
    ///
    /// **Ids are never reused**, which is why this is kept rather than read off the rows. Numbering
    /// from the highest row would hand a deleted command's id to the next one registered, and an id
    /// is what a project's answer and this person's habit are written down as
    /// ([`Config::project_agent`], [`Config::last_agent`]): the new command would inherit the old
    /// one's place without anybody choosing it.
    #[serde(default)]
    pub custom_agent_seq: u64,
}

/// One command the reader registered themselves — a row in [`Config::custom_agents`] (`AMB-D-794`).
///
/// It stands beside a catalog row ([`crate::harness::Launch`]) wherever an agent is offered, and
/// differs from one in what it is allowed to hold: a catalog row names a bare program, and this
/// names a **whole command line**, arguments and all. That is the point of it — `claude --model opus`
/// is an ordinary thing to want, and telling somebody to write a wrapper script is not an answer.
///
/// **The line is never taken apart.** It is handed to the pane's shell as written, because Amenbo
/// does not know where in it an opening instruction would go — which is why what is registered is
/// always spoken to in two stages instead (`AMB-D-793`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CustomAgent {
    /// The token a face names this row by, `custom:` and a number ([`CUSTOM_PREFIX`]).
    ///
    /// It is a token rather than the line itself for the same reason a catalog row has one: what
    /// crosses the command seam from the webview is an id, and the command it stands for is looked
    /// up on this side (`app/src-tauri/src/wake.rs`). An id that arrived as a command line would be
    /// a shell with a webview at the other end of it.
    pub id: String,
    /// What the reader calls it, as they wrote it — what a face draws in the row.
    pub label: String,
    /// The command line to start, as the reader wrote it.
    pub line: String,
}

/// The front every registered id is spelled with, which is what tells one from a catalog id
/// ([`crate::harness::Launch::id`]). A catalog id is a product's own token and holds no colon.
pub const CUSTOM_PREFIX: &str = "custom:";

impl CustomAgent {
    /// The word this row is looked for on the `PATH` by: the first word of the line.
    ///
    /// **Only the first word can be judged**, and that is the whole of what is claimed about a
    /// registered row. Whether `--model` is a flag this build of that program still takes is not
    /// knowable until it runs, which is the same line drawn everywhere else here (`crate::wake`).
    pub fn command(&self) -> &str {
        self.line.split_whitespace().next().unwrap_or("")
    }
}

/// The two halves of a registered command, trimmed — or the refusal for a half that is empty.
///
/// Written once because both doors ask it: registering a row and correcting one are the same
/// question about the same two strings.
fn named(label: &str, line: &str) -> Result<(String, String)> {
    let label = label.trim();
    let line = line.trim();
    if label.is_empty() {
        return Err(crate::error::Error::invalid("a registered command needs a name"));
    }
    if line.is_empty() {
        return Err(crate::error::Error::invalid("a registered command needs a command line"));
    }
    Ok((label.to_string(), line.to_string()))
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
            autostart_exe: None,
            perf_log: None,
            attachment_limits: crate::blob::CapacityPolicy::default(),
            human_name: None,
            ai_name: None,
            human_avatar: None,
            ai_avatar: None,
            hook_consent: None,
            tick_consent: None,
            project_agent: Default::default(),
            last_agent: None,
            custom_agents: Vec::new(),
            custom_agent_seq: 0,
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

/// **The languages Amenbo is read in** (`AMB-D-394`) — Tier 2 and up, spelled the way every document
/// keyed by one spells it. `en` leads because it is where everything falls back to: an unset setting,
/// a code from outside this list, and a value nobody translated all end there.
///
/// The list is data rather than a match arm because two roads ask it a question a `match` cannot
/// answer: which codes exist at all. A plugin's translation overlay is named by its code
/// (`plugins/<name>.<lang>.yaml`, `AMB-D-621`) and refused when the code is not one of these
/// ([`crate::plugin_validate::validate_overlays`]), and the GUI carries the same list as `LANGS`
/// (`app/src/core/i18n/lang.ts`) — a code Amenbo accepted that the GUI cannot read would be a
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

    /// Which agent this project's panes are opened with, where one has been settled
    /// ([`Config::project_agent`]).
    pub fn agent_for(&self, project: i64) -> Option<&str> {
        self.project_agent.get(&project.to_string()).map(String::as_str)
    }

    /// Keep this project's answer, replacing any earlier one — what the project's settings write,
    /// and what the first pane opened in a project leaves behind ([`crate::wake`]).
    pub fn remember_agent(&mut self, project: i64, id: &str) {
        self.project_agent.insert(project.to_string(), id.to_string());
    }

    /// Forget this project's answer, so the next pane opened in it settles one again — what
    /// clearing the choice on the project's settings leaves behind.
    pub fn forget_agent(&mut self, project: i64) {
        self.project_agent.remove(&project.to_string());
    }

    /// What this person last opened a pane with, where they have opened one
    /// ([`Config::last_agent`]).
    pub fn last_agent(&self) -> Option<&str> {
        self.last_agent.as_deref()
    }

    /// Keep what was just opened with as this person's own answer, replacing whatever they opened
    /// with before — what every press that chooses one leaves behind ([`crate::wake`]).
    pub fn remember_last_agent(&mut self, id: &str) {
        self.last_agent = Some(id.to_string());
    }

    /// The commands this person registered, in the order they registered them.
    pub fn custom_agents(&self) -> &[CustomAgent] {
        &self.custom_agents
    }

    /// The registered command an id names, or `None` where nothing is registered under it — the
    /// lookup that turns what a webview says into what a shell is handed.
    pub fn custom_agent(&self, id: &str) -> Option<&CustomAgent> {
        self.custom_agents.iter().find(|one| one.id == id)
    }

    /// Register a command, and answer with the row it became — the id in it is what a face hands
    /// back to open a pane with.
    ///
    /// Both halves are trimmed and neither may be empty: a row with no name cannot be drawn, and a
    /// row with no line cannot be started. Nothing else is judged. What is written here runs in a
    /// terminal the reader already has (`AMB-D-794`), so a guard on its spelling would be Amenbo
    /// deciding which of their own commands they are allowed to name.
    pub fn register_agent(&mut self, label: &str, line: &str) -> Result<&CustomAgent> {
        let (label, line) = named(label, line)?;
        self.custom_agent_seq += 1;
        self.custom_agents.push(CustomAgent {
            id: format!("{CUSTOM_PREFIX}{}", self.custom_agent_seq),
            label,
            line,
        });
        Ok(self.custom_agents.last().expect("the row just pushed"))
    }

    /// Rewrite a registered command in place, keeping its id — so a row that is corrected stays the
    /// one this project pinned and this person last opened with.
    pub fn amend_agent(&mut self, id: &str, label: &str, line: &str) -> Result<()> {
        let (label, line) = named(label, line)?;
        let row = self
            .custom_agents
            .iter_mut()
            .find(|one| one.id == id)
            .ok_or_else(|| crate::error::Error::invalid(format!("nothing is registered as '{id}'")))?;
        row.label = label;
        row.line = line;
        Ok(())
    }

    /// Drop a registered command, answering whether there was one. The id is left wherever it was
    /// written down: [`crate::wake::settle`] passes over an answer that no longer holds, the same as
    /// it does for a catalogued tool that has been uninstalled.
    pub fn forget_custom_agent(&mut self, id: &str) -> bool {
        let before = self.custom_agents.len();
        self.custom_agents.retain(|one| one.id != id);
        self.custom_agents.len() != before
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

    /// One name per build, all the way through: what to type, what the bundle carries and what the
    /// app-data is called are the same word. A test compiles on the production channel, so what it
    /// can see from in here is that the three answers agree — the split itself is the build's
    /// (`Makefile`, `GUI_DEV_CONFIG`) and `guards/check-sidecar-name.sh` is what holds that.
    #[test]
    fn the_command_to_type_is_this_build_s_own_name() {
        assert_eq!(Paths::command_name(), Paths::APP_NAME);
        assert_eq!(Paths::sidecar_file_name().trim_end_matches(".exe"), Paths::command_name());
        assert_eq!(
            Paths::sidecar_file_name().ends_with(".exe"),
            cfg!(windows),
            "only Windows carries the extension"
        );
    }

    /// Who puts a build's CLI within reach, per build and per OS. The two rows that matter are the
    /// last two: they are one build on two machines, and only one of them has an address that
    /// outlives the run.
    #[test]
    fn a_build_is_reached_by_name_where_an_installer_puts_it_there_and_by_path_where_none_does() {
        let bundled = std::path::Path::new("/Applications/amenbo (dev 3519).app/Contents/MacOS/amenbo-dev-3519");
        // Nothing is on this imaginary machine's PATH unless a case says so.
        let reach = |name: &str, os: &str| Paths::command_to_run_for(name, os, Some(bundled), || false);

        // Windows installs every channel's CLI on PATH, so the name is what there is to type.
        assert_eq!(reach("amenbo", "windows").as_deref(), Some("amenbo"));
        assert_eq!(reach("amenbo-dev-3519", "windows").as_deref(), Some("amenbo-dev-3519"));
        // So does the unified installer for production, and `make install-dev` for the shared build.
        assert_eq!(reach("amenbo", "macos").as_deref(), Some("amenbo"));
        assert_eq!(reach("amenbo-dev", "linux").as_deref(), Some("amenbo-dev"));
        // A preview on macOS is dragged into /Applications and stays there, so it has an address.
        assert_eq!(
            reach("amenbo-dev-3519", "macos").as_deref(),
            Some(r"/Applications/amenbo\ \(dev\ 3519\).app/Contents/MacOS/amenbo-dev-3519"),
        );
        // The same preview on Linux is an AppImage: what is inside it is mounted for the length of
        // the run and gone after. The CLI ships beside it as its own file, so the answer is whether
        // the member has put that file somewhere a shell finds it — and until they do, there is
        // still nothing to hand over.
        assert_eq!(reach("amenbo-dev-3519", "linux"), None);
        assert_eq!(
            Paths::command_to_run_for("amenbo-dev-3519", "linux", Some(bundled), || true).as_deref(),
            Some("amenbo-dev-3519"),
        );
        // Not the bundle's copy, even where one is beside it: on Linux that path outlives nothing.
        assert!(
            !Paths::command_to_run_for("amenbo-dev-3519", "linux", Some(bundled), || true)
                .is_some_and(|c| c.contains('/')),
            "a Linux preview is worded as the name, never as a path",
        );
    }

    /// What `found_on_path` is actually asking. A file by the right name is not the answer — a copy
    /// made without the executable bit is found and then refused by the kernel, which is the "runs
    /// and is wrong" the naming rule exists to avoid.
    #[cfg(unix)]
    #[test]
    fn a_command_is_on_path_only_once_it_can_actually_run() {
        use std::os::unix::fs::PermissionsExt;
        let dir = amenbo_scratch::scratch("cli-on-path");
        let name = "amenbo-dev-3519";
        let file = dir.join(name);
        std::fs::write(&file, "#!/bin/sh\n").unwrap();

        assert!(!Paths::is_runnable(&file), "a copy nobody made executable is not a command");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(Paths::is_runnable(&file), "and once it is, it is");
        assert!(!Paths::is_runnable(&dir), "a directory of that name is not one either");
        assert!(!Paths::is_runnable(&dir.join("nothing-here")));
    }

    /// Out of a build tree the bundle's copy is not beside the running binary, and a macOS preview
    /// has no address either — the wording falls back to the name, which is what
    /// [`Paths::command_name`] answers with wherever there is nothing better.
    #[test]
    fn a_preview_with_no_bundle_beside_it_is_worded_as_the_name() {
        assert_eq!(Paths::command_to_run_for("amenbo-dev-3519", "macos", None, || false), None);
        let own = "a test build is production, which is always its own name";
        assert_eq!(Paths::command_name(), Paths::APP_NAME, "{own}");
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
        // A theme's preview carries its slug there, and the member knows the theme under that word,
        // so the suffix is shown as it stands rather than read as a task that does not exist.
        assert_eq!(
            Paths::dev_badge_for("amenbo-dev-color-rework"),
            Some("DEV color-rework".to_owned())
        );
    }

    /// The two bakes of one theme a member is holding differ in nothing the badge said before, so
    /// the commit and the minute are what tell them apart. Production is checked here too: the
    /// build fields must not be able to put a commit into a shipped window.
    #[test]
    fn the_badge_carries_the_commit_and_the_minute_a_preview_was_baked() {
        let sha = Some("7901f2b9c0deadbeef0000000000000000000000");
        let at = Some("2026-08-22T07:36:11Z");
        let badge = Paths::dev_badge_with("amenbo-dev-3493", sha, at).expect("a dev build has one");
        assert!(badge.starts_with("DEV AMB-T-3493 · 7901f2b9 · "), "{badge}");
        // Local, so the minute itself moves with the machine; its shape does not.
        let minute = badge.rsplit(" · ").next().expect("the last field is the minute");
        assert_eq!(minute.len(), "08-22 07:36".len(), "{badge}");

        // Production reads neither field.
        assert_eq!(Paths::dev_badge_with("amenbo", sha, at), None);

        // A build handed nothing wears the badge it always did.
        assert_eq!(
            Paths::dev_badge_with("amenbo-dev", None, None),
            Some("DEV".to_owned())
        );
    }

    /// Nothing downstream can tell a wrong caption from a right one, so anything that is not a hash
    /// or an instant is dropped rather than shown.
    #[test]
    fn a_build_field_that_is_not_what_it_claims_is_dropped_not_shown() {
        // Too short to be a name in this repository, and not hex at all.
        assert_eq!(badge_sha("7901f2"), None);
        assert_eq!(badge_sha("not-a-sha-at-all"), None);
        assert_eq!(badge_sha(""), None);
        // Abbreviated on the way in is fine, as long as there is enough of it.
        assert_eq!(badge_sha("7901F2B9C0"), Some("7901f2b9".to_owned()));

        assert_eq!(badge_built_at("yesterday"), None);
        assert_eq!(badge_built_at(""), None);
        assert!(badge_built_at("2026-08-22T07:36:11Z").is_some());

        // Dropped one at a time: an unreadable minute does not cost the commit its place.
        assert_eq!(
            Paths::dev_badge_with("amenbo-dev", Some("7901f2b9"), Some("nonsense")),
            Some("DEV · 7901f2b9".to_owned())
        );
    }

    /// A registered command keeps the line as written and is looked for by its first word — which
    /// is the whole point of allowing arguments (`AMB-D-794`).
    #[test]
    fn a_registered_command_keeps_its_line_and_is_looked_for_by_its_first_word() {
        let mut config = Config::default();
        let id = config.register_agent("  Mine  ", "  mine --model big  ").unwrap().id.clone();
        let row = config.custom_agent(&id).expect("the row just registered");
        // Trimmed at the edges and untouched in the middle: the line is the reader's, not ours.
        assert_eq!(row.label, "Mine");
        assert_eq!(row.line, "mine --model big");
        assert_eq!(row.command(), "mine");
    }

    /// Both halves are needed and nothing else is judged. A name that cannot be drawn and a line
    /// that cannot be started are the two ways a registration is not one.
    #[test]
    fn a_registration_needs_a_name_and_a_line_and_nothing_more() {
        let mut config = Config::default();
        assert!(config.register_agent("   ", "mine").is_err());
        assert!(config.register_agent("Mine", "   ").is_err());
        // Anything the reader could type into the pane's own shell registers, because that is
        // exactly what it is going to be.
        assert!(config.register_agent("Mine", "mine 'a b' | tee log && echo $HOME").is_ok());
    }

    /// Correcting a row keeps its id, so the answer somebody pinned survives a typo being fixed.
    #[test]
    fn correcting_a_registration_keeps_the_id_it_was_pinned_under() {
        let mut config = Config::default();
        let id = config.register_agent("Mine", "mien").unwrap().id.clone();
        config.remember_agent(7, &id);
        config.amend_agent(&id, "Mine", "mine").unwrap();
        assert_eq!(config.custom_agent(&id).map(|one| one.line.as_str()), Some("mine"));
        assert_eq!(config.agent_for(7), Some(id.as_str()));
        assert!(config.amend_agent("custom:99", "Mine", "mine").is_err());
    }

    /// **Ids are never reused.** A new registration after a deleted one would otherwise inherit the
    /// place the deleted one held — this project's pin, and this person's habit — without anybody
    /// having chosen it.
    #[test]
    fn a_deleted_registrations_id_is_never_handed_to_another() {
        let mut config = Config::default();
        let first = config.register_agent("One", "one").unwrap().id.clone();
        let second = config.register_agent("Two", "two").unwrap().id.clone();
        assert_ne!(first, second);
        assert!(config.forget_custom_agent(&second));
        assert!(!config.forget_custom_agent(&second), "there is nothing left to drop");
        let third = config.register_agent("Three", "three").unwrap().id.clone();
        assert_ne!(third, second);
        assert_ne!(third, first);
        assert_eq!(config.custom_agents().len(), 2);
    }

    /// Registrations survive a round trip through the file, which is where they live: a row written
    /// on one run is the row a later one starts.
    #[test]
    fn the_registered_commands_are_written_down_and_read_back() {
        let mut config = Config::default();
        config.register_agent("Mine", "mine --model big").unwrap();
        let text = serde_json::to_string(&config).unwrap();
        let back: Config = serde_json::from_str(&text).unwrap();
        assert_eq!(back.custom_agents(), config.custom_agents());
        assert_eq!(back.custom_agent_seq, config.custom_agent_seq);

        // And a settings file written before any of this existed reads as one with nothing
        // registered, rather than failing to read at all and taking the whole config down with it.
        let mut older: serde_json::Value = serde_json::from_str(&text).unwrap();
        let map = older.as_object_mut().unwrap();
        map.remove("custom_agents");
        map.remove("custom_agent_seq");
        let older: Config = serde_json::from_value(older).unwrap();
        assert!(older.custom_agents().is_empty());
        assert_eq!(older.custom_agent_seq, 0);
    }
}
