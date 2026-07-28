// Lightweight i18n: UI labels are localized by config.language (read through the snapshot). No
// translation library — one dictionary file per language, and English underneath them all.
//
// Every lookup here reads the same way: take what the current language has for the key, and take
// English when it has nothing. Translation arrives a language at a time and mostly by machine, so a
// missing key must never cost the screen — the reader sees the English string and the page stays
// whole. What is *not* here shows up as the bare key, which is the one case nothing can render.
//
// Falling back is the runtime answer, not permission to ship half a language: because a gap is
// silent by construction, coverage.test.ts counts it at build time and fails on a dictionary that
// does not cover the English key set.
import type { EventDto } from "../../bindings/bindings";
import { type ErrorCode, isErrorCode } from "../errorCodes";
import { type DoctorIssueKind, isDoctorIssueKind } from "../doctorKinds";
import type { Priority, Status } from "../../mock/types";
import type { DoctorTemplate, Translation, UiKey, ViewKind } from "./keys";
import { en } from "./locales/en";
import { ja } from "./locales/ja";
import { zhHans } from "./locales/zh-Hans";
import { zhHant } from "./locales/zh-Hant";
import { ko } from "./locales/ko";
import { es } from "./locales/es";
import { ptBR } from "./locales/pt-BR";
import { fr } from "./locales/fr";
import { de } from "./locales/de";
import { it } from "./locales/it";
import { ru } from "./locales/ru";
import { hi } from "./locales/hi";
import { id } from "./locales/id";
import { vi } from "./locales/vi";
import { th } from "./locales/th";
import { tr } from "./locales/tr";
import { pl } from "./locales/pl";
import { nl } from "./locales/nl";
import { uk } from "./locales/uk";
import { currentLang, type Lang } from "./lang";
import { formatNumber } from "./format";

export { currentLang, dateLocale, DEFAULT_LANG, guessLang, langEndonym, LANGS, normalizeLang, type Lang } from "./lang";
// Dates, times and numbers are not dictionary entries — `Intl` writes them (see ./format).
export {
  agoLabel, dueLabel, formatDay, formatDayTime, formatNumber, monthLabel, weekdayLabels,
} from "./format";
export type { ViewKind } from "./keys";

/**
 * Every dictionary this build carries — one file per supported language, nineteen of them. The type
 * stays partial because that is the runtime guarantee underneath: a language this map does not hold
 * resolves to English everywhere instead of breaking a screen.
 *
 * Exported because the coverage gate has to read what the app actually loads: a list of languages
 * kept beside this one would go stale the first time a dictionary is added, and the gate would then
 * pass by not looking.
 */
export const DICTIONARIES: Partial<Record<Lang, Translation>> = {
  en, ja, "zh-Hans": zhHans, "zh-Hant": zhHant, ko, es, "pt-BR": ptBR, fr, de, it, ru,
  hi, id, vi, th, tr, pl, nl, uk,
};

