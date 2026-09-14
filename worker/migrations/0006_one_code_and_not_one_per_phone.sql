-- One read code, where there was a row per phone.
--
-- The table held a label as its key, because cutting off one phone meant naming it. But this
-- Worker compares a hash and never learns which phone offered it: one code photographed by three
-- phones is one row here, and all three read. So the label named something on the PC and nothing
-- in the store, and "the phones that may read" was a list of what the PC believed.
--
-- What the store can honestly hold is a code, present or absent. Issuing replaces it, deleting
-- takes it away, and either way whoever held the old one stops reading.
--
-- **The newest row survives, and the rest do not.** A store with several was several phones
-- reading, and every one of them but the last-issued is cut here — which is the same thing that
-- happens from now on the next time a code is issued. Keeping the newest is what leaves the phone
-- somebody paired most recently still reading; emptying the table would cut that one too, for
-- nothing.
CREATE TABLE tokens_one (
  id        INTEGER NOT NULL PRIMARY KEY CHECK (id = 1),
  hash      TEXT NOT NULL,
  issued_at TEXT NOT NULL
);

-- `issued_at` is an ISO instant, so ordering it as text is ordering it in time. The rowid breaks a
-- tie, which two codes issued in the same millisecond would otherwise leave to the table's whim.
INSERT INTO tokens_one (id, hash, issued_at)
SELECT 1, hash, issued_at FROM tokens ORDER BY issued_at DESC, rowid DESC LIMIT 1;

DROP TABLE tokens;
ALTER TABLE tokens_one RENAME TO tokens;
