import { useEffect, useId, useMemo, useState } from "react";
import type { WakeDto } from "../bindings/bindings";
import { invoke } from "../core/ipc";
import { SHELL } from "../talk/terminal";
import { t, tf } from "../core/i18n";
import { Icon } from "../components/Icon";

/**
 * The empty frame on a page with room: the dashed place that says a terminal can be opened here, and
 * the one press that opens it.
 *
 * **It says there is room, and nothing else.** A frame with nothing open on it once carried the
 * project's leftovers as well — work reserved with nobody at it, decisions nobody would close — and
 * that is gone. The reading was true only of a machine that had been running a while:
 * just after the app comes up no session is alive, so everything ever reserved from a pane read as
 * left behind and the frame opened on a standing inventory. A signal that fires always is not one.
 * The fact underneath is real and worth showing; the place for it is the ledger, not this frame.
 *
 * **There is one of these on a page and only where the page has a gap** — that is the face's to decide,
 * and it decides it from the frames (`./TerminalFace`).
 *
 * **What a terminal is opened with is chosen here**, on the frame, and pressed once. The row above
 * the button is every agent Amenbo knows how to start plus the plain shell, with the answer the host
 * arrived at already on — so the common press is the same one press it always was, and choosing
 * something else costs a press and no dialog (`AMB-T-3667`). **The row is asked about in words as
 * well as drawn as pressable**: shape alone leaves it to be worked out, and a reader who takes the
 * names for a label of the button under them never tries one. The question is what the row opens
 * with, not which AI — the shell on it is neither (`AMB-T-3682`). What comes up on is the project's
 * pin where it has one, else what this person last opened with, and the choice made here is kept as
 * the second of those (`../talk/agent` · `crate::wake`).
 *
 * **The row holds what this machine has not got as well, folded away** (`AMB-D-792`). An agent that
 * is not installed is drawn greyed and cannot be pressed — pressing it would open a pane on
 * `command not found` — and it is on the row all the same, because a reader who cannot see that
 * Cursor is one of the choices has no way of learning that installing it would give them one. They
 * are folded because they are usually the many: two agents installed out of six leaves four rows
 * that do nothing standing over the two that do.
 *
 * **The first run is the one time nothing is on.** Nobody has chosen, there is more than one thing
 * to choose between, and the frame says so on the button rather than guessing: it reads "choose one"
 * and does not press. A reader is never told why a press did nothing, because there is no press that
 * does nothing (`AMB-T-3686`). A machine with a single startable agent is not that case — one thing
 * to open with is not a question — and neither is a machine with none, where the shell is the whole
 * of what can be started and is on, whatever else the row draws beside it.
 *
 * `onOpen` is what this is for: opening a pane in this project, with the agent that was chosen —
 * {@link SHELL} for a prompt with nothing started at it, and null where the read that would have
 * said never came back, which leaves the answer to be settled on the pane's own side
 * (`../talk/agent`).
 */
/** One thing the row offers: what a press says, and what it is called. */
type Start = { id: string; label: string };

