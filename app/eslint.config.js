// typescript-eslint cannot read TypeScript 7, so the sources are parsed by Babel. Types, unused
// names and undefined names are left to `tsc`; layout is left to the hand (see .prettierignore).
import babelParser from "@babel/eslint-parser";
import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";

const babel = (plugins) => ({
  parser: babelParser,
  parserOptions: {
    requireConfigFile: false,
    babelOptions: { babelrc: false, configFile: false, parserOpts: { plugins } },
  },
});

export default [
  { ignores: ["dist/", "src-tauri/"] },
  js.configs.recommended,
  { languageOptions: { globals: { ...globals.browser, ...globals.node } } },
  { files: ["**/*.ts"], languageOptions: babel(["typescript"]) },
  { files: ["**/*.tsx"], languageOptions: babel(["typescript", "jsx"]) },
  {
    files: ["**/*.{ts,tsx}"],
    plugins: { "react-hooks": reactHooks },
    rules: {
      "no-undef": "off",
      "no-unused-vars": "off",
      "no-redeclare": "off",
      "no-irregular-whitespace": ["error", { skipRegExps: true }],
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
    },
  },
];
