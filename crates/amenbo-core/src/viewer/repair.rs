//! **Putting right what the Viewer's server holds, when it has drifted from what this machine holds**
//! (`AMB-D-884`, `AMB-D-814`, ported from the `viewer` plugin the retreat retired).
//!
//! **Nothing here reads the feed.** The ordinary send ([`super::send`]) carries what moved, and it is
//! right whenever the two ends started level; what it cannot do is notice that they never were. A key
//! written under a name the far end files differently, a turn that was answered and never written, a
//! server stood up beside an older one — each leaves a gap no later change points at, so no stretch of the
//! feed will ever carry it.
//!
//! So this asks the other question: **what is over there, and what is here?** The server answers with its
//! keys, the picture answers with this machine's, and the difference goes on the same queue every other
//! record travels on. Nothing is emptied and nothing is reset — a phone reading while this runs sees
//! records arrive, and no more.
//!
//! **It is two presses, not one.** The comparison is cheap and the placing is not: a backlog that has
//! drifted whole is tens of thousands of writes, which was measured at 49,143 and is half a free
//! Cloudflare plan's day. So the first press counts and says the number, and the second places it — which
//! is the only shape in which a person is told what it costs before they spend it.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, TimeDelta, Utc};
use rusqlite::OptionalExtension as _;

use crate::error::{Error, Result};
use crate::store::Store;
use crate::store_engine::schema::col;
use crate::store_engine::sql::{Delete, Insert, Pred, Select, Sql};
use crate::store_engine::{StoreEngine, StoreEngineError};

use super::carried::{Op, Waiting};
use super::send::{carry_at, the_whole_picture, Rebuffed, Sent, Server};

/// How long a count that was shown stands for.
///
/// Past it the next press counts again rather than placing: what a person consented to is the number they
/// were shown, and a server that has moved since is one they were shown nothing about.
const THE_ASK_STANDS_FOR: TimeDelta = TimeDelta::minutes(10);

/// What the two ends differ by: the records this machine holds that the server is not holding, and the
/// keys the server is holding that this machine no longer has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Drift {
    pub to_place: usize,
    pub to_drop: usize,
}

impl Drift {
    /// Whether the two ends are level, which is the answer a healthy device gives.
    pub fn is_level(&self) -> bool {
        self.to_place == 0 && self.to_drop == 0
    }
}

/// What one press did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum Repaired {
    /// Setup has not run on this device, so there is no server to compare against. It is not a failure —
    /// a device nobody has set the Viewer up on is not drifting from anything.
    NotSetUp,
    /// The two ends hold the same keys.
    Level,
    /// The difference was counted and written down. The next press inside [`THE_ASK_STANDS_FOR`] places
    /// it.
    Counted(Drift),
    /// The difference went on the queue, and as much of it as the server would take has gone.
    Placed { drift: Drift, sent: Sent },
}

/// What the repair last counted, and when it said so.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    pub asked_at: String,
    pub drift: Drift,
}

/// Compare the server with this machine, and — on the press that spends it — put the difference on the
/// queue and carry it.
///
/// `place` is the second press for whoever typed the first one: the person is standing at the terminal
/// that printed the count, so there is nobody left to ask.
pub fn repair(store: &Store, place: bool) -> Result<Repaired> {
    repair_at(store, place, Utc::now())
}

/// [`repair`], with the moment handed in — what a test drives, and what keeps the window a count stands
/// for out of a clock nothing can move.
pub fn repair_at(store: &Store, place: bool, now: DateTime<Utc>) -> Result<Repaired> {
    let Some(server) = Server::of_device(store)? else {
        return Ok(Repaired::NotSetUp);
    };

    let holds = what_it_holds(&server).map_err(|rebuffed| rebuffed.in_words(now))?;
    let placing = place || the_count_still_stands(store, now)?;
    let drift = compared_with(store, &holds, placing)?;

    if drift.is_level() {
        store.forget_viewer_asked()?;
        return Ok(Repaired::Level);
    }
    if !placing {
        store.set_viewer_asked(&Asked {
            asked_at: crate::time::Timestamp(now).to_rfc3339_z(),
            drift,
        })?;
        tracing::info!(
            "the Viewer's server is out by {} record(s) to place and {} to drop — the next press \
             carries them",
            drift.to_place,
            drift.to_drop,
        );
        return Ok(Repaired::Counted(drift));
    }

    // The count has been spent. Forgetting it before the carrying is what keeps a press after the sending
    // counting again rather than placing a second time on the strength of a number already spent.
    store.forget_viewer_asked()?;
    let sent = carry_at(store, now)?;
    tracing::info!(
        "{} record(s) to place and {} to drop went on the Viewer's queue",
        drift.to_place,
        drift.to_drop,
    );
    Ok(Repaired::Placed { drift, sent })
}

