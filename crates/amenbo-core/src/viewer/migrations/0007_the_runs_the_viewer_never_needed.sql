-- The rows of automation runs that reached this store before the PC stopped sending them.
--
-- The PC no longer sends them, and a repair only turns the ones here into deletions — rows that
-- still sit in the store and still go to every phone that reads it from the start. This takes
-- them out once, `put` and `del` alike.
--
-- **This is a one-off clean-up, not Amenbo's shape coming into the schema.** It is the one place
-- that reads a key for what it means, and it does so only to clear what was sent before the PC
-- knew better; nothing that writes or reads the store from now on looks at the key.
--
-- The key is `<dataset>/<id>`, and the dataset is compared whole rather than with LIKE: `_` is a
-- wildcard there, and a prefix match would also take `automation/…` or `automation_runX…`.
--
-- **Where the order stands is left alone.** A phone's cursor only means anything if the order
-- never rewinds, and `store.seq` is what keeps it from rewinding when the newest rows go.
DELETE FROM records
WHERE substr(k, 1, instr(k, '/') - 1) IN (
  'automation_run',
  'automation_run_def',
  'automation_run_pass',
  'automation_run_step',
  'automation_run_task',
  'automation_run_value'
);
