//! Always-on, low-overhead perf instrumentation primitives shared by the read layer and the GUI
//! command layer.
//!
//! Design (the "why", so the seams below stay coherent):
//! - **One seam per layer = automatic coverage.** Sites emit `tracing` events/spans with
//!   [`TARGET`] (`"perf"`). With no subscriber installed (the CLI today) they are cheap no-ops; the
//!   GUI installs a reloadable file subscriber (`app/src-tauri/src/perf.rs`). We instrument the
//!   store-open / command helpers ([`Timer`], [`timed`]) and the read-layer list queries
//!   ([`record_query`]) — a handful of places, not every call site.
//! - **Budget WARN, not all-logs.** A site logs at WARN *only* when it busts a budget (a unit of
//!   work slower than [`BUDGET_MS`], or a query whose `scanned / returned` complexity ratio exceeds
//!   [`COMPLEXITY_WARN_RATIO`] while touching at least [`COMPLEXITY_MIN_ROWS`] rows); otherwise it
//!   logs at DEBUG. This maps the three log modes straight onto level filters, so the toggle is just
//!   an [`EnvFilter`](https://docs.rs/tracing-subscriber) directive: off / `perf=warn` (budget-only)
//!   / `perf=debug` (verbose).
//! - **Complexity ratio is machine-independent.** `scanned / returned` shows an O(total) read at a
//!   glance regardless of CPU — a list that scans 10 000 rows to return a page of 50 stands out as
//!   ratio 200 on any machine, where a wall-clock number would not.
//!
//! The toggle resolution (env > config > channel > build) lives in [`resolve_directive`] so both the
//! GUI subscriber install and its runtime reload (`config_set_perf_log`) share one source of truth.

use std::cell::Cell;
use std::time::{Duration, Instant};

use crate::config::PerfLog;

/// tracing target for every perf event. The GUI subscriber filters on this so perf logging never
/// mixes with the app's `log`-crate output.
pub const TARGET: &str = "perf";

/// A unit of work (a command/helper) slower than this many milliseconds busts the time budget and
/// logs at WARN.
pub const BUDGET_MS: u128 = 50;

/// A read query whose `scanned / returned` ratio reaches this busts the complexity budget — it is
/// doing O(total) work to surface a small page.
pub const COMPLEXITY_WARN_RATIO: usize = 20;

/// Below this many scanned rows the complexity ratio is noise (a tiny store), so it never WARNs on
/// ratio alone — only the time budget applies.
pub const COMPLEXITY_MIN_ROWS: usize = 200;

/// Resolve the active `EnvFilter` directive for the perf target by precedence **env > config >
/// channel > build**. Always returns a directive string (never empty): `"off"` means nothing passes
/// (no file is written), so the GUI can install the layer unconditionally and still flip ON at
/// runtime via reload without restarting.
///
/// - **env** `AMENBO_PERF` (`RUST_LOG` form) wins outright; empty / `off` / `0` / `false` is an
///   explicit OFF.
/// - **config** `perf_log` (`off` / `budget-only` / `verbose`) when set.
/// - **channel** default: the development channel is ON (budget-only) — the shared `amenbo-dev`
///   build and each throwaway per-task instance alike — and `amenbo` (prod) is OFF.
/// - **build** default: debug builds are ON (budget-only), release is OFF. (Release keeps the spans
///   compiled in — only the default filter differs — so a prod binary can be turned ON locally.)
pub fn resolve_directive(perf_log: Option<PerfLog>) -> String {
    // 1. env override (RUST_LOG form). Take it verbatim unless it is an explicit off.
    if let Some(raw) = crate::env::perf() {
        let trimmed = raw.trim();
        return match trimmed {
            "" | "off" | "0" | "false" => "off".to_string(),
            other => other.to_string(),
        };
    }
    // 2. config.
    if let Some(mode) = perf_log {
        return mode.directive().to_string();
    }
    // 3. channel default.
    if crate::config::Paths::is_dev_channel() {
        return PerfLog::BudgetOnly.directive().to_string();
    }
    // amenbo (prod) channel is OFF by default — only env/config can turn it on.
    if crate::config::Paths::APP_NAME == "amenbo" {
        return PerfLog::Off.directive().to_string();
    }
    // 4. build default for any other channel name: debug ON (budget-only), release OFF.
    if cfg!(debug_assertions) {
        PerfLog::BudgetOnly.directive().to_string()
    } else {
        PerfLog::Off.directive().to_string()
    }
}