/** The UI string this language has for the key, else the English one. */
function ui(key: string, lang: Lang): string | undefined {
  const k = key as UiKey;
  return DICTIONARIES[lang]?.ui[k] ?? en.ui[k];
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
  const tmpl = code && (DICTIONARIES[lang]?.err[code] ?? en.err[code]);
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

/**
 * Fills a sentence's `{name}` placeholders. A number goes through `Intl` on the way in: every number
 * a template interpolates is a quantity — a count, a page, a percentage — and a quantity is written
 * the way the reader's locale writes one.
 */
function fill(tmpl: string, params: Record<string, string | number>): string {
  return tmpl.replace(/\{(\w+)\}/g, (_, k) => {
    if (!(k in params)) return `{${k}}`;
    const v = params[k];
    return typeof v === "number" ? formatNumber(v) : v;
  });
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
  return DICTIONARIES[lang]?.doctor[kind] ?? en.doctor[kind];
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
  return DICTIONARIES[lang]?.status[s] ?? en.status[s];
}

/** Built once per language: selecting is cheap, constructing the rules is not. */
const PLURAL_RULES = new Map<Lang, Intl.PluralRules>();

/**
 * Which arm this language uses for this count. The rules are the platform's, not ours: English
 * splits at one, Japanese and Korean never split at all, and Russian and Polish take a different
 * form again at two-through-four and at the teens. Nineteen languages is nineteen sets of rules,
 * and `Intl` already carries them all.
 */
export function pluralCategory(n: number, lang: Lang = currentLang()): Intl.LDMLPluralRule {
  let rules = PLURAL_RULES.get(lang);
  if (!rules) {
    rules = new Intl.PluralRules(lang);
    PLURAL_RULES.set(lang, rules);
  }
  return rules.select(n);
}

/**
 * A counted sentence: the arm of `base` this language uses for `n`, with `{n}` filled in.
 *
 * `other` is the arm every language has, so it is where a count falls when the arm its rules asked
 * for is not translated — a Russian dictionary that stops after `one` and `other` reads a little
 * wrong at three rather than printing a key at the reader.
 */
export function tn(base: string, n: number, lang: Lang = currentLang()): string {
  const template = ui(`${base}.${pluralCategory(n, lang)}`, lang) ?? ui(`${base}.other`, lang);
  return fill(template ?? `${base}.other`, { n });
}

/**
 * The name a timeline row's target goes by. An empty one is not a blank to render: core sends it
 * empty when the target is deleted **and** the ledger row that carried its name is past recovering
 * (compacted away, or beyond the name lookback budget), so the reader gets a stand-in saying so.
 */
export function targetTitle(title: string, lang: Lang = currentLang()): string {
  return title === "" ? t("act.nameless", lang) : title;
}

/**
 * One system event as a line of prose. Like `doctorText`, the backend holds no wording for it — it
 * sends the kind that names the template and the values that fill it — so this is where a timeline
 * line is written, once, for both the Tauri path and the browser fallback. A kind this build has
 * never heard of falls to the generic "updated" line rather than showing nothing.
 */
export function eventText(
  event: EventDto,
  title: string,
  lang: Lang = currentLang(),
): string {
  const name = targetTitle(title, lang);
  switch (event.kind) {
    case "task.created":
      return tf("act.created", { title: name }, lang);
    case "task.status_changed": {
      const s = event.status;
      const status = s && isStatus(s) ? statusLabel(s, lang) : (s ?? "");
      return tf("act.statusChanged", { title: name, status }, lang);
    }
    case "task.assigned":
      if (event.toKind === undefined) return tf("act.unassigned", { title: name }, lang);
      return tf(event.toKind === "ai" ? "act.assignedAi" : "act.assigned", { title: name }, lang);
    case "task.moved":
      return tf("act.moved", { title: name }, lang);
    case "task.unblocked":
      return tf("act.unblocked", { title: name }, lang);
    case "task.deleted":
    case "decision.deleted":
      return tf("act.deleted", { title: name }, lang);
    case "project.deleted": {
      const tasks = event.tasks ?? 0;
      const decisions = event.decisions ?? 0;
      if (tasks + decisions === 0) return tf("act.deleted", { title: name }, lang);
      return tf("act.deletedWith", {
        title: name,
        tasks: tn("act.nTasks", tasks, lang),
        decisions: tn("act.nDecisions", decisions, lang),
      }, lang);
    }
    default:
      return tf("act.updated", { title: name }, lang);
  }
}

function isStatus(s: string): s is Status {
  return s in en.status;
}
export function priorityLabel(p: Priority, lang: Lang = currentLang()): string {
  return DICTIONARIES[lang]?.priority[p] ?? en.priority[p];
}
export function viewLabel(v: ViewKind, lang: Lang = currentLang()): string {
  return DICTIONARIES[lang]?.view[v] ?? en.view[v];
}
