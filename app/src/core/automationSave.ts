// **What each write on an automation or an action became of**, for the build screens that say it was
// saved (`../screens/AutomationSaved`, `AMB-D-1005`). The writes are `./automations`'; this is only
// the line they report on, kept apart so a screen can listen without reaching for the writes.
/**
 * **What one write on a definition is doing**, told to the build screens that draw that it was saved
 * (`../screens/AutomationSaved`, `AMB-D-1005`). There is no Save press (`AMB-D-32`), so the screen says
 * so itself: when the last write landed, which field it came from, and why one was refused.
 *
 * `start` is told **before the call returns**, inside the handler that made the write — that is how
 * the screen knows which field the write came from.
 */
export type SaveNews =
  | { id: number; state: "start" }
  | { id: number; state: "saved" }
  | { id: number; state: "failed"; error: unknown };

const saveListeners = new Set<(news: SaveNews) => void>();
let saveSeq = 0;

/** Hear every write on a definition from here on. Hands back the way to stop hearing. */
export function onAutomationSave(listen: (news: SaveNews) => void): () => void {
  saveListeners.add(listen);
  return () => saveListeners.delete(listen);
}

function tell(news: SaveNews) {
  for (const listen of saveListeners) listen(news);
}

/** Send one write on a definition, and tell the listeners what became of it. */
export function told<T>(write: () => Promise<T>): Promise<T> {
  const id = ++saveSeq;
  tell({ id, state: "start" });
  return write().then(
    (answer) => {
      tell({ id, state: "saved" });
      return answer;
    },
    (error: unknown) => {
      tell({ id, state: "failed", error });
      throw error;
    },
  );
}
