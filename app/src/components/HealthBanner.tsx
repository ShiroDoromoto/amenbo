import { useEffect, useState, useSyncExternalStore } from "react";
import { getSnapshot, inTauri, subscribe } from "../core/snapshot";
import { Icon } from "./Icon";
import { doctorText, t, tn } from "../core/i18n";
import { fetchPointerIssues, repairPointers } from "../core/mutations";
import type { DoctorIssueDto } from "../bindings/bindings";
import { DismissButton } from "./DismissButton";

// The banner speaks for two layers. What is inside the store (`startupHealth`) is carried by the snapshot on every
// tick, but issues with a bound folder's `.amenbo` (legacy format, or gone) are asked of core exactly once at startup
// (`pointer_issues`) — probing the environment costs an FS walk per folder, which is not a price to pay on every tick
// that tracks store changes. Broken pointers can be fixed from this banner (`repair_pointers`).
export function HealthBanner() {
  const health = useSyncExternalStore(subscribe, () => getSnapshot().startupHealth);
  const [pointers, setPointers] = useState<DoctorIssueDto[]>([]);
  const [dismissed, setDismissed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [repaired, setRepaired] = useState(0);

  useEffect(() => {
    if (!inTauri()) return;
    let alive = true;
    fetchPointerIssues()
      .then((p) => alive && setPointers(p))
      .catch(() => {}); // A failure to detect is swallowed (we just do not show that line).
    return () => {
      alive = false;
    };
  }, []);

  // A folder whose owner is not uniquely determined comes back from core as `unresolved`, so only the rows we could fix disappear and the rest remain.
  const onRepair = async () => {
    setBusy(true);
    try {
      const report = await repairPointers();
      setPointers(await fetchPointerIssues()); // Confirm they really are fixed, through the detection path itself.
      setRepaired(report.repaired.length);
    } catch {
      // On failure leave the rows where they are (never claim they are fixed).
    } finally {
      setBusy(false);
    }
  };

  const issues = [...health.issues, ...pointers];
  const lines = issues.map((i) => doctorText(i).message);
  // Which step the band takes is what the check found, not that it found something: a fault leaves the
  // store not to be trusted until it is dealt with, where a warning is the reader's to know while
  // everything keeps working. The two came out on one ground, which made the louder one worth nothing.
  const step = issues.some((i) => i.severity === "error") ? "stop" : "heed";
  if (dismissed) return null;
  if (lines.length === 0) {
    // Right after a repair, and only then, stay up to say so (with nothing at all we never render).
    if (repaired === 0) return null;
    return (
      // What was wrong is not wrong any more, so the band drops to the quiet one: it is a receipt.
      <div className="healthbanner" role="status">
        <Icon name="check" size="lg" />
        <div className="healthbanner__body">
          <div className="healthbanner__title">{tn("health.repaired", repaired)}</div>
        </div>
        <DismissButton onClick={() => setDismissed(true)} />
      </div>
    );
  }
  return (
    <div className={`healthbanner healthbanner--${step}`} role="alert">
      <Icon name={step === "stop" ? "error" : "warning"} size="lg" />
      <div className="healthbanner__body">
        <div className="healthbanner__title">{t("health.title")}</div>
        {lines.map((message, i) => (
          <div key={i} className="healthbanner__line">{message}</div>
        ))}
        {health.issues.length > 0 && <div className="healthbanner__hint">{t("health.hint")}</div>}
      </div>
      {pointers.length > 0 && (
        <button className="healthbanner__action" onClick={onRepair} disabled={busy}>
          {busy ? t("health.repairing") : t("health.repair")}
        </button>
      )}
      <DismissButton onClick={() => setDismissed(true)} disabled={busy} />
    </div>
  );
}
