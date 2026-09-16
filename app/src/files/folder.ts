// The seam to what a folder holds (`crate::folder`, `crate::folder_watch`, `crate::folder_write`).
//
// The names inside one folder, what one file has to show and what git says about it are asked for;
// **that the folder moved is watched** — the host installs a watch over it and says so as it
// happens, rather than this side guessing when to go and look (`AMB-T-3604`). The word carries no
// rows: it is the moment to ask again, and the asking is the two calls above it (`AMB-D-785`). The
// root all of them are rooted at is a folder the project is bound to, and the host checks that
// against the store rather than taking this side's word for it, so nothing here has to be careful
// about which path it passes.
//
// **Writing goes the same way and answers differently.** Reading swallows what it cannot do — the
// row is about to stop being drawn anyway — but a name refused is a person's next keystroke, so
// every door that writes lets the refusal through to whoever asked for it. The git ones carry
// git's own sentence, word for word (`AMB-D-906`).
//
// Outside Tauri (`npm run dev` in a browser) there is no filesystem to ask, and the face draws its
// empty state rather than an error: a folder with nothing in it is what the browser fallback is.
import type {
  DropEffectDto, FolderAppDto, FolderCarriedDto, FolderChangesDto, FolderEntryDto, FolderFileDto,
  FolderGitDto, FolderRestoredDto, FolderTrashedDto, GitBranchDto, GitCommitDto, GitFileDto,
  GitStashDto,
} from "../bindings/bindings";
import { invoke } from "../core/ipc";
import { inTauri } from "../core/snapshot";

/**
 * The host's word that a folder moved. It says nothing about what moved in it, and names the folder
 * it is about — a project can be bound to several and each is watched on its own.
 */
const CHANGED_EVENT = "folder://changed";

/** The names directly inside one folder, folders first. `path` is the segments from the root. */
export async function folderEntries(
  projectId: number,
  root: string,
  path: string[],
): Promise<FolderEntryDto[]> {
  if (!inTauri()) return [];
  return await invoke<FolderEntryDto[]>("folder_entries", { projectId, root, path });
}

/**
 * Which mount of a watcher is asking. Counted here rather than in the host, because what it tells
 * apart is two mounts of the same part of this page and nothing outside it — a page reloaded starts
 * again at one and writes over what the page before it left (`crate::folder_watch`).
 */
let mounts = 0;

/** The next one. One per run of the effect that watches, and the same one when it lets go. */
export function nextWatchTag(): number {
  mounts += 1;
  return mounts;
}

/**
 * Start watching one of a project's folders, and take what is in it now.
 *
 * The one call is both the subscription and the first answer, which is what keeps the panel from
 * drawing an empty list for the length of a walk. **Asking for a different folder adds one**: the
 * folders a project is bound to are watched side by side, not one at a time.
 *
 * **More than one part of this face watches the same folder**, so each says which part it is
 * (`watcher`) and which mount of it (`tag`, from {@link nextWatchTag}) — and hands both back to
 * {@link folderUnwatch}. Asking again for a folder somebody is already watching lays no second
 * watch over it: the host's one thread tells every window (`crate::folder_watch`).
 */
export async function folderWatch(
  projectId: number,
  root: string,
  watcher: string,
  tag: number,
): Promise<FolderChangesDto> {
  if (!inTauri()) return { root, capped: false, unwatched: false, gone: false };
  return await invoke<FolderChangesDto>("folder_watch", { projectId, root, watcher, tag });
}

/** What a folder with no git answer looks like, which is what the browser has and what a folder
 *  that is no repository gets: no branch, no rows, and no front to take off a path. */
const NO_GIT: FolderGitDto = { prefix: "", branch: null, rows: [] };

/**
 * What git says about one of a project's folders: where its branch stands, and the paths it named.
 *
 * Asked per bound folder rather than once for the project: what `git status` costs is the amount of
 * tree it is asked about, and two folders of one repository asked together cost five times two
 * folders asked apart (`AMB-D-774`). A folder that is no repository, and a machine with no git,
 * both answer with nothing — which is a tree with no colours on it and not an error to draw.
 *
 * **The branch rides on the same call as the rows.** Where it stands is one line of what `status`
 * already writes, so asking costs nothing over asking for the rows (`AMB-T-4899`).
 */
