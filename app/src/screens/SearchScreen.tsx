import { useState, useSyncExternalStore } from "react";
import { PriorityDot } from "../components/atoms";
import { agoLabel, errText, isPriority, isStatus, statusLabel, t, tf } from "../core/i18n";
import { parseRef } from "../core/idref";
import { SEARCH_PAGE, useSearch, type SearchFace, type SearchHit, type SearchKind } from "../core/reads";
import { asTyped } from "../core/keys";
import { getSnapshot, subscribe } from "../core/snapshot";

/**
 * The two knobs, each its own axis (`AMB-D-562`). `null` leads both: it is the arm that narrows nothing,
 * and it has to be reachable or a reader who narrowed once could never widen again.
 *
 * They are separate because they answer separate questions — which record the words are on, and which of
 * its faces — so every pairing of them is a question someone can ask. As one row they were exclusive, and
 * "a comment on a task" was the answer nobody could reach: picking the face gave up the side.
 */
const KINDS: readonly (SearchKind | null)[] = [null, "task", "decision"];
const FACES: readonly (SearchFace | null)[] = [null, "title", "body", "comment", "label", "attachment"];

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
 *
 * The narrowings around the words come in two shapes, and which shape each takes is the point. The box
 * takes an expression because the keys and values differ per side and compose (`AMB-D-563`); the project
 * takes a pull-down because it is one finite list of names, the same for both sides, and nothing about it
 * is worth spelling (`AMB-D-564`).
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
  // Two axes, held apart (`AMB-D-562`): which record the words are on, and which of its faces. Either
  // alone narrows, and both together are a product — "a comment on a task" is now a question the screen
  // can put, where one row of chips could only give up one to ask the other.
  const [kind, setKind] = useState<SearchKind | null>(null);
  const [face, setFace] = useState<SearchFace | null>(null);
  // The one narrowing that is a pull-down (`AMB-D-564`): a project is finite and named, so there is
  // nothing here for a reader to spell. `null` is every project — the arm that has to lead, since a
  // reader who scoped once must be able to widen again.
  const [projectId, setProjectId] = useState<number | null>(null);
  const [offset, setOffset] = useState(0);
  const projects = useSyncExternalStore(subscribe, () => getSnapshot().projects);

  // The narrowing is one side's vocabulary or the other's, so with no side named there is nothing to
  // read it in (`AMB-D-563`) — the box is off, and what is written in it is not part of the question.
  // Kept rather than cleared: crossing to the other side to look and back again is no reason to
  // retype it.
  const narrowable = kind !== null;
  const narrowing = narrowable ? filter : "";

  const submit = () => {
    setAsked(draft);
    setFilter(filterDraft);
    setOffset(0); // A new question starts at its first page, never wherever the last one was being read.
  };
  // Narrowing is not a new question, so it keeps the words — but it does change what the pages are cut
  // from, so it still returns to the first one. Setting one axis leaves the other where it was: that is
  // what makes the two of them compose rather than replace each other.
  const narrow = (set: () => void) => {
    set();
    setOffset(0);
  };

  const { answer, loading, error } = useSearch({
    text: asked,
    kind,
    face,
    filter: narrowing,
    projectId,
    offset,
  });
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
        {/* Off until a side is named, and saying which side it is being read in while it is on: the
            two grammars share their keys and mean different things by them, so a box that looked the
            same either way would be the screen keeping that to itself (`AMB-D-563`). */}
        <input
          {...asTyped}
          className="palette__input srch__filter"
          placeholder={narrowable ? t(`search.filterPh.${kind}`) : t("search.filterPhOff")}
          title={narrowable ? undefined : t("search.filterPhOff")}
          disabled={!narrowable}
          value={filterDraft}
          onChange={(e) => setFilterDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") submit();
          }}
        />
        <button className="btn btn--primary" onClick={submit}>{t("search.run")}</button>
        <div className="board__sep" />
        {/* The scope, and the only narrowing the screen spells out for the reader rather than taking
            as an expression (`AMB-D-564`). It sits outside the box on purpose: a project is an axis
            both sides carry, so naming one inside the narrowing would drop the decisions from the
            answer as a side effect of choosing it. */}
        <label className="faint srch__label">
          {t("search.project")}{" "}
          <select
            value={projectId ?? ""}
            onChange={(e) => narrow(() => setProjectId(e.target.value === "" ? null : Number(e.target.value)))}
          >
            <option value="">{t("search.projectAll")}</option>
            {projects.map((p) => (
              <option key={p.id} value={p.id}>{p.name}</option>
            ))}
          </select>
        </label>
        <div className="board__sep" />
        <span className="faint srch__label">{t("search.kind")}</span>
        {KINDS.map((k) => (
          <button
            key={k ?? "all"}
            className={kind === k ? "filterchip filterchip--on" : "filterchip"}
            onClick={() => narrow(() => setKind(k))}
          >
            {k === null ? t("search.kindAll") : t(`search.kind.${k}`)}
          </button>
        ))}
        <div className="board__sep" />
        <span className="faint srch__label">{t("search.faceAxis")}</span>
        {FACES.map((f) => (
          <button
            key={f ?? "all"}
            className={face === f ? "filterchip filterchip--on" : "filterchip"}
            onClick={() => narrow(() => setFace(f))}
          >
            {f === null ? t("search.faceAll") : t(`search.face.${f}`)}
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
 * The mark for the side a hit's record is on, drawn **beside** the face's rather than folded into it
 * (`AMB-D-565`). Which record and which of its faces are two questions, and one emoji answering both is
 * what left the side legible only in the ref — where a reader had to spell `AMB-T-` out to find it.
 *
 * `⚖` is the decision's mark everywhere else on the screen, so it is the decision's here too.
 */
const KIND_GLYPH = { task: "☑", decision: "⚖" } as const;

/** Which side a hit is on. The wire carries a bare string, and everything but `task` is the other side. */
function sideOf(hit: SearchHit): keyof typeof KIND_GLYPH {
  return hit.kind === "task" ? "task" : "decision";
}

/**
 * What the words were found on, as one of the four things a hit can be (`AMB-D-565`): the record itself
 * or a remark on it, on either side.
 *
 * An attachment hangs off whichever of the two it was put on, so this is also what tells "attached to the
 * record" from "attached to a remark" — the target says which thing, and the face says it is an
 * attachment.
 */
function targetKey(hit: SearchHit): string {
  const side = sideOf(hit);
  return `search.on.${hit.comment ? `${side}Comment` : side}`;
}

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
      <span className="srch__face">
        <span title={t(targetKey(hit))}>{KIND_GLYPH[sideOf(hit)]}</span>
        <span title={t(`search.face.${hit.face}`)}>{FACE_GLYPH[hit.face]}</span>
      </span>
      <div className="feed__body">
        <div className="feed__line">
          {open ? (
            <button className="feed__target srch__ref" onClick={open}>{hit.ref}</button>
          ) : (
            <span className="feed__target feed__target--gone srch__ref">{hit.ref}</span>
          )}{" "}
          {hit.title}
        </div>
        <Standing hit={hit} />
        <div className="srch__snippet"><Excerpt snippet={hit.snippet} matches={hit.matches} /></div>
        <div className="feed__meta">
          {/* The two axes in words, in the order they are asked: which thing, then where on it. A hit on
              a remark's own text needs no second word — the target already said "remark", and repeating
              it as the face is what made one word stand for two axes in the first place. */}
          <span>{t(targetKey(hit))}</span>
          {hit.face !== "comment" && <span>{t(`search.face.${hit.face}`)}</span>}
          {hit.comment && <span>{hit.comment}</span>}
          <span>{agoLabel(hit.at)}</span>
        </div>
      </div>
    </div>
  );
}

/**
 * The record's state in the words of the side it is on. The wire carries a bare string with no union left
 * on it, so `task` is what says which of the two vocabularies reads it — the same job `kind` does for
 * everything else the two sides share on a hit. A value neither dictionary has a word for is shown as it
 * came, rather than letting a key escape onto the screen.
 */
function statusWord(status: string, task: boolean): string {
  if (task) return isStatus(status) ? statusLabel(status) : status;
  const key = `dec.status.${status}`;
  const word = t(key);
  return word === key ? status : word;
}

/**
 * Where the record a hit points at stands (`AMB-D-567`), between the ref and the excerpt — where the CLI
 * puts it too, so the same reading order holds across the two surfaces.
 *
 * Each side says what it has: a task its status, its priority and what it is filed under, a decision its
 * status, which is all there is. The placements are written `axis=value`, the way the narrowing box above
 * takes them, so a row that came back on 🏷 can be typed straight back in as `dim:axis=value`.
 *
 * **The line clips rather than wraps.** A task can be filed on any number of axes, and a row that grew
 * with them would push the excerpt — what the reader came here for — down the page a hit at a time.
 *
 * Nothing is drawn at all when the read that fills this in came back empty: the words really are written
 * there, so the row stays, but a chip claiming no state would say less than saying nothing.
 */
function Standing({ hit }: { hit: SearchHit }) {
  if (!hit.standing) return null;
  const { status, priority, labels } = hit.standing;
  const task = sideOf(hit) === "task";
  return (
    <div className="srch__standing">
      <span className="chip">{statusWord(status, task)}</span>
      {priority !== undefined && isPriority(priority) && <PriorityDot priority={priority} />}
      {labels.map((l) => (
        <span key={`${l.axis}=${l.value}`} className="chip srch__filed">
          {FACE_GLYPH.label} {l.axis}={l.value}
        </span>
      ))}
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
