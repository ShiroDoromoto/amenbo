//! Executable CI guard for the whole-device export's scaling: what matters is the **structural**
//! proof, not a wall-clock number — the streaming export stays O(1) memory, holding a small,
//! N-independent number of read-model rows in memory at once, so a regression to a "materialise every
//! row into a `Vec` first" path (O(N) memory) cannot land silently. The signal is
//! [`perf::peak_materialized_rows`]: the high-water mark of rows the export held at once. A
//! row-at-a-time stream keeps it at 1 regardless of store size; a full-materialize path would push it
//! toward N. The guard is deterministic and machine-independent (no timing), so it runs on every PR at
//! a medium seed (`scale`) — the peak is seed-size-independent, so a medium store proves the point as
//! well as the full 10k.

mod common;

use amenbo_core::progress;
use amenbo_core::store_engine::schema::DATASETS;
use amenbo_core::{export, perf};
use common::{seed, Seeded};

/// Medium seed for the PR gate — large enough that an O(N)-memory regression is unmistakable against
/// the fixed row-at-a-time peak, small enough to seed in a few seconds. The peak-rows guard is
/// seed-size-independent, so this proves O(1) memory just as well as the full 10k.
#[cfg(not(feature = "scale-heavy"))]
const BIG: usize = 2_000;
/// The full seed for the nightly heavy tier.
#[cfg(feature = "scale-heavy")]
const BIG: usize = 10_000;
/// A small store for the N-independence comparison.
const SMALL: usize = 200;

/// A small constant the streaming peak must stay under. Row-at-a-time streaming holds exactly one
/// row; the ceiling leaves headroom without admitting an O(N) (or even O(batch)) regression.
const O1_ROW_CEILING: usize = 4;

/// Run the whole-device **per-store** streaming export against a seeded store's connection (discarding
/// the bytes) and return the peak number of read-model rows it held in memory at once.
fn peak_export_rows(s: &Seeded) -> usize {
    perf::reset_row_watermark();
    let mut sink = std::io::sink();
    export::stream_store_tables(s.engine.conn(), DATASETS, &mut sink, None, &mut progress::ignore)
        .unwrap();
    perf::peak_materialized_rows()
}

/// The streaming export holds an O(1), N-independent number of rows in memory — the direct structural
/// proof that it never accumulates the whole store. Deterministic (no wall-clock), so it never flakes
/// and runs on every PR at the medium seed.
#[test]
fn streaming_export_holds_o1_rows_as_the_store_grows() {
    let small = seed(SMALL);
    let big = seed(BIG);

    let peak_small = peak_export_rows(&small);
    let peak_big = peak_export_rows(&big);

    // It actually streamed rows (didn't skip every table and vacuously "pass").
    assert!(peak_big >= 1, "export held no rows — the seed or the stream is broken");
    // O(1) memory: the peak is a small constant, unchanged as the store grows ×10.
    assert!(
        peak_big <= O1_ROW_CEILING,
        "streaming export held {peak_big} rows at N={BIG} (ceiling {O1_ROW_CEILING}) — it regressed \
         away from row-at-a-time streaming toward O(N) memory"
    );
    assert_eq!(
        peak_small, peak_big,
        "peak materialized rows grew with the store (SMALL={SMALL} -> {peak_small}, BIG={BIG} -> \
         {peak_big}) — the export is no longer O(1) memory"
    );
}
