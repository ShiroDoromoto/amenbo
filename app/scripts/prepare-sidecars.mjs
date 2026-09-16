// Build the two binaries the GUI bundle carries beside itself and stage them as Tauri sidecars
// (externalBin), so one installer lands the GUI, the CLI and the askpass helper together. PATH
// exposure is each OS installer's job; this only puts the binaries into the bundle. Runs as
// tauri's `beforeBuildCommand` (cwd = app/), so every `tauri build` — prod
// (`make gui`) and dev (`make gui-dev`) — refreshes them.
//
// The two are:
//   * the amenbo CLI, which is the face an agent and a terminal reach;
//   * amenbo-askpass, which is what git and ssh are pointed at when they need to ask the person
//     something and have no terminal to ask it in (crates/amenbo-askpass).
//
// Tauri resolves `bundle.externalBin: ["binaries/<stem>"]` (relative to
// tauri.conf.json) to `binaries/<stem>-<target-triple>[.exe]` and picks the one
// matching the build's target triple. We must emit exactly that name.
//
// The stem is this build's own name, which is also its app-data name: the Windows installer puts
// the whole install directory on PATH, so a stem shared across channels puts production, the shared
// dev build and every theme preview there as `amenbo.exe` at once. AMENBO_APP_NAME is what carries
// it — the same variable amenbo-core compiles `Paths::command_name()` from — so this and the
// Makefile's `GUI_DEV_CONFIG` cannot name different files. Unset is production. The helper hangs
// `-askpass` off that same stem (`Paths::askpass_file_name()`), and splits with it for the same
// reason: PATH does not distinguish between the files it was given and the files it was given by
// accident.
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

// What cargo builds, and the stem each one is bundled under. The built name is the crate's own
// binary name and never splits by channel; the bundled name always does.
const sidecars = [
  { crate: "amenbo-cli", built: "amenbo", stem },
  { crate: "amenbo-askpass", built: "amenbo-askpass", stem: `${stem}-askpass` },
];

console.log(`[sidecar] building ${sidecars.map((s) => s.crate).join(" + ")} for ${triple}…`);
execFileSync(
  "cargo",
  [
    "build",
    "--release",
    ...sidecars.flatMap((s) => ["-p", s.crate]),
    "--manifest-path",
    join(repoRoot, "Cargo.toml"),
    ...(isCross ? ["--target", triple] : []),
  ],
  { stdio: "inherit" },
);

const outDir = isCross
  ? join(repoRoot, "target", triple, "release")
  : join(repoRoot, "target", "release");
const destDir = join(appDir, "src-tauri", "binaries");
mkdirSync(destDir, { recursive: true });

for (const sidecar of sidecars) {
  const src = join(outDir, `${sidecar.built}${exe}`);
  // The name a sidecar carries is what Tauri trusts, so a mismatched slice would
  // ship silently and only fail on the user's machine. Check the artifact itself.
  assertArch(src, triple);
  const dest = join(destDir, `${sidecar.stem}-${triple}${exe}`);
  copyFileSync(src, dest);
  console.log(`[sidecar] staged ${dest}`);
}

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
