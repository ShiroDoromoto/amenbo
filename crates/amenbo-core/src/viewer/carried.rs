//! **What the Viewer's carrier was left holding** (`AMB-D-884`, `AMB-D-583`, ported from the `viewer`
//! plugin the retreat retired).
//!
//! Four numbers and a queue, and none of them means anything without the others. The whole of it is read
//! and written together, because a cursor that moved without the records it read is a stretch of the
//! backlog nothing will ever read again — which was measured: a cursor written ahead of the queue lost a
//! task for good, and no later send went looking for it.
//!
//! **What is read out and what has been told apart are two numbers.** The feed's window is five thousand
//! rows wide and it turns, so a send that cannot land for a day would be read out of the window
//! altogether if the reading waited on it. What is copied out is safe in the queue whether or not it can
//! be sent, so the reading moves at the speed of this machine and nothing else.
//!
//! **Losing the numbers is not damage; losing the queue is.** A carrier that comes back with no memory
//! places the whole backlog, which is what a first run does. The queue is the one thing here that cannot
//! be worked out again — the cursor above it says the backlog was already read that far.

use crate::error::Result;
use crate::store_engine::schema::col;
use rusqlite::OptionalExtension as _;

use crate::store_engine::sql::{Delete, Insert, Pred, Select, Sort, Sql};
use crate::store_engine::{StoreEngine, StoreEngineError};

const VS: col::viewer_send::Cols = col::viewer_send::ALL;
const VP: col::viewer_pending::Cols = col::viewer_pending::ALL;

/// The row this device's memory is, there being one.
const THE_ROW: i64 = 1;

/// What a carrier that has never sent anything stands at, and what one whose memory was thrown away
/// stands at again. Zero on every number: a backlog version nothing was sent under, a cursor at the
/// beginning, and an ordering this machine has not been told.
///
/// **Zero is "not known" for `seq`, and that is the same thing as a server that has taken nothing.** The
/// two need not be told apart: a turn sent to a server that has taken nothing has nothing behind it to
/// have been dropped, and the answer it comes back with makes the next turn checkable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Carried {
    /// The backlog version last placed there. Compared for inequality only — a restore winds the store
    /// back and the version with it.
    pub version: i64,
    /// Where to read changes on from. It is the feed's number, not ours.
    pub cursor: i64,
    /// The number the server was last left standing at, which is `version` except where that number was
    /// already the one standing there. It is what keeps the server from reading two different turns as
    /// one repeated turn.
    pub placed: i64,
    /// The ordering the server answered with when this carrier last took a turn, which is what the next
    /// turn checks its own answer against.
    pub seq: i64,
    /// The moment the server asked not to be sent to before, as it named one in a `Retry-After`.
    ///
    /// **It is remembered because the process is not.** A wait held in memory is a wait that lasts until
    /// the run ends, which is no wait at all — a server that asked to be left for a minute would be asked
    /// again by the next write, and the one after that.
    pub quiet_until: Option<String>,
    /// How many rows the server's database actually wrote on `spent_on`, as it reported them.
    ///
    /// **Measured rather than worked out.** What an upsert costs turns on whether the key was already
    /// there, so counting from this side would mean keeping a model of that database here, to go stale
    /// the day it changes. The server hands the number over with every write, so it is taken.
    pub spent: i64,
    /// The UTC day that count belongs to. A count carrying another day's date is not today's, which is
    /// the whole of the rollover — there is nothing to reset and nothing to run at midnight.
    pub spent_on: Option<String>,
    /// Which build of the Worker the server last answered a write as being.
    ///
    /// **It is remembered because a screen cannot ask.** The number travels on the answer to a write and
    /// nowhere else, so the only place it can be had is a write that already happened. Zero is "nothing
    /// has been written yet", not "an old Worker".
    pub build: i64,
}

/// One record as it waits: the key it is filed under, what happened to it, and — for everything but a
/// delete — the row as it was copied out, in the clear.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Waiting {
    pub record_key: String,
    pub op: Op,
    pub body: Option<String>,
}

/// What happened to a record. The two words are the server's own, so what is written here is what goes
/// out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Placed,
    Deleted,
}

