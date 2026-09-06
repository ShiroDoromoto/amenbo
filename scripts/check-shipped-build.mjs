// Ask a freshly built artifact what it carries, and stop the release if the values this workflow
// injected did not reach it.
//
// Three things are compiled in and none of them has a default: the release stamp (`AMENBO_BUILD`),
// the app-data name (`AMENBO_APP_NAME`) and the update endpoint (`AMENBO_LATEST_JSON_URL`). A build
// cannot check its own injection while it is still building — the material the check would read is
// exactly what goes missing. `AMENBO_BUILD` crossed `docker run` into nothing for twenty versions of
// the Linux distribution and no build ever noticed, because the boundary that drops a variable also
// drops the evidence that it was dropped. So the artifact is asked afterwards, and it answers in
// `version --json`.
//
// Run it in every OS's build job, on the binaries that job produced, before they are uploaded:
//
//   node scripts/check-shipped-build.mjs <binary> [<binary>…]
//
// The pre-release scenario suite (`verification/`) cannot stand here. It runs on a Mac, so it can
// drive the mac artifacts and nothing else; Linux and Windows are the legs where the boundary is.
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { resolve } from "node:path";

// What the workflow said it was injecting. Read from the environment rather than written here, so
// this file cannot come to hold a second copy of the production values that would agree with a
// build no matter what the workflow set.
function injected(name) {
  const value = process.env[name]?.trim();
  if (!value) {
    fail(`${name} is not set — this gate has nothing to hold the artifacts to`);
    process.exit(1);
  }
  return value;
}

function fail(message) {
  console.log(`::error::${message}`);
}

const want = {
  // The stamp is a boolean, and only the release workflow may raise it.
  release_build: true,
  channel: injected("AMENBO_APP_NAME"),
  latest_json_url: injected("AMENBO_LATEST_JSON_URL"),
};

const binaries = process.argv.slice(2);
if (binaries.length === 0) {
  fail("name at least one built binary to ask");
  process.exit(1);
}

// Ask the binary about itself and nothing else: from a directory no checkout is bound in, and with
// the run-time override cleared, so what comes back is what was compiled in rather than what this
// runner happens to say. Each path is resolved against the caller's directory first — the run
// happens elsewhere, and a job names its artifacts relative to the checkout.
const env = { ...process.env };
delete env.AMENBO_HOME;
delete env.AMENBO_UPDATE_JSON_URL;

let bad = 0;
for (const binary of binaries) {
  let said;
  try {
    said = JSON.parse(
      execFileSync(resolve(binary), ["version", "--json"], { cwd: tmpdir(), env, encoding: "utf8" }),
    );
  } catch (e) {
    fail(`${binary}: could not be asked \`version --json\` — ${e.message}`);
    bad += 1;
    continue;
  }
  const wrong = Object.entries(want).filter(([key, value]) => said[key] !== value);
  for (const [key, value] of wrong) {
    fail(`${binary}: ${key} is ${JSON.stringify(said[key])}, not ${JSON.stringify(value)} — this build did not get what the workflow injected`);
  }
  if (wrong.length > 0) {
    bad += 1;
  } else {
    console.log(`✓ ${binary}: ${said.version} · ${said.channel} · stamped · ${said.latest_json_url}`);
  }
}

process.exit(bad === 0 ? 0 : 1);
