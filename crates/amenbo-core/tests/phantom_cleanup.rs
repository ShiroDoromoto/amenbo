//! `store_file_is_content_empty` is the safety gate for `doctor --fix`'s phantom
//! empty-store cleanup: it must report a freshly bootstrapped store (no user
//! content) as empty, and refuse the moment any
//! project/task exists (guardrail — never delete real data).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use amenbo_core::config::Paths;
use amenbo_core::model::Priority;
use amenbo_core::ops::task::NewTask;
use amenbo_core::Store;

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn temp_paths() -> Paths {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let base: PathBuf =
        std::env::temp_dir().join(format!("amenbo-phantom-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    Paths::at(base)
}

#[test]
fn fresh_store_reads_as_content_empty_then_not_after_a_task() {
    let paths = temp_paths();
    let store_file = paths.store_file.clone();

    // A just-opened store holds no user content → counts as a phantom-empty store
    // that --fix may clean.
    {
        let _store = Store::open_at(paths.clone()).unwrap();
    }
    assert!(
        amenbo_core::store::store_file_is_content_empty(&store_file).unwrap(),
        "bootstrapped store (workspace + self records only) must read as content-empty"
    );

    // Add one task: now it holds real content and must NOT be treated as phantom.
    {
        let mut store = Store::open_at(paths.clone()).unwrap();
        // The engine is the truth source, so write through the wrapper, which commits in its own
        // transaction.
        store
            .add_task(NewTask {
                title: "real work".to_string(),
                project_id: None,
                due_on: None,
                start_on: None,
                priority: Some(Priority::High),
                notes: String::new(),
                created_by_kind: None,
            })
            .unwrap();
    }
    assert!(
        !amenbo_core::store::store_file_is_content_empty(&store_file).unwrap(),
        "a store with a live task must never read as content-empty"
    );
}
