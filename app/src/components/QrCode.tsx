// A QR code, drawn as Amenbo's own paint.
//
// **Nothing here decodes or fetches anything.** It is handed a string and draws the squares, which is
// what lets a code stand on a settings form without an image file, a temporary path, or an operating
// system that has to have something registered for the type.
//
// Two surfaces draw one: the Viewer's settings, where the code is the reader's own pairing code and the
// two store links beside it, and a plugin's form, where the string is the author's.
import { useMemo } from "react";
import qrcode from "qrcode-generator";

/**
 * The code for `text`.
 *
 * **It is black on white whatever the reader's theme is**: a camera reads contrast, and a QR inverted for
 * a dark background is one many scanners will not take.
 *
 * Drawn as one SVG path rather than a square per module — the same picture in one node instead of a
 * thousand — and sized in `em` off the box it stands in, so it grows with the form rather than being
 * pinned to a pixel count that is wrong on the next display.
 *
 * A string too long to encode draws nothing rather than throwing the form away: whatever words stand
 * beside it are still there to read.
 */
export function QrCode({ text, label, className = "qrcode" }: {
  text: string;
  label: string;
  className?: string;
}) {
  const drawn = useMemo(() => qrModules(text), [text]);
  if (!drawn) return null;
  const { count, path } = drawn;
  // One module of quiet zone on each side — less than the spec's four, which is what a camera wants when
  // the code is printed. On a screen the form's own whitespace is the margin.
  const span = count + 2;
  return (
    <svg
      className={className}
      viewBox={`0 0 ${span} ${span}`}
      role="img"
      aria-label={label}
      shapeRendering="crispEdges"
    >
      <rect width={span} height={span} fill="#fff" />
      <path d={path} fill="#000" transform="translate(1 1)" />
    </svg>
  );
}

/** The dark modules of `text`, as an SVG path, or `null` for a string this format cannot carry. */
function qrModules(text: string): { count: number; path: string } | null {
  try {
    // `0` picks the smallest version the string fits in, and `M` is the correction level a code read off
    // a screen wants — the higher levels buy recovery from damage a screen does not have.
    const code = qrcode(0, "M");
    code.addData(text);
    code.make();
    const count = code.getModuleCount();
    let path = "";
    for (let row = 0; row < count; row++) {
      for (let col = 0; col < count; col++) {
        if (code.isDark(row, col)) path += `M${col} ${row}h1v1h-1z`;
      }
    }
    return { count, path };
  } catch (e) {
    console.error("[amenbo] a QR was asked for of a string that will not encode:", e);
    return null;
  }
}
