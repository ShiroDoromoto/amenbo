import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { Element as HastElement } from "hast";
import { findRefTokens, isExternalHref, mermaidSourceFromPre, Markdown, resolveAgainstBase } from "./Markdown";

// SSR needs no DOM, and it pins the whole remark wiring: detection → link node → urlTransform lets it through → an a.reflink is rendered.
const render = (md: string, linkBase?: string) =>
  renderToStaticMarkup(createElement(Markdown, { children: md, linkBase }));

// A plugin README's repository, in the form the detail passes down (`core/pluginCatalog`).
const BASE = "https://github.com/owner/repo/blob/HEAD/";

describe("findRefTokens", () => {
  const raws = (s: string) => findRefTokens(s).map((r) => r.raw);

  it("picks up: AMB-T-NNN / AMB-D-NNN (with non-alphanumeric boundaries on both sides)", () => {
    expect(raws("see AMB-T-123 and AMB-D-85 plus AMB-T-45.")).toEqual(["AMB-T-123", "AMB-D-85", "AMB-T-45"]);
    expect(raws("(AMB-T-7)")).toEqual(["AMB-T-7"]);
    expect(raws("AMB-D-1, AMB-D-22")).toEqual(["AMB-D-1", "AMB-D-22"]);
    expect(raws("amb-t-7")).toEqual(["amb-t-7"]); // the kind code folds case
  });

  // The namespace is the whole point: a body's `#12` is a GitHub issue and its `T-12` may be another
  // tracker's, so linking either would hijack a reference that was never Amenbo's.
  it("does not pick up a bare #NNN, a bare T-NN, or a ref glued to alphanumerics", () => {
    expect(raws("see #123 and D-85 plus T-45.")).toEqual([]);
    expect(raws("issue#12")).toEqual([]);
    expect(raws("ABCD-3")).toEqual([]);
    expect(raws("STATUS-3")).toEqual([]);
    expect(raws("XAMB-T-3")).toEqual([]);
  });

  it("returns the correct position (the split point)", () => {
    const [tok] = findRefTokens("x AMB-T-9 y");
    expect(tok).toEqual({ raw: "AMB-T-9", start: 2, end: 9 });
  });

  it("no state leaks across calls (lastIndex reset)", () => {
    expect(raws("AMB-T-1")).toEqual(["AMB-T-1"]);
    expect(raws("AMB-T-2")).toEqual(["AMB-T-2"]);
  });
});

describe("Markdown reference-link rendering", () => {
  it("renders a reference as a.reflink (the ref: scheme is not stripped by urlTransform)", () => {
    const html = render("done AMB-T-123 per AMB-D-85");
    // The ref: scheme never survives into the href — the click handler resolves it — and the link text is the raw token.
    expect(html).toContain('class="reflink"');
    expect(html).toContain(">AMB-T-123</a>");
    expect(html).toContain(">AMB-D-85</a>");
  });

  it("non-reference prose stays a plain paragraph (no false positives)", () => {
    const html = render("just prose, issue#9 not linked");
    expect(html).not.toContain("reflink");
  });

  it("does not linkify a reference inside a code span", () => {
    const html = render("inline `AMB-T-123` code");
    expect(html).toContain("<code>AMB-T-123</code>");
    expect(html).not.toContain("reflink");
  });
});

describe("Markdown GFM extensions", () => {
  it("renders a table as <table>, wrapped for sideways scroll in the fixed-width pane", () => {
    const html = render("| a | b |\n| --- | --- |\n| 1 | 2 |");
    expect(html).toContain('<div class="markdown__tablewrap"><table>');
    expect(html).toContain("<th>a</th>");
    expect(html).toContain("<td>1</td>");
  });

  it("references inside table cells are linkified too (GFM → remarkRefs interplay)", () => {
    const html = render("| ref |\n| --- |\n| see AMB-T-123 |");
    expect(html).toContain("<table>");
    expect(html).toContain("class=\"reflink\"");
    expect(html).toContain(">AMB-T-123</a>");
  });

  it("renders a task list with checkboxes", () => {
    const html = render("- [x] done\n- [ ] todo");
    expect(html).toContain("type=\"checkbox\"");
    expect(html).toContain("checked");
  });

  it("renders strikethrough as <del>", () => {
    const html = render("~~gone~~");
    expect(html).toContain("<del>gone</del>");
  });
});

