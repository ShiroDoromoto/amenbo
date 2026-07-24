import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { Element as HastElement } from "hast";
import { findRefTokens, isExternalHref, mermaidSourceFromPre, Markdown } from "./Markdown";

// SSR needs no DOM, and it pins the whole remark wiring: detection → link node → urlTransform lets it through → an a.reflink is rendered.
const render = (md: string) => renderToStaticMarkup(createElement(Markdown, { children: md }));

describe("findRefTokens", () => {
  const raws = (s: string) => findRefTokens(s).map((r) => r.raw);

  it("picks up: AMB-T-NNN / AMB-D-NNN (with non-alphanumeric boundaries on both sides)", () => {
    expect(raws("see AMB-T-123 and AMB-D-85 plus AMB-T-45.")).toEqual(["AMB-T-123", "AMB-D-85", "AMB-T-45"]);
    expect(raws("(AMB-T-7)")).toEqual(["AMB-T-7"]);
    expect(raws("AMB-D-1, AMB-D-22")).toEqual(["AMB-D-1", "AMB-D-22"]);
    expect(raws("amb-t-7")).toEqual(["amb-t-7"]); // the kind code folds case
  });

  // The namespace is the whole point: a body's `#12` is a GitHub issue and its `T-12` may be another
  // tracker's, so linking either would hijack a reference that was never amenbo's.
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
  // The app window has no address bar and no way back, so a link that leaves amenbo must go to the
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
