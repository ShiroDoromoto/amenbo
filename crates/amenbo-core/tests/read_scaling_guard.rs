//! Executable CI guard for the read hot paths' scaling: the `cargo test` half of "criterion scaling
//! bench + CI guard" — the bench (`benches/read_hotpath.rs`) lets a human *watch* the scaling; this
//! file *fails the build* when a read regresses to O(total). It carries the perf budget —
//! machine-independent complexity ratio (`scanned / returned`) plus a coarse wall-clock sublinearity
//! net — as a red/green assertion, in two complementary guards, each scoped to where it is valid.
//! First, the **complexity ratio stays bounded and N-independent** (deterministic,
//! machine-independent): for the instrumented paged reads (`list_task_ids`, `decision_page`) the
//! recorded `scanned` is the total matched count, and on a *selective* hot query it must equal the
//! fixed hot carve-out, not grow with N — a regression that dropped the workspace/selectivity from the
//! count would push `scanned` toward N and blow past the budget, caught without any timing. Second,
//! **per-call time is sublinear in store size** (coarse, catches the index/scan regressions the ratio
//! cannot — e.g. a dropped index that makes a selective query physically scan the table): only the
//! genuinely-O(result) reads are checked, with a noise floor so a sub-millisecond healthy read never
//! trips, so the guard bites only when a read both gets slow *and* scales with N. The
//! inherently-O(store) aggregates (`store_activity`, `project_overview`) are deliberately **not** in
//! the time guard — they aggregate the whole workspace by design — but the bench still observes them.

mod common;

use amenbo_core::perf;
use common::{seed, Seeded, HOT_TASKS};

/// The largest store the guard builds, split by seed size. The deterministic O(result) ratio
/// guards prove their point at any N ≫ the hot carve-out, so the PR gate (`scale`) seeds a *medium*
/// store and stays a few seconds. The wall-clock guards (sublinearity, the board's O(N²) net) only
/// bite at the full 10k — an O(store) regression's growth at the medium seed sits inside their noise
/// margin — so the full 10k is reserved for the `scale-heavy` (nightly / dispatch) tier, which also
/// turns those timing guards on.
#[cfg(feature = "scale-heavy")]
const BIG: usize = 10_000;
/// Medium seed for the PR gate: large enough that an O(total) regression is unmistakable against the
/// fixed 50-task hot carve-out (and that the count-only read matches > BIG), small enough to seed in
/// a few seconds. The ratio guards are seed-size-independent, so this proves O(result) just as well.
#[cfg(not(feature = "scale-heavy"))]
const BIG: usize = 2_000;
/// A small store above the perf complexity-ratio noise floor, for the N-independence comparison.
const SMALL: usize = 200;

/// A read returning its (scanned, returned) complexity-ratio inputs.
type RatioRead = fn(&Seeded) -> (usize, usize);
/// A read invoked purely for its wall-clock (return value black-boxed away).
#[cfg(feature = "scale-heavy")]
type TimedRead = fn(&Seeded);

/// All guards run off **one** pair of seeded stores (building the big store is the test's whole cost,
/// so we pay it once, not once per guard). They split into two kinds: the **complexity ratio stays
/// bounded and N-independent** (deterministic, machine-independent — for the instrumented paged reads
/// the recorded `scanned` is the total matched count, which on a *selective* hot query must equal the
/// fixed hot carve-out rather than grow with N; these are seed-size-independent, so they run on every
/// PR at the medium seed), and **per-call time is sublinear in store size** (coarse, catching the
/// index/scan regressions the ratio cannot, with a noise floor so a healthy indexed read that stays
/// sub-millisecond at N=BIG is skipped; the thresholds are tuned for N=10k, so they run only under
/// `scale-heavy`, where BIG=10k).
#[test]
fn hot_reads_stay_o_result_as_the_store_grows() {
    let small = seed(SMALL);
    let big = seed(BIG);

    // Deterministic, machine-independent, seed-size-independent — every PR, medium seed.
    assert_complexity_ratio_n_independent(&small, &big);
    assert_count_only_read_not_flagged(&big);

    // Wall-clock guards: only meaningful at the full 10k, so reserved for the heavy tier.
    #[cfg(feature = "scale-heavy")]
    {
        assert_time_sublinear(&small, &big);
        assert_board_read_within_budget(&big);
    }
}

/// Guard 4 — a **count-only** read (`limit 0`, the `task_count_assigned` badge shape) must not be
/// flagged as an O(total) regression. It matches the whole project, so `scanned ≈ N`, but it surfaces
/// no page (returned=0 is intentional — only the total is wanted). The generic `scanned / returned`
/// ratio would read that as ratio ≈ N and WARN every time (a false positive an index cannot fix, since
/// `scanned` is the *matched count*, not rows examined). The count-only rule drops the ratio and keeps
/// only the time budget. This pins both halves: the generic ratio rule *would* have flagged it, and
/// the count-only rule does not.
fn assert_count_only_read_not_flagged(big: &Seeded) {
    let (scanned, returned) = common::run_count_only_list(big);
    assert_eq!(returned, 0, "a count-only read surfaces no page rows by design");
    assert!(
        scanned > BIG,
        "count-only read should match the whole project (>{BIG}), matched {scanned} — fixture/scope drift"
    );
    // The generic ratio rule misfires on a count-only read…
    assert!(
        perf::query_busts_budget(scanned, returned, 0),
        "precondition: the generic ratio rule flags this count-only read (scanned {scanned}, returned 0)"
    );
    // …but the count-only rule must not: ratio is inapplicable, only the time budget can WARN.
    assert!(
        !perf::count_query_busts_budget(scanned, 0),
        "count-only read busts the budget at N={BIG} (scanned {scanned}) — ratio must not apply to limit-0 counts"
    );
}

