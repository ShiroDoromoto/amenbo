// Looking through the file that is open, from a panel this draws itself.
//
// The finding is `@codemirror/search`'s — its query, its cursor, its marks on the matches, and its
// keys. What is ours is the row of controls, for three things its own panel does that a panel in
// this column cannot (`AMB-T-4916` met all three on a build with the stock one in it).
//
// 1. **Its field is a bare `<input>` with `commit` on `keyup`**, and `keyup` fires all the way
//    through a conversion. A Japanese word typed into it was searched for at each step on the way
//    to itself — four of them for a four-syllable word, most of them half a syllable — and the
//    selection was carried somewhere else by every one. A field of our own can wait for the
//    conversion to be settled.
// 2. **Its colours are its own.** `.cm-panels` is white whatever is around it, so the dark theme
//    got a white band across the file.
// 3. **It wraps to four rows in this column**, pushing the file down by that much: six controls
//    spelled out as words do not fit a panel as narrow as this one.
//
// **Escape is the fourth, and it is not the panel's own doing.** None of the search keys declares
// that it stops the press, so one Escape closed the panel *and* folded the column behind it — the
// file column takes Escape as "back one layer" (`./FilesPanel`). What kept it from doing both while
// the field had focus was that the panel's DOM was gone before React could read the event, which is
// an accident rather than a rule. So the press is stopped here, in both places it can be made from:
// on the editor by a binding that declares it, and in the panel by the panel.

import {
  SearchQuery,
  closeSearchPanel,
  findNext,
  findPrevious,
  getSearchQuery,
  search,
  searchKeymap,
  searchPanelOpen,
  setSearchQuery,
} from "@codemirror/search";
import { Prec, type Extension } from "@codemirror/state";
import { EditorView, keymap, runScopeHandlers, type Panel } from "@codemirror/view";
import { composing } from "../core/keys";

/** What the editor needs to be able to find things in: the state, the panel and the keys. */
export function finding(): Extension[] {
  return [
    search({ top: true, createPanel: drawn }),
    // Ahead of the search keys' own Escape, which closes the panel and lets the press carry on up
    // to the column. This one says it is taken — and says so only when there was a panel to close,
    // so an Escape pressed with no panel open still reaches the column it belongs to.
    Prec.high(keymap.of([{ key: "Escape", run: shut, stopPropagation: true }])),
    keymap.of(searchKeymap),
  ];
}

/** Close the panel, or leave the press to whoever else wants it. */
function shut(view: EditorView): boolean {
  if (!searchPanelOpen(view.state)) return false;
  closeSearchPanel(view);
  return true;
}

/** The three switches, in the order they are drawn. The mark is what the editors people come from
 *  draw for each: two letters for the letters themselves, a pattern's own two characters, and the
 *  word a boundary is drawn around. A word in the mark would be a word in nineteen languages, and
 *  this row has no room for one. */
const SWITCHES = [
  { of: "caseSensitive", mark: "Aa", says: "Match case" },
  { of: "regexp", mark: ".*", says: "Regular expression" },
  { of: "wholeWord", mark: "ab", says: "Whole word" },
] as const;

/** One panel, for one editor. */
function drawn(view: EditorView): Panel {
  const dom = document.createElement("div");
  dom.className = "cmfind";
  // It is a row of controls about the file, not part of the file: a reader tabbing through the
  // column should meet it as one thing.
  dom.setAttribute("role", "search");

  const field = document.createElement("input");
  field.className = "cmfind__field";
  // What `openSearchPanel` looks for when it is pressed a second time: the field to put the focus
  // back in, and to fill from whatever is selected.
  field.setAttribute("main-field", "true");
  field.setAttribute("aria-label", "Find");
  field.placeholder = "Find";
  field.spellcheck = false;
  field.autocapitalize = "off";
  field.autocomplete = "off";

  const switches = SWITCHES.map(({ of, mark, says }) => {
    const button = document.createElement("button");
    button.className = "cmfind__switch";
    button.type = "button";
    button.textContent = mark;
    button.title = says;
    button.setAttribute("aria-label", says);
    button.setAttribute("aria-pressed", "false");
    button.addEventListener("click", () => {
      button.setAttribute("aria-pressed", button.getAttribute("aria-pressed") === "true" ? "false" : "true");
      commit();
      view.focus();
    });
    return { of, button };
  });

  const step = (says: string, mark: string, go: (view: EditorView) => boolean) => {
    const button = document.createElement("button");
    button.className = "cmfind__step";
    button.type = "button";
    button.textContent = mark;
    button.title = says;
    button.setAttribute("aria-label", says);
    button.addEventListener("click", () => go(view));
    return button;
  };

  const close = document.createElement("button");
  close.className = "cmfind__close";
  close.type = "button";
  close.textContent = "✕";
  close.title = "Close";
  close.setAttribute("aria-label", "Close");
  close.addEventListener("click", () => closeSearchPanel(view));

  dom.append(
    field,
    step("Previous match", "↑", findPrevious),
    step("Next match", "↓", findNext),
    ...switches.map(({ button }) => button),
    close,
  );

  // What the panel last told the editor to look for. A query it has not moved off is not dispatched
  // again: every dispatch re-runs the search over the whole file.
  let asked = getSearchQuery(view.state);

  const commit = () => {
    const query = new SearchQuery({
      search: field.value,
      caseSensitive: pressed("caseSensitive"),
      regexp: pressed("regexp"),
      wholeWord: pressed("wholeWord"),
    });
    if (query.eq(asked)) return;
    asked = query;
    view.dispatch({ effects: setSearchQuery.of(query) });
  };

  const pressed = (of: string) =>
    switches.find((one) => one.of === of)?.button.getAttribute("aria-pressed") === "true";

  const show = (query: SearchQuery) => {
    asked = query;
    field.value = query.search;
    for (const { of, button } of switches) {
      button.setAttribute("aria-pressed", String(query[of]));
    }
  };

  // **The reading is left alone until the input method has settled it.** Between the first key and
  // the conversion being accepted, what is in the field is a guess the reader has not chosen, and
  // searching for it moves the selection away from where they were looking.
  let writing = false;
  field.addEventListener("compositionstart", () => { writing = true; });
  field.addEventListener("compositionend", () => { writing = false; commit(); });
  field.addEventListener("input", () => { if (!writing) commit(); });

  dom.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      // **The Escape that takes a conversion back is the input method's.** On macOS it arrives with
      // `isComposing` already 0 and keyCode still 229, so the only thing that tells it from a real
      // Escape is the code. Passed on, it would close the panel the reader is still typing in; it
      // is not prevented either, because the input method is the one that has to see it.
      if (composing(event)) {
        event.stopPropagation();
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      closeSearchPanel(view);
      return;
    }
    // **Every other press made mid-conversion is the input method's too.** The Return that settles a
    // reading is the same Return that walks to the next match, and it arrives before the word it
    // settled — so taken here it would walk the file on the reading before this one.
    if (composing(event)) return;
    if (runScopeHandlers(view, event, "search-panel")) {
      event.preventDefault();
      return;
    }
    if (event.key === "Enter" && event.target === field) {
      event.preventDefault();
      (event.shiftKey ? findPrevious : findNext)(view);
    }
  });

  return {
    dom,
    top: true,
    mount() {
      show(getSearchQuery(view.state));
      field.select();
    },
    update(update) {
      for (const one of update.transactions) {
        for (const effect of one.effects) {
          if (effect.is(setSearchQuery) && !effect.value.eq(asked)) show(effect.value);
        }
      }
    },
  };
}
