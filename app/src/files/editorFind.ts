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
  openSearchPanel,
  replaceAll,
  replaceNext,
  search,
  searchKeymap,
  searchPanelOpen,
  setSearchQuery,
} from "@codemirror/search";
import { Prec, StateEffect, StateField, type Extension } from "@codemirror/state";
import { EditorView, keymap, runScopeHandlers, type Panel } from "@codemirror/view";
import { t } from "../core/i18n";
import { composing } from "../core/keys";

/** What the editor needs to be able to find things in: the state, the panel and the keys. */
export function finding(): Extension[] {
  return [
    replacing,
    search({ top: true, createPanel: drawn }),
    // Ahead of the search keys' own Escape, which closes the panel and lets the press carry on up
    // to the column. This one says it is taken — and says so only when there was a panel to close,
    // so an Escape pressed with no panel open still reaches the column it belongs to.
    Prec.high(keymap.of([{ key: "Escape", run: shut, stopPropagation: true }])),
    // **Not `Mod-r`**, which is what an editor would ordinarily put this on. Windows' WebView2 takes
    // that one before the page sees it and reloads the webview: every pane, every open file and
    // everything typed into them, gone, with no keydown having arrived to say why (`AMB-T-4916`
    // measured it on a real machine). `Mod-Alt-f` reaches the page on all three.
    keymap.of([{ key: "Mod-Alt-f", run: openReplace }]),
    keymap.of(searchKeymap),
  ];
}

/** Whether the panel is showing its replacing half.
 *
 * It is state rather than a flag on the panel because the panel is built again every time it opens,
 * and a reader who went looking for the replacing half once is looking for it the next time. It
 * lives past the panel and not past the editor, which is one file.
 */
const showReplace = StateEffect.define<boolean>();
const replacing = StateField.define<boolean>({
  create: () => false,
  update(showing, tr) {
    for (const effect of tr.effects) if (effect.is(showReplace)) return effect.value;
    return showing;
  },
});

/** Open the panel with its replacing half showing, and put the caret in it. */
function openReplace(view: EditorView): boolean {
  // A file this editor will not write back has nothing to replace in it: it is read-only because it
  // was cut at the read cap, or because its bytes and its text do not round-trip (`AMB-D-773`). The
  // press is left to whoever else wants it rather than opening a half a reader cannot use.
  if (view.state.readOnly) return false;
  openSearchPanel(view);
  view.dispatch({ effects: showReplace.of(true) });
  const field = view.dom.querySelector<HTMLInputElement>(".cmfind__replace");
  field?.select();
  return true;
}

/** Open the panel, from something that is not a key.
 *
 * The column above reaches for this when a Markdown file is being drawn rather than edited: `Mod-f`
 * has nothing to press there, because there is no editor on the page at all, so the column puts one
 * up and then asks it for the panel (`./FilesPanel`).
 */
export function findIn(view: EditorView): void {
  openSearchPanel(view);
}

/** Close the panel, or leave the press to whoever else wants it. */
function shut(view: EditorView): boolean {
  if (!searchPanelOpen(view.state)) return false;
  closeSearchPanel(view);
  return true;
}

/** The three switches, in the order they are drawn. The mark is what the editors people come from
 *  draw for each: two letters for the letters themselves, a pattern's own two characters, and the
 *  word a boundary is drawn around. **The mark is the same in every language and the name is not**
 *  — a word in the mark would be a word in nineteen of them, and this row has room for none. */
const SWITCHES = [
  { of: "caseSensitive", mark: "Aa", says: "files.findCase" },
  { of: "regexp", mark: ".*", says: "files.findRegex" },
  { of: "wholeWord", mark: "ab", says: "files.findWord" },
] as const;

