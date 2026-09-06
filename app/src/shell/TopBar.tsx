import { useEffect, useRef, useState } from "react";
import { BrandMark } from "../components/BrandMark";
import { reconcile, subscribeStoreChangeReflected } from "../core/snapshot";
import { fetchDevBadge, openExternalUrl } from "../core/mutations";
import { t } from "../core/i18n";
import type { Face } from "../core/windowShape";
import { Icon } from "../components/Icon";

/** The product page, opened in the default browser by clicking "amenbo" in the TopBar. */
const PRODUCT_URL = "https://amenbo.work/";

/** How long the rainbow flash lasts (ms). Must match the CSS animation-duration. */
const REFLECT_FLASH_MS = 700;

export function TopBar({
  onBack,
  onForward,
  canBack,
  canForward,
  face,
  onSelectFace,
  terminalBadge,
}: {
  onBack: () => void;
  onForward: () => void;
  canBack: boolean;
  canForward: boolean;
  face: Face;
  onSelectFace: (face: Face) => void;
  terminalBadge: boolean;
}) {
  const [reflecting, setReflecting] = useState(false);
  const flashTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Which build this is, beside the brand it qualifies. Fetched once — the channel is stamped in at
  // build time — and null on production, which is the whole point: the badge only ever marks a
  // development window, so it costs a shipped user nothing and stands out where it does appear.
  const [devBadge, setDevBadge] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    fetchDevBadge()
      .then((b) => alive && setDevBadge(b))
      .catch(() => {}); // Unanswered: show no badge rather than a wrong one.
    return () => {
      alive = false;
    };
  }, []);
  useEffect(() => {
    const unsub = subscribeStoreChangeReflected(() => {
      // Debounce: writes landing during the flash fold into that one flash, so it never glows continuously.
      if (flashTimer.current !== null) return;
      setReflecting(true);
      flashTimer.current = setTimeout(() => {
        flashTimer.current = null;
        setReflecting(false);
      }, REFLECT_FLASH_MS);
    });
    return () => {
      unsub();
      if (flashTimer.current !== null) clearTimeout(flashTimer.current);
    };
  }, []);
  return (
    <div className="topbar">
      <span className="topbar__nav">
        <button
          className="topbar__navbtn"
          title={t("topbar.back")}
          aria-label={t("topbar.back")}
          disabled={!canBack}
          onClick={onBack}
        >
          <Icon name="chevronLeft" />
        </button>
        <button
          className="topbar__navbtn"
          title={t("topbar.forward")}
          aria-label={t("topbar.forward")}
          disabled={!canForward}
          onClick={onForward}
        >
          <Icon name="chevronRight" />
        </button>
      </span>
      <button
        className="topbar__refresh"
        title={t("topbar.refresh")}
        aria-label={t("topbar.refresh")}
        onClick={() => void reconcile("manual")}
      >
        <Icon name="refresh" />
      </button>
      <div className="topbar__spacer" />
      <FaceSwitch face={face} onSelect={onSelectFace} badge={terminalBadge} />
      <div className="topbar__spacer" />
      {devBadge && <span className="topbar__envbadge">{devBadge}</span>}
      <span className="topbar__brand" title="amenbo"><BrandMark /></span>
      <span
        className={`topbar__ws${reflecting ? " topbar__ws--reflect" : ""}`}
        role="link"
        tabIndex={0}
        title={t("topbar.brandLink")}
        onClick={() => void openExternalUrl(PRODUCT_URL)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            void openExternalUrl(PRODUCT_URL);
          }
        }}
      >
        amenbo
      </span>
    </div>
  );
}

/**
 * The two faces of the window, and which one is up (`AMB-D-753`).
 *
 * It stays here once the terminal has been split out into a window of its own, where pressing
 * "terminal" raises that window instead of changing this one — so the control says the same thing
 * in both shapes, and there is one place to look for the terminal however the app is arranged.
 * Which is why the terminal segment is not marked as the current one there: the face this window is
 * showing is the ledger, and it does not stop being so because the other window came forward.
 *
 * `badge` is a turn standing behind the other face (`./terminalBadge`). It is a dot and no number:
 * what is waiting is one pane's business and is said on the pane, so all this has to carry across
 * the switch is that there is something over there to go and look at. Its label is read as part of
 * the segment's own name — "Terminal, waiting on you" — so it is worded to finish that sentence
 * rather than to stand alone.
 */
function FaceSwitch({ face, onSelect, badge }: { face: Face; onSelect: (face: Face) => void; badge: boolean }) {
  return (
    <div className="topbar__faces" role="tablist" aria-label={t("face.switch")}>
      <button
        className={`topbar__face${face === "tasks" ? " topbar__face--on" : ""}`}
        role="tab"
        aria-selected={face === "tasks"}
        onClick={() => onSelect("tasks")}
      >
        {t("face.tasks")}
      </button>
      <button
        className={`topbar__face${face === "terminal" ? " topbar__face--on" : ""}`}
        role="tab"
        aria-selected={face === "terminal"}
        onClick={() => onSelect("terminal")}
      >
        {t("face.terminal")}
        {badge && <span className="topbar__face-badge" role="img" aria-label={t("face.needsYou")} title={t("face.needsYou")} />}
      </button>
    </div>
  );
}
