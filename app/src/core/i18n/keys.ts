// The dictionary's shape, and where its keys come from.
//
// English is the source of both. It is the language everything falls back to, so it is the one
// that has to be complete — and once one file is complete, writing the key set out a second time
// only creates a list to keep in step. So the keys are read off `en` instead: a key that exists
// only in another language's file is a typo, and it stops type-checking there rather than showing
// up as a blank on screen.
import type { en } from "./locales/en";

/** The views a project's board can be shown in (a project's `view`, as far as labels go). */
export type ViewKind = "list" | "board" | "calendar" | "timeline";

/** What a doctor issue says: what is broken, and how to fix it. */
export type DoctorTemplate = { message: string; fix: string };

/** Every string the UI can show, in the language that has all of them. */
export type Dictionary = typeof en;

/** A UI-chrome key, written "area.name". */
export type UiKey = keyof Dictionary["ui"];

/** The base of a counted key — every one of them is written with an `other` arm, in every language. */
type BaseOf<K> = K extends `${infer B}.other` ? B : never;

/**
 * The arms a counted key may be written in. English has two of them and asks for no more, but the
 * arm a language uses is its own business: Russian takes a third form at two-through-four and a
 * fourth in the teens, and those arms have to be writable in the Russian file without existing in
 * the English one. Bounded to the bases English declares, so a misspelled key is still a type error.
 */
type PluralArm = `${BaseOf<UiKey>}.${Intl.LDMLPluralRule}`;

/**
 * What one language file supplies. Every section is named, but each entry inside it is optional:
 * a key with no translation is rendered from English, so a half-written dictionary degrades to
 * English instead of breaking a screen. That is a guarantee about the running app, not a standard
 * for what may be committed — coverage.test.ts holds a dictionary that exists to the full key set.
 */
export type Translation = { [S in keyof Dictionary]: Partial<Dictionary[S]> } & {
  ui: Partial<Record<PluralArm, string>>;
};
