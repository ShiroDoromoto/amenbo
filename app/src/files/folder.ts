// The seam to what a folder holds (`crate::folder`).
//
// Three questions, one fence behind them: the names inside one folder, the files written to most
// recently, and what one file has to show. The root every one of them is rooted at is a folder the
// project is bound to — the host checks that against the store rather than taking this side's word
// for it, so nothing here has to be careful about which path it passes.
//
// Outside Tauri (`npm run dev` in a browser) there is no filesystem to ask, and the face draws its
// empty state rather than an error: a folder with nothing in it is what the browser fallback is.
import type { FolderChangedDto, FolderEntryDto, FolderFileDto } from "../bindings/bindings";
import { invoke } from "../core/ipc";
import { inTauri } from "../core/snapshot";

/** The names directly inside one folder, folders first. `path` is the segments from the root. */
export async function folderEntries(
  projectId: number,
  root: string,
  path: string[],
): Promise<FolderEntryDto[]> {
  if (!inTauri()) return [];
  return await invoke<FolderEntryDto[]>("folder_entries", { projectId, root, path });
}

/** The files written to most recently, newest first. A walk, asked again rather than watched. */
export async function folderRecent(projectId: number, root: string): Promise<FolderChangedDto[]> {
  if (!inTauri()) return [];
  return await invoke<FolderChangedDto[]>("folder_recent", { projectId, root });
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
