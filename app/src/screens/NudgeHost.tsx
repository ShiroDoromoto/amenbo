// The surface a nudge is put on: "you have been using this a while — would you like …" (`AMB-D-542`).
//
// It holds no judgement of its own. Whether a nudge is due — the thresholds, what has already gone out —
// is core's to answer (`AMB-D-544`), and this asks for it, shows what comes back, and says it has been
// shown. What belongs here is the other half: the wording, the look, and which stages this surface is
// currently in.
//
// **Adding a nudge is a line in core's table and a line in each of the two below.** Neither table
// carries wording of its own — a nudge's view holds that, along with the dictionary keys it needs.
//
// It takes its turn behind the other two questions rather than beside them — see `AppShell` for the
// order and why it is spelled out there.
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { fetchPendingNudges, markNudgePut } from "../core/mutations";
import { AutostartNudge, autostartOfferable } from "./AutostartNudge";

/**
 * How a nudge looks and reads on this surface. `onClose` takes it off the screen — a nudge is put once
 * and then it is over, whatever the person did with it (what an *answer* changes is the setting the
 * nudge is about, which the view records itself).
 */
export type NudgeView = (props: { onClose: () => void }) => ReactNode;

/** The nudges this build knows how to put, keyed by the id core declares them under. */
export const NUDGE_VIEWS: Record<string, NudgeView> = {
  autostart: ({ onClose }) => <AutostartNudge onClose={onClose} />,
};

/**
 * The stages this surface can answer for, keyed by the name a nudge declares — each says whether we are
 * in it now ("this build has the setting to offer, and it is not already on"). A stage with no entry
 * here is never reported open, so a nudge behind it stays unput rather than going out unjudged.
 */
export const NUDGE_STAGES: Record<string, () => Promise<boolean>> = {
  autostart_offerable: autostartOfferable,
};

/**
 * How long after an evaluation a focus return earns the next one. The trigger is startup and then focus
 * (`AMB-D-542`), and the interval is what keeps "back in the window" from meaning "ask core again": what
 * a nudge is judged on is an order of magnitude of use, which does not turn over between two glances at
 * the app.
 */
export const NUDGE_REEVALUATE_AFTER_MS = 60 * 60 * 1000;

/** Both tables default to the module's own; passing them is how a test declares a nudge to put. */
export type NudgeHostProps = {
  views?: Record<string, NudgeView>;
  stages?: Record<string, () => Promise<boolean>>;
};

/** Puts the nudge that is due, if this build has one to put. */
export function NudgeHost({ views = NUDGE_VIEWS, stages = NUDGE_STAGES }: NudgeHostProps) {
  const [due, setDue] = useState<string | null>(null);
  // Read by the trigger without re-arming it: a nudge already on screen is not replaced by another.
  const dueRef = useRef<string | null>(null);
  dueRef.current = due;
  const lastAskedAt = useRef(0);

  const ask = useCallback(async () => {
    // A build with no view declared can put nothing, so it asks nothing — the whole evaluation, IPC and
    // all, is skipped in the state every build is in until the first nudge is declared.
    if (dueRef.current || Object.keys(views).length === 0) return;
    lastAskedAt.current = Date.now();
    const open: string[] = [];
    for (const [name, isOpen] of Object.entries(stages)) {
      if (await isOpen()) open.push(name);
    }
    const ids = await fetchPendingNudges(open);
    // A nudge this build cannot word is left alone: not shown, and — because it was not shown — not
    // recorded either, so the build that can word it still gets to put it.
    const next = ids.find((id) => id in views);
    if (next) setDue(next);
  }, [views, stages]);

  // Startup is the first evaluation (`AMB-D-542`). A failure to ask is swallowed: a nudge is a courtesy,
  // and there is nothing here worth putting an error on the screen for.
  useEffect(() => {
    void ask().catch(() => {});
  }, [ask]);

  // Then focus returns, at most one evaluation per interval. Both events are listened for, as the
  // reconcile triggers do, since a window can come back by either.
  useEffect(() => {
    const onFocus = () => {
      if (Date.now() - lastAskedAt.current < NUDGE_REEVALUATE_AFTER_MS) return;
      void ask().catch(() => {});
    };
    const onVisible = () => {
      if (document.visibilityState === "visible") onFocus();
    };
    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [ask]);

  // Told after the render that put it there, which is what core asks of the caller: a nudge recorded and
  // then not drawn would be closed for someone who never saw it. A failure to record is swallowed — the
  // person has seen it, and the worst it costs is being asked once more.
  useEffect(() => {
    if (due) void markNudgePut(due).catch(() => {});
  }, [due]);

  const View = due ? views[due] : undefined;
  return View ? <View onClose={() => setDue(null)} /> : null;
}
