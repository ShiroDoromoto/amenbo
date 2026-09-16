//! The host a test points Amenbo at when a path has to be *fetched*.
//!
//! Every route Amenbo takes to the network — a catalog, the key beside it, a detail document, an asset —
//! is reached by URL, and a file on disk cannot stand in for one being fetched. So a test that walks such
//! a route needs something answering on a port, and the four things it needs of it are always the same:
//! a port nobody else took, a body per path, a 404 for everything else, and **the ability to change what
//! one path serves while it is running** — which is what a publisher rotating a key looks like from the
//! outside.
//!
//! It is deliberately not a web server. There is no concurrency, no keep-alive and no content type, and
//! the method is recorded rather than routed on: connections are answered one at a time, in order, because
//! what is on the other end is Amenbo fetching one document and then the next. Test support only, never
//! linked into a shipped binary.
//!
//! Two things beyond a body come with the route, for the caller that is writing rather than fetching: a
//! [`Reply`] carries a status, so a refusal can be the answer under test, and [`StaticHost::heard`] hands
//! back every request — method, target and body — so a test can say what went out as well as what came
//! back. Answering the second is why the whole request is read before anything is written.
//!
//! It is a crate of its own rather than a helper inside one suite because the same host is wanted from
//! both sides of the tree: this workspace's tests reach it as a dev-dependency, and the pre-distribution
//! harness — its own cargo workspace, deliberately outside this one — by path, where a scenario's
//! premise stands one up behind a git remote that turns every request away and sends git looking for
//! a credential. A third hand-rolled listener is the thing it exists to stop.
//!
//! ```no_run
//! # use amenbo_static_host::StaticHost;
//! let host = StaticHost::serve([("/catalog.json", "{}")]);
//! let url = host.url("/catalog.json");        // http://127.0.0.1:<port>/catalog.json
//! host.set("/catalog.json", r#"{"v":2}"#);    // same port, a different answer
//! ```
//!
//! The host stops when it is dropped, so it has to be held for as long as the thing under test is
//! fetching from it — a `let` binding for the body of the test, not a temporary.

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// The most of a request head this reads — the line and the headers under it. A request whose head does
/// not fit is one no caller here makes; the body past it is read by what it declares, not by this.
const REQUEST_LIMIT: usize = 8192;

/// What one path answers with. A body alone is [`Reply::ok`], which is what `serve` and `set` write; a
/// status of its own is for the route whose refusal is the answer under test — an API that turns a caller
/// away, a door that says the credential is missing.
#[derive(Clone, Debug)]
pub struct Reply {
    pub status: u16,
    pub body: String,
    /// Headers written above the body, beside the ones every answer carries. They are here for the
    /// caller whose behaviour turns on one — a refusal naming the moment to come back at, a door saying
    /// which credential it wanted — since a body alone cannot say those things.
    pub headers: Vec<(String, String)>,
}

impl Reply {
    /// `200` with this body — what a path serves unless it was given a status.
    pub fn ok(body: impl Into<String>) -> Reply {
        Reply { status: 200, body: body.into(), headers: Vec::new() }
    }

    /// This status with this body.
    pub fn status(status: u16, body: impl Into<String>) -> Reply {
        Reply { status, body: body.into(), headers: Vec::new() }
    }

    /// The same answer with one more header on it.
    pub fn and_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Reply {
        self.headers.push((name.into(), value.into()));
        self
    }
}

