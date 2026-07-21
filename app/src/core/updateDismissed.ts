// The version whose update banner was last dismissed. A device-local setting (persisted), kept in localStorage like
// the theme (core/theme) — deliberately **not** store data: which version is installed differs per device, so syncing
// a dismissal would silence the banner on a machine that is still behind.
//
// The banner asks `isUpdateDismissed(offered)`: a dismissal covers the version it was made on and every older one, so
// it stays quiet until a *newer* version shows up upstream. Comparison mirrors core's `version_is_newer`
// (`store/mod.rs`) — pre-release / build metadata ignored, unparsable input compares as "not newer".
const KEY = "amenbo.updateDismissedVersion";

/** Loosely parse `major.minor.patch`, ignoring anything after `-` or `+`. `null` when unparsable. */
function parseVersion(v: string): [number, number, number] | null {
  const core = v.split(/[-+]/)[0] ?? v;
  const parts = core.split(".");
  const nums = [parts[0], parts[1] ?? "0", parts[2] ?? "0"].map((p) => Number((p ?? "").trim()));
  if (nums.some((n) => !Number.isInteger(n) || n < 0)) return null;
  return [nums[0]!, nums[1]!, nums[2]!];
}

/** True when `candidate` is newer than `base`. Either side unparsable → false (the safe side). */
export function versionIsNewer(candidate: string, base: string): boolean {
  const c = parseVersion(candidate);
  const b = parseVersion(base);
  if (!c || !b) return false;
  for (let i = 0; i < 3; i++) {
    if (c[i]! !== b[i]!) return c[i]! > b[i]!;
  }
  return false;
}

/** The last dismissed version, or `null` if none was ever dismissed (or localStorage is unavailable). */
export function getDismissedUpdate(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null; // no localStorage: nothing was remembered, so nothing is dismissed
  }
}

/**
 * Remember `version` as dismissed. A version-less offer (`null`) is not recorded — there is nothing to compare a
 * later offer against, so that dismissal only lasts the session.
 */
export function dismissUpdate(version: string | null): void {
  if (!version) return;
  try { localStorage.setItem(KEY, version); } catch { /* dismiss for this session even where localStorage is unavailable */ }
}

/**
 * Forget any dismissed version, so the banner is no longer silenced. The manual "check for updates" action calls this
 * when its fresh check finds an update: asking explicitly should surface the offer even if a prior dismissal covered it.
 */
export function clearDismissedUpdate(): void {
  try { localStorage.removeItem(KEY); } catch { /* nothing was persisted, so nothing to clear */ }
}

/** Whether the offer of `version` is already covered by an earlier dismissal (a newer offer is never covered). */
export function isUpdateDismissed(version: string | null): boolean {
  const dismissed = getDismissedUpdate();
  if (!dismissed) return false;
  if (!version) return false; // no version to compare: show it and let the session dismissal handle it
  return !versionIsNewer(version, dismissed);
}
