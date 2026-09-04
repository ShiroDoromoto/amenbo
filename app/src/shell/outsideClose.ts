/**
 * While the right pane is open, decides whether a pointerdown outside it counts as "clicked the blank body, so
 * close" — a pure function over the DOM, free of React and therefore testable. Grabbing blank space in the list
 * closes the pane, but the chrome that is not blank space is excluded: the pane itself and modals
 * (.modal__overlay, the one backdrop class every modal wears — `modalOverlay.test.ts` holds that) are
 * ignored; a row or card in the list ([data-pane-select]) leaves the switch to its onClick
 * (closing on pointerdown and relying on the following click to re-select fails — the reflow moves the row out
 * from under the click and all that happens is the close); the TopBar (.topbar) is ignored too (closing on
 * pointerdown would have closeRight push "no selection" onto the history, and the goBack of the following click
 * would only undo what was just pushed, leaving no way back past the detail pane); and a screen's own toolbar
 * (.board__toolbar) is ignored for the same reason the TopBar is — it is chrome the reader aims at, not blank
 * space. The class is what is excluded rather than the header slot it is portalled into (.shell__header),
 * because the toolbar is the only thing that ever stands in that slot, while the screens that draw one inline
 * (search, activity) show the pane too and press the same controls.
 */
export function isBlankSpaceClose(target: Node | null, rightpane: HTMLElement | null): boolean {
  if (target && rightpane?.contains(target)) return false; // inside the pane: ignore
  if (!(target instanceof Element)) return true; // blank body (a text node and the like): close
  if (target.closest(".modal__overlay")) return false; // inside a modal: ignore
  if (target.closest("[data-pane-select]")) return false; // a list row or card: leave the switch to onClick
  if (target.closest(".topbar")) return false;
  if (target.closest(".board__toolbar")) return false;
  return true; // any other click on blank space: close
}