describe("mermaidSourceFromPre detection", () => {
  // A minimal reproduction of the pre>code hast structure react-markdown builds.
  const preNode = (lang: string, text: string): HastElement => ({
    type: "element",
    tagName: "pre",
    properties: {},
    children: [
      {
        type: "element",
        tagName: "code",
        properties: { className: [`language-${lang}`] },
        children: [{ type: "text", value: text }],
      },
    ],
  });

  it("a mermaid fence returns the raw source with the trailing newline removed", () => {
    expect(mermaidSourceFromPre(preNode("mermaid", "graph TD\n  A-->B\n"))).toBe("graph TD\n  A-->B");
  });

  it("a non-mermaid language is null (treated as an ordinary code block)", () => {
    expect(mermaidSourceFromPre(preNode("js", "const x = 1\n"))).toBeNull();
  });

  it("a pre with no language class is null", () => {
    const node: HastElement = {
      type: "element",
      tagName: "pre",
      properties: {},
      children: [{ type: "element", tagName: "code", properties: {}, children: [{ type: "text", value: "x" }] }],
    };
    expect(mermaidSourceFromPre(node)).toBeNull();
  });

  it("null without crashing even when the node is undefined", () => {
    expect(mermaidSourceFromPre(undefined)).toBeNull();
  });
});

describe("Markdown Mermaid rendering", () => {
  it("a mermaid fence is swapped for a diagram container, showing the raw source before it renders", () => {
    const html = render("```mermaid\ngraph TD\n  A-->B\n```");
    expect(html).toContain('class="mermaid mermaid--pending"');
    expect(html).not.toContain("language-mermaid");
    expect(html).toContain("graph TD");
  });

  it("a non-mermaid code fence renders as usual (regression)", () => {
    const html = render("```js\nconst x = 1\n```");
    expect(html).toContain('class="language-js"');
    expect(html).toContain("const x = 1");
    expect(html).not.toContain("mermaid");
  });
});

describe("external links", () => {
  // The app window has no address bar and no way back, so a link that leaves Amenbo must go to the
  // browser. Everything else — the ref scheme, an anchor — stays in the document.
  it("recognises only http(s) as leaving the app", () => {
    expect(isExternalHref("https://github.com/owner/repo")).toBe(true);
    expect(isExternalHref("  http://example.invalid/x  ")).toBe(true);
    expect(isExternalHref("ref:AMB-T-1")).toBe(false);
    expect(isExternalHref("#section")).toBe(false);
    expect(isExternalHref("./relative.md")).toBe(false);
    expect(isExternalHref("javascript:alert(1)")).toBe(false);
  });

  // Diverting the click must not cost the link its href: it is still what a copy-link does, and what
  // the status bar shows. (The click handler itself is a DOM behaviour, out of SSR's reach.)
  it("keeps the href on an external link", () => {
    expect(render("[amenbo](https://example.invalid/x)")).toContain('href="https://example.invalid/x"');
  });
});

describe("images", () => {
  // No image is ever emitted: the CSP allows no remote one, so a rendered <img> would only be a broken
  // frame. The alt text is what still says something, and it is drawn as a label.
  it("draws an image as its alt text, and nothing when it has none", () => {
    const html = render("![License: Apache-2.0](https://img.example.invalid/l.svg)");
    expect(html).toContain('class="markdown__imgalt"');
    expect(html).toContain("License: Apache-2.0");
    expect(html).not.toContain("<img");

    expect(render("![](https://img.example.invalid/l.svg)")).not.toContain("markdown__imgalt");
  });

  // A README writes its badges one per line, and remark-breaks would stack them into a column.
  it("keeps a badge row on one line, without touching the breaks in prose", () => {
    const badges = render("[![CI](https://i.invalid/ci.svg)](https://ci.invalid)\n![Release](https://i.invalid/r.svg)");
    expect(badges).not.toContain("<br");
    expect(badges).toContain("CI");
    expect(badges).toContain("Release");

    expect(render("first line\nsecond line")).toContain("<br");
  });
});

