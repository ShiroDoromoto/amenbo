import { useEffect, useState, useRef } from "react";
import type { CSSProperties, PointerEvent as ReactPointerEvent } from "react";
import { acrossIn, movedWithin, pageShape, type Frame, type Layout } from "../talk/layout";
import { frameLabel, type FrameNames } from "../talk/frames";
import { draggedFar, elementUnder, type Point } from "../core/pointerDrag";
import { faceOf, sayText, type Plate as Row, type Say } from "../talk/nameplate";
import { BLINK_MS, hueOf, phaseDelay } from "../talk/moving";
import { sideOfBox } from "./rowDrag";
import { Icon } from "../components/Icon";
import { currentLang, t, tf } from "../core/i18n";

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
 *
 * **A card carries what the pane's own label carries** (`../talk/nameplate`), because three panes
 * open on one repository are three cards reading `repo` otherwise — and which of them a person wants
 * moved is exactly what the lamp and the one thing said tell them apart by. It is the row itself,
 * read off the pane rather than worked out again, so a pane is never described two ways at once.
 */
export function PaneOrder({ layout, panes, names, rows, onClose, onOrder }: {
  layout: Layout;
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
  const lang = currentLang();
  /**
   * The row of one pane, as it stood when this opened.
   *
   * **A pane that is drawn is read; one that is not says nothing.** Only the page on the screen has
   * panes mounted on it, so what is known about a pane — output arriving, a sentence left unsent, how
   * long the silence has run — exists for those and for no others. The rest is left unsaid rather
   * than filled in: silence is silence (`AMB-D-858`).
   */
  function rowOf(frame: Frame): Row {
    const read = rows.get(frame.id);
    const drawn = read?.() ?? null;
    if (drawn !== null) return drawn;
    const say: Say = { kind: "silent" };
    return {
      name: frameLabel(names, frame.id, frame.folder),
      say,
      dot: { frame: frame.id, face: faceOf(say, false) },
    };
  }
  // Read once, as the modal opens. What is drawn in here is a proposal about an arrangement, not a
  // second screen for watching the panes on: a card that moved under the hand carrying it would be
  // the reader's own drag fighting a redraw.
  const [plates] = useState(() => new Map(panes.map((one) => [one.id, rowOf(one)] as const)));
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
        // The beat and the phase a calling lamp blinks to, which are the row's own
        // (`../talk/moving`): a card and the pane it stands for have to fall together, or the one
        // signal reads as two things being asked.
        style={{ "--blink": `${BLINK_MS}ms`, "--phase": phaseDelay(Date.now()) } as CSSProperties}
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
                  const row = plates.get(frame.id);
                  const name = row?.name ?? t("face.orderNoName");
                  const said = row === undefined
                    ? { mark: null, text: "" }
                    : sayText(row.say, lang);
                  const was = from.get(frame.id);
                  return (
                    <div
                      key={frame.id}
                      className={`paneorder__card${held === frame.id ? " paneorder__card--held" : ""}`}
                      // The same attribute the label reads by, so a card asking for a person is drawn
                      // the way the row above that pane is (`../styles/global.css`).
                      data-say={row?.say.kind}
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
                          data-face={row?.dot.face ?? "out"}
                          style={{ "--dot-hue": String(hueOf(frame.id)) } as CSSProperties}
                        />
                        <span className="paneorder__name" title={name}>{name}</span>
                      </div>
                      {/* The one thing the pane said, at the rank the row says it at. A card is not
                          the row's one line, so nothing is dropped for width — what does not fit is
                          elided and given back in full by the machine. */}
                      {said.text !== "" && (
                        <span className="paneorder__say" title={said.text}>
                          {said.mark !== null && <Icon name={said.mark} />}
                          {said.text}
                        </span>
                      )}
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
