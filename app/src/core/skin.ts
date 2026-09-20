// Wearing a skin: the token values a person's file carries, put where the stylesheet's own can be
// overridden by them.
//
// The application's CSS is bundled by Vite, so there is no `:root` left outside it to redefine. What
// there is instead is one sheet of our own, appended to `<head>`, holding the two sides a skin has
// and the drawings it hands over for this build's icons.
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
// It is asked in core as well (`usable_value` in `crates/amenbo-core/src/skin.rs`), which is the
// side that can say why a value of the author's went nowhere — this one is handed a table over IPC
// with nobody left to tell. Two spellings of one rule, held to each other by `skin.test.ts`.
//
// An app draws more than one window and each wears the same skin, so this rides the road appearance
// already takes: the window it was changed in applies it and tells the others (`CHANGED`).
import { currentLang, type Lang } from "./i18n";
import { invoke } from "./ipc";
import { pinTheme } from "./theme";
import type { SkinFontDto, SkinJudgementDto, SkinListDto, SkinTablesDto } from "../bindings/bindings";

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

/**
 * The name of an icon a skin may hand a drawing over for. The names are `Icon.tsx`'s own, written
 * the way the markup writes them, so letters and digits and nothing else — which is also what lets
 * one be put into a selector here without anything to escape.
 *
 * Held to `ICONS` in `crates/amenbo-core/src/skin.rs` by the check, which is what says whether a
 * name the author wrote is one this build draws. Asked again here for the reason a token name is:
 * what arrives came out of a file somebody was handed.
 */
const ICON_NAME = /^[A-Za-z][A-Za-z0-9]*$/;

/**
 * The shape a drawing arrives in: a `data:` URI carrying one of the three forms a mask can be read
 * off, base64 and nothing else. The host builds it out of bytes it read the form off, so this is
 * not where the form is decided — it is where a value that is about to be written into a `url()`
 * is read over, the way {@link usable} reads a token's value over.
 *
 * A jpeg is not in the list: it carries no transparency, so laid as a mask it draws a filled square
 * where the drawing was. The check turns one away on the way in (`icon` in `skin.rs`).
 */
const DRAWING = /^data:image\/(png|webp|svg\+xml);base64,[A-Za-z0-9+/]+={0,2}$/;

/** Is this a value a skin may set a token to? */
function usable(value: string): boolean {
  return VALUE.test(value) && !NOT_IN_A_VALUE.test(value);
}

/**
 * What to call a skin on this screen, in the language the reader is reading it in.
 *
 * An author may write a name per language (`titles:`), and where they wrote none for this reader
 * the one name they did write (`title`) stands. That is the author's own word for their work
 * rather than a language fallen back to, so nothing is said on screen about the choice
 * (`AMB-D-935`, and the same silence as `AMB-D-623`).
 *
 * The reader's own code is looked up rather than the map walked: the codes are the author's, as
 * written, so one this build has never heard of is in there too and matches nothing.
 */
