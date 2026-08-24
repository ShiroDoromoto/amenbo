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
  FolderAppDto, FolderChangesDto, FolderEntryDto, FolderFileDto,
} from "../bindings/bindings";
import { invoke } from "../core/ipc";
import { inTauri } from "../core/snapshot";

/** The host's word that the folder moved. It carries the whole list, not what moved in it. */
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
 * Start watching a project's folder, and take what is in it now.
 *
 * Asking again replaces whatever was being watched, so a face that remounts leaves no watch behind
 * it — and the one call is both the subscription and the first answer, which is what keeps the
 * panel from drawing an empty list for the length of a walk.
 */
export async function folderWatch(projectId: number, root: string): Promise<FolderChangesDto> {
  if (!inTauri()) return { changed: [], partial: false };
  return await invoke<FolderChangesDto>("folder_watch", { projectId, root });
}

/** Stop watching. Called when the face goes away; nothing else has to. */
export async function folderUnwatch(): Promise<void> {
  if (!inTauri()) return;
  await invoke<void>("folder_unwatch", {});
}

/**
 * Be told when the folder moves, until the returned function is called.
 *
 * The payload is the whole list rather than a delta: the face draws a list, and a delta it had to
 * apply would be a second copy of the truth to keep in step with the host's.
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
  if (!inTauri()) return { truncated: false };
  return await invoke<FolderFileDto>("folder_read", { projectId, root, path });
}

/**
 * Open one file the way the machine opens that kind of file.
 *
 * The face reads and does not edit (`AMB-T-3602`), so what it can offer is the reader's own
 * applications — which one that is belongs to the OS, and Amenbo keeps no opinion about it.
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