/// One request the host was sent, kept so a test can say what went out rather than only what came back.
///
/// `target` is the request line's second word — the path **and its query**, exactly as the client wrote
/// it, because which of the two a caller put a name in is part of what is being tested.
#[derive(Clone, Debug)]
pub struct Heard {
    pub method: String,
    pub target: String,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// What every path answers with, shared with the thread answering on the port. A path holds a list
/// because a route may answer differently the second time it is asked (see [`StaticHost::set_replies`]).
type Routes = Arc<Mutex<Vec<(String, Vec<Reply>)>>>;

/// A loopback host serving fixed bodies by path (see the module docs).
pub struct StaticHost {
    port: u16,
    routes: Routes,
    heard: Arc<Mutex<Vec<Heard>>>,
    stop: Arc<AtomicBool>,
}

impl StaticHost {
    /// Start a host serving `routes` — pairs of path (`/catalog.json`, leading slash and all) and body —
    /// on a loopback port the OS hands out. It answers until it is dropped.
    ///
    /// A loopback that will not bind panics rather than coming back as an error every caller has to
    /// thread: a test that cannot reach the loopback has nothing left to say about what it was testing.
    pub fn serve<P: Into<String>, B: Into<String>>(
        routes: impl IntoIterator<Item = (P, B)>,
    ) -> StaticHost {
        let routes: Vec<(String, Vec<Reply>)> =
            routes.into_iter().map(|(p, b)| (p.into(), vec![Reply::ok(b)])).collect();
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("the bound address").port();
        let routes = Arc::new(Mutex::new(routes));
        let heard = Arc::new(Mutex::new(Vec::new()));
        let (served, kept, flag) =
            (routes.clone(), heard.clone(), Arc::new(AtomicBool::new(false)));
        let stop = flag.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                // The drop knocks on the port to get here, so the flag is read on the way in rather than
                // on the way out: what wakes this loop last is not a request to answer.
                if flag.load(Ordering::Relaxed) {
                    return;
                }
                let Ok(stream) = stream else { return };
                answer(stream, &served, &kept);
            }
        });
        StaticHost { port, routes, heard, stop }
    }

    /// The port it was given, for a caller that needs the address rather than a URL.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The URL `path` is served at — what the thing under test is pointed at.
    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    /// Replace what one path serves, the host keeping its port. A path it did not have is added, so this
    /// is also how something starts being published part-way through a test.
    pub fn set(&self, path: &str, body: &str) {
        self.set_reply(path, Reply::ok(body));
    }

    /// The same, for a path whose answer is not a `200` — the refusal a caller is meant to read.
    pub fn set_reply(&self, path: &str, reply: Reply) {
        self.set_replies(path, [reply]);
    }

    /// The same, for a path that does not answer the same thing twice — a store asked what tables it has
    /// and then what it has had, a name that is taken on the second look. Each request takes the next
    /// reply; the last one stays, so a path never runs out of answers.
    pub fn set_replies(&self, path: &str, replies: impl IntoIterator<Item = Reply>) {
        let replies: Vec<Reply> = replies.into_iter().collect();
        assert!(!replies.is_empty(), "a path answers with something");
        let mut routes = self.routes.lock().expect("the routes");
        routes.retain(|(p, _)| p != path);
        routes.push((path.to_string(), replies));
    }

    /// Every request the host has answered, in the order they arrived.
    ///
    /// What a test asks of this is the half a body cannot answer: that a call went out at all, that it
    /// carried the method and the target it should, and that what it uploaded is what was meant to go.
    pub fn heard(&self) -> Vec<Heard> {
        self.heard.lock().expect("what was heard").clone()
    }
}

/// Read one request, write one answer, and let the connection close. A path that is not served answers
/// 404 — which is how "this catalog publishes no key" is expressed, so it is an answer and not a failure.
///
/// **The whole request is read before anything is answered**, body included. A server that replies and
/// closes while the client is still writing hands it a broken pipe instead of a status, which passes on an
/// idle machine and fails on a loaded one. Reading it is also what lets [`StaticHost::heard`] say what
/// went out.
fn answer(stream: TcpStream, routes: &Routes, kept: &Mutex<Vec<Heard>>) {
    let mut stream = stream;
    let Some(request) = read_request(&mut stream) else { return };
    let reply = routes
        .lock()
        .expect("the routes")
        .iter_mut()
        .find(|(p, _)| *p == request.target)
        .map(|(_, replies)| {
            if replies.len() > 1 { replies.remove(0) } else { replies[0].clone() }
        });
    kept.lock().expect("what was heard").push(request);
    let reply = reply.unwrap_or_else(|| Reply::status(404, ""));
    let named = reply
        .headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect::<String>();
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\n{named}Connection: close\r\n\r\n{}",
        reply.status,
        reason(reply.status),
        reply.body.len(),
        reply.body,
    );
    let _ = stream.write_all(response.as_bytes());
}

