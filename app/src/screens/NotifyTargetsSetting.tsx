import { useState } from "react";
import { errText, t, tf } from "../core/i18n";
import {
  addNotifyTarget, checkNotifyTarget, deleteNotifyTarget, saveNotifyTarget, setDefaultNotifyTarget,
  testNotifyTarget, useNotifyTargets,
  NOTIFY_KINDS, type NotifyKind, type NotifyTarget, type NotifyTargetEdit,
} from "../core/notifyTargets";
import { confirmDialog } from "../core/dialog";
import { ErrorNote } from "../components/ErrorNote";
import { DoneNote } from "../components/DoneNote";
import { asTyped, isEnterSubmit } from "../core/keys";
import { openExternalUrl } from "../core/mutations";
import { Kind, kindLabel } from "../components/NotifyKind";

// Settings > Notification targets: **the device's shelf** (`AMB-D-885`).
//
// A connection is written here once under a name, and a project selects from the shelf rather than
// holding one of its own — so a webhook that changes is one edit, and reading where a project's
// notifications go takes one screen. What a project does with the shelf (on or off, which targets, which
// events) is the project's settings, not this.
//
// **The kind is drawn twice, as a colour and as a word** (`components/NotifyKind`), which is what the left
// edge of every row carries.
//
// **A credential is never drawn.** A row says whether it holds one, never what it is, so the box for it is
// masked and starts empty on a target that has one: leaving it that way keeps what is saved
// (`core/notifyTargets`).

/** The line under a target's name: what it connects to, in the part that is not a credential. */
function connectionSummary(target: NotifyTarget): string {
  if (target.kind === "slack") return t(target.secretSet ? "notify.webhookSet" : "notify.webhookUnset");
  if (!target.smtpHost) return t("notify.mailUnset");
  const server = target.smtpPort ? `${target.smtpHost}:${target.smtpPort}` : target.smtpHost;
  return target.smtpUser ? `${server} · ${target.smtpUser}` : server;
}

/** Which row the section is standing on: the shelf, a target being edited, or a kind being raised. */
type Standing = { at: "shelf" } | { at: "edit"; id: number } | { at: "new"; kind: NotifyKind };

export function NotifyTargetsSetting() {
  const { targets, loading, error } = useNotifyTargets();
  const [standing, setStanding] = useState<Standing>({ at: "shelf" });

  if (standing.at === "new") {
    return <TargetForm kind={standing.kind} onDone={() => setStanding({ at: "shelf" })} />;
  }
  if (standing.at === "edit") {
    const target = targets.find((row) => row.id === standing.id);
    // The row went while the form was open (deleted from the CLI, or another window). Fall back to the
    // shelf rather than drawing a form over nothing.
    if (target) return <TargetForm target={target} onDone={() => setStanding({ at: "shelf" })} />;
  }

  return (
    <>
      {error !== undefined && <ErrorNote tone="quiet">{errText(error)}</ErrorNote>}
      {targets.length > 0 && (
        <div className="shelf">
          {targets.map((target) => (
            <div className="shelf__row" key={target.id}>
              <span className="shelf__kind"><Kind kind={target.kind} /></span>
              <span className="shelf__grow">
                <span className="shelf__name">{target.name}</span>
                <span className="shelf__sub">{connectionSummary(target)}</span>
              </span>
              {target.isDefault && <span className="chip chip--accent">{t("notify.default")}</span>}
              <button className="btn" onClick={() => setStanding({ at: "edit", id: target.id })}>
                {t("notify.edit")}
              </button>
            </div>
          ))}
        </div>
      )}
      {targets.length === 0 && !loading && <span className="settings__fine">{t("notify.empty")}</span>}
      <div className="settings__row">
        <select
          className="btn"
          value=""
          aria-label={t("notify.add")}
          onChange={(e) => setStanding({ at: "new", kind: e.target.value as NotifyKind })}
        >
          <option value="" disabled>{t("notify.add")}</option>
          {NOTIFY_KINDS.map((kind) => (
            <option value={kind} key={kind}>{kindLabel(kind)}</option>
          ))}
        </select>
      </div>
      <span className="settings__fine">{t("notify.defaultNote")}</span>
    </>
  );
}

