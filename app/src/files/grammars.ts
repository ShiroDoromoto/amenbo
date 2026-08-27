// What the editor can colour, and where each grammar comes from.
//
// Colour comes from TextMate grammars — JSON data run through the JavaScript regular-expression
// engine, never wasm, which this window's CSP refuses (`AMB-D-769`). A grammar is therefore data
// we ship rather than code we run, and this file is the whole list of what we ship.
//
// **Twelve, not the 361 `@shikijs/langs` holds.** The package declares itself MIT, but the grammars
// inside it come from as many different projects under as many different terms: five are GPL-3.0,
// and thirty-three name nothing that could be judged at all. A wildcard import would ride every one
// of them into an Apache-2.0 bundle, so the set is named here one at a time, and
// `guards/check-grammar-licenses.mjs` holds each name to a licence somebody read at its source.
//
// Twelve covers 94.1% of this repository's text files (`AMB-T-3737`). Each entry is its own dynamic
// import, so opening a Rust file fetches the Rust grammar and nothing else.

/** A language this editor can colour: the id `@shikijs/langs` publishes its grammar under. */
export type LangId = keyof typeof GRAMMARS;

/**
 * The bundled grammars, by language id.
 *
 * A value loads one module, and a module carries every grammar it embeds — `html` arrives with the
 * JavaScript and CSS grammars its `<script>` and `<style>` blocks descend into.
 */
export const GRAMMARS = {
  css: () => import("@shikijs/langs/css"),
  go: () => import("@shikijs/langs/go"),
  html: () => import("@shikijs/langs/html"),
  json: () => import("@shikijs/langs/json"),
  markdown: () => import("@shikijs/langs/markdown"),
  python: () => import("@shikijs/langs/python"),
  rust: () => import("@shikijs/langs/rust"),
  shellscript: () => import("@shikijs/langs/shellscript"),
  sql: () => import("@shikijs/langs/sql"),
  toml: () => import("@shikijs/langs/toml"),
  tsx: () => import("@shikijs/langs/tsx"),
  yaml: () => import("@shikijs/langs/yaml"),
} as const;

/** The scope a language's own grammar is registered under — where tokenizing a file starts. */
export const SCOPES: Record<LangId, string> = {
  css: "source.css",
  go: "source.go",
  html: "text.html.basic",
  json: "source.json",
  markdown: "text.html.markdown",
  python: "source.python",
  rust: "source.rust",
  shellscript: "source.shell",
  sql: "source.sql",
  toml: "source.toml",
  tsx: "source.tsx",
  yaml: "source.yaml",
};

// The JavaScript family is one grammar, not four. VS Code's TypeScriptReact grammar is a superset
// of the other three — it reads plain JavaScript and JSX as well as TypeScript — so a second copy
// of nearly the same 100 KB would buy nothing.
const BY_EXTENSION: Record<string, LangId> = {
  ".bash": "shellscript",
  ".cjs": "tsx",
  ".css": "css",
  ".cts": "tsx",
  ".go": "go",
  ".htm": "html",
  ".html": "html",
  ".js": "tsx",
  ".json": "json",
  ".jsonc": "json",
  ".jsx": "tsx",
  // The panel draws `.md` and `.markdown` as rendered Markdown rather than as text
  // (`./FilesPanel`), so today the grammar answers for the other spellings — and for those two the
  // day a reader can ask for the source instead.
  ".markdown": "markdown",
  ".md": "markdown",
  ".mdown": "markdown",
  ".mjs": "tsx",
  ".mkd": "markdown",
  ".mts": "tsx",
  ".py": "python",
  ".pyi": "python",
  ".rs": "rust",
  ".sh": "shellscript",
  ".sql": "sql",
  ".toml": "toml",
  ".ts": "tsx",
  ".tsx": "tsx",
  ".yaml": "yaml",
  ".yml": "yaml",
  ".zsh": "shellscript",
};

// Files a language owns by name rather than by suffix: a dotfile has no suffix to read, and its
// leading dot would otherwise be taken for one.
const BY_NAME: Record<string, LangId> = {
  ".bash_profile": "shellscript",
  ".bashrc": "shellscript",
  ".zprofile": "shellscript",
  ".zshrc": "shellscript",
};

/**
 * Which language `name` is written in, or null for one nothing bundled here reads.
 *
 * Null is an ordinary answer, not a failure: an uncoloured file is what the panel drew before there
 * were grammars at all, and most of what a folder holds is still uncoloured.
 */
export function langFor(name: string): LangId | null {
  const lower = name.toLowerCase();
  const byName = BY_NAME[lower];
  if (byName !== undefined) return byName;
  const dot = lower.lastIndexOf(".");
  if (dot <= 0) return null;
  return BY_EXTENSION[lower.slice(dot)] ?? null;
}

/**
 * Fetch `lang`'s grammar and stand it up ready to tokenize.
 *
 * The registry is per-editor rather than shared: one open file needs one language, and a cache
 * across files would hold every grammar ever opened for as long as the window is up.
 */
export async function loadGrammar(lang: LangId) {
  const [{ INITIAL, Registry }, { createJavaScriptRegexEngine }, module] = await Promise.all([
    import("@shikijs/vscode-textmate"),
    import("@shikijs/engine-javascript"),
    GRAMMARS[lang](),
  ]);

  // The engine speaks shiki's vocabulary and the registry speaks TextMate's; the two differ by the
  // names of two methods. `forgiving` drops a pattern that will not translate into a JavaScript
  // regular expression rather than refusing the whole grammar — one pattern in the 377,000 shiki
  // ships needs it (`AMB-T-3738`), and losing colour on one construct beats losing the file's.
  const engine = createJavaScriptRegexEngine({ forgiving: true });
  const registry = new Registry({
    onigLib: {
      createOnigString: (s) => engine.createString(s),
      createOnigScanner: (patterns) => engine.createScanner(patterns),
    },
    // Synchronous by design: every grammar this call can reach is already in hand, so a scope the
    // module does not carry is a scope nothing will ever supply.
    loadGrammar: (scope) => module.default.find((g) => g.scopeName === scope) ?? null,
  });

  const grammar = registry.loadGrammar(SCOPES[lang]);
  if (grammar === null) throw new Error(`no grammar for ${SCOPES[lang]}`);
  return { grammar, initial: INITIAL };
}
