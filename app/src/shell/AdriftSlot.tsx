import { useEffect, useId, useMemo, useState } from "react";
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
 * the button is every agent this machine can start plus the plain shell, with the project's own
 * answer already on — so the common press is the same one press it always was, and choosing
 * something else costs a press and no dialog (`AMB-T-3667`). **The row is asked about in words as
 * well as drawn as pressable**: shape alone leaves it to be worked out, and a reader who takes the
 * names for a label of the button under them never tries one. The question is what the row opens
 * with, not which AI — the shell on it is neither (`AMB-T-3682`). What is chosen here is this pane's:
 * the project settles its answer the first time and changes it on its own settings, never by
 * somebody reaching for a different tool for one turn (`../talk/agent`).
 *
 * `onOpen` is what this is for when there is nothing to ask: opening a pane in this project, with
 * the agent that was chosen — {@link SHELL} for a prompt with nothing started at it.
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
  onOpen: (agent: string) => void;
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
  // what is on before that is the project's answer and that arrives with the read.
  const [chose, setChose] = useState<string | null>(null);
  const key = folders.join("\n");
  // What names the row of choices, for a reader who is hearing the frame rather than seeing it. The
  // question is on the screen, so the row is pointed at it rather than given a second wording of its
  // own — two names for one thing is how the spoken frame and the drawn one drift apart.
  const askId = useId();

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
  const starts = useMemo(() => {
    const offered = (wake?.offered ?? [])
      .flatMap((id) => (wake?.candidates ?? []).filter((one) => one.id === id))
      .map((one) => ({ id: one.id, label: one.label }));
    return [...offered, { id: SHELL, label: t("talk.shell") }];
  }, [wake]);
  // What is on: what the reader said, else the project's answer, else the first thing on the row.
  const on = chose ?? wake?.settled ?? starts[0]?.id ?? SHELL;

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
        <div className="slot__pick">
          {/* The question the row is an answer to. It is put where the row is put and nowhere else:
              a frame with one thing to open with has nothing to ask about, and asking anyway would
              be a question with one answer. */}
          <p className="slot__ask" id={askId}>{t("face.whichStart")}</p>
          <div className="slot__starts" role="radiogroup" aria-labelledby={askId}>
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
        </div>
      )}
      <button className="slot__open" onClick={() => onOpen(on)}>{t("face.open")}</button>
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