/**
 * One target's connection, filled in whole and saved whole.
 *
 * It opens on a target being edited, or on a `kind` being raised — and the difference is one call: a new
 * one is put on the shelf first, because a credential needs a row to hang off before it can be written
 * (`ops::notify::add_target`).
 */
function TargetForm({ target, kind, onDone }: {
  target?: NotifyTarget;
  kind?: NotifyKind;
  onDone: () => void;
}) {
  const at = target?.kind ?? kind!;
  const [name, setName] = useState(target?.name ?? kindLabel(at));
  const [smtpHost, setSmtpHost] = useState(target?.smtpHost ?? "");
  const [smtpPort, setSmtpPort] = useState(target?.smtpPort ? String(target.smtpPort) : "587");
  const [smtpUser, setSmtpUser] = useState(target?.smtpUser ?? "");
  const [mailFrom, setMailFrom] = useState(target?.mailFrom ?? "");
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);
  // What the last press answered, in one line. It is one piece of state and not three, because the
  // three presses answer the same question — did that work — and two answers standing at once is a
  // screen saying the connection was accepted next to a saying it was refused.
  const [said, setSaid] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function run(what: () => Promise<void>) {
    setBusy(true);
    setError(null);
    setSaid(null);
    try {
      await what();
    } catch (err) {
      setError(errText(err));
    } finally {
      setBusy(false);
    }
  }

  async function save() {
    if (name.trim() === "") return;
    await run(async () => {
      const port = Number.parseInt(smtpPort, 10);
      const edit: NotifyTargetEdit = {
        name: name.trim(),
        // A mail target's four; core ignores them on a Slack row, and the form does not draw them there.
        ...(at === "mail"
          ? { smtpHost, smtpPort: Number.isNaN(port) ? undefined : port, smtpUser, mailFrom }
          : {}),
        // An untouched box sends nothing, which is what keeps the saved credential where it is.
        ...(secret === "" ? {} : { secret }),
      };
      const id = target?.id ?? (await addNotifyTarget(at, edit.name))?.id;
      if (id === undefined) return;
      await saveNotifyTarget(id, edit);
      setSecret("");
      setSaid(t("notify.saved"));
      if (target === undefined) onDone();
    });
  }

  /**
   * Ask whether the connection is usable. What that could mean differs by kind, and the answer says
   * which it was — a mail relay was spoken to and the account offered to it, a Slack webhook had only
   * its URL read, and a screen that claimed the second was the first would be telling somebody a
   * revoked webhook still works.
   */
  async function check() {
    if (!target) return;
    await run(async () => {
      const found = await checkNotifyTarget(target.id);
      setSaid(t(found?.reached === true ? "notify.checkReached" : "notify.checkShape"));
    });
  }

  /** Post one message, which is the only thing that answers whether the connection still works. */
  async function sendTest() {
    if (!target) return;
    await run(async () => {
      await testNotifyTarget(target.id);
      setSaid(t(at === "slack" ? "notify.testSentSlack" : "notify.testSentMail"));
    });
  }

  async function remove() {
    if (!target) return;
    if (!(await confirmDialog(tf("notify.deleteConfirm", { name: target.name })))) return;
    await run(async () => {
      await deleteNotifyTarget(target.id);
      onDone();
    });
  }

  return (
    <>
      <div className="settings__row">
        <span className="settings__k">{t("notify.name")}</span>
        <span className="settings__v">
          <span style={{ display: "flex", alignItems: "center", gap: "var(--s-2)" }}>
            <Kind kind={at} />
            <input
              {...asTyped}
              className="btn"
              value={name}
              disabled={busy}
              aria-label={t("notify.name")}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => { if (isEnterSubmit(e)) void save(); }}
            />
          </span>
          <span className="settings__fine">{t("notify.nameNote")}</span>
        </span>
      </div>

      {at === "slack" ? (
        <SecretRow
          label={t("notify.webhook")}
          value={secret}
          held={target?.secretSet === true}
          disabled={busy}
          onChange={setSecret}
          onSubmit={save}
        />
      ) : (
        <>
          <TextRow label={t("notify.smtpHost")} value={smtpHost} disabled={busy} onChange={setSmtpHost} onSubmit={save} />
          <TextRow label={t("notify.smtpPort")} value={smtpPort} disabled={busy} onChange={setSmtpPort} onSubmit={save} />
          <TextRow label={t("notify.smtpUser")} value={smtpUser} disabled={busy} onChange={setSmtpUser} onSubmit={save} />
          <SecretRow
            label={t("notify.smtpPassword")}
            value={secret}
            held={target?.secretSet === true}
            disabled={busy}
            onChange={setSecret}
            onSubmit={save}
          >
            <span className="settings__fine">{t("notify.appPassword")}</span>
            <span>
              <button
                className="btn"
                onClick={() => void openExternalUrl("https://myaccount.google.com/apppasswords")}
              >
                {t("notify.appPasswordLink")}
              </button>
            </span>
          </SecretRow>
          <TextRow
            label={t("notify.mailFrom")}
            value={mailFrom}
            disabled={busy}
            placeholder={t("notify.mailFromPlaceholder")}
            onChange={setMailFrom}
            onSubmit={save}
          />
        </>
      )}

      <div className="settings__row">
        <span className="settings__k" />
        <span className="settings__v">
          <span style={{ display: "flex", gap: "var(--s-2)", flexWrap: "wrap", alignItems: "center" }}>
            <button className="btn btn--primary" disabled={busy || name.trim() === ""} onClick={() => void save()}>
              {t("notify.save")}
            </button>
            <button className="btn" disabled={busy} onClick={onDone}>{t("notify.cancel")}</button>
            {/* Both want a row to read the connection off, so neither is drawn before the first save. */}
            {target && (
              <button className="btn" disabled={busy} onClick={() => void check()}>
                {t("notify.check")}
              </button>
            )}
            {target && (
              <button className="btn" disabled={busy} onClick={() => void sendTest()}>
                {t("notify.test")}
              </button>
            )}
            {target && !target.isDefault && (
              <button className="btn" disabled={busy} onClick={() => void run(() => setDefaultNotifyTarget(target.id))}>
                {t("notify.makeDefault")}
              </button>
            )}
            {target && (
              <button className="btn btn--danger" disabled={busy} onClick={() => void remove()}>
                {t("notify.delete")}
              </button>
            )}
            {said !== null && <DoneNote>{said}</DoneNote>}
          </span>
          {target && (
            <>
              <span className="settings__fine">{tf("notify.usedBy", { n: target.projectsUsing })}</span>
              <span className="settings__fine">{t("notify.deleteLoses")}</span>
            </>
          )}
          {error !== null && <ErrorNote tone="quiet">{error}</ErrorNote>}
        </span>
      </div>
    </>
  );
}

