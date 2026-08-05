import { useState } from "react";
import { agoLabel, errText, t, tf } from "../core/i18n";
import { parseRef } from "../core/idref";
import { SEARCH_PAGE, useSearch, type SearchFace, type SearchHit, type SearchKind } from "../core/reads";
import { asTyped } from "../core/keys";

/**
 * What the chip row can be set to. Two of them name a kind, one names a face and one names neither, which
 * is exactly the mixing `AMB-D-562` takes apart underneath — the row is still the one control it was, and
 * drawing the two axes as two of them is its own piece of work.
 */
type Chip = SearchKind | "comment" | null;
const CHIPS: readonly Chip[] = [null, "task", "decision", "comment"];

/**
 * The cross-cutting search (`AMB-D-449`): where a word is written, across tasks, decisions and the
 * comments on either.
 *
 * It is a screen and not a filter on one, because what it answers is not a list. A board's search box
 * narrows the rows in front of you and returns ids; this returns **places** — a face, the record it
 * belongs to, and a short excerpt pointing at it — and a place has nowhere to land in a column of task
 * cards. The excerpt is a pointer and not the reading: pressing the ref opens the record, which is what
 * holds the whole of it.
 *
 * The words are submitted rather than searched per keystroke. A page of hits is drawn from every record
 * in reach, the narrowing box takes an expression that can fail to parse, and neither wants to be re-run
 * mid-word — a half-typed `status:t` would answer a search with an error message.
 */
export function SearchScreen({
  onOpenTask,
  onOpenDecision,
}: {
  onOpenTask: (id: number) => void;
  onOpenDecision: (id: number) => void;
}) {
  // What is being typed, and what was asked. They differ between a keystroke and Enter, which is the
  // whole point: `asked` is what the query key is built from, so typing costs no reads.
  const [draft, setDraft] = useState("");
  const [asked, setAsked] = useState("");
  const [filterDraft, setFilterDraft] = useState("");
  const [filter, setFilter] = useState("");
  // One row of chips, but no longer one axis: "comment" is a face and the two named ones are kinds
  // (`AMB-D-562`). What is held is therefore which chip is on, and the two axes are derived from it.
  const [chip, setChip] = useState<Chip>(null);
  const kind: SearchKind | null = chip === "comment" ? null : chip;
  const face: SearchFace | null = chip === "comment" ? "comment" : null;
  const [offset, setOffset] = useState(0);

  const submit = () => {
    setAsked(draft);
    setFilter(filterDraft);
    setOffset(0); // A new question starts at its first page, never wherever the last one was being read.
  };
  // Narrowing is not a new question, so it keeps the words — but it does change what the pages are cut
  // from, so it still returns to the first one.
  const narrow = (c: Chip) => {
    setChip(c);
    setOffset(0);
  };

  const { answer, loading, error } = useSearch({ text: asked, kind, face, filter, offset });
  const total = answer?.totalMatched ?? 0;
  const shown = answer?.hits.length ?? 0;

  return (
    <>
      <div className="board__toolbar board__toolbar--search">
        <input
          {...asTyped}
          className="palette__input srch__input"
          placeholder={t("search.placeholder")}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") submit();
          }}
        />
        <input
          {...asTyped}
          className="palette__input srch__filter"
          placeholder={t("search.filterPh")}
          value={filterDraft}
          onChange={(e) => setFilterDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") submit();
          }}
        />
        <button className="btn btn--primary" onClick={submit}>{t("search.run")}</button>
        <div className="board__sep" />
        <span className="faint srch__label">{t("search.kind")}</span>
        {CHIPS.map((c) => (
          <button
            key={c ?? "all"}
            className={chip === c ? "filterchip filterchip--on" : "filterchip"}
            onClick={() => narrow(c)}
          >
            {c === null ? t("search.kindAll") : t(`search.kind.${c}`)}
          </button>
        ))}
        <div className="topbar__spacer" />
        <span className="faint srch__note">{t("search.note")}</span>
      </div>

      <div className="feed feed--virtual">
        {/* A search that could not run is said out loud. Left to fall through as an empty page it would
            wear the face of a word nothing is written with, and the next failure here would be as quiet
            as the first — an unparsable narrowing expression above all. */}
        {error != null && <div className="feed__item srch__error">{t("search.failed")} {errText(error)}</div>}
        {asked.trim() === "" ? (
          <div className="feed__item faint">{t("search.idle")}</div>
        ) : loading && answer === null ? (
          <div className="feed__item faint">{t("app.loading")}</div>
        ) : error == null && total === 0 ? (
          <div className="feed__item faint">{t("search.empty")}</div>
        ) : (
          answer?.hits.map((hit, i) => (
            <HitRow
              key={`${hit.ref}:${hit.comment ?? ""}:${hit.face}:${i}`}
              hit={hit}
              onOpenTask={onOpenTask}
              onOpenDecision={onOpenDecision}
            />
          ))
        )}
      </div>

      {total > 0 && (
        <div className="pager">
          <button className="btn" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - SEARCH_PAGE))} aria-label="prev">‹</button>
          <span className="pager__info">
            {tf("pager.range", { from: shown === 0 ? 0 : offset + 1, to: offset + shown, total })}
          </span>
          <button className="btn" disabled={offset + shown >= total} onClick={() => setOffset(offset + SEARCH_PAGE)} aria-label="next">›</button>
        </div>
      )}
    </>
  );
}

