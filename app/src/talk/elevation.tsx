// The band the talk window shows while Amenbo itself holds an administrator's token.
//
// This is the one thing that can be done about elevation, and it is worth being clear about why it
// is only this. A token is inherited and cannot be handed back: an app the user started as
// administrator starts its terminals as administrator, and `launch` refusing to elevate does not
// undo a launch that already was. On Windows that costs more than a right nobody needed — an
// elevated process will not traverse a junction a standard user made, and scoop keeps every one of
// its packages behind one, so tools that are installed are unreachable from inside the pane
// (`AMB-T-3565`).
//
// **Nothing here refuses anything.** The terminal opens, the probe answers the same way it always
// does, and the pane is the pane. What is missing without this band is not a capability but a
// reason: Amenbo says a tool is not installed, the person installed it, and the two cannot be
// reconciled from anything on screen. So the band states the fact and names the way out, and that
// is the whole of it.

import { t } from "../core/i18n";
import type { Lang } from "../core/i18n";

/**
 * The band, ready to stand above the face.
 *
 * `role="status"` rather than `alert`: what it reports has been true since the window opened and
 * will be true until the app is restarted, so it is a state of the window, not an event that
 * interrupted one. A screen reader announcing it at the next pause is the right amount of urgency.
 *
 * `lang` is for a reader that wants to name one; the window is rebuilt when the language changes
 * (`../talk.tsx`), so leaving it out is what the page itself does.
 */
export function ElevationBand({ lang }: { lang?: Lang }) {
  return (
    <div className="talk__elevated" role="status">
      <strong className="talk__elevated-title">{t("talk.elevated.title", lang)}</strong>
      <p className="talk__elevated-body">{t("talk.elevated.body", lang)}</p>
      <p className="talk__elevated-fix">{t("talk.elevated.fix", lang)}</p>
    </div>
  );
}
