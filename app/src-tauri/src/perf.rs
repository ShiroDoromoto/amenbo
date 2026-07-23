//! The GUI-side perf subscriber. Subscribes to the `target="perf"` tracing spans and events emitted by
//! core/commands and writes them to a rolling file. The toggle is a single EnvFilter directive
//! ([`amenbo_core::perf::resolve_directive`] resolves it from env > config > channel > build; `off` passes
//! nothing through). **A file exists only while it is ON** — the writer is lazy ([`LazyRolling`]) and does not
//! build the rolling appender until an event that got past the EnvFilter actually writes, so while OFF
//! make_writer is never called and neither the file nor the directory comes into being (we do not litter the
//! user's environment). The filter is held behind a [`reload::Layer`] whose handle lives in [`RELOAD`], so
//! `config_set_perf_log` can call [`reload()`] and change the level with no restart (spans are not compiled out
//! in release either, so even a production binary can be switched ON locally).

use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use tracing::subscriber::set_global_default;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{reload, EnvFilter, Layer, Registry};

use amenbo_core::config::{Config, PerfLog};

/// How many generations of the perf log we keep. Rotation is daily, so in practice this is a number of days.
/// Older `perf.log.YYYY-MM-DD` files beyond it are deleted by the rolling appender at the next rotation.
/// Leaving verbose ON for a long stretch therefore retains only the last few days — this cap is what stops
/// old files piling up without bound.
const MAX_LOG_FILES: usize = 7;

/// Reload handle for the running filter. `config_set_perf_log` switches the level through it.
static RELOAD: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();

/// Where perf logs are written: the shared [`crate::diag::logs_dir`], which is also the diagnostic log's.
/// Install carries on even if that fails to resolve — the writer is lazy, so unless perf logging is turned
/// ON the path is never touched.
fn log_dir() -> PathBuf {
    crate::diag::logs_dir().unwrap_or_else(|| PathBuf::from("logs"))
}

/// Lazy rolling writer: the rolling appender is built on the first write that actually happens. While OFF the
/// EnvFilter drops the events, so `make_writer` is never even called and no file is created.
#[derive(Clone)]
struct LazyRolling {
    dir: PathBuf,
    inner: Arc<Mutex<Option<RollingFileAppender>>>,
}

impl LazyRolling {
    fn new(dir: PathBuf) -> LazyRolling {
        LazyRolling { dir, inner: Arc::new(Mutex::new(None)) }
    }
}

/// The borrowed handle `make_writer` returns. It constructs the appender lazily, on `write`.
struct LazyHandle(LazyRolling);

impl Write for LazyHandle {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut guard = self.0.inner.lock().unwrap_or_else(|p| p.into_inner());
        if guard.is_none() {
            std::fs::create_dir_all(&self.0.dir)?;
            let appender = RollingFileAppender::builder()
                .rotation(Rotation::DAILY)
                .filename_prefix("perf.log")
                .max_log_files(MAX_LOG_FILES)
                .build(&self.0.dir)
                .map_err(io::Error::other)?;
            *guard = Some(appender);
        }
        guard.as_mut().expect("just constructed").write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut guard = self.0.inner.lock().unwrap_or_else(|p| p.into_inner());
        match guard.as_mut() {
            Some(w) => w.flush(),
            None => Ok(()),
        }
    }
}

impl<'a> MakeWriter<'a> for LazyRolling {
    type Writer = LazyHandle;
    fn make_writer(&'a self) -> LazyHandle {
        LazyHandle(self.clone())
    }
}

/// Resolve the initial directive and build the EnvFilter. With `config.perf_log` unset, the channel/build default applies.
fn initial_filter(config: &Config) -> EnvFilter {
    let directive = amenbo_core::perf::resolve_directive(config.perf_log);
    EnvFilter::new(directive)
}

/// Install the perf subscriber exactly once (called from `tauri` setup). The filter is held reloadable, so even
/// when it starts OFF, `config_set_perf_log` can switch it ON while the app runs. It coexists with
/// `tauri_plugin_log` (the `log` crate, registered by [`crate::diag`]), which is a separate ecosystem — we take the
/// global *tracing* subscriber but deliberately leave the `log` global logger alone, so `tauri_plugin_log` can own
/// it. (`SubscriberInitExt::try_init` would instead install the `tracing-log` bridge and grab that logger, so the
/// later `tauri_plugin_log` `set_logger` would panic on a double init.)
pub fn install(config: &Config) {
    let (filter, handle) = reload::Layer::new(initial_filter(config));
    if RELOAD.set(handle).is_err() {
        // A double install (from tests, say). Respect the first subscriber and do nothing.
        return;
    }
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_writer(LazyRolling::new(log_dir()))
        .with_filter(filter);
    // A layer-level filter (`with_filter`) lets only perf through, so no other target ends up here.
    // `set_global_default` errors (rather than panics) on a double init — the RELOAD guard above already
    // returned early on that path, so this only runs once; the `let _` is belt-and-suspenders.
    let _ = set_global_default(Registry::default().with(fmt_layer));
}

/// After `config.perf_log` is saved, reload the running filter to the new level (no restart needed). A no-op
/// before install, when the handle is not set.
pub fn reload(perf_log: Option<PerfLog>) {
    if let Some(handle) = RELOAD.get() {
        let directive = amenbo_core::perf::resolve_directive(perf_log);
        let _ = handle.reload(EnvFilter::new(directive));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises directly what **a file only while it is ON** rests on: LazyRolling creates nothing when merely
    /// constructed, and the directory and file appear only once an event that got past the EnvFilter writes.
    /// While OFF (the `off` filter) make_writer is never called, so this write never happens and no file is made.
    #[test]
    fn lazy_writer_creates_file_only_on_first_write() {
        // One level below a scratch directory: the point of the test is that nothing exists until the
        // first write, and `scratch` creates what it hands back.
        let dir = amenbo_scratch::scratch("perf-test").join("logs");
        let lazy = LazyRolling::new(dir.clone());

        // Merely constructing it, and even taking a writer from it, creates nothing (the OFF case).
        let mut handle = lazy.make_writer();
        assert!(!dir.exists(), "no dir before first write");

        // The directory and the daily file come into being at the first write (the ON case).
        handle.write_all(b"perf line\n").unwrap();
        handle.flush().unwrap();
        assert!(dir.exists(), "dir created on first write");
        let wrote_any = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().starts_with("perf.log"));
        assert!(wrote_any, "perf.log.* created on first write");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Exercises the **retention cap**: even starting from more `perf.log.*` files than the cap allows, building
    /// the appender on the first write prunes the old generations down to the last [`MAX_LOG_FILES`].
    #[test]
    fn retention_prunes_old_files_to_the_cap() {
        let dir = amenbo_scratch::scratch("perf-retention");

        // Lay down far more old generations than the cap allows (past dates, not today).
        for day in 1..=(MAX_LOG_FILES + 5) {
            std::fs::write(dir.join(format!("perf.log.2020-01-{day:02}")), b"old\n").unwrap();
        }

        // The first write builds the appender: pruning runs at that point, and today's file is created.
        let lazy = LazyRolling::new(dir.clone());
        let mut handle = lazy.make_writer();
        handle.write_all(b"perf line\n").unwrap();
        handle.flush().unwrap();

        let count = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with("perf.log"))
            .count();
        assert_eq!(
            count, MAX_LOG_FILES,
            "old perf.log.* generations should be pruned down to the cap"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
