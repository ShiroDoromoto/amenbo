//! The Viewer's road end to end: this build's sender against the repository's own Worker (`worker/`),
//! run on this machine by `wrangler dev --local`, read back the way the phone reads it.
//!
//! **Nothing here is a stand-in for either end.** The sender is `viewer::send::carry_at` as it ships, and
//! the Worker is `worker/src/index.ts` over a local D1 with the migrations applied — no Cloudflare account
//! is touched. The phone is not the app, but it reads as the app does: `GET /records` from its cursor with
//! the read code, and on `409` it throws what it holds away and reads again from `since=0`.
//!
//! One store and one Worker carry four things through:
//!
//! - **the send** — what the store holds lands, sealed, where the phone reads it;
//! - **finding a run's leftover rows** — `automation_run*` keys an older build placed are seen through
//!   `keys=1`;
//! - **deleting them** — they go through `DELETE /records`, the headstone with them;
//! - **asking the phones to read again** — a phone reading on from before the delete is answered `409`,
//!   reads from the beginning, and finds the rows gone.
//!
//! **A machine without wrangler skips it rather than failing.** `worker/node_modules/.bin/wrangler`
//! (after `npm ci` in `worker/`) or a `wrangler` on `PATH` is what it looks for.

use std::collections::BTreeMap;
use std::fs::File;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use amenbo_core::config::Paths;
use amenbo_core::model::SecretArea;
use amenbo_core::viewer::{self, pairing, send};
use amenbo_core::Store;
use base64::Engine as _;
use chrono::Utc;

/// What the Worker is given as its write token. It guards a database that lives for one test.
const WRITE_TOKEN: &str = "a-write-token-for-the-through-test";

/// How long the Worker is given to start answering. A first run bundles the script and starts workerd.
const STARTUP: Duration = Duration::from_secs(90);

/// The tables the Viewer is never sent, which an older build did send. One of each kind of row: a
/// record, and a headstone.
const LEFT_BEHIND: &[(&str, &str)] = &[
    ("automation_run/1", "put"),
    ("automation_run_step/2", "del"),
];

