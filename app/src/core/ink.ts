// The ink for a colour the user picked, rather than one we chose.
//
// The ink over a fill is held by a token per fill (`--c-on-accent` and its two siblings): three fills, three
// tokens, each value picked by hand so the pair clears AA in both themes. A project's colour sits outside that.
// It is whatever the human dialled into `<input type="color">`, so there is no pair to name ahead of time and no
// token that could hold one — which is why the letter over it was left at a flat `#fff`, and why a light colour
// made it unreadable (the default `#9aa7b2` carried the letter at 2.5:1).
//
// The rule the tokens follow still holds — white over a dark fill, a darker shade of the same hue over a light
// one. What changes is where it is applied: not at the token, but at render, where the colour is finally known.

/** The contrast WCAG asks of text — the floor the tokens are held to, applied here to the one letter. */
const AA_TEXT = 4.5;

const WHITE = "#ffffff";

type Rgb = [number, number, number];

/** `#rgb` and `#rrggbb`, and nothing else — a colour we cannot read is a ground we cannot measure. */
function parseHex(raw: string): Rgb | null {
  const s = raw.trim();
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(s);
  if (!m) return null;
  const hex = m[1].length === 3 ? [...m[1]].map((c) => c + c).join("") : m[1];
  return [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16)) as Rgb;
}

function toHex([r, g, b]: Rgb): string {
  return `#${[r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("")}`;
}

/** WCAG relative luminance. */
function luminance([r, g, b]: Rgb): number {
  const lin = (v: number) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/** The WCAG ratio between two colours, 1 (identical) to 21 (black on white). */
export function contrastRatio(a: string, b: string): number {
  const [x, y] = [parseHex(a), parseHex(b)];
  if (!x || !y) return 1;
  const [hi, lo] = [luminance(x), luminance(y)].sort((p, q) => q - p);
  return (hi + 0.05) / (lo + 0.05);
}

function toHsl([r, g, b]: Rgb): [number, number, number] {
  const [rn, gn, bn] = [r / 255, g / 255, b / 255];
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const l = (max + min) / 2;
  const d = max - min;
  if (d === 0) return [0, 0, l];
  const s = d / (1 - Math.abs(2 * l - 1));
  const h = max === rn
    ? ((gn - bn) / d + (gn < bn ? 6 : 0))
    : max === gn ? (bn - rn) / d + 2 : (rn - gn) / d + 4;
  return [h * 60, s, l];
}

function fromHsl(h: number, s: number, l: number): Rgb {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const hp = ((h % 360) + 360) % 360 / 60;
  const x = c * (1 - Math.abs((hp % 2) - 1));
  const [r, g, b] = hp < 1 ? [c, x, 0]
    : hp < 2 ? [x, c, 0]
      : hp < 3 ? [0, c, x]
        : hp < 4 ? [0, x, c]
          : hp < 5 ? [x, 0, c]
            : [c, 0, x];
  const m = l - c / 2;
  return [r, g, b].map((v) => Math.round((v + m) * 255)) as Rgb;
}

/**
 * The ink to draw over `background`: white where white clears AA, otherwise the lightest shade of the ground's
 * own hue that does.
 *
 * The search walks lightness down from the ground itself and stops at the first step that clears the floor, so
 * the ink stays as close to the colour as readability allows rather than falling to black every time. It always
 * finds one: where white fails, the ground is light enough that black clears AA by itself, and black is the last
 * step. A ground written in some form we cannot read (the field is a free string — nothing stops another client
 * from putting `rebeccapurple` in it) leaves nothing to measure, so it keeps the white it had.
 */
export function inkOn(background: string): string {
  const ground = parseHex(background);
  if (!ground) return WHITE;
  if (contrastRatio(background, WHITE) >= AA_TEXT) return WHITE;

  const [h, s, l] = toHsl(ground);
  for (let step = Math.round(l * 100); step >= 0; step--) {
    const ink = toHex(fromHsl(h, s, step / 100));
    if (contrastRatio(background, ink) >= AA_TEXT) return ink;
  }
  return "#000000";
}
