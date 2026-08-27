// The IO boundary for the file panel's editor. CodeMirror is a heavy dependency — about 100 KB
// gzipped — so it is pulled in through a dynamic import and never reaches the bundle the window
// starts from: opening a file is what fetches it, the same shape mermaid already takes
// (`../components/mermaidRender`). Measured, the eager set grows by 858 bytes (`AMB-T-3737`).
//
// It lives in a module of its own so the panel's tests can stand in for it, rather than loading an
// editor whose layout does not run under jsdom.

/** One mounted editor: the element it drew into, and the way to take it down again. */
export type Mounted = {
  /** Replace the text being shown, for a panel that moved to another file without unmounting. */
  show(text: string): void;
  /**
   * What is in the editor now — what a save writes.
   *
   * **It comes back with `\n` for every newline, whatever the file had.** CodeMirror reads
   * `\r\n`, `\r` and `\n` alike and keeps one kind, so what the file's newline was is not in this
   * text: it travels beside it, out of the read and back into the save (`crate::folder_save`).
   */
  text(): string;
  /** Take the editor off the page. */
  close(): void;
};

/**
 * Draw `text` into `parent`, as an editor.
 *
 * `editable` is false for a file this panel will not be able to save: one cut at the read cap, and
 * one whose bytes and text do not round-trip (`FolderFileDto.clean`). Such a file is said to be
 * unsavable now rather than after somebody has typed into it.
 *
 * `onEdit` is told every time the reader changes the text, which is how the panel above knows there
 * is something to save. It is not told when {@link Mounted.show} replaces the text: that is the
 * panel moving to another file, not a person typing.
 */
export async function mountEditor(
  parent: HTMLElement,
  text: string,
  editable: boolean,
  onEdit?: () => void,
): Promise<Mounted> {
  const [{ EditorState }, view, commands] = await Promise.all([
    import("@codemirror/state"),
    import("@codemirror/view"),
    import("@codemirror/commands"),
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
          "&.cm-focused": { outline: "none" },
        }),
        EditorView.lineWrapping,
        // Told apart from the panel replacing the text, which dispatches its own change: only what
        // came from the reader means there is something to save.
        EditorView.updateListener.of((update) => {
          if (update.docChanged && !update.transactions.every((one) => one.isUserEvent("panel"))) {
            onEdit?.();
          }
        }),
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
        userEvent: "panel",
      });
    },
    text() {
      return editor.state.doc.toString();
    },
    close() {
      editor.destroy();
    },
  };
}
