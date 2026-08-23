import { useEffect, useRef, useState } from "react";
import { mountTerminal } from "../talk/terminal";
import { nameFrame, ONLY_FRAME } from "../talk/frames";
import { mountPlate } from "../talk/plate";
import { FilesPanel } from "../files/FilesPanel";
import { t } from "../core/i18n";

/**
 * The terminal, drawn inside the board's window — the second face of the one window (`AMB-D-753`).
 *
 * It is put up once and then left alone. Switching back to the ledger hides it with CSS rather than
 * taking it down, which is the one thing this component exists to guarantee: unmounting would take
 * the emulator with it, and a terminal whose pane went away is an agent nobody can get back to. The
 * caller therefore keeps this rendered for as long as the window is the terminal's home, and hides
 * it by hiding its own container.
 *
 * When the window *stops* being the terminal's home — the user splits it out, or a language change
 * rebuilds the interface — this does come down, and the session does not: the pane detaches, and
 * whatever draws next adopts what is still running (`app/src/talk/terminal.ts`).
 *
 * `note` is what the shell has to say about this face that the pane cannot — a window that could not
 * be split out, which is the press of the button here having come to nothing.
 *
 * Beside the pane is the file face (`app/src/files/FilesPanel.tsx`), which is rooted at the
 * project's folder rather than at this pane's session — so it does not move when the pane does
 * (`AMB-T-3602`). It is why this component is told which project the window is on, and how to leave
 * this face for the ledger, neither of which the terminal itself has any use for.
 */
export function TerminalFace({ onSplitOut, note, projectId, onOpenLedger }: {
  onSplitOut: () => void;
  note: string | null;
  projectId?: number | null;
  onOpenLedger?: () => void;
}) {
  const paneRef = useRef<HTMLDivElement>(null);
  const labelRef = useRef<HTMLDivElement>(null);
  // What the pane has to say for itself when it is not simply a terminal: the host's refusal if one
  // could not be started, and the fact of the program having exited, which the screen cannot show on
  // its own — what a finished shell leaves behind looks exactly like one waiting to be typed at.
  const [failed, setFailed] = useState<string | null>(null);
  const [ended, setEnded] = useState(false);

  useEffect(() => {
    const host = paneRef.current;
    const label = labelRef.current;
    if (!host || !label) return;
    let taken = false;
    let detach: (() => void) | null = null;
    setEnded(false);
    // The line above the pane. It holds what is known about the session running there for as long as
    // it runs, which is the same line the split-out window draws (`app/src/talk/plate.ts`).
    const plate = mountPlate(label);
    void mountTerminal(host, {
      opened: (session, startedAt) => {
        plate.opened(session, startedAt);
      },
      said: (statement) => {
        plate.said(statement);
      },
      closed: (session) => {
        plate.closed(session);
        setEnded(true);
      },
      // The window's own title is not the pane's to say — this window is the board. The name goes to
      // the store, and what draws it is the line above the pane.
      name: (text, by) => {
        void nameFrame(ONLY_FRAME, text, by).then(plate.named).catch(() => {});
      },
    })
      .then((take) => {
        // Taken away while the host was still answering. Detaching leaves the terminal running for
        // whatever draws it next, which is exactly what a pane that never got shown should do.
        if (taken) take();
        else detach = take;
      })
      .catch((e: unknown) => setFailed(e instanceof Error ? e.message : String(e)));
    return () => {
      taken = true;
      detach?.();
      plate.stop();
    };
  }, []);

  return (
    <div className="termface">
      <div className="termface__bar">
        <button className="termface__action" onClick={onSplitOut}>{t("face.splitOut")}</button>
        {ended && <span className="termface__note">{t("face.ended")}</span>}
        {note !== null && <span className="termface__note">{note}</span>}
      </div>
      <div ref={labelRef} />
      <div className="termface__split">
        {failed === null
          ? <div className="termface__pane" ref={paneRef} />
          : <div className="termface__failed">{failed}</div>}
        <FilesPanel projectId={projectId ?? null} onOpenLedger={onOpenLedger} />
      </div>
    </div>
  );
}
