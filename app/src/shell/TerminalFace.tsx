import { useEffect, useRef, useState } from "react";
import { mountTerminal } from "../talk/terminal";
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
 */
export function TerminalFace({ onSplitOut, note }: { onSplitOut: () => void; note: string | null }) {
  const paneRef = useRef<HTMLDivElement>(null);
  // What the pane has to say for itself when it is not simply a terminal: the host's refusal if one
  // could not be started, and the fact of the program having exited, which the screen cannot show
  // on its own — what a finished shell leaves behind looks exactly like one waiting to be typed at.
  const [failed, setFailed] = useState<string | null>(null);
  const [ended, setEnded] = useState(false);

  useEffect(() => {
    const host = paneRef.current;
    if (!host) return;
    let taken = false;
    let pane: { detach: () => void } | null = null;
    setEnded(false);
    void mountTerminal(host, () => setEnded(true))
      .then((mounted) => {
        // Taken away while the host was still answering. Detaching leaves the terminal running for
        // whatever draws it next, which is exactly what a pane that never got shown should do.
        if (taken) mounted.detach();
        else pane = mounted;
      })
      .catch((e: unknown) => setFailed(e instanceof Error ? e.message : String(e)));
    return () => {
      taken = true;
      pane?.detach();
    };
  }, []);

  return (
    <div className="termface">
      <div className="termface__bar">
        <button className="termface__action" onClick={onSplitOut}>{t("face.splitOut")}</button>
        {ended && <span className="termface__note">{t("face.ended")}</span>}
        {note !== null && <span className="termface__note">{note}</span>}
      </div>
      {failed === null
        ? <div className="termface__pane" ref={paneRef} />
        : <div className="termface__failed">{failed}</div>}
    </div>
  );
}
