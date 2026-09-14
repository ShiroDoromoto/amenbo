import { SELF, env } from "cloudflare:test";
import { beforeEach, describe, expect, it } from "vitest";

import worker, { type Env } from "../src/index";

const WRITE_TOKEN = "a-long-random-write-token";
const AT = "https://viewer.example.workers.dev";

/** A request carrying whichever token the test wants it to. */
function carrying(token: string, path: string, init: RequestInit = {}): Promise<Response> {
	return SELF.fetch(`${AT}${path}`, {
		...init,
		headers: { Authorization: `Bearer ${token}`, ...(init.headers ?? {}) },
	});
}

/** A request carrying the token the PC writes with. */
function signed(path: string, init: RequestInit = {}): Promise<Response> {
	return carrying(WRITE_TOKEN, path, init);
}

/** The hash a PC sends when it pairs a phone: SHA-256, lower-case hex. */
async function hashOf(token: string): Promise<string> {
	const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(token));
	return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

/** Issues the read code the way the PC does, over whatever was there. */
async function pair(token: string): Promise<Response> {
	return signed("/tokens", {
		method: "PUT",
		body: JSON.stringify({ hash: await hashOf(token) }),
	});
}

/** A body the way the PC sends one. */
function placement(version: number, records: unknown[]): string {
	return JSON.stringify({ spec_v: 1, version, records });
}

/** One part of a turn the PC is sending in more than one request. */
function sendingPart(
	path: "/records",
	version: number,
	part: number,
	parts: number,
	records: unknown[],
): Promise<Response> {
	return signed(path, { method: "PUT", body: JSON.stringify({ spec_v: 1, version, part, parts, records }) });
}

/** A fingerprint the way a PC writes one: SHA-256, lower-case hex. */
const SEALED_WITH = "a".repeat(64);
const SEALED_WITH_ANOTHER = "b".repeat(64);

/** A body naming the key its records were sealed with. */
function sendingSealedWith(
	path: "/records",
	version: number,
	fingerprint: unknown,
	records: unknown[] = [],
): Promise<Response> {
	return signed(path, {
		method: "PUT",
		body: JSON.stringify({ spec_v: 1, version, key_fingerprint: fingerprint, records }),
	});
}

/** One record sealed on the PC, as it travels. */
function sealed(k: string): Record<string, string> {
	return { k, op: "put", n: `nonce-for-${k}`, c: `ciphertext-for-${k}` };
}

/** Sends a body the way the PC does, to either of the two doors that take one. */
function sending(path: "/records", version: number, records: unknown[]): Promise<Response> {
	return signed(path, { method: "PUT", body: placement(version, records) });
}

/**
 * Reads the way a phone does. Issuing is done here rather than in each test because every read
 * needs a code and none of them are about it — and issuing again over the same hash leaves the
 * same phone reading, so it costs nothing to do it every time.
 */
async function reading(path: string): Promise<Response> {
	await pair("the-phone's-own-token");
	return carrying("the-phone's-own-token", path);
}

beforeEach(async () => {
	await env.RECORDS.exec("DELETE FROM tokens");
	await env.RECORDS.exec("DELETE FROM records");
	await env.RECORDS.exec(
		"UPDATE store SET version = NULL, updated_at = NULL, seq = 0, replacing = 0, placed_from = 0, key_fingerprint = NULL WHERE id = 1",
	);
});

describe("the gate", () => {
	// The gate stands in front of the routing, so a caller carrying nothing we know cannot learn
	// which paths exist by watching 404 turn into 401.
	it("answers the same for a path that exists and one that does not", async () => {
		const real = await SELF.fetch(`${AT}/records`);
		const invented = await SELF.fetch(`${AT}/there-is-no-such-thing`);

		expect(real.status).toBe(401);
		expect(invented.status).toBe(401);
	});

	it("names the scheme it wants, so a caller can fix its request", async () => {
		const response = await SELF.fetch(`${AT}/meta`);

		expect(response.status).toBe(401);
		expect(response.headers.get("WWW-Authenticate")).toBe("Bearer");
	});

	it.each([
		["merely close", `${WRITE_TOKEN.slice(0, -1)}x`],
		["the wrong length", "short"],
		["empty", ""],
	])("refuses a token that is %s", async (_what, token) => {
		const response = await SELF.fetch(`${AT}/meta`, { headers: { Authorization: `Bearer ${token}` } });

		expect(response.status).toBe(401);
	});

	it("refuses another scheme carrying the right value", async () => {
		const response = await SELF.fetch(`${AT}/meta`, { headers: { Authorization: `Basic ${WRITE_TOKEN}` } });

		expect(response.status).toBe(401);
	});

	it("lets the write token through to the endpoint behind it", async () => {
		const response = await signed("/records", { method: "PUT", body: placement(1, []) });

		expect(response.status).toBe(200);
	});
});

