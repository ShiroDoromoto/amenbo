//! The single observation-hook **dispatcher** — the wiring that turns committed lifecycle events into
//! fired plugin hooks (`AMB-D-367`).
//!
//! The pieces this joins already exist: ops write points append semantic events to the transactional
//! [`outbox`](crate::store_engine::outbox) (`AMB-D-367`); [`Payload`] is the wire shape a plugin receives
//! (`AMB-D-348`); [`plugin_hooks::fire`](crate::plugin_hooks::fire) launches each hook fire-and-forget,
//! under a timeout, warning on anything but a clean exit (`AMB-D-352`). What was missing is the seam
//! between them — draining the outbox and, for each event, handing the right plugins their payload. That
//! is [`deliver`].
//!
//! **Stateless, cursor owned by the caller** (`AMB-D-367`). Like [`events_since`], this holds no cursor of
//! its own: [`deliver`] takes the caller's cursor and returns the one to store next. The single dispatcher
//! keeps *one* cursor — a short-lived CLI persists it in the store between runs, a long-lived GUI holds it
//! in memory — and that mounting, together with the persistence, is the caller's (`AMB-T-1975`). Keeping
//! the delivery function pure is what lets both faces drive it the same way.
//!
//! **Who subscribes is a seam.** [`deliver`] asks a [`Subscribers`] which plugins observe an event and
//! composes each one's stdin — the event payload, with that plugin's non-secret config folded under
//! `config` (`AMB-D-356`); it does not itself know what is installed or enabled. The real resolver is
//! [`EnabledSubscribers`](crate::plugin_subscribe::EnabledSubscribers), which the install≠enable lifecycle
//! supplies (`AMB-T-2032`): only an *enabled*, subscribed plugin fires (`AMB-D-351`). Where no resolver is
//! mounted yet, [`NoSubscribers`] is the honest default — nothing is installed, so no event has an
//! observer, and delivery is a no-op that still advances the cursor.
//!
//! **Delivery is best-effort** (`AMB-D-352`). Generation is leak-free (the event landed in the same
//! transaction as its cause), but firing is after the fact: a hook that will not spawn, exits non-zero, or
//! overruns its timeout is a warning and nothing more. And if retention (`AMB-T-2021`) has trimmed past the
//! caller's cursor, the lost span cannot be replayed — [`deliver`] resyncs the cursor to the outbox head
//! and reports [`Delivered::gapped`] rather than pretend nothing fired.

use std::thread::JoinHandle;

use rusqlite::Connection;

use crate::error::Result;
use crate::plugin_exec::PluginInvocation;
use crate::plugin_hooks::Hook;
use crate::plugin_payload::Payload;
use crate::store_engine::{events_since, outbox_head, OutboxSlice};

/// How many events one [`deliver`] call drains per page. A dispatcher fired after each write sees one
/// event at a time; this only bounds a catch-up drain after downtime, so it is generous — the page cost is
/// one query, not one process.
const DELIVER_PAGE: i64 = 256;

/// One resolved subscriber: the plugin to fire and its non-secret config to hand it alongside the event.
///
/// The resolver builds the `invocation` with its program and the plugin's **secret** config as environment
/// variables (`AMB-T-2016`, off argv and off logs), and carries the **text** (non-secret) config as
/// `config` — a JSON object [`deliver`] places under the payload's `config` key on stdin. The split is the
/// author's `secret` flag's (`AMB-D-356`), resolved by [`plugin_inject`](crate::plugin_inject). The
/// resolver does **not** set the payload event fields; [`deliver`] composes the whole stdin document, so
/// the payload channel stays the dispatcher's.
pub struct Subscriber {
    /// The plugin's name, as the installed registry knows it. Carried alongside the invocation because a
    /// program path is not a name: it is what [`deliver`] puts on the [`Hook`] so a warning or an
    /// execution log can say which plugin ran.
    pub plugin: String,
    /// The plugin to run, with its program and secret env already set. Its stdin is left for [`deliver`].
    pub invocation: PluginInvocation,
    /// The plugin's non-secret config, placed under the payload's `config` key on stdin. Empty when the
    /// plugin has no text settings — [`deliver`] then adds no `config` key at all.
    pub config: serde_json::Map<String, serde_json::Value>,
}

impl Subscriber {
    /// A named subscriber with no text config — an invocation whose stdin is entirely [`deliver`]'s
    /// payload.
    pub fn new(plugin: impl Into<String>, invocation: PluginInvocation) -> Self {
        Self { plugin: plugin.into(), invocation, config: serde_json::Map::new() }
    }
}

