// Where a custom protocol lives, as a webview has to address it.
//
// Tauri serves a custom scheme at a different origin depending on the platform: `scheme://localhost`
// on macOS and Linux, `http://scheme.localhost` on Windows and Android. Every module that builds a
// URL for one of our doors has to know that, and none of them should have to know it twice.
//
// The answer comes off the user agent, which is the only thing the frontend is given without a
// plugin — the same signal `platform.ts` reads.

/** `http://<scheme>.localhost` on Windows/Android, `<scheme>://localhost` everywhere else. */
export function schemeBase(scheme: string): string {
  const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
  const isWindowsLike = /Windows|Android/i.test(ua);
  return isWindowsLike ? `http://${scheme}.localhost` : `${scheme}://localhost`;
}
