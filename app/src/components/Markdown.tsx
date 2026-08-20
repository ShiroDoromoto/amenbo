// The app's one Markdown renderer. Bare react-markdown follows CommonMark, where a single newline
// (\n) collapses to a space instead of a <br>, so the line breaks people typed into notes, comments
// and decision bodies never reach the screen; remark-breaks renders a single newline as a break.
// Every render site goes through this wrapper, so the plugin set cannot drift apart.
//
// GFM (tables, task lists, strikethrough) is on. remarkGfm runs first so that the table structure
// exists by the time remarkRefs walks the text nodes in each cell and turns refs into links. Raw
// HTML is not allowed (no rehype-raw).
//
// Nothing here ever navigates the window. The app is a single page with no address bar and no way
// back, so a followed link either strands the user outside Amenbo or — for a link that resolves
// against Amenbo's own origin — reloads the SPA and drops it back at its opening screen. So a link is
// sorted by what it names: a ref opens in the app, an http(s) URL opens in the browser, `#section`
// stays in the document, and anything else (a relative path, an unknown scheme) is drawn as its text.
//
// A relative path is the one of those that can be recovered, and only where the body came from
// somewhere: a plugin's README is read out of a repository, so its `LICENSE` and `./docs/x.md` name
// files that exist and have a URL. `linkBase` is that place, passed by the render site that knows it —
// a note, a comment and a decision body have no such origin, so they leave it unset and a relative
// link there stays inert.
//
// An image is never drawn as one. Amenbo's own bodies keep images in attachments rather than inline
// (`conventions.markdown`), and a body from outside — a plugin's README — cannot draw one either: the
// app's CSP allows no remote image, so the browser would put a broken-image frame where the picture
// was. What still carries meaning is the alt text, most visibly in the badge row a README opens with,
// so an image is rendered as that text in a small label.
//
// Headings carry an id (rehype-slug), so a body's own `#section` link has something to land on. The
// ids are for this app only: they are recomputed on every render, nothing stores them, and no URL
// outside the app can point at one — the app has no address bar to paste it into. Since a screen
// shows several bodies at once (a task's notes and each of its comments), and two of them may head a
// section the same way, an anchor is resolved inside the body it was written in and nowhere else.
//
// Conversational references in the body (AMB-T-NNN / AMB-D-NNN) are detected and made clickable.
// Detection is a remark plugin (syntactic — it produces link nodes) whose pattern lives in core/idref.ts;
// resolution is deferred to core's resolve_ref on click, so the number→id grammar is not reimplemented here
// and defined twice. Numbers are globally unique on the device, so no project context is needed to resolve
// one.

import { useMemo, type ReactNode } from "react";
import ReactMarkdown, { defaultUrlTransform, type Components } from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import rehypeSlug from "rehype-slug";
import type { Root, Text, Link, RootContent } from "mdast";
import type { Element as HastElement, Text as HastText } from "hast";
import { useRefNav, type RefNav } from "../core/refNav";
import { openExternalUrl } from "../core/mutations";
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

/** An image, or the link a badge wraps itself in (`[![CI](badge.svg)](ci-url)`) — one label either way. */
function isBadge(node: RootContent | undefined): boolean {
  if (!node) return false;
  if (node.type === "image" || node.type === "imageReference") return true;
  return (node.type === "link" || node.type === "linkReference") &&
    node.children.length === 1 && isBadge(node.children[0]);
}

/**
 * Put a badge row back on one line. A README writes its badges one per source line, which CommonMark
 * joins into a single line — but remark-breaks (above) turns every one of those newlines into a break,
 * and the row comes out as a column of labels. So a break next to a badge is dropped, which leaves the
 * breaks people actually typed into their prose untouched.
 */
