import { useState } from "react";
import { currentLang, t, tf, type Lang } from "../core/i18n";

/** The part of a progress event this modal reads: the [done/total] and the phase carried by `DataProgressDto`, the
 *  mirror of core's `Progress`. */
export interface DataProgressLike {
  phase: string;
  done: number;
  total: number | null;
}

/** The bar's percentage. A tick that does not know the total (`total` is null) has no percentage, so null = indeterminate bar. */
export function progressPct(progress: DataProgressLike | null): number | null {
  return progress?.total ? Math.round((progress.done / progress.total) * 100) : null;
}

/** Turn one tick into one line a human reads. A tick from core may carry no total (a streaming export ships rows
 *  without counting them first), and then only the count is shown — `[5/0]` would claim a total of zero. Progress
 *  carries no name (the machine has a single store, and an archive does not name one either). */
export function progressLabel(progress: DataProgressLike | null, lang: Lang): string {
  if (!progress) return t("settings.dataOpPreparing", lang);
  const phase = t(`settings.dataOpPhase.${progress.phase}`, lang);
  const total = progress.total;
  return total
    ? tf("settings.dataOpProgress", { done: Math.min(progress.done + 1, total), total, phase }, lang)
    : tf("settings.dataOpProgressUnbounded", { done: progress.done + 1, phase }, lang);
}

/** The progress modal for the whole-store operations (backup/restore/export). It mirrors core's progress as
 *  "[done/total] phase" and draws a bar where it can. "Cancel" stops core at the next phase boundary, leaving nothing
 *  half-applied. It cannot be dismissed while the operation runs (clicking the overlay does nothing). The startup
 *  migration does not use this: it cannot be cancelled — walking away would leave the store on the old version — so it
 *  must never be offered a live "cancel". The migration screen mirrors the same progress shape (`DataProgressDto`)
 *  with a bar of its own. */
export function DataProgressModal({
  progress,
  onCancel,
}: {
  progress: DataProgressLike | null;
  onCancel: () => void;
}) {
  const lang = currentLang();
  const [cancelling, setCancelling] = useState(false);
  const pct = progressPct(progress);
  const label = progressLabel(progress, lang);
  return (
    <div className="modal__overlay">
      <div className="modal__card" style={{ maxWidth: 420, display: "flex", flexDirection: "column", gap: 12 }}>
        <span style={{ fontSize: "var(--fs-md)" }}>{label}</span>
        <div style={{ height: 6, background: "var(--c-border)", borderRadius: 3, overflow: "hidden" }}>
          <div style={{ width: pct !== null ? `${pct}%` : "100%", height: "100%", background: "var(--c-accent)" }} />
        </div>
        <button
          className="btn"
          disabled={cancelling}
          style={{ alignSelf: "flex-end" }}
          onClick={() => { setCancelling(true); onCancel(); }}
        >
          {cancelling ? t("settings.dataOpCancelling", lang) : t("settings.dataOpCancel", lang)}
        </button>
      </div>
    </div>
  );
}
