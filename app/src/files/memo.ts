// The seam to the project's draft page (`amenbo_core::memo`).
//
// One page per project, plain text, kept in the store rather than on the machine: the page belongs
// to the project, and two machines on the same store are writing on the same one (`AMB-T-3608`).
//
// Outside Tauri (`npm run dev` in a browser) there is no store to write to, and the page is an empty
// one that keeps nothing — the same shape the rest of this folder's seams take.
import { invoke } from "../core/ipc";
import { inTauri } from "../core/snapshot";

/** What is written on this project's page — empty where nothing is. */
export async function projectMemo(projectId: number): Promise<string> {
  if (!inTauri()) return "";
  return await invoke<string>("project_memo", { projectId });
}

/** Write the page. Blank erases it: a page nobody wrote on is not a page. */
export async function setProjectMemo(projectId: number, text: string): Promise<void> {
  if (!inTauri()) return;
  await invoke<void>("set_project_memo", { projectId, text });
}
