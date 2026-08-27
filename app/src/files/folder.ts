// The seam to what a folder holds (`crate::folder`, `crate::folder_watch`).
//
// The names inside one folder and what one file has to show are asked for; what changed lately is
// **watched** — the host installs a watch over the folder and says so as it happens, rather than
// this side guessing when to go and look (`AMB-T-3604`). The root all of them are rooted at is a
// folder the project is bound to, and the host checks that against the store rather than taking
// this side's word for it, so nothing here has to be careful about which path it passes.
//
// Outside Tauri (`npm run dev` in a browser) there is no filesystem to ask, and the face draws its
// empty state rather than an error: a folder with nothing in it is what the browser fallback is.
import type {
  DropEffectDto, FolderAppDto, FolderCarriedDto, FolderChangesDto, FolderEntryDto, FolderFileDto,
} from "../bindings/bindings";
import { invoke } from "../core/ipc";
import { inTauri } from "../core/snapshot";

/**
 * The host's word that a folder moved. It carries the whole list, not what moved in it, and names
 * the folder it is about — a project can be bound to several and each is watched on its own.
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
  if (!inTauri()) return { root, changed: [], partial: false, gone: false };
  return await invoke<FolderChangesDto>("folder_watch", { projectId, root });
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
 * caller drawing one of them has to say which. The payload is the whole list rather than a delta:
 * the face draws a list, and a delta it had to apply would be a second copy of the truth to keep in
 * step with the host's.
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
