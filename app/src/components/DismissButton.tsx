// The move that puts a banner away (`AMB-D-686`).
//
// Every banner across the top of the app ends in the same button: a cross, the word for putting it
// away, and the banner's own dark-on-warning look. What each banner keeps is *what dismissing means*
// there — a version silenced, a session's worth of quiet, a set of builds no longer offered — and that
// is the handler it passes in. The button itself is the same six times over, so it is written once.
import { Icon } from "./Icon";
import { t } from "../core/i18n";

/**
 * `label` is for the banner that puts something more particular away than "this notice" — the update
 * banner silences one version, and says so. The rest take the shared word.
 */
export function DismissButton({
  onClick, disabled = false, label,
}: { onClick: () => void; disabled?: boolean; label?: string }) {
  return (
    <button className="healthbanner__close" onClick={onClick} disabled={disabled}>
      <Icon name="close" /> {label ?? t("health.dismiss")}
    </button>
  );
}
