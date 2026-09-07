import { useEffect, useState, useRef } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { acrossIn, movedWithin, pageShape, type Frame, type Layout } from "../talk/layout";
import { frameLabel, type FrameNames } from "../talk/frames";
import { draggedFar, elementUnder, type Point } from "../core/pointerDrag";
import { sideOfBox } from "./rowDrag";
import { t, tf } from "../core/i18n";

/**
 * Where the panes of one project are put in order (`AMB-D-853`).
 *
 * **The panes do not move on the page they are drawn on.** A pane is a terminal somebody is reading,
 * so a drag over the face itself would move what is under their eyes and their paste while a program
 * is running in it. The order is changed here instead, and only what is pressed for is kept: the
 * cross, Escape and the backdrop all leave the arrangement exactly as it was, however much has been
 * dragged about in here.
 *
 * **It draws the pages, because the list is what is being reordered and the pages are that list cut
 * at the count** (`../talk/layout`). A pane carried onto another page is the same move as one carried
 * across a page, so nothing here is a page-to-page operation — and a card that has ended up somewhere
 * other than where it began says which page it came from, which is the one thing the grid cannot show
 * on its own.
 *
 * The gaps a page has are not drawn. What is being ordered is the panes, and an empty box in here
 * would read as somewhere to drop one — a place, when the only places are the cards themselves.
 */
