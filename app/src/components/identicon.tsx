import type { CSSProperties } from "react";

// Deterministic, server-less identicon: a 5×5 left-right-symmetric glyph whose
// pattern and hue derive purely from a seed. Same seed → same image on every device,
// so each facet gets a stable, unique avatar with no upload and no network — matching
// Amenbo's no-server identity model. FacetAvatar seeds it per facet (by kind), so
// a person's human and AI read as distinct glyphs without any badge (see FacetAvatar).

/** FNV-1a (32-bit): small, deterministic, well-spread over short keys. */
function hash32(seed: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < seed.length; i++) {
    h ^= seed.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

export function Identicon({ seed, size = 18 }: { seed: string; size?: number }) {
  const h = hash32(seed || "?");
  const grid = 5;
  const cell = size / grid;
  const rects = [];
  // Columns 0..2 drive the pattern; columns 3,4 mirror 1,0 for a vertical axis of
  // symmetry (the classic identicon look). Each cell's on/off is one hash bit.
  for (let col = 0; col < 3; col++) {
    for (let row = 0; row < grid; row++) {
      if (((h >> (col * grid + row)) & 1) === 0) continue;
      const xs = col === 2 ? [2] : [col, grid - 1 - col];
      for (const x of xs) {
        rects.push(
          <rect key={`${x}-${row}`} x={x * cell} y={row * cell} width={cell} height={cell} />,
        );
      }
    }
  }
  return (
    <svg
      className="identicon"
      // The hue rides in as a property because it is this glyph's alone; how strong and how light it
      // is drawn comes off the tokens beside it, which a skin may move (`.identicon` in components.css).
      style={{ "--identicon-h": String(h % 360) } as CSSProperties}
      viewBox={`0 0 ${size} ${size}`}
      width={size}
      height={size}
      aria-hidden="true"
    >
      {rects}
    </svg>
  );
}
