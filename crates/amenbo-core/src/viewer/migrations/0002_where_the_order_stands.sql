-- Where the ordering has got to, kept on the store row rather than read off the records.
--
-- `MAX(seq)` looks like the same number, and is — right up until a reset. A reset empties the
-- records and places the whole store again, so the rows that held the high point are gone and the
-- numbering would start over, underneath every phone still holding a cursor from before it. Those
-- phones would ask to read on from a point that now names somebody else's row, and be handed the
-- wrong half of the store with nothing to notice by.
--
-- A cursor only means anything if the order never rewinds. So the number is kept where emptying
-- the records does not reach, and a reset carries on from it.
ALTER TABLE store ADD COLUMN seq INTEGER NOT NULL DEFAULT 0;

-- Whatever the rows had reached before this migration is where the order stands.
UPDATE store SET seq = (SELECT COALESCE(MAX(seq), 0) FROM records) WHERE id = 1;
