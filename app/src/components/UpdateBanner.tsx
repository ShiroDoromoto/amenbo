import { useEffect, useState, useSyncExternalStore } from "react";
import { getSnapshot, inTauri, subscribe } from "../core/snapshot";
import { Icon } from "./Icon";
import { dismissUpdate, isUpdateDismissed, sessionDismissCovers, type SessionDismiss } from "../core/updateDismissed";
import { t, tf } from "../core/i18n";
import { openLatestInstaller, installUpdate, restartApp, setStatus } from "../core/mutations";
import type { UpdateProgress } from "../core/mutations";
import { DismissButton } from "./DismissButton";
import { HoldingAsk, heldByAll, openPanes, restartWords } from "../shell/HoldingAsk";
import { confirmDialog } from "../core/dialog";

// One line for the phase the in-app update is in — the hint that replaces `update.hint` while it runs. A download with
// a known size shows a percentage; without one (the manifest carried no length) it is just "Downloading…".
function updatePhaseHint(progress: UpdateProgress | null): string {
  if (!progress || progress.phase === "checking") return t("update.checking");
  if (progress.phase === "downloading") {
    return progress.total
      ? tf("update.downloading", { pct: Math.round((progress.downloaded / progress.total) * 100) })
      : t("update.downloadingUnknown");
  }
  return t("update.installing"); // "installing" | "ready" — the ready copy is shown by the caller, not here.
}

// A newer release exists upstream: when the published `latest.json` names a version newer than the one running, we
// show "an update is available" right under the TopBar. That is the only thing that raises the flag — the local
// version state on its own never does. Pressing "update now" runs the in-app self-update: the Tauri updater
// downloads + minisign-verifies + installs the newer signed build (`installUpdate`), then the banner offers a restart
// to apply it. Both the apply and the restart are user actions — nothing updates in the background. If the updater
// manifest offers nothing (or the update errors), it falls back to opening the all-in-one installer in the browser,
// so the user is never stuck. The cross dismisses it per version (core/updateDismissed): the version dismissed stays quiet
// across launches, and the banner returns on its own once a newer one is offered.
type UpdateStage = "idle" | "working" | "ready";
export function UpdateBanner({ recheck }: { recheck: number }) {
  const vs = useSyncExternalStore(subscribe, () => getSnapshot().versionStatus);
  // Session dismissal, keyed to the version dismissed (core/updateDismissed): it silences the version-less offer that
  // `dismissUpdate` cannot persist, and stands in where localStorage is unavailable. Keyed so a newer offer surfaced
  // this session still shows; a manual re-check (`recheck`) clears it, since asking again overrides an earlier dismiss.
  const [dismissed, setDismissed] = useState<SessionDismiss>(undefined);
  const [stage, setStage] = useState<UpdateStage>("idle");
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  // The reservations the restart is about to leave standing, while the question about them is up
  // (`../shell/HoldingAsk`). Null is no question up, and it is never an empty list.
  const [restartAsking, setRestartAsking] = useState<readonly number[] | null>(null);
  useEffect(() => {
    if (recheck > 0) setDismissed(undefined); // a manual re-check surfaced an offer: drop the session dismissal
  }, [recheck]);
  if (sessionDismissCovers(dismissed, vs.newerVersion) || !vs.updateAvailable || isUpdateDismissed(vs.newerVersion))
    return null;

  const onUpdate = async () => {
    setStage("working");
    setProgress({ phase: "checking" });
    try {
      const applied = await installUpdate(setProgress);
      if (applied) {
        setStage("ready"); // installed — offer the restart that applies it.
      } else {
        // The updater manifest offered nothing newer: fall back to the installer in the browser and step back.
        await openLatestInstaller();
        setStage("idle");
      }
    } catch {
      // The in-app update failed (network, signature, disk). Fall back to the installer so the user is not stuck,
      // and drop back to idle so they can retry.
      try { await openLatestInstaller(); } catch { /* leave the banner up to retry by hand */ }
      setStage("idle");
    }
  };

  // Applying the update means the process this is running in ends, and no session in it comes back —
  // the same loss the way out of the app names, so it is named the same way (`../shell/HoldingAsk`).
  // With no pane open there is nothing to say and the press restarts; with panes open but nothing
  // reserved it is the plain confirmation, and the box only comes up to name reservations by number.
  const onRestart = async () => {
    try {
      if (await openPanes() === 0) { await restartApp(); return; }
      const holding = await heldByAll();
      if (holding.length > 0) { setRestartAsking(holding); return; }
      if (!await confirmDialog(t("restart.confirm"))) return;
      await restartApp();
    } catch { /* the relaunch did not take; the banner stays up to retry */ }
  };

  const pct = stage === "working" && progress?.phase === "downloading" && progress.total
    ? Math.round((progress.downloaded / progress.total) * 100)
    : null;

  return (
    // A version being available asks nothing: nothing is broken and nothing is waiting on the reader,
    // so it is the quiet band and a status rather than an alert. It wore the same orange ground and the
    // same `alert` as the integrity check, which is what made that one easy to close without reading.
    <div className="healthbanner" role="status">
      <Icon name="arrowUp" size="lg" />
      <div className="healthbanner__body">
        <div className="healthbanner__title">{t("update.title")}{vs.newerVersion ? ` (${vs.newerVersion})` : ""}</div>
        <div className="healthbanner__hint">
          {stage === "working" ? updatePhaseHint(progress) : stage === "ready" ? t("update.ready") : t("update.hint")}
        </div>
        {stage === "working" && (
          <div style={{ height: 6, marginTop: 6, background: "var(--c-sunken)", borderRadius: 3, overflow: "hidden" }}>
            <div style={{ width: pct !== null ? `${pct}%` : "100%", height: "100%", background: "var(--c-accent)" }} />
          </div>
        )}
      </div>
      {inTauri() && stage === "idle" && (
        <button className="healthbanner__action" onClick={onUpdate}>{t("update.open")}</button>
      )}
      {inTauri() && stage === "ready" && (
        <button className="healthbanner__action" onClick={onRestart}>{t("update.restart")}</button>
      )}
      {/* No dismiss while the download/install is running — walking away mid-swap is exactly what we do not offer. */}
      {stage !== "working" && (
        <DismissButton
          onClick={() => { dismissUpdate(vs.newerVersion); setDismissed(vs.newerVersion); }}
          label={t("update.dismiss")}
        />
      )}
      {restartAsking !== null && (
        <HoldingAsk
          holding={restartAsking}
          words={restartWords()}
          onHandBack={async () => {
            // One at a time, so a refusal stops at the one it refused: the tasks after it are still
            // held, and the box says so rather than restarting on top of them.
            for (const id of restartAsking) await setStatus(id, "todo");
            await restartApp();
          }}
          onLeave={async () => { await restartApp(); }}
          onCancel={() => setRestartAsking(null)}
        />
      )}
    </div>
  );
}

