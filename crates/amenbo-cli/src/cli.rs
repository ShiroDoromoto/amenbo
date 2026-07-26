//! Command definitions, via clap.
//! The single source of truth for the command spec is `agent.rs` (`amenbo agent --json`); this file is kept in step with it.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "amenbo",
    version,
    about = "Local-first, server-less task management (CLI-first, AI-agent ready)",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// machine-readable JSON output (for AI)
    #[arg(long, global = true)]
    pub json: bool,
    /// skip confirmation for destructive operations (non-interactive)
    #[arg(long, short = 'y', global = true)]
    pub yes: bool,
    /// deprecated: accepted but ignored — a store owns a single workspace
    #[arg(long, global = true, hide = true)]
    pub workspace: Option<String>,
    /// suppress the human-facing success message
    #[arg(long, global = true)]
    pub quiet: bool,
    /// disable color
    #[arg(long, global = true)]
    pub no_color: bool,
    /// facet of this operation (human / ai). AI agents pass ai. Required by every operation that uses the
    /// facet — the writes that stamp it, and the reads that draw an AI's reach from it. Never defaulted
    #[arg(long, global = true)]
    pub actor: Option<String>,

    /// operate within a specific project (name or id) — overrides the bound project context used for
    /// ref resolution and defaults. Place before the subcommand: `amenbo --project <name> decision add …`.
    /// Explicit override: `--project` > `.amenbo` (CWD) > error, with no silent guessing
    #[arg(long, value_name = "NAME_OR_ID")]
    pub project: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Present how to work here — the workflow and rules in full, plus an index of the commands (the
    /// AI's entry point). Pull one command's full spec with `--command <name>`; `--full` prints them
    /// all.
    Agent {
        /// Print one command's full spec (flags, args, examples) instead of the entry point
        #[arg(long, value_name = "NAME")]
        command: Option<String>,
        /// Print every command's full spec inline, instead of an index (scripts / verification)
        #[arg(long)]
        full: bool,
    },
    /// Version information
    Version,
    /// Update amenbo to the latest release. By default this opens this OS's one-piece installer
    /// (resolved from the published latest.json, falling back to the releases page) in your browser.
    /// Pass --apply to self-update the standalone CLI in place instead — download the new CLI archive
    /// over TLS and swap this binary, no installer, no elevation (CLI-only installs; a GUI-managed CLI
    /// is updated from the desktop app). Pass --rollback to undo the last --apply, restoring the binary
    /// it kept aside (offline, no download). Pass --print to only print the installer URL (no browser).
    Update {
        /// print the installer URL instead of opening a browser (headless / scripted use)
        #[arg(long)]
        print: bool,
        /// self-update the standalone CLI in place (download + swap this binary) instead of opening the installer
        #[arg(long, conflicts_with = "print")]
        apply: bool,
        /// undo the last --apply, restoring the previous binary kept beside this one (offline, no download)
        #[arg(long, conflicts_with_all = ["print", "apply"])]
        rollback: bool,
    },
    /// Show / change configuration
    Config {
        #[command(subcommand)]
        sub: Option<ConfigCmd>,
    },
    /// Show this store's identity (display name / hardware-copy check)
    Whoami,
    /// Initialize a folder so an AI launched there may operate amenbo (it does not read or write the
    /// project's contents — source or files). The store itself lives in app-data; only the `.amenbo`
    /// pointer and AGENTS.md (the AI guide) are placed in the folder
    Init {
        #[arg(long)]
        name: Option<String>,
        /// user language (e.g. ja / en). Sets it in the global config and embeds it in the AGENTS.md directive
        #[arg(long)]
        language: Option<String>,
        /// create a new store and overwrite even if a `.amenbo` pointer already exists (default rejects
        /// it = prevents clobbering the production pointer; use `bind` to re-bind to an existing store).
        /// It does not reach a git worktree cut inside a managed tree: amenbo is refused there, and the
        /// project this would raise outlives the checkout that asked for it
        #[arg(long)]
        force: bool,
    },
    /// Allow an AI launched in this folder to operate an existing project/store (it does not touch the
    /// contents; it just places a `.amenbo` pointer). Shows the current binding when omitted
    Bind {
        /// project to bind (name or ID). Shows the current binding when omitted
        #[arg(long)]
        project: Option<String>,
        /// bind another folder instead of the current directory: place the `.amenbo` pointer in that
        /// directory (which must exist) rather than CWD. Lets you link a folder from outside it (git -C style)
        #[arg(long)]
        dir: Option<String>,
        /// bind even when this folder is already inside an amenbo-managed tree (a parent has a
        /// `.amenbo`). Off by default so a stray bind in a source subdirectory cannot shadow the
        /// root pointer (and scatter `.amenbo`/AGENTS.md/CLAUDE.md there). It does not reach a git
        /// worktree cut inside that tree: amenbo is refused there whatever pointer it holds, so
        /// binding one could only write a pointer nothing would read
        #[arg(long)]
        force: bool,
    },

    /// Remove this folder's `.amenbo` binding (and amenbo's managed blocks in AGENTS.md/CLAUDE.md),
    /// keeping the store itself. Many-to-one: only this folder's pointer is removed; other folders
    /// bound to the same store are untouched. Use --dir to unbind another folder
    Unbind {
        /// folder to unbind (defaults to the current directory)
        #[arg(long)]
        dir: Option<String>,
    },

    /// Re-sync amenbo's managed guidance block in bound folders to this binary's current version. A folder
    /// follows on its own the moment you run amenbo in it, so this is for the folders you have not been in
    /// (and for a block amenbo could not write). Idempotent and low-churn: a folder's CLAUDE.md/AGENTS.md is
    /// rewritten only when its managed block actually changed, each folder's own language label is preserved,
    /// and your content outside the markers is untouched. Targets every locally bound folder by
    /// default; pass --dir to resync just one
    SyncGuide {
        /// resync just this folder (defaults to every locally bound folder)
        #[arg(long)]
        dir: Option<String>,
    },

    /// Summary of what to do now (overdue / today / in progress)
    Status {
        #[arg(long, default_value = "today")]
        scope: String,
    },
    /// Show activity (system events + comments), newest first (humans and the AI read the same stream)
    Activity {
        /// only this task's activity
        #[arg(long)]
        task: Option<String>,
        /// only activity for tasks belonging to this project
        #[arg(long)]
        project: Option<String>,
        /// on or after this date (today / tomorrow / +3d / YYYY-MM-DD), or an opaque cursor from a
        /// previous response (returns only what is strictly newer, oldest-first — an AI's incremental watch)
        #[arg(long)]
        since: Option<String>,
        /// filter by kind: system / comment
        #[arg(long)]
        kind: Option<String>,
        /// filter by the issuer's facet: human / ai (separate from the global --actor; a read filter)
        #[arg(long)]
        by: Option<String>,
        /// scope to what this facet should act on: me / human / ai (destination axis — a task assigned to
        /// that facet; separate from --by which filters by the issuer)
        #[arg(long = "for")]
        for_scope: Option<String>,
        /// max count (newest first)
        #[arg(long)]
        limit: Option<usize>,
        /// number of items to skip, newest first (paging / going back through history)
        #[arg(long)]
        offset: Option<usize>,
    },
    /// Data integrity check (orphan references, broken ordering, key-ledger tampering, etc.)
    Doctor {
        /// repair fixable problems (reclaim unreferenced attachment files; forget folder bindings no live project claims) - all non-destructive
        #[arg(long)]
        fix: bool,
    },
    /// Shape-check the given tasks (all data when omitted)
    Validate { ids: Vec<String> },

    /// Projects
    Project {
        #[command(subcommand)]
        sub: ProjectCmd,
    },
    /// Dimensions (user-defined classification axes: an axis, its values, and task assignments)
    Dimension {
        #[command(subcommand)]
        sub: DimensionCmd,
    },
    /// Tasks
    Task {
        #[command(subcommand)]
        sub: TaskCmd,
    },
    /// Comments (a task's stories)
    Comment {
        #[command(subcommand)]
        sub: CommentCmd,
    },
    /// Decision records (append-only "why we chose X"; a Task sibling, not a task)
    Decision {
        #[command(subcommand)]
        sub: DecisionCmd,
    },
    /// Attachments — list/show/open/remove (add via `task attach` / `decision attach`)
    Attach {
        #[command(subcommand)]
        sub: AttachCmd,
    },
    /// Export data — everything on this device, as JSON. That is the only shape: export
    /// exists for migrating into other tools, and neither an excerpt nor a human-readable table serves
    /// that. Export is **one-way**: the way back into amenbo is a `backup` archive and `restore`,
    /// not this output.
    Export {
        /// The **export directory** to create — `export.json` plus `attachments/` with every
        /// attachment's bytes. Must not exist yet. With no `--out` the dump streams to stdout (records
        /// only — no attachments).
        #[arg(long)]
        out: Option<String>,
    },
    /// Back up this device — its store and its attachment bytes — into a single
    /// verified `.amenbo-backup` archive at `path`. The store is snapshotted with `VACUUM INTO`
    /// (checkpointed, no torn DB+WAL) and bounded-verified; a `manifest.json` records its migration
    /// generation. Secrets (keys/identity) are never included. The destination must not already exist.
    Backup {
        /// Destination `.amenbo-backup` archive; must not already exist.
        path: Option<String>,
    },

    /// Restore this device from a verified `.amenbo-backup` archive at `path`: a
    /// destructive replace of this device's store. The snapshot is
    /// validated and gated on its format generation before anything is swapped in (all-or-nothing);
    /// the replaced truth source is set aside with a timestamp. An archive newer than this build is
    /// refused — update first. An archive written before the consolidation (layout v4 or older)
    /// is refused too: restore it with the build that wrote it. Destructive —
    /// prompts for confirmation unless `--yes`.
    Restore {
        /// `.amenbo-backup` archive to restore from.
        path: Option<String>,
    },

    /// Find amenbo refs (`AMB-T-<n>`, `AMB-D-<n>`, …) in text on its way out of this store — a commit
    /// message, a diff, a file — and exit non-zero if there are any.
    ///
    /// An id names something only someone holding this store can look up; anywhere else it is a
    /// reference into nothing. This reports every one it finds as `path:line` and **changes nothing**:
    /// removing them is yours to do (there is no `--fix`).
    ///
    /// With no arguments it reads the staged diff (`git diff --cached`) and scans what the commit
    /// **adds**. Pass file paths to lint those instead — the commit message file git hands a `commit-msg`
    /// hook included — or `--stdin` to lint piped text. A bare `#<n>` is left alone: that is a GitHub
    /// issue, and a `T-<n>` may be another tracker's.
    ///
    /// It opens no store and resolves no id — the `AMB-` prefix is the whole test — so it answers the
    /// same in a checkout, in CI, and over any text at all, and needs no `.amenbo` to run.
    Lint {
        /// files to lint (default: the staged diff)
        paths: Vec<String>,
        /// lint the text piped on stdin instead
        #[arg(long, conflicts_with = "paths")]
        stdin: bool,
    },

    /// The entry point amenbo's own `pre-commit` hook calls — it lints the staged diff, the same as a bare
    /// `lint`. Hidden because it exists for the hook, not the hand: the managed block names this fixed line so
    /// the hook's behaviour can grow in later versions without every installed hook being rewritten.
    #[command(hide = true)]
    GithookPreCommit,

    /// The entry point amenbo's own `commit-msg` hook calls — it lints the message file git hands the hook.
    /// Hidden for the same reason as `githook-pre-commit`: it is the hook's fixed line, not a command for the
    /// hand (`lint <file>` is that).
    #[command(hide = true)]
    GithookCommitMsg {
        /// the commit message file git passes the hook
        path: String,
    },

    /// The entry point a **plugin runner** is launched through: it works one plugin's queue of observation
    /// events to its end, in a process of its own, and exits (`AMB-D-399`, `AMB-T-2175`). Hidden because
    /// amenbo launches it — never a hand. It is not a daemon: it is started only when there is a queue and a
    /// free lease, and there is nothing to stop, since it ends when its queue is empty.
    ///
    /// It takes the store as an argument rather than resolving one: a runner must work the store the drive
    /// that launched it drove. Its own output goes nowhere — what each run did is in the execution log
    /// (`AMB-D-361`).
    #[command(hide = true)]
    PluginRunner {
        /// the plugin whose queue to work
        plugin: String,
        /// the lease the launching drive took on this runner's behalf
        owner: String,
        /// the base directory of the store to work (app-data, or `AMENBO_HOME`)
        store: String,
    },

    /// Manage the git hooks that run `amenbo lint`: `pre-commit` for the staged diff, and `commit-msg`
    /// for the message, which is the only place git offers it. Installing writes into your git plumbing,
    /// which amenbo does not do unasked: it asks once — for the lint as a feature, on this device — and
    /// that one answer covers the repositories it works in, the ones bound later included. These are the
    /// explicit faces of that: `install` wires this repository (and takes back an earlier `uninstall`
    /// here), `uninstall` opts this one out so a device-wide yes does not re-wire it, and both are usable
    /// any time, whatever was answered. amenbo touches only the hooks it wrote, which it marks as its own:
    /// a hook from husky, lefthook, or your own hand is never overwritten and never removed, and install
    /// steps around it, wiring the slots it may own and naming the line to add to the rest.
    Hooks {
        #[command(subcommand)]
        sub: HooksCmd,
    },

    /// Physically erase content from this store's truth source.
    ///
    /// An ordinary delete removes the row but leaves its bytes in the file's freed pages, and editing a
    /// decision body in place (`decision edit`) likewise leaves the prior bytes there, so the everyday
    /// commands cannot make content leave the file. This is the deliberate, gated exception: it deletes the
    /// read-model row / overwrites the field in place and VACUUMs so the bytes leave the file,
    /// unrecoverable.
    ///
    /// A destructive maintenance op: it takes a safety backup first (`pre-erase-<stamp>.amenbo-backup`
    /// next to the store — the archive `amenbo restore` puts the store back from) and prompts unless
    /// `--yes`. Only the newest one is kept: taking it sweeps the ones earlier erases left, naming what it
    /// removed. It still contains the erased content — delete it once you have verified the erase.
    HardErase {
        #[command(subcommand)]
        sub: HardEraseCmd,
    },

    /// Manage this machine's plugins, and self-check a manifest you are authoring.
    ///
    /// A plugin is distributed as a manifest in the public catalog repository (`AMB-D-347`) and installed
    /// under the app-data `plugins/` directory (`AMB-D-350`). `install` is the door those bytes come
    /// through; `list` / `enable` / `disable` are the machine-local face of what came through it: what is
    /// installed, and whose gate is open (`AMB-D-351` — installing a plugin never runs it; and each plugin
    /// has exactly one gate, at the level its author declared — `AMB-D-379`). `run` is the one command that
    /// actually *calls* a plugin on purpose: its command face, whose return value comes back to you
    /// (`AMB-D-353`). `validate` is
    /// the author's side — it runs the same rules amenbo enforces at
    /// the door (a well-formed id, checksum, OS set and config schema — `AMB-D-354`/`AMB-D-360`/`AMB-D-356`)
    /// over a manifest file you point it at, so you can self-check before opening a catalog PR, and it
    /// alone reads no store and needs no binding.
    Plugin {
        #[command(subcommand)]
        sub: PluginCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum PluginCmd {
    /// Validate a plugin manifest file against the catalog rules, reporting every problem it finds
    /// (`AMB-D-354`). The path may be `.yaml` (the form authored in the catalog repo) or `.json` (the
    /// aggregated `catalog.json` form) — the format is read from the extension, defaulting to YAML.
    /// Exits non-zero if the manifest is invalid, so it drops into a pre-submit check.
    Validate {
        /// path to the manifest file (`plugins/<name>.yaml` or a `.json` manifest)
        path: String,
    },

    /// List the plugins installed on this machine, and whether each one is enabled (`AMB-D-350`/`AMB-D-351`).
    /// Reads only what is on disk under the app-data `plugins/` directory plus this machine's enable
    /// state — no network, no catalog. In a bound folder the gate shown is the effective one: this
    /// project's override where it declares one, the machine answer otherwise.
    List,

    /// Install a plugin from the catalog: resolve the name, fetch its asset, verify its provenance
    /// fail-closed (the catalog signature, then the checksum of the distributable published for this OS —
    /// `AMB-D-371`/`AMB-D-351`/`AMB-D-381`), and lay it down under the app-data `plugins/` directory. **Installing never enables**: the plugin lands inert and
    /// `plugin enable` is the separate act. A name already installed is refused rather than overwritten
    /// (`AMB-D-360`).
    Install {
        /// the plugin's name, as the catalog lists it
        name: String,
    },

    /// Open an installed plugin's gate: record the one-time consent to run its code and let it fire
    /// (`AMB-D-351` — `install ≠ enable`, so nothing runs until this). **Which** switch this is was
    /// declared by the plugin's author (`AMB-D-379`): a project-scoped plugin turns on for the project you
    /// are in (so it needs a bound folder), a machine-scoped one for the device. There is no `--scope` —
    /// a plugin has one switch, and the message says which level it moved. Refused while a setting the
    /// author marked `required` is still empty; fill it with `plugin config set` first. Refused too when
    /// the plugin is not compatible with this build — a different payload contract, or a floor above the
    /// running version (`AMB-D-359`).
    Enable {
        /// the installed plugin's name
        name: String,
    },

    /// Close an enabled plugin's gate — the same one switch `enable` opens (`AMB-D-379`). The plugin stays
    /// installed and stays consented, so a later `enable` does not ask again (`disable ≠ uninstall`,
    /// `AMB-D-357`). A plugin whose manifest cannot be read has no declaration left to follow, so every
    /// gate the name could hold is closed.
    Disable {
        /// the plugin's name
        name: String,
    },

    /// Remove a plugin and everything it left behind (`AMB-D-357`): the binary, its settings in every
    /// project, its secrets, and the consent. Disables it first, so an interrupted removal never leaves
    /// a plugin still firing. **A re-install starts clean** — nothing here comes back.
    Uninstall {
        /// the plugin's name
        name: String,
    },

    /// Invoke an installed plugin's command face and hand its return value back to you (`AMB-D-353`).
    ///
    /// The other face runs itself: an observation hook fires on an event, asynchronously, and nobody waits
    /// for it. This one you call, and you get an answer — **the plugin's stdout is this command's stdout,
    /// verbatim**, so a plugin that returns a directory to enter drops straight into a shell:
    /// `eval "$(amenbo plugin run worktree start 123)"`. Its stderr — the human-facing diagnostics — is
    /// relayed to stderr, and a plugin that exits non-zero is a failed call whose return value is
    /// discarded rather than handed on (`AMB-D-354`).
    ///
    /// Everything after the name is the plugin's, passed through untouched — dashes and all: amenbo
    /// neither parses nor rewrites it, because what the words mean is the plugin's business (`AMB-D-346`).
    /// The corollary is that amenbo's own flags have to come *before* the plugin's name
    /// (`amenbo plugin run --json worktree …`), since after it every word is the plugin's. Refused when the
    /// plugin is not installed, not enabled (`install ≠ enable`, `AMB-D-351`), or not compatible with this
    /// build (`AMB-D-359`) — a caller waiting on a return value is told why there is none.
    Run {
        /// the installed plugin's name
        name: String,
        /// arguments handed to the plugin verbatim, dashes included
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Read the plugin execution log — the last runs of each plugin, and what each one said
    /// (`AMB-D-361`). The face for *why did nothing happen*: a hook is fire-and-forget, so a plugin that
    /// failed, timed out or never launched said so nowhere a caller was listening (`AMB-D-352`).
    ///
    /// One line per run — when, which plugin, on which event, how it ended, its exit code and how long it
    /// took. A run that did not end cleanly is followed by what the plugin wrote to **stderr**, which is
    /// where its author put the diagnosis (`AMB-D-353`); `--json` carries that text for every run,
    /// successful ones included. A `gap` line is not a run at all: it marks events the dispatcher could
    /// never deliver because retention had passed its cursor.
    ///
    /// It leads with the **dispatch cursor** — how far this store's outbox has been fanned out onto the
    /// plugins' queues, and which face last advanced it (`AMB-D-380`, `AMB-D-399`). That is the other half
    /// of the same question: the runs say what fired, the cursor says what was handed out, and a double
    /// fire or a silent miss is where the two disagree. The
    /// face is a stamp for lining the store up against this log, not a turn order — both faces drive, and
    /// nothing chooses between them.
    ///
    /// Reads one machine-local file and two store rows, and nothing else — no network. The log is bounded by
    /// construction (the last runs of each installed plugin), so there is no window to ask for and no
    /// deep history to page: a longer one is a logging plugin's business.
    Log {
        /// narrow to one plugin's runs; omit for every plugin's, newest first
        name: Option<String>,
    },

    /// Bring an installed plugin onto the build the catalog publishes — or, with `--check`, only report
    /// which installs it has moved past (`AMB-D-359`).
    ///
    /// Detection is the catalog amenbo already fetches whole laid beside the manifest sitting next to
    /// each installed binary — no central server, and no per-plugin request. A manifest carries no
    /// version number, so what is compared is the checksum of this machine's asset (`AMB-D-381`): the
    /// digest of the exact bytes that would run here, and therefore the build's identity. It reports *different*, not *newer* — the catalog is the authority
    /// on what is published, including a rollback.
    ///
    /// **Nothing is ever applied on amenbo's own account**: naming a plugin, or `--all`, is the whole
    /// consent. Applying re-walks the install door over the new asset — the catalog signature, then this
    /// OS's checksum (`AMB-D-351`) — and retains the build it replaced beside the new one, so there is
    /// something to go back to. It **keeps** the plugin's gate, its settings and its secrets: an update is
    /// not a re-install, and wiping those is `uninstall`'s job (`AMB-D-357`). Any step that refuses —
    /// a build this amenbo cannot speak to, an asset that will not verify — leaves the working plugin
    /// exactly as it was.
    ///
    /// `--check` is cheap on purpose: with nothing installed no catalog is read at all, and otherwise a
    /// cached catalog younger than an hour answers with no request. Applying always asks for the current
    /// index, since replacing a binary on an hour-old answer is not the same bargain.
    Update {
        /// the installed plugin to update; omit it with --all or --check
        name: Option<String>,
        /// report what has an update without applying anything
        #[arg(long)]
        check: bool,
        /// apply every update the catalog holds, one plugin at a time
        #[arg(long)]
        all: bool,
    },

    /// Undo the last `plugin update` for one plugin, restoring the build it retained (`AMB-D-359`).
    ///
    /// An update kept the previous executable and its manifest as a `.bak` pair beside the new ones; this
    /// puts both back — the same shape self-update's `update --rollback` uses (`AMB-D-341`). Offline and
    /// instant: nothing is fetched and nothing is re-verified, because the retained build already passed
    /// the door on its way in and a rollback is a deliberate return to it. It leaves the gate, the
    /// settings and the secrets alone, exactly as the update did.
    ///
    /// Goes back **one** build, and only one: the retained copy is consumed, so a second rollback has
    /// nothing to restore and says so. Refused, changing nothing, when the plugin is not installed or was
    /// never updated (there is no retained build to return to).
    Rollback {
        /// the installed plugin to roll back
        name: String,
    },

    /// Fill in an installed plugin's settings — the keys its author declared in the manifest
    /// (`AMB-D-356`). Where a value is kept is the author's `secret` flag's to decide, not yours:
    /// a secret goes to the user-area secret file (off the store, off every backup), the rest to the
    /// ordinary machine-default / per-project tiers.
    Config {
        #[command(subcommand)]
        sub: PluginConfigCmd,
    },

    /// Register the third-party catalogs to browse alongside the official one, and list what is registered
    /// (`AMB-D-347`, the "free" tier). The unit is the **catalog**, not the plugin: what grows is the
    /// number of indexes (few), never per-plugin requests. Registering a catalog only widens what
    /// `discover` (and, later, the GUI browser) *shows* — it does **not** widen what `install` accepts. An
    /// asset is trusted only by amenbo's catalog key (`AMB-D-371`), so a third-party plugin still cannot be
    /// installed; this is discovery, not a second install root.
    Catalog {
        #[command(subcommand)]
        sub: PluginCatalogCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum PluginCatalogCmd {
    /// List the catalogs that make up the browsing view: the official one, then each registered
    /// third-party catalog in the order it was added, with how many plugins each currently offers and
    /// whether it could be reached (from the network, or its cache). Reads caches the incidental way — a
    /// catalog fresh on disk answers without a request.
    List,

    /// Register a third-party catalog by the URL of its `catalog.json`. Idempotent: registering the same
    /// URL twice is a no-op. Refuses a non-`http(s)` URL and the official catalog's own URL. The catalog is
    /// fetched once here so the first browse is warm; an unreachable URL still registers (it will be
    /// retried on the next browse).
    Add {
        /// the URL of the third-party catalog's `catalog.json`
        url: String,
    },

    /// Unregister a third-party catalog by its URL, and drop its cached copy. Idempotent: removing a URL
    /// that is not registered is a no-op.
    Remove {
        /// the URL that was registered with `plugin catalog add`
        url: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum PluginConfigCmd {
    /// Store one setting's value. The key must be one the plugin's manifest declares — that declaration
    /// is what says whether the value is a secret, and amenbo never guesses (`AMB-D-356`). An empty value
    /// clears the setting rather than storing a blank.
    Set {
        /// the installed plugin's name
        name: String,
        /// the setting's key, as the manifest declares it
        key: String,
        /// the value; `-` reads it from stdin (which keeps a secret off argv and out of shell history),
        /// and an empty string clears the setting
        value: String,
        /// which tier to write: `machine` (the default, in config.json) or `project` (this project's
        /// override). Ignored for a secret — there is one secret per plugin and key.
        #[arg(long, default_value = "machine")]
        scope: String,
    },

    /// Read one setting back **at that tier** — the machine default or the project override as stored,
    /// not the effective value precedence would pick. A secret is never echoed: it reports only whether
    /// one is set.
    Get {
        /// the installed plugin's name
        name: String,
        /// the setting's key, as the manifest declares it
        key: String,
        /// which tier to read: `machine` (the default) or `project` (this project's override)
        #[arg(long, default_value = "machine")]
        scope: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum HooksCmd {
    /// Write the lint hooks, and record that this project consented. A slot amenbo did not write is
    /// stepped around rather than overwritten — the rest are still wired, and the one line to add by
    /// hand is named; re-running over amenbo's own hooks rewrites them, which is how a newer build's
    /// hooks land. Only an install with no slot to write at all is refused.
    Install,

    /// Remove the lint hooks amenbo wrote, and record that this project does not want them. The mirror
    /// of install, refusal for refusal: a hook amenbo did not write is not amenbo's to delete, and with
    /// nothing of ours there it records the answer and does nothing else.
    Uninstall,

    /// Show what is in each hook slot and what this project answered — the two facts, side by side.
    Status,
}

#[derive(Subcommand, Debug)]
pub enum HardEraseCmd {
    /// Remove a task comment in full — its row, and the freed pages with it. Identify comments by
    /// id; find them with `comment list <task> --json`.
    Comment {
        /// task comment ref(s) to erase, AMB-TC-n
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Remove a decision comment in full — the same surgery as `hard-erase comment`, on the other comment
    /// table. It is its own subcommand because the two tables number apart: a bare id says nothing about
    /// which one it belongs to, so the command is what says it. Find ids with
    /// `decision comment list <decision> --json`.
    DecisionComment {
        /// decision comment ref(s) to erase, AMB-DC-n
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Redact an accepted decision's body: overwrite it in place with the given text and scrub the prior
    /// bytes from the file (which `decision edit` alone does not). The decision — its number, links and other fields — stays.
    Decision {
        /// decision reference (AMB-D-n)
        id: String,
        /// replacement body text (Markdown); omit and pass --body-file, or pipe on stdin
        #[arg(long)]
        body: Option<String>,
        /// read the replacement body from this file instead of --body / stdin
        #[arg(long, conflicts_with = "body")]
        body_file: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Change a configuration value
    Set { key: String, value: String },
}

#[derive(Subcommand, Debug)]
pub enum ProjectCmd {
    Add {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "board")]
        view: String,
        #[arg(long, default_value = "")]
        notes: String,
        #[arg(long)]
        color: Option<String>,
    },
    List {
        #[arg(long)]
        archived: bool,
    },
    Show { id: String },
    Update {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        notes: Option<String>,
        #[arg(long)]
        view: Option<String>,
        #[arg(long)]
        color: Option<String>,
    },
    Move {
        id: String,
        #[arg(long)]
        before: Option<String>,
        #[arg(long)]
        after: Option<String>,
        #[arg(long)]
        top: bool,
        #[arg(long)]
        bottom: bool,
    },
    Archive { id: String },
    Unarchive { id: String },
    Delete { id: String },
}

/// A dimension is a purely user-defined classification axis. The axis, its values, and task
/// assignments are handled symmetrically, and the verbs for axis operations line up with the other
/// resources' (only value operations and assignment are specific to this mechanism).
#[derive(Subcommand, Debug)]
pub enum DimensionCmd {
    /// Add a dimension (classification axis) to a project (appended after existing dimensions)
    Add {
        /// project (name or ID; defaults to the bound project)
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        name: String,
        /// description / notes (Markdown)
        #[arg(long, default_value = "")]
        notes: String,
        /// give the values an explicit order (default: unordered)
        #[arg(long)]
        ordered: bool,
        /// mark this axis as the project's time axis — its values carry periods
        #[arg(long)]
        time_axis: bool,
    },
    /// List a project's dimensions (display order) with their values
    List {
        /// project (name or ID; defaults to the bound project)
        #[arg(long)]
        project: Option<String>,
    },
    /// Show a dimension (name, notes, cardinality/ordered/role, values)
    Show {
        /// dimension ref (AMB-DIM-n) or name
        id: String,
    },
    /// Rename a dimension
    Rename {
        /// dimension ref (AMB-DIM-n) or name
        id: String,
        #[arg(long)]
        name: String,
    },
    /// Update a dimension's name, notes, value ordering, and/or time-axis role (only the given fields change)
    Update {
        /// dimension ref (AMB-DIM-n) or name
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        notes: Option<String>,
        /// whether the values carry an explicit order (`--ordered true|false`)
        #[arg(long)]
        ordered: Option<bool>,
        /// whether this axis is the project's time axis (`--time-axis true|false`)
        #[arg(long)]
        time_axis: Option<bool>,
    },
    /// Reorder a dimension within its project
    Move {
        /// dimension ref (AMB-DIM-n) or name
        id: String,
        #[arg(long)]
        before: Option<String>,
        #[arg(long)]
        after: Option<String>,
        #[arg(long)]
        top: bool,
        #[arg(long)]
        bottom: bool,
    },
    /// Delete a dimension permanently; its values and task assignments go with it (alias: delete)
    #[command(alias = "delete")]
    Rm {
        /// dimension ref (AMB-DIM-n) or name
        id: String,
    },
    /// Add a value to a dimension (appended after existing values)
    ValueAdd {
        /// dimension ref (AMB-DIM-n) or name
        dimension: String,
        #[arg(long)]
        name: String,
        /// first day of the value's period (time-axis dimensions only)
        #[arg(long)]
        start: Option<String>,
        /// last day of the value's period; omit to leave it ongoing (time-axis dimensions only)
        #[arg(long)]
        end: Option<String>,
    },
    /// Rename a dimension value
    ValueRename {
        /// dimension ref (AMB-DIM-n) or name
        dimension: String,
        /// value ref (AMB-DIMV-n) or name (within the dimension)
        value: String,
        #[arg(long)]
        name: String,
    },
    /// Update a dimension value's name and/or period (only the given fields change)
    ValueUpdate {
        /// dimension ref (AMB-DIM-n) or name
        dimension: String,
        /// value ref (AMB-DIMV-n) or name (within the dimension)
        value: String,
        #[arg(long)]
        name: Option<String>,
        /// first day of the value's period (time-axis dimensions only)
        #[arg(long)]
        start: Option<String>,
        /// last day of the value's period (time-axis dimensions only)
        #[arg(long)]
        end: Option<String>,
        /// open the period's start
        #[arg(long, conflicts_with = "start")]
        clear_start: bool,
        /// open the period's end (the value becomes ongoing)
        #[arg(long, conflicts_with = "end")]
        clear_end: bool,
    },
    /// Reorder a value within its dimension
    ValueMove {
        /// dimension ref (AMB-DIM-n) or name
        dimension: String,
        /// value ref (AMB-DIMV-n) or name (within the dimension)
        value: String,
        #[arg(long)]
        before: Option<String>,
        #[arg(long)]
        after: Option<String>,
        #[arg(long)]
        top: bool,
        #[arg(long)]
        bottom: bool,
    },
    /// Delete a dimension value permanently; its task assignments go with it (alias: value-delete)
    #[command(alias = "value-delete")]
    ValueRm {
        /// dimension ref (AMB-DIM-n) or name
        dimension: String,
        /// value ref (AMB-DIMV-n) or name (within the dimension)
        value: String,
    },
    /// Assign a task a value of a dimension (single-select replaces the task's prior value)
    Set {
        /// task ref (AMB-T-n)
        task: String,
        /// dimension ref (AMB-DIM-n) or name
        dimension: String,
        /// value ref (AMB-DIMV-n) or name (within the dimension)
        value: String,
    },
    /// Clear a task's value of a dimension
    Unset {
        /// task ref (AMB-T-n)
        task: String,
        /// dimension ref (AMB-DIM-n) or name
        dimension: String,
        /// value ref (AMB-DIMV-n) or name (within the dimension)
        value: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum TaskCmd {
    Add {
        #[arg(long)]
        title: String,
        /// owning project (name or ID). Required — a project-less task is refused;
        /// omitting it lists existing projects to pick from.
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        due: Option<String>,
        #[arg(long)]
        start: Option<String>,
        #[arg(long)]
        priority: Option<String>,
        /// description / notes, as Markdown (GUI renders GFM tables/task lists + ```mermaid; no raw HTML).
        /// Lead with the conclusion, prefer bullets/tables, one point per line (a single newline is a break).
        /// Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument).
        #[arg(long, default_value = "")]
        notes: String,
        /// delegate the new task to a facet in one step — `me`/`self`/`human` or the human's name →
        /// the human; `me-ai`/`ai` → the human's AI. Same as a follow-up `task assign`,
        /// saving the create+assign round trip when filing AI work.
        #[arg(long)]
        to: Option<String>,
        /// with --to, delegate to "that person's AI" (assignee_kind=ai)
        #[arg(long)]
        ai: bool,
        /// classify the new task as `<axis>=<value>` — the same resolution as `dimension set` (id, or
        /// an exact name, case-insensitive). Repeatable for different axes; an axis is single-select,
        /// so naming one twice is refused. What you name here wins over the time-axis default.
        #[arg(long = "dim", value_name = "AXIS=VALUE")]
        dim: Vec<String>,
    },
    List {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value = "order")]
        sort: String,
        /// max count (in sort order; pairs with --offset for paging)
        #[arg(long)]
        limit: Option<usize>,
        /// number of items to skip in sort order (paging)
        #[arg(long)]
        offset: Option<usize>,
    },
    Show { id: String },
    Update {
        id: String,
        #[arg(long)]
        title: Option<String>,
        /// replacement notes, as Markdown (GUI renders GFM tables/task lists + ```mermaid; no raw HTML).
        /// Lead with the conclusion, prefer bullets/tables, one point per line (a single newline is a break).
        /// Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument).
        #[arg(long)]
        notes: Option<String>,
        #[arg(long)]
        due: Option<String>,
        #[arg(long)]
        start: Option<String>,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long)]
        clear_due: bool,
        #[arg(long)]
        clear_start: bool,
        #[arg(long)]
        clear_priority: bool,
    },
    /// Mark a task done
    Done { id: String },
    Reopen { id: String },
    /// Explicitly change the progress state (todo / in_progress / done / blocked / rejected). Setting
    /// in_progress reserves it — a compare-and-swap that only succeeds from todo, so a second
    /// session's reserve is rejected with already_reserved (the double-work guard); todo releases it
    Status {
        id: String,
        /// new state: todo / in_progress / done / blocked / rejected
        status: String,
    },
    /// Mark blocked (stuck)
    Block {
        id: String,
        /// reason (recorded as a comment). Pass `-` to read it from stdin
        #[arg(long)]
        reason: Option<String>,
    },
    /// End a task that will not be done — the terminal for work decided against, next to `done`
    Reject {
        id: String,
        /// why it will not be done (required, recorded as a comment). Pass `-` to read it from stdin
        #[arg(long)]
        reason: String,
    },
    Move {
        id: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        before: Option<String>,
        #[arg(long)]
        after: Option<String>,
        #[arg(long)]
        top: bool,
        #[arg(long)]
        bottom: bool,
    },
    /// Make this task depend on another (make --on a blocker that must be done first)
    Depend {
        id: String,
        /// task ID of the blocker that must be done first
        #[arg(long)]
        on: String,
    },
    /// Remove a dependency
    Undepend {
        id: String,
        /// blocker task ID to remove
        #[arg(long)]
        on: String,
    },
    /// Attach a file (blob, ingested) or external link (--url) to a task (manage via `attach`)
    Attach {
        /// target task ref (AMB-T-n)
        id: String,
        /// file path to ingest as a blob, or the external URL with --url
        source: String,
        /// treat <source> as an external URL link instead of ingesting a file
        #[arg(long)]
        url: bool,
        /// display label (defaults to the file name / URL)
        #[arg(long)]
        name: Option<String>,
    },
    /// Record / list / forget the git commit SHAs that implemented a task — the anchor from
    /// history back to a task (amenbo stores each SHA opaquely and never reads git)
    Commit {
        #[command(subcommand)]
        sub: TaskCommitCmd,
    },
    /// Assign an assignee to a task
    Assign {
        id: String,
        /// assignee facet: `me`/`self`/`human` or the human's display name → the human;
        /// `me-ai`/`ai` → the human's AI
        #[arg(long)]
        to: String,
        /// delegate to "that person's AI" (assignee_kind=ai)
        #[arg(long)]
        ai: bool,
    },
    /// Remove a task's assignee
    Unassign { id: String },
    Delete { id: String },
}

/// A task's git commit SHAs (`add`/`list`/`rm`) — the anchor from history back to a task, since a
/// public commit carries no store-local reference. amenbo stores each SHA as an opaque full-length
/// hex string: it never reads git, verifies the commit, or knows which forge it lives on.
#[derive(Subcommand, Debug)]
pub enum TaskCommitCmd {
    /// Record a commit SHA on a task (idempotent; full-length lower-case hex only)
    Add {
        /// target task ref (AMB-T-n)
        task: String,
        /// the full commit SHA — 40 hex for SHA-1, 64 for SHA-256 (short forms, branches, tags and
        /// revisions are refused)
        sha: String,
    },
    /// List a task's recorded commit SHAs, oldest first
    List {
        /// target task ref (AMB-T-n)
        task: String,
    },
    /// Forget a commit SHA on a task — permanently (alias: remove)
    #[command(alias = "remove")]
    Rm {
        /// target task ref (AMB-T-n)
        task: String,
        /// the commit SHA to forget (any case — normalised the way it was stored)
        sha: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum CommentCmd {
    Add {
        /// target task ID
        task: String,
        /// comment body, as Markdown (GUI renders GFM tables/task lists + ```mermaid; no raw HTML).
        /// Lead with the conclusion, prefer bullets/tables, one point per line (a single newline is a break).
        /// Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument).
        #[arg(long)]
        text: String,
    },
    List {
        /// target task ID
        task: String,
        /// max count (oldest first; pairs with --offset for paging)
        #[arg(long)]
        limit: Option<usize>,
        /// number of items to skip, oldest first (paging)
        #[arg(long)]
        offset: Option<usize>,
    },
    /// Delete a comment posted by mistake — permanently, with its attachments.
    /// The id comes from `comment list`
    Rm {
        /// target task comment ref, AMB-TC-n (from `comment list`)
        comment: String,
    },
    /// Rewrite a comment's body in place — the id, its place on the timeline, and its
    /// attachments all stay. The id comes from `comment list`
    Edit {
        /// target task comment ref, AMB-TC-n (from `comment list`)
        comment: String,
        /// the new body, as Markdown — it replaces the old one outright. Pass `-` to read it from stdin
        /// (a shell eats code spans out of a quoted argument).
        #[arg(long)]
        text: String,
    },
    /// Attach a file (blob, ingested) or external link (--url) to a single task comment — kept
    /// separate from the parent task's own attachments (manage via `attach`)
    Attach {
        /// target task comment ref, AMB-TC-n (from `comment list`)
        comment: String,
        /// file path to ingest as a blob, or the external URL with --url
        source: String,
        /// treat <source> as an external URL link instead of ingesting a file
        #[arg(long)]
        url: bool,
        /// display label (defaults to the file name / URL)
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum DecisionCmd {
    /// Record a new decision (proposed). The project defaults to the bound project
    Add {
        #[arg(long)]
        title: String,
        /// the decision body: conclusion + rationale (compress; do not paste raw discussion). Markdown —
        /// GUI renders GFM tables/task lists + ```mermaid (no raw HTML); a single newline shows as a break.
        /// Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument).
        #[arg(long, default_value = "")]
        body: String,
        /// project (name or ID; defaults to the bound project)
        #[arg(long)]
        project: Option<String>,
    },
    /// List decisions (filter by status:/current:/text:/project:/number: (alias ref:, e.g. `D-<n>`/`#<n>`)/task: (the decisions a task rests on, e.g. `task:#<n>`)/decided_before:/decided_after: (the day a decision was accepted, YYYY-MM-DD or today/-30d; both ends inclusive), sort by decided/created/number/title/status)
    List {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value = "-created")]
        sort: String,
        /// max count (in sort order; pairs with --offset for paging)
        #[arg(long)]
        limit: Option<usize>,
        /// number of items to skip in sort order (paging)
        #[arg(long)]
        offset: Option<usize>,
        /// include each decision's body in the listing (a projection — composes with
        /// --filter/--limit/--offset; narrow and page, don't dump the whole corpus)
        #[arg(long)]
        with_body: bool,
    },
    /// Show a decision (body, status, supersession chain, the premises it builds on — overturned ones flagged —
    /// the decisions that build on it, and linked tasks)
    Show {
        /// decision ref (AMB-D-n)
        id: String,
    },
    /// Edit a decision's title/body in place — proposed or accepted alike (supersede to overturn an accepted one, not edit)
    Edit {
        id: String,
        #[arg(long)]
        title: Option<String>,
        /// replacement body (Markdown). Pass `-` to read it from stdin
        /// (a shell eats code spans out of a quoted argument).
        #[arg(long)]
        body: Option<String>,
    },
    /// Accept a decision (proposed → accepted)
    Accept {
        id: String,
        /// reason for accepting — recorded as a decision comment, not a dedicated field. Pass `-` to read it from stdin
        #[arg(long)]
        reason: Option<String>,
    },
    /// Reject a decision (proposed → rejected)
    Reject {
        id: String,
        /// reason for rejecting — recorded as a decision comment, not a dedicated field. Pass `-` to read it from stdin
        #[arg(long)]
        reason: Option<String>,
    },
    /// Return an accepted decision to discussion (accepted → proposed) — un-settle it. Editing does not need it. Non-destructive and audited
    Reopen {
        id: String,
    },
    /// Delete (retire) a decision — accepted ones included; the row goes, the bytes stay in the file (confirms unless --yes)
    Delete {
        /// decision ref (AMB-D-n)
        id: String,
    },
    /// Record a new decision that replaces an existing one (supersession chain)
    Supersede {
        /// the new decision (it replaces the old one)
        decision: String,
        /// the decision being replaced
        #[arg(long)]
        replaces: String,
    },
    /// Record that a decision amends (partially revises) an existing one — the target stays current (not superseded)
    Amend {
        /// the new decision (it amends the old one)
        decision: String,
        /// the decision being amended (stays current)
        #[arg(long)]
        amends: String,
    },
    /// Record that a decision builds on (takes as a premise) an existing one — read the premise first, and revisit
    /// this decision if the premise is ever overturned. The target stays current and is not corrected
    #[command(name = "builds-on")]
    BuildsOn {
        /// the decision that stands on the premise
        decision: String,
        /// the premise it stands on (stays current)
        #[arg(long)]
        on: String,
    },
    /// Remove a decision-to-decision edge drawn by mistake (supersedes / amends / builds_on — the pair names it).
    /// Superseding again is a new decision; unlinking is for the edge that should never have been drawn
    Unlink {
        /// the decision the edge was drawn from (the newer one)
        decision: String,
        /// the decision it points at (the older one)
        #[arg(long)]
        from: String,
    },
    /// Link (or --unlink) a decision and a task (the motivating decision ⇄ its implementation tasks)
    Link {
        /// decision ref
        decision: String,
        /// task ref
        task: String,
        #[arg(long)]
        unlink: bool,
    },
    /// Promote a task comment into a decision (the comment text becomes the body; links to the task)
    Promote {
        /// the task comment ref to promote, AMB-TC-n
        comment: String,
        #[arg(long)]
        title: String,
        /// project (defaults to the comment's task project)
        #[arg(long)]
        project: Option<String>,
    },
    /// Discuss on a decision's timeline (append-only comments)
    Comment {
        #[command(subcommand)]
        sub: DecisionCommentCmd,
    },
    /// Attach a file (blob, ingested) or external link (--url) to a decision (manage via `attach`)
    Attach {
        /// target decision ref (AMB-D-n)
        id: String,
        /// file path to ingest as a blob, or the external URL with --url
        source: String,
        /// treat <source> as an external URL link instead of ingesting a file
        #[arg(long)]
        url: bool,
        /// display label (defaults to the file name / URL)
        #[arg(long)]
        name: Option<String>,
    },
}

/// Decision comment operations (`add`/`list`/`rm`/`edit`) — a decision's timeline (a comment
/// posted by mistake is deleted outright or rewritten in place, not retracted).
/// Mirrors the task [`CommentCmd`]; `accept`/`reject --reason` are thin sugar over `comment add`.
#[derive(Subcommand, Debug)]
pub enum DecisionCommentCmd {
    /// Add a comment to a decision's timeline
    Add {
        /// target decision ref (AMB-D-n)
        decision: String,
        /// comment body, as Markdown (GUI renders GFM tables/task lists + ```mermaid; no raw HTML).
        /// Lead with the conclusion, prefer bullets/tables, one point per line (a single newline is a break).
        /// Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument).
        #[arg(long)]
        text: String,
    },
    /// List a decision's comments (oldest first; pairs with --offset for paging)
    List {
        /// target decision ref (AMB-D-n)
        decision: String,
        /// max count (oldest first; pairs with --offset for paging)
        #[arg(long)]
        limit: Option<usize>,
        /// number of items to skip, oldest first (paging)
        #[arg(long)]
        offset: Option<usize>,
    },
    /// Delete a comment posted by mistake — permanently, with its attachments.
    /// The id comes from `decision comment list`
    Rm {
        /// target decision comment ref, AMB-DC-n (from `decision comment list`)
        comment: String,
    },
    /// Rewrite a comment's body in place — the id, its place on the timeline, and its
    /// attachments all stay. The id comes from `decision comment list`
    Edit {
        /// target decision comment ref, AMB-DC-n (from `decision comment list`)
        comment: String,
        /// the new body, as Markdown — it replaces the old one outright. Pass `-` to read it from stdin
        /// (a shell eats code spans out of a quoted argument).
        #[arg(long)]
        text: String,
    },
    /// Attach a file (blob, ingested) or external link (--url) to a single decision comment — kept
    /// separate from the parent decision's own attachments (manage via `attach`)
    Attach {
        /// target decision comment ref, AMB-DC-n (from `decision comment list`)
        comment: String,
        /// file path to ingest as a blob, or the external URL with --url
        source: String,
        /// treat <source> as an external URL link instead of ingesting a file
        #[arg(long)]
        url: bool,
        /// display label (defaults to the file name / URL)
        #[arg(long)]
        name: Option<String>,
    },
}

/// Attachment management (`ls`/`show`/`open`/`rm`). Add attachments with `task attach` /
/// `decision attach`.
#[derive(Subcommand, Debug)]
pub enum AttachCmd {
    /// List the attachments on a task, decision, or a single comment
    Ls {
        /// target task / decision ref (AMB-T-n / AMB-D-n) — for a comment, pass --task-comment /
        /// --decision-comment instead (the two comment tables number apart, so a bare id says
        /// nothing about which one it is)
        target: Option<String>,
        /// list the attachments on this task comment (id from `comment list`)
        #[arg(long, value_name = "ID", conflicts_with_all = ["target", "decision_comment"])]
        task_comment: Option<String>,
        /// list the attachments on this decision comment (id from `decision comment list`)
        #[arg(long, value_name = "ID", conflicts_with_all = ["target", "task_comment"])]
        decision_comment: Option<String>,
    },
    /// Show one attachment's metadata
    Show {
        /// attachment ref (AMB-ATT-n)
        id: String,
    },
    /// Open an attachment — a blob via the OS opener, or the external URL
    Open {
        /// attachment ref (AMB-ATT-n)
        id: String,
    },
    /// Save a blob attachment's bytes to a file (the CLI counterpart of the GUI's download —
    /// `open` only spills to a temp file). URL attachments have nothing to save; open the link
    /// with `attach open`.
    Save {
        /// attachment ref (AMB-ATT-n)
        id: String,
        /// where to write it — a file path, or a directory to save under the attachment's own
        /// filename. Omitted, it lands in the current directory under that filename.
        #[arg(long, value_name = "PATH")]
        out: Option<String>,
        /// overwrite the destination if it already exists (the default refuses, to not clobber
        /// unasked)
        #[arg(long)]
        force: bool,
    },
    /// Remove an attachment, permanently — confirms unless -y (the blob bytes are GC'd once
    /// nothing references them)
    Rm {
        /// attachment ref (AMB-ATT-n)
        id: String,
    },
}
