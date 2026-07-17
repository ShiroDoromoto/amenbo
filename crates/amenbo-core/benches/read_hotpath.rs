//! Scaling benchmark for the read hot paths. Run with `cargo bench -p amenbo-core --bench
//! read_hotpath`. For each read it sweeps N ∈ {100, 1k, 10k} background tasks so the scaling is
//! visible side by side: an O(result) read (the selective mailbox list, the AI-claim probe, the paged
//! decision list) stays flat across N, while the by-design O(store) aggregates (store-wide activity,
//! project overview) climb. This is the human-observation companion to the executable CI guard in
//! `tests/read_scaling_guard.rs`; both seed the store identically via the shared `common` module
//! (`#[path]`-included below) so the numbers you watch here and the invariant CI enforces match. The
//! store is seeded **outside** the timing loop — `iter` measures only the read, not the one-time
//! projection that builds the read-model.

#[path = "../tests/common/mod.rs"]
mod common;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use common::{seed, Seeded};

/// Background task counts to sweep (the hot carve-out is fixed-size on top of each).
const SIZES: [usize; 3] = [100, 1_000, 10_000];

fn bench_reads(c: &mut Criterion) {
    // Seed once per N and reuse across every read group, so building the 10k store is paid once.
    let seeds: Vec<(usize, Seeded)> = SIZES.iter().map(|&n| (n, seed(n))).collect();

    // Each entry: a group name and the read to time against a seeded store.
    type Run = fn(&Seeded);
    let reads: [(&str, Run); 6] = [
        ("list_task_ids_mailbox", |s| {
            std::hint::black_box(common::run_mailbox_list(s));
        }),
        // Unfiltered board page: matches the whole project, so it climbs with N like the aggregates —
        // watch it stay O(N log N) (tens of ms at 10k). An O(N²) blow-up here (a dropped child-table
        // FK index) is the regression `read_scaling_guard`'s board budget fails on.
        ("list_task_ids_board", |s| {
            std::hint::black_box(common::run_board_list(s));
        }),
        // Count-only list (limit 0): the `task_count_assigned` badge shape. Matches the whole
        // project but surfaces no page, so it climbs with N like the board — its job here is to make
        // visible that returned=0 is by design, not the O(total) regression the ratio guard would
        // otherwise flag.
        ("list_task_ids_count_only", |s| {
            std::hint::black_box(common::run_count_only_list(s));
        }),
        ("decision_page", |s| {
            std::hint::black_box(common::run_decision_page(s));
        }),
        ("store_activity", |s| {
            std::hint::black_box(common::run_store_activity(s));
        }),
        ("project_overview", |s| {
            std::hint::black_box(common::run_project_overview(s));
        }),
    ];

    for (name, run) in reads {
        let mut group = c.benchmark_group(name);
        for (n, s) in &seeds {
            group.throughput(Throughput::Elements(s.total_tasks as u64));
            group.bench_with_input(BenchmarkId::from_parameter(n), s, |b, s| b.iter(|| run(s)));
        }
        group.finish();
    }
}

criterion_group!(benches, bench_reads);
criterion_main!(benches);
