CREATE TABLE IF NOT EXISTS project (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    color TEXT,
    icon TEXT,
    icon_source TEXT,
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
    draft BOOLEAN NOT NULL DEFAULT 0 CHECK(draft IN (0, 1)),
    created_by_kind TEXT CHECK(created_by_kind IN ('human', 'ai')),
    assignee_kind TEXT CHECK(assignee_kind IN ('human', 'ai')),
    start_on TEXT CHECK(start_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    due_on TEXT CHECK(due_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    priority TEXT CHECK(priority IN ('high', 'medium', 'low')),
    project_id BIGINT REFERENCES project(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    order_key TEXT,
    at_binding_id BIGINT,
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS decision (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    title TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT '' CHECK(status IN ('', 'decided', 'rejected')),
    draft BOOLEAN NOT NULL DEFAULT 0 CHECK(draft IN (0, 1)),
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
    posted_at TEXT CHECK(posted_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    edited_at TEXT CHECK(edited_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    automation_run_step_id BIGINT REFERENCES automation_run_step(id) ON DELETE SET NULL ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
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
CREATE TABLE IF NOT EXISTS task_made_in (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    task_id BIGINT NOT NULL DEFAULT 0 REFERENCES task(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    pane TEXT NOT NULL DEFAULT '',
    pane_name TEXT,
    pane_resume TEXT,
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS decision_made_in (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    decision_id BIGINT NOT NULL DEFAULT 0 REFERENCES decision(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    pane TEXT NOT NULL DEFAULT '',
    pane_name TEXT,
    pane_resume TEXT,
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS dimension (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    cardinality TEXT NOT NULL DEFAULT '' CHECK(cardinality IN ('', 'single', 'multi')),
    ordered BOOLEAN NOT NULL DEFAULT 0 CHECK(ordered IN (0, 1)),
    role TEXT NOT NULL DEFAULT '' CHECK(role IN ('', 'none', 'time_axis', 'closable')),
    show_on_card BOOLEAN NOT NULL DEFAULT 0 CHECK(show_on_card IN (0, 1)),
    required BOOLEAN NOT NULL DEFAULT 0 CHECK(required IN (0, 1)),
    applies_to TEXT NOT NULL DEFAULT '' CHECK(applies_to IN ('', 'task', 'decision', 'both')),
    order_key TEXT NOT NULL DEFAULT '',
    slug TEXT,
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    UNIQUE (project_id, slug)
);
CREATE TABLE IF NOT EXISTS dimension_value (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    dimension_id BIGINT NOT NULL DEFAULT 0 REFERENCES dimension(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL DEFAULT '',
    order_key TEXT NOT NULL DEFAULT '',
    slug TEXT,
    start_on TEXT CHECK(start_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    end_on TEXT CHECK(end_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    closed BOOLEAN NOT NULL DEFAULT 0 CHECK(closed IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    UNIQUE (dimension_id, slug)
);
CREATE TABLE IF NOT EXISTS task_dimension_value (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    task_id BIGINT NOT NULL DEFAULT 0 REFERENCES task(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    dimension_id BIGINT NOT NULL DEFAULT 0 REFERENCES dimension(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    value_id BIGINT NOT NULL DEFAULT 0 REFERENCES dimension_value(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS decision_dimension_value (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    decision_id BIGINT NOT NULL DEFAULT 0 REFERENCES decision(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    dimension_id BIGINT NOT NULL DEFAULT 0 REFERENCES dimension(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    value_id BIGINT NOT NULL DEFAULT 0 REFERENCES dimension_value(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS attachment (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    target_type TEXT NOT NULL DEFAULT '' CHECK(target_type IN ('', 'task', 'decision', 'task_comment', 'decision_comment', 'automation_run_step')),
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
CREATE TABLE IF NOT EXISTS secret (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT REFERENCES project(id) ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    area TEXT NOT NULL DEFAULT '' CHECK(area IN ('', 'notify', 'viewer')),
    owner_id BIGINT,
    field_key TEXT NOT NULL DEFAULT '',
    value TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS notify_target (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    kind TEXT NOT NULL DEFAULT '' CHECK(kind IN ('', 'slack', 'mail')),
    name TEXT NOT NULL DEFAULT '',
    is_default BOOLEAN NOT NULL DEFAULT 0 CHECK(is_default IN (0, 1)),
    smtp_host TEXT,
    smtp_port BIGINT,
    smtp_user TEXT,
    mail_from TEXT,
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS project_notify (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    enabled BOOLEAN NOT NULL DEFAULT 0 CHECK(enabled IN (0, 1)),
    mail_to TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    UNIQUE (project_id)
);
CREATE TABLE IF NOT EXISTS project_notify_target (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    target_id BIGINT NOT NULL DEFAULT 0 REFERENCES notify_target(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    UNIQUE (project_id, target_id)
);
CREATE TABLE IF NOT EXISTS project_notify_event (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) ON DELETE CASCADE ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    event TEXT NOT NULL DEFAULT '' CHECK(event IN ('', 'task.created', 'task.status_changed', 'task.done', 'task.rejected', 'task.assigned', 'task.moved', 'task.deleted', 'decision.accepted', 'decision.rejected', 'comment.added', 'comment.removed', 'task.due', 'task.due_tomorrow')),
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    UNIQUE (project_id, event)
);
CREATE TABLE IF NOT EXISTS automation_action (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT REFERENCES project(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL DEFAULT '',
    note TEXT NOT NULL DEFAULT '',
    entry_step_id BIGINT REFERENCES automation_action_step(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    builtin TEXT,
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    entry_placement_id BIGINT REFERENCES automation_placement(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    archived BOOLEAN NOT NULL DEFAULT 0 CHECK(archived IN (0, 1)),
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_placement (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    automation_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    action_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_action(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_action_step (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    action_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_action(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL DEFAULT '',
    prompt TEXT NOT NULL DEFAULT '',
    builtin TEXT,
    interactive BOOLEAN NOT NULL DEFAULT 0 CHECK(interactive IN (0, 1)),
    work_dir_ref TEXT,
    report_to_task BOOLEAN NOT NULL DEFAULT 0 CHECK(report_to_task IN (0, 1)),
    show_history BOOLEAN NOT NULL DEFAULT 0 CHECK(show_history IN (0, 1)),
    show_task BOOLEAN NOT NULL DEFAULT 0 CHECK(show_task IN (0, 1)),
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_placement_step (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    placement_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_placement(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    step_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_action_step(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    agent TEXT NOT NULL DEFAULT '',
    model TEXT,
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    UNIQUE (placement_id, step_id)
);
CREATE TABLE IF NOT EXISTS automation_cfg (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT '' CHECK(owner_kind IN ('', 'action', 'placement')),
    owner_id BIGINT NOT NULL DEFAULT 0,
    name TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL DEFAULT '' CHECK(kind IN ('', 'taskfilter', 'folder', 'choice', 'number', 'text')),
    required BOOLEAN NOT NULL DEFAULT 0 CHECK(required IN (0, 1)),
    options TEXT,
    value TEXT,
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_exit (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT '' CHECK(owner_kind IN ('', 'step', 'action')),
    owner_id BIGINT NOT NULL DEFAULT 0,
    name TEXT,
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_port (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT '' CHECK(owner_kind IN ('', 'step', 'action', 'exit')),
    owner_id BIGINT NOT NULL DEFAULT 0,
    direction TEXT NOT NULL DEFAULT '' CHECK(direction IN ('', 'in', 'out')),
    name TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL DEFAULT '' CHECK(kind IN ('', 'value', 'file', 'task_take', 'task_make')),
    required BOOLEAN NOT NULL DEFAULT 0 CHECK(required IN (0, 1)),
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_edge (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT '' CHECK(owner_kind IN ('', 'automation', 'action')),
    owner_id BIGINT NOT NULL DEFAULT 0,
    from_id BIGINT NOT NULL DEFAULT 0,
    exit_id BIGINT NOT NULL DEFAULT 0,
    to_id BIGINT,
    ends TEXT NOT NULL DEFAULT '' CHECK(ends IN ('', 'go', 'done', 'halt', 'exit')),
    exit_to_id BIGINT,
    max_times BIGINT,
    order_key TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_wire (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    owner_kind TEXT NOT NULL DEFAULT '' CHECK(owner_kind IN ('', 'automation', 'action')),
    owner_id BIGINT NOT NULL DEFAULT 0,
    from_id BIGINT NOT NULL DEFAULT 0,
    from_exit_id BIGINT,
    from_port_id BIGINT NOT NULL DEFAULT 0,
    to_id BIGINT NOT NULL DEFAULT 0,
    to_port_id BIGINT NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_run (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    automation_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    project_id BIGINT NOT NULL DEFAULT 0 REFERENCES project(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    status TEXT NOT NULL DEFAULT '' CHECK(status IN ('', 'running', 'paused', 'completed', 'failed', 'canceled')),
    pause_requested BOOLEAN NOT NULL DEFAULT 0 CHECK(pause_requested IN (0, 1)),
    stopped_reason TEXT CHECK(stopped_reason IN ('crashed', 'max_times', 'no_agent', 'no_input', 'no_way_on', 'halted', 'left_task_open')),
    started_by_kind TEXT CHECK(started_by_kind IN ('human', 'ai')),
    started_at TEXT CHECK(started_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    ended_at TEXT CHECK(ended_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    acknowledged_at TEXT CHECK(acknowledged_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_run_def (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    run_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_run(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    placement_id BIGINT REFERENCES automation_placement(id) ON DELETE SET NULL ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    step_id BIGINT REFERENCES automation_action_step(id) ON DELETE SET NULL ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL DEFAULT '',
    prompt TEXT,
    builtin TEXT,
    agent TEXT NOT NULL DEFAULT '',
    model TEXT,
    interactive BOOLEAN NOT NULL DEFAULT 0 CHECK(interactive IN (0, 1)),
    work_dir_ref TEXT,
    report_to_task BOOLEAN NOT NULL DEFAULT 0 CHECK(report_to_task IN (0, 1)),
    show_history BOOLEAN NOT NULL DEFAULT 0 CHECK(show_history IN (0, 1)),
    show_task BOOLEAN NOT NULL DEFAULT 0 CHECK(show_task IN (0, 1)),
    exits TEXT NOT NULL DEFAULT '',
    ins TEXT NOT NULL DEFAULT '',
    cfg TEXT NOT NULL DEFAULT '',
    entry BOOLEAN NOT NULL DEFAULT 0 CHECK(entry IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_run_task (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    run_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_run(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    seq BIGINT NOT NULL DEFAULT 0,
    task_id BIGINT REFERENCES task(id) ON DELETE SET NULL ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    started_at TEXT CHECK(started_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    ended_at TEXT CHECK(ended_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_run_step (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    run_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_run(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    run_def_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_run_def(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    run_task_id BIGINT REFERENCES automation_run_task(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    seq BIGINT NOT NULL DEFAULT 0,
    exit_id BIGINT,
    report TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT '' CHECK(status IN ('', 'running', 'done', 'failed', 'stopped')),
    started_at TEXT CHECK(started_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    ended_at TEXT CHECK(ended_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    created_at TEXT NOT NULL DEFAULT '' CHECK(created_at = '' OR created_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z'),
    updated_at TEXT NOT NULL DEFAULT '' CHECK(updated_at = '' OR updated_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]T[0-9][0-9]:[0-9][0-9]:[0-9][0-9]Z')
);
CREATE TABLE IF NOT EXISTS automation_run_value (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    run_step_id BIGINT NOT NULL DEFAULT 0 REFERENCES automation_run_step(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    direction TEXT NOT NULL DEFAULT '' CHECK(direction IN ('', 'in', 'out')),
    exit_id BIGINT,
    port_id BIGINT NOT NULL DEFAULT 0,
    kind TEXT NOT NULL DEFAULT '' CHECK(kind IN ('', 'value', 'file', 'task_take', 'task_make')),
    value TEXT,
    attachment_id BIGINT REFERENCES attachment(id) ON DELETE SET NULL ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    task_id BIGINT REFERENCES task(id) ON DELETE SET NULL ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
    from_run_step_id BIGINT REFERENCES automation_run_step(id) ON DELETE RESTRICT ON UPDATE CASCADE DEFERRABLE INITIALLY DEFERRED,
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
    op      TEXT NOT NULL,
    project BIGINT
);
CREATE TABLE IF NOT EXISTS project_version (
    project_id INTEGER PRIMARY KEY REFERENCES project(id) ON DELETE CASCADE NOT NULL,
    version    BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS outbox (
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
CREATE TABLE IF NOT EXISTS search_doc (
    id         INTEGER PRIMARY KEY NOT NULL,
    owner_kind TEXT NOT NULL,
    owner_id   BIGINT NOT NULL,
    field      TEXT NOT NULL,
    norm       TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS binding_project_dir (
    id         INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    project_id BIGINT NOT NULL,
    dir        TEXT NOT NULL,
    UNIQUE (project_id, dir)
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
CREATE TABLE IF NOT EXISTS nudge_fired (
    nudge_id TEXT PRIMARY KEY NOT NULL,
    at       TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS viewer_send (
    id             INTEGER PRIMARY KEY CHECK (id = 1) NOT NULL,
    version        BIGINT NOT NULL,
    cursor         BIGINT NOT NULL,
    placed         BIGINT NOT NULL,
    seq            BIGINT NOT NULL,
    quiet_until    TEXT,
    spent          BIGINT NOT NULL,
    spent_on       TEXT,
    build          BIGINT NOT NULL,
    last_placed_at TEXT
);
CREATE TABLE IF NOT EXISTS viewer_pending (
    id         INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    record_key TEXT NOT NULL,
    op         TEXT CHECK(op IN ('put', 'del')) NOT NULL,
    body       TEXT
);
CREATE TABLE IF NOT EXISTS viewer_asked (
    id       INTEGER PRIMARY KEY CHECK (id = 1) NOT NULL,
    asked_at TEXT NOT NULL,
    to_place BIGINT NOT NULL,
    to_drop  BIGINT NOT NULL
);
CREATE TABLE IF NOT EXISTS viewer_switch (
    id      INTEGER PRIMARY KEY CHECK (id = 1) NOT NULL,
    sending INTEGER CHECK(sending IN (0, 1)) NOT NULL
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
-- The decision side's FK index, and there for the same reason: a decision's own assignments are sought
-- rather than found by scanning the whole link table — the sweep its delete op reads, and the axis
-- filter the read layer grows onto it.
CREATE INDEX IF NOT EXISTS decision_dimension_value_by_decision ON decision_dimension_value(decision_id);
-- A task's commit SHAs. The pair is UNIQUE so the same commit cannot be recorded twice on one task
-- (the ops layer reads this to stay idempotent, and the door normalises case so two spellings of one
-- SHA cannot slip past it). The `by_sha` index is the reverse chain (SHA → tasks) the later filter
-- seeks by; without it that lookup scans every row. `by_task` is the FK index every child table keeps,
-- so a task's own commits (and the ready/detail subqueries) seek instead of scanning the whole table.
CREATE UNIQUE INDEX IF NOT EXISTS task_commit_task_sha ON task_commit(task_id, sha);
CREATE INDEX IF NOT EXISTS task_commit_by_sha  ON task_commit(sha);
CREATE INDEX IF NOT EXISTS task_commit_by_task ON task_commit(task_id);
-- The session a task or a decision was made in (`AMB-D-897`). One row an owner, so the owner's key is
-- the natural one and the index is UNIQUE — a second row would be a second answer to a question with
-- one. It doubles as the FK index every child table keeps: the detail screen seeks its owner's row
-- rather than scanning the table.
CREATE UNIQUE INDEX IF NOT EXISTS task_made_in_by_task ON task_made_in(task_id);
CREATE UNIQUE INDEX IF NOT EXISTS decision_made_in_by_decision ON decision_made_in(decision_id);
-- One secret per (layer, area, owner, field): the address is the natural key, so the write boundary
-- upserts a credential by finding this row rather than appending a second. `owner_id` is nullable — the
-- Viewer's keys hang off no row — and SQLite counts NULLs in an index as distinct, which would let a
-- second copy of the same device-wide secret in beside the first; `COALESCE(owner_id, 0)` is what closes
-- that, `0` being a value no row's id ever is. The device layer needs a restatement of its own for the same
-- reason: its `project_id` is NULL, so the general index says nothing about it.
CREATE UNIQUE INDEX IF NOT EXISTS secret_address ON secret(project_id, area, COALESCE(owner_id, 0), field_key);
CREATE UNIQUE INDEX IF NOT EXISTS secret_address_device ON secret(area, COALESCE(owner_id, 0), field_key) WHERE project_id IS NULL;
-- Which projects a notification target carries (`AMB-D-885`), sought from the target. The table's own
-- `UNIQUE (project_id, target_id)` already seeks the other way — this project's targets — but leads on
-- the project, so the count a delete asks for ("two projects use this") would scan every row in the
-- table. The delete is where that count is read, and it is read before anything goes.
CREATE INDEX IF NOT EXISTS project_notify_target_by_target ON project_notify_target(target_id);
-- The read layer's own two seeks over the task table: `status` narrows a mailbox query, and
-- `project_id` — placement is folded onto the task — scopes every list to one project.
CREATE INDEX IF NOT EXISTS task_by_status    ON task(status);
CREATE INDEX IF NOT EXISTS task_by_project   ON task(project_id);
-- The tasks that carry a due day, and only those. Partial, because the question every read of it puts
-- is about dated work — "is anything still owed a warning" (the tick's banner, `AMB-D-718`), and the
-- warning's own windows — so the tasks with no day on them are not a smaller answer, they are no part of
-- the question. On a store where due days are barely used that is an index of a handful of rows instead
-- of one entry per task, and the EXISTS that says "nothing is dated here" is answered off it rather than
-- by reading every task. `due_on` is a genesis column, so this belongs here rather than in a step.
CREATE INDEX IF NOT EXISTS task_by_due ON task(due_on) WHERE due_on IS NOT NULL;
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
-- Which run handled a given task. The run side already seeks its own rows by `run_id`, but the
-- question a task's screen puts is the other way round — "what has been run on this one" — and
-- without this index that lookup scans every row of the table once per task asked about.
CREATE INDEX IF NOT EXISTS automation_run_task_by_task ON automation_run_task(task_id);
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
