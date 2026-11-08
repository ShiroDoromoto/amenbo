//! Every screen road in `verification/scenarios/` must render in this harness.
//!
//! The registry is closed and the mapping fails closed, so an op written into a `steps_gui` road but
//! never mapped here is caught — but only when someone runs that scenario, which is once before a
//! release, in front of a screen, by hand. This asks the same question without any of that.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use amenbo_scenario::{Domain, Driver, Scenario, Step};

fn scenarios_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("verification/")
        .join("scenarios")
}

#[test]
fn every_screen_road_renders_into_instructions() {
    let mut roads = 0;
    for f in scenario_files() {
        let scenario = amenbo_scenario::lint_file(&f, None)
            .unwrap_or_else(|errs| panic!("{} does not lint: {}", f.display(), errs.join("\n")));
        if !scenario.runs_on(Driver::Gui) {
            continue;
        }
        roads += 1;
        amenbo_verify_gui::instructions(&scenario).unwrap_or_else(|e| {
            panic!("{} carries a screen road this harness cannot render: {e}", f.display())
        });
    }
    assert!(roads > 0, "expected at least one screen road");
}

/// The two steps that name a project by word rather than by binding: opening one on the ledger, and
/// going to one on the workspace's rail. Both tell an operator to find that name in a list, so it
/// has to be in one. Nothing else checks it — a step naming a project nobody raised renders, lints and
/// walks, and the operator is the one who finds out, mid-run, hunting a list for a name that was never
/// in it.
///
/// What may be named is a project the world stood up, or the one a run finds itself in already
/// (Amenbo raises a project for the folder the run works in, and calls it after that folder).
#[test]
fn every_project_a_screen_road_opens_is_a_project_that_exists() {
    let by_name = [(Domain::Project, "open"), (Domain::Workspace, "go-project")];
    for f in scenario_files() {
        let scenario = amenbo_scenario::lint_file(&f, None).expect("lints");
        if !scenario.runs_on(Driver::Gui) {
            continue;
        }
        let standing = projects_standing(&scenario);
        for step in scenario.steps(Driver::Gui) {
            let Step::Action { domain, op, with, .. } = step else { continue };
            if !by_name.contains(&(*domain, op.as_str())) {
                continue;
            }
            let named = with.get("project").and_then(|v| v.as_str()).unwrap_or_default();
            assert!(
                standing.contains(named),
                "{} goes to the project `{named}`, which nothing stands up — the world raises {:?}, \
                 and a run is already in `{}`",
                f.display(),
                standing,
                amenbo_verify_cli::scratch::CWD_DIR
            );
        }
    }
}

/// The way in in the middle of the workspace is drawn only on a face with no folder bound to it. A
/// premise is stood up by a driver that boots by raising a project on the run's folder, so a road
/// with a world — one that did not take the device back to nothing raised — opens on a face that
/// already has its folder, and an operator told to press the way in finds nothing to press. Nothing
/// else catches it: the step renders, lints and walks, and the operator is left to guess which of
/// the shapes it lists they have landed in.
#[test]
fn no_screen_road_opens_a_folder_on_a_face_that_already_has_one() {
    for f in scenario_files() {
        let scenario = amenbo_scenario::lint_file(&f, None).expect("lints");
        if !scenario.runs_on(Driver::Gui) || scenario.given.is_empty() {
            continue;
        }
        if !projects_standing(&scenario).contains(amenbo_verify_cli::scratch::CWD_DIR) {
            continue;
        }
        for step in scenario.steps(Driver::Gui) {
            let Step::Action { domain, op, .. } = step else { continue };
            assert!(
                !(*domain == Domain::Workspace && op.as_str() == "open-folder"),
                "{} opens a folder on the workspace, but its world leaves the run standing in `{}` \
                 with the run's folder bound to it, so the face has no way in to press",
                f.display(),
                amenbo_verify_cli::scratch::CWD_DIR
            );
        }
    }
}