/// Whether a count has been shown recently enough to be what this press consents to.
///
/// Anything absent, anything unreadable and anything older is "nobody has been told yet" — the safe
/// answer, since being asked twice costs a comparison and being asked never costs the writes.
fn the_count_still_stands(store: &Store, now: DateTime<Utc>) -> Result<bool> {
    let Some(asked) = store.viewer_asked()? else {
        return Ok(false);
    };
    let Some(shown) = crate::time::Timestamp::parse_rfc3339(&asked.asked_at) else {
        return Ok(false);
    };
    Ok(now >= shown.0 && now - shown.0 <= THE_ASK_STANDS_FOR)
}

/// Work out what the server is missing and what it is holding that is gone, and — when this is the press
/// that spends it — put both on the back of the queue.
///
/// **The cursor is not moved.** Nothing was read out of the feed, so nothing has been dealt with: what this
/// adds is beside the ordinary send's work, never instead of it.
fn compared_with(store: &Store, holds: &Holding, queue: bool) -> Result<Drift> {
    // What is queued here is compared against a picture, and a stretch of the feed copied out between the
    // two would sit behind records answering to an older reading of the same rows — so a delete worked out
    // here could land on top of a write already queued. The lock that keeps one turn to one process is
    // `AMB-T-4855`'s, and this is where it is taken once it exists.
    let (here, _) = the_whole_picture(store)?;
    let (mut missing, gone) = the_difference(&here, &holds.keys);
    let drift = Drift { to_place: missing.len(), to_drop: gone.len() };
    if !queue || drift.is_level() {
        return Ok(drift);
    }

    missing.extend(gone);
    store.enqueue_viewer(&missing)?;

    // **Where the server stands is taken from the server**, which nothing else here can do. The ordering a
    // send checks its answer against is remembered from the last answer, and a server written by something
    // else — a second machine, a placement that was taken and never answered for — stands somewhere that
    // memory does not name. Every send after that fails the check and nothing lands again, so a comparison
    // that did not re-anchor this would report a drift it had just made permanent.
    let mut left = store.viewer_carried()?;
    left.seq = holds.seq;
    store.set_viewer_carried(&left)?;
    Ok(drift)
}

/// The two halves of a drift.
///
/// **A key the server has already been told is gone counts as missing.** A `del` row is the phone being
/// told to forget it, so a record that is here and filed there as deleted is one the phone has thrown
/// away — the same gap as a record that never arrived, and the same cure.
///
/// The keys to drop come out of a sorted map rather than a hashed one, so one drift produces one queue
/// however many times it is worked out.
fn the_difference(
    here: &[Waiting],
    held: &BTreeMap<String, String>,
) -> (Vec<Waiting>, Vec<Waiting>) {
    let mut mine = BTreeSet::new();
    let mut missing = Vec::new();
    for record in here {
        mine.insert(record.record_key.as_str());
        if held.get(&record.record_key).map(String::as_str) != Some(Op::Placed.as_str()) {
            missing.push(record.clone());
        }
    }
    let gone = held
        .iter()
        .filter(|(key, op)| op.as_str() == Op::Placed.as_str() && !mine.contains(key.as_str()))
        .map(|(key, _)| Waiting { record_key: key.clone(), op: Op::Deleted, body: None })
        .collect();
    (missing, gone)
}

/// What the server answered about itself: the keys it is holding with the last word it heard about each,
/// and the point its ordering stands at.
struct Holding {
    keys: BTreeMap<String, String>,
    seq: i64,
}

/// One page of what the server holds. Only the key and what happened to it are read: the envelope is the
/// phone's business, and this is not reading the backlog, it is counting it.
#[derive(serde::Deserialize)]
struct KeysHeld {
    #[serde(default)]
    seq: i64,
    #[serde(default)]
    more: bool,
    #[serde(default)]
    records: Vec<KeyHeld>,
}

#[derive(serde::Deserialize)]
struct KeyHeld {
    #[serde(rename = "k")]
    key: String,
    op: String,
}

