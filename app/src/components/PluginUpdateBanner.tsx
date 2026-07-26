import { useEffect, useState, useSyncExternalStore } from "react";
import { errText, t, tf } from "../core/i18n";
import {
  applyAllPluginUpdates,
  applyPluginUpdate,
  dismissPluginUpdates,
  getDismissedPluginUpdates,
  pendingPluginUpdates,
  refreshPluginUpdates,
  subscribeDismissedPluginUpdates,
  usePluginUpdates,
} from "../core/pluginUpdates";
import { subscribeOutsideStore } from "../core/snapshot";

/**
 * "Some of your plugins have a newer build" — said once, under the top bar, with the button that takes it
 * (`AMB-D-359`).
 *
 * **The update is applied from here.** Taking one is the common case and it needs no judgement, so it costs
 * no navigation: the button runs the same `plugin update` core does for the CLI, gates and all. What does
 * need judgement — a build this amenbo cannot speak to, or a new schema whose `required` settings are unset —
 * is not offered as a button but named, with the way to the screen where it can be resolved. That split is
 * the whole reason this is a banner and not a notification: it is quiet when there is nothing to decide.
 *
 * **Nothing here holds a timer** (`AMB-D-331`). It re-asks on a focus return, whenever a plugin screen is
 * opened, and when the user asks explicitly — and core answers those from the catalog's freshness window, so
 * the triggers cost nothing inside the hour.
 *
 * The ✕ dismisses the **builds** currently offered, persisted (`core/pluginUpdates`): the same offer stays
 * quiet across launches, and a plugin whose catalog entry moves again comes back on its own.
 */
export function PluginUpdateBanner({ onOpenInstalled }: {
  /** Take the user to the installed screen — offered only when something has to be resolved there. */
  onOpenInstalled: () => void;
}) {
  const { updates } = usePluginUpdates();
  // Held outside the component: an explicit "check for updates" clears the dismissal, and this has to obey it.
  const dismissed = useSyncExternalStore(subscribeDismissedPluginUpdates, getDismissedPluginUpdates);
  const [busy, setBusy] = useState(false);
  // What the last run did, kept so the outcome survives the offer disappearing from under it.
  const [result, setResult] = useState<{ applied: number; failed: string[] } | null>(null);
  const [error, setError] = useState<string | null>(null);

  // The focus return that found the store unmoved: a plugin catalog does not live in the store, so nothing
  // else would refetch it there (every other reconcile ends in a full invalidation, which does).
  useEffect(() => subscribeOutsideStore(refreshPluginUpdates), []);

  // A plain report auto-clears; one carrying a failure stays until the next run, because it is the only
  // place the reason is said.
  useEffect(() => {
    if (!result || result.failed.length > 0) return;
    const id = setTimeout(() => setResult(null), 6000);
    return () => clearTimeout(id);
  }, [result]);

  const pending = pendingPluginUpdates(updates, dismissed);
  if (pending.length === 0 && !result) return null;

  const ready = pending.filter((u) => !u.hold);
  const held = pending.filter((u) => u.hold);

  const run = async (op: () => Promise<{ applied: number; failed: string[] }>) => {
    setBusy(true);
    setError(null);
    try {
      setResult(await op());
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  const onApplyOne = (name: string) =>
    run(async () => ({ applied: (await applyPluginUpdate(name)) ? 1 : 0, failed: [] }));

  const onApplyAll = () =>
    run(async () => {
      const outcomes = await applyAllPluginUpdates();
      return {
        applied: outcomes.filter((o) => o.applied).length,
        failed: outcomes.filter((o) => !o.applied).map((o) => `${o.name}: ${o.error ?? ""}`),
      };
    });

  return (
    <div className="healthbanner" role="status">
      <span className="healthbanner__icon" aria-hidden>🧩</span>
      <div className="healthbanner__body">
        <div className="healthbanner__title">
          {tf("plugins.updates.title", { count: pending.length })}
        </div>
        {pending.length > 0 && (
          <div className="healthbanner__hint">
            {pending.length === 1 && pending[0]
              ? `${pending[0].name} — ${pending[0].desc}`
              : pending.map((u) => u.name).join(", ")}
          </div>
        )}
        {/* Only what needs a decision is spelled out, and only then is a screen offered. */}
        {held.map((u) => (
          <div key={u.name} className="healthbanner__line">
            {u.hold === "incompatible"
              ? tf("plugins.updates.holdIncompatible", { name: u.name })
              : tf("plugins.updates.holdSettings", { name: u.name, keys: u.missing.join(", ") })}
          </div>
        ))}
        {result && (
          <div className="healthbanner__line">
            {tf("plugins.updates.applied", { count: result.applied })}
          </div>
        )}
        {result?.failed.map((line) => (
          <div key={line} className="healthbanner__line">{line}</div>
        ))}
        {error && <div className="healthbanner__line">{error}</div>}
      </div>
      {ready.length === 1 && ready[0] && (
        <button className="healthbanner__action" disabled={busy} onClick={() => void onApplyOne(ready[0]!.name)}>
          {busy ? t("plugins.updates.applying") : t("plugins.updates.apply")}
        </button>
      )}
      {ready.length > 1 && (
        <button className="healthbanner__action" disabled={busy} onClick={() => void onApplyAll()}>
          {busy ? t("plugins.updates.applying") : t("plugins.updates.applyAll")}
        </button>
      )}
      {held.length > 0 && (
        <button className="healthbanner__action" disabled={busy} onClick={onOpenInstalled}>
          {t("plugins.updates.open")}
        </button>
      )}
      {!busy && (
        <button
          className="healthbanner__close"
          onClick={() => {
            dismissPluginUpdates(updates);
            setResult(null);
          }}
        >
          ✕ {t("health.dismiss")}
        </button>
      )}
    </div>
  );
}
