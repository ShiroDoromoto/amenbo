// @vitest-environment jsdom
// The four things the find panel is drawn by hand for (`AMB-T-4916`): the reading is left alone
// until the input method has settled it, the Escape that takes a conversion back is the input
// method's, a real Escape closes the panel and goes no further, and the switches are what the query
// is built from.
//
// The editor itself is built here rather than stood in for. jsdom runs no layout, so nothing about
// where the matches are drawn can be asked — but the panel is a row of ordinary elements listening
// for ordinary events, and that is all of what is checked.
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { getSearchQuery, openSearchPanel, searchPanelOpen } from "@codemirror/search";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { finding } from "./editorFind";

// Every editor a test built. Taken down after it, because moving to a match asks the editor to
// scroll and the measuring that follows is one jsdom cannot do — left standing, it runs after the
// test that made it has ended and fails the file from outside any of them.
const built: EditorView[] = [];

/** One editor over that text, held so it can be taken down again. */
function editor(doc: string): EditorView {
  const parent = document.createElement("div");
  document.body.append(parent);
  const view = new EditorView({
    parent,
    state: EditorState.create({ doc, extensions: [...finding()] }),
  });
  built.push(view);
  return view;
}

/** An editor with the panel open over it, and the pieces a test presses. */
function opened(doc = "one needle two\nNEEDLE again\n") {
  const view = editor(doc);
  const parent = view.dom.parentElement as HTMLElement;
  openSearchPanel(view);
  const panel = parent.querySelector(".cmfind") as HTMLElement;
  const field = panel.querySelector(".cmfind__field") as HTMLInputElement;
  return { view, panel, field };
}

/** One switch, by what it says it is for. */
function toggle(panel: HTMLElement, says: string): HTMLButtonElement {
  const found = panel.querySelector(`.cmfind__switch[aria-label="${says}"]`);
  if (found === null) throw new Error(`no switch for ${says}`);
  return found as HTMLButtonElement;
}

/** Type into the field the way a keyboard does — the value first, then the word that it moved. */
function type(field: HTMLInputElement, text: string) {
  field.value = text;
  field.dispatchEvent(new Event("input", { bubbles: true }));
}

beforeEach(() => {
  document.body.replaceChildren();
});

afterEach(() => {
  for (const view of built.splice(0)) view.destroy();
});

describe("the panel the editor finds things from", () => {
  it("opens with a field to type in", () => {
    const { view, field } = opened();
    expect(searchPanelOpen(view.state)).toBe(true);
    expect(field).not.toBeNull();
  });

  it("looks for what was typed", () => {
    const { view, field } = opened();
    type(field, "needle");
    expect(getSearchQuery(view.state).search).toBe("needle");
  });

  it("leaves the reading alone until the input method has settled it", () => {
    const { view, field } = opened();
    field.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true }));
    // The steps a reader passes through on the way to the word below. None of them is a word they
    // chose, and the last of them is only the reading of it.
    for (const reading of ["け", "けん", "けんさ", "けんさく"]) type(field, reading);
    expect(getSearchQuery(view.state).search).toBe("");

    field.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true }));
    expect(getSearchQuery(view.state).search).toBe("けんさく");
  });

  it("builds the query out of the switches", () => {
    const { view, panel, field } = opened();
    type(field, "needle");
    expect(getSearchQuery(view.state).caseSensitive).toBe(false);

    toggle(panel, "Match case").click();
    expect(getSearchQuery(view.state).caseSensitive).toBe(true);
    expect(toggle(panel, "Match case").getAttribute("aria-pressed")).toBe("true");

    toggle(panel, "Regular expression").click();
    toggle(panel, "Whole word").click();
    const query = getSearchQuery(view.state);
    expect([query.regexp, query.wholeWord]).toEqual([true, true]);

    // And off again, which is the same press.
    toggle(panel, "Match case").click();
    expect(getSearchQuery(view.state).caseSensitive).toBe(false);
  });

  it("closes on Escape and lets nothing above it hear the press", () => {
    const { view, panel, field } = opened();
    let heard = 0;
    document.body.addEventListener("keydown", () => { heard += 1; });

    field.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
    expect(searchPanelOpen(view.state)).toBe(false);
    expect(heard).toBe(0);
    expect(panel.isConnected).toBe(false);
  });

  it("hands the input method back its own Escape, and still lets nothing above hear it", () => {
    const { view, field } = opened();
    let heard = 0;
    document.body.addEventListener("keydown", () => { heard += 1; });

    // macOS sends the Escape that takes a conversion back with `isComposing` already 0 and keyCode
    // still 229, so the code is the only thing that tells the two apart.
    const press = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
    Object.defineProperty(press, "keyCode", { value: 229 });
    field.dispatchEvent(press);

    expect(searchPanelOpen(view.state)).toBe(true);
    expect(heard).toBe(0);
    // Not prevented either: the input method is the one that has to see it.
    expect(press.defaultPrevented).toBe(false);
  });

  it("leaves the Return that settles a reading to the input method", () => {
    const { view, field } = opened();
    type(field, "needle");
    const first = view.state.selection.main.from;

    const press = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
    Object.defineProperty(press, "isComposing", { value: true });
    field.dispatchEvent(press);
    expect(view.state.selection.main.from).toBe(first);

    // The same press once the reading is settled is the one that walks to the next match.
    field.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
    expect(view.state.selection.main.from).not.toBe(first);
  });

  it("closes from the editor too, and the press stops there", () => {
    const { view, panel } = opened();
    let heard = 0;
    document.body.addEventListener("keydown", () => { heard += 1; });

    view.contentDOM.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", keyCode: 27, bubbles: true, cancelable: true }),
    );
    expect(searchPanelOpen(view.state)).toBe(false);
    expect(heard).toBe(0);
    expect(panel.isConnected).toBe(false);
  });

  it("leaves an Escape with no panel open to whoever wants it", () => {
    const view = editor("one\n");
    let heard = 0;
    document.body.addEventListener("keydown", () => { heard += 1; });

    view.contentDOM.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", keyCode: 27, bubbles: true, cancelable: true }),
    );
    // The column above takes Escape as "back one layer" (`./FilesPanel`), and with no panel to close
    // there is nothing here to take it first.
    expect(heard).toBe(1);
  });
});