#[test]
fn a_store_reaches_the_phone_through_the_worker_and_a_runs_rows_leave_it() {
    let Some(wrangler) = wrangler() else {
        eprintln!(
            "skipping: no wrangler here — run `npm ci` in worker/ or put wrangler on PATH to run this"
        );
        return;
    };
    let base = amenbo_scratch::scratch("viewer-worker-through");
    std::fs::create_dir_all(&base).unwrap();
    let worker = Worker::start(&wrangler, &base.join("worker"));

    let mut store = Store::open_at(Paths::at(base.join("store"))).unwrap();
    set_up(&mut store, &worker.url);
    store.set_viewer_switched_on(true).unwrap();
    let first = seed(&mut store, "something to read");

    // The phone is paired the way the PC pairs one: the code it reads carries the read token.
    let issued = pairing::issue(&store)
        .unwrap()
        .expect("the device has a server set up");
    let code: serde_json::Value = serde_json::from_str(&issued.carried).unwrap();
    let mut phone = Phone::new(&worker.url, code["t"].as_str().unwrap());
    let spec_v = code["v"].as_i64().unwrap();

    // What a build that still sent a run's rows left behind, put there with the write token.
    let records: Vec<serde_json::Value> = LEFT_BEHIND
        .iter()
        .map(|(k, op)| match *op {
            "put" => serde_json::json!({ "k": k, "op": op, "n": "bm9uY2U", "c": "c2VhbGVk" }),
            _ => serde_json::json!({ "k": k, "op": op }),
        })
        .collect();
    let (status, _) = worker.call(
        "PUT",
        "/records",
        WRITE_TOKEN,
        Some(serde_json::json!({ "spec_v": spec_v, "version": 0, "records": records })),
    );
    assert_eq!(status, 200, "the Worker took the rows an older build left");

    // The phone reads them before anything is deleted, which leaves it a cursor from before the delete.
    assert!(
        !phone.catch_up(),
        "a phone reading from nothing is not told to read again"
    );
    assert!(
        phone.holds("automation_run/1"),
        "the phone was holding the run's row"
    );
    let before = phone.cursor;
    assert!(before > 0);

    // One turn of the sender: the send, the looking, and the deleting.
    let sent = send::carry_at(&store, Utc::now()).unwrap();
    assert_eq!(sent.held_back, None, "{sent:?}");
    assert!(sent.placed > 0, "the turn placed the store: {sent:?}");
    assert_eq!(sent.waiting, 0, "and nothing is left waiting: {sent:?}");

    let left = store.viewer_carried().unwrap();
    assert_eq!(
        left.strays,
        Some(true),
        "the turn found the run's rows: {left:?}"
    );
    assert!(
        left.strays_gone_at.is_some(),
        "and finished deleting them: {left:?}"
    );
    assert_eq!(
        left.build,
        viewer::WORKER_BUILD as i64,
        "the Worker answered as this build's"
    );

    // The server's keys, read as the sender reads them: the store is there, the run's rows are not.
    let keys = worker.keys();
    let task = format!("task/{first}");
    assert!(
        keys.contains_key(&task),
        "the task reached the server: {keys:?}"
    );
    for (k, _) in LEFT_BEHIND {
        assert!(
            !keys.contains_key(*k),
            "{k} left the server, headstone or not: {keys:?}"
        );
    }

    // The phone reading on from before the delete is told to read again, and does.
    let (status, _) = phone.read(before);
    assert_eq!(status, 409, "a cursor from before the delete is refused");
    assert!(phone.catch_up(), "the phone was asked to read again");
    let sealed = phone
        .held
        .get(&task)
        .expect("the phone reads the task after reading again");
    assert!(!sealed.is_empty(), "the task reached the phone sealed");
    assert!(
        !sealed.contains("something to read"),
        "and not in the clear"
    );
    for (k, _) in LEFT_BEHIND {
        assert!(
            !phone.holds(k),
            "{k} is not on the phone after reading again"
        );
    }

    // Past the delete, the phone reads on as it always does.
    let second = seed(&mut store, "something written later");
    let sent = send::carry_at(&store, Utc::now()).unwrap();
    assert!(sent.placed > 0, "{sent:?}");
    assert!(
        !phone.catch_up(),
        "a phone level with the delete reads on without being refused"
    );
    assert!(
        phone.holds(&format!("task/{second}")),
        "and reads what was written since"
    );
    assert!(phone.holds(&task), "keeping what it already had");
}

/// The wrangler to run the Worker with: the one `npm ci` put in `worker/`, or one on `PATH`.
fn wrangler() -> Option<PathBuf> {
    let installed = worker_dir().join("node_modules").join(".bin");
    let on_path = amenbo_core::env::path()
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    std::iter::once(installed)
        .chain(on_path)
        .flat_map(|dir| ["wrangler", "wrangler.cmd"].map(|name| dir.join(name)))
        .find(|candidate| candidate.is_file())
}

/// The Worker's source in this repository.
fn worker_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../worker")
}

/// The repository's Worker, answering on a port of its own over a database of its own.
struct Worker {
    child: Child,
    url: String,
    agent: ureq::Agent,
}

impl Worker {
    /// Apply the migrations to a fresh local D1 under `state`, start `wrangler dev --local` over it, and
    /// wait until `/meta` answers.
    fn start(wrangler: &Path, state: &Path) -> Worker {
        std::fs::create_dir_all(state).unwrap();
        let persist = state.join("d1");
        let log = state.join("wrangler.log");

        let applied = wrangler_command(wrangler, &log)
            .args([
                "d1",
                "migrations",
                "apply",
                "amenbo-viewer",
                "--local",
                "--persist-to",
            ])
            .arg(&persist)
            .status()
            .expect("wrangler starts");
        assert!(
            applied.success(),
            "the migrations did not apply:\n{}",
            tail(&log)
        );

        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut command = wrangler_command(wrangler, &log);
        command
            .args([
                "dev",
                "--local",
                "--ip",
                "127.0.0.1",
                "--port",
                &port.to_string(),
            ])
            .args(["--show-interactive-dev-session=false", "--persist-to"])
            .arg(&persist)
            .args(["--var", &format!("WRITE_TOKEN:{WRITE_TOKEN}")]);
        // wrangler starts workerd under it, so the whole group is what is stopped at the end.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            command.process_group(0);
        }
        let child = command.spawn().expect("wrangler dev starts");
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();
        let mut worker = Worker {
            child,
            url: format!("http://127.0.0.1:{port}"),
            agent,
        };

