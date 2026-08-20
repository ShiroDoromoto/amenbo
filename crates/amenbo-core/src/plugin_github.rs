//! What GitHub knows about one plugin's repository — its stars, the downloads of its current
//! release, and its README — fetched for the **one** entry a user opened (`AMB-D-347`).
//!
//! This is the deliberate exception to [`crate::plugin_catalog`]'s "the unit of retrieval is the
//! catalog, never the plugin". The catalog stays one static file everybody fetches once, and these
//! numbers are exactly what a catalog cannot carry: they change without the index changing, and
//! putting them in it would mean the index asking GitHub about every listed plugin. So they are read
//! per repository, lazily, and only for an entry a user chose to open — never for a list.
//!
//! **Nothing here is trusted, and nothing here decides anything.** These are display figures: an
//! install is gated by the asset's signature against Amenbo's own key (`AMB-D-371`), never by a star
//! count. The README arrives as Markdown text and is rendered by the front end, which allows no raw
//! HTML. It is asked for only where it would be drawn ([`Readme`]): a plugin whose author wrote a
//! description of it has that on its detail face instead, so nothing goes and gets the README
//! (`AMB-D-638`).
//!
//! **The rate limit is what shapes the caching.** GitHub's API answers an unauthenticated client
//! about 60 times an hour per IP, and one opened plugin costs up to three requests, so a naive
//! fetch-on-every-open would run a browsing user into a wall in twenty clicks. Facts are therefore
//! cached per repository, on disk, and a cache inside [`FRESH_FOR`] answers with no request at all —
//! the same discipline as the catalog cache, with a window sized to the limit rather than to
//! freshness. Amenbo sends no credentials: there is no account to attach one to, and a plugin's star
//! count is public.
//!
//! Being offline costs the numbers, not the view: every request fails on its own, a partial answer
//! is returned as a partial answer, and the cache stands in when the network does not.

use crate::config::Paths;
use crate::error::{Error, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Where the GitHub REST API lives. Overridable through `AMENBO_GITHUB_API_URL`
/// ([`crate::env::github_api_url`]) so development and tests can point somewhere else.
pub const GITHUB_API_URL: &str = "https://api.github.com";

/// How long one request may take before it is treated as a failure. Three of these run in sequence
/// for one opened plugin, so it is deliberately shorter than the catalog's: the screen is already
/// drawn and only the figures are missing, which is a much cheaper failure than an empty market.
const FETCH_TIMEOUT: Duration = Duration::from_secs(6);

/// How long cached facts answer before a read goes back to GitHub.
///
/// Six hours, where the catalog settles on one (`AMB-D-359`), because the constraint here is not
/// freshness but GitHub's unauthenticated rate limit — 60 requests an hour per IP, three per opened
/// plugin. A window that short would let ordinary browsing exhaust the limit and turn every
/// subsequent detail into an empty one. Nothing depends on these numbers being current: they are the
/// deliberately lazy figures of `AMB-D-347`, read for a sense of scale, and a star count six hours old
/// tells the same story as a fresh one.
pub const FRESH_FOR: Duration = Duration::from_secs(6 * 60 * 60);

/// The most README we keep. A README is shown, not parsed, and a repository is free to publish a
/// megabyte of it; past this the rest is dropped rather than carried through the IPC boundary and
/// into the renderer.
const README_MAX_BYTES: usize = 128 * 1024;

/// The directory the per-repository facts are cached in, beside the catalog caches.
const CACHE_DIR_NAME: &str = "github";

/// What GitHub could tell us about one repository. Every figure is optional on its own: the three
/// requests fail independently, and a repository with no release simply has no download count.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RepoFacts {
    /// Stargazers — `AMB-D-347`'s stand-in for popularity, since Amenbo counts no installs.
    pub stars: Option<u64>,
    /// Downloads of the current release's assets, summed. `None` when the repository has published no
    /// release. The figure includes whatever else pulls an asset (CI, mirrors), so it is a scale, not
    /// a user count (`AMB-D-347`).
    pub downloads: Option<u64>,
    /// The README as Markdown, truncated at [`README_MAX_BYTES`]. `None` when the repository has none.
    pub readme: Option<String>,
    /// GitHub refused because too many requests came from this IP. Distinct from "could not reach it":
    /// the answer is to wait, not to check the network, and only saying so makes that legible.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rate_limited: bool,
}