export async function folderGitStatus(projectId: number, root: string): Promise<FolderGitDto> {
  if (!inTauri()) return NO_GIT;
  return await invoke<FolderGitDto>("folder_git_status", { projectId, root });
}

/**
 * The commits behind one of a project's folders, newest first — a hundred of them, or one path's
 * where `path` names one.
 *
 * `path` is spelled from the repository's root, which is how a commit's own files come back
 * (`folderGitShow`). A row of the tree is spelled from the bound folder, so what
 * `FolderGitDto.prefix` carries goes back on the front of it first.
 *
 * **It is asked for on its own and not with the rows.** Putting the history on the call the colours
 * come from doubles that call, and it would be paid by every reader whether or not they are looking
 * at it (`AMB-T-4899`).
 */
export async function folderGitLog(
  projectId: number,
  root: string,
  path?: string,
): Promise<GitCommitDto[]> {
  if (!inTauri()) return [];
  return await invoke<GitCommitDto[]>("folder_git_log", { projectId, root, path: path ?? null });
}

/** What one commit touched, as the rows the layer under it draws. The paths are the repository's. */
export async function folderGitShow(
  projectId: number,
  root: string,
  sha: string,
): Promise<GitFileDto[]> {
  if (!inTauri()) return [];
  return await invoke<GitFileDto[]>("folder_git_show", { projectId, root, sha });
}

/** The patch for one path of one commit, as git wrote it — empty for every way there is none. */
export async function folderGitDiff(
  projectId: number,
  root: string,
  sha: string,
  path: string,
): Promise<string> {
  if (!inTauri()) return "";
  return await invoke<string>("folder_git_diff", { projectId, root, sha, path });
}

/** What has been put aside in the folder's repository, newest first (`stash@{0}` last made). */
export async function folderGitStashes(
  projectId: number,
  root: string,
): Promise<GitStashDto[]> {
  if (!inTauri()) return [];
  return await invoke<GitStashDto[]>("folder_git_stashes", { projectId, root });
}

/**
 * Put `paths` into the index — `git add`.
 *
 * **What comes back when git says no is git's own sentence** (`AMB-D-906`), here and in every door
 * under it: a read swallows what it cannot do, because the row is about to stop being drawn anyway,
 * and a write's refusal is the reader's next move.
 */
export async function folderGitStage(
  projectId: number,
  root: string,
  paths: string[][],
): Promise<string> {
  if (!inTauri()) return "";
  return await invoke<string>("folder_git_stage", { projectId, root, paths });
}

/** Take `paths` back out of the index, leaving the working tree alone — `git reset`. */
export async function folderGitUnstage(
  projectId: number,
  root: string,
  paths: string[][],
): Promise<string> {
  if (!inTauri()) return "";
  return await invoke<string>("folder_git_unstage", { projectId, root, paths });
}

/**
 * Write `paths` down with `message` on them, naming each of them to git (`AMB-D-906`, 3-2).
 *
 * **Naming them is the point.** A call with none is a call about whatever the index holds at that
 * moment, and what it holds may be the agent in the pane's half-made work — on all three systems,
 * every time (`AMB-T-4901`). What git writes down for a named path is that path as the working tree
 * has it, which is the content the list beside the box was read from.
 */
export async function folderGitCommit(
  projectId: number,
  root: string,
  message: string,
  paths: string[][],
): Promise<string> {
  if (!inTauri()) return "";
  return await invoke<string>("folder_git_commit", { projectId, root, message, paths });
}

/**
 * Put `paths` aside — `git stash push`, under `message` where there is one.
 *
 * **Only paths git already follows.** An untracked one is refused by the pathspec rather than by
 * the stash — `did not match any file(s) known to git`, with nothing put aside (measured here) — so
 * the face hands over the followed ones alone. Putting aside what git has never seen is `-u`, which
 * is a different question and not one this asks.
 */
export async function folderGitStash(
  projectId: number,
  root: string,
  message: string,
  paths: string[][],
): Promise<string> {
  if (!inTauri()) return "";
  return await invoke<string>("folder_git_stash", { projectId, root, message, paths });
}

/**
 * Take one stash back out and drop it — `git stash pop`.
 *
 * `name` is one the list was just read with: `stash@{0}` is where a stash sits and not what it is,
 * so one kept from an earlier read names whatever has since moved into that place.
 */
