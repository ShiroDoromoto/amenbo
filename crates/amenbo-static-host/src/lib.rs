//! The host a test points amenbo at when a path has to be *fetched*.
//!
//! Every route amenbo takes to the network — a catalog, the key beside it, a detail document, an asset —
//! is reached by URL, and a file on disk cannot stand in for one being fetched. So a test that walks such
//! a route needs something answering on a port, and the four things it needs of it are always the same:
//! a port nobody else took, a body per path, a 404 for everything else, and **the ability to change what
//! one path serves while it is running** — which is what a publisher rotating a key looks like from the
//! outside.
//!
//! It is deliberately not a web server. There is no concurrency, no keep-alive, no content type and no
//! method: connections are answered one at a time, in order, because what is on the other end is amenbo
//! fetching one document and then the next. Test support only, never linked into a shipped binary.
//!
//! It is a crate of its own rather than a helper inside one suite because the same host is wanted from
//! both sides of the tree: this workspace's tests reach it as a dev-dependency, and the pre-distribution
//! harness — its own cargo workspace, deliberately outside this one — by path when a scenario has to
//! stand a catalog up. A third hand-rolled listener is the thing it exists to stop.
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

/// The most of a request line this reads. Only the path is taken from it, and a request whose line does
/// not fit is one no caller here makes.
const REQUEST_LIMIT: usize = 1024;

/// A loopback host serving fixed bodies by path (see the module docs).
pub struct StaticHost {
    port: u16,
    routes: Arc<Mutex<Vec<(String, String)>>>,
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
        let routes: Vec<(String, String)> =
            routes.into_iter().map(|(p, b)| (p.into(), b.into())).collect();
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("the bound address").port();
        let routes = Arc::new(Mutex::new(routes));
        let (served, flag) = (routes.clone(), Arc::new(AtomicBool::new(false)));
        let stop = flag.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                // The drop knocks on the port to get here, so the flag is read on the way in rather than
                // on the way out: what wakes this loop last is not a request to answer.
                if flag.load(Ordering::Relaxed) {
                    return;
                }
                let Ok(stream) = stream else { return };
                answer(stream, &served);
            }
        });
        StaticHost { port, routes, stop }
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
        let mut routes = self.routes.lock().expect("the routes");
        routes.retain(|(p, _)| p != path);
        routes.push((path.to_string(), body.to_string()));
    }
}

/// Read one request, write one answer, and let the connection close. A path that is not served answers
/// 404 — which is how "this catalog publishes no key" is expressed, so it is an answer and not a failure.
fn answer(mut stream: TcpStream, routes: &Mutex<Vec<(String, String)>>) {
    let mut buf = [0u8; REQUEST_LIMIT];
    let read = stream.read(&mut buf).unwrap_or(0);
    let request = String::from_utf8_lossy(&buf[..read]);
    let path = request.split_whitespace().nth(1).unwrap_or("/");
    let body = routes
        .lock()
        .expect("the routes")
        .iter()
        .find(|(p, _)| p == path)
        .map(|(_, b)| b.clone());
    let response = match body {
        Some(body) => format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
        None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
    };
    let _ = stream.write_all(response.as_bytes());
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