/// A run's role takes only the tasks filed with that role, so a screen road that starts an
/// automation whose round opens at the built-in that takes a task needs one such task in its world.
/// Without it the seat finds nothing, the run ends at once, and every step after the start reads a
/// launch that is no longer there. Nothing else catches it: the road renders, lints and walks, and the
/// operator is the one who finds the run already gone.
#[test]
fn every_screen_road_that_starts_on_taking_a_task_has_a_task_filed_with_the_role() {
    for f in scenario_files() {
        let scenario = amenbo_scenario::lint_file(&f, None).expect("lints");
        if !scenario.runs_on(Driver::Gui) {
            continue;
        }
        let takes_first = automations_starting_on_take_task(&scenario.given);
        let with_role = automations_with_a_task_of_their_role(&scenario.given);
        for step in scenario.steps(Driver::Gui) {
            let Step::Action { domain: Domain::Automation, op, with, .. } = step else { continue };
            if op != "start" {
                continue;
            }
            let Some(target) = with.get("target").and_then(|v| v.as_str()) else { continue };
            if !takes_first.contains(target) {
                continue;
            }
            assert!(
                with_role.contains(target),
                "{} starts `{target}` on the screen, whose round opens at take_task, but its world \
                 files no task with that automation's role (`task update` with `role_of: {target}`), \
                 so the seat finds nothing to take and the run ends at once",
                f.display()
            );
        }
    }
}

/// The board's one way to add a card is the "＋" on the todo column's head, and a board with no task on
/// it draws the first loop in place of its columns. So a screen road whose first task is filed from the
/// board needs a task already standing in its world, or the operator is told to press a head that is
/// not drawn. Nothing else catches it: the road renders, lints and walks, and the operator is the one
/// who finds the board bare.
#[test]
fn every_screen_road_that_files_a_task_from_the_board_has_a_task_in_its_world() {
    for f in scenario_files() {
        let scenario = amenbo_scenario::lint_file(&f, None).expect("lints");
        assert!(
            !files_a_task_on_a_bare_board(&scenario),
            "{} files a task from the board on the screen, but its world stands no task up, so the \
             board draws the first loop and no column head to press — file one in `given:` first",
            f.display()
        );
    }
}

/// Held here as well as in the scan above, so the rule is reached even while every road in
/// `verification/scenarios/` keeps to it.
#[test]
fn a_task_filed_from_a_board_with_nothing_on_it_is_caught() {
    let road = |given: &str| {
        amenbo_scenario::load_str(&format!(
            r#"
id: x
title: y
{given}
steps_gui:
  - {{ type: action, domain: task, op: create, with: {{ title: seed }}, as: seed }}
"#
        ))
        .expect("loads")
    };
    assert!(files_a_task_on_a_bare_board(&road("")), "no world, no columns");
    assert!(
        !files_a_task_on_a_bare_board(&road(
            "given:\n  - { type: action, domain: task, op: create, with: { title: standing }, as: standing }"
        )),
        "a task in the world puts the columns there"
    );
    assert!(
        files_a_task_on_a_bare_board(&road(
            "given:\n  - { type: action, domain: task, op: create, with: { title: standing }, as: standing }\n  - { type: action, domain: store, op: nothing-raised }"
        )),
        "a premise that empties the device takes the task with it"
    );
}

/// Whether a screen road files a task from the board while no task stands in its world, so the first
/// card it files goes onto a board drawn without columns.
fn files_a_task_on_a_bare_board(scenario: &Scenario) -> bool {
    if !scenario.runs_on(Driver::Gui) {
        return false;
    }
    let mut standing = false;
    for step in &scenario.given {
        if let Step::Action { domain, op, .. } = step {
            match (*domain, op.as_str()) {
                (Domain::Task, "create") => standing = true,
                (Domain::Store, "nothing-raised") => standing = false,
                _ => {}
            }
        }
    }
    let files_one = scenario
        .steps(Driver::Gui)
        .iter()
        .any(|step| matches!(step, Step::Action { domain: Domain::Task, op, .. } if op == "create"));
    files_one && !standing
}

/// The automations, by binding, whose placement marked to start is the built-in that takes a task.
fn automations_starting_on_take_task(given: &[Step]) -> HashSet<String> {
    given
        .iter()
        .filter_map(|step| {
            let Step::Action { domain: Domain::Automation, op, with, .. } = step else { return None };
            let starts = with.get("starts").and_then(|v| v.as_bool()).unwrap_or(false);
            let take = with.get("builtin").and_then(|v| v.as_str()) == Some("take_task");
            (op == "place-add" && starts && take)
                .then(|| with.get("target").and_then(|v| v.as_str()).map(str::to_string))
                .flatten()
        })
        .collect()
}