export async function folderGitStashPop(
  projectId: number,
  root: string,
  name: string,
): Promise<string> {
  if (!inTauri()) return "";
  return await invoke<string>("folder_git_stash_pop", { projectId, root, name });
}

/**
 * Every branch of the folder's repository, with how far each one stands from what it is measured by.
 *
 * **Which of them is checked out is not in here.** That is one line of what `folderGitStatus`
 * already answers, and a second answer to the same question is one that can disagree with the first.
 *
 * It is asked for when the list is opened rather than kept beside the rows: what a reader is shown
 * has to be the branches there are now, and the call is one `for-each-ref` (`AMB-T-4899`).
 */
export async function folderGitBranches(
  projectId: number,
  root: string,
): Promise<GitBranchDto[]> {
  if (!inTauri()) return [];
  return await invoke<GitBranchDto[]>("folder_git_branches", { projectId, root });
}

/**
 * Move the folder's repository onto `branch` — and hand back whatever git said in doing it.
 *
 * **git refusing is the ordinary answer here.** A file being edited that the other branch also
 * changes makes `checkout` exit 1 with its own account of which files stand in the way, and that
 * account is what the reader is shown, word for word (`AMB-D-906`, 3-4).
 */
export async function folderGitSwitch(
  projectId: number,
  root: string,
  branch: string,
): Promise<string> {
  if (!inTauri()) return "";
  return await invoke<string>("folder_git_switch", { projectId, root, branch });
}

/** Make `name` where the reader is standing and move onto it. Whether git will have the name is
 *  git's to say, and it says so in its own words. */
export async function folderGitBranchCreate(
  projectId: number,
  root: string,
  name: string,
): Promise<string> {
  if (!inTauri()) return "";
  return await invoke<string>("folder_git_branch_create", { projectId, root, name });
}

/**
 * The three that go out to the remote, each answering with what git wrote — its own words, whether
 * it worked or not (`AMB-D-906`, 3-4 and 3-5).
 *
 * **The window runs them itself** rather than writing the line into a pane for the agent to run.
 * A window opened from the Dock carries the same ssh agent a terminal does, HTTPS goes through the
 * credential helper, and neither git nor ssh can stop to wait for a passphrase where there is no
 * terminal to ask at (`AMB-T-4900`, `crate::folder_git_write`).
 *
 * ⚠ **The credential helper is the reader's own program and is under none of that.** git gives up in
 * half a second; a helper that waits for an answer nobody can give waits for as long as it likes,
 * and git waits on the helper — measured against Git Credential Manager, which sat there until it
 * was killed. Nothing here can cut that short, so the caller keeps its buttons down until git
 * answers rather than pretending the call came back.
 *
 * A refusal is not swallowed. git is the only one who knows why it stopped — an upstream that was
 * never set, a push someone else got to first, a repository whose LFS filter a window cannot run —
 * and every one of those sentences says more than a word of this app's own would.
 */
export async function folderGitFetch(projectId: number, root: string): Promise<string> {
  if (!inTauri()) return "";
  return await invoke<string>("folder_git_fetch", { projectId, root });
}

/** Read the remote and bring it in. Merge or rebase is the reader's own config, not this app's. */
export async function folderGitPull(projectId: number, root: string): Promise<string> {
  if (!inTauri()) return "";
  return await invoke<string>("folder_git_pull", { projectId, root });
}

/** Send the branch as it stands. */
export async function folderGitPush(projectId: number, root: string): Promise<string> {
  if (!inTauri()) return "";
  return await invoke<string>("folder_git_push", { projectId, root });
}

/**
 * Throw away what the working tree has done to `paths` — the one road here that cannot be walked
 * back (`crate::folder_git_write`).
 *
 * A commit, a stash and a branch switch are all still in the reflog afterwards; changes never
 * written down are nowhere once this returns. So the face asks first (`./RestoreAsk`), and what
 * comes back where git refuses is git's own sentence (`AMB-D-906`).
 */
export async function folderGitRestore(
  projectId: number,
  root: string,
  paths: string[][],
): Promise<string> {
  if (!inTauri()) return "";
  return await invoke<string>("folder_git_restore", { projectId, root, paths });
}

