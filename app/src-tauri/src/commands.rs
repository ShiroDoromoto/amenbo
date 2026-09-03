//! GUI ↔ core wiring. Every command opens the store, reads or writes, and drops it right away
//! (open-per-action). The lock is held for an instant only, so the CLI can touch the same store
//! concurrently. The shapes the answers take are declared in [`crate::dto`]; the shaping into them
//! — reading the store and filling one in — is here, in the command layer, beside the wiring that
//! needs it.

use crate::dto::*;
use crate::error::CmdError;
use amenbo_core::model::{ActorKind, DimensionCardinality, DimensionRole, Priority, TaskStatus};
use amenbo_core::time::Timestamp;
use amenbo_core::{query, Store};
use chrono::NaiveDate;

/// Fail with the reason if the startup migration ([`crate::migrate::run`]) is **still running** or
/// has **failed**. A store mid-migration sits between versions, and a failed migration is rolled
/// back whole — the store is intact but stuck at the old version, and nobody knows what this build
/// would do if it read that. So every path that opens the store comes through here and shows the
/// reason instead of the data — **a new command that opens the store must go through here**
/// (automatic via `open_store` / `open_store_read` / `with_store_read`).
fn ensure_migrated() -> Result<(), CmdError> {
    crate::migrate::gate()
}

/// Open the store for writing. There is exactly one store, so the target is always `resolve()`
/// (**directory-independent** — the GUI process has no `.amenbo` of its own).
///
/// The one long-lived connection this process holds is let go of first when it has been orphaned
/// ([`release_orphaned_watch`]) — held on to, it would fail this write and every write after it.
fn open_store() -> Result<Store, CmdError> {
    ensure_migrated()?;
    release_orphaned_watch();
    Store::open_at(amenbo_core::config::Paths::resolve()?).map_err(CmdError::from)
}

/// Lightweight read-only open: the same store as `open_store`, opened through the persistent
/// engine's back-projection instead of paying for a full hydrate (`Store::open_read_at`).
/// **Read commands only** — never call a write on the `Store` it hands back. Falls back to a full
/// open internally if the engine has not been primed yet.
fn open_store_read() -> Result<Store, CmdError> {
    ensure_migrated()?;
    Store::open_read_at(amenbo_core::config::Paths::resolve()?).map_err(CmdError::from)
}

/// Write side. Opens the store and hands out `&mut Store` to mutate (the write wrappers commit
/// per operation).
/// **Projection (build_snapshot) is done separately, after the lock is released** — it reopens the
/// same store, so projecting in here would collide re-entrantly with our own lock.
///
/// This is also the GUI's dispatch seam (`AMB-D-367`): every mutating command comes through here, so the
/// observation dispatcher is driven here — once, after the mutation committed, on the store that is still
/// open ([`crate::plugin_dispatch`]). It drains from the store's own cursor, shared with the CLI
/// (`AMB-D-380`), so there is nothing to start first. A command that errored rolled its mutation back and
/// has nothing to dispatch.
fn with_store_mut<T>(f: impl FnOnce(&mut Store) -> Result<T, CmdError>) -> Result<T, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("store.write");
    let mut store = open_store()?;
    let out = f(&mut store);
    if out.is_ok() {
        crate::plugin_dispatch::drive(&store);
    }
    drop(store);
    out
}

/// The read entry point. If there is no store yet (store file not created), nothing is opened and
/// `f` is never called — the GUI draws an empty state (we do not silently genesis one). Lightweight
/// read-only open (`Store::open_read_at`).
fn with_store_read(f: impl FnOnce(&Store) -> Result<(), CmdError>) -> Result<(), CmdError> {
    ensure_migrated()?;
    let paths = amenbo_core::config::Paths::resolve()?;
    if amenbo_core::env::home().is_some() || paths.store_file.exists() {
        let store = Store::open_read_at(paths)?;
        f(&store)?;
    }
    Ok(())
}

/// Value-returning flavour of [`with_store_read`]: `None` when there is no store.
fn find_in_store<T>(
    f: impl FnOnce(&Store) -> Result<Option<T>, CmdError>,
) -> Result<Option<T>, CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    if amenbo_core::env::home().is_some() || paths.store_file.exists() {
        let store = Store::open_read_at(paths)?;
        return f(&store);
    }
    Ok(None)
}

/// Emit a system event into the file ledger (same shape as the CLI's `emit_event`). The GUI actor
/// is always human. Call it **after** the mutation wrapper has committed. Activity is not a system
/// of record, so a failed row write must not fail the operation — warn, carry on, and err on the
/// side of a missing line.
///
/// The warning goes to the diagnostic log (`AMB-D-382`), which is the one a person can be asked for.
/// `tracing` is the perf subscriber's, and it takes `target="perf"` only and is off by default — a
/// missing line reported there is a missing line reported nowhere.
fn emit(store: &mut Store, target_id: i64, event: serde_json::Value) {
    if let Err(e) = store.add_system_event(ActorKind::Human, target_id, event) {
        log::warn!("could not record the activity event: {e}");
    }
}

impl StartupHealthDto {
    /// Absorb the startup_check of an opened store. A read open (`open_read_at`) deliberately
    /// **does not compute** the O(total) doctor pass (it keeps per-click reads inside their budget),
    /// so we compute it here, and only when it is needed. A full open (`open_at`) already computed
    /// it while opening, so that result is used. Does nothing when the startup integrity check is
    /// disabled.
    fn absorb(&mut self, store: &Store) {
        let computed;
        let h = match &store.startup_check {
            Some(h) => h,
            None if store.config.startup_integrity_check => {
                let Ok(health) = store.compute_startup_health() else { return };
                computed = health;
                &computed
            }
            None => return,
        };
        self.issues
            .extend(h.doctor.issues.iter().map(DoctorIssueDto::from));
    }
}

impl VersionStatusDto {
    /// Absorb this store's version state. `update_available` is raised when `upstream` (the
    /// published latest.json) names a version newer than the one running. `None` — update checking
    /// disabled, not fetched, or the fetch failed — means no update.
    fn absorb(&mut self, store: &Store, upstream: Option<&amenbo_core::update_check::LatestRelease>) {
        let vs = store.version_status().with_upstream(upstream);
        if self.app_version.is_empty() {
            self.app_version = vs.app_version.to_string();
        }
        if vs.update_available {
            self.update_available = true;
            if self.newer_version.is_none() {
                self.newer_version = vs.newer_version;
            }
        }
    }
}

/// Shape a facet (human / ai) into the GUI's [`ActorDto`]. A single local store has two facets —
/// "me (the human)" and "my AI" — and both belong to the same one person: me. `kind` separates human
/// from ai, and the display name is looked up in config (`human_name`/`ai_name`) — the read-model
/// carries no names. An [`ActorDto`] used as a label (assignee, author) gets no face: the roster is
/// the only thing that supplies avatars from config.
fn facet_actor(config: &amenbo_core::config::Config, kind: Option<ActorKind>) -> ActorDto {
    let kind = kind.unwrap_or(ActorKind::Human);
    let name = match kind {
        ActorKind::Ai => config.ai_display_name(),
        ActorKind::Human => config.human_display_name(),
    };
    ActorDto { name, kind: kind.as_str(), avatar: None }
}

fn date_iso(d: NaiveDate) -> String {
    amenbo_core::time::date_to_string(d)
}

/// Take a system event apart into the kind that names a sentence template and the values that fill
/// it, leaving the sentence itself to the GUI's dictionary (`eventText` in app/src/core/i18n). A
/// kind carries only the fields its own template asks for, so everything else stays absent; a kind
/// this build does not know keeps its name and reaches the reader through the generic template.
fn event_dto(ev: &serde_json::Value) -> EventDto {
    let kind = ev
        .get("kind")
        .and_then(|k| k.as_str())
        .unwrap_or("event")
        .to_string();
    let str_field = |field: &str| ev.get(field).and_then(|x| x.as_str()).map(str::to_string);
    let count = |field: &str| ev.get(field).and_then(|x| x.as_u64()).unwrap_or(0);
    let mut dto = EventDto { kind, status: None, to_kind: None, tasks: None, decisions: None };
    match dto.kind.as_str() {
        "task.status_changed" => dto.status = str_field("new"),
        // Absent `to_kind` is itself the answer here: the assignee was taken away.
        "task.assigned" => dto.to_kind = str_field("to_kind"),
        "project.deleted" => {
            dto.tasks = Some(count("tasks"));
            dto.decisions = Some(count("decisions"));
        }
        _ => {}
    }
    dto
}

/// Build a [`TaskCardDto`] from a read-model [`amenbo_core::store_engine::read::TaskCardRow`].
/// This is the card path: the row already carries the resolved project names, the actors' facets,
/// the open blockers and the comment count, so a card costs one indexed query. Actors are facet
/// one — the display name comes from `config` (`human_name`/`ai_name`). The top-level project id
/// comes from the placement.
fn task_card_from_row(store: &Store, row: amenbo_core::store_engine::read::TaskCardRow) -> TaskCardDto {
    let config = &store.config;
    let card_kind = |a: &amenbo_core::store_engine::read::CardActor| a.kind.as_deref().and_then(ActorKind::parse);

    let project_id = row.placement.as_ref().map(|p| p.project_id);

    let placement_dto = row.placement.as_ref().map(|p| PlacementDto {
        project: ProjectRefDto {
            id: p.project_id,
            name: p.project_name.clone().unwrap_or_default(),
        },
    });

    let assignee = row.assignee.as_ref().map(|a| facet_actor(config, card_kind(a)));
    let created_by = row.created_by.as_ref().map(|a| facet_actor(config, card_kind(a)));

    let due_date = row.due_on.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    // The start day is read through core's own predicate, so the card cannot drift from what the
    // reserve enforces — the GUI must not call a task startable that `task status` would refuse.
    let start_date = row.start_on.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let not_started_until =
        amenbo_core::view::not_started_until(start_date, amenbo_core::time::today());
    // Core's own predicate rather than a fourth restatement of it: `ready` on a card and `ready` at the
    // reserve are the same question, and the only way they cannot drift is by being one call.
    let ready = amenbo_core::view::is_ready(
        !row.blocked_by.is_empty(),
        !row.blocked_by_decisions.is_empty(),
        start_date,
        amenbo_core::time::today(),
        row.draft,
    );
    let blocked_by: Vec<TaskRefDto> = row
        .blocked_by
        .into_iter()
        .map(|(id, name)| TaskRefDto { id, name })
        .collect();

    let linked_decisions: Vec<DecisionRefDto> = row
        .linked_decisions
        .into_iter()
        .map(|r| DecisionRefDto { id: r.id, name: r.name, r#ref: r.display_ref })
        .collect();
    let blocked_by_decisions: Vec<DecisionRefDto> = row
        .blocked_by_decisions
        .into_iter()
        .map(|r| DecisionRefDto { id: r.id, name: r.name, r#ref: r.display_ref })
        .collect();

    let status = TaskStatus::parse(&row.status).unwrap_or_default();
    let premise_change = premise_change_dto(store, row.id, status);

    TaskCardDto {
        r#ref: amenbo_core::idref::task(row.id),
        id: row.id,
        title: row.title,
        notes: row.notes,
        project_id,
        status: status.as_str(),
        assignee,
        priority: row.priority.as_deref().and_then(Priority::parse).map(|p| p.as_str()),
        due: due_date.map(date_iso),
        start_on: start_date.map(date_iso),
        completed_at: row
            .completed_at
            .as_deref()
            .and_then(amenbo_core::time::Timestamp::parse_rfc3339)
            .map(|ts| ts.to_rfc3339_z()),
        comments: row.num_comments,
        ready,
        blocked_by,
        placement: placement_dto,
        created_by,
        linked_decisions,
        blocked_by_decisions,
        not_started_until: not_started_until.map(date_iso),
        draft: row.draft,
        premise_change,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// The holder-side premise-change surface for a card (`AMB-D-366`): premises pinned on after the task was
/// reserved. Only an `in_progress` task carries the reservation at risk, so the read runs for that status
/// alone; every other status yields `None` without touching the store. A read error also yields `None` —
/// this is additive context, never a reason to fail the card — as does an in_progress task whose premises
/// have not shifted, so the field is `Some` exactly when the surface should draw.
fn premise_change_dto(store: &Store, task_id: i64, status: TaskStatus) -> Option<PremiseChangeDto> {
    if status != TaskStatus::InProgress {
        return None;
    }
    let change = store.premise_change_since(task_id).ok()?;
    if !change.any() {
        return None;
    }
    let decisions = |refs: Vec<amenbo_core::view::DecisionRef>| -> Vec<DecisionRefDto> {
        refs.into_iter()
            .map(|d| DecisionRefDto {
                r#ref: Some(amenbo_core::idref::decision(d.id)),
                id: d.id,
                name: d.name,
            })
            .collect()
    };
    Some(PremiseChangeDto {
        added_blockers: change
            .added_blockers
            .into_iter()
            .map(|b| TaskRefDto { id: b.id, name: b.name })
            .collect(),
        added_decisions: decisions(change.added_decisions),
        reopened_decisions: decisions(change.reopened_decisions),
    })
}

/// Scratch accumulator for building the snapshot projection.
#[derive(Default)]
struct Acc {
    projects: Vec<ProjectDto>,
    activity: Vec<ActivityItemDto>,
}

/// Build a GUI [`DecisionDto`] from a read-model [`amenbo_core::store_engine::read::DecisionCardRow`] (the
/// decision twin of [`task_card_from_row`]). The row already carries every cross-ref's `D-n`/`#n`, so
/// the card costs one query and no scan of the decisions or the tasks. Timestamps are re-normalized
/// through `Timestamp` so the rendered rfc3339 is the one shape the GUI ever sees.
fn decision_card_from_row(row: amenbo_core::store_engine::read::DecisionCardRow) -> DecisionDto {
    use amenbo_core::time::Timestamp;
    let to_ref = |r: amenbo_core::store_engine::read::DecisionCardRef| DecisionRefDto {
        id: r.id,
        name: r.name,
        r#ref: r.display_ref,
    };
    let plain_ref = |r: amenbo_core::view::Ref| PlainRefDto { id: r.id, name: r.name };
    DecisionDto {
        r#ref: amenbo_core::idref::decision(row.id),
        id: row.id,
        title: row.title,
        body: row.body,
        status: row.status,
        project: row.project.map(|p| ProjectRefDto { id: p.id, name: p.name }),
        supersedes: row.supersedes.into_iter().map(to_ref).collect(),
        superseded_by: row.superseded_by.into_iter().map(to_ref).collect(),
        amends: row.amends.into_iter().map(to_ref).collect(),
        amended_by: row.amended_by.into_iter().map(to_ref).collect(),
        builds_on: row
            .builds_on
            .into_iter()
            .map(|p| PremiseRefDto {
                id: p.decision.id,
                name: p.decision.name,
                r#ref: p.decision.display_ref,
                superseded_by: p.superseded_by,
            })
            .collect(),
        built_on_by: row.built_on_by.into_iter().map(to_ref).collect(),
        decided_at: row
            .decided_at
            .as_deref()
            .and_then(Timestamp::parse_rfc3339)
            .map(|t| t.to_rfc3339_z()),
        decided_by: row.decided_by.map(plain_ref),
        linked_tasks: row
            .linked_tasks
            .into_iter()
            .map(|t| LinkedTaskRefDto {
                id: t.task.id,
                // A linked task is always live, so its title is always present — the `Option` on the
                // shared `DecisionCardRef` is only there for dangling decision edges, never for tasks.
                name: t.task.name.unwrap_or_default(),
                r#ref: t.task.display_ref,
                status: t.status,
            })
            .collect(),
        created_at: Timestamp::parse_rfc3339(&row.created_at).unwrap_or_default().to_rfc3339_z(),
        updated_at: Timestamp::parse_rfc3339(&row.updated_at).unwrap_or_default().to_rfc3339_z(),
    }
}

/// Build the store's projection into `acc` (projects + activity).
fn collect_store(store: &Store, acc: &mut Acc) -> Result<(), CmdError> {
    use amenbo_core::store_engine;

    let read_model = store.read_model();
    let conn = read_model.conn();

    let project_rows = store_engine::read::project_overview(conn, store.reach())?;
    for p in &project_rows {
        let dimensions: Vec<DimensionDto> = p
            .dimensions
            .iter()
            .map(|d| DimensionDto {
                id: d.id,
                name: d.name.clone(),
                slug: d.slug.clone(),
                notes: d.notes.clone(),
                cardinality: d.cardinality.clone(),
                role: d.role.clone(),
                ordered: d.ordered,
                show_on_card: d.show_on_card,
                required: d.required,
                applies_to: d.applies_to.clone(),
                values: d
                    .values
                    .iter()
                    .map(|v| DimensionValueDto {
                        id: v.id,
                        name: v.name.clone(),
                        slug: v.slug.clone(),
                        start_on: v.start_on.map(|d| d.to_string()),
                        end_on: v.end_on.map(|d| d.to_string()),
                        closed: v.closed,
                    })
                    .collect(),
            })
            .collect();
        acc.projects.push(ProjectDto {
            id: p.id,
            name: p.name.clone(),
            color: p.color.clone().unwrap_or_else(|| "#9aa7b2".to_string()),
            view: p.default_view.clone(),
            open_count: p.open_count,
            proposed_decision_count: p.proposed_decision_count,
            dimensions,
        });
    }

    let items = amenbo_core::activity::page(
        &amenbo_core::activity::Ledger::open(&store.paths.activity_file),
        conn,
        &amenbo_core::activity::Filter { limit: Some(100), ..Default::default() },
    )?;
    acc.activity.extend(items.into_iter().map(|it| activity_dto(it, &store.config)));
    Ok(())
}

/// The store's activity (the latest `limit` items, newest first), shaped into DTOs. This is what
/// `activity_page` reaches back with, over the same path as `collect_store`'s default read of 100
/// (the file ledger merged with `task_comment`).
fn store_activity_dtos(store: &Store, limit: usize) -> Result<Vec<ActivityItemDto>, CmdError> {
    let read_model = store.read_model();
    let items = amenbo_core::activity::page(
        &amenbo_core::activity::Ledger::open(&store.paths.activity_file),
        read_model.conn(),
        &amenbo_core::activity::Filter { limit: Some(limit), ..Default::default() },
    )?;
    Ok(items.into_iter().map(|it| activity_dto(it, &store.config)).collect())
}

/// Assemble the read data for every screen into one sheet. **Directory-independent**: what it opens
/// is the single store in this machine's app-data. If there is no store yet it returns an **empty
/// snapshot** (we never quietly create a default empty store — the empty state is explicit).
fn build_snapshot() -> Result<Snapshot, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("build_snapshot");
    let mut acc = Acc::default();

    let paths = amenbo_core::config::Paths::resolve().ok();
    let config = paths
        .as_ref()
        .map(|p| amenbo_core::config::Config::load(&p.config_file))
        .unwrap_or_default();
    let language = config.language.clone();
    let date_locale = config.date_locale.clone();

    let upstream = upstream_release(config.update_check, false);

    let mut startup_health = StartupHealthDto::default();
    let mut version_status = VersionStatusDto::default();
    with_store_read(|store| {
        startup_health.absorb(store);
        version_status.absorb(store, upstream.as_ref());
        collect_store(store, &mut acc)
    })?;

    Ok(Snapshot {
        language,
        date_locale,
        roster: config
            .roster()
            .into_iter()
            .map(|(kind, name)| ActorDto {
                name,
                kind: kind.as_str(),
                avatar: config.avatar_for(kind),
            })
            .collect(),
        projects: acc.projects,
        activity: acc.activity,
        startup_health,
        version_status,
        perf_log: config.perf_log.map(|p| p.as_config_str().to_string()),
        update_check: config.update_check,
        autostart: config.autostart,
        tick_consent: config.tick_consent.map(|c| c.as_str().to_string()),
        tick_removal_leaves_a_row: amenbo_core::tick::removal_leaves_a_row(),
        default_view: config.default_view.as_str().to_string(),
    })
}

/// Return the read data for every screen in one sheet (the seam in adapter.ts receives it).
#[tauri::command]
pub fn snapshot() -> Result<Snapshot, CmdError> {
    let snap = build_snapshot()?;
    log::info!(
        "snapshot: projects={} activity={}",
        snap.projects.len(),
        snap.activity.len(),
    );
    Ok(snap)
}

impl WriteAck {
    fn new(scopes: &[&'static str]) -> WriteAck {
        WriteAck { scopes: scopes.to_vec(), ..Default::default() }
    }
    fn task(mut self, id: i64) -> WriteAck {
        self.tasks.push(id);
        self
    }
    fn decision(mut self, id: i64) -> WriteAck {
        self.decisions.push(id);
        self
    }
}

/// The store file being watched (there is only one store). `None` when the path cannot be resolved.
/// Kept in one place so the watcher (change detection) and `store_signature` (deduping our own
/// writes) look at the same file.
fn store_file() -> Option<std::path::PathBuf> {
    amenbo_core::config::Paths::resolve().ok().map(|p| p.store_file)
}

/// The config file (`config.json`), which sits in the same directory as the store and so is already
/// covered by the watcher. It is the signature's third leg — see [`store_signature_string`].
fn config_file() -> Option<std::path::PathBuf> {
    amenbo_core::config::Paths::resolve().ok().map(|p| p.config_file)
}

/// A file's identity (mtime, size). **Not for detecting changes** — its only job is to tell whether
/// the file itself was swapped out from under us (see [`store_signature_string`] below).
fn file_identity(p: &std::path::Path) -> (u128, u64) {
    let Ok(m) = std::fs::metadata(p) else { return (0, 0) };
    let mtime = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    (mtime, m.len())
}

/// The **read connection we keep open** for change detection, plus the identity of the file it is
/// reading. `PRAGMA data_version` is a value SQLite guarantees will answer "**has another connection
/// committed?**" — but **it is only comparable against values read from the same connection** (SQLite
/// states outright that values from another connection cannot be compared). So we hold exactly one
/// connection for the life of the process: the watcher (which wakes the UI) and `store_signature`
/// (which dedupes our own writes) look at the **same connection**, so their verdicts on "did it
/// change?" can never disagree.
struct Watch {
    store: Option<Store>,
    path: std::path::PathBuf,
    file: (u128, u64),
}

static WATCH: std::sync::OnceLock<std::sync::Mutex<Watch>> = std::sync::OnceLock::new();

fn watch() -> &'static std::sync::Mutex<Watch> {
    WATCH.get_or_init(|| {
        // The one handle this process keeps across actions is also the one a swap trips over: Windows
        // refuses to replace a file anybody still has open (`AMB-D-704`). Core asks before it swaps, so
        // register the answer here, where the connection is born — not at a call site that would have to
        // remember.
        amenbo_core::swap_lock::release_before_swap(release_watch_for_swap);
        std::sync::Mutex::new(Watch { store: None, path: std::path::PathBuf::new(), file: (0, 0) })
    })
}

/// Let go of the watch connection so a `restore` or a migration can replace the store file underneath it.
/// The next signature read opens a fresh one against whatever is there then — the same recovery
/// [`release_orphaned_watch`] performs, arrived at before the swap instead of after it.
///
/// Reads the static directly rather than going through [`watch`]: this runs from core, mid-swap, and
/// re-entering the initialiser that registered it would be a way to deadlock. Nothing open yet means
/// nothing to let go of.
fn release_watch_for_swap() {
    let Some(watch) = WATCH.get() else { return };
    if let Ok(mut w) = watch.lock() {
        w.store = None;
    }
}

/// The WAL's shared-memory index beside the store file (`store.sqlite-shm`).
fn shm_file(store: &std::path::Path) -> std::path::PathBuf {
    let mut p = store.as_os_str().to_os_string();
    p.push("-shm");
    std::path::PathBuf::from(p)
}

/// Whether the connection we are holding is attached to a `-shm` that no longer exists on disk.
///
/// A live connection to a WAL store keeps `-shm` there, so its absence while we hold one says the
/// index we are attached to is an orphan — the file was unlinked (a `restore`/migration clears the
/// sidecars; so does anything else that reaches into the store directory), and we are the last
/// reference to an inode nobody will ever commit to again.
///
/// The orphan does not stay ours alone. SQLite shares one shared-memory node **per process**, so
/// every connection opened afterwards inherits the dead index: `open` still succeeds, reads still
/// answer, and only writes fail — with `disk I/O error`, until the process exits. That is exactly the
/// one guarantee open-per-action is supposed to give us (a broken connection costs one action, never
/// the session), and this single long-lived connection is the only thing standing in its way.
fn watch_is_orphaned(w: &Watch, store: &std::path::Path) -> bool {
    w.store.is_some() && !shm_file(store).exists()
}

/// Let go of an orphaned watch connection, so the open that follows attaches to the live index
/// instead of inheriting the dead one. The next signature read opens a fresh connection.
fn release_orphaned_watch() {
    let Some(path) = store_file() else { return };
    let Ok(mut w) = watch().lock() else { return };
    if watch_is_orphaned(&w, &path) {
        log::warn!("the store's -shm was deleted under the watch connection; reopening it");
        w.store = None;
    }
}

/// The store's change signature, on three legs: **`PRAGMA data_version`, the identity of the main
/// file, and the identity of `config.json`**. `data_version` is the value SQLite guarantees will
/// tell you that **some connection has
/// committed**; in WAL mode an external writer's commit lands only in `-wal` and never moves the
/// main file's mtime, so guessing from mtime/size would miss the arrival entirely (system events
/// number themselves in the same transaction — a DB commit — so there is no need to stat the ledger
/// separately). The GUI's own writes commit on another connection and move this value too, and
/// filtering those out is the front end's job: after a write, `loadSnapshot` records this signature,
/// and when `store-changed` arrives with a matching one, it does not refetch. We watch the file's
/// identity alongside it because `fold`, `restore` and migration **swap the file out wholesale** —
/// the connection we are holding would go on reading a dead inode where nobody will ever commit
/// again, so when mtime/size moves we reopen it. That degrades cleanly into "the whole file changed
/// → gap → refetch everything". The same reopen covers a sidecar cleared without the main file
/// moving ([`watch_is_orphaned`]), where the connection is just as dead but nothing about the main
/// file says so.
///
/// The third leg is `config.json`, which is not in the database at all: it is written straight to
/// disk (`Store::save_config`) and so moves neither `data_version` nor the store file. It needs a leg
/// of its own because the watcher already sees it — `config.json` shares the store's directory, so
/// the kernel wakes us for it, and this gate is the only thing that can tell that wake apart from a
/// spurious one. Without the leg, a language, a theme or a default view set from the CLI — the AI's
/// ordinary route, run beside a GUI somebody has open — would reach the screen only at the next
/// restart, and not even on focus return, since the focus catch-up asks this same question.
fn store_signature_string() -> String {
    let Some(path) = store_file() else { return String::new() };
    let Ok(mut w) = watch().lock() else { return String::new() };

    let file = file_identity(&path);
    if w.store.is_none() || watch_is_orphaned(&w, &path) || w.file != file || w.path != path {
        w.store = amenbo_core::config::Paths::resolve().ok().and_then(|p| Store::open_read_at(p).ok());
        w.file = file;
        w.path = path;
    }
    let version = w
        .store
        .as_ref()
        .and_then(|s| amenbo_core::store_engine::read::data_version(s.read_model().conn()).ok())
        .unwrap_or(0);
    let config = config_file().map(|p| file_identity(&p)).unwrap_or((0, 0));
    format!("{}:{}:{}:{}:{}", file.0, file.1, version, config.0, config.1)
}

/// The signature (`store_signature_string`) the GUI uses to filter out the `store-changed` events
/// its own writes caused.
#[tauri::command]
pub fn store_signature() -> String {
    store_signature_string()
}

/// The published release this build measures itself against, or `None` when there is none to have.
/// Every update-available answer the GUI gives is computed from this, so it is the one place the
/// question "is this build even in the self-update business" is asked.
///
/// **A development build is not.** Its endpoint is production's manifest and its version is normally
/// *behind* what that manifest names, so an offer taken here would replace the bundle under test
/// with the production one — identifier, executable name and app-data all — and the developer would
/// be clicking production from then on. Withholding the material is what keeps the offer from ever
/// being raised: no banner to press, no "up to date" note claiming something this build cannot know,
/// and no traffic to fetch either. The plugin that would perform the swap is withheld too (`lib.rs`),
/// so the two halves fail closed independently.
///
/// `fresh` bypasses the TTL cache, and only the menu's manual check asks for it
/// (`AMB-D-710`) — exactly as [`amenbo_core::update_check::check_fresh`] describes. Every reading
/// nobody asked for, process start included, goes through the cache.
fn upstream_release(
    enabled: bool,
    fresh: bool,
) -> Option<amenbo_core::update_check::LatestRelease> {
    if amenbo_core::config::Paths::is_dev_channel() {
        return None;
    }
    if fresh {
        amenbo_core::update_check::check_fresh(enabled)
    } else {
        amenbo_core::update_check::check(enabled)
    }
}

/// Just the update-available state, without assembling a whole snapshot. The GUI asks this on every
/// focus return, which is the moment the user starts using the app again; the snapshot cannot serve
/// that, since it is only rebuilt when the store itself has moved, and someone who only reads never
/// moves it. Cheap by construction: the cache TTL lives in `update_check::check`, so a call inside
/// the window answers from the cache with no traffic at all, and only a stale one queries upstream
/// (timed out, silent on failure). Never `check_fresh` — bypassing the cache belongs to the menu's
/// manual check alone, not to a trigger the user can fire by alt-tabbing.
#[tauri::command]
pub fn version_status() -> Result<VersionStatusDto, CmdError> {
    let config = amenbo_core::config::Paths::resolve()
        .map(|p| amenbo_core::config::Config::load(&p.config_file))
        .unwrap_or_default();
    let upstream = upstream_release(config.update_check, false);
    let mut dto = VersionStatusDto::default();
    with_store_read(|store| {
        dto.absorb(store, upstream.as_ref());
        Ok(())
    })?;
    Ok(dto)
}

/// A **fresh** update check, for the app menu's manual "check for updates" action. Where
/// [`version_status`] answers from the TTL cache (so alt-tabbing stays cheap and never bypasses it),
/// this queries upstream every time (`check_fresh`) because the user explicitly asked "is there one
/// right now". It forces the check on regardless of the `update_check` config toggle — the same "an
/// explicit user action goes and fetches" stance as [`open_latest_installer`] and
/// `resolve_update_url` — so it still works for someone who turned automatic checking off, which is
/// the whole point of the manual action; only the env kill switch silences it. Returns the same
/// [`VersionStatusDto`]: the menu path shows the update banner when it reports one and an "up to
/// date" note when it does not.
///
/// The channel still decides, though ([`upstream_release`]) — which is why a development build does
/// not carry the menu item that calls this (`menu.rs`).
#[tauri::command]
pub fn check_updates_fresh() -> Result<VersionStatusDto, CmdError> {
    let upstream = upstream_release(true, true);
    let mut dto = VersionStatusDto::default();
    with_store_read(|store| {
        dto.absorb(store, upstream.as_ref());
        Ok(())
    })?;
    Ok(dto)
}

/// Return the real path of the app-data root, for the "location" line under Settings > Data.
#[tauri::command]
pub fn store_locations() -> StoreLocationsDto {
    StoreLocationsDto {
        root: amenbo_core::config::Paths::data_root()
            .to_string_lossy()
            .into_owned(),
    }
}

/// What this build calls itself in the header, or `null` on production — production ships no badge
/// (`AMB-D-390`). Constant for the life of the process: the channel is fixed at build time by
/// `AMENBO_APP_NAME`, so the GUI asks once at startup and never again.
///
/// It exists because the three builds a developer runs side by side — production, the shared dev app,
/// one task's throwaway instance — are the same process under the same name, and the window title is
/// the only thing telling them apart until a screenshot crops it off.
#[tauri::command]
pub fn dev_badge() -> Option<String> {
    amenbo_core::config::Paths::dev_badge()
}

/// How this build's CLI is run where someone types it — `amenbo` in production, `amenbo-dev` on the
/// shared dev build, the path into the bundle on a macOS preview, and `None` on a Linux one, where
/// nothing on the machine reaches it at all
/// ([`amenbo_core::config::Paths::command_to_run`]). Asked once at startup for the reason
/// [`dev_badge`] is: the channel is stamped in at build time.
///
/// Every screen that hands over a command to run is the surface that needs it, and each of them has
/// to be able to say nothing rather than name something — a dev window spelling a command `amenbo`
/// names a CLI that is not installed beside it, and a preview window naming one at all, where the
/// build ships none a member can reach, is the same lie one step further on.
#[tauri::command]
pub fn cli_command_name() -> Option<&'static str> {
    amenbo_core::config::Paths::command_to_run()
}

/// Open the folder holding this machine's logs in the OS file manager — the one step between "please
/// attach your logs" and a file the user can drag onto an issue (`AMB-D-382`).
///
/// The **folder**, not a file: `amenbo.log` and `perf.log` live side by side and a report usually wants
/// both, so opening either one alone hands over half the answer. That is also why the log was put here
/// rather than in the platform's own log directory — one folder to ask for.
///
/// A folder that is not there yet is reported rather than created. The diagnostic log is on by default
/// and written from startup (`AMB-D-382`), so in practice it exists by the time anyone opens Settings;
/// creating an empty one to make the button always succeed would answer "here are your logs" with a
/// folder that holds none.
#[tauri::command]
pub fn open_logs_dir() -> Result<(), CmdError> {
    let dir = crate::diag::logs_dir()
        .ok_or_else(|| CmdError::from("cannot work out where the logs are kept".to_string()))?;
    if !dir.is_dir() {
        return Err(format!("there are no logs yet ({})", dir.display()).into());
    }
    os_open(&dir.to_string_lossy())
        .map_err(|e| format!("cannot open '{}': {e}", dir.display()).into())
}

/// The paged read behind history mode. Skips `offset` items newest-first and returns the next
/// `limit`. The default `snapshot` stays light by carrying only the latest 100; when the GUI's
/// virtual scroller reaches back past those, it calls this for its scroll window and nothing more.
#[tauri::command]
pub fn activity_page(offset: usize, limit: usize) -> Result<Vec<ActivityItemDto>, CmdError> {
    let need = offset.saturating_add(limit);
    let mut all: Vec<ActivityItemDto> = Vec::new();
    with_store_read(|store| {
        all.extend(store_activity_dtos(store, need)?);
        Ok(())
    })?;
    all.sort_by(|a, b| b.at.cmp(&a.at));
    Ok(all.into_iter().skip(offset).take(limit).collect())
}

/// The default page size. Bounds a single incremental read (if the feed piled up while the user was
/// away, the caller watches `more` and pages through it).
const CHANGES_PAGE: usize = 500;

/// Read the change feed from a cursor onward. Returns the rows after `cursor`, oldest first (just a
/// forward read of the feed table with `id > ?`). Passing 0 means "from the oldest the feed still
/// has", and `expired` is raised if that has been truncated away. **Not the same thing as activity's
/// `cur1_`/`cur2_` cursors** — those are a three-part cursor merging the file ledger and
/// `task_comment` in time order, whereas this is a single table with monotonic ids, where a plain
/// `id > ?` is all it takes.
///
/// The window is `None`: the GUI stands at the open reach and shows the whole device, so it reads every
/// change rather than one project's. Narrowing the same feed to a window is the carrier's road
/// (`Store::sync_changes`, `AMB-D-582`).
#[tauri::command]
pub fn changes_since(cursor: i64, limit: Option<usize>) -> Result<ChangesDto, CmdError> {
    use amenbo_core::store_engine::read::{self, FeedSlice};

    let _perf = amenbo_core::perf::Timer::start("changes_since");
    let store = open_store_read()?;
    let conn = store.read_model().conn();
    let limit = limit.unwrap_or(CHANGES_PAGE);
    match read::changes_since(conn, cursor, limit as i64, None)? {
        FeedSlice::Changes { rows, more } => Ok(ChangesDto {
            cursor: rows.last().map(|r| r.id).unwrap_or(cursor),
            rows: rows
                .into_iter()
                .map(|r| ChangeRowDto { dataset: r.dataset, row_id: r.row_id, op: r.op })
                .collect(),
            more,
            expired: false,
        }),
        FeedSlice::Gap => Ok(ChangesDto {
            rows: Vec::new(),
            cursor: read::change_feed_head(conn)?,
            more: false,
            expired: true,
        }),
    }
}

/// The feed's current head id. The starting cursor for a caller that has just read the store from
/// the source of truth and now wants to wait for "only the changes after this point". 0 on an empty
/// feed. **Take it before you read, not after** — take it first and then refetch, and any change
/// that lands in between stays ahead of the cursor (you see it twice, but you never lose it). Do it
/// the other way round and changes falling in the gap between the refetch and the read are lost for
/// good.
#[tauri::command]
pub fn change_cursor() -> Result<i64, CmdError> {
    let store = open_store_read()?;
    Ok(amenbo_core::store_engine::read::change_feed_head(store.read_model().conn())?)
}

/// Shape one row of the persistent read-model into a GUI DTO — timestamps and an event's parts, with
/// nothing worded. An unrecoverable target name arrives from core as an empty `title` and is passed
/// on empty: the stand-in a reader sees is a sentence, and sentences are the GUI's to write.
fn activity_dto(it: amenbo_core::activity::Item, config: &amenbo_core::config::Config) -> ActivityItemDto {
    // Read before `it` is taken apart below: the sequence is derived from the whole row.
    let seq = it.seq().rank();
    let event = it.event.as_ref().map(event_dto);
    ActivityItemDto {
        id: it.id,
        seq,
        at: it.at.to_rfc3339_z(),
        kind: it.kind.as_str().to_string(),
        author: facet_actor(config, it.author_kind),
        target: ActivityTargetDto {
            target_type: it.target_type.as_str().to_string(),
            id: it.target_id,
            title: it.title,
            live: it.target_live,
        },
        event,
        text: it.text,
        edited_at: it.edited_at.as_ref().map(Timestamp::to_rfc3339_z),
    }
}

/// One task's activity (comments included), newest first, for the comment list in the detail pane.
/// The latest-100 window in `snapshot` is not enough: as the task count grows, an older task's
/// comments fall outside the window and go missing (the 💬 count stays right, since it comes from
/// num_comments, while the list below it goes empty). So this bypasses the window and queries the
/// persistent read-model directly, per task.
#[tauri::command]
pub fn task_activity(task_id: i64, limit: Option<usize>) -> Result<Vec<ActivityItemDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("task_activity");
    let collect_from = |store: &Store| -> Result<Vec<ActivityItemDto>, CmdError> {
        let read_model = store.read_model();
        let items = amenbo_core::activity::page(
            &amenbo_core::activity::Ledger::open(&store.paths.activity_file),
            read_model.conn(),
            &amenbo_core::activity::Filter { task_id: Some(task_id), limit, ..Default::default() },
        )?;
        Ok(items.into_iter().map(|it| activity_dto(it, &store.config)).collect())
    };

    let found = find_in_store(|store| {
        let items = collect_from(store)?;
        Ok((!items.is_empty()).then_some(items))
    })?;
    Ok(found.unwrap_or_default())
}

/// The **paged read** behind the task list. `store_engine::read::list_task_ids` gives back **one
/// page of task ids** through an indexed `WHERE`/`ORDER BY`/`LIMIT`/`OFFSET`, and only those ids are
/// hydrated, per task, into `TaskCardDto` (no all-rows blob comes back — memory stays bounded). The
/// `filter` grammar is exactly that of `task list --filter` (they share `query::Filter`). The `Store`
/// opened here queries the persistent read-model directly, projects just the window, and is released
/// on return (it adds nothing resident). A read that goes straight to the engine **passes its reach
/// explicitly** (the GUI is the human's place — the whole machine): leave it to the default, and on
/// the day another surface appears, the reach could quietly fall back to All with nobody noticing.
#[tauri::command]
pub fn task_page(
    project_id: Option<i64>,
    filter: Option<String>,
    sort: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<TaskPageDto, CmdError> {
    use amenbo_core::store_engine::{self, TaskQuery};

    let _perf = amenbo_core::perf::Timer::start("task_page");
    let store = open_store_read()?;
    let today = amenbo_core::time::today();

    let filter_expr = filter.unwrap_or_default();
    let mut parsed = query::Filter::parse(&filter_expr, today)?;
    let sort = sort.unwrap_or_else(|| "order".to_string());

    let read_model = store.read_model();

    parsed.resolve(read_model.conn())?;

    let page = store_engine::list_task_ids(
        read_model.conn(),
        &TaskQuery {
            reach: store.reach(),
            project_id,
            filter: &parsed,
            sort: &sort,
            today,
            limit,
            offset,
        },
    )
    ?;

    let conn = read_model.conn();
    let mut tasks: Vec<TaskCardDto> = Vec::with_capacity(page.ids.len());
    for &id in &page.ids {
        if let Some(row) = amenbo_core::store_engine::read::task_card_row(conn, id)? {
            tasks.push(task_card_from_row(&store, row));
        }
    }

    Ok(TaskPageDto { tasks, total_matched: page.total_matched, offset: offset.unwrap_or(0), limit })
}

/// The ids of a project's tasks matching a free-text search — the task twin of [`decision_search`],
/// and the board's own door to the match. The term is passed **structurally**, not spelled into a
/// filter expression: the grammar splits on whitespace, so a search box that hands over two words
/// would silently lose everything after the first (`AMB-D-449` takes `text:` out of the grammar for
/// exactly that kind of reason; the match itself stays).
///
/// It returns ids and not cards because the screen already holds the project's tasks: the search
/// narrows what it has rather than being a second listing to reconcile with the first. Both faces run
/// the same match the read-model carries, so they cannot come to disagree about what a word matches.
#[tauri::command]
pub fn task_search(project_id: i64, text: String) -> Result<Vec<i64>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("task_search");
    let store = open_store_read()?;
    let result = query::list(
        store.read_model().conn(),
        store.reach(),
        query::ListParams {
            project_id: Some(project_id),
            text: Some(text),
            // The board re-sorts what it holds, so the order here is only a stable one to page by.
            sort: "order".to_string(),
            ..Default::default()
        },
    )?;
    Ok(result.tasks.into_iter().map(|t| t.id).collect())
}

/// Hydrate the given ids into `TaskCardDto` (input order preserved). `task_page` returns "the ids on
/// this page" and `tasks_by_ids` returns "any set of ids" — a pair of reads that lets the front end
/// get by without ever holding an array of every task. It is used (1) to fetch a single task for the
/// detail pane (getTask), and (2) to hydrate the inbox's comment tasks (the ids
/// `mailbox_comment_tasks` returns) so they can be unioned into the view's set. The cost is bounded
/// by the number of ids, not by the size of the store. Ids that do not exist are dropped silently
/// (the caller treats such a task as deleted or out of reach).
#[tauri::command]
pub fn tasks_by_ids(ids: Vec<i64>) -> Result<Vec<TaskCardDto>, CmdError> {
    use std::collections::HashMap;

    let _perf = amenbo_core::perf::Timer::start("tasks_by_ids");
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut found: HashMap<i64, TaskCardDto> = HashMap::new();

    let scan = |store: &Store| -> Result<(), CmdError> {
        let pending: Vec<i64> = ids.iter().copied().filter(|id| !found.contains_key(id)).collect();
        if pending.is_empty() {
            return Ok(());
        }
        let read_model = store.read_model();
        let conn = read_model.conn();
        let present = amenbo_core::store_engine::read::present_task_ids(conn, &pending)?;
        for id in present {
            if let Some(row) = amenbo_core::store_engine::read::task_card_row(conn, id)? {
                found.insert(id, task_card_from_row(store, row));
            }
        }
        Ok(())
    };

    with_store_read(scan)?;

    Ok(ids.iter().filter_map(|id| found.remove(id)).collect())
}

/// Return a project's decision records (the decisions tab fetches just its own window). Status
/// filtering, search and sorting are layered on in the client, since the count is bounded. Omitting
/// `limit` means everything (from `offset` on).
#[tauri::command]
pub fn decision_page(
    project_id: i64,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<DecisionPageDto, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("decision_page");
    let store = open_store_read()?;
    let read_model = store.read_model();
    let conn = read_model.conn();
    let page = amenbo_core::store_engine::decision_page(
        conn,
        store.reach(),
        project_id,
        limit,
        offset.unwrap_or(0),
    )?;
    let mut decisions: Vec<DecisionDto> = Vec::with_capacity(page.ids.len());
    for id in &page.ids {
        if let Some(row) = amenbo_core::store_engine::read::decision_card_row(conn, *id)? {
            decisions.push(decision_card_from_row(row));
        }
    }
    Ok(DecisionPageDto { decisions, total_matched: page.total_matched })
}

/// The ids of a project's decisions matching a free-text search — title, body, **and any live comment
/// body**, which is the arm the client cannot reach on its own (comments are not on the page payload, and
/// loading every decision's thread to look through them is exactly what the bounded page exists to avoid).
///
/// It returns ids and not cards on purpose: the screen already holds the project's decisions, so the search
/// is a narrowing of what it has rather than a second listing to reconcile. And it reads the same word
/// index the CLI's `search` does, so the two faces cannot come to disagree about what a word matches — the
/// term is passed structurally because the filter grammar carries no words, and splits on whitespace
/// besides, whereas a search box hands over phrases.
#[tauri::command]
pub fn decision_search(project_id: i64, text: String) -> Result<Vec<i64>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("decision_search");
    let store = open_store_read()?;
    let result = amenbo_core::query::decision_list(
        store.read_model().conn(),
        store.reach(),
        amenbo_core::query::DecisionListParams {
            project_id: Some(project_id),
            text: Some(text),
            ..Default::default()
        },
    )?;
    Ok(result.decisions.into_iter().map(|d| d.id).collect())
}

impl From<amenbo_core::query::HitFace> for SearchFaceDto {
    fn from(face: amenbo_core::query::HitFace) -> Self {
        use amenbo_core::query::HitFace;
        match face {
            HitFace::Title => Self::Title,
            HitFace::Body => Self::Body,
            HitFace::Comment => Self::Comment,
            HitFace::Label => Self::Label,
            HitFace::Attachment => Self::Attachment,
        }
    }
}

/// The GUI's side of `search` (`AMB-D-449`): every place the words are written, hit by hit, across
/// tasks, decisions and the comments on either.
///
/// It is not the board's narrowing under another name. Narrowing answers "which of the rows in front of
/// me match" and returns ids ([`decision_search`], and the task twin the board uses); this answers "where
/// is this written" and returns places, which is why it is a screen of its own rather than a filter on one.
///
/// **The reach is the store's, and the store's reach on this face is every project.** That is not a
/// property of being the GUI: [`amenbo_core::query::search`] takes the reach as an argument, so an AI
/// facet running the same read is held to its bound project by the same call. The window here is wide
/// because the human's is.
///
/// `filter` is read in the grammar of the side `kind` names (`AMB-D-563`), and so needs one: an
/// expression that does not parse — or one sent with no side to read it in — comes back as the error it
/// is, rather than as a silently unfiltered page.
///
/// `kind` and `face` are the two axes (`AMB-D-562`) and travel apart: which record the words are on, and
/// which face of it. Either left unnamed keeps everything on that axis.
///
/// `project_id` is an argument of its own and deliberately not a key of `filter` (`AMB-D-564`): a project
/// is an axis both sides carry, so putting it in the expression would drop the decisions from the answer
/// as the side effect of naming a project. `None` is every project the reach allows.
#[tauri::command]
pub fn search(
    text: String,
    kind: Option<String>,
    face: Option<String>,
    filter: Option<String>,
    project_id: Option<i64>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<SearchResultDto, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("search");
    let store = open_store_read()?;
    let result = store.search(amenbo_core::query::SearchParams {
        text,
        project_id,
        filter_expr: filter.filter(|f| !f.trim().is_empty()),
        kind: kind.as_deref().map(amenbo_core::query::SearchKind::parse).transpose()?,
        face: face.as_deref().map(amenbo_core::query::HitFace::parse).transpose()?,
        sort: amenbo_core::query::SearchSort::default(),
        limit,
        offset,
    })?;
    Ok(SearchResultDto {
        total_matched: result.total_matched,
        hits: result
            .hits
            .into_iter()
            .map(|h| SearchHitDto {
                face: h.face.into(),
                kind: h.kind,
                r#ref: h.r#ref,
                title: h.title,
                comment: h.comment,
                at: h.at.to_rfc3339_z(),
                snippet: h.snippet,
                matches: h
                    .matches
                    .into_iter()
                    .map(|m| SearchMatchDto { start: m.start, end: m.end })
                    .collect(),
                standing: h.standing.map(|s| SearchStandingDto {
                    status: s.status,
                    priority: s.priority,
                    labels: s
                        .labels
                        .into_iter()
                        .map(|l| SearchLabelDto { axis: l.axis, value: l.value })
                        .collect(),
                }),
            })
            .collect(),
    })
}

/// Hydrate the given ids into `DecisionDto` (input order preserved). The decision twin of
/// `tasks_by_ids`; the decision detail pane uses it to fetch a single decision. Ids that do not
/// exist are dropped silently.
#[tauri::command]
pub fn decisions_by_ids(ids: Vec<i64>) -> Result<Vec<DecisionDto>, CmdError> {
    use std::collections::HashMap;

    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut found: HashMap<i64, DecisionDto> = HashMap::new();
    let scan = |store: &Store| -> Result<(), CmdError> {
        let pending: Vec<i64> = ids.iter().copied().filter(|id| !found.contains_key(id)).collect();
        if pending.is_empty() {
            return Ok(());
        }
        let read_model = store.read_model();
        let conn = read_model.conn();
        let present = amenbo_core::store_engine::read::present_decision_ids(conn, &pending)?;
        for id in present {
            if let Some(row) = amenbo_core::store_engine::read::decision_card_row(conn, id)? {
                found.insert(id, decision_card_from_row(row));
            }
        }
        Ok(())
    };

    with_store_read(scan)?;

    Ok(ids.iter().filter_map(|id| found.remove(id)).collect())
}

/// Resolve one conversational reference from a body (`#NNN`, `T-NN`, `D-NN`, ...) to the id of the
/// task or decision it names, so the GUI can turn it into a link. It is the GUI's way in to core's
/// `resolve_any_ref`, so the grammar is never defined twice. Numbers are **globally unique on the
/// machine**, so no project context is needed: `#NNN` names exactly one entity.
/// Ambiguous or unknown gives `Ok(None)` and the UI quietly no-ops (a false positive in link
/// detection must not raise an error dialog).
#[tauri::command]
pub fn resolve_ref(input: String) -> Result<Option<RefTargetDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("resolve_ref");
    let mut hit: Option<RefTargetDto> = None;
    with_store_read(|store| {
        if hit.is_some() {
            return Ok(());
        }
        if let Ok(r) = store.resolve_any_ref(input.trim()) {
            hit = Some(match r {
                amenbo_core::ops::Ref::Task(id) => RefTargetDto { kind: "task".into(), id },
                amenbo_core::ops::Ref::Decision(id) => {
                    RefTargetDto { kind: "decision".into(), id }
                }
            });
        }
        Ok(())
    })?;
    Ok(hit)
}

use amenbo_core::read_receipts::ReadReceipts;

/// Return this machine's read state (per-task last_seen plus the mailbox-wide last_seen). Read at
/// GUI startup and when the inbox renders.
#[tauri::command]
pub fn read_receipts() -> Result<ReadReceipts, CmdError> {
    Ok(open_store()?.read_receipts()?)
}

/// Mark a task as seen (last viewed = now). Called when the detail pane opens. Returns the whole
/// updated state.
#[tauri::command]
pub fn mark_task_seen(task_id: i64) -> Result<ReadReceipts, CmdError> {
    let store = open_store()?;
    store.mark_task_seen(task_id, &Timestamp::now().to_rfc3339_z())?;
    Ok(store.read_receipts()?)
}

/// Mark the whole inbox as seen (advance the reference time for badge freshness to now). Called when
/// the inbox view opens.
#[tauri::command]
pub fn mark_mailbox_seen() -> Result<ReadReceipts, CmdError> {
    let store = open_store()?;
    store.mark_mailbox_seen(&Timestamp::now().to_rfc3339_z())?;
    Ok(store.read_receipts()?)
}

/// Return the inbox items archived on this machine (a list of task_ids). The inbox reads it while
/// rendering and leaves those items out of the list.
#[tauri::command]
pub fn inbox_archived() -> Result<Vec<i64>, CmdError> {
    Ok(open_store()?.inbox_archive_ids()?)
}

/// Archive an inbox item (drop it from the list). Returns the full id list afterwards.
#[tauri::command]
pub fn inbox_archive(task_id: i64) -> Result<Vec<i64>, CmdError> {
    let store = open_store()?;
    store.inbox_archive_add(task_id)?;
    Ok(store.inbox_archive_ids()?)
}

/// Unarchive an inbox item (put it back in the inbox). Returns the full id list afterwards.
#[tauri::command]
pub fn inbox_unarchive(task_id: i64) -> Result<Vec<i64>, CmdError> {
    let store = open_store()?;
    store.inbox_archive_remove(task_id)?;
    Ok(store.inbox_archive_ids()?)
}

/// Return the inbox items this machine has already raised an OS notification for (task_ids). The
/// mailbox loads it once at startup as its "already announced" baseline, so an arrival notifies
/// exactly once even across restarts.
#[tauri::command]
pub fn mailbox_notified_ids() -> Result<Vec<i64>, CmdError> {
    Ok(open_store()?.mailbox_notified_ids()?)
}

/// Record that these inbox items have now been notified. Idempotent and batched — the mailbox adds
/// the ids it just announced (one startup catch-up, or a live arrival) so they are never announced
/// again. Returns the full id list afterwards.
#[tauri::command]
pub fn mailbox_notified_add(task_ids: Vec<i64>) -> Result<Vec<i64>, CmdError> {
    let store = open_store()?;
    store.mailbox_notified_add(&task_ids)?;
    Ok(store.mailbox_notified_ids()?)
}

/// GC for device state (read receipts, the inbox archive and the mailbox notified set). Each
/// accumulates a task id on every view, dismissal or notification, including ids of tasks that have
/// since been deleted. So we build **the complete set of live task ids** and DELETE any row whose id
/// is not in it. Writes only when
/// something actually changed. Does nothing if the store could not be opened — otherwise an empty
/// set would wipe everything. Meant to be called once, at startup. Failure is not fatal (the caller
/// only logs it).
pub fn gc_device_state() -> Result<(), CmdError> {
    use std::collections::HashSet;
    let mut live: HashSet<i64> = HashSet::new();
    let mut opened = false;
    let scan = |store: &Store| -> Result<(), CmdError> {
        opened = true;
        let read_model = store.read_model();
        for id in amenbo_core::store_engine::read::live_task_ids(read_model.conn())? {
            live.insert(id);
        }
        Ok(())
    };

    with_store_read(scan)?;

    if !opened {
        return Ok(());
    }

    let store = open_store()?;
    if store.retain_live_read_receipts(|id| live.contains(&id))? {
        log::info!("gc_device_state: pruned stale read_receipts entries");
    }
    if store.retain_live_inbox_archive(|id| live.contains(&id))? {
        log::info!("gc_device_state: pruned stale inbox_archive entries");
    }
    if store.retain_live_mailbox_notified(|id| live.contains(&id))? {
        log::info!("gc_device_state: pruned stale mailbox_notified entries");
    }
    Ok(())
}

/// The nudges to put to the person now, as the ids they are declared under (`AMB-D-542`). Judging them
/// is core's (`AMB-D-544`); what crosses back is only which ones are due, because the wording and the
/// look belong to the surface that shows them.
///
/// `open_stages` is the caller's half of that judgement: the stages it is currently in — "the setting
/// this nudge is about is still unanswered", say. A nudge that declares a stage not on the list is held
/// back, so a stage the caller cannot vouch for is one it leaves off and the nudge stays unput.
///
/// A machine with no store yet has no nudge: nothing is counted, and no store is genesised to count it.
#[tauri::command]
pub fn pending_nudges(open_stages: Vec<String>) -> Result<Vec<String>, CmdError> {
    Ok(find_in_store(|store| {
        let due = store.pending_nudges(|stage| open_stages.iter().any(|s| s == stage))?;
        Ok(Some(due.iter().map(|n| n.id.to_string()).collect::<Vec<_>>()))
    })?
    .unwrap_or_default())
}

/// Record that a nudge has been put to the person here.
///
/// The caller calls it **once the nudge is actually on screen**, never when it was judged due: a
/// once-only nudge marked put and never shown is one the person never saw, and the log would have closed
/// it for good.
#[tauri::command]
pub fn mark_nudge_put(nudge_id: String) -> Result<(), CmdError> {
    open_store()?.mark_nudge_put(&nudge_id)?;
    Ok(())
}

/// Count this launch of the app on this device — the tally the launch-shaped metrics are read from
/// (`amenbo_core::nudge::Metric::LaunchCount`, and the day of the first launch).
///
/// Counted here rather than by the front end because a launch is a launch of the *process*: the webview
/// runs its startup effects twice under React StrictMode, and a launch counted twice would carry every
/// threshold to its mark at half the use it was declared to stand for.
///
/// A machine with no store yet counts nothing. There is nowhere to put the tally, and genesising a store
/// to hold one would create the very thing its owner has not asked for; what that costs is the launches
/// before the first store, which is the stretch in which no nudge could be due anyway.
pub fn record_launch() -> Result<(), CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    // The same "is there a store to speak of" test the read entry point makes (`with_store_read`).
    if amenbo_core::env::home().is_none() && !paths.store_file.exists() {
        return Ok(());
    }
    open_store()?.record_launch()?;
    Ok(())
}

/// The comment slot of the inbox, independent of read state: of the open tasks assigned to **my human
/// facet**, return every one that has at least one **comment addressed to me** (something my AI facet
/// said), as `(task_id, unread)`. A task the AI is carrying stays out — its comments are the AI
/// reporting on its own work, which is read by pulling the task, not by being rung about. The GUI
/// unions these into the inbox view. Membership is decided by **the
/// existence of a comment** — marking it read does not remove it; only archiving does — and each
/// task's comments are pulled straight from the read-model over indexed SQL (the single-pass SQL in
/// `store_engine::read::mailbox_comment_tasks`). "Is it me?" is decided on the facet alone (the human
/// facet token `"human"`), and what I said myself, as the human facet, does not count as received.
/// `unread` is an unread flag relative to the per-task last_seen
/// (`ReadReceipts::has_unread_comment`); it is purely for display (the unread dot) and has no say in
/// membership.
#[tauri::command]
pub fn mailbox_comment_tasks() -> Result<Vec<(i64, bool)>, CmdError> {
    let rr = open_store_read()?.read_receipts()?;
    let mut out: Vec<(i64, bool)> = Vec::new();

    let scan = |store: &Store| -> Result<(), CmdError> {
        let me = ActorKind::Human.as_str();
        let read_model = store.read_model();
        for mt in
            amenbo_core::store_engine::read::mailbox_comment_tasks(read_model.conn(), store.reach())?
        {
            let unread = rr.has_unread_comment(
                mt.task_id,
                me,
                mt.comments.iter().map(|(u, h, a)| (u.as_str(), *h, a.as_str())),
            );
            out.push((mt.task_id, unread));
        }
        Ok(())
    };

    with_store_read(scan)?;
    Ok(out)
}

/// Work out, per task_id, when the activity that put the item in the inbox happened (triggeredAt).
/// Two things put an item there: the latest comment from someone other than me (the human facet),
/// and the latest `task.assigned` naming me (a fresh assignment); the later of the two is what the
/// inbox displays and sorts on (matching is on the facet alone). What comes back is the timestamp
/// (RFC3339 UTC) of the most recent such activity, with tasks that have none left out.
/// `amenbo_core::activity::mailbox_triggered_at` folds every inbox id in one go (one pass over the
/// ledger plus one comment query).
#[tauri::command]
pub fn mailbox_triggered_at(task_ids: Vec<i64>) -> Result<Vec<(i64, String)>, CmdError> {
    if task_ids.is_empty() {
        return Ok(Vec::new());
    }
    let want = task_ids.clone();
    let mut out: Vec<(i64, String)> = Vec::new();

    let scan = |store: &Store| -> Result<(), CmdError> {
        let read_model = store.read_model();
        out.extend(amenbo_core::activity::mailbox_triggered_at(
            &store.paths.activity_file,
            read_model.conn(),
            &want,
        )?);
        Ok(())
    };

    with_store_read(scan)?;
    Ok(out)
}

/// Tell the front end, via `store-changed`, that the app's data on disk (`store.sqlite` and its WAL
/// sidecar `-wal`, plus the `config.json` beside them) has moved — this is how writes from another
/// process (the AI, the CLI, another session) reach the GUI.
/// **Waking up is left to the kernel**: the OS-specific watching and coalescing live
/// in [`crate::store_watch`], and all that happens here is "once woken, check whether it really
/// changed, then emit", with `store_signature_string` answering from `PRAGMA data_version` plus the
/// file's identity (file watching also fires on things that mean nothing to us, such as SHM updates
/// from a read, and this gate drops those spurious emits). **There is no payload — it is a wake-up
/// signal saying "something changed", nothing more**: what changed is something the front end learns
/// by reading the **change feed** (`changes_since`, written at the same seam as the write
/// transaction) from its cursor onward, and it refetches only the queries touching those scopes (a
/// watcher looking at a file cannot say which dataset moved). It takes no lock on the store itself
/// (one `PRAGMA` on a read connection contends with no writer), and **this watcher is for rendering
/// only**. The GUI's own writes move the signature too, so the front end holds on to
/// `store_signature` and filters those out.
pub fn watch_store(app: tauri::AppHandle) {
    use tauri::Emitter;

    let mut last = store_signature_string();
    let mut emit_if_changed = move || {
        let cur = store_signature_string();
        if cur == last {
            return;
        }
        last = cur;
        let _ = app.emit("store-changed", ());
    };

    let dir = store_file().and_then(|f| f.parent().map(std::path::Path::to_path_buf));
    crate::store_watch::run(dir.as_deref(), &mut emit_if_changed);
}

/// Create a task (directly under a project, or in the inbox; stamped created_by=human).
/// Classification is added afterwards, on the dimension axes.
///
/// The two days are optional and normally absent — a task is registered with a title and filled in
/// afterwards — but they are taken here as well, so that someone who already knows when the work is due,
/// or when it may start, does not have to file the task and then go back into it to say so.
#[tauri::command]
pub fn task_add(
    project_id: Option<i64>,
    title: String,
    notes: Option<String>,
    due: Option<String>,
    start: Option<String>,
) -> Result<WriteAck, CmdError> {
    let id = with_store_mut(|store| {
        let t = store.add_task(amenbo_core::ops::task::NewTask {
            title,
            project_id,
            due_on: day_arg(due.as_deref())?,
            start_on: day_arg(start.as_deref())?,
            priority: None,
            notes: notes.unwrap_or_default(),
            created_by_kind: Some(ActorKind::Human),
            at_binding_id: None,
        })?;
        emit(store, t.id, amenbo_core::activity_log::event::task_created(&t.title));
        Ok(t.id)
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// Finish creating a task — the second stage of the creation [`task_add`] began (`AMB-D-554`). It clears
/// the fourth premise of `ready` and touches nothing else, so the task stops being held out of the mailbox
/// and out of a reservation, and keeps every edge drawn while it was being put together.
///
/// Already finished is a no-op rather than a refusal, and short-circuited here the way the CLI's
/// `task finish-creating` does it: core would write the same row back, and a ledger row saying nothing
/// changed is not worth keeping. It runs one way — a task filed by mistake ends at [`task_reject`] or
/// [`task_delete`].
#[tauri::command]
pub fn task_finish_creating(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        if store.task(id)?.is_some_and(|t| t.draft) {
            store.finish_task_creation(id, ActorKind::Human)?;
        }
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// Set the status explicitly (done keeps completed in step). Setting the same status again is a
/// no-op, with one exception: `in_progress → in_progress` is never waved through. It goes down to
/// `set_status` so the reservation CAS is not defused, and a second session trying to start the same
/// task is turned away with `AlreadyReserved` (same shape as the CLI; the collision surfaces as a
/// CmdError and reaches the front end's toast).
#[tauri::command]
pub fn task_status(id: i64, status: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let new_status = TaskStatus::parse(&status)
            .ok_or_else(|| format!("status '{status}' is not one of todo / in_progress / done / blocked / rejected"))?;
        let current = store.task(id)?.map(|t| t.status);
        if current != Some(new_status) || new_status == TaskStatus::InProgress {
            let old = current.unwrap_or_default();
            store.set_task_status(id, new_status, ActorKind::Human)?;
            emit(store, id, amenbo_core::activity_log::event::task_status_changed(old.as_str(), new_status.as_str()));
        }
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// End a task that will not be done, with the reasoning kept (`AMB-D-397`) — the same shape as the
/// CLI's `task reject <id> --reason <why>`. `task_status` above can reach `rejected` too, and this
/// exists for what that path cannot ask for: **the reason, which is required**. It is the part worth
/// keeping when a task is closed unfinished, and it lands as a comment on the timeline rather than a
/// field of its own — free text keeps its one home, exactly as the CLI has it.
///
/// The pull-down is the GUI's only door to this status, and it collects the reason before it calls,
/// so an empty one is a slip rather than a choice: it is refused here as well, and nothing is written.
#[tauri::command]
pub fn task_reject(id: i64, reason: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(amenbo_core::Error::invalid("a rejection needs its reason — say why the task will not be done")
            .into());
        }
        let old = store.task(id)?.map(|t| t.status).unwrap_or_default();
        if old == TaskStatus::Rejected {
            // Idempotent, and the reason is not piled on: a re-reject changes nothing, so it has
            // nothing new to explain (the CLI's `task reject` and `decision reject` behave the same).
            return Ok(());
        }
        store.set_task_status(id, TaskStatus::Rejected, ActorKind::Human)?;
        emit(store, id, amenbo_core::activity_log::event::task_status_changed(old.as_str(), TaskStatus::Rejected.as_str()));
        store.add_task_comment(id, ActorKind::Human, reason)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// Delete a task for good (core's rules decide how the subtree is swept). Same shape as the CLI's
/// `task delete`. The GUI's actor is always human, so the guardrail aimed at the AI — the limit on
/// deleting human-created tasks — never applies. The confirmation dialog is the UI's job.
#[tauri::command]
pub fn task_delete(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.delete_task(id, ActorKind::Human)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// Add a comment (facet = human).
#[tauri::command]
pub fn comment_add(task_id: i64, text: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.add_task_comment(task_id, ActorKind::Human, &text)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(task_id))
}

/// Take back a comment posted by mistake — delete it for good (same shape as the CLI's
/// `comment rm`). Any attachments on it go with it (core's delete op sweeps them). The confirmation
/// dialog is the UI's job.
#[tauri::command]
pub fn comment_remove(id: i64, task_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.remove_task_comment(id, ActorKind::Human)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(task_id))
}

/// Rewrite a comment's body in place (same shape as the CLI's `comment edit`). It is not a repost,
/// so the id, the position in the timeline and the attachments all stay. Overwriting alone needs no
/// confirmation dialog (taking a comment back does).
#[tauri::command]
pub fn comment_edit(id: i64, task_id: i64, text: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.edit_task_comment(id, &text)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(task_id))
}

/// Shape one read-model row (`CommentRow`) into a decision comment DTO.
fn decision_comment_dto_from_row(
    row: amenbo_core::store_engine::read::CommentRow,
    config: &amenbo_core::config::Config,
) -> DecisionCommentDto {
    let normalize = |s: &str| Timestamp::parse_rfc3339(s).map(|ts| ts.to_rfc3339_z()).unwrap_or_default();
    let author_kind = row.author_kind.as_deref().and_then(ActorKind::parse);
    DecisionCommentDto {
        id: row.id,
        at: normalize(&row.created_at),
        author: facet_actor(config, author_kind),
        text: row.text,
        edited_at: row.edited_at.as_deref().map(normalize),
    }
}

/// One decision's live comments, oldest first, for the thread in the decision detail pane. Like
/// `task_activity`, it queries the read-model directly, per decision, bypassing the window. Empty if
/// the decision is not found.
#[tauri::command]
pub fn decision_comments(decision_id: i64) -> Result<Vec<DecisionCommentDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("decision_comments");
    let found = find_in_store(|store| {
        let read_model = store.read_model();
        let conn = read_model.conn();
        if amenbo_core::store_engine::read::decision_title(conn, decision_id)?.is_none() {
            return Ok(None);
        }
        let rows = amenbo_core::store_engine::read::decision_comment_list(conn, decision_id)?;
        let dtos = rows
            .into_iter()
            .map(|r| decision_comment_dto_from_row(r, &store.config))
            .collect::<Vec<_>>();
        Ok(Some(dtos))
    })?;
    Ok(found.unwrap_or_default())
}

/// Add a comment to a decision record (facet = human). The decision twin of the task's
/// [`comment_add`], writing to the dedicated `decision_comment` table. The reason comment attached
/// when accepting or rejecting is thin sugar over the same path — the front end composes it and adds
/// one comment here.
#[tauri::command]
pub fn decision_comment_add(decision_id: i64, text: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.add_decision_comment(decision_id, ActorKind::Human, &text)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(decision_id))
}

/// Take back a decision comment — delete it for good (the decision twin of [`comment_remove`]).
#[tauri::command]
pub fn decision_comment_remove(id: i64, decision_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.remove_decision_comment(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(decision_id))
}

/// Rewrite a decision comment's body in place (the decision twin of [`comment_edit`]).
#[tauri::command]
pub fn decision_comment_edit(id: i64, decision_id: i64, text: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.edit_decision_comment(id, &text)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(decision_id))
}

/// The live attachments of a target (task/decision), in the order they were attached. A direct
/// read-model query, O(result).
#[tauri::command]
pub fn attachments_for(target_type: String, target_id: i64) -> Result<Vec<AttachmentDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("attachments_for");
    let Some(target_type) = amenbo_core::model::AttachmentTarget::parse(&target_type) else {
        return Err(format!("attachment target '{target_type}' is not one of task / decision / task_comment / decision_comment").into());
    };
    let found = find_in_store(|store| {
        let read_model = store.read_model();
        let conn = read_model.conn();
        let rows = amenbo_core::store_engine::read::attachments_for_target(conn, target_type, target_id)?;
        if rows.is_empty() {
            return Ok(None);
        }
        let blobs = store.blobs();
        let dtos = rows
            .into_iter()
            .map(|r| {
                let present = r.blob_hash.as_deref().is_some_and(|h| blobs.path(h).is_some());
                AttachmentDto {
                    id: r.id,
                    kind: r.kind,
                    blob_hash: r.blob_hash,
                    filename: r.filename,
                    mime: r.mime,
                    size_bytes: r.size_bytes,
                    url: r.url,
                    present,
                    created_by_kind: r.created_by_kind,
                }
            })
            .collect::<Vec<_>>();
        Ok(Some(dtos))
    })?;
    Ok(found.unwrap_or_default())
}

/// Ingest a file as a blob and attach it to a task or decision record. Same shape as the CLI's
/// `task/decision attach`: check the per-file size cap, ingest content-addressed, record the
/// metadata. The MIME type is guessed from the extension.
#[tauri::command]
pub fn attachment_add(
    target_type: String,
    target_id: i64,
    path: String,
) -> Result<WriteAck, CmdError> {
    let target = amenbo_core::model::AttachmentTarget::parse(&target_type)
        .ok_or_else(|| format!("attachment target '{target_type}' is not one of task / decision / task_comment / decision_comment"))?;
    with_store_mut(|store| {
        let src = std::path::Path::new(&path);
        let meta = std::fs::metadata(src).map_err(|e| format!("cannot read the file '{path}': {e}"))?;
        if !meta.is_file() {
            return Err(format!("'{path}' is not a regular file").into());
        }
        let filename = src
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("attachment")
            .to_string();
        let mime = amenbo_core::blob::mime_from_filename(&filename);
        store.config.attachment_limits.check_per_file(mime, meta.len())?;
        let blob = store.blobs().ingest_path(src)?;
        store.attach_blob(
            target,
            target_id,
            &blob.hash,
            &filename,
            mime,
            blob.size_bytes as i64,
            ActorKind::Human,
        )?;
        Ok(())
    })?;
    let scope: &[&'static str] = if target == amenbo_core::model::AttachmentTarget::Decision {
        &["decisions"]
    } else {
        &["tasks"]
    };
    let ack = WriteAck::new(scope);
    Ok(if target == amenbo_core::model::AttachmentTarget::Decision {
        ack.decision(target_id)
    } else {
        ack.task(target_id)
    })
}

/// Ingest raw bytes as a blob and attach them. HTML5 drag-and-drop inside the webview cannot give us
/// an OS path (`dragDropEnabled:false` is the setting that lets card drag-and-drop on the board work
/// at all), so the front end reads the dropped File itself and hands the bytes over this path. Large
/// files are better off going through the file picker ([`attachment_add`] takes a path and ingests
/// as a stream). The body is the same as [`attachment_add`]: check the cap, ingest
/// content-addressed, record the metadata.
#[tauri::command]
pub fn attachment_add_bytes(
    target_type: String,
    target_id: i64,
    filename: String,
    bytes: Vec<u8>,
) -> Result<WriteAck, CmdError> {
    let target = amenbo_core::model::AttachmentTarget::parse(&target_type)
        .ok_or_else(|| format!("attachment target '{target_type}' is not one of task / decision / task_comment / decision_comment"))?;
    let filename = if filename.trim().is_empty() { "attachment".to_string() } else { filename };
    with_store_mut(|store| {
        let mime = amenbo_core::blob::mime_from_filename(&filename);
        store.config.attachment_limits.check_per_file(mime, bytes.len() as u64)?;
        let blob = store.blobs().ingest_bytes(&bytes)?;
        store.attach_blob(
            target,
            target_id,
            &blob.hash,
            &filename,
            mime,
            blob.size_bytes as i64,
            ActorKind::Human,
        )?;
        Ok(())
    })?;
    let scope: &[&'static str] = if target == amenbo_core::model::AttachmentTarget::Decision {
        &["decisions"]
    } else {
        &["tasks"]
    };
    let ack = WriteAck::new(scope);
    Ok(if target == amenbo_core::model::AttachmentTarget::Decision {
        ack.decision(target_id)
    } else {
        ack.task(target_id)
    })
}

/// Open a url-mode attachment in the OS's default application — "open externally". A url is the only
/// kind that has anywhere to be opened: a blob is a file, so it is written where the user asked for it
/// (`attachment_save`) rather than into a temp copy they cannot find again. The front end passes the
/// DTO's `url` through as it is (no need to resolve the id again). Even though the entry point
/// (`ops::attachment::add_url`) admits web schemes only, the scheme is checked again right before
/// opening: rows written before that validation existed still come through here, and an OS opener
/// will interpret whatever it is handed (`file:` is a local file; a leading `-` is an option to the
/// command).
#[tauri::command]
pub fn attachment_open(url: String) -> Result<(), CmdError> {
    let url = url.trim().to_string();
    if !amenbo_core::ops::attachment::is_web_url(&url) {
        return Err(format!("this URL cannot be opened (http, https and mailto only): {url}").into());
    }
    os_open(&url).map_err(|e| format!("cannot open '{url}': {e}").into())
}

/// Write a blob attachment to the path the user picked — "download", and the only way to take an
/// attachment out of the store as a file the user keeps (`export` writes the whole device, which is
/// data sovereignty, not "I want this one file"). The destination is the user's own choice — somewhere
/// they picked and can find again — so it is written with ordinary permissions. The front end passes
/// the DTO's `blobHash` through as it is, and has already resolved `dest` through the OS save dialog,
/// which is where overwrite confirmation happens.
#[tauri::command]
pub fn attachment_save(blob_hash: String, dest: String) -> Result<(), CmdError> {
    let bytes = blob_bytes(&blob_hash)?;
    std::fs::write(&dest, &bytes).map_err(|e| format!("cannot write to '{dest}': {e}").into())
}

/// A blob's contents, by hash, out of this device's blob store — the read both attachment faces
/// (open externally, download) start from. A hash whose bytes are not here is a miss, not an error
/// in the store: attachment rows travel while blobs are fetched separately.
fn blob_bytes(hash: &str) -> Result<Vec<u8>, CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    let blobs =
        amenbo_core::blob::BlobStore::at(paths.base_dir.join(amenbo_core::blob::BLOBS_SUBDIR));
    blobs.read(hash).map_err(CmdError::from)
}

/// Open a path or URL in the OS's default application (macOS `open`, Windows `cmd /C start`,
/// otherwise `xdg-open`). Same shape as the CLI helper of the same name — a minimal copy, since the
/// GUI cannot reuse it.
fn os_open(target: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = amenbo_core::sys::command("open");
        c.arg(target);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = amenbo_core::sys::command("cmd");
        c.args(["/C", "start", "", target]);
        c
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut cmd = {
        let mut c = amenbo_core::sys::command("xdg-open");
        c.arg(target);
        c
    };
    cmd.status().map(|_| ())
}

/// One of the "what next" affordances on the project-created screen: open the bound folder in the
/// OS's file manager (Finder on macOS, Explorer on Windows, the default file manager elsewhere).
/// Handing `os_open` a folder path opens that folder. If the folder is not there, fail with a
/// message that says so — it may have been dropped after we got hold of it.
#[tauri::command]
pub fn reveal_folder(path: String) -> Result<(), CmdError> {
    if !std::path::Path::new(&path).is_dir() {
        return Err(format!("folder not found: {path}").into());
    }
    os_open(&path).map_err(|e| format!("cannot open '{path}': {e}").into())
}

/// One of the "what next" affordances on the project-created screen: open the bound folder in a
/// terminal. Launches the terminal application per OS (`open -a Terminal` on macOS, `cmd start` on
/// Windows, `x-terminal-emulator` elsewhere). Where there is no terminal (on Linux, say, if none is
/// installed) this is best-effort — a failure to launch simply comes back as the error.
#[tauri::command]
pub fn open_terminal(path: String) -> Result<(), CmdError> {
    if !std::path::Path::new(&path).is_dir() {
        return Err(format!("folder not found: {path}").into());
    }
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = amenbo_core::sys::command("open");
        c.args(["-a", "Terminal", &path]);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = amenbo_core::sys::command("cmd");
        c.args(["/C", "start", "", "cmd", "/K", "cd", "/d", &path]);
        c
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut cmd = {
        let mut c = amenbo_core::sys::command("x-terminal-emulator");
        c.current_dir(&path);
        c
    };
    cmd.status()
        .map(|_| ())
        .map_err(|e| format!("cannot open a terminal: {e}").into())
}

/// Delete an attachment for good. The blob's bytes stay until nothing references them and are
/// reclaimed on a separate path (GC).
/// Invalidates the affected target (task/decision) so any open detail view refetches.
#[tauri::command]
pub fn attachment_remove(
    id: i64,
    target_type: String,
    target_id: i64,
) -> Result<WriteAck, CmdError> {
    let target = amenbo_core::model::AttachmentTarget::parse(&target_type)
        .ok_or_else(|| format!("attachment target '{target_type}' is not one of task / decision / task_comment / decision_comment"))?;
    with_store_mut(|store| {
        store.remove_attachment(id)?;
        Ok(())
    })?;
    let scope: &[&'static str] = if target == amenbo_core::model::AttachmentTarget::Decision {
        &["decisions"]
    } else {
        &["tasks"]
    };
    let ack = WriteAck::new(scope);
    Ok(if target == amenbo_core::model::AttachmentTarget::Decision {
        ack.decision(target_id)
    } else {
        ack.task(target_id)
    })
}

/// A task's recorded commit SHAs, oldest first. A direct read-model query; empty if the task has none.
#[tauri::command]
pub fn task_commits(task_id: i64) -> Result<Vec<TaskCommitDto>, CmdError> {
    let _perf = amenbo_core::perf::Timer::start("task_commits");
    let found = find_in_store(|store| {
        let rows = store.task_commits(task_id)?;
        if rows.is_empty() {
            return Ok(None);
        }
        let dtos = rows
            .into_iter()
            .map(|r| TaskCommitDto {
                id: r.id,
                sha: r.sha,
                created_by_kind: r.created_by_kind.map(|k| k.as_str().to_string()),
            })
            .collect::<Vec<_>>();
        Ok(Some(dtos))
    })?;
    Ok(found.unwrap_or_default())
}

/// Record a commit SHA on a task. Same shape as the CLI's `task commit add`: the SHA is validated
/// and normalised at the ops door (full-length lower-case hex only; case folded), and a SHA already
/// on the task is a no-op. Invalidates the task so any open detail view refetches.
#[tauri::command]
pub fn task_commit_add(task_id: i64, sha: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.add_task_commit(task_id, &sha, Some(ActorKind::Human))?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(task_id))
}

/// Forget a commit SHA on a task (a hard delete; idempotent — a SHA not recorded is a no-op). The
/// commit itself and the task are untouched.
#[tauri::command]
pub fn task_commit_remove(task_id: i64, sha: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.remove_task_commit(task_id, &sha)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(task_id))
}

/// Record a decision (Proposed), created under `project_id`. The GUI's actor is always human.
#[tauri::command]
pub fn decision_add(
    project_id: i64,
    title: String,
    body: Option<String>,
    dimension_value_ids: Option<Vec<i64>>,
) -> Result<WriteAck, CmdError> {
    let id = with_store_mut(|store| {
        // The classification rides in the create's own transaction, the way `task add --dim` does it:
        // a decision filed under an axis can never commit without it, and a refused create leaves no
        // half-classified decision behind.
        let d = store.add_decision_with_dimensions(amenbo_core::ops::decision::NewDecision {
            title, body: body.unwrap_or_default(), project_id,
        }, &dimension_value_ids.unwrap_or_default())?;
        Ok(d.id)
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(id))
}

/// Accept a decision (Proposed → Accepted). decided_by is me.
#[tauri::command]
pub fn decision_accept(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let by = ActorKind::Human.as_str().to_string();
        store.accept_decision(id, Some(by), ActorKind::Human)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(id))
}

/// Reject a decision (Proposed → Rejected).
#[tauri::command]
pub fn decision_reject(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.reject_decision(id, ActorKind::Human)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(id))
}

/// Put an accepted decision back under discussion (Accepted → Proposed, clearing decided_*). The
/// sanctioned way to fix a minor flaw without dirtying the supersession chain, while keeping the
/// freeze meaningful. Non-destructive, reversible, auditable.
#[tauri::command]
pub fn decision_reopen(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.reopen_decision(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(id))
}

/// Edit a decision's title/body in place — proposed or accepted alike (`AMB-D-363`); rejected is terminal.
#[tauri::command]
pub fn decision_edit(id: i64, title: Option<String>, body: Option<String>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.update_decision(id, amenbo_core::ops::decision::DecisionPatch { title, body })?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(id))
}

/// Have decision `new_id` replace `old_id` (the supersession chain).
#[tauri::command]
pub fn decision_supersede(new_id: i64, old_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let by = ActorKind::Human.as_str().to_string();
        store.supersede_decision(new_id, old_id, Some(by), ActorKind::Human)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(new_id).decision(old_id))
}

/// Have decision `new_id` partially revise `old_id` (amends — the target stays current).
#[tauri::command]
pub fn decision_amend(new_id: i64, old_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.amend_decision(new_id, old_id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(new_id).decision(old_id))
}

/// Record that decision `new_id` builds on `old_id` (builds_on). Both decisions stay current; all
/// that changes is the order they should be read in and the blast radius.
#[tauri::command]
pub fn decision_builds_on(new_id: i64, old_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.decision_builds_on(new_id, old_id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(new_id).decision(old_id))
}

/// Remove one edge between decisions (all three kinds share this). The edge is named by its pair —
/// `decision_edge_pair` is UNIQUE, so the kind is not needed. This corrects wiring that was drawn by
/// mistake; it does not undo a decision — remove a `supersedes` edge and the target simply becomes
/// current again, since currency is a derived projection (there is nothing to clean up after).
#[tauri::command]
pub fn decision_unlink_edge(decision_id: i64, target_decision_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.unlink_decision_edge(decision_id, target_decision_id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(decision_id).decision(target_decision_id))
}

/// Link a decision to a task, or unlink it (`link=false` unlinks). The editing affordance behind
/// cross-links.
#[tauri::command]
pub fn decision_set_link(decision_id: i64, task_id: i64, link: bool) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        if link {
            store.link_decision(decision_id, task_id)?;
        } else {
            store.unlink_decision(decision_id, task_id)?;
        }
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions", "tasks"]).decision(decision_id).task(task_id))
}

/// Promote a task comment (task_comment) into a decision: the comment's text becomes the body, the
/// task's project becomes the decision's project, and the new decision is linked back to that task.
#[tauri::command]
pub fn decision_promote(comment_id: i64, title: String) -> Result<WriteAck, CmdError> {
    let (decision_id, task_id) = with_store_mut(|store| {
        let c = store.task_comment(comment_id)?
            .ok_or_else(|| format!("comment '{comment_id}' was not found"))?;
        let task_id = c.task_id;
        let body = c.text.clone();
        let project_id = store.task(task_id)?
            .and_then(|t| t.project_id)
            .ok_or_else(|| "the comment's task belongs to no project".to_string())?;
        let d = store.add_decision(amenbo_core::ops::decision::NewDecision {
            title, body, project_id,
        })?;
        let did = d.id;
        store.link_decision(did, task_id)?;
        Ok((did, task_id))
    })?;
    Ok(WriteAck::new(&["decisions", "tasks"]).decision(decision_id).task(task_id))
}

/// Set or edit a task's description (notes, Markdown); an empty string clears it. Like core's
/// `task::update`, it emits no system event (same shape as the CLI's `task update`). The watcher
/// picks the change up, so other sessions see it too.
#[tauri::command]
pub fn task_set_notes(id: i64, notes: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.update_task(id, amenbo_core::ops::task::TaskPatch {
            notes: Some(notes),
            ..Default::default()
        })?;
        Ok(())
    })?;
    Ok(WriteAck::new(&[]).task(id))
}

/// Set or edit a task's title. Like core's `task::update`, an empty title is refused by core, and no
/// system event is emitted (same shape as the CLI's `task update --title`). The title also shows on
/// the list cards, so we raise the tasks scope and let the board and list refetch too.
#[tauri::command]
pub fn task_set_title(id: i64, title: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.update_task(id, amenbo_core::ops::task::TaskPatch {
            title: Some(title),
            ..Default::default()
        })?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// Read a day off this face: nothing at all, and the empty string a cleared date input sends, both mean
/// "no day"; anything else goes through core's own parser, so the GUI reads a date exactly as the CLI's
/// `--due` / `--start` does — a calendar day, or a relative form counted from this machine's own today
/// (`AMB-D-429`).
fn day_arg(input: Option<&str>) -> Result<Option<NaiveDate>, CmdError> {
    match input.map(str::trim) {
        None | Some("") => Ok(None),
        Some(s) => Ok(Some(amenbo_core::time::parse_date(s, amenbo_core::time::today())?)),
    }
}

/// Set the due date; due=None (or the empty string) clears it. Same shape as the CLI's
/// `task update --due/--clear-due`. The date shows on the list cards, so the tasks scope is raised and
/// the board and list refetch too.
#[tauri::command]
pub fn task_set_due(id: i64, due: Option<String>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let patch = match day_arg(due.as_deref())? {
            None => amenbo_core::ops::task::TaskPatch {
                clear_due: true,
                ..Default::default()
            },
            Some(d) => amenbo_core::ops::task::TaskPatch {
                due_on: Some(d),
                ..Default::default()
            },
        };
        store.update_task(id, patch)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// Set the start day; start=None (or the empty string) clears it. Same shape as the CLI's
/// `task update --start/--clear-start`. A day still ahead is the third premise of `ready`, so this write
/// moves whether the task can be reserved at all — the tasks scope is raised for that as much as for the
/// chip the cards draw from it.
#[tauri::command]
pub fn task_set_start(id: i64, start: Option<String>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let patch = match day_arg(start.as_deref())? {
            None => amenbo_core::ops::task::TaskPatch {
                clear_start: true,
                ..Default::default()
            },
            Some(d) => amenbo_core::ops::task::TaskPatch {
                start_on: Some(d),
                ..Default::default()
            },
        };
        store.update_task(id, patch)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// Change the priority; priority=None clears it. Same shape as the CLI's
/// `task update --priority/--clear-priority`.
#[tauri::command]
pub fn task_set_priority(id: i64, priority: Option<String>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let patch = match priority.as_deref() {
            None | Some("") => amenbo_core::ops::task::TaskPatch {
                clear_priority: true,
                ..Default::default()
            },
            Some(p) => {
                let pri = match p {
                    "high" => Priority::High,
                    "medium" => Priority::Medium,
                    "low" => Priority::Low,
                    other => return Err(format!("priority '{other}' is not one of high / medium / low").into()),
                };
                amenbo_core::ops::task::TaskPatch {
                    priority: Some(pri),
                    ..Default::default()
                }
            }
        };
        store.update_task(id, patch)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// Create **one project row** in the store. What it brings into being is a project, not a store — it
/// doubles as genesis only on a machine that has no store yet (the GUI's first launch). The GUI raises
/// projects one way, [`project_add_folder`], and this is the row half of it. Returns `(the still-open,
/// already-saved store, project_id)`.
fn provision_project(name: &str) -> Result<(Store, i64), CmdError> {
    ensure_migrated()?;
    let paths = amenbo_core::config::Paths::resolve()?;
    let mut store = if amenbo_core::store_engine::probe_is_populated(&paths.store_file) {
        amenbo_core::store::Store::open_at(paths)?
    } else {
        amenbo_core::store::Store::init(paths, None)?
    };
    let pname = {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            amenbo_core::config::default_project_name(store.config.language.as_deref())
        } else {
            trimmed.to_string()
        }
    };
    // The GUI's creation screen asks for a name and nothing else, so the view a new project opens on
    // is the configured `default_view` — the same answer the CLI gives when `--view` is omitted.
    let project = store.project_add(amenbo_core::ops::project::NewProject {
        name: pname,
        view: store.config.default_view,
        notes: String::new(),
        color: None,
    })?;
    let project_id = project.id;
    store.save_config()?;
    Ok((store, project_id))
}

/// Turn the chosen folder into **a new Amenbo project**. The flow: (1) if a `.amenbo` already exists
/// (in the folder or above it), refuse and respect what is there (`init_pointer_exists`); (2) if
/// there is no `.amenbo` but an Amenbo managed block is present, **do not refuse on the marker
/// alone** — look up living projects in the bindings registry and branch (the same shape as guard 2
/// in the CLI's `init`): exactly one living project means the pointer was lost and we **recover** it
/// (`recover_lost_pointer`); several means `init_ambiguous_owners`, offering the candidates; none
/// means carry on; (3) bring one project into being; (4) write the `.amenbo` pointer into the
/// folder, record it in the reference registry, and upsert the managed block in the AI guidance
/// files (AGENTS.md / CLAUDE.md). **The folder's own contents (the source) are never touched** — all
/// we place there is `.amenbo` and the managed block of guidance. The project's name is the `name`
/// the creation screen passes, falling back to the folder's basename if it is omitted. A marker is
/// thin and is no proof of ownership (it is a borrowed surface, carried along by clones, copies and
/// sync), so the truth about ownership is taken from Amenbo's own artifacts (`.amenbo` plus the
/// bindings registry) — and the reverse lookup counts **only the projects that still read back**: a
/// deleted project's rows are physically gone, while the teardown that forgets its bindings entry is
/// best-effort, so an entry can outlive the project it names. Recover onto one of those and the folder
/// is bound to an id that names nothing, leaving nothing at all in the sidebar. Once the pointer is
/// written, always call `claim_project_ref` (the project → folders index); forget it and the folder
/// you just bound goes missing from the list on the settings screen. It is a *claim* because the
/// pointer names one project: the records other projects held for this folder are retracted with it.
#[tauri::command]
pub fn project_add_folder(dir: String, name: Option<String>) -> Result<WriteAck, CmdError> {
    let path = std::path::Path::new(&dir);
    if let Some((bound_dir, _)) = amenbo_core::binding::find_upward(path) {
        return Err(CmdError::coded(
            "init_pointer_exists",
            format!(
                "this folder (or an ancestor) is already bound to an Amenbo project: {}",
                bound_dir.display()
            ),
            serde_json::json!({ "path": bound_dir.display().to_string() }),
        ));
    }
    if amenbo_core::agents::dir_has_managed_block(path) {
        let owners: Vec<i64> = match open_store_read() {
            Ok(store) => amenbo_core::binding::live_projects_claiming(&store, path),
            Err(_) => Vec::new(),
        };
        match owners.as_slice() {
            [project_id] => {
                return recover_lost_pointer(path, *project_id);
            }
            many if many.len() > 1 => {
                let candidates =
                    many.iter().map(|pid| pid.to_string()).collect::<Vec<_>>().join(", ");
                return Err(CmdError::coded(
                    "init_ambiguous_owners",
                    format!(
                        "several living projects claim this folder, so the lost pointer can't be recovered unambiguously (candidates: {candidates}): {}",
                        path.display()
                    ),
                    serde_json::json!({
                        "path": path.display().to_string(),
                        "candidates": candidates,
                    }),
                ));
            }
            _ => {}
        }
    }
    let name = name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| {
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string()
        });
    let (store, project_id) = provision_project(&name)?;
    amenbo_core::binding::pointer_for(&store, project_id).write(path)?;
    let mut registry = store.bindings();
    registry.claim_project_ref(project_id, path.to_string_lossy());
    let _ = store.save_bindings(&registry);
    amenbo_core::agents::upsert_into_dir(
        path,
        store.config.language.as_deref(),
        amenbo_core::config::Paths::command_name(),
    );
    Ok(WriteAck::new(&["tasks"]))
}

/// When the `.amenbo` is gone but the bindings registry's reverse lookup names **exactly one living
/// project** as this folder's owner, **recover** the pointer rather than quietly creating a new
/// project — the same shape as the CLI `init` helper of the same name, equivalent to
/// `bind --project`. Rewrites the pointer and the bindings index, and upserts the managed block
/// idempotently (everything outside the markers is preserved).
fn recover_lost_pointer(path: &std::path::Path, project_id: i64) -> Result<WriteAck, CmdError> {
    let store = open_store()?;
    amenbo_core::binding::pointer_for(&store, project_id).write(path)?;
    {
        let mut reg = store.bindings();
        reg.claim_project_ref(project_id, path.to_string_lossy());
        let _ = store.save_bindings(&reg);
    }
    amenbo_core::agents::upsert_into_dir(
        path,
        store.config.language.as_deref(),
        amenbo_core::config::Paths::command_name(),
    );
    Ok(WriteAck::new(&["tasks"]))
}

/// Return one project's editable fields (name/notes/color/view/archived) straight from the
/// read-model, to prefill the project settings screen. Archived projects are returned too (this
/// screen is where they get unarchived). A project that is not found (deleted, say) yields a coded
/// error.
#[tauri::command]
pub fn project_get(project_id: i64) -> Result<ProjectSettingsDto, CmdError> {
    let store = open_store_read()?;
    let read_model = store.read_model();
    let row = amenbo_core::store_engine::read::project_settings(read_model.conn(), project_id)?
        .ok_or_else(|| {
            amenbo_core::Error::not_found(format!("project '{project_id}' not found"))
        })?;
    Ok(ProjectSettingsDto {
        id: row.id,
        name: row.name,
        notes: row.notes,
        color: row.color.unwrap_or_else(|| "#9aa7b2".to_string()),
        view: row.default_view,
        archived: row.archived,
    })
}

/// Return the archived (but not deleted) projects straight from the read-model, for the sidebar's
/// "Archived" section. Complementary to the snapshot that supplies the active sidebar list
/// (`project_overview`, which is `archived = 0`): no project ever appears in both. Most recently
/// updated first, with id as a stable tiebreak.
#[tauri::command]
pub fn project_list_archived() -> Result<Vec<ArchivedProjectDto>, CmdError> {
    let store = open_store_read()?;
    let read_model = store.read_model();
    let rows = amenbo_core::store_engine::read::archived_projects(read_model.conn())?;
    Ok(rows
        .into_iter()
        .map(|r| ArchivedProjectDto {
            id: r.id,
            name: r.name,
            color: r.color.unwrap_or_else(|| "#9aa7b2".to_string()),
        })
        .collect())
}

/// Update a project's settings — rename, notes, color, default view (same shape as the CLI's
/// `project update`). Only the fields that were passed are changed; None leaves a field alone.
/// `view` arrives as an enum string (list/board/calendar/timeline), and anything else is an error.
#[tauri::command]
pub fn project_update(
    project_id: i64,
    name: Option<String>,
    notes: Option<String>,
    color: Option<String>,
    view: Option<String>,
) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let view = match view {
            Some(v) => Some(
                amenbo_core::model::View::parse(&v)
                    .ok_or_else(|| format!("view '{v}' is not one of list / board / calendar / timeline"))?,
            ),
            None => None,
        };
        store.project_update(
            project_id,
            amenbo_core::ops::project::ProjectPatch { name, notes, view, color, ..Default::default() },
        )?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Reorder a project (the command/data layer the sidebar's drag-and-drop rests on). `position` is
/// one of `top`, `bottom`, `before`, `after`, and `before`/`after` need an `anchor_id` (the project
/// to sit next to) — drag-and-drop resolves its drop target mainly through those two. The order is
/// nothing but `project.order_key` in the single DB (the sidebar list draws in that order), and
/// `Store::project_move` (→ `ops::project::move_to`) resolves the anchor on `order_key` and computes
/// the key that goes between. The anchor is another project id in the same `project` table, and
/// always resolves.
#[tauri::command]
pub fn project_move(
    project_id: i64,
    position: String,
    anchor_id: Option<i64>,
) -> Result<WriteAck, CmdError> {
    let pos = match position.as_str() {
        "top" => amenbo_core::ops::Position::Top,
        "bottom" => amenbo_core::ops::Position::Bottom,
        "before" => amenbo_core::ops::Position::Before(
            anchor_id.ok_or("position 'before' needs an anchor_id to place against")?,
        ),
        "after" => amenbo_core::ops::Position::After(
            anchor_id.ok_or("position 'after' needs an anchor_id to place against")?,
        ),
        other => return Err(format!("position '{other}' is not one of top / bottom / before / after").into()),
    };
    with_store_mut(|store| {
        store.project_move(
            project_id, pos)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Archive or unarchive a project (same shape as the CLI's `project archive` / `unarchive`).
/// Archiving takes it out of the sidebar list (`project_overview` — live and not archived).
#[tauri::command]
pub fn project_set_archived(project_id: i64, archived: bool) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.project_set_archived(
            project_id, archived)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Delete a project **destructively** (same shape as the CLI's `project delete`). Its tasks, decisions
/// and dimensions are physically deleted with it — the op walks the subtree child-first, the schema
/// refusing to let a project go out from under a surviving child — and the `.amenbo`, the managed block
/// and the bindings entry of every folder **still pointing at it** are released (a folder re-pointed at
/// another project is that project's, and keeps all three). Keeping it around but out of sight
/// is archiving's job
/// ([`project_set_archived`]).
#[tauri::command]
pub fn project_delete(project_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.project_delete(project_id, ActorKind::Human)?;
        Ok(())
    })?;
    if let Ok(store) = open_store() {
        let _ = amenbo_core::project_teardown::teardown_deleted_project(&store, project_id);
    }
    Ok(WriteAck::new(&["tasks"]))
}

/// Folder management on the project settings screen: the folders bound to this project (folders
/// whose `.amenbo` points at it), found by reverse lookup — many folders to one project. Reads
/// `project_dirs` and flags each folder for existence (stale, moved or deleted folders come back
/// with `exists:false`). Same shape as `bound_folders` in the CLI's `project show`. Each row checks
/// its own `.amenbo` against the store, and carries a `mismatch` when the recorded slug disagrees
/// with reality — "this pointer belongs to a different store (its id may quietly name something
/// else)". **The listing is never blocked**; the id is authoritative. A folder with no pointer at
/// all comes back on the same row as `pointer_missing` (the registry still names this project, so it
/// shows up in the list, yet an AI started in that folder will not resolve here). That verdict comes
/// from core's shared path [`amenbo_core::binding::is_pointer_missing`], the same one behind the
/// CLI's `doctor` and `project show`. A pointer in the old format (`project_id` unreadable) comes
/// back as `legacy` on the same row: in the CLI, running a command in that folder lets
/// `resolve_upward` upgrade it automatically, but the GUI has no cwd and so the upgrade never gets
/// its chance. We surface it here and steer the user to a relink.
///
/// A pointer another store wrote comes back as `foreign` (`AMB-D-685`). It is read out of the
/// folder's own `.amenbo`, exactly as `mismatch` is, and not through
/// [`amenbo_core::binding::foreign_pointer`]: that one answers for a *starting point* and walks
/// upward, so a row whose own pointer is missing would be handed an ancestor's verdict and reported
/// as another store's when nothing here says so. The judgement itself is core's either way
/// ([`amenbo_core::binding::DirBinding::mismatched_store`]).
#[tauri::command]
pub fn project_bound_folders(project_id: i64) -> Result<Vec<BoundFolderDto>, CmdError> {
    let store = open_store_read()?;
    let registry = store.bindings();
    Ok(registry
        .dirs_for_project(project_id)
        .into_iter()
        .map(|dir| {
            let path = std::path::Path::new(dir);
            let exists = path.is_dir();
            let pointer = amenbo_core::binding::read_pointer(path);
            let mismatch = pointer
                .as_ref()
                .and_then(|b| amenbo_core::binding::slug_mismatch(&store, b))
                .map(|m| SlugMismatchDto {
                    project_id: m.project_id,
                    recorded: m.recorded,
                    actual: m.actual,
                });
            let foreign = pointer.as_ref().and_then(|b| b.mismatched_store()).map(|recorded| ForeignStoreDto {
                recorded: recorded.to_string(),
                running: amenbo_core::config::Paths::APP_NAME.to_string(),
            });
            let legacy = amenbo_core::binding::is_legacy_pointer(path);
            let pointer_missing = amenbo_core::binding::is_pointer_missing(path);
            BoundFolderDto { path: dir.to_string(), exists, mismatch, legacy, pointer_missing, foreign }
        })
        .collect())
}

/// Folder management on the project settings screen: bind an existing folder to this **existing
/// project** (the Tauri path for `bind --project`). Places `.amenbo` in the folder, records it in
/// the store's binding table (project_dirs), and upserts the managed block in the
/// AI guidance files (AGENTS.md / CLAUDE.md). **The folder's own contents (the source) are never
/// touched.** The nested-binding guard — refuse when an ancestor is already a managed tree — is the
/// CLI `bind`'s same "respect the tree that is already there". Unlike `project_add_folder`, which
/// creates a new project, this binds a folder to a project that already exists — including a folder
/// that already named another one: re-pointing it here takes it off that project's books, because the
/// pointer this writes names one project (`Registry::claim_project_ref`).
#[tauri::command]
pub fn project_bind_folder(project_id: i64, dir: String) -> Result<WriteAck, CmdError> {
    use amenbo_core::binding::find_upward_ancestor;
    let path = std::path::Path::new(&dir);
    if !path.is_dir() {
        return Err(CmdError::from(amenbo_core::Error::not_found(format!("folder not found: {dir}"))));
    }
    let cwd = amenbo_core::binding::canonical_dir(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some((bound_dir, _)) = find_upward_ancestor(&cwd) {
        return Err(CmdError::coded(
            "binding_nested_tree",
            format!(
                "this folder is already inside an Amenbo-managed tree (bound at {}); binding a subfolder would shadow that pointer",
                bound_dir.display()
            ),
            serde_json::json!({ "path": bound_dir.display().to_string() }),
        ));
    }
    let store = open_store()?;
    amenbo_core::binding::pointer_for(&store, project_id).write(&cwd)?;
    let mut registry = store.bindings();
    registry.claim_project_ref(project_id, cwd.to_string_lossy());
    store.save_bindings(&registry)?;
    amenbo_core::agents::upsert_into_dir(
        &cwd,
        store.config.language.as_deref(),
        amenbo_core::config::Paths::command_name(),
    );
    Ok(WriteAck::new(&[]))
}

/// Folder management on the project settings screen: unbind this folder (the Tauri path for
/// `unbind`). Removes only the folder's `.amenbo` pointer and Amenbo's managed block (AGENTS.md /
/// CLAUDE.md), and forgets the folder in the registry — many folders map to one project, so the
/// other folders pointing at the same project are left alone. **The store itself is never deleted**:
/// this severs a binding, it does not remove a store. Confirming the destructive part is the GUI's
/// job (plugin-dialog). For a stale folder (moved or deleted), cleaning up the registry entry still
/// works.
#[tauri::command]
pub fn project_unbind_folder(dir: String) -> Result<WriteAck, CmdError> {
    let target = std::path::PathBuf::from(&dir);
    let marker = target.join(".amenbo");
    if marker.is_file() {
        std::fs::remove_file(&marker)
            .map_err(|e| CmdError::from(format!("cannot remove {}: {e}", marker.display())))?;
    }
    let _ = amenbo_core::agents::remove_from_dir(&target);
    let mut store = open_store()?;
    let mut registry = store.bindings();
    // Read before the forgetting: these are the projects that are about to lose the folder, and after it
    // the registry can no longer say who held it.
    let owners = registry.projects_for_dir(&dir);
    let mut forgot = registry.forget_dir(&dir);
    if let Ok(canon) = amenbo_core::binding::canonical_dir(&target) {
        let canon_str = canon.to_string_lossy().to_string();
        if canon_str != dir {
            forgot += registry.forget_dir(&canon_str);
        }
    }
    if forgot > 0 {
        store.save_bindings(&registry)?;
        // The folder is gone, so the tasks that named it lose their place (`AMB-D-648`) — the same move
        // the CLI's `unbind` makes, and best-effort for the same reason.
        for project_id in owners {
            let _ = store.forget_gone_task_folders(project_id);
        }
    }
    Ok(WriteAck::new(&[]))
}

/// Add a dimension (classification axis), scoped to the project and appended at the end. The GUI
/// creates it in its plain default form: single-select, unordered, no role — a generic user axis.
/// Same shape as the CLI's `dimension add`.
#[tauri::command]
pub fn dimension_add(project_id: i64, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.dimension_add(
            project_id,
            amenbo_core::ops::dimension::NewDimension { name, ..Default::default() },
        )?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Rename a dimension — the name alone, which is the one edit the screen makes inline (the rest of the
/// axis is edited through `dimension_update`).
#[tauri::command]
pub fn dimension_rename(id: i64, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.dimension_update(id, Some(&name), None, None, None, None, None, None, None, None)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Rename a dimension's readable key (`AMB-D-735`) — the key alone, the slug's counterpart of
/// [`dimension_rename`]. A key is never cleared, only replaced, so there is no empty arm: core refuses
/// a shape it cannot carry outside (`invalid_dimension_slug_shape`) and a key another axis in the same
/// project already answers to (`invalid_dimension_slug_taken`), and the panel puts that refusal in
/// front of the reader rather than guessing a key of its own.
#[tauri::command]
pub fn dimension_set_slug(id: i64, slug: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.dimension_update(id, None, None, None, None, None, None, None, None, Some(&slug))?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Update a dimension's description (notes), whether one record may answer it with several values
/// (multi), whether its values are ordered (ordered), whether it is
/// the time axis (time_axis), whether it goes on the task card (show_on_card), whether it refuses
/// to be left empty (required), and which of the two entities it classifies (applies_to). Only the
/// fields passed are changed — same shape as the CLI's `dimension update`. Turning `multi` on lets a
/// record hold several of this axis's values at once (`AMB-D-826`), and core refuses the way back
/// down while any record still holds several, and refuses the pair `multi` × time axis at either
/// door; turning `ordered` on makes reordering values (`dimension_value_move`) take
/// effect; turning `time_axis` on makes that axis's values carry periods, and turning `closable` on
/// lets them be closed instead of deleted (`AMB-D-829`) — one role per axis, so the two switches are
/// the same field and never arrive together; turning `show_on_card` on
/// puts this axis on every task card, for everyone (`AMB-D-651` — the axis holds the answer, not the
/// device); turning `required` on makes a creation on this project wait until the axis is answered
/// (`AMB-D-734`), and core refuses it on an axis that offers no values; narrowing `applies_to` takes
/// the axis out of the side it no longer classifies, leaving the assignments already made there in
/// place, meaning nothing (`AMB-D-789`).
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn dimension_update(
    id: i64,
    notes: Option<String>,
    multi: Option<bool>,
    ordered: Option<bool>,
    time_axis: Option<bool>,
    closable: Option<bool>,
    show_on_card: Option<bool>,
    required: Option<bool>,
    applies_to: Option<String>,
) -> Result<WriteAck, CmdError> {
    // The two nominations are one field, and an axis holds one role, so the panel moves one switch at a
    // time and either arm alone says what the role becomes. Both at once is the screen's defect, not the
    // person's — refused here rather than silently letting one win.
    let role = match (time_axis, closable) {
        (Some(_), Some(_)) => {
            return Err(amenbo_core::Error::invalid(
                "an axis holds one role: pass time_axis or closable, not both",
            )
            .into())
        }
        (Some(on), None) => Some(if on { DimensionRole::TimeAxis } else { DimensionRole::None }),
        (None, Some(on)) => Some(if on { DimensionRole::Closable } else { DimensionRole::None }),
        (None, None) => None,
    };
    let cardinality = multi.map(|on| {
        if on { DimensionCardinality::Multi } else { DimensionCardinality::Single }
    });
    // A word this build does not know is the screen's defect, not the person's, so it is refused here
    // rather than silently read as `both` — the same door core's own parse holds.
    let applies_to = match applies_to.as_deref() {
        Some(word) => Some(
            amenbo_core::model::DimensionAppliesTo::parse(word)
                .ok_or_else(|| amenbo_core::Error::invalid(format!("unknown applies_to '{word}'")))?,
        ),
        None => None,
    };
    with_store_mut(|store| {
        store.dimension_update(id, None, notes.as_deref(), cardinality, ordered, role, show_on_card, required, applies_to, None)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Reorder a dimension (give the anchor dimension's id to exactly one of `before` / `after` — same
/// shape as the CLI's `dimension move`).
#[tauri::command]
pub fn dimension_move(id: i64, before: Option<i64>, after: Option<i64>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let pos = amenbo_core::ops::Position::from_flags(false, false, before, after)?;
        store.dimension_move(id, pos)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Delete a dimension permanently (the delete op takes its values and the task assignments on them
/// first, children before the row they hang on — same shape as the CLI's `dimension rm`).
#[tauri::command]
pub fn dimension_rm(id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.dimension_delete(id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Add a value (a choice) to a dimension, at the end (same shape as the CLI's
/// `dimension value-add`).
#[tauri::command]
pub fn dimension_value_add(dimension_id: i64, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.dimension_value_add(dimension_id, &name, None, None)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Rename a dimension value — the name alone, the value's counterpart of [`dimension_rename`].
#[tauri::command]
pub fn dimension_value_rename(value_id: i64, name: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.dimension_value_update(value_id, Some(&name), None, None)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Rename a dimension value's readable key (`AMB-D-735`) — [`dimension_set_slug`] one value wide, and
/// checked for a collision within the axis, which is as far as a value's key has to be unique.
#[tauri::command]
pub fn dimension_value_set_slug(value_id: i64, slug: String) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.dimension_value_update(value_id, None, Some(&slug), None)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Replace a dimension value's period `[startOn, endOn]` wholesale (both ends inclusive,
/// `YYYY-MM-DD`) — the same landing place as the CLI's
/// `dimension value-update --start/--end/--clear-*`. `None` opens that end (`endOn: None` means
/// ongoing). The GUI's date fields always send both ends, so there is no partial-update merge to do
/// as in the CLI. A period is the payload of `role: time_axis`, so a value on a non-time_axis
/// dimension is **refused here** — following core's arrangement that the gatekeeper lives above it,
/// in the CLI and the GUI.
#[tauri::command]
pub fn dimension_value_set_period(
    value_id: i64,
    start_on: Option<String>,
    end_on: Option<String>,
) -> Result<WriteAck, CmdError> {
    let start = parse_iso_date(start_on.as_deref())?;
    let end = parse_iso_date(end_on.as_deref())?;
    with_store_mut(|store| {
        let value = store
            .dimension_value(value_id)?
            .ok_or_else(|| amenbo_core::Error::not_found(format!("dimension value '{value_id}' not found")))?;
        let role = store.dimension(value.dimension_id)?.map(|d| d.role);
        if !matches!(role, Some(amenbo_core::model::DimensionRole::TimeAxis)) {
            return Err(amenbo_core::Error::invalid("only a time-axis dimension's values carry a period")
            .into());
        }
        store.dimension_value_update(value_id, None, None, Some((start, end)))?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Close a dimension value, or open it again (`AMB-D-829`) — the panel's counterpart of the CLI's
/// `dimension value-close` / `value-reopen`. Closing retires the value from what a record is newly filed
/// under and takes nothing away: the records already on it keep it, its name, key and place stay, and a
/// filter naming it goes on resolving. Unlike a period, the role is checked by core rather than here —
/// closing *is* what the nomination means, so the refusal belongs at the door that writes it
/// (`invalid_dimension_close_not_closable`), together with the one keeping a required axis answerable
/// (`invalid_dimension_close_last_open`). Reopening asks for nothing.
#[tauri::command]
pub fn dimension_value_set_closed(value_id: i64, closed: bool) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.dimension_value_set_closed(value_id, closed)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Turn `YYYY-MM-DD` into a date. An empty string (a date field the user cleared) opens that end,
/// just as `None` does.
fn parse_iso_date(s: Option<&str>) -> Result<Option<NaiveDate>, CmdError> {
    match s.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d").map(Some).map_err(|_| {
            CmdError::from(amenbo_core::Error::invalid(format!("'{s}' is not a date (expected YYYY-MM-DD)")))
        }),
    }
}

/// Reorder a dimension value (give the anchor value's id to exactly one of `before` / `after` — same
/// shape as the CLI's `dimension value-move`).
#[tauri::command]
pub fn dimension_value_move(value_id: i64, before: Option<i64>, after: Option<i64>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let pos = amenbo_core::ops::Position::from_flags(false, false, before, after)?;
        store.dimension_value_move(value_id, pos)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Delete a dimension value permanently (the delete op takes the task assignments on it first — same
/// shape as the CLI's `dimension value-rm`). `reassign_to` names another value of the same axis to move
/// those assignments to, which a required axis demands whenever there are any.
#[tauri::command]
pub fn dimension_value_rm(value_id: i64, reassign_to: Option<i64>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.dimension_value_delete(value_id, reassign_to)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]))
}

/// Assign a dimension value to a task (on a single-select dimension this replaces whatever was
/// assigned on that axis — same shape as the CLI's `dimension set`).
#[tauri::command]
pub fn task_set_dimension_value(task_id: i64, value_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.set_task_dimension_value(task_id, value_id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(task_id))
}

/// Take a particular dimension value off a task (a no-op if it was not assigned — same shape as the
/// CLI's `dimension unset`).
#[tauri::command]
pub fn task_unset_dimension_value(task_id: i64, value_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.unset_task_dimension_value(task_id, value_id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(task_id))
}

/// The dimension assignments a task currently carries (`dimensionId`→`valueId`), straight from the
/// read-model. The detail pane's assignment selects use it to reflect the current value.
#[tauri::command]
pub fn task_dimensions(task_id: i64) -> Result<Vec<TaskDimensionAssignmentDto>, CmdError> {
    let store = open_store_read()?;
    let read_model = store.read_model();
    let rows = amenbo_core::store_engine::read::task_dimension_assignments(read_model.conn(), task_id)?;
    Ok(rows
        .into_iter()
        .map(|(dimension_id, value_id)| TaskDimensionAssignmentDto { dimension_id, value_id })
        .collect())
}

/// Assign a dimension value to a decision — the decision side of [`task_set_dimension_value`]
/// (`AMB-D-781`). On a single-select dimension this replaces whatever was assigned on that axis.
#[tauri::command]
pub fn decision_set_dimension_value(decision_id: i64, value_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.set_decision_dimension_value(decision_id, value_id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(decision_id))
}

/// Take a particular dimension value off a decision (a no-op if it was not assigned). Unconditional:
/// `required` bites where a creation is finished, and a decision has none (`AMB-D-781`).
#[tauri::command]
pub fn decision_unset_dimension_value(decision_id: i64, value_id: i64) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        store.unset_decision_dimension_value(decision_id, value_id)?;
        Ok(())
    })?;
    Ok(WriteAck::new(&["decisions"]).decision(decision_id))
}

/// The dimension assignments a decision currently carries (`dimensionId`→`valueId`), straight from
/// the read-model — [`task_dimensions`] on the decision side.
#[tauri::command]
pub fn decision_dimensions(decision_id: i64) -> Result<Vec<DecisionDimensionAssignmentDto>, CmdError> {
    let store = open_store_read()?;
    let read_model = store.read_model();
    let rows = amenbo_core::store_engine::read::decision_dimension_assignments(read_model.conn(), decision_id)?;
    Ok(rows
        .into_iter()
        .map(|(dimension_id, value_id)| DecisionDimensionAssignmentDto { dimension_id, value_id })
        .collect())
}

/// Every task assignment (`taskId`→`valueId`) for one project on one dimension, in a single read
/// straight from the read-model. The board uses it to bundle tasks by value on the chosen dimension
/// (browsing/grouping).
#[tauri::command]
pub fn project_dimension_assignments(project_id: i64, dimension_id: i64) -> Result<Vec<DimensionTaskValueDto>, CmdError> {
    let store = open_store_read()?;
    let read_model = store.read_model();
    let rows = amenbo_core::store_engine::read::project_dimension_assignments(read_model.conn(), project_id, dimension_id)?;
    Ok(rows
        .into_iter()
        .map(|(task_id, value_id)| DimensionTaskValueDto { task_id, value_id })
        .collect())
}

/// Every decision assignment (`decisionId`→`valueId`) for one project on one dimension, in a single
/// read — [`project_dimension_assignments`] on the decision side. The decisions tab holds the whole
/// project's decisions already, so its filters narrow what it has rather than asking core per chip.
#[tauri::command]
pub fn project_decision_dimension_assignments(project_id: i64, dimension_id: i64) -> Result<Vec<DimensionDecisionValueDto>, CmdError> {
    let store = open_store_read()?;
    let read_model = store.read_model();
    let rows = amenbo_core::store_engine::read::project_decision_dimension_assignments(read_model.conn(), project_id, dimension_id)?;
    Ok(rows
        .into_iter()
        .map(|(decision_id, value_id)| DimensionDecisionValueDto { decision_id, value_id })
        .collect())
}

/// Assign the task to a facet (`kind=Some("ai")` means the person's AI — it lands in the mailbox),
/// or clear it (`kind=None`). Assignment is on the facet alone. Idempotent — same facet is a no-op —
/// because `set_task_assignee` commits in a transaction of its own, so calling it with an unchanged
/// value would still move `updated_at`. From the GUI, the path that actually gets used is "hand it
/// to my AI".
#[tauri::command]
pub fn task_assign(id: i64, kind: Option<String>) -> Result<WriteAck, CmdError> {
    with_store_mut(|store| {
        let kind_arg = match kind.as_deref() {
            Some("ai") => Some(ActorKind::Ai),
            Some("human") => Some(ActorKind::Human),
            Some(other) => return Err(format!("facet '{other}' is not one of human / ai").into()),
            None => None,
        };
        let noop = store.task(id)?.is_some_and(|t| t.assignee_kind == kind_arg);
        if !noop {
            store.set_task_assignee(id, kind_arg, ActorKind::Human)?;
            let ev = amenbo_core::activity_log::event::task_assigned(kind_arg.map(|k| k.as_str()));
            emit(store, id, ev);
        }
        Ok(())
    })?;
    Ok(WriteAck::new(&["tasks"]).task(id))
}

/// Rewrite this person's facet display names (the roster — `config.human_name` / `ai_name`). `human`
/// and `ai` are each set only when they are `Some` and non-empty; `None` or empty leaves that facet
/// alone. Both None does nothing. Does nothing if there is no store yet (we never quietly genesis
/// one).
fn write_facet_names(human: Option<&str>, ai: Option<&str>) -> Result<(), CmdError> {
    let human = human.map(str::trim).filter(|s| !s.is_empty());
    let ai = ai.map(str::trim).filter(|s| !s.is_empty());
    if human.is_none() && ai.is_none() {
        return Ok(());
    }
    ensure_migrated()?;
    let paths = amenbo_core::config::Paths::resolve()?;
    if amenbo_core::env::home().is_none() && !paths.store_file.exists() {
        return Ok(());
    }
    let mut store = Store::open_at(paths)?;
    if let Some(h) = human {
        store.config.set("human_name", h)?;
    }
    if let Some(a) = ai {
        store.config.set("ai_name", a)?;
    }
    store.save_config()?;
    Ok(())
}

/// Change the user's language `config.language`. Nobody is asked for it on a first launch: the front
/// end settles it from the OS through this same command, and the settings screen changes it
/// afterwards. The language lives in the user-level global `config.json`, outside the store, so it
/// can be written whether or not a store exists. When the front end applies the `language` in the
/// snapshot we return, i18n switches over **without a restart**, with no help from `watch_store`. The change is also carried into the managed block of
/// AGENTS.md and CLAUDE.md in every bound directory — closing the gap where the GUI switched to
/// English while the AI kept being told Japanese. That part is best-effort and re-syncs only the
/// directories the registry knows about (unregistered ones fall into line at the next bind).
#[tauri::command]
pub fn config_set_language(language: String) -> Result<WriteAck, CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    let mut config = amenbo_core::config::Config::load(&paths.config_file);
    config
        .set("language", language.trim())
        ?;
    config.save(&paths.config_file)?;
    let lang_code = config.language.as_deref();
    let registry = open_store_read().map(|s| s.bindings()).unwrap_or_default();
    for dir in registry.all_dirs() {
        amenbo_core::agents::upsert_into_dir(
            std::path::Path::new(&dir),
            lang_code,
            amenbo_core::config::Paths::command_name(),
        );
    }
    Ok(WriteAck::new(&[]))
}

/// Switch the level of perf instrumentation (`config.perf_log`) from the settings screen. The values
/// are `off`, `budget-only` and `verbose`. Saves to config.json and then `reload`s the running
/// tracing filter, so it takes effect **without a restart** (if `AMENBO_PERF` is set in the
/// environment, it wins).
#[tauri::command]
pub fn config_set_perf_log(mode: String) -> Result<WriteAck, CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    let mut config = amenbo_core::config::Config::load(&paths.config_file);
    config.set("perf_log", mode.trim())?;
    config.save(&paths.config_file)?;
    crate::perf::reload(config.perf_log);
    Ok(WriteAck::new(&[]))
}

/// Turn update checking on or off from the settings screen. A thin wrapper straight onto core's
/// `Config::set("update_check", …)` (on by default). The next snapshot reflects the new value in
/// `updateCheck`, and when it is off, upstream latest.json is no longer queried.
#[tauri::command]
pub fn config_set_update_check(enabled: bool) -> Result<WriteAck, CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    let mut config = amenbo_core::config::Config::load(&paths.config_file);
    config.set("update_check", if enabled { "true" } else { "false" })?;
    config.save(&paths.config_file)?;
    Ok(WriteAck::new(&[]))
}

/// Change the view a new project opens in (`config.default_view`) from the settings screen. A thin
/// wrapper onto core's `Config::set("default_view", …)`, which is where a value outside
/// list|board|calendar|timeline is refused. What it moves is the answer for a project created
/// without a view of its own — every project that already exists keeps the one it carries, so
/// nothing on screen changes until the next project is made.
#[tauri::command]
pub fn config_set_default_view(view: String) -> Result<WriteAck, CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    let mut config = amenbo_core::config::Config::load(&paths.config_file);
    config.set("default_view", view.trim())?;
    config.save(&paths.config_file)?;
    Ok(WriteAck::new(&[]))
}

/// Turn "start when I log in" on or off from the settings screen (`AMB-D-541`).
///
/// Two things move, and the order between them is the whole of the error handling: the OS
/// registration is written first ([`crate::autostart::set`]) and `config.autostart` only after it
/// came back. A registration that could not be written therefore leaves the config saying what is
/// true — off — rather than a switch that reads on over a login that starts nothing. There is no
/// `config set` key for the field for the same reason: the CLI cannot write the OS half, so it has
/// no honest way to move this one.
///
/// A development build never reaches here (the section holding the switch is not built on that
/// channel); if something calls it anyway, `autostart::set` refuses and nothing is saved.
#[tauri::command]
pub fn config_set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<WriteAck, CmdError> {
    crate::autostart::set(&app, enabled)?;
    let paths = amenbo_core::config::Paths::resolve()?;
    let mut config = amenbo_core::config::Config::load(&paths.config_file);
    config.autostart = enabled;
    config.save(&paths.config_file)?;
    Ok(WriteAck::new(&[]))
}

/// Write the roster's two avatars (human / AI): the display version into `config.human_avatar` /
/// `ai_avatar`, and the image the user picked into the blob store, named on that facet's
/// `*_avatar_source` (`AMB-D-839`). Each argument has three states: `None` leaves that facet alone,
/// `Some(("", _))` clears it (back to the identicon), and `Some((dataUrl, _))` sets it. Unlike display
/// names ([`write_facet_names`]), an avatar **can be cleared**, so an empty string and an absent key
/// mean different things. Format and size limits are checked by core's
/// [`amenbo_core::config::validate_avatar`] before anything is written, and the two halves go in
/// together through [`amenbo_core::config::Config::set_avatar`], so a display version never stands
/// beside the previous image's original. A facet handed no bytes keeps a display version alone —
/// which is what happens when the caller has no original to keep.
fn write_facet_avatars(
    human: Option<(&str, Option<&[u8]>)>,
    ai: Option<(&str, Option<&[u8]>)>,
) -> Result<(), CmdError> {
    if human.is_none() && ai.is_none() {
        return Ok(());
    }
    for (key, v) in [("human_avatar", human), ("ai_avatar", ai)] {
        if let Some(val) = v.map(|(display, _)| display.trim()).filter(|s| !s.is_empty()) {
            amenbo_core::config::validate_avatar(key, val)?;
        }
    }
    ensure_migrated()?;
    let paths = amenbo_core::config::Paths::resolve()?;
    if amenbo_core::env::home().is_none() && !paths.store_file.exists() {
        return Ok(());
    }
    let mut store = Store::open_at(paths)?;
    for (kind, arg) in [(ActorKind::Human, human), (ActorKind::Ai, ai)] {
        let Some((display, source)) = arg else { continue };
        let display = Some(display.trim()).filter(|s| !s.is_empty());
        // The original goes in only beside a display version: clearing takes both away.
        let hash = match (display, source) {
            (Some(_), Some(bytes)) => Some(store.blobs().ingest_bytes(bytes)?.hash),
            _ => None,
        };
        store.config.set_avatar(kind, display, hash.as_deref())?;
    }
    store.save_config()?;
    Ok(())
}

/// Set or clear the per-facet (human / AI) avatars from the settings screen. The counterpart of
/// [`write_facet_names`] for display names: the roster's two faces live in config. For each facet, an
/// absent key leaves it alone, an empty string clears it, and a data URL sets it. `human_source` /
/// `ai_source` carry the bytes of the image the user picked, kept as the original beside the display
/// version the front end shrank (`AMB-D-839`); a facet sent no bytes is registered with a display
/// version alone.
#[tauri::command]
pub fn set_facet_avatars(
    human_avatar: Option<String>,
    ai_avatar: Option<String>,
    human_source: Option<Vec<u8>>,
    ai_source: Option<Vec<u8>>,
) -> Result<WriteAck, CmdError> {
    write_facet_avatars(
        human_avatar.as_deref().map(|d| (d, human_source.as_deref())),
        ai_avatar.as_deref().map(|d| (d, ai_source.as_deref())),
    )?;
    Ok(WriteAck::new(&[]))
}

/// Rewrite the roster's two display names (human / AI) from the settings screen; config is
/// authoritative for display names. Only a facet given as `Some(non-empty)` is updated; the other is
/// left alone. Both None or empty is an error — a call that would change nothing is refused.
#[tauri::command]
pub fn set_facet_names(human_name: Option<String>, ai_name: Option<String>) -> Result<WriteAck, CmdError> {
    let human = human_name.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let ai = ai_name.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if human.is_none() && ai.is_none() {
        return Err("a display name cannot be empty".into());
    }
    write_facet_names(human, ai)?;
    Ok(WriteAck::new(&[]))
}

/// The cancellation flag for the whole-store operations (backup/restore/export). "Abort" in the
/// progress modal raises it through [`cancel_data_op`], and core's per-store progress callback reads
/// it at each boundary and `Break`s. Every operation resets it to false when it starts, so a
/// cancellation never carries over into the next one.
static DATA_OP_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Abort a running whole-store backup/restore at the next store boundary ("Abort" in the progress
/// modal). Core leaves nothing half-applied: a backup deletes its unfinished archive, and a restore
/// rolls back every swap it had completed.
#[tauri::command]
pub fn cancel_data_op() {
    DATA_OP_CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Return `config.language` (or `None` when unset) **without opening the store**. Normally the UI
/// language rides in the snapshot — but a store that a newer build has moved past yields no
/// snapshot, and the restart screen still has to speak the user's language. `config.json` is a file
/// of its own, outside the store, and can be read without passing the version gate.
#[tauri::command]
pub fn ui_language() -> Option<String> {
    amenbo_core::config::Paths::resolve()
        .ok()
        .and_then(|p| amenbo_core::config::Config::load(&p.config_file).language)
}

/// Exit this process and launch the same executable again (the button on the restart screen). A
/// long-running GUI that a newer process has overtaken is nothing but **an old process still sitting
/// in memory**: the GUI and the CLI ship together, so the executable on disk is already the new
/// version, and relaunching `current_exe` simply becomes it (on Linux the updated AppImage takes
/// the same path, so only the running process is stale). This is not self-update: it touches no
/// network and fetches no new binary — it relaunches what is already there, and the user is the one
/// who presses it.
#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    app.restart();
}

/// What the startup migration is doing. The front end asks **first thing** at startup and, if it is
/// `running`, goes to the migration screen without reading the store (`idle` is the normal case —
/// straight into the app). After that it follows the `migration-changed` / `migration-progress`
/// events — subscribing alone is not enough, because the phase can advance before the window is even
/// mounted.
#[tauri::command]
pub fn migration_status() -> crate::migrate::MigrationStatusDto {
    crate::migrate::status()
}

/// Retry a failed migration ("Retry" on the migration screen). A failure means it was rolled back
/// whole and the store still stands exactly as it did before it began (core's envelope), so once
/// whatever was in the way is cleared — freeing disk space, say — the same path can simply be walked
/// again. On success, the store's resident threads (watching, GC), which the first launch skipped,
/// are started here. Heavy I/O goes off the main thread via `spawn_blocking`.
#[tauri::command]
pub async fn migration_retry(app: tauri::AppHandle) -> Result<(), CmdError> {
    crate::migrate::begin();
    tauri::async_runtime::spawn_blocking(move || {
        if crate::migrate::run(&app) {
            crate::start_store_threads(app.clone());
        }
    })
    .await
    .map_err(|e| CmdError::from(format!("retrying the migration did not finish: {e}")))?;
    Ok(())
}

/// Map core's [`amenbo_core::progress::Phase`] to the stable string the GUI localizes.
fn phase_str(phase: amenbo_core::progress::Phase) -> &'static str {
    use amenbo_core::progress::Phase;
    match phase {
        Phase::Snapshotting => "snapshotting",
        Phase::Blobs => "blobs",
        Phase::Unpacking => "unpacking",
        Phase::Verifying => "verifying",
        Phase::Exporting => "exporting",
        Phase::Copying => "copying",
        Phase::Migrating => "migrating",
    }
}

/// A progress sink: it streams progress to the webview as `data-progress` events, and returns
/// `Break` to cancel when [`DATA_OP_CANCEL`] is raised. It owns its `window`, so it is `'static` and
/// can be handed to `spawn_blocking`.
fn progress_sink(
    window: tauri::Window,
) -> impl FnMut(&amenbo_core::progress::Progress) -> std::ops::ControlFlow<()> {
    use tauri::Emitter;
    move |p| {
        let _ = window.emit("data-progress", DataProgressDto::of(p));
        if DATA_OP_CANCEL.load(std::sync::atomic::Ordering::SeqCst) {
            std::ops::ControlFlow::Break(())
        } else {
            std::ops::ControlFlow::Continue(())
        }
    }
}

impl DataProgressDto {
    /// Map a tick from core into the shape the webview can be fed.
    pub fn of(p: &amenbo_core::progress::Progress) -> Self {
        DataProgressDto {
            phase: phase_str(p.phase).to_string(),
            done: p.done as u32,
            total: p.total.map(|t| t as u32),
        }
    }
}

/// "Back up everything" under Settings > Data: write this machine's store out as a single, verified
/// archive (core's [`amenbo_core::archive::backup_from`]). The GUI is a thin wrapper, streaming
/// progress to the progress modal as `data-progress` events. The heavy I/O (VACUUM, verification)
/// goes off the main thread via `spawn_blocking` so the progress modal never freezes.
#[tauri::command]
pub async fn run_backup(window: tauri::Window, path: String) -> Result<BackupReportDto, CmdError> {
    DATA_OP_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(move || -> Result<BackupReportDto, CmdError> {
        let Some(source) = amenbo_core::archive::enumerate_store() else {
            return Err(CmdError::from(
                "this device holds no store to back up".to_string(),
            ));
        };
        let mut progress = progress_sink(window);
        let report =
            amenbo_core::archive::backup_from(&source, std::path::Path::new(&path), &mut progress)?;
        Ok(BackupReportDto { path: report.path, bytes: report.bytes as usize })
    })
    .await
    .map_err(|e| CmdError::from(format!("the backup did not finish: {e}")))?
}

/// "Restore everything" under Settings > Data (**destructive**). Swaps the whole store for the one
/// carried in a `.amenbo-backup` archive, migrating it forward to this build's generation on the way
/// (core's [`amenbo_core::archive::restore_into`]: stage-and-swap, rollback on failure, and the old
/// source of truth set aside under a timestamp). Progress goes to the progress modal as events, and
/// the heavy I/O goes off the main thread via `spawn_blocking`. On success the front end rebuilds
/// the screen by invalidating every query and refetching the snapshot.
#[tauri::command]
pub async fn run_restore(window: tauri::Window, path: String) -> Result<RestoreReportDto, CmdError> {
    DATA_OP_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
    tauri::async_runtime::spawn_blocking(move || -> Result<RestoreReportDto, CmdError> {
        let stamp = Timestamp::now().0.format("%Y%m%dT%H%M%SZ").to_string();
        let mut progress = progress_sink(window);
        let report = amenbo_core::archive::restore_into(
            std::path::Path::new(&path),
            &stamp,
            &amenbo_core::archive::restore_dest(),
            &mut progress,
        )?;
        let m = report.migration;
        let migration = m.migrated().then(|| MigrationRunDto {
            from: m.from,
            to: m.to,
            applied: m.applied.iter().map(|s| s.to_string()).collect(),
        });
        Ok(RestoreReportDto {
            previous_saved_to: report.previous_saved_to,
            blobs: report.blobs,
            superseded: report.superseded.len(),
            migration,
        })
    })
    .await
    .map_err(|e| CmdError::from(format!("the restore did not finish: {e}")))?
}

/// The progress sink for export (the sibling of [`progress_sink`]). It emits `data-progress` only
/// for store-boundary ticks (those with `total` set to `Some`); export's in-row cancel-poll ticks
/// (`total` of `None`, once every 256 rows) are used for the cancellation check alone and never
/// reach the progress modal — so even on a huge store the modal does not flicker and the IPC channel
/// does not flood. It owns its `window`, so it is `'static` and can be handed to `spawn_blocking`.
fn boundary_progress_sink(
    window: tauri::Window,
) -> impl FnMut(&amenbo_core::progress::Progress) -> std::ops::ControlFlow<()> {
    use tauri::Emitter;
    move |p| {
        if p.total.is_some() {
            let _ = window.emit("data-progress", DataProgressDto::of(p));
        }
        if DATA_OP_CANCEL.load(std::sync::atomic::Ordering::SeqCst) {
            std::ops::ControlFlow::Break(())
        } else {
            std::ops::ControlFlow::Continue(())
        }
    }
}

/// "Export" under Settings > Data: write everything on this machine out into an **export
/// directory** (core's [`amenbo_core::export::export_bundle`], bounded memory) — an `export.json`
/// plus an `attachments/` directory holding the attachment files themselves. There is no import, so
/// this bundle *is* the artifact you migrate with, and without the files themselves nothing has
/// really been carried out. The destination is the `path` chosen in the front end's dialog (an
/// existing path is refused). Progress is streamed to the progress modal as `data-progress` events,
/// and `cancel_data_op` can stop it partway (core builds the export aside and only renames it into
/// place once whole, so an abort or failure leaves no directory at all rather than a truncated one).
/// Heavy I/O goes off the main thread via `spawn_blocking`.
#[tauri::command]
pub async fn run_export(window: tauri::Window, path: String) -> Result<ExportReportDto, CmdError> {
    DATA_OP_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
    let out = std::path::PathBuf::from(&path);
    let report = tauri::async_runtime::spawn_blocking(move || {
        let mut progress = boundary_progress_sink(window);
        amenbo_core::export::export_bundle(&out, &mut progress).map_err(CmdError::from)
    })
    .await
    .map_err(|e| CmdError::from(format!("the export did not finish: {e}")))??;

    Ok(ExportReportDto {
        path: report.path,
        bytes: report.bytes as usize,
        attachments: report.attachments as usize,
        missing: report.missing as usize,
    })
}

/// Raise an OS notification when something arrives in the inbox. macOS delivers it ourselves through
/// UNUserNotificationCenter, Windows through notify-rust (with the click wired to inbox navigation),
/// and Linux through `tauri-plugin-notification` (D-Bus). If the OS drops it — permission not
/// granted, say — that is not fatal (the app has no sound of its own; the arrival sound is the OS
/// notification's).
#[tauri::command]
#[cfg_attr(any(target_os = "macos", target_os = "windows"), allow(unused_variables))]
pub fn notify_os(app: tauri::AppHandle, title: String, body: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        crate::macos_notify::send(&title, &body);
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        crate::windows_notify::send(&app, title, body);
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        use tauri_plugin_notification::NotificationExt;
        app.notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .map_err(|e| e.to_string())
    }
}

/// After the binary is updated, the `CLAUDE.md` / `AGENTS.md` of a bound folder can be left holding
/// an older managed block. A read-only command that lists those for the GUI over **the same core
/// detection path** as the CLI's `doctor` (`agents::stale_bound_blocks`) — no side effects, nothing
/// rewritten.
#[tauri::command]
pub fn stale_managed_blocks() -> Result<Vec<StaleBlockDto>, CmdError> {
    let current = amenbo_core::agents::MANAGED_BLOCK_VERSION;
    Ok(amenbo_core::agents::stale_bound_blocks(&open_store_read()?.bindings())
        .into_iter()
        .map(|s| StaleBlockDto { dir: s.dir, file: s.file.to_string(), version: s.version, current })
        .collect())
}

/// Re-sync stale managed blocks to the current version, over **the same core path** as the CLI's
/// `sync-guide` (`agents::resync_bound_blocks`). Give it a `dir` for one folder; omit it for every
/// bound folder. Low churn — it writes only when the content actually changes — and each folder's
/// language label is preserved, never degraded. Nothing outside the markers is touched. It writes
/// `CLAUDE.md` / `AGENTS.md` on the filesystem and leaves the store alone, so there is no snapshot to
/// refetch (and no `WriteAck` to return).
#[tauri::command]
pub fn resync_managed_blocks(dir: Option<String>) -> Result<ResyncReportDto, CmdError> {
    let report = amenbo_core::agents::resync_bound_blocks(
        &open_store_read()?.bindings(),
        dir.as_deref(),
        amenbo_core::config::Paths::command_name(),
    );
    Ok(ResyncReportDto {
        scanned: report.scanned as u32,
        updated: report
            .updated
            .into_iter()
            .map(|(dir, file)| ResyncedDto { dir, file: file.to_string() })
            .collect(),
    })
}

/// A read-only command listing the bound-folder rows that no living project claims — the debris a
/// deleted project left behind in the index. Over **the same core detection path** as the CLI's
/// `doctor` (`binding::orphan_dirs`). No other GUI surface can show these, structurally: the folder
/// list ([`project_bound_folders`]) does its reverse lookup per project, so a row with no claimant
/// appears under no project at all. (`legacy` and `pointer_missing` show up in the folder list;
/// stale managed blocks in the [`stale_managed_blocks`] banner.)
#[tauri::command]
pub fn orphan_bindings() -> Result<Vec<String>, CmdError> {
    Ok(amenbo_core::binding::orphan_dirs(&open_store_read()?))
}

/// Forget the debris folder rows in the index (over **the same core path** as the CLI's
/// `doctor --fix`, `Store::forget_orphan_dirs`). It drops rows in the binding table and nothing
/// else — not the folder's contents, not its `.amenbo` — so it is not destructive and asks for no
/// confirmation (the CLI does not either). Returns how many were forgotten. Since the only rows it
/// touches are the ones no project claims, not a single row of a living project's reads (the
/// snapshot, the folder list) moves — so it returns no `WriteAck`: there is nothing to refetch.
#[tauri::command]
pub fn forget_orphan_bindings() -> Result<u32, CmdError> {
    let store = open_store()?;
    Ok(store.forget_orphan_dirs()? as u32)
}

impl From<&amenbo_core::validate::DoctorIssue> for DoctorIssueDto {
    fn from(i: &amenbo_core::validate::DoctorIssue) -> Self {
        Self {
            kind: i.kind.as_str().to_string(),
            severity: i.severity.to_string(),
            target: i.target.clone(),
            params: i.params.clone(),
        }
    }
}

/// A read-only command listing the issues where a bound folder's `.amenbo` is broken (old format, or
/// gone). The GUI calls it **once, at app startup**, and adds the rows to the startup health banner
/// (the fix, [`repair_pointers`], can be pressed right there in the banner). It is deliberately not
/// folded into the snapshot ([`StartupHealthDto`]), because that is recomputed on every
/// store-changed tick, and inspecting the environment — an FS walk per bound folder — has no
/// business on that path. Detection goes through core's
/// [`amenbo_core::doctor::pointer_issues`] alone, so what surfaces here and what surfaces on the
/// doctor screen ([`doctor_report`]) always agree.
#[tauri::command]
pub fn pointer_issues() -> Result<Vec<DoctorIssueDto>, CmdError> {
    Ok(amenbo_core::doctor::pointer_issues(&open_store_read()?)
        .iter()
        .map(DoctorIssueDto::from)
        .collect())
}

/// Walk the bound git repositories and do what [`amenbo_core::hooks::reconcile`] says about each — the
/// GUI's half of it, where the CLI's is `lint_hook_setup`. Returns whether any of them left the one
/// question live.
///
/// The CLI acts on the repository it was run in. The GUI has no cwd, so it walks every bound folder,
/// taking each folder's own `.amenbo` as the answer to which project it belongs to — the same question an
/// AI started in that folder would resolve. A folder that is not a git repository has no hooks to have and
/// nothing to do.
///
/// This walk is what makes a `yes` device-wide: it reaches the folders bound long after the answer was
/// given, at the next startup, asking nothing. Judgment stays in core — this only carries
/// out what `reconcile` returns, and `Ask` is the only answer that needs a user. Installing is best-effort:
/// a hook is a convenience, and failing the startup over one would help no one.
fn sweep_bound_repos(store: &Store, consent: Option<amenbo_core::hooks::HookConsent>, can_ask: bool) -> bool {
    use amenbo_core::hooks::{self, HookAction};

    let cmd = amenbo_core::config::Paths::command_name();
    let mut question_is_live = false;
    for dir in store.bindings().all_dirs() {
        let path = std::path::Path::new(&dir);
        let Some(project_id) = amenbo_core::binding::read_pointer(path).and_then(|b| b.project_id) else {
            continue;
        };
        let Some(states) = hooks::probe(path) else { continue };
        let opted_out = store.hook_opted_out(project_id).unwrap_or(false);
        match hooks::reconcile(&hooks::HookContext { states: Some(states), consent, opted_out, can_ask }) {
            HookAction::Nothing => {}
            HookAction::Install => {
                let _ = hooks::install(path, cmd);
            }
            HookAction::Ask => question_is_live = true,
        }
        // Heal a block of ours left damaged or stale — the corruption reconcile steps past, since any
        // marker reads to it as a managed slot. It writes only when something is broken, and records what it
        // restored (in session_hook_repairs) so the banner can warn the block had been changed and is back.
        record_hook_repairs(&dir, &hooks::restore_blocks(path, cmd, consent, opted_out));
    }
    question_is_live
}

/// Per bound folder, the names of the slots restored there this session: `(dir, slot names)`.
type HookRepairLog = Vec<(String, Vec<String>)>;

/// What [`restore_blocks`](amenbo_core::hooks::restore_blocks) put back this session, per bound folder — a
/// transient the standing report ([`hook_notices`]) reads to warn about, since a healed block leaves no
/// damage on disk to detect after the fact. Accumulated (a second sweep that heals nothing does not erase
/// the first), and deduped, so [`hook_offer`]'s startup sweep firing twice under StrictMode still reports
/// each repair once.
fn session_hook_repairs() -> &'static std::sync::Mutex<HookRepairLog> {
    static REPAIRS: std::sync::OnceLock<std::sync::Mutex<HookRepairLog>> = std::sync::OnceLock::new();
    REPAIRS.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

/// Record the slots [`restore_blocks`](amenbo_core::hooks::restore_blocks) healed in `dir`, merged into any
/// already recorded for it. A no-op when nothing was restored.
fn record_hook_repairs(dir: &str, restored: &[amenbo_core::hooks::HookSlot]) {
    if restored.is_empty() {
        return;
    }
    let mut all = session_hook_repairs().lock().unwrap_or_else(|e| e.into_inner());
    let entry = match all.iter_mut().find(|(d, _)| d == dir) {
        Some(entry) => &mut entry.1,
        None => {
            all.push((dir.to_string(), Vec::new()));
            &mut all.last_mut().expect("just pushed").1
        }
    };
    for slot in restored {
        let name = slot.name().to_string();
        if !entry.contains(&name) {
            entry.push(name);
        }
    }
}

/// The slots restored in `dir` so far this session (empty when none).
fn hook_repairs_for(dir: &str) -> Vec<String> {
    session_hook_repairs()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(d, _)| d == dir)
        .map(|(_, slots)| slots.clone())
        .unwrap_or_default()
}

/// The one question the GUI should ask about the lint hooks, or `None` when there is nothing to ask —
/// which is the overwhelmingly common case, since it can only ever be asked once on this device.
///
/// The GUI calls it **once, at app startup**, for the reason [`pointer_issues`] is called there and not on
/// the snapshot path: probing costs a `git` spawn per folder, and the environment does not change on a
/// store tick. The same call is what carries an answer already given out to the folders bound since.
#[tauri::command]
pub fn hook_offer() -> Result<Option<HookOfferDto>, CmdError> {
    let store = open_store()?;
    let live = sweep_bound_repos(&store, store.config.hook_consent, true);
    Ok(live.then(|| HookOfferDto { cmd: amenbo_core::config::Paths::command_name().to_string() }))
}

/// Where the lint is not running — the GUI's third channel for it, alongside the CLI's `--json` field and
/// stderr line. This is the standing report ([`amenbo_core::hooks::setup_notice`]), not [`hook_offer`]'s
/// one-time question: it tells and offers no button, because the answer to it was either already given or
/// already declined.
///
/// The GUI calls it **once, after [`hook_offer`] has had its turn**, and that order is the point rather
/// than a detail of scheduling. `hook_offer`'s sweep both installs the hooks a yes wired and heals the
/// damaged blocks it found (recording them in [`session_hook_repairs`]); this then probes the disk that
/// sweep *changed*, so `unwired` names only slots still without a block, and `restored` names what the
/// sweep just put back. A notice computed before the sweep would report slots that are now wired, and would
/// miss the repairs entirely.
#[tauri::command]
pub fn hook_notices() -> Result<Vec<HookNoticeDto>, CmdError> {
    use amenbo_core::hooks;

    let store = open_store_read()?;
    let cmd = amenbo_core::config::Paths::command_name();
    let consent = store.config.hook_consent;
    let mut notices = Vec::new();
    for dir in store.bindings().all_dirs() {
        let path = std::path::Path::new(&dir);
        let Some(project_id) = amenbo_core::binding::read_pointer(path).and_then(|b| b.project_id) else {
            continue;
        };
        let opted_out = store.hook_opted_out(project_id).unwrap_or(false);
        let unwired: Vec<String> = hooks::setup_notice(hooks::probe(path), consent, opted_out)
            .map(|n| n.unwired.iter().map(|s| s.name().to_string()).collect())
            .unwrap_or_default();
        let restored = hook_repairs_for(&dir);
        if unwired.is_empty() && restored.is_empty() {
            continue;
        }
        let Ok(Some(project)) = store.project(project_id) else { continue };
        notices.push(HookNoticeDto { project_name: project.name, dir: dir.clone(), cmd: cmd.to_string(), unwired, restored });
    }
    Ok(notices)
}

/// Write down what the user answered to the [`HookOfferDto`], and carry it out. The answer is the
/// **device's** — one click, once, covering every repository Amenbo works in and the ones bound after —
/// so it lands in `config.hook_consent` and not against whichever project happened to be on screen.
///
/// The record is what decides whether the question is ever asked again, so it is written **only when an
/// answer was actually given**: a modal the user dismissed calls nothing at all, and the device stays
/// unanswered for the next startup to ask again. That is why this takes a `yes` rather than an "outcome" —
/// there is no third value to pass, because the third outcome is this command not running.
///
/// A yes is carried out by the same sweep the startup runs, rather than by a second install path here:
/// the answer's whole meaning is "whatever `reconcile` says, everywhere", and writing that out twice is
/// how the two would come to disagree. Recording comes first so the sweep reads the answer just given.
/// Installing is best-effort per repository — a stranger's slot is not a failure (the install steps around
/// it, and the setup banner says so afterwards), and one unwritable repository must not lose an answer that
/// was about all of them.
#[tauri::command]
pub fn hook_answer(yes: bool) -> Result<(), CmdError> {
    use amenbo_core::hooks::HookConsent;

    let mut store = open_store()?;
    store.config.hook_consent = Some(if yes { HookConsent::Yes } else { HookConsent::No });
    store.save_config()?;
    sweep_bound_repos(&store, store.config.hook_consent, false);
    Ok(())
}

/// Whether the tick's banner has a question to put on this device today (`AMB-D-718`).
///
/// Called **once, at app startup**, after `setup` has settled the answer against the scheduler
/// ([`crate::tick::reconcile`]) — a yes whose registration the user removed is back to unanswered by
/// then, which is exactly the state this is meant to catch. Core makes the whole judgement
/// ([`amenbo_core::tick::banner_shows`]); nothing is weighed here.
///
/// A machine with no store yet says no, the way [`record_launch`] counts nothing there: the question is
/// about work with days on it, and there is none until there is a store to hold it.
#[tauri::command]
pub fn tick_banner() -> Result<bool, CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    if amenbo_core::env::home().is_none() && !paths.store_file.exists() {
        return Ok(false);
    }
    Ok(open_store_read()?.tick_banner_shows(amenbo_core::time::today())?)
}

/// Write down what the reader answered on the tick's banner — the device's one answer, given once
/// (`AMB-D-707` / `AMB-D-718`).
///
/// **On a yes the registration is written first, and the answer only after it came back.** A scheduler
/// that refused leaves the device unanswered, so the config never claims a timer that is not there — the
/// same order [`config_set_autostart`] and the CLI's `tick install` both keep, and for the same reason.
///
/// **And a no takes the registration away first, for the same reason.** The two are one act, the way
/// `tick install` and `tick uninstall` are: this is the road the settings switch takes as well as the
/// band's, and a switch moved to off over a timer that is still held would be the same lie the other way
/// round. Both writes are idempotent, so a no from the band — where the device is unanswered and holds
/// nothing — asks the scheduler for the state it is already in.
///
/// The ack is empty of ids and still worth returning: `config.json` is outside the store, so it is the
/// snapshot reload at the tail of the ack that carries the new answer to the settings switch. That is
/// what keeps the switch in step when the band is what was pressed.
#[tauri::command]
pub fn tick_answer(yes: bool) -> Result<WriteAck, CmdError> {
    use amenbo_core::tick::{self, TickConsent};

    if yes {
        tick::register()?;
    } else {
        tick::unregister()?;
    }
    let paths = amenbo_core::config::Paths::resolve()?;
    let mut config = amenbo_core::config::Config::load(&paths.config_file);
    config.tick_consent = Some(if yes { TickConsent::Yes } else { TickConsent::No });
    config.save(&paths.config_file)?;
    Ok(WriteAck::new(&[]))
}

/// Put the tick's banner off until tomorrow — what **later** records, and the whole of what it records.
///
/// It is not an answer: the question stays open, and the banner comes back the next day the conditions
/// hold. The day is written because this banner spans the whole app and outlives any one screen — held in
/// the webview alone, "later" would be a button that changes nothing past the next launch.
#[tauri::command]
pub fn tick_banner_later() -> Result<(), CmdError> {
    let day = amenbo_core::time::date_to_string(amenbo_core::time::today());
    open_store()?.defer_tick_banner(&day)?;
    Ok(())
}

/// The row for one catalog entry.
fn agent_hook_tool(harness: &amenbo_core::harness::Harness, cmd: &str) -> AgentHookToolDto {
    AgentHookToolDto {
        tool: harness.id.to_string(),
        label: harness.label.to_string(),
        paste_into: harness.paste_into.to_string(),
        request: amenbo_core::harness::request(harness, cmd),
    }
}

/// Write down what the user answered about this project's session-start hook. It is **per project**
/// (`AMB-D-440`), so it takes the project the answer was given about rather than landing on the device.
///
/// **On this surface the answer is a no** (`AMB-D-460`): the standing row is the GUI's only face for this,
/// and the one button on it that records anything is the refusal — which is what silences the row
/// ([`amenbo_core::harness::setup_notice`]). A yes reaches the record from the CLI, where the question is
/// still put; both are read back by [`agent_hook_consent`].
///
/// Whether this was the first asking or the one re-ask is not passed in but read off the record: a
/// consent row already there means the question had been answered before, which is the only occasion
/// [`amenbo_core::harness::reconcile`] puts it again. Recording it as spent
/// ([`Consent::answered_again`](amenbo_core::harness::Consent::answered_again)) is what keeps the re-ask
/// to one — a caller cannot get that wrong, because it never says.
///
/// **Nothing is wired by answering.** Amenbo writes no provider settings file, so a yes buys the text and
/// not the wiring, and the row keeps reporting until the wiring lands.
///
/// Call it **only when there is an answer**: the row's "close" records nothing, and the project stays
/// unanswered for the next opening of it to report again.
#[tauri::command]
pub fn agent_hook_answer(project_id: i64, yes: bool) -> Result<(), CmdError> {
    use amenbo_core::harness::Consent;

    let store = open_store()?;
    let asked_before = store.harness_consent(project_id)?.is_some();
    let answer =
        if asked_before { Consent::answered_again(yes) } else { Consent::answered(yes) };
    store.set_harness_consent(project_id, answer)?;
    Ok(())
}

/// What this project answered about starting its AI on Amenbo: `true` for yes, `false` for no, and
/// `None` for a project that has never been asked. Three values and not two — a screen that folded
/// "never asked" into "no" would report a refusal nobody gave, and a refusal is the one answer that
/// silences the standing row (`AMB-D-459`, `AMB-D-460`).
///
/// This is the settings screen reading the record, so it says only what was answered. Whether the
/// wiring is actually in place is a different fact, read from the folder every time and reported by
/// [`agent_hook_project_wiring`].
#[tauri::command]
pub fn agent_hook_consent(project_id: i64) -> Result<Option<bool>, CmdError> {
    let store = open_store_read()?;
    Ok(store.harness_consent(project_id)?.map(|had| had.allowed))
}

/// Take this project's answer off the record, back to never having been asked — the way out of a
/// refusal, which silences the standing row on its own (`AMB-D-459`). Opening the project again brings
/// the row back, since the state it reads is the one a project starts in.
///
/// Clearing a project that never answered is the state asked for rather than an error.
#[tauri::command]
pub fn agent_hook_consent_clear(project_id: i64) -> Result<(), CmdError> {
    let store = open_store()?;
    store.clear_harness_consent(project_id)?;
    Ok(())
}

/// What this project still has to wire, folder by folder — the standing row on the project screen, which
/// is the GUI's only face for this at all: it reports the work, hands over the text, and carries the
/// refusal that ends it (`AMB-D-459`, `AMB-D-460`).
///
/// **It answers for one project, because that is where it is read.** Consent is recorded per project and
/// the wiring lands per folder, so a reader who pasted into one of four folders is answered as done and
/// told nothing about the other three. This walks that project's folders and reports each one that is
/// still waiting, which is what lets the row stand until the last of them is wired and then go by itself.
///
/// **Grouped by harness, in catalog order** ([`amenbo_core::harness::HARNESSES`]), so the surface can put
/// the text up once and list its folders under it. A folder that traces no provider is offered the whole
/// catalog — it appears under every tool, and whichever one the reader picks is the one they are handed.
///
/// A folder that is not there is skipped: nothing can be pasted into it, so a row naming it would be one
/// the reader cannot end. So is one reached over MCP with no sign of an AI being worked with in it
/// ([`mcp_reaches`], [`amenbo_core::harness::ai_in_use`]) — the hook this asks for runs in a shell, and
/// nothing there opens one (`AMB-D-680`). What silences the report otherwise is core's
/// ([`amenbo_core::harness::setup_notice`]) — a refusal, or the wiring landing. A standing yes is not
/// among them: consent is not wiring.
///
/// Which folders are waiting is the whole of what this answers; when the row is drawn is the surface's
/// own (`AMB-D-516`). A project with no task in it holds the first loop, and the board keeps this row
/// back until a task lands — so an answer with folders in it is not on its own a row on screen.
#[tauri::command]
pub fn agent_hook_project_wiring(project_id: i64) -> Result<Vec<AgentHookWiringDto>, CmdError> {
    use amenbo_core::harness;

    let store = open_store_read()?;
    let cmd = amenbo_core::config::Paths::command_name();
    let consent = store.harness_consent(project_id).unwrap_or(None);
    let mut waiting: Vec<Vec<String>> = vec![Vec::new(); harness::HARNESSES.len()];
    let registry = store.bindings();
    for dir in registry.dirs_for_project(project_id) {
        let path = std::path::Path::new(dir);
        if !path.is_dir() {
            continue;
        }
        let found = harness::probe(path, cmd);
        // Asked in this order because the cheap half answers nearly every folder: a folder anybody works
        // an AI in is reported whatever MCP holds, and only a traceless one is worth reading settings
        // files for.
        if !harness::ai_in_use(path, &found) && mcp_reaches(path) {
            continue;
        }
        let Some(notice) = harness::setup_notice(&found, consent) else {
            continue;
        };
        for one in harness::offered(&notice) {
            if let Some(at) = harness::HARNESSES.iter().position(|row| row.id == one.id) {
                waiting[at].push(dir.to_string());
            }
        }
    }
    Ok(harness::HARNESSES
        .iter()
        .zip(waiting)
        .filter(|(_, dirs)| !dirs.is_empty())
        .map(|(one, dirs)| AgentHookWiringDto { tool: agent_hook_tool(one, cmd), dirs })
        .collect())
}

/// Whether an app already reaches **this folder** over MCP (`AMB-D-680`) — the same read-back the
/// settings screen draws its rows from ([`amenbo_core::mcp_probe`]), asked here of one folder rather
/// than of a project. There is no second way of reading it: an entry Amenbo would not report as set up
/// is not one this may act on either.
///
/// **The entry has to name this folder**, and not merely be there — which is why the folders it reaches
/// are read rather than [`Setup::set`](amenbo_core::mcp_probe::Setup::set). Most of the catalog keeps
/// one settings file for the whole machine, so an entry a reader wrote for some other project sits in
/// the same file as this one's would, and "there is an entry" would silence the report for every
/// traceless folder on the device. `AMB-D-680` says which way to fall when the answer is unclear: a
/// notice shown to somebody who did not need it is noise they can close, and one withheld is a setup
/// they never learn is unfinished.
fn mcp_reaches(dir: &std::path::Path) -> bool {
    use amenbo_core::{mcp::Server, mcp_probe};

    let folders = [dir.to_path_buf()];
    let exe = mcp_exe();
    let server = Server { folders: &folders, exe: &exe };
    mcp_probe::probe(&server).iter().any(|found| found.folders.iter().any(|at| at == dir))
}

/// The request for any tool in the catalog, whatever this project has already wired (`AMB-D-670`).
///
/// **It hangs on nothing.** [`agent_hook_project_wiring`] reads the folders through
/// [`amenbo_core::harness::setup_notice`], which falls silent once the wiring lands or a refusal is
/// recorded — and the standing row and the waiting-folders list go with it. That silence is right for a
/// report of work left and wrong for a face someone pressed: a reader who wired Claude Code and then
/// moved to Codex CLI had no way to the text at all. So this reads no notice and no consent, and answers
/// the same before and after.
///
/// **The whole catalog, not the traces.** A tool being moved to has left nothing in the folder yet, which
/// is the same reasoning that makes [`amenbo_core::harness::offered`] hand a traceless folder every row.
/// Picking is the reader's.
///
/// **Every bound folder, including one that is not on disk.** What is answered is where this project is,
/// and a folder gone missing is a fact the settings screen already reports beside this. Skipping it here
/// would take a paste target off the list for a reason the reader is being told about a section away.
#[tauri::command]
pub fn agent_hook_requests(project_id: i64) -> Result<AgentHookRequestsDto, CmdError> {
    use amenbo_core::harness;

    let store = open_store_read()?;
    let cmd = amenbo_core::config::Paths::command_name();
    let dirs = store.bindings().dirs_for_project(project_id).iter().map(|d| d.to_string()).collect();
    let tools = harness::HARNESSES.iter().map(|one| agent_hook_tool(one, cmd)).collect();
    Ok(AgentHookRequestsDto { tools, dirs })
}

/// The Amenbo a host will run: the command shipped beside this build's own binary.
///
/// A bundle and a request both name a path rather than a command word, because the host that runs it
/// is not a shell and has no `PATH` of the reader's to look one up in. The CLI ships as a sidecar next
/// to the app's binary, so that is where it is looked for; where it is not found — an unusual install,
/// or a run out of a build tree — the command's own name is the best that can be said, and a reader
/// whose `PATH` carries it is still reached.
///
/// One name is looked for, because the bundle carries the CLI under this build's own name and that
/// is the same word this build's guidance tells someone to type (`Paths::sidecar_file_name`). Where
/// it is not found the bare name is still the best that can be said, and a reader whose `PATH`
/// carries it is reached anyway.
fn mcp_exe() -> std::path::PathBuf {
    let named = amenbo_core::config::Paths::sidecar_file_name();
    let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|at| at.to_path_buf()));
    if let Some(at) = beside {
        let file = at.join(&named);
        if file.is_file() {
            return file;
        }
    }
    std::path::PathBuf::from(named)
}

/// The folders every project a server could be pointed at is worked in, in the store's own order.
///
/// One folder per project — the first it has bound. A server answers about the folder it is sent to
/// rather than about a project (the Amenbo it starts works the project out from where it stands), so
/// where a project has several, any one of them reaches the same backlog; the one named is written
/// into the text the reader is handed, so nothing about which is silent. A project with none is not
/// here at all: there is nowhere to point a server, and a row offering to would write an entry naming
/// nothing.
fn mcp_projects(
    store: &amenbo_core::store::Store,
) -> Result<Vec<(i64, String, std::path::PathBuf)>, CmdError> {
    let registry = store.bindings();
    let read_model = store.read_model();
    Ok(amenbo_core::store_engine::read::project_overview(read_model.conn(), store.reach())?
        .into_iter()
        .filter_map(|project| {
            let dir = registry.dirs_for_project(project.id).into_iter().next()?;
            Some((project.id, project.name, std::path::PathBuf::from(dir)))
        })
        .collect())
}

/// The folders a set of projects is worked in, for the selection a row was ticked with.
///
/// A project the store no longer holds is dropped rather than refused: the screen and the store move
/// apart the moment somebody deletes one in another window, and a request built from what is left is
/// the answer that still means something.
fn mcp_folders_of(
    store: &amenbo_core::store::Store,
    project_ids: &[i64],
) -> Result<Vec<std::path::PathBuf>, CmdError> {
    let known = mcp_projects(store)?;
    Ok(project_ids
        .iter()
        .filter_map(|id| known.iter().find(|(known, _, _)| known == id))
        .map(|(_, _, folder)| folder.clone())
        .collect())
}

/// Everything the screen that connects an AI draws (`AMB-D-681`): the projects that can be reached,
/// and every app in catalog order with what it already holds (`AMB-D-673`).
///
/// It asks about **every** project's folder, not one — half the listed apps keep their settings inside
/// a folder, so an entry written beside one project cannot be seen from another
/// ([`amenbo_core::mcp_probe`]).
#[tauri::command]
pub fn mcp_setup() -> Result<McpSetupDto, CmdError> {
    use amenbo_core::{mcp::Server, mcp_apps, mcp_probe, mcp_request};

    let store = open_store_read()?;
    let projects = mcp_projects(&store)?;
    let folders: Vec<std::path::PathBuf> =
        projects.iter().map(|(_, _, folder)| folder.clone()).collect();
    let exe = mcp_exe();
    let asking = Server { folders: &folders, exe: &exe };

    let apps = mcp_apps::MCP_APPS
        .iter()
        .zip(mcp_probe::probe(&asking))
        .map(|(app, found)| McpAppDto {
            app: app.id.to_string(),
            label: app.label.to_string(),
            writes_file: app.amenbo_writes,
            configured: found.set,
            folders: found.folders.iter().map(|at| at.display().to_string()).collect(),
            stale: found
                .stale
                .into_iter()
                .map(|old| {
                    // The request has to name the file the old entry actually sits in, which for an
                    // app that keeps its settings inside a folder is the folder it was found in.
                    let at: Vec<std::path::PathBuf> = old.at.into_iter().collect();
                    let where_it_is = Server { folders: &at, exe: &exe };
                    McpStaleDto {
                        remove_request: mcp_request::remove_stale(app, &where_it_is, &old.name),
                        name: old.name,
                        folder: old.folder.map(|at| at.display().to_string()),
                    }
                })
                .collect(),
        })
        .collect();

    Ok(McpSetupDto {
        projects: projects
            .into_iter()
            .map(|(id, name, folder)| McpProjectDto {
                id,
                name,
                folder: folder.display().to_string(),
            })
            .collect(),
        apps,
    })
}

/// The two texts one app's row hands over, for the projects ticked on it (`AMB-D-681`).
///
/// The whole selection is what they carry, every time: the request asks for the entry to be replaced
/// rather than added to, so the second time round is the same move as the first. An empty selection
/// still answers — with the request to take the entry out, which is what "none of them" means.
///
/// **The two are addressed to different files.** What the add names is where the entry is to be
/// written, which the ticks decide. What the removal names is where the entry already is, which they do
/// not: a reader who unticks one folder and ticks another is asking for the entry they have to go, and
/// a request built from the new ticks would send their AI to a file that entry was never in. So the
/// removal is addressed off the read-back ([`amenbo_core::mcp_probe::Setup::at`]), asked of every
/// project's folder the way the screen asks it.
#[tauri::command]
pub fn mcp_request_for(app: String, project_ids: Vec<i64>) -> Result<McpRequestDto, CmdError> {
    use amenbo_core::{mcp::Server, mcp_apps, mcp_probe, mcp_request};

    let Some(app) = mcp_apps::find(&app) else {
        return Err(CmdError::coded(
            "mcp.no_app",
            "no app is listed under that name",
            serde_json::Value::Null,
        ));
    };
    if app.amenbo_writes {
        // Nothing to hand anybody: this one takes a file, and the button beside it writes one.
        return Ok(McpRequestDto { add: String::new(), remove: String::new() });
    }
    let store = open_store_read()?;
    let ticked = mcp_folders_of(&store, &project_ids)?;
    let exe = mcp_exe();
    let chosen = Server { folders: &ticked, exe: &exe };

    // Where the entry actually sits, read across every project's folder — an entry written beside one
    // project cannot be seen from another, so asking only the ticked ones would miss the very folder
    // the reader has just unticked.
    let everywhere: Vec<std::path::PathBuf> =
        mcp_projects(&store)?.into_iter().map(|(_, _, folder)| folder).collect();
    let holding = mcp_probe::read(app, &Server { folders: &everywhere, exe: &exe }).at;
    // Nothing held anywhere is either an app that is not set up — whose row draws no removal at all —
    // or one whose settings are the machine's, where the folder is not read in resolving that path.
    // Either way the ticks are as good an answer as there is.
    let holding = if holding.is_empty() { ticked.clone() } else { holding };
    let standing = Server { folders: &holding, exe: &exe };

    Ok(McpRequestDto {
        add: mcp_request::add(app, &chosen),
        remove: mcp_request::remove(app, &standing),
    })
}

/// Write the bundle for the projects ticked into `into_dir`, and hand back the file that was written
/// (`AMB-D-672`).
///
/// The folder is the reader's to choose, asked for on the surface: what happens to this file next is
/// that they open it, so it has to land somewhere they can find. Amenbo writes nothing into the app's
/// own settings — the file is the hand-over, and the app is the thing that reads it.
#[tauri::command]
pub fn mcp_bundle_write(project_ids: Vec<i64>, into_dir: String) -> Result<String, CmdError> {
    use amenbo_core::{mcp::Server, mcp_bundle};

    let store = open_store_read()?;
    let folders = mcp_folders_of(&store, &project_ids)?;
    if folders.is_empty() {
        return Err(CmdError::coded(
            "mcp.no_folder",
            "no project was chosen for a server to be pointed at",
            serde_json::Value::Null,
        ));
    }
    let exe = mcp_exe();
    let server = Server { folders: &folders, exe: &exe };
    let written = mcp_bundle::write_into(&server, std::path::Path::new(&into_dir))
        .map_err(|e| CmdError::coded("mcp.bundle_write", e.to_string(), serde_json::Value::Null))?;
    Ok(written.display().to_string())
}

/// Fix a broken `.amenbo` (old format, or gone) **right there**. The repair button on the startup
/// health banner calls it. Core already knows how — run Amenbo in that folder and `resolve_upward`
/// quietly fixes it — so we put the same fix within reach in the banner, and the user does not have
/// to go hunting through the settings screen. All it writes is each folder's `.amenbo`; the store is
/// untouched, so there is no snapshot to refetch.
#[tauri::command]
pub fn repair_pointers() -> Result<PointerRepairDto, CmdError> {
    let repair = amenbo_core::binding::repair_pointers(&open_store_read()?);
    Ok(PointerRepairDto { repaired: repair.repaired, unresolved: repair.unresolved })
}

/// The read-only command the GUI's doctor screen (Settings > Integrity) reads. It goes over **the
/// same core path** as the CLI's `doctor` (`doctor::report` — the store's internal consistency plus
/// this machine's environment), so the issues raised on the two surfaces never diverge (only the
/// prose differs: the GUI's UI language, the CLI's English). The startup health banner
/// ([`StartupHealthDto`]) sees only the store-internal doctor and the binding pointers
/// ([`pointer_issues`]); stale managed blocks and debris folder rows have banners of their own. So
/// this screen is the only place that shows **all of it together**.
#[tauri::command]
pub fn doctor_report() -> Result<DoctorReportDto, CmdError> {
    let store = open_store_read()?;
    let result = amenbo_core::doctor::report(&store)?;
    Ok(DoctorReportDto {
        ok: result.ok,
        errors: result.summary.error,
        warnings: result.summary.warning,
        issues: result.issues.iter().map(DoctorIssueDto::from).collect(),
    })
}

/// Run the repair from the GUI, calling **the same core cleanup entry points** as the CLI's
/// `doctor --fix`, in the same order. Every one of them is **non-destructive** (attachment rows whose
/// record is gone; blobs nothing references; folder rows nobody claims), so the surface may run it
/// without asking for
/// confirmation. And since nothing it cleans up is referenced by a single row of any live read,
/// there is no snapshot and no query to refetch — hence no `WriteAck`.
#[tauri::command]
pub fn doctor_fix() -> Result<DoctorFixDto, CmdError> {
    let mut store = open_store()?;
    // Ahead of the blob sweep, as on the CLI: an orphaned attachment holds its hash in the GC root set,
    // so its bytes are not collectible until the row is.
    let swept_attachments = store.sweep_orphan_attachments()?;
    let gc = store.gc_blobs(amenbo_core::blob::GC_MIN_AGE)?;
    let forgotten_bindings = store.forget_orphan_dirs()?;
    Ok(DoctorFixDto {
        swept_attachments,
        reclaimed_blobs: gc.removed as usize,
        freed_bytes: gc.freed_bytes as usize,
        forgotten_bindings,
    })
}

/// The shortest path to an update: open this OS's all-in-one installer (GUI and CLI together) in the
/// OS's default browser. Core resolves the installer URL for the current platform from the published
/// `latest.json` — falling back to the latest release page when it has not been fetched, is not
/// listed, or the check is disabled by the environment — and `os_open` opens it. There is no
/// self-update; it only opens. Because this is an explicit user action (the button on the update
/// banner), it goes and fetches regardless of the update_check toggle in config. Returns the URL it
/// opened, which the front end can display or log. The store is untouched: the only side effect is
/// launching an external browser.
#[tauri::command]
pub fn open_latest_installer() -> Result<String, CmdError> {
    let url = amenbo_core::update_check::resolve_update_url();
    os_open(&url).map_err(|e| -> CmdError { format!("cannot open the installer URL: {e}").into() })?;
    Ok(url)
}

/// The reader's own lines for the merged list, or nothing at all when they read the base language
/// (`AMB-D-394`).
///
/// English is where every untranslated line already falls back to, so no catalog publishes a
/// `catalog.en.json` for it — asking would be a request that 404s on every browse, and nothing is
/// cached from a miss. So the base language is answered here rather than at the catalog.
fn list_lines(
    paths: &amenbo_core::config::Paths,
    view: &amenbo_core::plugin_catalog::Discovery,
    lang: &str,
) -> amenbo_core::plugin_catalog::ListTranslations {
    if lang == BASE_LANGUAGE {
        return Default::default();
    }
    amenbo_core::plugin_catalog::list_translations(paths, view, lang)
}

/// The language every translated field falls back to (`AMB-D-394`) — the one the author wrote the
/// manifest in, and the one Amenbo's own catalog documents are published in.
const BASE_LANGUAGE: &str = amenbo_core::config::LANGUAGES[0];

/// Hand the GUI the merged plugin catalog for browsing (`AMB-D-347`): the official catalog plus every
/// registered third-party one, folded into one de-duplicated list by
/// [`amenbo_core::plugin_catalog::discover`].
///
/// **One fetch feeds the whole screen.** Filtering, searching and paging are the front end's, over
/// the list this returns — the browse never goes back to the network per keystroke, and never asks
/// GitHub about an entry it is merely listing. Each catalog is read the incidental way (a cache
/// inside the freshness window answers with no request), so re-opening the screen inside the hour
/// costs nothing, and a source that cannot be reached is reported as unreachable rather than failing
/// the view. The fetch goes off the main thread via `spawn_blocking`, because a dead source is only
/// found out by waiting for its timeout.
///
/// `lang` is the language the reader is reading Amenbo in, and it is the caller's to say
/// (`AMB-D-623`): the list half of a translation is a document per language (`AMB-D-622`), so the one
/// being asked for has to travel with the ask. It rides on the row beside the base line rather than
/// over it — a plugin this language has no line for is drawn in English, and the row says nothing
/// about having fallen back.
#[tauri::command]
pub async fn plugin_catalog_browse(lang: String) -> Result<PluginCatalogDto, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<PluginCatalogDto, CmdError> {
        let paths = amenbo_core::config::Paths::resolve()?;
        let discovery = amenbo_core::plugin_catalog::discover(&paths);
        let lines = list_lines(&paths, &discovery, &lang);
        Ok(PluginCatalogDto {
            entries: discovery
                .entries
                .into_iter()
                .map(|e| PluginEntryDto {
                    desc_i18n: lines.get(&e.entry.name).and_then(|o| o.desc.clone()),
                    name: e.entry.name,
                    title: e.entry.title,
                    desc: e.entry.desc,
                    author: e.entry.author,
                    repo: e.entry.repo,
                    os: e.entry.os.iter().map(|o| o.as_str().to_string()).collect(),
                    category: e.entry.category,
                    official: e.entry.official,
                    listed: e.listed,
                    source: e.source,
                    source_name: e.source_name,
                    featured: e.entry.featured,
                    added_at: e.entry.added_at,
                })
                .collect(),
            sources: discovery
                .sources
                .into_iter()
                .map(|s| PluginCatalogSourceDto {
                    url: s.url,
                    name: s.name,
                    fingerprint: s.fingerprint,
                    official: s.official,
                    reachable: s.reachable,
                    offered: s.offered,
                })
                .collect(),
            dropped: discovery.dropped.len(),
        })
    })
    .await
    .map_err(|e| -> CmdError { format!("fetching the plugin catalog did not finish: {e}").into() })?
}

/// Work out what registering `url` would mean, writing nothing — the read half of registration
/// (`AMB-D-389`).
///
/// The face shows the fingerprint this answers with, takes the user's agreement, and only then calls
/// [`plugin_catalog_add_source`] with the fingerprint that was agreed to.
///
/// Off the main thread, like browsing: it fetches the key document beside the catalog, and a host that
/// is not there is only found out by waiting for its timeout.
#[tauri::command]
pub async fn plugin_catalog_probe_source(url: String) -> Result<PluginCatalogProbeDto, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<PluginCatalogProbeDto, CmdError> {
        let paths = amenbo_core::config::Paths::resolve()?;
        let probe = amenbo_core::plugin_catalog::probe_source(&paths, &url)?;
        Ok(PluginCatalogProbeDto {
            pins_a_new_key: probe.pins_a_new_key(),
            url: probe.url,
            suggested_name: probe.suggested_name,
            fingerprint: probe.fingerprint,
            registered: probe.registered,
        })
    })
    .await
    .map_err(|e| -> CmdError { format!("probing the source did not finish: {e}").into() })?
}

/// Judge the agreement the screen took against the pin that is about to be written (`AMB-D-389`).
///
/// Registration in the GUI crosses a process boundary between showing a fingerprint and agreeing to
/// it, so this door re-reads what the catalog publishes and goes ahead only when the answer is still
/// what was on screen. The CLI needs none of this: it confirms with the probe in hand.
///
/// - nothing new would be pinned — no key, or the key is already the pinned one: no agreement is
///   asked for, because registering is a bookmark again and only the name can change.
/// - a new key, agreed to by that fingerprint: the ordinary case.
/// - a new key and no agreement at all: refused. A door that pins on the caller's silence is not
///   consent.
/// - a new key and an agreement naming a different one: refused. What was read is not what would be
///   pinned, so the answer is to look again — never to pin the one that arrived second.
fn agreed_pin(
    probe: &amenbo_core::plugin_catalog::SourceProbe,
    agreed: Option<&str>,
) -> Result<(), CmdError> {
    if !probe.pins_a_new_key() {
        return Ok(());
    }
    let serving = probe.fingerprint.as_deref().unwrap_or_default();
    match agreed {
        Some(agreed) if agreed == serving => Ok(()),
        Some(agreed) => Err(CmdError::coded(
            "plugin_catalog_key_changed",
            format!(
                "{} now publishes {serving}, not the {agreed} you were shown — register it again and check the new fingerprint.",
                probe.url
            ),
            serde_json::json!({ "url": probe.url, "agreed": agreed, "serving": serving }),
        )),
        None => Err(CmdError::coded(
            "plugin_catalog_consent_required",
            format!(
                "registering {} trusts its signing key ({serving}) — agree to the fingerprint before it is pinned.",
                probe.url
            ),
            serde_json::json!({ "url": probe.url, "serving": serving }),
        )),
    }
}

/// Register a third-party catalog so browsing shows what it offers (`AMB-D-347`), pinning the key it
/// publishes against the fingerprint the user agreed to (`AMB-D-389`). Returns `false` when the
/// registration already said exactly this — idempotent, not an error.
///
/// `agreed_fingerprint` is what the consent screen showed and the user accepted (see [`agreed_pin`]);
/// it is required exactly when a key would be pinned that is not pinned yet, and may be omitted when
/// nothing new is being trusted — a catalog that publishes no key, or a re-registration that only
/// changes the name.
///
/// Core refuses a URL that is not `http(s)://…`, the official catalog's own URL (it is not a
/// third-party source and is merged first anyway), and a catalog that now publishes a **different**
/// key than the one pinned: trusting a new key is unregistering and registering again.
///
/// The key is fetched here (one small document beside the catalog); the entries themselves arrive when
/// the caller browses again.
#[tauri::command]
pub async fn plugin_catalog_add_source(
    url: String,
    name: Option<String>,
    agreed_fingerprint: Option<String>,
) -> Result<bool, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<bool, CmdError> {
        let paths = amenbo_core::config::Paths::resolve()?;
        let probe = amenbo_core::plugin_catalog::probe_source(&paths, &url)?;
        agreed_pin(&probe, agreed_fingerprint.as_deref())?;
        Ok(amenbo_core::plugin_catalog::add_source(&paths, &probe, name.as_deref())?)
    })
    .await
    .map_err(|e| -> CmdError { format!("registering the source did not finish: {e}").into() })?
}

/// Unregister a third-party catalog and drop its cached copy (`AMB-D-347`). Returns `false` when the
/// URL was not registered — idempotent, like its opposite.
///
/// Removing a source removes nothing else: a plugin already installed from it stays installed and
/// enabled, because the catalog is where a plugin was *found*, not what keeps it running
/// (`AMB-D-350`).
#[tauri::command]
pub fn plugin_catalog_remove_source(url: String) -> Result<bool, CmdError> {
    let paths = amenbo_core::config::Paths::resolve()?;
    amenbo_core::plugin_catalog::remove_source(&paths, &url).map_err(CmdError::from)
}

/// Read the figures for the **one** plugin a user opened (`AMB-D-347`).
///
/// This is the detail's counterpart to [`plugin_catalog_browse`], and the one place the market talks
/// to GitHub. The list never does: stars, downloads and a README are per-repository, so fetching them
/// for a list would be exactly the "one request per plugin" shape the catalog exists to avoid. Core
/// caches per repository and answers from that cache well past the hour, because GitHub's
/// unauthenticated rate limit — not freshness — is what bounds this.
///
/// Failure is partial by design: what did not answer comes back absent, and the detail draws what it
/// has. An error here means nothing about the repository could be read at all, which the front end
/// shows as a note beside a detail that is otherwise complete from the catalog. Off the main thread,
/// because up to three requests run in sequence behind it.
///
/// `readme` is the caller saying whether it would draw one (`AMB-D-638`). A plugin whose author wrote
/// a description of it draws that instead, so the README is neither shown nor fetched — the front end
/// is what knows which of the two it is about to draw, so the ask is where the drawing is decided.
#[tauri::command]
pub async fn plugin_repo_facts(repo: String, readme: bool) -> Result<PluginRepoFactsDto, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<PluginRepoFactsDto, CmdError> {
        use amenbo_core::plugin_github::Readme;
        let paths = amenbo_core::config::Paths::resolve()?;
        let wanted = if readme { Readme::Fetch } else { Readme::Skip };
        let facts = amenbo_core::plugin_github::facts(&paths, &repo, wanted)?;
        Ok(PluginRepoFactsDto {
            stars: facts.stars,
            downloads: facts.downloads,
            readme: facts.readme,
            rate_limited: facts.rate_limited,
        })
    })
    .await
    .map_err(|e| -> CmdError { format!("fetching from GitHub did not finish: {e}").into() })?
}

/// Read the detail document for the **one** plugin a user opened (`AMB-D-385`).
///
/// The list stays one static file for everyone; this is the second half of that bargain, fetched for a
/// single entry and only when it is looked at — the same lazy shape, and the same place, as the stars and
/// the README ([`plugin_repo_facts`]).
///
/// It resolves against the **merged** view — the same catalogs the list drew from, and the same view an
/// install resolves against (`AMB-D-389`). The row a user opened is the thing being described, so the
/// question this answers has to be asked of whichever catalog served that row: the second half of an
/// entry lives beside its own `catalog.json`, and asking the official base for a registered catalog's
/// plugin would be asking the wrong publisher. Anything else would leave a whole tier listed but
/// unreadable — the events, the settings, the compatibility verdict are exactly what someone wants
/// *before* installing.
///
/// A name no catalog carries comes back as `null` — an answer, not a failure — so the market draws what
/// it has instead of an error.
///
/// `lang` is the reader's language, for the description text and the form labels this answers with
/// (`AMB-D-623`). Nothing is fetched for it: the detail document carries **every** language at once
/// (`AMB-D-622`), so the one asked for is picked out of what has already arrived, and a language its
/// author did not write leaves both as they are.
///
/// Off the main thread: it may fetch (core answers from its cache within the freshness window, and falls
/// back to it when the network does not answer).
#[tauri::command]
pub async fn plugin_detail(
    name: String,
    lang: String,
) -> Result<Option<PluginDetailDto>, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<PluginDetailDto>, CmdError> {
        let paths = amenbo_core::config::Paths::resolve()?;
        let discovery = amenbo_core::plugin_catalog::discover(&paths);
        let Some(found) = discovery.find(&name) else {
            return Ok(None);
        };
        let detail = amenbo_core::plugin_catalog::detail_of(&paths, found)?;
        // The compatibility gate reads a whole manifest, so the two halves are put back together once
        // here rather than teaching the gate to read a document at a time. The translations ride in the
        // detail already, keyed by language, so they are read off it rather than out of the join: this
        // face wants one language's labels, and the join's answer is every language's of both halves.
        let (manifest, _translations) =
            amenbo_core::plugin_wire::join(&found.entry, &Default::default(), &detail);
        let why = amenbo_core::plugin_compat::check(&manifest).err();
        Ok(Some(PluginDetailDto {
            events: detail.events.iter().map(|e| e.event.clone()).collect(),
            config: wanted_settings(
                &detail.config,
                detail.i18n.get(&lang).map(|o| &o.config),
                found.entry.official,
            ),
            about: detail.about.clone(),
            about_i18n: detail.i18n.get(&lang).and_then(|o| o.about.clone()),
            scope: detail.scope,
            compatible: why.is_none(),
            incompatible_reason: why.map(|why| why.to_string()),
        }))
    })
    .await
    .map_err(|e| -> CmdError { format!("fetching the plugin's detail did not finish: {e}").into() })?
}

/// One declared setting as its own DTO — the author's words, wherever a face asks what a plugin wants
/// without standing in a project, and whatever they wrote of them in the reader's language beside them.
///
/// `overlay` is this **one field's** translated half, already looked up by the field's key
/// (`AMB-D-621`): a translation carries no order of its own, so the pairing is by key at every step,
/// candidates included. Nothing is resolved here — both halves travel, and the face picks
/// (`AMB-D-623`).
/// The conditions a face still has to judge, as the DTO carries them (`AMB-D-727`) — the platform's half
/// already settled, so `None` is a thing this build's OS hides and is never drawn at all.
fn wanted_when(when: &[amenbo_core::plugin_when::When]) -> Option<Vec<PluginWhenDto>> {
    Some(
        amenbo_core::plugin_when::after_platform(when)?
            .into_iter()
            .map(|c| PluginWhenDto { field: c.field, has: c.has })
            .collect(),
    )
}

fn wanted_setting(
    field: &amenbo_core::plugin_manifest::ConfigField,
    overlay: Option<&amenbo_core::plugin_manifest::ConfigFieldOverlay>,
    when: Vec<PluginWhenDto>,
) -> PluginWantedSettingDto {
    PluginWantedSettingDto {
        when,
        key: field.key.clone(),
        label: field.label.clone(),
        label_i18n: overlay.and_then(|o| o.label.clone()),
        help: field.help.clone(),
        help_i18n: overlay.and_then(|o| o.help.clone()),
        placeholder: field.placeholder.clone(),
        placeholder_i18n: overlay.and_then(|o| o.placeholder.clone()),
        readonly: field.readonly,
        secret: field.secret,
        required: field.required,
        field_type: field.field_type,
        // A candidate this platform hides is dropped here rather than drawn greyed: an iCloud checkbox on
        // Windows is not a choice someone could make (`AMB-D-727`).
        options: field
            .options
            .iter()
            .filter_map(|o| {
                Some(PluginConfigOptionDto {
                    when: wanted_when(&o.when)?,
                    label_i18n: overlay.and_then(|f| f.options.get(&o.value).cloned()),
                    value: o.value.clone(),
                    label: o.label.clone(),
                })
            })
            .collect(),
        default_value: field.default.clone(),
    }
}

/// The whole declared form, each field carrying the reader's language beside the author's
/// (`AMB-D-621`) — the shape every face that draws settings asks for.
///
/// `overlay` is the language's own half of the translations, keyed by field key; `None` is a plugin
/// nobody translated, or a reader on the base language, and both mean the form draws as it always did.
fn wanted_settings(
    config: &[amenbo_core::plugin_manifest::ConfigEntry],
    overlay: Option<&std::collections::BTreeMap<String, amenbo_core::plugin_manifest::ConfigFieldOverlay>>,
    official: bool,
) -> Vec<PluginFormEntryDto> {
    use amenbo_core::plugin_manifest::ConfigEntry;
    config
        .iter()
        .filter_map(|entry| match entry {
            // A setting this platform hides is dropped whole (`AMB-D-727`); what is left of its condition
            // travels with it, for the form to re-read as its answers change.
            ConfigEntry::Field(field) => {
                let when = wanted_when(&field.when)?;
                Some(PluginFormEntryDto::Field {
                    field: wanted_setting(field, overlay.and_then(|o| o.get(&field.key)), when),
                })
            }
            // A destination is an official plugin's to draw (`AMB-D-727`), and this is where a third
            // party's is dropped: the validator tells an author their `qr` will not draw, and a manifest
            // that reached a machine anyway is answered here rather than trusted.
            ConfigEntry::Part(part) if part.part.official_only() && !official => None,
            // A part goes the way its neighbouring setting does (`AMB-D-727`): the platform's half of its
            // condition is settled here, and what reads another setting's answer travels with it. A
            // caption that outlived the box it is about would leave a step nobody could follow.
            ConfigEntry::Part(part) => Some(PluginFormEntryDto::Part {
                when: wanted_when(&part.when)?,
                part: show_part(&part.part),
            }),
        })
        .collect()
}

/// The operations a plugin's settings block declares, each with its words in the reader's language
/// (`AMB-D-664`) — empty for a plugin that declares none, which is the form's own answer to whether there
/// is anything to press.
///
/// `check` has no counterpart here: it is not a button, it is what an enable raises on its own, and what
/// it said comes back with the gate ([`PluginCheckDto`]).
fn wanted_actions(
    settings: Option<&amenbo_core::plugin_manifest::Settings>,
    overlay: Option<&amenbo_core::plugin_manifest::SettingsOverlay>,
) -> Vec<PluginActionDto> {
    let Some(settings) = settings else { return Vec::new() };
    settings
        .actions
        .iter()
        .filter_map(|action| {
            let when = wanted_when(&action.when)?;
            let translated = overlay.and_then(|o| o.actions.get(&action.cmd));
            Some(PluginActionDto {
                when,
                cmd: action.cmd.clone(),
                label: action.label.clone(),
                label_i18n: translated.and_then(|a| a.label.clone()),
                ask: action
                    .ask
                    .iter()
                    .map(|field| PluginAskDto {
                        label_i18n: translated.and_then(|a| a.ask.get(&field.key).cloned()),
                        key: field.key.clone(),
                        label: field.label.clone(),
                        secret: field.secret,
                    })
                    .collect(),
            })
        })
        .collect()
}

/// Read one declared setting into its DTO, at the layer this plugin's values live at
/// (`AMB-D-434` / `AMB-D-601`). The author's `secret` flag decides which of the two tables the value came
/// from, and it is the only thing read here to route the probe (`AMB-D-356`). A secret's value never leaves
/// core — only whether one is held.
fn config_field_row(
    store: &Store,
    plugin: &str,
    field: &amenbo_core::plugin_manifest::ConfigField,
    layer: amenbo_core::plugin_layer::Layer,
) -> Result<PluginConfigFieldDto, CmdError> {
    let held = amenbo_core::plugin_config::get(store, field, plugin, layer)?;
    let state = amenbo_core::plugin_config::answer(field, held.as_deref());
    let (value, secret_set) = if field.secret { (None, held.is_some()) } else { (held, false) };
    Ok(PluginConfigFieldDto {
        key: field.key.clone(),
        label: field.label.clone(),
        secret: field.secret,
        required: field.required,
        value,
        secret_set,
        state: state.as_str().to_string(),
    })
}

/// Read one installed plugin into its DTO, naming every row it has: the projects it crosses (`AMB-D-447`)
/// — the ones it fires in (`AMB-D-412`) and the ones that filled it in without turning it on — or, for a
/// plugin its author declared the machine's, the single row the device holds (`AMB-D-601`).
///
/// Which of the two is read comes from the declaration and not from a count: a machine-wide plugin crosses
/// no project, so asking the project list would answer "nowhere" for something that may well be firing.
///
/// `overlay` is what the catalog said about this plugin in the reader's language, kept beside the binary
/// at install time (`AMB-D-622`) — so the form follows a language change with no network at all. `None`
/// is a plugin nobody translated, or a reader on the base language.
fn install_row(
    store: &Store,
    plugin: &amenbo_core::plugin_subscribe::InstalledPlugin,
    overlay: Option<&amenbo_core::plugin_manifest::ManifestOverlay>,
) -> Result<PluginInstallDto, CmdError> {
    use amenbo_core::plugin_layer::Layer;
    let why = amenbo_core::plugin_compat::check(&plugin.manifest).err();
    let config =
        wanted_settings(&plugin.manifest.config, overlay.map(|o| &o.config), plugin.manifest.official);
    let actions = wanted_actions(
        plugin.manifest.settings.as_ref(),
        overlay.and_then(|o| o.settings.as_ref()),
    );
    let projects =
        amenbo_core::plugin_config::intersections(store, &plugin.name, &plugin.manifest.fields())?
            .into_iter()
            .map(|at| PluginProjectRowDto {
                project: at.project,
                enabled: at.enabled,
                has_value: at.has_value,
                required_unset: at.required_unset,
            })
            .collect();
    let device = match plugin.manifest.scope {
        amenbo_core::plugin_manifest::Scope::Project => None,
        amenbo_core::plugin_manifest::Scope::Machine => {
            let held = amenbo_core::plugin_config::held_at(
                store,
                &plugin.name,
                &plugin.manifest.fields(),
                Layer::Device,
            )?;
            Some(PluginDeviceRowDto {
                enabled: amenbo_core::plugin_trust::effective_enabled_in(
                    store,
                    &plugin.name,
                    Layer::Device,
                )?,
                has_value: held.has_value,
                required_unset: held.required_unset,
            })
        }
    };
    Ok(PluginInstallDto {
        name: plugin.name.clone(),
        title: plugin.manifest.title.clone(),
        projects,
        device,
        scope: plugin.manifest.scope,
        compatible: why.is_none(),
        incompatible_reason: why.map(|why| why.to_string()),
        config,
        actions,
    })
}

/// What this machine has installed, and the state of every "project × plugin" intersection each one has a
/// row at — the state the market draws over the catalog it is browsing (`AMB-D-351`).
///
/// **It is asked from nowhere in particular** (`AMB-D-412`). Every row names the projects holding its
/// gate open, so a face draws the whole answer without first choosing a project to look through — and a
/// plugin still running somewhere else cannot be hidden by where the screen happened to be standing.
/// Each of those names carries its crossing's whole state (`AMB-D-447`), so drawing the rows costs this
/// one read rather than one per project.
///
/// Reads the app-data `plugins/` directory and this store, and nothing else — no network, no catalog
/// fetch — so it answers the same offline, and a directory that will not read as an install is skipped
/// rather than allowed to hide the rest. **A language change is one of those reads** (`AMB-D-622`): the
/// translations an install kept are beside the binary, so re-asking in another language costs no request.
#[tauri::command]
pub fn plugin_installs(lang: String) -> Result<Vec<PluginInstallDto>, CmdError> {
    let store = open_store_read()?;
    let installed = amenbo_core::plugin_installed::installed(&store.paths)?;
    installed
        .iter()
        .map(|p| {
            let translations = amenbo_core::plugin_installed::translations(&store.paths, &p.name);
            install_row(&store, p, translations.get(&lang))
        })
        .collect()
}

/// What one project holds for a plugin's declared settings (`AMB-D-434`) — the form's own read, made
/// once a project is named.
///
/// It is separate from [`plugin_installs`] because a value is one project's while an install is not: the
/// list says what is installed and where it fires, and this says what a single project has filled in.
/// A secret comes back as held or not, never as itself (`AMB-D-356`).
#[tauri::command]
pub fn plugin_config_read(
    name: String,
    project_id: Option<i64>,
) -> Result<Vec<PluginConfigFieldDto>, CmdError> {
    let store = open_store_read()?;
    let installed = amenbo_core::plugin_installed::read(&store.paths, &name)?;
    // A `scope: project` plugin with no project named is refused rather than answered with blanks: "no
    // project named" and "this project holds nothing" are different states, and a form that cannot tell them
    // apart draws the second for the first. A `scope: machine` plugin has one answer wherever it is asked
    // from (`AMB-D-601`), so the same call needs no project for it.
    let layer = amenbo_core::plugin_layer::Layer::of(installed.manifest.scope, project_id)?;
    installed
        .manifest
        .fields()
        .iter()
        .map(|f| config_field_row(&store, &name, f, layer))
        .collect()
}

/// Install one plugin from the catalog by name (`AMB-D-351`) — the GUI's half of `plugin install`.
///
/// Every gate is core's ([`amenbo_core::plugin_install::install`]): the name resolves against the catalog,
/// the asset is verified fail-closed against Amenbo's own catalog key and the manifest's checksum, and
/// only then is anything written. This command adds no trust of its own, and cannot: the key is not a
/// parameter down there.
///
/// **Installing never enables.** The plugin lands inert and [`plugin_set_enabled`] is the separate,
/// explicit act that lets it run — which is why this returns the fresh row rather than an enabled one,
/// and why no project is named here: installing is not aimed at one (`AMB-D-412`).
///
/// `lang` is the reader's, for the row this hands back — the same language every other plugin read takes,
/// so a freshly installed plugin's form is captioned like the ones beside it rather than in English until
/// the next refetch.
/// Off the main thread: it downloads.
#[tauri::command]
pub async fn plugin_install(name: String, lang: String) -> Result<PluginInstallDto, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<PluginInstallDto, CmdError> {
        let paths = amenbo_core::config::Paths::resolve()?;
        amenbo_core::plugin_install::install(&paths, &name)?;
        let store = open_store_read()?;
        let installed = amenbo_core::plugin_installed::read(&store.paths, &name)?;
        let translations = amenbo_core::plugin_installed::translations(&store.paths, &name);
        install_row(&store, &installed, translations.get(&lang))
    })
    .await
    .map_err(|e| -> CmdError { format!("installing the plugin did not finish: {e}").into() })?
}

/// Move one installed plugin's gate — the GUI's `plugin enable` / `plugin disable`, through the one
/// boundary that moves that state ([`amenbo_core::plugin_trust`]).
///
/// There is one switch, and *which* layer it sits at is the plugin author's declaration rather than this
/// call's (`AMB-D-434` / `AMB-D-601`): `project_id` only says which project the caller is speaking for, and
/// a `scope: project` plugin asked without one is refused rather than answered device-wide. Enabling is
/// fail-closed twice over, both in core: on the compatibility declarations
/// (`AMB-D-359`) before anything is written, and on the author's `required` settings, probed at the
/// layer that gate is for ([`amenbo_core::plugin_config::satisfied_keys`], `AMB-D-356`).
///
/// **Calling this to enable is the permission** (`AMB-D-434`) — turning a plugin on is what running
/// somebody else's code means, so there is no second answer to record; for a `scope: machine` plugin that
/// one act is also the consent to let it read the whole device (`AMB-D-601`). Returns where the gate ended
/// up, and what closing it threw away.
#[tauri::command]
pub fn plugin_set_enabled(
    name: String,
    project_id: Option<i64>,
    enabled: bool,
) -> Result<PluginGateMovedDto, CmdError> {
    use amenbo_core::plugin_trust::{disable, effective_enabled_in, enable};
    with_store_mut(|store| {
        let installed = amenbo_core::plugin_installed::read(&store.paths, &name)?;
        let layer = amenbo_core::plugin_layer::Layer::of(installed.manifest.scope, project_id)?;
        let mut dropped_queued = 0;
        let mut check = None;
        if enabled {
            amenbo_core::plugin_compat::check(&installed.manifest)
                .map_err(|incompatible| CmdError::from(incompatible.into_error(&name)))?;
            let fields = installed.manifest.fields();
            let satisfied =
                amenbo_core::plugin_config::satisfied_keys(store, &name, &fields, layer)?;
            // What the author's conditions make of this layer's answers (`AMB-D-727`): the gate is judged
            // on the fields this form draws, so a `required` field hidden here does not shut it — the user
            // would have no box to fill in.
            let stage = amenbo_core::plugin_config::stage(store, &name, &fields, layer)?;
            // The author's own check, raised before the gate — pressing enable is the consent to run this
            // code (`AMB-D-664` / `AMB-D-351`).
            let checked = amenbo_core::plugin_check::run(
                store,
                &installed,
                project_id,
                amenbo_core::plugin_check::TIMEOUT,
            )?;
            let has_value = |f: &amenbo_core::plugin_manifest::ConfigField| {
                satisfied.iter().any(|k| k == &f.key)
            };
            // A check that refused is **the answer, not an error**: what it refused over is the sentences
            // it wrote, and those are this face's to draw beside the boxes they are about (`AMB-D-664`).
            // Core's refusal names the keys and drops the sentences, deliberately, because it also
            // travels to faces with nobody in front of them — so the gate coming back shut, with the
            // verdict, is what the form works from. Every other refusal is still thrown: an incompatible
            // build and an empty `required` field have no verdict to draw, only a sentence to show.
            let shut_by_the_check = !checked.opens_the_gate()
                && amenbo_core::plugin_trust::missing_required(&fields, &stage, has_value).is_empty();
            match enable(store, &name, layer, &fields, &stage, has_value, &checked) {
                Ok(()) => {}
                Err(_) if shut_by_the_check => {}
                Err(refused) => return Err(refused.into()),
            }
            check = checked_dto(&checked);
        } else {
            dropped_queued = disable(store, &name, layer)?.queued;
        }
        Ok(PluginGateMovedDto {
            enabled: effective_enabled_in(store, &name, layer)?,
            dropped_queued,
            check,
        })
    })
}

/// What the author's check said, as the settings form reads it (`AMB-D-664`) — `None` for a plugin that
/// declares none, which is every plugin written before the block existed.
///
/// The sentences travel whole and are drawn plain: Amenbo does not read them (`AMB-D-356`), and the form
/// puts each one where it belongs — the per-field lines beside their boxes, the `message` at the head.
/// What the check asked to have drawn travels the same way (`AMB-D-727`): core has already settled which
/// parts this author may have, so this is a rename and nothing more.
fn checked_dto(checked: &amenbo_core::plugin_check::Checked) -> Option<PluginCheckDto> {
    match checked {
        amenbo_core::plugin_check::Checked::NotDeclared => None,
        amenbo_core::plugin_check::Checked::Answered(verdict) => Some(PluginCheckDto {
            ok: verdict.ok,
            message: verdict.message.clone(),
            fields: verdict.fields.clone(),
            show: crate::dto::show_parts(&verdict.show),
            answered: true,
        }),
        // A silence has nothing of the plugin's in it (`AMB-D-354`), so nothing is invented here either:
        // the face says the check did not answer, and the execution log holds why (`AMB-D-361`).
        amenbo_core::plugin_check::Checked::Silent(_) => Some(PluginCheckDto {
            ok: false,
            message: None,
            fields: std::collections::BTreeMap::new(),
            show: Vec::new(),
            answered: false,
        }),
    }
}

/// Write one plugin setting — the GUI form's half of `plugin config set`, through the one write
/// boundary every face shares ([`amenbo_core::plugin_config::set`], `AMB-D-356`).
///
/// This side does what the CLI's does and no more: find the field the key names in the installed
/// manifest, and settle which project the value is for. The author's `secret` flag on that field is what
/// routes the value — a secret to `plugin_secret`, text to `plugin_config` — and **Amenbo never decides
/// secrecy here**; a key the manifest does not declare has no routing rule, so it is refused rather than
/// guessed at.
///
/// `project_id` names the project whose value this is. It is required for a `scope: project` plugin — the
/// layer is that project's row and there is no tier to write to without one — and unused for a
/// `scope: machine` one, whose values are the device's (`AMB-D-434` / `AMB-D-601`).
///
/// An **empty** `value` clears the setting, which is how the form's clear works — "not provided" is
/// unset, the same reading `required` uses. Nothing is echoed back: the caller has the value it typed,
/// and a secret has no business coming back out.
#[tauri::command]
pub fn plugin_config_set(
    name: String,
    key: String,
    value: String,
    project_id: Option<i64>,
) -> Result<(), CmdError> {
    with_store_mut(|store| {
        let installed = amenbo_core::plugin_installed::read(&store.paths, &name)?;
        let fields = installed.manifest.fields();
        let field = fields.iter().find(|f| f.key == key).cloned().ok_or_else(|| {
            let declared: Vec<&str> = fields.iter().map(|f| f.key.as_str()).collect();
            let known = if declared.is_empty() { "none".to_string() } else { declared.join(", ") };
            CmdError::from(amenbo_core::Error::invalid(
                format!("plugin '{name}' declares no setting '{key}' (it declares: {known})"),
            ))
        })?;
        let layer = amenbo_core::plugin_layer::Layer::of(installed.manifest.scope, project_id)?;
        amenbo_core::plugin_config::set(store, &field, &name, layer, &value)?;
        Ok(())
    })
}

/// Raise the author's check on the values as they now stand (`AMB-D-664`) — the **second** of the two
/// moments a check runs, and the one that undoes nothing.
///
/// **It is raised after a save, never in front of one.** The write has already landed through
/// [`plugin_config_set`] by the time this is called, so a verdict that refuses changes nothing: the value
/// stays written and an enabled plugin stays enabled (`AMB-D-664`). What the run is *for* here is the
/// sentence on the screen — someone has just typed a webhook and can still fix it — which is why the
/// answer is the same [`PluginCheckDto`] the switch comes back with, drawn in the same places.
///
/// **Only while the gate is open.** A check at an enable is the press's own consent to run the author's
/// code (`AMB-D-351`); a save is not that press, so at a crossing the plugin is off in nothing is raised
/// and the answer is `None` — as it is for a plugin that declares no check at all.
///
/// This is one call per save rather than one per field: [`plugin_config_set`] writes a single setting, and
/// a form with three changed boxes uses that door three times. Checking inside it would spawn the author's
/// program on each of them, and the first two would be judging a half-written state.
#[tauri::command]
pub fn plugin_settings_check(
    name: String,
    project_id: Option<i64>,
) -> Result<Option<PluginCheckDto>, CmdError> {
    // A read handle, as the press beside it takes: the run writes nothing of Amenbo's, and what the
    // plugin has to write it writes by calling Amenbo back (`AMB-D-406`).
    let store = &open_store_read()?;
    let installed = amenbo_core::plugin_installed::read(&store.paths, &name)?;
    let layer = amenbo_core::plugin_layer::Layer::of(installed.manifest.scope, project_id)?;
    if !amenbo_core::plugin_trust::effective_enabled_in(store, &name, layer)? {
        return Ok(None);
    }
    let checked = amenbo_core::plugin_check::run(
        store,
        &installed,
        project_id,
        amenbo_core::plugin_check::TIMEOUT,
    )?;
    Ok(checked_dto(&checked))
}

/// Raise one operation a plugin's settings block declared (`AMB-D-664`) — the settings face's one way of
/// running somebody else's code, and the only one it has.
///
/// **`cmd` names a declaration; it is never a line to run.** Core looks it up among the manifest's
/// `settings.actions` and takes the words from there, so a caller chooses *which* declared call runs and
/// never what it is handed (`AMB-D-522`); a `cmd` the manifest does not declare is refused there. The
/// gate is the ordinary one — what a form raises, it raises on an enabled plugin.
///
/// `supplied` carries the values this press asked for (`ask`), and they reach the child process and go no
/// further: not the config table, not the secret store, not this call's answer (`AMB-D-664`). The values
/// already saved are injected as they are for every run, and are not repeated here.
///
/// `project_id` is the project the form is standing in, as everywhere else on this face — required for a
/// `scope: project` plugin, and the device's own for a `scope: machine` one (`AMB-D-601`).
///
/// **What comes back is the run's verdict, its line, and the parts it asked to have drawn**
/// (`AMB-D-727`). The parts are read off stdout, which this face never consumed before — so a plugin
/// writing anything else there is not at fault and simply draws nothing. Whether the run succeeded is
/// still the exit code's to say (`AMB-D-353`); an `ok` written into that document is the check's word,
/// not an operation's, and is not read here.
#[tauri::command]
pub fn plugin_settings_action(
    name: String,
    cmd: String,
    supplied: std::collections::BTreeMap<String, String>,
    project_id: Option<i64>,
) -> Result<PluginActionRanDto, CmdError> {
    // A read handle, though this runs somebody's code: the press writes nothing of its own, and what the
    // plugin has to write it writes by calling Amenbo back (`AMB-D-406`) — which is a door of its own, and
    // not one to hold shut for however long a `setup` takes (`AMB-D-664` puts no bound on a press).
    {
        let store = &open_store_read()?;
        // The badge, off the manifest this machine holds: it is what decides whether a `qr` or a `link`
        // the run asks for may be drawn (`AMB-D-727` / `AMB-D-347`), and an author cannot set it.
        let official = amenbo_core::plugin_installed::read(&store.paths, &name)?.manifest.official;
        let outcome =
            amenbo_core::plugin_invoke::call_declared(store, &name, &cmd, &supplied, project_id)?;
        let (ok, diagnostic) = match &outcome {
            amenbo_core::plugin_command::CommandOutcome::Returned { diagnostic, .. } => {
                (true, diagnostic)
            }
            amenbo_core::plugin_command::CommandOutcome::Failed { diagnostic, .. } => {
                (false, diagnostic)
            }
        };
        let line = diagnostic.lines().find(|l| !l.trim().is_empty()).map(str::to_string);
        // A failed run's stdout is not consumed (`AMB-D-354`), so there is nothing of a failure's to draw
        // beyond its line — which is what `value()` already answers.
        let show = outcome
            .value()
            .map(|stdout| amenbo_core::plugin_show::of_stdout(stdout, official))
            .unwrap_or_default();
        Ok(PluginActionRanDto { ok, message: line, show: crate::dto::show_parts(&show) })
    }
}

/// Remove one plugin and everything it left behind (`AMB-D-357`) — the GUI's `plugin uninstall`.
///
/// **Uninstall is not disable.** It closes every gate on the way out and then takes the binary, every
/// project's settings, the secrets and the run log with it — so the face must have said as much before
/// calling this. What came back is the receipt, not a promise: a piece
/// that was not there is reported as one less thing removed rather than as a failure, which is also how a
/// half-broken install gets cleaned up.
#[tauri::command]
pub fn plugin_uninstall(name: String) -> Result<PluginRemovedDto, CmdError> {
    with_store_mut(|store| {
        let r = amenbo_core::plugin_uninstall::uninstall(store, &name)?;
        Ok(PluginRemovedDto {
            was_enabled: r.was_enabled,
            secrets: r.secrets,
            project_values: r.project_values,
            project_gates: r.project_gates,
            directory: r.directory,
            runs_log: r.runs_log,
            anything: r.anything(),
        })
    })
}

impl From<amenbo_core::plugin_update::Against> for PluginCatalogReadDto {
    fn from(against: amenbo_core::plugin_update::Against) -> Self {
        use amenbo_core::plugin_catalog::Freshness;
        use amenbo_core::plugin_update::Against;

        let secs = |age: std::time::Duration| Some(u32::try_from(age.as_secs()).unwrap_or(u32::MAX));
        let (read, age_seconds) = match against {
            Against::Catalog(Freshness::Fetched) => ("fetched", None),
            Against::Catalog(Freshness::Cached { age }) => ("cached", secs(age)),
            Against::Catalog(Freshness::Offline { age }) => ("offline", secs(age)),
            Against::NothingInstalled => ("notNeeded", None),
            Against::Unavailable => ("unavailable", None),
        };
        Self { read: read.to_string(), age_seconds }
    }
}

impl From<PluginUpdateReachDto> for amenbo_core::plugin_update::Reach {
    fn from(reach: PluginUpdateReachDto) -> Self {
        match reach {
            PluginUpdateReachDto::Incidental => Self::Incidental,
            PluginUpdateReachDto::Now => Self::Now,
        }
    }
}

/// Which installed plugins the catalog holds a different build of, and which of them need a decision first
/// (`AMB-D-359`) — the GUI's `plugin update --check`.
///
/// **The automatic triggers add no traffic of their own.** Under [`PluginUpdateReachDto::Incidental`] the
/// comparison reads the catalog through its freshness boundary, so a trigger arriving inside the window is
/// answered from the cache and one outside it costs a single fetch of the whole index — which is what lets
/// the face re-ask on a focus return and on opening the plugin screens without a resident timer. Nothing
/// installed costs no read at all.
///
/// **What a person pressed goes and looks** ([`PluginUpdateReachDto::Now`], `AMB-D-462`). The boundary is
/// there to keep the automatic triggers cheap, and one press is not part of that reckoning: answered from a
/// cache, "no updates" would mean "none an hour ago" while reading as the stronger thing, and the button
/// would look like it did nothing. A fetch that fails still falls back to the cache, so asking costs
/// freshness at worst and never function.
///
/// **The verdict travels with what it was measured against** ([`PluginCatalogReadDto`]). An empty list is
/// the ordinary answer and it means two different things — nothing has moved, or nothing recent enough to
/// tell was read — so the face is handed both halves off the one read rather than left to assume.
///
/// The `settings` judgment takes no project: a plugin has a gate per project (`AMB-D-434`)
/// and an update replaces the build for all of them, so every gate it is enabled at is judged. That is what
/// lets the banner be answered the same way from the screens that are in no project at all.
///
/// `lang` is the reader's, for the one line each offer carries. It costs no request either: the
/// translations came with the detail document the offer was read from (`AMB-D-622`). Off the main
/// thread, because past the boundary this fetches.
#[tauri::command]
pub async fn plugin_updates(
    reach: PluginUpdateReachDto,
    lang: String,
) -> Result<PluginUpdatesDto, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<PluginUpdatesDto, CmdError> {
        let paths = amenbo_core::config::Paths::resolve()?;
        let checked = amenbo_core::plugin_update::check(&paths, reach.into())?;
        let catalog = PluginCatalogReadDto::from(checked.against);
        if checked.updates.is_empty() {
            return Ok(PluginUpdatesDto { updates: Vec::new(), catalog });
        }
        let store = open_store_read()?;
        let updates = checked
            .updates
            .into_iter()
            .map(|u| {
                // The two gates that hold an update back, in the order the apply path applies them: a build
                // this Amenbo cannot speak to is not an improvement, and a schema that grew a `required`
                // field an enabled plugin has no value for is not one either.
                let (hold, missing) = if amenbo_core::plugin_compat::check(&u.available).is_err() {
                    (Some("incompatible".to_string()), Vec::new())
                } else {
                    let missing =
                        amenbo_core::plugin_config::required_unset_for_update(&store, &u.available)?;
                    ((!missing.is_empty()).then(|| "settings".to_string()), missing)
                };
                Ok(PluginUpdateDto {
                    desc_i18n: u.available_i18n.get(&lang).and_then(|o| o.desc.clone()),
                    name: u.name,
                    title: u.available.title,
                    available_detail_sum: u.available.detail_sum,
                    desc: u.available.desc,
                    hold,
                    missing,
                })
            })
            .collect::<Result<Vec<_>, CmdError>>()?;
        Ok(PluginUpdatesDto { updates, catalog })
    })
    .await
    .map_err(|e| -> CmdError { format!("checking for a plugin update did not finish: {e}").into() })?
}

/// Put the catalog's build of one plugin in place (`AMB-D-359`) — the GUI's `plugin update <name>`, the
/// button the update banner offers so no screen has to be visited to take an update.
///
/// Every gate is core's ([`amenbo_core::plugin_update::apply`]): the asset is re-verified against Amenbo's
/// catalog key and its checksum, the previous build is retained as a `.bak`, and the gate and every setting
/// the new build still declares are carried over untouched — the values of keys it has stopped declaring
/// are purged there, once the build is in place (`AMB-D-456`). The one gate this side adds is the config
/// re-check
/// ([`amenbo_core::plugin_config::required_unset_for_update`], the same one the CLI runs) — a new schema
/// that would leave a plugin missing a `required` value at any gate it is enabled at keeps the working build
/// and says so.
///
/// `false` means there was nothing to apply: the catalog publishes the build already installed. Off the
/// main thread — it downloads.
#[tauri::command]
pub async fn plugin_update_apply(name: String) -> Result<bool, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<bool, CmdError> {
        with_store_mut(|store| {
            let applied = amenbo_core::plugin_update::apply(store, &name, |store, available| {
                refuse_update_leaving_required_unset(store, available)
            })?;
            Ok(applied.is_some())
        })
    })
    .await
    .map_err(|e| -> CmdError { format!("updating the plugin did not finish: {e}").into() })?
}

/// Apply every update the catalog holds, one plugin at a time (`AMB-D-359`) — the banner's "update all".
///
/// Best-effort across plugins, exact within one: a plugin that fails is left exactly as it was and the next
/// is still attempted, so one asset that will not verify cannot hold back every other update. The refusals
/// come back as rows rather than as an error, because the caller has to report both halves of a mixed run.
#[tauri::command]
pub async fn plugin_update_apply_all() -> Result<Vec<PluginUpdateOutcomeDto>, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<PluginUpdateOutcomeDto>, CmdError> {
        use amenbo_core::plugin_update::Outcome;
        with_store_mut(|store| {
            let outcomes =
                amenbo_core::plugin_update::apply_all(store, |store, available| {
                    refuse_update_leaving_required_unset(store, available)
                })?;
            Ok(outcomes
                .into_iter()
                .map(|o| match o {
                    Outcome::Replaced(r) => {
                        PluginUpdateOutcomeDto { name: r.name, applied: true, error: None }
                    }
                    Outcome::Failed { name, error } => PluginUpdateOutcomeDto {
                        name,
                        applied: false,
                        error: Some(error.to_string()),
                    },
                })
                .collect())
        })
    })
    .await
    .map_err(|e| -> CmdError { format!("updating the plugin did not finish: {e}").into() })?
}

/// The config re-check the two apply paths above hand to core as their `approve` gate (`AMB-D-359`).
/// [`amenbo_core::plugin_config::required_unset_for_update`] decides *whether* a build is held back — the
/// same call the CLI makes — and this only words the refusal for a window, where the way out is the
/// plugin's settings and not a shell command.
fn refuse_update_leaving_required_unset(
    store: &Store,
    available: &amenbo_core::plugin_manifest::Manifest,
) -> amenbo_core::error::Result<()> {
    let missing = amenbo_core::plugin_config::required_unset_for_update(store, available)?;
    if missing.is_empty() {
        return Ok(());
    }
    let name = available.name.as_str();
    Err(amenbo_core::error::Error::invalid(
        format!(
            "the new build of '{name}' needs setting(s) not provided: {}. Set them first, then update — the build in place is unchanged",
            missing.join(", ")
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use amenbo_core::model::View;
    use std::sync::Mutex;

    /// These tests all swap out AMENBO_HOME, which is shared across the process, so they are
    /// serialized to keep parallel runs from treading on each other.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Finish creating a task — the second stage every creation has (`AMB-D-554`), without which the
    /// task cannot be reserved. It goes through the command the pane's button calls, so every test that
    /// needs a task nobody is still writing crosses that path too.
    fn finish_creating(id: i64) {
        task_finish_creating(id).unwrap();
    }

    /// When the target is gone and no ledger row carrying its name can be recovered either (dropped
    /// in compaction, or beyond the lookback budget), core returns an empty title. It is passed on
    /// empty: the stand-in a reader sees is a sentence in their language, so it is the GUI that puts
    /// one there, and a DTO that filled the blank here would hide the emptiness from it.
    #[test]
    fn a_subject_whose_name_is_gone_is_passed_on_empty() {
        let nameless = |title: &str| amenbo_core::activity::Item {
            id: 1,
            at: Timestamp::now(),
            kind: amenbo_core::activity::Kind::System,
            author_kind: Some(ActorKind::Ai),
            target_type: amenbo_core::activity::TargetType::Task,
            target_id: 42,
            title: title.to_string(),
            target_live: false,
            event: Some(serde_json::json!({"kind": "task.status_changed", "new": "done"})),
            text: None,
            edited_at: None,
        };
        let config = amenbo_core::config::Config::default();

        let gone = activity_dto(nameless(""), &config);
        assert_eq!(gone.target.title, "");
        let event = gone.event.unwrap();
        assert_eq!(event.kind, "task.status_changed");
        assert_eq!(event.status.as_deref(), Some("done"));

        let alive = activity_dto(nameless("生きているタスク"), &config);
        assert_eq!(alive.target.title, "生きているタスク");
    }

    /// Each event kind carries the values its own sentence asks for, and nothing else. A field left
    /// behind here cannot be recovered on the other side, and one sent for a kind that has no use
    /// for it invites a sentence built on a value that means nothing there.
    #[test]
    fn an_event_carries_the_values_its_own_sentence_needs() {
        let project = |tasks: u64, decisions: u64| {
            serde_json::json!({"kind": "project.deleted", "name": "旧サイト", "tasks": tasks, "decisions": decisions})
        };

        let deleted = event_dto(&project(4, 1));
        assert_eq!(deleted.kind, "project.deleted");
        assert_eq!((deleted.tasks, deleted.decisions), (Some(4), Some(1)));
        // Zero is a count, not an absence: the sentence for "nothing went with it" is chosen from
        // the numbers, so they are sent even when both are nought.
        let empty = event_dto(&project(0, 0));
        assert_eq!((empty.tasks, empty.decisions), (Some(0), Some(0)));

        // An assignment that was taken away sends no facet — that absence is what says so.
        let unassigned = event_dto(&serde_json::json!({"kind": "task.assigned"}));
        assert_eq!(unassigned.to_kind, None);
        let delegated = event_dto(&serde_json::json!({"kind": "task.assigned", "to_kind": "ai"}));
        assert_eq!(delegated.to_kind.as_deref(), Some("ai"));

        // A kind with no values of its own carries none, and one this build never heard of keeps
        // its name rather than being flattened into a known kind.
        let moved = event_dto(&serde_json::json!({"kind": "task.moved"}));
        assert_eq!(moved.kind, "task.moved");
        assert_eq!((moved.status, moved.to_kind, moved.tasks, moved.decisions), (None, None, None, None));
        assert_eq!(event_dto(&serde_json::json!({"kind": "task.hatched"})).kind, "task.hatched");
    }

    /// The tests' env guard. It takes ENV_LOCK to serialize, and disables the update check so the
    /// `build_snapshot` path talks to no upstream and touches no real OS cache — hermetic. Every
    /// test that goes through a snapshot goes through this.
    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        let g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("AMENBO_UPDATE_CHECK", "0");
        g
    }

    /// Plant an installed plugin under the test's app-data: the manifest (which is the install marker)
    /// and the executable named after it — the whole on-disk shape `plugin_installed::read` looks for.
    /// `config` is the settings schema its author declares, which is what a form is generated from.
    fn plant_plugin_with(home: &std::path::Path, name: &str, config: serde_json::Value) {
        let dir = home.join("plugins").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = serde_json::json!({
            "name": name,
            "desc": "テスト用",
            "author": "amenbo",
            "repo": "ShiroDoromoto/amenbo-plugin-test",
            "os": ["macos", "linux", "windows"],
            "category": "workflow",
            "url": "https://example.com/x.tar.gz",
            "checksum": "sha256:deadbeef",
            "config": config,
        });
        std::fs::write(dir.join("manifest.json"), serde_json::to_vec(&manifest).unwrap()).unwrap();
        let program = amenbo_core::plugin_installed::program_file_name(name);
        std::fs::write(dir.join(program), b"x").unwrap();
    }

    /// The same plant for a plugin whose author declared no settings at all.
    fn plant_plugin(home: &std::path::Path, name: &str) {
        plant_plugin_with(home, name, serde_json::json!([]));
    }

    /// The same plant for a plugin whose author declared it the machine's (`AMB-D-601`) — the layer no
    /// published plugin declares, and the one this face draws a row of its own for.
    fn plant_machine_plugin(home: &std::path::Path, name: &str, config: serde_json::Value) {
        plant_plugin_with(home, name, config);
        let path = home.join("plugins").join(name).join("manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        manifest["scope"] = serde_json::json!("machine");
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    }

    /// The same plant for a plugin whose author declared a check (`AMB-D-664`). The program beside the
    /// manifest is the plant's stand-in and will not launch, so the check is always a silence — the
    /// fail-closed answer, and the one a test can have without shipping an executable.
    fn plant_checking_plugin(home: &std::path::Path, name: &str, config: serde_json::Value) {
        plant_plugin_with(home, name, config);
        let path = home.join("plugins").join(name).join("manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        manifest["settings"] = serde_json::json!({ "check": "config check" });
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    }

    /// The GUI's creation screen asks for a name and nothing else, so the view a new project opens on
    /// has one source: the configured `default_view`. It is the same answer the CLI gives when `--view`
    /// is omitted, and the reason the setting is a setting rather than a value nothing acts on.
    #[test]
    fn a_project_the_gui_creates_opens_on_the_configured_view() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("project-default-view");
        std::env::set_var("AMENBO_HOME", &tmp);
        {
            let mut store = Store::open().unwrap();
            store.config.set("default_view", "timeline").unwrap();
            store.save_config().unwrap();
        }

        let (_store, project_id) = provision_project("SCENARIO PJ").unwrap();

        let store = Store::open().unwrap();
        let detail = store.project_detail(project_id).unwrap();
        assert_eq!(
            detail.default_view,
            amenbo_core::model::View::Timeline,
            "the creation screen names no view, so the setting is what answered"
        );
    }

    /// The GUI's gate commands are the CLI's `plugin enable/disable` through the same boundary: one
    /// switch, and it is the named project's (`AMB-D-434`). What the list answers with is every project
    /// holding that switch open (`AMB-D-412`) — asked from nowhere in particular, so a plugin firing in
    /// one project cannot be read as "off" from another.
    #[test]
    fn the_gate_commands_move_one_projects_switch() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("plugin-gate");
        std::env::set_var("AMENBO_HOME", &tmp);
        let project = |name: &str| {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: name.into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let project_id = project("テストPJ");
        let other_id = project("となりのPJ");
        plant_plugin(&tmp, "notify");

        // Installed is not enabled: the row is here, and it names no project at all.
        let rows = plugin_installs("en".into()).unwrap();
        assert_eq!(rows.len(), 1, "the plant reads as installed");
        assert!(rows[0].projects.is_empty(), "off everywhere is an empty list, not a false");

        let row = || plugin_installs("en".into()).unwrap().into_iter().find(|r| r.name == "notify").unwrap();
        let firing = || -> Vec<i64> {
            row().projects.iter().filter(|p| p.enabled).map(|p| p.project).collect()
        };
        assert!(plugin_set_enabled("notify".into(), Some(project_id), true).unwrap().enabled);
        assert_eq!(firing(), vec![project_id]);

        // A second project's switch is its own, and the list carries both rather than the one a caller
        // happened to ask through.
        assert!(plugin_set_enabled("notify".into(), Some(other_id), true).unwrap().enabled);
        let mut on = firing();
        on.sort_unstable();
        assert_eq!(on, vec![project_id, other_id]);

        // Disabling closes that project's gate and no other; the plugin stays installed
        // (`disable ≠ uninstall`).
        let off_gate = plugin_set_enabled("notify".into(), Some(project_id), false).unwrap();
        assert!(!off_gate.enabled);
        assert_eq!(off_gate.dropped_queued, 0, "nothing was queued, so nothing was thrown away");
        assert_eq!(firing(), vec![other_id], "the other project is still firing");

        // Without a project there is no switch to move.
        assert!(
            plugin_set_enabled("notify".into(), None, true).is_err(),
            "there is no device-wide answer for a gate to fall back on"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// What the row of a crossing carries besides its switch (`AMB-D-447`): a project that filled the
    /// plugin in without turning it on is on the list at all, and a project whose `required` setting is
    /// empty carries the mark saying an enable there would be refused — both answered by the read that
    /// lists the installs, so a face draws its rows without asking once per project.
    #[test]
    fn each_crossing_carries_its_values_and_the_refusal_ahead_of_the_switch() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("plugin-crossing");
        std::env::set_var("AMENBO_HOME", &tmp);
        let project = |name: &str| {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: name.into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let filled = project("埋めたPJ");
        let short = project("必須が空のPJ");
        plant_plugin_with(
            &tmp,
            "notify",
            serde_json::json!([
                {"key": "token", "label": "APIトークン", "secret": true, "required": true},
                {"key": "events", "label": "通知するイベント", "secret": false, "required": false},
            ]),
        );
        let rows = || plugin_installs("en".into()).unwrap().into_iter().find(|r| r.name == "notify").unwrap().projects;

        // Nobody has touched it anywhere: there is no crossing to draw, which is itself the answer.
        assert!(rows().is_empty(), "installed alone puts no project on the list");

        plugin_config_set("notify".into(), "token".into(), "t".into(), Some(filled)).unwrap();
        plugin_config_set("notify".into(), "events".into(), "deploy".into(), Some(short)).unwrap();

        let drawn = rows();
        assert_eq!(drawn.len(), 2, "a value puts a project on the list with the gate still shut");
        assert_eq!((drawn[0].project, drawn[0].enabled, drawn[0].has_value), (filled, false, true));
        assert!(!drawn[0].required_unset, "everything the author requires is held here");
        assert_eq!((drawn[1].project, drawn[1].enabled, drawn[1].has_value), (short, false, true));
        assert!(drawn[1].required_unset, "the required setting is empty at this crossing");

        // The mark is not decoration: it is core's own refusal, said before the switch is pressed.
        assert!(plugin_set_enabled("notify".into(), Some(short), true).is_err());
        assert!(plugin_set_enabled("notify".into(), Some(filled), true).unwrap().enabled);
        assert!(rows()[0].enabled, "the gate opened where nothing was in the way");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// **The platform's half of a condition is settled here, and the rest travels** (`AMB-D-727`).
    ///
    /// A face never learns an OS name: what this build's platform hides is gone before the DTO is built,
    /// which is the half core is the only one that can answer. What reads another setting is handed on,
    /// because while a form is open the answers are the form's — the store has not been told about a box
    /// still being filled in.
    #[test]
    fn what_this_platform_hides_never_reaches_the_form_and_the_rest_is_handed_on() {
        use amenbo_core::plugin_manifest::{
            ConfigEntry, ConfigField, ConfigOption, FieldType, Os, Settings, SettingsAction,
        };
        use amenbo_core::plugin_when::When;

        let here = Os::here().expect("this build runs on a platform Amenbo names");
        let elsewhere = if here == Os::Windows { Os::Macos } else { Os::Windows };

        let config = vec![
            ConfigField {
                field_type: FieldType::Multi,
                options: vec![
                    ConfigOption { when: vec![When::on([elsewhere])], ..ConfigOption::new("icloud", "iCloud") },
                    ConfigOption::new("cloudflare", "Cloudflare"),
                ],
                ..ConfigField::new("transport", "経路")
            },
            ConfigField {
                when: vec![When::on([elsewhere])],
                ..ConfigField::new("apple_id", "Apple ID")
            },
            ConfigField {
                when: vec![When::on([here]), When::field_has("transport", "cloudflare")],
                ..ConfigField::new("worker_url", "Worker の URL")
            },
        ];

        let entries = wanted_settings(&ConfigEntry::schema(config), None, true);
        let drawn: Vec<&PluginWantedSettingDto> = entries
            .iter()
            .filter_map(|entry| match entry {
                PluginFormEntryDto::Field { field } => Some(field),
                PluginFormEntryDto::Part { .. } => None,
            })
            .collect();
        assert_eq!(
            drawn.iter().map(|f| f.key.as_str()).collect::<Vec<_>>(),
            ["transport", "worker_url"],
            "the field this platform hides is not on the form at all",
        );
        assert_eq!(
            drawn[0].options.iter().map(|o| o.value.as_str()).collect::<Vec<_>>(),
            ["cloudflare"],
            "nor is the candidate it hides",
        );
        assert!(drawn[0].when.is_empty(), "an unconditional field carries no condition on");
        assert_eq!(
            drawn[1].when.iter().map(|c| (c.field.as_str(), c.has.as_str())).collect::<Vec<_>>(),
            [("transport", "cloudflare")],
            "the platform clause held and is spent; what reads a setting is handed on",
        );

        let settings = Settings {
            check: None,
            actions: vec![
                SettingsAction { when: vec![When::on([elsewhere])], ..SettingsAction::new("apple", "Apple") },
                SettingsAction {
                    when: vec![When::field_has("transport", "cloudflare")],
                    ..SettingsAction::new("tunnel", "Raise the tunnel")
                },
            ],
        };
        let actions = wanted_actions(Some(&settings), None);
        assert_eq!(
            actions.iter().map(|a| a.cmd.as_str()).collect::<Vec<_>>(),
            ["tunnel"],
            "the button this platform hides is gone with its fields",
        );
        assert_eq!(actions[0].when.len(), 1, "and the one that is left carries what is still to judge");
    }

    /// The translations an install keeps beside its manifest (`AMB-D-622`), as the catalog published them.
    fn plant_translations(home: &std::path::Path, name: &str, translations: serde_json::Value) {
        let dir = home.join("plugins").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("i18n.json"), serde_json::to_vec(&translations).unwrap()).unwrap();
    }

    /// The translated half is paired to the base by **key**, never by position (`AMB-D-621`), and it
    /// arrives beside the author's words rather than over them (`AMB-D-623`) — so a field the author did
    /// not translate is not a hole in the form, it is the line they wrote.
    #[test]
    fn a_translated_form_pairs_by_key_and_leaves_the_untranslated_fields_alone() {
        use amenbo_core::plugin_manifest::{
            ConfigField, ConfigFieldOverlay, ConfigOption, FieldType,
        };
        let field = |key: &str, label: &str, options: Vec<ConfigOption>| ConfigField {
            field_type: if options.is_empty() { FieldType::Text } else { FieldType::Multi },
            options,
            ..ConfigField::new(key, label)
        };
        let config = vec![
            field("endpoint", "Endpoint", Vec::new()),
            field(
                "events",
                "Events",
                vec![
                    ConfigOption::new("task.done", "Task finished"),
                    ConfigOption::new("task.created", "Task filed"),
                ],
            ),
        ];
        // Only the second field, and only one of its two candidates — the ordinary shape of a
        // translation in progress, and the one that would break if the two lists were zipped.
        let overlay = std::collections::BTreeMap::from([(
            "events".to_string(),
            ConfigFieldOverlay {
                label: Some("通知するできごと".into()),
                options: std::collections::BTreeMap::from([(
                    "task.done".to_string(),
                    "タスクが終わったとき".to_string(),
                )]),
                ..ConfigFieldOverlay::default()
            },
        )]);

        let form = wanted_settings(
            &amenbo_core::plugin_manifest::ConfigEntry::schema(config.clone()),
            Some(&overlay),
            true,
        );
        let drawn = drawn_fields(&form);
        assert_eq!(drawn[0].label, "Endpoint");
        assert_eq!(drawn[0].label_i18n, None, "an untranslated field carries no second line");
        assert_eq!(drawn[1].label, "Events", "the author's own line is never overwritten");
        assert_eq!(drawn[1].label_i18n.as_deref(), Some("通知するできごと"));
        assert_eq!(drawn[1].options[0].value, "task.done", "the wire value is not translated");
        assert_eq!(drawn[1].options[0].label_i18n.as_deref(), Some("タスクが終わったとき"));
        assert_eq!(drawn[1].options[1].label_i18n, None);

        // No layer at all is the same answer as a layer that says nothing: the form as its author wrote it.
        let bare = wanted_settings(
            &amenbo_core::plugin_manifest::ConfigEntry::schema(config),
            None,
            true,
        );
        assert!(drawn_fields(&bare).iter().all(|f| f.label_i18n.is_none()));
    }

    /// The settings on a drawn form (`AMB-D-727`) — what a test about a *field* wants out of a list that
    /// also carries the parts Amenbo draws between them.
    fn drawn_fields(form: &[PluginFormEntryDto]) -> Vec<&PluginWantedSettingDto> {
        form.iter()
            .filter_map(|entry| match entry {
                PluginFormEntryDto::Field { field } => Some(field),
                PluginFormEntryDto::Part { .. } => None,
            })
            .collect()
    }

    /// A form is drawn in the order its author wrote (`AMB-D-727`), parts and settings together — where a
    /// part sits is what it is for, so a face that sorted or split them would lose the whole point of
    /// writing one there.
    #[test]
    fn a_drawn_form_keeps_the_order_its_author_wrote() {
        use amenbo_core::plugin_manifest::{ConfigEntry, ConfigField};
        use amenbo_core::plugin_show::Part;

        let declared = vec![
            ConfigEntry::from(Part::Link {
                url: "https://myaccount.google.com/apppasswords".into(),
                label: "Create an app password".into(),
            }),
            ConfigField { secret: true, ..ConfigField::new("smtp_password", "Password") }.into(),
            ConfigEntry::from(Part::Note("One per mailbox.".into())),
        ];

        let drawn = wanted_settings(&declared, None, true);
        assert!(matches!(drawn[0], PluginFormEntryDto::Part { part: PluginShowPartDto::Link { .. }, .. }));
        assert!(matches!(&drawn[1], PluginFormEntryDto::Field { field } if field.key == "smtp_password"));
        assert!(matches!(drawn[2], PluginFormEntryDto::Part { part: PluginShowPartDto::Note { .. }, .. }));
    }

    /// A destination is an official plugin's (`AMB-D-727`), and this is the second place that is answered:
    /// the validator tells an author, and a manifest that reached a machine anyway is drawn without it.
    /// What is around it still draws — the reader loses the button, not the form.
    #[test]
    fn a_third_partys_destination_never_reaches_the_form() {
        use amenbo_core::plugin_manifest::{ConfigEntry, ConfigField};
        use amenbo_core::plugin_show::Part;

        let declared = vec![
            ConfigEntry::from(Part::Qr("https://apps.apple.com/x".into())),
            ConfigEntry::from(Part::Link {
                url: "https://example.test/x".into(),
                label: "Go".into(),
            }),
            ConfigEntry::from(Part::Copy("https://example.test/x".into())),
            ConfigField::new("token", "Token").into(),
        ];

        let drawn = wanted_settings(&declared, None, false);
        assert_eq!(drawn.len(), 2, "the two that carry a destination are gone");
        assert!(matches!(drawn[0], PluginFormEntryDto::Part { part: PluginShowPartDto::Copy { .. }, .. }));
        assert!(matches!(&drawn[1], PluginFormEntryDto::Field { field } if field.key == "token"));

        assert_eq!(
            wanted_settings(&declared, None, true).len(),
            4,
            "an official plugin draws all four"
        );
    }

    /// A part is read the way its neighbouring setting is (`AMB-D-727`) — the platform's half settled
    /// here, and what reads another setting's answer handed on for the form to re-read. A caption that
    /// outlived the box it is about would leave a step nobody could follow.
    #[test]
    fn a_part_is_conditioned_the_way_the_settings_around_it_are() {
        use amenbo_core::plugin_manifest::{ConfigEntry, ConfigField, ConfigPart, Os};
        use amenbo_core::plugin_show::Part;
        use amenbo_core::plugin_when::When;

        let here = Os::here().expect("this build runs on a platform Amenbo names");
        let elsewhere = if here == Os::Windows { Os::Macos } else { Os::Windows };

        let declared = vec![
            ConfigField::new("transport", "経路").into(),
            ConfigEntry::Part(ConfigPart {
                part: Part::Note("Worker を先に立ててください".into()),
                when: vec![When::on([here]), When::field_has("transport", "cloudflare")],
            }),
            ConfigEntry::Part(ConfigPart {
                part: Part::Text("この経路は Mac だけです".into()),
                when: vec![When::on([elsewhere])],
            }),
            ConfigEntry::from(Part::Text("どの経路でも読めます".into())),
        ];

        let drawn = wanted_settings(&declared, None, true);
        let parts: Vec<&Vec<PluginWhenDto>> = drawn
            .iter()
            .filter_map(|entry| match entry {
                PluginFormEntryDto::Part { when, .. } => Some(when),
                PluginFormEntryDto::Field { .. } => None,
            })
            .collect();
        assert_eq!(parts.len(), 2, "the part this platform hides is not on the form at all");
        assert_eq!(
            parts[0].iter().map(|c| (c.field.as_str(), c.has.as_str())).collect::<Vec<_>>(),
            [("transport", "cloudflare")],
            "the platform clause held and is spent; what reads a setting is handed on",
        );
        assert!(parts[1].is_empty(), "a part written without a condition carries none on");
    }

    /// A part a stranger may not draw and a part this platform hides are two separate answers, and a
    /// third party's `qr` is gone either way — the badge is read where it always was.
    #[test]
    fn a_conditioned_destination_is_still_an_official_plugins_alone() {
        use amenbo_core::plugin_manifest::{ConfigEntry, ConfigPart, Os};
        use amenbo_core::plugin_show::Part;
        use amenbo_core::plugin_when::When;

        let here = Os::here().expect("this build runs on a platform Amenbo names");
        let elsewhere = if here == Os::Windows { Os::Macos } else { Os::Windows };
        let qr = |os| {
            vec![ConfigEntry::Part(ConfigPart {
                part: Part::Qr("https://apps.apple.com/x".into()),
                when: vec![When::on([os])],
            })]
        };
        assert!(wanted_settings(&qr(elsewhere), None, true).is_empty(), "hidden by the platform");
        assert_eq!(wanted_settings(&qr(here), None, true).len(), 1);
        assert!(wanted_settings(&qr(here), None, false).is_empty(), "still official-only");
    }

    /// The buttons are paired the same way (`AMB-D-664`): an operation by the call it raises, and a value
    /// it asks for by the name that value travels under. Neither of those is translated — they are what
    /// the press hands back — and the words beside them are.
    #[test]
    fn the_buttons_a_form_draws_pair_by_the_call_and_the_name_they_travel_under() {
        use amenbo_core::plugin_manifest::{
            AskField, Settings, SettingsAction, SettingsActionOverlay, SettingsOverlay,
        };
        let settings = Settings {
            check: Some("config check".into()),
            actions: vec![
                SettingsAction {
                    ask: vec![AskField {
                        key: "api_token".into(),
                        label: "API token".into(),
                        secret: true,
                        extra: Default::default(),
                    }],
                    ..SettingsAction::new("config test", "Send a test message")
                },
                SettingsAction::new("setup", "Set up"),
            ],
        };
        // Only the first operation, written in the other order than the manifest declares them — which is
        // exactly what a pairing by position would get wrong.
        let overlay = SettingsOverlay {
            actions: std::collections::BTreeMap::from([(
                "config test".to_string(),
                SettingsActionOverlay {
                    label: Some("テスト送信".into()),
                    ask: std::collections::BTreeMap::from([(
                        "api_token".to_string(),
                        "API トークン".to_string(),
                    )]),
                    ..SettingsActionOverlay::default()
                },
            )]),
            ..SettingsOverlay::default()
        };

        let drawn = wanted_actions(Some(&settings), Some(&overlay));
        assert_eq!(drawn[0].cmd, "config test", "the call is the handle, and is not translated");
        assert_eq!(drawn[0].label, "Send a test message", "the author's own words stand");
        assert_eq!(drawn[0].label_i18n.as_deref(), Some("テスト送信"));
        assert_eq!(drawn[0].ask[0].key, "api_token", "nor is the name the value travels under");
        assert_eq!(drawn[0].ask[0].label_i18n.as_deref(), Some("API トークン"));
        assert!(drawn[0].ask[0].secret, "the author said so, and the box hides what is typed");
        assert_eq!(drawn[1].label_i18n, None, "an untranslated button carries no second line");

        assert!(wanted_actions(Some(&settings), None).iter().all(|a| a.label_i18n.is_none()));
        assert!(wanted_actions(None, None).is_empty(), "a plugin declaring no block has no buttons");
    }

    /// What a verdict reaches the screen as (`AMB-D-664`). The three states are three answers: a plugin
    /// with no check has nothing to draw, one that answered carries the author's sentences whole, and one
    /// that said nothing readable carries none of them — the silence is Amenbo's own reading of the run.
    #[test]
    fn a_verdict_reaches_the_form_whole_and_a_silence_reaches_it_as_a_silence() {
        use amenbo_core::plugin_check::{Checked, Silence, Verdict};

        assert!(checked_dto(&Checked::NotDeclared).is_none());

        let answered = checked_dto(&Checked::Answered(Verdict {
            ok: false,
            message: Some("the mailbox would not answer".into()),
            fields: std::collections::BTreeMap::from([(
                "smtp_host".to_string(),
                "there is a space in it".to_string(),
            )]),
            show: vec![amenbo_core::plugin_show::Part::Link {
                url: "https://myaccount.google.com/apppasswords".into(),
                label: "Create an app password".into(),
            }],
        }))
        .expect("a check that answered is something to draw");
        assert!(!answered.ok && answered.answered);
        assert_eq!(answered.message.as_deref(), Some("the mailbox would not answer"));
        assert_eq!(answered.fields["smtp_host"], "there is a space in it");
        // What the check asked to have drawn rides with its sentences (`AMB-D-727`) — the road to the
        // page that issues the value is worth the most beside the box that is refusing it.
        assert!(
            matches!(
                answered.show.as_slice(),
                [PluginShowPartDto::Link { url, label }]
                    if url == "https://myaccount.google.com/apppasswords"
                        && label == "Create an app password"
            ),
            "the check's parts reach the form"
        );

        let silent = checked_dto(&Checked::Silent(Silence::TimedOut)).expect("a silence is drawn too");
        assert!(!silent.ok, "a silence never opens a gate, and the form says the gate is shut");
        assert!(!silent.answered);
        assert_eq!(silent.message, None, "there is no sentence of the author's in it");
        assert!(silent.fields.is_empty());
        assert!(silent.show.is_empty(), "nor anything of theirs to draw");
    }

    /// The other moment a check runs (`AMB-D-664`) — after a save, where it costs nothing. It is raised
    /// only at a crossing whose gate is already open, because a save is not the press that consents to run
    /// somebody else's code (`AMB-D-351`), and what it says reaches nothing behind it: the value stays
    /// written and the plugin stays on, even for the silence that would have kept the gate shut.
    #[test]
    fn a_save_raises_the_check_where_the_gate_is_open_and_takes_nothing_back() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("plugin-check-after-save");
        std::env::set_var("AMENBO_HOME", &tmp);
        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        plant_checking_plugin(
            &tmp,
            "mail",
            serde_json::json!([{"key": "smtp_host", "label": "Host", "secret": false, "required": false}]),
        );
        let saved = || {
            plugin_config_set(
                "mail".into(),
                "smtp_host".into(),
                "smtp.example.test".into(),
                Some(project_id),
            )
            .unwrap()
        };
        let held = || {
            plugin_config_read("mail".into(), Some(project_id)).unwrap()[0].value.clone()
        };

        // Off: a save is a save, and nobody's code runs behind it.
        saved();
        assert!(
            plugin_settings_check("mail".into(), Some(project_id)).unwrap().is_none(),
            "an off crossing raises nothing — the press that consents has not happened"
        );

        // The gate is opened through the trust door rather than the switch: the switch would raise this
        // same check, and the plant's program will not start, which is the silence that keeps a gate shut.
        // What is being tested is the moment *after* that, so the plugin stands in for one whose check
        // said yes when it was enabled.
        {
            let mut store = Store::open().unwrap();
            amenbo_core::plugin_trust::enable(
                &mut store,
                "mail",
                amenbo_core::plugin_layer::Layer::Project(project_id),
                &[],
                &amenbo_core::plugin_when::Stage::default(),
                |_| true,
                &amenbo_core::plugin_check::Checked::NotDeclared,
            )
            .unwrap();
        }

        saved();
        let said = plugin_settings_check("mail".into(), Some(project_id))
            .unwrap()
            .expect("an open gate raises the check the manifest declared");
        assert!(!said.ok && !said.answered, "the plant's program will not start, so it said nothing");
        assert_eq!(held().as_deref(), Some("smtp.example.test"), "the save is never taken back");
        let row = plugin_installs("en".into()).unwrap().into_iter().find(|r| r.name == "mail").unwrap();
        assert!(
            row.projects.iter().any(|p| p.project == project_id && p.enabled),
            "and an enabled plugin is not switched off behind the user's back"
        );
    }

    /// The installed face draws its form in the reader's language off what the install kept beside the
    /// binary (`AMB-D-622`) — no catalog, no network — and a language nobody published leaves the author's
    /// words standing (`AMB-D-623`).
    #[test]
    fn an_installed_plugins_form_is_captioned_in_the_language_it_is_asked_in() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("plugin-installs-language");
        std::env::set_var("AMENBO_HOME", &tmp);
        plant_plugin_with(
            &tmp,
            "notify",
            serde_json::json!([{"key": "endpoint", "label": "Endpoint", "secret": false, "required": true}]),
        );
        plant_translations(
            &tmp,
            "notify",
            serde_json::json!({ "ja": { "config": { "endpoint": { "label": "送り先" } } } }),
        );
        let label = |lang: &str| {
            let row = plugin_installs(lang.into())
                .unwrap()
                .into_iter()
                .find(|r| r.name == "notify")
                .unwrap();
            let first = drawn_fields(&row.config)[0];
            (first.label.clone(), first.label_i18n.clone())
        };

        assert_eq!(label("ja"), ("Endpoint".into(), Some("送り先".into())));
        // A language the author wrote nothing in is the base language's own answer, and neither says so.
        assert_eq!(label("de"), ("Endpoint".to_string(), None));
        assert_eq!(label("en"), ("Endpoint".to_string(), None));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A plugin its author declared the machine's has one gate and crosses no project (`AMB-D-601`), so
    /// what a face draws it from is the device's own row. It has to come back **with the install**: the
    /// project list is rightly empty for such a plugin, and a screen reading only that would draw "off
    /// everywhere" over something firing on the whole machine — which is what it did before this row
    /// existed.
    #[test]
    fn a_machine_wide_plugin_answers_with_the_devices_row_and_no_crossings() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("plugin-device-row");
        std::env::set_var("AMENBO_HOME", &tmp);
        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        plant_machine_plugin(
            &tmp,
            "carry",
            serde_json::json!([
                {"key": "endpoint", "label": "送り先", "secret": false, "required": true},
            ]),
        );
        let row = || plugin_installs("en".into()).unwrap().into_iter().find(|r| r.name == "carry").unwrap();

        // Off, holding nothing, and short of what the author requires — the mark said before the switch
        // is pressed, exactly as a crossing wears it.
        let before = row();
        assert!(before.projects.is_empty(), "no project crosses a device-wide plugin");
        let device = before.device.expect("the declaration is what puts a device row on the answer");
        assert_eq!(
            (device.enabled, device.has_value, device.required_unset),
            (false, false, true)
        );

        // The value is the device's, and it is written with no project named.
        plugin_config_set("carry".into(), "endpoint".into(), "https://example.invalid/in".into(), None)
            .unwrap();
        let filled = row().device.unwrap();
        assert!(filled.has_value && !filled.required_unset, "the device holds it now");

        // One gate, opened without naming a project — and opening it is itself the consent to let the
        // plugin read every project on the machine.
        assert!(plugin_set_enabled("carry".into(), None, true).unwrap().enabled);
        let on = row();
        assert!(on.device.unwrap().enabled);
        assert!(on.projects.is_empty(), "opening the device's gate draws no project row");

        // Standing in a project changes none of that: the declaration picks the layer, the caller's
        // location only feeds it — so a face that passed its project along still moved the one gate.
        assert!(!plugin_set_enabled("carry".into(), Some(project_id), false).unwrap().enabled);
        assert!(!row().device.unwrap().enabled);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Turning a plugin off hands back what that threw away (`AMB-D-399`), so the switch on screen can
    /// say it. The events are gone for good — a disabled plugin is not caught up on afterwards — and
    /// the number is the only trace of them there will ever be.
    #[test]
    fn disabling_reports_the_queued_events_it_dropped() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("plugin-gate-dropped");
        std::env::set_var("AMENBO_HOME", &tmp);
        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        plant_plugin(&tmp, "notify");
        assert!(plugin_set_enabled("notify".into(), Some(project_id), true).unwrap().enabled);

        {
            let store = Store::open().unwrap();
            let tx = store.read_model().write().unwrap();
            for record_id in 1..=2 {
                tx.queue_event(&amenbo_core::store_engine::QueuedEvent {
                    plugin: "notify",
                    face: "gui",
                    event: "task.created",
                    record_id,
                    actor: "human",
                    at: "2026-07-26T09:00:00Z",
                    new_state: None,
                    project: Some(project_id),
                    record: None,
                    parent: None,
                })
                .unwrap();
            }
            tx.commit().unwrap();
        }

        let off = plugin_set_enabled("notify".into(), Some(project_id), false).unwrap();
        assert!(!off.enabled);
        assert_eq!(off.dropped_queued, 2, "both waiting events went, and the count came back");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// What a generated settings form is drawn from and writes through (`AMB-D-356`): the author's
    /// schema comes back with what the named project holds, the write routes by the author's `secret`
    /// flag alone, and a secret's value never comes back out — the form has "held" and nothing more.
    #[test]
    fn the_settings_carry_one_projects_values_and_a_secret_only_as_held() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("plugin-config");
        std::env::set_var("AMENBO_HOME", &tmp);
        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        plant_plugin_with(
            &tmp,
            "notify",
            serde_json::json!([
                {"key": "events", "label": "通知するイベント", "secret": false, "required": false},
                {"key": "token", "label": "APIトークン", "secret": true, "required": true},
            ]),
        );
        let field = |project: Option<i64>, key: &str| {
            plugin_config_read("notify".into(), project)
                .unwrap()
                .into_iter()
                .find(|f| f.key == key)
                .unwrap()
        };

        // The install row carries the author's schema and nothing a project holds — that is this read's
        // to answer (`AMB-D-412`).
        let declared = plugin_installs("en".into()).unwrap().into_iter().find(|r| r.name == "notify").unwrap();
        assert_eq!(
            drawn_fields(&declared.config).iter().map(|f| f.key.as_str()).collect::<Vec<_>>(),
            vec!["events", "token"],
            "the schema arrives in the author's order, without a project being named"
        );

        // The schema arrives whole, holding nothing yet — which is what the form draws "not provided"
        // and the enable gate refuses over.
        let events = field(Some(project_id), "events");
        assert_eq!(events.label, "通知するイベント");
        assert!(!events.secret && !events.required);
        assert_eq!((events.value, events.secret_set), (None, false));

        // Text: the value is the project's, and it is read for the project asked about and no other
        // (`AMB-D-434`). Without a project there is nothing to answer with, so the read refuses rather
        // than drawing every field blank.
        plugin_config_set("notify".into(), "events".into(), "deploy".into(), Some(project_id)).unwrap();
        assert_eq!(field(Some(project_id), "events").value.as_deref(), Some("deploy"));
        assert!(plugin_config_read("notify".into(), None).is_err(), "no project named, nothing to read");

        // Secret: routed by the author's flag to the table of its own, and reported as held —
        // the value itself is for injection at run time, never for a webview.
        plugin_config_set("notify".into(), "token".into(), "s3cret".into(), Some(project_id)).unwrap();
        let token = field(Some(project_id), "token");
        assert!(token.secret_set, "a held secret is what the form masks");
        assert_eq!(token.value, None, "never the value itself");
        let config_raw = std::fs::read_to_string(tmp.join("config.json")).unwrap_or_default();
        assert!(!config_raw.contains("s3cret"), "a secret must not reach config.json");

        // The empty value is the clear.
        plugin_config_set("notify".into(), "events".into(), String::new(), Some(project_id)).unwrap();
        assert_eq!(field(Some(project_id), "events").value, None, "the setting is gone");

        // A key the manifest does not declare has no routing rule — Amenbo does not invent one.
        assert!(plugin_config_set("notify".into(), "nope".into(), "x".into(), Some(project_id)).is_err());

        // And a write with no project named is refused rather than aimed somewhere.
        assert!(plugin_config_set("notify".into(), "events".into(), "x".into(), None).is_err());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// What a field offering candidates hands the form (`AMB-D-415`): the author's declaration — what to
    /// draw, what to offer, what is in force before anyone answers — and, per project, which of the three
    /// answers is being given. The form draws checkboxes from the first and ticks them from the second, so
    /// a state it had to infer from the stored string is a rule that would drift from core's.
    #[test]
    fn a_setting_that_offers_candidates_hands_over_its_choices_and_which_answer_is_given() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("plugin-config-multi");
        std::env::set_var("AMENBO_HOME", &tmp);
        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        plant_plugin_with(
            &tmp,
            "notify",
            serde_json::json!([{
                "key": "events", "label": "通知するイベント", "type": "multi",
                "options": [
                    {"value": "task.done", "label": "完了した"},
                    {"value": "task.rejected", "label": "見送った"},
                ],
                "default": "task.done",
            }]),
        );
        let declared = plugin_installs("en".into()).unwrap().into_iter().find(|r| r.name == "notify").unwrap();
        let events = drawn_fields(&declared.config)[0];
        assert_eq!(events.field_type, amenbo_core::plugin_manifest::FieldType::Multi);
        assert_eq!(
            events.options.iter().map(|o| o.value.as_str()).collect::<Vec<_>>(),
            vec!["task.done", "task.rejected"],
            "the candidates arrive in the author's order, without a project being named",
        );
        assert_eq!(events.default_value.as_deref(), Some("task.done"));

        let held = |key: &str| {
            plugin_config_read("notify".into(), Some(project_id))
                .unwrap()
                .into_iter()
                .find(|f| f.key == key)
                .unwrap()
        };

        // Nobody has answered: the default is what a run receives, and the form says as much rather than
        // drawing an empty box that looks like a refusal.
        assert_eq!(held("events").state, "unanswered");

        plugin_config_set("notify".into(), "events".into(), "task.rejected".into(), Some(project_id))
            .unwrap();
        let chosen = held("events");
        assert_eq!((chosen.state.as_str(), chosen.value.as_deref()), ("chosen", Some("task.rejected")));

        // Wanting none of them is its own answer, and the form has to be able to draw it apart from the
        // one above.
        plugin_config_set("notify".into(), "events".into(), "none".into(), Some(project_id)).unwrap();
        assert_eq!(held("events").state, "none");

        // Back to unanswered through the door an empty value opens — which is what "restore the default"
        // is made of.
        plugin_config_set("notify".into(), "events".into(), String::new(), Some(project_id)).unwrap();
        assert_eq!(held("events").state, "unanswered");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// What the market reads for the one plugin someone opened (`AMB-D-385`): the detail document's own
    /// fields — the switch, the events, the settings — plus the one judgement Amenbo adds, whether this
    /// build can run the thing at all. Driven off the caches, with the network pointed nowhere, because
    /// falling back to them is how the market answers offline anyway.
    #[test]
    fn the_opened_entrys_detail_says_what_installing_would_mean() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("plugin-detail");
        std::env::set_var("AMENBO_HOME", &tmp);
        std::env::set_var("AMENBO_PLUGIN_CATALOG_URL", "http://127.0.0.1:9/catalog.json");
        let registry = amenbo_core::config::Paths::at(tmp.clone()).registry_dir();
        std::fs::create_dir_all(&registry).unwrap();
        std::fs::write(
            registry.join("official.json"),
            serde_json::json!({
                "catalog_v": 1,
                "generated_at": "2026-07-23T04:57:10Z",
                "plugins": [{
                    "name": "notify",
                    "desc": "a plugin",
                    "author": "amenbo",
                    "repo": "ShiroDoromoto/amenbo",
                    "os": ["macos", "linux", "windows"],
                    "category": "workflow",
                }],
            })
            .to_string(),
        )
        .unwrap();
        let write_detail = |payload_v: u32| {
            std::fs::write(
                registry.join("detail-notify.json"),
                serde_json::json!({
                    "name": "notify",
                    "url": "https://example.com/notify.tar.gz",
                    "checksum": "sha256:deadbeef",
                    "payload_v": payload_v,
                    "config": [
                        {"key": "webhook", "label": "Webhook URL", "secret": true, "required": true},
                    ],
                    "events": ["task.created", "task.completed"],
                    "about": "## what it is for\n\nin the author's own words",
                    "i18n": {"ja": {"about": "作者の言葉で"}},
                })
                .to_string(),
            )
            .unwrap();
        };

        write_detail(amenbo_core::plugin_payload::VERSION);
        let detail = tauri::async_runtime::block_on(plugin_detail("notify".into(), "en".into())).unwrap().unwrap();
        assert_eq!(detail.events, vec!["task.created".to_string(), "task.completed".to_string()]);
        let declared = drawn_fields(&detail.config);
        assert_eq!(declared.len(), 1);
        assert!(declared[0].secret && declared[0].required);
        assert_eq!(declared[0].label, "Webhook URL");
        assert!(detail.compatible && detail.incompatible_reason.is_none());

        // The author's description (`AMB-D-638`), and their own text beside the reader's language rather
        // than under it (`AMB-D-623`) — the face is what picks between the two.
        assert!(detail.about.as_deref().unwrap().contains("in the author's own words"));
        assert_eq!(detail.about_i18n, None, "nothing was published for the language asked for");
        let ja = tauri::async_runtime::block_on(plugin_detail("notify".into(), "ja".into())).unwrap().unwrap();
        assert_eq!(ja.about_i18n.as_deref(), Some("作者の言葉で"));
        assert_eq!(ja.about, detail.about, "the base text travels whatever is asked for");

        // A build speaking another payload contract is answered before an install, not at the enable.
        write_detail(amenbo_core::plugin_payload::VERSION + 1);
        let other = tauri::async_runtime::block_on(plugin_detail("notify".into(), "en".into())).unwrap().unwrap();
        assert!(!other.compatible);
        assert!(other.incompatible_reason.is_some(), "core's own sentence, not a made-up one");

        // A name no catalog carries has no detail here — an answer, not a failure.
        assert!(tauri::async_runtime::block_on(plugin_detail("elsewhere".into(), "en".into())).unwrap().is_none());

        std::env::remove_var("AMENBO_PLUGIN_CATALOG_URL");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A plugin off a **registered** catalog is readable before it is installed (`AMB-D-389`): the row is
    /// in the merged list, so the detail behind it has to come from the catalog that served the row.
    /// Answered on a port rather than from a file, because a registered catalog is reached by URL and
    /// caches under a name derived from it — a fixture on disk would prove the cache, not the join.
    #[test]
    fn a_registered_catalogs_plugin_opens_from_the_catalog_that_served_it() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("plugin-detail-registered");
        std::env::set_var("AMENBO_HOME", &tmp);
        // The official catalog answers from its cache alone: nothing listens there, and it lists a
        // different name, so what comes back can only have come off the registered shelf.
        std::env::set_var("AMENBO_PLUGIN_CATALOG_URL", "http://127.0.0.1:9/catalog.json");
        let registry = amenbo_core::config::Paths::at(tmp.clone()).registry_dir();
        std::fs::create_dir_all(&registry).unwrap();
        let entry = |name: &str| {
            serde_json::json!({
                "name": name,
                "desc": "a plugin",
                "author": "amenbo",
                "repo": "ShiroDoromoto/amenbo",
                "os": ["macos", "linux", "windows"],
                "category": "workflow",
            })
        };
        let catalog = |name: &str| {
            serde_json::json!({
                "catalog_v": 1,
                "generated_at": "2026-07-27T00:00:00Z",
                "plugins": [entry(name)],
            })
            .to_string()
        };
        std::fs::write(registry.join("official.json"), catalog("notify")).unwrap();

        let host = amenbo_static_host::StaticHost::serve([
            ("/third/catalog.json", catalog("inhouse")),
            (
                "/third/plugins/inhouse.json",
                serde_json::json!({
                    "name": "inhouse",
                    "url": "https://example.invalid/inhouse.tar.gz",
                    "checksum": format!("sha256:{}", "c".repeat(64)),
                    "scope": "machine",
                    "payload_v": amenbo_core::plugin_payload::VERSION,
                    "config": [],
                    "events": ["task.created"],
                })
                .to_string(),
            ),
        ]);
        let source_url = host.url("/third/catalog.json");
        std::fs::write(
            registry.join(amenbo_core::plugin_catalog::SOURCES_FILE_NAME),
            serde_json::json!({ "sources": [{ "url": source_url, "name": "社内カタログ" }] })
                .to_string(),
        )
        .unwrap();

        let detail =
            tauri::async_runtime::block_on(plugin_detail("inhouse".into(), "en".into())).unwrap().unwrap();
        assert_eq!(
            detail.events,
            vec!["task.created".to_string()],
            "the registered catalog's own document, not the official one"
        );
        assert!(detail.compatible && detail.incompatible_reason.is_none());

        std::env::remove_var("AMENBO_PLUGIN_CATALOG_URL");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// In WAL mode, an external writer's commit (the CLI, the AI) lands only in `store.sqlite-wal`,
    /// and the mtime of `store.sqlite` itself does not move until a checkpoint. The change signature
    /// rests on **`PRAGMA data_version`**, so a write from another process always moves it, even
    /// though the main file was never touched — pinned here against a real store.
    #[test]
    fn store_signature_moves_on_an_external_writers_wal_only_commit() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("wal-sig");
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let before = store_signature_string();
        assert!(!before.is_empty(), "a signature is produced when a store exists");

        {
            let mut writer = Store::open().unwrap();
            writer
                .add_task(amenbo_core::ops::task::NewTask {
                    title: "外から届いたタスク".into(),
                    project_id: Some(project_id),
                    due_on: None,
                    start_on: None,
                    priority: None,
                    notes: String::new(),
                    created_by_kind: Some(ActorKind::Ai),
                    at_binding_id: None,
                })
                .unwrap();
        }

        assert_ne!(before, store_signature_string(), "an external writer's commit moves the signature");

        assert!(!store_signature_string().contains('|'), "does not mix in the `|` separator");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The store's `-wal`/`-shm` can be cleared while this process holds the watch connection — a
    /// `restore` and a migration both do it deliberately, and they cannot reach a connection that was
    /// already open. What is left is an orphan, and SQLite shares one shared-memory node per process,
    /// so it takes every later connection down with it: opens succeed, reads answer, writes come back
    /// `disk I/O error` — for the life of the process, which is why restarting the app was the only
    /// way out. Pinned here at the write open: the sidecars go, and the next write still lands.
    ///
    /// Unix only, because the scene this sets is one Windows will not let anyone build: a delete
    /// there is refused outright while another handle holds the file open, so the sidecars cannot go
    /// out from under a live connection and the orphan this guards against cannot arise.
    #[cfg(unix)]
    #[test]
    fn a_write_survives_the_sidecars_being_cleared_under_the_watch_connection() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("orphaned-shm");
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };

        // Take the watch connection, and leave committed frames in the WAL for it to be holding an
        // index of — an empty WAL leaves nothing to go stale.
        assert!(!store_signature_string().is_empty(), "the watch connection is open");
        {
            let mut writer = Store::open().unwrap();
            writer
                .add_task(amenbo_core::ops::task::NewTask {
                    title: "先に届いたタスク".into(),
                    project_id: Some(project_id),
                    due_on: None,
                    start_on: None,
                    priority: None,
                    notes: String::new(),
                    created_by_kind: Some(ActorKind::Ai),
                    at_binding_id: None,
                })
                .unwrap();
        }

        let store_path = store_file().expect("the store path resolves");
        assert!(shm_file(&store_path).exists(), "a live WAL connection keeps -shm on disk");
        for ext in ["-wal", "-shm"] {
            let mut side = store_path.as_os_str().to_os_string();
            side.push(ext);
            std::fs::remove_file(std::path::PathBuf::from(side)).expect("the sidecar is cleared");
        }

        let mut store = open_store().expect("the store opens");
        store
            .add_task(amenbo_core::ops::task::NewTask {
                title: "巻き添えを免れたタスク".into(),
                project_id: Some(project_id),
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: Some(ActorKind::Human),
                at_binding_id: None,
            })
            .expect("a write still lands after the sidecars were cleared");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `config.json` is written straight to disk, never through a write transaction, so it moves
    /// neither `data_version` nor the store file. The signature has to carry it on a leg of its own:
    /// the watcher is woken for the file already (it shares the store's directory), and with nothing
    /// in the signature to show for it that wake is dropped as spurious — which is a language set
    /// from the CLI sitting unread in a GUI somebody has open, until the next restart.
    #[test]
    fn store_signature_moves_when_the_config_file_is_written_from_outside() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("config-sig");
        std::env::set_var("AMENBO_HOME", &tmp);

        Store::open().unwrap();
        let before = store_signature_string();
        assert!(!before.is_empty(), "a signature is produced when a store exists");

        let paths = amenbo_core::config::Paths::resolve().unwrap();
        let mut config = amenbo_core::config::Config::load(&paths.config_file);
        config.set("language", "de").unwrap();
        config.save(&paths.config_file).unwrap();

        assert_ne!(
            before,
            store_signature_string(),
            "a config written outside the database moves the signature"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `decision_search` **against a real store**, because a command whose only exercise is a mocked
    /// frontend is a command nobody has run. This one shipped broken: it built its params with
    /// `..Default::default()`, whose empty sort string reached core as an unknown sort key, so every call
    /// failed — and the screen, reading "no answer" as "nothing was asked", answered a search by showing
    /// every decision. Every layer's own tests were green.
    ///
    /// So this asserts the thing the mocks cannot: that calling it returns the ids, and that the match
    /// reaches a **comment body** — the arm the whole command exists for, since the page payload the client
    /// filters over does not carry one.
    #[test]
    fn decision_search_runs_against_a_real_store_and_reaches_comment_bodies() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("decision-search");
        std::env::set_var("AMENBO_HOME", &tmp);

        let mut store = Store::open().unwrap();
        let project_id = store
            .project_add(amenbo_core::ops::project::NewProject {
                name: "テストPJ".into(),
                view: View::List,
                notes: String::new(),
                color: None,
            })
            .unwrap()
            .id;
        let new = |title: &str, body: &str| amenbo_core::ops::decision::NewDecision {
            title: title.to_string(),
            body: body.to_string(),
            project_id,
        };
        // The term is in neither title nor body — only in a comment, which is what the client-side search
        // could not see.
        let commented = store.add_decision(new("カタログの署名", "公開鍵は同梱する")).unwrap();
        store.add_decision_comment(commented.id, ActorKind::Ai, "ここには出るはず").unwrap();
        let other = store.add_decision(new("別の決定", "無関係な本文")).unwrap();
        drop(store);

        let hits = decision_search(project_id, "出るはず".to_string()).expect("the command runs");
        assert_eq!(hits, vec![commented.id], "the comment arm hits, and narrows to it");
        assert!(!hits.contains(&other.id));

        // A term nowhere is an empty answer, not an error — the screen shows nothing rather than everything.
        assert!(decision_search(project_id, "どこにも無い語".to_string()).unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `task_search` against a real store, for the reason its decision twin above is: the board's search
    /// is one call, and a call that only ever runs against a mock is a call nobody has run.
    ///
    /// The two things the mocks cannot say: that a **phrase** reaches core intact — the door the board
    /// gained by handing the term over structurally instead of through a filter expression the grammar
    /// splits on whitespace — and that the words may land on different faces of the same task.
    #[test]
    fn task_search_runs_against_a_real_store_and_takes_a_phrase() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("task-search");
        std::env::set_var("AMENBO_HOME", &tmp);

        let mut store = Store::open().unwrap();
        let project_id = store
            .project_add(amenbo_core::ops::project::NewProject {
                name: "テストPJ".into(),
                view: View::List,
                notes: String::new(),
                color: None,
            })
            .unwrap()
            .id;
        let mut add = |title: &str, notes: &str| {
            store
                .add_task(amenbo_core::ops::task::NewTask {
                    title: title.into(),
                    project_id: Some(project_id),
                    due_on: None,
                    start_on: None,
                    priority: None,
                    notes: notes.into(),
                    created_by_kind: Some(ActorKind::Ai),
                    at_binding_id: None,
                })
                .unwrap()
                .id
        };
        let both = add("配送の見直し", "計測してから決める");
        let title_only = add("配送の値付け", "");
        let commented = add("無関係な題", "");
        store.add_task_comment(commented, ActorKind::Ai, "ここには出るはず").unwrap();
        drop(store);

        // Two words, each landing on a different face of the same task — the query that had no way of
        // being written in the filter grammar at all.
        let hits = task_search(project_id, "配送 計測".to_string()).expect("the command runs");
        assert_eq!(hits, vec![both], "both terms must land, though not on the same face");
        assert!(!hits.contains(&title_only), "one of the two words is not enough");

        // The comment arm, as on the decision side: the board's cards carry a comment count, not bodies.
        assert_eq!(task_search(project_id, "出るはず".to_string()).unwrap(), vec![commented]);

        // A term nowhere is an empty answer, not an error.
        assert!(task_search(project_id, "どこにも無い語".to_string()).unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The card dates the task: both stamps ride the wire, so the detail pane can say when a task was
    /// filed and whether anything has written to it since. Pinned here because the two are the row's own
    /// columns forwarded untouched — a drop between the read and the DTO would leave the pane silently
    /// dateless rather than failing.
    #[test]
    fn the_card_carries_when_the_task_was_written() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("card-stamps");
        std::env::set_var("AMENBO_HOME", &tmp);

        let mut store = Store::open().unwrap();
        let project_id = store
            .project_add(amenbo_core::ops::project::NewProject {
                name: "テストPJ".into(),
                view: View::List,
                notes: String::new(),
                color: None,
            })
            .unwrap()
            .id;
        let id = store
            .add_task(amenbo_core::ops::task::NewTask {
                title: "いつ書かれたか".into(),
                project_id: Some(project_id),
                due_on: None,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: Some(ActorKind::Ai),
                at_binding_id: None,
            })
            .unwrap()
            .id;

        let card = {
            let read_model = store.read_model();
            let row = amenbo_core::store_engine::read::task_card_row(read_model.conn(), id).unwrap().unwrap();
            task_card_from_row(&store, row)
        };
        // The stamps core wrote, forwarded as they are stored. A task nobody has written to since carries
        // them equal, which is what lets the pane drop the second one.
        let detail = store.task_detail(id).unwrap();
        assert_eq!(card.created_at, detail.created_at.to_rfc3339_z(), "the card dates the task");
        assert_eq!(card.updated_at, detail.updated_at.to_rfc3339_z(), "and says when it was last written to");
        assert_eq!(card.created_at, card.updated_at, "nothing has written to it since it was filed");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The card gates the holder-side premise surface (`AMB-D-366`) on `in_progress`: a task that was never
    /// reserved carries no `premise_change` even with a blocker on it, and a premise that was already there
    /// *before* the reservation is not a change *after* it. (Detection of a premise pinned on after the
    /// status began — the `Some` path — is core's, pinned in `store_engine::read`'s own tests; here we pin
    /// that `task_card_from_row` runs the read only for the holder and forwards a no-change as `None`.)
    #[test]
    fn the_card_reads_the_premise_surface_only_for_an_in_progress_holder() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("premise-card");
        std::env::set_var("AMENBO_HOME", &tmp);

        let mut store = Store::open().unwrap();
        let project_id = store
            .project_add(amenbo_core::ops::project::NewProject {
                name: "テストPJ".into(),
                view: View::List,
                notes: String::new(),
                color: None,
            })
            .unwrap()
            .id;
        let mk = |store: &mut Store, title: &str| {
            let id = store
                .add_task(amenbo_core::ops::task::NewTask {
                    title: title.into(),
                    project_id: Some(project_id),
                    due_on: None,
                    start_on: None,
                    priority: None,
                    notes: String::new(),
                    created_by_kind: Some(ActorKind::Ai),
                    at_binding_id: None,
                })
                .unwrap()
                .id;
            store.finish_task_creation(id, ActorKind::Human).unwrap();
            id
        };
        let held = mk(&mut store, "タスク");
        let blocker = mk(&mut store, "ブロッカー");

        let card = |store: &Store, id: i64| {
            let read_model = store.read_model();
            let row = amenbo_core::store_engine::read::task_card_row(read_model.conn(), id)
                .unwrap()
                .unwrap();
            task_card_from_row(store, row)
        };

        // A blocker on a task that is still `todo` (never reserved): only a holder is at risk, so no surface.
        store.depend_task(held, blocker, Some(ActorKind::Ai)).unwrap();
        assert!(
            card(&store, held).premise_change.is_none(),
            "a task that was never reserved carries no holder-side surface, blocker or not"
        );

        // Drop the blocker so the task is ready, then reserve it. The blocker was there *before* the
        // reservation and is now gone, so nothing was pinned on *after* the status began → no change.
        store.undepend_task(held, blocker).unwrap();
        store.set_task_status(held, TaskStatus::InProgress, ActorKind::Ai).unwrap();
        assert_eq!(card(&store, held).status, "in_progress", "the task is reserved");
        assert!(
            card(&store, held).premise_change.is_none(),
            "with no premise pinned on after the reservation, the holder sees no change"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Pins that one wing of the reservation guard — **a reservation is refused while its premises
    /// are unmet** — holds on the GUI path too. `task_status` **passes core's error straight
    /// through**, and the front end's mutator puts the exception in a toast (`run` in `store.tsx`),
    /// so as long as `code` stays `not_ready` and `message_en` states **the reason and the way
    /// out**, a drop on the kanban board becomes a toast that says why. Let that slip and,
    /// from the GUI, it turns into "I dragged it and it silently snapped back". A card's column is
    /// drawn from the source of truth (`status`), so a refused reservation never moved the column in
    /// the first place — no optimistic-update rollback is needed. Also pins that the status does not
    /// regress.
    #[test]
    fn task_status_surfaces_not_ready_with_its_reason() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("notready");
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };

        let blocker = task_add(Some(project_id), "先行".into(), None, None, None).unwrap().tasks[0];
        let dependent = task_add(Some(project_id), "後続".into(), None, None, None).unwrap().tasks[0];
        finish_creating(blocker);
        finish_creating(dependent);
        {
            let mut store = Store::open().unwrap();
            store.depend_task(dependent, blocker, Some(ActorKind::Human)).unwrap();
        }

        let err = task_status(dependent, "in_progress".into())
            .err()
            .expect("reservation is rejected when the premise is unmet");
        assert_eq!(err.code, "not_ready", "code reaches the webview as not_ready");
        assert!(err.message_en.contains("blocker"), "the reason names the blocker: {}", err.message_en);

        let card = |id: i64| tasks_by_ids(vec![id]).unwrap().into_iter().next().unwrap();
        assert_eq!(card(dependent).status, "todo", "a rejected reservation does not move the column (no rollback needed)");

        task_status(blocker, "done".into()).unwrap();
        task_status(dependent, "in_progress".into()).expect("reservation succeeds once the premise clears");
        assert_eq!(card(dependent).status, "in_progress");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The card derives `ready` itself, so it can drift from what the reserve enforces. A start day
    /// still ahead holds a reservation down in core; if the card ignored it, the GUI would offer a
    /// task that `task status` then refuses. It names the day too, so the `ready: false` it draws is
    /// never one without a reason on screen.
    #[test]
    fn task_card_holds_ready_down_until_the_declared_start_day_arrives() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("startday");
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };

        let task = task_add(Some(project_id), "実装".into(), None, None, None).unwrap().tasks[0];
        finish_creating(task);
        let card = |id: i64| tasks_by_ids(vec![id]).unwrap().into_iter().next().unwrap();

        let c = card(task);
        assert!(c.ready, "nothing declared, nothing in the way");
        assert!(c.not_started_until.is_none(), "no start day is no reason");

        let set_start = |d: chrono::NaiveDate| {
            let mut store = Store::open().unwrap();
            store
                .update_task(task, amenbo_core::ops::task::TaskPatch {
                    start_on: Some(d),
                    ..Default::default()
                })
                .unwrap();
        };

        let today = amenbo_core::time::today();
        set_start(today + chrono::Duration::days(7));
        let c = card(task);
        assert!(!c.ready, "a start day still ahead holds the reservation down");
        assert_eq!(
            c.not_started_until.as_deref(),
            Some((today + chrono::Duration::days(7)).to_string().as_str()),
            "and the card names the day, so the reason is on screen"
        );

        set_start(today);
        let c = card(task);
        assert!(c.ready, "the day arrives and the task is startable");
        assert!(c.not_started_until.is_none(), "a day that has come is no longer a reason");
    }

    /// The reason a reservation was refused shows up only in a toast that vanishes in seconds. The
    /// card holds the same fact permanently and names **which decision is holding it down** — the
    /// detail pane draws that as a clickable affordance, and the ref leads somewhere from the card,
    /// not from the toast. The unsettled premises are a subset of `linked_decisions`; settle them
    /// and they disappear and `ready` goes up.
    #[test]
    fn task_card_names_the_unsettled_premise_that_holds_ready_down() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("premise");
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };

        let task = task_add(Some(project_id), "実装".into(), None, None, None).unwrap().tasks[0];
        finish_creating(task);
        let did = decision_add(project_id, "決めごと".into(), Some("結論".into()), None).unwrap().decisions[0];
        decision_set_link(did, task, true).unwrap();

        let card = |id: i64| tasks_by_ids(vec![id]).unwrap().into_iter().next().unwrap();

        let c = card(task);
        assert!(!c.ready, "ready stays false while the basis is unsettled");
        assert_eq!(c.blocked_by_decisions.len(), 1, "names the decision it is held on");
        assert_eq!(c.blocked_by_decisions[0].id, did, "the detail pane can navigate by that decision's id");
        assert!(c.blocked_by_decisions[0].r#ref.is_some(), "the conversational ref (D-n) is carried too");
        assert_eq!(c.linked_decisions.len(), 1, "an unsettled premise is a subset of linked_decisions");
        assert!(task_status(task, "in_progress".into()).is_err(), "reservation is rejected");

        decision_accept(did).unwrap();
        let c = card(task);
        assert!(c.ready, "ready once the basis is settled");
        assert!(c.blocked_by_decisions.is_empty(), "a settled premise no longer holds it back");
        assert_eq!(c.linked_decisions.len(), 1, "the link itself remains (traceability)");
        task_status(task, "in_progress".into()).expect("reservation succeeds once the premise settles");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `task_reject` exists for the one thing `task_status` cannot ask for — the reason — so what is
    /// under test is that the reason is **kept and required**: it lands on the timeline, an empty one
    /// is refused with nothing written, and re-rejecting does not pile a second copy on.
    #[test]
    fn task_reject_keeps_the_reason_and_refuses_an_empty_one() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("reject");
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let task = task_add(Some(project_id), "やらないと決めた作業".into(), None, None, None).unwrap().tasks[0];
        let card = |id: i64| tasks_by_ids(vec![id]).unwrap().into_iter().next().unwrap();

        let err = task_reject(task, "   ".into()).err().expect("an empty reason must be refused");
        assert_eq!(err.code, "invalid_value", "the refusal carries core's code, not a GUI-local one");
        assert_eq!(card(task).status, "todo", "and nothing was written — the status did not move");
        assert_eq!(card(task).comments, 0, "nor was a blank comment left behind");

        let ack = task_reject(task, "  測っても何も変わらなかった  ".into()).unwrap();
        assert_eq!(ack.tasks, vec![task], "the reject acks its task");
        let c = card(task);
        assert_eq!(c.status, "rejected");
        assert!(c.completed_at.is_none(), "a terminal, but not an achievement — no completion time");
        assert_eq!(c.comments, 1, "the reasoning is kept, as a comment");
        let body = {
            let store = Store::open().unwrap();
            store.comment_list(task, None, None).unwrap().comments[0].text.clone()
        };
        assert_eq!(body, "測っても何も変わらなかった", "trimmed, and otherwise as it was given");

        task_reject(task, "同じことを繰り返す".into()).unwrap();
        assert_eq!(card(task).comments, 1, "re-rejecting changes nothing, so it explains nothing twice");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Round-trips the write commands against an isolated store and checks that they land in the
    /// snapshot. What is under test is the wiring — args, emit, save, projection; the core ops
    /// themselves are already tested in core.
    #[test]
    fn write_commands_round_trip() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-test");
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                },
            )
            .unwrap();
            p.id
        };

        let ack = task_add(Some(project_id), "結線テスト".into(), None, None, None).unwrap();
        assert_eq!(ack.tasks.len(), 1, "task_add returns the new task id");
        assert!(ack.scopes.contains(&"tasks"), "task_add invalidates the task lists");
        let id = ack.tasks[0];
        let card = |id: i64| tasks_by_ids(vec![id]).unwrap().into_iter().next();
        let t = card(id).expect("task added");
        assert_eq!(t.project_id, Some(project_id));
        assert_eq!(t.status, "todo");
        assert_eq!(t.created_by.as_ref().map(|a| a.kind), Some("human"));
        assert_eq!(t.id, 1, "the id is the conversational number");
        assert_eq!(t.r#ref, "AMB-T-1", "ref is the namespaced form");

        finish_creating(id);
        let ack = task_status(id, "in_progress".into()).unwrap();
        assert_eq!(ack.tasks, vec![id], "status acks the task");
        assert!(ack.scopes.contains(&"tasks"), "status invalidates the task lists");
        let t = card(id).unwrap();
        assert_eq!(t.status, "in_progress", "todo→in_progress reserves it");

        let err = task_status(id, "in_progress".into())
            .err()
            .expect("re-reserving an already in_progress task must be rejected");
        assert_eq!(err.code, "already_reserved", "GUI double-reserve is rejected via the core CAS");
        assert_eq!(card(id).unwrap().status, "in_progress", "rejected reserve does not regress the status");

        let _ = task_status(id, "blocked".into()).unwrap();
        assert_eq!(card(id).unwrap().status, "blocked");

        let ack = comment_add(id, "コメント".into()).unwrap();
        assert_eq!(ack.tasks, vec![id], "comment acks its task");
        assert_eq!(card(id).unwrap().comments, 1);

        let comment_id = {
            let store = Store::open().unwrap();
            store.comment_list(id, None, None).unwrap().comments[0].id
        };
        let ack = decision_promote(comment_id, "昇格した決定".into()).unwrap();
        let did = ack.decisions[0];
        let promoted = decisions_by_ids(vec![did]).unwrap().into_iter().next().unwrap();
        assert_eq!(promoted.body, "コメント", "promoted decision body is the task_comment text");
        assert!(promoted.linked_tasks.iter().any(|l| l.id == id), "promoted decision links its task");

        task_status(id, "done".into()).unwrap();
        assert_eq!(card(id).unwrap().status, "done");

        let snap = snapshot().unwrap();
        assert!(snap.activity.iter().any(|a| a.kind == "system"), "system events emitted");
        assert!(snap.activity.iter().any(|a| a.kind == "comment"), "comment in activity");

        let ack = dimension_add(project_id, "軸2".into()).unwrap();
        assert!(ack.scopes.contains(&"tasks"), "dimension change invalidates the board lists");
        let proj_snap = snapshot().unwrap();
        let proj = proj_snap.projects.iter().find(|p| p.id == project_id).unwrap();
        assert!(proj.dimensions.iter().any(|d| d.name == "軸2"), "dimension added");

        let del_id = task_add(Some(project_id), "消す対象".into(), None, None, None).unwrap().tasks[0];
        let ack = task_delete(del_id).unwrap();
        assert_eq!(ack.tasks, vec![del_id], "delete acks the removed task");
        assert!(card(del_id).is_none(), "deleted task drops from the list");
        let snap = snapshot().unwrap();
        let deleted = snap
            .activity
            .iter()
            .find(|a| a.event.as_ref().is_some_and(|e| e.kind == "task.deleted"))
            .expect("the deletion is on the timeline");
        assert_eq!(deleted.kind, "system");
        assert_eq!(deleted.target.id, del_id);
        assert_eq!(
            deleted.target.title, "消す対象",
            "a deleted row's name lives only in the ledger payload (the DB cannot join to it)"
        );

        let ack = task_assign(id, Some("ai".into())).unwrap();
        assert!(ack.scopes.contains(&"tasks"), "assign invalidates the assignee-filtered lists");
        let t = card(id).unwrap();
        assert_eq!(t.assignee.as_ref().map(|a| a.kind), Some("ai"), "delegated to my AI");
        let _ = task_assign(id, None).unwrap();
        assert!(card(id).unwrap().assignee.is_none(), "unassigned");
        let snap = snapshot().unwrap();
        assert!(snap.activity.iter().any(|a| a.kind == "system" && a.event.as_ref().map(|e| e.kind == "task.assigned").unwrap_or(false)), "assigned event emitted");

        let sig_before = store_signature();
        let _ = task_add(Some(project_id), "シグネチャ確認".into(), None, None, None).unwrap();
        assert!(!sig_before.is_empty(), "store signature is non-empty when a store exists");
        assert_ne!(store_signature(), sig_before, "a write advances the store signature");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The two days a task can carry, from the face that had no way of writing them. Each one is set,
    /// read back off the card, and cleared — and clearing is asked for on its own, because a day that
    /// will not come off is a day the person cannot take back. The start day is asked one more thing:
    /// it is a premise of `ready`, so writing it has to move whether the task can be reserved at all.
    #[test]
    fn the_two_days_are_written_read_back_and_taken_away() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-days");
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "期日PJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let card = |id: i64| tasks_by_ids(vec![id]).unwrap().into_iter().next().expect("task");

        // Given at creation, both days are on the card the board draws from — no second visit needed.
        let id = task_add(
            Some(project_id),
            "日付つきで登録".into(),
            None,
            Some("2099-12-31".into()),
            Some("2099-01-01".into()),
        )
        .unwrap()
        .tasks[0];
        finish_creating(id);
        assert_eq!(card(id).due.as_deref(), Some("2099-12-31"));
        assert_eq!(card(id).start_on.as_deref(), Some("2099-01-01"));
        assert!(!card(id).ready, "a start day still ahead holds the task unready");
        assert_eq!(
            card(id).not_started_until.as_deref(),
            Some("2099-01-01"),
            "and says so as the premise, beside the field itself"
        );

        // Edited afterwards, one field at a time.
        task_set_due(id, Some("2099-06-30".into())).unwrap();
        assert_eq!(card(id).due.as_deref(), Some("2099-06-30"));

        // A day that has come is still the value the person put there, so the card carries it even
        // though it is no longer a reason to wait.
        task_set_start(id, Some("2000-01-01".into())).unwrap();
        let c = card(id);
        assert_eq!(c.start_on.as_deref(), Some("2000-01-01"), "the field holds what was written");
        assert!(c.not_started_until.is_none(), "a day that has come is no longer a premise");
        assert!(c.ready, "and the task is reservable again");

        // Relative forms are read the way the CLI reads them, off this machine's own today
        // (`AMB-D-429`) — the GUI's date input sends `YYYY-MM-DD`, but the face does not narrow to it.
        task_set_due(id, Some("today".into())).unwrap();
        assert_eq!(
            card(id).due.as_deref(),
            Some(amenbo_core::time::date_to_string(amenbo_core::time::today()).as_str())
        );

        // Both ways of saying "no day": nothing at all, and the empty string a cleared date input sends.
        let ack = task_set_due(id, None).unwrap();
        assert!(ack.scopes.contains(&"tasks"), "the cards draw the due date, so they refetch");
        assert!(card(id).due.is_none(), "the due date comes off");
        task_set_start(id, Some(String::new())).unwrap();
        assert!(card(id).start_on.is_none(), "and so does the start day");

        // A date that is not one is refused rather than quietly ignored.
        let err = task_set_due(id, Some("31/12/2099".into())).err().expect("not a date");
        assert_eq!(err.code, "invalid_value");
        assert!(card(id).due.is_none(), "and the refusal wrote nothing");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The two stages of a creation on the GUI's face (`AMB-D-554`): what `task_add` hands back is still
    /// being created — drawn on the board like any other card, but refused a reservation — and
    /// `task_finish_creating` is what ends that. Finishing one already finished is a no-op, and it is
    /// short-circuited rather than written, so nothing lands on the ledger saying nothing changed.
    #[test]
    fn a_creation_is_two_stages_here_too() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-finish-creating");
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "作成PJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let card = |id: i64| tasks_by_ids(vec![id]).unwrap().into_iter().next().unwrap();

        let id = task_add(Some(project_id), "作りかけ".into(), None, None, None).unwrap().tasks[0];
        let t = card(id);
        assert!(t.draft, "a creation lands unfinished");
        assert!(!t.ready, "which is the fourth premise holding the reservation back");
        let err = task_status(id, "in_progress".into())
            .err()
            .expect("a task still being created cannot be reserved");
        assert_eq!(err.code, "not_ready", "and it is the premise, not the CAS, that turns it away");

        let ack = task_finish_creating(id).unwrap();
        assert_eq!(ack.tasks, vec![id], "finishing acks the task");
        assert!(ack.scopes.contains(&"tasks"), "and invalidates the lists it now belongs in");
        let t = card(id);
        assert!(!t.draft, "the creation is finished");
        assert!(t.ready, "so nothing holds the reservation back");
        assert_eq!(t.status, "todo", "and ending a creation moves nothing else");

        let before = store_signature();
        task_finish_creating(id).unwrap();
        assert_eq!(store_signature(), before, "finishing an already finished creation writes nothing");

        let _ = task_status(id, "in_progress".into()).unwrap();
        assert_eq!(card(id).status, "in_progress", "and now it can be picked up");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Checks the decision-comment wiring end to end against an isolated store.
    /// `decision_comment_add` writes to the dedicated `decision_comment` table, and
    /// `decision_comments` reads back DTOs, oldest first, carrying the author's facet and the
    /// relative time. The ack invalidates the decisions scope and the target decision — what makes
    /// the GUI refetch the thread. Reading an unknown decision is empty; posting to one is an error.
    #[test]
    fn decision_comment_commands_round_trip() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-deccomment");
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "決定PJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                },
            )
            .unwrap();
            p.id
        };
        let did = decision_add(project_id, "決めごと".into(), Some("結論".into()), None)
            .unwrap()
            .decisions[0];

        assert!(decision_comments(did).unwrap().is_empty(), "no comments initially");

        let ack = decision_comment_add(did, "一言目".into()).unwrap();
        assert_eq!(ack.decisions, vec![did], "comment_add acks its decision");
        assert!(ack.scopes.contains(&"decisions"), "invalidates the decision views");
        let _ = decision_comment_add(did, "二言目".into()).unwrap();

        let comments = decision_comments(did).unwrap();
        assert_eq!(comments.len(), 2, "both comments read back");
        assert_eq!(comments[0].text, "一言目", "oldest first");
        assert_eq!(comments[1].text, "二言目");
        assert_eq!(comments[0].author.kind, "human", "human facet author");
        assert!(!comments[0].at.is_empty(), "the time the front end words is carried");

        let rm = decision_comment_remove(comments[0].id, did).unwrap();
        assert_eq!(rm.decisions, vec![did], "comment_remove acks its decision");
        assert!(rm.scopes.contains(&"decisions"), "invalidates the decision views");
        let left = decision_comments(did).unwrap();
        assert_eq!(left.len(), 1, "only the deleted comment is gone");
        assert_eq!(left[0].text, "二言目");
        assert!(decision_comment_remove(9999, did).is_ok(), "removing a gone comment is a noop");

        assert!(decision_comments(9999).unwrap().is_empty(), "unknown decision reads empty");
        assert!(decision_comment_add(9999, "x".into()).is_err(), "unknown decision rejects a comment");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Pins that the decision card (`decisions_by_ids` → `decision_card_row` →
    /// `decision_card_from_row`) carries every one of its cross-link fields. Everything the decision
    /// detail pane draws — the supersession chain, amendments, premises and their rot, the status of
    /// the work it spawned — rides on this one DTO.
    #[test]
    fn decision_card_carries_every_cross_link() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-deccard");
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "決定PJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let add = |title: &str| decision_add(project_id, title.into(), Some("結論".into()), None).unwrap().decisions[0];
        let card = |id: i64| decisions_by_ids(vec![id]).unwrap().into_iter().next().unwrap();

        let old = add("UTC で保存する");
        let partial = add("端では現地時刻で出す");
        let premise = add("台帳は末尾から読む");
        let head = add("整数キーで持つ");

        decision_supersede(head, old).unwrap();
        decision_amend(head, partial).unwrap();
        decision_builds_on(head, premise).unwrap();
        let shipped = task_add(Some(project_id), "整数キーへ移行".into(), None, None, None).unwrap().tasks[0];
        let pending = task_add(Some(project_id), "GUI を追従させる".into(), None, None, None).unwrap().tasks[0];
        decision_set_link(head, shipped, true).unwrap();
        decision_set_link(head, pending, true).unwrap();
        task_status(shipped, "done".into()).unwrap();

        let c = card(head);
        assert_eq!(c.r#ref, amenbo_core::idref::decision(head), "the conversational ref is the display form of the id");
        assert_eq!(c.status, "accepted", "supersede promotes the drawing side to accepted");
        assert!(c.decided_at.is_some(), "an accepted decision has a decided-on date");
        assert!(!c.decided_by.as_ref().unwrap().name.is_empty(), "who decided is carried too");
        assert_eq!(c.supersedes.len(), 1, "the decision it superseded");
        assert_eq!(c.supersedes[0].id, old);
        assert_eq!(c.supersedes[0].r#ref, Some(amenbo_core::idref::decision(old)), "carries the other side's ref too");
        assert_eq!(c.amends.len(), 1, "the decision it partly amended");
        assert_eq!(c.amends[0].id, partial);
        assert_eq!(c.builds_on.len(), 1, "the decision it builds on");
        assert_eq!(c.builds_on[0].id, premise);
        assert!(c.builds_on[0].superseded_by.is_none(), "no rot note when nothing replaced the premise");
        assert!(c.superseded_by.is_empty(), "the reverse lookup is still empty");
        assert!(c.amended_by.is_empty());
        assert!(c.built_on_by.is_empty());

        let mut linked: Vec<_> = c.linked_tasks.iter().map(|t| (t.id, t.status.as_str())).collect();
        linked.sort();
        assert_eq!(linked, vec![(shipped, "done"), (pending, "todo")], "remaining work and finished work");
        assert!(c.linked_tasks[0].r#ref.is_some(), "the task's conversational ref is carried too (the detail pane uses it to navigate)");

        let c_old = card(old);
        assert_eq!(c_old.superseded_by.iter().map(|r| r.id).collect::<Vec<_>>(), vec![head]);
        assert_eq!(card(partial).amended_by.iter().map(|r| r.id).collect::<Vec<_>>(), vec![head]);
        assert!(card(partial).superseded_by.is_empty(), "amending draws no supersedes edge at the target");
        assert_eq!(card(premise).built_on_by.iter().map(|r| r.id).collect::<Vec<_>>(), vec![head], "the radius of impact");

        let killer = add("台帳は先頭から読む");
        decision_supersede(killer, premise).unwrap();
        let c = card(head);
        assert_eq!(
            c.builds_on[0].superseded_by,
            Some(amenbo_core::idref::decision(killer)),
            "surfaces the decision standing on the rotted premise",
        );

        decision_unlink_edge(head, old).unwrap();
        assert!(card(head).supersedes.is_empty(), "an unlinked edge disappears from the card");
        assert!(card(old).superseded_by.is_empty(), "unlinking supersedes takes the edge off the target (no cleanup)");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Checks taking back a task comment (deleting it for good) at the GUI command layer. A comment
    /// added with `comment_add` is removed by `comment_remove` and drops out of the task's activity,
    /// which is where the GUI's comment list comes from. The ack has the same scope as `comment_add`
    /// (tasks plus the target task), so the card's comment count is refetched too.
    #[test]
    fn comment_remove_drops_it_from_the_task_activity() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-commentrm");
        std::env::set_var("AMENBO_HOME", &tmp);

        provision_project("PJ").unwrap();
        let project_id = snapshot().unwrap().projects[0].id;
        let tid = task_add(Some(project_id), "コメントを消す".into(), None, None, None).unwrap().tasks[0];

        let _ = comment_add(tid, "誤投稿".into()).unwrap();
        let _ = comment_add(tid, "残すコメント".into()).unwrap();
        let comments = |id: i64| -> Vec<ActivityItemDto> {
            task_activity(id, None).unwrap().into_iter().filter(|a| a.kind == "comment").collect()
        };
        let before = comments(tid);
        assert_eq!(before.len(), 2, "both comments are on the timeline");
        let mistaken = before.last().unwrap();
        assert_eq!(mistaken.text.as_deref(), Some("誤投稿"));

        let ack = comment_remove(mistaken.id, tid).unwrap();
        assert_eq!(ack.tasks, vec![tid], "comment_remove acks its task (the card's comment count moves)");
        assert!(ack.scopes.contains(&"tasks"), "invalidates the lists");

        let left = comments(tid);
        assert_eq!(left.len(), 1, "only the deleted comment is gone");
        assert_eq!(left[0].text.as_deref(), Some("残すコメント"));
        assert!(comment_remove(9999, tid).is_ok(), "removing a gone comment is a noop");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A comment can be fixed in place — not deleted and reposted. The id does not change, and its
    /// position on the timeline does not move. The ack has the same scope as `comment_add`.
    #[test]
    fn comment_edit_rewrites_the_body_in_place() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-commentedit");
        std::env::set_var("AMENBO_HOME", &tmp);

        provision_project("PJ").unwrap();
        let project_id = snapshot().unwrap().projects[0].id;
        let tid = task_add(Some(project_id), "コメントを直す".into(), None, None, None).unwrap().tasks[0];
        let _ = comment_add(tid, "誤字のある投稿".into()).unwrap();

        let posted: Vec<ActivityItemDto> =
            task_activity(tid, None).unwrap().into_iter().filter(|a| a.kind == "comment").collect();
        let cid = posted[0].id;

        let ack = comment_edit(cid, tid, "直した投稿".into()).unwrap();
        assert_eq!(ack.tasks, vec![tid], "comment_edit acks its task");
        assert!(ack.scopes.contains(&"tasks"), "invalidates the lists");

        let after: Vec<ActivityItemDto> =
            task_activity(tid, None).unwrap().into_iter().filter(|a| a.kind == "comment").collect();
        assert_eq!(after.len(), 1, "editing does not post a second comment");
        assert_eq!(after[0].id, cid, "the id survives the edit");
        assert_eq!(after[0].text.as_deref(), Some("直した投稿"));
        assert!(comment_edit(9999, tid, "x".into()).is_err(), "editing a gone comment is an error");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Round-trips the project settings commands: `project_get` returns the fields for the prefill,
    /// `project_update` applies the delta (name/notes/color/view), `project_set_archived` takes the
    /// project out of the snapshot (`project_overview` — live and not archived) and brings it back,
    /// and `project_delete` destroys it for good. The evidence that the wiring holds.
    #[test]
    fn project_settings_round_trip_via_command() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-projset");
        std::env::set_var("AMENBO_HOME", &tmp);

        provision_project("設定PJ").unwrap();
        let project_id = snapshot()
            .unwrap()
            .projects
            .iter()
            .find(|p| p.name == "設定PJ")
            .expect("created project is visible")
            .id;

        let got = project_get(project_id).unwrap();
        assert_eq!(got.name, "設定PJ");
        assert_eq!(got.notes, "");
        assert!(!got.archived);

        let ack = project_update(
            project_id,
            Some("改名PJ".into()),
            Some("メモ本文".into()),
            None,
            Some("list".into()),
        )
        .unwrap();
        assert!(ack.scopes.contains(&"tasks"), "project_update invalidates the board/project lists");
        let got = project_get(project_id).unwrap();
        assert_eq!(got.name, "改名PJ", "rename persisted");
        assert_eq!(got.notes, "メモ本文", "notes persisted");
        assert_eq!(got.view, "list", "default view persisted");

        assert!(
            project_update(project_id, None, None, None, Some("kanban".into())).is_err(),
            "an invalid view is rejected"
        );

        project_set_archived(project_id, true).unwrap();
        assert!(
            !snapshot().unwrap().projects.iter().any(|p| p.id == project_id),
            "archived project drops out of the sidebar projection"
        );
        assert!(
            project_list_archived().unwrap().iter().any(|p| p.id == project_id),
            "archived project appears in the archived read path"
        );
        assert!(project_get(project_id).unwrap().archived, "get still reads the archived project");

        project_set_archived(project_id, false).unwrap();
        assert!(
            snapshot().unwrap().projects.iter().any(|p| p.id == project_id),
            "unarchived project returns to the sidebar projection"
        );
        assert!(
            !project_list_archived().unwrap().iter().any(|p| p.id == project_id),
            "unarchived project leaves the archived read path"
        );

        project_delete(project_id).unwrap();
        assert!(
            !snapshot().unwrap().projects.iter().any(|p| p.id == project_id),
            "deleted project is gone from the projection"
        );
        assert!(project_get(project_id).is_err(), "get of a deleted project errors");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The `project_move` command wires through to `project.order_key` (`Store::project_move`) and
    /// changes the order of the sidebar projection (`build_snapshot` stacks projects in `order_key`
    /// order). Creates three projects — appended at the bottom, so they come out in creation
    /// order — reorders them with `before`, `top` and `bottom`, and checks that the snapshot's order
    /// moves with them. This is what drag-and-drop rests on. An invalid position, or a missing
    /// anchor, is refused.
    #[test]
    fn project_move_reorders_via_command() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-projmove");
        std::env::set_var("AMENBO_HOME", &tmp);

        let (a, b, c) = {
            let mut store = Store::open().unwrap();
            let mk = |store: &mut Store, name: &str| {
                store.project_add(
                    amenbo_core::ops::project::NewProject {
                        name: name.into(),
                        view: View::List,
                        notes: String::new(),
                        color: None,
                    },
                )
                .unwrap()
                .id
            };
            let a = mk(&mut store, "A");
            let b = mk(&mut store, "B");
            let c = mk(&mut store, "C");
            (a, b, c)
        };

        let order = || -> Vec<i64> { snapshot().unwrap().projects.iter().map(|p| p.id).collect() };
        assert_eq!(order(), vec![a, b, c], "initial order is creation (bottom) order");

        let ack = project_move(c, "before".into(), Some(a)).unwrap();
        assert!(ack.scopes.contains(&"tasks"), "project_move invalidates the board/project lists");
        assert_eq!(order(), vec![c, a, b], "C moved before A");

        project_move(a, "bottom".into(), None).unwrap();
        assert_eq!(order(), vec![c, b, a], "A moved to the bottom");

        project_move(b, "top".into(), None).unwrap();
        assert_eq!(order(), vec![b, c, a], "B moved to the top");

        assert!(project_move(a, "sideways".into(), None).is_err(), "an invalid position is rejected");
        assert!(project_move(a, "before".into(), None).is_err(), "before without an anchor is rejected");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Round-trips per-comment attachments at the command layer. `attachment_add_bytes` with
    /// `target_type="task_comment"` hangs an attachment off a comment id, `attachments_for` reads it
    /// back by the same id, and it **never bleeds into the task body's attachments** — they are
    /// different targets. The ack puts the comment id in `tasks` so the attachments query gets
    /// invalidated (`applyAck` matches on `["attachments", type, id]`).
    #[test]
    fn comment_attachment_round_trips_via_command() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-cattach");
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "添付PJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                },
            )
            .unwrap();
            p.id
        };
        let task_id = task_add(Some(project_id), "添付親タスク".into(), None, None, None).unwrap().tasks[0];
        comment_add(task_id, "添付を付けるコメント".into()).unwrap();
        let comment_id = {
            let store = Store::open().unwrap();
            store.comment_list(task_id, None, None).unwrap().comments[0].id
        };

        let ack = attachment_add_bytes(
            "task_comment".into(),
            comment_id,
            "note.txt".into(),
            b"hello".to_vec(),
        )
        .unwrap();
        assert!(
            ack.tasks.contains(&comment_id),
            "comment attach acks the comment id for attachments invalidation"
        );

        let on_comment = attachments_for("task_comment".into(), comment_id).unwrap();
        assert_eq!(on_comment.len(), 1, "the comment carries its own attachment");
        assert_eq!(on_comment[0].filename.as_deref(), Some("note.txt"));
        let on_body = attachments_for("task".into(), task_id).unwrap();
        assert!(on_body.is_empty(), "the comment attachment does not bleed into the task body");

        attachment_remove(on_comment[0].id, "task_comment".into(), comment_id).unwrap();
        assert!(attachments_for("task_comment".into(), comment_id).unwrap().is_empty(), "removed comment attachment is gone");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `project_add_folder` turns the chosen folder into a new project: (1) on a machine with no store
    /// yet it brings one into being, (2) the project row, named after the folder, appears in the
    /// snapshot, (3) a `.amenbo` pointer is written into the folder, and (4) a folder that already has
    /// a `.amenbo` is refused with `init_pointer_exists`. It is the GUI's only way to raise a project
    /// (`AMB-D-532`), so the first of those is the genesis path for the whole app. The native folder
    /// picker cannot be driven from a Rust test, so the command itself (which takes a dir argument) is
    /// called directly to check the wiring: guard, creation, pointer.
    #[test]
    fn project_add_folder_inits_visible_project_and_guards() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-projfolder-home");
        let dir = amenbo_scratch::scratch("app-projfolder-dir");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        project_add_folder(dir.to_string_lossy().to_string(), None).unwrap();

        let engine = amenbo_core::config::Paths::resolve().unwrap().store_file;
        assert!(engine.is_file(), "the store is created on disk at {}", engine.display());
        assert!(dir.join(".amenbo").is_file(), ".amenbo pointer is written into the folder");

        let folder_name = dir.file_name().unwrap().to_string_lossy().to_string();
        let snap = snapshot().unwrap();
        assert!(
            snap.projects.iter().any(|p| p.name == folder_name),
            "the folder-init project (named after the folder) is visible"
        );

        match project_add_folder(dir.to_string_lossy().to_string(), None) {
            Ok(_) => panic!("re-init on a bound folder must be rejected"),
            Err(e) => assert_eq!(e.code, "init_pointer_exists", "re-init on a bound folder is rejected"),
        }

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A folder with no `.amenbo` but an Amenbo managed block — a stale marker left by a clone, a
    /// copy, or debris — is **not hard-blocked**. A marker is no proof of ownership, so when no
    /// living project in the registry claims it, init carries on: it brings a project into being and
    /// writes the pointer.
    #[test]
    fn project_add_folder_marker_only_continues_and_recovers_the_pointer() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-markeronly-home");
        let dir = amenbo_scratch::scratch("app-markeronly-dir");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        amenbo_core::agents::upsert_into_dir(&dir, None, amenbo_core::config::Paths::command_name());
        assert!(amenbo_core::agents::dir_has_managed_block(&dir), "precondition: a borrowed managed block is present");
        assert!(!dir.join(".amenbo").is_file(), "precondition: no owning pointer yet");

        project_add_folder(dir.to_string_lossy().to_string(), None).unwrap();
        assert!(dir.join(".amenbo").is_file(), "the pointer is written after continuing init");

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The input step of the creation screen is "name required, folder optional". Checks that when a
    /// folder is bound, the `name` the front end passes is what the project is named — **not the
    /// folder's name** — with surrounding whitespace trimmed.
    #[test]
    fn project_add_folder_uses_provided_name_over_basename() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-projfolder-name-home");
        let dir = amenbo_scratch::scratch("app-projfolder-name-dir");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        project_add_folder(dir.to_string_lossy().to_string(), Some("  マイPJ  ".to_string())).unwrap();

        let folder_name = dir.file_name().unwrap().to_string_lossy().to_string();
        let snap = snapshot().unwrap();
        assert!(
            snap.projects.iter().any(|p| p.name == "マイPJ"),
            "the project is named after the provided name (trimmed), not the folder"
        );
        assert!(
            !snap.projects.iter().any(|p| p.name == folder_name),
            "the folder basename is not used when a name is provided"
        );

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Folder management in project settings, round-tripped: bind an existing folder to an existing
    /// project (`project_bind_folder`), list it by reverse lookup (`project_bound_folders`), then
    /// unbind it (`project_unbind_folder`). The native folder picker cannot be driven from Rust, so
    /// the commands themselves (taking a dir argument) are called directly to check that (1) bind
    /// places `.amenbo` and the AI guidance managed block and one row appears in the reverse lookup,
    /// (2) a nested binding is refused, (3) a folder that does not exist is refused, and (4) unbind
    /// removes the pointer and the managed block and the row leaves the reverse lookup, while the
    /// store itself remains.
    #[test]
    fn project_bind_unbind_folder_round_trips() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-bindfolder-home");
        let dir = amenbo_scratch::scratch("app-bindfolder-dir");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "紐付けPJ".into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                },
            )
            .unwrap();
            p.id
        };

        match project_bind_folder(project_id, dir.join("does-not-exist").to_string_lossy().to_string()) {
            Ok(_) => panic!("binding a non-existent folder must be rejected"),
            Err(e) => assert_eq!(e.code, "not_found", "a missing folder is rejected"),
        }

        project_bind_folder(project_id, dir.to_string_lossy().to_string()).unwrap();
        assert!(dir.join(".amenbo").is_file(), ".amenbo pointer is written into the bound folder");
        assert!(amenbo_core::agents::dir_has_managed_block(&dir), "bind upserts the AI guidance managed block");
        let listed = project_bound_folders(project_id).unwrap();
        assert_eq!(listed.len(), 1, "the reverse lookup shows exactly the bound folder");
        assert!(listed[0].exists, "an existing bound folder is flagged AI-ready (exists)");

        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        match project_bind_folder(project_id, sub.to_string_lossy().to_string()) {
            Ok(_) => panic!("binding a subfolder of a managed tree must be rejected"),
            Err(e) => assert_eq!(e.code, "binding_nested_tree", "a nested binding is rejected"),
        }

        project_unbind_folder(dir.to_string_lossy().to_string()).unwrap();
        assert!(!dir.join(".amenbo").is_file(), "unbind removes the .amenbo pointer");
        assert!(!amenbo_core::agents::dir_has_managed_block(&dir), "unbind strips the managed block");
        assert!(project_bound_folders(project_id).unwrap().is_empty(), "the folder is gone from the reverse lookup");
        assert!(
            snapshot().unwrap().projects.iter().any(|p| p.id == project_id),
            "unbind keeps the project (store) — it only detaches the folder"
        );

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// When a bound folder loses its `.amenbo`, the list says so (`pointer_missing`). The registry
    /// still names this project, so the row stays, but an AI in that folder no longer resolves here.
    /// Round-trips all the way through a relink (`project_bind_folder`), which writes the pointer
    /// back and clears the flag.
    #[test]
    fn a_bound_folder_that_lost_its_pointer_says_so() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-nopointer-home");
        let dir = amenbo_scratch::scratch("app-nopointer-dir");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "ポインタ喪失PJ".into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        project_bind_folder(project_id, dir.to_string_lossy().to_string()).unwrap();
        let bound = project_bound_folders(project_id).unwrap();
        assert!(!bound[0].pointer_missing, "a freshly bound folder has its pointer");

        std::fs::remove_file(dir.join(".amenbo")).unwrap();
        let lost = project_bound_folders(project_id).unwrap();
        assert_eq!(lost.len(), 1, "the folder still shows up (the registry still points here)");
        assert!(lost[0].exists, "the folder itself is not stale — only the pointer is gone");
        assert!(lost[0].pointer_missing, "the missing pointer is reported instead of passing as AI-ready");
        assert!(lost[0].mismatch.is_none() && !lost[0].legacy, "with no pointer there is nothing to inspect");

        project_bind_folder(project_id, dir.to_string_lossy().to_string()).unwrap();
        let relinked = project_bound_folders(project_id).unwrap();
        assert!(dir.join(".amenbo").is_file(), "relink writes the pointer back");
        assert!(!relinked[0].pointer_missing, "the relinked folder is AI-ready again");

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every row of the list inspects its own `.amenbo` (`mismatch` and `legacy`). The verdicts
    /// themselves (core's `slug_mismatch` and `is_legacy_pointer`) are covered by core's own tests,
    /// so what is pinned here is **whether the command assembles the row correctly**: a current
    /// pointer written by bind says nothing, a pointer carried over from another store reports the
    /// disagreement along with the recorded slug and the real one, and an old-format pointer comes
    /// back as `legacy`. In none of these cases does the listing stop — the id is authoritative.
    #[test]
    fn bound_folder_rows_inspect_their_pointer() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-pointerscan-home");
        let dir = amenbo_scratch::scratch("app-pointerscan-dir");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        let (project_id, slug) = {
            let mut store = Store::open().unwrap();
            let id = store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "検分PJ".into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id;
            (id, store.project(id).unwrap().unwrap().slug)
        };
        assert!(slug.is_some(), "a project carries a slug — it is the material the pointer is checked against");
        project_bind_folder(project_id, dir.to_string_lossy().to_string()).unwrap();

        let row = project_bound_folders(project_id).unwrap().remove(0);
        assert!(row.mismatch.is_none(), "a pointer written by bind matches the store");
        assert!(!row.legacy, "a pointer written by bind is the current format");
        assert!(!row.pointer_missing, "the pointer is there");
        assert!(row.foreign.is_none(), "a pointer written by bind names the store that wrote it");

        amenbo_core::binding::DirBinding::new(Some(project_id), Some("wharfy".into())).write(&dir).unwrap();
        let row = project_bound_folders(project_id).unwrap().remove(0);
        let mismatch = row.mismatch.expect("a pointer from another store is reported");
        assert_eq!(mismatch.project_id, project_id);
        assert_eq!(mismatch.recorded, "wharfy", "the row carries the slug the pointer recorded");
        assert_eq!(mismatch.actual, slug, "the row carries the slug the id actually resolves to");
        assert!(!row.legacy, "a mismatched pointer is still the current format");
        assert!(row.exists, "the folder is listed as before — the mismatch does not hide it");

        // A pointer another channel's build wrote: the CLI refuses to run in that folder at all
        // (`pointer_other_store`), so the row has to say so rather than look healthy (`AMB-D-685`).
        amenbo_core::binding::DirBinding {
            v: amenbo_core::binding::POINTER_VERSION,
            store: Some("amenbo-dev".into()),
            project_id: Some(project_id),
            slug: slug.clone(),
        }
        .write(&dir)
        .unwrap();
        let row = project_bound_folders(project_id).unwrap().remove(0);
        let foreign = row.foreign.expect("a pointer naming another store is reported");
        assert_eq!(foreign.recorded, "amenbo-dev", "the row carries the store the pointer names");
        assert_eq!(
            foreign.running,
            amenbo_core::config::Paths::APP_NAME,
            "and the store of the build listing it — the sentence needs both names"
        );
        assert!(row.mismatch.is_none(), "the slug agrees; it is the store that does not");
        assert!(row.exists, "the folder is listed as before — being another store's does not hide it");

        std::fs::write(dir.join(".amenbo"), r#"{"v":1,"project_id":"01LEGACY","slug":"wharfy"}"#).unwrap();
        let row = project_bound_folders(project_id).unwrap().remove(0);
        assert!(row.legacy, "a pointer whose project_id cannot be read is reported as legacy");
        assert!(row.mismatch.is_none(), "with no readable id there is nothing to check the slug against");
        assert!(!row.pointer_missing, "the pointer is there — it is just old");
        assert!(row.foreign.is_none(), "a pointer that predates the store field is read, not refused");

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The GUI can see the folder rows nobody claims and clean them up itself, over the same core
    /// path as the CLI's `doctor` / `doctor --fix`. Detection is covered by core's own tests, so what
    /// is pinned here is **the command's wiring**: only the debris is raised, forgetting it leaves a
    /// living project's folder in the index, and neither the folder's contents nor its `.amenbo` are
    /// touched.
    #[test]
    fn the_gui_sees_and_forgets_folder_bindings_no_project_claims() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-orphanbind-home");
        let dir = amenbo_scratch::scratch("app-orphanbind-dir");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "残骸PJ".into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        project_bind_folder(project_id, dir.to_string_lossy().to_string()).unwrap();
        assert!(orphan_bindings().unwrap().is_empty(), "a folder claimed by a live project is not an orphan");

        let orphan = amenbo_scratch::scratch("app-orphanbind-left");
        {
            let store = Store::open().unwrap();
            let mut reg = store.bindings();
            reg.record_project_ref(project_id + 1_000, orphan.to_string_lossy());
            store.save_bindings(&reg).unwrap();
        }

        assert_eq!(
            orphan_bindings().unwrap(),
            vec![orphan.to_string_lossy().to_string()],
            "only rows with no claimant are surfaced to the GUI"
        );
        assert_eq!(forget_orphan_bindings().unwrap(), 1, "the cleanup drops it from the index");
        assert!(orphan_bindings().unwrap().is_empty(), "no orphans remain after the cleanup");
        assert!(orphan.is_dir(), "only the index row was dropped (the folder is untouched)");
        let bound = project_bound_folders(project_id).unwrap();
        assert_eq!(bound.len(), 1, "a live project's folder stays in the index");
        assert!(dir.join(".amenbo").is_file(), "that folder's pointer is intact too");

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&orphan);
    }

    /// Re-pointing a folder from the settings screen takes it off the folder list of the project it
    /// named before — the same claim the CLI's `bind` makes, over the same core path. Leave the old row
    /// standing and that project's screen goes on offering a folder that no longer leads to it.
    #[test]
    fn the_gui_re_pointing_a_folder_takes_it_off_the_former_projects_list() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-repoint-home");
        let dir = amenbo_scratch::scratch("app-repoint-dir");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        let project = |name: &str| -> i64 {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: name.into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let former = project("元PJ");
        let keeper = project("移り先PJ");

        project_bind_folder(former, dir.to_string_lossy().to_string()).unwrap();
        assert_eq!(project_bound_folders(former).unwrap().len(), 1, "premise: the folder is the former project's");

        project_bind_folder(keeper, dir.to_string_lossy().to_string()).unwrap();
        assert!(project_bound_folders(former).unwrap().is_empty(), "the former project stops listing it");
        let now = project_bound_folders(keeper).unwrap();
        assert_eq!(now.len(), 1, "and the keeper lists it");
        // The bind records the resolved path (`binding::canonical_dir` — symlinks, and the verbatim
        // spelling Windows answers in), which is what comes back here.
        let canon = amenbo_core::binding::canonical_dir(&dir).unwrap();
        assert_eq!(now[0].path, canon.to_string_lossy(), "that folder, and not another");
        assert!(!now[0].pointer_missing, "whose pointer is the one the bind just wrote");

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The GUI's doctor screen sees **the same issues** as the CLI and repairs them through **the
    /// same cleanup entry points**. What is pinned here is the command's wiring: detection (core's
    /// `doctor::report`) carries the environment's issues through to the GUI, and the repair
    /// (`doctor_fix`) clears them.
    #[test]
    fn the_gui_doctor_face_shows_the_same_issues_and_repairs_them() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-doctor-home");
        let orphan = amenbo_scratch::scratch("app-doctor-left");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&orphan);
        std::fs::create_dir_all(&orphan).unwrap();
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "整合PJ".into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        // A folder leads to that project, the way one does after `init`, and the question about starting
        // its AI on Amenbo is answered with a no. Both are issues of their own — a project no folder leads
        // to, and a folder whose AI does not start on Amenbo — so without them the "plain store" below
        // would start two warnings down.
        let home_dir = amenbo_scratch::scratch("app-doctor-dir");
        {
            let store = Store::open().unwrap();
            amenbo_core::binding::pointer_for(&store, project_id).write(&home_dir).unwrap();
            let mut reg = store.bindings();
            reg.record_project_ref(project_id, home_dir.to_string_lossy());
            store.save_bindings(&reg).unwrap();
            store
                .set_harness_consent(project_id, amenbo_core::harness::Consent::answered(false))
                .unwrap();
        }
        let clean = doctor_report().unwrap();
        assert!(clean.ok && clean.issues.is_empty(), "a plain store has no issues");

        {
            let store = Store::open().unwrap();
            let mut reg = store.bindings();
            reg.record_project_ref(project_id + 1_000, orphan.to_string_lossy());
            store.save_bindings(&reg).unwrap();
        }

        let dirty = doctor_report().unwrap();
        assert_eq!(dirty.issues.len(), 1, "an environment issue reaches the GUI surface");
        assert_eq!(dirty.issues[0].kind, "orphan_binding");
        assert_eq!(dirty.warnings, 1);
        assert_eq!(
            dirty.issues[0].params.get("dir").map(String::as_str),
            Some(orphan.to_string_lossy().as_ref()),
            "the GUI receives the details it needs (which folder) to compose a sentence in its own language",
        );

        let fixed = doctor_fix().unwrap();
        assert_eq!(fixed.forgotten_bindings, 1, "the GUI's repair drops it from the index");
        assert!(doctor_report().unwrap().issues.is_empty(), "the re-check after repair is clean");
        assert!(orphan.is_dir(), "only the index row was dropped (the folder is untouched)");

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&orphan);
        let _ = std::fs::remove_dir_all(&home_dir);
    }

    /// The "what next" affordances on the completion screen (reveal_folder / open_terminal) refuse a
    /// folder that does not exist, with an error that says so. The success path would really launch
    /// Finder or a terminal, so it is left untested; only the guard (the is_dir check) is checked.
    #[test]
    fn reveal_and_terminal_reject_missing_folder() {
        // One level below a scratch directory, so the name exists nowhere: `scratch` creates what it hands back.
        let missing = amenbo_scratch::scratch("app-missing").join("gone");
        let path = missing.to_string_lossy().to_string();
        assert!(reveal_folder(path.clone()).is_err(), "reveal_folder rejects a non-existent folder");
        assert!(open_terminal(path).is_err(), "open_terminal rejects a non-existent folder");
    }

    /// Round-trips the axis (dimension) assignment commands driven from the task detail view. The
    /// wiring — args, save, projection — is checked through per-task hydration and `task_dimensions`
    /// (the core ops themselves are already tested in core).
    #[test]
    fn axis_commands_round_trip() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-axis");
        std::env::set_var("AMENBO_HOME", &tmp);

        let (project_id, dim_id, v1, v2) = {
            let mut store = Store::open().unwrap();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "AxisPJ".into(), view: View::List, notes: String::new(), color: None,                },
            ).unwrap();
            let d = store.dimension_add(
                p.id,
                amenbo_core::ops::dimension::NewDimension { name: "軸".into(), ..Default::default() },
            ).unwrap();
            let v1 = store.dimension_value_add(d.id, "V1", None, None).unwrap();
            let v2 = store.dimension_value_add(d.id, "V2", None, None).unwrap();
            (p.id, d.id, v1.id, v2.id)
        };
        let card = |id: i64| tasks_by_ids(vec![id]).unwrap().into_iter().next();

        let id = task_add(Some(project_id), "軸テスト".into(), None, None, None).unwrap().tasks[0];
        assert_eq!(card(id).unwrap().project_id, Some(project_id));

        let ack = task_set_dimension_value(id, v1).unwrap();
        assert!(ack.scopes.contains(&"tasks"), "dimension change invalidates the board lists");
        assert_eq!(
            task_dimensions(id).unwrap().into_iter().map(|a| a.value_id).collect::<Vec<_>>(),
            vec![v1],
            "assigned V1"
        );
        let _ = task_set_dimension_value(id, v2).unwrap();
        assert_eq!(
            task_dimensions(id).unwrap().into_iter().map(|a| (a.dimension_id, a.value_id)).collect::<Vec<_>>(),
            vec![(dim_id, v2)],
            "single-select axis replaced V1 with V2"
        );
        let _ = task_unset_dimension_value(id, v2).unwrap();
        assert!(task_dimensions(id).unwrap().is_empty(), "cleared the axis assignment");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// A task can be added straight into an empty project by passing `project_id` to `task_add` —
    /// what the + in the "To do" column of the GUI's status board does. The task is placed there, so it
    /// shows up in a project-scoped `task_page`, which is the board's read path.
    #[test]
    fn task_add_into_empty_project_places_the_task() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-emptypj");
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "空PJ".into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                },
            )
            .unwrap();
            p.id
        };

        let ack = task_add(Some(project_id), "空PJタスク".into(), None, None, None).unwrap();
        assert!(ack.scopes.contains(&"tasks"), "task_add invalidates the board lists");
        let task_id = ack.tasks[0];

        let page = task_page(Some(project_id), Some(String::new()), None, None, None).unwrap();
        assert_eq!(page.total_matched, 1, "the task belongs to the project");
        assert!(page.tasks.iter().any(|t| t.id == task_id), "the new task shows on the project board");

        let card = tasks_by_ids(vec![task_id]).unwrap().into_iter().next().unwrap();
        assert_eq!(card.project_id, Some(project_id), "belongs to the project");
        assert_eq!(card.r#ref, "AMB-T-1", "the task is numbered");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Checks the wiring by which `task_page` returns "the total number of matches, plus just that
    /// window", over a SQLite projection and a paged read: the filter grammar is shared with
    /// task list, per-task hydration carries the id, the number and the status, limit/offset take
    /// effect, and the filter narrows. The semantics of the indexed read itself (WHERE/ORDER
    /// BY/LIMIT) are covered by the tests on core's read layer.
    #[test]
    fn task_page_pages_and_filters() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-taskpage");
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "PagePJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                },
            )
            .unwrap();
            let mut done_id = None;
            for i in 0..5 {
                let t = store.add_task(amenbo_core::ops::task::NewTask {
                    title: format!("T{i}"),
                    project_id: Some(p.id),
                    due_on: None,
                    start_on: None,
                    priority: None,
                    notes: String::new(),
                    created_by_kind: Some(ActorKind::Human),
                    at_binding_id: None,
                })
                .unwrap();
                if i == 4 {
                    done_id = Some(t.id);
                }
            }
            store.set_task_completed(done_id.unwrap(), true, ActorKind::Human).unwrap();
            p.id
        };

        let p1 = task_page(None, Some(String::new()), Some("created".into()), Some(2), Some(0)).unwrap();
        assert_eq!(p1.total_matched, 5, "total counts every match before paging");
        assert_eq!(p1.tasks.len(), 2, "page returns only the window");
        assert_eq!(p1.offset, 0);
        assert_eq!(p1.limit, Some(2));
        assert!(p1.tasks[0].r#ref.starts_with("AMB-T-"), "hydrated card carries its ref");

        let p2 = task_page(None, Some(String::new()), Some("created".into()), Some(2), Some(2)).unwrap();
        let p3 = task_page(None, Some(String::new()), Some("created".into()), Some(2), Some(4)).unwrap();
        assert_eq!(p2.tasks.len(), 2);
        assert_eq!(p3.tasks.len(), 1, "last page holds the remainder");
        let mut titles: Vec<String> = p1.tasks.iter().chain(&p2.tasks).chain(&p3.tasks).map(|t| t.title.clone()).collect();
        titles.sort();
        titles.dedup();
        assert_eq!(titles, vec!["T0", "T1", "T2", "T3", "T4"], "paging covers every task exactly once");

        let todo = task_page(None, Some("status:todo".into()), Some("created".into()), None, None).unwrap();
        assert_eq!(todo.total_matched, 4, "status:todo excludes the done task");
        assert_eq!(todo.tasks.len(), 4, "no limit returns all matches");
        assert!(todo.tasks.iter().all(|t| t.status == "todo"));

        let scoped = task_page(Some(project_id), Some(String::new()), None, None, None).unwrap();
        assert_eq!(scoped.total_matched, 5, "project scope matches the whole set");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `tasks_by_ids` hydrates the given ids in input order and drops silently any that do not
    /// exist. It is what the detail pane's single fetch and the inbox's union hydration (tasks with
    /// unread comments) rest on.
    #[test]
    fn tasks_by_ids_hydrates_in_input_order_and_drops_missing() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-byids");
        std::env::set_var("AMENBO_HOME", &tmp);

        let mut ids = Vec::new();
        {
            let mut store = Store::open().unwrap();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "ByIdsPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                },
            )
            .unwrap();
            for i in 0..3 {
                let t = store.add_task(amenbo_core::ops::task::NewTask {
                    title: format!("B{i}"),
                    project_id: Some(p.id),
                    due_on: None,
                    start_on: None,
                    priority: None,
                    notes: String::new(),
                    created_by_kind: Some(ActorKind::Human),
                    at_binding_id: None,
                })
                .unwrap();
                ids.push(t.id);
            }
        }

        let req = vec![ids[2], 999_999, ids[0]];
        let cards = tasks_by_ids(req).unwrap();
        assert_eq!(cards.len(), 2, "missing id is dropped");
        assert_eq!(cards[0].id, ids[2], "preserves input order");
        assert_eq!(cards[1].id, ids[0]);
        assert_eq!(cards[0].title, "B2");
        assert!(cards[0].r#ref.starts_with("AMB-T-"), "hydrated card carries its ref");

        assert!(tasks_by_ids(Vec::new()).unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// `task_activity` queries the persistent read-model directly
    /// (`store_engine::read::task_activity`) and returns comments and system events newest first.
    /// Checks the comment bodies, the wording of the system events, and that other tasks' rows are
    /// left out.
    #[test]
    fn task_activity_reads_newest_first_from_read_model() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-activity");
        std::env::set_var("AMENBO_HOME", &tmp);

        let (task_id, other_id) = {
            let mut store = Store::open().unwrap();
            let add_task = |store: &mut Store, title: &str| {
                store.add_task(amenbo_core::ops::task::NewTask {
                    title: title.into(),
                    project_id: None,
                    due_on: None,
                    start_on: None,
                    priority: None,
                    notes: String::new(),
                    created_by_kind: Some(ActorKind::Human),
                    at_binding_id: None,
                })
                .unwrap()
                .id
            };
            let task_id = add_task(&mut store, "Subject");
            let other_id = add_task(&mut store, "Other");

            store
                .add_system_event(
                    ActorKind::Human,
                    task_id,
                    amenbo_core::activity_log::event::task_status_changed("todo", "in_progress"),
                )
                .unwrap();
            store.add_task_comment(task_id, ActorKind::Human, "進めます").unwrap();
            store.add_task_comment(other_id, ActorKind::Human, "別件").unwrap();
            (task_id, other_id)
        };

        let items = task_activity(task_id, None).unwrap();
        assert_eq!(items.len(), 2, "only this task's stories, not the other task's");
        assert!(
            items.iter().all(|it| it.target.id == task_id),
            "every row targets the queried task"
        );
        assert!(items.iter().all(|it| it.target.title == "Subject"), "title resolved by join");

        let oracle_ids: Vec<i64> = {
            let store = Store::open().unwrap();
            store
                .activity(query::ActivityParams { task_id: Some(task_id), ..Default::default() })
                .unwrap()
                .items
                .into_iter()
                .map(|it| it.id)
                .collect()
        };
        assert_eq!(
            items.iter().map(|it| it.id).collect::<Vec<_>>(),
            oracle_ids,
            "the direct-SQL ordering matches core's own activity reader"
        );

        let comment = items.iter().find(|it| it.kind == "comment").expect("comment present");
        assert_eq!(comment.text.as_deref(), Some("進めます"));
        assert!(comment.event.is_none(), "comments carry no rendered event");
        let system = items.iter().find(|it| it.kind == "system").expect("system event present");
        assert!(system.event.is_some(), "system stories carry a rendered event");

        let one = task_activity(task_id, Some(1)).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].id, oracle_ids[0], "limit keeps the newest, matching the oracle");

        assert!(task_activity(999_999, None).unwrap().is_empty());
        let _ = other_id;

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Read state: read_receipts / mark_task_seen / mark_mailbox_seen round-trip through the
    /// commands, and persist across calls — that is, across rereading the file.
    #[test]
    fn read_receipts_round_trip() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-rr");
        std::env::set_var("AMENBO_HOME", &tmp);

        let rr = read_receipts().unwrap();
        assert!(rr.tasks.is_empty() && rr.mailbox_last_seen.is_none(), "empty to start");

        let after = mark_task_seen(12345).unwrap();
        assert!(after.tasks.contains_key(&12345), "mark_task is reflected");
        let reloaded = read_receipts().unwrap();
        assert!(reloaded.tasks.contains_key(&12345), "it persists across a separate call");

        let mb = mark_mailbox_seen().unwrap();
        assert!(mb.mailbox_last_seen.is_some(), "mark_mailbox is reflected");
        assert!(read_receipts().unwrap().mailbox_last_seen.is_some(), "it persists across a separate call");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The inbox's comment slot, independent of read state: a comment addressed to me on a task
    /// assigned to me shows up in `mailbox_comment_tasks`. Marking it seen (mark_task_seen)
    /// **does not remove it** — only the unread flag goes false (leaving the inbox on archive is
    /// reads.ts's job). A comment I made myself, as the human, does not show up, and neither does a
    /// task once the AI is the one carrying it.
    #[test]
    fn mailbox_comment_tasks_stays_after_read() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-m5");
        std::env::set_var("AMENBO_HOME", &tmp);

        let id = task_add(None, "受信箱D".into(), None, None, None).unwrap().tasks[0];
        task_assign(id, Some("human".into())).unwrap();
        assert!(mailbox_comment_tasks().unwrap().is_empty(), "no comments = no membership");

        {
            let mut store = Store::open().unwrap();
            store.add_task_comment(id, ActorKind::Ai, "AIからの確認").unwrap();
        }
        assert_eq!(
            mailbox_comment_tasks().unwrap(),
            vec![(id, true)],
            "an AI comment makes it present and unread (unread=true)"
        );

        mark_task_seen(id).unwrap();
        assert_eq!(
            mailbox_comment_tasks().unwrap(),
            vec![(id, false)],
            "presence/unread clears after mark_task_seen"
        );

        comment_add(id, "了解".into()).unwrap();
        assert_eq!(
            mailbox_comment_tasks().unwrap(),
            vec![(id, false)],
            "your own (human) remark does not affect presence/unread"
        );

        // Handing the task to the AI takes it out: the same comments are now the AI reporting on its
        // own work, and a report is pulled, not rung.
        task_assign(id, Some("ai".into())).unwrap();
        assert!(
            mailbox_comment_tasks().unwrap().is_empty(),
            "a task the AI is carrying is out, however many AI comments it holds"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// mailbox triggeredAt: returns the latest time of whatever put the item in the inbox (an
    /// assignment naming me, or a comment from someone other than me). Empty input gives empty
    /// output, ids with no such cause are left out, and a comment I made myself, as the human, is
    /// not a cause.
    #[test]
    fn mailbox_triggered_at_reports_latest_cause() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-trig");
        std::env::set_var("AMENBO_HOME", &tmp);

        assert!(mailbox_triggered_at(vec![]).unwrap().is_empty(), "empty input is empty");

        let id = task_add(None, "triggeredAt".into(), None, None, None).unwrap().tasks[0];

        let get = |ids: Vec<i64>| -> Option<String> {
            mailbox_triggered_at(ids).unwrap().into_iter().find(|(i, _)| *i == id).map(|(_, at)| at)
        };

        assert!(get(vec![id]).is_none(), "a task with no inbox trigger is omitted");
        assert!(mailbox_triggered_at(vec![999_999]).unwrap().is_empty(), "an unknown id is omitted");

        task_assign(id, Some("human".into())).unwrap();
        let after_assign = get(vec![id]).expect("an assignment yields a triggeredAt");

        {
            let mut store = Store::open().unwrap();
            store.add_task_comment(id, ActorKind::Ai, "確認お願いします").unwrap();
        }
        let after_comment = get(vec![id]).expect("a comment yields a triggeredAt");
        assert!(after_comment >= after_assign, "triggeredAt follows the latest inbox trigger (the later comment)");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The lightweight read-only open (`Store::open_read_at`) reads the same thing as a full open
    /// (`Store::open_at`). Checks that (1) startup health (doctor) agrees on the read and the write
    /// path, and (2) a read open never writes a single byte to the source-of-truth file.
    #[test]
    fn read_open_matches_full_open() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-readopen");
        std::env::set_var("AMENBO_HOME", &tmp);

        let _seeded = {
            let mut store = Store::open().unwrap();
            let me = ActorKind::Human.as_str().to_string();
            let p = store.project_add(
                amenbo_core::ops::project::NewProject {
                    name: "読みPJ".into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                },
            )
            .unwrap();
            let mk = |store: &mut Store, title: &str| {
                let id = store
                    .add_task(amenbo_core::ops::task::NewTask {
                        title: title.into(),
                        project_id: Some(p.id),
                        due_on: None,
                        start_on: None,
                        priority: Some(amenbo_core::model::Priority::High),
                        notes: "本文".into(),
                        created_by_kind: Some(ActorKind::Human),
                        at_binding_id: None,
                    })
                    .unwrap()
                    .id;
                store.finish_task_creation(id, ActorKind::Human).unwrap();
                id
            };
            let a = mk(&mut store, "親");
            let b = mk(&mut store, "ブロッカー");
            store.set_task_assignee(a, Some(ActorKind::Ai), ActorKind::Human).unwrap();
            store.set_task_status(a, TaskStatus::InProgress, ActorKind::Human).unwrap();
            store.depend_task(a, b, Some(ActorKind::Human)).unwrap();
            store.add_task_comment(a, ActorKind::Human, "確認").unwrap();
            let d = store
                .add_decision(amenbo_core::ops::decision::NewDecision {
                    title: "方針X".into(),
                    body: "理由".into(),
                    project_id: p.id,
                })
                .unwrap();
            store.link_decision(d.id, a).unwrap();
            store.accept_decision(d.id, Some(me.clone()), ActorKind::Human).unwrap();
            let d2 = store
                .add_decision(amenbo_core::ops::decision::NewDecision {
                    title: "方針Y".into(),
                    body: "改訂".into(),
                    project_id: p.id,
                })
                .unwrap();
            store.supersede_decision(d2.id, d.id, Some(me.clone()), ActorKind::Human).unwrap();
            let d3 = store
                .add_decision(amenbo_core::ops::decision::NewDecision {
                    title: "方針Y改".into(),
                    body: "一部改訂".into(),
                    project_id: p.id,
                })
                .unwrap();
            store.amend_decision(d3.id, d2.id).unwrap();
            (vec![a, b], vec![d.id, d2.id, d3.id])
        };

        let paths = amenbo_core::config::Paths::resolve().unwrap();
        drop(Store::open_at(paths.clone()).unwrap());

        let store_file = paths.store_file.clone();
        let mtime_before = std::fs::metadata(&store_file).and_then(|m| m.modified()).unwrap();

        let full = Store::open_at(paths.clone()).unwrap();
        let read = Store::open_read_at(paths.clone()).unwrap();

        let mtime_after = std::fs::metadata(&store_file).and_then(|m| m.modified()).unwrap();
        assert_eq!(mtime_before, mtime_after, "read/full open must not rewrite the truth-source file");

        let full_health =
            serde_json::to_value(full.startup_check.as_ref().expect("write open computes health")).unwrap();
        let read_health = serde_json::to_value(read.compute_startup_health().unwrap()).unwrap();
        assert_eq!(full_health, read_health, "startup health diverged between full (doctor) and read (doctor) open");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The GUI can read the feed forward by cursor: **only the changes after the store was read**
    /// come back, and what has been read never comes back again. Let that slip and either
    /// invalidations go missing and the screen freezes on stale data, or everything comes back every
    /// time and it degrades into refetching the world. Also pins that the rows a delete takes with it
    /// are caught: deleting a task deletes its comment rows too, and the feed learns of those from
    /// `update_hook` rather than from anything the ops layer says.
    #[test]
    fn changes_since_advances_with_the_cursor() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("feed");
        std::env::set_var("AMENBO_HOME", &tmp);

        let project_id = {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: "テストPJ".into(),
                    view: View::List,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };

        let start = change_cursor().unwrap();
        let empty = changes_since(start, None).unwrap();
        assert!(empty.rows.is_empty() && !empty.expired, "empty when unchanged (not expired)");
        assert_eq!(empty.cursor, start, "when empty, the cursor stays as passed");

        let task = task_add(Some(project_id), "実装".into(), None, None, None).unwrap().tasks[0];
        let after_add = changes_since(start, None).unwrap();
        assert!(
            after_add.rows.iter().any(|r| r.dataset == "task" && r.row_id == task && r.op == "insert"),
            "the added task's row is included: {:?}",
            after_add.rows
        );
        assert!(!after_add.more && !after_add.expired);
        assert!(after_add.cursor > start, "the cursor advances");

        let drained = changes_since(after_add.cursor, None).unwrap();
        assert!(drained.rows.is_empty(), "no new changes after the cursor: {:?}", drained.rows);

        comment_add(task, "ひとこと".into()).unwrap();
        let before_delete = changes_since(drained.cursor, None).unwrap().cursor;
        task_delete(task).unwrap();
        let after_delete = changes_since(before_delete, None).unwrap();
        assert!(
            after_delete.rows.iter().any(|r| r.dataset == "task" && r.op == "delete"),
            "the task deletion is included: {:?}",
            after_delete.rows
        );
        assert!(
            after_delete.rows.iter().any(|r| r.dataset == "task_comment" && r.op == "delete"),
            "comment rows deleted along with it are included too (a deletion only update_hook sees): {:?}",
            after_delete.rows
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Make `dir` a git repository, so `hooks::probe` has a hook directory to answer with.
    fn git_init(dir: &std::path::Path) {
        let out = std::process::Command::new("git").arg("init").arg("-q").current_dir(dir).output().unwrap();
        assert!(out.status.success(), "git init failed: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// Which folders the walk even looks at. Core decides what to do per folder, but the GUI has no cwd,
    /// so it is here that a folder gets skipped: one that is not a git repository has no hooks to have,
    /// and one whose `.amenbo` is gone names no project whose opt-out could be read. Neither raises the
    /// question, and the folder that does raises it **once for the device**, not once for itself.
    #[test]
    fn hook_offer_is_raised_only_by_a_bound_git_folder_and_only_once() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-hookoffers-home");
        let base = amenbo_scratch::scratch("app-hookoffers-dirs");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
        std::env::set_var("AMENBO_HOME", &tmp);

        let new_project = |name: &str| -> i64 {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: name.into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        // Canonicalized, because binding records the folder that way and the walk reads it back.
        let dir_of = |leaf: &str| -> std::path::PathBuf {
            let d = base.join(leaf);
            std::fs::create_dir_all(&d).unwrap();
            amenbo_core::binding::canonical_dir(&d).unwrap()
        };

        let plain = new_project("素のPJ");
        let lost = new_project("ポインタを失うPJ");

        // Not a git repository: bound and pointing at a project, but there are no hooks to have.
        let plain_dir = dir_of("plain");
        project_bind_folder(plain, plain_dir.to_string_lossy().to_string()).unwrap();

        // A git repository the registry still names, whose pointer has been removed by hand.
        let lost_dir = dir_of("lost");
        git_init(&lost_dir);
        project_bind_folder(lost, lost_dir.to_string_lossy().to_string()).unwrap();
        std::fs::remove_file(lost_dir.join(".amenbo")).unwrap();

        assert!(hook_offer().unwrap().is_none(), "a non-git folder and a folder with no pointer raise no question");

        // A git repository with no hooks and nothing answered: the one live question.
        let asked = new_project("問われるPJ");
        let asked_dir = dir_of("asked");
        git_init(&asked_dir);
        project_bind_folder(asked, asked_dir.to_string_lossy().to_string()).unwrap();

        let offer = hook_offer().unwrap().expect("an unwired git repository raises the question");
        assert_eq!(offer.cmd, amenbo_core::config::Paths::command_name(), "the wording gets the command name this build's channel installs");

        // A second unwired repository does not make a second question: the answer is the device's, so the
        // number of folders is not the number of clicks. That is the whole of the one-question design.
        let second = new_project("2つめのPJ");
        let second_dir = dir_of("second");
        git_init(&second_dir);
        project_bind_folder(second, second_dir.to_string_lossy().to_string()).unwrap();
        assert!(hook_offer().unwrap().is_some(), "still exactly one question, whatever the folder count");

        // Answering it once settles it for both, and wires both without a second question.
        hook_answer(true).unwrap();
        assert!(hook_offer().unwrap().is_none(), "answered once, never asked again");
        for dir in [&asked_dir, &second_dir] {
            assert!(
                amenbo_core::hooks::probe(dir).unwrap().all_managed(),
                "one yes reached {dir:?}, which was never asked about on its own",
            );
        }

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The nudge port over a live store, in the state a shipping build is in today: core declares no
    /// nudge, so nothing is due whatever stages this surface reports open, and the two writes behind it
    /// (the launch tally, and recording one as put) go through rather than erroring on a store the GUI
    /// opened this way. It is the passthrough that is under test — which nudge is due, and when, is
    /// pinned in core (`amenbo_core::nudge`).
    #[test]
    fn the_nudge_port_answers_over_a_store_with_none_declared() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-nudgeport-home");
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);
        Store::open().unwrap(); // The store this launch is tallied against.

        record_launch().unwrap();
        assert!(pending_nudges(Vec::new()).unwrap().is_empty(), "nothing is declared, so nothing is due");
        assert!(
            pending_nudges(vec!["some_stage_this_surface_is_in".into()]).unwrap().is_empty(),
            "a stage the caller is in raises no nudge on its own",
        );

        // Recording one is the caller's report that it drew it, and the log takes the same id twice
        // (a repeating nudge writes it on each showing).
        mark_nudge_put("test.put".into()).unwrap();
        mark_nudge_put("test.put".into()).unwrap();

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// What the walk carries out without asking, once the device has answered: a folder bound **after**
    /// the answer is wired at the next startup, and one that was opted out is left exactly as its
    /// `hooks uninstall` left it. The second is what makes the escape hatch an escape hatch — without it a
    /// yes on record would undo the uninstall on the next launch.
    #[test]
    fn a_yes_reaches_folders_bound_later_but_never_an_opted_out_one() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-hooksettle-home");
        let base = amenbo_scratch::scratch("app-hooksettle-dirs");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
        std::env::set_var("AMENBO_HOME", &tmp);
        std::fs::create_dir_all(&base).unwrap();

        let new_project = |name: &str| -> i64 {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: name.into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let git_dir = |leaf: &str| -> std::path::PathBuf {
            let d = base.join(leaf);
            std::fs::create_dir_all(&d).unwrap();
            let d = amenbo_core::binding::canonical_dir(&d).unwrap();
            git_init(&d);
            d
        };

        // The device says yes, with nothing bound yet: an answer given before the folders exist.
        {
            let mut store = Store::open().unwrap();
            store.config.hook_consent = Some(amenbo_core::hooks::HookConsent::Yes);
            store.save_config().unwrap();
        }

        let later = new_project("あとで bind する PJ");
        let later_dir = git_dir("later");
        project_bind_folder(later, later_dir.to_string_lossy().to_string()).unwrap();

        let refused = new_project("ここだけ要らない PJ");
        let refused_dir = git_dir("refused");
        project_bind_folder(refused, refused_dir.to_string_lossy().to_string()).unwrap();
        Store::open().unwrap().set_hook_optout(refused, true).unwrap();

        assert!(hook_offer().unwrap().is_none(), "the device has answered, so there is nothing to ask");
        assert!(
            amenbo_core::hooks::probe(&later_dir).unwrap().all_managed(),
            "a folder bound after the yes is wired by it, with no second question",
        );
        assert!(
            !amenbo_core::hooks::probe(&refused_dir).unwrap().any_managed(),
            "`hooks uninstall` said not this one, and a device-wide yes does not overrule it",
        );

        // The slot an upgrade added, in a repository an older build wired: filled under the same answer.
        let hooks = amenbo_core::hooks::hooks_dir(&later_dir).unwrap();
        std::fs::remove_file(hooks.join("commit-msg")).unwrap();
        std::fs::write(hooks.join("pre-commit"), "#!/bin/sh\n# amenbo:hook (managed v1)\nexit 0\n").unwrap();
        assert!(hook_offer().unwrap().is_none(), "completing a consented install is not a question");
        let states = amenbo_core::hooks::probe(&later_dir).unwrap();
        assert!(states.all_managed(), "the missing slot was wired under the answer already given: {states:?}");

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A folder with Claude Code's settings directory in it, so the probe traces the provider. `wired`
    /// writes the two tokens `harness::probe` reads for — this build's `<cmd> agent` call and the
    /// provider's session-start event — rather than the real configuration, which is core's to compose.
    fn claude_folder(dir: &std::path::Path, wired: bool) {
        let cmd = amenbo_core::config::Paths::command_name();
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        let text = if wired {
            format!("{{ \"SessionStart\": \"{cmd} agent\" }}")
        } else {
            "{ }".to_string()
        };
        std::fs::write(dir.join(".claude/settings.json"), text).unwrap();
    }

    /// What the GUI writes on the session-start record (`AMB-D-440`, `AMB-D-460`). The question itself is
    /// the CLI's; what reaches here from the standing row is the refusal that ends the row.
    ///
    /// Two things are this door's own, and neither is core's `reconcile` (tested there, row by row): the
    /// answer belongs to a **project** and so is written against one, and whether it is the first answer
    /// or the one re-ask is read off the record rather than passed in — which is what keeps the re-ask
    /// to one.
    #[test]
    fn an_answer_is_written_against_its_project_and_the_re_ask_is_spent_once() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-agenthook-home");
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("AMENBO_HOME", &tmp);

        let mut store = Store::open().unwrap();
        let project = store
            .project_add(amenbo_core::ops::project::NewProject {
                name: "答えを持つPJ".into(),
                view: View::Board,
                notes: String::new(),
                color: None,
            })
            .unwrap()
            .id;
        drop(store);

        agent_hook_answer(project, true).unwrap();
        let first = Store::open()
            .unwrap()
            .harness_consent(project)
            .unwrap()
            .expect("the answer is recorded against the project it was given about");
        assert!(first.allowed, "a yes, as the CLI's question would have taken it");
        assert!(!first.asked_again, "the first asking, with the one re-ask still unspent");

        // The row's refusal, landing on a standing yes: the reader changed their mind, which is the whole
        // reason the no is on the row rather than behind a question asked once.
        agent_hook_answer(project, false).unwrap();
        let again = Store::open().unwrap().harness_consent(project).unwrap().unwrap();
        assert!(!again.allowed, "the later answer is the one on the record");
        assert!(again.asked_again, "and it spends the re-ask, so nothing is put a third time");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// What the standing row is left with, folder by folder.
    ///
    /// A folder somebody wired by hand has nothing left, so it draws nothing. The report that does follow
    /// names **only what the folder points at**, with the request attached: a warning about a tool there
    /// is no sign of is one a person cannot act on, and a copy button with nothing behind it would leave
    /// a setup that reads as finished and is not.
    #[test]
    fn a_wired_folder_has_nothing_left_and_the_report_carries_the_text_that_asks_for_the_wiring() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-agenthooknotice-home");
        let base = amenbo_scratch::scratch("app-agenthooknotice-dirs");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
        std::env::set_var("AMENBO_HOME", &tmp);

        let new_project = |name: &str| -> i64 {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: name.into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let bound = |project: i64, leaf: &str, wired: bool| -> std::path::PathBuf {
            let d = base.join(leaf);
            std::fs::create_dir_all(&d).unwrap();
            let d = amenbo_core::binding::canonical_dir(&d).unwrap();
            claude_folder(&d, wired);
            project_bind_folder(project, d.to_string_lossy().to_string()).unwrap();
            d
        };

        let wired = new_project("自分で配線したPJ");
        let wired_dir = bound(wired, "wired", true);
        // Traced by no provider at all: nothing here a person could be pointed at.
        let bare = new_project("痕跡の無いPJ");
        let bare_dir = base.join("bare");
        std::fs::create_dir_all(&bare_dir).unwrap();
        let bare_dir = amenbo_core::binding::canonical_dir(&bare_dir).unwrap();
        project_bind_folder(bare, bare_dir.to_string_lossy().to_string()).unwrap();

        // One folder is wired and has nothing left to finish; the other points at no tool, and is the
        // case this surface exists for — it is offered the catalog to pick from rather than nothing,
        // since a reader who has just said yes there would otherwise be handed no text at all.
        assert!(
            agent_hook_project_wiring(wired).unwrap().is_empty(),
            "the wired folder is done, so its project's row has nothing to stand for",
        );
        let waiting = agent_hook_project_wiring(bare).unwrap();
        assert_eq!(
            waiting.len(),
            amenbo_core::harness::HARNESSES.len(),
            "a folder pointing at nothing waits on every tool, not none",
        );
        assert_eq!(
            waiting[0].dirs,
            [bare_dir.to_string_lossy()],
            "the one folder that still has to be handed something",
        );

        // Now a folder that says which tool it uses, and does not run it at session start.
        let traced = new_project("痕跡はあるが未配線のPJ");
        let traced_dir = bound(traced, "traced", false);
        let waiting = agent_hook_project_wiring(traced).unwrap();
        assert_eq!(
            waiting.iter().map(|one| one.tool.tool.as_str()).collect::<Vec<_>>(),
            ["claude-code"],
            "a folder that points somewhere waits on that, not the whole catalog",
        );
        assert_eq!(
            waiting[0].dirs,
            [traced_dir.to_string_lossy()],
            "and the folder listed is the one that points there",
        );
        let tool = &waiting[0].tool;
        assert_eq!(tool.tool, "claude-code");
        assert_eq!(tool.paste_into, ".claude/settings.json", "where the text goes");
        assert!(
            tool.request.contains(amenbo_core::config::Paths::command_name()),
            "the text carries the command this build answers to: {}",
            tool.request,
        );
        // And it is a request rather than the settings on their own, which is what lets the AI it is
        // given to merge into a file that already holds something.
        assert!(
            tool.request.contains(&tool.paste_into) && tool.request.contains("Merge"),
            "the text does not ask for a merge into a named file: {}",
            tool.request,
        );

        // A refusal ends the report — for the project that gave it, and no other. Nothing is forbidden by
        // it, the text stays there for the asking, but a reader with no setup pending is not warned.
        agent_hook_answer(traced, false).unwrap();
        assert!(
            agent_hook_project_wiring(traced).unwrap().is_empty(),
            "a no is silence, not a standing warning",
        );

        let _ = std::fs::remove_dir_all(&wired_dir);
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The request is there for the asking whatever the report says (`AMB-D-670`).
    ///
    /// Everything else about the session-start hook is drawn from `harness::setup_notice`, which goes
    /// quiet on the wiring landing and on a refusal — and that silence took the GUI's only way to the text
    /// with it, leaving the reader who wired one tool and then moved to another with nothing to press.
    /// So this reads neither, and the two folders below are the two states that silence it.
    #[test]
    fn the_request_face_hands_over_the_whole_catalog_whatever_the_report_has_gone_quiet_about() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-agenthookreq-home");
        let base = amenbo_scratch::scratch("app-agenthookreq-dirs");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
        std::env::set_var("AMENBO_HOME", &tmp);

        let mut store = Store::open().unwrap();
        let project = store
            .project_add(amenbo_core::ops::project::NewProject {
                name: "配線済みのPJ".into(),
                view: View::Board,
                notes: String::new(),
                color: None,
            })
            .unwrap()
            .id;
        drop(store);
        let dir = base.join("wired");
        std::fs::create_dir_all(&dir).unwrap();
        let dir = amenbo_core::binding::canonical_dir(&dir).unwrap();
        claude_folder(&dir, true);
        project_bind_folder(project, dir.to_string_lossy().to_string()).unwrap();

        assert!(
            agent_hook_project_wiring(project).unwrap().is_empty(),
            "the report has nothing to say here — which is the state this face is for",
        );

        let taken = agent_hook_requests(project).unwrap();
        assert_eq!(
            taken.tools.iter().map(|one| one.tool.as_str()).collect::<Vec<_>>(),
            amenbo_core::harness::HARNESSES.iter().map(|one| one.id).collect::<Vec<_>>(),
            "the whole catalog in its own order — the tool being moved to has left no trace yet",
        );
        assert_eq!(
            taken.dirs,
            [dir.to_string_lossy().to_string()],
            "and this project's folders, which is where any of them would be pasted",
        );
        let codex = taken.tools.iter().find(|one| one.tool == "codex-cli").expect("in the catalog");
        assert!(
            codex.request.contains(&codex.paste_into) && codex.request.contains("Merge"),
            "each row carries its own request, not the one the folder traces: {}",
            codex.request,
        );

        // A refusal is the other way the report falls silent, and it is not a way out of this one: it
        // ends a warning nobody asked for, while this is a face somebody pressed.
        agent_hook_answer(project, false).unwrap();
        let after = agent_hook_requests(project).unwrap();
        assert_eq!(after.tools.len(), taken.tools.len(), "a no does not take the text away");
        assert_eq!(after.dirs, taken.dirs, "nor the folders it is pasted into");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The project screen's standing row (`AMB-D-459`): it answers for **one project**, carries the text
    /// **once** with its folders listed under it, and empties as the wiring lands — which is the only way
    /// a row with no close button ever goes.
    ///
    /// The gap it closes is that consent is per project while wiring is per folder: a reader who pasted
    /// into one of several folders is, on the record, answered — so nothing but this counts the rest.
    #[test]
    fn the_project_row_carries_one_text_for_its_own_folders_and_empties_as_they_are_wired() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-agenthookrow-home");
        let base = amenbo_scratch::scratch("app-agenthookrow-dirs");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
        std::env::set_var("AMENBO_HOME", &tmp);

        let new_project = |name: &str| -> i64 {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: name.into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let bound = |project: i64, leaf: &str, wired: bool| -> std::path::PathBuf {
            let d = base.join(leaf);
            std::fs::create_dir_all(&d).unwrap();
            let d = amenbo_core::binding::canonical_dir(&d).unwrap();
            claude_folder(&d, wired);
            project_bind_folder(project, d.to_string_lossy().to_string()).unwrap();
            d
        };

        let mine = new_project("4フォルダのPJ");
        let one = bound(mine, "row-one", false);
        let two = bound(mine, "row-two", false);
        // Another project's folder, equally unwired: it belongs to that project's screen, not this one.
        let theirs = new_project("隣のPJ");
        let elsewhere = bound(theirs, "row-elsewhere", false);

        let waiting = agent_hook_project_wiring(mine).unwrap();
        assert_eq!(
            waiting.iter().map(|w| w.tool.tool.as_str()).collect::<Vec<_>>(),
            ["claude-code"],
            "the folders point at one tool, so there is one text to hand over",
        );
        assert_eq!(
            waiting[0].dirs,
            [one.to_string_lossy(), two.to_string_lossy()],
            "both of this project's folders wait on that one text — and only this project's",
        );
        assert!(
            !waiting[0].dirs.contains(&elsewhere.to_string_lossy().to_string()),
            "another project's folder is answered on its own screen",
        );
        assert!(waiting[0].tool.request.contains(".claude/settings.json"), "the text says where it goes");

        // The reader pastes into one of the two. The record already said yes — this is the state where the
        // question is spent and the remaining folder would otherwise go unmentioned.
        claude_folder(&one, true);
        agent_hook_answer(mine, true).unwrap();
        let waiting = agent_hook_project_wiring(mine).unwrap();
        assert_eq!(
            waiting[0].dirs,
            [two.to_string_lossy()],
            "a standing yes does not end it: what is left is the folder still unwired",
        );

        // The last one lands, and the row has nothing to say — it goes by itself, having no other ending.
        claude_folder(&two, true);
        assert!(
            agent_hook_project_wiring(mine).unwrap().is_empty(),
            "wiring the last folder is what ends the row",
        );

        // A folder that is no longer there cannot be pasted into, so it is not reported: a row naming it
        // would be one the reader has no way to end.
        let gone = bound(mine, "row-gone", false);
        assert!(!agent_hook_project_wiring(mine).unwrap().is_empty(), "while it is there, it is reported");
        std::fs::remove_dir_all(&gone).unwrap();
        assert!(
            agent_hook_project_wiring(mine).unwrap().is_empty(),
            "a folder that is gone is not work left",
        );

        // A refusal is silence here too, as it is for the banner — core decides that, and this reads it.
        let refused = new_project("断ったPJ");
        bound(refused, "row-refused", false);
        agent_hook_answer(refused, false).unwrap();
        assert!(
            agent_hook_project_wiring(refused).unwrap().is_empty(),
            "a no is silence, not a standing row",
        );

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A folder an app reaches over MCP, showing no sign that an AI is worked with in it, is not counted
    /// as waiting on the session-start hook (`AMB-D-680`). What that hook wires is a shell command, and
    /// nothing opens a shell there — the same duty is already done by the server's own `agent` tool.
    ///
    /// The three folders below are the three ways out of it, and each one is a way the report stands:
    /// the folder says which provider it uses, it leaves instructions for whichever AI opens it, or the
    /// entry is not about this folder at all.
    #[test]
    fn a_folder_reached_only_over_mcp_is_not_waiting_on_the_hook() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-agenthookmcp-home");
        let base = amenbo_scratch::scratch("app-agenthookmcp-dirs");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
        std::env::set_var("AMENBO_HOME", &tmp);

        let new_project = |name: &str| -> i64 {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: name.into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        // Bound the way the GUI binds one — which writes Amenbo's own managed block into this folder's
        // `CLAUDE.md` and `AGENTS.md`. That is the state every folder here starts in, and it is not a
        // sign of anyone: what follows is about what the reader adds to it.
        let bound = |project: i64, leaf: &str| -> std::path::PathBuf {
            let d = base.join(leaf);
            std::fs::create_dir_all(&d).unwrap();
            let d = amenbo_core::binding::canonical_dir(&d).unwrap();
            project_bind_folder(project, d.to_string_lossy().to_string()).unwrap();
            d
        };
        // An entry as an app that keeps its settings inside the folder writes one. `.mcp.json` is the
        // catalog's one place that is neither a harness's own directory nor inside one, so what silences
        // the report here is this rule rather than a trace the entry brought with it.
        let mcp_entry = |dir: &std::path::Path, bound_to: &[&std::path::Path]| {
            let mut args = vec!["mcp".to_string(), "--dir".to_string()];
            args.extend(bound_to.iter().map(|at| at.to_string_lossy().to_string()));
            let mut servers = serde_json::Map::new();
            servers.insert(
                amenbo_core::mcp::name().to_string(),
                serde_json::json!({ "command": "amenbo", "args": args }),
            );
            let document = serde_json::json!({ "mcpServers": servers });
            std::fs::write(dir.join(".mcp.json"), document.to_string()).unwrap();
        };

        let only = new_project("MCPからだけ届くPJ");
        let only_dir = bound(only, "mcp-only");
        // Named second in an entry that carries two folders — one server reaches a set of them, and a
        // folder is no less reached for not being the one the reader started from.
        mcp_entry(&only_dir, &[&base.join("somebody-elses-folder"), &only_dir]);
        assert!(
            agent_hook_project_wiring(only).unwrap().is_empty(),
            "a hook nothing would ever run is not work left",
        );

        // The same folder, once the reader has written instructions of their own beside Amenbo's block.
        // That is an AI being worked with here, so the report is back — and it is the whole catalog, the
        // folder still pointing at no one provider.
        let claude = only_dir.join("CLAUDE.md");
        let managed = std::fs::read_to_string(&claude).unwrap();
        std::fs::write(&claude, format!("# how to work here\n\n{managed}")).unwrap();
        assert_eq!(
            agent_hook_project_wiring(only).unwrap().len(),
            amenbo_core::harness::HARNESSES.len(),
            "a folder that instructs an AI is worked in, whatever else reaches it",
        );

        // A folder that says which provider it uses is worked in too, and waits on that one.
        let traced = new_project("MCPもシェルも使うPJ");
        let traced_dir = bound(traced, "mcp-traced");
        mcp_entry(&traced_dir, &[&traced_dir]);
        claude_folder(&traced_dir, false);
        assert_eq!(
            agent_hook_project_wiring(traced)
                .unwrap()
                .iter()
                .map(|one| one.tool.tool.as_str())
                .collect::<Vec<_>>(),
            ["claude-code"],
            "a traced folder waits on its provider, MCP or no MCP",
        );

        // And an entry that names some other folder is not this folder's. Most of the catalog keeps one
        // settings file for the whole machine, so reading "an entry exists" as "this folder is reached"
        // would silence every traceless folder on the device.
        let elsewhere = new_project("隣のフォルダのエントリしか無いPJ");
        let elsewhere_dir = bound(elsewhere, "mcp-elsewhere");
        mcp_entry(&elsewhere_dir, &[&base.join("somebody-elses-folder")]);
        assert!(
            !agent_hook_project_wiring(elsewhere).unwrap().is_empty(),
            "an entry about another folder says nothing about this one",
        );

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The removal a row hands over is addressed to the file the entry is actually in, and does not
    /// move with what is ticked. Unticking one project and ticking another is the very move that asks
    /// for the entry already there to go: a request built from the new ticks would send the reader's AI
    /// to a file that entry was never written into, and they would be told there was nothing to delete.
    ///
    /// The add is read beside it, because the two are addressed to different files on purpose — what is
    /// ticked is where the entry is to be written next, and that half must keep following the ticks.
    #[test]
    fn a_removal_names_the_file_the_entry_is_in_whatever_is_ticked() {
        let _env = env_guard();
        let tmp = amenbo_scratch::scratch("app-mcpremove-home");
        let base = amenbo_scratch::scratch("app-mcpremove-dirs");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
        std::env::set_var("AMENBO_HOME", &tmp);

        let new_project = |name: &str| -> i64 {
            let mut store = Store::open().unwrap();
            store
                .project_add(amenbo_core::ops::project::NewProject {
                    name: name.into(),
                    view: View::Board,
                    notes: String::new(),
                    color: None,
                })
                .unwrap()
                .id
        };
        let bound = |project: i64, leaf: &str| -> std::path::PathBuf {
            let d = base.join(leaf);
            std::fs::create_dir_all(&d).unwrap();
            let d = amenbo_core::binding::canonical_dir(&d).unwrap();
            project_bind_folder(project, d.to_string_lossy().to_string()).unwrap();
            d
        };

        let greenhouse = new_project("温室");
        let greenhouse_dir = bound(greenhouse, "greenhouse");
        let shop = new_project("店");
        let shop_dir = bound(shop, "shop");

        // The entry, in the folder of the project that is about to be unticked — written the way an
        // app that keeps its settings inside a folder holds one, and naming a folder that is not the
        // one it sits in, so the file it is in cannot be mistaken for the folder it reaches.
        let mut servers = serde_json::Map::new();
        servers.insert(
            amenbo_core::mcp::name().to_string(),
            serde_json::json!({"command": "amenbo", "args": ["mcp", "--dir", "/work/elsewhere"]}),
        );
        let entry = serde_json::json!({ "mcpServers": servers });
        std::fs::write(greenhouse_dir.join(".mcp.json"), entry.to_string()).unwrap();

        let said = mcp_request_for("claude-code".to_string(), vec![shop]).unwrap();
        let holds = greenhouse_dir.join(".mcp.json").display().to_string();
        let ticked = shop_dir.join(".mcp.json").display().to_string();
        assert!(said.add.contains(&ticked), "the add is written where the ticks say: {}", said.add);
        assert!(said.remove.contains(&holds), "the removal names the file it is in: {}", said.remove);
        assert!(!said.remove.contains(&ticked), "and not the ticked one: {}", said.remove);

        // Nothing ticked at all is the plainest way to ask for it to go, and it is addressed the same.
        let none = mcp_request_for("claude-code".to_string(), Vec::new()).unwrap();
        assert!(none.remove.contains(&holds), "{}", none.remove);

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A probe as the door would have built it, without the fetch: what the network does is core's
    /// business, and the rule under test is what the door does with the answer.
    fn probe_of(
        fingerprint: Option<&str>,
        pinned: Option<&str>,
    ) -> amenbo_core::plugin_catalog::SourceProbe {
        amenbo_core::plugin_catalog::SourceProbe {
            url: "https://example.invalid/catalog.json".to_string(),
            suggested_name: "example.invalid".to_string(),
            // A key is present exactly when there is a fingerprint to show for it; its bytes are core's
            // to verify and are never read here.
            key: fingerprint.map(|f| format!("a key whose fingerprint is {f}")),
            fingerprint: fingerprint.map(str::to_string),
            registered: pinned.is_some(),
            pinned: pinned.map(str::to_string),
        }
    }

    /// The consent rule, as a truth table (`AMB-D-389`). The GUI shows a fingerprint in one call and
    /// registers in the next, so the pin that gets written has to be the one that was agreed to —
    /// silence and a stale agreement are both refusals, and neither is a pin.
    #[test]
    fn a_catalog_is_pinned_only_on_the_fingerprint_that_was_agreed_to() {
        let fp = "6272CBB782CB57A0";
        assert!(agreed_pin(&probe_of(Some(fp), None), Some(fp)).is_ok(), "agreed to what is served");
        assert!(agreed_pin(&probe_of(None, None), None).is_ok(), "no key: nothing to agree to");
        assert!(
            agreed_pin(&probe_of(Some(fp), Some(fp)), None).is_ok(),
            "already pinned: re-registering only renames it",
        );

        let silent = agreed_pin(&probe_of(Some(fp), None), None).unwrap_err();
        assert_eq!(silent.code, "plugin_catalog_consent_required", "silence does not pin a key");
        let stale = agreed_pin(&probe_of(Some(fp), None), Some("0000000000000000")).unwrap_err();
        assert_eq!(stale.code, "plugin_catalog_key_changed", "a key that moved under the screen");
        assert!(stale.message_en.contains(fp), "the refusal names what is served now: {}", stale.message_en);
    }
}