export function skinTitle(
  skin: { title: string; titles: Record<string, string> },
  lang: Lang = currentLang(),
): string {
  // A name written empty is a name the author did not write. Their one name draws instead of a
  // blank where a skin's name goes.
  return skin.titles[lang]?.trim() || skin.title;
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

/**
 * Write a whole skin out to `path`, to start from — this build's, or the one that is on. It lands
 * packed, which is the shape one is handed over in: what comes out goes straight back in.
 */
export function writeSkinTemplate(path: string): Promise<void> {
  return invoke<void>("skin_template_to", { path });
}

/**
 * Write a held skin out to `path` — the file itself, byte for byte, materials and all. Only the
 * ones this device keeps in a file: the four that ship inside the build have none, and
 * {@link writeSkinTemplate} is the road from those.
 */
export function writeSkinOut(name: string, path: string): Promise<void> {
  return invoke<void>("skin_write_out", { name, path });
}

/** The licence of the font a held skin carries, in full. `null` where it carries none. */
export function skinFontLicence(name: string): Promise<string | null> {
  return invoke<string | null>("skin_font_licence", { name });
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

/**
 * The face this window is wearing, so the next change can take it off again. A window holds at
 * most one: a skin carries one font and no weights, bold being synthesised.
 *
 * Held here rather than looked up in `document.fonts`, which has every face the page loaded and no
 * way to say which of them was ours.
 */
let worn: FontFace | null = null;

/**
 * Register the skin's font with this window, or take off whatever was registered (`null`).
 *
 * **The bytes go to `FontFace` rather than into a `data:` URI.** Measured, that is 19.5ms against
 * 34ms for two megabytes, and it touches no CSP directive — a URI would have to be allowed under
 * `font-src`, and the point of carrying the bytes is that nothing is fetched.
 *
 * Each window does this for itself: the webviews do not share a font cache, so the same assembly
 * runs once per window and neither waits on the other.
 *
 * A face that will not load is dropped rather than raised. What the skin said about it was already
 * read over on the way in (format, size, the bytes' own header); what is left here is the parser's
 * own verdict, and the answer to it is the name stack the author wrote beside it.
 */
export async function wearFont(font: SkinFontDto | null): Promise<void> {
  if (worn) {
    document.fonts.delete(worn);
    worn = null;
  }
  if (!font) return;
  try {
    const face = new FontFace(font.family, bytesOf(font.data));
    await face.load();
    document.fonts.add(face);
    worn = face;
  } catch {
    // The stack the author wrote beside it is what draws instead.
  }
}

/**
 * One character per language this application is read in, for asking a face what it has glyphs
 * for. A face is a set of drawings and nothing says what is in it but the drawings, so the only
 * way to find out is to ask about a character.
 *
 * One character stands for a script rather than for a language: a face that draws one kana draws
 * kana, and one that draws one han character draws han. The Latin ones are here for completeness
 * and answer yes for anything that draws letters at all.
 */
const A_LETTER_OF: Record<string, string> = {
  en: "A", ja: "あ", "zh-Hans": "汉", "zh-Hant": "漢", ko: "가", es: "ñ", "pt-BR": "ã",
  fr: "é", de: "ä", it: "à", ru: "Я", hi: "अ", id: "A", vi: "ế", th: "ก", tr: "ğ",
  pl: "ł", nl: "A", uk: "Ї",
};

/**
 * Which of the nineteen this face has no glyphs for, by language code. A face that carries only
 * Latin makes a Japanese screen half pixels and half the machine's own letters, and that is worth
 * knowing before the file is taken in rather than after.
 *
 * The face is registered to be asked and taken straight back off, so nothing on screen changes and
 * the cost is the decode.
 *
 * Empty where this window cannot answer — a face it will not read, and a page with no canvas to
 * measure in. Saying nothing is the honest answer to a question that could not be put; the caller
 * shows a line only when there is something in the list.
 */
export async function scriptsMissingFrom(font: SkinFontDto): Promise<string[]> {
  const measure = document.createElement("canvas").getContext("2d");
  if (!measure) return [];
  let face: FontFace;
  try {
    face = new FontFace(font.family, bytesOf(font.data));
    await face.load();
  } catch {
    // Not a face this window can read. What it covers is the parser's answer, and there is none.
    return [];
  }
  document.fonts.add(face);
  try {
    return Object.entries(A_LETTER_OF)
      .filter(([, letter]) => !hasGlyph(measure, font.family, letter))
      .map(([lang]) => lang);
  } finally {
    document.fonts.delete(face);
  }
}

/**
 * Does this face draw that character?
 *
 * **`FontFaceSet.check` cannot answer this.** It says whether the faces a piece of text would use
 * are loaded, decided by `unicode-range` — and a face declared without one claims every character,
 * so it answers yes for a glyph it does not have.
 *
 * What does answer is the drawing. The character is measured with the face in front of two
 * different fallbacks: where the face supplies the glyph, both measurements are its own and agree;
 * where it does not, each falls through to a different family and they come out apart. Two
 * fallbacks rather than one, because one comparison cannot tell "the face drew it" from "the face
 * and the fallback happen to draw it the same width".
 */
function hasGlyph(measure: CanvasRenderingContext2D, family: string, letter: string): boolean {
  const width = (stack: string) => {
    measure.font = `12px ${stack}`;
    return measure.measureText(letter).width;
  };
  return width(`"${family}", monospace`) === width(`"${family}", serif`);
}

/** base64 to bytes. The wrapping is already out — what arrives here is clean. */
function bytesOf(base64: string): ArrayBuffer {
  const raw = atob(base64);
  const out = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
  return out.buffer;
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
  // The face first, so the values that name it land on a family the window already has.
  void wearFont(tables?.font ?? null);
  // Then the side, before the values: a skin written for one side only has no other side to draw,
  // and a window standing on the side it did not write would show this build's own colours under
  // that skin's name. Done here rather than on the screen that chooses a skin, because a skin is
  // worn at startup and in every window, and only one of those has that screen open.
  pinTheme(onlySide(tables));
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
  layDrawings(s, tables.icons);
}

/**
 * Lay the skin's drawings over the icons they stand in for — one rule per name, after the two
 * sides because an icon is the header's rather than a side's: a skin that draws its own gear draws
 * it on both.
 *
 * **The drawing is a mask and not a picture.** What is painted is `currentColor`, through the
 * drawing's own alpha, so an icon goes on taking the colour of the text beside it and goes on
 * changing with the theme — which is what an inline `<svg>` of this build's own does, and the
 * whole of what has to keep holding (`AMB-D-937`). It also keeps a drawing a drawing: an `<svg>`
 * behind `mask-image` is never a document, so nothing written inside it runs and nothing it names
 * is fetched.
 *
 * The rule lands on the element `Icon.tsx` already draws, by the name the markup already carries
 * (`data-icon`), so no call site is touched and the one mark built without React around it
 * (`iconSvg`) is covered by the same rule. This build's own geometry is inside that element, so it
 * is turned off rather than removed: `fill` and `stroke` are what `.icon` gives the paths, and
 * with neither of them there is nothing left to draw.
 *
 * `-webkit-` as well as the plain names: WebView2 took the unprefixed ones late, and the prefixed
 * pair is what every webview this ships into has.
 */
function layDrawings(s: CSSStyleSheet, icons: Record<string, string>): void {
  for (const [name, drawing] of Object.entries(icons)) {
    if (!ICON_NAME.test(name) || !DRAWING.test(drawing)) continue;
    const at = s.insertRule(`.icon[data-icon="${name}"] {}`, s.cssRules.length);
    const rule = s.cssRules[at] as CSSStyleRule;
    for (const [property, value] of [
      ["-webkit-mask-image", `url("${drawing}")`],
      ["mask-image", `url("${drawing}")`],
      ["-webkit-mask-size", "contain"],
      ["mask-size", "contain"],
      ["-webkit-mask-repeat", "no-repeat"],
      ["mask-repeat", "no-repeat"],
      ["-webkit-mask-position", "center"],
      ["mask-position", "center"],
      ["background-color", "currentColor"],
      ["fill", "none"],
      ["stroke", "none"],
    ]) {
      rule.style.setProperty(property, value);
    }
  }
}

/**
 * The one side a skin wrote, where it wrote one. Read off the tables rather than off `themes`,
 * which is not in them: a side the author declared and left empty is refused by the check, so a
 * table with values is a side that was declared and a table without one is a side that was not.
 */
function onlySide(tables: SkinTablesDto | null): "light" | "dark" | undefined {
  if (!tables) return undefined;
  const light = Object.keys(tables.light).length > 0;
  const dark = Object.keys(tables.dark).length > 0;
  // Neither is a skin that set nothing, and both is a skin with a choice left in it. Only one of
  // the two takes the choice away.
  if (light === dark) return undefined;
  return light ? "light" : "dark";
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