/// The `scanned / returned` complexity ratio (≥1; clamps `returned` to 1 to avoid div-by-zero). A
/// high ratio means a read touched far more rows than it surfaced — an O(total) read.
pub fn complexity_ratio(scanned: usize, returned: usize) -> usize {
    scanned / returned.max(1)
}

/// Whether a read query busts a budget: either it took longer than [`BUDGET_MS`], or its complexity
/// ratio reached [`COMPLEXITY_WARN_RATIO`] while scanning at least [`COMPLEXITY_MIN_ROWS`] rows (so a
/// tiny store never WARNs on ratio alone). Pure so the budget rule is unit-testable.
pub fn query_busts_budget(scanned: usize, returned: usize, ms: u128) -> bool {
    let over_complexity =
        scanned >= COMPLEXITY_MIN_ROWS && complexity_ratio(scanned, returned) >= COMPLEXITY_WARN_RATIO;
    over_complexity || ms > BUDGET_MS
}

/// Whether a **count-only** read busts a budget. A count-only read is one the caller issued with
/// `limit 0` to fetch just the total (no page rows), so `returned` is intentionally 0 and the
/// `scanned / returned` ratio is inapplicable — counting N matches is inherently O(N) with no page to
/// be "wasteful" against, so a ratio bust here is a false positive (a count of 278 matches would
/// always read as ratio 278). Only the wall-clock budget applies. Pure, mirroring
/// [`query_busts_budget`] so the count-only rule is unit-testable.
pub fn count_query_busts_budget(_scanned: usize, ms: u128) -> bool {
    ms > BUDGET_MS
}

/// Record one read query's complexity ratio + duration to the perf target. `scanned` is the rows the
/// filter examined (the O(total) work), `returned` the rows that reached the page — `scanned ≫
/// returned` is the signal a query touches everything. Logs WARN on a budget bust, else DEBUG.
pub fn record_query(query: &'static str, scanned: usize, returned: usize, elapsed: Duration) {
    let ms = elapsed.as_millis();
    let ratio = complexity_ratio(scanned, returned);
    if query_busts_budget(scanned, returned, ms) {
        tracing::warn!(
            target: TARGET,
            query,
            scanned,
            returned,
            ratio,
            ms,
            "perf budget exceeded"
        );
    } else {
        tracing::debug!(target: TARGET, query, scanned, returned, ratio, ms);
    }
}

/// Record a **count-only** read (the caller passed `limit 0` to fetch just `total`, no page). The
/// complexity ratio is forced to 1 (it surfaced exactly the count it was asked to compute), so only
/// the time budget can WARN — see [`count_query_busts_budget`] for why the ratio is inapplicable.
/// `count_only = true` tags the line so a high `scanned` here is not mistaken for a pagination
/// regression.
pub fn record_count_query(query: &'static str, scanned: usize, elapsed: Duration) {
    let ms = elapsed.as_millis();
    if count_query_busts_budget(scanned, ms) {
        tracing::warn!(target: TARGET, query, scanned, returned = 0_usize, ratio = 1, count_only = true, ms, "perf budget exceeded");
    } else {
        tracing::debug!(target: TARGET, query, scanned, returned = 0_usize, ratio = 1, count_only = true, ms);
    }
}