impl RepoFacts {
    /// Whether anything at all came back — what decides between caching an answer and keeping the one
    /// already on disk.
    fn is_empty(&self) -> bool {
        self.stars.is_none() && self.downloads.is_none() && self.readme.is_none()
    }
}

/// **Whether the README is part of what is being asked for** (`AMB-D-638`).
///
/// A plugin whose author wrote a description of it has that on its detail face, and the README is not
/// drawn there at all — so it is not fetched either. It is a third of what one opened plugin spends
/// against GitHub's limit, and the caller that would draw nothing with it is the one that says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Readme {
    /// Ask for it: nothing else describes this plugin, so the README is what its detail draws.
    Fetch,
    /// Leave it: the plugin describes itself, and a README nobody draws is a request nobody needs.
    Skip,
}

/// One repository's facts as they are written to the cache: the answer, and whether the README was
/// among the questions actually put.
///
/// `readme: None` says two different things — GitHub has none to give, and nobody has ever asked —
/// and only the second is a reason to ask again inside [`FRESH_FOR`]. Before `AMB-D-638` there was
/// only the first, every caller having asked; a copy written with the question skipped is what makes
/// the distinction load-bearing.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct CachedFacts {
    /// The answer itself, written flat so a cache from an earlier build still reads back as one.
    #[serde(flatten)]
    facts: RepoFacts,
    /// Whether the README was asked for in the exchange this was written from. A cache written
    /// before the field existed has it absent, and reads as "not asked" — one request more, once,
    /// rather than six hours of a `None` nobody ever went looking for.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    readme_asked: bool,
}

impl CachedFacts {
    /// Whether this copy answers what is being asked. Fresh is not enough on its own: a copy written
    /// without the README question having been put has nothing to say to a caller that draws one.
    fn answers(&self, readme: Readme) -> bool {
        readme == Readme::Skip || self.readme_asked
    }
}

/// The facts for one `owner/name` repository, cached per repository (`AMB-D-347`).
///
/// A cache inside [`FRESH_FOR`] answers with no request, so long as it holds what is being asked
/// for — one written by a caller that skipped the README has nothing to say to one that draws it.
/// Past it, GitHub is asked and a partial answer is still an answer — a README that did not come
/// back leaves the stars in place. What fails entirely falls back to the cached copy, however old;
/// only a repository we have never reached, and cannot reach now, is an error.
///
/// `repo` comes from a catalog entry, and the delivery path is not trusted (`AMB-D-354`), so the
/// shape is checked here before it is ever pasted into a URL or a file name.
///
/// `readme` is the caller saying whether it would draw one ([`Readme`]). What is not asked for is
/// absent from the answer however it was reached — a fetch that skipped it, or a cache written when
/// someone else did want it — so a caller never draws what it declared it had no place for.
pub fn facts(paths: &Paths, repo: &str, readme: Readme) -> Result<RepoFacts> {
    facts_at(paths, &api_url(), repo, FRESH_FOR, readme)
}

/// [`facts`] against a named API base and freshness window — the seam the tests drive, so nothing in
/// this module's test run reaches api.github.com, and the window can be closed to prove what happens
/// past it.
fn facts_at(
    paths: &Paths,
    api: &str,
    repo: &str,
    fresh_for: Duration,
    readme: Readme,
) -> Result<RepoFacts> {
    let repo = checked_repo(repo)?;
    let cache = cache_file(paths, &repo);
    let on_disk = cached(&cache);
    if cache_age(&cache).is_some_and(|age| age < fresh_for) {
        if let Some(cached) = on_disk.clone().filter(|c| c.answers(readme)) {
            return Ok(as_asked(cached.facts, readme));
        }
    }
    let fetched = fetch_facts(api, &repo, readme);
    if !fetched.is_empty() {
        // Not asking for the README is not learning that it is gone, so a copy already cached stays,
        // and so does the record of whether anyone has asked: the next reader who *would* draw one
        // gets it without a request of their own, or goes for it because nobody has yet.
        let mut to_cache =
            CachedFacts { facts: fetched.clone(), readme_asked: readme == Readme::Fetch };
        if let (Readme::Skip, Some(prev)) = (readme, on_disk.clone()) {
            to_cache.facts.readme = prev.facts.readme;
            to_cache.readme_asked = prev.readme_asked;
        }
        // Best-effort: failing to cache costs a request next time, never the answer in hand.
        let _ = write_cache(&cache, &to_cache);
        return Ok(fetched);
    }
    // Nothing came back. A stale copy is a better answer than none — but a rate limit is news of its
    // own, and saying "too many requests" over yesterday's numbers is more honest than silence.
    match on_disk {
        Some(stale) => {
            Ok(RepoFacts { rate_limited: fetched.rate_limited, ..as_asked(stale.facts, readme) })
        }
        None if fetched.rate_limited => Ok(fetched),
        None => Err(Error::Io(std::io::Error::other(format!(
            "could not read anything about {repo} from GitHub"
        )))),
    }
}

