import { useEffect, useMemo, useState } from "react";
import type { AdriftDto, AdriftRowDto } from "../bindings/bindings";
import { fetchAdrift } from "../core/mutations";
import { useRefNav } from "../core/refNav";
import { t } from "../core/i18n";

/**
 * An empty slot, and — where there is one — what this project was left in the middle of.
 *
 * **A process can die and the ledger not hear about it.** What is left is a task sitting reserved with
 * nobody at it — out of the mailbox, so nobody is offered it — or a decision put up for discussion that
 * nobody will ever bring to a close. Nothing on any screen said so. This is where it is said, because
 * an empty slot is the one place on the face with room for it and the one place a reader is already
 * looking for something to start.
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
  const [adrift, setAdrift] = useState<AdriftDto>({ tasks: [], decisions: [] });
  const nav = useRefNav();

  // Re-read whenever the store moves: a task the reader has just picked up again is one this must stop
  // asking about, and the answer changes from outside this window by construction.
  useEffect(() => {
    let alive = true;
    if (folder === null) {
      setAdrift(NOTHING);
      return;
    }
    const read = () => {
      fetchAdrift(folder)
        .then((left) => { if (alive) setAdrift(left); })
        // A read that failed says nothing, which is the safe half of being wrong here: the question is
        // an offer, and an offer nobody gets costs less than one raised on a guess.
        .catch(() => { if (alive) setAdrift(NOTHING); });
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

  // Opening one leaves this face, and which face it lands on is the record's: a task and a decision are
  // read in different places, which is why the two are answered for apart.
  const open = useMemo(
    () => ({
      task: (id: number) => { onOpenLedger?.(); nav.selectTask?.(id); },
      decision: (id: number) => { onOpenLedger?.(); nav.selectDecision?.(id); },
    }),
    [nav, onOpenLedger],
  );

  if (adrift.tasks.length === 0 && adrift.decisions.length === 0) {
    return <button className="slot slot--empty" onClick={onOpen}>{t("face.open")}</button>;
  }

  // One question over both. What tells a reader which kind a row is, and so what pressing it opens, is
  // the ref it carries: the two are separate numbering spaces and the app names them apart everywhere.
  const rows: (AdriftRowDto & { press: () => void })[] = [
    ...adrift.tasks.map((one) => ({ ...one, press: () => open.task(one.id) })),
    ...adrift.decisions.map((one) => ({ ...one, press: () => open.decision(one.id) })),
  ];

  return (
    <div className="slot slot--adrift">
      <p className="adrift__ask">{t("face.adrift")}</p>
      <ul className="adrift__list">
        {rows.map((row) => (
          <li key={row.ref}>
            <button className="adrift__task" onClick={row.press}>
              <span className="adrift__ref">{row.ref}</span>
              <span className="adrift__title">{row.title}</span>
            </button>
          </li>
        ))}
      </ul>
      <button className="slot__open adrift__open" onClick={onOpen}>{t("face.open")}</button>
    </div>
  );
}

/** Nothing left behind — what a slot with no project to ask about draws, and what a failed read says. */
const NOTHING: AdriftDto = { tasks: [], decisions: [] };