/** One plain field of a connection. */
function TextRow({ label, value, disabled, placeholder, onChange, onSubmit }: {
  label: string;
  value: string;
  disabled: boolean;
  placeholder?: string;
  onChange: (v: string) => void;
  onSubmit: () => Promise<void>;
}) {
  return (
    <div className="settings__row">
      <span className="settings__k">{label}</span>
      <span className="settings__v">
        <input
          {...asTyped}
          className="btn"
          value={value}
          disabled={disabled}
          placeholder={placeholder}
          aria-label={label}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={(e) => { if (isEnterSubmit(e)) void onSubmit(); }}
        />
      </span>
    </div>
  );
}

/**
 * The credential's box. It is masked, and it starts empty even where one is held — the value cannot be
 * read back, so there is nothing to put in it. What is under it says which of the two states the target
 * is in, and that leaving the box alone keeps what is saved.
 */
function SecretRow({ label, value, held, disabled, onChange, onSubmit, children }: {
  label: string;
  value: string;
  held: boolean;
  disabled: boolean;
  onChange: (v: string) => void;
  onSubmit: () => Promise<void>;
  children?: React.ReactNode;
}) {
  return (
    <div className="settings__row">
      <span className="settings__k">{label}</span>
      <span className="settings__v">
        <input
          {...asTyped}
          type="password"
          className="btn"
          value={value}
          disabled={disabled}
          aria-label={label}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={(e) => { if (isEnterSubmit(e)) void onSubmit(); }}
        />
        <span className="settings__fine">{t(held ? "notify.keepSecret" : "notify.needSecret")}</span>
        {children}
      </span>
    </div>
  );
}
