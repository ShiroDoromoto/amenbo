import { useEffect, useMemo, useState } from "react";
import type { WakeDto } from "../bindings/bindings";
import { invoke } from "../core/ipc";
import { SHELL } from "../talk/terminal";
import { t } from "../core/i18n";
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
 * the button is every agent this machine can start plus the plain shell, with the project's own
 * answer already on — so the common press is the same one press it always was, and choosing
 * something else costs a press and no dialog (`AMB-T-3667`). What is chosen here is this pane's: the
 * project settles its answer the first time and changes it on its own settings, never by somebody
 * reaching for a different tool for one turn (`../talk/agent`).
 *
 * `onOpen` is what this is for: opening a pane in this project, with the agent that was chosen —
 * {@link SHELL} for a prompt with nothing started at it.
 */
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
  onOpen: (agent: string) => void;
}) {
  const folderKey = folders.join("\n");
  // What this machine can start, and what this project has settled on. Read once per project rather
  // than per press: a probe is a login shell reading the reader's own profile (`crate::wake`), and a
  // frame that ran one on every render would pay for it on every keystroke elsewhere on the page.
  const [wake, setWake] = useState<WakeDto | null>(null);
  // Which of them the next pane opens with, once the reader has said. Null until they do, because
  // what is on before that is the project's answer and that arrives with the read.
  const [chose, setChose] = useState<string | null>(null);

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

  return (
    <div className="slot slot--empty">
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
      <button className="slot__open" onClick={() => onOpen(on)}>{t("face.open")}</button>
    </div>
  );
}
