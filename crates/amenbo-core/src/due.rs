//! **The due warning** — the purpose the hourly tick carries out, once a calendar day (`AMB-D-706`).
//!
//! What it does is put one event on the outbox per task whose day has come or is coming, and stop. What
//! becomes of them is a plugin's: the carriage outward, the wording, and the grouping into one message
//! per project are all the subscriber's work (`AMB-D-467` / `AMB-D-468`), which is why an event is one
//! task and never a digest.
//!
//! **Two steps, and they are the screen's own** (`app/src/core/due.ts`): a day that has come or gone, and
//! a day that is tomorrow. Keeping a third cut here would put a warning on a channel at a distance the
//! chip on the task never draws, and a reader with both in front of them could not say which was right.
//! They are read through the same filter grammar the screen queries with, so the two cannot drift apart
//! by being written twice.
//!
//! **Nobody acted, so the events carry no actor.** A day arriving is not a write, and the tick that
//! noticed it is not an author ([`crate::plugin_payload::name::TASK_DUE`]).
//!
//! **A task nobody closes is named again tomorrow.** The turn is per calendar day, not per task
//! (`AMB-D-708`): what the day mark holds is that this device has warned today, and a day that is still
//! past tomorrow is warned about again. That is the intent — an overdue task that went quiet after one
//! message would be a task nobody is reminded of.

use crate::error::Result;
use crate::plugin_payload::name;
use crate::query::ListParams;
use crate::store::Store;
use crate::store_engine::outbox::EventRow;
use crate::time::Timestamp;

/// The id the tick keeps this purpose's day mark under ([`crate::overview::tick_day`]).
pub const PURPOSE: &str = "due";

/// The windows the warning is built from, most urgent first, each with the event a task in it fires.
///
/// `done:false` is closed-or-not (`AMB-D-397`), so work that was finished and work that was decided
/// against both drop out. The three are disjoint — one `due_on`, three different days — so no task is
/// named twice, and the two that share an event share it because they share a step.
const WINDOWS: [(&str, &str); 3] = [
    ("done:false due:overdue", name::TASK_DUE),
    ("done:false due:today", name::TASK_DUE),
    ("done:false due:tomorrow", name::TASK_DUE_TOMORROW),
];

/// Put the day's warnings on the outbox — the purpose's whole face, as the tick calls it.
///
/// One transaction for all of them: the events of one day's warning are one act, and a half-written run
/// would leave a reader warned about some of their tasks with nothing saying the rest were missed. A run
/// with nothing to warn about writes nothing at all, and is not a failure — most days on most stores are
/// that.
pub fn emit(store: &Store) -> Result<()> {
    let mut rows = Vec::new();
    for (filter, event) in WINDOWS {
        for task in due_in(store, filter)? {
            rows.push((event, task));
        }
    }
    if rows.is_empty() {
        return Ok(());
    }

    let at = Timestamp::now().to_rfc3339_z();
    let tx = store.engine.write()?;
    for (event, (id, project)) in &rows {
        tx.emit_event(&EventRow {
            event,
            record_id: *id,
            // The actorless column, written as the change signal writes it: the mapping back reads the
            // absence rather than parsing this (`crate::plugin_payload`).
            actor: "",
            at: &at,
            new_state: None,
            project: *project,
            record: None,
            parent: None,
        })?;
    }
    Ok(tx.commit()?)
}