/// Cut from an answer what the caller did not ask for. Only the README is optional in that sense: the
/// figures are what every caller opens a detail for.
fn as_asked(facts: RepoFacts, readme: Readme) -> RepoFacts {
    match readme {
        Readme::Fetch => facts,
        Readme::Skip => RepoFacts { readme: None, ..facts },
    }
}

/// The API base to talk to: [`GITHUB_API_URL`], unless the environment overrides it.
fn api_url() -> String {
    crate::env::github_api_url().unwrap_or_else(|| GITHUB_API_URL.to_string())
}

/// A repository reference Amenbo will act on: exactly `owner/name`, both made of the characters
/// GitHub allows in one. The check is what keeps a catalog's string out of the parts of a URL it has
/// no business reaching — a query, another host, a path segment above the cache directory.
fn checked_repo(repo: &str) -> Result<String> {
    let repo = repo.trim();
    let refused = || {
        Error::invalid(format!("not a GitHub repository reference (owner/name): {repo}"))
    };
    let (owner, name) = repo.split_once('/').ok_or_else(refused)?;
    let usable = |part: &str| {
        !part.is_empty()
            && part.len() <= 100
            && part != "."
            && part != ".."
            && part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if !usable(owner) || !usable(name) {
        return Err(refused());
    }
    Ok(repo.to_string())
}

/// Where one repository's facts are cached: `<base>/plugins/registry/github/<owner>__<name>.json`.
/// The name is the repository's own — [`checked_repo`] has already ruled out everything that could
/// make it anything but a file name — so a cache is legible on disk and one repository maps to one
/// file forever.
fn cache_file(paths: &Paths, repo: &str) -> PathBuf {
    paths.registry_dir().join(CACHE_DIR_NAME).join(format!("{}.json", repo.replace('/', "__")))
}

/// The cached facts, or `None` when there are none — absent, unreadable, or written in a shape this
/// build no longer reads. Never an error: the answer to it is to fetch.
fn cached(cache_file: &Path) -> Option<CachedFacts> {
    let json = std::fs::read_to_string(cache_file).ok()?;
    serde_json::from_str(&json).ok()
}

/// How long ago the cache was written, or `None` when there is none — or when the clock says it was
/// written in the future, which is no evidence of freshness.
fn cache_age(cache_file: &Path) -> Option<Duration> {
    std::fs::metadata(cache_file).ok()?.modified().ok()?.elapsed().ok()
}

/// Replace the cached facts, atomically — written beside the target and renamed over it, so a crash
/// mid-write cannot leave half a file to be read back as facts.
fn write_cache(cache_file: &Path, facts: &CachedFacts) -> Result<()> {
    if let Some(parent) = cache_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string(facts).map_err(|e| Error::Io(std::io::Error::other(e)))?;
    let tmp = cache_file.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, cache_file)?;
    Ok(())
}

