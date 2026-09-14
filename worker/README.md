# worker — the Cloudflare Worker

Stood up in the user's own Cloudflare account. We host nothing. Amenbo helps with the setup, and
once it stands, the owner of it is the user.

It interprets nothing. What it takes is a key the PC chose and a sealed run of bytes the PC wrote.

Authentication is a long random token generated at setup. No account, no password. The encryption
key never travels, so what Cloudflare sees is ciphertext.

| response | |
|---|---|
| `401` | no token, or one this Worker does not know. The check stands ahead of routing, so a caller who is not through it cannot see the routes either |
| `403` | a read code tried to write, or asked `/tokens` (**a write token reading is allowed** — see "Two kinds of token") |
| `404` / `405` | no such route / not that method (an `Allow` header comes back) |
| `400` | the body is not in a shape this can take |
| `409` | the phone's cursor points past where the order stands, or **a placement has overtaken it** (see "The order never rewinds" and "A cursor a placement overtook is refused") |
| `410` | **`PUT /reset` is gone** (see "There is no door that empties this") |
| `413` | one write carried more rows than this takes |
| `503` | the Secret is not set, or **whatever was thrown**, whatever it was — D1's own sentence comes back with `Retry-After: 60` (see "It passes on what it is told, and judges none of it") |

## Two kinds of token

The PC writes and the phone reads. Neither carries the other's credential.

| | where it lives | what it opens |
|---|---|---|
| write token | this Worker's Secret (`WRITE_TOKEN`), one of them | **every route** |
| read code | one row in `tokens`, **the hash alone** | `GET` |

**There is one read code, not one per phone.** This Worker sees a hash and nothing else: it does not
know which phone presented it, so one code photographed by three phones is one row that all three
read from. Cutting a single phone loose by name would need something this side does not hold — the
name would only ever have pointed at something inside the PC.

So what the row holds is whether a code exists at all. Issuing one overwrites the one before it, and
that is the moment the old one stops working.

| route | |
|---|---|
| `PUT /tokens` | takes `{"hash":…}` and places one. **The code's own value never arrives.** The previous one is cut here |
| `DELETE /tokens` | removes that one. `200` even when there is none — `{"cut":false}` says there was nothing to cut |
| `GET /tokens` | `{"paired":…,"issued_at":…}`. **A write token's question**; a phone gets `403` |

- The hash is **SHA-256, lowercase hex, 64 characters**. The PC computes it; this Worker only compares
- **Issuing overwrites and never refuses.** Reconnecting is one step, not a cut followed by an issue
- **`DELETE` has no typo to catch.** There is no name to type, so asking to remove what is not there
  is asking for the state that already holds
- **`GET` does not return the hash.** The only copy of the reading side's credential is here, and a
  write token has no use for it
- **The half worth guarding is the phone's.** A code photographed off a screen must not be able to
  write anything into the store. **The other direction is deliberately open**: the PC holds the
  encryption key and the ledger, so reading its own records back hands it nothing it did not send
  (see "A read route takes a write token too")
- The write token is compared in constant time. The read side is compared **by hash**, so it is not —
  what could leak there is the hash, and knowing it does not get anyone through

## The store is D1

**One record, one row.** Overwriting a single key cannot answer either question this store is asked —
"what has changed since this point in the order" and "forget this row" — so what a phone reads is a
range, and what the PC sends is the handful of rows that moved.

| table | what is in it |
|---|---|
| `records` | key, order (`seq`), operation (`put` / `del`), nonce, ciphertext |
| `tokens` | the read code's **hash** and its issue date (**one row**, like `store` — `CHECK (id = 1)` says so) |
| `store` | the ledger's version, **where the order stands**, last write, **where the current placement began**, and **the fingerprint of the key the records in it were sealed with** (one row). The `replacing` column is still there and **nothing reads or writes it** — dropping it is a migration that would run in the user's account, which is not worth what it buys |

- **A deletion stays as a row.** Made to vanish, it takes with it the only chance a phone that was
  away has of learning the record is gone
- `seq` is unique. Two rows at one position leave the page boundary undefined, and a phone either
  skips rows or reads them twice
- **The code's value is not stored.** Comparison is all this does, so keeping it would only widen
  what can leak

### A read route takes a write token too

`GET /meta` and `GET /records` accept **either the read code or the write token**. What is refused is
a read code trying to write, and a read code asking `/tokens`.

**`repair` is what needs it.** Reconciling reads every key the store holds, which takes a credential
that may read. With one read code in the table, a reconcile that issued itself a code would cut the
phone loose the moment it issued, and leave zero codes the moment it cleaned up — a reconnect for
every single `repair`.

**Opening it adds no reach.** The write token belongs to the PC, and the PC has the encryption key and
the ledger. What reading gets it is what it sealed itself.

### It passes on what it is told, and judges none of it

**Anything thrown comes back as `503` plus `Retry-After: 60` plus D1's sentence, verbatim.** This
Worker does not read what happened.

