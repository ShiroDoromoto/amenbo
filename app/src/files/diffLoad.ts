// The IO boundary for the panel's difference screen, the same shape the editor's is (`./editorLoad`):
// `@codemirror/merge` is pulled in through a dynamic import and never reaches the bundle the window
// starts from. Asking to see the difference is what fetches it, and a reader who never meets the
// case never pays for it.
//
// It lives in a module of its own so the panel's tests can stand in for it, rather than laying out
// two editors under jsdom, which cannot measure.

import { wrappable } from "./editorLoad";

/** One mounted comparison, and the way to take it down again. */
export type Compared = { close(): void };

/**
 * Draw the two texts into `parent`, side by side, with what differs marked.
 *
 * `theirs` is what is on the disk now and `mine` is what the reader has typed. Neither can be typed
 * into and there is no way here to take one line from one side and one from the other: which of the
 * two texts stands is the question this screen is opened to answer, and the answers are beside it
 * (`AMB-D-863`). A reader who wants some of each takes one side and edits it afterwards.
 *
 * **Nothing here is coloured by language.** What this screen is read for is what differs, and a
 * second set of colours over the marks that say so is the one thing that would make them harder to
 * find — where the editor, which is read for the file itself, is coloured.
 */
export async function mountDiff(
  parent: HTMLElement,
  theirs: string,
  mine: string,
): Promise<Compared> {
  const [{ EditorState }, view, { MergeView }] = await Promise.all([
    import("@codemirror/state"),
    import("@codemirror/view"),
    import("@codemirror/merge"),
  ]);
  const { EditorView, lineNumbers } = view;

  // Which theme the window resolved to (`../core/theme` keeps it on `<html>`). The marks over what
  // changed come in a light and a dark pair, and CodeMirror picks between them by this facet rather
  // than by looking at the page — left unsaid, the dark window gets the light one.
  const dark = document.documentElement.dataset.theme === "dark";

  // The panel's own type and colours, so the comparison reads as part of the window rather than as
  // something embedded. Everything here is a token the rest of the panel already uses; what marks
  // the changes is the package's own, which is drawn in alpha over whatever ground it lands on.
  const theme = EditorView.theme({
    "&": { color: "var(--c-text)", backgroundColor: "transparent", fontSize: "var(--fs-xs)" },
    ".cm-content": { fontFamily: "var(--font-mono)" },
    ".cm-gutters": {
      backgroundColor: "transparent",
      color: "var(--c-text-muted)",
      border: "none",
    },
    "&.cm-focused": { outline: "none" },
  });

  const side = (doc: string) => ({
    doc,
    extensions: [
      lineNumbers(),
      // Read-only in both senses: the text cannot be changed, and the caret is still there to
      // select with — this is a screen to read off and to copy out of.
      EditorState.readOnly.of(true),
      EditorView.editable.of(false),
      wrappable(doc) ? EditorView.lineWrapping : [],
      EditorView.darkTheme.of(dark),
      theme,
    ],
  });

  const merged = new MergeView({
    parent,
    // The disk on the left and the reader's own on the right, which is the order the answers under
    // it are in and the order the two are named in above it.
    a: side(theirs),
    b: side(mine),
    // **No revert controls.** They are the per-line choice this screen deliberately does not offer.
    gutter: true,
    // A file is mostly the part nobody touched, and a screen opened to show what differs should not
    // be scrolled through to find it.
    collapseUnchanged: {},
  });

  return { close: () => merged.destroy() };
}