describe("the two kinds of token", () => {
	// The whole point of two kinds: what a phone holds cannot write, so a token photographed off
	// somebody's screen cannot put anything into the store.
	it("refuse a read token at a door that writes", async () => {
		await pair("the-phone's-own-token");

		const response = await carrying("the-phone's-own-token", "/records", { method: "PUT", body: placement(1, []) });

		expect(response.status).toBe(403);
		expect(await response.json()).toMatchObject({ error: expect.stringContaining("write") });
	});

	// The other direction is not the point, and used to be refused anyway. The PC holds the key
	// and the backlog these records were sealed from, so reading them back tells it nothing it did
	// not hand over — and being refused meant the comparison it runs had to issue itself a read
	// code, which with one code in the store is the phone's.
	it("let the write token read", async () => {
		expect((await signed("/meta")).status).toBe(200);
		expect((await signed("/records")).status).toBe(200);
	});

	it("let a read token read", async () => {
		await pair("the-phone's-own-token");

		const response = await carrying("the-phone's-own-token", "/meta");

		expect(response.status).toBe(200);
	});

	// A refusal for the wrong kind is only reached once the caller is known, so it never tells an
	// outsider which endpoints exist.
	it("say nothing about the endpoint to a caller with no token at all", async () => {
		const response = await SELF.fetch(`${AT}/tokens`, { method: "PUT", body: "{}" });

		expect(response.status).toBe(401);
	});
});

describe("the routes", () => {
	it.each([
		["PUT", "/records", WRITE_TOKEN],
		["GET", "/records", "the-phone's-own-token"],
		["GET", "/meta", "the-phone's-own-token"],
		["GET", "/records", WRITE_TOKEN],
		["GET", "/meta", WRITE_TOKEN],
	])("%s %s is dispatched", async (method, path, token) => {
		await pair("the-phone's-own-token");

		const response = await carrying(token, path, { method, body: method === "PUT" ? placement(1, []) : undefined });

		expect(response.status).toBe(200);
	});

	it("has nothing else", async () => {
		const response = await signed("/tasks");

		expect(response.status).toBe(404);
	});

	// **410 and not 404.** A sender told there is no such endpoint reads a Worker it does not
	// recognise and says to press setup — which on an older amenbo deploys the Worker that has this
	// door, round and round. Gone is a thing it can read.
	it("says PUT /reset is gone, and where the records go instead", async () => {
		const response = await sending("/reset" as "/records", 1, [sealed("task/1")]);

		expect(response.status).toBe(410);
		expect(await response.json()).toMatchObject({ error: expect.stringContaining("PUT /records") });
	});

	// It is not emptied and it is not half-emptied: a store asked to empty is left exactly as it
	// was, which is what makes the refusal something a sender can act on.
	it("leaves the store alone when one is asked for", async () => {
		await sending("/records", 1, [sealed("task/1")]);

		await sending("/reset" as "/records", 2, [sealed("task/2")]);

		const { results } = await env.RECORDS.prepare("SELECT k FROM records").all();
		expect(results).toEqual([{ k: "task/1" }]);
	});

	// A method the endpoint does not take is not a missing endpoint, and the difference is what
	// tells a caller with a typo from one pointed at the wrong Worker entirely.
	it.each([
		["DELETE", "/records", "PUT, GET, HEAD"],
		["GET", "/reset", "PUT"],
		["PUT", "/meta", "GET, HEAD"],
		["POST", "/meta", "GET, HEAD"],
		["POST", "/tokens", "PUT, DELETE, GET, HEAD"],
	])("%s %s is refused as a method, and says which are allowed", async (method, path, allow) => {
		const response = await signed(path, { method, body: method === "PUT" || method === "POST" ? "…" : undefined });

		expect(response.status).toBe(405);
		expect(response.headers.get("Allow")).toBe(allow);
	});
});

describe("a Worker whose Secret was never set", () => {
	// Nobody gets in either way, so nothing leaks — but a deployment missing its Secret is a
	// mistake the owner can fix, and a flat 401 would send them hunting for the wrong thing.
	it("says so, rather than reading as a wrong token", async () => {
		const unconfigured = { ...env, WRITE_TOKEN: "" } as Env;

		const response = await worker.fetch(new Request(`${AT}/meta`), unconfigured);

		expect(response.status).toBe(503);
		expect(await response.json()).toMatchObject({ error: expect.stringContaining("WRITE_TOKEN") });
	});
});

describe("issuing the read code", () => {
	it("takes a hash and says when it was issued", async () => {
		const response = await pair("the-phone's-own-token");

		expect(response.status).toBe(200);
		expect(await response.json()).toMatchObject({ issued_at: expect.stringContaining("T") });
	});

	// The code itself never arrives — the PC shows it on a QR and sends only its hash, so the
	// value exists on the PC and on the phone that photographed it, and nowhere else.
	it("never receives the code itself", async () => {
		await pair("the-phone's-own-token");

		const { results } = await env.RECORDS.prepare("SELECT hash FROM tokens").all();

		expect(results[0].hash).not.toBe("the-phone's-own-token");
		expect(results[0].hash).toBe(await hashOf("the-phone's-own-token"));
	});

	// There is one code and not one per phone: this Worker compares a hash and never learns which
	// phone offered it, so issuing is what cuts the last one.
	it("replaces the code that was there, and stops whoever held it", async () => {
		await pair("the-old-token");

		const response = await pair("the-new-token");

		expect(response.status).toBe(200);
		expect((await carrying("the-new-token", "/meta")).status).toBe(200);
		expect((await carrying("the-old-token", "/meta")).status).toBe(401);
	});

	// One row, whatever the store has been through: a second code that survived issuing would be a
	// phone reading that nothing on the screen says is there.
	it("leaves one row behind however many times it is issued", async () => {
		await pair("the-first-token");
		await pair("the-second-token");
		await pair("the-third-token");

		const { results } = await env.RECORDS.prepare("SELECT id FROM tokens").all();

		expect(results).toEqual([{ id: 1 }]);
	});

	it.each([
		["nothing at all", "{}"],
		["a hash that is not one", JSON.stringify({ hash: "not-a-hash" })],
		["a hash of the wrong length", JSON.stringify({ hash: "abc" })],
		["something that is not a document", "{{{"],
	])("refuses %s", async (_what, body) => {
		const response = await signed("/tokens", { method: "PUT", body });

		expect(response.status).toBe(400);
	});
});

