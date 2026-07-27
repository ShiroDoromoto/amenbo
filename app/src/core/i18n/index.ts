// Lightweight i18n: UI labels are localized by config.language (read through the snapshot). No
// translation library — one dictionary file per language, and English underneath them all.
//
// Every lookup here reads the same way: take what the current language has for the key, and take
// English when it has nothing. Translation arrives a language at a time and mostly by machine, so a
// missing key is the normal state rather than a fault — the screen shows the English string and
// stays whole. What is *not* here shows up as the bare key, which is the one case nothing can
// render.
import { type ErrorCode, isErrorCode } from "../errorCodes";
import { type DoctorIssueKind, isDoctorIssueKind } from "../doctorKinds";
import type { Priority, Status } from "../../mock/types";
import type { DoctorTemplate, Translation, UiKey, ViewKind } from "./keys";
import { en } from "./locales/en";
import { ja } from "./locales/ja";
import { currentLang, type Lang } from "./lang";

export { currentLang, dateLocale, normalizeLang, type Lang } from "./lang";
export type { ViewKind } from "./keys";

const DICTS: Record<Lang, Translation> = { en, ja };

/** The UI string this language has for the key, else the English one. */
function ui(key: string, lang: Lang): string | undefined {
  const k = key as UiKey;
  return DICTS[lang].ui[k] ?? en.ui[k];
}

/** Localizes a fixed UI-chrome string. A key no language has falls back to the key itself. */
export function t(key: string, lang: Lang = currentLang()): string {
  return ui(key, lang) ?? key;
}

/**
 * t() with interpolation: substitutes `{name}` placeholders from vars. Use it wherever the
 * dictionary value is a sentence template rather than a plain label — dynamic activity lines, or
 * labels that carry a count. A placeholder with no matching var is left as `{name}`, so a missing
 * substitution shows up on screen instead of vanishing.
 */
export function tf(
  key: string,
  vars: Record<string, string | number> = {},
  lang: Lang = currentLang(),
): string {
  return fill(t(key, lang), vars);
}

/**
 * The structured error a Tauri command rejects with (mirrors `CmdError` in src-tauri/error.rs).
 * Localization maps the stable `code` onto a template; a code with no template (the free-text
 * variants) falls back to the `message` (ja) / `message_en` that core returns — lossless, since
 * core carries both languages correctly.
 */
export interface CmdError {
  code: string;
  message: string; // Japanese (core's Display)
  message_en: string; // English
  fields?: Record<string, unknown> | null;
}

function isCmdError(e: unknown): e is CmdError {
  if (typeof e !== "object" || e === null) return false;
  const o = e as Record<string, unknown>;
  return typeof o.code === "string" && typeof o.message === "string" && typeof o.message_en === "string";
}

/** Renders a CmdError as one line in the current UI language: code template, else the message. */
export function errLabel(err: CmdError, lang: Lang = currentLang()): string {
  const code: ErrorCode | undefined = isErrorCode(err.code) ? err.code : undefined;
  const tmpl = code && (DICTS[lang].err[code] ?? en.err[code]);
  if (tmpl) {
    const f = err.fields ?? {};
    return tmpl.replace(/\{(\w+)\}/g, (_, k) => {
      const v = (f as Record<string, unknown>)[k];
      if (v === undefined || v === null) return `{${k}}`;
      return Array.isArray(v) ? v.join(", ") : String(v);
    });
  }
  return lang === "ja" ? err.message : err.message_en;
}

/**
 * One doctor issue (a Tauri `DoctorIssueDto` / an entry of `StartupHealthDto.issues`). Core holds no
 * prose for an issue — it returns only the kind (template id) and params (the specifics), and this
 * surface composes the sentence a human reads. The suggested fix is likewise written in terms of
 * **this surface's own affordances**: the "Repair" action under Settings > Integrity, re-linking a
 * folder in the project settings folder list, the re-sync banner for the AI guide. Never point a GUI
 * user at a CLI command — the CLI has its own English prose in
 * `crates/amenbo-cli/src/doctor_text.rs`.
 */
export interface DoctorIssueLike {
  kind: string;
  params: Record<string, string>;
}

function fill(tmpl: string, params: Record<string, string | number>): string {
  return tmpl.replace(/\{(\w+)\}/g, (_, k) => (k in params ? String(params[k]) : `{${k}}`));
}

/**
 * Turns a doctor issue into "what is broken" plus "how to fix it" in the current UI language. A kind
 * outside the contract — a newer core reporting an issue this build has never heard of — is printed
 * as the bare kind, so the screen degrades instead of crashing.
 */
export function doctorText(
  issue: DoctorIssueLike,
  lang: Lang = currentLang(),
): { message: string; fixHint: string } {
  if (!isDoctorIssueKind(issue.kind)) return { message: issue.kind, fixHint: "" };
  const tmpl = doctorTemplate(issue.kind, lang);
  return { message: fill(tmpl.message, issue.params), fixHint: fill(tmpl.fix, issue.params) };
}

function doctorTemplate(kind: DoctorIssueKind, lang: Lang): DoctorTemplate {
  return DICTS[lang].doctor[kind] ?? en.doctor[kind];
}

/**
 * Turns an invoke rejection into one human-readable line: a structured `CmdError` is localized by
 * code, a bare string or Error passes through. Every catch site must go through this — `String(e)`
 * would render a CmdError as "[object Object]".
 */
export function errText(e: unknown): string {
  if (isCmdError(e)) return errLabel(e);
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

export function statusLabel(s: Status, lang: Lang = currentLang()): string {
  return DICTS[lang].status[s] ?? en.status[s];
}
export function priorityLabel(p: Priority, lang: Lang = currentLang()): string {
  return DICTS[lang].priority[p] ?? en.priority[p];
}
export function viewLabel(v: ViewKind, lang: Lang = currentLang()): string {
  return DICTS[lang].view[v] ?? en.view[v];
}
