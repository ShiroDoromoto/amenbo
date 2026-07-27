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

/**
 * What one language file supplies. Every section is named, but each entry inside it is optional:
 * a key with no translation is rendered from English, so a language ships whatever has been
 * translated so far and the screen never waits on the rest.
 */
export type Translation = { [S in keyof Dictionary]: Partial<Dictionary[S]> };