/// Read the head, then as much body as it declares. A request with no `Content-Length` has no body here:
/// nothing this answers to sends one chunked.
fn read_request(stream: &mut TcpStream) -> Option<Heard> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > REQUEST_LIMIT {
            return None;
        }
        match stream.read(&mut byte) {
            Ok(0) | Err(_) => return None,
            Ok(_) => head.push(byte[0]),
        }
    }
    let head = String::from_utf8_lossy(&head).to_string();
    let mut lines = head.lines();
    let line = lines.next().unwrap_or_default();
    let mut words = line.split_whitespace();
    let method = words.next().unwrap_or("GET").to_string();
    let target = words.next().unwrap_or("/").to_string();

    let header = |name: &str| {
        let wanted = format!("{}:", name.to_ascii_lowercase());
        head.lines()
            .skip(1)
            .find_map(|l| l.to_ascii_lowercase().starts_with(&wanted).then(|| l[wanted.len()..].trim().to_string()))
    };
    let wants: usize = header("content-length").and_then(|n| n.parse().ok()).unwrap_or(0);
    let mut body = vec![0u8; wants];
    if wants > 0 && stream.read_exact(&mut body).is_err() {
        return None;
    }
    Some(Heard { method, target, content_type: header("content-type"), body })
}

/// The reason phrase beside the status. Clients read the number, so this only has to be there —
/// the handful this host is asked for are spelled, and anything else says so.
fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

impl Drop for StaticHost {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Unblock the accept loop so the thread sees the flag and returns, rather than sitting on the
        // port until the process ends.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bare GET, so the tests need no HTTP client: the host answers one request per connection and
    /// closes, which is exactly what reading to the end means here.
    fn get(host: &StaticHost, path: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", host.port())).expect("the host answers");
        stream
            .write_all(format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_bytes())
            .expect("the request goes out");
        let mut answer = String::new();
        stream.read_to_string(&mut answer).expect("the answer comes back");
        answer
    }

    #[test]
    fn serves_a_body_per_path_and_404s_the_rest() {
        let host = StaticHost::serve([("/catalog.json", "{\"catalog_v\":1}")]);
        assert!(get(&host, "/catalog.json").ends_with("{\"catalog_v\":1}"));
        assert!(get(&host, "/catalog.json").starts_with("HTTP/1.1 200 OK"));
        // Not "the host is down": a path nobody publishes is the answer some of these tests are about.
        assert!(get(&host, "/catalog-key.pub").starts_with("HTTP/1.1 404"));
    }

    #[test]
    fn one_path_changes_under_a_running_host() {
        let host = StaticHost::serve([("/catalog-key.pub", "the first key")]);
        assert!(get(&host, "/catalog-key.pub").ends_with("the first key"));
        // The port does not move, because a publisher rotating a key does not move either.
        host.set("/catalog-key.pub", "a different key");
        assert!(get(&host, "/catalog-key.pub").ends_with("a different key"));
        // A path it did not have is added the same way, so a document can start being published
        // part-way through a test.
        host.set("/catalog.json", "{}");
        assert!(get(&host, "/catalog.json").ends_with("{}"));
    }

    #[test]
    fn two_hosts_take_two_ports() {
        let (a, b) = (StaticHost::serve([("/a", "a")]), StaticHost::serve([("/b", "b")]));
        assert_ne!(a.port(), b.port(), "each host binds its own port, so tests can run side by side");
        assert!(a.url("/a").starts_with(&format!("http://127.0.0.1:{}", a.port())));
    }
}
