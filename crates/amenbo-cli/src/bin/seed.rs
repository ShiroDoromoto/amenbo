//! Large-scale seed generator (perf/scale). A dev-only tool that builds a store holding N tasks
//! (say 1k or 10k) with their comments and dependencies in one go, so startup, search and board
//! rendering can be watched at realistic data sizes. **Always run it against an isolated store**:
//! to rule out seeding the real store or clobbering a `.amenbo` pointer, it only runs when
//! `AMENBO_HOME` (the explicit root that confines the user layer to a single directory) is set,
//! and fails loudly without touching anything when it is not. For example:
//! `AMENBO_HOME=/tmp/amenbo-scale cargo run -p amenbo-cli --bin seed -- --tasks 10000`. Writes open
//! [`Store`] once and run every op through it, each landing as its own transaction.

use std::time::Instant;

use amenbo_core::model::{ActorKind, DimensionCardinality, DimensionRole, Priority, View};
use amenbo_core::ops;
use amenbo_core::ops::dimension::NewDimension;
use amenbo_core::Store;

/// Generation parameters.
struct Opts {
    /// Total number of tasks to generate.
    tasks: usize,
    /// Comments per task.
    comments_per_task: usize,
    /// Dependency density: every Nth task gets one edge onto the task before it (0 = no edges).
    dep_every: usize,
    /// Name of the project to seed into.
    project_name: String,
    /// How many dimension values to spread the tasks across (stands in for board columns).
    values: usize,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            tasks: 1000,
            comments_per_task: 1,
            dep_every: 5,
            project_name: "Scale Test".to_string(),
            values: 4,
        }
    }
}

/// Summary of what was generated.
struct Summary {
    tasks: usize,
    comments: usize,
    dependencies: usize,
}

/// Write N tasks with their comments and dependencies into `store`, one op at a time.
fn generate(store: &mut Store, opts: &Opts) -> amenbo_core::Result<Summary> {
    // Set up the project to seed into, plus the dimension that stands in for board columns.
    let project = store.project_add(
        ops::project::NewProject {
            name: opts.project_name.clone(),
            view: View::Board,
            notes: String::new(),
            color: None,
        },
    )?;
    let dimension = store.dimension_add(
        project.id,
        NewDimension {
            name: "Stage".to_string(),
            notes: String::new(),
            cardinality: DimensionCardinality::Single,
            ordered: true,
            role: DimensionRole::None,
        },
    )?;
    let value_ids: Vec<i64> = (0..opts.values.max(1))
        .map(|i| store.dimension_value_add(dimension.id, &format!("Stage {}", i + 1), None).map(|v| v.id))
        .collect::<amenbo_core::Result<_>>()?;

    let priorities = [Priority::High, Priority::Medium, Priority::Low];

    let mut summary = Summary { tasks: 0, comments: 0, dependencies: 0 };
    let mut prev_task: Option<i64> = None;

    for i in 0..opts.tasks {
        let value_id = value_ids[i % value_ids.len()];
        let task = store.add_task(ops::task::NewTask {
            title: format!("シードタスク #{}", i + 1),
            project_id: Some(project.id),
            due_on: None,
            start_on: None,
            priority: Some(priorities[i % priorities.len()]),
            notes: format!("scale-test seed task index={i}"),
            created_by_kind: Some(ActorKind::Ai),
        })?;
        store.set_task_dimension_value(task.id, value_id)?;
        summary.tasks += 1;

        for c in 0..opts.comments_per_task {
            store.add_task_comment(
                task.id,
                ActorKind::Ai,
                &format!("シードコメント {} 件目（task index={i}）", c + 1),
            )?;
            summary.comments += 1;
        }

        // Every Nth task depends on the one before it, so the graph stays an acyclic DAG.
        if opts.dep_every > 0 && i % opts.dep_every == 0 {
            if let Some(blocker) = prev_task {
                let (_, added) = store.depend_task(task.id, blocker, Some(ActorKind::Ai))?;
                if added {
                    summary.dependencies += 1;
                }
            }
        }
        prev_task = Some(task.id);
    }

    Ok(summary)
}

