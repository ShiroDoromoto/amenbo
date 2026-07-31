CREATE TABLE IF NOT EXISTS project (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    color TEXT,
    default_view TEXT NOT NULL DEFAULT '' CHECK(default_view IN ('', 'list', 'board', 'calendar', 'timeline')),
    archived BOOLEAN NOT NULL DEFAULT 0 CHECK(archived IN (0, 1)),
    order_key TEXT NOT NULL DEFAULT '',
    slug TEXT,
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS task (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    subtype TEXT NOT NULL DEFAULT '' CHECK(subtype IN ('', 'default', 'milestone')),
    completed_at TEXT CHECK(completed_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    status TEXT NOT NULL DEFAULT '' CHECK(status IN ('', 'todo', 'in_progress', 'done', 'blocked', 'rejected')),
    status_changed_at TEXT CHECK(status_changed_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    created_by_kind TEXT CHECK(created_by_kind IN ('human', 'ai')),
    assignee_kind TEXT CHECK(assignee_kind IN ('human', 'ai')),
    start_on TEXT CHECK(start_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    due_on TEXT CHECK(due_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    priority TEXT CHECK(priority IN ('high', 'medium', 'low')),
    project_id BIGINT REFERENCES project(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    order_key TEXT,
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS decision (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    title TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT '' CHECK(status IN ('', 'proposed', 'accepted', 'rejected')),
    status_changed_at TEXT CHECK(status_changed_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    decided_at TEXT CHECK(decided_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    decided_by TEXT,
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS decision_edge (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    decision_id BIGINT NOT NULL DEFAULT 0 REFERENCES decision(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    target_decision_id BIGINT NOT NULL DEFAULT 0 REFERENCES decision(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    kind TEXT NOT NULL DEFAULT '' CHECK(kind IN ('', 'supersedes', 'amends', 'builds_on')),
    drawn_at TEXT CHECK(drawn_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS decision_task_link (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    decision_id BIGINT NOT NULL DEFAULT 0 REFERENCES decision(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    task_id BIGINT NOT NULL DEFAULT 0 REFERENCES task(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    linked_at TEXT CHECK(linked_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS task_comment (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    task_id BIGINT NOT NULL DEFAULT 0 REFERENCES task(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    author_kind TEXT CHECK(author_kind IN ('human', 'ai')),
    text TEXT NOT NULL DEFAULT '',
    edited_at TEXT CHECK(edited_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS decision_comment (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    decision_id BIGINT NOT NULL DEFAULT 0 REFERENCES decision(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    author_kind TEXT CHECK(author_kind IN ('human', 'ai')),
    text TEXT NOT NULL DEFAULT '',
    edited_at TEXT CHECK(edited_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS task_dependency (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    task_id BIGINT NOT NULL DEFAULT 0 REFERENCES task(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    blocked_by_id BIGINT NOT NULL DEFAULT 0 REFERENCES task(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    created_by_kind TEXT CHECK(created_by_kind IN ('human', 'ai')),
    established_at TEXT CHECK(established_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS task_commit (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    task_id BIGINT NOT NULL DEFAULT 0 REFERENCES task(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    sha TEXT NOT NULL DEFAULT '',
    created_by_kind TEXT CHECK(created_by_kind IN ('human', 'ai')),
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS dimension (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    cardinality TEXT NOT NULL DEFAULT '' CHECK(cardinality IN ('', 'single')),
    ordered BOOLEAN NOT NULL DEFAULT 0 CHECK(ordered IN (0, 1)),
    role TEXT NOT NULL DEFAULT '' CHECK(role IN ('', 'none', 'time_axis')),
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS dimension_value (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    dimension_id BIGINT NOT NULL DEFAULT 0 REFERENCES dimension(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL DEFAULT '',
    order_key TEXT NOT NULL DEFAULT '',
    start_on TEXT CHECK(start_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    end_on TEXT CHECK(end_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS task_dimension_value (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    task_id BIGINT NOT NULL DEFAULT 0 REFERENCES task(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    dimension_id BIGINT NOT NULL DEFAULT 0 REFERENCES dimension(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    value_id BIGINT NOT NULL DEFAULT 0 REFERENCES dimension_value(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS attachment (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    target_type TEXT NOT NULL DEFAULT '' CHECK(target_type IN ('', 'task', 'decision', 'task_comment', 'decision_comment')),
    target_id BIGINT NOT NULL DEFAULT 0,
    kind TEXT NOT NULL DEFAULT '' CHECK(kind IN ('', 'blob', 'url')),
    blob_hash TEXT CHECK(length(blob_hash) = 64 AND NOT blob_hash GLOB '*[^0-9a-f]*'),
    filename TEXT,
    mime TEXT,
    size_bytes BIGINT,
    url TEXT,
    created_by_kind TEXT CHECK(created_by_kind IN ('human', 'ai')),
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS plugin_config (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    plugin TEXT NOT NULL DEFAULT '',
    field_key TEXT NOT NULL DEFAULT '',
    value TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS plugin_secret (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    plugin TEXT NOT NULL DEFAULT '',
    field_key TEXT NOT NULL DEFAULT '',
    value TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS plugin_enable (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    plugin TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS store_meta (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT
);
CREATE TABLE IF NOT EXISTS change_feed (
    id      INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    dataset TEXT NOT NULL,
    row_id  BIGINT NOT NULL,
    op      TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS plugin_outbox (
    id        INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    event     TEXT NOT NULL,
    record_id BIGINT NOT NULL,
    actor     TEXT NOT NULL,
    at        TEXT NOT NULL,
    new_state TEXT,
    project   BIGINT,
    record    TEXT,
    parent    BIGINT
);
CREATE TABLE IF NOT EXISTS plugin_queue (
    id        INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    plugin    TEXT NOT NULL,
    face      TEXT NOT NULL,
    event     TEXT NOT NULL,
    record_id BIGINT NOT NULL,
    actor     TEXT NOT NULL,
    at        TEXT NOT NULL,
    new_state TEXT,
    project   BIGINT,
    record    TEXT,
    parent    BIGINT
);
CREATE TABLE IF NOT EXISTS plugin_runner (
    plugin     TEXT PRIMARY KEY NOT NULL,
    owner      TEXT NOT NULL,
    expires_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS search_doc (
    id         INTEGER PRIMARY KEY NOT NULL,
    owner_kind TEXT NOT NULL,
    owner_id   BIGINT NOT NULL,
    field      TEXT NOT NULL,
    norm       TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS binding_path (
    project_id INTEGER PRIMARY KEY NOT NULL,
    dir        TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS binding_project_dir (
    project_id BIGINT NOT NULL,
    dir        TEXT NOT NULL,
    PRIMARY KEY (project_id, dir)
);
CREATE TABLE IF NOT EXISTS read_receipt (
    task_id   INTEGER PRIMARY KEY NOT NULL,
    last_seen TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS inbox_archive (
    task_id INTEGER PRIMARY KEY NOT NULL
);
CREATE TABLE IF NOT EXISTS mailbox_notified (
    task_id INTEGER PRIMARY KEY NOT NULL
);
CREATE TABLE IF NOT EXISTS hook_optout (
    project_id INTEGER PRIMARY KEY REFERENCES project(id) ON DELETE CASCADE NOT NULL
);
CREATE TABLE IF NOT EXISTS harness_consent (
    project_id  INTEGER PRIMARY KEY REFERENCES project(id) ON DELETE CASCADE NOT NULL,
    allowed     INTEGER NOT NULL,
    asked_again INTEGER NOT NULL
);

PRAGMA journal_mode = WAL;

-- Foreign-key indexes for the read layer's correlated subqueries over child tables
-- (`read::list_task_ids`: the project placement EXISTS + the `order` sort's per-row placement
-- lookup and the ready/blocked dependency EXISTS). Without them every such subquery table-scans the
-- whole child table once per task — O(tasks × child), i.e. O(N²) on an unfiltered board page, which
-- is seconds rather than milliseconds on a board of ten thousand tasks.
CREATE INDEX IF NOT EXISTS task_dependency_by_task   ON task_dependency(task_id);
CREATE INDEX IF NOT EXISTS task_dependency_by_blocker ON task_dependency(blocked_by_id);
-- A word filter spans comment bodies: `read::task_text_term` reaches a task's comments through this
-- index on its way to their copies in `search_doc`. Without it that subquery scans every comment once
-- per task; keyed by `task_id` it seeks only the task's own comments (O(result)).
CREATE INDEX IF NOT EXISTS task_comment_by_task       ON task_comment(task_id);
-- A decision's `comment list` seeks its own comments by `decision_id` (mirrors `task_comment_by_task`),
-- so the read stays O(result) instead of scanning every decision comment.
CREATE INDEX IF NOT EXISTS decision_comment_by_decision ON decision_comment(decision_id);
-- The decision→decision edges, read in both directions. Forward (what this decision supersedes/amends)
-- seeks by `decision_id`; reverse (who superseded/amended it — a derived view, never a stored flag)
-- seeks by `target_decision_id`.
-- The forward index is UNIQUE over the pair, so one decision cannot hold two edges to the same target:
-- `supersedes` (the target is historicised) and `amends` (the target stays current) contradict. Dropping
-- an edge deletes the row, so it leaves the index and the pair can be drawn again.
CREATE UNIQUE INDEX IF NOT EXISTS decision_edge_pair ON decision_edge(decision_id, target_decision_id);
CREATE INDEX IF NOT EXISTS decision_edge_by_target ON decision_edge(target_decision_id);
-- FK index over the task↔value link of the dimension model, so an axis filter's EXISTS seeks a task's
-- own assignments instead of scanning the whole link table (the convention every child table follows:
-- task_dependency_by_task etc.). No read path consumes it yet; it is here so that moving the axes onto
-- the link table cannot reintroduce the O(N²) scan the other FK indexes exist to prevent.
CREATE INDEX IF NOT EXISTS task_dimension_value_by_task ON task_dimension_value(task_id);
-- A task's commit SHAs. The pair is UNIQUE so the same commit cannot be recorded twice on one task
-- (the ops layer reads this to stay idempotent, and the door normalises case so two spellings of one
-- SHA cannot slip past it). The `by_sha` index is the reverse chain (SHA → tasks) the later filter
-- seeks by; without it that lookup scans every row. `by_task` is the FK index every child table keeps,
-- so a task's own commits (and the ready/detail subqueries) seek instead of scanning the whole table.
CREATE UNIQUE INDEX IF NOT EXISTS task_commit_task_sha ON task_commit(task_id, sha);
CREATE INDEX IF NOT EXISTS task_commit_by_sha  ON task_commit(sha);
CREATE INDEX IF NOT EXISTS task_commit_by_task ON task_commit(task_id);
-- One plugin config value per (project, plugin, field): the triple is the natural key, so the write
-- boundary upserts a value by finding this row rather than appending a second. The unique index *is* that
-- constraint (the column decls carry no UNIQUE), and its leading columns also serve the reads that seek a
-- project's values — by `project_id`, or by `(project_id, plugin)` for one plugin's whole config.
CREATE UNIQUE INDEX IF NOT EXISTS plugin_config_triple ON plugin_config(project_id, plugin, field_key);
-- The secret half, keyed the same way and for the same reason (`AMB-D-434`).
CREATE UNIQUE INDEX IF NOT EXISTS plugin_secret_triple ON plugin_secret(project_id, plugin, field_key);
-- One enable override per (project, plugin): the pair is the natural key, so the trust boundary upserts the
-- gate by finding this row rather than appending a second answer. The unique index *is* that constraint,
-- and its leading column also serves a read that seeks one project's overrides.
CREATE UNIQUE INDEX IF NOT EXISTS plugin_enable_pair ON plugin_enable(project_id, plugin);
-- A plugin's queue, read the only way it is ever read: that plugin's own rows, oldest first. The pair is
-- the whole query (`plugin` seeks, `id` orders), so the runner reads its head without scanning the rows
-- queued for every other plugin, and the fan-out's "which plugins have work" seek stays on the index too.
CREATE INDEX IF NOT EXISTS plugin_queue_by_plugin ON plugin_queue(plugin, id);
-- The read layer's own two seeks over the task table: `status` narrows a mailbox query, and
-- `project_id` — placement is folded onto the task — scopes every list to one project.
CREATE INDEX IF NOT EXISTS task_by_status    ON task(status);
CREATE INDEX IF NOT EXISTS task_by_project   ON task(project_id);
-- The project slug's uniqueness. This index *is* the constraint — the column decl cannot carry
-- `UNIQUE` (see `SLUG`). NULLs are distinct in SQLite, so rows without a slug coexist.
CREATE UNIQUE INDEX IF NOT EXISTS project_by_slug ON project(slug);
-- One copy per text face. The triple is the natural key, so a field write upserts the face it already
-- has rather than appending a second, and its leading columns serve the read that seeks one record's
-- own faces (`owner_kind`, `owner_id`) — which is how a word filter stays O(result) instead of walking
-- the whole index once per candidate row.
CREATE UNIQUE INDEX IF NOT EXISTS search_doc_face ON search_doc(owner_kind, owner_id, field);
-- What hangs off a record, seekable from the record. `attachment` is polymorphic, so the pair is what a
-- lookup has (`read::attachment_term`, and the delete op's own sweep); without the index each such
-- lookup scans every attachment in the store once per candidate row.
CREATE INDEX IF NOT EXISTS attachment_by_target ON attachment(target_type, target_id);
-- The trigram index over that copy, and the three triggers that keep it in step. External-content
-- (`content='search_doc'`), so the text is stored once: the index holds only the trigrams, and the row
-- it points at is the copy itself. The triggers are the seam — a doc row cannot be written without its
-- trigrams following, whatever code wrote it — and they are pure SQL because the folding
-- (`store_engine::search::normalize`) has already happened by the time a row reaches this table.
CREATE VIRTUAL TABLE IF NOT EXISTS search_fts
    USING fts5(norm, content = 'search_doc', content_rowid = 'id', tokenize = 'trigram');
CREATE TRIGGER IF NOT EXISTS search_doc_insert AFTER INSERT ON search_doc BEGIN
    INSERT INTO search_fts(rowid, norm) VALUES (new.id, new.norm);
END;
CREATE TRIGGER IF NOT EXISTS search_doc_delete AFTER DELETE ON search_doc BEGIN
    INSERT INTO search_fts(search_fts, rowid, norm) VALUES ('delete', old.id, old.norm);
END;
CREATE TRIGGER IF NOT EXISTS search_doc_update AFTER UPDATE ON search_doc BEGIN
    INSERT INTO search_fts(search_fts, rowid, norm) VALUES ('delete', old.id, old.norm);
    INSERT INTO search_fts(rowid, norm) VALUES (new.id, new.norm);
END;
