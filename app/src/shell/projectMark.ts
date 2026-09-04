// What a project is drawn as where there is no room for its name — the colour it was given and the
// first character of what it is called (`AMB-D-838`).
//
// **The colour is the person's and any colour at all.** It is picked with a colour well in the
// project's settings (`../screens/ProjectSettingsScreen`), so nothing in the application chose it and
// nothing may assume it is dark. A letter written on it in one fixed colour is a letter that
// disappears on half of what a person can pick, so the ink is worked out from the ground it lands on.

/** The channels of `#rgb` or `#rrggbb`, or `null` where the colour is not one this can read. */
function channelsOf(color: string): [number, number, number] | null {
  const hex = color.trim().replace(/^#/, "");
  const full = hex.length === 3 ? [...hex].map((one) => one + one).join("") : hex;
  if (!/^[0-9a-fA-F]{6}$/.test(full)) return null;
  return [0, 2, 4].map((at) => parseInt(full.slice(at, at + 2), 16)) as [number, number, number];
}

/** One channel, off the curve the screen encodes it with and onto the light it stands for. */
function light(value: number): number {
  const share = value / 255;
  return share <= 0.03928 ? share / 12.92 : ((share + 0.055) / 1.055) ** 2.4;
}

/**
 * The ink to write on a project's colour, or `null` where the colour cannot be read.
 *
 * The threshold is where black and white stand at the same contrast against the ground (WCAG's
 * relative luminance, 0.179), so whichever side of it a colour falls on, the letter is on the more
 * readable of the two rather than on the one that happens to be the default.
 *
 * **`null` is not a colour.** A ground that could not be read is not drawn either — the mark falls
 * back to the face's own surface — and a letter told to be white on it would be a white letter on
 * whatever the theme's ground is. Nothing said is the theme's text colour, which is the answer that
 * holds in both themes.
 */
export function inkOn(color: string): string | null {
  const rgb = channelsOf(color);
  if (rgb === null) return null;
  const [r, g, b] = rgb.map(light);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.179 ? "#111" : "#fff";
}

/**
 * The character a project is known by where its name will not fit.
 *
 * The first one the name is written with, and by character rather than by `[0]`: a name that opens
 * with an emoji or with anything outside the basic plane is two units long in one character, and
 * half of it is not a letter. A project named with nothing but spaces has no character to show, and
 * nothing is drawn rather than a placeholder standing where a letter would be.
 */
export function initialOf(name: string): string {
  return [...name.trim()][0] ?? "";
}
