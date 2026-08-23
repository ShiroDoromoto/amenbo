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
// **The page has two widths.** The panel is a column beside a terminal, and a paragraph typed in a
// column is read a few words at a time. The wide one is the same text in the middle of the window,
// stopped at a width a paragraph is read at.
import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { t } from "../core/i18n";
import { asTyped } from "../core/keys";
import { projectMemo, setProjectMemo } from "./memo";

/** How long the typing has to settle before it is written. */
const SETTLE_MS = 600;

export function MemoPage({ projectId }: { projectId: number }) {
  const [text, setText] = useState("");
  const [wide, setWide] = useState(false);
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
    clearTimeout(timer.current);
    timer.current = setTimeout(write, SETTLE_MS);
  };

  const field = (
    <textarea
      {...asTyped}
      className={wide ? "memo__field memo__field--wide" : "memo__field"}
      value={text}
      placeholder={t("files.memoPlaceholder")}
      aria-label={t("files.memo")}
      onChange={(e) => typed(e.target.value)}
      // Nothing here submits: there is nothing to submit to. Escape closes the wide page, which is
      // the one thing this can be asked to do.
      onKeyDown={(e) => { if (e.key === "Escape" && wide) setWide(false); }}
    />
  );

  return (
    <div className="files__row memo">
      <div className="memo__bar">
        <h3 className="files__head">{t("files.memo")}</h3>
        <button className="files__back" onClick={() => setWide(true)}>{t("files.memoWide")}</button>
      </div>
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
            <div className="memo__bar">
              <h3 className="files__head">{t("files.memo")}</h3>
              <button className="files__back" onClick={() => setWide(false)}>{t("files.memoNarrow")}</button>
            </div>
            {field}
          </div>
        </div>,
        document.body,
      )}
    </div>
  );
}
