import { useEffect, useMemo, useState } from "react";
import type { TaskCardDto } from "../bindings/bindings";
import { fetchAdriftTasks } from "../core/mutations";
import { useRefNav } from "../core/refNav";
import { t } from "../core/i18n";

/**
 * An empty slot, and — where there is one — the work in this project that nothing is doing any more.
 *
 * **A process can die and the ledger not hear about it.** What is left is a task sitting reserved with
 * nobody at it: it is out of the mailbox, so nobody is offered it, and nothing on any screen says so.
 * This is where that is said, because an empty slot is the one place on the face with room for it and
 * the one place a reader is already looking for something to start.
 *
 * **It asks; it does not decide** (`AMB-D-748`). Amenbo knows the reservation was made in a pane it
 * opened and that the pane has gone — it does not know whether the person went on at their own terminal,
 * so the row is a question and pressing one opens the task rather than moving it. Nothing here writes.
 *
 * The read is scoped to the page's folder, so a screen only ever asks about the project it is in
 * (`../talk/layout`). A page with no folder has nothing to ask about and draws the plain way in.
 *
 * `onOpen` is what the slot is for when there is nothing to ask: starting a terminal here.
 */
export function AdriftSlot({
  folder,
  onOpen,
  onOpenLedger,
}: {
  /** The folder this page's panes work in, or null before one has been settled. */
  folder: string | null;
  onOpen: () => void;
  /** Bring the ledger up. Selecting a task happens on the other face, so following one from here has
   *  to leave this one — the same move the file face makes (`../files/FilesPanel`). */
  onOpenLedger?: () => void;
}) {
  const [adrift, setAdrift] = useState<TaskCardDto[]>([]);
  const nav = useRefNav();

  // Re-read whenever the store moves: a task the reader has just picked up again is one this must stop
  // asking about, and the answer changes from outside this window by construction.
  useEffect(() => {
    let alive = true;
    if (folder === null) {
      setAdrift([]);
      return;
    }
    const read = () => {
      fetchAdriftTasks(folder)
        .then((tasks) => { if (alive) setAdrift(tasks); })
        // A read that failed says nothing, which is the safe half of being wrong here: the question is
        // an offer, and an offer nobody gets costs less than one raised on a guess.
        .catch(() => { if (alive) setAdrift([]); });
    };
    read();
    let off: (() => void) | null = null;
    void import("@tauri-apps/api/event")
      .then(({ listen }) => listen("store-changed", read))
      .then((stop) => { if (alive) off = stop; else stop(); })
      // Outside Tauri (browser iteration) nothing writes to a store and nothing announces one.
      .catch(() => {});
    return () => { alive = false; off?.(); };
  }, [folder]);

  const open = useMemo(
    () => (id: number) => { onOpenLedger?.(); nav.selectTask?.(id); },
    [nav, onOpenLedger],
  );

  if (adrift.length === 0) {
    return <button className="slot slot--empty" onClick={onOpen}>{t("face.open")}</button>;
  }

  return (
    <div className="slot slot--adrift">
      <p className="adrift__ask">{t("face.adrift")}</p>
      <ul className="adrift__list">
        {adrift.map((task) => (
          <li key={task.id}>
            <button className="adrift__task" onClick={() => open(task.id)}>
              <span className="adrift__ref">{task.ref}</span>
              <span className="adrift__title">{task.title}</span>
            </button>
          </li>
        ))}
      </ul>
      <button className="slot__open adrift__open" onClick={onOpen}>{t("face.open")}</button>
    </div>
  );
}