/** Stop git following `paths`, leaving them on disk. Writing them down again puts them back, which
 *  is why nothing is asked before it. */
export async function folderGitUntrack(
  projectId: number,
  root: string,
  paths: string[][],
): Promise<string> {
  if (!inTauri()) return "";
  return await invoke<string>("folder_git_untrack", { projectId, root, paths });
}

/** Write `paths` into the bound folder's own `.gitignore`, one line each. A path already on the
 *  list is left where it is. */
export async function folderGitIgnore(
  projectId: number,
  root: string,
  paths: string[][],
): Promise<string> {
  if (!inTauri()) return "";
  return await invoke<string>("folder_git_ignore", { projectId, root, paths });
}

/**
 * Stop watching one folder, for the one mount that is saying so.
 *
 * Called for each folder a part of the face drew as that part goes away, with the `watcher` and
 * `tag` it watched under. The watch itself comes down with the last part still on it: a reader
 * closing the file they had open is not the tree in the rail letting go of the same folder
 * (`crate::folder_watch`).
 */
export async function folderUnwatch(root: string, watcher: string, tag: number): Promise<void> {
  if (!inTauri()) return;
  await invoke<void>("folder_unwatch", { root, watcher, tag });
}

/**
 * Be told when a watched folder moves, until the returned function is called.
 *
 * One listener hears every watched folder, so what comes back names the one it is about and a
 * caller drawing one of them has to say which. **Being told is the whole of it**: the payload
 * carries no rows, so what a caller does with it is go and ask again — the names of the level it
 * has open, and the colours beside them (`AMB-D-785`).
 */
