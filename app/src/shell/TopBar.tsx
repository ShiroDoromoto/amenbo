import { useEffect, useRef, useState } from "react";
import { BrandMark } from "../components/BrandMark";
import { reconcile, subscribeStoreChangeReflected } from "../core/snapshot";
import { fetchDevBadge, openExternalUrl } from "../core/mutations";
import { t } from "../core/i18n";
import { Icon } from "../components/Icon";

/** The product page, opened in the default browser by clicking "Amenbo" in the TopBar. */
const PRODUCT_URL = "https://amenbo.work/";

/** How long the rainbow flash lasts (ms). Must match the CSS animation-duration. */
const REFLECT_FLASH_MS = 700;

export function TopBar({
  onBack,
  onForward,
  canBack,
  canForward,
  sidebarCollapsed,
  onToggleSidebar,
}: {
  onBack: () => void;
  onForward: () => void;
  canBack: boolean;
  canForward: boolean;
  sidebarCollapsed: boolean;
  onToggleSidebar: () => void;
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
      <button
        className={`topbar__navbtn topbar__sidebartoggle${sidebarCollapsed ? " topbar__sidebartoggle--collapsed" : ""}`}
        title={sidebarCollapsed ? t("sidebar.expand") : t("sidebar.collapse")}
        aria-label={sidebarCollapsed ? t("sidebar.expand") : t("sidebar.collapse")}
        aria-pressed={sidebarCollapsed}
        onClick={onToggleSidebar}
      >
        <Icon name="menu" />
      </button>
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
        Amenbo
      </span>
    </div>
  );
}
