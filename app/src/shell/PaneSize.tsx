// The size a pane takes, picked from its row (`AMB-D-959`).
//
// **The corner cannot reach every size.** What the corner's drag settles is how many columns lie
// between the pane's left edge and the pointer (`./paneDrag`), and a pane in the right-hand column
// has its corner against the window's edge — so it can never be pulled out to the whole page. The
// row therefore carries the sizes as a list as well, and the corner stays for the reader whose hand
// is already there.
//
// **The mark is the size it is now**, drawn on the page's own twelve-by-two grid, so the row says how
// much of the page the pane takes before anything is opened. A fixed mark would say only that there
// is a list behind it.
//
// **The list opens as the pointer comes onto the mark**, and on a press for a hand that does not
// hover. The six sit in one row, each drawn the same way with its name under it: two halves that
// differ only by which way they lie are not told apart by the drawing alone. The one standing now is
// framed.
//
// **It is drawn against the window rather than inside the pane**, because the smallest pane is
// narrower than the list: held inside it, the list would be cut off by the very pane it resizes. It
// is kept inside the window's width for the same reason.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ACROSS, BOXES, DOWN, SIZES, type Size } from "../talk/layout";
import { t } from "../core/i18n";

// Spelled out rather than built from the size, so the key gate can see every label a reader can be
// shown (`core/i18n/sourceKeys.test.ts`).
const NAMES: Readonly<Record<Size, () => string>> = {
  whole: () => t("pane.size.whole"),
  half: () => t("pane.size.half"),
  "half-down": () => t("pane.size.halfDown"),
  quarter: () => t("pane.size.quarter"),
  sixth: () => t("pane.size.sixth"),
  eighth: () => t("pane.size.eighth"),
};

/** How long the list stays after the pointer leaves it, so a hand crossing the gap to it keeps it. */
const LINGER_MS = 150;

/** The window's edge the list keeps clear of. */
const EDGE = 8;

/** One size drawn on the page's grid: the page outlined, and what the size takes of it filled in. */
export function SizeMark({ size }: { size: Size }) {
  const box = BOXES[size];
  return (
    <svg className="panesize__mark" viewBox={`0 0 ${ACROSS * 2} ${DOWN * 4}`} aria-hidden="true">
      <rect className="panesize__page" x="0.5" y="0.5" width={ACROSS * 2 - 1} height={DOWN * 4 - 1} />
      <rect className="panesize__took" x="0.5" y="0.5" width={box.across * 2 - 1} height={box.down * 4 - 1} />
    </svg>
  );
}

export function PaneSize({ size, onSize }: { size: Size; onSize: (to: Size) => void }) {
  const [open, setOpen] = useState(false);
  const [at, setAt] = useState<{ left: number; top: number } | null>(null);
  const markRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const leaving = useRef<number | null>(null);

  const stay = () => {
    if (leaving.current !== null) window.clearTimeout(leaving.current);
    leaving.current = null;
  };
  const show = () => {
    stay();
    setOpen(true);
  };
  const leave = () => {
    stay();
    leaving.current = window.setTimeout(() => setOpen(false), LINGER_MS);
  };
  useEffect(() => stay, []);

  // Placed under the mark, and moved left where it would run past the window's right edge.
  useLayoutEffect(() => {
    if (!open) {
      setAt(null);
      return;
    }
    const mark = markRef.current?.getBoundingClientRect();
    const width = listRef.current?.offsetWidth ?? 0;
    if (mark === undefined) return;
    const left = Math.max(EDGE, Math.min(mark.left, window.innerWidth - width - EDGE));
    setAt({ left, top: mark.bottom + 4 });
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  return (
    <>
      <button
        ref={markRef}
        className="panesize"
        title={t("face.paneSize")}
        aria-label={t("face.paneSize")}
        aria-haspopup="menu"
        aria-expanded={open}
        onMouseEnter={show}
        onMouseLeave={leave}
        // It opens and does not toggle: the pointer that pressed it came onto it first and has opened
        // it already, and a press that shut it again would take the list away from the hand reaching
        // for it. It goes with a pick, with Escape, or with the pointer leaving.
        onClick={show}
      >
        <SizeMark size={size} />
      </button>
      {open && createPortal(
        <div
          ref={listRef}
          className="panesize__list"
          role="menu"
          // Measured before it is placed, so it is drawn out of sight for that one pass.
          style={at === null ? { visibility: "hidden", left: 0, top: 0 } : at}
          onMouseEnter={stay}
          onMouseLeave={leave}
        >
          {SIZES.map((one) => (
            <button
              key={one}
              role="menuitemradio"
              aria-checked={one === size}
              className={one === size ? "panesize__one panesize__one--now" : "panesize__one"}
              onClick={() => {
                setOpen(false);
                if (one !== size) onSize(one);
              }}
            >
              <SizeMark size={one} />
              <span className="panesize__name">{NAMES[one]()}</span>
            </button>
          ))}
        </div>,
        document.body,
      )}
    </>
  );
}