/// Resolves which plugins observe an event — the seam the enable lifecycle fills (`AMB-T-1975`).
///
/// Given an event name (one of [`crate::plugin_payload::V1_EVENTS`]), return one [`Subscriber`] per
/// enabled, subscribed plugin: its program, whatever the resolver injects alongside (secret config as env
/// vars, `AMB-T-2016`), and its non-secret config for the payload's `config` key. The resolver does **not**
/// set the payload event fields — [`deliver`] composes the stdin document, so the payload channel stays the
/// dispatcher's. Return an empty vector for an event nobody observes.
pub trait Subscribers {
    /// The subscribers to fire for `event`, before the event payload is composed onto stdin.
    fn resolve(&self, event: &str) -> Vec<Subscriber>;
}

/// The default resolver until the enable lifecycle supplies a real one (`AMB-T-1975`): nothing is
/// installed or enabled, so no event has a subscriber. Delivery under it fires nothing but still walks the
/// cursor, so a plugin enabled later observes what fires *next*, not the whole backlog.
pub struct NoSubscribers;

impl Subscribers for NoSubscribers {
    fn resolve(&self, _event: &str) -> Vec<Subscriber> {
        Vec::new()
    }
}

/// The result of one [`deliver`] pass: how far the cursor advanced, the hooks it launched, and whether it
/// hit a retention gap.
///
/// The `hooks` are the launched threads — **join them before a short-lived process exits** (so the hooks
/// it started are not cut short), or **drop them to forget** (the true fire-and-forget a long-lived GUI
/// wants). That choice is the caller's, exactly as [`plugin_hooks::fire`](crate::plugin_hooks::fire)
/// hands it over.
#[must_use = "advance the stored cursor to `cursor`, and join or drop `hooks`"]
pub struct Delivered {
    /// The cursor to store for the next pass — the id of the last event drained, or the outbox head when a
    /// gap forced a resync. Never moves backwards.
    pub cursor: i64,
    /// The launched hook threads (one per fired invocation). Join before exiting a short-lived process;
    /// drop to forget.
    pub hooks: Vec<JoinHandle<()>>,
    /// Retention had trimmed past the caller's cursor: the span between it and the head is lost and was
    /// not fired. The cursor is resynced to the head. A caller may log this (`AMB-D-361`); delivery being
    /// best-effort, it is not an error (`AMB-D-352`).
    pub gapped: bool,
}

/// Drain every event past `cursor`, fire each one's subscribers, and report how far the cursor advanced.
///
/// Pure over its cursor (`AMB-D-367`): the caller passes the cursor it stored and stores back
/// [`Delivered::cursor`]; this holds none of its own. For each drained event it rebuilds the [`Payload`]
/// ([`Payload::from_outbox_row`]), asks `subs` which plugins observe it, attaches the payload to each on
/// stdin, and launches them all through [`plugin_hooks::fire`](crate::plugin_hooks::fire). A row this
/// amenbo does not recognise (an event outside the v1 catalog, an unparseable actor/time) is warned about
/// and skipped, and the cursor still walks past it. On a retention gap the cursor is resynced to the head
/// and nothing is fired for the lost span (see [`Delivered::gapped`]).
pub fn deliver(conn: &Connection, cursor: i64, subs: &dyn Subscribers) -> Result<Delivered> {
    let mut cursor = cursor;
    let mut hooks: Vec<Hook> = Vec::new();
    loop {
        match events_since(conn, cursor, DELIVER_PAGE)? {
            OutboxSlice::Gap => {
                // Retention passed the cursor; the lost events cannot be replayed. Resync to the head and
                // fire nothing for the gap — delivery is best-effort (`AMB-D-352`). A gap can only surface
                // on the first page (the cursor only ever moves forward), so nothing has been built yet.
                return Ok(Delivered { cursor: outbox_head(conn)?, hooks: Vec::new(), gapped: true });
            }
            OutboxSlice::Events { rows, more } => {
                for row in &rows {
                    cursor = row.id;
                    let Some(payload) = Payload::from_outbox_row(row) else {
                        tracing::warn!(
                            event = %row.event,
                            id = row.record_id,
                            "unrecognised plugin outbox event; skipped"
                        );
                        continue;
                    };
                    // Serialize the event once per row; each subscriber's stdin is this payload with the
                    // plugin's own text config merged under `config` (`AMB-D-356` — the payload's config
                    // key). The base payload is a JSON object (`Payload` serializes to a map), so the merge
                    // is a key insertion, not a re-parse.
                    let base = serde_json::to_value(&payload)?;
                    for sub in subs.resolve(payload.event) {
                        let json = serde_json::to_string(&with_config(base.clone(), sub.config))?;
                        // The event is kept beside the plugin's name here, where both are still known:
                        // a page of events folds into one list of hooks, and the runner below has no
                        // other way back to which row fired which plugin.
                        hooks.push(Hook::new(
                            sub.plugin,
                            payload.event,
                            sub.invocation.stdin_json(json),
                        ));
                    }
                }
                if !more {
                    break;
                }
            }
        }
    }
    let hooks = crate::plugin_hooks::fire(hooks);
    Ok(Delivered { cursor, hooks, gapped: false })
}

