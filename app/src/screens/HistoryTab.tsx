// The "history" tab — the runs that are over and need nobody: completed, canceled, and failures a
// person has acknowledged, newest first (`AMB-D-955`). Opened from a project it is that project's
// alone, read so from the store a page at a time, and the rows leave the project off; opened from the
// sidebar it is every project's (`AMB-D-954`).
//
// **One page at a time, with page numbers.** The history only grows and lives in the store, so the
// screen holds one page of it and reads that page alone (`../core/automations`). A "show more" that
// kept adding rows under the last would grow the screen, and what it holds, for as long as somebody
// pressed it.
//
// **Narrowed by ending.** "All", or one of completed, failed and canceled; changing it goes back to the
// first page, because the page a reader was on counts rows of a list that is no longer there.
//
// **A row here is read, not pressed.** What it did is over, and its pane with it; the line is the same
// one the "running" tab draws (`./RunningTab`), so a run reads the same on either side of ending.
import { useState } from "react";
import { useRunHistory, type RunHistoryFilter } from "../core/automations";
import { t, tf } from "../core/i18n";
import { formatNumber } from "../core/i18n/format";
import { RunLine } from "./RunningTab";

// Spelled out rather than built from the id, so the key gate can see every label a reader can be
// shown (`core/i18n/sourceKeys.test.ts`).
const FILTERS: readonly { id: RunHistoryFilter; label: () => string }[] = [
  { id: "all", label: () => t("auto.history.all") },
  { id: "completed", label: () => t("auto.run.completed") },
  { id: "failed", label: () => t("auto.run.failed") },
  { id: "canceled", label: () => t("auto.run.canceled") },
];

/**
 * The page numbers the pager offers: the first and the last always, and two either side of the one
 * showing. A gap between them is `null` and is drawn as an ellipsis, so a long history is still one
 * row of numbers.
 */
export function pageNumbers(current: number, pages: number): (number | null)[] {
  const out: (number | null)[] = [];
  for (let n = 0; n < pages; n += 1) {
    const near = Math.abs(n - current) <= 2;
    if (n === 0 || n === pages - 1 || near) out.push(n);
    else if (out[out.length - 1] !== null) out.push(null);
  }
  return out;
}

export function HistoryTab({
  projectId,
}: {
  /** The project whose runs these are, or `null` for every project's — the sidebar's. */
  projectId: number | null;
}) {
  const [filter, setFilter] = useState<RunHistoryFilter>("all");
  const [page, setPage] = useState(0);
  const history = useRunHistory(filter, projectId, page);

  const total = history?.total ?? 0;
  const size = history?.pageSize ?? 20;
  const pages = Math.max(1, Math.ceil(total / size));
  // A page that emptied under the reader — the runs on it moved on — is walked back to the last one
  // that still has rows, rather than drawn as an empty page with a pager that says there is more.
  if (history !== null && page > pages - 1) setPage(pages - 1);
  const runs = history?.runs ?? [];
  const from = page * size;

  return (
    <>
      <div className="autohist__filter" role="group">
        {FILTERS.map((one) => (
          <button
            key={one.id}
            type="button"
            aria-pressed={filter === one.id}
            className={`autohist__only ${filter === one.id ? "autohist__only--on" : ""}`}
            onClick={() => {
              setFilter(one.id);
              setPage(0);
            }}
          >
            {one.label()}
          </button>
        ))}
      </div>

      {history !== null && runs.length === 0 && <div className="auto__empty">{t("auto.history.empty")}</div>}
      {runs.length > 0 && (
        <ul className="autoruns">
          {runs.map((run) => <RunLine key={run.run} run={run} withProject={projectId === null} />)}
        </ul>
      )}

      {/* No pager over nothing: "0–0 of 0" says less than the empty line above it already has. */}
      {total > 0 && (
        <nav className="autohist__pager">
          <button type="button" className="btn" disabled={page === 0} onClick={() => setPage(page - 1)}>
            {t("auto.history.prev")}
          </button>
          {pageNumbers(page, pages).map((n, i) =>
            n === null ? (
              <span key={`gap-${i}`} className="autohist__gap">…</span>
            ) : (
              <button
                key={n}
                type="button"
                aria-current={n === page ? "page" : undefined}
                className={`autohist__page ${n === page ? "autohist__page--on" : ""}`}
                onClick={() => setPage(n)}
              >
                {formatNumber(n + 1)}
              </button>
            ),
          )}
          <button type="button" className="btn" disabled={page >= pages - 1} onClick={() => setPage(page + 1)}>
            {t("auto.history.next")}
          </button>
          <span className="autohist__count">
            {tf("auto.history.count", {
              from: formatNumber(from + 1),
              to: formatNumber(from + runs.length),
              total: formatNumber(total),
            })}
          </span>
        </nav>
      )}
    </>
  );
}
