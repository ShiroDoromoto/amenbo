// The amenbo brand mark, at the top left of the TopBar. The paths are copied from
// `assets/brand/mark-small.svg` — the origin cut for sizes below 32 pixels, where the delivered
// mark's stroke is thinner than a pixel and comes out pale grey — and held inline rather than
// resolved as an asset (`AMB-D-686`), so it draws the same in the browser and in Tauri. A new
// delivery lands at that origin; this is one of the surfaces that then follows it (`AMB-D-712`).
//
// Drawn at 16, which is where that origin's unit is a pixel: the strokes sit on half-units so a
// vertical or horizontal one covers a pixel column instead of smearing over two.
//
// One colour, and it comes from outside: the origin is held as two files, black for a light ground
// and white for a dark one, and a drawing rendered inline needs neither — it takes `currentColor`, so
// the ground it is laid on names the colour (`--c-brand-mark`, tokens.css) and the theme switch
// carries the mark with it. No plate behind it: it sits on the topbar's own surface.
export function BrandMark({ size = 16 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      xmlns="http://www.w3.org/2000/svg"
      aria-label="amenbo"
      role="img"
    >
      {/* links, and the leg below the body */}
      <g fill="none" stroke="currentColor" strokeWidth="1" strokeLinecap="butt">
        <path d="M 6.380,7.500 L 3.020,7.500" />
        <path d="M 7.500,6.380 L 7.500,3.020" />
        <path d="M 8.060,6.940 L 12.540,2.460" />
        <path d="M 6.940,8.060 L 3.580,11.420" />
        <path d="M 8.060,8.060 L 12.540,12.540" />
        <path d="M 7.500,8.620 L 7.500,11.980" />
      </g>
      {/* nodes — filled at this size, the hole an outline needs being thinner than a pixel */}
      <g fill="currentColor" stroke="none">
        <path d="M 6.100,7.500 L 7.500,6.100 L 8.900,7.500 L 7.500,8.900 Z" />
        <path d="M 6.100,1.900 L 7.500,0.500 L 8.900,1.900 L 7.500,3.300 Z" />
        <path d="M 0.500,7.500 L 1.900,6.100 L 3.300,7.500 L 1.900,8.900 Z" />
        <path d="M 11.700,1.900 L 13.100,0.500 L 14.500,1.900 L 13.100,3.300 Z" />
        <path d="M 1.620,11.980 L 3.020,10.580 L 4.420,11.980 L 3.020,13.380 Z" />
        <path d="M 11.700,13.100 L 13.100,11.700 L 14.500,13.100 L 13.100,14.500 Z" />
      </g>
    </svg>
  );
}
