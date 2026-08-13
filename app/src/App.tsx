import { useCallback, useEffect, useState } from "react";
import { AppShell } from "./shell/AppShell";
import { MigrationScreen } from "./screens/MigrationScreen";
import { RestartGate } from "./screens/RestartGate";
import { StoreProvider } from "./store/store";
import { installReconcileTriggers, loadSnapshot, watchStore } from "./core/snapshot";
import { settleLanguage } from "./core/mutations";
import { isFormatAhead, subscribeFormatAhead } from "./core/formatAhead";
import { migrationGate } from "./core/migration";
import { errText, t } from "./core/i18n";
import type { MigrationStatusDto } from "./bindings/bindings";

/**
 * The root. At startup it fetches enough for every screen from core (the Tauri command `snapshot`)
 * before mounting the shell, because StoreProvider and dataAdapter read the snapshot cache
 * synchronously on mount (outside Tauri they fall back to fixtures). But it looks at the startup
 * migration before a single byte of the store is read: while a migration is due, core's
 * `migrate::gate()` blocks every open, so we go to the migration screen without ever calling
 * `loadSnapshot` (`migration` is `undefined` until decided, `null` when there is nothing to migrate).
 * A store that has moved ahead of us (`format_ahead`) can also appear after startup, written by
 * another process, so it is checked before any other branch and swaps the whole app for the restart
 * gate.
 */
export default function App() {
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ahead, setAhead] = useState(isFormatAhead);
  const [migration, setMigration] = useState<MigrationStatusDto | null | undefined>(undefined);
  const migrationDone = useCallback(() => setMigration(null), []);

  useEffect(() => subscribeFormatAhead(() => setAhead(true)), []);

  useEffect(() => {
    let alive = true;
    void migrationGate().then((s) => { if (alive) setMigration(s); });
    return () => { alive = false; };
  }, []);

  useEffect(() => {
    if (migration !== null) return; // Do not touch the store before the check, or while migrating.
    let alive = true;
    let unlisten: (() => void) | undefined;
    let stopTriggers: (() => void) | undefined;
    loadSnapshot()
      // Before the shell is mounted, so a reader whose language is still unset never sees the English
      // frame flash past on the way to their own (settleLanguage writes on a first launch only).
      .then(settleLanguage)
      .then(() => { if (alive) setReady(true); })
      .catch((e) => { if (alive) setError(errText(e)); });
    void watchStore().then((un) => { if (alive) unlisten = un; else un(); });
    stopTriggers = installReconcileTriggers();
    return () => { alive = false; unlisten?.(); stopTriggers?.(); };
  }, [migration]);

  if (ahead) return <RestartGate />;

  if (migration === undefined) {
    return <div style={{ padding: 24, fontFamily: "system-ui", opacity: 0.6 }}>{t("app.loading")}</div>;
  }
  if (migration) return <MigrationScreen initial={migration} onDone={migrationDone} />;

  if (error) {
    return (
      <div style={{ padding: 24, fontFamily: "system-ui", color: "#c0392b" }}>
        <strong>{t("app.loadError")}</strong>
        <pre style={{ whiteSpace: "pre-wrap" }}>{error}</pre>
      </div>
    );
  }

  if (!ready) {
    return <div style={{ padding: 24, fontFamily: "system-ui", opacity: 0.6 }}>{t("app.loading")}</div>;
  }

  return (
    <StoreProvider>
      <AppShell />
    </StoreProvider>
  );
}
