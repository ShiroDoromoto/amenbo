// The toast: one transient line, said to whoever is looking at this window.
//
// It is a window's own, not the store's. The board and the terminal window are two React roots, and
// what used to draw this was `StoreProvider` — which only the board mounts. So a word pushed from
// the terminal window (a folder import that stopped on a name, a pane that failed to start) went to
// a bus nobody was listening on and was dropped in silence (`AMB-T-4668`). Every window that a
// person can be looking at puts one of these up, and the bus in `core/notice` is where the words
// arrive from — a failed write included, which is why `StoreProvider` pushes rather than draws.
//
// One message at a time, the newest winning: a toast is `position: fixed` at the bottom of the
// window, so a second one would be drawn over the first. It goes on its own after four seconds, and
// a click takes it away sooner.
import { useEffect, useState } from "react";
import { subscribeNotice } from "../core/notice";
import { Icon } from "./Icon";

export function NoticeToast() {
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => subscribeNotice(setNotice), []);

  useEffect(() => {
    if (!notice) return;
    const id = setTimeout(() => setNotice(null), 4000);
    return () => clearTimeout(id);
  }, [notice]);

  if (!notice) return null;
  return (
    <div className="toast toast--warn" role="alert" onClick={() => setNotice(null)}>
      <Icon name="warning" /> {notice}
    </div>
  );
}