impl Op {
    pub fn as_str(&self) -> &'static str {
        match self {
            Op::Placed => "put",
            Op::Deleted => "del",
        }
    }

    fn parse(word: &str) -> Option<Op> {
        match word {
            "put" => Some(Op::Placed),
            "del" => Some(Op::Deleted),
            _ => None,
        }
    }
}

/// Where the carrier was left. A store nothing has been sent from answers the default, which is the state
/// a first run is in.
pub fn read(engine: &StoreEngine) -> Result<Carried> {
    let conn = engine.conn();
    let mut sel = Select::new();
    let (version, cursor, placed, seq) =
        (sel.col(VS.version), sel.col(VS.cursor), sel.col(VS.placed), sel.col(VS.seq));
    let (quiet_until, spent, spent_on, build) =
        (sel.col(VS.quiet_until), sel.col(VS.spent), sel.col(VS.spent_on), sel.col(VS.build));
    let mut sql = Sql::from(&sel, VS.table);
    sql.push_where(Some(&Pred::eq(VS.id, THE_ROW)));

    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let found = stmt
        .query_row(rusqlite::params_from_iter(sql.params()), |r| {
            Ok(Carried {
                version: version.get(r)?,
                cursor: cursor.get(r)?,
                placed: placed.get(r)?,
                seq: seq.get(r)?,
                quiet_until: quiet_until.get(r)?,
                spent: spent.get(r)?,
                spent_on: spent_on.get(r)?,
                build: build.get(r)?,
            })
        })
        .optional()
        .map_err(StoreEngineError::from)?;
    Ok(found.unwrap_or_default())
}

/// Write where the carrier was left. The whole row goes at once — there is no writing one of these
/// numbers without the others.
pub fn write(engine: &StoreEngine, left: &Carried) -> Result<()> {
    Insert::into(VS.table)
        .set(VS.id, THE_ROW)
        .set(VS.version, left.version)
        .set(VS.cursor, left.cursor)
        .set(VS.placed, left.placed)
        .set(VS.seq, left.seq)
        .set_opt(VS.quiet_until, left.quiet_until.as_deref())
        .set(VS.spent, left.spent)
        .set_opt(VS.spent_on, left.spent_on.as_deref())
        .set(VS.build, left.build)
        .on_conflict_update(VS.id)
        .sql()
        .execute(engine.conn())
        .map_err(StoreEngineError::from)?;
    Ok(())
}

/// Throw away what the carrier was holding, so the next turn places the whole backlog again.
///
/// **What this remembers is what it sent, not what the server holds**, and the two part company whenever
/// that server is stood up anew: an empty one behind a memory that says "level" would be handed the next
/// edit and nothing else. Nothing can ask the server which it is — the write token is refused at its
/// reading door on purpose — so the moment of standing one up is the only place this can be settled.
///
/// The queue goes with the numbers. What waits in it was read out under a cursor this forgets, so keeping
/// it would send the same records twice and place them a second time under the new key besides.
///
/// The count a repair last showed goes too. It says how far apart the two ends were, and a server stood
/// up anew is one nobody has been shown anything about.
pub fn forget(engine: &StoreEngine) -> Result<()> {
    let tx = engine.transaction()?;
    for sql in [Delete::from(VS.table).sql(), Delete::from(VP.table).sql()] {
        sql.execute(&tx).map_err(StoreEngineError::from)?;
    }
    super::repair::forget_asked_in(&tx)?;
    tx.commit().map_err(StoreEngineError::from)?;
    Ok(())
}

/// How many records are waiting.
pub fn waiting(engine: &StoreEngine) -> Result<i64> {
    // The count is asked for on its own rather than as the correlated form `Count` builds, which is an
    // expression to put inside another statement. The table is still the registry's — there is no name
    // spelled here that a rename would leave behind.
    let sql = Sql::new(format!("SELECT COUNT(*) FROM {}", VP.table.to_sql()));
    let mut stmt = engine.conn().prepare(sql.text()).map_err(StoreEngineError::from)?;
    let count: i64 = stmt
        .query_row(rusqlite::params_from_iter(sql.params()), |r| r.get(0))
        .map_err(StoreEngineError::from)?;
    Ok(count)
}

