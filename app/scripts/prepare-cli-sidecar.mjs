// Build the amenbo CLI and stage it as a Tauri sidecar (externalBin) so the GUI
// bundle ships the CLI alongside the app (one installer = GUI + CLI). PATH exposure
// is each OS installer's job; this only puts the binary into the bundle. Runs as
// tauri's `beforeBuildCommand` (cwd = app/), so every `tauri build` — prod
// (`make gui`) and dev (`make gui-dev`) — refreshes it.
//
// Tauri resolves `bundle.externalBin: ["binaries/<stem>"]` (relative to
// tauri.conf.json) to `binaries/<stem>-<target-triple>[.exe]` and picks the one
// matching the build's target triple. We must emit exactly that name.
//
// The stem is this build's own name, which is also its app-data name: the Windows installer puts
// the bundled CLI on PATH, so a stem shared across channels puts production, the shared dev build
// and every theme preview there as `amenbo.exe` at once. AMENBO_APP_NAME is what carries it — the
// same variable amenbo-core compiles `Paths::command_name()` from — so this and the Makefile's
// `GUI_DEV_CONFIG` cannot name different files. Unset is production.
import { execFileSync } from "node:child_process";
import { mkdirSync, copyFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(appDir, "..");
const isWindows = process.platform === "win32";
const exe = isWindows ? ".exe" : "";
const stem = process.env.AMENBO_APP_NAME?.trim() || "amenbo";

function hostTriple() {
  const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  const host = out.split(/\r?\n/).find((l) => l.startsWith("host:"));
  if (!host) throw new Error("could not determine host target triple from `rustc -vV`");
  return host.slice("host:".length).trim();
}

// Tauri exports the resolved triple to build hooks; fall back to the host triple
// (the default target when no --target is passed) so the file name matches.
const host = hostTriple();
const fromTauri = process.env.TAURI_ENV_TARGET_TRIPLE?.trim();
const triple = fromTauri || host;

// A cross build (e.g. x86_64 mac from an arm64 machine) must build the CLI for that
// triple too — otherwise a host binary ships under the cross triple's name.
const isCross = triple !== host;

console.log(`[sidecar] building the ${stem} CLI for ${triple}…`);
execFileSync(
  "cargo",
  [
    "build",
    "--release",
    "-p",
    "amenbo-cli",
    "--manifest-path",
    join(repoRoot, "Cargo.toml"),
    ...(isCross ? ["--target", triple] : []),
  ],
  { stdio: "inherit" },
);

const src = isCross
  ? join(repoRoot, "target", triple, "release", `amenbo${exe}`)
  : join(repoRoot, "target", "release", `amenbo${exe}`);

// The name a sidecar carries is what Tauri trusts, so a mismatched slice would
// ship silently and only fail on the user's machine. Check the artifact itself.
assertArch(src, triple);

const destDir = join(appDir, "src-tauri", "binaries");
const dest = join(destDir, `${stem}-${triple}${exe}`);
mkdirSync(destDir, { recursive: true });
copyFileSync(src, dest);
console.log(`[sidecar] staged ${dest}`);

// macOS is the only platform we cross-build for; elsewhere the artifact is a host
// build by construction and there is nothing a check could catch.
function assertArch(path, triple) {
  if (process.platform !== "darwin") return;
  const want = triple.startsWith("x86_64-") ? "x86_64" : triple.startsWith("aarch64-") ? "arm64" : null;
  if (!want) throw new Error(`[sidecar] unknown mac target triple: ${triple}`);
  const archs = execFileSync("lipo", ["-archs", path], { encoding: "utf8" }).trim().split(/\s+/);
  if (!archs.includes(want)) {
    throw new Error(
      `[sidecar] arch mismatch: ${path} is [${archs.join(", ")}] but ${triple} needs ${want}`,
    );
  }
}
