//! The **runner leases** — at most one runner per plugin, and the horizon that keeps that from becoming a
//! deadlock (`AMB-D-399`).
//!
//! A queue is read by one runner at a time. Not because two would corrupt it — each row leaves it on its
//! own transaction — but because two would reorder it: the second runner's next row can finish before the
//! first runner's last, and a plugin is promised its own events in the order they were queued. One row per
//! plugin in this table *is* that rule, and both sides of it pass through one transaction:
//!
//! - **A drive claims before it starts.** [`claim`] takes the row when no live lease stands, and starts a
//!   runner only if it got it. A fan-out that finds the row standing leaves the work to whoever holds it.
//! - **A runner releases when its queue is empty.** [`release`] is issued on the same transaction that read
//!   the queue and found nothing left, so a row queued a moment later is either seen by that read (the
//!   runner does not leave) or lands after the release (the next drive starts a runner). There is no gap
//!   between them for an event to fall into.
//!
//! **The horizon is what a crash leaves behind.** A runner killed with its process — or with the machine —
//! never releases, and a lease with no end would mean that plugin is "already running" for good. So a lease
//! carries `expires_at`, a runner pushes it out while it works ([`extend`]), and a lease past it is void:
//! whoever finds it takes the queue over. Instants are compared as the text they are stored as, which is
//! the same order as the instants themselves — fixed-width UTC to the second, `2026-07-25T09:00:00Z`.
//!
//! **`owner` is what makes a takeover safe.** The runner that was taken over may still be alive (a machine
//! that slept through its own horizon, a clock moved). When it eventually leaves, it must not delete the
//! lease of the runner that replaced it — so [`extend`] and [`release`] name the owner they were given, and
//! a runner whose lease is no longer its own finds it has nothing to extend and nothing to release.

use rusqlite::Connection;

use super::engine::{Result, StoreEngineError};
use super::schema::col;
use super::sql::{Delete, Insert, Pred, Select, Sql, Update};

/// One plugin's runner lease, as the store holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    /// The plugin whose queue is being run.
    pub plugin: String,
    /// The runner the lease is for — the token [`extend`] and [`release`] must name to move it.
    pub owner: String,
    /// When the lease stops meaning anything unless the runner pushes it out (RFC3339 UTC).
    pub expires_at: String,
}

/// The lease standing for `plugin`, expired or not — the row as it is, with no judgement about its horizon.
/// Reading it is how a diagnosis answers *why has nothing run*; deciding on it is [`claim`]'s.
pub fn lease_of(conn: &Connection, plugin: &str) -> Result<Option<Lease>> {
    let r = col::plugin_runner::ALL;
    let mut sel = Select::new();
    let (owner, expires_at) = (sel.col(r.owner), sel.col(r.expires_at));
    let mut sql = Sql::from(&sel, r.table);
    sql.push_where(Some(&Pred::eq(r.plugin, plugin)));
    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let mut rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |row| {
            Ok(Lease {
                plugin: plugin.to_string(),
                owner: owner.get(row)?,
                expires_at: expires_at.get(row)?,
            })
        })
        .map_err(StoreEngineError::from)?;
    rows.next().transpose().map_err(StoreEngineError::from)
}

/// Take `plugin`'s lease for `owner` until `expires_at`, and say whether it was taken. `false` means a live
/// lease is already standing — someone is running that queue, and the caller starts nobody.
///
/// Read-then-write, which is sound only under the write lock: the caller's transaction is `BEGIN IMMEDIATE`
/// (every [`WriteTx`](super::write::WriteTx) is), so no second claimant can read the same absent lease and
/// take it too. A lease whose horizon has passed is not in the way — it is deleted and replaced, which is
/// how the queue of a runner that died with its machine is picked up again.
pub(super) fn claim(
    conn: &Connection,
    plugin: &str,
    owner: &str,
    expires_at: &str,
    now: &str,
) -> Result<bool> {
    if let Some(live) = lease_of(conn, plugin)?.filter(|l| l.expires_at.as_str() > now) {
        tracing::debug!(
            plugin = %plugin,
            holder = %live.owner,
            expires_at = %live.expires_at,
            "a runner is already on this queue; not starting another"
        );
        return Ok(false);
    }
    let r = col::plugin_runner::ALL;
    Delete::from(r.table).filter(Pred::eq(r.plugin, plugin)).sql().execute(conn)?;
    Insert::into(r.table)
        .set(r.plugin, plugin)
        .set(r.owner, owner)
        .set(r.expires_at, expires_at)
        .sql()
        .execute(conn)
        .map(|_| true)
        .map_err(StoreEngineError::from)
}

