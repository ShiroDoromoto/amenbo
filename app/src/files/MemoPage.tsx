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
// **How much room it gets is the column's answer, not its own.** The page is one of the reading
// column's tabs, and that column has a narrow width the panes are drawn beside and a wide one that
// lies over them (`AMB-D-835`). A page carrying a second answer to the same question would be one
// question with two controls, drifting apart at the first change.
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { takesPastedFiles, writesPastedImage } from "../core/clipFiles";
import { t } from "../core/i18n";
import { asTyped } from "../core/keys";
import { projectMemo, setProjectMemo } from "./memo";

/** How long the typing has to settle before it is written. */
const SETTLE_MS = 600;

/** Where the writing stands: nothing typed yet, typing in hand, or written. */
type Keep = "none" | "typing" | "kept";

export function MemoPage({ projectId }: { projectId: number }) {
  const [text, setText] = useState("");
  const [keep, setKeep] = useState<Keep>("none");
  // A CSS animation starts on an element that is new and not on one that has re-rendered, so every
  // keystroke hands the ring a fresh key and it fills from nothing again.
  const [fills, setFills] = useState(0);
  // What has been typed but not yet written, so the way out can write it whatever the timer was
  // about to do.
  const pending = useRef<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  // The field itself, because a paste carrying files is answered on the element rather than
  // through React: what has to be stopped is the page's own answer to the press, and that is only
  // reachable on the way down to it (`../core/clipFiles`).
  const box = useRef<HTMLTextAreaElement | null>(null);
  // Where the caret goes once what arrived has been drawn. The text is state, so it comes back
  // down on the next render — and a field re-rendered with a longer value puts the caret at the
  // end of it, which is not where a person pasting into the middle of a draft left off.
  const caret = useRef<number | null>(null);

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

  const typed = useCallback((value: string) => {
    setText(value);
    pending.current = value;
    setKeep("typing");
    setFills((n) => n + 1);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => { write(); setKeep("kept"); }, SETTLE_MS);
  }, [write]);

  // **A paste carrying files puts the paths in as words** (`AMB-T-4404`). A reader who copied a
  // row in the panel this page shares its column with has the file itself on the clipboard, and
  // the page's own paste is handed nothing at all for it — so the press lands and the draft is
  // unchanged. Bare and one to a line, which is how they were copied: quoting belongs to a pane,
  // where a name with a space in it would otherwise be two words to a shell (`AMB-D-832`).
  //
  // It goes in where the caret is and takes the selection with it, which is what every other
  // paste into a text box does.
  //
  // **A screenshot is written down first and its path put in, the same as a copied file's**
  // (`AMB-D-854`). An image on the clipboard is bytes and no file, so there is nothing to name until
  // it has been put somewhere; the host puts it in a directory belonging to the run, because this
  // page is no pane and has no session of its own (`../core/clipFiles`). What goes in is the path
  // and nothing around it — not a markdown image, which would be this page deciding the draft is
  // markdown, and it is plain text.
  //
  // ⚠ **The picture does not outlast the app.** A draft kept over a restart keeps a path that
  // reaches nothing, which is the trade this door is: it is for a screenshot being handed to an
  // agent now, not for a draft that holds pictures.
  useEffect(() => {
    const field = box.current;
    if (field === null) return;
    return takesPastedFiles(
      field,
      (paths, words) => {
        const arrived = paths.length > 0 ? paths.join("\n") : words;
        if (arrived === "") return;
        const from = field.selectionStart;
        const to = field.selectionEnd;
        typed(field.value.slice(0, from) + arrived + field.value.slice(to));
        caret.current = from + arrived.length;
      },
      (bytes, mime) => writesPastedImage(bytes, mime, null),
    );
  }, [typed]);

  // And the caret put back, before the browser has drawn the field the state came down into.
  useLayoutEffect(() => {
    const where = caret.current;
    if (where === null) return;
    caret.current = null;
    box.current?.setSelectionRange(where, where);
  });

  const field = (
    <textarea
      {...asTyped}
      ref={box}
      className="memo__field"
      value={text}
      aria-label={t("files.memo")}
      onChange={(e) => typed(e.target.value)}
    />
  );

  return (
    <div className="files__row memo">
      {/* How the writing stands, and nothing else on the row: which page this is, is said by the tab
          it is under (`./FilesPanel`). */}
      <div className="memo__bar">
        <span className={keep === "none" ? "memo__keep" : `memo__keep memo__keep--on memo__keep--${keep}`}>
          <span
            key={fills}
            className="memo__ring"
            style={{ "--memo-settle": `${SETTLE_MS}ms` } as CSSProperties}
          />
          {/* The state is said as well as drawn: a ring is nothing to a reader being read to. */}
          <span className="memo__word" aria-live="polite">
            {keep === "none" ? "" : t(keep === "typing" ? "files.memoTyping" : "files.memoKept")}
          </span>
        </span>
      </div>
      {field}
    </div>
  );
}