/// Put records at the back of the queue, in the order they were read out.
pub fn enqueue(engine: &StoreEngine, records: &[Waiting]) -> Result<()> {
    if records.is_empty() {
        return Ok(());
    }
    let tx = engine.transaction()?;
    for record in records {
        Insert::into(VP.table)
            .set(VP.record_key, record.record_key.as_str())
            .set(VP.op, record.op.as_str())
            .set_opt(VP.body, record.body.as_deref())
            .sql()
            .execute(&tx)
            .map_err(StoreEngineError::from)?;
    }
    tx.commit().map_err(StoreEngineError::from)?;
    Ok(())
}

/// The first `how_many` records waiting, oldest first — what one turn takes.
pub fn front(engine: &StoreEngine, how_many: i64) -> Result<Vec<Waiting>> {
    let conn = engine.conn();
    let mut sel = Select::new();
    let (record_key, op, body) = (sel.col(VP.record_key), sel.col(VP.op), sel.col(VP.body));
    let mut sql = Sql::from(&sel, VP.table);
    sql.order_by([Sort::by(VP.id)]);
    sql.limit(how_many);

    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql.params()), |r| {
            Ok(Waiting {
                record_key: record_key.get(r)?,
                op: Op::parse(&op.get(r)?).unwrap_or(Op::Placed),
                body: body.get(r)?,
            })
        })
        .map_err(StoreEngineError::from)?;
    let mut waiting = Vec::new();
    for row in rows {
        waiting.push(row.map_err(StoreEngineError::from)?);
    }
    Ok(waiting)
}

