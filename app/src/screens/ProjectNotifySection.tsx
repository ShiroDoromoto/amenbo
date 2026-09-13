import { useEffect, useState } from "react";
import { errText, t } from "../core/i18n";
import {
  selectProjectTarget, setProjectMailTo, setProjectNotifyEnabled, setProjectNotifyEvent,
  useProjectNotify,
} from "../core/projectNotify";
import { useNotifyTargets, type NotifyTarget } from "../core/notifyTargets";
import { NotifyTargetsSetting } from "./NotifyTargetsSetting";
import { KindDot } from "../components/NotifyKind";
import { ErrorNote } from "../components/ErrorNote";
import { asTyped } from "../core/keys";

// Project settings > Notifications: **what this project does with the device's shelf** (`AMB-D-885`).
//
// What a person chooses here is *what they are told about*, never *what carries it*: the carrier is a
// target's own property, and a target is picked from the shelf rather than described again. So no
// connection field appears on this screen — no webhook, no server, no password — and the one thing that
// looks like a connection setting, the mail address, is not one: it answers **who is told**, which is this
// project's business.
//
// **The switch is apart from the selection.** A fortnight away costs one switch and finds the targets and
// the ticks still standing on the way back; turning notifications off is not undoing the setting up.
//
// Every control writes as it is pressed. There is no save button, because the store holds these one field
// at a time and a form collecting them would have to invent a half-written state for something that has
// none.

export function ProjectNotifySection({ projectId }: { projectId: number }) {
  const { notify } = useProjectNotify(projectId);
  const { targets } = useNotifyTargets();
  const [error, setError] = useState<string | null>(null);
  const run = (what: () => Promise<void>) => {
    setError(null);
    void what().catch((e) => setError(errText(e)));
  };

  const chosen = targets.filter((row) => notify.targetIds.includes(row.id));
  const rest = targets.filter((row) => !notify.targetIds.includes(row.id));

  return (
    <div className="settings__section">
      <div className="settings__h">{t("projset.notify")}</div>
      <div className="settings__body">
        <div className="settings__row">
          <span className="settings__k">{t("notify.projectSwitch")}</span>
          <span className="settings__v">
            <select
              className="btn"
              style={{ maxWidth: 120 }}
              value={notify.enabled ? "on" : "off"}
              aria-label={t("notify.projectSwitch")}
              onChange={(e) => run(() => setProjectNotifyEnabled(projectId, e.target.value === "on"))}
            >
              <option value="on">{t("notify.on")}</option>
              <option value="off">{t("notify.off")}</option>
            </select>
            <span className="settings__fine">{t("notify.switchNote")}</span>
          </span>
        </div>

        <div className="settings__row">
          <span className="settings__k">{t("notify.targets")}</span>
          <span className="settings__v">
            {targets.length === 0 ? (
              // Nothing on the shelf yet, so the shelf's own control is drawn here rather than the reader
              // being sent away to find it (`AMB-D-885`). What is made lands on the device all the same —
              // this is the same section as the one in the device's settings, not a second way in.
              <>
                <span className="settings__fine">{t("notify.noneOnDevice")}</span>
                <NotifyTargetsSetting />
              </>
            ) : (
              <>
                <span style={{ display: "flex", gap: "var(--s-2)", flexWrap: "wrap", alignItems: "center" }}>
                  {chosen.map((row) => (
                    <Chosen
                      key={row.id}
                      target={row}
                      onDrop={() => run(() => selectProjectTarget(projectId, row.id, false))}
                    />
                  ))}
                  {rest.length > 0 && (
                    <select
                      className="btn"
                      style={{ maxWidth: 190 }}
                      value=""
                      aria-label={t("notify.addTarget")}
                      onChange={(e) =>
                        run(() => selectProjectTarget(projectId, Number(e.target.value), true))
                      }
                    >
                      <option value="" disabled>{t("notify.addTarget")}</option>
                      {rest.map((row) => (
                        <option value={row.id} key={row.id}>{row.name}</option>
                      ))}
                    </select>
                  )}
                </span>
                <span className="settings__fine">{t("notify.targetsNote")}</span>
              </>
            )}
          </span>
        </div>

        <div className="settings__row">
          <span className="settings__k">{t("notify.events")}</span>
          <span className="settings__v">
            <div className="events">
              {notify.reportable.map((event) => (
                <label key={event}>
                  <input
                    type="checkbox"
                    checked={notify.events.includes(event)}
                    onChange={(e) => run(() => setProjectNotifyEvent(projectId, event, e.target.checked))}
                  />
                  {t(`notify.event.${event}`)}
                </label>
              ))}
            </div>
            <span className="settings__fine">{t("notify.eventsNote")}</span>
          </span>
        </div>

        {/* Only while a mail target is among the ones above: `to` is the one field a kind decides the
            meaning of, and a project sending through Slack alone has no use for it. */}
        {chosen.some((row) => row.kind === "mail") && (
          <MailTo projectId={projectId} held={notify.mailTo} onFail={setError} />
        )}

        {error !== null && <ErrorNote tone="quiet">{error}</ErrorNote>}
      </div>
    </div>
  );
}

/** One target this project's notifications are carried by, and the way to stop carrying them through it. */
function Chosen({ target, onDrop }: { target: NotifyTarget; onDrop: () => void }) {
  return (
    <span className="chip chip--dest">
      <KindDot kind={target.kind} />
      {target.name}
      <button className="chip__x" onClick={onDrop} aria-label={t("notify.dropTarget")}>×</button>
    </span>
  );
}

/**
 * Where this project's mail is addressed. It is a box rather than a press, so it is written when the
 * reader leaves it — the only control on this face that is not settled by the act of touching it.
 */
function MailTo({ projectId, held, onFail }: {
  projectId: number;
  held: string;
  onFail: (why: string) => void;
}) {
  const [draft, setDraft] = useState(held);
  // A value written elsewhere (another window, the CLI) reaches an untouched box; this is also what
  // settles the box after its own write lands.
  useEffect(() => { setDraft(held); }, [held]);

  const save = () => {
    if (draft === held) return;
    void setProjectMailTo(projectId, draft).catch((e) => onFail(errText(e)));
  };

  return (
    <div className="settings__row">
      <span className="settings__k">{t("notify.mailTo")}</span>
      <span className="settings__v">
        <input
          {...asTyped}
          className="btn"
          value={draft}
          placeholder={t("notify.mailToPlaceholder")}
          aria-label={t("notify.mailTo")}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={save}
          onKeyDown={(e) => { if (e.key === "Enter") save(); }}
        />
        <span className="settings__fine">{t("notify.mailToNote")}</span>
      </span>
    </div>
  );
}
