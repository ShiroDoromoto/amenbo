//! The lint hooks: whether amenbo may write them into a project's `.git/hooks`, and what is in those slots
//! right now. Installing means writing into the user's git plumbing, which amenbo does not do unasked, so
//! it asks once per project and remembers the answer in the store's `hook_consent` table (read and written
//! through [`crate::store::Store`], implemented in [`crate::overview`]). What is consented to is the lint
//! as a *feature*, not a file: one answer covers every [`HookSlot`], because the slots are a consequence of
//! git's lifecycle — `pre-commit` cannot see a commit message that does not exist yet, and `commit-msg` is
//! the only slot git hands it to — and nobody has an opinion about which of git's doors one lint is let in
//! through. Consent and the hooks on disk are two independent facts: the record ([`HookConsent`]) says what
//! was answered and never what `.git/hooks` holds, which is [`probe`]'s answer and is read from the
//! filesystem every time — reading the record as a mirror of the disk breaks the moment a hook is deleted
//! or added by hand, so the two meet in exactly one place, [`reconcile`]. amenbo owns only what carries its
//! marker: a hook it wrote opens with [`HOOK_MARKER`], the same shape as the managed block in `CLAUDE.md` /
//! `AGENTS.md` ([`crate::agents`]), while anything else in a slot belongs to someone else — husky,
//! lefthook, a hand-written script — and is never written to and never removed, which [`install`] and
//! [`uninstall`] honour slot by slot rather than all-or-nothing: a stranger holding one slot leaves the
//! others writable and earns a [`guidance_line`], because refusing the lint everywhere over husky's
//! `pre-commit` would fail the commonest repository there is.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Msg, Result};

/// The answer on record. There is no `Unanswered` variant: never having answered is the *absence* of a
/// record (`Option::None`), which is what makes "asked and refused" different from "never asked" —
/// the first must never be asked again, the second must.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HookConsent {
    /// Install it. Consent is given to the *project*, so a later clone of it on this device is covered
    /// too and is not asked again (`.git/hooks` is per-clone; the answer is not).
    Yes,
    /// Don't install it, and don't ask again. It answers the question for good, but it forbids
    /// nothing: an explicit install asked for later is still honoured.
    No,
}

impl HookConsent {
    /// The stored spelling (the `answer` column's `CHECK`ed values).
    pub fn as_str(self) -> &'static str {
        match self {
            HookConsent::Yes => "yes",
            HookConsent::No => "no",
        }
    }

    /// Read the stored spelling back. `None` for anything else — a row the `CHECK` should have refused.
    pub fn parse(s: &str) -> Option<HookConsent> {
        match s {
            "yes" => Some(HookConsent::Yes),
            "no" => Some(HookConsent::No),
            _ => None,
        }
    }
}

/// Format version of the marker on the hook **this binary writes**. Paired with [`HOOK_MARKER`], and the
/// same device as [`crate::agents::MANAGED_BLOCK_VERSION`]: bump it when the hook's body changes, and a
/// hook written by an older build is recognised as ours and rewritten rather than mistaken for a
/// stranger's.
pub const HOOK_MARKER_VERSION: u32 = 2;

/// Version-independent prefix of the marker. Detection matches on this rather than on [`HOOK_MARKER`]
/// whole, so a hook written by any version of amenbo is still known to be ours.
const MARKER_PREFIX: &str = "# amenbo:hook (managed";
/// Close of the marker. The version token (` vN`) sits between the prefix and this close.
const MARKER_SUFFIX: &str = ")";

/// The marker at the version this binary writes. It is a shell comment, so it lives in the hook itself:
/// the file *is* the record of who wrote it, and there is no side ledger to fall out of step with it.
pub const HOOK_MARKER: &str = "# amenbo:hook (managed v2)";

/// A git hook slot the lint needs. There are two because of git's lifecycle, not because the lint is two
/// things: `pre-commit` can see the staged diff but runs before a commit message exists, and `commit-msg`
/// is the only slot git hands the message to. One lint, two doors it has to be let in through.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookSlot {
    /// Lints what the commit is about to record, from the staged diff.
    PreCommit,
    /// Lints the commit message, whose path git passes as the hook's first argument.
    CommitMsg,
}

/// Every slot the lint needs, in the order git runs them.
pub const HOOK_SLOTS: [HookSlot; 2] = [HookSlot::PreCommit, HookSlot::CommitMsg];

impl HookSlot {
    /// The slot's name, which is also the hook file's name.
    pub fn name(self) -> &'static str {
        match self {
            HookSlot::PreCommit => "pre-commit",
            HookSlot::CommitMsg => "commit-msg",
        }
    }

    /// How this slot calls the lint. `commit-msg` gets the message file's path from git as `$1` and hands
    /// it straight over; `pre-commit` is given nothing and lints the staged diff, which is the bare
    /// command's default.
    fn lint_call(self, cmd: &str) -> String {
        match self {
            HookSlot::PreCommit => format!("{cmd} lint"),
            HookSlot::CommitMsg => format!("{cmd} lint \"$1\""),
        }
    }

    /// What this slot's hook reads, said the way the refusal and the prompts say it.
    fn subject(self) -> &'static str {
        match self {
            HookSlot::PreCommit => "the staged diff",
            HookSlot::CommitMsg => "the commit message",
        }
    }
}

/// The marker's version in `text`, or `None` when there is no marker — i.e. when the text is not a hook
/// amenbo wrote. Mirrors [`crate::agents::managed_block_version`]; an unparsable token reads as version 1,
/// staying conservative (it is still ours).
fn marker_version(text: &str) -> Option<u32> {
    let start = text.find(MARKER_PREFIX)?;
    let after_prefix = start + MARKER_PREFIX.len();
    let close = after_prefix + text[after_prefix..].find(MARKER_SUFFIX)?;
    let token = text[after_prefix..close].trim();
    Some(token.strip_prefix('v').and_then(|n| n.parse::<u32>().ok()).unwrap_or(1))
}