describe("taking the read code away", () => {
	it("stops the phone that held it", async () => {
		await pair("the-lost-phone's-token");

		const response = await signed("/tokens", { method: "DELETE" });

		expect(response.status).toBe(200);
		expect(await response.json()).toMatchObject({ cut: true });
		expect((await carrying("the-lost-phone's-token", "/meta")).status).toBe(401);
	});

	// Asking a store with no code to have no code is asking for the state it is in. There is no
	// name to mistype any more, so there is nothing for a refusal to catch — but whether there was
	// one to cut is still worth saying, since the screen draws it.
	it("says there was none, rather than refusing", async () => {
		const response = await signed("/tokens", { method: "DELETE" });

		expect(response.status).toBe(200);
		expect(await response.json()).toMatchObject({ cut: false });
	});
});

describe("asking whether a phone can read", () => {
	it("says so, and when the code was issued", async () => {
		await pair("the-phone's-own-token");

		const response = await signed("/tokens");

		expect(response.status).toBe(200);
		expect(await response.json()).toMatchObject({ paired: true, issued_at: expect.stringContaining("T") });
	});

	it("says there is none on a store nobody has paired with", async () => {
		const response = await signed("/tokens");

		expect(response.status).toBe(200);
		expect(await response.json()).toEqual({ paired: false, issued_at: null });
	});

	// The hash is the only copy of the read credential this end holds. A write token has no use
	// for it, so it does not travel.
	it("does not hand back the hash", async () => {
		await pair("the-phone's-own-token");

		const response = await signed("/tokens");

		expect(JSON.stringify(await response.json())).not.toContain(await hashOf("the-phone's-own-token"));
	});

	// It is the PC's question — the screen that draws it holds the write token, and a phone
	// holding the code has no business enumerating the store.
	it("is not a phone's to ask", async () => {
		await pair("the-phone's-own-token");

		const response = await carrying("the-phone's-own-token", "/tokens");

		expect(response.status).toBe(403);
	});
});

describe("placing what moved", () => {
	it("takes the records and answers with where the order now stands", async () => {
		const response = await sending("/records", 12345, [sealed("task/1"), sealed("task/2")]);

		expect(response.status).toBe(200);
		expect(await response.json()).toMatchObject({ seq: 2 });
	});

	// **What the database actually did, rather than what the sender guessed it would do.** A
	// sender keeping a budget against a daily row limit would otherwise carry a model of what an
	// upsert costs onto a key that is here and onto one that is not — a model living in the wrong
	// part, going stale the day D1 changes.
	it("answers with the rows the database wrote", async () => {
		const response = await sending("/records", 12345, [sealed("task/1"), sealed("task/2")]);

		const { rows_written } = (await response.json()) as { rows_written: number };
		expect(rows_written).toBeGreaterThanOrEqual(2);
	});

	// **The sender holds the write token and nothing else**, so which Worker it is talking to has
	// to come back on a write. A sender that reads no number is talking to one deployed before
	// there was a number, which is exactly what it needs to know.
	it("names which build of this Worker it is", async () => {
		const response = await sending("/records", 1, []);

		expect(await response.json()).toMatchObject({ build: expect.any(Number) });
	});

	// A record that moves again is the same record, so it keeps its one row and takes a later
	// place in the order — two rows for one key would show a phone the record twice, once stale.
	it("moves a record that comes again to the front of the order", async () => {
		await sending("/records", 1, [sealed("task/1"), sealed("task/2")]);

		await sending("/records", 2, [sealed("task/1")]);

		const { results } = await env.RECORDS.prepare("SELECT k, seq FROM records ORDER BY seq").all();
		expect(results).toEqual([
			{ k: "task/2", seq: 2 },
			{ k: "task/1", seq: 3 },
		]);
	});

	// A deletion is a row of its own with nothing in the envelope, so a phone that was away learns
	// the record went rather than never hearing of it again.
	it("takes a deletion as a record with an empty envelope", async () => {
		await sending("/records", 1, [sealed("task/1")]);

		await sending("/records", 2, [{ k: "task/1", op: "del" }]);

		const { results } = await env.RECORDS.prepare("SELECT op, nonce, ciphertext FROM records").all();
		expect(results).toEqual([{ op: "del", nonce: null, ciphertext: null }]);
	});

	// **The failure this pins shut.** Dropping a turn whose version the store already stands at, and
	// answering `200` to it, reasons that the same number means the same records. The backlog's
	// version is too coarse a name for a turn — a send made while the backlog has not moved carries
	// that number without being a repeat of anything — so such a turn is real edits, thrown away and
	// reported as landed, and the one command for sending them again dies the same way.
	it("writes what it is handed even under the version it already holds", async () => {
		await sending("/records", 12345, [sealed("task/1")]);

		const response = await sending("/records", 12345, [sealed("task/2")]);

		expect(await response.json()).toMatchObject({ seq: 2 });
		const { results } = await env.RECORDS.prepare("SELECT k FROM records ORDER BY seq").all();
		expect(results).toEqual([{ k: "task/1" }, { k: "task/2" }]);
	});

	// The version travels so a phone can tell what it is looking at, and it only means anything if
	// it moved with the records it came in with.
	it("remembers the version and when the records landed", async () => {
		await sending("/records", 12345, [sealed("task/1")]);

		const { version, updated_at } = (await reading("/meta").then((it) => it.json())) as {
			version: number;
			updated_at: string;
		};

		expect(version).toBe(12345);
		expect(Date.parse(updated_at)).not.toBeNaN();
	});

	// Too much is refused at the door with a sentence saying what to do about it, rather than
	// failing somewhere inside the database where the sender learns only that it did not work.
	it("refuses more records than one write takes, and says to send it in parts", async () => {
		const records = Array.from({ length: 501 }, (_at, n) => sealed(`task/${n}`));

		const response = await sending("/records", 1, records);

		expect(response.status).toBe(413);
		expect(await response.json()).toMatchObject({ error: expect.stringContaining("in parts") });
	});

	it.each([
		["a contract version it does not read", JSON.stringify({ spec_v: 2, version: 1, records: [] })],
		["nothing at all", "{}"],
		["no version", JSON.stringify({ spec_v: 1, records: [] })],
		["a version that is not a number", JSON.stringify({ spec_v: 1, version: "12345", records: [] })],
		["records that are not a list", JSON.stringify({ spec_v: 1, version: 1, records: {} })],
		["a record with no key", JSON.stringify({ spec_v: 1, version: 1, records: [{ op: "put", n: "n", c: "c" }] })],
		["an operation it has no reading for", JSON.stringify({ spec_v: 1, version: 1, records: [{ k: "task/1", op: "archive" }] })],
		["a put with no envelope", JSON.stringify({ spec_v: 1, version: 1, records: [{ k: "task/1", op: "put" }] })],
		["something that is not a document", "{{{"],
	])("refuses %s", async (_what, body) => {
		const response = await signed("/records", { method: "PUT", body });

		expect(response.status).toBe(400);
	});
});

