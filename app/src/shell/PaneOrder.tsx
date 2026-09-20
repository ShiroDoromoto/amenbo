import { useEffect, useState, useRef } from "react";
import type { CSSProperties, PointerEvent as ReactPointerEvent } from "react";
import { gridAt, movedWithin, placing, type Frame } from "../talk/layout";
import { frameLabel, type FrameNames } from "../talk/frames";
import { draggedFar, elementUnder, type Point } from "../core/pointerDrag";
import { faceOf, type Face, type Plate as Row } from "../talk/nameplate";
import { hueOf } from "../talk/moving";
import { sideOnPane } from "./paneDrag";
import { t, tf } from "../core/i18n";

/**
 * Where the panes of one project are put in order (`AMB-D-853`).
 *
 * **The page itself is where a pane is carried past the one beside it** (`./paneDrag`, `AMB-D-939`),
 * so what is left to this is the move the page cannot show: a pane carried onto a page that is not
 * on the screen. Every page of the list is drawn in here, which is the one thing the face cannot do —
 * and the way in is drawn only where there is a second page to reach, since on one page the face can
 * already make every move this could (`./WorkspaceFace`).
 *
 * **A card is still dropped on a card, and one on the same page still moves.** Where a pane lands on
 * the page it is carried to is said the only way it can be said — beside the pane it is going in
 * front of or behind — and a gesture that worked on one page and died on another would be a rule the
 * reader has to learn from a drag that did nothing.
 *
 * **Nothing leaves here except by the button.** The cross, Escape and the backdrop all leave the
 * arrangement exactly as it was, however much has been dragged about in here — this is a proposal
 * about an arrangement, and the face's own drag is the one that settles as it goes.
 *
 * **It draws the pages, because the list is what is being reordered and the pages are that list laid
 * down in order** (`../talk/layout`). A pane carried onto another page is the same move as one carried
 * across a page, so nothing here is a page-to-page operation — and a card that has ended up somewhere
 * other than where it began says which page it came from, which is the one thing the grid cannot show
 * on its own.
 *
 * The gaps a page has are not drawn. What is being ordered is the panes, and an empty box in here
 * would read as somewhere to drop one — a place, when the only places are the cards themselves.
 *
 * **A card carries what the pane's own label carries** (`../talk/nameplate`), because three panes
 * open on one repository are three cards reading `repo` otherwise — and which of them a person wants
 * moved is exactly what the lamp and the one thing said tell them apart by. It is the row itself,
 * read off the pane rather than worked out again, so a pane is never described two ways at once.
 */
export function PaneOrder({ panes, names, rows, onClose, onOrder }: {
  /** The panes of the project on the screen, in the order they stand in now. */
  panes: readonly Frame[];
  names: FrameNames;
  /** How to read the row of each pane that is drawn, by frame (`../talk/plate`). */
  rows: ReadonlyMap<string, () => Row | null>;
  onClose: () => void;
  /** The order the reader pressed for. Nothing is written until they do. */
  onOrder: (order: readonly Frame[]) => void;
}) {
  const [order, setOrder] = useState<readonly Frame[]>(panes);
  /**
   * What one pane's card says: its name, and which face its lamp was on when this opened.
   *
   * **A pane that is drawn is read; one that is not has its lamp out.** Only the page on the screen
   * has panes mounted on it, so the one thing a row measures — whether output is arriving — is known
   * for those and for no others. It is left unsaid rather than filled in: a lamp out is a lamp out
   * (`AMB-D-858`).
   *
   * The hue the lamp is drawn in is not taken from the row: it belongs to the slot the card is in,
   * and the cards here are being dragged between slots (`../talk/moving`).
   */
  function rowOf(frame: Frame): { name: string | null; face: Face } {
    const drawn = rows.get(frame.id)?.() ?? null;
    if (drawn !== null) return { name: drawn.name, face: drawn.dot.face };
    return { name: frameLabel(names, frame.id, frame.folder), face: faceOf(false) };
  }
  // Read once, as the modal opens. What is drawn in here is a proposal about an arrangement, not a
  // second screen for watching the panes on: a card that moved under the hand carrying it would be
  // the reader's own drag fighting a redraw.
  const [plates] = useState(() => new Map(panes.map((one) => [one.id, rowOf(one)] as const)));
  // The page each pane sat on when this was opened, so a card that has moved off it can say so. It is
  // read once: the answer is about where the reader left things, and one recomputed as they drag
  // would go on agreeing with wherever the card is now and never say anything.
  const [from] = useState(() =>
    new Map(placing(panes).map((one) => [one.frame.id, one.page] as const)));

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
    // Which midline of this card puts one before another. It is read off the card being dropped on
    // rather than off the page, because the cards are no longer one shape (`./paneDrag`).
    const size = order.find((one) => one.id === target)?.size;
    if (size === undefined) return;
    const side = sideOnPane(latest.current, card.getBoundingClientRect(), size);
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

  // The order laid down the way the face lays it down, cut into the pages it makes
  // (`../talk/layout`).
  const laid = placing(order);
  const pages = [...new Set(laid.map((one) => one.page))].map((page) =>
    laid.filter((one) => one.page === page));

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
              <div className="workspace__page-grid paneorder__grid">
                {slots.map(({ frame, across, down }, slot) => {
                  const row = plates.get(frame.id);
                  const name = row?.name ?? t("face.orderNoName");
                  const was = from.get(frame.id);
                  return (
                    <div
                      key={frame.id}
                      className={`paneorder__card${held === frame.id ? " paneorder__card--held" : ""}`}
                      style={gridAt(frame.size, across, down)}
                      data-pane-card={frame.id}
                      onPointerDown={(e) => onCardDown(e, frame.id)}
                    >
                      <div className="paneorder__head-row">
                        {/* The lamp the pane is known by, drawn from the same two answers the row
                            above it is: the hue says which pane, the face what is happening in it
                            (`../talk/nameplate`). */}
                        <span
                          className="plate__dot"
                          aria-hidden="true"
                          data-face={row?.face ?? "out"}
                          style={{ "--dot-hue": String(hueOf(slot)) } as CSSProperties}
                        />
                        <span className="paneorder__name" title={name}>{name}</span>
                      </div>
                      {frame.folder !== null && (
                        <span className="paneorder__folder" title={frame.folder}>{frame.folder}</span>
                      )}
                      {/* Nothing is running here any more. The screen cannot show it — what a
                          finished shell leaves behind looks exactly like one waiting to be typed at
                          — and a person putting the panes in order is deciding which of them to keep
                          in front of them. */}
                      {frame.session === null && (
                        <span className="paneorder__ended">{t("face.orderEnded")}</span>
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