export function PaneOrder({ layout, panes, names, onClose, onOrder }: {
  layout: Layout;
  /** The panes of the project on the screen, in the order they stand in now. */
  panes: readonly Frame[];
  names: FrameNames;
  onClose: () => void;
  /** The order the reader pressed for. Nothing is written until they do. */
  onOrder: (order: readonly Frame[]) => void;
}) {
  const [order, setOrder] = useState<readonly Frame[]>(panes);
  // The page each pane sat on when this was opened, so a card that has moved off it can say so. It is
  // read once: the answer is about where the reader left things, and one recomputed as they drag
  // would go on agreeing with wherever the card is now and never say anything.
  const [from] = useState(() =>
    new Map(panes.map((one, at) => [one.id, Math.floor(at / layout.count) + 1] as const)));
  // Which way the cards run, and therefore which midline puts one before another: the grid at this
  // count and orientation, taken from the same arithmetic that lays out the face (`../talk/layout`).
  const axis = acrossIn(layout.count, layout.orient) === 1 ? "down" : "across";

  // The press in flight. A ref because a move fires far more often than the screen redraws and none
  // of what it carries is drawn — what is drawn is the order and the card being held.
  const press = useRef<{ id: string; from: Point; dragging: boolean } | null>(null);
  const [held, setHeld] = useState<string | null>(null);
  const latest = useRef<Point>({ x: 0, y: 0 });
  const pending = useRef<number | null>(null);
  // How to let go of the press — the listeners it put on the document, and the frame it asked for.
  const stop = useRef<(() => void) | null>(null);

  // A press outliving the modal would leave listeners on a document that has nothing to reorder.
  useEffect(() => () => stop.current?.(), []);

  // Escape closes it, as every other modal here does, and nothing is kept: what was dragged about is
  // a proposal until the button is pressed.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  // The two fences a held card needs, both of them the list's (`./Sidebar`): the webview's own menu
  // takes the pointer away mid-drag, and dragging across the cards selects their text.
  useEffect(() => {
    if (held === null) return;
    const block = (e: Event) => e.preventDefault();
    document.addEventListener("contextmenu", block, true);
    document.body.classList.add("dragging-row");
    return () => {
      document.removeEventListener("contextmenu", block, true);
      document.body.classList.remove("dragging-row");
    };
  }, [held]);

  // Where the held card stands now, from wherever the pointer is. The order moves as the hand does —
  // what a person is looking at while they drag is the arrangement they are about to ask for.
  const retest = () => {
    const on = press.current;
    if (on?.dragging !== true) return;
    const card = elementUnder(latest.current, "data-pane-card");
    const target = card?.dataset.paneCard;
    if (card == null || target == null || target === on.id) return;
    const side = sideOfBox(latest.current, card.getBoundingClientRect(), axis);
    setOrder((was) => movedWithin(was, on.id, target, side));
  };

  const onCardDown = (e: ReactPointerEvent<HTMLElement>, id: string) => {
    // The primary button alone. A right-click is the menu's, and a middle-click is nobody's here.
    if (e.button !== 0) return;
    press.current = { id, from: { x: e.clientX, y: e.clientY }, dragging: false };
    latest.current = { x: e.clientX, y: e.clientY };
    // The listeners go on the document rather than on the card, because the card is redrawn into
    // another place the moment the order changes and a listener on it would be cut off mid-drag
    // (`AMB-D-775` rebuilt these gestures out of pointer events; nothing puts them back).
    const move = (ev: PointerEvent) => {
      const on = press.current;
      if (on === null) return;
      latest.current = { x: ev.clientX, y: ev.clientY };
      if (!on.dragging) {
        if (!draggedFar(on.from, latest.current)) return;
        on.dragging = true;
        setHeld(on.id);
      }
      // Against the selection, on top of the body's `user-select`.
      ev.preventDefault();
      // One hit test a frame: the pointer reports far more often than the screen redraws, and a
      // second test inside one frame is a rectangle read nothing can act on (`./Sidebar`).
      if (pending.current !== null) return;
      pending.current = requestAnimationFrame(() => { pending.current = null; retest(); });
    };
    const up = () => stop.current?.();
    stop.current = () => {
      document.removeEventListener("pointermove", move);
      document.removeEventListener("pointerup", up);
      document.removeEventListener("pointercancel", up);
      if (pending.current !== null) cancelAnimationFrame(pending.current);
      pending.current = null;
      press.current = null;
      stop.current = null;
      setHeld(null);
    };
    document.addEventListener("pointermove", move);
    document.addEventListener("pointerup", up);
    document.addEventListener("pointercancel", up);
  };

  const pages: Frame[][] = [];
  for (let at = 0; at < order.length; at += layout.count) pages.push(order.slice(at, at + layout.count));

  return (
    <div className="modal__overlay" onClick={onClose}>
      <div
        className="modal__card paneorder"
        role="dialog"
        aria-modal="true"
        aria-label={t("face.orderTitle")}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="paneorder__head">{t("face.orderTitle")}</div>
        <div className="paneorder__pages">
          {pages.map((slots, at) => (
            <section className="paneorder__page" key={at}>
              <div className="paneorder__pagename">{tf("face.page", { n: at + 1 })}</div>
              <div
                className={`termface__page-grid termface__page-grid--${
                  pageShape(layout.count, layout.orient)} paneorder__grid`}
              >
                {slots.map((frame) => {
                  const name = frameLabel(names, frame.id, frame.folder) ?? t("face.orderNoName");
                  const was = from.get(frame.id);
                  return (
                    <div
                      key={frame.id}
                      className={`paneorder__card${held === frame.id ? " paneorder__card--held" : ""}`}
                      data-pane-card={frame.id}
                      onPointerDown={(e) => onCardDown(e, frame.id)}
                    >
                      <span className="paneorder__name" title={name}>{name}</span>
                      {frame.folder !== null && (
                        <span className="paneorder__folder" title={frame.folder}>{frame.folder}</span>
                      )}
                      {was !== undefined && was !== at + 1 && (
                        <span className="paneorder__from">{tf("face.orderFrom", { n: was })}</span>
                      )}
                    </div>
                  );
                })}
              </div>
            </section>
          ))}
        </div>
        <div className="buttonrow">
          <button className="btn btn--primary" onClick={() => onOrder(order)}>
            {t("face.orderApply")}
          </button>
          <button className="btn" onClick={onClose}>{t("face.orderCancel")}</button>
        </div>
      </div>
    </div>
  );
}
