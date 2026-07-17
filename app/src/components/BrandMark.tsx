// The amenbo brand mark (a water strider, drawn circuit-style). It holds the same artwork as
// app/src-tauri/icon.svg as inline SVG, so it renders in the browser and in Tauri with no asset resolution.
// For the small brand mark at the top left of the TopBar. The rounded white plate behind it is dropped so it sits on the background.
export function BrandMark({ size = 18 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 1024 1024"
      xmlns="http://www.w3.org/2000/svg"
      aria-label="amenbo"
      role="img"
    >
      <g stroke="#1B93C2" strokeWidth="50" strokeLinecap="round" strokeLinejoin="round" fill="none">
        <path d="M482,410 L382,410 L172,200" />
        <path d="M542,410 L642,410 L852,200" />
        <path d="M482,558 L110,558" />
        <path d="M542,558 L914,558" />
        <path d="M482,706 L382,706 L172,916" />
        <path d="M542,706 L642,706 L852,916" />
      </g>
      <g fill="#1B93C2">
        <circle cx="172" cy="200" r="24" />
        <circle cx="852" cy="200" r="24" />
        <circle cx="110" cy="558" r="24" />
        <circle cx="914" cy="558" r="24" />
        <circle cx="172" cy="916" r="24" />
        <circle cx="852" cy="916" r="24" />
      </g>
      <line x1="512" y1="383" x2="512" y2="733" stroke="#1B93C2" strokeWidth="64" strokeLinecap="round" />
      <circle cx="512" cy="317" r="58" fill="#FF7E5F" />
    </svg>
  );
}
