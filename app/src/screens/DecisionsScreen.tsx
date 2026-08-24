import { useState } from "react";
import { type Decision, type DecisionStatus } from "../core/snapshot";
import { addDecision } from "../core/mutations";
import { useDecisionPage, useDecisionSearchIds } from "../core/reads";
import { useAdrift } from "../core/adrift";
import { Pager, usePager } from "../components/Pager";
import { errText, formatDay, t, tf } from "../core/i18n";
import { decisionRef } from "../core/idref";
import { parseRefQuery } from "../core/filters";
import { asTyped } from "../core/keys";
import { ErrorNote } from "../components/ErrorNote";
import { Icon } from "../components/Icon";
import { AdriftChip } from "../components/atoms";

// The list of decision records. A decision is a first-class entity that keeps "why we went with X"
// (edited in place to refine, superseded to overturn), and it lives on a plane of its own, apart from
// tasks (which have a mailbox). The list is scoped to one project and embeds in the board as its decisions tab.
type DecisionSort = "numberDesc" | "numberAsc" | "decidedDesc" | "decidedAsc";
const SORTS: DecisionSort[] = ["numberDesc", "numberAsc", "decidedDesc", "decidedAsc"];

// The sort comparison. The number is the id itself; the decision date is decidedAt, or createdAt where there is none.
function compareDecisions(a: Decision, b: Decision, sort: DecisionSort): number {
  switch (sort) {
    case "numberDesc":
    case "numberAsc": {
      const an = Number(a.id), bn = Number(b.id);
      return sort === "numberDesc" ? bn - an : an - bn;
    }
    case "decidedDesc":
    case "decidedAsc": {
      const at = a.decidedAt ?? a.createdAt;
      const bt = b.decidedAt ?? b.createdAt;
      const cmp = at < bt ? -1 : at > bt ? 1 : 0;
      return sort === "decidedDesc" ? -cmp : cmp;
    }
  }
}

/**
 * The decision records of a single project. Only this project's decisions are fetched, and the status
 * filter and the sort are layered on the client because the count is bounded. "Superseded" is not a status
 * and no badge says it — a decision holds its status and the edges its author drew, and nothing else
 * (`AMB-D-410`). It is a filter choice, and what it selects on is the edge itself: rows another decision
 * points at with `supersedes`.
 *
 * **The search is core's, not the client's.** It reaches title, body and any live comment body — the third
 * of those being why it cannot be a substring match over the page: comments are not on the page payload,
 * and fetching every thread to look through them is what the bounded page exists to avoid. So the typed
 * text goes to `decision_search`, the same match the CLI's `search` runs, and comes back as the ids to
 * narrow to. Recognise a decision ref (`AMB-D-n`, or the bare `D-n`) and it narrows to that number instead,
 * without asking core at all (a task number lives in another space, so on this plane it matches nothing).
 */
export function DecisionsScreen({ projectId, selectedDecisionId, onSelectDecision }: {
  projectId: number;
  selectedDecisionId?: number | null;
  onSelectDecision?: (id: number) => void;
}) {
  const decisions = useDecisionPage(projectId);
  // The proposals nothing is bringing to a close. Asked for the same project the board asks for, which is the
  // same query and so the same one answer (`core/reads`'s `useAdrift`).
  const adrift = useAdrift(projectId);
  const [filter, setFilter] = useState<DecisionFilterKey>("all");
  const [sort, setSort] = useState<DecisionSort>("numberDesc");
  const [search, setSearch] = useState("");
  const [composing, setComposing] = useState(false);

  const FILTERS: Exclude<DecisionFilterKey, "all">[] = ["proposed", "accepted", "rejected", "superseded"];
  const q = search.trim();
  const ref = parseRefQuery(search);
  // A ref query is answered here, so it never becomes a text search: `D-12` is a number, not a word to look
  // for. `null` back from the hook is "nothing was asked", which is not the same as "nothing matched".
  const { hits, error: searchError } = useDecisionSearchIds(projectId, ref ? "" : q);
  const shown = decisions
    .filter((d) => filter === "all" || (filter === "superseded" ? d.supersededBy.length > 0 : d.status === filter))
    .filter((d) =>
      ref ? ref.space === "decision" && Number(d.id) === ref.num : hits === null || hits.has(Number(d.id)),
    )
    .sort((a, b) => compareDecisions(a, b, sort));
  // Paging sits outside filtering and sorting: change the filter, the search or the sort and we return to the first page.
  const pager = usePager(shown, `${projectId}|${filter}|${sort}|${q}`);

  return (
    <>
      <div className="filterbar">
        <span className="faint" style={{ fontSize: "var(--fs-xs)" }}><Icon name="scales" /> {t("dec.title")}</span>
        <input
          {...asTyped}
          className="board__search"
          type="search"
          placeholder={t("dec.searchPh")}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          style={{ fontSize: "var(--fs-xs)", width: 180 }}
        />
        {/* A search that could not run narrows nothing, and narrowing nothing looks exactly like a word
            that matched everything. Say which it was, next to the box that asked. */}
        {searchError != null && (
          <ErrorNote tone="quiet">
            {t("dec.searchFailed")} — {errText(searchError)}
          </ErrorNote>
        )}
        <label style={{ fontSize: "var(--fs-xs)" }}>
          {t("board.filter")}{" "}
          <select value={filter} onChange={(e) => setFilter(e.target.value as DecisionFilterKey)}>
            <option value="all">{t("dec.filterAll")}</option>
            {/* Every arm but one names a status; "superseded" names an edge, and takes a label of its
                own so the status namespace is not made to hold something that is not a status. */}
            {FILTERS.map((s) => (
              <option key={s} value={s}>{t(s === "superseded" ? "dec.filterSuperseded" : `dec.status.${s}`)}</option>
            ))}
          </select>
        </label>
        <label style={{ fontSize: "var(--fs-xs)" }}>
          {t("dec.sort")}{" "}
          <select value={sort} onChange={(e) => setSort(e.target.value as DecisionSort)}>
            {SORTS.map((s) => (
              <option key={s} value={s}>{t(`dec.sort.${s}`)}</option>
            ))}
          </select>
        </label>
        <span className="topbar__spacer" style={{ flex: 1 }} />
        <button className="feed__action" onClick={() => setComposing((v) => !v)}><Icon name="plus" /> {t("dec.new")}</button>
      </div>

      <div style={{ padding: 12, overflowY: "auto" }}>
        {composing && <DecisionCompose projectId={projectId} onDone={() => setComposing(false)} />}
        {shown.length === 0 && !composing && (
          <div style={{ color: "var(--c-muted)", padding: 16 }}>{t("dec.empty")}</div>
        )}
        {pager.pageItems.map((d) => (
          <DecisionCard
            key={d.id}
            d={d}
            adrift={adrift.decisions.has(Number(d.id))}
            selected={d.id === selectedDecisionId}
            onSelect={onSelectDecision}
          />
        ))}
        <Pager
          page={pager.page}
          pageCount={pager.pageCount}
          total={pager.total}
          start={pager.start}
          pageSize={pager.pageSize}
          onPage={pager.setPage}
        />
      </div>
    </>
  );
}