/// Push `owner`'s horizon out to `expires_at`, and say whether there was still a lease of its to push.
/// `false` means the lease was taken over while this runner worked — it holds nothing, and what it does
/// about that is its own business (`crate::plugin_runner` stops).
pub(super) fn extend(conn: &Connection, plugin: &str, owner: &str, expires_at: &str) -> Result<bool> {
    let r = col::plugin_runner::ALL;
    Update::table(r.table)
        .set_value(r.expires_at.name(), rusqlite::types::Value::Text(expires_at.to_string()))
        .filter(Pred::eq(r.plugin, plugin).and(Pred::eq(r.owner, owner)))
        .sql()
        .execute(conn)
        .map(|moved| moved > 0)
        .map_err(StoreEngineError::from)
}

/// Give `owner`'s lease up, and say whether it was still its to give. Issue it on the transaction that
/// found the queue empty, never on one of its own — that pairing is what leaves no gap between "nothing
/// left to run" and "nobody is running".
pub(super) fn release(conn: &Connection, plugin: &str, owner: &str) -> Result<bool> {
    let r = col::plugin_runner::ALL;
    Delete::from(r.table)
        .filter(Pred::eq(r.plugin, plugin).and(Pred::eq(r.owner, owner)))
        .sql()
        .execute(conn)
        .map(|removed| removed > 0)
        .map_err(StoreEngineError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_engine::StoreEngine;

    const T9: &str = "2026-07-25T09:00:00Z";
    const T10: &str = "2026-07-25T10:00:00Z";
    const T11: &str = "2026-07-25T11:00:00Z";

    /// Claim through a committed transaction, the way a drive does.
    fn claim_as(e: &StoreEngine, plugin: &str, owner: &str, until: &str, now: &str) -> bool {
        let tx = e.write().unwrap();
        let took = tx.claim_runner(plugin, owner, until, now).unwrap();
        tx.commit().unwrap();
        took
    }

    /// The rule itself: the first claimant takes the lease, and while it stands nobody else does.
    #[test]
    fn one_runner_per_plugin_while_the_lease_stands() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert!(claim_as(&e, "slack", "first", T10, T9), "an unheld queue is taken");
        assert!(!claim_as(&e, "slack", "second", T10, T9), "and is then not taken again");

        // Another plugin's queue is another lease — one stalled plugin holds nobody else up.
        assert!(claim_as(&e, "mail", "third", T10, T9));
        assert_eq!(lease_of(e.conn(), "slack").unwrap().unwrap().owner, "first");
    }

    /// A lease past its horizon is void: the runner that held it died without releasing, and the queue is
    /// taken over rather than left unrun for good.
    #[test]
    fn an_expired_lease_is_taken_over() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert!(claim_as(&e, "slack", "died", T10, T9));
        assert!(claim_as(&e, "slack", "next", T11, T10), "the horizon has passed, so the queue is free");
        assert_eq!(lease_of(e.conn(), "slack").unwrap().unwrap().owner, "next");
    }

    /// Extending is what keeps a working runner's lease alive — and it is the runner's own lease it moves.
    /// A runner that was taken over finds nothing to extend, which is how it learns it holds nothing.
    #[test]
    fn only_the_holder_extends_its_own_lease() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert!(claim_as(&e, "slack", "mine", T10, T9));

        let tx = e.write().unwrap();
        assert!(tx.extend_runner("slack", "mine", T11).unwrap());
        assert!(!tx.extend_runner("slack", "someone-else", T11).unwrap());
        tx.commit().unwrap();
        assert_eq!(lease_of(e.conn(), "slack").unwrap().unwrap().expires_at, T11);
    }

    /// Releasing frees the queue for the next drive — and a runner that was taken over releases nothing, so
    /// it cannot pull the lease out from under its successor on its way out.
    #[test]
    fn a_release_only_takes_the_holder_s_own_lease() {
        let e = StoreEngine::open_in_memory().unwrap();
        assert!(claim_as(&e, "slack", "died", T10, T9));
        assert!(claim_as(&e, "slack", "next", T11, T10), "taken over after the horizon passed");

        // The predecessor comes to at last and leaves: the lease standing is not the one it took.
        let tx = e.write().unwrap();
        assert!(!tx.release_runner("slack", "died").unwrap());
        tx.commit().unwrap();
        assert_eq!(
            lease_of(e.conn(), "slack").unwrap().unwrap().owner,
            "next",
            "the successor keeps running"
        );

        let tx = e.write().unwrap();
        assert!(tx.release_runner("slack", "next").unwrap());
        tx.commit().unwrap();
        assert_eq!(lease_of(e.conn(), "slack").unwrap(), None, "and the queue is free again");
        assert!(claim_as(&e, "slack", "after", T11, T10));
    }
}
