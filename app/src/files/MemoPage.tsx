// The project's draft page: the one place in Amenbo a person writes that is not a record
// (`AMB-T-3608`).
//
// **It stays and it does not grow.** One page per project, plain text, and only ever the text on it
// now: no versions, no second page, nothing drawn from it. A long request is put together here and
// then sent; what is worth keeping moves to a task or a decision, and the draft has done its job.
//
// **What is typed is kept without being asked for.** A draft one has to remember to keep is a draft
// that gets lost, and this is the workbench rather than the work. Writing settles after a moment's
// quiet — the same reason a watch debounces — and the last of it is written on the way out, because
// a person who closes the window mid-sentence meant to keep the sentence.
//
// **The keeping is shown rather than explained.** Nothing here says what to write or when to move
// it — the page is the person's, not a place Amenbo instructs from — so the only thing it says is
// what it is doing with what has been typed. A ring fills for as long as the settle lasts and closes
// on the write, which is the wait drawn at its own length: watching it twice is the whole of the
// explanation. The word beside it carries the state the ring has stopped moving to say, and it is
// not taken away afterwards, because the quiet after typing is exactly when a person looks to see
// whether anything was kept (`AMB-T-3684`).
//
// **The page has two widths.** The panel is a column beside a terminal, and a paragraph typed in a
// column is read a few words at a time. The wide one is the same text in the middle of the window,
// stopped at a width a paragraph is read at.
import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { t } from "../core/i18n";
import { asTyped } from "../core/keys";
import { projectMemo, setProjectMemo } from "./memo";

/** How long the typing has to settle before it is written. */
const SETTLE_MS = 600;

/** Where the writing stands: nothing typed yet, typing in hand, or written. */
type Keep = "none" | "typing" | "kept";

export function MemoPage({ projectId }: { projectId: number }) {
  const [text, setText] = useState("");
  const [wide, setWide] = useState(false);
  const [keep, setKeep] = useState<Keep>("none");
  // A CSS animation starts on an element that is new and not on one that has re-rendered, so every
  // keystroke hands the ring a fresh key and it fills from nothing again.
  const [fills, setFills] = useState(0);
  // What has been typed but not yet written, so the way out can write it whatever the timer was
  // about to do.
  const pending = useRef<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const write = useCallback(() => {
    const unsaved = pending.current;
    pending.current = null;
    clearTimeout(timer.current);
    if (unsaved !== null) void setProjectMemo(projectId, unsaved).catch(() => {});
  }, [projectId]);

  useEffect(() => {
    let alive = true;
    setText("");
    setKeep("none");
    void projectMemo(projectId).then((kept) => { if (alive) setText(kept); }).catch(() => {});
    // The page going away — the face coming down, the project changing — writes what is in hand.
    return () => {
      alive = false;
      write();
    };
  }, [projectId, write]);

  const typed = (value: string) => {
    setText(value);
    pending.current = value;
    setKeep("typing");
    setFills((n) => n + 1);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => { write(); setKeep("kept"); }, SETTLE_MS);
  };

  const field = (
    <textarea
      {...asTyped}
      className={wide ? "memo__field memo__field--wide" : "memo__field"}
      value={text}
      aria-label={t("files.memo")}
      onChange={(e) => typed(e.target.value)}
      // Nothing here submits: there is nothing to submit to. Escape closes the wide page, which is
      // the one thing this can be asked to do.
      onKeyDown={(e) => { if (e.key === "Escape" && wide) setWide(false); }}
    />
  );

  /** The head of a face: what the page is, how the writing stands, and the way to the other one. */
  const bar = (shown: Keep, toWide: boolean) => (
    <div className="memo__bar">
      <h3 className="files__head">{t("files.memo")}</h3>
      <span className={shown === "none" ? "memo__keep" : `memo__keep memo__keep--on memo__keep--${shown}`}>
        <span
          key={fills}
          className="memo__ring"
          style={{ "--memo-settle": `${SETTLE_MS}ms` } as CSSProperties}
        />
        {/* The state is said as well as drawn: a ring is nothing to a reader being read to. */}
        <span className="memo__word" aria-live="polite">
          {shown === "none" ? "" : t(shown === "typing" ? "files.memoTyping" : "files.memoKept")}
        </span>
      </span>
      <button
        className="files__back"
        // The wide page opens on a blank mark. It is a face just arrived at, and whatever the ring
        // was in the middle of saying, it was saying to the panel.
        onClick={() => { if (toWide) setKeep("none"); setWide(toWide); }}
      >
        {t(toWide ? "files.memoWide" : "files.memoNarrow")}
      </button>
    </div>
  );

  return (
    <div className="files__row memo">
      {bar(wide ? "none" : keep, true)}
      {/* One field at a time: the wide page is the same text in the middle of the window, not a
          second copy of it beside the first. */}
      {!wide && field}
      {wide && createPortal(
        <div
          className="modal__overlay"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => { e.stopPropagation(); if (e.target === e.currentTarget) setWide(false); }}
        >
          <div className="memo__page" role="dialog" aria-modal="true" aria-label={t("files.memo")}>
            {bar(keep, false)}
            {field}
          </div>
        </div>,
        document.body,
      )}
    </div>
  );
}
