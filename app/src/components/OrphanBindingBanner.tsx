import { useEffect, useState } from "react";
import { inTauri } from "../core/snapshot";
import { Icon } from "./Icon";
import { t, tn } from "../core/i18n";
import { fetchOrphanBindings, forgetOrphanBindings } from "../core/mutations";
import { DismissButton } from "./DismissButton";

// Point out the bound-folder wreckage a deleted project left in the index (rows no live project claims) and forget
// them from the index in one click (the same core path as CLI `doctor --fix`, `forget_orphan_dirs`). The GUI's folder
// list is a reverse lookup per project, so a row with no claimant never shows up there. This drops the index row and
// nothing more — it touches neither the folder's contents nor its `.amenbo`. Detected once at startup, dismissible
// with the ✕ for the session. Outside Tauri (in the browser) it is always empty, hence hidden.
export function OrphanBindingBanner() {
  const [dirs, setDirs] = useState<string[]>([]);
  const [dismissed, setDismissed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);

  useEffect(() => {
    if (!inTauri()) return;
    let alive = true;
    fetchOrphanBindings()
      .then((d) => alive && setDirs(d))
      .catch(() => {}); // A failure to detect is swallowed (we just do not show the banner).
    return () => {
      alive = false;
    };
  }, []);

  if (dismissed || dirs.length === 0) return null;

  const onForget = async () => {
    setBusy(true);
    try {
      await forgetOrphanBindings();
      const remaining = await fetchOrphanBindings(); // Check they really were swept (rows added concurrently can remain).
      setDirs(remaining);
      setDone(remaining.length === 0);
    } catch {
      // On failure leave the banner up (dirs stays as it was).
    } finally {
      setBusy(false);
    }
  };

  if (done) {
    return (
      <div className="healthbanner managedblock-banner" role="status">
        <span className="healthbanner__icon" aria-hidden>✓</span>
        <div className="healthbanner__body">
          <div className="healthbanner__title">{t("orphanBinding.done")}</div>
        </div>
        <DismissButton onClick={() => setDismissed(true)} />
      </div>
    );
  }

  return (
    <div className="healthbanner managedblock-banner" role="alert">
      <Icon name="warning" size="lg" />
      <div className="healthbanner__body">
        <div className="healthbanner__title">{t("orphanBinding.title")}</div>
        <div className="healthbanner__line">{tn("orphanBinding.hint", dirs.length)}</div>
      </div>
      <button className="healthbanner__action" onClick={onForget} disabled={busy}>
        {busy ? t("orphanBinding.forgetting") : t("orphanBinding.forget")}
      </button>
      <DismissButton onClick={() => setDismissed(true)} disabled={busy} />
    </div>
  );
}