/// What is actually in the hook slot — read from the filesystem, never from the record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum HookState {
    /// Nothing there. The lint runs on nobody's commit.
    Unwired,
    /// A hook carrying amenbo's marker, at `version`. Ours to rewrite and ours to remove.
    Managed { version: u32 },
    /// A hook that is not ours — husky, lefthook, a hand-written script. amenbo never writes it and never
    /// removes it; the most it does is say [which line to add](guidance_line).
    Foreign,
}

impl HookState {
    /// Is the lint actually wired *by us*? A stranger's hook occupies the slot but says nothing about
    /// whether it runs the lint, so it does not count as wired.
    pub fn is_managed(self) -> bool {
        matches!(self, HookState::Managed { .. })
    }
}

/// The body of `slot`'s hook, where `cmd` is the launch command (`amenbo` in production, `amenbo-dev` in
/// development) so the hook calls the channel that installed it. It bails out when `cmd` is not on `PATH`
/// rather than failing: a hook that blocks every commit because amenbo was uninstalled would be a trap,
/// and this is a convenience, not a gate the repository depends on.
pub fn hook_body(slot: HookSlot, cmd: &str) -> String {
    let name = slot.name();
    let subject = slot.subject();
    let call = slot.lint_call(cmd);
    format!(
        "#!/bin/sh\n\
         {HOOK_MARKER}\n\
         #\n\
         # Written by `{cmd} hooks install`. amenbo owns this file only while the marker above is\n\
         # here: delete the marker, or the file, and amenbo leaves it alone for good.\n\
         # Remove it with `{cmd} hooks uninstall`.\n\
         #\n\
         # Refuses a commit whose {subject} carries an amenbo ref (AMB-T-… / AMB-D-…): an id\n\
         # resolves only in the store that issued it, so it says nothing to a reader outside.\n\
         # The {name} slot is where git offers {subject}.\n\
         # Bypass one commit with `git commit --no-verify`.\n\
         \n\
         # No amenbo on PATH (uninstalled?) — this hook is a convenience, not a gate.\n\
         command -v {cmd} >/dev/null 2>&1 || exit 0\n\
         exec {call}\n"
    )
}

/// The one line to add to `slot`'s hook when amenbo must not touch it. What [`install`] hands back instead
/// of writing when a slot is a stranger's, and what the unwired report shows in the same case.
pub fn guidance_line(slot: HookSlot, cmd: &str) -> String {
    format!("{} || exit 1", slot.lint_call(cmd))
}

