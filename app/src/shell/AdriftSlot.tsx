import { useEffect, useMemo, useState } from "react";
import type { AdriftDto, AdriftRowDto, WakeDto } from "../bindings/bindings";
import { fetchAdrift } from "../core/mutations";
import { invoke } from "../core/ipc";
import { useRefNav } from "../core/refNav";
import { SHELL } from "../talk/terminal";
import { t } from "../core/i18n";
import { Icon } from "../components/Icon";

/**
 * The empty frame on a page with room, and — where there is one — what this project was left in the
 * middle of.
 *
 * **A process can die and the ledger not hear about it.** What is left is a task sitting reserved with
 * nobody at it — out of the mailbox, so nobody is offered it — or a decision put up for discussion that
 * nobody will ever bring to a close. Nothing on any screen said so. This is where it is said, because
 * a face with no panes on it is the one place with room for it and the one place a reader is already
 * looking for something to start.
 *
 * **It asks; it does not decide** (`AMB-D-748`). Amenbo knows the reservation was made in a pane it
 * opened and that the pane has gone — it does not know whether the person went on at their own terminal,
 * so the row is a question and pressing one opens the task rather than moving it. Nothing here writes.
 *
 * The read is scoped to a folder of the project being shown, so the face only ever asks about the
 * project it is on (`../talk/layout`). A project bound to no folder has nothing to ask about and draws
 * the plain empty frame.
 *
 * **There is one of these on a page and only where the page has a gap** — that is the face's to decide,
 * and it decides it from the frames (`./TerminalFace`). What arrives here is already the single empty
 * frame, so this draws unconditionally: four identical questions is the thing the count of them
 * prevents, not the thing this component checks.
 *
 * **What a terminal is opened with is chosen here**, on the frame, and pressed once. The row above
 * the button is every agent this machine can start plus the plain shell, with the answer the host
 * arrived at already on — so the common press is the same one press it always was, and choosing
 * something else costs a press and no dialog (`AMB-T-3667`). What comes up on is the project's pin
 * where it has one, else what this person last opened with, and the choice made here is kept as the
 * second of those (`../talk/agent` · `crate::wake`).
 *
 * **The first run is the one time nothing is on.** Nobody has chosen, there is more than one thing
 * to choose between, and the frame says so on the button rather than guessing: it reads "choose one"
 * and does not press. A reader is never told why a press did nothing, because there is no press that
 * does nothing (`AMB-T-3686`). A machine with a single startable agent is not that case — one thing
 * to open with is not a question — and neither is a machine with none, where the shell is the whole
 * of the row and is on.
 *
 * `onOpen` is what this is for when there is nothing to ask: opening a pane in this project, with
 * the agent that was chosen — {@link SHELL} for a prompt with nothing started at it, and null where
 * the read that would have said never came back, which leaves the answer to be settled on the pane's
 * own side (`../talk/agent`).
 */
export function AdriftSlot({
  folders,
  project,
  onOpen,
  onOpenLedger,
}: {
  /** The folders the project being shown is bound to. The first is what the adrift read is scoped to
   *  and the whole list is what the agents are traced across (`crate::wake`). */
  folders: readonly string[];
  /** The project being shown, or null on a face that has none — which is a face with nothing to
   *  keep an answer against. */
  project: number | null;
  onOpen: (agent: string | null) => void;
  /** Bring the ledger up. Selecting a task happens on the other face, so following one from here has
   *  to leave this one — the same move the file face makes (`../files/FilesPanel`). */
  onOpenLedger?: () => void;
}) {
  const [adrift, setAdrift] = useState<AdriftDto>({ tasks: [], decisions: [] });
  const nav = useRefNav();
  const folder = folders[0] ?? null;
  // What this machine can start, and what this project has settled on. Read once per project rather
  // than per press: a probe is a login shell reading the reader's own profile (`crate::wake`), and a
  // frame that ran one on every render would pay for it on every keystroke elsewhere on the page.
  const [wake, setWake] = useState<WakeDto | null>(null);
  // Which of them the next pane opens with, once the reader has said. Null until they do, because
  // what is on before that is the host's answer and that arrives with the read.
  const [chose, setChose] = useState<string | null>(null);
  const key = folders.join("\n");

  useEffect(() => {
    let alive = true;
    setWake(null);
    setChose(null);
    invoke<WakeDto>("wake_choices", { project, folders: key === "" ? [] : key.split("\n") })
      .then((said) => { if (alive) setWake(said); })
      // A read that failed leaves the row off and the button alone: the frame's job is to open a
      // pane, and the agent it opens with is settled again on the other side (`../talk/agent`).
      .catch(() => { if (alive) setWake(null); })
    ;
    return () => { alive = false; };
  }, [project, key]);

  // The row, in catalog order, with the plain shell at the end of it. The shell is not an agent and
  // has no row in the catalogue, so it is put here rather than found (`../talk/agent`).
  //
  // **Empty until the read comes back, and empty again if it did not.** A row is what this machine
  // can start, and a frame that has not been told cannot draw one — the shell alone would be a row
  // saying this machine has no agents on it, which is a different answer from having no answer.
  const starts = useMemo(() => {
    if (wake === null) return [];
    const offered = wake.offered
      .flatMap((id) => wake.candidates.filter((one) => one.id === id))
      .map((one) => ({ id: one.id, label: one.label }));
    return [...offered, { id: SHELL, label: t("talk.shell") }];
  }, [wake]);
  // What is on: what the reader said, else what the host arrived at, else — where the row has one
  // thing on it — that one, since a row of one is not a choice. Null is nobody having chosen: the
  // first run, and the read that has not come back or did not come.
  const on = chose ?? wake?.settled ?? (starts.length === 1 ? starts[0]!.id : null);
  // The first run, and only it: the row is drawn, nothing on it is on, and the button says so. A
  // frame that never heard from the host is not this — it opens, and the pane settles what with.
  const asking = wake !== null && on === null;

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

  /** The row of things to open with, and the one press that opens on the one that is on. */
  const ways = (
    <>
      {starts.length > 1 && (
        <div className="slot__starts" role="radiogroup" aria-label={t("talk.startWith")}>
          {starts.map((one) => (
            <button
              key={one.id}
              className={`slot__start${one.id === on ? " slot__start--on" : ""}`}
              // Choosing between things, not turning one of them on: what a press does is say which
              // of the row the pane opens with, and exactly one of them is always on.
              role="radio"
              aria-checked={one.id === on}
              onClick={() => setChose(one.id)}
            >
              {/* An agent wears the mark; the plain shell does not. What the row is choosing between
                  is what runs in the pane, and one of the choices is nothing running at all — a mark
                  on that one would say the shell is an agent of some kind, which is the distinction
                  the row exists to draw. It is one mark for all of them and not one each: which
                  agent it is, is what the name says. */}
              {one.id !== SHELL && <Icon name="robot" />}
              {one.label}
            </button>
          ))}
        </div>
      )}
      {/* Not pressed while nothing is on, and it says which it is rather than explaining a refusal
          afterwards: what the reader has to do is written on the thing they would press. */}
      <button className="slot__open" onClick={() => onOpen(on)} disabled={asking}>
        {asking ? t("face.openPick") : t("face.open")}
      </button>
    </>
  );

  if (adrift.tasks.length === 0 && adrift.decisions.length === 0) {
    return <div className="slot slot--empty">{ways}</div>;
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
      <div className="adrift__open">{ways}</div>
    </div>
  );
}

/** Nothing left behind — what a face with no project to ask about draws, and what a failed read says. */
const NOTHING: AdriftDto = { tasks: [], decisions: [] };