/** One panel, for one editor. */
function drawn(view: EditorView): Panel {
  // A file this editor will not write back is read-only, and there is nothing to replace in one. The
  // whole replacing half is left off rather than drawn and refused, which is the same answer the
  // column gives above it: a file it could never save is read-only from the first screen
  // (`./FileEditor`, `AMB-D-773`).
  const writable = !view.state.readOnly;

  const dom = document.createElement("div");
  dom.className = "cmfind";
  // It is a row of controls about the file, not part of the file: a reader tabbing through the
  // column should meet it as one thing.
  dom.setAttribute("role", "search");
  const finds = document.createElement("div");
  finds.className = "cmfind__row";
  const replaces = document.createElement("div");
  replaces.className = "cmfind__row cmfind__row--replace";

  const field = document.createElement("input");
  field.className = "cmfind__field";
  // What `openSearchPanel` looks for when it is pressed a second time: the field to put the focus
  // back in, and to fill from whatever is selected.
  field.setAttribute("main-field", "true");
  field.setAttribute("aria-label", t("files.find"));
  field.placeholder = t("files.find");
  field.spellcheck = false;
  field.autocapitalize = "off";
  field.autocomplete = "off";

  const switches = SWITCHES.map(({ of, mark, says: key }) => {
    const says = t(key);
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
  close.title = t("files.findClose");
  close.setAttribute("aria-label", t("files.findClose"));
  close.addEventListener("click", () => closeSearchPanel(view));

  // The way to the replacing half for a reader who does not know `Mod-Alt-f`. Every editor people
  // come from puts one here, and without it the half is reachable only by a key nobody was told
  // about — this one is two characters wide, which is what the row can spare.
  // **Not called "Replace".** That is the name of the press that makes one, and two things on one
  // panel under one name are two things neither a reader of the screen nor a driver of it can tell
  // apart. What this one does is show the row, and it says which way it would move.
  const more = document.createElement("button");
  more.className = "cmfind__more";
  more.type = "button";
  more.setAttribute("aria-expanded", "false");
  more.addEventListener("click", () => {
    view.dispatch({ effects: showReplace.of(more.getAttribute("aria-expanded") !== "true") });
  });

  const replace = document.createElement("input");
  replace.className = "cmfind__field cmfind__replace";
  replace.setAttribute("aria-label", t("files.searchReplaceWith"));
  replace.placeholder = t("files.searchReplaceWith");
  replace.spellcheck = false;
  replace.autocapitalize = "off";
  replace.autocomplete = "off";

  // **Replacing is about the match the editor is standing on**, which after a query has just been
  // typed is none of them: the first press walks to one and the next replaces it, the same way the
  // arrow beside the field walks. `Replace all` needs no such standing place.
  const does = (says: string, go: (view: EditorView) => boolean) => {
    const button = document.createElement("button");
    button.className = "cmfind__does";
    button.type = "button";
    button.textContent = says;
    button.addEventListener("click", () => go(view));
    return button;
  };

  if (writable) finds.append(more);
  finds.append(
    field,
    step(t("files.findPrev"), "↑", findPrevious),
    step(t("files.findNext"), "↓", findNext),
    ...switches.map(({ button }) => button),
    close,
  );
  // A spacer the width of the toggle, so the two fields start at the same place.
  const under = document.createElement("span");
  under.className = "cmfind__under";
  replaces.append(
    under,
    replace,
    does(t("files.searchReplaceGo"), replaceNext),
    does(t("files.replaceAll"), replaceAll),
  );
  dom.append(finds);
  if (writable) dom.append(replaces);

  // What the panel last told the editor to look for. A query it has not moved off is not dispatched
  // again: every dispatch re-runs the search over the whole file.
  let asked = getSearchQuery(view.state);

  const commit = () => {
    const query = new SearchQuery({
      search: field.value,
      replace: replace.value,
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
    replace.value = query.replace;
    for (const { of, button } of switches) {
      button.setAttribute("aria-pressed", String(query[of]));
    }
  };

  /** Draw the replacing half, or leave it out of the row entirely. */
  const showing = (open: boolean) => {
    if (!writable) return;
    const says = t(open ? "files.searchReplaceHide" : "files.searchReplaceShow");
    more.setAttribute("aria-expanded", String(open));
    more.setAttribute("aria-label", says);
    more.title = says;
    more.textContent = open ? "⌄" : "›";
    replaces.hidden = !open;
  };

  // **The reading is left alone until the input method has settled it.** Between the first key and
  // the conversion being accepted, what is in the field is a guess the reader has not chosen, and
  // searching for it moves the selection away from where they were looking.
  let writing = false;
  for (const one of [field, replace]) {
    one.addEventListener("compositionstart", () => { writing = true; });
    one.addEventListener("compositionend", () => { writing = false; commit(); });
    one.addEventListener("input", () => { if (!writing) commit(); });
  }

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
      return;
    }
    // In the replacing field the same press does what the field is for. Held under shift it does
    // all of them, which is the pair the two buttons beside it are.
    if (event.key === "Enter" && event.target === replace) {
      event.preventDefault();
      (event.shiftKey ? replaceAll : replaceNext)(view);
    }
  });

  return {
    dom,
    top: true,
    mount() {
      show(getSearchQuery(view.state));
      showing(view.state.field(replacing));
      field.select();
    },
    update(update) {
      for (const one of update.transactions) {
        for (const effect of one.effects) {
          if (effect.is(setSearchQuery) && !effect.value.eq(asked)) show(effect.value);
          if (effect.is(showReplace)) {
            showing(effect.value);
            if (effect.value) replace.select();
          }
        }
      }
    },
  };
}