/// The hook directory of the repository at `dir`, or `None` when `dir` is not in a git repository. Asked
/// of git rather than assembled from `.git/hooks`, because that guess is wrong in the two cases that
/// matter: `core.hooksPath` moves the directory wholesale, and in a linked worktree `.git` is a file.
/// `--git-path hooks` honours both and answers relative to `dir`.
pub fn hooks_dir(dir: &Path) -> Option<PathBuf> {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args(["rev-parse", "--git-path", "hooks"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    Some(dir.join(path))
}

/// Would git show a hook written here? That is the whole of the difference between a hook that stays on
/// this machine and one that lands in everybody's checkout. `.git/hooks` is outside the working tree, so
/// nothing there is ever git's to show; a `core.hooksPath` aimed into the tree (`.githooks`, `.husky`) is
/// the repository's own versioned decision, and a file amenbo drops in it surfaces in `git status` and
/// rides out on the next `git add -A`. Which is why [`install`] pairs a shared write with
/// [`exclude_locally`] rather than refusing it: the lint is not optional, so the write happens either way
/// and it is amenbo's job — not the user's — to keep it off everybody else's machine.
fn hooks_are_shared(dir: &Path, hooks: &Path) -> bool {
    let Some(root) = worktree_root(dir) else {
        return false;
    };
    let Ok(hooks) = std::fs::canonicalize(hooks) else {
        return false;
    };
    // Inside the git directory first, and by path rather than by reasoning: `.git` sits *within* the
    // working tree, so the ordinary `.git/hooks` passes a prefix test against the root and would be called
    // shared. Git shows nothing in there whatever the prefix says. `--git-common-dir` rather than
    // `--git-dir` because a linked worktree keeps its hooks in the main repository's.
    if let Some(gitdir) = common_git_dir(dir) {
        if hooks.starts_with(gitdir) {
            return false;
        }
    }
    // What is left is in the tree proper. A hook git already ignores is as local as one it cannot see.
    hooks.starts_with(root) && !is_ignored(dir, &hooks)
}

/// The git directory shared by every worktree of this repository, resolved through any symlink. This is
/// where hooks live unless `core.hooksPath` says otherwise, and nothing under it is ever git's to show.
fn common_git_dir(dir: &Path) -> Option<PathBuf> {
    let path = git_line(dir, &["rev-parse", "--git-common-dir"])?;
    std::fs::canonicalize(dir.join(path)).ok()
}

/// The working tree's root, resolved through any symlink on the way. Both halves must be canonical or the
/// comparison silently lies: git answers with the real path (on macOS a temp dir under `/var` comes back
/// under `/private/var`), while the caller's `dir` is whatever it was handed. A prefix test between the two
/// spellings says "outside the tree" for a path plainly inside it — and this guard failing open is a hook
/// committed to everybody's checkout, with nothing on screen to say so.
fn worktree_root(dir: &Path) -> Option<PathBuf> {
    let root = git_line(dir, &["rev-parse", "--show-toplevel"])?;
    std::fs::canonicalize(root).ok()
}

/// The first line of a git command's output, or `None` when git has nothing to say (not a repository, or
/// the call failed). Trimmed, because git terminates its answers with a newline.
fn git_line(dir: &Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git").current_dir(dir).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if line.is_empty() { None } else { Some(line) }
}

/// Whether git already ignores `path`. `check-ignore` answers by exit code: 0 is ignored, 1 is not, and
/// anything else is an error we read as "not ignored" — guessing "ignored" there would hide a shared write.
fn is_ignored(dir: &Path, path: &Path) -> bool {
    std::process::Command::new("git")
        .current_dir(dir)
        .args(["check-ignore", "-q"])
        .arg(path)
        .status()
        .map(|s| s.code() == Some(0))
        .unwrap_or(false)
}

/// Keep a hook written into a shared directory out of everybody else's checkout, by naming it in
/// `.git/info/exclude` — git's per-clone ignore file, which is inside `.git` and so is never committed and
/// never travels. The hook then runs for the person who said yes and shows up for nobody: not in
/// `git status`, not in `git add -A`.
///
/// It is deliberately additive and idempotent: the file is the user's to keep other lines in, so this
/// appends one and only when it is not already there. A failure to write is *not* an error — the hook is
/// already on disk and working, and the alternative (unwinding a successful install because a comfort
/// measure failed) would trade a working lint for a tidy tree.
fn exclude_locally(dir: &Path, hook: &Path) {
    // Canonical on both sides, for the reason `worktree_root` gives: a prefix test between two spellings of
    // the same directory strips nothing, and the line that reaches the file would be an absolute path git
    // reads as a pattern that matches nothing.
    let Some(root) = worktree_root(dir) else {
        return;
    };
    let Ok(hook) = std::fs::canonicalize(hook) else {
        return;
    };
    let Ok(rel) = hook.strip_prefix(root) else {
        return;
    };
    // Always forward slashes: this file is read by git, not by the platform.
    let line = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
    let Some(exclude) = git_line(dir, &["rev-parse", "--git-path", "info/exclude"]) else {
        return;
    };
    let exclude = dir.join(exclude);
    let current = std::fs::read_to_string(&exclude).unwrap_or_default();
    if current.lines().any(|l| l.trim() == line) {
        return;
    }
    if let Some(parent) = exclude.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let sep = if current.is_empty() || current.ends_with('\n') { "" } else { "\n" };
    let _ = std::fs::write(&exclude, format!("{current}{sep}{line}\n"));
}

/// What every slot holds right now, read from the filesystem and never from the record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct HookStates {
    /// The `pre-commit` slot, which lints the staged diff.
    pub pre_commit: HookState,
    /// The `commit-msg` slot, which lints the commit message.
    pub commit_msg: HookState,
}

impl HookStates {
    /// What `slot` holds.
    pub fn get(self, slot: HookSlot) -> HookState {
        match slot {
            HookSlot::PreCommit => self.pre_commit,
            HookSlot::CommitMsg => self.commit_msg,
        }
    }

    /// Every slot with its state, in the order git runs them.
    pub fn iter(self) -> impl Iterator<Item = (HookSlot, HookState)> {
        HOOK_SLOTS.into_iter().map(move |slot| (slot, self.get(slot)))
    }

    /// Slots in `state`, which is how a caller renders one line per slot that needs the same thing said.
    pub fn slots_in(self, state: HookState) -> Vec<HookSlot> {
        self.iter().filter(|&(_, s)| s == state).map(|(slot, _)| slot).collect()
    }

    /// Does amenbo own every slot? The one shape in which the lint is fully wired.
    pub fn all_managed(self) -> bool {
        self.iter().all(|(_, s)| s.is_managed())
    }

    /// Does amenbo own any slot at all? Its own hook on disk is the disk's way of saying yes.
    pub fn any_managed(self) -> bool {
        self.iter().any(|(_, s)| s.is_managed())
    }

    /// Is any slot empty, i.e. is there anything for an install to write?
    fn any_unwired(self) -> bool {
        self.iter().any(|(_, s)| s == HookState::Unwired)
    }

    /// Is any slot a stranger's?
    fn any_foreign(self) -> bool {
        self.iter().any(|(_, s)| s == HookState::Foreign)
    }

    /// Does a hook here predate this build's set of slots? A marker older than [`HOOK_MARKER_VERSION`] was
    /// written before this build knew the slot it is missing, which is what tells an install that is merely
    /// out of date from a hook the user removed on purpose: the first is this build's to complete under the
    /// consent already on record, the second is never restored behind their back.
    fn predates_this_build(self) -> bool {
        self.iter().any(|(_, s)| matches!(s, HookState::Managed { version } if version < HOOK_MARKER_VERSION))
    }
}

/// What each of `dir`'s hook slots holds right now, or `None` when `dir` is not a git repository — there
/// are no hooks to have, and no question to ask. This is the only source of truth for the hooks' existence.
/// One `git` call locates the directory and every slot is read from it, so a caller that wants both the
/// question and the report pays for one spawn by asking once and passing the answer around. A hook whose
/// bytes will not read back as text — a binary, or one we may not open — is a stranger's: what amenbo
/// wrote, it can always read.
pub fn probe(dir: &Path) -> Option<HookStates> {
    let hooks = hooks_dir(dir)?;
    Some(HookStates {
        pre_commit: probe_slot(&hooks, HookSlot::PreCommit),
        commit_msg: probe_slot(&hooks, HookSlot::CommitMsg),
    })
}

/// What one slot of an already-located hook directory holds.
fn probe_slot(hooks: &Path, slot: HookSlot) -> HookState {
    let hook = hooks.join(slot.name());
    let Ok(text) = std::fs::read_to_string(&hook) else {
        return if hook.exists() { HookState::Foreign } else { HookState::Unwired };
    };
    match marker_version(&text) {
        Some(version) => HookState::Managed { version },
        None => HookState::Foreign,
    }
}

/// What [`install`] did, so the caller can say it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Installed {
    /// Written into an empty slot.
    Wrote,
    /// A hook of ours was already there and was replaced (an older marker version, or a hand-edited body).
    Rewrote,
}

/// What [`install`] did, slot by slot: what it wrote, and what it left to its owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallReport {
    /// The slots amenbo wrote, each with what writing it amounted to.
    pub installed: Vec<(HookSlot, Installed)>,
    /// The slots a stranger holds, left untouched. The caller offers [`guidance_line`] for each.
    pub refused: Vec<HookSlot>,
}

