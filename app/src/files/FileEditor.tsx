// The file panel's editor: what a file's text is drawn in, where it is not drawn as Markdown.
//
// **It is an editor before it can save**, and that order was deliberate. What the panel showed was
// a `<pre>`: no line numbers, no caret, nothing to select a line by. An editor answers all of that
// while reading, and the door that writes landed on top of it rather than beside it (`AMB-D-769`).
//
// **A file it could never save is read-only from the start** — one cut at the read cap, and one
// whose bytes and text do not round-trip, which is a file in an encoding nothing writes back
// (`AMB-D-773`). Telling somebody after they have typed is worse than not letting them.
//
// **The text stays in the editor.** The panel above holds what was read and nothing more: pulling
// every keystroke up into React state would replace the editor's own document on the way back down
// and take the caret with it. So the panel is told *that* something was typed, and asks for the
// text at the one moment it needs it — when it saves.
//
// The editor arrives asynchronously, so the raw text is drawn until it does. Nothing is lost if it
// never does: a `<pre>` is what the panel showed before, and it is what a failure falls back to —
// which is also why saving is offered only once the editor is up.

import { useEffect, useRef, useState } from "react";
import { mountEditor, type Mounted } from "./editorLoad";

/** One file's text, in an editor once one has loaded. */
export function FileEditor({ text, editable, onEdit, hold }: {
  text: string;
  editable: boolean;
  /** Told when the reader changes the text — not when this component replaces it. */
  onEdit?: () => void;
  /** Handed the way to read the text back, and handed nothing when the editor goes away. */
  hold?: (read: (() => string) | null) => void;
}) {
  const host = useRef<HTMLDivElement | null>(null);
  const mounted = useRef<Mounted | null>(null);
  const [drawn, setDrawn] = useState(false);
  // The callbacks as they are now, so the effect below does not take them as reasons to build
  // another editor: a panel that re-renders on every keystroke would otherwise rebuild it on every
  // keystroke.
  const told = useRef({ onEdit, hold });
  told.current = { onEdit, hold };

  useEffect(() => {
    // A file that changed which file it is takes a new editor: read-only-ness is fixed at mount,
    // and the text is replaced only where the editor already stands.
    if (mounted.current !== null) {
      mounted.current.show(text);
      return;
    }
    let alive = true;
    const parent = host.current;
    if (parent === null) return;
    void mountEditor(parent, text, editable, () => told.current.onEdit?.()).then(
      (one) => {
        if (!alive) {
          one.close();
          return;
        }
        mounted.current = one;
        told.current.hold?.(() => one.text());
        setDrawn(true);
      },
      // An editor that never arrives leaves the text where it already is. Nothing is lost: what is
      // drawn below is exactly what this panel showed before there was an editor at all.
      () => {},
    );
    return () => { alive = false; };
  }, [text, editable]);

  // Taking the editor down is its own effect, run when this leaves the page rather than whenever
  // the text changes — the one above replaces the text in the editor that already stands.
  useEffect(() => () => {
    mounted.current?.close();
    mounted.current = null;
    told.current.hold?.(null);
  }, []);

  return (
    <>
      <div className="files__editor" ref={host} />
      {!drawn && <pre className="files__text">{text}</pre>}
    </>
  );
}
