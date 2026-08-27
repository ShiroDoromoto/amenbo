// The IO boundary for the file panel's editor. CodeMirror is a heavy dependency — about 100 KB
// gzipped — so it is pulled in through a dynamic import and never reaches the bundle the window
// starts from: opening a file is what fetches it, the same shape mermaid already takes
// (`../components/mermaidRender`). Measured, the eager set grows by 858 bytes (`AMB-T-3737`).
//
// It lives in a module of its own so the panel's tests can stand in for it, rather than loading an
// editor whose layout does not run under jsdom.

import type { Extension } from "@codemirror/state";
import { langFor, type LangId } from "./grammars";

// What the editor knows about the file is a second dynamic import behind the editor's own, and one
// that fails to arrive costs colour and manners, not the editor — a plain editor over plain text is
// what the panel had before either existed.
async function mannersFor(lang: LangId | null): Promise<Extension[]> {
  try {
    const { language } = await import("./language");
    return [await language(lang)];
  } catch {
    return [];
  }
}

/** One mounted editor: the element it drew into, and the way to take it down again. */
export type Mounted = {
  /** Replace the text being shown, for a panel that moved to another file without unmounting. */
  show(text: string): void;
  /** Take the editor off the page. */
  close(): void;
};

/**
 * Draw `text` into `parent`, as an editor.
 *
 * `editable` is false for a file this panel will not be able to save: one cut at the read cap, and
 * one whose bytes and text do not round-trip (`FolderFileDto.clean`). Nothing here can save yet
 * either way — the door that writes is being built — but a file that will never be savable is a
 * file to say so about now rather than after somebody has typed into it.
 *
 * `name` is the file's own name, which is the only thing that says what language it is written in
 * (`./grammars`). A name nothing here reads simply arrives uncoloured.
 */
export async function mountEditor(
  parent: HTMLElement,
  text: string,
  editable: boolean,
  name: string,
): Promise<Mounted> {
  // Fetched beside the editor, not after it: a file that appears uncoloured and then repaints reads
  // as a glitch, where one that was never coloured reads as a plain file.
  const [{ EditorState }, view, commands, manners] = await Promise.all([
    import("@codemirror/state"),
    import("@codemirror/view"),
    import("@codemirror/commands"),
    mannersFor(langFor(name)),
  ]);
  const { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter } = view;
  const { history, defaultKeymap, historyKeymap } = commands;

  const editor = new EditorView({
    parent,
    state: EditorState.create({
      doc: text,
      extensions: [
        lineNumbers(),
        highlightActiveLine(),
        highlightActiveLineGutter(),
        history(),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        // The panel's own type and colours, so an editor in it reads as part of it rather than as
        // something embedded. Everything here is a token the rest of the panel already uses.
        EditorView.theme({
          "&": {
            color: "var(--c-text)",
            backgroundColor: "transparent",
            fontSize: "var(--fs-xs)",
          },
          ".cm-content": { fontFamily: "var(--font-mono)" },
          ".cm-gutters": {
            backgroundColor: "transparent",
            color: "var(--c-text-muted)",
            border: "none",
          },
          // The fold arrows are drawn faintly and come up on hover: they sit beside every line of
          // the file and would otherwise be a second column of marks competing with the numbers.
          ".cm-foldGutter .cm-gutterElement": { color: "var(--c-text-faint)", cursor: "pointer" },
          ".cm-foldGutter .cm-gutterElement:hover": { color: "var(--c-text)" },
          // What stands in for the lines that were folded away. It is a thing to click, so it is
          // drawn as one rather than as text that happens to be there.
          ".cm-foldPlaceholder": {
            background: "var(--c-surface-sunken)",
            border: "1px solid var(--c-border)",
            borderRadius: "var(--r-sm)",
            color: "var(--c-text-muted)",
            padding: "0 var(--s-2)",
          },
          "&.cm-focused": { outline: "none" },
        }),
        EditorView.lineWrapping,
        ...manners,
        EditorState.readOnly.of(!editable),
        // A read-only editor still takes focus and a caret, which is what makes it selectable and
        // navigable by keyboard; what it refuses is changing the text.
        EditorView.editable.of(editable),
      ],
    }),
  });

  return {
    show(next: string) {
      editor.dispatch({
        changes: { from: 0, to: editor.state.doc.length, insert: next },
      });
    },
    close() {
      editor.destroy();
    },
  };
}