/// The open tasks one window holds, as `(id, project)` — the two fields an event needs, and no more. The
/// project is what a project-scoped subscription is routed on (`AMB-D-405`); a task in no project has
/// none, which is a real answer rather than a missing one.
fn due_in(store: &Store, filter: &str) -> Result<Vec<(i64, Option<i64>)>> {
    let params = ListParams {
        project_id: None,
        filter_expr: Some(filter.to_string()),
        text: None,
        sort: "due".to_string(),
        limit: None,
        offset: None,
    };
    Ok(store
        .list_tasks(params)?
        .tasks
        .into_iter()
        .map(|t| (t.id, t.project.map(|p| p.id)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Paths;
    use crate::model::{ActorKind, TaskStatus, View};
    use crate::ops::project::NewProject;
    use crate::ops::task::NewTask;
    use crate::store_engine::outbox::{events_since, OutboxSlice};
    use chrono::Duration;

    /// A store with one project, and a way to file a task due on a given day.
    fn store_with_project() -> (Store, i64) {
        let base = amenbo_scratch::scratch("due-emit");
        let mut store = Store::open_at(Paths::at(base)).unwrap();
        let project = store
            .project_add(NewProject {
                name: "期日".to_string(),
                view: View::Board,
                notes: String::new(),
                color: None,
            })
            .unwrap();
        (store, project.id)
    }

    fn file(store: &mut Store, project_id: i64, title: &str, due: Option<chrono::NaiveDate>) -> i64 {
        let task = store
            .add_task(NewTask {
                title: title.to_string(),
                project_id: Some(project_id),
                due_on: due,
                start_on: None,
                priority: None,
                notes: String::new(),
                created_by_kind: Some(ActorKind::Human),
                at_binding_id: None,
            })
            .unwrap();
        task.id
    }

    /// What a run leaves on the outbox, as `(event, record_id)` in the order it was written.
    fn warned(store: &Store) -> Vec<(String, i64)> {
        written(store).into_iter().map(|r| (r.event, r.record_id)).collect()
    }

    /// The warnings on the outbox, in the order they were written. Everything else the store put there on
    /// the way — a creation, the ledger's own signal — is left out: what this file is about is the rows
    /// this purpose wrote.
    fn written(store: &Store) -> Vec<crate::store_engine::OutboxRow> {
        match events_since(store.read_model().conn(), 0, 10_000).unwrap() {
            OutboxSlice::Events { rows, .. } => {
                rows.into_iter().filter(|r| r.event.starts_with("task.due")).collect()
            }
            OutboxSlice::Gap => panic!("nothing was trimmed, so there is no gap"),
        }
    }

    /// The two steps, over the three days they are drawn from: a day gone and a day here take the same
    /// event, tomorrow takes the other, and a day further out takes none.
    #[test]
    fn a_day_gone_and_a_day_here_warn_alike_and_tomorrow_warns_apart() {
        let (mut store, project) = store_with_project();
        let today = crate::time::today();
        let past = file(&mut store, project, "過ぎた", Some(today - Duration::days(3)));
        let now = file(&mut store, project, "今日", Some(today));
        let soon = file(&mut store, project, "明日", Some(today + Duration::days(1)));
        file(&mut store, project, "先の話", Some(today + Duration::days(2)));
        file(&mut store, project, "期日なし", None);

        emit(&store).unwrap();

        assert_eq!(
            warned(&store),
            [
                (name::TASK_DUE.to_string(), past),
                (name::TASK_DUE.to_string(), now),
                (name::TASK_DUE_TOMORROW.to_string(), soon),
            ]
        );
    }

    /// Closed work drops out, whichever way it was closed (`AMB-D-397`): a day that has gone on a task
    /// nobody is going to touch again is not something to warn anyone about.
    #[test]
    fn work_that_is_closed_is_not_warned_about() {
        let (mut store, project) = store_with_project();
        let today = crate::time::today();
        let done = file(&mut store, project, "済んだ", Some(today));
        let rejected = file(&mut store, project, "やらないと決めた", Some(today));
        let open = file(&mut store, project, "残っている", Some(today));
        store.set_task_status(done, TaskStatus::Done, ActorKind::Human).unwrap();
        store.set_task_status(rejected, TaskStatus::Rejected, ActorKind::Human).unwrap();

        emit(&store).unwrap();

        assert_eq!(warned(&store), [(name::TASK_DUE.to_string(), open)]);
    }

    /// A store with nothing due writes nothing — the ordinary day, and not a failure.
    #[test]
    fn a_day_with_nothing_due_writes_nothing() {
        let (mut store, project) = store_with_project();
        file(&mut store, project, "期日なし", None);

        emit(&store).unwrap();

        assert!(warned(&store).is_empty());
    }

    /// Each event names the project its task is in, which is what a project-scoped subscription is routed
    /// on (`AMB-D-405`).
    #[test]
    fn each_warning_names_the_project_its_task_is_in() {
        let (mut store, project) = store_with_project();
        file(&mut store, project, "今日", Some(crate::time::today()));

        emit(&store).unwrap();

        let rows = written(&store);
        let due = rows.iter().find(|r| r.event == name::TASK_DUE).expect("the warning was written");
        assert_eq!(due.project, Some(project));
    }
}