/// The filter choices: the three statuses, plus the derived "superseded".
type DecisionFilterKey = DecisionStatus | "all" | "superseded";

function statusColor(s: DecisionStatus): string {
  switch (s) {
    case "accepted": return "#2e9e6b";
    case "proposed": return "#b88600";
    case "rejected": return "#c0504d";
  }
}

// Format the decision date (decidedAt, else createdAt) as a calendar date, in the locale dates are
// written in (`dateLocale`). An invalid value formats as empty.
function decidedLabel(d: Decision): string {
  return formatDay(new Date(d.decidedAt ?? d.createdAt));
}

// The list is a compact overview: a row carries the ref, the title, the status and the decision date,
// and nothing else. The body, the supersession chain, the related tasks and accept/reject are the
// detail pane's business (DecisionDetailPane).
function DecisionCard({ d, adrift, selected, onSelect }: {
  d: Decision;
  /** Whether the pane that put it up has gone and nobody has settled it since. */
  adrift?: boolean;
  selected?: boolean;
  onSelect?: (id: number) => void;
}) {
  const date = decidedLabel(d);
  return (
    <div
      onClick={onSelect ? () => onSelect(d.id) : undefined}
      data-pane-select
      style={{
        display: "flex", alignItems: "baseline", gap: 8,
        border: `1px solid ${selected ? "var(--c-accent)" : "var(--c-border)"}`,
        background: selected ? "var(--c-accent-weak)" : undefined,
        borderRadius: 8, padding: "6px 10px", marginBottom: 6,
        cursor: onSelect ? "pointer" : undefined,
      }}
    >
      {d.ref && <span style={{ color: "var(--c-muted)", fontVariantNumeric: "tabular-nums" }}>{d.ref}</span>}
      <span style={{ fontWeight: 600, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{d.title}</span>
      <span style={{
        fontSize: "var(--fs-xs)", padding: "1px 8px", borderRadius: 10, color: "#fff", whiteSpace: "nowrap",
        background: statusColor(d.status),
      }}>{t(`dec.status.${d.status}`)}</span>
      {/* Beside the status rather than instead of it: `proposed` says it is open, and this says nothing is
          bringing it to a close — the second is only readable against the first. */}
      {adrift && <AdriftChip />}
      {/* The edge, said in the row: which decision overturned this one. It sits beside the status rather
          than instead of it — a rejected decision that was later superseded is both, and a badge that
          picked one of the two would be hiding the other. */}
      {d.supersededBy.length > 0 && (
        <span className="faint" style={{ fontSize: "var(--fs-xs)", whiteSpace: "nowrap" }}>
          {tf("dec.supersededByRef", { by: d.supersededBy.map((r) => r.ref ?? decisionRef(r.id)).join(", ") })}
        </span>
      )}
      <span style={{ flex: 1 }} />
      {date && <span style={{ fontSize: "var(--fs-sm)", color: "var(--c-muted)", whiteSpace: "nowrap" }}>{date}</span>}
    </div>
  );
}

function DecisionCompose({ projectId, onDone }: { projectId: number; onDone: () => void }) {
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [busy, setBusy] = useState(false);

  async function submit() {
    if (!title.trim()) return;
    setBusy(true);
    try {
      await addDecision(projectId, title.trim(), body);
      onDone();
    } finally {
      setBusy(false);
    }
  }

  return (
    <div style={{ border: "1px solid var(--c-border)", borderRadius: 8, padding: 12, marginBottom: 12 }}>
      <input
        {...asTyped}
        style={{ width: "100%", marginBottom: 8 }}
        placeholder={t("dec.newTitlePh")}
        value={title}
        onChange={(e) => setTitle(e.target.value)}
      />
      <textarea
        {...asTyped}
        style={{ width: "100%", minHeight: 80, marginBottom: 8 }}
        placeholder={t("dec.newBodyPh")}
        value={body}
        onChange={(e) => setBody(e.target.value)}
      />
      <div style={{ display: "flex", gap: 8 }}>
        <button className="feed__action" disabled={busy || !title.trim()} onClick={() => void submit()}>
          {t("dec.add")}
        </button>
        <button className="feed__action" onClick={onDone}>{t("dec.cancel")}</button>
      </div>
    </div>
  );
}
