import { useEffect, useRef, useState } from "react";
import { mountAgentFrame } from "../talk/agent";
import { nameFrame, ONLY_FRAME } from "../talk/frames";
import { mountPlate } from "../talk/plate";
import { currentLang, t } from "../core/i18n";

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
 * What runs in the pane is not settled here. The frame put up inside it asks the host which agent this
 * folder starts with, and draws the offer or the install notice where that has no single answer
 * (`app/src/talk/agent.ts`) — which is also where a refusal to start one is shown, so nothing here
 * holds a failure of its own.
 *
 * `note` is what the shell has to say about this face that the pane cannot — a window that could not
 * be split out, which is the press of the button here having come to nothing.
 *
 * `onWaiting` is the one thing this face says back to the shell: whether the pane in it is waiting on
 * a person. Behind the other face the label above the pane cannot be seen at all, so the shell puts a
 * badge on the face switch instead (`./terminalBadge`) — and it is told the fact, not what to do
 * about it. The answer is the plate's, which is what holds the session (`../talk/plate`).
 */
export function TerminalFace({
  onSplitOut,
  note,
  onWaiting,
}: {
  onSplitOut: () => void;
  note: string | null;
  onWaiting: (waiting: boolean) => void;
}) {
  const paneRef = useRef<HTMLDivElement>(null);
  const labelRef = useRef<HTMLDivElement>(null);
  // The fact of the program having exited, which the screen cannot show on its own — what a finished
  // shell leaves behind looks exactly like one waiting to be typed at.
  const [ended, setEnded] = useState(false);
  // Held in a ref because the pane is put up once, in an effect that runs once: reading the prop
  // through this is what lets the shell pass a fresh callback without the terminal coming down.
  const tell = useRef(onWaiting);
  tell.current = onWaiting;

  useEffect(() => {
    const host = paneRef.current;
    const label = labelRef.current;
    if (!host || !label) return;
    let taken = false;
    let detach: (() => void) | null = null;
    setEnded(false);
    // The line above the pane. It holds what is known about the session running there for as long as
    // it runs, which is the same line the split-out window draws (`app/src/talk/plate.ts`).
    const plate = mountPlate(label, currentLang, (waiting) => tell.current(waiting));
    void mountAgentFrame(host, currentLang(), {
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
    }, "termface__pane")
      .then((take) => {
        // Taken away while the host was still answering. Detaching leaves the terminal running for
        // whatever draws it next, which is exactly what a pane that never got shown should do.
        if (taken) take();
        else detach = take;
      })
      .catch(() => {});
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
      <div className="termface__face" ref={paneRef} />
    </div>
  );
}
