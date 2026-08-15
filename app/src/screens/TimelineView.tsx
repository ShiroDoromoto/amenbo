import type { TaskCard } from "../mock/types";
import { timelineModel, todayStr, relativeDays } from "../core/calendar";
import { t, tf } from "../core/i18n";
import { BlockedChips, PriorityDot } from "../components/atoms";
import { isClosed } from "../core/status";
import { Icon } from "../components/Icon";

// Cap on the chips laid out in the no-due bucket (kept in step with the calendar).
const NODUE_CAP = 24;

/**
 * Timeline (Gantt) view: every task that carries a `due` is laid out on one shared time axis. A bar
 * spans the interval between today and the due date — a future due extends right from the today
 * line (time left), a past due extends left from the due date back to it (overdue, red) — and a
 * done task gets the completed colour, so which deadlines are near or blown, and how far the work
 * has come, read at a glance. Tasks with no due cannot sit on the axis and go to the bucket below.
 * The layout arithmetic lives in core/calendar's timelineModel (a pure function).
 */
export function TimelineView({ tasks, selectedTaskId, onSelectTask }: {
  tasks: TaskCard[];
  selectedTaskId: number | null;
  onSelectTask: (id: number) => void;
}) {
  const today = todayStr();
  const m = timelineModel(tasks, today);

  if (m.rows.length === 0 && m.noDue.length === 0) {
    return (
      <div className="placeholder">
        <Icon name="calendar" size="lg" />
        <div>{t("cal.empty")}</div>
      </div>
    );
  }

  const relLabel = (due: string): string => {
    const d = relativeDays(due, today);
    if (d === 0) return t("cal.today");
    return d < 0 ? tf("cal.overdueDays", { n: -d }) : tf("cal.inDays", { n: d });
  };

  return (
    <div className="tl">
      {m.rows.length > 0 && (
        <>
          <div className="tl__axis">
            <span className="tl__axispad" />
            <span className="tl__ruler">
              <span className="tl__ruler-end tl__ruler-start">{m.axisStart}</span>
              <span className="tl__ruler-today" style={{ left: `${m.todayPct}%` }}>{t("cal.today")}</span>
              <span className="tl__ruler-end">{m.axisEnd}</span>
            </span>
            <span className="tl__axispad" />
          </div>

          <div className="tl__rows">
            {m.rows.map((r) => (
              <button
                key={r.task.id}
                className={`tl__row ${r.task.id === selectedTaskId ? "tl__row--selected" : ""}`.trim()}
                onClick={() => onSelectTask(r.task.id)}
                title={r.task.title}
                data-pane-select
              >
                <span className="tl__label">
                  <PriorityDot priority={r.task.priority} />
                  <BlockedChips task={r.task} compact />
                  <span className="tl__title">{r.task.title}</span>
                </span>
                <span className="tl__track">
                  <span className="tl__todayline" style={{ left: `${m.todayPct}%` }} />
                  <span
                    className={[
                      "tl__bar",
                      `tl__bar--${r.kind}`,
                      isClosed(r.task.status) ? "tl__bar--closed" : "",
                    ].join(" ").trim()}
                    style={{ left: `${r.leftPct}%`, width: `${r.widthPct}%` }}
                  />
                </span>
                <span className="tl__rel">{relLabel(r.due)}</span>
              </button>
            ))}
          </div>
        </>
      )}

      {m.noDue.length > 0 && (
        <div className="cal__nodue">
          <span className="cal__nodue-label">{tf("cal.noDue", { n: m.noDue.length })}</span>
          {m.noDue.slice(0, NODUE_CAP).map((tk) => (
            <button
              key={tk.id}
              className={`cal__chip ${tk.id === selectedTaskId ? "cal__chip--selected" : ""}`.trim()}
              title={tk.title}
              onClick={() => onSelectTask(tk.id)}
            >
              <BlockedChips task={tk} compact />
              {tk.title}
            </button>
          ))}
          {m.noDue.length > NODUE_CAP && (
            <span className="cal__more">{tf("cal.more", { n: m.noDue.length - NODUE_CAP })}</span>
          )}
        </div>
      )}
    </div>
  );
}
