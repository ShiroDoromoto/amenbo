// The one way Amenbo speaks to somebody who is not looking at it.
//
// Delivery is concentrated in the native `notify_os` command (UNUserNotificationCenter on macOS; the
// notification plugin on Windows and Linux; permission is settled at startup, so JS needs no
// permission dance of its own). Both the sound and the toast belong to the OS — the app makes no noise
// of its own.
//
// **Failure is never swallowed.** A denied permission, a missing plugin or an unregistered delegate
// would kill notifications silently and nobody would ever notice, so it always reaches the log, and
// the first time it also raises one toast in the UI — the way back to enabling permission. Once,
// because one every time would be insufferable, and because the second failure has the same cause.
//
// **A notification calls; it does not carry anybody anywhere.** Where a click lands is settled on the
// host side by the kind (`crate::notify`), and nothing here brings a window forward.
import { inTauri } from "./snapshot";
import { invoke } from "./ipc";
import { t } from "./i18n";
import { pushNotice } from "./notice";

/**
 * What a toast is about, which is what decides where a click on it lands.
 *
 * Two, because that is what the host knows how to route (`crate::notify`). Only the inbox raises one
 * now — a record on the board. Nothing says a turn is standing any more: whether one is standing was
 * an agent's own word about itself, and a toast made of that was a knock the app could not stand
 * behind (`AMB-D-862`).
 */
export type NotifyKind = "arrival" | "turn";

// Whether the UI has already been told once that the OS notification path failed.
let failureHintShown = false;

/** Raise one OS notification. Outside Tauri (browser iteration) there is no OS to raise one with. */
export async function notifyOs(kind: NotifyKind, title: string, body: string): Promise<void> {
  if (!inTauri()) return;
  try {
    await invoke("notify_os", { title, body, kind });
  } catch (e) {
    console.error("[amenbo] OS notification (notify_os) failed:", e);
    if (!failureHintShown) {
      failureHintShown = true;
      pushNotice(t("mailbox.notifyFailed"));
    }
  }
}