Reading it would mean **telling a full database from a broken one by matching a string**: D1's binding
throws a bare `Error` with no code and no status (the `7500` the REST API returns never reaches a
Worker), leaving `Exceeded maximum DB size` in the text as the only handle. **The direction that reading
fails in is the bad one** — answering "buy more capacity" to a database that was briefly unreachable
sends a person to an expense they do not owe.

And **this Worker cannot correct a reading that turns out wrong**: it stands in the user's account and
changes only when someone presses setup. So it does not read. The sentence goes to the sender
untouched, and **reading it is the job of the side that can be updated** — Amenbo.

**`Retry-After` is above ten seconds because the phone reads that number and nothing else.** Ten or
under means "the PC is still sending"; above it means "the server cannot answer right now". A full
database answered as the first leaves a phone saying "the PC is still sending" forever.

### There is no door that empties this

**`PUT /reset` returns `410` and a sentence.** Emptying the store and refilling it is not how a full
placement is done.

- **No phone can read a single row until the refill finishes.** A store part-way through holds a
  fragment of the ledger, which a phone cannot tell from a ledger that genuinely shrank. That leaves
  closing the read route as the only option, and the refilling side is the slow one, so the window
  lasts exactly as long as the sending is slow
- **A placement nobody finished pins the store at "writable, permanently unreadable".** Only the last
  part reopens the closed route, and an abandoned run never reaches its last part
- **It costs twice the rows** (measured: 24,541 records, 98,200 rows against 49,143)

A full placement goes through `PUT /records` like any other, **writing over each key in turn**. There
is nothing to close.

**`410` rather than `404`, because `404` reads to a sender as "an unknown Worker — press setup".**
An old client that obeys goes on to rebuild a Worker that already has this route, and the loop closes.
**Dropping the route and answering quietly would be worse**: a sender that believes it emptied the
store carries on against a store that is not empty.

## The record routes

| route | |
|---|---|
| `PUT /records` | place the records that moved, deletions included. The answer is `{"seq":…,"rows_written":…,"build":…}` |
| `GET /records?since=<seq>` | what follows that point in the order, in pages (200 per page) |
| `GET /records?since=<seq>&keys=1` | **keys and their last word only** (2,000 per page — see "Reading the keys alone") |
| `GET /meta` | version, where the order stands, last write, where the placement began, **the fingerprint of the sealing key**. **The cheap question to ask before fetching** |
| `PUT /reset` | **gone.** `410` and a sentence (see "There is no door that empties this") |

```json
PUT /records
{"spec_v":1,"version":12345,"records":[
  {"k":"task/2812","op":"put","n":"<nonce>","c":"<ciphertext>"},
  {"k":"task/2799","op":"del"}
]}
```

- **Nothing here is interpreted.** `k` is a string the PC chose and the envelope is bytes that cannot
  be opened; the shape is all that is checked
- **One write carries at most 500 records.** Past that comes `413` and a sentence saying to split it.
  Failing inside the database leaves the sender nothing but "it did not go through"
- **A read page holds 200.** `more` says another follows and `seq` is where that page reached, so a
  phone writes a page and asks for the next — the same loop whether it has been away a minute or a week
- `version` is the ledger's own version. **The same version arriving twice still writes what it
  carries.** A version is far too coarse a name for one send: a store that is standing hands its number
  straight back to a send made while the ledger did not move, so reading "same version, same records"
  and discarding the body unread loses real edits behind a `200`. Telling a resend apart is **the
  sender's** job, being the side that knows what it sent
- `rows_written` in the answer is **what the database actually wrote**, which is what a sender needs to
  budget against a day's limit. `build` is **this Worker's own version** — the sender holds only a write
  token, so a write's answer is where an outdated Worker has to be visible. **No number coming back is
  itself the answer** (a Worker handed out before this field). **The number moves only for a change
  that cannot be fixed without replacing the Worker that stands** — Amenbo carries the same number, and
  a mismatch is what makes the settings screen ask for a press
- `key_fingerprint` is **the fingerprint of the key the records were sealed with** (optional, SHA-256
  in lowercase hex, 64 characters). A different shape is refused with `400`. **The key itself never
  arrives** (see "The fingerprint of the sealing key")

### The fingerprint of the sealing key

`GET /meta` returns `key_fingerprint`: the SHA-256 of **the key the records now in the store were
sealed with**, so a phone can hold it against its own before fetching anything.

**It is needed because there are two reasons a phone cannot open what it reads.** Someone running two
Amenbo installs, one for work and one for their own things, has both **contending for a single server**:
the Worker and the database are named `amenbo-viewer` either way. The one stood up second draws a new
key while the earlier records stay where they are — and "the encryption key differs (the PC's next write
fixes it, so wait)" and "the records are corrupt (nothing fixes it, so reconnect)" reach the phone
wearing the same face. What to do next is opposite, so **the two are separated before the fetch**.

- **Only the hash is carried.** Anyone holding a read token gets this answer, so the key does not go in
- **Only the last part writes it** (as with the version), so a run that was cut short does not leave the
  store claiming a key it does not hold
- **`null` is "nobody has claimed one", not a mismatch.** A store written by a sender older than this
  column has nothing to compare against, so the phone reads as it always has