export function EmptySlot({
  folders,
  project,
  onOpen,
}: {
  /** The folders the project being shown is bound to — what the agents are traced across
   *  (`crate::wake`). */
  folders: readonly string[];
  /** The project being shown, or null on a face that has none — which is a face with nothing to
   *  keep an answer against. */
  project: number | null;
  onOpen: (agent: string | null) => void;
}) {
  const folderKey = folders.join("\n");
  // What this machine can start, and what this project has settled on. Read once per project rather
  // than per press: a probe is a login shell reading the reader's own profile (`crate::wake`), and a
  // frame that ran one on every render would pay for it on every keystroke elsewhere on the page.
  const [wake, setWake] = useState<WakeDto | null>(null);
  // Which of them the next pane opens with, once the reader has said. Null until they do, because
  // what is on before that is the host's answer and that arrives with the read.
  const [chose, setChose] = useState<string | null>(null);
  // What names the row of choices, for a reader who is hearing the frame rather than seeing it. The
  // question is on the screen, so the row is pointed at it rather than given a second wording of its
  // own — two names for one thing is how the spoken frame and the drawn one drift apart.
  const askId = useId();
  // Whether the ones this machine has not got are unfolded. Folded to begin with, every time: what
  // the frame is for is opening a terminal, and the row that does it is the one that has to be in
  // front of the reader.
  const [shown, setShown] = useState(false);

  useEffect(() => {
    let alive = true;
    setWake(null);
    setChose(null);
    invoke<WakeDto>("wake_choices", { project, folders: folderKey === "" ? [] : folderKey.split("\n") })
      .then((said) => { if (alive) setWake(said); })
      // A read that failed leaves the row off and the button alone: the frame's job is to open a
      // pane, and the agent it opens with is settled again on the other side (`../talk/agent`).
      .catch(() => { if (alive) setWake(null); })
    ;
    return () => { alive = false; };
  }, [project, folderKey]);

  // The row, in catalog order, in two groups: what this machine can start, and what it has not got
  // (`AMB-D-792`). The plain shell ends the first — it is not an agent and has no row in the
  // catalogue, so it is put here rather than found, and it is the one thing that can never be
  // missing, since a folder always has a prompt (`../talk/agent`).
  //
  // **Both empty until the read comes back, and empty again if it did not.** A frame that has not
  // been told cannot draw a row — the shell alone would say this machine has no agents on it, which
  // is a different answer from having no answer.
  const rows = useMemo(() => {
    if (wake === null) return { usable: [] as Start[], missing: [] as Start[] };
    const drawn = wake.offered.flatMap((id) => wake.candidates.filter((one) => one.id === id));
    return {
      usable: [
        ...drawn.filter((one) => one.installed).map((one) => ({ id: one.id, label: one.label })),
        { id: SHELL, label: t("talk.shell") },
      ],
      missing: drawn.filter((one) => !one.installed).map((one) => ({ id: one.id, label: one.label })),
    };
  }, [wake]);
  // What is on: what the reader said, else what the host arrived at, else — where there is one thing
  // to start — that one, since a row of one is not a choice. It is never one of the missing: the
  // host settles on what it can start and nothing else (`crate::wake::startable`). Null is nobody
  // having chosen: the first run, and the read that has not come back or did not come.
  const on = chose ?? wake?.settled ?? (rows.usable.length === 1 ? rows.usable[0]!.id : null);
  // The first run, and only it: the row is drawn, nothing on it is on, and the button says so. A
  // frame that never heard from the host is not this — it opens, and the pane settles what with.
  const asking = wake !== null && on === null;

  /** One thing to open with, drawn as a pill. `missing` is a provider this machine has not got: it
   *  stays in the group so the row reads as "these are the choices, some of them not yet here", and
   *  it is not pressable, because pressing it would open a pane on `command not found`. */
  const pill = (one: Start, missing: boolean) => (
    <button
      key={one.id}
      className={`slot__start${one.id === on ? " slot__start--on" : ""}${missing ? " slot__start--missing" : ""}`}
      // Choosing between things, not turning one of them on: what a press does is say which of the
      // row the pane opens with, and exactly one of them is always on.
      role="radio"
      aria-checked={one.id === on}
      // Said rather than left to the greying, which no screen reader reports, and `aria-disabled`
      // rather than `disabled`: a row nothing can reach is a row nobody learns is there.
      aria-disabled={missing || undefined}
      onClick={() => { if (!missing) setChose(one.id); }}
    >
      {/* An agent wears the mark; the plain shell does not. What the row is choosing between
          is what runs in the pane, and one of the choices is nothing running at all — a mark
          on that one would say the shell is an agent of some kind, which is the distinction
          the row exists to draw. It is one mark for all of them and not one each: which
          agent it is, is what the name says. */}
      {one.id !== SHELL && <Icon name="robot" />}
      {one.label}
    </button>
  );

  return (
    <div className="slot slot--empty">
      {rows.usable.length + rows.missing.length > 1 && (
        <div className="slot__pick">
          {/* The question the row is an answer to. It is put where the row is put and nowhere else:
              a frame with one thing to open with has nothing to ask about, and asking anyway would
              be a question with one answer. */}
          <p className="slot__ask" id={askId}>{t("face.whichStart")}</p>
          <div className="slot__starts" role="radiogroup" aria-labelledby={askId}>
            {rows.usable.map((one) => pill(one, false))}
            {shown && rows.missing.map((one) => pill(one, true))}
          </div>
          {/* Folded away rather than always drawn: on a machine with two agents the other four would
              take more of the frame than the two that work. It is a press and not a choice, so it is
              outside the group — a button among radios is read as one of them. */}
          {rows.missing.length > 0 && (
            <button className="slot__more" aria-expanded={shown} onClick={() => setShown(!shown)}>
              {tf("face.moreStarts", { n: rows.missing.length })}
            </button>
          )}
        </div>
      )}
      {/* Not pressed while nothing is on, and it says which it is rather than explaining a refusal
          afterwards: what the reader has to do is written on the thing they would press. */}
      <button className="slot__open" onClick={() => onOpen(on)} disabled={asking}>
        {asking ? t("face.openPick") : t("face.open")}
      </button>
    </div>
  );
}