describe("reading what moved", () => {
	it("hands back everything after a point in the order", async () => {
		await sending("/records", 1, [sealed("task/1"), sealed("task/2"), sealed("task/3")]);

		const answered = (await reading("/records?since=1").then((it) => it.json())) as {
			records: Record<string, string>[];
		};

		expect(answered.records).toEqual([sealed("task/2"), sealed("task/3")]);
	});

	it("reads from the beginning when asked for no point at all", async () => {
		await sending("/records", 1, [sealed("task/1")]);

		const answered = (await reading("/records").then((it) => it.json())) as { records: unknown[] };

		expect(answered.records).toEqual([sealed("task/1")]);
	});

	// The phone remembers where the page got to and asks from there, so what it costs to be a week
	// behind is a few more pages rather than the whole store in one answer.
	it("splits a long stretch into pages, and says there is more", async () => {
		await sending("/records", 1, Array.from({ length: 250 }, (_at, n) => sealed(`task/${n}`)));

		const first = (await reading("/records?since=0").then((it) => it.json())) as {
			seq: number;
			more: boolean;
			records: unknown[];
		};
		const next = (await reading(`/records?since=${first.seq}`).then((it) => it.json())) as {
			seq: number;
			more: boolean;
			records: unknown[];
		};

		expect(first).toMatchObject({ seq: 200, more: true });
		expect(first.records).toHaveLength(200);
		expect(next).toMatchObject({ seq: 250, more: false });
		expect(next.records).toHaveLength(50);
	});

	it("says where it is and what it is reading, alongside the records", async () => {
		await sending("/records", 12345, [sealed("task/1")]);

		const answered = await reading("/records?since=0").then((it) => it.json());

		expect(answered).toMatchObject({ spec_v: 1, version: 12345, seq: 1, more: false });
	});

	it("stays where it is when there is nothing after the point asked for", async () => {
		await sending("/records", 1, [sealed("task/1")]);

		const answered = await reading("/records?since=1").then((it) => it.json());

		expect(answered).toMatchObject({ seq: 1, more: false, records: [] });
	});

	// A deletion has nothing to open, and a phone that was handed an empty envelope would try.
	it("hands a deletion back with no envelope on it", async () => {
		await sending("/records", 1, [{ k: "task/1", op: "del" }]);

		const answered = (await reading("/records?since=0").then((it) => it.json())) as { records: unknown[] };

		expect(answered.records).toEqual([{ k: "task/1", op: "del" }]);
	});

	// The order never rewinds, so a cursor past the end is not a phone that is early — it is one
	// paired with a store that no longer exists, and answering the tail would hand it somebody
	// else's rows under numbers it recognises.
	it("refuses a cursor that is past the end of the order", async () => {
		await sending("/records", 1, [sealed("task/1")]);

		const response = await reading("/records?since=99");

		expect(response.status).toBe(409);
		expect(await response.json()).toMatchObject({ error: expect.stringContaining("from the beginning") });
	});

	it.each([
		["not a number", "?since=soon"],
		["a number with a sign on it", "?since=-1"],
		["empty", "?since="],
	])("refuses a point in the order that is %s", async (_what, query) => {
		const response = await reading(`/records${query}`);

		expect(response.status).toBe(400);
	});
});

