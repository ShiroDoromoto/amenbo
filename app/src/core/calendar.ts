// The pure logic behind the calendar and timeline views. Everything that needs no DOM — bucketing by date,
// building the month grid, laying out the gantt axis — lives here, so that the `.tsx` side (CalendarView,
// TimelineView) does nothing but draw the result. A start or a due is a calendar day or an instant (`Schedule`);
// wherever it is cut by day it is read as the reader's own calendar day (`scheduleDay`), and day arithmetic on
// those days runs on the UTC epoch to keep it out of the timezone's reach (no off-by-one from a local DST shift
// or offset).
// The headings drawn beside this grid — the month, the weekdays — are not here: they are formatting, and
// `Intl` writes them (core/i18n/format).
import type { Schedule, TaskCard } from "../mock/types";

// Split a "YYYY-MM-DD" into [year, month(1-12), day].
export function parseYmd(s: string): [number, number, number] {
  const [y, m, d] = s.split("-").map(Number);
  return [y, m, d];
}

// Format a local calendar day as "YYYY-MM-DD". A due date names a day, so the local year/month/day is taken
// rather than the UTC one — it has to agree with the "today" the person in front of the device is living in.
export function fmtDate(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

// The device's local "today" (`YYYY-MM-DD`). `now` is injectable for tests.
export function todayStr(now: Date = new Date()): string {
  return fmtDate(now);
}

// The reader's calendar day a start or a due falls on: the day itself, or the local day of the instant.
export function scheduleDay(s: Schedule): string {
  return s.kind === "day" ? s.day : fmtDate(new Date(s.at));
}

// The reader's local time (`HH:MM`) a start or a due names, or null for a bare day.
export function scheduleTime(s: Schedule): string | null {
  if (s.kind === "day") return null;
  const at = new Date(s.at);
  const hh = String(at.getHours()).padStart(2, "0");
  const mm = String(at.getMinutes()).padStart(2, "0");
  return `${hh}:${mm}`;
}

// A start or a due as plain text: the day, and for an instant the reader's local time after it.
export function scheduleText(s: Schedule): string {
  const time = scheduleTime(s);
  return time === null ? scheduleDay(s) : `${scheduleDay(s)} ${time}`;
}

// A start or a due as the task detail writes it — `YYYY-MM-DD`, or `YYYY-MM-DD HH:MM` on the reader's
// own clock — read back into the value a card carries: a day, or the UTC instant that local time names.
export function parseSchedule(s: string): Schedule {
  const [day, time] = s.split(" ");
  if (!time) return { kind: "day", day };
  const [y, m, d] = parseYmd(day);
  const [hh, mm] = time.split(":").map(Number);
  return { kind: "at", at: new Date(y, m - 1, d, hh, mm).toISOString().replace(".000Z", "Z") };
}

// The order of two dues: by the reader's day, and within one day the instants first, in time order, with
// the bare days last.
export function compareDue(a: Schedule, b: Schedule): number {
  const ad = scheduleDay(a), bd = scheduleDay(b);
  if (ad !== bd) return ad < bd ? -1 : 1;
  if (a.kind === "at" && b.kind === "at") return new Date(a.at).getTime() - new Date(b.at).getTime();
  if (a.kind === b.kind) return 0;
  return a.kind === "at" ? -1 : 1;
}

// The day difference b - a. The subtraction runs on the UTC epoch so that the timezone cannot skew it.
export function daysBetween(a: string, b: string): number {
  const [ay, am, ad] = parseYmd(a);
  const [by, bm, bd] = parseYmd(b);
  return Math.round((Date.UTC(by, bm - 1, bd) - Date.UTC(ay, am - 1, ad)) / 86400000);
}

// The `YYYY-MM-DD` n days after the calendar day s (n may be negative).
export function addDays(s: string, n: number): string {
  const [y, m, d] = parseYmd(s);
  const dt = new Date(Date.UTC(y, m - 1, d + n));
  const yy = dt.getUTCFullYear();
  const mm = String(dt.getUTCMonth() + 1).padStart(2, "0");
  const dd = String(dt.getUTCDate()).padStart(2, "0");
  return `${yy}-${mm}-${dd}`;
}

// month is 0..11, as in JS's getMonth. Returns the {year, month} delta months away.
export function shiftMonth(year: number, month: number, delta: number): { year: number; month: number } {
  const total = year * 12 + month + delta;
  return { year: Math.floor(total / 12), month: ((total % 12) + 12) % 12 };
}

// The weeks that cover the given month (year, month=0..11), as an array of arrays of seven `YYYY-MM-DD`. Days
// from the neighbouring months fill the grid out (weekStart=0 starts the week on Sunday). 4 to 6 weeks, exactly
// enough to cover the month.
export function monthMatrix(year: number, month: number, weekStart = 0): string[][] {
  const firstStr = fmtDate(new Date(year, month, 1));
  const firstWeekday = new Date(year, month, 1).getDay(); // 0 = Sunday
  const lead = (firstWeekday - weekStart + 7) % 7;
  const lastStr = fmtDate(new Date(year, month + 1, 0));
  let cursor = addDays(firstStr, -lead);
  const weeks: string[][] = [];
  do {
    const week: string[] = [];
    for (let i = 0; i < 7; i++) {
      week.push(cursor);
      cursor = addDays(cursor, 1);
    }
    weeks.push(week);
  } while (daysBetween(cursor, lastStr) >= 0); // one more week while the next one still starts inside the month
  return weeks;
}

// Where a date stands relative to today. Tomorrow is a step of its own because the colour ladder needs it:
// a day that has come is a stop (nothing but doing the work moves it), and the last day it can still be
// heeded is the one before — see `--c-due-*` in styles/tokens.css.
export type DueKind = "overdue" | "today" | "tomorrow" | "future";

// A due at an instant is overdue once that instant has passed (`now`), even on its own day.
export function dueKind(due: Schedule, today: string, now: number = Date.now()): DueKind {
  if (due.kind === "at" && new Date(due.at).getTime() < now) return "overdue";
  const d = scheduleDay(due);
  if (d < today) return "overdue";
  if (d === today) return "today";
  return d === addDays(today, 1) ? "tomorrow" : "future";
}

// The reader's due day (`YYYY-MM-DD`) → the tasks due on it. Tasks with no due date are left out.
export function groupByDue(tasks: TaskCard[]): Map<string, TaskCard[]> {
  const m = new Map<string, TaskCard[]>();
  for (const t of tasks) {
    if (!t.due) continue;
    const key = scheduleDay(t.due);
    const arr = m.get(key);
    if (arr) arr.push(t);
    else m.set(key, [t]);
  }
  return m;
}

// ───────────────────────── Timeline (gantt) ─────────────────────────

export interface TimelineRow {
  task: TaskCard;
  due: string; // the reader's due day, YYYY-MM-DD
  leftPct: number; // left edge of the bar (% of the axis window)
  widthPct: number; // width of the bar (%, clamped to a minimum)
  kind: DueKind;
}

export interface TimelineModel {
  rows: TimelineRow[]; // ascending by due (`compareDue`)
  noDue: TaskCard[]; // no due date, so nothing to place on the axis
  axisStart: string; // the day at the left edge of the axis
  axisEnd: string; // the day at the right edge
  spanDays: number; // length of the axis in days (at least 1)
  todayPct: number; // where the "today" line falls (%)
}

// The gantt model: tasks with a due date laid out along time. Each bar spans today and the due date — a future
// due date runs right from today (the time left), a past one runs left from today back to the due date (the
// overrun). The axis window covers every due date and today, padded by a day at each end. Tasks with no due date
// go to noDue and are never placed on the axis.
export function timelineModel(tasks: TaskCard[], today: string, now: number = Date.now()): TimelineModel {
  const dued = tasks.flatMap((t) => (t.due ? [{ task: t, schedule: t.due, due: scheduleDay(t.due) }] : []));
  const noDue = tasks.filter((t) => !t.due);
  dued.sort((a, b) => compareDue(a.schedule, b.schedule));

  const dues = dued.map((x) => x.due);
  const lo = dues.reduce((m, d) => (d < m ? d : m), today);
  const hi = dues.reduce((m, d) => (d > m ? d : m), today);
  const axisStart = addDays(lo, -1);
  const axisEnd = addDays(hi, 1);
  const spanDays = Math.max(1, daysBetween(axisStart, axisEnd));
  const pctOf = (d: string) => (daysBetween(axisStart, d) / spanDays) * 100;

  const rows: TimelineRow[] = dued.map(({ task, schedule, due }) => {
    const a = due < today ? due : today;
    const b = due < today ? today : due;
    const leftPct = pctOf(a);
    const widthPct = Math.max(1.5, pctOf(b) - leftPct); // a minimum width, so a same-day point is still visible
    return { task, due, leftPct, widthPct, kind: dueKind(schedule, today, now) };
  });

  return { rows, noDue, axisStart, axisEnd, spanDays, todayPct: pctOf(today) };
}
