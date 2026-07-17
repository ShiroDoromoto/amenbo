// Paging for the flat lists (the smart views in ListScreen, and a board with view=list) keeps the rendered DOM down to
// one page: a stable position, predictable memory, and the ability to jump. The ever-growing Done and All-tasks lists
// are read through this pager too.
import { useEffect, useState } from "react";
import { tf } from "../core/i18n";

/** Rows per page. The small views (today, inbox) effectively fit on a single one. */
export const PAGE_SIZE = 50;

/**
 * A hook that windows an array down to one page. A change in `resetKey` (the view id, the filter, …) returns it to the
 * first page, and if the list shrinks — under a filter, say — until the page falls out of range, the page is clamped.
 */
export function usePager<T>(items: T[], resetKey: string, pageSize = PAGE_SIZE) {
  const [page, setPage] = useState(0);
  // Back to the first page whenever the view or the filter changes.
  useEffect(() => { setPage(0); }, [resetKey]);

  const pageCount = Math.max(1, Math.ceil(items.length / pageSize));
  const clamped = Math.min(page, pageCount - 1);
  // Once out of range, pull the state back to the real value (it lands on the next render).
  useEffect(() => { if (page !== clamped) setPage(clamped); }, [page, clamped]);

  const start = clamped * pageSize;
  return {
    pageItems: items.slice(start, start + pageSize),
    page: clamped,
    pageCount,
    total: items.length,
    pageSize,
    start,
    setPage,
  };
}

/** The pager controls (first / prev / range / next / last). Draws nothing when everything fits on one page. */
export function Pager({ page, pageCount, total, start, pageSize, onPage }: {
  page: number;
  pageCount: number;
  total: number;
  start: number;
  pageSize: number;
  onPage: (p: number) => void;
}) {
  if (pageCount <= 1) return null;
  const from = start + 1;
  const to = Math.min(start + pageSize, total);
  return (
    <div className="pager">
      <button className="btn" disabled={page === 0} onClick={() => onPage(0)} aria-label="first">«</button>
      <button className="btn" disabled={page === 0} onClick={() => onPage(page - 1)} aria-label="prev">‹</button>
      <span className="pager__info">
        {tf("pager.range", { from, to, total })} · {tf("pager.page", { page: page + 1, pages: pageCount })}
      </span>
      <button className="btn" disabled={page >= pageCount - 1} onClick={() => onPage(page + 1)} aria-label="next">›</button>
      <button className="btn" disabled={page >= pageCount - 1} onClick={() => onPage(pageCount - 1)} aria-label="last">»</button>
    </div>
  );
}