// The manual "check for updates" menu action reports here. While the fresh check runs it says so (`checking`); after
// that it shows nothing when an update was found — the UpdateBanner above is the standing offer — and a short-lived
// "up to date" / "couldn't check" note otherwise. The note auto-dismisses because it is only an acknowledgement, unlike
// an available update, which stays up until acted on or dismissed. Hidden while `state` is null, and outside Tauri the
// menu event never fires, so it never appears there.
export function UpdateCheckFeedback({
  state,
  onDismiss,
}: {
  state: "checking" | "uptodate" | "error" | null;
  onDismiss: () => void;
}) {
  const appVersion = useSyncExternalStore(subscribe, () => getSnapshot().versionStatus.appVersion);
  useEffect(() => {
    if (state !== "uptodate" && state !== "error") return;
    const id = setTimeout(onDismiss, 6000);
    return () => clearTimeout(id);
  }, [state, onDismiss]);

  if (!state) return null;
  if (state === "checking") {
    return (
      <div className="healthbanner" role="status">
        <Icon name="refresh" size="lg" />
        <div className="healthbanner__body">
          <div className="healthbanner__title">{t("update.checking")}</div>
        </div>
      </div>
    );
  }
  const failed = state === "error";
  return (
    // Not having been able to ask is worth knowing — the reader asked and got no answer — where being up
    // to date is the answer they wanted, and asks nothing.
    <div className={failed ? "healthbanner healthbanner--heed" : "healthbanner"} role="status">
      {failed ? <Icon name="warning" size="lg" /> : <Icon name="check" size="lg" />}
      <div className="healthbanner__body">
        <div className="healthbanner__title">
          {failed ? t("update.checkFailed") : tf("update.upToDate", { version: appVersion })}
        </div>
      </div>
      <DismissButton onClick={onDismiss} />
    </div>
  );
}
