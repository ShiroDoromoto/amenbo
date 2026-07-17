//! The unique suffix that names a throwaway directory.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A collision-free suffix for the name of a working directory under `std::env::temp_dir()`.
///
/// The only neighbours it can have are another process running at the same time, and another thread — or
/// another call — inside this one, so the triple of process id, monotonic counter and current time is
/// enough: the time alone collides between two calls in the same millisecond, and the counter alone
/// collides across processes.
///
/// This is **not an identifier** — a record's id is an INTEGER primary key. All this makes is a file name
/// nobody else will take.
pub(crate) fn suffix() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}-{:x}-{:x}", std::process::id(), nanos, seq)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Back-to-back calls in one process always differ, even inside the same millisecond.
    #[test]
    fn consecutive_suffixes_are_distinct() {
        let s: std::collections::HashSet<String> = (0..64).map(|_| suffix()).collect();
        assert_eq!(s.len(), 64);
    }
}