export async function onFolderChanged(
  take: (changes: FolderChangesDto) => void,
): Promise<() => void> {
  if (!inTauri()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return await listen<FolderChangesDto>(CHANGED_EVENT, ({ payload }) => take(payload));
}

/**
 * What one file has to show: its text, or its picture, or neither.
 *
 * **A text file comes back with a short mark of the bytes it was read from** (`digest`), and that
 * is what a panel holding the file open knows it by: the folder says it moved, the panel reads
 * again, and a mark that came back different is somebody having written to this file
 * (`AMB-D-784`). The same mark goes back into {@link folderSave}. It is over the bytes, so a file
 * read again in another encoding is the same file and answers to the same mark.
 *
 * `encoding` is the reader overruling the guess. Left out, the host guesses as it always does;
 * named — one of the names `folderEncodings` gave — the bytes are decoded as that and nothing is
 * guessed (`AMB-D-773`).
 */
export async function folderRead(
  projectId: number,
  root: string,
  path: string[],
  encoding?: string,
): Promise<FolderFileDto> {
  if (!inTauri()) return { truncated: false, bom: false, lineEnding: "lf", clean: true };
  return await invoke<FolderFileDto>("folder_read", { projectId, root, path, encoding });
}

/**
 * The encodings a file may be reopened in, in the order to offer them.
 *
 * Asked for rather than written here, because what may be offered is what the host can write back
 * and only the host knows that list (`crate::encoding`).
 */
export async function folderEncodings(): Promise<string[]> {
  if (!inTauri()) return [];
  return await invoke<string[]>("folder_encodings");
}

/**
 * Write one file's text back, in the encoding and the newline it was read in.
 *
 * **What the read answered with is what travels back**: the encoding it was read in, whether it
 * began with a byte order mark, and how its lines end. The host remembers none of it between the
 * two calls — a file is written in what it was read in, and this side is what was holding that
 * (`crate::folder_save`).
 *
 * `lineEnding` is never `"mixed"` here. A file that came back mixed has both kinds in it and no
 * right answer about which to keep, so the panel asks the reader and sends what they chose
 * (`AMB-D-773`).
 *
 * It refuses rather than half-saves. What a reader can actually cause is a character the encoding
 * has no room for — a `✓` in a Shift_JIS file — which comes back named, with the file untouched.
 *
 * `seen` is the mark {@link folderRead} handed back, or the one the last save did. **A file that no
 * longer answers to it is not written to**: somebody wrote to it after this side read it, and what
 * the editor holds is older than what is on the disk (`folder_changed_underneath`, `AMB-D-784`).
 * What a save that did happen answers with is the mark of what it wrote, which is what the panel
 * goes on knowing the file by — without it, the panel's next look would find its own save and read
 * it as somebody else's.
 */
export async function folderSave(
  projectId: number,
  root: string,
  path: string[],
  text: string,
  encoding: string,
  bom: boolean,
  lineEnding: "lf" | "crlf",
  seen: string,
): Promise<string> {
  if (!inTauri()) return seen;
  return await invoke<string>(
    "folder_save",
    { projectId, root, path, text, encoding, bom, lineEnding, seen },
  );
}

/**
 * Open one file the way the machine opens that kind of file.
 *
 * The face has an editor of its own, and this is still the way out of it: what a person wants of a
 * file is as often to hand it to something else. Which application that is belongs to the OS, and
 * Amenbo keeps no opinion about it.
 */
export async function folderOpenFile(
  projectId: number,
  root: string,
  path: string[],
): Promise<void> {
  if (!inTauri()) return;
  await invoke<void>("folder_open_file", { projectId, root, path });
}

/**
 * Ask what to open a file with, and draw whatever comes back.
 *
 * **The answer is empty on the operating systems that have a chooser of their own** (Windows,
 * Linux): the host showed it, the reader picked in it and the file is already open — there is
 * nothing left for this side to draw. macOS has no such dialog, so the applications that claim the
 * file come back instead, the usual one first, and the face draws the list itself and hands one
 * back to {@link folderOpenFileWith} (`crate::open_with`).
 *
 * Which of those two a machine is means nothing here. A caller that draws the list it is given and
 * does nothing with an empty one is right on all three.
 */
export async function folderOpenWith(
  projectId: number,
  root: string,
  path: string[],
): Promise<FolderAppDto[]> {
  if (!inTauri()) return [];
  return await invoke<FolderAppDto[]>("folder_open_with", { projectId, root, path });
}

/**
 * Open one file with the application picked off the list {@link folderOpenWith} handed back.
 *
 * `app` is that row's `path` and nothing else: the host checks it against the same list before
 * opening anything, so a name this side made up is refused rather than run.
 */
export async function folderOpenFileWith(
  projectId: number,
  root: string,
  path: string[],
  app: string,
): Promise<void> {
  if (!inTauri()) return;
  await invoke<void>("folder_open_file_with", { projectId, root, path, app });
}

/** Show one file where it lives, in the machine's file manager. */
export async function folderRevealFile(
  projectId: number,
  root: string,
  path: string[],
): Promise<void> {
  if (!inTauri()) return;
  await invoke<void>("folder_reveal_file", { projectId, root, path });
}

/**
 * Put rows into the machine's own bin. Nothing here deletes: the bin is the whole of it, and a
 * machine that cannot offer one says so rather than deleting instead (`crate::trash`).
 *
 * The answer is a line through the list — what went, and the row it stopped on — because once one
 * row is in the bin no single word covers the press. Outside Tauri nothing is binned and the answer
 * says so, which is the empty list.
 */
export async function folderTrash(
  projectId: number,
  root: string,
  paths: string[][],
): Promise<FolderTrashedDto> {
  if (!inTauri()) return { gone: [], stopped: null };
  return await invoke<FolderTrashedDto>("folder_trash", { projectId, root, paths });
}

/**
 * Put back what the last press binned, and say what came back.
 *
 * `null` means there was nothing left to undo — an answer rather than a failure, since pressing undo
 * once more than one deleted is not a mistake. What the host remembers goes when the app does, so a
 * fresh window has nothing to undo either.
 */
export async function folderUntrash(): Promise<FolderRestoredDto | null> {
  if (!inTauri()) return null;
  return await invoke<FolderRestoredDto | null>("folder_untrash", {});
}

/**
 * Move rows into another of the project's folders — the panel's own carry
 * (`crate::folder_write::folder_move`).
 *
 * **What a plain carry inside the panel does.** Both ends are folders the project is bound to, so
 * nothing is taken out of a place Amenbo does not answer for — which is the reason a drop from the
 * desktop copies instead (`folderImport`).
 *
 * `paths` are the rows as the panel knows them, under `root`; `toRoot` and `to` are the folder they
 * are aimed at, and the host proves both ends against the store rather than taking this side's word
 * for them. The answer is a line through the list — the names that arrived, and the one it stopped
 * on. Outside Tauri nothing moves and nothing arrives.
 */
export async function folderMove(
  projectId: number,
  root: string,
  paths: string[][],
  toRoot: string,
  to: string[],
): Promise<FolderCarriedDto> {
  if (!inTauri()) return { arrived: [], stopped: null };
  return await invoke<FolderCarriedDto>("folder_move", { projectId, root, paths, toRoot, to });
}

/**
 * The same carry, leaving the rows where they are (`crate::folder_write::folder_copy`).
 *
 * What asks for this rather than a move is the key held as the row was let go — the platform's own,
 * unlevelled (`./handDrag`).
 */
export async function folderCopy(
  projectId: number,
  root: string,
  paths: string[][],
  toRoot: string,
  to: string[],
): Promise<FolderCarriedDto> {
  if (!inTauri()) return { arrived: [], stopped: null };
  return await invoke<FolderCarriedDto>("folder_copy", { projectId, root, paths, toRoot, to });
}

/**
 * Bring files dropped in from the desktop into one of the project's folders
 * (`crate::folder_write::folder_import`).
 *
 * `paths` are the host's own — whole paths to wherever the reader dragged them from, which is
 * almost never inside the project. Only `toRoot` and `to` are the project's, and the host proves
 * those against the store rather than taking this side's word for them.
 *
 * `effect` is what the operating system said the reader was holding as they let go, passed on
 * unread: **a plain drop copies**, and the host is where that is decided, so a face that reads the
 * keys differently from another face is not a thing that can happen.
 *
 * The answer is a line through the list rather than a yes or a no — the names that arrived, and the
 * one it stopped on. Outside Tauri there is no folder to carry anything into, and nothing arrives.
 */
export async function folderImport(
  projectId: number,
  paths: string[],
  toRoot: string,
  to: string[],
  effect: DropEffectDto,
): Promise<FolderCarriedDto> {
  if (!inTauri()) return { arrived: [], stopped: null };
  return await invoke<FolderCarriedDto>("folder_import", { projectId, paths, toRoot, to, effect });
}

/**
 * Make one name inside a folder: an empty file, or a folder. `path` is the segments of the name
 * being made, the new name last.
 *
 * **Whether the name is free is the filesystem's answer, not one asked for first.** The host makes it
 * in the one call that would have refused an existing one, so nothing here has to guess whether
 * `Alpha.md` counts as taken on a machine that already holds `alpha.md` (`crate::folder_write`).
 *
 * The refusal is not swallowed: which name a person may write is the one thing they cannot work out
 * for themselves, so it goes back to the row they are typing in.
 */
export async function folderMake(
  projectId: number,
  root: string,
  path: string[],
  dir: boolean,
): Promise<void> {
  if (!inTauri()) return;
  await invoke<void>("folder_make", { projectId, root, path, dir });
}

/** Give one name a different one, in the folder it is already in. `name` is the new name alone. */
export async function folderRename(
  projectId: number,
  root: string,
  path: string[],
  name: string,
): Promise<void> {
  if (!inTauri()) return;
  await invoke<void>("folder_rename", { projectId, root, path, name });
}

/**
 * Put rows on the machine's own clipboard, as the files they are (`AMB-D-796`).
 *
 * **What `⌘C` means here is what it means everywhere else on the machine**, so a row copied in this
 * panel pastes into the reader's file manager and a file copied there pastes in here. The webview
 * cannot carry a file, which is why both ends of that are the host's (`crate::clipboard`).
 */
export async function folderClipCopy(
  projectId: number,
  root: string,
  paths: string[][],
): Promise<void> {
  if (!inTauri()) return;
  await invoke<void>("folder_clip_copy", { projectId, root, paths });
}

/**
 * Bring what is on the machine's clipboard into this folder.
 *
 * It answers the way a drop does, and for the same reason: what arrives is paths from outside, and a
 * name already in the way stops the carry and says where it got to. A clipboard holding no files
 * answers with a carry that carried nothing rather than a refusal — pasting a picture into a folder
 * is not a failure, it is a paste of something a folder cannot hold.
 */
export async function folderClipPaste(
  projectId: number,
  toRoot: string,
  to: string[],
): Promise<FolderCarriedDto> {
  if (!inTauri()) return { arrived: [], stopped: null };
  return await invoke<FolderCarriedDto>("folder_clip_paste", { projectId, toRoot, to });
}