/// Guard 3 — the **unfiltered board page**. It legitimately matches every task in the project, so
/// neither the complexity-ratio guard (scanned ≈ N by design) nor the sublinearity guard (it is
/// genuinely O(N log N), not O(result)) applies. The failure mode it must catch is a *dropped
/// child-table index* turning each `order`-sort / placement-EXISTS subquery into a per-task table
/// scan — O(N²): a 10k-task board read jumps from tens of ms to ~13s. An O(N²) regression cannot be
/// distinguished from O(N log N) by any machine-independent ratio (both scan ~N rows), so the only
/// net is wall-clock with generous headroom: the indexed read is sub-100ms even in a debug test, so
/// a budget two orders of magnitude above that flags the O(N²) blow-up while never flaking on a
/// healthy machine. Timing-based, so it runs only in the `scale-heavy` tier (BIG=10k).
#[cfg(feature = "scale-heavy")]
fn assert_board_read_within_budget(big: &Seeded) {
    // Sanity: the board really does match the whole store (else we are not exercising the O(N²) path).
    let (scanned, _returned) = common::run_board_list(big);
    assert!(
        scanned > BIG,
        "board read should match the whole project (>{BIG}), matched {scanned} — fixture/scope drift"
    );

    const BUDGET_MS: u128 = 3_000;
    const ITERS: usize = 5;
    let t = common::median_time(ITERS, || {
        std::hint::black_box(common::run_board_list(big));
    });
    assert!(
        t.as_millis() < BUDGET_MS,
        "unfiltered board page took {t:?} at N={BIG} (budget {BUDGET_MS}ms) — a child-table index was \
         likely dropped, turning the order-sort/placement subqueries into per-task table scans (O(N²))"
    );
}

/// Guard 1 — deterministic, no wall-clock, so it can never flake.
fn assert_complexity_ratio_n_independent(small: &Seeded, big: &Seeded) {
    let reads: [(&str, RatioRead); 2] = [
        ("engine.list_task_ids", common::run_mailbox_list),
        ("engine.decision_page", common::run_decision_page),
    ];
    for (label, run) in reads {
        let (scanned_s, _returned_s) = run(small);
        let (scanned_b, returned_b) = run(big);

        // The hot slice is fixed-size, so the selective read matches the same set at both N.
        assert_eq!(
            scanned_s, HOT_TASKS,
            "{label}: selective read should match only the fixed hot carve-out at N={SMALL}, scanned {scanned_s}"
        );
        assert_eq!(
            scanned_b, HOT_TASKS,
            "{label}: scanned grew with the store (N={BIG}) — the read regressed to O(total): scanned {scanned_b}"
        );

        // Machine-independent budget: a healthy selective read never busts the ratio. (ms=0 so
        // only the complexity component is asserted — timing is guard 2's job.)
        assert!(
            !perf::query_busts_budget(scanned_b, returned_b, 0),
            "{label}: complexity ratio {} busts the budget at N={BIG} (scanned {scanned_b}, returned {returned_b})",
            perf::complexity_ratio(scanned_b, returned_b)
        );
    }
}

/// Guard 2 — coarse by design: it only fires when a read is both above a sub-millisecond noise floor
/// *and* scaled with N, the signature of an O(total) regression. Timing-based, and its ×20 growth
/// threshold is tuned for the SMALL→10k (×50 data) span, so it runs only in the `scale-heavy` tier.
#[cfg(feature = "scale-heavy")]
fn assert_time_sublinear(small: &Seeded, big: &Seeded) {
    // Below this, a read is "instant" and cannot be doing O(N) work at N=BIG (a table scan of 10k
    // rows costs well over this), so the ratio of two tiny numbers is pure noise — skip it.
    const FLOOR_US: u128 = 500;
    // O(store) growth from SMALL→BIG (×50 data) would be ~50×; a constant-factor / index-served
    // read stays ~1–3×. 20× sits far above noise yet far below a linear blow-up.
    const MAX_GROWTH: f64 = 20.0;
    const ITERS: usize = 25;

    let reads: [(&str, TimedRead); 2] = [
        ("list_task_ids(mailbox)", |s| {
            std::hint::black_box(common::run_mailbox_list(s));
        }),
        ("decision_page", |s| {
            std::hint::black_box(common::run_decision_page(s));
        }),
    ];

    for (label, run) in reads {
        let t_small = common::median_time(ITERS, || run(small));
        let t_big = common::median_time(ITERS, || run(big));

        if t_big.as_micros() < FLOOR_US {
            // Healthy: still instant at N=BIG — definitely not O(store).
            continue;
        }
        let growth = t_big.as_secs_f64() / t_small.as_secs_f64().max(1e-9);
        assert!(
            growth < MAX_GROWTH,
            "{label}: per-call time scaled {growth:.1}× from N={SMALL} ({t_small:?}) to N={BIG} ({t_big:?}) \
             — looks O(total), not O(result)"
        );
    }
}