function remarkBadgeRows() {
  return (tree: Root) => {
    const walk = (node: { type: string; children?: RootContent[] }): void => {
      if (!node.children) return;
      if (node.type === "paragraph") {
        node.children = node.children.filter(
          (child, i, all) => child.type !== "break" || !(isBadge(all[i - 1]) || isBadge(all[i + 1])),
        );
      }
      for (const child of node.children) walk(child as { type: string; children?: RootContent[] });
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

/** Whether a link leaves the app: `http(s)` only, so the `ref:` scheme and in-document anchors stay put. */
export function isExternalHref(href: string): boolean {
  return /^https?:\/\//i.test(href.trim());
}

/**
 * Resolves a body's relative target against the place the body came from, and returns null when there
 * is no such place or the target does not name a file under it.
 *
 * Landing outside the base is the interesting case: `../../elsewhere`, a root-relative `/owner/other`
 * and a foreign scheme all resolve to something, and none of them is a file of this repository. So
 * containment is the test rather than the syntax of the path — what the base cannot vouch for is left
 * to be drawn as text, exactly as if there were no base at all.
 */
export function resolveAgainstBase(href: string, base: string | undefined): string | null {
  if (!base) return null;
  try {
    const resolved = new URL(href, base).href;
    return resolved.startsWith(base) ? resolved : null;
  } catch {
    return null; // not a URL even with a base to lean on
  }
}

/** The id a `#…` href names. A hand-written anchor can arrive percent-encoded, while the id rehype-slug
 * puts on the heading never is, so the href is what gets decoded. */
function anchorId(href: string): string {
  const raw = href.slice(1);
  try {
    return decodeURIComponent(raw);
  } catch {
    return raw; // a stray % is not an encoding — take the text as written
  }
}

/**
 * The element a `#…` link names, searched within one rendered body. Ids are compared as values rather
 * than handed to a selector: a heading slugged from Japanese, or from a title carrying punctuation,
 * makes an id no `#…` selector would accept without escaping.
 */
export function findAnchorTarget(root: ParentNode, href: string): HTMLElement | null {
  const id = anchorId(href);
  if (!id) return null;
  for (const el of root.querySelectorAll<HTMLElement>("[id]")) if (el.id === id) return el;
  return null;
}

/** Following an anchor: scroll to the heading, inside the body the link sits in (every render site wraps
 * this component in `.markdown`). An anchor naming no heading here does nothing, which is the honest
 * answer — the window is never navigated, so there would be no way back from a jump. */
function scrollToAnchor(from: HTMLElement, href: string): void {
  const root: ParentNode = from.closest(".markdown") ?? from.ownerDocument;
  findAnchorTarget(root, href)?.scrollIntoView({ block: "start", behavior: "smooth" });
}

/** Clicking a reference link: core resolves the id and the detail pane switches to it. Unknown or ambiguous is a no-op. */
async function openRef(raw: string, nav: RefNav): Promise<void> {
  const target = await resolveRef(raw);
  if (!target) return;
  if (target.kind === "task") nav.selectTask?.(target.id);
  else nav.selectDecision?.(target.id);
}

/** A link that leaves Amenbo: it keeps its href (copy-link, and what the status bar shows) and the
 * click is diverted to the browser instead of the webview. */
function browserLink(href: string, children: ReactNode) {
  return (
    <a href={href} onClick={(e) => { e.preventDefault(); void openExternalUrl(href); }}>
      {children}
    </a>
  );
}

export function Markdown({ children, linkBase }: {
  children: string;
  /**
   * The absolute URL this body's relative paths are read against — a plugin README's repository, and
   * nothing else so far. Left unset, a relative link stays inert (see the note at the top).
   */
  linkBase?: string;
}) {
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
      // The alt text in place of the picture. An image with no alt says nothing without its pixels, so
      // it leaves nothing behind either. The title carries where the picture is, which for a relative
      // one is only an answer once it is resolved — the label is all the reader has left of it.
      img({ alt, src }) {
        if (!alt) return null;
        const raw = typeof src === "string" ? src : undefined;
        const title = raw && !isExternalHref(raw) ? (resolveAgainstBase(raw, linkBase) ?? raw) : raw;
        return <span className="markdown__imgalt" title={title}>{alt}</span>;
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
        // An http(s) link goes to the browser, never to this window. The app window has no address
        // bar and no way back, so letting the webview follow a link would strand the user outside
        // Amenbo with the app gone — and a rendered README (`AMB-D-347`) is mostly such links.
        if (href && isExternalHref(href)) return browserLink(href, children);
        // A link inside the document (`#section`) stays a link, and following it only moves the scroll
        // position. The jump is made here rather than left to the browser, whose own is document-wide:
        // with several bodies on the screen it would land on whichever heading slugged that way first,
        // which need not be one in the body being read. It is not resolved against the base, even where
        // there is one — the heading it names is on this screen, so sending the reader out to the
        // repository to read what is already in front of them would be the wrong answer.
        if (href?.startsWith("#")) {
          return (
            <a href={href} onClick={(e) => { e.preventDefault(); scrollToAnchor(e.currentTarget, href); }}>
              {children}
            </a>
          );
        }
        // A relative link — a README's `LICENSE`, `./docs/x.md` — must never be followed as written: it
        // resolves against *this app's* origin, so the webview lands on Amenbo's own index.html, the
        // whole SPA reloads and comes back at its opening screen, which reads as the detail closing
        // itself. Resolved against the repository it was written in, it names a real file and goes to
        // the browser like any other outside link.
        const resolved = href ? resolveAgainstBase(href, linkBase) : null;
        if (resolved) return browserLink(resolved, children);
        // Anything left has nowhere to go — no base to resolve against, or a target that base cannot
        // vouch for — so it is drawn as the text it carries and does not pretend to be a link.
        return <span className="markdown__deadlink">{children}</span>;
      },
    }),
    [nav, linkBase],
  );
  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm, remarkBreaks, remarkBadgeRows, remarkRefs]}
      // Slugging happens on the hast tree, after remarkRefs has already turned any reference in a
      // heading into a link node — the id is read off the heading's text either way, so a heading that
      // names a task slugs the same as the words it reads.
      rehypePlugins={[rehypeSlug]}
      urlTransform={refUrlTransform}
      components={components}
    >
      {children}
    </ReactMarkdown>
  );
}
