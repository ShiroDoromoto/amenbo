// Put in place of `src/talk/plate.ts`: the row over a pane is a live thing of its own, fed by the host,
// and is not drawn again by a key pressed in the box. It is left empty here.
export * from "@app/talk/plate";

export function mountPlate() {
  const none = () => {};
  return {
    opened: none, closed: none, named: none, took: none, stated: none, numbered: none, timed: none, focused: none, stop: none,
  };
}