/// Read every key the server is holding, what it last heard about each, and where its ordering stands.
///
/// **It reads with the write token this device already holds.** The reading doors take either kind for
/// exactly this. What the two kinds keep apart is the phone's half — a code photographed off a screen
/// writes nothing — and this end already holds the key and the backlog these records were sealed from.
/// Issuing a read code here instead would replace the phone's and taking it away again would leave none,
/// so a comparison would end with nobody paired.
///
/// **Where the ordering stands is asked before the pages** rather than read off the last of them, because
/// a server holding nothing answers no page at all and still stands somewhere.
///
/// **The envelopes come back and are thrown away.** `?keys=1` asks for the keys alone; a Worker deployed
/// before that question was answered ignores it and sends the ciphertext too, which costs the bandwidth
/// and reads exactly the same — so this works against either.
fn what_it_holds(server: &Server) -> std::result::Result<Holding, Rebuffed> {
    let standing = server.standing()?;
    let mut keys = BTreeMap::new();
    let mut since = 0i64;
    loop {
        let answered = server.ask("GET", &format!("/records?since={since}&keys=1"), None)?;
        let page: KeysHeld = serde_json::from_slice(&answered).map_err(|err| {
            Rebuffed::Unreadable(Error::invalid(format!(
                "/records answered with something this build cannot read: {err}"
            )))
        })?;
        for record in page.records {
            keys.insert(record.key, record.op);
        }
        if !page.more {
            return Ok(Holding { keys, seq: standing.seq });
        }
        if page.seq <= since {
            return Err(Rebuffed::Unreadable(Error::invalid(format!(
                "the Viewer's server claims another page of records and its ordering did not move \
                 past {since}"
            ))));
        }
        since = page.seq;
    }
}

const VA: col::viewer_asked::Cols = col::viewer_asked::ALL;

/// The row this device's count is, there being one.
const THE_ROW: i64 = 1;

/// The count last shown on this device, or `None` where none has been.
pub fn read_asked(engine: &StoreEngine) -> Result<Option<Asked>> {
    let conn = engine.conn();
    let mut sel = Select::new();
    let (asked_at, to_place, to_drop) =
        (sel.col(VA.asked_at), sel.col(VA.to_place), sel.col(VA.to_drop));
    let mut sql = Sql::from(&sel, VA.table);
    sql.push_where(Some(&Pred::eq(VA.id, THE_ROW)));

    let mut stmt = conn.prepare(sql.text()).map_err(StoreEngineError::from)?;
    let found = stmt
        .query_row(rusqlite::params_from_iter(sql.params()), |r| {
            let (to_place, to_drop): (i64, i64) = (to_place.get(r)?, to_drop.get(r)?);
            Ok(Asked {
                asked_at: asked_at.get(r)?,
                drift: Drift {
                    to_place: to_place.max(0) as usize,
                    to_drop: to_drop.max(0) as usize,
                },
            })
        })
        .optional()
        .map_err(StoreEngineError::from)?;
    Ok(found)
}

/// Write down that a count was shown, so the next press is the one that spends it.
pub fn write_asked(engine: &StoreEngine, asked: &Asked) -> Result<()> {
    Insert::into(VA.table)
        .set(VA.id, THE_ROW)
        .set(VA.asked_at, asked.asked_at.as_str())
        .set(VA.to_place, asked.drift.to_place as i64)
        .set(VA.to_drop, asked.drift.to_drop as i64)
        .on_conflict_update(VA.id)
        .sql()
        .execute(engine.conn())
        .map_err(StoreEngineError::from)?;
    Ok(())
}

/// Take that record away.
pub fn forget_asked(engine: &StoreEngine) -> Result<()> {
    Delete::from(VA.table).sql().execute(engine.conn()).map_err(StoreEngineError::from)?;
    Ok(())
}

