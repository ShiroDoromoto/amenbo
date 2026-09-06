import type { PtySessionDto } from "../bindings/bindings";
import { invoke } from "../core/ipc";

/**
 * How many panes this process has open (`crate::pty::pty_sessions`) — whether a road that ends all
 * of them at once has anything to end.
 *
 * **It is the whole of what the ways out ask.** Two roads end every session in the process at once
 * and neither can be taken back: ending the app (`./AppShell`, `crate::quit`), and starting it again
 * to come back on a newer build, which ends them the same way (`../components/UpdateBanner`). What
 * each of them asks before it goes is whether there is a terminal to lose — a count, and nothing
 * about what any of them was doing.
 *
 * **What is lost is not named, because nothing could name it honestly** (`AMB-D-858`). A pane and a
 * reservation used to be joined by a key the world could rewrite behind the pane, so a box naming
 * what was about to be lost named as often a task somebody had already finished elsewhere. A
 * reservation left standing is on the ledger and stays there — `amenbo task list` finds it, and
 * `amenbo task status <id> todo` hands it back.
 *
 * It needs no store, which is what lets the overtaking gate ask it too (`../screens/RestartGate`):
 * that road restarts out of a store it cannot open.
 */
export async function openPanes(): Promise<number> {
  return invoke<PtySessionDto[]>("pty_sessions").then((open) => open.length).catch(() => 0);
}
