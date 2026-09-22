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

/// The command tree's root, and the one line drawn across it (`AMB-D-757`).
///
/// **A command belongs to the core or to the talk window, decided here rather than at run time.** The
/// question is whether it would mean anything typed outside that window: if it would, it is core, and it
/// sits at this level. **The line is not drawn on capability** — what only works in the window today may
/// work anywhere tomorrow, and a line drawn on that leaves the verbs behind when it moves. Where it runs
/// changes; where it belongs does not.
///
/// **What the window's own namespace ([`Command::Talk`]) takes is narrower still**: only what the window
/// knows and the store cannot record. Anything that would still be true tomorrow goes to the core, whoever
/// happens to be able to say it.
///
/// **The surface layer is closed by default.** A new verb goes to the core unless it is argued past both
/// of the above.
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
    /// (resolved from the published latest.json) in your browser — and where that cannot be read,
    /// says so rather than opening a page of its own choosing.
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
    /// the comments on both, the labels either is filed under, the names of what is attached and the
    /// documents an automation's steps share, and answers with the face the words landed on, the record
    /// it belongs to, and a short excerpt.
    ///
    /// Words are ANDed and match as substrings (no word boundaries, so part of a compound word finds
    /// it); full-width,
    /// case and kana spellings are brought together. Every word has to land somewhere on the record — not
    /// all on one face — and each face that carries one is a line, which is what makes the answer "here is
    /// where each of your words is written". A word written as a ref (`AMB-T-<n>` / `AMB-D-<n>`) pins that
    /// record to the top, so holding a number takes the same command as holding a phrase. A number alone
    /// (`12` / `#12`) is a ref as well: the two sides number themselves apart, so it pins the task and the
    /// decision that carry it, and `--kind` keeps the side you named.
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
        /// `--kind`, and takes neither `automation` nor no side at all: the two grammars share spellings
        /// that mean different things, and an automation has no listing to take one from
        #[arg(long)]
        filter: Option<String>,
        /// keep one side: task / decision / automation (the documents an automation's steps share)
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
    /// Decisions (the premises that hold now; a Task sibling, not a task)
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
    /// One thing stays behind: your **secrets** (`AMB-D-884`) — a notification connection, the Viewer's
    /// keys. This file goes out to another tool and stays in its hands, and a credential in the clear is
    /// not something to hand over on the way past — they ride `backup`, which leads back to your own store,
    /// instead.
    Export {
        /// The **export directory** to create — `export.json` plus `attachments/` with every
        /// attachment's bytes. Must not exist yet. With no `--out` the dump streams to stdout (records
        /// only — no attachments), except on a closed reach, which is handed a directory named
        /// `amenbo-export-<UTC stamp>` under the current folder instead of the stream.
        #[arg(long)]
        out: Option<String>,
    },
    /// Back up this device — its store and its attachment bytes — into a single
    /// verified `.amenbo-backup` archive at `path`. The store is snapshotted with `VACUUM INTO`
    /// (checkpointed, no torn DB+WAL) and bounded-verified; a `manifest.json` records its migration
    /// generation. The device's own secrets (at-rest key / identity) are never included; your notification
    /// and Viewer secrets are store rows, so those ride along and come back working (`AMB-D-884`). The
    /// destination must not already exist.
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

    /// A worktree of this task's own — cut when the work starts, folded when its commits have landed
    /// (`AMB-D-881`).
    ///
    /// **Where it goes is not a question anyone is asked**: `<the repository's parent>/<its
    /// name>-worktrees/<id>`, on `task/<id>`. Beside the project rather than inside it, because a
    /// checkout cut within inherits the project's `.amenbo` and would drive the real backlog from a
    /// throwaway folder — which is what Amenbo refuses to run in (`nested_worktree`).
    ///
    /// Run it in the repository the task is worked in. What is cut is derived from the folder the
    /// command was typed in, never from the task, so a `start` typed in the wrong repository would
    /// otherwise hand back a checkout of a different project — and `start` refuses when the task names
    /// a folder that lies in another repository (`AMB-D-649`).
    Worktree {
        #[command(subcommand)]
        sub: WorktreeCmd,
    },

    /// The **surface layer**: what you say about the session you are running in, inside the talk window's
    /// terminal (`AMB-D-749`). It moves the pane on the person's screen and writes to no store — nothing
    /// said here outlives the window, and every verb of it **fails outside one**, loudly, rather than
    /// answering "ok" where nothing was shown.
    ///
    /// Named for the window it speaks to (`AMB-D-757`), so the namespace and the boundary are one word:
    /// under `talk` is the surface layer, and everywhere else is the core.
    ///
    /// Hidden because it is not a command for every place `amenbo` runs: `--help` is read in terminals
    /// this vocabulary does not exist in, and `agent --json` is read in all of them. The canon that
    /// teaches it is `talk --json`, which is inside the window, where it can be used.
    ///
    /// It declares no `--actor`: the facet says who is writing to the store, and this writes to none.
    #[command(hide = true)]
    Talk {
        /// A verb of the layer. With none, the canon is printed — the vocabulary and what is owed
        /// (`--json` for the machine-readable shape). A bare word that is not a verb is refused rather
        /// than taken as something to say: this is not the mouth that talks to the agent.
        #[command(subcommand)]
        sub: Option<TalkCmd>,
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

    /// The entry point a **notification sender** is launched through: it posts one drive's worth of
    /// messages and exits (`AMB-D-885`, `AMB-D-352`). Hidden because Amenbo launches it — never a hand.
    ///
    /// The messages arrive on stdin, already worded, because a burst is a paragraph of text and a command
    /// line is a place with a length limit and every process list on the machine reading it. The store is
    /// an argument rather than resolved: it posts through the connections of the store the drive that
    /// launched it drove, not whichever one its own working directory would answer with.
    #[command(hide = true)]
    NotifySender {
        /// the base directory of the store whose connections to post through (app-data, or `AMENBO_HOME`)
        store: String,
    },

    /// The entry point a **Viewer carrier** is launched through: it takes one turn of the send and exits
    /// (`AMB-D-884`). Hidden because Amenbo launches it — never a hand.
    ///
    /// Nothing arrives with it: what a carrier is to carry is in the store. The store is an argument for
    /// the reason a notification sender's is — it carries the store the write that launched it wrote to.
    /// A run that finds another carrier already taking the turn stops at once, which is what keeps a burst
    /// of writes from becoming a burst of turns.
    #[command(hide = true)]
    ViewerCarrier {
        /// the base directory of the store to carry (app-data, or `AMENBO_HOME`)
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

    /// The skins this device holds: what is in, what is on, and the two faces an author needs — a
    /// check that says why a file would be turned away, and a starting point that is this build's
    /// own colours. A skin is a file somebody was handed, so it is taken in from a path rather than
    /// fetched: there is nowhere to fetch one from. One is worn at a time, so putting one on is a
    /// choice among what is held rather than a switch, and `use none` is the way back to the
    /// colours Amenbo ships with. All of it is here and not only in the window, because a skin's one
    /// failure mode is a window nobody can read.
    Skin {
        #[command(subcommand)]
        sub: SkinCmd,
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


    /// Notifications: the connections this device can send through, and what this project reports
    /// (`AMB-D-885`).
    ///
    /// A connection is written **here once, under a name** — the device's shelf — and a project selects
    /// from it. So a webhook that changes is one edit rather than one per project, and reading where a
    /// project's notifications go takes one place.
    ///
    /// With no sub-command it shows both halves: the shelf, and what the bound project does with it.
    Notify {
        #[command(subcommand)]
        sub: Option<NotifyCmd>,
    },

    /// Amenbo Viewer: the server this store is read from on a phone, and which phone may read it
    /// (`AMB-D-884`).
    ///
    /// **The server is the reader's own.** `setup` stands a Worker and a database up in their Cloudflare
    /// account, and nothing of theirs is hosted anywhere else. What is put there is sealed with a key this
    /// device holds, so the account it runs in cannot read it either.
    ///
    /// **There is one read code, not one per phone.** Pairing a second phone is the same press as the
    /// first, and `revoke` takes every phone off at once — there is nothing to name and so nothing to
    /// single out.
    Viewer {
        #[command(subcommand)]
        sub: ViewerCmd,
    },

    /// Automations: a library of prompts, and the pictures built out of them — steps, the ways out of
    /// each one, what runs after each way out is taken, and what is handed along.
    ///
    /// These are the building commands. Nothing here refuses an unfinished automation — a step with no
    /// way onward, no entry named, a required setting nobody answered. What refuses them is the launch
    /// check, where a person is about to be let down by them.
    ///
    /// **The parts are named by id**, the one each `add` prints. They carry no ref of their own: a step
    /// is named by the automation it sits in, not by a number anybody types back.
    ///
    /// **An edge and a wire name a way out by name, not by key**, because an action's declarations can
    /// be rewritten underneath a step that points at it. Written `<step>:<way out>` — `4:` is the
    /// unnamed way out, `4:*` the error one.
    Automation {
        #[command(subcommand)]
        sub: AutomationCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum WorktreeCmd {
    /// Cut this task a worktree, and hand back the way into it.
    ///
    /// **The return value on stdout is one `cd` line**, so the caller enters the checkout with
    /// `eval "$(amenbo worktree start <id>)"` — `iex (amenbo worktree start <id>)` in PowerShell, the
    /// same line either way. Everything a person reads goes to stderr beside it, and `--json` answers
    /// with the path and the branch instead. This is the one command whose stdout is a value to run
    /// rather than an account of what happened, and it is written that way so no second step is needed
    /// to get where the work is (`AMB-D-881`).
    ///
    /// The backlog is not touched: reserving the task is its own act, and this one is only git.
    Start {
        /// the task
        id: String,
        /// the branch to cut from (default: the branch the repository is standing on)
        #[arg(long, value_name = "BRANCH")]
        base: Option<String>,
    },

    /// Take the worktree and its branch away again, once its commits have landed.
    ///
    /// It refuses while there is anything left to lose — work nobody committed, or a branch carrying
    /// changes the base does not have. Whether the changes landed is measured by the patch each commit
    /// carries rather than by lineage, so a squash or a rebase merge reads as merged (`AMB-D-699`).
    /// `--force` overrides both, which is the only way to discard work on purpose.
    ///
    /// The task itself is untouched: close it with `task done`, or hand it back with `task status
    /// <id> todo`.
    Finish {
        /// the task
        id: String,
        /// the branch the worktree is measured against (default: the branch the repository is standing on)
        #[arg(long, value_name = "BRANCH")]
        base: Option<String>,
        /// tear it down regardless — discarding uncommitted work and unmerged commits
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ViewerCmd {
    /// Stand the server up in a Cloudflare account, and leave behind what a send needs.
    ///
    /// **The API token is asked for here and written down nowhere.** It is taken on stdin rather than as an
    /// argument — a token on the command line is visible in the process list and lands in shell history —
    /// and what is left in the store afterwards is the address, the write token and the encryption key,
    /// none of which can create anything in that account.
    ///
    /// Pressing it again on a device already set up keeps the keys it finds: a new key would open nothing
    /// already up there.
    Setup {
        /// which account to build in, where the token reaches more than one
        #[arg(long)]
        account: Option<String>,
        /// what to call the server, for a second one in the same account — or to say that one already
        /// standing under the usual name is this store's
        #[arg(long)]
        name: Option<String>,
    },

    /// Draw a new read code, for the phone's camera.
    ///
    /// **It carries the encryption key**, which is what makes the screen and the camera the one path with
    /// no network on it. Where it is drawn is somewhere that key has been, and a terminal's scrollback is
    /// one of those.
    ///
    /// It replaces whatever code the server was holding, so the phone that had the one before stops
    /// reading.
    Qr {
        /// draw it even where stdout is not a terminal
        #[arg(long)]
        terminal: bool,
    },

    /// Where the Viewer app is got, as a code each phone's camera can read — and as the addresses in
    /// words, for whoever cannot point a camera at one.
    App {
        /// draw the codes even where stdout is not a terminal
        #[arg(long)]
        terminal: bool,
    },

    /// Ask the server whether a phone may read, and since when.
    ///
    /// **It asks the server rather than answering from here.** A list on this side could only say what
    /// this machine believes it issued, and a server stood up anew underneath it makes every row of that
    /// list name a phone that reads nothing.
    Phones,

    /// Take the read code away, so whatever was holding it stops reading.
    ///
    /// **This takes every phone off at once** — there is one code, so there is nothing to single out.
    /// Pairing again is one `qr`.
    Revoke,

    /// Carry what has moved to the server: copy the backlog's changes into the queue, then empty as much
    /// of that queue as the server will take.
    Send,

    /// Compare the server with this machine, and place the difference.
    ///
    /// **It is two presses.** Comparing is cheap and placing is not — a backlog that has drifted whole is
    /// tens of thousands of writes — so this counts and says the number, and the next run inside ten
    /// minutes places it. `--send` is that second press for whoever typed the first.
    Repair {
        /// place the difference rather than only counting it
        #[arg(long)]
        send: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum NotifyCmd {
    /// Start reporting from this project, through whatever it has selected.
    On,

    /// Stop reporting from this project. The selection and the events stay where they are, so a fortnight
    /// away costs one command and finds the settings standing on the way back.
    Off,

    /// Send this project's notifications through one more target.
    Use {
        /// the target on the shelf (`notify target-list` says which)
        target: i64,
    },

    /// Stop sending this project's notifications through one target. The target stays on the shelf.
    Unuse {
        /// the target on the shelf
        target: i64,
    },

    /// Where a mail target's message is addressed, as several addresses on one line separated by commas.
    /// Empty falls back to the account the relay authenticates as.
    To {
        /// the addresses, comma-separated (empty clears them)
        addresses: String,
    },

    /// Start or stop reporting one event (`task.done`, `task.due`, …). `notify` with no sub-command lists
    /// the ones this project may report.
    Event {
        /// the event's name, as the catalog spells it
        name: String,
        /// report it (default), or `--off` to stop
        #[arg(long)]
        off: bool,
    },

    /// Every connection on this device's shelf, in the order they were raised.
    TargetList,

    /// Raise a connection under a name. The connection itself is written afterwards
    /// (`notify target-set`), which is what gives its credential a row to hang off.
    ///
    /// The first one ever raised carries the default mark, there being nothing else a new project could
    /// point at.
    TargetAdd {
        /// what carries it
        #[arg(long, value_parser = ["slack", "mail"])]
        kind: String,
        /// the name a project's settings offers it under
        name: String,
    },

    /// Write one target's connection. Only what is named is written; everything else stays.
    ///
    /// **The credential is `--secret`**, and `-` reads it from stdin — a webhook URL or a password on the
    /// command line is visible in the process list and lands in shell history. An empty value clears it.
    TargetSet {
        /// the target on the shelf
        target: i64,
        /// the name it is offered under
        #[arg(long)]
        name: Option<String>,
        /// the relay a mail target hands the message to
        #[arg(long)]
        smtp_host: Option<String>,
        /// the port that relay listens on (587 on nearly every provider)
        #[arg(long)]
        smtp_port: Option<i64>,
        /// the account to authenticate as; empty for a relay that asks for none
        #[arg(long)]
        smtp_user: Option<String>,
        /// the address to send from; empty falls back to the account
        #[arg(long)]
        mail_from: Option<String>,
        /// the credential — a Slack webhook URL, a mail password. `-` reads it from stdin
        #[arg(long)]
        secret: Option<String>,
    },

    /// Move the default mark — where a **newly created** project starts out pointing. The projects already
    /// standing keep the selection they made.
    TargetDefault {
        /// the target on the shelf
        target: i64,
    },

    /// Remove a target, and with it every project's selection of it and the credential it held.
    ///
    /// It asks first, and says how many projects lose it — the global `--yes` answers ahead of the ask.
    TargetRm {
        /// the target on the shelf
        target: i64,
    },

    /// Ask whether the connection is usable, without sending anything.
    ///
    /// How much that means is the kind's: a mail relay is connected to and the account offered to it, a
    /// Slack webhook has only the shape of its URL read — it has no door but posting, and a webhook
    /// revoked yesterday still has the shape.
    TargetCheck {
        /// the target on the shelf
        target: i64,
    },

    /// Send one message through it, which is the only thing that answers whether it still works.
    TargetTest {
        /// the target on the shelf
        target: i64,
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
pub enum SkinCmd {
    /// What is held, and which of them is on. A file in the directory that will not read is listed
    /// as unreadable rather than left out — it is one of your own files, sitting right there.
    List,

    /// Take a skin file in. It is read, checked and measured first: what the check refuses is
    /// refused here, and what it only warns about is said and taken anyway. A pairing that falls
    /// short of AA is said too and does not stop it — an author has to be able to try their own
    /// work in progress.
    Add {
        /// The zip to take in: `skin.yaml` at its root, beside the materials it names.
        path: std::path::PathBuf,
        /// Replace the skin already kept under that name. Without it, a name that is taken is
        /// refused with both versions named.
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Put one of the held skins on, or `none` to go back to the colours this build ships with.
    Use {
        /// The skin's name, or `none`.
        name: String,
    },

    /// Take a skin off the device. The one that is on goes off with it, rather than leaving the
    /// config naming a file that is gone.
    Rm {
        /// The skin's name.
        name: String,
    },

    /// Run the reading and the check over a file without taking it in — the author's face of what
    /// an import does, so the reason a file will be turned away is learned before it is handed on.
    Validate {
        /// The zip to read: `skin.yaml` at its root, beside the materials it names.
        path: std::path::PathBuf,
    },

    /// Write out a skin to start from: this build's own values, or the ones the skin that is on
    /// sets, as a zip holding `skin.yaml`. Both sides and every name a skin may set, so the first
    /// edit is to a value rather than to an empty file — and the zip is where the materials go.
    Template {
        /// Where to write the zip. It must not already be a file.
        path: std::path::PathBuf,
    },

    /// Write a skin this device holds back out, as the file it arrived in. Byte for byte: the
    /// materials the author packed with it ride along, which rebuilding the document from its
    /// values could not do.
    WriteOut {
        /// The skin's name, as `skin list` shows it.
        name: String,
        /// Where to write the file. It must not already be a file.
        path: std::path::PathBuf,
    },
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
    /// it — never a hand. It is not a daemon: it starts, judges what the calendar day owes, carries out
    /// whatever the outbox still holds, and exits, so nothing is running between two ticks.
    ///
    /// Being woken is not being due: the timer carries no meaning, so an hour that is owed nothing is the
    /// ordinary case, and what is owed is counted in calendar days rather than in wake-ups (`AMB-D-708`).
    /// A round with nothing owed is still not a wasted one — it carries the outbox out, which is where a
    /// walk a killed run left standing gets picked up.
    ///
    /// It resolves no folder and takes no facet: a scheduler runs it from wherever it happens to stand,
    /// and what it works is this device's one store. On a device holding no store there is nothing to do,
    /// and it says nothing and exits 0 rather than raising one on a schedule.
    #[command(hide = true)]
    Run,
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
    /// `decision comment-list <decision> --json`.
    DecisionComment {
        /// decision comment ref(s) to erase, AMB-DC-n
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// Redact a settled decision's body: overwrite it in place with the given text and scrub the prior
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

/// The verbs of the surface layer (`talk`). Both are owed: they say the two things nothing outside the
/// pane can find out — which pane this is, and whether a person is needed (`AMB-D-859`).
#[derive(Subcommand, Debug)]
pub enum TalkCmd {
    /// Name this pane. The name sticks to the frame rather than to what runs in it
    Name {
        /// the name to show on the pane
        text: String,
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
        /// mark this axis as one whose values can be closed — retired without taking what is filed under them (one axis holds one role, so this and --time-axis are refused together)
        #[arg(long, conflicts_with = "time_axis")]
        closable: bool,
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
    /// List a project's dimensions (display order) with their open values
    List {
        /// project (name or ID; defaults to the bound project)
        #[arg(long)]
        project: Option<String>,
        /// list the closed values too (they are left out by default)
        #[arg(long)]
        closed: bool,
    },
    /// Show a dimension (name, notes, cardinality/ordered/role/card/applies-to, open values)
    Show {
        /// dimension ref (AMB-DIM-n), slug or name
        id: String,
        /// show the closed values too (they are left out by default)
        #[arg(long)]
        closed: bool,
    },
    /// Update a dimension's name, notes, how many values one record may hold, value ordering, its role (time-axis / closable), whether it goes on the task card, whether it must be answered, which side it classifies, and/or its slug (only the given fields change)
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
        /// whether this axis's values can be closed (`--closable true|false`); an axis that gives the role up keeps whatever was closed under it closed
        #[arg(long)]
        closable: Option<bool>,
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
    /// Close a dimension value: nothing new is filed under it, and everything already filed keeps it.
    /// Only an axis marked --closable can close one, and a required axis keeps one open value to offer
    ValueClose {
        /// dimension ref (AMB-DIM-n), slug or name
        dimension: String,
        /// value ref (AMB-DIMV-n), slug or name (within the dimension)
        value: String,
    },
    /// Reopen a closed dimension value, so it takes new records again. Free on any axis, whatever role
    /// it carries now
    ValueReopen {
        /// dimension ref (AMB-DIM-n), slug or name
        dimension: String,
        /// value ref (AMB-DIMV-n), slug or name (within the dimension)
        value: String,
    },
    /// Delete a dimension value permanently; its task assignments go with it unless --reassign-to
    /// moves them. On a required axis: assignments demand --reassign-to, and the last open value is
    /// refused (a closed one is not an answer, so it does not count) — lower the requirement first
    /// (alias: value-delete)
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
        /// named by its path or just its folder name (`--at amenbo-worker`). Only what you name
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
        /// named by its path or just its folder name (`--at amenbo-worker`)
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
    /// Record a commit SHA on a task (idempotent; full-length lower-case hex only)
    ///
    /// The anchor from history back to a task, since a public commit carries no store-local
    /// reference. Amenbo stores each SHA opaquely: it never reads git, verifies the commit, or
    /// knows which forge it lives on.
    CommitAdd {
        /// target task ref (AMB-T-n)
        task: String,
        /// the full commit SHA — 40 hex for SHA-1, 64 for SHA-256 (short forms, branches, tags and
        /// revisions are refused)
        sha: String,
    },
    /// List a task's recorded commit SHAs, oldest first
    CommitList {
        /// target task ref (AMB-T-n)
        task: String,
    },
    /// Forget a commit SHA on a task — permanently
    CommitRm {
        /// target task ref (AMB-T-n)
        task: String,
        /// the commit SHA to forget (any case — normalised the way it was stored)
        sha: String,
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
    /// Open a new decision, as a draft — only what the human settled with you belongs on one. The project defaults to the bound project
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
        /// axis left empty is what `decision finish-writing` refuses later, so filling it here saves the
        /// round trip (`AMB-D-925`).
        #[arg(long = "dim", value_name = "AXIS=VALUE")]
        dim: Vec<String>,
    },
    /// List decisions (filter by status:/draft: (yes|no — whether the writing is still unfinished)/superseded:/project:/number: (alias ref:, e.g. `D-<n>`/`#<n>`)/task: (the decisions a task rests on, e.g. `task:#<n>`)/`dim:<axis>=<value>` and its `time_axis:<value>` sugar (the same axes tasks are classified on: different axes AND, the same axis ORs, `=none` is unclassified)/decided_before:/decided_after: (the day a decision was settled, YYYY-MM-DD or today/-30d; both ends inclusive), sort by decided/created/number/title/status). Words are not a key here — `amenbo search <word> --kind decision` finds where they are written
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
    /// Edit a decision's title/body in place — one still being written and a settled one alike (supersede to overturn a settled one, not edit)
    Edit {
        id: String,
        #[arg(long)]
        title: Option<String>,
        /// replacement body (Markdown). Pass `-` to read it from stdin
        /// (a shell eats code spans out of a quoted argument).
        #[arg(long)]
        body: Option<String>,
    },
    /// Finish writing a decision — the second stage of recording one, which settles it and releases the tasks that rest on it. A human or an AI may say it; a decision already written is a no-op
    FinishWriting {
        id: String,
        /// reason for settling it — recorded as a decision comment, not a dedicated field. Pass `-` to read it from stdin
        #[arg(long)]
        reason: Option<String>,
    },
    /// Turn down a decision still being written (draft → rejected) — a settled one is refused, and a record written in error goes by `decision delete`
    Reject {
        id: String,
        /// reason for rejecting — recorded as a decision comment, not a dedicated field. Pass `-` to read it from stdin
        #[arg(long)]
        reason: Option<String>,
    },
    /// Put a settled decision back in hand — raise the draft flag again and clear decided_at/decided_by (the status stays decided). Editing does not need it. Non-destructive and audited
    Reopen {
        id: String,
    },
    /// Delete (retire) a decision — settled ones included; the row goes, the bytes stay in the file (confirms unless --yes)
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
        /// classify the new decision as `<axis>=<value>` — the same resolution as `dimension set` (id, or
        /// an exact name, case-insensitive). Repeatable for different axes; an axis is single-select, so
        /// naming one twice is refused, and so is an axis that does not classify decisions. A required
        /// axis left empty is what `decision finish-writing` refuses later, so filling it here saves the
        /// round trip (`AMB-D-925`).
        #[arg(long = "dim", value_name = "AXIS=VALUE")]
        dim: Vec<String>,
    },
    /// Add a comment to a decision's timeline
    CommentAdd {
        /// target decision ref (AMB-D-n)
        decision: String,
        /// comment body, as Markdown (GUI renders GFM tables/task lists + ```mermaid; no raw HTML).
        /// Lead with the conclusion, prefer bullets/tables, one point per line (a single newline is a break).
        /// Pass `-` to read it from stdin (a shell eats code spans out of a quoted argument).
        #[arg(long)]
        text: String,
    },
    /// List a decision's comments (oldest first; pairs with --offset for paging)
    CommentList {
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
    /// The id comes from `decision comment-list`
    CommentRm {
        /// target decision comment ref, AMB-DC-n (from `decision comment-list`)
        comment: String,
    },
    /// Rewrite a comment's body in place — the id, its place on the timeline, and its
    /// attachments all stay. The id comes from `decision comment-list`
    CommentEdit {
        /// target decision comment ref, AMB-DC-n (from `decision comment-list`)
        comment: String,
        /// the new body, as Markdown — it replaces the old one outright. Pass `-` to read it from stdin
        /// (a shell eats code spans out of a quoted argument).
        #[arg(long)]
        text: String,
    },
    /// Attach a file (blob, ingested) or external link (--url) to a single decision comment — kept
    /// separate from the parent decision's own attachments (manage via `attach`)
    CommentAttach {
        /// target decision comment ref, AMB-DC-n (from `decision comment-list`)
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
        /// list the attachments on this decision comment (id from `decision comment-list`)
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

#[derive(Subcommand, Debug)]
pub enum AutomationCmd {
    /// Create an automation
    Add {
        /// project (name or ID; defaults to the bound project)
        #[arg(long)]
        project: Option<String>,
        /// what this automation is called
        #[arg(long)]
        name: String,
        /// what it is for, in Markdown (`-` reads it from stdin)
        #[arg(long, default_value = "")]
        notes: String,
        /// the text prepended to every step's launch. Left out, the standing operating rules go in;
        /// pass an empty string for none (`-` reads it from stdin)
        #[arg(long)]
        preamble: Option<String>,
    },
    /// The automations of one project — what each is called, how many steps it is built out of, and
    /// whether it is archived
    List {
        /// project (name or ID; defaults to the bound project)
        #[arg(long)]
        project: Option<String>,
    },
    /// One automation in full: each step with the prompt, ways out, inputs and settings it runs
    /// under, what happens after each way out, what is handed along, and the documents it shares
    Show {
        /// automation id
        id: i64,
    },
    /// Change an automation's name, notes, preamble, or whether it is archived (only the given fields change)
    Update {
        /// automation id
        id: i64,
        #[arg(long)]
        name: Option<String>,
        /// what it is for, in Markdown (`-` reads it from stdin)
        #[arg(long)]
        notes: Option<String>,
        /// the text prepended to every step's launch (`-` reads it from stdin)
        #[arg(long)]
        preamble: Option<String>,
        /// whether it is archived (`--archived true|false`)
        #[arg(long)]
        archived: Option<bool>,
    },
    /// Delete an automation with every step, way out, edge and wire built into it — confirms unless -y
    Rm {
        /// automation id
        id: i64,
    },
    /// Name the step a run starts at, or clear it
    EntrySet {
        /// automation id
        id: i64,
        /// the step to start at
        #[arg(long, value_name = "ID", conflicts_with = "clear")]
        step: Option<i64>,
        /// leave the automation with no entry
        #[arg(long)]
        clear: bool,
    },
    /// Add a prompt to the library
    ActionAdd {
        /// project (name or ID; defaults to the bound project)
        #[arg(long, conflicts_with = "global")]
        project: Option<String>,
        /// put it in the device's library, which every project on this machine reaches (a human's to write)
        #[arg(long)]
        global: bool,
        /// what this action is called
        #[arg(long)]
        name: String,
        /// the prompt itself (`-` reads it from stdin)
        #[arg(long)]
        prompt: String,
    },
    /// The library this project reaches — the device's actions, then the project's own
    ActionList {
        /// project (name or ID; defaults to the bound project)
        #[arg(long, conflicts_with = "global")]
        project: Option<String>,
        /// the device's library alone, which every project on this machine reaches
        #[arg(long)]
        global: bool,
    },
    /// One library action: its prompt, what it declares, and how many automations run it
    ActionShow {
        /// action id
        id: i64,
    },
    /// Rename a library action, or rewrite its prompt (only the given fields change)
    ActionUpdate {
        /// action id
        id: i64,
        #[arg(long)]
        name: Option<String>,
        /// the prompt itself (`-` reads it from stdin)
        #[arg(long)]
        prompt: Option<String>,
    },
    /// Delete a library action with everything it declared — refused while a step runs it; confirms unless -y
    ActionRm {
        /// action id
        id: i64,
    },
    /// Add a step to an automation. It either runs a library action (--action) or carries a prompt of
    /// its own (--prompt), and which it is decides where its ways out, settings and inputs are read from
    StepAdd {
        /// automation id
        automation: i64,
        /// what this step is called
        #[arg(long)]
        name: String,
        /// run this library action
        #[arg(long, value_name = "ID", conflicts_with = "prompt")]
        action: Option<i64>,
        /// the prompt written for this step alone (`-` reads it from stdin)
        #[arg(long)]
        prompt: Option<String>,
        /// who is asked to carry it out (e.g. claude)
        #[arg(long)]
        agent: String,
        /// which model; left out, the agent's own default stands
        #[arg(long)]
        model: Option<String>,
        /// let this step wait for a person
        #[arg(long)]
        interactive: bool,
        /// the name of the setting or the input the working folder is taken from — a name, not a path
        #[arg(long, value_name = "NAME")]
        work_dir: Option<String>,
        /// also land this step's report as a comment on the task
        #[arg(long)]
        report_to_task: bool,
        /// do not hand this step the run's story so far (it is handed on unless this is passed)
        #[arg(long)]
        no_history: bool,
    },
    /// Change a step (only the given fields change). Switching where its prompt comes from takes its
    /// declarations with it
    StepUpdate {
        /// step id
        id: i64,
        #[arg(long)]
        name: Option<String>,
        /// run this library action instead
        #[arg(long, value_name = "ID", conflicts_with = "prompt")]
        action: Option<i64>,
        /// carry this prompt instead (`-` reads it from stdin)
        #[arg(long)]
        prompt: Option<String>,
        /// who is asked to carry it out
        #[arg(long)]
        agent: Option<String>,
        /// which model
        #[arg(long, conflicts_with = "clear_model")]
        model: Option<String>,
        /// leave the agent's own default model
        #[arg(long)]
        clear_model: bool,
        /// whether this step may wait for a person (`--interactive true|false`)
        #[arg(long)]
        interactive: Option<bool>,
        /// the name of the setting or the input the working folder is taken from
        #[arg(long, value_name = "NAME", conflicts_with = "clear_work_dir")]
        work_dir: Option<String>,
        /// take the working folder from nothing
        #[arg(long)]
        clear_work_dir: bool,
        /// whether this step's report also lands as a comment on the task (`--report-to-task true|false`)
        #[arg(long)]
        report_to_task: Option<bool>,
        /// whether this step is handed the run's story so far (`--history true|false`)
        #[arg(long)]
        history: Option<bool>,
    },
    /// Delete a step with its declarations and every edge and wire naming it — confirms unless -y
    StepRm {
        /// step id
        id: i64,
    },
    /// Declare a way out of a step or a library action. Both are born carrying the unnamed way out and
    /// the error one (`*`), so this is for the second and every one after it
    ExitAdd {
        /// the step that declares it (one carrying its own prompt)
        #[arg(long, value_name = "ID", conflicts_with = "action")]
        step: Option<i64>,
        /// the library action that declares it
        #[arg(long, value_name = "ID")]
        action: Option<i64>,
        /// what this way out is called
        #[arg(long)]
        name: String,
    },
    /// Rename a way out. Whatever named the old name is parted from it — the edges and wires that named
    /// it stop resolving, visibly, rather than being rewritten underneath
    ExitRename {
        /// way out id
        id: i64,
        /// the new name
        #[arg(long, conflicts_with = "clear")]
        name: Option<String>,
        /// make it the unnamed way out
        #[arg(long)]
        clear: bool,
    },
    /// Delete a way out with the outputs declared on it — confirms unless -y
    ExitRm {
        /// way out id
        id: i64,
    },
    /// Declare a port. An input belongs to the step or the action that reads it (--step / --action); an
    /// output belongs to the way out that produced it (--exit)
    PortAdd {
        /// the step that takes it in (one carrying its own prompt)
        #[arg(long, value_name = "ID", conflicts_with_all = ["action", "exit"])]
        step: Option<i64>,
        /// the library action that takes it in
        #[arg(long, value_name = "ID", conflicts_with = "exit")]
        action: Option<i64>,
        /// the way out that hands it on
        #[arg(long, value_name = "ID")]
        exit: Option<i64>,
        /// what this port is called
        #[arg(long)]
        name: String,
        /// what it carries: value | file | task_take | task_make
        #[arg(long)]
        kind: String,
        /// refuse to run the step without it
        #[arg(long)]
        required: bool,
    },
    /// Change a port's name, what it carries, or whether it is required (only the given fields change).
    /// Renaming parts every wire that named the old name
    PortUpdate {
        /// port id
        id: i64,
        #[arg(long)]
        name: Option<String>,
        /// what it carries: value | file | task_take | task_make
        #[arg(long)]
        kind: Option<String>,
        /// whether the step is refused without it (`--required true|false`)
        #[arg(long)]
        required: Option<bool>,
    },
    /// Delete a port — confirms unless -y. The wires that named it are left where they are, parted
    PortRm {
        /// port id
        id: i64,
    },
    /// Declare a setting on a step or a library action
    CfgAdd {
        /// the step that declares it (one carrying its own prompt)
        #[arg(long, value_name = "ID", conflicts_with = "action")]
        step: Option<i64>,
        /// the library action that declares it
        #[arg(long, value_name = "ID")]
        action: Option<i64>,
        /// what this setting is called
        #[arg(long)]
        name: String,
        /// what kind of answer it takes: taskfilter | folder | choice | number | text
        #[arg(long)]
        kind: String,
        /// refuse to run the step until it is answered
        #[arg(long)]
        required: bool,
        /// the choices, as a JSON array — for `--kind choice` and nothing else
        #[arg(long, value_name = "JSON")]
        options: Option<String>,
    },
    /// Change a setting's declaration (only the given fields change). The answer is `cfg set`
    CfgUpdate {
        /// setting id
        id: i64,
        #[arg(long)]
        name: Option<String>,
        /// what kind of answer it takes: taskfilter | folder | choice | number | text
        #[arg(long)]
        kind: Option<String>,
        /// whether the step is refused until it is answered (`--required true|false`)
        #[arg(long)]
        required: Option<bool>,
        /// the choices, as a JSON array — for `--kind choice` and nothing else
        #[arg(long, value_name = "JSON", conflicts_with = "clear_options")]
        options: Option<String>,
        /// leave it with no choice list
        #[arg(long)]
        clear_options: bool,
    },
    /// Answer a setting on one step. The answer is written in the shape its kind takes, never as one
    /// filter string: for `taskfilter`, the same option twice is any-of and two different options are
    /// both
    CfgSet {
        /// step id
        step: i64,
        /// the setting's name, as it was declared
        #[arg(long)]
        name: String,
        /// leave it unanswered
        #[arg(long)]
        clear: bool,
        /// `folder`: the working folder
        #[arg(long, value_name = "PATH")]
        folder: Option<String>,
        /// `choice`: one of the declared choices
        #[arg(long, value_name = "VALUE")]
        choice: Option<String>,
        /// `number`: the number
        #[arg(long, value_name = "N")]
        number: Option<i64>,
        /// `text`: the text
        #[arg(long, value_name = "STR")]
        text: Option<String>,
        /// `taskfilter`: the status a task is in (repeat for any-of)
        #[arg(long, value_name = "VALUE")]
        status: Vec<String>,
        /// `taskfilter`: the priority (repeat for any-of)
        #[arg(long, value_name = "VALUE")]
        priority: Vec<String>,
        /// `taskfilter`: who it is assigned to — none | me | me-ai (repeat for any-of)
        #[arg(long, value_name = "VALUE")]
        assignee: Vec<String>,
        /// `taskfilter`: a classification, `<axis>=<value>` (repeat the same axis for any-of)
        #[arg(long, value_name = "AXIS=VALUE")]
        dim: Vec<String>,
        /// `taskfilter`: whether the premises it declared are met — yes | no
        #[arg(long, value_name = "VALUE")]
        ready: Vec<String>,
        /// `taskfilter`: whether it is closed — true | false
        #[arg(long, value_name = "VALUE")]
        done: Vec<String>,
        /// `taskfilter`: when it is due — today | overdue | week | none | YYYY-MM-DD
        #[arg(long, value_name = "VALUE")]
        due: Vec<String>,
    },
    /// Delete a setting — confirms unless -y
    CfgRm {
        /// setting id
        id: i64,
    },
    /// Say what happens after one step leaves through one way out. The way out is the whole condition:
    /// the edge carries none of its own
    EdgeAdd {
        /// where it leaves from, `<step>:<way out>` — `4:` is the unnamed way out, `4:*` the error one
        #[arg(long, value_name = "STEP:EXIT")]
        from: String,
        /// go on to this step
        #[arg(long, value_name = "ID", conflicts_with_all = ["done", "halt"])]
        to: Option<i64>,
        /// close the run
        #[arg(long, conflicts_with = "halt")]
        done: bool,
        /// stop the run and call a person
        #[arg(long)]
        halt: bool,
        /// how often this edge may be taken for one task (left out: 10, the standing limit)
        #[arg(long, value_name = "N", conflicts_with = "no_max")]
        max_times: Option<i64>,
        /// let it be taken as often as the run reaches it
        #[arg(long)]
        no_max: bool,
    },
    /// Change where an edge goes, or how often it may be taken (only the given fields change)
    EdgeUpdate {
        /// edge id
        id: i64,
        /// go on to this step
        #[arg(long, value_name = "ID", conflicts_with_all = ["done", "halt"])]
        to: Option<i64>,
        /// close the run
        #[arg(long, conflicts_with = "halt")]
        done: bool,
        /// stop the run and call a person
        #[arg(long)]
        halt: bool,
        /// how often this edge may be taken for one task
        #[arg(long, value_name = "N", conflicts_with = "no_max")]
        max_times: Option<i64>,
        /// take the limit off
        #[arg(long)]
        no_max: bool,
    },
    /// Delete an edge — confirms unless -y
    EdgeRm {
        /// edge id
        id: i64,
    },
    /// Join what one way out hands on to what a later step takes in. Both ends are named, never keyed
    WireAdd {
        /// where it comes from, `<step>:<way out>` — `4:` is the unnamed way out, `4:*` the error one
        #[arg(long, value_name = "STEP:EXIT")]
        from: String,
        /// the output's name on that way out
        #[arg(long, value_name = "NAME")]
        from_port: String,
        /// the step that takes it in
        #[arg(long, value_name = "ID")]
        to: i64,
        /// the input's name on that step
        #[arg(long, value_name = "NAME")]
        to_port: String,
    },
    /// Delete a wire — confirms unless -y
    WireRm {
        /// wire id
        id: i64,
    },
    /// Write a document the steps of one automation share. Long is fine here — which steps are handed
    /// it is `note link`'s to say
    NoteAdd {
        /// automation id
        automation: i64,
        /// what this document is called
        #[arg(long)]
        name: String,
        /// the document itself, in Markdown (`-` reads it from stdin)
        #[arg(long)]
        body: String,
    },
    /// Rename a shared document, or rewrite it (only the given fields change)
    NoteUpdate {
        /// document id
        id: i64,
        #[arg(long)]
        name: Option<String>,
        /// the document itself, in Markdown (`-` reads it from stdin)
        #[arg(long)]
        body: Option<String>,
    },
    /// Delete a shared document with the links that hand it to steps — confirms unless -y
    NoteRm {
        /// document id
        id: i64,
    },
    /// Hand a shared document to a step
    NoteLink {
        /// step id
        step: i64,
        /// document id
        note: i64,
    },
    /// Stop handing a shared document to a step
    NoteUnlink {
        /// step id
        step: i64,
        /// document id
        note: i64,
    },
    /// The runs that worked one task, or the runs one automation has behind it (newest first)
    RunList {
        /// the task a run worked (AMB-T-n)
        #[arg(long, value_name = "ID", conflicts_with = "automation")]
        task: Option<String>,
        /// the automation the runs came from
        #[arg(long, value_name = "ID")]
        automation: Option<i64>,
        /// max count (newest first)
        #[arg(long)]
        limit: Option<usize>,
    },
    /// One run in full: every step it ran, the way out each took, how long it stood, what it handed on,
    /// and the whole of what it reported
    RunShow {
        /// run id
        id: i64,
    },

    /// Start an automation: check it, copy its steps into a run, and start it. It takes nothing
    /// else — the tasks a step works on and the folder it runs in are the automation's own answers,
    /// given while it was built
    Start {
        /// automation id
        id: i64,
    },
    /// Ask a run to pause. A step under way finishes first and the run pauses at the end of it,
    /// keeping the task it is working
    Pause {
        /// run id
        run: i64,
    },
    /// Pick a paused run up again, from the way out the step before it left through
    Resume {
        /// run id
        run: i64,
    },
    /// Stop a run: hand the task it was working back, and leave a comment on the task saying how
    /// far it got
    Stop {
        /// run id
        run: i64,
    },

    /// **Take the task this stretch of the run is about** — reserve it and declare it in one act.
    /// Typed by the agent carrying a step out; which step that is comes from the environment the
    /// window opened its terminal with
    StepTake {
        /// the task to take (AMB-T-n)
        task: String,
    },
    /// **Put down one thing this step produced**, under the name its port was declared with. Written
    /// `<name>=<value>`, or `<name> --file <path>` for a file
    StepOut {
        /// `<name>=<value>`, or just `<name>` beside --file
        value: String,
        /// a file to hand on, instead of a value
        #[arg(long, value_name = "PATH")]
        file: Option<String>,
    },
    /// **This step is finished**: say which way out it took and what it did. The run reads the way out
    /// to decide what happens next
    StepDone {
        /// what this step did, for the record and for the steps after it (`-` reads stdin)
        #[arg(long, value_name = "TEXT")]
        report: String,
        /// the way out taken, as the step declared it. Left out is the unnamed one
        #[arg(long, value_name = "NAME")]
        exit: Option<String>,
        /// one more thing produced, `<name>=<value>` — repeat for several
        #[arg(long = "out", value_name = "NAME=VALUE")]
        outs: Vec<String>,
    },
}










