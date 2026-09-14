import { cloudflareTest, readD1Migrations } from "@cloudflare/vitest-pool-workers";
import { defineConfig } from "vitest/config";

// The migrations are read here, in Node, and handed to the tests as a binding — so what the
// tests run against is the schema a deploy applies, rather than a second copy of it written to
// agree with the first.
const migrations = await readD1Migrations("./migrations");

// The tests run inside workerd, against the same wrangler.jsonc a deploy reads — so a binding
// that is wrong in the config is wrong in the tests too, rather than passing against a mock and
// failing in the account it was deployed to.
export default defineConfig({
	plugins: [
		cloudflareTest({
			wrangler: { configPath: "./wrangler.jsonc" },
			miniflare: {
				// The real one is a Secret set on the deployed Worker. Here it is whatever the
				// tests agree on — what is under test is the gate, not the value. The read
				// tokens are not bindings at all: they are rows, so the tests issue them.
				bindings: { WRITE_TOKEN: "a-long-random-write-token", TEST_MIGRATIONS: migrations },
			},
		}),
	],
	test: {
		setupFiles: ["./test/schema.ts"],
	},
});
