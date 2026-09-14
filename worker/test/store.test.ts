import { env } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";

// Each test starts from an empty store. The schema is applied once for the file; what is under
// test is what it allows and what it refuses, so the rows must not carry over.
beforeEach(async () => {
	await env.RECORDS.exec("DELETE FROM records");
	await env.RECORDS.exec("DELETE FROM tokens");
});

/** Writes one record, the way a send does. */
function place(k: string, seq: number, op: "put" | "del", nonce?: string, ciphertext?: string) {
	return env.RECORDS.prepare("INSERT INTO records (k, seq, op, nonce, ciphertext) VALUES (?, ?, ?, ?, ?)")
		.bind(k, seq, op, nonce ?? null, ciphertext ?? null)
		.run();
}

describe("the records", () => {
	// The whole reason for a database rather than one overwritten key: a phone asks for what came
	// after the point it got to, and gets only that.
	it("are read as everything after a point in the order", async () => {
		await place("task/1", 1, "put", "nonce-1", "ciphertext-1");
		await place("task/2", 2, "put", "nonce-2", "ciphertext-2");
		await place("task/3", 3, "put", "nonce-3", "ciphertext-3");

		const { results } = await env.RECORDS.prepare("SELECT k FROM records WHERE seq > ? ORDER BY seq").bind(1).all();

		expect(results.map((row) => row.k)).toEqual(["task/2", "task/3"]);
	});

	// A deletion keeps its place in the order rather than leaving a hole, so a phone that was away
	// learns the record went instead of never hearing of it again.
	it("carry a deletion as a row of its own", async () => {
		await place("task/1", 1, "put", "nonce-1", "ciphertext-1");
		await place("task/2", 2, "del");

		const { results } = await env.RECORDS.prepare("SELECT k, op, ciphertext FROM records WHERE seq > 0 ORDER BY seq").all();

		expect(results).toEqual([
			{ k: "task/1", op: "put", ciphertext: "ciphertext-1" },
			{ k: "task/2", op: "del", ciphertext: null },
		]);
	});

	// One key is one record. A second write to it replaces what is there and takes a new place in
	// the order — two rows for one key would show a phone the record twice, once stale.
	it("hold one row per key", async () => {
		await place("task/1", 1, "put", "nonce-1", "ciphertext-1");

		await expect(place("task/1", 2, "put", "nonce-2", "ciphertext-2")).rejects.toThrow();
	});

	// Two rows sharing a place in the order would make a page boundary ambiguous: a phone
	// resuming from there would skip one of them or read it twice.
	it("refuse two rows in one place in the order", async () => {
		await place("task/1", 1, "put", "nonce-1", "ciphertext-1");

		await expect(place("task/2", 1, "put", "nonce-2", "ciphertext-2")).rejects.toThrow();
	});

	// Anything but a write or a deletion is something the phone has no reading for, so it is
	// refused where it is written rather than found later by whatever has to interpret it.
	it("take no operation but a write and a deletion", async () => {
		await expect(
			env.RECORDS.prepare("INSERT INTO records (k, seq, op) VALUES (?, ?, ?)").bind("task/1", 1, "archive").run(),
		).rejects.toThrow();
	});
});

describe("the tokens", () => {
	// The Worker only ever compares, so the value has no reason to be here. Keeping it would put
	// the read credential's only copy in one place, to be taken along with everything else.
	it("keep a hash and a date, and nothing else", async () => {
		await env.RECORDS.prepare("INSERT INTO tokens (id, hash, issued_at) VALUES (1, ?, ?)")
			.bind("a-sha-256-of-the-code", "2026-08-09T12:00:00Z")
			.run();

		const { results } = await env.RECORDS.prepare("SELECT * FROM tokens").all();

		expect(Object.keys(results[0]).sort()).toEqual(["hash", "id", "issued_at"]);
	});

	// One row, ever. A second code surviving an issue would be a phone reading that nothing on the
	// PC's screen says is there — and the screen has no way to find out, since the store answers
	// present or absent and not a list.
	it("is one row that cannot become two", async () => {
		await env.RECORDS.prepare("INSERT INTO tokens (id, hash, issued_at) VALUES (1, ?, ?)")
			.bind("a-sha-256-of-the-code", "2026-08-09T12:00:00Z")
			.run();

		await expect(
			env.RECORDS.prepare("INSERT INTO tokens (id, hash, issued_at) VALUES (2, ?, ?)")
				.bind("a-sha-256-of-another-code", "2026-08-09T12:00:01Z")
				.run(),
		).rejects.toThrow();
	});
});

describe("where the store stands", () => {
	// One row, ever. A second would leave two answers to "what version is this" with nothing to
	// say which one the phone should believe.
	it("is one row that cannot become two", async () => {
		const { results } = await env.RECORDS.prepare("SELECT id, version, updated_at FROM store").all();

		expect(results).toEqual([{ id: 1, version: null, updated_at: null }]);

		await expect(env.RECORDS.prepare("INSERT INTO store (id) VALUES (2)").run()).rejects.toThrow();
	});
});
