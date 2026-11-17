// The screen the setup-again band opens: the Viewer's server moved up with nothing but an API token.
//
// **It is the same setup as Settings › Viewer, pared down to the one box.** No account and no name: the
// press goes to `setUpViewer(token)`, and core reads the rest off what this store already holds — the keys,
// and the address the name is read back from (`AMB-D-930`). With those kept it is the same server, so core
// sends nothing again and the day's allowance is not spent on it (`AMB-D-1161`). That is why the screen can
// promise that the data and the phones stay as they are.
//
// **Settings keeps its own "create the server again".** That one asks for everything; this one is for the
// reader who only has to move the Worker up.
//
// The one refusal this press can meet that the other screen gets past is a token reaching more than one
// account: core will not choose for the reader. There is no account box here, so the refusal is shown and
// the reader is pointed at Settings, where there is one.
//
// It says nothing of the cleanup the new Worker makes possible — no progress and no stretch of time
// (`AMB-D-1160`). The band goes down with the press itself: core writes down the build it placed
// (`AMB-D-1167`), and the band is asked again once the press is answered.
import { useEffect, useState } from "react";
import { errText, t, tf } from "../core/i18n";
import { asTyped, isEnterSubmit } from "../core/keys";
import { openExternalUrl } from "../core/mutations";
import { recheckSetupAgain, setUpViewer, useViewerState } from "../core/viewer";
import { DoneNote } from "../components/DoneNote";
import { ErrorNote } from "../components/ErrorNote";

export function ViewerUpdate({ onClose }: { onClose: () => void }) {
  const { state } = useViewerState();
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [said, setSaid] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Escape closes, except while the press is out: closing then would hide how it went.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  const update = async () => {
    if (token.trim() === "") return;
    setBusy(true);
    setError(null);
    setSaid(null);
    try {
      const stood = await setUpViewer(token.trim());
      // The token is spent, and it is the one credential that can create things in the account.
      setToken("");
      if (stood !== null) {
        setSaid(t(stood.keys === "kept" ? "viewer.update.done" : "viewer.stoodGenerated"));
      }
      recheckSetupAgain();
    } catch (err) {
      setError(errText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal__overlay">
      <div className="viewerupdate__modal" role="dialog" aria-modal="true" aria-labelledby="viewerupdate-title">
        <div className="viewerupdate__title" id="viewerupdate-title">{t("viewer.update.title")}</div>
        <div className="viewerupdate__why">{t("viewer.update.keeps")}</div>

        <input
          {...asTyped}
          type="password"
          className="btn viewerupdate__input"
          value={token}
          disabled={busy}
          aria-label={t("viewer.token")}
          placeholder={t("viewer.token")}
          onChange={(e) => setToken(e.target.value)}
          onKeyDown={(e) => { if (isEnterSubmit(e)) void update(); }}
        />
        <div className="viewerupdate__hint">{t("viewer.tokenNote")}</div>
        <div>
          <button className="btn" disabled={busy} onClick={() => void openExternalUrl(state.tokenLink)}>
            {t("viewer.tokenLink")}
          </button>
        </div>

        {said !== null && <DoneNote>{said}</DoneNote>}
        {error !== null && (
          <>
            <ErrorNote tone="quiet">{error}</ErrorNote>
            <div className="viewerupdate__hint">
              {tf("viewer.update.elsewhere", { recreate: t("viewer.recreate") })}
            </div>
          </>
        )}

        <div className="buttonrow">
          <button className="btn btn--primary" disabled={busy || token.trim() === ""} onClick={() => void update()}>
            {t("viewer.update.run")}
          </button>
          <button className="btn" disabled={busy} onClick={onClose}>
            {t("viewer.update.close")}
          </button>
        </div>
      </div>
    </div>
  );
}