// **What is here that the backlog no longer has cannot be worked out from this end.** This Worker
// does not know what a key means, so the sender is the only one that can compare — and comparing
// means reading every key. The whole answer is twenty-seven megabytes of envelopes for a question
// that turns on the strings alone.
describe("reading the keys alone", () => {
	it("hands back the keys and their last word, and no envelopes", async () => {
		await sending("/records", 1, [sealed("task/1")]);
		await sending("/records", 2, [{ k: "task/2", op: "del" }]);

		const answered = (await reading("/records?since=0&keys=1").then((it) => it.json())) as {
			records: Record<string, string>[];
		};

		expect(answered.records).toEqual([
			{ k: "task/1", op: "put" },
			{ k: "task/2", op: "del" },
		]);
	});

	// **A key whose last word was `del` is a headstone, not something the store holds.** A sender
	// that could not tell the two apart would send a delete for every one of them, every time,
	// and each of those writes a headstone of its own.
	it("says which of them are headstones rather than leaving them out", async () => {
		await sending("/records", 1, [sealed("task/1"), { k: "task/2", op: "del" }]);

		const answered = (await reading("/records?since=0&keys=1").then((it) => it.json())) as {
			records: { k: string }[];
		};

		expect(answered.records.map((one) => one.k)).toEqual(["task/1", "task/2"]);
	});

	// A page of keys is far larger than a page of records, because a key is a fraction of one:
	// reading a whole store's keys two hundred at a time would be a hundred and twenty requests
	// for something a dozen can carry.
	it("carries far more of them in one page than a page of records holds", async () => {
		const many = Array.from({ length: 250 }, (_at, n) => sealed(`task/${n}`));
		await sendingPart("/records", 1, 1, 1, many.slice(0, 250));

		const keys = (await reading("/records?since=0&keys=1").then((it) => it.json())) as { more: boolean };
		const rows = (await reading("/records?since=0").then((it) => it.json())) as { more: boolean };

		expect(keys.more).toBe(false);
		expect(rows.more).toBe(true);
	});

	// It is the same read, so it is refused for the same reasons: a cursor the order never
	// reached is not one this store counted.
	it("is refused where the whole read is refused", async () => {
		await sending("/records", 1, [sealed("task/1")]);

		expect((await reading("/records?since=99&keys=1")).status).toBe(409);
	});

	// Anything but `keys=1` is the ordinary read, so a phone that has never heard of the
	// parameter and one that passes something else are answered the same way.
	it("is the ordinary read for anything but keys=1", async () => {
		await sending("/records", 1, [sealed("task/1")]);

		const answered = (await reading("/records?since=0&keys=yes").then((it) => it.json())) as {
			records: unknown[];
		};

		expect(answered.records).toEqual([sealed("task/1")]);
	});
});

describe("where the store stands", () => {
	// The cheap question, asked before the expensive one: a phone that is level learns so for the
	// price of one small answer, which is what lets it ask often.
	it("says nothing has landed yet, rather than nothing at all", async () => {
		const answered = await reading("/meta").then((it) => it.json());

		expect(answered).toEqual({
			spec_v: 1,
			version: null,
			seq: 0,
			updated_at: null,
			placed_from: 0,
			key_fingerprint: null,
		});
	});

	it("says the version and how far the order has got", async () => {
		await sending("/records", 12345, [sealed("task/1"), sealed("task/2")]);

		const answered = await reading("/meta").then((it) => it.json());

		expect(answered).toMatchObject({ spec_v: 1, version: 12345, seq: 2 });
	});
});

