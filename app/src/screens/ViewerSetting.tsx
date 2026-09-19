import { useState } from "react";
import { errText, t, tf, tn } from "../core/i18n";
import { exactLabel, whenLabel } from "../core/i18n/format";
import {
  cutOffViewer, issueViewerCode, repairViewer, sendToViewer, setUpViewer, setViewerCarrying,
  useViewerApps, useViewerPairing, useViewerState,
  type ViewerCode, type ViewerState,
} from "../core/viewer";
import { confirmDialog } from "../core/dialog";
import { openExternalUrl } from "../core/mutations";
import { ErrorNote } from "../components/ErrorNote";
import { DoneNote } from "../components/DoneNote";
import { QrCode } from "../components/QrCode";
import { asTyped, isEnterSubmit } from "../core/keys";

// Settings > Viewer: **the phone that reads this store, and the server it reads from** (`AMB-D-884`).
//
// The seven numbered items the plugin's form used to carry were never settings — they were a procedure,
// and they were numbered because there was nowhere else to put an order. Here the order is the layout:
// what this device is doing now, then the switch, then the things a person presses in the order they
// press them.
//
// **It is the device's, not a project's.** One account, one key, one read code — so nothing on this
// screen asks which project it is about.
//
// **Three things it must say out loud**, none of which the old form said:
//   - taking the read code away stops *every* phone. There is one code, so a single phone cannot be
//     taken off (`viewer::pairing::cut_off`).
//   - how many phones are reading is not a number anybody has. The server never learns which phone
//     offered the code, so the pairing line says whether any may read and stops there.
//   - standing the server up again with new keys makes every record already on it unreadable, so every
//     phone has to be paired again. Which of the two happened is what the press answers with.
//
// **The server's address, the API token and the encryption key are not drawn.** They were read-only rows
// on the old form; they are written by the press that stands the server up, and there is nothing for a
// person to check about them.

export function ViewerSetting() {
  const { state, error } = useViewerState();
  const [standing, setStanding] = useState<"settings" | "setup">("settings");

  if (standing === "setup") {
    return <SetUpForm state={state} onDone={() => setStanding("settings")} />;
  }
  return (
    <>
      {error !== undefined && <ErrorNote tone="quiet">{errText(error)}</ErrorNote>}
      <StateRow state={state} />
      <SyncRow state={state} />
      <ServerRow state={state} onSetUp={() => setStanding("setup")} />
      {state.setUp && <UnpairRow />}
      {state.setUp && <TroubleRow />}
      <DetailsRow />
    </>
  );
}

/** What this device is doing now: whether it carries, whether a phone may read, and how far behind it is. */
function StateRow({ state }: { state: ViewerState }) {
  // Whether a phone may read is the server's answer and arrives over the network, so it fills in behind
  // the rest rather than holding the first paint (`core/viewer`).
  const { pairing, loading } = useViewerPairing();
  const paired = !state.setUp
    ? t("viewer.noServer")
    : loading || pairing === null
      ? t("viewer.pairingAsking")
      : t(pairing.paired ? "viewer.paired" : "viewer.notPaired");

  return (
    <div className="settings__row">
      <span className="settings__k">{t("viewer.state")}</span>
      <span className="settings__v">
        <span>
          {t(state.carrying ? "viewer.syncing" : "viewer.notSyncing")}
          {t("common.listSeparator")}
          {paired}
        </span>
        <span className="settings__fine">
          {state.lastPlacedAt === undefined ? (
            t("viewer.neverPlaced")
          ) : (
            <span title={exactLabel(state.lastPlacedAt)}>
              {tf("viewer.lastPlaced", { when: whenLabel(state.lastPlacedAt) })}
            </span>
          )}
          {" "}
          {tn("viewer.waiting", state.waiting)}
        </span>
        {/* When the code the server is holding was issued. It is the server's own word for it, and the
            only thing besides "somebody may read" that a read code can be asked about. */}
        {pairing?.issuedAt !== undefined && (
          <span className="settings__fine">
            {tf("viewer.codeIssued", { date: whenLabel(pairing.issuedAt) })}
          </span>
        )}
      </span>
    </div>
  );
}

