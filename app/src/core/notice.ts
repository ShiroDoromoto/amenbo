// The notice bus: a way in from outside for a one-line message on the transient toast (`.toast`).
//
// The toast itself is a window's, drawn by `components/NoticeToast`. Everything with a word to say goes through here
// rather than reaching for it — a failed write in `store/store`, and modules outside React too, `mailbox.notifyArrival`
// pointing out that an OS notification died in silence. This is the smallest bus that joins the two: `pushNotice(msg)`
// sends one message, and each window's toast takes it with `subscribeNotice`.
//
// One subscriber per window is what we expect — the board and the terminal window are two webviews, so each runs its
// own copy of this module and mounts its own toast (`AMB-T-4668`) — but a Set keeps it general. A message reaches the
// window it was pushed from and no other: what is said about a pane is said where the pane is.
type NoticeListener = (msg: string) => void;

const listeners = new Set<NoticeListener>();

/** Put one transient warning toast on the UI (dropped silently when nobody is subscribed). */
export function pushNotice(msg: string): void {
  for (const l of listeners) l(msg);
}

/** Subscribe to the notice bus (`NoticeToast` puts what arrives on the toast). The return value unsubscribes. */
export function subscribeNotice(fn: NoticeListener): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}