describe("the key the records were sealed with", () => {
	// A phone whose own key hashes to something else cannot open a single row here, and the whole
	// point of naming it is that learning so costs one small answer rather than a whole fetch.
	it("comes back on the cheap question, once a placement has named one", async () => {
		await sendingSealedWith("/records", 1, SEALED_WITH, [sealed("task/1")]);

		const answered = await reading("/meta").then((it) => it.json());

		expect(answered).toMatchObject({ key_fingerprint: SEALED_WITH });
	});

	// Every send written before this field existed names no key, so refusing them would turn an
	// upgrade of this Worker into a store nobody could write to.
	it("is nothing at all when the sender named none", async () => {
		await sending("/records", 1, [sealed("task/1")]);

		const answered = await reading("/meta").then((it) => it.json());

		expect(answered).toMatchObject({ key_fingerprint: null });
	});

	// It says what is in the records now, not what the last sender happened to say. Left standing,
	// a phone would compare its key against an answer nothing here vouches for any more.
	it("is let go by a turn that names none, rather than standing for the last one", async () => {
		await sendingSealedWith("/records", 1, SEALED_WITH, [sealed("task/1")]);

		await sending("/records", 2, [sealed("task/2")]);

		const answered = await reading("/meta").then((it) => it.json());
		expect(answered).toMatchObject({ key_fingerprint: null });
	});

	// The one placement that matters most: a key drawn afresh comes with the whole store sent
	// again, and the answer has to move with it.
	it("moves when the whole store is sent again under another key", async () => {
		await sendingSealedWith("/records", 1, SEALED_WITH, [sealed("task/1")]);

		await sendingSealedWith("/records", 2, SEALED_WITH_ANOTHER, [sealed("task/1")]);

		const answered = await reading("/meta").then((it) => it.json());
		expect(answered).toMatchObject({ key_fingerprint: SEALED_WITH_ANOTHER });
	});

	// Settled by the last part alone, like the version — a turn cut short leaves the store naming
	// the key it really holds rather than one that is only half placed.
	it("is written down only once the last part has landed", async () => {
		await signed("/records", {
			method: "PUT",
			body: JSON.stringify({
				spec_v: 1,
				version: 7,
				part: 1,
				parts: 2,
				key_fingerprint: SEALED_WITH,
				records: [sealed("task/1")],
			}),
		});

		const midway = await env.RECORDS.prepare("SELECT key_fingerprint FROM store WHERE id = 1").first();
		expect(midway).toEqual({ key_fingerprint: null });

		await signed("/records", {
			method: "PUT",
			body: JSON.stringify({
				spec_v: 1,
				version: 7,
				part: 2,
				parts: 2,
				key_fingerprint: SEALED_WITH,
				records: [sealed("task/2")],
			}),
		});

		const answered = await reading("/meta").then((it) => it.json());
		expect(answered).toMatchObject({ key_fingerprint: SEALED_WITH });
	});

	// What is kept is a hash and never a key, so anything that is not one is turned away at the
	// door rather than written down for a phone to compare against.
	it("refuses anything that is not a SHA-256", async () => {
		for (const notAHash of ["", "the-key-itself", "a".repeat(63), "z".repeat(64), 12345, {}]) {
			const answered = await sendingSealedWith("/records", 1, notAHash, [sealed("task/1")]);

			expect(answered.status, JSON.stringify(notAHash)).toBe(400);
			expect(await answered.json<{ error: string }>()).toMatchObject({
				error: expect.stringContaining("64 hex characters"),
			});
		}
	});

	// A hash is the same hash however it was spelled, and this Worker only ever compares strings
	// — so the case is settled here rather than at both far ends.
	it("is kept in lower case, whichever case it arrived in", async () => {
		await sendingSealedWith("/records", 1, SEALED_WITH.toUpperCase(), [sealed("task/1")]);

		const answered = await reading("/meta").then((it) => it.json());

		expect(answered).toMatchObject({ key_fingerprint: SEALED_WITH });
	});
});

// **Nothing writes `placed_from` any more, and it is still read.** The door that emptied and
// filled a store is gone, but the stores it left behind are on people's accounts and the number
// it wrote there is what still keeps a phone from reading half of one. Reading it costs a column
// on a row already being read; dropping it would cost a phone its backlog, on a Worker that
// cannot be corrected afterwards.
describe("a store an older Worker emptied and filled again", () => {
	/** Marks the store the way the door that emptied it did, above the records placed since. */
	async function emptiedAbove(seq: number): Promise<void> {
		await env.RECORDS.prepare("UPDATE store SET placed_from = ? WHERE id = 1").bind(seq).run();
	}

	// The failure the whole of this exists for. Without it the phone reads on from 1, is handed
	// only what came after the emptying, and keeps a record the backlog no longer has for good.
	it("sends a phone that was level back to the beginning", async () => {
		await sending("/records", 1, [sealed("task/1")]);
		await emptiedAbove(1);
		await sending("/records", 2, [sealed("task/2")]);

		expect((await reading("/records?since=1")).status).toBe(409);
		const answered = (await reading("/records?since=0").then((it) => it.json())) as { records: unknown[] };
		expect(answered.records).toEqual([sealed("task/1"), sealed("task/2")]);
	});

	// A phone reading inside the placement is not sent back: what it holds is the front of this
	// placement rather than of the one before it, so it is behind and not wrong.
	it("lets a phone that is partway through the placement read on", async () => {
		await sending("/records", 1, [sealed("task/1")]);
		await emptiedAbove(1);
		await sending("/records", 2, [sealed("task/2"), sealed("task/3")]);

		const answered = (await reading("/records?since=2").then((it) => it.json())) as { records: unknown[] };
		expect(answered.records).toEqual([sealed("task/3")]);
	});

	it("says where the placement began, so a phone can tell before it asks", async () => {
		await sending("/records", 1, [sealed("task/1")]);
		await emptiedAbove(1);
		await sending("/records", 2, [sealed("task/2")]);

		expect(await reading("/meta").then((it) => it.json())).toMatchObject({ placed_from: 1, seq: 2 });
	});

	// **A store left half-emptied by an abandoned placement answers again.** The flag came down
	// with the last part alone, so a placement nobody finished shut both reading doors for good —
	// writable, permanently unreadable, and nothing in it able to put itself right. There is
	// nothing to come down now, because nothing goes up.
	it("answers again even with the half-placed flag left standing", async () => {
		await sending("/records", 1, [sealed("task/1")]);
		await env.RECORDS.prepare("UPDATE store SET replacing = 1 WHERE id = 1").run();

		expect((await reading("/meta")).status).toBe(200);
		expect((await reading("/records?since=0")).status).toBe(200);
	});
});

