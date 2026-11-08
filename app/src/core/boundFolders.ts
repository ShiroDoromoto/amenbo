// The folders a project is bound to, read once for whoever on the screen needs them.
//
// A project is bound to its folders and not the other way round, so anything that speaks about "this
// project's folder" has to ask. Three things on the board do: whether to warn that there is none
// (`AMB-D-533`), which folder the first loop opens a terminal in, and — since the answer decides which
// standing notice is drawn at all — the board's own ordering.
//
// `answered` is the half that is easy to drop. Nothing should be drawn from an unanswered read: a flash
// of "no folder here" on a project that has one reads as a broken binding, which is worse to say than
// nothing. A read that fails is treated as answered-with-none, since the invitation to link one is the
// safe half of being wrong.
//
// It is re-read when the "projects" scope moves, because a binding is exactly the kind of thing that
// happens while this is on screen — the settings screen links one, and the workspace binds a project's
// first folder on the way to opening a pane. Held from the first read, the answer would send the next
// press back to the folder picker for a project that has just been given one. A binding folds to that
// scope (`core/changes`), and a write anywhere else in the store leaves it alone.
//
// Whether a folder is still there is read off the disk, not the store, so a folder moved away while
// this is on screen shows up only on the next read: a binding written, or a whole reload of the store.
import { useQuery } from "./query";
import { fetchBoundFolders } from "./mutations";
import type { BoundFolderDto } from "../bindings/bindings";

export type BoundFolders = {
  /** Every folder recorded for this project, gone ones included. */
  all: BoundFolderDto[];
  /** The ones that are actually there — the only ones an AI can be started in. */
  live: BoundFolderDto[];
  /** False until the read comes back. Draw nothing before it. */
  answered: boolean;
};

const NONE: BoundFolderDto[] = [];

/** `projectId` is nullable for the workspace, which asks before it has been told which project it
 *  is on. A read for nothing comes back answered-with-none: there is no project whose folders could be
 *  missing, so there is nothing to warn about and nothing to wait for. */
export function useBoundFolders(projectId: number | null): BoundFolders {
  const { data, error } = useQuery(["boundFolders", projectId], () =>
    projectId === null ? Promise.resolve(NONE) : fetchBoundFolders(projectId),
  );
  const all = data ?? NONE;
  return {
    all,
    live: all.filter((one) => one.exists),
    answered: projectId === null || data !== undefined || error !== undefined,
  };
}