fn parse_opts(args: impl Iterator<Item = String>) -> Result<Opts, String> {
    let mut opts = Opts::default();
    let mut args = args.peekable();
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{flag} expects a value"));
        match flag.as_str() {
            "--tasks" => opts.tasks = value()?.parse().map_err(|_| "--tasks expects a number".to_string())?,
            "--comments" => {
                opts.comments_per_task = value()?.parse().map_err(|_| "--comments expects a number".to_string())?
            }
            "--dep-every" => {
                opts.dep_every = value()?.parse().map_err(|_| "--dep-every expects a number".to_string())?
            }
            "--values" => {
                opts.values = value()?.parse().map_err(|_| "--values expects a number".to_string())?
            }
            "--project" => opts.project_name = value()?,
            "-h" | "--help" => return Err("help".to_string()),
            other => return Err(format!("unknown flag: {other}")),
        }
    }
    Ok(opts)
}

const USAGE: &str = "\
seed — large-scale store generator (dev only)

USAGE:
    AMENBO_HOME=<isolated dir> seed [--tasks N] [--comments N] [--dep-every N] [--values N] [--project NAME]

Refuses to run unless AMENBO_HOME is set, so it can never touch the prod store
or the project's .amenbo pointer.";

fn main() {
    // Isolation guard: without AMENBO_HOME this could reach the real app-data tree or a `.amenbo`
    // pointer, so refuse to run.
    if amenbo_core::env::home().is_none() {
        eprintln!("error: refusing to run without AMENBO_HOME set (use a throwaway isolated store).\n\n{USAGE}");
        std::process::exit(2);
    }

    let opts = match parse_opts(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(msg) => {
            if msg == "help" {
                println!("{USAGE}");
                std::process::exit(0);
            }
            eprintln!("error: {msg}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    let started = Instant::now();
    let mut store = match Store::open() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to open store: {e}");
            std::process::exit(1);
        }
    };

    let summary = match generate(&mut store, &opts) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: generation failed: {e}");
            std::process::exit(1);
        }
    };

    let elapsed = started.elapsed();
    let bytes = std::fs::metadata(&store.paths.store_file).map(|m| m.len()).unwrap_or(0);
    println!(
        "seeded {} tasks, {} comments, {} dependencies into \"{}\" in {:.1}s (store {:.1} MiB)",
        summary.tasks,
        summary.comments,
        summary.dependencies,
        opts.project_name,
        elapsed.as_secs_f64(),
        bytes as f64 / (1024.0 * 1024.0),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use amenbo_core::config::Paths;
    
        fn temp_base(tag: &str) -> std::path::PathBuf {
        let p = amenbo_scratch::scratch(&format!("seed-{tag}"));
        p
    }

    // This test is after correctness (generate -> persist -> reopen and read back), so N stays
    // small. Performance at real sizes is watched by running the `--release` binary; a debug build
    // saves and loads slowly, which would only make the test heavy without telling us anything.
    #[test]
    fn generates_and_persists() {
        let base = temp_base("gen");
        let paths = Paths::at(base);
        let opts = Opts {
            tasks: 30,
            comments_per_task: 2,
            dep_every: 5,
            project_name: "Scale Test".to_string(),
            values: 4,
        };

        let summary = {
            let mut store = Store::open_at(paths.clone()).unwrap();
            generate(&mut store, &opts).unwrap()
        };

        assert_eq!(summary.tasks, 30);
        assert_eq!(summary.comments, 60);
        // i in {0,5,10,15,20,25} are the candidates, but i=0 has no previous task to point at, so
        // only 5 edges land.
        assert_eq!(summary.dependencies, 5);

        // Reopen to confirm it persisted: the fixture a read test would build on must come back
        // intact.
        let store = Store::open_at(paths).unwrap();
        let db = amenbo_core::store_engine::hydrate_database(store.read_model().conn()).unwrap();
        let live_tasks = db.tasks.len();
        let comments = db.task_comments.len();
        let deps = db.task_dependencies.len();
        assert_eq!(live_tasks, 30);
        assert_eq!(comments, 60);
        assert_eq!(deps, 5);
    }

    #[test]
    fn parse_opts_reads_flags() {
        let args = ["--tasks", "10000", "--comments", "3", "--dep-every", "0", "--project", "Big"]
            .into_iter()
            .map(String::from);
        let opts = parse_opts(args).unwrap();
        assert_eq!(opts.tasks, 10000);
        assert_eq!(opts.comments_per_task, 3);
        assert_eq!(opts.dep_every, 0);
        assert_eq!(opts.project_name, "Big");
    }
}