/// The same, inside a transaction somebody else opened — which is how forgetting the carrier takes the
/// count with it ([`super::carried::forget`]).
pub(super) fn forget_asked_in(tx: &rusqlite::Transaction<'_>) -> Result<()> {
    Delete::from(VA.table).sql().execute(tx).map_err(StoreEngineError::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SecretArea;
    use amenbo_static_host::{Reply, StaticHost};
    use base64::Engine as _;

    /// A store on a scratch base, so what a press reads and writes lands somewhere.
    fn store_at(tag: &str) -> Store {
        let dir = amenbo_scratch::scratch(&format!("viewer-repair-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        Store::open_at(crate::config::Paths::at(dir)).unwrap()
    }

    /// Leave behind what setup leaves behind, pointed at a stand-in.
    fn set_up(store: &mut Store, host: &StaticHost) {
        let key = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode([7u8; super::super::sealing::KEY_SIZE]);
        for (field, value) in [
            (super::super::WORKER_URL, host.url("")),
            (super::super::AUTH_TOKEN, "write-token".to_string()),
            (super::super::ENCRYPTION_KEY, key),
        ] {
            store.set_secret(None, SecretArea::Viewer, None, field, Some(&value)).unwrap();
        }
    }

    /// A project in the store, so the picture has something in it.
    fn seed(store: &mut Store) {
        store
            .project_add(crate::ops::project::NewProject {
                name: "drifted".into(),
                view: crate::model::View::List,
                notes: String::new(),
                color: None,
            })
            .unwrap();
    }

    /// A carrier that has been sending, which is the state a repair is pressed in. The version is the one
    /// the store stands at, so the placing press copies nothing out on top of what the comparison queued.
    fn already_carrying(store: &Store, seq: i64) {
        let version = store.device_sync_version().unwrap();
        store
            .set_viewer_carried(&super::super::carried::Carried {
                version,
                cursor: 1,
                placed: version,
                seq,
                ..Default::default()
            })
            .unwrap();
    }

    /// Every key this machine holds, in the order the picture names them.
    fn here(store: &Store) -> Vec<String> {
        the_whole_picture(store)
            .unwrap()
            .0
            .into_iter()
            .map(|record| record.record_key)
            .collect()
    }

    /// A server standing at `seq`.
    fn meta(seq: i64) -> Reply {
        Reply::ok(serde_json::json!({ "spec_v": 1, "version": 0, "seq": seq }).to_string())
    }

    /// One page of what a server holds.
    fn page(held: &[(&str, &str)], seq: i64, more: bool) -> Reply {
        let records: Vec<serde_json::Value> = held
            .iter()
            .map(|(key, op)| serde_json::json!({ "k": key, "op": op }))
            .collect();
        Reply::ok(serde_json::json!({ "seq": seq, "more": more, "records": records }).to_string())
    }

    /// What a server holding every one of this machine's keys answers.
    fn holding_all(store: &Store) -> Reply {
        let held: Vec<(String, String)> =
            here(store).into_iter().map(|key| (key, "put".to_string())).collect();
        let named: Vec<(&str, &str)> =
            held.iter().map(|(key, op)| (key.as_str(), op.as_str())).collect();
        page(&named, 0, false)
    }

    /// A server taking a write and answering where its ordering now stands.
    fn took(seq: i64, rows: i64) -> Reply {
        Reply::ok(serde_json::json!({ "seq": seq, "rows_written": rows, "build": 3 }).to_string())
    }

    fn at(when: &str) -> DateTime<Utc> {
        crate::time::Timestamp::parse_rfc3339(when).unwrap().0
    }

    /// The keys one press put on the queue, oldest first.
    fn queued(store: &Store) -> Vec<(String, String)> {
        store
            .viewer_front(1_000)
            .unwrap()
            .into_iter()
            .map(|record| (record.record_key, record.op.as_str().to_string()))
            .collect()
    }

    /// **A device nobody has set the Viewer up on is not drifting from anything.** There is no server to
    /// compare against, and saying so is not a failure to report.
    #[test]
    fn a_device_with_no_server_is_not_compared() {
        let store = store_at("no-server");
        assert_eq!(repair(&store, false).unwrap(), Repaired::NotSetUp);
    }

    /// **The comparison is cheap and the placing is not**, so the first press counts and says the number,
    /// and the second places it. Nothing reaches the queue on the first.
    #[test]
    fn the_first_press_counts_and_the_second_places() {
        let mut store = store_at("two-presses");
        seed(&mut store);
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        already_carrying(&store, 0);
        host.set_reply("/meta", meta(0));
        host.set_reply("/records?since=0&keys=1", page(&[], 0, false));

        let mine = here(&store);
        let counted = at("2026-09-14T10:00:00Z");
        let Repaired::Counted(drift) = repair_at(&store, false, counted).unwrap() else {
            panic!("a server holding nothing is a drift, and the first press counts it");
        };
        assert_eq!(drift, Drift { to_place: mine.len(), to_drop: 0 });
        assert_eq!(store.viewer_waiting().unwrap(), 0, "counting queues nothing");
        assert!(store.viewer_asked().unwrap().is_some(), "the count that was shown is written down");

        host.set_reply("/records", took(mine.len() as i64, mine.len() as i64));
        let placed = repair_at(&store, false, counted + TimeDelta::minutes(1)).unwrap();
        assert_eq!(
            placed,
            Repaired::Placed {
                drift,
                sent: Sent { placed: mine.len(), waiting: 0 },
            },
            "the press after the count is the one that spends it",
        );
        assert!(
            store.viewer_asked().unwrap().is_none(),
            "a number that has been spent stands for no further press",
        );
    }

    /// **A count is what a person was shown**, so one older than the window it stands for is counted again
    /// rather than placed — a store that has moved since is one they were shown nothing about.
    #[test]
    fn a_count_older_than_its_window_is_counted_again() {
        let mut store = store_at("stale-count");
        seed(&mut store);
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        already_carrying(&store, 0);
        host.set_reply("/meta", meta(0));
        host.set_reply("/records?since=0&keys=1", page(&[], 0, false));

        let counted = at("2026-09-14T10:00:00Z");
        repair_at(&store, false, counted).unwrap();
        let pressed = repair_at(&store, false, counted + THE_ASK_STANDS_FOR + TimeDelta::seconds(1));

        assert!(
            matches!(pressed.unwrap(), Repaired::Counted(_)),
            "past the window the press counts rather than placing",
        );
        assert_eq!(store.viewer_waiting().unwrap(), 0);
    }

    /// **The press that types `--send` is the second press for whoever typed the first one**: the person
    /// is standing at the terminal that printed the count, so there is nobody left to ask.
    #[test]
    fn asking_to_place_needs_no_count_before_it() {
        let mut store = store_at("straight-to-placing");
        seed(&mut store);
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        already_carrying(&store, 0);
        host.set_reply("/meta", meta(0));
        host.set_reply("/records?since=0&keys=1", page(&[], 0, false));
        let mine = here(&store);
        host.set_reply("/records", took(mine.len() as i64, mine.len() as i64));

        let placed = repair_at(&store, true, at("2026-09-14T10:00:00Z")).unwrap();
        assert!(matches!(placed, Repaired::Placed { .. }));
    }

    /// **Two ends that hold the same keys are level**, and a count that was shown of a drift since put
    /// right stands for nothing.
    #[test]
    fn two_ends_holding_the_same_keys_are_level() {
        let mut store = store_at("level");
        seed(&mut store);
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        already_carrying(&store, 0);
        host.set_reply("/meta", meta(4));
        host.set_reply("/records?since=0&keys=1", holding_all(&store));
        store
            .set_viewer_asked(&Asked {
                asked_at: "2026-09-14T09:59:00Z".into(),
                drift: Drift { to_place: 3, to_drop: 0 },
            })
            .unwrap();

        assert_eq!(repair_at(&store, false, at("2026-09-14T10:00:00Z")).unwrap(), Repaired::Level);
        assert_eq!(store.viewer_waiting().unwrap(), 0);
        assert!(store.viewer_asked().unwrap().is_none());
    }

    /// **A key the server has already been told is gone counts as missing.** A `del` row is the phone
    /// being told to forget it, so a record that is here and filed there as deleted is one the phone has
    /// thrown away — the same gap as a record that never arrived.
    ///
    /// And a key the server holds that this machine no longer has is queued as a delete.
    #[test]
    fn a_dropped_key_is_missing_and_an_extra_key_is_dropped() {
        let mut store = store_at("both-halves");
        seed(&mut store);
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        already_carrying(&store, 0);

        let mine = here(&store);
        let mut held: Vec<(&str, &str)> =
            mine.iter().map(|key| (key.as_str(), "put")).collect();
        held[0].1 = "del";
        held.push(("task/9999", "put"));
        host.set_reply("/meta", meta(9));
        host.set_reply("/records?since=0&keys=1", page(&held, 9, false));
        host.set_reply("/records", took(9 + 2, 2));

        let Repaired::Placed { drift, .. } = repair_at(&store, true, Utc::now()).unwrap() else {
            panic!("a press that asks to place, places");
        };
        assert_eq!(drift, Drift { to_place: 1, to_drop: 1 });
        assert_eq!(
            queued(&store),
            Vec::<(String, String)>::new(),
            "what the server took is dropped from the queue",
        );
        let sent: Vec<serde_json::Value> = host
            .heard()
            .iter()
            .filter(|heard| heard.method == "PUT")
            .map(|heard| serde_json::from_slice(&heard.body).unwrap())
            .collect();
        let keys: Vec<(String, String)> = sent[0]["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|record| {
                (record["k"].as_str().unwrap().into(), record["op"].as_str().unwrap().into())
            })
            .collect();
        assert_eq!(
            keys,
            vec![(mine[0].clone(), "put".to_string()), ("task/9999".into(), "del".into())],
            "what is missing is placed, and what is left over is dropped",
        );
    }

    /// **Where the server stands is taken from the server.** A server written by something else stands
    /// somewhere this device's memory does not name, and every send after that fails the check the
    /// ordering is for — so a comparison that did not re-anchor it would report a drift it had just made
    /// permanent.
    #[test]
    fn the_ordering_is_re_anchored_to_what_the_server_said() {
        let mut store = store_at("re-anchor");
        seed(&mut store);
        already_carrying(&store, 2);
        let holds = Holding { keys: BTreeMap::new(), seq: 87 };

        let drift = compared_with(&store, &holds, true).unwrap();

        assert_eq!(drift.to_place, here(&store).len());
        assert_eq!(store.viewer_carried().unwrap().seq, 87);
    }

    /// **The cursor is not moved.** Nothing was read out of the feed, so nothing has been dealt with:
    /// what a comparison adds is beside the ordinary send's work, never instead of it.
    #[test]
    fn comparing_leaves_the_feed_where_it_was() {
        let mut store = store_at("cursor-stays");
        seed(&mut store);
        already_carrying(&store, 2);
        let was = store.viewer_carried().unwrap();

        compared_with(&store, &Holding { keys: BTreeMap::new(), seq: 87 }, true).unwrap();

        let now = store.viewer_carried().unwrap();
        assert_eq!((now.cursor, now.version, now.placed), (was.cursor, was.version, was.placed));
    }

    /// A server holding more keys than one page carries is read on from, page by page.
    #[test]
    fn what_the_server_holds_is_read_a_page_at_a_time() {
        let mut store = store_at("pages");
        seed(&mut store);
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        host.set_reply("/meta", meta(12));
        host.set_reply("/records?since=0&keys=1", page(&[("task/1", "put")], 7, true));
        host.set_reply("/records?since=7&keys=1", page(&[("task/2", "put")], 12, false));

        let server = Server::of_device(&store).unwrap().unwrap();
        let holds = what_it_holds(&server).expect("both pages are read");

        assert_eq!(holds.seq, 12, "where it stands is asked of /meta, not read off a page");
        assert_eq!(
            holds.keys.keys().cloned().collect::<Vec<_>>(),
            vec!["task/1".to_string(), "task/2".to_string()],
        );
    }

    /// A server that claims another page and does not move its ordering is one this would read forever.
    #[test]
    fn a_page_that_does_not_move_the_ordering_stops_the_reading() {
        let mut store = store_at("stuck-page");
        let host = StaticHost::serve(Vec::<(String, String)>::new());
        set_up(&mut store, &host);
        host.set_reply("/meta", meta(3));
        host.set_reply("/records?since=0&keys=1", page(&[("task/1", "put")], 0, true));

        let server = Server::of_device(&store).unwrap().unwrap();
        assert!(what_it_holds(&server).is_err());
    }

    /// What one press did reads as one shape with a word for which it was, so a caller printing it does
    /// not have to know the variants apart to say what happened.
    #[test]
    fn what_a_press_did_says_which_it_was() {
        assert_eq!(
            serde_json::to_value(Repaired::Counted(Drift { to_place: 2, to_drop: 1 })).unwrap(),
            serde_json::json!({ "outcome": "counted", "to_place": 2, "to_drop": 1 }),
        );
        assert_eq!(
            serde_json::to_value(Repaired::NotSetUp).unwrap(),
            serde_json::json!({ "outcome": "not_set_up" }),
        );
    }

    /// Standing a server up throws away what this device remembers of the last one — the count a repair
    /// showed included, since it says how far apart the two ends were.
    #[test]
    fn forgetting_the_carrier_takes_the_count_with_it() {
        let store = store_at("forget-the-count");
        store
            .set_viewer_asked(&Asked {
                asked_at: "2026-09-14T10:00:00Z".into(),
                drift: Drift { to_place: 2, to_drop: 1 },
            })
            .unwrap();

        store.forget_viewer_carried().unwrap();

        assert!(store.viewer_asked().unwrap().is_none());
    }
}
