// Re-bake a registered image down to the pixels the box actually draws it at.
//
// A registered image is stored at 96px square (`./mutations`, `fileToAvatarDataUrl`), and the places
// it is shown are small: a project's mark is 24px on both faces, a facet's avatar is 18px. On a
// Retina screen 24 CSS px is 48 device pixels, so the browser halves 96 once. On a 1x screen — an
// external monitor, which is where this was seen — the same box is 24 device pixels, and the browser
// takes 96 down to a quarter in a single step. One step filters over too few neighbouring pixels to
// stand a quarter, so thin strokes drop out and the mark reads as grainy (`AMB-T-5172`).
//
// So the reduction is done here instead, halving at each step, and the `<img>` is handed a version
// already at the device-pixel size. Halving is the one ratio every browser's filter covers well, and
// the last step lands exactly on the target so the browser itself draws 1:1.
//
// Nothing is stored: this is a display-time bake off the display version that is already held
// (`AMB-D-839`), so an image registered before this reads the same as one registered after it.
import { useEffect, useState } from "react";

/**
 * The largest reduction a single browser step is trusted with.
 *
 * At a half the browser's own filter is as good as anything done here, and below that the stored
 * image is handed over untouched — a bake that changed nothing would only cost a canvas and a second
 * copy of the bytes.
 */
const ONE_STEP_MAX = 2;

/** How many bakes are held. The marks on a screen are few; a store with many registered images would
 * otherwise keep every bake it ever made for as long as the window is open. */
const CACHE_MAX = 64;

/**
 * The sides a reduction steps through, from a source `side` px square down to `px` square.
 *
 * Empty means hand the source over as it is: it is already at or below the target, or one browser
 * step covers the whole reduction. Otherwise each entry halves the one before it, except the last,
 * which lands exactly on the target however far short of a half it falls.
 */
export function shrinkSteps(side: number, px: number): number[] {
  if (!(side > 0) || !(px > 0) || side <= px * ONE_STEP_MAX) return [];
  const steps: number[] = [];
  let step = side;
  while (step > px) {
    step = Math.max(px, Math.round(step / 2));
    steps.push(step);
  }
  return steps;
}

/** The image, once the browser has it. Data URLs are all this is ever handed, so nothing crosses the network. */
function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const el = new Image();
    el.onload = () => resolve(el);
    el.onerror = () => reject(new Error("image could not be read"));
    el.src = src;
  });
}

/**
 * Bakes `src` down to `px` square, halving at each step, and returns it as a PNG data URL.
 *
 * It returns `src` itself whenever the bake would buy nothing — the source is already small enough,
 * or there is no canvas to draw on. The caller therefore never has to tell a shrunk image from an
 * untouched one.
 */
export async function shrinkToPixels(src: string, px: number): Promise<string> {
  const img = await loadImage(src);
  // The centre square, the same crop the registration bake takes, so a source that is not square
  // shrinks to what `object-fit: cover` would have shown of it anyway.
  const side = Math.min(img.naturalWidth, img.naturalHeight);
  const steps = shrinkSteps(side, px);
  if (steps.length === 0) return src;

  let from: CanvasImageSource = img;
  let sx = (img.naturalWidth - side) / 2;
  let sy = (img.naturalHeight - side) / 2;
  let sSide = side;
  let out: HTMLCanvasElement | null = null;
  for (const step of steps) {
    const canvas = document.createElement("canvas");
    canvas.width = step;
    canvas.height = step;
    const ctx = canvas.getContext("2d");
    if (!ctx) return src;
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(from, sx, sy, sSide, sSide, 0, 0, step, step);
    from = canvas;
    sx = 0;
    sy = 0;
    sSide = step;
    out = canvas;
  }
  return out === null ? src : out.toDataURL("image/png");
}

const baked = new Map<string, Promise<string>>();

const cacheKey = (src: string, px: number) => `${px}|${src}`;

/**
 * The bake for one source at one size, made once and then held.
 *
 * Both faces draw the same project's mark at the same size, so without this the same image would be
 * baked twice on a switch between them. A bake that throws resolves to the source instead: a mark
 * drawn from the stored image is the state before this existed, which is worse-looking and not broken.
 */
function shrunkOnce(src: string, px: number): Promise<string> {
  const key = cacheKey(src, px);
  const held = baked.get(key);
  if (held) return held;
  const making = shrinkToPixels(src, px).catch(() => src);
  baked.set(key, making);
  while (baked.size > CACHE_MAX) {
    const oldest = baked.keys().next();
    if (oldest.done) break;
    baked.delete(oldest.value);
  }
  return making;
}

const pixelRatio = () =>
  typeof window === "undefined" || !(window.devicePixelRatio > 0) ? 1 : window.devicePixelRatio;

/**
 * The window's device-pixel ratio, kept up to date.
 *
 * It changes without the window changing size — dragging the window from a Retina screen onto a 1x
 * monitor is exactly the case this is for — and there is no event for it. The way to hear about it is
 * a media query pinned to the ratio in hand, which stops matching the moment the ratio moves.
 */
function usePixelRatio(): number {
  const [dpr, setDpr] = useState(pixelRatio);
  useEffect(() => {
    const mq = window.matchMedia?.(`(resolution: ${dpr}dppx)`);
    if (!mq?.addEventListener) return;
    const moved = () => setDpr(pixelRatio());
    mq.addEventListener("change", moved);
    return () => mq.removeEventListener("change", moved);
  }, [dpr]);
  return dpr;
}

/**
 * The source to put on an `<img>` that is drawn `cssPx` wide: the stored image at first, and the
 * version baked for this screen once it is ready.
 *
 * `cssPx` is the box's own width. A ring or padding inside the box only makes the bake denser than
 * the image needs to be, which is the harmless direction — what has to be avoided is the other one.
 */
export function useShrunkImage(src: string | null, cssPx: number): string | null {
  const dpr = usePixelRatio();
  const px = Math.round(cssPx * dpr);
  const key = cacheKey(src ?? "", px);
  const [ready, setReady] = useState<{ key: string; url: string } | null>(null);

  useEffect(() => {
    if (src === null) return;
    let live = true;
    void shrunkOnce(src, px).then((url) => {
      if (live) setReady({ key, url });
    });
    return () => { live = false; };
  }, [src, px, key]);

  if (src === null) return null;
  // Anything baked for another source or another screen is last screen's answer, so the stored image
  // stands in until this one's bake lands rather than the mark flicking through a stale size.
  return ready?.key === key ? ready.url : src;
}
