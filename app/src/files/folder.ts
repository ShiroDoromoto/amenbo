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
// row is about to stop being drawn anyway — but a name refused is a person's next keystroke, so the
// two doors at the end let the refusal through to whoever asked for it.
//
// Outside Tauri (`npm run dev` in a browser) there is no filesystem to ask, and the face draws its
// empty state rather than an error: a folder with nothing in it is what the browser fallback is.
import type {
  FolderAppDto, FolderChangesDto, FolderEntryDto, FolderFileDto, GitEntryDto,
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
 * Start watching one of a project's folders, and take what is in it now.
 *
 * Asking again for the same folder replaces its watch, so a face that remounts leaves no watch
 * behind it — and the one call is both the subscription and the first answer, which is what keeps
 * the panel from drawing an empty list for the length of a walk. **Asking for a different folder
 * adds one**: the folders a project is bound to are watched side by side, not one at a time.
 */
export async function folderWatch(projectId: number, root: string): Promise<FolderChangesDto> {
  if (!inTauri()) return { root, capped: false, unwatched: false, gone: false };
  return await invoke<FolderChangesDto>("folder_watch", { projectId, root });
}

/**
 * What git says about one of a project's folders, as the rows a tree draws its colours from.
 *
 * Asked per bound folder rather than once for the project: what `git status` costs is the amount of
 * tree it is asked about, and two folders of one repository asked together cost five times two
 * folders asked apart (`AMB-D-774`). A folder that is no repository, and a machine with no git,
 * both answer with nothing — which is a tree with no colours on it and not an error to draw.
 */
export async function folderGitStatus(
  projectId: number,
  root: string,
): Promise<GitEntryDto[]> {
  if (!inTauri()) return [];
  return await invoke<GitEntryDto[]>("folder_git_status", { projectId, root });
}

/** Stop watching one folder. Called for each folder the face drew as it goes away. */
export async function folderUnwatch(root: string): Promise<void> {
  if (!inTauri()) return;
  await invoke<void>("folder_unwatch", { root });
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

/** What one file has to show: its text, or its picture, or neither. */
export async function folderRead(
  projectId: number,
  root: string,
  path: string[],
): Promise<FolderFileDto> {
  if (!inTauri()) return { truncated: false, bom: false, lineEnding: "lf", clean: true };
  return await invoke<FolderFileDto>("folder_read", { projectId, root, path });
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
