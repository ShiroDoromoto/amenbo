// The answer to one of a step's settings, in the shape its kind takes — read out of what core keeps,
// and built back up from what a control took (`AMB-T-5256`).
//
// **A task filter is never one string here.** `--filter`'s expression is a thing to write, and what
// the panel takes is a row per axis with the values on it pressed: two on one row is any-of, one on
// each of two rows is both. So the answer is an object naming each part, which is the same reading
// core gives the expression and the same shape the command line writes
// (`crates/amenbo-cli/src/cmd/automation.rs`'s `cfg_value`).
//
// **What the JSON's own type says is which kind answered it**: an object for a task filter, a number
// for a number, a string for the other three. Nothing here reads the declaration to decide that — the
// control that took the answer knew which kind it was taking.
import type { AutomationCfgDto } from "../bindings/bindings";

/** What is pressed on each row of a task filter, by the name of the part it answers. */
export type TaskFilter = Record<string, readonly string[]>;

/**
 * The rows a task filter is taken on, in the order they are drawn.
 *
 * **Three, and the three a person reaching for one wants first**: whose it is, where it has got to,
 * and whether anything is in the way. The parts core's filter accepts are more than these, and the
 * ones not here are not lost — they are the next rows to draw, not a different control.
 *
 * `single` is the part core takes one answer for: `ready` is a yes or a no, and pressing the other
 * replaces it rather than asking for both (`amenbo_core::query::Filter::parse`).
 */
export const FILTER_ROWS: readonly { key: string; single: boolean; values: readonly string[] }[] = [
  { key: "assignee", single: false, values: ["none", "me", "me-ai"] },
  { key: "status", single: false, values: ["todo", "in_progress", "done", "blocked", "rejected"] },
  { key: "ready", single: true, values: ["yes", "no"] },
];

/**
 * The orders a task filter can take its tasks in, as `task list --sort` spells them, in the order the
 * list draws them.
 *
 * **The first is what an answer that names none is taken in** (`amenbo_core::ops::automation_step::
 * TASKFILTER_SORT_DEFAULT`), so it is also what the list shows for one. These are the orders a person
 * picking the next task to work reaches for; `task list --sort` takes more, and an answer written with
 * one of those on the command line is shown as it is rather than put back to the first (`sortChoices`).
 */
export const FILTER_SORTS: readonly string[] = ["priority", "due", "created"];

/** The key a task filter's answer keeps its order under (`TASKFILTER_SORT_KEY`). It is not a part: it
 *  says which task comes first, not which are in. */
const SORT_KEY = "sort";

/** What a step's settings say once the declaration and the answer are read together. */
export function isAnswered(cfg: AutomationCfgDto): boolean {
  return cfg.value !== undefined && cfg.value !== "";
}

/** Whatever was parsed out of a stored answer, or nothing where it is unanswered or unreadable. */
function parsed(value: string | undefined): unknown {
  if (value === undefined || value === "") return undefined;
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return undefined;
  }
}

/** A `folder`, a `choice` or a `text` answer, as the box holding it shows it. */
export function readText(value: string | undefined): string {
  const one = parsed(value);
  return typeof one === "string" ? one : "";
}

/** A `number` answer, or nothing where none was given. */
export function readNumber(value: string | undefined): number | null {
  const one = parsed(value);
  return typeof one === "number" ? one : null;
}

/** A task filter answer, as the rows show it. An unreadable one draws as nothing pressed. */
export function readFilter(value: string | undefined): TaskFilter {
  const one = parsed(value);
  if (one === null || typeof one !== "object" || Array.isArray(one)) return {};
  const out: Record<string, string[]> = {};
  for (const [key, part] of Object.entries(one as Record<string, unknown>)) {
    if (key === SORT_KEY || !Array.isArray(part)) continue;
    const values = part.filter((v): v is string => typeof v === "string");
    if (values.length > 0) out[key] = values;
  }
  return out;
}

/** The order a task filter answer takes its tasks in, or the first of `FILTER_SORTS` where it names
 *  none — which is the order core runs such an answer in. */
export function readSort(value: string | undefined): string {
  const one = parsed(value);
  if (one === null || typeof one !== "object" || Array.isArray(one)) return FILTER_SORTS[0];
  const sort = (one as Record<string, unknown>)[SORT_KEY];
  return typeof sort === "string" && sort !== "" ? sort : FILTER_SORTS[0];
}

/** The orders the list offers for an answer taken in `sort`: the usual ones, and `sort` itself where
 *  it is none of them — written on the command line, and still the order the step runs in. */
export function sortChoices(sort: string): readonly string[] {
  return FILTER_SORTS.includes(sort) ? FILTER_SORTS : [...FILTER_SORTS, sort];
}

/** What a press on one row does: it takes the value up, or lets it go. */
export function pressed(now: TaskFilter, key: string, value: string, single: boolean): TaskFilter {
  const had = now[key] ?? [];
  const next = had.includes(value)
    ? had.filter((one) => one !== value)
    : single
      ? [value]
      : [...had, value];
  const out: Record<string, readonly string[]> = { ...now };
  if (next.length === 0) delete out[key];
  else out[key] = next;
  return out;
}

/** A text answer on its way to core, or `null` where the box was emptied. */
export function writeText(value: string): string | null {
  return value === "" ? null : JSON.stringify(value);
}

/** A number answer on its way to core, or `null` where the box was emptied. */
export function writeNumber(value: string): string | null {
  if (value.trim() === "") return null;
  const n = Number(value);
  return Number.isFinite(n) ? JSON.stringify(n) : null;
}

/**
 * A task filter on its way to core, or `null` where nothing is pressed on any row.
 *
 * **The order goes with it on every write**, so a press on a row does not take away an order the
 * answer was given — on the list beside the rows, or with `--sort` on the command line. The default
 * order is not written: an answer that names none is taken in it, and one that did would stop
 * following it.
 */
export function writeFilter(filter: TaskFilter, sort: string = FILTER_SORTS[0]): string | null {
  const parts = Object.entries(filter).filter(([, values]) => values.length > 0);
  if (parts.length === 0) return null;
  const out: Record<string, unknown> = Object.fromEntries(parts);
  if (sort !== FILTER_SORTS[0]) out[SORT_KEY] = sort;
  return JSON.stringify(out);
}