describe("sending one turn in more than one request", () => {
	// The later parts carry the same version as the first, and a store that wrote the version down
	// on the first would read every one of them as a repeat and throw them away — leaving a phone
	// the first five hundred records of a backlog and the version saying that is all of it.
	it("takes every part rather than reading the later ones as a repeat", async () => {
		await sendingPart("/records", 12345, 1, 2, [sealed("task/1")]);

		await sendingPart("/records", 12345, 2, 2, [sealed("task/2")]);

		const { results } = await env.RECORDS.prepare("SELECT k FROM records ORDER BY seq").all();
		expect(results).toEqual([{ k: "task/1" }, { k: "task/2" }]);
	});

	// The version is what a phone reads to know what it is looking at, so a turn that has not
	// finished arriving must not be named by it.
	it("writes the version down only once the last part has landed", async () => {
		await sendingPart("/records", 12345, 1, 2, [sealed("task/1")]);
		expect(await reading("/meta").then((it) => it.json())).toMatchObject({ version: null, seq: 1 });

		await sendingPart("/records", 12345, 2, 2, [sealed("task/2")]);

		expect(await reading("/meta").then((it) => it.json())).toMatchObject({ version: 12345, seq: 2 });
	});

	// **Nothing closes while a turn is in flight.** What has landed of one is a true prefix of
	// what is coming — the store was not emptied first — so a phone reading it is behind rather
	// than wrong, and being behind is the state every phone is in between writes anyway.
	it("keeps both reading doors open while a turn is half sent", async () => {
		await sendingPart("/records", 2, 1, 2, [sealed("task/1")]);

		expect((await reading("/meta")).status).toBe(200);
		const answered = (await reading("/records?since=0").then((it) => it.json())) as { records: unknown[] };
		expect(answered.records).toEqual([sealed("task/1")]);
	});

	// One request is one whole turn, which is what every send was before there were parts at all.
	it("takes a body that says nothing about parts as the whole of a turn", async () => {
		await sending("/records", 2, [sealed("task/1")]);

		expect(await reading("/meta").then((it) => it.json())).toMatchObject({ version: 2, seq: 1 });
	});

	it.each([
		["a part that is not a number", JSON.stringify({ spec_v: 1, version: 1, part: "1", parts: 2, records: [] })],
		["a part before the first", JSON.stringify({ spec_v: 1, version: 1, part: 0, parts: 2, records: [] })],
		["a part past the last", JSON.stringify({ spec_v: 1, version: 1, part: 3, parts: 2, records: [] })],
		["a turn of no parts at all", JSON.stringify({ spec_v: 1, version: 1, part: 1, parts: 0, records: [] })],
		["a count of parts that is not whole", JSON.stringify({ spec_v: 1, version: 1, part: 1, parts: 1.5, records: [] })],
	])("refuses %s", async (_what, body) => {
		const response = await signed("/records", { method: "PUT", body });

		expect(response.status).toBe(400);
	});
});

// **Every failure is one answer, and the answer carries what was said.** Telling a full database
// from an unreachable one meant matching on the sentence D1 threw, and a reading that is wrong
// sends someone to buy storage they do not need. This Worker cannot be corrected when a reading
// turns out to be wrong, so it does not read — it hands the sentence to the part that can be.
describe("a store that could not answer", () => {
	/**
	 * What the binding throws when D1 is out of room, copied off a real database filled to its
	 * ceiling: a plain `Error`, no code and no status, and the `7500` the REST API answers with
	 * nowhere in it. The message is the only thing there is to go on.
	 */
	const NO_ROOM = () =>
		new Error("D1_ERROR: Exceeded maximum DB size", { cause: new Error("Exceeded maximum DB size") });

	/** A D1 failure that is not about room, as one arrives. */
	const SOMETHING_ELSE = () =>
		new Error(
			"D1_ERROR: UNIQUE constraint failed: records.seq: SQLITE_CONSTRAINT (extended: SQLITE_CONSTRAINT_UNIQUE)",
		);

	/**
	 * The same database, but every write fails the given way. Reads are left alone: the gate has
	 * to let the caller in for any of this to be reached, and a full store still answers them.
	 */
	function whereWritesFail(error: () => Error): Env {
		const statement = (real: D1PreparedStatement): D1PreparedStatement =>
			new Proxy(real, {
				get(target, property, receiver) {
					if (property === "bind") {
						return (...values: unknown[]) => statement(target.bind(...values));
					}
					if (property === "run") {
						return () => Promise.reject(error());
					}
					const value = Reflect.get(target, property, receiver);
					return typeof value === "function" ? value.bind(target) : value;
				},
			});
		const database = new Proxy(env.RECORDS, {
			get(target, property, receiver) {
				if (property === "batch") {
					return () => Promise.reject(error());
				}
				if (property === "prepare") {
					return (sql: string) => statement(target.prepare(sql));
				}
				const value = Reflect.get(target, property, receiver);
				return typeof value === "function" ? value.bind(target) : value;
			},
		});
		return { ...env, RECORDS: database } as Env;
	}

	/** A request the PC's token carries, against a store standing in for a full one. */
	function into(store: Env, path: string, init: RequestInit = {}): Promise<Response> {
		return worker.fetch(
			new Request(`${AT}${path}`, {
				...init,
				headers: { Authorization: `Bearer ${WRITE_TOKEN}`, ...(init.headers ?? {}) },
			}),
			store,
		);
	}

	it("hands back what the database said, and how long to leave it", async () => {
		const response = await into(whereWritesFail(NO_ROOM), "/records", {
			method: "PUT",
			body: placement(1, [sealed("task/1")]),
		});

		expect(response.status).toBe(503);
		expect(await response.json()).toMatchObject({ error: expect.stringContaining("Exceeded maximum DB size") });
	});

	// **The phone reads the number and not the reason.** Ten seconds or under means "the PC is
	// still sending"; anything above means the store cannot answer. A full database answered as
	// the first would have every phone saying the PC is busy, forever.
	it("waits longer than the number that means the PC is still sending", async () => {
		const response = await into(whereWritesFail(NO_ROOM), "/records", {
			method: "PUT",
			body: placement(1, [sealed("task/1")]),
		});

		expect(Number(response.headers.get("Retry-After"))).toBeGreaterThan(10);
	});

	it("says the same when a phone is being paired", async () => {
		const response = await into(whereWritesFail(NO_ROOM), "/tokens", {
			method: "PUT",
			body: JSON.stringify({ hash: await hashOf("the-phone's-own-token") }),
		});

		expect(response.status).toBe(503);
	});

	// **Every other failure comes back the same way**, rather than falling out of the Worker as a
	// bare 500 the phone reads as "look at the PC". What it is is in the sentence.
	it("answers a failure that is nothing to do with room the same way", async () => {
		const response = await into(whereWritesFail(SOMETHING_ELSE), "/records", {
			method: "PUT",
			body: placement(1, [sealed("task/1")]),
		});

		expect(response.status).toBe(503);
		expect(await response.json()).toMatchObject({ error: expect.stringContaining("UNIQUE constraint failed") });
	});
});