/** The emoji beside a face. Decoration that does not depend on the language, so it stays out of the dictionary. */
const FACE_GLYPH: Record<SearchFace, string> = {
  title: "📌",
  body: "📄",
  comment: "💬",
  label: "🏷",
  attachment: "📎",
};

/**
 * One place the words are written. The ref reads first because it is what the reader opens next; the
 * excerpt sits under it, and a comment ref says which remark when the hit is not on the record's own
 * faces.
 */
function HitRow({
  hit,
  onOpenTask,
  onOpenDecision,
}: {
  hit: SearchHit;
  onOpenTask: (id: number) => void;
  onOpenDecision: (id: number) => void;
}) {
  // The ref is the only handle a hit carries — it holds no id — and reading the number back out of it is
  // what the ref spelling is for (`core/idref`).
  const target = parseRef(hit.ref);
  const open = target
    ? () => (target.space === "task" ? onOpenTask(target.num) : onOpenDecision(target.num))
    : undefined;
  return (
    <div className="feed__item">
      <span className="srch__face" title={t(`search.face.${hit.face}`)}>{FACE_GLYPH[hit.face]}</span>
      <div className="feed__body">
        <div className="feed__line">
          {open ? (
            <button className="feed__target srch__ref" onClick={open}>{hit.ref}</button>
          ) : (
            <span className="feed__target feed__target--gone srch__ref">{hit.ref}</span>
          )}{" "}
          {hit.title}
        </div>
        <div className="srch__snippet"><Excerpt snippet={hit.snippet} matches={hit.matches} /></div>
        <div className="feed__meta">
          <span>{t(`search.face.${hit.face}`)}</span>
          {hit.comment && <span>{hit.comment}</span>}
          <span>{agoLabel(hit.at)}</span>
        </div>
      </div>
    </div>
  );
}

/**
 * The excerpt, with the runs the words landed on marked.
 *
 * **The ranges are taken as given and never re-derived here** (`AMB-D-566`). Deciding which characters a
 * term matches takes the folding the index already applied once (NFKC, case, kana), and a screen matching
 * again for itself would be a second answer to what a word matches — so the core says where, and this only
 * slices. `<mark>` is the element for it: relevance to what the reader asked, which is exactly what a hit
 * is.
 *
 * The positions are counted in **characters**, so the split is `Array.from` and not `snippet[i]` — an
 * excerpt is a person's prose, and indexing would cut a surrogate pair in half and draw the wreckage.
 *
 * A range that does not fit the excerpt is skipped rather than trusted: this is display, and a bad pair
 * should cost the emphasis, never a character of the text.
 */
function Excerpt({ snippet, matches }: { snippet: string; matches: SearchHit["matches"] }) {
  if (matches.length === 0) return <>{snippet}</>;
  const chars = Array.from(snippet);
  const parts: React.ReactNode[] = [];
  let at = 0;
  matches.forEach((m, i) => {
    const start = Math.min(Math.max(m.start, at), chars.length);
    const end = Math.min(m.end, chars.length);
    if (end <= start) return;
    if (start > at) parts.push(chars.slice(at, start).join(""));
    parts.push(<mark key={i}>{chars.slice(start, end).join("")}</mark>);
    at = end;
  });
  if (at < chars.length) parts.push(chars.slice(at).join(""));
  return <>{parts}</>;
}
