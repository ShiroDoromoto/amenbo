// The startup migration screen. It appears full-screen, ahead of the app proper, and only when a migration is due.
//
// Full-screen because a store mid-migration sits at a half-moved format version, and a store after a failed
// migration sits at the old one — this build must not read either (`migrate::gate()` blocks every open), so there
// is no app to show behind it.
//
// What it says:
// - While running: what is being moved, from which version to which, in how many steps, and the space the
//   pre-migration backup needs against the space available (core's `Pending`). Progress ticks come from that
//   backup. There is no cancel button — backing out would leave the version old and this build still unable to
//   open the store, which is exactly what sets this apart from the backup/restore progress modal.
// - When done: where the pre-migration backup lives (the only road back), and which old rollback points this
//   migration swept away (only the newest is kept, and it is never removed silently).
// - When it fails: the reason (core's wording) and a retry. A failed migration is rolled back whole, so the store
//   stands exactly as it did before it started; clear the obstruction (free up space, say) and the same path can
//   be walked again.
//
// This is shown before the snapshot exists, so the UI language is re-read from `config.json` (`ui_language`, as RestartGate does).
import { useEffect, useState } from "react";
import type { DataProgressDto, MigrationStatusDto } from "../bindings/bindings";
import { progressLabel, progressPct } from "../components/DataProgressModal";
import { currentLang, errLabel, normalizeLang, t, tn, tf, type Lang } from "../core/i18n";
import { invoke } from "../core/ipc";
import { listenMigrationChanged, listenMigrationProgress, mib, migrationStatus, retryMigration } from "../core/migration";
import { inTauri } from "../core/snapshot";
import { Icon } from "../components/Icon";

export function MigrationScreen({
  initial,
  onDone,
}: {
  initial: MigrationStatusDto;
  onDone: () => void;
}) {
  const [status, setStatus] = useState(initial);
  const [progress, setProgress] = useState<DataProgressDto | null>(initial.progress);
  const [lang, setLang] = useState<Lang>(currentLang);

  useEffect(() => {
    if (!inTauri()) return;
    let alive = true;
    void invoke<string | null>("ui_language")
      .then((code) => { if (alive) setLang(normalizeLang(code)); })
      .catch(() => {}); // If it cannot be read, stay on the default — better than failing to show the screen at all.
    return () => { alive = false; };
  }, []);

  // Subscribe first, then read the stage again. A one-step chain finishes in about a second — well inside the window
  // between the read that raised this screen and the subscriptions below — and the `migration-changed` carrying its
  // end is published to whoever is listening at that instant, which is nobody. The screen would then hold the last
  // thing it was told, for as long as the app is open, over a store that finished moving. The second read is what
  // closes that window; it stands down the moment an event has arrived, since from there the subscription is the
  // fresher of the two (a retry moves the stage back to `running`, and only the events carry that).
  useEffect(() => {
    let alive = true;
    let pushed = false;
    const stops: Array<() => void> = [];
    const keep = (un: () => void) => { if (alive) stops.push(un); else un(); };
    const apply = (s: MigrationStatusDto) => { setStatus(s); setProgress(s.progress); };
    void Promise.all([
      listenMigrationChanged((s) => { pushed = true; apply(s); }).then(keep),
      listenMigrationProgress(setProgress).then(keep),
    ])
      .then(() => migrationStatus())
      .then((s) => { if (alive && s && !pushed) apply(s); });
    return () => { alive = false; for (const stop of stops) stop(); };
  }, []);

  // Nothing left to move (the CLI finished the migration while we waited) — with nothing to show, go straight in.
  useEffect(() => {
    if (status.stage === "idle") onDone();
  }, [status.stage, onDone]);

  return (
    <div className="modal__overlay">
      <div className="modal__card modal__card--wide">
        <div className="modal__hero">
          <div className="modal__goose"><Icon name="goose" size="lg" /></div>
          <h2>{t(status.stage === "done" ? "migrate.doneTitle" : "migrate.title", lang)}</h2>
          <p className="muted">
            {status.stage === "done" && status.report
              ? tf("migrate.doneIntro", { version: status.report.to }, lang)
              : status.pending
                ? tf(
                    "migrate.intro",
                    { from: status.pending.from, to: status.pending.to, steps: status.pending.steps },
                    lang,
                  )
                : t("migrate.preparing", lang)}
          </p>
        </div>

        {status.stage === "running" && <Running pending={status.pending} progress={progress} lang={lang} />}
        {status.stage === "done" && status.report && <Done report={status.report} lang={lang} onDone={onDone} />}
        {status.stage === "failed" && <Failed status={status} lang={lang} />}
      </div>
    </div>
  );
}

/** While running: the space needed (core's estimate) and the progress of the pre-migration backup. Nothing to press. */
function Running({
  pending,
  progress,
  lang,
}: {
  pending: MigrationStatusDto["pending"];
  progress: DataProgressDto | null;
  lang: Lang;
}) {
  const pct = progressPct(progress);
  const label = progressLabel(progress, lang);

  return (
    <div className="migrate__body">
      {pending && (
        <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>
          {tf(
            "migrate.space",
            {
              required: mib(pending.requiredBytes),
              archive: mib(pending.archiveBytes),
              staging: mib(pending.stagingBytes),
              free: mib(pending.availableBytes),
            },
            lang,
          )}
        </div>
      )}
      <span style={{ fontSize: "var(--fs-md)" }}>{label}</span>
      <div className="migrate__bar">
        <div className="migrate__bar-fill" style={{ width: pct !== null ? `${pct}%` : "100%" }} />
      </div>
      <p className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("migrate.safety", lang)}</p>
    </div>
  );
}

/** When done: the road back (the pre-migration backup), the old rollback points swept away, and the warning about older builds. */
function Done({
  report,
  lang,
  onDone,
}: {
  report: NonNullable<MigrationStatusDto["report"]>;
  lang: Lang;
  onDone: () => void;
}) {
  return (
    <div className="migrate__body">
      {report.backupPath && (
        <div>
          <span className="settings__k">{t("migrate.backupTo", lang)}</span>
          <code className="migrate__path">{report.backupPath}</code>
        </div>
      )}
      {report.superseded.length > 0 && (
        <div className="faint" style={{ fontSize: "var(--fs-xs)" }}>
          {tn("migrate.superseded", report.superseded.length, lang)}
        </div>
      )}
      <p className="faint" style={{ fontSize: "var(--fs-xs)" }}>{t("migrate.olderBuilds", lang)}</p>
      <div className="migrate__actions">
        <button className="btn btn--primary" onClick={onDone}>{t("migrate.continue", lang)}</button>
      </div>
    </div>
  );
}

/** When it fails: that it was rolled back, why (core's wording), and the way to try again. */
function Failed({ status, lang }: { status: MigrationStatusDto; lang: Lang }) {
  const [retrying, setRetrying] = useState(false);
  return (
    <div className="migrate__body">
      <div className="restart__error">
        <strong>{t("migrate.failedTitle", lang)}</strong>
        <pre style={{ whiteSpace: "pre-wrap", margin: 0 }}>
          {status.error ? errLabel(status.error, lang) : t("migrate.failedTitle", lang)}
        </pre>
      </div>
      <div className="migrate__actions">
        <button
          className="btn btn--primary"
          disabled={retrying}
          onClick={() => { setRetrying(true); void retryMigration().catch(() => setRetrying(false)); }}
        >
          {t("migrate.retry", lang)}
        </button>
      </div>
    </div>
  );
}
