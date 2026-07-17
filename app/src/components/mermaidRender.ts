// The IO boundary for mermaid rendering. mermaid is a heavy dependency, so it is pulled in via a
// dynamic import and kept out of the main bundle (the extra chunk loads only when a mermaid fence
// is first drawn). This lives in its own module so that the component's tests (Mermaid.tsx) can
// vi.mock it instead of loading the real mermaid, whose SVG layout does not run under jsdom.

let initialized = false;
let seq = 0;

/** Turns mermaid source into an SVG string. Syntax errors and the like reject, and the caller
 *  falls back. securityLevel is pinned to "strict" (generated HTML is sanitized, scripts off). */
export async function renderMermaid(source: string): Promise<string> {
  const mermaid = (await import("mermaid")).default;
  if (!initialized) {
    mermaid.initialize({ startOnLoad: false, securityLevel: "strict" });
    initialized = true;
  }
  // render creates a temporary DOM node under this id, so it must be unique (monotonic is enough).
  const id = `amenbo-mermaid-${seq++}`;
  const { svg } = await mermaid.render(id, source);
  return svg;
}