        let until = Instant::now() + STARTUP;
        loop {
            if let Some(ended) = worker.child.try_wait().unwrap() {
                panic!(
                    "wrangler dev ended ({ended}) before answering:\n{}",
                    tail(&log)
                );
            }
            let answered = worker
                .agent
                .get(format!("{}/meta", worker.url))
                .header("authorization", format!("Bearer {WRITE_TOKEN}"))
                .call();
            if answered.is_ok_and(|answer| answer.status() == 200) {
                return worker;
            }
            assert!(
                Instant::now() < until,
                "the Worker did not answer /meta in time:\n{}",
                tail(&log)
            );
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// Call the Worker and hand back the status and the body read as JSON (`null` where it was not).
    fn call(
        &self,
        method: &str,
        path: &str,
        token: &str,
        body: Option<serde_json::Value>,
    ) -> (u16, serde_json::Value) {
        call(
            &self.agent,
            method,
            &format!("{}{path}", self.url),
            token,
            body,
        )
    }

    /// Every key the server holds and its last word, read with the write token as the sender reads them.
    fn keys(&self) -> BTreeMap<String, String> {
        let mut keys = BTreeMap::new();
        let mut since = 0;
        loop {
            let (status, page) = self.call(
                "GET",
                &format!("/records?since={since}&keys=1"),
                WRITE_TOKEN,
                None,
            );
            assert_eq!(status, 200, "{page}");
            for record in page["records"].as_array().unwrap() {
                keys.insert(
                    record["k"].as_str().unwrap().to_string(),
                    record["op"].as_str().unwrap().to_string(),
                );
            }
            since = page["seq"].as_i64().unwrap();
            if !page["more"].as_bool().unwrap() {
                return keys;
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            // The group's id is wrangler's own, as it was started at the head of one.
            let group = -(self.child.id() as libc::pid_t);
            signal(group, libc::SIGTERM);
            let until = Instant::now() + Duration::from_secs(5);
            while Instant::now() < until && !matches!(self.child.try_wait(), Ok(Some(_))) {
                std::thread::sleep(Duration::from_millis(100));
            }
            // Whatever in the group did not stop on being asked is stopped now — workerd among them.
            signal(group, libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Send a signal to a process group.
#[cfg(unix)]
fn signal(group: libc::pid_t, sig: libc::c_int) {
    // SAFETY: kill(2) takes two integers and touches no memory of this process.
    unsafe {
        libc::kill(group, sig);
    }
}

/// wrangler run in `worker/`, answering no prompts and writing what it says to `log`.
fn wrangler_command(wrangler: &Path, log: &Path) -> Command {
    let out = File::options().create(true).append(true).open(log).unwrap();
    let mut command =
        amenbo_scratch::command(wrangler.to_str().expect("the path to wrangler is UTF-8"));
    command
        .current_dir(worker_dir())
        .env("CI", "true")
        .env("WRANGLER_SEND_METRICS", "false")
        .stdin(Stdio::null())
        .stdout(out.try_clone().unwrap())
        .stderr(out);
    command
}

/// The end of what wrangler said, for a failure to carry.
fn tail(log: &Path) -> String {
    let said = std::fs::read_to_string(log).unwrap_or_default();
    let lines: Vec<&str> = said.lines().collect();
    lines[lines.len().saturating_sub(40)..].join("\n")
}

/// One call, with the status handed back rather than raised.
fn call(
    agent: &ureq::Agent,
    method: &str,
    url: &str,
    token: &str,
    body: Option<serde_json::Value>,
) -> (u16, serde_json::Value) {
    let request = ureq::http::Request::builder()
        .method(method)
        .uri(url)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(
            body.map(|body| body.to_string().into_bytes())
                .unwrap_or_default(),
        )
        .unwrap();
    let mut answer = agent.run(request).unwrap();
    let status = answer.status().as_u16();
    let said = answer.body_mut().read_to_string().unwrap();
    (
        status,
        serde_json::from_str(&said).unwrap_or(serde_json::Value::Null),
    )
}

/// A phone reading the way the app reads: from its cursor with the read code, and from the beginning
/// when it is refused.
struct Phone {
    url: String,
    token: String,
    agent: ureq::Agent,
    cursor: i64,
    /// What it holds, by key: the sealed row. A headstone takes the key away.
    held: BTreeMap<String, String>,
}

impl Phone {
    fn new(url: &str, token: &str) -> Phone {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .into();
        Phone {
            url: url.to_string(),
            token: token.to_string(),
            agent,
            cursor: 0,
            held: BTreeMap::new(),
        }
    }

    /// One page from `since`.
    fn read(&self, since: i64) -> (u16, serde_json::Value) {
        call(
            &self.agent,
            "GET",
            &format!("{}/records?since={since}", self.url),
            &self.token,
            None,
        )
    }

    /// Read on until level, and answer whether the server asked for everything to be read again.
    fn catch_up(&mut self) -> bool {
        let mut told_to_reread = false;
        loop {
            let (status, page) = self.read(self.cursor);
            if status == 409 {
                assert!(
                    !told_to_reread,
                    "refused again after reading from the beginning: {page}"
                );
                told_to_reread = true;
                self.held.clear();
                self.cursor = 0;
                continue;
            }
            assert_eq!(status, 200, "{page}");
            for record in page["records"].as_array().unwrap() {
                let k = record["k"].as_str().unwrap().to_string();
                match record["op"].as_str().unwrap() {
                    "del" => {
                        self.held.remove(&k);
                    }
                    _ => {
                        assert!(
                            !record["n"].as_str().unwrap().is_empty(),
                            "{k} carries a nonce"
                        );
                        self.held
                            .insert(k, record["c"].as_str().unwrap().to_string());
                    }
                }
            }
            self.cursor = page["seq"].as_i64().unwrap();
            if !page["more"].as_bool().unwrap() {
                return told_to_reread;
            }
        }
    }

    fn holds(&self, k: &str) -> bool {
        self.held.contains_key(k)
    }
}

/// Leave behind what setup leaves behind, pointed at the local Worker.
fn set_up(store: &mut Store, url: &str) {
    let key = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([7u8; 32]);
    for (field, value) in [
        (viewer::WORKER_URL, url.to_string()),
        (viewer::AUTH_TOKEN, WRITE_TOKEN.to_string()),
        (viewer::ENCRYPTION_KEY, key),
    ] {
        store
            .set_secret(None, SecretArea::Viewer, None, field, Some(&value))
            .unwrap();
    }
}

/// A project and a task in it, answering the task's id.
fn seed(store: &mut Store, title: &str) -> i64 {
    let project = store
        .project_add(amenbo_core::ops::project::NewProject {
            name: format!("carried {title}"),
            view: amenbo_core::model::View::List,
            notes: String::new(),
            color: None,
        })
        .unwrap()
        .id;
    store
        .add_task(amenbo_core::ops::task::NewTask {
            title: title.into(),
            project_id: Some(project),
            due: None,
            start: None,
            priority: None,
            notes: String::new(),
            created_by_kind: None,
            at_binding_id: None,
            role_id: None,
            made_in_run_step_id: None,
            made_in: None,
        })
        .unwrap()
        .id
}