/// Write the lint hook into every slot of `dir` amenbo may own, replacing a hook of ours but never a
/// stranger's. It is deliberately partial: a [`HookState::Foreign`] slot is reported in
/// [`InstallReport::refused`] rather than failing the install, because the alternative — refusing every
/// slot because one is husky's — would leave the commonest setup of all unable to wire the lint anywhere.
/// Only an install that could write nothing at all is an error, which is the one case where a caller has
/// nothing to show but [`guidance_line`].
pub fn install(dir: &Path, cmd: &str) -> Result<InstallReport> {
    let states = probe(dir).ok_or_else(|| {
        Error::Invalid(Msg::new(
            "Not a git repository, so there are no hooks to install.",
            "git リポジトリではないため、インストールするフックがありません。",
        ))
    })?;
    if !states.any_managed() && !states.any_unwired() {
        return Err(Error::Conflict(Msg::new(
            format!(
                "The hooks here were not written by amenbo, and amenbo does not overwrite them. Add these lines yourself:\n{}",
                guidance_block(states.slots_in(HookState::Foreign), cmd, "    ")
            ),
            format!(
                "ここのフックは amenbo が書いたものではないため、上書きしません。次の行をご自身で足してください:\n{}",
                guidance_block(states.slots_in(HookState::Foreign), cmd, "    ")
            ),
        )));
    }
    let dir_path = hooks_dir(dir).expect("probe succeeded, so the hooks dir resolves");
    std::fs::create_dir_all(&dir_path).map_err(io_err)?;
    // Asked once for the directory, not once per slot: the answer is a property of where the hooks live.
    let shared = hooks_are_shared(dir, &dir_path);
    let mut report = InstallReport { installed: Vec::new(), refused: Vec::new() };
    for (slot, state) in states.iter() {
        if state == HookState::Foreign {
            report.refused.push(slot);
            continue;
        }
        let hook = dir_path.join(slot.name());
        std::fs::write(&hook, hook_body(slot, cmd)).map_err(|e| io_err_slot(slot, e))?;
        make_executable(&hook)?;
        if shared {
            exclude_locally(dir, &hook);
        }
        report.installed.push((slot, if state.is_managed() { Installed::Rewrote } else { Installed::Wrote }));
    }
    Ok(report)
}

/// The lines to add by hand, one per slot, each indented by `indent` — what a caller shows for the slots
/// amenbo will not write.
pub fn guidance_block(slots: Vec<HookSlot>, cmd: &str, indent: &str) -> String {
    slots
        .into_iter()
        .map(|slot| format!("{indent}{}: {}", slot.name(), guidance_line(slot, cmd)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Remove the lint hook from every slot of `dir` that carries our marker, and return those slots. The
/// mirror of [`install`], partial for partial and refusal for refusal: a stranger's hook is not ours to
/// delete, an empty slot is already the asked-for state, and only a call with nothing of ours to remove and
/// a stranger in the way is an error.
pub fn uninstall(dir: &Path, cmd: &str) -> Result<Vec<HookSlot>> {
    let states = probe(dir).ok_or_else(|| {
        Error::Invalid(Msg::new(
            "Not a git repository, so there are no hooks to remove.",
            "git リポジトリではないため、削除するフックがありません。",
        ))
    })?;
    if !states.any_managed() && states.any_foreign() {
        let lines = guidance_block(states.slots_in(HookState::Foreign), cmd, "    ");
        return Err(Error::Conflict(Msg::new(
            format!(
                "The hooks here were not written by amenbo, so amenbo will not remove them. Remove these lines yourself if you put them there:\n{lines}"
            ),
            format!(
                "ここのフックは amenbo が書いたものではないため、削除しません。ご自身で足した行であれば、ご自身で外してください:\n{lines}"
            ),
        )));
    }
    let dir_path = hooks_dir(dir).expect("probe succeeded, so the hooks dir resolves");
    let mut removed = Vec::new();
    for (slot, state) in states.iter() {
        if state.is_managed() {
            std::fs::remove_file(dir_path.join(slot.name())).map_err(|e| io_err_slot(slot, e))?;
            removed.push(slot);
        }
    }
    Ok(removed)
}

fn io_err_slot(slot: HookSlot, e: std::io::Error) -> Error {
    let name = slot.name();
    Error::Invalid(Msg::new(
        format!("Cannot write the {name} hook: {e}"),
        format!("{name} フックを書けません: {e}"),
    ))
}

fn io_err(e: std::io::Error) -> Error {
    Error::Invalid(Msg::new(
        format!("Cannot write the hooks: {e}"),
        format!("フックを書けません: {e}"),
    ))
}

/// git runs a hook only if it is executable. No-op off unix, where git for Windows reads the shebang and
/// runs it through its own shell regardless of any file mode.
fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).map_err(io_err)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// What a caller knows when it comes to the question: the two independent facts, and the surface it came
/// in through.
pub struct HookContext {
    /// What is in each hook slot ([`probe`]), or `None` when the folder is not a git repository — one
    /// probe answers every slot, so they are not carried apart.
    pub states: Option<HookStates>,
    /// The answer on record, or `None` if this project has never been asked.
    pub consent: Option<HookConsent>,
    /// Can this surface actually get an answer? An interactive terminal and the GUI's dialog can; a
    /// machine caller cannot (see [`reconcile`]).
    pub can_ask: bool,
}

/// What the two facts, taken together, call for: the drift table as a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookAction {
    /// Leave it be. The overwhelmingly common answer.
    Nothing,
    /// Ask whether to install, and [`install`] on a yes. Record whichever answer comes back. One question
    /// covers every slot, because what is being asked is whether the lint may place its files here, and
    /// the slots are amenbo's business rather than anything the user holds an opinion about. The caller
    /// says what will happen to which slot, reading it off the states it already has: there is no separate
    /// action for a stranger's slot, since an install can write one slot while pointing at
    /// [`guidance_line`] for another.
    Ask,
    /// [`install`] without asking: consent is already on record, and a slot this build added is missing
    /// from an install written before it existed. Completing what was consented to is not the same as
    /// restoring a hook the user removed, which is why the two are told apart by
    /// [`HookStates::predates_this_build`] rather than lumped together.
    Install,
    /// Our hook is on disk although the record does not say yes: someone installed it by hand, so the
    /// filesystem wins and the record moves to [`HookConsent::Yes`], silently and with nothing to ask.
    SyncConsentToYes,
}

/// Read the record and the disk against each other and say what to do — the drift table, which the tests
/// below walk row by row. Consent is asked once for the lint as a feature rather than once per file, so
/// the slots are folded into one answer here in a fixed order of precedence. A hook of ours on disk under
/// a record that does not say `yes` was installed by hand, so the record follows the disk with nothing to
/// ask, and that outranks everything. A recorded `no` is silent from there on. An install written before
/// this build knew a slot is completed under the consent already given, without a question, which is what
/// keeps a slot added by an upgrade from re-asking every project that already said yes. What is left is a
/// question: never having answered, or having said yes with nothing of ours on disk — the latter because a
/// hook deleted by hand may well have been deleted on purpose, and consent was consent to an install, not
/// a promise to keep the file alive, so it is never silently restored. Either way there must be a slot a
/// yes would write: a repository whose every slot is a stranger's is not asked about, because the question
/// exists to get permission for a write and there is no write to permit. A machine caller is never asked,
/// whatever the disk holds: `--json`, an AI harness and a script cannot answer, and a prompt there hangs a
/// caller on a terminal nobody is watching, so `can_ask` is false, the question does not happen, and
/// nothing is recorded — the unanswered state carries intact to the next surface that can ask, and what a
/// machine gets instead is the unwired hook in its output.
pub fn reconcile(ctx: &HookContext) -> HookAction {
    let Some(states) = ctx.states else {
        return HookAction::Nothing;
    };
    if states.any_managed() && ctx.consent != Some(HookConsent::Yes) {
        return HookAction::SyncConsentToYes;
    }
    if ctx.consent == Some(HookConsent::No) {
        return HookAction::Nothing;
    }
    if ctx.consent == Some(HookConsent::Yes) && states.any_unwired() && states.predates_this_build() {
        return HookAction::Install;
    }
    // A question amenbo cannot act on is not a question. Where every slot is a stranger's there is nothing
    // a yes would write, so asking would put amenbo's problem in front of the user and hand them back a
    // button that cannot fire — the lint still wants wiring there, which is what `setup_notice` is for, on
    // a surface where the line to add can actually be pasted.
    let question_is_live = match ctx.consent {
        None | Some(HookConsent::Yes) => states.any_unwired(),
        Some(HookConsent::No) => false,
    };
    if question_is_live && ctx.can_ask {
        HookAction::Ask
    } else {
        HookAction::Nothing
    }
}

/// What setup is still missing — the standing report that the lint is not actually running, as opposed to
/// [`reconcile`]'s one-time question. The two lists are separate because the fix differs and both can be
/// live at once: a repository where husky holds `pre-commit` and nothing holds `commit-msg` needs a line
/// added by hand *and* a command run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupNotice {
    /// Slots with no hook at all: the lint is wired to nothing there, and `hooks install` writes them.
    pub unwired: Vec<HookSlot>,
    /// Slots a stranger holds, which amenbo will not write — [`guidance_line`] is the way in, by hand.
    pub foreign: Vec<HookSlot>,
}