- **A run that claims none puts it back to `null`.** The fingerprint speaks for the records that are in
  there **now**, so the previous sender's answer is not left standing in for it

### What does not fit in one write is sent in parts

A full placement runs past 500 records on its first pass once a ledger has grown. So the body names
**which part of this run it is** — `{"part":2,"parts":5}`. Both default to `1`, and a body that omits
them goes through as a run that completes in one — the shape a sender that does not split uses.

Every part of one run carries **the same `version`**. On top of that:

**The version and the key fingerprint are written by the last part alone.** A run cut short leaves the
store naming **what it actually holds**, and the next run is not misread as a resend.

**There is nothing to close.** Since a placement no longer empties anything first, what has arrived is
"the correct first half of what is coming" — a phone that reads it **is not wrong, only behind**, which
is the state it is in between any two writes.

### Reading the keys alone

`GET /records?since=0&keys=1` returns `k` and `op` and nothing else. **A page holds 2,000** (a normal
read holds 200).

**"In the store and no longer in the ledger" is not a question this Worker can answer.** It does not
know what a key means, so the sender is the only side that can reconcile the two — and reconciling takes
every key. Fetching them through the normal read brings the envelopes down with them, **paying 27 MB for
an answer that turns on strings** (measured: 24,541 records, 27.12 MB over 123 pages against 0.67 MB over
13 pages).

**`op` comes too, because a deletion is a row here.** A key whose last word is `del` is a headstone
rather than something the store holds. A sender that cannot tell the two apart **sends `del` for every
one of them, every time**, and each of those raises another headstone.

### The order never rewinds

`seq` lives in **`store`'s single row**, not in the maximum of `records`. Counting it off the rows would
renumber the order every time the store were emptied, and a renumbered order points a phone that still
holds an older cursor at a different record, with nothing to notice it by.

So the order climbs for the life of the store. On top of that, **a cursor pointing past where the order
stands is refused with `409`**: nothing can be past the end of an order that never rewinds, so that
cursor was counted against some other store, and the phone is sent back to read from the beginning.

### A cursor a placement overtook is refused

An order that keeps climbing also means **a phone on the far side of a placement sees an ordinary
catch-up**. What it does not see are the records that went away while it was gone: not being alive, they
are not in the placement, and the row that carried word of their deletion went when the store was
emptied. **Neither side holds them**, so that phone keeps a task that is gone, with no way to find out.

So `store` carries **where the current placement began** (`placed_from`), written by the first part of a
reset as a copy of where the order stood just before the emptying. A placement's contents start at the
next number, so **every row a cursor at or below that points to has been rebuilt** — "what you hold is
not a continuation" follows from the definition of a placement rather than from a guess.

**Nothing writes it any more, and reading it does not stop.** A store that was reset stands in a user's
account right now, and the number left in it is still what protects that phone. Reading costs **one more
column on a row already being read**; stopping costs **a phone its ledger**, on a Worker that cannot be
corrected afterwards.

So `GET /records?since=` refuses with `409` unless `since` is past `placed_from`. `since=0` is never
refused — that is the shape of a phone that dropped what it held and came back to the beginning.

**The refusal is here so that nothing rests on the caller behaving.** `GET /meta` returns `placed_from`,
so a phone can find out before it asks; answering a phone that does not ask lets the mistake come back
silently. Standing the token check ahead of routing rests on the same reasoning.

`wrangler.jsonc` names no `database_id` because this Worker goes into the user's account. The deploy
creates the database on the spot, and Amenbo fills the same slot over the API.

## Running it

```
npm ci
npm run typegen    # generate Env's types from wrangler.jsonc
npm test           # run inside workerd; the tests apply the migrations themselves
npm run check      # typegen -> tsc -> deploy --dry-run
npm run dev        # local; copy .dev.vars.example to .dev.vars
```

A change to a table is a new file under `migrations/`. The tests apply **the same migrations a deploy
does**, so a migration that does not run fails here rather than in somebody's account.

**What holds the record of having applied one is the database itself** — a row in `d1_migrations` says
"this migration was applied", and the table and the names are wrangler's. `wrangler d1 migrations apply`,
the tests' `applyD1Migrations` and Amenbo's setup all write **the same names** (the filenames as they
stand) into **the same table**. A ledger of our own could be correct on both sides and still disagree
with it.

`npm run deploy` is for doing it by hand. It sits where the root Makefile cannot reach it.

## Amenbo bakes it in and carries it

What stands the Worker up in the user's account is Amenbo, and the user's PC has no Node on it. So
**what is built here goes inside the Amenbo binary**: `make -C worker baked` writes
`crates/amenbo-core/src/viewer/worker.js` (what esbuild emits) and
`crates/amenbo-core/src/viewer/migrations/` (a copy of `migrations/`), and core embeds both at compile
time. `make worker-build` and `make worker-test` bake first, so **an edit under `src/` or `migrations/`
that the copy does not follow leaves a dirty working tree** rather than an account quietly getting the
Worker from an older commit.

**The migrations are copied one file at a time, under the names they have here.** Those names are what a
database records having had, so joining them into one file leaves nothing able to say how far it got.

**Both copies are generated. Do not edit them by hand.**
