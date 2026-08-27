// The file panel's editor: what a file's text is drawn in, where it is not drawn as Markdown.
//
// **It is an editor before it can save**, and that order is deliberate. What the panel showed was a
// `<pre>`: no line numbers, no caret, nothing to select a line by. An editor answers all of that
// while reading, and the door that writes lands on top of it rather than beside it (`AMB-D-769`).
//
// **A file it could never save is read-only from the start** — one cut at the read cap, and one
// whose bytes and text do not round-trip, which is a file in an encoding nothing writes back
// (`AMB-D-773`). Telling somebody after they have typed is worse than not letting them.
//
// The editor arrives asynchronously, so the raw text is drawn until it does. Nothing is lost if it
// never does: a `<pre>` is what the panel showed before, and it is what a failure falls back to.

import { useEffect, useRef, useState } from "react";
import { mountEditor, type Mounted } from "./editorLoad";

/** One file's text, in an editor once one has loaded. */
export function FileEditor({ text, editable, name }: { text: string; editable: boolean; name: string }) {
  const host = useRef<HTMLDivElement | null>(null);
  const mounted = useRef<Mounted | null>(null);
  const [drawn, setDrawn] = useState(false);

  useEffect(() => {
    // A file that changed which file it is takes a new editor: read-only-ness and the language its
    // colour comes from are both fixed at mount, and the text is replaced only where the editor
    // already stands.
    if (mounted.current !== null) {
      mounted.current.show(text);
      return;
    }
    let alive = true;
    const parent = host.current;
    if (parent === null) return;
    void mountEditor(parent, text, editable, name).then(
      (one) => {
        if (!alive) {
          one.close();
          return;
        }
        mounted.current = one;
        setDrawn(true);
      },
      // An editor that never arrives leaves the text where it already is. Nothing is lost: what is
      // drawn below is exactly what this panel showed before there was an editor at all.
      () => {},
    );
    return () => { alive = false; };
  }, [text, editable, name]);

  // Taking the editor down is its own effect, run when this leaves the page rather than whenever
  // the text changes — the one above replaces the text in the editor that already stands.
  useEffect(() => () => {
    mounted.current?.close();
    mounted.current = null;
  }, []);

  return (
    <>
      <div className="files__editor" ref={host} />
      {!drawn && <pre className="files__text">{text}</pre>}
    </>
  );
}