/// Drop the first `how_many` records — what a turn does with what the server took.
///
/// **The queue is the mark.** Nothing else records how far a turn got, because a second mark is one that
/// can disagree with the queue, and a mark that disagrees is a record lost.
pub fn drop_front(engine: &StoreEngine, how_many: i64) -> Result<()> {
    if how_many <= 0 {
        return Ok(());
    }
    let conn = engine.conn();
    let mut sel = Select::new();
    let id = sel.col(VP.id);
    let mut oldest = Sql::from(&sel, VP.table);
    oldest.order_by([Sort::by(VP.id)]);
    oldest.limit(how_many);

    let mut stmt = conn.prepare(oldest.text()).map_err(StoreEngineError::from)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(oldest.params()), |r| id.get(r))
        .map_err(StoreEngineError::from)?;
    let mut taken: Vec<i64> = Vec::new();
    for row in rows {
        taken.push(row.map_err(StoreEngineError::from)?);
    }
    if taken.is_empty() {
        return Ok(());
    }
    Delete::from(VP.table)
        .filter(Pred::is_in(VP.id, taken))
        .sql()
        .execute(conn)
        .map_err(StoreEngineError::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    /// A store on a scratch base, so the rows land somewhere.
    fn store_at(tag: &str) -> Store {
        let dir = amenbo_scratch::scratch(&format!("viewer-carried-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        Store::open_at(crate::config::Paths::at(dir)).unwrap()
    }

    fn placed(key: &str) -> Waiting {
        Waiting {
            record_key: key.to_string(),
            op: Op::Placed,
            body: Some(format!("{{\"id\":\"{key}\"}}")),
        }
    }

    /// A store nothing has been sent from answers the default. That is the state a first run is in, and
    /// it is why losing the numbers costs one whole placement and no correctness.
    #[test]
    fn a_carrier_that_has_sent_nothing_stands_at_the_beginning() {
        let store = store_at("first-run");
        assert_eq!(store.viewer_carried().unwrap(), Carried::default());
        assert_eq!(store.viewer_waiting().unwrap(), 0);
    }

    /// What was written comes back, every number of it. They are written together because none of them
    /// means anything without the others.
    #[test]
    fn what_was_left_is_what_comes_back() {
        let store = store_at("round-trip");
        let left = Carried {
            version: 12_345,
            cursor: 980,
            placed: 12_344,
            seq: 41,
            quiet_until: Some("2026-09-14T09:00:00Z".into()),
            spent: 1_200,
            spent_on: Some("2026-09-14".into()),
            build: 3,
        };
        store.set_viewer_carried(&left).unwrap();
        assert_eq!(store.viewer_carried().unwrap(), left);

        // Writing again replaces the one row rather than adding a second: there is one carrier here.
        let moved = Carried { cursor: 1_400, quiet_until: None, ..left };
        store.set_viewer_carried(&moved).unwrap();
        assert_eq!(store.viewer_carried().unwrap(), moved);
    }

    /// The queue keeps what was read out, in that order, and a turn takes from the front and drops what
    /// landed. The queue is the mark — nothing else says how far a turn got.
    #[test]
    fn the_queue_is_the_mark_of_how_far_a_turn_got() {
        let store = store_at("queue");
        store
            .enqueue_viewer(&[
                placed("task/1"),
                placed("task/2"),
                Waiting { record_key: "task/3".into(), op: Op::Deleted, body: None },
            ])
            .unwrap();
        assert_eq!(store.viewer_waiting().unwrap(), 3);

        let front = store.viewer_front(2).unwrap();
        assert_eq!(front.iter().map(|w| w.record_key.as_str()).collect::<Vec<_>>(), [
            "task/1", "task/2"
        ]);

        store.drop_viewer_front(2).unwrap();
        assert_eq!(store.viewer_waiting().unwrap(), 1);
        let rest = store.viewer_front(10).unwrap();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].op, Op::Deleted, "a deletion is the key and the word, and carries no row");
        assert_eq!(rest[0].body, None);
    }

    /// Records added later go behind the ones already waiting, however many turns apart they were read
    /// out. Oldest first is what keeps a placement in the order the backlog moved.
    #[test]
    fn what_is_read_out_later_waits_behind_what_was_read_out_first() {
        let store = store_at("order");
        store.enqueue_viewer(&[placed("task/1")]).unwrap();
        store.enqueue_viewer(&[placed("task/2")]).unwrap();
        store.drop_viewer_front(1).unwrap();
        store.enqueue_viewer(&[placed("task/3")]).unwrap();

        let waiting = store.viewer_front(10).unwrap();
        assert_eq!(waiting.iter().map(|w| w.record_key.as_str()).collect::<Vec<_>>(), [
            "task/2", "task/3"
        ]);
    }

    /// Forgetting takes the numbers and the queue together. Keeping the queue would send those records
    /// twice; keeping the numbers would leave a cursor saying the backlog was read past records nothing
    /// holds any more.
    #[test]
    fn forgetting_takes_the_numbers_and_the_queue_together() {
        let store = store_at("forget");
        store.set_viewer_carried(&Carried { cursor: 900, version: 12, ..Carried::default() }).unwrap();
        store.enqueue_viewer(&[placed("task/1"), placed("task/2")]).unwrap();

        store.forget_viewer_carried().unwrap();

        assert_eq!(store.viewer_carried().unwrap(), Carried::default());
        assert_eq!(store.viewer_waiting().unwrap(), 0);
    }

    /// Dropping more than is waiting takes what is there and no more, and dropping none is nothing at
    /// all — a turn that landed nothing must not empty the queue.
    #[test]
    fn dropping_takes_only_what_is_waiting() {
        let store = store_at("drop-edges");
        store.enqueue_viewer(&[placed("task/1")]).unwrap();

        store.drop_viewer_front(0).unwrap();
        assert_eq!(store.viewer_waiting().unwrap(), 1, "a turn that landed nothing drops nothing");

        store.drop_viewer_front(5).unwrap();
        assert_eq!(store.viewer_waiting().unwrap(), 0);
        store.drop_viewer_front(5).unwrap();
    }

    /// The two words a record travels under are the server's own, so what is written here is what goes
    /// out — and a word the server does not know cannot be written at all.
    #[test]
    fn a_record_travels_under_the_word_the_server_reads() {
        assert_eq!(Op::Placed.as_str(), "put");
        assert_eq!(Op::Deleted.as_str(), "del");
        assert_eq!(Op::parse("put"), Some(Op::Placed));
        assert_eq!(Op::parse("del"), Some(Op::Deleted));
        assert_eq!(Op::parse("moved"), None);
    }
}