/// The automations, by binding, that a task in the world is filed with the role of.
fn automations_with_a_task_of_their_role(given: &[Step]) -> HashSet<String> {
    given
        .iter()
        .filter_map(|step| {
            let Step::Action { domain: Domain::Task, op, with, .. } = step else { return None };
            (op == "update")
                .then(|| with.get("role_of").and_then(|v| v.as_str()).map(str::to_string))
                .flatten()
        })
        .collect()
}

/// A run that puts its task off ends with the task back at `todo`, still on the AI. The seat does not
/// stop with the run: its next round's take finds that task first unless something makes it wait for
/// another. So a screen road that reads such a task as `todo` once a step has been reported done reads
/// a task the next round has already taken again. Nothing else catches it: the road renders, lints and
/// walks, and the operator is the one who finds the task in progress.
#[test]
fn no_screen_road_reads_a_task_put_off_as_waiting_when_nothing_makes_it_wait() {
    for f in scenario_files() {
        let scenario = amenbo_scenario::lint_file(&f, None).expect("lints");
        if !scenario.runs_on(Driver::Gui) {
            continue;
        }
        let read = todo_reads_of_a_task_taken_again(&scenario);
        assert!(
            read.is_empty(),
            "{} reads {read:?} as `todo` after a step is reported done, but the run that took it puts \
             it off and nothing makes it wait for another (`task depend`), so the seat goes round \
             again and its next run takes it — it is `in_progress` by then",
            f.display()
        );
    }
}

/// Held here as well as in the scan above, so the rule is reached even while every road in
/// `verification/scenarios/` keeps to it.
#[test]
fn a_task_put_off_with_nothing_to_wait_for_is_not_read_as_todo() {
    let road = |depend: &str| {
        format!(
            r#"
id: x
title: y
given:
  - {{ type: action, domain: task, op: create, with: {{ title: seed }}, as: seed }}
  - {{ type: action, domain: task, op: create, with: {{ title: fix }}, as: fix }}
  - {{ type: action, domain: automation, op: create, with: {{ name: auto }}, as: auto }}
  - {{ type: action, domain: automation, op: place-add, with: {{ target: auto, builtin: take_task, starts: true }}, as: at_start }}
  - {{ type: action, domain: task, op: update, with: {{ target: seed, role_of: auto }} }}
  - {{ type: action, domain: automation, op: place-add, with: {{ target: auto, builtin: defer_task }}, as: at_later }}
{depend}
steps_gui:
  - {{ type: action, domain: automation, op: start, with: {{ target: auto }}, as: run }}
  - {{ type: action, domain: automation, op: done-in-pane, with: {{ target: run, exit: got one }} }}
  - {{ type: assert, domain: task, op: field, with: {{ target: seed, field: status, equals: todo }} }}
"#
        )
    };

    let taken_again = amenbo_scenario::load_str(&road("")).expect("loads");
    assert_eq!(
        todo_reads_of_a_task_taken_again(&taken_again),
        vec!["seed".to_string()],
        "a task put off with nothing to wait for is taken again by the seat's next round, so `todo` \
         is not what a road reads after the step is done"
    );

    let waiting = amenbo_scenario::load_str(&road(
        "  - { type: action, domain: task, op: depend, with: { target: seed, on: fix } }",
    ))
    .expect("loads");
    assert!(
        todo_reads_of_a_task_taken_again(&waiting).is_empty(),
        "a task that waits for another is not taken again, so it does stay `todo`"
    );
}

