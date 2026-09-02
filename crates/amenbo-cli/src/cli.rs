//! Command definitions, via clap.
//! The single source of truth for the command spec is `agent.rs` (`amenbo agent --json`); this file is kept in step with it.
//!
//! Every command worded here for someone to run is spelled `amenbo`, the name the production build
//! installs — the derive takes literals, so there is nothing to interpolate a channel's name into.
//! `retargeted_cli` in `main.rs` does the swapping as the help is built, so a dev build never hands
//! out a command it does not answer to. Write the production spelling and let it do that.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    // The fallback only; `retargeted_cli` sets the name this build actually installs.
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
    /// suppress the human-facing success message
    #[arg(long, global = true)]
    pub quiet: bool,
    /// disable color. It is off already when NO_COLOR is set, or when the output is not a terminal —
    /// this is for saying so on a terminal that would otherwise get it
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
    /// Update Amenbo to the latest release. By default this opens this OS's one-piece installer
    /// (resolved from the published latest.json, falling back to the releases page) in your browser.
    /// Pass --apply to self-update the standalone CLI in place instead — download the new CLI archive
    /// over TLS and swap this binary, no installer, no elevation (CLI-only installs; a GUI-managed CLI
    /// is updated from the desktop app). Pass --rollback to undo the last --apply, restoring the binary
    /// it kept aside (offline, no download). Pass --print to only print the installer URL (no browser).
    /// Typing this asks upstream rather than answering from the detection cache, so no route here
    /// reports on an entry up to 24 hours old; offline it still falls back to the last one it had.
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
    /// Initialize a folder so an AI launched there may operate Amenbo (it does not read or write the
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
        /// It does not reach a git worktree cut inside a managed tree: Amenbo is refused there, and the
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
        /// re-point the binding with this id at the folder being bound, instead of recording a new one.
        /// The binding keeps its id, so whatever points at it — a task filed at that folder — follows the
        /// folder to its new path rather than being left naming nothing. This is the way to answer a
        /// folder that moved, was renamed, or was restored somewhere else. The ids are in the answer
        /// `bind` gives when it finds a folder gone
        #[arg(long, value_name = "BINDING-ID", requires = "project")]
        rebind: Option<i64>,
        /// bind even when this folder is already inside an Amenbo-managed tree (a parent has a
        /// `.amenbo`). Off by default so a stray bind in a source subdirectory cannot shadow the
        /// root pointer (and scatter `.amenbo`/AGENTS.md/CLAUDE.md there). It does not reach a git
        /// worktree cut inside that tree: Amenbo is refused there whatever pointer it holds, so
        /// binding one could only write a pointer nothing would read
        #[arg(long)]
        force: bool,
    },

    /// Remove this folder's `.amenbo` binding (and Amenbo's managed blocks in AGENTS.md/CLAUDE.md),
    /// keeping the store itself. Many-to-one: only this folder's pointer is removed; other folders
    /// bound to the same store are untouched. Use --dir to unbind another folder
    Unbind {
        /// folder to unbind (defaults to the current directory)
        #[arg(long)]
        dir: Option<String>,
    },

    /// Re-sync Amenbo's managed guidance block in bound folders to this binary's current version. A folder
    /// follows on its own the moment you run Amenbo in it, so this is for the folders you have not been in
    /// (and for a block Amenbo could not write). Idempotent and low-churn: a folder's CLAUDE.md/AGENTS.md is
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
    /// Find where words are written — one line per **place**, not per record. Reaches tasks, decisions,
    /// the comments on both, the labels either is filed under and the names of what is attached, and
    /// answers with the face the words landed on, the record it belongs to, and a short excerpt.
    ///
    /// Words are ANDed and match as substrings (no word boundaries, so part of a compound word finds
    /// it); full-width,
    /// case and kana spellings are brought together. Every word has to land somewhere on the record — not
    /// all on one face — and each face that carries one is a line, which is what makes the answer "here is
    /// where each of your words is written". A word written as a ref (`AMB-T-<n>` / `AMB-D-<n>`) pins that
    /// record to the top, so holding a number takes the same command as holding a phrase.
    Search {
        /// the words to look for (ANDed)
        #[arg(value_name = "WORD", required = true)]
        words: Vec<String>,
        /// look in this project alone (name or ID; human only — an AI is already scoped to its bound
        /// project). It is an argument of its own rather than a `--filter` key, so scoping to a project
        /// keeps decisions in the answer
        #[arg(long)]
        project: Option<String>,
        /// narrow structurally, in the grammar of the side `--kind` names — `task list`'s for a task
        /// (e.g. `--kind task --filter "status:todo"`), `decision list`'s for a decision. Requires
        /// `--kind`: the two grammars share spellings that mean different things
        #[arg(long)]
        filter: Option<String>,
        /// keep one side: task / decision
        #[arg(long)]
        kind: Option<String>,
        /// keep one face: title / body / comment / label / attachment. The other axis, judged apart from
        /// `--kind`, so naming both is the product of them (`--kind decision --face comment` is the
        /// remarks on decisions)
        #[arg(long)]
        face: Option<String>,
        /// face (the face first, newest within it) / -time (newest first) / time (oldest first)
        #[arg(long, default_value = "face")]
        sort: String,
        /// max hits. Defaults to 20 — a hit carries an excerpt, so this read has a ceiling of its own; the
        /// reported total says what it left behind
        #[arg(long)]
        limit: Option<usize>,
        /// number of hits to skip in sort order (paging)
        #[arg(long)]
        offset: Option<usize>,
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
        /// repair fixable problems (sweep attachment rows whose record is gone, reclaim unreferenced attachment files, forget folder bindings no live project claims) - all non-destructive
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
    /// that. Export is **one-way**: the way back into Amenbo is a `backup` archive and `restore`,
    /// not this output.
    ///
    /// One thing stays behind: a plugin's **secrets** (`AMB-D-434`). This file goes out to another tool
    /// and stays in its hands, and a credential in the clear is not something to hand over on the way
    /// past — they ride `backup`, which leads back to your own store, instead.
    Export {
        /// The **export directory** to create — `export.json` plus `attachments/` with every
        /// attachment's bytes. Must not exist yet. With no `--out` the dump streams to stdout (records
        /// only — no attachments).
        #[arg(long)]
        out: Option<String>,
    },
    /// The road out for a plugin that carries this store's data somewhere else — a viewer, an audit
    /// trail, a mirror in another tool (`AMB-D-581`).
    ///
    /// Two faces, and the order they are used in is the whole design: **ask the version**, and take a
    /// **snapshot** only if it moved. What comes back is closed to the window the caller reads through,
    /// so a plugin launched to observe one project gets that project and nothing else. The road is
    /// one-way — nothing here reads a snapshot back in (`AMB-D-578`).
    Sync {
        #[command(subcommand)]
        sub: SyncCmd,
    },
    /// Back up this device — its store and its attachment bytes — into a single
    /// verified `.amenbo-backup` archive at `path`. The store is snapshotted with `VACUUM INTO`
    /// (checkpointed, no torn DB+WAL) and bounded-verified; a `manifest.json` records its migration
    /// generation. The device's own secrets (at-rest key / identity) are never included; a plugin's
    /// secrets are store rows, so those ride along and come back working (`AMB-D-434`). The destination
    /// must not already exist.
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

    /// Find Amenbo refs (`AMB-T-<n>`, `AMB-D-<n>`, …) in text on its way out of this store — a commit
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

    /// The entry point Amenbo's own `pre-commit` hook calls — it lints the staged diff, the same as a bare
    /// `lint`. Hidden because it exists for the hook, not the hand: the managed block names this fixed line so
    /// the hook's behaviour can grow in later versions without every installed hook being rewritten.
    #[command(hide = true)]
    GithookPreCommit,

    /// The entry point Amenbo's own `commit-msg` hook calls — it lints the message file git hands the hook.
    /// Hidden for the same reason as `githook-pre-commit`: it is the hook's fixed line, not a command for the
    /// hand (`lint <file>` is that).
    #[command(hide = true)]
    GithookCommitMsg {
        /// the commit message file git passes the hook
        path: String,
    },

    /// The entry point a **plugin runner** is launched through: it works one plugin's queue of observation
    /// events to its end, in a process of its own, and exits (`AMB-D-399`, `AMB-T-2175`). Hidden because
    /// Amenbo launches it — never a hand. It is not a daemon: it is started only when there is a queue and a
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
    /// which Amenbo does not do unasked: it asks once — for the lint as a feature, on this device — and
    /// that one answer covers the repositories it works in, the ones bound later included. These are the
    /// explicit faces of that: `install` wires this repository (and takes back an earlier `uninstall`
    /// here), `uninstall` opts this one out so a device-wide yes does not re-wire it, and both are usable
    /// any time, whatever was answered. Amenbo touches only the hooks it wrote, which it marks as its own:
    /// a hook from husky, lefthook, or your own hand is never overwritten and never removed, and install
    /// steps around it, wiring the slots it may own and naming the line to add to the rest.
    Hooks {
        #[command(subcommand)]
        sub: HooksCmd,
    },

    /// Manage the hourly tick: the one plain timer Amenbo asks this machine's scheduler to hold, so
    /// that what has to happen on time happens with no app open and nothing resident. What is
    /// registered carries no meaning — it wakes Amenbo once an hour, and Amenbo works out once awake
    /// what is due — so however many things come to depend on it, this stays one row in your system
    /// settings, and switching that row off stops all of them. Registering writes into your
    /// scheduler, which Amenbo does not do unasked: it asks once, for the tick as a feature, on this
    /// device. These are the explicit faces of that — `install` is that yes, `uninstall` is that no,
    /// and `status` shows the answer beside what the scheduler actually holds.
    Tick {
        #[command(subcommand)]
        sub: TickCmd,
    },

    /// Hand over the configuration that makes an AI tool run `amenbo agent` when a session starts — the
    /// session-start hook, which reaches the model over the protocol instead of hoping the managed block
    /// in CLAUDE.md/AGENTS.md is read. **Amenbo never writes a provider's settings**: this hands you the
    /// text and the file to put it in, and pasting is yours. Not `hooks`, which is git's plumbing and is
    /// Amenbo's to write.
    AgentHook {
        #[command(subcommand)]
        sub: AgentHookCmd,
    },

    /// Speak MCP on stdin/stdout, so an AI that cannot open a folder can still reach one. Started by
    /// the host that reads it, never by hand: what goes over the two streams is JSON-RPC, so a terminal
    /// gets nothing out of typing this.
    ///
    /// It is a thin mediator and nothing else — every tool call runs this same executable again as a
    /// child process, in the folder that call named, and hands back what that run wrote (`AMB-D-665`).
    /// Which project the folder belongs to is the `.amenbo` pointer's answer there, exactly as it is
    /// for a person typing in it.
    ///
    /// One server, as many folders as `--dir` was given (`AMB-D-679`): the set is the person's to
    /// choose, and every call names one of it — including when it holds a single folder, so a call
    /// never leaves where it lands to a default.
    ///
    /// Three tools (`AMB-D-667`): `agent`, the whole of how to work here; `agent_command`, one
    /// command's spec; and `run`, which types the caller's own words. Two things are named rather than
    /// passed through — the facet is this server's to declare (`AMB-D-668`), and `bind` and `init` are
    /// refused, either of them being a way for an AI to re-point a folder it was given.
    Mcp {
        /// the folders this server works in, one call in one of them. The project is whatever each
        /// one's `.amenbo` points at
        #[arg(long, value_name = "PATH", num_args = 1.., required = true)]
        dir: Vec<String>,
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
    /// has exactly one gate, at the layer its author declared — `AMB-D-434`/`AMB-D-601`). `run` is the one command that
    /// actually *calls* a plugin on purpose: its command face, whose return value comes back to you
    /// (`AMB-D-353`). `validate` is
    /// the author's side — it runs the same rules Amenbo enforces at
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
    ///
    /// The translations beside it are read with it (`AMB-D-621`): every sibling
    /// `<name>.<lang>.<ext>` is checked against the manifest it translates — the language is one Amenbo
    /// is read in, everything it names exists in the base, and the text obeys the cap its base field
    /// obeys.
    Validate {
        /// path to the manifest file (`plugins/<name>.yaml` or a `.json` manifest)
        path: String,
    },

    /// List the plugins installed on this machine, and whether each one is enabled (`AMB-D-350`/`AMB-D-351`).
    /// Reads only what is on disk under the app-data `plugins/` directory plus this machine's enable
    /// state — no network, no catalog. A row names every project holding the plugin's switch open
    /// (`AMB-D-434`/`AMB-D-412`) rather than answering yes/no from wherever the terminal happens to stand —
    /// or, for a plugin whose author declared the machine layer, whether its one device gate is open
    /// (`AMB-D-601`).
    List,

    /// Install a plugin from the catalogs: resolve the name across the official catalog and every
    /// registered one (official wins a clash), fetch its asset, verify its provenance fail-closed — the
    /// signature against the key that catalog answers for, Amenbo's own or the one pinned at registration
    /// (`AMB-D-371`/`AMB-D-389`), then the checksum of the distributable published for this OS
    /// (`AMB-D-351`/`AMB-D-381`) — and lay it down under the app-data `plugins/` directory, with a note of
    /// which catalog it came from, which is where `plugin update` will go back to. **Installing never
    /// enables**: the plugin lands inert and `plugin enable` is the separate act. A name already installed
    /// is refused rather than overwritten (`AMB-D-360`).
    ///
    /// What the author declared about the plugin's layer is said here, in words (`AMB-D-601`): a plugin
    /// that declares `scope: machine` is the device's, so enabling it lets it read every project on this
    /// machine, and that is worth knowing before the gate is opened rather than after. A plugin that
    /// declares nothing is a project's and says nothing extra.
    Install {
        /// the plugin's name, as the catalog lists it
        name: String,
    },

    /// Open an installed plugin's gate and let it fire (`AMB-D-351` — `install ≠ enable`, so nothing runs
    /// until this; and doing it is itself the permission to run the plugin's code). The switch is the
    /// project's you are in (`AMB-D-434`), so it needs a bound folder, and turning it on elsewhere is a
    /// separate act — unless the author declared the machine layer, where the one switch is the device's
    /// and opening it is also the permission to read every project on this machine (`AMB-D-601`). There is
    /// still no `--scope`: a plugin has one switch, and where it sits is the manifest's to say, not this
    /// command's. Refused while a setting the author
    /// marked `required` is still empty; fill it with `plugin config set` first. Refused too when
    /// the plugin is not compatible with this build — a different payload contract, or a floor above the
    /// running version (`AMB-D-359`). And refused when the plugin's **own check** says no: a call the
    /// author declared under `settings`, run here with the values in hand (`AMB-D-664`), where a check
    /// that fails, hangs or answers unreadably counts as a no. The refusal names the settings the check
    /// spoke about; what it *said* about them is drawn on the settings screen in the app, and the run
    /// itself is on `plugin log`.
    Enable {
        /// the installed plugin's name
        name: String,
    },

    /// Close an enabled plugin's gate — the same one switch `enable` opens (`AMB-D-434`). The plugin stays
    /// installed, so a later `enable` costs nothing (`disable ≠ uninstall`, `AMB-D-357`). The manifest is
    /// read for one thing only, which layer that switch is at (`AMB-D-601`); a manifest that cannot be read
    /// falls back to the project layer, so a broken install is stopped just the same.
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
    /// `eval "$(amenbo plugin run worktree start 123)"`, or `iex (amenbo plugin run worktree start 123)`
    /// in PowerShell — the line a plugin returns is written to go through either (`AMB-D-444`), because
    /// Amenbo hands it over without knowing which shell asked. Its stderr — the human-facing diagnostics — is
    /// relayed to stderr, and a plugin that exits non-zero is a failed call whose return value is
    /// discarded rather than handed on (`AMB-D-354`).
    ///
    /// Everything after the name is the plugin's, passed through untouched — dashes and all: Amenbo
    /// neither parses nor rewrites it, because what the words mean is the plugin's business (`AMB-D-346`).
    /// That covers `--help`, which is where a plugin's author puts its usage: the word travels through and
    /// the plugin answers it. Only the form that names no plugin — `amenbo plugin run --help` — has nobody
    /// else to answer, and there the help you get back is this command's.
    ///
    /// The corollary is that Amenbo's own flags have to come *before* the plugin's name
    /// (`amenbo plugin run --json worktree …`), since after it every word is the plugin's. Refused when the
    /// plugin is not installed, not enabled (`install ≠ enable`, `AMB-D-351`), or not compatible with this
    /// build (`AMB-D-359`) — a caller waiting on a return value is told why there is none.
    #[command(disable_help_flag = true)]
    Run {
        /// the installed plugin's name
        #[arg(allow_hyphen_values = true)]
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
    /// Under it, one `waiting` line per plugin that still owes something: how many events are on its queue,
    /// since when, and whether a runner is on it. Nothing ran and nothing is queued is a working store; a
    /// queue piling up with nobody running it is a plugin that stopped, which the runs below cannot show,
    /// because what did not run wrote no line. Nothing is printed when nothing is waiting.
    ///
    /// Reads one machine-local file and a few store rows, and nothing else — no network. The log is bounded
    /// by construction (the last runs of each installed plugin), so there is no window to ask for and no
    /// deep history to page: a longer one is a logging plugin's business.
    Log {
        /// narrow to one plugin's runs; omit for every plugin's, newest first
        name: Option<String>,
    },

    /// Deliver what is waiting on the plugins' queues **now**, and report what each one got through
    /// (`AMB-D-399`).
    ///
    /// Delivery normally rides along with whatever you were doing: a write fans its events out and starts a
    /// runner per queue, and Amenbo makes no command wait for a plugin. When a runner is killed mid-queue,
    /// or a fan-out dies half-done, what is left waits for the next write — which may be days away. This is
    /// the door for pushing it through on purpose: the queues are worked **in this process**, so it returns
    /// once they are empty and can say how much left each one, rather than starting a runner nobody watches.
    ///
    /// It reports per plugin how many events left its queue and how many are still on it. A queue with a
    /// live runner on it is left alone and named as such: two runners on one queue is what the lease exists
    /// to prevent. Nothing waiting is not an error — it says so and exits 0.
    ///
    /// **How each run ended is not here**: a failed delivery is dropped rather than retried (`AMB-D-399`),
    /// and `plugin log` is where every run's outcome and the plugin's own diagnosis are written
    /// (`AMB-D-361`). This says what moved; the log says how it went.
    Flush,

    /// Bring an installed plugin onto the build the catalog publishes — or, with `--check`, only report
    /// which installs it has moved past (`AMB-D-359`).
    ///
    /// **A plugin is updated from the catalog it was installed from** (`AMB-D-389`), which Amenbo recorded
    /// beside it. Not from whichever catalog carries the name today: a second catalog publishing a name you
    /// already have offers your install nothing, so a distributor cannot change hands under an update. The
    /// visible edge of that is a plugin whose own catalog has dropped it — there is no build to move to, and
    /// the refusal names the catalog it asked. An install made before Amenbo recorded this is looked for in
    /// the official catalog; re-installing it records where it really comes from.
    ///
    /// Detection is the catalog Amenbo already fetches whole laid beside the manifest sitting next to
    /// each installed binary — no central server, and no per-plugin request. A manifest carries no
    /// version number, so what is compared is `detail_sum` (`AMB-D-438`): one digest per catalog entry,
    /// over the whole document an install acts on — the assets, the config schema, the compatibility
    /// floor, and what the plugin says for itself at the AI's entry point. That is why **an update which
    /// changes no binary is still an update**: comparing the executables would hide every one of them.
    /// The asset checksums are still checked where they mean something (`AMB-D-381`), at the install
    /// door — the bytes that arrive must be the bytes the entry published. It reports *different*, not
    /// *newer* — the catalog is the authority on what is published, including a rollback.
    ///
    /// **Nothing is ever applied on Amenbo's own account**: naming a plugin, or `--all`, is the whole
    /// consent. Applying re-walks the install door over the new asset — the catalog signature, then this
    /// OS's checksum (`AMB-D-351`) — and retains the build it replaced beside the new one, so there is
    /// something to go back to. It **keeps** the plugin's gate and every setting the new build still
    /// declares: an update is not a re-install, and wiping a plugin's settings wholesale is `uninstall`'s
    /// job (`AMB-D-357`). What it does take is a value under a key the new build has **stopped** declaring
    /// (`AMB-D-456`) — nothing would read it again, and a rollback does not bring it back. Any step that refuses —
    /// a build this Amenbo cannot speak to, an asset that will not verify, a new schema whose `required`
    /// settings have no value where the plugin is enabled — leaves the working plugin exactly as it was.
    /// That last check is asked of **every** gate the plugin is enabled at, not of the folder you happen to
    /// be in (`AMB-D-434`): one update replaces the build for all of them, so a project short of a value
    /// holds it back from anywhere. `plugin config set` is the way past it.
    ///
    /// `--check` is cheap on purpose: with nothing installed no catalog is read at all, and otherwise a
    /// cached catalog younger than an hour answers with no request. It says which of the two it did, so
    /// "nothing has changed" is never read for "nothing had changed an hour ago"; `--check --fresh` fetches
    /// the index first when that distance matters — after publishing, say. Applying always asks for the
    /// current index, since replacing a binary on an hour-old answer is not the same bargain.
    Update {
        /// the installed plugin to update; omit it with --all or --check
        name: Option<String>,
        /// report what has an update without applying anything
        #[arg(long)]
        check: bool,
        /// apply every update the catalog holds, one plugin at a time
        #[arg(long)]
        all: bool,
        /// with --check: fetch the catalog now instead of letting a cache under an hour old answer
        #[arg(long)]
        fresh: bool,
    },

    /// Undo the last `plugin update` for one plugin, restoring the build it retained (`AMB-D-359`).
    ///
    /// An update kept the previous executable and its manifest as a `.bak` pair beside the new ones; this
    /// puts both back — the same shape self-update's `update --rollback` uses (`AMB-D-341`). Offline and
    /// instant: nothing is fetched and nothing is re-verified, because the retained build already passed
    /// the door on its way in and a rollback is a deliberate return to it. It leaves the gate, the
    /// settings and the secrets alone — including a value the update purged for want of a declaration,
    /// which stays gone: what is retained is one generation of the build and its manifest (`AMB-D-456`).
    ///
    /// Goes back **one** build, and only one: the retained copy is consumed, so a second rollback has
    /// nothing to restore and says so. Refused, changing nothing, when the plugin is not installed or was
    /// never updated (there is no retained build to return to).
    Rollback {
        /// the installed plugin to roll back
        name: String,
    },

    /// Fill in an installed plugin's settings — the keys its author declared in the manifest
    /// (`AMB-D-356`). Where a value is kept is the author's `secret` flag's to decide, not yours: a
    /// secret goes to the store table an `export` must leave, the rest to the ordinary one. Either
    /// way it is this project's value and there is no tier under it (`AMB-D-434`).
    Config {
        #[command(subcommand)]
        sub: PluginConfigCmd,
    },

    /// Register the third-party catalogs to browse and install from alongside the official one
    /// (`AMB-D-347`, the "free" tier), and list what is registered. The unit is the **catalog**, not the
    /// plugin: what grows is the number of indexes (few), never per-plugin requests. Registering one adds
    /// a trust root — its plugins are verified against the key it publishes, pinned when you agree to the
    /// fingerprint (`AMB-D-389`) — so a catalog that publishes no key can only be browsed.
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

    /// Register a third-party catalog by the URL of its `catalog.json`, pinning the signing key it
    /// publishes beside it. The fingerprint is shown and confirmed first: plugins installed from this
    /// catalog are trusted on that key, so registering one is a trust decision (`--yes` to confirm
    /// non-interactively). A catalog that publishes no key registers without a question and can only be
    /// browsed. A catalog that has changed its key is refused — unregister it and register it again to
    /// trust the new one. Idempotent, and refuses a non-`http(s)` URL and the official catalog's own URL.
    /// The catalog is fetched once here so the first browse is warm; an unreachable URL still registers
    /// (it will be retried on the next browse).
    Add {
        /// the URL of the third-party catalog's `catalog.json`
        url: String,
        /// what to call this catalog on screen (default: the host of its URL)
        #[arg(long)]
        name: Option<String>,
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
    /// Store one setting's value, for this project (`AMB-D-434`). The key must be one the plugin's
    /// manifest declares — that declaration is what says whether the value is a secret, and Amenbo never
    /// guesses (`AMB-D-356`). An empty value clears the setting rather than storing a blank.
    ///
    /// A setting that offers candidates takes them comma-separated, and `none` to choose none of them;
    /// anything else is refused here rather than stored to match nothing later (`AMB-D-415`). Run
    /// `plugin config get` to see the candidates and what is in force.
    Set {
        /// the installed plugin's name
        name: String,
        /// the setting's key, as the manifest declares it
        key: String,
        /// the value; `-` reads it from stdin (which keeps a secret off argv and out of shell history),
        /// and an empty string clears the setting (a setting with a default goes back to it)
        value: String,
    },

    /// Read one setting back, as this project holds it, and — for a setting that offers candidates — what
    /// it offers, with what is in force ticked (`AMB-D-415`). A secret is never echoed: it reports only
    /// whether one is set.
    ///
    /// What the author wrote about the setting is here too (`AMB-D-656`): the paragraph explaining it, and
    /// a note where the value is the plugin's own to write rather than yours to type.
    Get {
        /// the installed plugin's name
        name: String,
        /// the setting's key, as the manifest declares it
        key: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum AgentHookCmd {
    /// Print the request that has one AI tool wired, for the reader to give the AI they work with —
    /// it carries the settings this build's launch instruction goes in, the file they belong in, and
    /// that whatever is already in that file stays. **stdout is that text and nothing else**, so it
    /// pipes to a clipboard (`amenbo agent-hook snippet claude-code | pbcopy`); where it is going, and
    /// that Amenbo wired nothing, is said on stderr. `--copy` hands it to this machine's clipboard
    /// instead, printing it on stderr as it goes so it is read before it is handed on. Opens no store:
    /// it needs no bound folder, and reads nothing about this one.
    Snippet {
        /// the AI tool to be wired
        #[arg(value_parser = harness_ids())]
        tool: String,
        /// put it on this machine's clipboard instead of printing it
        #[arg(long)]
        copy: bool,
    },

    /// Record what a person answered when asked whether this folder's AI may be started on Amenbo —
    /// the way an AI writes back an answer it obtained on Amenbo's behalf, since Amenbo puts no
    /// question to a non-interactive face. **It records the answer and touches nothing else**: no
    /// settings file is read or written here, so a `yes` still leaves the wiring to be done
    /// (`agent-hook snippet <tool>` is the text that asks for it), and a `no` only means Amenbo stops
    /// asking — the text stays available. The answer is kept per project, so it covers every folder
    /// bound to it.
    Answer {
        /// what the person answered
        #[arg(value_parser = ["yes", "no"])]
        answer: String,
    },
}

/// The tool names `agent-hook snippet` takes, read off the catalog itself (`AMB-D-440`) — so a harness
/// added there is offered in `--help` and accepted here with nothing else to update, and a name nobody
/// lists is refused by clap with the whole list rather than by a branch further in.
fn harness_ids() -> clap::builder::PossibleValuesParser {
    clap::builder::PossibleValuesParser::new(
        amenbo_core::harness::HARNESSES.iter().map(|harness| harness.id).collect::<Vec<_>>(),
    )
}

#[derive(Subcommand, Debug)]
pub enum HooksCmd {
    /// Write the lint hooks, and record that this project consented. A slot Amenbo did not write is
    /// stepped around rather than overwritten — the rest are still wired, and the one line to add by
    /// hand is named; re-running over Amenbo's own hooks rewrites them, which is how a newer build's
    /// hooks land. Only an install with no slot to write at all is refused.
    Install,

    /// Remove the lint hooks Amenbo wrote, and record that this project does not want them. The mirror
    /// of install, refusal for refusal: a hook Amenbo did not write is not Amenbo's to delete, and with
    /// nothing of ours there it records the answer and does nothing else.
    Uninstall,

    /// Show what is in each hook slot and what this project answered — the two facts, side by side.
    Status,
}

#[derive(Subcommand, Debug)]
pub enum TickCmd {
    /// Register the hourly tick, and record that this device consented. Idempotent: run over a
    /// registration that is already there, it writes it again, which is how the timer comes to name
    /// the build running now after an upgrade.
    Install,

    /// Take the registration away, and record that this device does not want it. Unlike the lint's,
    /// this is a device-wide no, because the device is the only scale a timer has — and it closes the
    /// question rather than the door: `tick install` registers it again whenever you want it back. On
    /// macOS the row outlives this: the OS keeps its own record of the item, so it stays in your login
    /// items with nothing behind it.
    Uninstall,

    /// Show what the scheduler is holding and what this device answered — the two facts, side by
    /// side. They are read independently on purpose, so a registration you switched off yourself is
    /// something Amenbo can see rather than something it talks over.
    Status,

    /// The face the scheduler itself calls, once an hour (`AMB-D-706`). Hidden because a scheduler calls
    /// it — never a hand. It is not a daemon: it starts, judges what the calendar day owes, works what the
    /// plugin queues still hold, and exits, so nothing is running between two ticks.
    ///
    /// Being woken is not being due: the timer carries no meaning, so an hour that is owed nothing is the
    /// ordinary case, and what is owed is counted in calendar days rather than in wake-ups (`AMB-D-708`).
    /// A round with nothing owed is still not a wasted one — it works the queues, which is where a
    /// delivery a killed runner left standing gets picked up.
    ///
    /// It resolves no folder and takes no facet: a scheduler runs it from wherever it happens to stand,
    /// and what it works is this device's one store. On a device holding no store there is nothing to do,
    /// and it says nothing and exits 0 rather than raising one on a schedule.
    #[command(hide = true)]
    Run,
}

#[derive(Subcommand, Debug)]
pub enum SyncCmd {
    /// Ask what version this window is at — one number, and the only question worth asking often.
    ///
    /// It moves whenever something in the window is written and stays put when nothing is, so a carrier
    /// that remembers the last number it sent can tell in one cheap call whether there is anything to
    /// send. **Compare it for inequality, not for order**: a `restore` winds the store back, and the
    /// version comes back with it — lower is still changed.
    ///
    /// No snapshot is built to answer this (`AMB-D-582`).
    Version,

    /// Take one whole picture of what this window may see, as JSON on stdout.
    ///
    /// Every table comes from **one instant**, so nothing in it refers to something that is not in it,
    /// and a plugin's secrets stay home (`AMB-D-434`). Records only: an attachment's row travels, its
    /// bytes do not (`AMB-D-588`).
    ///
    /// This is a **carrier's** road, not a reading one — the whole window lands on stdout, so send it
    /// somewhere, do not read it. To look at the work, use `task list` / `task show`.
    Snapshot,

    /// Read what has changed in this window since a cursor — the unread, and the cursor to come back
    /// with.
    ///
    /// What comes back is which records moved and how (`insert` / `update` / `delete`), not what they now
    /// hold: read a changed record back by name, and drop a deleted one. The cursor to start from is the
    /// one a `sync snapshot` names in its header; each answer names the next.
    ///
    /// A page is bounded, and says when it cut one short — come straight back with the cursor it handed
    /// you. When the cursor has fallen out of the ledger's window the answer is a **gap**
    /// (`sync_gap`), not an empty page: take a fresh `sync snapshot` and read on from the cursor it
    /// names (`AMB-D-583`).
    Changes {
        /// the cursor to read on from — everything after it, exclusive (0 reads from the oldest the
        /// ledger still holds)
        #[arg(long, value_name = "CURSOR")]
        since: i64,
    },

    /// Read named records back, in the shape a `sync snapshot` carries them in.
    ///
    /// This is where `sync changes` leads: it names which records moved, never what they now hold, so the
    /// rows are read back here rather than by taking the whole window again. The dataset is the name the
    /// changes named it by, and the ids are that call's `record_id`s.
    ///
    /// An id this window does not reach, and an id that is no longer there, both simply come back absent
    /// — a `delete` in the changes already said which is which. One read answers a page of changes at
    /// most; past that, ask in pages.
    Records {
        /// which records — the dataset name `sync changes` gave (`task`, `dependency`, …)
        #[arg(long, value_name = "NAME")]
        dataset: String,
        /// the record ids to read back, comma-separated (`--ids 12,15,31`)
        #[arg(long, value_name = "IDS", required = true, value_delimiter = ',')]
        ids: Vec<i64>,
    },
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
        /// The folder this project is linked to — an existing directory, which receives the
        /// `.amenbo` pointer and the managed guidance block, exactly as `bind` places them. Required:
        /// a project nothing is linked to is a project no AI can reach.
        #[arg(long)]
        dir: String,
        /// The view this project opens on: list | board | calendar | timeline. Omitted, the
        /// configured `default_view` answers (`config set default_view <view>`).
        #[arg(long)]
        view: Option<String>,
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
        /// display name for this axis (no whitespace: a filter names it as `dim:<axis>=<value>`)
        #[arg(long)]
        name: String,
        /// description / notes (Markdown)
        #[arg(long, default_value = "")]
        notes: String,
        /// how many of this axis's values one record may hold: `single` or `multi` (default: single)
        #[arg(long)]
        cardinality: Option<String>,
        /// give the values an explicit order (default: unordered)
        #[arg(long)]
        ordered: bool,
        /// mark this axis as the project's time axis — its values carry periods
        #[arg(long)]
        time_axis: bool,
        /// mark this axis to show on the task card (default: not marked)
        #[arg(long)]
        show_on_card: bool,
        /// require a value on this axis before a creation can be finished (refused here: a new axis has no values yet)
        #[arg(long)]
        required: bool,
        /// which side this axis classifies: `task`, `decision` or `both` (default: both)
        #[arg(long)]
        applies_to: Option<String>,
        /// readable key for naming this axis outside Amenbo (lower-case letters, digits and hyphens, starting with a letter; defaults to `d<id>`)
        #[arg(long)]
        slug: Option<String>,
    },
    /// List a project's dimensions (display order) with their values
    List {
        /// project (name or ID; defaults to the bound project)
        #[arg(long)]
        project: Option<String>,
    },
    /// Show a dimension (name, notes, cardinality/ordered/role/card/applies-to, values)
    Show {
        /// dimension ref (AMB-DIM-n), slug or name
        id: String,
    },
    /// Update a dimension's name, notes, how many values one record may hold, value ordering, time-axis role, whether it goes on the task card, whether it must be answered, which side it classifies, and/or its slug (only the given fields change)
    Update {
        /// dimension ref (AMB-DIM-n), slug or name
        id: String,
        /// rename this axis (no whitespace: a filter names it as `dim:<axis>=<value>`)
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        notes: Option<String>,
        /// how many of this axis's values one record may hold (`--cardinality single|multi`); going back to single is refused while records answer with several
        #[arg(long)]
        cardinality: Option<String>,
        /// whether the values carry an explicit order (`--ordered true|false`)
        #[arg(long)]
        ordered: Option<bool>,
        /// whether this axis is the project's time axis (`--time-axis true|false`)
        #[arg(long)]
        time_axis: Option<bool>,
        /// whether this axis is marked to show on the task card (`--show-on-card true|false`)
        #[arg(long)]
        show_on_card: Option<bool>,
        /// whether a task must carry a value here before its creation can be finished (`--required true|false`)
        #[arg(long)]
        required: Option<bool>,
        /// which side this axis classifies (`--applies-to task|decision|both`); narrowing it takes no assignment away
        #[arg(long)]
        applies_to: Option<String>,
        /// rename the readable key this axis is named by outside Amenbo
        #[arg(long)]
        slug: Option<String>,
    },
    /// Reorder a dimension within its project
    Move {
        /// dimension ref (AMB-DIM-n), slug or name
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
        /// dimension ref (AMB-DIM-n), slug or name
        id: String,
    },
    /// Add a value to a dimension (appended after existing values)
    ValueAdd {
        /// dimension ref (AMB-DIM-n), slug or name
        dimension: String,
        /// display name for this value (no whitespace: a filter names it as `dim:<axis>=<value>`)
        #[arg(long)]
        name: String,
        /// first day of the value's period (time-axis dimensions only)
        #[arg(long)]
        start: Option<String>,
        /// last day of the value's period; omit to leave it ongoing (time-axis dimensions only)
        #[arg(long)]
        end: Option<String>,
        /// readable key for naming this value outside Amenbo (lower-case letters, digits and hyphens, starting with a letter; defaults to `v<id>`)
        #[arg(long)]
        slug: Option<String>,
    },
    /// Update a dimension value's name, slug and/or period (only the given fields change)
    ValueUpdate {
        /// dimension ref (AMB-DIM-n), slug or name
        dimension: String,
        /// value ref (AMB-DIMV-n), slug or name (within the dimension)
        value: String,
        /// rename this value (no whitespace: a filter names it as `dim:<axis>=<value>`)
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
        /// rename the readable key this value is named by outside Amenbo
        #[arg(long)]
        slug: Option<String>,
    },
    /// Reorder a value within its dimension
    ValueMove {
        /// dimension ref (AMB-DIM-n), slug or name
        dimension: String,
        /// value ref (AMB-DIMV-n), slug or name (within the dimension)
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
    /// Delete a dimension value permanently; its task assignments go with it unless --reassign-to
    /// moves them. On a required axis: assignments demand --reassign-to, and the last value is refused
    /// — lower the requirement first (alias: value-delete)
    #[command(alias = "value-delete")]
    ValueRm {
        /// dimension ref (AMB-DIM-n), slug or name
        dimension: String,
        /// value ref (AMB-DIMV-n), slug or name (within the dimension)
        value: String,
        /// move the tasks classified as this value to another value of the same dimension, instead of
        /// letting their classification go with it (ref, slug or name)
        #[arg(long, value_name = "VALUE")]
        reassign_to: Option<String>,
    },
    /// Assign a task or a decision a value of a dimension (single-select replaces its prior value on
    /// that axis)
    Set {
        /// task or decision ref (AMB-T-n / AMB-D-n). A bare number is refused: the two number
        /// independently, so the same digits name a row on each side
        target: String,
        /// dimension ref (AMB-DIM-n), slug or name
        dimension: String,
        /// value ref (AMB-DIMV-n), slug or name (within the dimension)
        value: String,
    },
    /// Clear a task's or a decision's value of a dimension
    Unset {
        /// task or decision ref (AMB-T-n / AMB-D-n). A bare number is refused: the two number
        /// independently, so the same digits name a row on each side
        target: String,
        /// dimension ref (AMB-DIM-n), slug or name
        dimension: String,
        /// value ref (AMB-DIMV-n), slug or name (within the dimension)
        value: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum TaskCmd {
    Add {
        #[arg(long)]
        title: String,
        /// owning project (name or ID). A human names one, or omits it to list the
        /// projects to pick from; an AI omits it — the binding fills the slot, and
        /// naming a project is refused.
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
        /// the bound folder this task is to be worked in — one of the project's own linked folders,
        /// named by its path or just its folder name (`--at amenbo-plugin-mail`). Only what you name
        /// here lands: the folder the create was typed in is never taken as the default. Having one
        /// refuses nothing — no reservation and no worktree is stopped for it
        #[arg(long, value_name = "FOLDER")]
        at: Option<String>,
    },
    /// Finish creating a task — the second stage of every create. `task add` leaves the task still
    /// being created: it is on the board and in every listing, but held out of the mailbox and refused
    /// a reservation, so dependencies, premises and classification can be drawn before anyone picks it
    /// up. This says the writing is finished. It asks for nobody's approval — the one who created it is
    /// the one who ends the creation — and it runs one way (a task filed by mistake ends with
    /// `task reject` or `task delete`)
    FinishCreating {
        /// target task ref (AMB-T-n)
        id: String,
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
        /// the bound folder this task is to be worked in — one of the project's own linked folders,
        /// named by its path or just its folder name (`--at amenbo-plugin-mail`)
        #[arg(long, value_name = "FOLDER")]
        at: Option<String>,
        /// forget the folder this task named (it goes back to naming none)
        #[arg(long)]
        clear_at: bool,
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
        /// display label (defaults to the file name / URL). On a file it keeps that file's suffix,
        /// so `attach save` and `attach open` still have one to work from, and what the file is stays
        /// read from the file rather than from this.
        #[arg(long)]
        name: Option<String>,
    },
    /// Record / list / forget the git commit SHAs that implemented a task — the anchor from
    /// history back to a task (Amenbo stores each SHA opaquely and never reads git)
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
/// public commit carries no store-local reference. Amenbo stores each SHA as an opaque full-length
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
        /// display label (defaults to the file name / URL). On a file it keeps that file's suffix,
        /// so `attach save` and `attach open` still have one to work from, and what the file is stays
        /// read from the file rather than from this.
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
        /// classify the new decision as `<axis>=<value>` — the same resolution as `dimension set` (id, or
        /// an exact name, case-insensitive). Repeatable for different axes; an axis is single-select, so
        /// naming one twice is refused, and so is an axis that does not classify decisions. A required
        /// axis left empty is what `decision accept` refuses later, so fill it here.
        #[arg(long = "dim", value_name = "AXIS=VALUE")]
        dim: Vec<String>,
    },
    /// List decisions (filter by status:/superseded:/project:/number: (alias ref:, e.g. `D-<n>`/`#<n>`)/task: (the decisions a task rests on, e.g. `task:#<n>`)/`dim:<axis>=<value>` and its `time_axis:<value>` sugar (the same axes tasks are classified on: different axes AND, the same axis ORs, `=none` is unclassified)/decided_before:/decided_after: (the day a decision was accepted, YYYY-MM-DD or today/-30d; both ends inclusive), sort by decided/created/number/title/status). Words are not a key here — `amenbo search <word> --kind decision` finds where they are written
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
    /// Promote a comment into a decision (the comment text becomes the body; a task comment links to its task)
    Promote {
        /// the comment ref to promote, AMB-TC-n (on a task) or AMB-DC-n (on a decision)
        comment: String,
        #[arg(long)]
        title: String,
        /// project (defaults to the project of the comment's task or decision)
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
        /// display label (defaults to the file name / URL). On a file it keeps that file's suffix,
        /// so `attach save` and `attach open` still have one to work from, and what the file is stays
        /// read from the file rather than from this.
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
        /// display label (defaults to the file name / URL). On a file it keeps that file's suffix,
        /// so `attach save` and `attach open` still have one to work from, and what the file is stays
        /// read from the file rather than from this.
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
