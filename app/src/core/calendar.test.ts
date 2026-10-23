import { describe, it, expect } from "vitest";
import {
  parseYmd, fmtDate, todayStr, daysBetween, addDays, shiftMonth,
  monthMatrix, dueKind, groupByDue, timelineModel, scheduleDay, scheduleText, compareDue,
} from "./calendar";
import type { Schedule, TaskCard } from "../mock/types";

const day = (d: string): Schedule => ({ kind: "day", day: d });
// An instant at a local time, so the day it falls on is the same whatever timezone the test runs in.
const at = (y: number, mo: number, d: number, h: number, mi = 0): Schedule =>
  ({ kind: "at", at: new Date(y, mo - 1, d, h, mi).toISOString() });

// A minimal TaskCard for the tests (only the fields we need are filled in). A string due is a day.
function task(id: number, due: string | Schedule | null, title = `#${id}`): TaskCard {
  return { id, title, due: typeof due === "string" ? day(due) : due } as unknown as TaskCard;
}

describe("date helpers", () => {
  it("fmtDate / todayStr format a local calendar day as YYYY-MM-DD", () => {
    expect(fmtDate(new Date(2026, 5, 9))).toBe("2026-06-09"); // month is 0-indexed (5 = June)
    expect(todayStr(new Date(2026, 11, 31))).toBe("2026-12-31");
  });

  it("parseYmd splits YYYY-MM-DD into [y,m,d]", () => {
    expect(parseYmd("2026-06-21")).toEqual([2026, 6, 21]);
  });

  it("daysBetween is the calendar-day difference (TZ-independent)", () => {
    expect(daysBetween("2026-06-21", "2026-06-21")).toBe(0);
    expect(daysBetween("2026-06-21", "2026-06-29")).toBe(8);
    expect(daysBetween("2026-06-29", "2026-06-21")).toBe(-8);
    // across a month boundary, and across a leap day (2028-02-29)
    expect(daysBetween("2026-12-31", "2027-01-01")).toBe(1);
    expect(daysBetween("2028-02-28", "2028-03-01")).toBe(2);
  });

  it("addDays carries across month and year boundaries", () => {
    expect(addDays("2026-06-29", 1)).toBe("2026-06-30");
    expect(addDays("2026-06-30", 1)).toBe("2026-07-01");
    expect(addDays("2026-01-01", -1)).toBe("2025-12-31");
    expect(addDays("2026-06-15", 30)).toBe("2026-07-15");
  });

  it("shiftMonth returns {year, month0} across year boundaries", () => {
    expect(shiftMonth(2026, 11, 1)).toEqual({ year: 2027, month: 0 }); // December → the next January
    expect(shiftMonth(2026, 0, -1)).toEqual({ year: 2025, month: 11 }); // January → the previous December
    expect(shiftMonth(2026, 5, 0)).toEqual({ year: 2026, month: 5 });
  });
});

describe("monthMatrix", () => {
  it("each week has 7 days, covers the whole month, and starts on Sunday by default", () => {
    // 2026-06 starts on a Monday (6/1 is a Monday). With weekStart=0 (Sunday), the first week runs 5/31 (Sun)..6/6 (Sat).
    const weeks = monthMatrix(2026, 5);
    expect(weeks.every((w) => w.length === 7)).toBe(true);
    expect(weeks[0][0]).toBe("2026-05-31"); // the first cell is the Sunday just before
    expect(weeks[0][1]).toBe("2026-06-01");
    const flat = weeks.flat();
    expect(flat).toContain("2026-06-30"); // the last day of the month is in there
    // contiguous (adjacent cells are one day apart)
    for (let i = 1; i < flat.length; i++) {
      expect(daysBetween(flat[i - 1], flat[i])).toBe(1);
    }
  });

  it("lead 0 when the first of the month lands exactly on the week start", () => {
    // 2026-03-01 is a Sunday, so with weeks starting on Sunday the first cell is 3/1.
    const weeks = monthMatrix(2026, 2);
    expect(weeks[0][0]).toBe("2026-03-01");
  });

  it("with weekStart=1 (Monday start) the first column is Monday", () => {
    const weeks = monthMatrix(2026, 5, 1);
    expect(weeks[0][0]).toBe("2026-06-01"); // 6/1 is a Monday
  });
});

describe("schedules", () => {
  it("scheduleDay is the day itself, or the reader's local day an instant falls on", () => {
    expect(scheduleDay(day("2026-06-21"))).toBe("2026-06-21");
    expect(scheduleDay(at(2026, 6, 21, 23, 30))).toBe("2026-06-21");
    expect(scheduleDay(at(2026, 6, 22, 0, 30))).toBe("2026-06-22");
  });

  it("scheduleText adds an instant's local time to its day", () => {
    expect(scheduleText(day("2026-08-01"))).toBe("2026-08-01");
    expect(scheduleText(at(2026, 8, 1, 9, 5))).toBe("2026-08-01 09:05");
  });

  it("compareDue orders by day, and within a day the instants first in time order, the bare day last", () => {
    const dues = [day("2026-06-21"), at(2026, 6, 21, 15), day("2026-06-20"), at(2026, 6, 21, 9), at(2026, 6, 22, 1)];
    expect([...dues].sort(compareDue)).toEqual([
      day("2026-06-20"), at(2026, 6, 21, 9), at(2026, 6, 21, 15), day("2026-06-21"), at(2026, 6, 22, 1),
    ]);
  });
});

