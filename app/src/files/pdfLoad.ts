// The IO boundary for the file panel's PDF reader. pdf.js is a heavy dependency — about 500 KB
// gzipped across the library and its worker, and 1.6 MB again in the character maps below — so
// every part of it is pulled in through a dynamic import and none of it reaches the bundle a window
// starts from: opening a PDF is what fetches the reader, and one map is fetched only where a
// document names it. The editor and mermaid take the same shape (`./editorLoad`).
//
// It lives in a module of its own so the panel's tests can stand in for it, rather than loading a
// library that draws into a canvas jsdom does not implement.
//
// **Why pdf.js and not the webview's own viewer** (`AMB-D-907`): handing a PDF to the built-in
// viewer needs the sandbox taken off the frame it is shown in, which would leave PDFs — and PDFs
// alone — a defence short. Every comparable application ships pdf.js for the same reason, and on
// Linux the built-in viewer *is* pdf.js. The version is the last one that carries no wasm, which
// `AMB-D-769` does not allow.
//
// **The bytes are fetched, not carried.** The address is the door that hands out a file by its path
// (`../core/fileUrl`), the same one the picture beside it is drawn from, so nothing of the file
// crosses the command seam.

import type { RenderTask } from "pdfjs-dist";

/**
 * The character maps a PDF written in Japanese, Chinese or Korean is read through.
 *
 * **Without them such a file draws its Latin letters and nothing else** — measured on a page of
 * Japanese, which came back as the two words in it that were not Japanese. A CID-keyed font names
 * the collection its glyphs are numbered in (`Adobe-Japan1` and the like) rather than carrying the
 * numbering, and these are that numbering, published by Adobe and shipped inside pdf.js.
 *
 * **Each is a module of its own, and none of them is loaded until a document asks for it by name**
 * — the shape the grammars beside them take (`./grammars`). They ride inside those modules as data
 * rather than sitting beside the bundle as files to go and fetch, so a map arrives the way the code
 * around it does and no door has to answer for it.
 */
const CMAPS = import.meta.glob<string>(
  "/node_modules/pdfjs-dist/cmaps/*.bcmap",
  { query: "?inline", import: "default" },
);

/** The bytes behind a `data:` URL that carries them in base64. */
function carried(url: string): Uint8Array {
  const bytes = atob(url.slice(url.indexOf(",") + 1));
  const out = new Uint8Array(bytes.length);
  for (let at = 0; at < bytes.length; at += 1) out[at] = bytes.charCodeAt(at);
  return out;
}

/**
 * What pdf.js asks for a character map with. It is handed the class rather than an address, so what
 * answers is `CMAPS` and nothing is fetched (`AMB-D-907`).
 *
 * A name nobody shipped is refused the way pdf.js refuses one: the page still draws, without the
 * glyphs that map could have named.
 */
class Maps {
  async fetch({ name }: { name: string }): Promise<{ cMapData: Uint8Array; isCompressed: boolean }> {
    const load = CMAPS[`/node_modules/pdfjs-dist/cmaps/${name}.bcmap`];
    if (load === undefined) throw new Error(`no character map called ${name} is shipped`);
    return { cMapData: carried(await load()), isCompressed: true };
  }
}

/**
 * How far outside the visible part of the panel a page is already drawn.
 *
 * A page is drawn when it comes near rather than when it arrives, so scrolling meets a page that is
 * already there instead of a blank one filling in behind the scroll. One screen's worth is what
 * covers an ordinary flick; a page further out than that is one nobody is looking at yet.
 */
const AHEAD = "100% 0px";

/**
 * How wide a page is drawn when the panel cannot say how wide it is.
 *
 * Only reachable where the panel has no width at the moment the document opens — a column that has
 * not been laid out yet. The number is a page of A4 at a hundred and fifty dots to the inch, so
 * what it produces is a readable page rather than a placeholder.
 */
const FALLBACK_WIDTH = 1240;

/** One mounted document: what it drew into is the caller's, and this takes it all down again. */
export type MountedPdf = {
  /** Take the pages off the page, stop what is being drawn, and let the document go. */
  close(): void;
};

/**
 * Draw the PDF at `url` into `parent`, a page at a time as the reader comes to them.
 *
 * `label` names a page for a reader who is not looking at it — the canvas is an image as far as
 * anything reading the page out is concerned, so the number it holds has to be written down. It is
 * passed in rather than read here because the wording belongs to the face (`./PdfView`).
 *
 * Throws where the document cannot be opened at all — a file that is not the PDF its first bytes
 * claimed, or one the door would not hand over. The panel says so and offers the way on, which is
 * the same answer it gives for a file it could not read.
 */
