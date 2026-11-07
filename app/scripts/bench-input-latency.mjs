// How long a key pressed in a pane's box takes to reach the screen, in WebKit — the engine the
// macOS app draws with — on the terminal face and on a run's roles.
//
//   npm run bench:input                       # the app as it is in this checkout
//   npm run bench:input -- --rev <commit>     # app/src as it was at that commit, for a before/after
//
// Options: --panes 1,4 (how many panes), --presses 60 (keys per case), --headed (a window instead of
// headless), --json <file> (every sample, for the record). WebKit is installed apart from the
// package: `npx playwright install webkit`.
//
// **What is measured.** The page (`./bench-input-latency/`) is the real panes on the real
// stylesheets, built for production; only what reaches the host is put in their place, and every
// pane's terminal is written to every 16ms. A key is pressed in the first pane's box every 60ms, and
// three things are read per case:
//   - key → screen: from keydown to two frames after it, by which time the frame that drew the
//     key is on screen;
//   - input → task: from the input event to the next task — what React does synchronously for it;
//   - frame gaps over 25ms while typing, out of all the frames drawn.
//
// **A commit's source is its app/src only.** The bench page, its stand-ins and node_modules are this
// checkout's, so a commit whose packages or pane props differ from these is not one it can measure.
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";
import react from "@vitejs/plugin-react";
import { webkit } from "playwright";
import { build } from "vite";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const benchDir = join(appDir, "scripts", "bench-input-latency");

const { values: opts } = parseArgs({
  options: {
    rev: { type: "string" },
    panes: { type: "string", default: "1,4" },
    presses: { type: "string", default: "60" },
    headed: { type: "boolean", default: false },
    json: { type: "string" },
  },
});
const paneCounts = opts.panes.split(",").map(Number);
const presses = Number(opts.presses);
const MODES = ["terminal", "roles"];

// The real path, since what the bundler hands back as an importer is one (macOS keeps tmp under a link).
const work = realpathSync(mkdtempSync(join(tmpdir(), "bench-input-latency-")));
try {
  const srcDir = opts.rev === undefined ? join(appDir, "src") : extractSrc(opts.rev);
  const outDir = join(work, "out");
  await buildPage(srcDir, outDir);
  const server = await serve(outDir);
  const base = `http://127.0.0.1:${server.address().port}/index.html`;
  const browser = await webkit.launch({ headless: !opts.headed });
  const results = [];
  console.log(`# ${opts.rev ?? "working tree"} · WebKit ${browser.version()} · ${opts.headed ? "headed" : "headless"}`);
  try {
    for (const mode of MODES) {
      for (const panes of paneCounts) {
        const result = await measure(browser, `${base}?panes=${panes}&mode=${mode}`);
        results.push({ mode, panes, ...result });
        console.log(line(mode, panes, result));
      }
    }
  } finally {
    await browser.close();
    server.close();
  }
  if (opts.json !== undefined) {
    writeFileSync(opts.json, JSON.stringify({ rev: opts.rev ?? null, presses, results }, null, 2));
  }
} finally {
  rmSync(work, { recursive: true, force: true });
}

/** `app/src` as it was at `rev`, written out under the work folder. */
function extractSrc(rev) {
  const tar = execFileSync("git", ["archive", rev, "src"], { cwd: appDir, maxBuffer: 1 << 30 });
  execFileSync("tar", ["-x", "-C", work], { input: tar });
  return join(work, "src");
}

/**
 * The bench page, built for production against `srcDir`. `@app` names that source; the modules that
 * reach the host are swapped for the bench's stand-ins wherever the source imports them; and a
 * package the source imports is looked up from this checkout, which is where node_modules is.
 */
async function buildPage(srcDir, outDir) {
  const standIns = new Map(
    [
      ["talk/agent.ts", "agent.ts"],
      ["talk/plate.ts", "plate.ts"],
      ["core/hostDrop.ts", "hostDrop.ts"],
    ].map(([real, standIn]) => [join(srcDir, real), join(benchDir, standIn)]),
  );
  const fromHere = join(appDir, "src", "main.tsx");
  const swap = {
    name: "bench-stand-ins",
    enforce: "pre",
    async resolveId(source, importer, options) {
      if (importer === undefined || [...standIns.values()].includes(importer)) return null;
      if (!importer.startsWith(srcDir) && !importer.startsWith(benchDir)) return null;
      const bare = !/^(\.|\/|@app\/|\0)/.test(source) && !source.startsWith("/@");
      const from = bare && importer.startsWith(srcDir) ? fromHere : importer;
      const found = await this.resolve(source, from, { ...options, skipSelf: true });
      if (found === null) return null;
      return standIns.get(found.id.split("?")[0]) ?? found;
    },
  };
  await build({
    configFile: false,
    root: benchDir,
    base: "./",
    mode: "production",
    logLevel: "warn",
    plugins: [swap, react()],
    resolve: { alias: { "@app": srcDir } },
    assetsInclude: ["**/*.bcmap", "**/*.pfb"],
    build: { outDir, emptyOutDir: true, reportCompressedSize: false, chunkSizeWarningLimit: 1 << 20 },
  });
}

/** A static server over the built page, on a port of the system's choosing. */
function serve(dir) {
  const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".woff2": "font/woff2" };
  const server = createServer((req, res) => {
    const path = join(dir, decodeURIComponent(new URL(req.url, "http://x").pathname));
    try {
      const body = readFileSync(path);
      res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
      res.end(body);
    } catch {
      res.writeHead(404).end();
    }
  });
  return new Promise((done) => server.listen(0, "127.0.0.1", () => done(server)));
}

/** One case: open the page, wait for every pane to be drawn, and type into the first box. */
async function measure(browser, url) {
  const page = await browser.newPage({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 2 });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  try {
    await page.goto(url);
    await page.waitForFunction(() => window.__bench.ready, null, { timeout: 30_000 }).catch((e) => {
      throw new Error(`the panes were not drawn: ${errors.join(" / ") || e.message}`);
    });
    await page.waitForTimeout(800);
    await page.focus(".compose__box");
    await page.evaluate(() => window.__bench.start());
    for (let i = 0; i < presses; i++) {
      await page.keyboard.press("a");
      await page.waitForTimeout(60);
    }
    await page.waitForTimeout(200);
    const got = await page.evaluate(() => {
      window.__bench.stop();
      const { lat, js, gaps } = window.__bench;
      return { lat, js, gaps, unknown: window.__bench.unknown() };
    });
    if (got.lat.length === 0) throw new Error(`no key was timed: ${errors.join(" / ")}`);
    return { ...got, errors };
  } finally {
    await page.close();
  }
}

function pct(samples, p) {
  const sorted = [...samples].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * p))];
}

function line(mode, panes, { lat, js, gaps, unknown, errors }) {
  const ms = (v) => `${v.toFixed(1)}ms`;
  const parts = [
    `panes=${panes}`,
    mode,
    `key→screen median ${ms(pct(lat, 0.5))} p90 ${ms(pct(lat, 0.9))}`,
    `input→task median ${ms(pct(js, 0.5))} p90 ${ms(pct(js, 0.9))}`,
    `frames over 25ms ${gaps.filter((g) => g > 25).length}/${gaps.length}`,
  ];
  if (unknown.length > 0) parts.push(`unanswered commands ${unknown.join(",")}`);
  if (errors.length > 0) parts.push(`page errors ${errors.length}`);
  return parts.join("\t");
}
