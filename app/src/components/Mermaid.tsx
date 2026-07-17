// Mermaid rendering, GUI only: a ```mermaid fence is drawn as an SVG diagram. The CLI keeps showing
// the raw source.
//
// Fault tolerance is not optional here: a diagram with broken syntax must not take the whole screen
// down, so there are two lines of defence.
//  1. The expected path: mermaid.render rejects asynchronously, and .catch drops us to the fallback.
//  2. The unexpected one: a synchronous throw during rendering is caught by an **error boundary**
//     (MermaidErrorBoundary).
// Both converge on the same fallback — no diagram, the raw source shown instead.

import { Component, useEffect, useState, type ReactNode } from "react";
import { renderMermaid } from "./mermaidRender";

/** The fallback when rendering fails: the raw source instead of the diagram, so nothing is lost (the CLI shows the same). */
function MermaidFallback({ source }: { source: string }) {
  return (
    <div className="mermaid mermaid--failed" role="img" aria-label="図の描画に失敗しました">
      <p className="mermaid__note">図の描画に失敗しました</p>
      <pre className="mermaid__source">
        <code>{source}</code>
      </pre>
    </div>
  );
}

/** Renders the diagram asynchronously: the SVG on success, the fallback on failure, and until then the raw source (so SSR and the first paint still say something). */
function MermaidDiagram({ source }: { source: string }) {
  const [svg, setSvg] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setFailed(false);
    setSvg(null);
    renderMermaid(source).then(
      (out) => { if (!cancelled) setSvg(out); },
      () => { if (!cancelled) setFailed(true); },
    );
    return () => { cancelled = true; };
  }, [source]);

  if (failed) return <MermaidFallback source={source} />;
  if (svg) return <div className="mermaid mermaid--ready" role="img" dangerouslySetInnerHTML={{ __html: svg }} />;
  // Before the render lands (first paint, SSR), show the raw source.
  return (
    <div className="mermaid mermaid--pending">
      <pre className="mermaid__source">
        <code>{source}</code>
      </pre>
    </div>
  );
}

/** The error boundary: a synchronous throw from the child (the diagram render) drops to the fallback instead of taking the app down. */
class MermaidErrorBoundary extends Component<{ source: string; children: ReactNode }, { crashed: boolean }> {
  state = { crashed: false };
  static getDerivedStateFromError() {
    return { crashed: true };
  }
  render() {
    if (this.state.crashed) return <MermaidFallback source={this.props.source} />;
    return this.props.children;
  }
}

/** The way in from Markdown: the diagram, wrapped in the error boundary. */
export function Mermaid({ source }: { source: string }) {
  return (
    <MermaidErrorBoundary source={source}>
      <MermaidDiagram source={source} />
    </MermaidErrorBoundary>
  );
}