describe("where a link may go", () => {
  // The bug this closes: a relative link resolves against Amenbo's own origin, so following it
  // reloaded the whole app and looked like the plugin detail closing itself.
  it("draws a relative link as text, and keeps an in-document one a link", () => {
    const relative = render("[LICENSE](LICENSE) and [docs](./docs/x.md)");
    expect(relative).toContain('class="markdown__deadlink"');
    expect(relative).not.toContain("<a");
    expect(relative).toContain("LICENSE");

    const anchor = render("[Layout](#layout)");
    expect(anchor).toContain('href="#layout"');
  });

  // A badge is a label wrapped in a link, and the same rule applies to it: nothing about being an
  // image makes a relative target reachable.
  it("applies the same rule to a badge's link", () => {
    const badge = render("[![License: Apache-2.0](https://i.invalid/l.svg)](LICENSE)");
    expect(badge).toContain("markdown__imgalt");
    expect(badge).not.toContain("<a");
  });
});

describe("resolveAgainstBase", () => {
  it("resolves a path under the base, and refuses everything that lands outside it", () => {
    expect(resolveAgainstBase("LICENSE", BASE)).toBe(`${BASE}LICENSE`);
    expect(resolveAgainstBase("./docs/x.md", BASE)).toBe(`${BASE}docs/x.md`);

    // Not this repository's files, however they resolve.
    expect(resolveAgainstBase("../../elsewhere", BASE)).toBeNull();
    expect(resolveAgainstBase("/owner/other", BASE)).toBeNull();
    expect(resolveAgainstBase("mailto:someone@example.invalid", BASE)).toBeNull();
    expect(resolveAgainstBase("//example.invalid/x", BASE)).toBeNull();
  });

  it("resolves nothing without a base — a note and a comment come from no repository", () => {
    expect(resolveAgainstBase("LICENSE", undefined)).toBeNull();
  });
});

describe("a body read out of a repository", () => {
  // The point of the base: the same relative link that has nowhere to go in a note names a real file
  // in a README, and that file opens in the browser like any other outside link.
  it("turns a relative link into a browser link, and leaves the same one dead without a base", () => {
    const withBase = render("[LICENSE](LICENSE) and [docs](./docs/x.md)", BASE);
    expect(withBase).toContain(`href="${BASE}LICENSE"`);
    expect(withBase).toContain(`href="${BASE}docs/x.md"`);
    expect(withBase).not.toContain("markdown__deadlink");

    expect(render("[LICENSE](LICENSE)")).toContain("markdown__deadlink");
  });

  it("still draws a target the base cannot vouch for as text", () => {
    expect(render("[out](../../elsewhere)", BASE)).toContain("markdown__deadlink");
  });

  // The heading an anchor names is on this screen (the ids are put there by rehype-slug), so there is
  // nothing to gain by sending the reader out to the repository to read what is already in front of them.
  it("does not send an in-document anchor to the repository", () => {
    expect(render("[Layout](#layout)", BASE)).toContain('href="#layout"');
  });

  // The picture is gone, so where it lives is all the label can still say.
  it("resolves the image label's title, and leaves an absolute one alone", () => {
    expect(render("![logo](docs/logo.png)", BASE)).toContain(`title="${BASE}docs/logo.png"`);
    expect(render("![CI](https://i.invalid/ci.svg)", BASE)).toContain('title="https://i.invalid/ci.svg"');
  });
});
