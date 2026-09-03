import { useState } from "react";
import { getSnapshot, type Decision, type DecisionStatus } from "../core/snapshot";
import { addDecision, fetchProjectDecisionDimensionAssignments } from "../core/mutations";
import { dataAdapter } from "../mock/adapter";
import { useDecisionPage, useDecisionSearchIds } from "../core/reads";
import { useQuery } from "../core/query";
import { axesFor } from "../core/appliesTo";
import { isTimeAxis } from "../core/timeAxis";
import { Pager, usePager } from "../components/Pager";
import { errText, formatDay, t, tf } from "../core/i18n";
import { decisionRef } from "../core/idref";
import {
  decisionFilterDimensions, parseRefQuery, passesFilters, selectionKey,
  type DimAssignments, type FilterSelection,
} from "../core/filters";
import { asTyped } from "../core/keys";
import { ErrorNote } from "../components/ErrorNote";
import { Icon } from "../components/Icon";

// The list of decision records. A decision is a first-class entity that keeps "why we went with X"
// (edited in place to refine, superseded to overturn), and it lives on a plane of its own, apart from
// tasks (which have a mailbox). The list is scoped to one project and embeds in the board as its decisions tab.
type DecisionSort = "numberDesc" | "numberAsc" | "decidedDesc" | "decidedAsc";
const SORTS: DecisionSort[] = ["numberDesc", "numberAsc", "decidedDesc", "decidedAsc"];

