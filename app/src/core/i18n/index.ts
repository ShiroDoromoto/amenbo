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
import { currentLang, type Lang } from "./lang";

export { currentLang, dateLocale, normalizeLang, type Lang } from "./lang";
export type { ViewKind } from "./keys";

/**
 * Every dictionary this build carries. Exported because the coverage gate has to read what the app
 * actually loads: a list of languages kept beside this one would go stale the first time a
 * dictionary is added, and the gate would then pass by not looking.
 */
export const DICTIONARIES: Record<Lang, Translation> = { en, ja };

/** The UI string this language has for the key, else the English one. */
function ui(key: string, lang: Lang): string | undefined {
  const k = key as UiKey;
  return DICTIONARIES[lang].ui[k] ?? en.ui[k];
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
  const tmpl = code && (DICTIONARIES[lang].err[code] ?? en.err[code]);
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
  return DICTIONARIES[lang].doctor[kind] ?? en.doctor[kind];
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
  return DICTIONARIES[lang].status[s] ?? en.status[s];
}

/**
 * A counted noun, from the `.one` / `.other` pair under `base`. English needs the two forms and
 * Japanese writes one, so the pair is what a dictionary can express in both; a language whose plural
 * rules need more than a count of one will want `Intl.PluralRules` picking the arm instead.
 */
function counted(base: string, n: number, lang: Lang): string {
  return tf(`${base}.${n === 1 ? "one" : "other"}`, { n }, lang);
}

/** How long ago, from a timestamp — the wording under every comment and activity line. */
export function agoLabel(at: string, lang: Lang = currentLang(), now: number = Date.now()): string {
  const secs = Math.max(0, Math.floor((now - new Date(at).getTime()) / 1000));
  if (secs < 60) return t("ago.justNow", lang);
  if (secs < 3600) return counted("ago.minutes", Math.floor(secs / 60), lang);
  if (secs < 86400) return counted("ago.hours", Math.floor(secs / 3600), lang);
  return counted("ago.days", Math.floor(secs / 86400), lang);
}

/**
 * The due chip's wording, from the bare date core holds. Days are counted in whole calendar days
 * from today, so "tomorrow" means the next date rather than 24 hours from now — which is what a due
 * date means to the person who set it.
 */
export function dueLabel(due: string, lang: Lang = currentLang(), today: Date = new Date()): string {
  // A due date is a day. Anything a caller has attached to it is cut off first, the same way
  // `dueKind` colours the chip, so the two never disagree about which day this is.
  const at = new Date(`${due.slice(0, 10)}T00:00:00`);
  const start = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  const diff = Math.round((at.getTime() - start.getTime()) / 86_400_000);
  if (diff === 0) return t("due.today", lang);
  if (diff === 1) return t("due.tomorrow", lang);
  if (diff === -1) return t("due.yesterday", lang);
  return diff > 0 ? counted("due.inDays", diff, lang) : counted("due.daysAgo", -diff, lang);
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
        tasks: counted("act.nTasks", tasks, lang),
        decisions: counted("act.nDecisions", decisions, lang),
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
  return DICTIONARIES[lang].priority[p] ?? en.priority[p];
}
export function viewLabel(v: ViewKind, lang: Lang = currentLang()): string {
  return DICTIONARIES[lang].view[v] ?? en.view[v];
}