/// Ask GitHub the questions, each on its own. There is no transaction here: what answers is kept and
/// what does not is left `None`, because a missing README is not a reason to throw away a star count
/// that arrived.
///
/// It is three questions, or two: a caller that would not draw a README does not pay for one
/// (`AMB-D-638`).
fn fetch_facts(api: &str, repo: &str, readme: Readme) -> RepoFacts {
    let mut facts = RepoFacts::default();

    match get(&format!("{api}/repos/{repo}"), "application/vnd.github+json") {
        Ok(body) => facts.stars = json_u64(&body, "stargazers_count"),
        Err(e) => facts.rate_limited |= e.rate_limited,
    }
    match get(&format!("{api}/repos/{repo}/releases/latest"), "application/vnd.github+json") {
        Ok(body) => facts.downloads = release_downloads(&body),
        Err(e) => facts.rate_limited |= e.rate_limited,
    }
    // The `raw` media type asks for the README's bytes rather than a JSON envelope carrying them
    // base64-encoded, so what comes back is the Markdown itself.
    if readme == Readme::Fetch {
        match get(&format!("{api}/repos/{repo}/readme"), "application/vnd.github.raw") {
            Ok(body) => facts.readme = Some(truncate_readme(body)),
            Err(e) => facts.rate_limited |= e.rate_limited,
        }
    }
    facts
}

/// A failed request, and the one thing a caller distinguishes about it.
struct FetchError {
    /// GitHub answered "too many requests" (429, or the 403 it uses for the same thing).
    rate_limited: bool,
}

