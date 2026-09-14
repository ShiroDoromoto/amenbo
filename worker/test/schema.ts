import type { D1Migration } from "@cloudflare/vitest-pool-workers";
import { applyD1Migrations, env } from "cloudflare:test";

/**
 * What the test config hands in on top of the bindings a deploy has. It is declared here rather
 * than added to the Worker's own Env, so nothing in src can reach for a binding that exists only
 * while the tests are running.
 */
interface FromTheConfig {
	TEST_MIGRATIONS: D1Migration[];
}

// Every test file starts against a database the migrations have been applied to — the same ones
// a deploy runs, so a migration that will not apply fails here rather than in someone's account.
await applyD1Migrations(env.RECORDS, (env as unknown as FromTheConfig).TEST_MIGRATIONS);
