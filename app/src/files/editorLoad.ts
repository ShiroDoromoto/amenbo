// The IO boundary for the file panel's editor. CodeMirror is a heavy dependency — about 100 KB
// gzipped — so it is pulled in through a dynamic import and never reaches the bundle the window
// starts from: opening a file is what fetches it, the same shape mermaid already takes
// (`../components/mermaidRender`). Measured, the eager set grows by 858 bytes (`AMB-T-3737`).
//
// It lives in a module of its own so the panel's tests can stand in for it, rather than loading an
// editor whose layout does not run under jsdom.

import type { Extension } from "@codemirror/state";
import { takesPastedFiles, takesPastedImages, writesPastedImage } from "../core/clipFiles";
import { langFor, type LangId } from "./grammars";

/**
 * The longest line an editor still wraps.
 *
 * Wrapping is what makes a long line expensive: the engine has to lay the whole line out to know
 * where to break it, and that cost climbs faster than the line does. Measured on WKWebView — the
 * engine the window runs on — a 1 MB file on one line takes **5.0 s** to lay out wrapped and
 * **2 ms** unwrapped. The two costs cross at about 20,000 characters a line; under that, wrapping
 * is the cheaper of the two, which is why this is a cap and not a switch.
 *
 * Files this hits are minified js, JSON squashed onto one line, and long base64 — none of which a
 * person reads by the wrapped line anyway. They scroll sideways instead.
 */
const WRAP_CAP = 20_000;

/**
 * Whether `text` is short-lined enough to wrap.
 *
 * Answered by the longest line, not the total size: `AMB-T-3737` measured that the number of lines
 * does not matter at all — a 5 MB file costs what a 10 KB one does, because only the lines on
 * screen are ever drawn.
 */
export function wrappable(text: string): boolean {
  let at = 0;
  for (;;) {
    const end = text.indexOf("\n", at);
    if (end < 0) return text.length - at <= WRAP_CAP;
    if (end - at > WRAP_CAP) return false;
    at = end + 1;
  }
}

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
 * `name` is the file's own name, which is the only thing that says what language it is written in
 * (`./grammars`). A name nothing here reads simply arrives uncoloured.
 *
 * `onEdit` is told every time the reader changes the text, which is how the panel above knows there
 * is something to save. It is not told when {@link Mounted.show} replaces the text: that is the
 * panel moving to another file, not a person typing.
 */
export async function mountEditor(
  parent: HTMLElement,
  text: string,
  editable: boolean,
  name: string,
  onEdit?: () => void,
): Promise<Mounted> {
  // Fetched beside the editor, not after it: a file that appears uncoloured and then repaints reads
  // as a glitch, where one that was never coloured reads as a plain file.
  const [{ EditorState, Compartment }, view, commands, manners] = await Promise.all([
    import("@codemirror/state"),
    import("@codemirror/view"),
    import("@codemirror/commands"),
    mannersFor(langFor(name)),
  ]);
  const { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter } = view;
  const { history, defaultKeymap, historyKeymap } = commands;

  // Wrapping is decided by the text, and the text changes under a panel that moved to another file
  // without unmounting — so it goes in a compartment rather than being fixed at mount.
  const wrapping = new Compartment();
  const wrap = (of: string) => (wrappable(of) ? EditorView.lineWrapping : []);

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
          // The line the caret is on, in both halves of the row. CodeMirror's own is a fixed pale
          // blue, which is a colour and not a token: under the dark theme it puts light text on a
          // light band and the one line a reader is looking at is the one they cannot read
          // (`AMB-T-3786` met it). The token flips with the theme, so the band is a shade of the
          // surface either way rather than a colour of its own.
          ".cm-activeLine": { backgroundColor: "var(--c-sunken)" },
          ".cm-activeLineGutter": {
            backgroundColor: "var(--c-sunken)",
            // And its number is the one number worth reading: the rest of the gutter is muted so
            // that the text beside it reads first, which leaves nothing to tell this one apart.
            color: "var(--c-text)",
          },
          // The fold arrows are drawn faintly and come up on hover: they sit beside every line of
          // the file and would otherwise be a second column of marks competing with the numbers.
          ".cm-foldGutter .cm-gutterElement": { color: "var(--c-text-faint)", cursor: "pointer" },
          ".cm-foldGutter .cm-gutterElement:hover": { color: "var(--c-text)" },
          // What stands in for the lines that were folded away. It is a thing to click, so it is
          // drawn as one rather than as text that happens to be there.
          ".cm-foldPlaceholder": {
            background: "var(--c-sunken)",
            border: "1px solid var(--c-edge)",
            borderRadius: "var(--r-sm)",
            color: "var(--c-text-muted)",
            padding: "0 var(--s-2)",
          },
          "&.cm-focused": { outline: "none" },
        }),
        wrapping.of(wrap(text)),
        ...manners,
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

  // **A paste carrying files puts the paths in as words** (`AMB-T-4400`). A reader who copied a row
  // in the panel beside this one has the file itself on the clipboard, and the editor's own paste —
  // which reads the words off the event — is handed nothing at all for it, so the press lands and
  // the file's text is unchanged. What the host reads back goes in instead.
  //
  // **Bare, one to a line, which is how they were copied** (`AMB-D-832`). Quoting is the pane's,
  // because a pane is a shell and a name with a space in it is two words there; a path pasted into
  // a file's text is a path, and a quote around it would be a character the reader has to delete.
  //
  // A file this panel cannot save takes no paste at all: the editor refuses every other way of
  // typing into it, and a door of our own that wrote where those will not would be the one way in.
  //
  // **A screenshot is written down first and its path put in, the same as a copied file's**
  // (`AMB-D-854`). An image on the clipboard is bytes and no file, so there is nothing to name until
  // it has been put somewhere; the host puts it in a directory belonging to the run, because an
  // editor is no pane and has no session of its own (`../core/clipFiles`). The path goes in bare
  // like every other — what the file being edited makes of it is the file's business, and an editor
  // that wrote markdown into a `.rs` would be wrong more often than right.
  //
  // ⚠ **The picture does not outlast the app**, so what is saved is a path that will stop reaching
  // it. That is the trade: it is for handing a screenshot to something running now.
  //
  // **On Linux the image comes in by the press rather than by the paste** — WebKitGTK hands a paste
  // nothing at all, so the clipboard is asked when `Ctrl+V` is pressed (`../core/clipFiles`). The
  // same writing and the same insert; on the other two machines the press listener is never put on.
  const insert = (text: string) => {
    if (!text) return;
    editor.dispatch(editor.state.replaceSelection(text), {
      scrollIntoView: true,
      userEvent: "input.paste",
    });
  };
  const writeImage = (bytes: Uint8Array, mime: string) => writesPastedImage(bytes, mime, null);
  const stopPaste = editable
    ? takesPastedFiles(
        parent,
        (paths, words) => insert(paths.length > 0 ? paths.join("\n") : words),
        writeImage,
      )
    : () => {};
  const stopImagePress = editable
    ? takesPastedImages(parent, writeImage, (paths) => insert(paths.join("\n")), "textbox")
    : () => {};

  return {
    show(next: string) {
      editor.dispatch({
        changes: { from: 0, to: editor.state.doc.length, insert: next },
        effects: wrapping.reconfigure(wrap(next)),
        userEvent: "panel",
      });
    },
    text() {
      return editor.state.doc.toString();
    },
    close() {
      stopPaste();
      stopImagePress();
      editor.destroy();
    },
  };
}