/// One GET against the GitHub API. Amenbo identifies itself (GitHub refuses a request with no
/// `User-Agent`) and sends nothing else — no token, no cookie, no user data.
fn get(url: &str, accept: &str) -> std::result::Result<String, FetchError> {
    let agent: ureq::Agent =
        ureq::Agent::config_builder().timeout_global(Some(FETCH_TIMEOUT)).build().into();
    let mut response = agent
        .get(url)
        .header("Accept", accept)
        .header("User-Agent", concat!("amenbo/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| FetchError { rate_limited: is_rate_limit(&e) })?;
    response.body_mut().read_to_string().map_err(|_| FetchError { rate_limited: false })
}

/// Whether a failed call is GitHub saying "too many requests". It answers 403 for a spent
/// unauthenticated quota as well as 429, so both count; every other status is just a request that did
/// not work.
fn is_rate_limit(error: &ureq::Error) -> bool {
    matches!(error, ureq::Error::StatusCode(403 | 429))
}

/// One unsigned number out of a JSON object, or `None` when the field is absent or not a number —
/// the shape of the answer is GitHub's to change, and a missing figure is simply not shown.
fn json_u64(body: &str, field: &str) -> Option<u64> {
    serde_json::from_str::<serde_json::Value>(body).ok()?.get(field)?.as_u64()
}

/// The downloads of one release: its assets' counts, summed. A release with no assets is a real
/// zero — the release exists and nothing has been pulled from it — so it is not `None`.
fn release_downloads(body: &str) -> Option<u64> {
    let release: serde_json::Value = serde_json::from_str(body).ok()?;
    let assets = release.get("assets")?.as_array()?;
    Some(assets.iter().filter_map(|a| a.get("download_count")?.as_u64()).sum())
}

/// Cut an over-long README at [`README_MAX_BYTES`], on a character boundary so what is kept is still
/// text.
fn truncate_readme(mut readme: String) -> String {
    if readme.len() <= README_MAX_BYTES {
        return readme;
    }
    let mut cut = README_MAX_BYTES;
    while cut > 0 && !readme.is_char_boundary(cut) {
        cut -= 1;
    }
    readme.truncate(cut);
    readme
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths_at(tag: &str) -> Paths {
        let dir = amenbo_scratch::scratch(&format!("plugin-github-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        Paths::at(dir)
    }

    /// An address nothing answers on — what being offline looks like from inside a fetch.
    const UNREACHABLE: &str = "http://127.0.0.1:1";

    /// A cache as a caller that asked about the README leaves it — the only shape there was before
    /// `AMB-D-638` let one skip the question.
    fn asked_for(facts: &RepoFacts) -> CachedFacts {
        CachedFacts { facts: facts.clone(), readme_asked: true }
    }

    #[test]
    fn a_repo_reference_must_be_owner_slash_name() {
        assert_eq!(checked_repo(" ShiroDoromoto/amenbo ").unwrap(), "ShiroDoromoto/amenbo");
        assert_eq!(checked_repo("a/b.c_d-e").unwrap(), "a/b.c_d-e");

        for bad in [
            "amenbo",                        // no owner
            "owner/",                        // no name
            "/name",                         // no owner
            "owner/name/extra",              // not a repository reference
            "owner/na me",                   // not a name GitHub gives out
            "owner/name?token=x",            // a query smuggled into the path
            "owner/../../../etc/passwd",     // a path, aimed above the cache directory
            "https://evil.invalid/owner/rp", // another host entirely
            "owner/.",
            "owner/..",
        ] {
            assert!(checked_repo(bad).is_err(), "refused: {bad}");
        }
    }

    /// The gate above is what makes the cache file name safe to build from the repository itself.
    #[test]
    fn a_cache_file_is_one_per_repo_and_stays_under_the_registry_dir() {
        let paths = paths_at("cache-name");
        let a = cache_file(&paths, "ShiroDoromoto/amenbo");
        assert_eq!(a, cache_file(&paths, "ShiroDoromoto/amenbo"), "stable");
        assert_ne!(a, cache_file(&paths, "someone/amenbo"), "different repos, different files");
        assert!(a.starts_with(paths.registry_dir()), "cached beside the catalog caches");
        assert_ne!(a, crate::plugin_catalog::cache_file(&paths), "never the catalog's own cache");
    }

    #[test]
    fn the_cache_round_trips_and_a_corrupt_one_reads_as_none() {
        let paths = paths_at("round-trip");
        let file = cache_file(&paths, "owner/name");
        assert!(cached(&file).is_none(), "nothing cached yet");

        let facts = RepoFacts {
            stars: Some(42),
            downloads: Some(7),
            readme: Some("# hi".to_string()),
            rate_limited: false,
        };
        write_cache(&file, &asked_for(&facts)).unwrap();
        assert_eq!(cached(&file).unwrap(), asked_for(&facts));

        // A cache from a build that wrote no such field: it reads back, as a copy nobody asked
        // about the README for.
        std::fs::write(&file, r##"{"stars": 42, "downloads": 7, "readme": "# hi"}"##).unwrap();
        let old = cached(&file).unwrap();
        assert_eq!(old.facts, facts, "the figures an earlier build wrote");
        assert!(!old.readme_asked, "and nothing saying the README was ever asked for");

        std::fs::write(&file, "half a file").unwrap();
        assert!(cached(&file).is_none(), "unreadable is 'fetch again', not an error to show");
    }

    /// A cache inside the freshness window answers on its own — the whole reason there is one, given
    /// GitHub's rate limit. The proof is that what comes back is the copy on disk, which no live
    /// request would have produced.
    #[test]
    fn a_fresh_cache_answers_without_the_network() {
        let paths = paths_at("fresh");
        let on_disk = RepoFacts { stars: Some(1234), ..RepoFacts::default() };
        write_cache(&cache_file(&paths, "owner/name"), &asked_for(&on_disk)).unwrap();

        // The API base is one nothing answers on: reaching it at all would fail the assertion.
        assert_eq!(
            facts_at(&paths, UNREACHABLE, "owner/name", FRESH_FOR, Readme::Fetch).unwrap(),
            on_disk
        );
    }

    /// Past the window the fetch is tried, and a stale copy is what stands in when it fails — being
    /// offline costs freshness, not the figures (`AMB-D-347`). A closed window (`ZERO`) is how the
    /// test gets past the boundary without touching the clock.
    #[test]
    fn a_failed_fetch_falls_back_to_the_stale_cache_and_never_clears_it() {
        let paths = paths_at("stale");
        let file = cache_file(&paths, "owner/name");
        let on_disk = RepoFacts { stars: Some(9), ..RepoFacts::default() };
        write_cache(&file, &asked_for(&on_disk)).unwrap();

        let got = facts_at(&paths, UNREACHABLE, "owner/name", Duration::ZERO, Readme::Fetch).unwrap();
        assert_eq!(got, on_disk, "the stale copy answered");
        assert_eq!(cached(&file).unwrap(), asked_for(&on_disk), "and the failed fetch left it alone");
    }

    #[test]
    fn a_repo_we_have_never_reached_and_cannot_reach_is_an_error() {
        let paths = paths_at("nothing");
        assert!(facts_at(&paths, UNREACHABLE, "owner/name", FRESH_FOR, Readme::Fetch).is_err());
    }

    /// **A plugin that describes itself costs GitHub one request fewer** (`AMB-D-638`). The README is
    /// served here and still does not come back: what proves the request was never made is that the
    /// body sitting on the host is not in the answer.
    #[test]
    fn a_caller_that_would_not_draw_a_readme_is_not_served_one() {
        let paths = paths_at("skip-readme");
        let host = amenbo_static_host::StaticHost::serve([
            ("/repos/owner/name", r#"{"stargazers_count": 12}"#),
            ("/repos/owner/name/readme", "# the readme nobody asked for"),
        ]);
        let api = host.url("");

        let skipped = facts_at(&paths, &api, "owner/name", FRESH_FOR, Readme::Skip).unwrap();
        assert_eq!(skipped.stars, Some(12), "the figures are read either way");
        assert!(skipped.readme.is_none(), "and the README is not");

        // The same repository, in the same store, asked for by someone who *would* draw one. The
        // cache the first reader left is fresh and holds no README — but it holds no answer about
        // one either, so it is not the fresh cache's to give.
        let fetched = facts_at(&paths, &api, "owner/name", FRESH_FOR, Readme::Fetch).unwrap();
        assert_eq!(fetched.readme.as_deref(), Some("# the readme nobody asked for"));

        // And now it is: the question has been put, so the next reader is answered off the disk.
        let again = facts_at(&paths, UNREACHABLE, "owner/name", FRESH_FOR, Readme::Fetch).unwrap();
        assert_eq!(again.readme.as_deref(), Some("# the readme nobody asked for"), "no request");
    }

    /// A repository that really has no README is asked about once, not once per reader: the cache
    /// records that the question was put, and `None` is the answer to it (`AMB-D-347`'s whole point
    /// being that GitHub's limit is spent one open at a time).
    #[test]
    fn a_repo_with_no_readme_is_not_asked_again_inside_the_window() {
        let paths = paths_at("no-readme");
        let host = amenbo_static_host::StaticHost::serve([(
            "/repos/owner/name",
            r#"{"stargazers_count": 12}"#,
        )]);

        let first = facts_at(&paths, &host.url(""), "owner/name", FRESH_FOR, Readme::Fetch).unwrap();
        assert!(first.readme.is_none(), "the host serves none");

        let second = facts_at(&paths, UNREACHABLE, "owner/name", FRESH_FOR, Readme::Fetch).unwrap();
        assert_eq!(second.stars, Some(12), "answered off the disk, with no request of its own");
    }

    /// A reader that draws no README is answered by the copy **another one of its kind** left, rather
    /// than going back for the two figures already on the disk. That copy records that nobody asked about
    /// the README, and a caller that would not draw one is not who that record is kept for: paying a
    /// request for it on every open is exactly the cost `AMB-D-638` took the question away to avoid.
    #[test]
    fn a_cache_written_with_the_readme_skipped_answers_the_next_reader_that_skips_it() {
        let paths = paths_at("skip-then-skip");
        let host = amenbo_static_host::StaticHost::serve([(
            "/repos/owner/name",
            r#"{"stargazers_count": 12}"#,
        )]);

        let first = facts_at(&paths, &host.url(""), "owner/name", FRESH_FOR, Readme::Skip).unwrap();
        assert_eq!(first.stars, Some(12), "fetched, and cached with the question never put");

        // What the host serves moves on. Inside the window the cache answers, so the new figure is the
        // one thing a second request would bring back — and what comes back is the old one.
        host.set("/repos/owner/name", r#"{"stargazers_count": 99}"#);
        let again = facts_at(&paths, &host.url(""), "owner/name", FRESH_FOR, Readme::Skip).unwrap();
        assert_eq!(again.stars, Some(12), "answered off the disk, with no request of its own");
    }

    /// The stale copy that stands in for a failed fetch is cut to what was asked for, exactly as a fresh
    /// one is: **what is not asked for is absent from the answer however it was reached**. A caller that
    /// declared it draws no README has nowhere to put one, whichever road the figures came down.
    #[test]
    fn a_stale_cache_standing_in_is_still_cut_to_what_was_asked_for() {
        let paths = paths_at("skip-stale");
        let file = cache_file(&paths, "owner/name");
        let on_disk = RepoFacts {
            stars: Some(3),
            readme: Some("# from a reader that draws one".to_string()),
            ..RepoFacts::default()
        };
        write_cache(&file, &asked_for(&on_disk)).unwrap();

        // Past the window, and nothing answers: the copy on disk is all there is to answer with.
        let got = facts_at(&paths, UNREACHABLE, "owner/name", Duration::ZERO, Readme::Skip).unwrap();
        assert_eq!(got.stars, Some(3), "the stale copy answered");
        assert!(got.readme.is_none(), "and not with a README the caller would draw nowhere");
        assert_eq!(cached(&file).unwrap(), asked_for(&on_disk), "still there for one that would");
    }

    /// The cache is a shared thing — one file per repository, whoever wrote it — so a run that did not
    /// ask about the README neither hands one out nor throws the cached one away.
    #[test]
    fn skipping_the_readme_hides_the_cached_one_without_erasing_it() {
        let paths = paths_at("skip-cached");
        let file = cache_file(&paths, "owner/name");
        let on_disk = RepoFacts {
            stars: Some(3),
            readme: Some("# from an earlier reader".to_string()),
            ..RepoFacts::default()
        };
        write_cache(&file, &asked_for(&on_disk)).unwrap();

        // Fresh cache, so nothing is fetched: the README is cut from the answer, not from the file.
        let got = facts_at(&paths, UNREACHABLE, "owner/name", FRESH_FOR, Readme::Skip).unwrap();
        assert_eq!(got.stars, Some(3));
        assert!(got.readme.is_none(), "not handed to a caller that declared it draws none");
        assert_eq!(cached(&file).unwrap(), asked_for(&on_disk), "and still there for one that does");

        // Past the window, a fetch that answers with the figures alone must not overwrite it either.
        let host = amenbo_static_host::StaticHost::serve([(
            "/repos/owner/name",
            r#"{"stargazers_count": 99}"#,
        )]);
        let got = facts_at(&paths, &host.url(""), "owner/name", Duration::ZERO, Readme::Skip).unwrap();
        assert_eq!(got.stars, Some(99), "the fresh figure");
        let after = cached(&file).unwrap();
        assert_eq!(after.facts.readme, on_disk.readme, "the README nobody asked about survived");
        assert!(after.readme_asked, "and so did the fact that someone once asked");
    }

    #[test]
    fn the_figures_are_read_out_of_what_github_answers() {
        let repo = r#"{"stargazers_count": 512, "full_name": "owner/name"}"#;
        assert_eq!(json_u64(repo, "stargazers_count"), Some(512));
        assert_eq!(json_u64(repo, "subscribers_count"), None, "a field GitHub did not send");
        assert_eq!(json_u64("not json", "stargazers_count"), None);

        let release = r#"{"assets": [{"download_count": 10}, {"download_count": 5}]}"#;
        assert_eq!(release_downloads(release), Some(15), "summed across the release's assets");
        assert_eq!(release_downloads(r#"{"assets": []}"#), Some(0), "a release nobody pulled is 0");
        assert_eq!(release_downloads("{}"), None, "no release is not a zero");
    }

    #[test]
    fn an_over_long_readme_is_cut_on_a_character_boundary() {
        let short = "# hi".to_string();
        assert_eq!(truncate_readme(short.clone()), short);

        // Multi-byte characters straddling the cut: the result must still be text.
        let long = "あ".repeat(README_MAX_BYTES);
        let cut = truncate_readme(long);
        assert!(cut.len() <= README_MAX_BYTES);
        assert!(cut.chars().all(|c| c == 'あ'), "cut between characters, not through one");
    }

    /// The end-to-end this module cannot assert in CI: the real API, the real shapes. Run it by hand
    /// (`cargo nextest run -p amenbo-core plugin_github -- --ignored`) when GitHub's answers change.
    #[test]
    #[ignore = "reaches api.github.com over the network"]
    fn the_live_api_answers_for_amenbos_own_repository() {
        let paths = paths_at("live");
        let got = facts(&paths, "ShiroDoromoto/amenbo", Readme::Fetch).expect("GitHub answers");
        assert!(got.stars.is_some(), "a public repository has a star count: {got:?}");
        assert!(got.readme.is_some(), "and a README");
        assert!(cached(&cache_file(&paths, "ShiroDoromoto/amenbo")).is_some(), "cached for next time");
    }
}
