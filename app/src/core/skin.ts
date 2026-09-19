// Wearing a skin: the token values a person's file carries, put where the stylesheet's own can be
// overridden by them.
//
// The application's CSS is bundled by Vite, so there is no `:root` left outside it to redefine. What
// there is instead is one sheet of our own, appended to `<head>`, holding the two sides a skin has.
// Later in the document order at equal specificity, so its values win over the ones tokens.css set —
// which is the whole mechanism, and the reason the sheet is appended rather than placed anywhere
// else.
//
// **Both sides are written as `[data-theme]`, neither as `:root`.** A skin may set a name on one side
// and leave it on the other, and a `:root` rule would then carry the light value into the dark theme:
// it matches in both, and it would sit after tokens.css's own dark block. Two attribute rules match
// exactly one side each, so a name left alone on a side keeps what this build sets there.
//
// The values are put in through the CSSOM rather than by writing text into the element, and each one
// is read over first. A skin is a file a person was handed, and a value spliced into a declaration
// by string is a value that can end it. `setProperty` is the first half of the answer and not the
// whole of it: what a custom property accepts is nearly anything, and how strictly the brackets in
// it are counted is the engine's business rather than something to build on. So the shape a token
// value may have is asked here (`VALUE`), and a value outside it is left out.
//
// An app draws more than one window and each wears the same skin, so this rides the road appearance
// already takes: the window it was changed in applies it and tells the others (`CHANGED`).
import { invoke } from "./ipc";
import type { SkinJudgementDto, SkinListDto, SkinTablesDto } from "../bindings/bindings";

/** The id of the one sheet a skin is worn through. */
const SHEET_ID = "amenbo-skin";

/**
 * What one window says when the skin is changed in it, so the others change with it. The same road
 * and the same reason as the appearance's: `localStorage` says nothing to a page that has already
 * drawn, and a window split out on its own would keep the skin it came up with.
 */
const CHANGED = "skin-changed";

/**
 * A token name a skin may write — the rule the check holds them to, read again here. Opening on a
 * letter or a digit is part of it: a name beginning with a hyphen would be written as `----c-text`,
 * which is a property of its own rather than the one the author meant.
 */
const NAME = /^[a-z0-9][a-z0-9-]*$/;

/**
 * The shape a token's value may have: a colour, a length, a font stack, a keyword. What is kept out
 * is the punctuation that ends a declaration or opens a rule, the comment delimiters, and the angle
 * brackets — none of which a value of this kind has any use for, and each of which is a way out of
 * the declaration in an engine that counts brackets loosely.
 *
 * `url(` is out on its own terms: no token here takes one, and a skin is a file somebody was handed
 * — it says what colour a thing is, and it does not reach out from the reader's machine. A font it
 * carries rides its own way in.
 */
