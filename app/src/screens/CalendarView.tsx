import { useState } from "react";
import type { TaskCard } from "../mock/types";
import {
  monthMatrix, groupByDue, todayStr, shiftMonth, dueKind, parseYmd,
} from "../core/calendar";
import { monthLabel, t, tf, weekdayLabels } from "../core/i18n";
import { BlockedChips } from "../components/atoms";
import { isClosed } from "../core/status";

// How many tasks a single day cell will stack; the overflow becomes an "N more" line pointing at the
// list/detail view. Keeps a month row from growing without bound.
const DAY_CAP = 3;
// Cap on the chips laid out in the no-due bucket, so a backlog that is mostly undated stays cheap in the DOM.
const NODUE_CAP = 24;

/**
 * The month-grid calendar view. Tasks with a `due` land in that day's cell; the view can page
 * between months, jump to today, and highlights today's cell. Tasks with no due date go to a
 * dedicated bucket under the grid, since the month grid has nowhere to put them. The caller has
 * already applied the project/filter selection to `tasks`.
 */
export function CalendarView({ tasks, selectedTaskId, onSelectTask }: {
  tasks: TaskCard[];
  selectedTaskId: number | null;
  onSelectTask: (id: number) => void;
}) {
  const today = todayStr();
  const [ty, tm] = parseYmd(today); // tm is 1..12
  // The month on screen (month is 0..11), starting on today's.
  const [{ year, month }, setYM] = useState({ year: ty, month: tm - 1 });

  const weeks = monthMatrix(year, month);
  const byDue = groupByDue(tasks);
  const noDue = tasks.filter((tk) => !tk.due);
  const weekdays = weekdayLabels();

  const go = (delta: number) => setYM((s) => shiftMonth(s.year, s.month, delta));
  const goToday = () => setYM({ year: ty, month: tm - 1 });

  return (
    <div className="cal">
      <div className="cal__head">
        <button className="cal__nav" onClick={() => go(-1)} title={t("cal.prevMonth")}>◀</button>
        <span className="cal__month">{monthLabel(year, month)}</span>
        <button className="cal__nav" onClick={() => go(1)} title={t("cal.nextMonth")}>▶</button>
        <button className="cal__todaybtn" onClick={goToday}>{t("cal.today")}</button>
      </div>

      <div className="cal__weekdays">
        {weekdays.map((w) => <div key={w} className="cal__weekday">{w}</div>)}
      </div>

      <div className="cal__grid">
        {weeks.flat().map((day) => {
          const [, mo, dom] = parseYmd(day);
          const inMonth = mo === month + 1;
          const dayTasks = byDue.get(day) ?? [];
          const shown = dayTasks.slice(0, DAY_CAP);
          const extra = dayTasks.length - shown.length;
          return (
            <div
              key={day}
              className={[
                "cal__cell",
                inMonth ? "" : "cal__cell--out",
                day === today ? "cal__cell--today" : "",
              ].join(" ").trim()}
            >
              <div className="cal__date">{dom}</div>
              {shown.map((tk) => (
                <button
                  key={tk.id}
                  className={[
                    "cal__chip",
                    `cal__chip--${dueKind(day, today)}`,
                    tk.id === selectedTaskId ? "cal__chip--selected" : "",
                    isClosed(tk.status) ? "cal__chip--closed" : "",
                  ].join(" ").trim()}
                  title={tk.title}
                  onClick={() => onSelectTask(tk.id)}
                  data-pane-select
                >
                  <BlockedChips task={tk} compact />
                  {tk.title}
                </button>
              ))}
              {extra > 0 && <div className="cal__more">{tf("cal.more", { n: extra })}</div>}
            </div>
          );
        })}
      </div>

      {noDue.length > 0 && (
        <div className="cal__nodue">
          <span className="cal__nodue-label">{tf("cal.noDue", { n: noDue.length })}</span>
          {noDue.slice(0, NODUE_CAP).map((tk) => (
            <button
              key={tk.id}
              className={`cal__chip ${tk.id === selectedTaskId ? "cal__chip--selected" : ""}`.trim()}
              title={tk.title}
              onClick={() => onSelectTask(tk.id)}
              data-pane-select
            >
              <BlockedChips task={tk} compact />
              {tk.title}
            </button>
          ))}
          {noDue.length > NODUE_CAP && (
            <span className="cal__more">{tf("cal.more", { n: noDue.length - NODUE_CAP })}</span>
          )}
        </div>
      )}
    </div>
  );
}
