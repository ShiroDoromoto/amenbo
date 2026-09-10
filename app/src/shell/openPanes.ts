import type { PtySessionDto } from "../bindings/bindings";
import { invoke } from "../core/ipc";
import { listLabel, t, tf, type Lang } from "../core/i18n";

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
 * `amenbo task status <id> todo` hands it back. The one thing a question does name is a provider
 * whose pane will not be in its conversation on the next run (`endingConfirm` below), and that is
 * the opposite case: it is named because the sentence beside it promises the panes come back.
 *
 * It needs no store, which is what lets the overtaking gate ask it too (`../screens/RestartGate`):
 * that road restarts out of a store it cannot open.
 */
export async function openPanes(): Promise<number> {
  return invoke<PtySessionDto[]>("pty_sessions").then((open) => open.length).catch(() => 0);
}

/**
 * The sentence one of those roads asks with — `whole` where every pane comes back into its
 * conversation, and `named` where some do not (`AMB-T-4676`).
 *
 * **The names are asked for rather than written into the sentence**
 * (`crate::frames::panes_without_a_way_back`). Which providers come back moves as they gain a way
 * back and as a pane loses the one it had, so a sentence that spelled one out would go stale the
 * next time either happened; what the dictionary holds is the shape, and `{names}` is filled with
 * the panes actually on the screen. A host that cannot answer is asked nothing further and the
 * plain sentence stands: a question that failed to draw is worse than one that says less.
 */
export async function endingConfirm(whole: string, named: string, lang?: Lang): Promise<string> {
  const names = await invoke<string[]>("panes_without_a_way_back").catch(() => []);
  if (names.length === 0) return t(whole, lang);
  return tf(named, { names: listLabel(names, lang) }, lang);
}
