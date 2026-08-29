// The shell every small menu opened at a point wears: the file row's (`../files/FilesPanel`) and
// the pane row's are the same box with different items in it.
//
// **What is shared is the machinery, not the items.** A menu is a box placed where the pointer was,
// closed by anything outside it and by Escape, walked with the arrows, and handed back to whatever
// the reader was standing on when it goes. None of that is about what the items do, and written
// twice it is the pair that drifts — one gets the fix and the other keeps the bug.
//
// **The items are the caller's, but the class on them is not.** The arrows find what to walk by the
// item class, so a caller who spelt it themselves would lose the keyboard without anything looking
// wrong. `MenuItem` is how the class stays out of their hands.
import { useEffect, useRef } from "react";
import type { KeyboardEvent as ReactKeyboardEvent, ReactNode } from "react";

/** What the arrows walk. The one string both halves of this file agree on. */
const ITEM = "menu__item";

/**
 * A menu box at a point, holding whatever items it is given.
 *
 * Placed where the pointer was and above everything, because it is about the row it was opened on
 * and nothing else.
 */
export function Menu({ at, face, onClose, children }: {
  at: { x: number; y: number };
  /**
   * What the items are showing, where a menu has more than one face to show. Focus returns to the
   * first item whenever this changes, because the items under the reader have just been replaced
   * and the one they were standing on is gone.
   */
  face?: unknown;
  onClose: () => void;
  children: ReactNode;
}) {
  const box = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    // Anything the person does **outside** the menu closes it: one that outlived the next click
    // would sit over rows it is no longer about. Inside is the opposite — a press on an item is the
    // first half of choosing it, and closing there unmounts the button before the click can land on
    // it, so the item never fires at all.
    const away = (event: Event) => {
      if (event.target instanceof Node && box.current?.contains(event.target)) return;
      onClose();
    };
    // **Escape and nothing else.** This listened for every key once, which read as "any key means
    // move on" and worked only while no key meant anything else — the moment the rows answered to
    // the arrows, every press meant to walk the tree shut the menu on the way past (`AMB-D-780`).
    const key = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", key);
    window.addEventListener("blur", away);
    return () => {
      document.removeEventListener("pointerdown", away);
      document.removeEventListener("keydown", key);
      window.removeEventListener("blur", away);
    };
  }, [onClose]);

  // The first item, once there is one. A menu opened from a row the arrows reached is a menu the
  // arrows have to be able to walk, and a list nothing is standing on is one every key falls out of.
  //
  // **And the row it was opened on, once it closes.** Focus left where the menu used to be is a
  // reader standing on nothing: the next arrow reaches no tree and the panel goes quiet, which is
  // the same dead end as never having taken the focus at all.
  const from = useRef<HTMLElement | null>(null);
  useEffect(() => {
    from.current ??= document.activeElement instanceof HTMLElement ? document.activeElement : null;
    box.current?.querySelector<HTMLElement>(`.${ITEM}`)?.focus();
    // Given back only where the menu going is what left the focus nowhere. A reader who clicked
    // away has already said where they are, and taking them back to the row would be undoing it.
    return () => {
      if (document.activeElement === null || document.activeElement === document.body) {
        from.current?.focus();
      }
    };
  }, [face]);

  /** Up and down the items. The pattern a menu is read by, and the reason a blanket key handler
   *  cannot be one of these: these presses are the menu's own, not a way out of it. */
  const walk = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const items = [...e.currentTarget.querySelectorAll<HTMLElement>(`.${ITEM}`)];
    const at = items.indexOf(document.activeElement as HTMLElement);
    const step = e.key === "ArrowDown" ? 1 : -1;
    // Round, because a menu is short and a reader who has walked to the end of one means to go on.
    items[(at + step + items.length) % items.length]?.focus();
  };

  return (
    <div
      className="menu"
      style={{ left: at.x, top: at.y }}
      role="menu"
      ref={box}
      onKeyDown={walk}
    >
      {children}
    </div>
  );
}

/** One item. `apart` holds it off from the items above, for the one that is not their kind. */
export function MenuItem({ apart = false, onClick, children }: {
  apart?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      className={apart ? `${ITEM} ${ITEM}--apart` : ITEM}
      role="menuitem"
      onClick={onClick}
    >
      {children}
    </button>
  );
}