export async function mountPdf(
  parent: HTMLElement,
  url: string,
  label: (page: number, of: number) => string,
): Promise<MountedPdf> {
  const pdfjs = await import("pdfjs-dist");
  // The worker is addressed as a file of ours: Vite emits it beside the bundle, so it is fetched
  // from the window's own origin and `script-src 'self'` needs no widening for it.
  const built = await import("pdfjs-dist/build/pdf.worker.min.mjs?url");

  // **The worker is started here rather than by pdf.js**, which is what keeps that address the one
  // used. Handed `workerSrc`, pdf.js first asks whether it is same-origin — and it asks by comparing
  // `URL.origin`, which every scheme but http and https answers `"null"` to. The window's own page is
  // served over a scheme of ours, so its own worker reads as a stranger's, and pdf.js wraps it in a
  // `blob:` that imports it. A blob is not `'self'`: the CSP refuses the worker, and a PDF that was
  // never going to be a problem does not open. Handed a port instead, pdf.js uses what it is given.
  const port = new Worker(built.default, { type: "module" });
  const worker = pdfjs.PDFWorker.fromPort({ port });

  // **The file is fetched here rather than by pdf.js**, for the reason the worker is started here:
  // pdf.js fetches a document itself only over http and https, and everything else falls to a path
  // of its own that our door answers with nothing at all — measured, a status of 0 and no bytes.
  // `fetch` is what reaches these doors (`../components/Attachments`), and what the host already
  // refused to answer for is anything over the cap, so what arrives here is bounded (`AMB-D-907`).
  //
  // `isEvalSupported` off is CVE-2024-4367's fix standing on its own: pdf.js compiles a font's
  // charstrings with `eval` where it can, and the window's CSP already refuses that — turning it
  // off here is what keeps a refused eval from being the first thing anyone hears about it.
  let doc;
  try {
    const answer = await fetch(url);
    // A door of ours answers with a status; one that answered with none still answered.
    if (!answer.ok && answer.status !== 0) throw new Error(`the file door said ${answer.status}`);
    const data = new Uint8Array(await answer.arrayBuffer());
    doc = await pdfjs.getDocument({
      data,
      worker,
      isEvalSupported: false,
      CMapReaderFactory: Maps,
    }).promise;
  } catch (e) {
    // The worker is this document's, so a document that never opened takes it back down with it.
    port.terminate();
    throw e;
  }

  const of = doc.numPages;
  const first = await doc.getPage(1);
  const shape = first.getViewport({ scale: 1 });
  // What is drawn is fixed to the width the panel had when the document opened, and CSS carries it
  // from there: a canvas redrawn on every drag of the splitter would cost a full render per frame.
  const width = parent.clientWidth > 0 ? parent.clientWidth : FALLBACK_WIDTH;
  // The backing store is in device pixels and the CSS size in the page's, so a page is drawn at the
  // resolution the screen actually has rather than at a quarter of it.
  const scale = (width / shape.width) * (window.devicePixelRatio || 1);

  const drawing = new Set<RenderTask>();
  const frames: HTMLElement[] = [];
  let gone = false;

  const draw = (frame: HTMLElement, at: number) => {
    void (async () => {
      const page = await doc.getPage(at);
      if (gone) return;
      const view = page.getViewport({ scale });
      const canvas = frame.ownerDocument.createElement("canvas");
      canvas.width = Math.floor(view.width);
      canvas.height = Math.floor(view.height);
      canvas.className = "files__pdfpaper";
      canvas.setAttribute("role", "img");
      canvas.setAttribute("aria-label", label(at, of));
      const into = canvas.getContext("2d");
      if (into === null) return;
      const task = page.render({ canvasContext: into, viewport: view });
      drawing.add(task);
      try {
        await task.promise;
      } catch {
        // A page that was asked for and then scrolled past is cancelled on the way out, and a page
        // that would not draw is one page of a document that otherwise reads. Neither is the
        // document failing to open, which is the one thing the caller is told about.
        return;
      } finally {
        drawing.delete(task);
      }
      if (gone) return;
      frame.replaceChildren(canvas);
      // The room kept for the page was page one's shape, and this page is now its own: the guess is
      // dropped so a document whose pages are not all one size sits right rather than to the first
      // page's ratio.
      frame.style.aspectRatio = "";
    })();
  };

  // Each page is watched where it sits, and drawn the first time it comes near. `IntersectionObserver`
  // is what a webview has and jsdom has not, which is the other reason this module is stood in for.
  const watcher = new IntersectionObserver((seen) => {
    for (const one of seen) {
      if (!one.isIntersecting) continue;
      const frame = one.target as HTMLElement;
      watcher.unobserve(frame);
      draw(frame, Number(frame.dataset.page));
    }
  }, { root: parent.closest(".files__body"), rootMargin: AHEAD });

  for (let at = 1; at <= of; at += 1) {
    const frame = parent.ownerDocument.createElement("div");
    frame.className = "files__pdfpage";
    frame.dataset.page = String(at);
    // The room a page will take, before anything of it has been drawn: without it every page is a
    // box of no height, they all meet the reader's screen at once, and the whole document draws.
    frame.style.aspectRatio = `${shape.width} / ${shape.height}`;
    frames.push(frame);
    watcher.observe(frame);
  }
  parent.replaceChildren(...frames);

  return {
    close() {
      gone = true;
      watcher.disconnect();
      for (const task of drawing) task.cancel();
      drawing.clear();
      parent.replaceChildren();
      void doc.destroy();
      // Started here, so stopped here: `destroy` lets go of a port it was handed and leaves the
      // worker behind it running.
      port.terminate();
    },
  };
}