describe("dueKind", () => {
  it("judges overdue/today/tomorrow/future relative to today", () => {
    expect(dueKind(day("2026-06-20"), "2026-06-21")).toBe("overdue");
    expect(dueKind(day("2026-06-21"), "2026-06-21")).toBe("today");
    expect(dueKind(day("2026-06-22"), "2026-06-21")).toBe("tomorrow");
    expect(dueKind(day("2026-06-23"), "2026-06-21")).toBe("future");
  });

  it("judges an instant by the reader's day it falls on, and as overdue once it has passed", () => {
    const noon = new Date(2026, 5, 21, 12).getTime();
    expect(dueKind(at(2026, 6, 21, 23), "2026-06-21", noon)).toBe("today");
    expect(dueKind(at(2026, 6, 22, 1), "2026-06-21", noon)).toBe("tomorrow");
    expect(dueKind(at(2026, 6, 21, 11), "2026-06-21", noon)).toBe("overdue");
  });

  // Tomorrow is the next calendar day, so it crosses the end of a month and the end of a year
  // rather than being the string one greater.
  it("takes tomorrow across a month and a year boundary", () => {
    expect(dueKind(day("2026-07-01"), "2026-06-30")).toBe("tomorrow");
    expect(dueKind(day("2027-01-01"), "2026-12-31")).toBe("tomorrow");
    expect(dueKind(day("2026-07-02"), "2026-06-30")).toBe("future");
  });
});

describe("groupByDue", () => {
  it("groups by due day and excludes tasks with no due", () => {
    const tasks = [task(1, "2026-06-21"), task(2, "2026-06-21"), task(3, "2026-06-22"), task(4, null)];
    const m = groupByDue(tasks);
    expect(m.get("2026-06-21")?.map((t) => t.id)).toEqual([1, 2]);
    expect(m.get("2026-06-22")?.map((t) => t.id)).toEqual([3]);
    expect(m.has("4")).toBe(false);
    expect([...m.keys()]).toHaveLength(2);
  });

  it("puts an instant on the reader's day it falls on", () => {
    const m = groupByDue([task(1, at(2026, 6, 21, 23, 30)), task(2, at(2026, 6, 22, 0, 30))]);
    expect(m.get("2026-06-21")?.map((t) => t.id)).toEqual([1]);
    expect(m.get("2026-06-22")?.map((t) => t.id)).toEqual([2]);
  });
});

describe("timelineModel", () => {
  const today = "2026-06-21";

  it("orders by ascending due and separates no-due tasks into noDue", () => {
    const tasks = [task(1, "2026-06-25", "future"), task(2, null, "none"), task(3, "2026-06-18", "past")];
    const m = timelineModel(tasks, today);
    expect(m.rows.map((r) => r.task.id)).toEqual([3, 1]);
    expect(m.noDue.map((t) => t.id)).toEqual([2]);
  });

  it("within one day puts the instants first, in time order, and the bare day last", () => {
    const tasks = [task(1, "2026-06-25"), task(2, at(2026, 6, 25, 15)), task(3, at(2026, 6, 25, 9))];
    const m = timelineModel(tasks, today);
    expect(m.rows.map((r) => r.task.id)).toEqual([3, 2, 1]);
    expect(m.rows.map((r) => r.due)).toEqual(["2026-06-25", "2026-06-25", "2026-06-25"]);
  });

  it("the axis window spans all dues and today, padded by one day on each side", () => {
    const tasks = [task(3, "2026-06-18", "past"), task(1, "2026-06-25", "future")];
    const m = timelineModel(tasks, today);
    expect(m.axisStart).toBe("2026-06-17"); // min(due, today) - 1
    expect(m.axisEnd).toBe("2026-06-26"); // max(due, today) + 1
    expect(m.spanDays).toBe(9);
    // the today line sits inside the window (0..100)
    expect(m.todayPct).toBeGreaterThan(0);
    expect(m.todayPct).toBeLessThan(100);
  });

  it("a bar spans the interval between today and due and carries a kind", () => {
    const tasks = [task(1, "2026-06-25", "future"), task(3, "2026-06-18", "past")];
    const m = timelineModel(tasks, today);
    const past = m.rows.find((r) => r.task.id === 3)!;
    const future = m.rows.find((r) => r.task.id === 1)!;
    expect(past.kind).toBe("overdue");
    expect(future.kind).toBe("future");
    // a past bar ends left of the today line; a future bar runs rightwards from it
    expect(past.leftPct).toBeLessThan(m.todayPct);
    expect(future.leftPct).toBeCloseTo(m.todayPct, 5);
    expect(future.widthPct).toBeGreaterThan(0);
  });

  it("a same-day due (a point) still has a minimum width", () => {
    const m = timelineModel([task(1, today, "now")], today);
    expect(m.rows[0].widthPct).toBeGreaterThanOrEqual(1.5);
  });

  it("with only no-due tasks, rows is empty and the axis is a minimal window centered on today", () => {
    const m = timelineModel([task(2, null, "none")], today);
    expect(m.rows).toHaveLength(0);
    expect(m.noDue).toHaveLength(1);
    expect(m.axisStart).toBe("2026-06-20");
    expect(m.axisEnd).toBe("2026-06-22");
  });
});
