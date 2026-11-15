import type { D1Migration } from "@cloudflare/vitest-pool-workers";
import { SELF, env } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";

const CLEARING = "0007_the_runs_the_viewer_never_needed";

/** The clean-up migration, as a deploy hands it to the database. */
function clearing(): D1Migration {
	const migrations = (env as unknown as { TEST_MIGRATIONS: D1Migration[] }).TEST_MIGRATIONS;
	const found = migrations.find((migration) => migration.name.startsWith(CLEARING));
	if (!found) throw new Error(`no migration named ${CLEARING}`);
	return found;
}

/** Runs the clean-up again, against whatever rows the test has put down. */
async function clear() {
	for (const query of clearing().queries) await env.RECORDS.prepare(query).run();
}

/** Writes one record, the way a send does. */
function place(k: string, seq: number, op: "put" | "del") {
	const sealed = op === "put";
	return env.RECORDS.prepare("INSERT INTO records (k, seq, op, nonce, ciphertext) VALUES (?, ?, ?, ?, ?)")
		.bind(k, seq, op, sealed ? `nonce-${seq}` : null, sealed ? `ciphertext-${seq}` : null)
		.run();
}

async function keys(): Promise<string[]> {
	const { results } = await env.RECORDS.prepare("SELECT k FROM records ORDER BY seq").all();
	return results.map((row) => row.k as string);
}

/** Sends one record the way the PC does, through the Worker rather than past it. */
function send(k: string) {
	return SELF.fetch("https://viewer.example.workers.dev/records", {
		method: "PUT",
		headers: { Authorization: "Bearer a-long-random-write-token" },
		body: JSON.stringify({ spec_v: 1, version: 1, records: [{ k, op: "put", n: `nonce-for-${k}`, c: `ciphertext-for-${k}` }] }),
	});
}

async function seqOf(k: string): Promise<number | undefined> {
	const row = await env.RECORDS.prepare("SELECT seq FROM records WHERE k = ?").bind(k).first();
	return row?.seq as number | undefined;
}

async function standing(): Promise<number> {
	const row = await env.RECORDS.prepare("SELECT seq FROM store WHERE id = 1").first();
	return row?.seq as number;
}

beforeEach(async () => {
	await env.RECORDS.exec("DELETE FROM records");
	await env.RECORDS.exec(
		"UPDATE store SET version = NULL, updated_at = NULL, seq = 0, replacing = 0, placed_from = 0, key_fingerprint = NULL WHERE id = 1",
	);
});

describe("the clean-up of the runs the PC used to send", () => {
	// Writes and deletions alike: a deletion left behind is still a row every phone reads.
	it("takes the rows of every run table, writes and deletions both", async () => {
		await place("automation_run/1", 1, "put");
		await place("automation_run_def/2", 2, "put");
		await place("automation_run_pass/3", 3, "del");
		await place("automation_run_step/4", 4, "put");
		await place("automation_run_task/5", 5, "del");
		await place("automation_run_value/6", 6, "put");

		await clear();

		expect(await keys()).toEqual([]);
	});

	// `_` is a wildcard in LIKE, so a prefix match would take these too. They are not the runs.
	it("leaves the rows whose names only look like them", async () => {
		await place("task/1", 1, "put");
		await place("automation/7", 2, "put");
		await place("automation_runXstep/4", 3, "put");
		await place("automation_run_steps/5", 4, "del");
		await place("automation_run", 5, "put");
		await place("automation_run/8", 6, "put");

		await clear();

		expect(await keys()).toEqual(["task/1", "automation/7", "automation_runXstep/4", "automation_run_steps/5", "automation_run"]);
	});

	// A phone's cursor only means anything if the order never rewinds, so taking the newest rows
	// must not take the number back with them.
	it("leaves where the order stands, so the next write carries on past it", async () => {
		await place("task/1", 1, "put");
		await place("automation_run/2", 2, "put");
		await place("automation_run_step/3", 3, "put");
		await env.RECORDS.exec("UPDATE store SET seq = 3 WHERE id = 1");

		await clear();

		expect(await standing()).toBe(3);
		expect(await keys()).toEqual(["task/1"]);

		expect((await send("task/2")).status).toBe(200);
		expect(await seqOf("task/2")).toBe(4);
	});
});
