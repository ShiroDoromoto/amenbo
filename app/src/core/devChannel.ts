// Whether this window belongs to a development build, for the surfaces that must not offer what such a
// build cannot do.
//
// The channel already decides things on the Rust side — the updater plugin is not registered, the
// upstream release is withheld, the CLI refuses to swap itself — and each of those leaves a control on
// screen that would now swap a value nothing reads. A switch that does nothing is worse than an absent
// one: the reader concludes the setting is broken, or that the check is on when it cannot be.
//
// Only the presence of a control goes through here. Prose and badges that *say* which build this is are
// the dev badge's own business (`fetchDevBadge`), which is where this asks.
import { useEffect, useState } from "react";
import { fetchDevBadge } from "./mutations";

/**
 * Whether this is a development build. Asked once per screen that needs it — the channel is stamped in
 * at build time, so the answer cannot change while the process runs — and it stands at "production"
 * until the answer comes, so the shipped build, where that *is* the answer, never shows a change.
 */
export function useIsDevBuild(): boolean {
  const [dev, setDev] = useState(false);
  useEffect(() => {
    let alive = true;
    fetchDevBadge()
      .then((badge) => alive && setDev(badge !== null))
      .catch(() => {}); // Unanswered (the browser preview): production is the shape a reader there wants.
    return () => {
      alive = false;
    };
  }, []);
  return dev;
}
