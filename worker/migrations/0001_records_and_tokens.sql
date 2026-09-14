-- The whole of what this Worker holds: rows the PC wrote, the read tokens that may fetch them,
-- and where the store as a whole stands.
--
-- **Nothing here knows what a record is.** The key is a string the PC chose and the ciphertext is
-- bytes it sealed; give this schema Amenbo's shape instead and every change to Amenbo becomes a
-- migration in every user's own account, with nobody able to roll them forward.

-- One row per record. The key is the PC's; `seq` is this store's own order, and it is what a
-- phone reads on from.
--
-- A deletion is a row, not a missing one: it keeps its place in the order, so a phone that was
-- away learns the record went rather than never hearing of it again. Its envelope is empty,
-- which is the only thing that tells the two apart in the table.
CREATE TABLE records (
  k          TEXT    NOT NULL PRIMARY KEY,
  seq        INTEGER NOT NULL,
  op         TEXT    NOT NULL CHECK (op IN ('put', 'del')),
  nonce      TEXT,
  ciphertext TEXT
);

-- Reading is always "everything after this point in the order", so that is the one index worth
-- keeping. It is UNIQUE because two rows sharing a place in the order would make a page boundary
-- ambiguous — a phone resuming from there would skip one of them or read it twice.
CREATE UNIQUE INDEX records_by_seq ON records (seq);

-- One row per phone that may read. **The token itself is not here** — only the hash of it, since
-- this Worker only ever compares. Keeping the value would put every paired phone's credential in
-- one place, to be taken all at once.
--
-- The label is the phone's name and the key: revoking is deleting the row, and a name that could
-- repeat would revoke the wrong one.
CREATE TABLE tokens (
  label     TEXT NOT NULL PRIMARY KEY,
  hash      TEXT NOT NULL,
  issued_at TEXT NOT NULL
);

-- Where the store as a whole stands: the version of the PC's backlog the rows came from, and
-- when they last moved. One row, ever — the CHECK is what says so, rather than a convention
-- somebody has to remember.
--
-- The version is compared for inequality only. A backlog can be restored to an earlier state and
-- brings its version back with it, so "older" is not a thing this can ask.
CREATE TABLE store (
  id         INTEGER NOT NULL PRIMARY KEY CHECK (id = 1),
  version    INTEGER,
  updated_at TEXT
);

INSERT INTO store (id, version, updated_at) VALUES (1, NULL, NULL);