/// Is the lint actually running, and if not, where it is not — the slots of a git repository that are not
/// ours, under a record that is not `no`, or `None` when there is nothing to say. It is deliberately
/// quieter than that sounds: hooks of ours in every slot and a recorded refusal are both silent, so it
/// speaks only while setup is genuinely unfinished and cannot become noise to tune out. A recorded `yes`
/// does **not** silence it, because the record is not a mirror of the disk: consent with no hook there is
/// exactly the state worth reporting.
pub fn setup_notice(states: Option<HookStates>, consent: Option<HookConsent>) -> Option<SetupNotice> {
    if consent == Some(HookConsent::No) {
        return None;
    }
    let states = states?;
    let notice =
        SetupNotice { unwired: states.slots_in(HookState::Unwired), foreign: states.slots_in(HookState::Foreign) };
    (!notice.unwired.is_empty() || !notice.foreign.is_empty()).then_some(notice)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OURS: HookState = HookState::Managed { version: HOOK_MARKER_VERSION };
    const OLD: HookState = HookState::Managed { version: HOOK_MARKER_VERSION - 1 };

    /// Every state the record can be in.
    const CONSENTS: [Option<HookConsent>; 3] = [None, Some(HookConsent::Yes), Some(HookConsent::No)];

    fn states(pre_commit: HookState, commit_msg: HookState) -> Option<HookStates> {
        Some(HookStates { pre_commit, commit_msg })
    }

    /// Every combination the disk can be in, for the tests that must not miss a row.
    fn every_disk() -> Vec<Option<HookStates>> {
        let each = [HookState::Unwired, OURS, HookState::Foreign];
        let mut all = vec![None];
        for pre in each {
            for msg in each {
                all.push(states(pre, msg));
            }
        }
        all
    }

    /// The context in which the question is live: a git repo, nothing wired, never answered, on a
    /// surface that can ask. Each test below negates exactly one of those.
    fn askable() -> HookContext {
        HookContext { states: states(HookState::Unwired, HookState::Unwired), consent: None, can_ask: true }
    }

    #[test]
    fn unanswered_git_repo_without_the_hooks_is_asked() {
        assert_eq!(reconcile(&askable()), HookAction::Ask);
    }

    #[test]
    fn a_machine_caller_is_never_asked() {
        // The whole question is skipped rather than answered: no prompt to hang on, and nothing
        // recorded, so the next interactive run still asks.
        let ctx = HookContext { can_ask: false, ..askable() };
        assert_eq!(reconcile(&ctx), HookAction::Nothing);
    }

    #[test]
    fn an_answered_project_with_its_hooks_in_place_is_not_asked_again() {
        let ctx = HookContext { states: states(OURS, OURS), consent: Some(HookConsent::Yes), ..askable() };
        assert_eq!(reconcile(&ctx), HookAction::Nothing);
    }

    /// A recorded `no` settles every row where amenbo has nothing of its own on disk — which is what
    /// recording a refusal buys — whatever the slots hold.
    #[test]
    fn a_refusal_is_never_asked_again() {
        for pre in [HookState::Unwired, HookState::Foreign] {
            for msg in [HookState::Unwired, HookState::Foreign] {
                let ctx = HookContext { states: states(pre, msg), consent: Some(HookConsent::No), ..askable() };
                assert_eq!(reconcile(&ctx), HookAction::Nothing, "{pre:?} / {msg:?} was refused");
            }
        }
    }

    #[test]
    fn outside_a_git_repo_there_are_no_hooks_to_install() {
        let ctx = HookContext { states: None, ..askable() };
        assert_eq!(reconcile(&ctx), HookAction::Nothing);
    }

    /// The first refusal: consent was consent to an install, not a promise to keep the files alive. Hooks
    /// deleted by hand may well have been deleted on purpose, so they are not restored behind the user's
    /// back — nothing of ours is left on disk, so the state changed and the question is live again.
    #[test]
    fn hand_deleted_hooks_are_asked_about_again_not_silently_restored() {
        let ctx = HookContext {
            states: states(HookState::Unwired, HookState::Unwired),
            consent: Some(HookConsent::Yes),
            ..askable()
        };
        assert_eq!(reconcile(&ctx), HookAction::Ask);
    }

    /// The same refusal at slot granularity: our hook is in one slot at the version this build writes, so
    /// this install already knew the empty slot and the user emptied it. Not ours to put back.
    #[test]
    fn a_slot_emptied_under_a_current_install_is_asked_about_not_refilled() {
        let ctx = HookContext { states: states(OURS, HookState::Unwired), consent: Some(HookConsent::Yes), ..askable() };
        assert_eq!(reconcile(&ctx), HookAction::Ask);
    }

    /// The row this build adds: an install written before `commit-msg` existed as a slot. The consent on
    /// record covers the lint as a feature, and the empty slot is one the user never saw, let alone
    /// removed — so it is filled without asking a project that already said yes.
    #[test]
    fn a_slot_this_build_added_is_installed_under_the_consent_already_given() {
        let ctx = HookContext { states: states(OLD, HookState::Unwired), consent: Some(HookConsent::Yes), ..askable() };
        assert_eq!(reconcile(&ctx), HookAction::Install);
        assert_eq!(
            reconcile(&HookContext { can_ask: false, ..ctx }),
            HookAction::Install,
            "it asks no one, so a machine caller completes the wiring too",
        );
    }

    /// The mirror row: our hook is on disk though the record does not say yes, so someone installed it by
    /// hand. The filesystem wins and the record follows it — no question, because the answer is on disk.
    /// It holds for a bare `no` and for never having been asked alike, and one slot of ours is enough.
    #[test]
    fn a_hand_installed_hook_pulls_the_record_to_yes() {
        for consent in [Some(HookConsent::No), None] {
            for disk in [states(OURS, OURS), states(OURS, HookState::Unwired), states(HookState::Foreign, OURS)] {
                let ctx = HookContext { states: disk, consent, ..askable() };
                assert_eq!(reconcile(&ctx), HookAction::SyncConsentToYes, "record: {consent:?}, disk: {disk:?}");
                assert_eq!(
                    reconcile(&HookContext { can_ask: false, ..ctx }),
                    HookAction::SyncConsentToYes,
                    "it asks no one, so it holds on a machine caller too",
                );
            }
        }
    }

    /// A stranger in every slot is not a question. The question exists to get permission for a write, and
    /// there is no write to permit: a yes could touch nothing, so asking would spend the user's attention
    /// on amenbo's own difficulty and leave them holding a choice that changes nothing. The lint still
    /// wants wiring, which `setup_notice` says on a surface where the line can be pasted.
    #[test]
    fn a_strangers_hook_is_not_asked_about_since_no_answer_could_write_it() {
        let ctx = HookContext { states: states(HookState::Foreign, HookState::Foreign), ..askable() };
        assert_eq!(reconcile(&ctx), HookAction::Nothing);
    }

    /// One writable slot is enough to make the question live, and it is asked without mentioning the other:
    /// the stranger's slot is amenbo's problem to route to `setup_notice`, not a fork in the user's road.
    #[test]
    fn one_writable_slot_beside_a_stranger_is_still_asked_about() {
        let ctx = HookContext { states: states(HookState::Unwired, HookState::Foreign), ..askable() };
        assert_eq!(reconcile(&ctx), HookAction::Ask);
    }

    /// Having said yes with a stranger in every slot leaves nothing to do: the guidance was given when the
    /// question was answered, and there is no slot amenbo may write.
    #[test]
    fn consent_with_every_slot_taken_asks_nothing_further() {
        let ctx = HookContext {
            states: states(HookState::Foreign, HookState::Foreign),
            consent: Some(HookConsent::Yes),
            ..askable()
        };
        assert_eq!(reconcile(&ctx), HookAction::Nothing);
    }

    /// The report speaks exactly while the lint is not running and has not been refused, naming each slot
    /// under its own reason — the two lists are live at once when husky holds one slot and nothing holds
    /// the other, because the fixes differ.
    #[test]
    fn an_unwired_repository_is_reported_as_unfinished() {
        for consent in [None, Some(HookConsent::Yes)] {
            assert_eq!(
                setup_notice(states(HookState::Unwired, HookState::Unwired), consent),
                Some(SetupNotice { unwired: vec![HookSlot::PreCommit, HookSlot::CommitMsg], foreign: vec![] }),
            );
            assert_eq!(
                setup_notice(states(HookState::Foreign, HookState::Unwired), consent),
                Some(SetupNotice { unwired: vec![HookSlot::CommitMsg], foreign: vec![HookSlot::PreCommit] }),
                "a stranger in one slot and an empty other is two different fixes at once",
            );
            assert_eq!(
                setup_notice(states(OURS, HookState::Unwired), consent),
                Some(SetupNotice { unwired: vec![HookSlot::CommitMsg], foreign: vec![] }),
                "one slot wired is not the lint running",
            );
        }
    }

    /// The two silences that keep it from becoming noise: our hooks are in every slot (nothing is
    /// missing), or the project said no (it was asked and answered, and a refusal is not an unfinished
    /// setup).
    #[test]
    fn wired_hooks_or_a_refusal_say_nothing() {
        for consent in CONSENTS {
            assert_eq!(
                setup_notice(states(OURS, OURS), consent),
                None,
                "the hooks are ours and on disk, so nothing is missing (record: {consent:?})",
            );
        }
        for disk in every_disk() {
            assert_eq!(setup_notice(disk, Some(HookConsent::No)), None, "refused, so {disk:?} is silent");
        }
    }

    /// A recorded `yes` does not silence it: the record is not a mirror of the disk, and consent with the
    /// hooks gone is precisely the state worth reporting.
    #[test]
    fn consent_alone_does_not_silence_the_report() {
        assert_eq!(
            setup_notice(states(HookState::Unwired, HookState::Unwired), Some(HookConsent::Yes)),
            Some(SetupNotice { unwired: vec![HookSlot::PreCommit, HookSlot::CommitMsg], foreign: vec![] }),
        );
    }

    #[test]
    fn outside_a_git_repository_there_is_no_setup_to_finish() {
        for consent in CONSENTS {
            assert_eq!(setup_notice(None, consent), None);
        }
    }

    #[test]
    fn consent_round_trips_through_its_stored_spelling() {
        for answer in [HookConsent::Yes, HookConsent::No] {
            assert_eq!(HookConsent::parse(answer.as_str()), Some(answer));
        }
        assert_eq!(HookConsent::parse("maybe"), None);
    }

    /// Catches the [`HOOK_MARKER`] literal and [`HOOK_MARKER_VERSION`] drifting apart — the same guard
    /// [`crate::agents`] keeps over its own marker.
    #[test]
    fn the_marker_literal_carries_the_current_version() {
        assert_eq!(marker_version(HOOK_MARKER), Some(HOOK_MARKER_VERSION));
    }

    /// Each slot calls the lint the way git speaks to it: `commit-msg` is handed the message file's path
    /// and passes it on, `pre-commit` is handed nothing and lints the staged diff. A hook that ignored `$1`
    /// in the `commit-msg` slot would lint the diff twice and never read a commit message.
    #[test]
    fn each_slot_hands_the_lint_what_git_gives_it() {
        assert!(hook_body(HookSlot::CommitMsg, "amenbo").contains("exec amenbo lint \"$1\"\n"));
        assert!(hook_body(HookSlot::PreCommit, "amenbo").contains("exec amenbo lint\n"));
        assert_eq!(guidance_line(HookSlot::CommitMsg, "amenbo"), "amenbo lint \"$1\" || exit 1");
        assert_eq!(guidance_line(HookSlot::PreCommit, "amenbo"), "amenbo lint || exit 1");
    }

    /// The point of the marker: what amenbo wrote is recognised as amenbo's, and nothing else is — a
    /// hand-written hook that calls the lint names us without being ours, and is still the user's file.
    #[test]
    fn a_written_hook_is_recognised_as_ours_and_a_strangers_is_not() {
        for slot in HOOK_SLOTS {
            assert_eq!(marker_version(&hook_body(slot, "amenbo")), Some(HOOK_MARKER_VERSION));
            assert_eq!(marker_version(&hook_body(slot, "amenbo-dev")), Some(HOOK_MARKER_VERSION));
        }
        for foreign in [
            "#!/bin/sh\nnpx husky run\n",
            "#!/usr/bin/env bash\nlefthook run pre-commit\n",
            "#!/bin/sh\namenbo lint || exit 1\n",
            "",
        ] {
            assert_eq!(marker_version(foreign), None, "{foreign:?} is not ours");
        }
    }

    /// An older marker still reads as ours, which is what lets a body change ship as a rewrite rather than
    /// as amenbo failing to recognise its own hook. An unparsable version stays conservative for the same
    /// reason: ours, so we may rewrite it, and never mistaken for a stranger's.
    #[test]
    fn an_older_marker_is_still_ours() {
        assert_eq!(marker_version("#!/bin/sh\n# amenbo:hook (managed v1)\n"), Some(1));
        assert_eq!(marker_version("#!/bin/sh\n# amenbo:hook (managed vX)\n"), Some(1));
    }

    /// A throwaway git repository, so the tests below exercise the real `git` the installer asks. Returns
    /// the working tree; the caller wipes it.
    fn git_repo(key: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("amenbo-hooks-{key}-{}", crate::tmpdir::suffix()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = std::process::Command::new("git").current_dir(&dir).args(["init", "-q"]).output().unwrap();
        assert!(out.status.success(), "git init failed: {}", String::from_utf8_lossy(&out.stderr));
        dir
    }

    fn hook_path(dir: &Path, slot: HookSlot) -> PathBuf {
        hooks_dir(dir).unwrap().join(slot.name())
    }

    #[test]
    fn install_then_uninstall_leaves_no_trace() {
        let dir = git_repo("roundtrip");
        assert_eq!(probe(&dir), states(HookState::Unwired, HookState::Unwired));

        let done = install(&dir, "amenbo").unwrap();
        assert_eq!(done.refused, vec![], "an empty repository has nobody to refuse");
        assert_eq!(done.installed, vec![(HookSlot::PreCommit, Installed::Wrote), (HookSlot::CommitMsg, Installed::Wrote)]);
        assert_eq!(probe(&dir), states(OURS, OURS), "every slot the lint needs is wired in one go");
        #[cfg(unix)]
        for slot in HOOK_SLOTS {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(hook_path(&dir, slot)).unwrap().permissions().mode();
            assert!(mode & 0o111 != 0, "git runs a hook only if it is executable, {slot:?} got {mode:o}");
        }

        assert_eq!(
            install(&dir, "amenbo").unwrap().installed,
            vec![(HookSlot::PreCommit, Installed::Rewrote), (HookSlot::CommitMsg, Installed::Rewrote)],
            "installing over our own hooks is a rewrite, not a refusal — that is what carries a body change",
        );

        assert_eq!(uninstall(&dir, "amenbo").unwrap(), vec![HookSlot::PreCommit, HookSlot::CommitMsg]);
        assert_eq!(probe(&dir), states(HookState::Unwired, HookState::Unwired));
        assert!(
            uninstall(&dir, "amenbo").unwrap().is_empty(),
            "symmetric to the end: removing what is already gone is the asked-for state, not an error",
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The whole point of the marker: a hook amenbo did not write is never written and never removed. With
    /// husky in `pre-commit` and nothing in `commit-msg` — the commonest repository there is — the refusal
    /// is per slot, so the lint still gets wired where amenbo may write.
    #[test]
    fn a_strangers_hook_is_stepped_around_not_written_over() {
        let dir = git_repo("foreign");
        let theirs = "#!/bin/sh\nnpx husky run pre-commit\n";
        std::fs::write(hook_path(&dir, HookSlot::PreCommit), theirs).unwrap();
        assert_eq!(probe(&dir), states(HookState::Foreign, HookState::Unwired));

        let done = install(&dir, "amenbo").unwrap();
        assert_eq!(done.refused, vec![HookSlot::PreCommit], "husky's slot is left to husky");
        assert_eq!(done.installed, vec![(HookSlot::CommitMsg, Installed::Wrote)], "the free slot is still wired");
        assert_eq!(std::fs::read_to_string(hook_path(&dir, HookSlot::PreCommit)).unwrap(), theirs, "untouched");

        assert_eq!(uninstall(&dir, "amenbo").unwrap(), vec![HookSlot::CommitMsg], "and only ours comes back out");
        assert_eq!(std::fs::read_to_string(hook_path(&dir, HookSlot::PreCommit)).unwrap(), theirs, "still untouched");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Nothing to write and nothing of ours to remove is the one case that fails: a caller has nothing to
    /// report but the lines to add by hand, and saying "installed" would be a lie.
    #[test]
    fn an_install_that_could_write_nothing_is_an_error() {
        let dir = git_repo("all-foreign");
        for slot in HOOK_SLOTS {
            std::fs::write(hook_path(&dir, slot), "#!/bin/sh\nnpx husky run\n").unwrap();
        }
        assert!(install(&dir, "amenbo").is_err(), "every slot is a stranger's, so the install wrote nothing");
        assert!(uninstall(&dir, "amenbo").is_err(), "and there is nothing of ours to take back");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// `core.hooksPath` moves the hook directory wholesale (this repository sets it), and a hook written to
    /// a guessed `.git/hooks` in such a repo would never run. Ask git, and the install lands where git
    /// looks.
    #[test]
    fn the_hooks_follow_core_hookspath() {
        let dir = git_repo("hookspath");
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(["config", "core.hooksPath", ".githooks"])
            .output()
            .unwrap();

        install(&dir, "amenbo").unwrap();
        for slot in HOOK_SLOTS {
            assert!(dir.join(".githooks").join(slot.name()).exists(), "{slot:?} must land where git looks");
            assert!(!dir.join(".git/hooks").join(slot.name()).exists(), "and not in the guessed directory");
        }
        assert_eq!(probe(&dir), states(OURS, OURS));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The whole of the shared-directory guard, stated the way it is felt: after an install, `git status` is
    /// as empty as it was before. A `core.hooksPath` aimed into the tree is the repository's own versioned
    /// choice — everybody's `.githooks`, not this machine's — so a hook dropped there would ride out on the
    /// next `git add -A` and land amenbo's lint on people who never installed amenbo. Writing it and naming
    /// it in `.git/info/exclude` keeps both halves: the lint runs for whoever said yes, and git never sees
    /// it. Nothing is asked and nothing is reported, because neither is the user's problem.
    #[test]
    fn a_hook_written_into_a_shared_directory_stays_off_everybody_elses_machine() {
        let dir = git_repo("shared-hooks");
        std::process::Command::new("git")
            .current_dir(&dir)
            .args(["config", "core.hooksPath", ".githooks"])
            .output()
            .unwrap();

        install(&dir, "amenbo").unwrap();

        let status = std::process::Command::new("git")
            .current_dir(&dir)
            .args(["status", "--porcelain"])
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&status.stdout).trim(),
            "",
            "the install must leave the tree exactly as it found it"
        );
        for slot in HOOK_SLOTS {
            assert!(is_ignored(&dir, &dir.join(".githooks").join(slot.name())), "{slot:?} must be excluded");
        }
        // The hook is still there and still ours — excluded, not skipped.
        assert_eq!(probe(&dir), states(OURS, OURS));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The mirror: `.git/hooks` is inside the git directory, so git was never going to show the hook and
    /// there is nothing to exclude. Writing to `info/exclude` anyway would put a line in the user's file for
    /// no reason — the guard is for shared directories, and this is not one. Worth its own test because
    /// `.git` sits *within* the working tree: the ordinary case passes a naive prefix test against the root
    /// and reaches this code looking exactly like a shared one.
    #[test]
    fn an_ordinary_repository_gets_no_exclude_line() {
        let dir = git_repo("plain-hooks");

        install(&dir, "amenbo").unwrap();

        let exclude = dir.join(".git/info/exclude");
        let text = std::fs::read_to_string(&exclude).unwrap_or_default();
        assert!(!text.contains("hooks"), "nothing was hidden, because nothing was ever visible: {text:?}");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The hooks are asked for from wherever the caller stands, which for a person is rarely the repo root.
    #[test]
    fn a_subdirectory_resolves_the_same_hooks() {
        let dir = git_repo("subdir");
        let sub = dir.join("crates").join("deep");
        std::fs::create_dir_all(&sub).unwrap();

        install(&sub, "amenbo").unwrap();
        assert_eq!(probe(&dir), states(OURS, OURS));
        assert_eq!(
            hook_path(&sub, HookSlot::CommitMsg).canonicalize().unwrap(),
            hook_path(&dir, HookSlot::CommitMsg).canonicalize().unwrap(),
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn outside_a_repository_there_is_nothing_to_probe_or_install() {
        let dir = std::env::temp_dir().join(format!("amenbo-hooks-bare-{}", crate::tmpdir::suffix()));
        std::fs::create_dir_all(&dir).unwrap();

        assert_eq!(probe(&dir), None);
        assert!(install(&dir, "amenbo").is_err());
        assert!(uninstall(&dir, "amenbo").is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
