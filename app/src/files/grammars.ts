// What the editor can colour, and where each grammar comes from.
//
// Colour comes from TextMate grammars — JSON data run through the JavaScript regular-expression
// engine, never wasm, which this window's CSP refuses (`AMB-D-769`). A grammar is therefore data
// we ship rather than code we run, and this file is the whole list of what we ship.
//
// **Twelve, not the 260 `tm-grammars` holds.** The package declares itself MIT, but the grammars
// inside it come from as many different projects under as many different terms: a handful are
// GPL-3.0 and a good many name nothing that could be judged at all. A wildcard import would ride
// every one of them into an Apache-2.0 bundle, so the set is named here one at a time, and
// `guards/check-grammar-licenses.mjs` holds each name to a licence somebody read at its source.
//
// **What a language is drawn into is named here too, rather than arriving with it** (`AMB-D-908`).
// The set this replaces handed over one module per language with everything that language descends
// into already inside it: asking for Ruby brought thirty grammars, one of which claims no licence at
// all, and a static import leaves no way to take that one back out. Here a language is a list, and
// the list is ours — HTML is drawn with JavaScript and CSS because it descends into them, and
// nothing else rides along.
//
// Twelve covers 94.1% of this repository's text files (`AMB-T-3737`). Each entry is its own dynamic
// import, so opening a Rust file fetches the Rust grammar and nothing else.

// The one thing the registry is handed is a grammar, so its own type is what a loaded one is read
// as. It is a type and nothing else: the library itself is still fetched when a file is opened.
import type { IRawGrammar } from "@shikijs/vscode-textmate";

/** A language this editor can colour: the id `tm-grammars` publishes its grammar under. */
export type LangId = keyof typeof GRAMMARS;

/**
 * The bundled grammars, by language id.
 *
 * A value is every grammar that language is drawn with: its own first, then whatever it descends
 * into. `html` is the only one with anything behind it — a `<script>` is JavaScript and a `<style>`
 * is CSS — and a language whose list is one entry is drawn out of that one grammar and nothing else.
 */
export const GRAMMARS = {
  css: () => [import("tm-grammars/grammars/css.json")],
  go: () => [import("tm-grammars/grammars/go.json")],
  html: () => [
    import("tm-grammars/grammars/html.json"),
    import("tm-grammars/grammars/javascript.json"),
    import("tm-grammars/grammars/css.json"),
  ],
  json: () => [import("tm-grammars/grammars/json.json")],
  markdown: () => [import("tm-grammars/grammars/markdown.json")],
  // **The root is the HTML one, not `source.php`** — a PHP file is HTML with `<?php` cut into it,
  // and reading it the other way round draws `<h1 class="a">` as PHP operators and constants, which
  // is worse than no colour at all (`AMB-T-4907` read both). That root is the one grammar no
  // package republishes, so it is baked into the tree beside this (`./tmgrammar`), and the eight
  // behind it are what a PHP file can hold: its own language, the HTML it sits in, and what each of
  // those descends into in turn.
  php: () => [
    import("./tmgrammar/text.html.php.json"),
    import("tm-grammars/grammars/php.json"),
    import("tm-grammars/grammars/html.json"),
    import("tm-grammars/grammars/html-derivative.json"),
    import("tm-grammars/grammars/javascript.json"),
    import("tm-grammars/grammars/css.json"),
    import("tm-grammars/grammars/json.json"),
    import("tm-grammars/grammars/sql.json"),
    import("tm-grammars/grammars/xml.json"),
    import("tm-grammars/grammars/java.json"),
  ],
  python: () => [import("tm-grammars/grammars/python.json")],
  rust: () => [import("tm-grammars/grammars/rust.json")],
  shellscript: () => [import("tm-grammars/grammars/shellscript.json")],
  sql: () => [import("tm-grammars/grammars/sql.json")],
  toml: () => [import("tm-grammars/grammars/toml.json")],
  tsx: () => [import("tm-grammars/grammars/tsx.json")],
  yaml: () => [import("tm-grammars/grammars/yaml.json")],
} as const;

/** The scope a language's own grammar is registered under — where tokenizing a file starts. */
export const SCOPES: Record<LangId, string> = {
  css: "source.css",
  go: "source.go",
  html: "text.html.basic",
  json: "source.json",
  markdown: "text.html.markdown",
  php: "text.html.php",
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
  ".ctp": "php",
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
  // Five names are PHP's, and they are the five VS Code's own extension claims — `.ctp` is up
  // among the c's, and CakePHP's. `.blade.php` ends in one of them, so a Laravel template is read
  // as the PHP it is: the tags, the attributes and everything between `<?php` and `?>` are
  // coloured, and Blade's own `@if` and `{{ }}` come back plain rather than wrong (`AMB-T-4907`).
  ".php": "php",
  ".php4": "php",
  ".php5": "php",
  ".phtml": "php",
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
  const [{ INITIAL, Registry }, { createJavaScriptRegexEngine }, ...loaded] = await Promise.all([
    import("@shikijs/vscode-textmate"),
    import("@shikijs/engine-javascript"),
    ...GRAMMARS[lang](),
  ]);
  const carried = loaded.map((one) => one.default as IRawGrammar);

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
    // list does not carry is a scope nothing will ever supply.
    loadGrammar: (scope) => carried.find((one) => one.scopeName === scope) ?? null,
  });

  const grammar = registry.loadGrammar(SCOPES[lang]);
  if (grammar === null) throw new Error(`no grammar for ${SCOPES[lang]}`);
  return { grammar, initial: INITIAL };
}
