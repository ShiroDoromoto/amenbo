// One language, assembled: colour, indentation, folding and brackets, over one shared reading of
// the file's tokens.
//
// They are put together here rather than each fetching what it needs because they all need the same
// thing — where in the file the strings and the comments are — and tokenizing a document four times
// would cost four times as much for one answer (`./tmdoc`).
//
// Nothing in here is required for a file to open — every piece is behind the same dynamic import as
// the editor itself. And a language nothing was baked for is not a language with no manners: the
// generic bracket rule alone puts 85% of the lines of a brace language where its formatter put them
// (`AMB-T-3745`).

import { foldGutter, foldKeymap, indentOnInput } from "@codemirror/language";
import { EditorState, type Extension } from "@codemirror/state";
import { keymap } from "@codemirror/view";
import { brackets } from "./brackets";
import { folding } from "./fold";
import { loadGrammar, type LangId } from "./grammars";
import { painting } from "./highlight";
import { compile, configFor } from "./langconfig";
import { indenting } from "./indent";
import { tmDocField } from "./tmdoc";

/**
 * Everything the editor should do differently because the file is written in `lang`.
 *
 * `null` means a file nothing here reads, and it is not nothing: folding is a count of the spaces at
 * the head of each line and needs no grammar at all, so a plain text file gets it too. What it does
 * not get is colour, and the brackets — those need to know where the strings are, and without a
 * grammar the answer would be a guess (`./brackets`).
 */
export async function language(lang: LangId | null): Promise<Extension> {
  if (lang === null) return [folding(compile({})), foldGutter(), keymap.of(foldKeymap)];

  const [{ grammar, initial }, config] = await Promise.all([loadGrammar(lang), configFor(lang)]);
  const rules = compile(config);
  const field = tmDocField(grammar, initial);

  return [
    field,
    painting(field),
    indenting(field, rules),
    // The one rule that has to run as the character lands rather than at the next line break: `}`
    // and `else` pull their own line back out as they are written.
    ...(rules.decrease === null ? [] : [
      EditorState.languageData.of(() => [{ indentOnInput: rules.decrease }]),
      indentOnInput(),
    ]),
    folding(rules),
    foldGutter(),
    keymap.of(foldKeymap),
    brackets(field, rules),
  ];
}