/// RAII span for a unit of work (a store open, a command body). Records its duration to the perf
/// target on drop, so every early return / `?` is covered by one line at the top of the function.
/// WARNs when the elapsed time busts [`BUDGET_MS`].
#[must_use = "the timer records on drop; bind it to a `_perf` guard for the scope you want to time"]
pub struct Timer {
    name: &'static str,
    start: Instant,
}

impl Timer {
    /// Start timing a named unit of work.
    pub fn start(name: &'static str) -> Timer {
        Timer { name, start: Instant::now() }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let ms = self.start.elapsed().as_millis();
        if ms > BUDGET_MS {
            tracing::warn!(target: TARGET, unit = self.name, ms, "perf budget exceeded");
        } else {
            tracing::debug!(target: TARGET, unit = self.name, ms);
        }
    }
}

/// Time a closure as a named unit of work (the functional form of [`Timer`]). Returns the closure's
/// value; records its duration to the perf target with the same budget rule.
pub fn timed<T>(name: &'static str, f: impl FnOnce() -> T) -> T {
    let _perf = Timer::start(name);
    f()
}

// ───────────────────── materialized-row high-water mark ─────────────────────
//
// The **primary, machine-independent** guard for the bulk ops (whole-device backup/restore
// verification and streaming export) is: *how many read-model rows are held in memory at once?* A
// streaming path that flushes each row as it goes holds a small constant regardless of store size
// (O(1) memory); a path that materialises every row into a `Vec` first holds N. This gauge lets a
// `cargo test --features scale` guard prove the constant is N-independent — a direct O(1)-memory
// proof, which outranks any wall-clock number. The instrumentation is a thread-local `Cell` add/sub
// per held row (negligible), so it is always on rather than test-gated; only the scale guard ever
// reads it.

thread_local! {
    /// Read-model rows this thread currently holds materialized in memory for a bulk op — a live
    /// gauge, incremented while a [`MaterializedRows`] guard is alive and decremented on its drop.
    static LIVE_ROWS: Cell<usize> = const { Cell::new(0) };
    /// High-water mark of [`LIVE_ROWS`] since the last [`reset_row_watermark`].
    static PEAK_ROWS: Cell<usize> = const { Cell::new(0) };
}

/// Reset the materialized-row high-water mark to zero. A scale guard calls this before a bulk op, then
/// reads [`peak_materialized_rows`] after, so successive measurements don't bleed into each other.
pub fn reset_row_watermark() {
    LIVE_ROWS.with(|c| c.set(0));
    PEAK_ROWS.with(|c| c.set(0));
}

/// The peak number of read-model rows a bulk op held in memory at once since the last
/// [`reset_row_watermark`]. The scale guard asserts this stays a small constant as the store grows —
/// the direct O(1)-memory / streaming proof.
pub fn peak_materialized_rows() -> usize {
    PEAK_ROWS.with(Cell::get)
}

/// RAII marker that `n` read-model rows are materialized in memory for its lifetime. A streaming path
/// wraps each row (`n = 1`) so the peak gauge proves it never accumulates; a full-materialize path
/// would hold `n = N`. Bumps the high-water mark on construction and releases the live count on drop.
#[must_use = "bind the guard to the scope the rows are alive; dropping it immediately frees them"]
pub struct MaterializedRows(usize);

impl MaterializedRows {
    /// Mark `n` rows as concurrently materialized until the returned guard drops.
    pub fn hold(n: usize) -> MaterializedRows {
        LIVE_ROWS.with(|live| {
            let now = live.get() + n;
            live.set(now);
            PEAK_ROWS.with(|peak| {
                if now > peak.get() {
                    peak.set(now);
                }
            });
        });
        MaterializedRows(n)
    }
}

