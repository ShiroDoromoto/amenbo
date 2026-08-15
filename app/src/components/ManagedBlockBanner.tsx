import { useEffect, useState } from "react";
import { inTauri } from "../core/snapshot";
import { Icon } from "./Icon";
import { t, tn } from "../core/i18n";
import { fetchStaleManagedBlocks, resyncManagedBlocks } from "../core/mutations";
import type { StaleBlockDto } from "../bindings/bindings";
import { DismissButton } from "./DismissButton";

// After a binary update, a bound folder's CLAUDE.md/AGENTS.md can be left holding an older version of the managed
// block. The CLI fixes itself by following along whenever it starts in that folder, but the GUI starts in no folder
// at all, so every bound folder is in scope. When the same core detection path as CLI `doctor`
// (`stale_managed_blocks`) finds stale folders, we offer a line under the TopBar that resyncs them in one click (the
// `resync_managed_blocks` path, the same one CLI `sync-guide` takes). The only side effect is rewriting the md on
// disk (low churn, language label preserved, nothing outside the markers touched); the store is untouched, so no
// snapshot refetch. Detected once at startup, dismissible with the cross for the session. Outside Tauri (in the browser)
// it is always empty, hence hidden.
export function ManagedBlockBanner() {
  const [stale, setStale] = useState<StaleBlockDto[]>([]);
  const [dismissed, setDismissed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);

  useEffect(() => {
    if (!inTauri()) return;
    let alive = true;
    fetchStaleManagedBlocks()
      .then((s) => alive && setStale(s))
      .catch(() => {}); // A failure to detect is swallowed (we just do not show the banner).
    return () => {
      alive = false;
    };
  }, []);

  if (dismissed || stale.length === 0) return null;

  // How many folders hold a stale block (a folder whose CLAUDE.md and AGENTS.md are both stale still counts once).
  const folderCount = new Set(stale.map((s) => s.dir)).size;

  const onResync = async () => {
    setBusy(true);
    try {
      const report = await resyncManagedBlocks(); // Resync every bound folder to the current version.
      const remaining = await fetchStaleManagedBlocks(); // Check they actually followed along (folders that are gone or renamed can remain).
      setStale(remaining);
      setDone(report.updated.length > 0 && remaining.length === 0);
    } catch {
      // On failure leave the banner up (stale stays as it was).
    } finally {
      setBusy(false);
    }
  };

  if (done) {
    return (
      // The offer is spent — nothing is being handed to the reader any more — so the band drops to the
      // quiet one and reads as the receipt it is, the same way the integrity check's does.
      <div className="healthbanner" role="status">
        <Icon name="check" size="lg" />
        <div className="healthbanner__body">
          <div className="healthbanner__title">{t("managedBlock.done")}</div>
        </div>
        <DismissButton onClick={() => setDismissed(true)} />
      </div>
    );
  }

  return (
    <div className="healthbanner healthbanner--offer" role="alert">
      <Icon name="warning" size="lg" />
      <div className="healthbanner__body">
        <div className="healthbanner__title">{t("managedBlock.title")}</div>
        <div className="healthbanner__line">{tn("managedBlock.hint", folderCount)}</div>
      </div>
      <button className="healthbanner__action" onClick={onResync} disabled={busy}>
        {busy ? t("managedBlock.resyncing") : t("managedBlock.resync")}
      </button>
      <DismissButton onClick={() => setDismissed(true)} disabled={busy} />
    </div>
  );
}