/// Fold a subscriber's non-secret config into the event payload under `config` (`AMB-D-356`). An empty
/// config adds no key, so a plugin with no text settings receives the bare event payload — the absent and
/// the empty forms stay one wire document, as elsewhere in the plugin contract.
fn with_config(
    mut payload: serde_json::Value,
    config: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    if !config.is_empty() {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("config".to_string(), serde_json::Value::Object(config));
        }
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_engine::{outbox::EventRow, StoreEngine};

    /// A resolver that fires one fixed invocation for each of the named events, and nothing for the rest.
    struct Fixed {
        events: Vec<&'static str>,
        invocation: PluginInvocation,
    }

    impl Subscribers for Fixed {
        fn resolve(&self, event: &str) -> Vec<Subscriber> {
            if self.events.contains(&event) {
                vec![Subscriber::new("fixed", self.invocation.clone())]
            } else {
                Vec::new()
            }
        }
    }

    /// An invocation whose program does not exist: `fire` spawns a thread, the spawn fails and warns, and
    /// the thread ends at once — enough to count fires without depending on a real subprocess.
    fn bogus() -> PluginInvocation {
        PluginInvocation::new("/nonexistent/amenbo-dispatch-test-plugin")
    }

    fn emit(e: &StoreEngine, event: &str, id: i64, new: Option<&str>) {
        let tx = e.write().unwrap();
        tx.emit_event(&EventRow { event, record_id: id, actor: "ai", at: "2026-07-22T09:00:00Z", new_state: new })
            .unwrap();
        tx.commit().unwrap();
    }

    /// With no subscriber, delivery fires nothing but still walks the cursor to the head — so a plugin
    /// enabled later starts from what fires next, not the whole backlog.
    #[test]
    fn no_subscriber_advances_the_cursor_and_fires_nothing() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1, None);
        emit(&e, "task.status_changed", 2, Some("in_progress"));

        let d = deliver(e.conn(), 0, &NoSubscribers).unwrap();
        assert_eq!(d.cursor, 2, "the cursor walks to the head even with nobody listening");
        assert!(d.hooks.is_empty(), "no subscriber, no fire");
        assert!(!d.gapped);
    }

    /// One hook fires per subscribed event; an event nobody subscribed to fires nothing.
    #[test]
    fn fires_one_hook_per_subscribed_event() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1, None);
        emit(&e, "task.status_changed", 2, Some("in_progress"));
        emit(&e, "task.status_changed", 3, Some("blocked"));

        let subs = Fixed { events: vec!["task.status_changed"], invocation: bogus() };
        let d = deliver(e.conn(), 0, &subs).unwrap();
        assert_eq!(d.cursor, 3);
        assert_eq!(d.hooks.len(), 2, "two status_changed events fire, the creation does not");
        for h in d.hooks {
            h.join().unwrap();
        }
    }

    /// Delivering again from the returned cursor fires nothing — a committed event is delivered once.
    #[test]
    fn does_not_refire_from_the_returned_cursor() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.status_changed", 1, Some("in_progress"));
        let subs = Fixed { events: vec!["task.status_changed"], invocation: bogus() };

        let first = deliver(e.conn(), 0, &subs).unwrap();
        assert_eq!(first.hooks.len(), 1);
        for h in first.hooks {
            h.join().unwrap();
        }

        let second = deliver(e.conn(), first.cursor, &subs).unwrap();
        assert_eq!(second.cursor, first.cursor, "nothing new, so the cursor holds");
        assert!(second.hooks.is_empty(), "the event already fired once");
    }

    /// A row whose event is outside the v1 catalog is warned about and skipped, but the cursor still walks
    /// past it so it is never revisited.
    #[test]
    fn an_unrecognised_event_is_skipped_and_the_cursor_advances() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1, None);
        emit(&e, "task.exploded", 2, None); // not a v1 event

        let subs = Fixed { events: vec!["task.created", "task.exploded"], invocation: bogus() };
        let d = deliver(e.conn(), 0, &subs).unwrap();
        assert_eq!(d.cursor, 2, "the cursor walks past the unrecognised row");
        assert_eq!(d.hooks.len(), 1, "only the recognised event resolved a subscriber");
        for h in d.hooks {
            h.join().unwrap();
        }
    }

    /// A cursor behind the retention watermark is a gap: nothing is fired for the lost span and the cursor
    /// is resynced to the head.
    #[test]
    fn a_retention_gap_resyncs_the_cursor_and_fires_nothing() {
        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.created", 1, None);
        emit(&e, "task.created", 2, None);
        // Pretend retention trimmed through id 1.
        let tx = e.write().unwrap();
        tx.set_meta(crate::store_engine::outbox::META_OUTBOX_TRUNCATED_THROUGH, Some("1")).unwrap();
        tx.commit().unwrap();

        let subs = Fixed { events: vec!["task.created"], invocation: bogus() };
        let d = deliver(e.conn(), 0, &subs).unwrap();
        assert!(d.gapped, "a cursor behind the watermark is a gap");
        assert_eq!(d.cursor, 2, "the cursor resyncs to the head");
        assert!(d.hooks.is_empty(), "the lost span is not replayed");
    }

    /// `with_config` folds a plugin's text config under `config`, and adds no key when the config is empty
    /// so a plugin with no text settings still receives the bare event payload.
    #[test]
    fn with_config_merges_under_the_config_key_only_when_present() {
        let base = serde_json::json!({ "v": 1, "event": "task.created", "id": 7 });

        // Empty config: the payload is untouched, no `config` key appears.
        let bare = with_config(base.clone(), serde_json::Map::new());
        assert!(bare.get("config").is_none(), "an empty config adds no key");
        assert_eq!(bare, base);

        // Non-empty config: it lands under `config`, the event fields untouched.
        let mut cfg = serde_json::Map::new();
        cfg.insert("channel".to_string(), serde_json::Value::String("#ops".to_string()));
        let merged = with_config(base, cfg);
        assert_eq!(merged["config"]["channel"], "#ops");
        assert_eq!(merged["event"], "task.created", "the event fields are left alone");
    }

    /// The whole chain end to end: a committed event's payload reaches the subscribed plugin on stdin, in
    /// the v1 wire shape, with the plugin's text config folded under `config`. A real subprocess needs a
    /// shell, so this is unix-only (the same gate the exec and command round-trip tests use).
    #[cfg(unix)]
    #[test]
    fn the_event_payload_and_config_reach_the_plugin_on_stdin() {
        /// A resolver that fires one invocation with a fixed text config for the named event.
        struct WithConfig {
            event: &'static str,
            invocation: PluginInvocation,
            config: serde_json::Map<String, serde_json::Value>,
        }
        impl Subscribers for WithConfig {
            fn resolve(&self, event: &str) -> Vec<Subscriber> {
                if event == self.event {
                    vec![Subscriber { plugin: "configured".into(), invocation: self.invocation.clone(), config: self.config.clone() }]
                } else {
                    Vec::new()
                }
            }
        }

        let dir = amenbo_scratch::scratch("plugin-dispatch-config-stdin");
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("payload.json");
        let _ = std::fs::remove_file(&out);

        let invocation = PluginInvocation::new("/bin/sh")
            .arg("-c")
            .arg(format!("cat > {}", out.display()));
        let mut config = serde_json::Map::new();
        config.insert("channel".to_string(), serde_json::Value::String("#ops".to_string()));
        let subs = WithConfig { event: "task.status_changed", invocation, config };

        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.status_changed", 42, Some("in_progress"));

        let d = deliver(e.conn(), 0, &subs).unwrap();
        assert_eq!(d.hooks.len(), 1);
        for h in d.hooks {
            h.join().unwrap();
        }

        let got: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(got["event"], "task.status_changed");
        assert_eq!(got["new"], "in_progress");
        // The plugin's text config rides the same stdin document, under `config`.
        assert_eq!(got["config"]["channel"], "#ops");
    }

    /// The whole chain end to end: a committed event's payload reaches the subscribed plugin on stdin, in
    /// the v1 wire shape. A real subprocess needs a shell, so this is unix-only (the same gate the exec and
    /// command round-trip tests use).
    #[cfg(unix)]
    #[test]
    fn the_event_payload_reaches_the_plugin_on_stdin() {
        let dir = amenbo_scratch::scratch("plugin-dispatch-stdin");
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("payload.json");
        let _ = std::fs::remove_file(&out);

        let invocation = PluginInvocation::new("/bin/sh")
            .arg("-c")
            .arg(format!("cat > {}", out.display()));
        let subs = Fixed { events: vec!["task.status_changed"], invocation };

        let e = StoreEngine::open_in_memory().unwrap();
        emit(&e, "task.status_changed", 42, Some("in_progress"));

        let d = deliver(e.conn(), 0, &subs).unwrap();
        assert_eq!(d.hooks.len(), 1);
        for h in d.hooks {
            h.join().unwrap();
        }

        let got: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(got["v"], 1);
        assert_eq!(got["event"], "task.status_changed");
        assert_eq!(got["id"], 42);
        assert_eq!(got["actor"], "ai");
        assert_eq!(got["at"], "2026-07-22T09:00:00Z");
        assert_eq!(got["new"], "in_progress");
    }
}
