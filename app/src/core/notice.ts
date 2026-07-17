// The notice bus: a way in from outside for a one-line message on the transient toast (`.toast`).
//
// The toast itself belongs to `StoreProvider` (its `notice` state, used to warn about a failed mutator). But modules
// outside React — `mailbox.notifyArrival`, for one — sometimes need a word with the user too, e.g. to point out that
// an OS notification died in silence. This is the smallest bus that joins the two: `pushNotice(msg)` sends one
// message, `StoreProvider` takes it with `subscribeNotice` and puts it on the toast it already has. A single
// subscriber (StoreProvider) is what we expect, but a Set keeps it general.
type NoticeListener = (msg: string) => void;

const listeners = new Set<NoticeListener>();

/** Put one transient warning toast on the UI (dropped silently when nobody is subscribed). */
export function pushNotice(msg: string): void {
  for (const l of listeners) l(msg);
}

/** Subscribe to the notice bus (`StoreProvider` puts what arrives on the toast). The return value unsubscribes. */
export function subscribeNotice(fn: NoticeListener): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}