/**
 * The size of a real backlog, as the store that went to iCloud held it.
 *
 * **This is the run that was never made.** Every test above works at two or three records, and
 * what broke on the other route broke only once there were twenty thousand — so the number itself
 * is the point here, not the shape. It is not a round one on purpose: a backlog that divides
 * evenly into writes and pages never sends a short last one, and a real turn always does.
 */
const A_REAL_BACKLOG = 20202;

/**
 * How long a test at that size is given.
 *
 * The default of five seconds is right for a test at three records and wrong for one at twenty
 * thousand: reading this backlog back is a hundred and two pages, and a runner slower than a
 * laptop crosses five seconds on the way. The number is a ceiling on a test that has already
 * passed, not a target — what it buys is the difference between a slow machine and a broken one.
 */
const LONG_ENOUGH_FOR_A_REAL_BACKLOG = 60_000;

describe("the whole of a real backlog", () => {
	/** Sends `total` records through the one door that takes them, in the parts a write takes. */
	async function placeWhole(total: number, version: number): Promise<void> {
		const parts = Math.ceil(total / 500);
		for (let part = 1; part <= parts; part++) {
			const from = (part - 1) * 500;
			const records = Array.from({ length: Math.min(500, total - from) }, (_, at) => sealed(`task/${from + at}`));
			const answered = await sendingPart("/records", version, part, parts, records);
			expect(answered.status, `part ${part} of ${parts}`).toBe(200);
		}
	}

	it("holds every record it was sent, once each", async () => {
		await placeWhole(A_REAL_BACKLOG, 7);

		const held = await env.RECORDS.prepare("SELECT COUNT(*) AS n FROM records").first<{ n: number }>();
		expect(held?.n).toBe(A_REAL_BACKLOG);
		const distinct = await env.RECORDS.prepare("SELECT COUNT(DISTINCT k) AS n FROM records").first<{ n: number }>();
		expect(distinct?.n).toBe(A_REAL_BACKLOG);
	}, LONG_ENOUGH_FOR_A_REAL_BACKLOG);

	it("names the version only once the last part has landed", async () => {
		const parts = Math.ceil(A_REAL_BACKLOG / 500);
		for (let part = 1; part < parts; part++) {
			await sendingPart("/records", 7, part, parts, [sealed(`task/${part}`)]);
			const midway = await env.RECORDS.prepare("SELECT version FROM store WHERE id = 1").first<{ version: number | null }>();
			expect(midway?.version, `after part ${part}`).toBeNull();
		}
		await sendingPart("/records", 7, parts, parts, [sealed("task/last")]);

		const settled = await env.RECORDS.prepare("SELECT version FROM store WHERE id = 1").first<{ version: number }>();
		expect(settled?.version).toBe(7);
	}, LONG_ENOUGH_FOR_A_REAL_BACKLOG);

	it("reads back to a phone with nothing missing and nothing twice", async () => {
		await placeWhole(A_REAL_BACKLOG, 7);

		const seen = new Set<string>();
		let cursor = 0;
		let pages = 0;
		for (;;) {
			const answered = await reading(`/records?since=${cursor}`);
			expect(answered.status, `page ${pages + 1}`).toBe(200);
			const read = (await answered.json()) as { records: { k: string }[]; seq: number; more: boolean };
			for (const record of read.records) {
				// A key twice would have the phone write one row over another and never learn of
				// the one it lost.
				expect(seen.has(record.k), `${record.k} came twice`).toBe(false);
				seen.add(record.k);
			}
			// The order belongs to the page rather than to each row, and it is what the next read
			// asks from — a page that did not move it on would have the phone read it again.
			expect(read.seq, `page ${pages + 1} did not move the order on`).toBeGreaterThan(cursor);
			cursor = read.seq;
			pages++;
			if (!read.more) break;
			// A backlog this size is a hundred pages; a read that stopped answering would
			// otherwise sit here rather than saying so.
			expect(pages, "the phone is still reading well past the pages this backlog holds").toBeLessThan(A_REAL_BACKLOG);
		}

		expect(seen.size).toBe(A_REAL_BACKLOG);
		expect(pages).toBe(Math.ceil(A_REAL_BACKLOG / 200));
	}, LONG_ENOUGH_FOR_A_REAL_BACKLOG);
});