/// The tasks, by binding, a screen road reads as `todo` after a step has been reported done, where
/// the task is filed with the role of an automation it started that puts a task off, and nothing in
/// the world makes the task wait for another.
fn todo_reads_of_a_task_taken_again(scenario: &Scenario) -> Vec<String> {
    let puts_off: HashSet<String> = scenario
        .given
        .iter()
        .filter_map(|step| {
            let Step::Action { domain: Domain::Automation, op, with, .. } = step else { return None };
            let defer = with.get("builtin").and_then(|v| v.as_str()) == Some("defer_task");
            (op == "place-add" && defer)
                .then(|| with.get("target").and_then(|v| v.as_str()).map(str::to_string))
                .flatten()
        })
        .collect();
    let mut role_of: Vec<(String, String)> = Vec::new();
    let mut waits: HashSet<String> = HashSet::new();
    for step in &scenario.given {
        let Step::Action { domain: Domain::Task, op, with, .. } = step else { continue };
        let target = with.get("target").and_then(|v| v.as_str()).map(str::to_string);
        match op.as_str() {
            "update" => {
                if let (Some(task), Some(role)) =
                    (target, with.get("role_of").and_then(|v| v.as_str()))
                {
                    role_of.push((task, role.to_string()));
                }
            }
            "depend" => waits.extend(target),
            _ => {}
        }
    }

    let mut started: HashSet<String> = HashSet::new();
    let mut done = false;
    let mut read = Vec::new();
    for step in scenario.steps(Driver::Gui) {
        match step {
            Step::Action { domain: Domain::Automation, op, with, .. } => match op.as_str() {
                "start" => started.extend(with.get("target").and_then(|v| v.as_str()).map(str::to_string)),
                "done-in-pane" => done = true,
                _ => {}
            },
            Step::Assert { domain: Domain::Task, op, with, .. } if done && op == "field" => {
                let field = with.get("field").and_then(|v| v.as_str());
                let equals = with.get("equals").and_then(|v| v.as_str());
                let Some(task) = with.get("target").and_then(|v| v.as_str()) else { continue };
                let taken_again = role_of
                    .iter()
                    .any(|(t, role)| t == task && started.contains(role) && puts_off.contains(role));
                if field == Some("status") && equals == Some("todo") && taken_again && !waits.contains(task) {
                    read.push(task.to_string());
                }
            }
            _ => {}
        }
    }
    read
}

/// The projects an operator could find in the list when a road opens: whatever the world raised, and
/// the one the run is standing in before it raised anything.
///
/// Walked in order, because a premise can take them all away again: `store nothing-raised` leaves a
/// device with none, and a road opening on that names no project at all.
fn projects_standing(scenario: &Scenario) -> HashSet<String> {
    let mut names: HashSet<String> = HashSet::new();
    names.insert(amenbo_verify_cli::scratch::CWD_DIR.to_string());
    for step in &scenario.given {
        if let Step::Action { domain, op, with, .. } = step {
            match (*domain, op.as_str()) {
                (Domain::Project, "create") => {
                    if let Some(name) = with.get("name").and_then(|v| v.as_str()) {
                        names.insert(name.to_string());
                    }
                }
                (Domain::Store, "nothing-raised") => names.clear(),
                _ => {}
            }
        }
    }
    names
}

/// A premise that takes the device back to nothing raised on it leaves no project to name — the
/// boot's own included. Held here rather than left to the scan above, because no scenario walks that
/// world yet: the roads it exists for are written against ops that are still being added, and until
/// one of them lands the rule would be a line nothing ever reaches.
#[test]
fn a_premise_that_empties_the_device_leaves_no_project_to_name() {
    let emptied = amenbo_scenario::load_str(
        r#"
id: x
title: y
given:
  - type: action
    domain: project
    op: create
    with: { name: Greenhouse }
  - type: action
    domain: store
    op: nothing-raised
steps_gui:
  - type: action
    domain: project
    op: create
    with: { name: Seedbed }
"#,
    )
    .expect("loads");
    assert!(
        projects_standing(&emptied).is_empty(),
        "nothing stands on a device the premise emptied — not what it raised, and not the boot's own"
    );

    let raised = amenbo_scenario::load_str(
        r#"
id: x
title: y
given:
  - type: action
    domain: store
    op: nothing-raised
  - type: action
    domain: project
    op: create
    with: { name: Greenhouse }
steps_gui:
  - type: action
    domain: project
    op: create
    with: { name: Seedbed }
"#,
    )
    .expect("loads");
    assert_eq!(
        projects_standing(&raised),
        ["Greenhouse".to_string()].into_iter().collect::<HashSet<String>>(),
        "and what the premise raises after it does stand — the emptying is a moment, not a mode"
    );
}

fn scenario_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(scenarios_dir())
        .expect("scenarios dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("yaml"))
        .collect();
    files.sort();
    files
}
