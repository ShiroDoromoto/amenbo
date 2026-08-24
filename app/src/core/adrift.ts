// The ledger's read of what nothing is working on: the reservations and proposals whose pane has gone.
//
// It sits apart from the paged reads (`core/reads`) because it is not a page of the store. The host answers
// it from two halves neither of which can be read off the other — the store says what is reserved and what is
// still proposed, and the running process says which of the panes it started are still alive
// (`commands.rs::adrift`) — and no filter can ask for it, because liveness is not in the store.
import { inTauri } from "./snapshot";
import { invoke } from "./ipc";
import { useQuery } from "./query";
import type { AdriftDto } from "../bindings/bindings";

/**
 * Which of a project's reservations and proposals nothing is working on, as the two sets of ids the
 * ledger looks its own rows up in.
 *
 * The host answers it from two halves neither of which can be read off the other: the store says what
 * is reserved and what is still proposed, and this process says which of the panes it started are
 * still running (`commands.rs::adrift`). What comes back is only ever ids, because the face asking is
 * the one already drawing the rows.
 *
 * **It is a standing answer, not news.** Just after the app comes up nothing is running, so everything
 * a pane ever reserved is in here — which is true, and is why the ledger is where it is put: a person
 * reading a board is reading stock, and this never interrupts to say so (`AMB-D-748`).
 */
export type Adrift = { tasks: Set<number>; decisions: Set<number> };

/** The answer for a project nobody has asked about yet, and the one the browser loop always gets. */
const NOTHING_ADRIFT: Adrift = { tasks: new Set(), decisions: new Set() };

/**
 * Ask the host what nothing is at in `projectId`. Outside Tauri there are no panes to have gone, so
 * the browser iteration loop answers with nothing rather than with a fixture.
 */
export async function fetchAdrift(projectId: number): Promise<Adrift> {
  if (!inTauri()) return NOTHING_ADRIFT;
  const dto = await invoke<AdriftDto>("adrift", { project: projectId });
  return { tasks: new Set(dto.tasks), decisions: new Set(dto.decisions) };
}

/**
 * Subscribing read of the same. It goes stale on any write in the task or decision scopes
 * (`core/query`'s `invalidateScopes`), which is what carries a reservation being handed back or a
 * proposal being settled — the two ways a row leaves this answer while the screen is up.
 */
export function useAdrift(projectId: number | null): Adrift {
  const { data } = useQuery<Adrift>(
    ["adrift", projectId],
    () => (projectId === null ? Promise.resolve(NOTHING_ADRIFT) : fetchAdrift(projectId)),
  );
  return data ?? NOTHING_ADRIFT;
}
