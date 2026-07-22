// The app's one Markdown renderer. Bare react-markdown follows CommonMark, where a single newline
// (\n) collapses to a space instead of a <br>, so the line breaks people typed into notes, comments
// and decision bodies never reach the screen; remark-breaks renders a single newline as a break.
// Every render site goes through this wrapper, so the plugin set cannot drift apart.
//
// GFM (tables, task lists, strikethrough) is on. remarkGfm runs first so that the table structure
// exists by the time remarkRefs walks the text nodes in each cell and turns refs into links. Raw
// HTML is not allowed (no rehype-raw).
//
// Conversational references in the body (AMB-T-NNN / AMB-D-NNN) are detected and made clickable.
// Detection is a remark plugin (syntactic — it produces link nodes) whose pattern lives in core/idref.ts;
// resolution is deferred to core's resolve_ref on click, so the number→id grammar is not reimplemented here
// and defined twice. Numbers are globally unique on the device, so no project context is needed to resolve
// one.

import { useMemo } from "react";
import ReactMarkdown, { defaultUrlTransform, type Components } from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import type { Root, Text, Link, RootContent } from "mdast";
import type { Element as HastElement, Text as HastText } from "hast";
import { useRefNav, type RefNav } from "../core/refNav";
import { resolveRef } from "../core/reads";
import { REF_RE } from "../core/idref";
import { Mermaid } from "./Mermaid";

// A reference gets its own scheme so urlTransform does not strip it; the click turns it back into the raw token.
const REF_SCHEME = "ref:";

/** Picks the reference tokens (AMB-T-NNN / AMB-D-NNN) out of a line, in order, boundary rules included. */
export function findRefTokens(text: string): { raw: string; start: number; end: number }[] {
  const out: { raw: string; start: number; end: number }[] = [];
  REF_RE.lastIndex = 0;
  for (let m = REF_RE.exec(text); m !== null; m = REF_RE.exec(text)) {
    out.push({ raw: m[0], start: m.index, end: m.index + m[0].length });
  }
  return out;
}

/** Splits a text node on its reference tokens, replacing each with a `ref:<token>` link node. */
function splitRefs(node: Text): RootContent[] {
  const value = node.value;
  const tokens = findRefTokens(value);
  if (tokens.length === 0) return [node]; // nothing matched, leave it alone
  const parts: RootContent[] = [];
  let last = 0;
  for (const { raw, start, end } of tokens) {
    if (start > last) parts.push({ type: "text", value: value.slice(last, start) });
    const link: Link = { type: "link", url: `${REF_SCHEME}${raw}`, title: null, children: [{ type: "text", value: raw }] };
    parts.push(link);
    last = end;
  }
  if (last < value.length) parts.push({ type: "text", value: value.slice(last) });
  return parts;
}

/** The remark plugin that detects references. It leaves link and code nodes alone — a ref inside an
 * existing link or a code span is not linked — and walks every other text node. */
function remarkRefs() {
  return (tree: Root) => {
    const walk = (node: { type: string; children?: RootContent[] }): void => {
      if (!node.children) return;
      if (node.type === "link" || node.type === "linkReference") return; // hands off inside an existing link
      const next: RootContent[] = [];
      for (const child of node.children) {
        if (child.type === "text") {
          next.push(...splitRefs(child));
        } else {
          walk(child as { type: string; children?: RootContent[] });
          next.push(child);
        }
      }
      node.children = next;
    };
    walk(tree);
  };
}

// urlTransform's default sanitizer would strip the ref: scheme, so let it through and leave every other URL to the default.
function refUrlTransform(url: string): string {
  return url.startsWith(REF_SCHEME) ? url : defaultUrlTransform(url);
}

/**
 * Returns the raw source if this pre>code hast node is a ```mermaid fence, and null otherwise. Only
 * then is the block swapped for a diagram; the source stays text, since the diagram is a GUI-only aid.
 */
export function mermaidSourceFromPre(node: HastElement | undefined): string | null {
  const code = node?.children?.find(
    (c): c is HastElement => c.type === "element" && c.tagName === "code",
  );
  if (!code) return null;
  const cls = code.properties?.className;
  if (!Array.isArray(cls) || !cls.includes("language-mermaid")) return null;
  return code.children
    .filter((c): c is HastText => c.type === "text")
    .map((c) => c.value)
    .join("")
    .replace(/\n$/, "");
}

/** Clicking a reference link: core resolves the id and the detail pane switches to it. Unknown or ambiguous is a no-op. */
async function openRef(raw: string, nav: RefNav): Promise<void> {
  const target = await resolveRef(raw);
  if (!target) return;
  if (target.kind === "task") nav.selectTask?.(target.id);
  else nav.selectDecision?.(target.id);
}

export function Markdown({ children }: { children: string }) {
  const nav = useRefNav();
  const components = useMemo<Components>(
    () => ({
      pre({ node, children }) {
        const src = mermaidSourceFromPre(node);
        if (src !== null) return <Mermaid source={src} />;
        return <pre>{children}</pre>;
      },
      // A wide table would blow up the fixed-width pane, so it scrolls sideways in its own wrapper.
      table({ children }) {
        return <div className="markdown__tablewrap"><table>{children}</table></div>;
      },
      a({ href, children }) {
        if (href?.startsWith(REF_SCHEME)) {
          const raw = href.slice(REF_SCHEME.length);
          return (
            <a
              className="reflink"
              role="button"
              tabIndex={0}
              onClick={(e) => { e.preventDefault(); void openRef(raw, nav); }}
              onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); void openRef(raw, nav); } }}
            >
              {children}
            </a>
          );
        }
        return <a href={href}>{children}</a>;
      },
    }),
    [nav],
  );
  return (
    <ReactMarkdown remarkPlugins={[remarkGfm, remarkBreaks, remarkRefs]} urlTransform={refUrlTransform} components={components}>
      {children}
    </ReactMarkdown>
  );
}
