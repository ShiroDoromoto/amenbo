// The pure logic behind the calendar and timeline views. Everything that needs no DOM — bucketing by date,
// building the month grid, laying out the gantt axis — lives here, so that the `.tsx` side (CalendarView,
// TimelineView) does nothing but draw the result. A due date is a calendar day (`YYYY-MM-DD`), so day arithmetic
// runs on the UTC epoch to keep it out of the timezone's reach (no off-by-one from a local DST shift or offset).
import type { TaskCard } from "../mock/types";
import type { Lang } from "./i18n";

// Split a "YYYY-MM-DD" (its first 10 characters) into [year, month(1-12), day].
export function parseYmd(s: string): [number, number, number] {
  const [y, m, d] = s.slice(0, 10).split("-").map(Number);
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

// Where a date stands relative to today: overdue, today, or still ahead.
export function dueKind(due: string, today: string): "overdue" | "today" | "future" {
  const d = due.slice(0, 10);
  return d < today ? "overdue" : d === today ? "today" : "future";
}

// Due day (`YYYY-MM-DD`) → the tasks due on it. Tasks with no due date are left out.
export function groupByDue(tasks: TaskCard[]): Map<string, TaskCard[]> {
  const m = new Map<string, TaskCard[]>();
  for (const t of tasks) {
    if (!t.due) continue;
    const key = t.due.slice(0, 10);
    const arr = m.get(key);
    if (arr) arr.push(t);
    else m.set(key, [t]);
  }
  return m;
}

// Days relative to today (due - today). Negative is past, 0 is today, positive is future.
export function relativeDays(due: string, today: string): number {
  return daysBetween(today, due.slice(0, 10));
}

// ───────────────────────── Timeline (gantt) ─────────────────────────

export interface TimelineRow {
  task: TaskCard;
  due: string; // YYYY-MM-DD
  leftPct: number; // left edge of the bar (% of the axis window)
  widthPct: number; // width of the bar (%, clamped to a minimum)
  kind: "overdue" | "today" | "future";
}

export interface TimelineModel {
  rows: TimelineRow[]; // ascending by due
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
export function timelineModel(tasks: TaskCard[], today: string): TimelineModel {
  const dued = tasks.filter((t) => t.due).map((t) => ({ task: t, due: t.due!.slice(0, 10) }));
  const noDue = tasks.filter((t) => !t.due);
  dued.sort((a, b) => (a.due < b.due ? -1 : a.due > b.due ? 1 : 0));

  const dues = dued.map((x) => x.due);
  const lo = dues.reduce((m, d) => (d < m ? d : m), today);
  const hi = dues.reduce((m, d) => (d > m ? d : m), today);
  const axisStart = addDays(lo, -1);
  const axisEnd = addDays(hi, 1);
  const spanDays = Math.max(1, daysBetween(axisStart, axisEnd));
  const pctOf = (d: string) => (daysBetween(axisStart, d) / spanDays) * 100;

  const rows: TimelineRow[] = dued.map(({ task, due }) => {
    const a = due < today ? due : today;
    const b = due < today ? today : due;
    const leftPct = pctOf(a);
    const widthPct = Math.max(1.5, pctOf(b) - leftPct); // a minimum width, so a same-day point is still visible
    return { task, due, leftPct, widthPct, kind: dueKind(due, today) };
  });

  return { rows, noDue, axisStart, axisEnd, spanDays, todayPct: pctOf(today) };
}

// ───────────────────────── Display labels ─────────────────────────

const MONTHS_EN = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];

// The month heading (year, month=0..11).
export function monthLabel(year: number, month: number, lang: Lang): string {
  return lang === "ja" ? `${year}年${month + 1}月` : `${MONTHS_EN[month]} ${year}`;
}

// The weekday headings (weekStart=0 starts the week on Sunday).
export function weekdayLabels(lang: Lang, weekStart = 0): string[] {
  const base = lang === "ja"
    ? ["日", "月", "火", "水", "木", "金", "土"]
    : ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];
  return [...base.slice(weekStart), ...base.slice(0, weekStart)];
}