impl Drop for MaterializedRows {
    fn drop(&mut self) {
        LIVE_ROWS.with(|live| live.set(live.get().saturating_sub(self.0)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directive_maps_each_mode() {
        assert_eq!(PerfLog::Off.directive(), "off");
        assert_eq!(PerfLog::BudgetOnly.directive(), "perf=warn");
        assert_eq!(PerfLog::Verbose.directive(), "perf=debug");
    }

    #[test]
    fn config_directive_wins_over_channel_and_build() {
        // env (AMENBO_PERF) is unset under `cargo test`, so config short-circuits channel/build.
        assert_eq!(resolve_directive(Some(PerfLog::Verbose)), "perf=debug");
        assert_eq!(resolve_directive(Some(PerfLog::BudgetOnly)), "perf=warn");
        assert_eq!(resolve_directive(Some(PerfLog::Off)), "off");
    }

    #[test]
    fn unset_config_falls_through_to_a_known_directive() {
        // With no config, the result is the channel/build default — always one of the known forms.
        let d = resolve_directive(None);
        assert!(
            matches!(d.as_str(), "off" | "perf=warn" | "perf=debug"),
            "unexpected default directive: {d}"
        );
    }

    #[test]
    fn config_set_parses_perf_log_and_resolves() {
        let mut config = crate::config::Config::default();
        assert_eq!(config.perf_log, None);
        config.set("perf_log", "budget-only").unwrap();
        assert_eq!(config.perf_log, Some(PerfLog::BudgetOnly));
        assert_eq!(resolve_directive(config.perf_log), "perf=warn");
        config.set("perf_log", "off").unwrap();
        assert_eq!(resolve_directive(config.perf_log), "off");
        assert!(config.set("perf_log", "loud").is_err());
    }

    #[test]
    fn complexity_ratio_clamps_returned_to_one() {
        assert_eq!(complexity_ratio(1000, 0), 1000);
        assert_eq!(complexity_ratio(1000, 50), 20);
        assert_eq!(complexity_ratio(0, 0), 0);
    }

    #[test]
    fn query_budget_rule() {
        // Slow query busts on time regardless of ratio.
        assert!(query_busts_budget(10, 10, BUDGET_MS + 1));
        // Big O(total) scan busts on ratio (scanned ≫ returned, above the min-rows floor).
        assert!(query_busts_budget(10_000, 50, 0));
        // A tiny set with a high ratio does NOT bust (below the min-rows floor — ratio is noise).
        assert!(!query_busts_budget(100, 1, 0));
        // A healthy paged read (ratio under the warn threshold) does NOT bust.
        assert!(!query_busts_budget(1000, 500, 5));
    }

    #[test]
    fn count_only_query_drops_the_ratio_keeps_time() {
        // A count-only read (limit 0) counts the whole store: the generic ratio rule would WARN…
        assert!(query_busts_budget(10_000, 0, 0));
        // …but the count-only rule never busts on the ratio — counting N matches is O(N) by design.
        assert!(!count_query_busts_budget(10_000, 0));
        // It still honours the time budget (a count that gets genuinely slow is a real problem).
        assert!(count_query_busts_budget(10_000, BUDGET_MS + 1));
    }

    #[test]
    fn materialized_row_watermark_tracks_peak_and_releases() {
        reset_row_watermark();
        assert_eq!(peak_materialized_rows(), 0);

        // One row held at a time (the streaming shape): peak never exceeds 1.
        for _ in 0..1000 {
            let _hold = MaterializedRows::hold(1);
            assert_eq!(peak_materialized_rows(), 1);
        }
        // The live count is back to zero (guards released), but the peak is remembered.
        assert_eq!(peak_materialized_rows(), 1);

        // A full-materialize shape (all rows held at once) records the higher peak…
        {
            let _all = MaterializedRows::hold(500);
            assert_eq!(peak_materialized_rows(), 500);
        }
        // …and the peak is the max seen, not the current live count.
        assert_eq!(peak_materialized_rows(), 500);

        // A fresh measurement starts clean.
        reset_row_watermark();
        assert_eq!(peak_materialized_rows(), 0);
    }
}