/** The switch this device carries under — every project on this PC, or none of them. */
function SyncRow({ state }: { state: ViewerState }) {
  const [error, setError] = useState<string | null>(null);
  const change = async (on: boolean) => {
    setError(null);
    try {
      await setViewerCarrying(on);
    } catch (err) {
      setError(errText(err));
    }
  };
  return (
    <div className="settings__row">
      <span className="settings__k">{t("viewer.sync")}</span>
      <span className="settings__v">
        <span>
          <select
            className="btn"
            value={state.carrying ? "on" : "off"}
            aria-label={t("viewer.sync")}
            onChange={(e) => void change(e.target.value === "on")}
          >
            <option value="on">{t("viewer.on")}</option>
            <option value="off">{t("viewer.off")}</option>
          </select>
        </span>
        <span className="settings__fine">{t("viewer.syncNote")}</span>
        {error !== null && <ErrorNote tone="quiet">{error}</ErrorNote>}
      </span>
    </div>
  );
}

/** The server, the pairing code, and where the app is got — the three presses, in the order they go. */
function ServerRow({ state, onSetUp }: { state: ViewerState; onSetUp: () => void }) {
  const apps = useViewerApps();
  const [code, setCode] = useState<ViewerCode | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /**
   * Draw a new read code. It replaces whatever the server was holding, so the phone that had the one
   * before stops reading — which is why the sentence under the button says so before it is pressed, and
   * why pairing a second phone and re-pairing after a lost one are both this same press.
   */
  const issue = async () => {
    setBusy(true);
    setError(null);
    try {
      setCode(await issueViewerCode());
    } catch (err) {
      setError(errText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="settings__row">
      <span className="settings__k">{t("viewer.server")}</span>
      <span className="settings__v">
        <span>
          <button className="btn btn--primary" onClick={onSetUp}>
            {t(state.setUp ? "viewer.recreate" : "viewer.createServer")}
          </button>
        </span>
        <span className="settings__fine">{buildLine(state)}</span>

        {state.setUp && (
          <>
            <span>
              <button className="btn" disabled={busy} onClick={() => void issue()}>
                {t(code === null ? "viewer.showQr" : "viewer.showQrAgain")}
              </button>
            </span>
            {code !== null && (
              <>
                <QrCode text={code.carried} label={t("viewer.pairDevice")} />
                <span className="settings__fine">{t("viewer.codeDrawn")}</span>
                <span className="settings__fine">{t("viewer.codeCarriesKey")}</span>
                <span className="settings__fine">{t("viewer.codeReplaced")}</span>
              </>
            )}
            <span className="settings__fine">{t("viewer.howMany")}</span>
            {error !== null && <ErrorNote tone="quiet">{error}</ErrorNote>}
          </>
        )}

        {apps.length > 0 && (
          <>
            <span className="settings__fine">{t("viewer.appNote")}</span>
            <span style={{ display: "flex", gap: "var(--s-3)", flexWrap: "wrap" }}>
              {apps.map((app) => (
                <span key={app.phone} style={{ display: "grid", justifyItems: "center", gap: "var(--s-1)" }}>
                  <QrCode text={app.link} label={app.phone} className="qrcode qrcode--sm" />
                  {/* The phone is a brand and is not translated: it is the word a reader matches
                      against the thing in their hand. */}
                  <span className="settings__fine">{app.phone}</span>
                </span>
              ))}
            </span>
          </>
        )}
      </span>
    </div>
  );
}

/**
 * Which build the server is running, said beside the one this Amenbo carries.
 *
 * **The number travels on the answer to a write and nowhere else**, so a device that has never sent has
 * never been told one — and zero is that, not an old Worker. Pressing setup is what moves it, and
 * pressing it is the reader's to do, so this says where the two stand rather than doing anything about it.
 */
function buildLine(state: ViewerState): string {
  if (!state.setUp) return t("viewer.noServerNote");
  if (state.serverBuild === 0) return t("viewer.buildUnknown");
  if (state.serverBuild < state.workerBuild) {
    return tf("viewer.buildBehind", { have: state.serverBuild, want: state.workerBuild });
  }
  return tf("viewer.buildCurrent", { n: state.serverBuild });
}

/** Taking the read code away — one code, so every phone at once. */
function UnpairRow() {
  const [busy, setBusy] = useState(false);
  const [said, setSaid] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const cut = async () => {
    if (!(await confirmDialog(t("viewer.cutOffConfirm")))) return;
    setBusy(true);
    setError(null);
    setSaid(null);
    try {
      const gone = await cutOffViewer();
      setSaid(t(gone === true ? "viewer.cutOffDone" : "viewer.cutOffNothing"));
    } catch (err) {
      setError(errText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="settings__row">
      <span className="settings__k">{t("viewer.unpair")}</span>
      <span className="settings__v">
        <span>
          <button className="btn btn--danger" disabled={busy} onClick={() => void cut()}>
            {t("viewer.cutOff")}
          </button>
        </span>
        <span className="settings__fine">{t("viewer.cutOffNote")}</span>
        {said !== null && <DoneNote>{said}</DoneNote>}
        {error !== null && <ErrorNote tone="quiet">{error}</ErrorNote>}
      </span>
    </div>
  );
}

/** The two presses for a device that is not keeping up: carry now, and put right what has drifted. */
function TroubleRow() {
  const [busy, setBusy] = useState(false);
  const [said, setSaid] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // What the last repair counted, which is what the next press consents to. It is the screen's own
  // memory of the sentence it just showed — core keeps the count too, and the press that spends it says
  // so either way.
  const [counted, setCounted] = useState(false);

  async function run(what: () => Promise<string | null>) {
    setBusy(true);
    setError(null);
    setSaid(null);
    try {
      setSaid(await what());
    } catch (err) {
      setError(errText(err));
    } finally {
      setBusy(false);
    }
  }

  /** One turn, asked for by hand. Neither reason a turn does nothing is a failure. */
  const send = () =>
    run(async () => {
      const sent = await sendToViewer();
      if (sent === null) return null;
      if (sent.heldBack === "another_turn") return t("viewer.sentAnotherTurn");
      if (sent.heldBack === "switched_off") return t("viewer.sentSwitchedOff");
      return tf("viewer.sentPlaced", { placed: sent.placed, waiting: sent.waiting });
    });

  /**
   * Compare the two ends, and on the press that spends it, place the difference. The first press counts
   * and says the number; the second is the consent to that number, which is why it is two presses and
   * not one.
   */
  const repair = () =>
    run(async () => {
      const done = await repairViewer(counted);
      if (done === null) return null;
      switch (done.outcome) {
        case "counted":
          setCounted(true);
          return tf("viewer.repairCounted", {
            place: done.drift?.toPlace ?? 0,
            drop: done.drift?.toDrop ?? 0,
          });
        case "placed":
          setCounted(false);
          return tf("viewer.repairPlaced", {
            placed: done.sent?.placed ?? 0,
            waiting: done.sent?.waiting ?? 0,
          });
        case "level":
          setCounted(false);
          return t("viewer.repairLevel");
        case "sending_elsewhere":
          return t("viewer.repairElsewhere");
        case "not_set_up":
          return t("viewer.noServer");
      }
    });

  return (
    <div className="settings__row">
      <span className="settings__k">{t("viewer.trouble")}</span>
      <span className="settings__v">
        <span style={{ display: "flex", gap: "var(--s-2)", flexWrap: "wrap" }}>
          <button className="btn" disabled={busy} onClick={() => void send()}>
            {t("viewer.send")}
          </button>
          <button className="btn" disabled={busy} onClick={() => void repair()}>
            {t(counted ? "viewer.repairAgain" : "viewer.repair")}
          </button>
        </span>
        <span className="settings__fine">{t("viewer.repairNote")}</span>
        {said !== null && <DoneNote>{said}</DoneNote>}
        {error !== null && <ErrorNote tone="quiet">{error}</ErrorNote>}
      </span>
    </div>
  );
}

/** What the old form drew as four read-only rows, said in two sentences instead. */
function DetailsRow() {
  return (
    <div className="settings__row">
      <span className="settings__k">{t("viewer.details")}</span>
      <span className="settings__v">
        <span className="settings__fine">{t("viewer.detailsWritten")}</span>
        <span className="settings__fine">{t("viewer.detailsKeys")}</span>
      </span>
    </div>
  );
}

/**
 * Standing the server up in the reader's own Cloudflare account.
 *
 * **The token is asked for and not kept.** It builds the Worker and the database and is gone with the
 * press; what stays behind is the address, the write token and the key, none of which can create
 * anything in that account.
 *
 * The account box is here because a token that reaches more than one account is one core will not choose
 * for them — it answers with the accounts it found and asks which. Left blank, which is the usual case,
 * nothing is asked.
 *
 * **The name box is the way past the one refusal this press can meet.** Where the reader names nothing
 * and a server of the usual name is already standing in that account that this store holds no key to,
 * core will not build over it — the owner of that server would stop being able to write to it and
 * would be told nothing (`AMB-D-930`). Both ways past it are a name: another one stands a second server
 * up, and the usual one said out loud is the reader saying that server is theirs. Without this box the
 * refusal would point at a flag only the CLI beside the app carries.
 */
function SetUpForm({ state, onDone }: { state: ViewerState; onDone: () => void }) {
  const [token, setToken] = useState("");
  const [account, setAccount] = useState("");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [said, setSaid] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const stand = async () => {
    if (token.trim() === "") return;
    setBusy(true);
    setError(null);
    setSaid(null);
    try {
      const stood = await setUpViewer(
        token.trim(),
        account.trim() === "" ? undefined : account.trim(),
        name.trim() === "" ? undefined : name.trim(),
      );
      // The token is spent. Clearing it is not tidiness: the box is on screen until the reader leaves,
      // and what is in it is the one credential that can create things in their account.
      setToken("");
      if (stood !== null) setSaid(t(stood.keys === "kept" ? "viewer.stoodKept" : "viewer.stoodGenerated"));
    } catch (err) {
      setError(errText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <span className="settings__fine">{t(state.setUp ? "viewer.recreateNote" : "viewer.createNote")}</span>

      <div className="settings__row">
        <span className="settings__k">{t("viewer.token")}</span>
        <span className="settings__v">
          <input
            {...asTyped}
            type="password"
            className="btn"
            value={token}
            disabled={busy}
            aria-label={t("viewer.token")}
            onChange={(e) => setToken(e.target.value)}
            onKeyDown={(e) => { if (isEnterSubmit(e)) void stand(); }}
          />
          <span className="settings__fine">{t("viewer.tokenNote")}</span>
          <span>
            <button className="btn" disabled={busy} onClick={() => void openExternalUrl(state.tokenLink)}>
              {t("viewer.tokenLink")}
            </button>
          </span>
        </span>
      </div>

      <div className="settings__row">
        <span className="settings__k">{t("viewer.account")}</span>
        <span className="settings__v">
          <input
            {...asTyped}
            className="btn"
            value={account}
            disabled={busy}
            aria-label={t("viewer.account")}
            placeholder={t("viewer.accountPlaceholder")}
            onChange={(e) => setAccount(e.target.value)}
            onKeyDown={(e) => { if (isEnterSubmit(e)) void stand(); }}
          />
          <span className="settings__fine">{t("viewer.accountNote")}</span>
        </span>
      </div>

      <div className="settings__row">
        <span className="settings__k">{t("viewer.name")}</span>
        <span className="settings__v">
          <input
            {...asTyped}
            className="btn"
            value={name}
            disabled={busy}
            aria-label={t("viewer.name")}
            placeholder={t("viewer.namePlaceholder")}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => { if (isEnterSubmit(e)) void stand(); }}
          />
          <span className="settings__fine">{t("viewer.nameNote")}</span>
        </span>
      </div>

      <div className="settings__row">
        <span className="settings__k" />
        <span className="settings__v">
          <span style={{ display: "flex", gap: "var(--s-2)", flexWrap: "wrap", alignItems: "center" }}>
            <button className="btn btn--primary" disabled={busy || token.trim() === ""} onClick={() => void stand()}>
              {t("viewer.create")}
            </button>
            <button className="btn" disabled={busy} onClick={onDone}>{t("viewer.back")}</button>
          </span>
          {said !== null && <DoneNote>{said}</DoneNote>}
          {error !== null && <ErrorNote tone="quiet">{error}</ErrorNote>}
        </span>
      </div>
    </>
  );
}