// Stable empty map for the instant before the assignment read comes back (and in the browser mock, where it
// stays empty). A fresh `{}` per render would rebuild the filter dimensions for nothing.
const NO_ASSIGNMENTS: DimAssignments = {};

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
 * The decision records of a single project. Only this project's decisions are fetched, and the filters
 * and the sort are layered on the client because the count is bounded. "Superseded" is not a status
 * and no badge says it — a decision holds its status and the edges its author drew, and nothing else
 * (`AMB-D-410`). It is a filter choice, and what it selects on is the edge itself: rows another decision
 * points at with `supersedes`.
 *
 * **Narrowing is the board's, not a second invention.** Status and the project's classification axes are
 * one filter panel of the same shape the board has, built from the same `core/filters` pieces: a row per
 * axis, values that pile up rather than replace, and a count on the toggle so a filter left in force out
 * of sight cannot pass for decisions that are simply gone. The assignments the classification axes judge
 * on are read in bulk, one query per axis, so a chip narrows what the screen already holds instead of
 * asking core again on every click. Only the axes that classify decisions are offered (`AMB-D-789`).
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
  const [sel, setSel] = useState<FilterSelection>({});
  // Whether the filters are open. Closed is where the tab starts, for the reason the board's are
  // (`AMB-D-654`): the values of every axis do not fit on a line, and a reader narrowing nothing
  // should be given that room for the decisions themselves.
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [sort, setSort] = useState<DecisionSort>("numberDesc");
  const [search, setSearch] = useState("");
  const [composing, setComposing] = useState(false);

  // The decisions tab is the decision side, so an axis narrowed to tasks is not one of its axes at all
  // (`AMB-D-789`).
  const projectDims = axesFor("decision", dataAdapter.getProject(projectId)?.dimensions ?? []);
  const dimIdsKey = projectDims.map((d) => d.id).join(",");
  // The assignments of every axis this side offers (decisionId→dimId→the values on it), one query each,
  // through the query cache rather than a bare effect: a value assigned elsewhere — the detail pane's
  // selects, the CLI — acks with the "decisions" scope, and that is what brings the answer back.
  const dimAssign = useQuery<DimAssignments>(
    ["decisionDimAssign", projectId, dimIdsKey],
    async () => {
      const results = await Promise.all(
        projectDims.map((d) =>
          fetchProjectDecisionDimensionAssignments(projectId, d.id).then((rows) => ({ dimId: d.id, rows })),
        ),
      );
      const m: DimAssignments = {};
      for (const { dimId, rows } of results) {
        // An axis admitting several values at once (`AMB-D-826`) answers with several rows for the one
        // decision, and they are collected rather than overwriting each other.
        for (const r of rows) ((m[r.decisionId] ??= {})[dimId] ??= []).push(r.valueId);
      }
      return m;
    },
  ).data ?? NO_ASSIGNMENTS;

  const dims = decisionFilterDimensions(projectDims, dimAssign);
  // How many axes are actually narrowing, counted over the axes that exist: a selection left behind by a
  // deleted dimension narrows nothing and must not be counted as if it did.
  const narrowedAxes = dims.filter((d) => (sel[d.id]?.length ?? 0) > 0).length;
  const q = search.trim();
  // This box searches decisions, so a number with no type code on it is a decision ref (`AMB-D-833`) —
  // the side the board reads the other way.
  const ref = parseRefQuery(search, "decision");
  // The words are asked for whether or not the query is also a ref: the pin sits on top of what they
  // match, it does not stand in for it. `null` back from the hook is "nothing was asked", which is not the
  // same as "nothing matched".
  const { hits, error: searchError } = useDecisionSearchIds(projectId, q);
  // The pinned decision is kept whatever the words did, and drawn ahead of them — it is the first row, and
  // the chosen sort orders everything below it.
  const pinnedId = ref?.space === "decision" ? ref.num : null;
  const shown = decisions
    .filter((d) => passesFilters(d, dims, sel))
    .filter((d) =>
      Number(d.id) === pinnedId || hits === null || hits.has(Number(d.id)),
    )
    .sort((a, b) => compareDecisions(a, b, sort))
    .sort((a, b) => Number(Number(b.id) === pinnedId) - Number(Number(a.id) === pinnedId));
  // Paging sits outside filtering and sorting: change the filter, the search or the sort and we return to the first page.
  const pager = usePager(shown, `${projectId}|${selectionKey(sel)}|${sort}|${q}`);
  // One value on one axis, turned on or off. Selecting is what composes the question (`AMB-D-655`), so
  // nothing here is exclusive: the values pile up within the axis, and an axis left empty narrows nothing.
  const toggleValue = (id: string, value: string) => setSel((prev) => {
    const chosen = prev[id] ?? [];
    return { ...prev, [id]: chosen.includes(value) ? chosen.filter((v) => v !== value) : [...chosen, value] };
  });

  return (
    <>
      <div className="filterbar">
        <span className="meta"><Icon name="scales" /> {t("dec.title")}</span>
        <input
          {...asTyped}
          className="board__search"
          type="search"
          placeholder={t("dec.searchPh")}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          style={{ width: 180 }}
        />
        {/* A search that could not run narrows nothing, and narrowing nothing looks exactly like a word
            that matched everything. Say which it was, next to the box that asked. */}
        {searchError != null && (
          <ErrorNote tone="quiet">
            {t("dec.searchFailed")} — {errText(searchError)}
          </ErrorNote>
        )}
        {/* The one control the filters have while they are closed, so it says how many axes are
            narrowing: a filter still in force with its values out of sight looks like decisions that
            are simply gone. */}
        <button
          className={`filtertoggle ${filtersOpen ? "filtertoggle--active" : ""}`}
          aria-expanded={filtersOpen}
          onClick={() => setFiltersOpen((open) => !open)}
        >
          <Icon name="search" /> {t("board.filters")}
          {narrowedAxes > 0 && <span className="filtertoggle__count">{narrowedAxes}</span>}
        </button>
        <label className="ctllabel">
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

      {/* The filters themselves, opened in place under the bar the toggle sits in. One line per axis,
          because a row of values does not fold onto the same line as the next axis's (`AMB-D-654`) —
          and closed, none of it takes room from the decisions. */}
      {filtersOpen && (
        <div className="filterpanel">
          {dims.map((d) => (
            <div key={d.id} className="filterpanel__axis">
              <span className="faint filterpanel__label">{d.label()}</span>
              <div className="filterpanel__values">
                {/* Each value is a switch: what a reader composes here is the set to narrow to (`AMB-D-655`). */}
                {d.options.map((o) => {
                  const on = sel[d.id]?.includes(o.value) ?? false;
                  return (
                    <button
                      key={o.value}
                      className={`filterchip ${on ? "filterchip--on" : ""}`}
                      aria-pressed={on}
                      onClick={() => toggleValue(d.id, o.value)}
                    >
                      {o.label()}
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      )}

      <div style={{ padding: 12, overflowY: "auto" }}>
        {composing && <DecisionCompose projectId={projectId} onDone={() => setComposing(false)} />}
        {shown.length === 0 && !composing && (
          <div style={{ color: "var(--c-muted)", padding: 16 }}>{t("dec.empty")}</div>
        )}
        {pager.pageItems.map((d) => (
          <DecisionCard
            key={d.id}
            d={d}
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
function DecisionCard({ d, selected, onSelect }: {
  d: Decision;
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
        border: `1px solid ${selected ? "var(--c-accent)" : "var(--c-rule)"}`,
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
      {/* The edge, said in the row: which decision overturned this one. It sits beside the status rather
          than instead of it — a rejected decision that was later superseded is both, and a badge that
          picked one of the two would be hiding the other. */}
      {d.supersededBy.length > 0 && (
        <span className="meta" style={{ whiteSpace: "nowrap" }}>
          {tf("dec.supersededByRef", { by: d.supersededBy.map((r) => r.ref ?? decisionRef(r.id)).join(", ") })}
        </span>
      )}
      <span style={{ flex: 1 }} />
      {date && <span style={{ color: "var(--c-muted)", whiteSpace: "nowrap" }}>{date}</span>}
    </div>
  );
}

// Recording a decision, and the one demand the form has to carry. A required axis is read where a
// decision is settled (`AMB-D-790`), and that press belongs to whoever accepts it — not to whoever
// wrote it. So the form asks here, while the writer is still in front of it, rather than letting the
// record go out blank and the acceptance come back refused at somebody else.
//
// Only the **required** axes draw a select, and not the time axis among them: the store puts the era
// containing today on a decision as it is recorded (`AMB-D-147`), so asking first would cost a choice
// every time and land on the same value. The rest are the detail pane's, where they can be filled in
// at leisure; putting every axis here would turn recording a decision into a form to work through.
export function DecisionCompose({ projectId, onDone }: { projectId: number; onDone: () => void }) {
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [busy, setBusy] = useState(false);
  const [values, setValues] = useState<Record<number, number>>({});

  // The decision side alone (`AMB-D-789`): an axis narrowed to tasks demands nothing of a decision, and
  // holding this button on one would ask for a value no decision can carry. The time axis comes off for
  // the opposite reason — the create fills it, so the answer is already there by the time anyone could
  // be refused for it. A project with no era over today fills nothing and asks nothing, and the
  // acceptance says so, exactly as it does on the task side.
  const project = getSnapshot().projects.find((p) => p.id === projectId);
  const demanded = axesFor("decision", project?.dimensions ?? [])
    .filter((d) => d.required && !isTimeAxis(d));
  const unmet = demanded.filter((d) => values[d.id] === undefined);

  async function submit() {
    if (!title.trim() || unmet.length > 0) return;
    setBusy(true);
    try {
      // The values ride with the create, so a decision filed under an axis never exists without it —
      // the same transaction the CLI's `--dim` writes in.
      await addDecision(projectId, title.trim(), body, demanded.map((d) => values[d.id]));
      onDone();
    } finally {
      setBusy(false);
    }
  }

  return (
    <div style={{ border: "1px solid var(--c-rule)", borderRadius: 8, padding: 12, marginBottom: 12 }}>
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
      {demanded.map((dim) => (
        <div key={dim.id} style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
          <span className="detail__flabel" style={{ whiteSpace: "nowrap" }}>{dim.name}</span>
          <select
            className="inlineselect"
            value={values[dim.id] ?? ""}
            onChange={(e) => {
              const valueId = Number(e.target.value);
              setValues((m) => {
                const n = { ...m };
                if (valueId) n[dim.id] = valueId;
                else delete n[dim.id];
                return n;
              });
            }}
          >
            <option value="">{t("detail.none")}</option>
            {/* Nothing here is carried yet — this is the create — so a closed value is simply left out
                (`AMB-D-829`): it is exactly what a record may not be newly filed under, and core would
                refuse the create it rode in on. */}
            {dim.values.filter((v) => !v.closed).map((v) => (
              <option key={v.id} value={v.id}>{v.name}</option>
            ))}
          </select>
        </div>
      ))}
      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <button
          className="feed__action"
          disabled={busy || !title.trim() || unmet.length > 0}
          onClick={() => void submit()}
        >
          {t("dec.add")}
        </button>
        <button className="feed__action" onClick={onDone}>{t("dec.cancel")}</button>
        {/* Why the button is held, named rather than left to a tooltip — the selects are right above.
            The sentence is the task pane's, because it is the same sentence: fill these in first. */}
        {unmet.length > 0 && (
          <span className="faint">
            {tf("detail.finishCreatingBlocked", { names: unmet.map((d) => d.name).join(", ") })}
          </span>
        )}
      </div>
    </div>
  );
}