const VALUE = /^[^;{}<>\\\n\r]{1,512}$/;
const NOT_IN_A_VALUE = /\/\*|\*\/|url\s*\(/i;

/** Is this a value a skin may set a token to? */
function usable(value: string): boolean {
  return VALUE.test(value) && !NOT_IN_A_VALUE.test(value);
}

/** What this device holds, and which of them is on. */
export function listSkins(): Promise<SkinListDto> {
  return invoke<SkinListDto>("skin_list");
}

/**
 * Read one file over without taking it in: what it calls itself, what was set aside, what fell
 * under its floor, and whether a skin is already kept under the name. A file the check turns away
 * rejects instead, carrying the sentence the terminal prints for the same file.
 */
export function readSkinFile(path: string): Promise<SkinJudgementDto> {
  return invoke<SkinJudgementDto>("skin_read", { path });
}

/** Take one in, under the name it gives itself. `replace` answers a name already held. */
export function addSkinFile(path: string, replace: boolean): Promise<string> {
  return invoke<string>("skin_add", { path, replace });
}

/**
 * Be told whenever the skin changes, wherever it was changed — another window, or the menu bar,
 * whose click never reaches the page at all. Answers with the way to stop listening.
 */
export async function watchSkinChanged(said: () => void): Promise<() => void> {
  try {
    const { listen } = await import("@tauri-apps/api/event");
    return await listen(CHANGED, () => said());
  } catch {
    // Outside Tauri there is one window and nothing that can change it from outside this page.
    return () => {};
  }
}

/** Write a whole skin out to `path`, to start from — this build's, or the one that is on. */
export function writeSkinTemplate(path: string): Promise<void> {
  return invoke<void>("skin_template_to", { path });
}

/** One held skin's tables, for trying it on. */
export function skinTables(name: string): Promise<SkinTablesDto | null> {
  return invoke<SkinTablesDto | null>("skin_tables", { name });
}

/**
 * Put a held skin on, or take whatever is on off (`null`), and have every window wear the answer.
 * The writing is the host's, so a window that changed it does not have to be the one that reads it
 * back.
 */
export async function useSkin(name: string | null): Promise<void> {
  await invoke<void>("skin_use", { name });
  setSkin(name === null ? null : await skinTables(name));
}

/**
 * Put one side's values on one element, so what is under it is drawn in them and nothing else is.
 * This is what a fitting is: the frame wears the skin, the screen around it keeps the one that is
 * on, and a set of colours nobody can read stays inside the box it is being read in.
 *
 * Inline rather than a rule, because a rule would need a selector for one element that has no name
 * of its own; the properties inherit from here down, which is the whole of what is wanted.
 */
export function fitOnto(el: HTMLElement | null, values: Record<string, string> | null): void {
  if (!el) return;
  for (const name of [...el.style].filter((p) => p.startsWith("--"))) {
    el.style.removeProperty(name);
  }
  if (!values) return;
  for (const [name, value] of Object.entries(values)) {
    if (!NAME.test(name) || !usable(value)) continue;
    el.style.setProperty(`--${name}`, value);
  }
}

/** The sheet this window wears a skin through, made on first use and emptied on every change. */
function sheet(): CSSStyleSheet | null {
  const head = document.head;
  let el = document.getElementById(SHEET_ID) as HTMLStyleElement | null;
  if (!el) {
    el = document.createElement("style");
    el.id = SHEET_ID;
    // Last in `<head>`, which is what puts it after the bundle's own.
    head.appendChild(el);
  } else if (el !== head.lastElementChild) {
    // Something was appended after it — a lazily loaded chunk's styles, say. Being last is not a
    // detail of how it was made, it is how the values win, so it is taken again.
    head.appendChild(el);
  }
  const s = el.sheet;
  while (s && s.cssRules.length > 0) s.deleteRule(0);
  return s;
}

/**
 * Wear these tables, or nothing (`null`) to go back to the colours this build ships with.
 *
 * A name that is not a name is skipped rather than written: the tables arrive over IPC from a file
 * somebody was handed, and the one place that has to hold is the one where they reach the document.
 */
export function applySkin(tables: SkinTablesDto | null): void {
  const s = sheet();
  if (!s) return;
  if (!tables) return;
  for (const [side, values] of [
    ["light", tables.light],
    ["dark", tables.dark],
  ] as const) {
    const at = s.insertRule(`[data-theme="${side}"] {}`, s.cssRules.length);
    const rule = s.cssRules[at] as CSSStyleRule;
    for (const [name, value] of Object.entries(values)) {
      if (!NAME.test(name) || !usable(value)) continue;
      rule.style.setProperty(`--${name}`, value);
    }
  }
}

/**
 * Wear these tables here, and tell the other windows to wear them too. What the screens that change
 * a skin call; the reading back from disk is {@link initSkin}'s.
 */
export function setSkin(tables: SkinTablesDto | null): void {
  applySkin(tables);
  void import("@tauri-apps/api/event")
    .then(({ emit }) => emit(CHANGED, tables))
    // Outside Tauri (`npm run dev` in a browser) there is one window and nobody to tell.
    .catch(() => {});
}

/**
 * Call once at startup: wear whatever skin this device has on, and keep wearing what it is changed
 * to. Nothing on, a name with no file behind it, and a file that no longer passes the check all come
 * back as nothing to wear, which is the built-in colours.
 */
export function initSkin(): void {
  void invoke<SkinTablesDto | null>("skin_in_use")
    .then(applySkin)
    // A window that cannot ask is a window with no skin on, which is a window that reads.
    .catch(() => {});
  void import("@tauri-apps/api/event")
    .then(({ listen }) => listen<SkinTablesDto | null>(CHANGED, (said) => applySkin(said.payload)))
    .catch(() => {});
}
