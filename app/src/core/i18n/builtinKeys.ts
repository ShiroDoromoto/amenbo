// **Where a built-in's word sits in the dictionary** (`AMB-D-964`).
//
// **The Japanese dictionary is the table.** A built-in is written into the store in Japanese, and its
// `auto.bi.*` entries are the store's words, each under the key every other language translates — so
// the word is looked up there, and the key it sits under is what the screen's language is read from.
// The core holds the dictionary to its own words (`amenbo_core::ops::automation_builtin`'s test), so a
// word renamed on one side and not the other stops the build rather than showing up untranslated.
//
// It sits here rather than in `../builtinWords` because the refusal's sentences are written in this
// folder too (`errSentence`), and that file reads `t` from here.
import { ja } from "./locales/ja";

/** Each built-in's key, and the part of the dictionary's keys its words sit under. */
const SECTIONS: Record<string, string> = {
  take_task: "takeTask",
  cut_worktree: "cutWorktree",
  fold_worktree: "foldWorktree",
  close_task: "closeTask",
};

/** For each built-in, the store's word → the dictionary key it sits under. */
const WORDS: ReadonlyMap<string, ReadonlyMap<string, string>> = new Map(
  Object.entries(SECTIONS).map(([key, section]) => [
    key,
    new Map(
      Object.entries(ja.ui)
        .filter(([dictKey]) => dictKey.startsWith(`auto.bi.${section}.`))
        .map(([dictKey, word]) => [word as string, dictKey]),
    ),
  ]),
);

/**
 * The dictionary key `word` sits under, where `builtin` is the key of the built-in it was written by.
 * Absent `builtin`, or a word that built-in does not have, and there is none.
 */
export function builtinDictKey(builtin: string | null | undefined, word: string): string | undefined {
  if (builtin === null || builtin === undefined) return undefined;
  return WORDS.get(builtin)?.get(word);
}
