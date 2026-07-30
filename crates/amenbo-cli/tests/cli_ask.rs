//! The questions amenbo puts on a terminal, walked on one.
//!
//! amenbo asks two things once each — whether to wire the lint into this device's git hooks, and
//! whether this folder's AI may be started on amenbo — and both are put **only where someone can
//! answer them**: an interactive stdin, no `--json`, not an AI. A test runner has no terminal, so
//! every other suite here goes past those branches without touching them, and the sibling
//! `cli_e2e.rs` is not the place to fix that: what it drives is the ordinary, unattended face.
//!
//! So this suite gives the child a terminal. `stdin` is the slave side of a pty, which is the one
//! input `is_terminal()` reads, while `stdout` and `stderr` stay pipes — a question goes to stderr,
//! and folding the two into one stream would leave a test unable to say which of them carried it.
//! Unix only, for the same reason [`asked`](Ask::asked) is: a pty is what the platform hands out.
//!
//! The isolation is the sibling suite's, line for line: a throwaway `AMENBO_HOME`, a CWD of its own,
//! and no update check.
#![cfg(unix)]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a run is given to reach its own end before the terminal is taken away from it. Nothing
/// here is slow — the point of the cap is a build that asks a question this suite did not answer,
/// which would otherwise sit on a read for ever and take the test run with it. Closing the terminal
/// turns that into the failure it is: the run ends, and the assertion says what was missing.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(20);

struct Ask {
    home: PathBuf,
}

impl Ask {
    fn new() -> Ask {
        let home = amenbo_scratch::scratch("cli-ask");
        std::fs::create_dir_all(&home).unwrap();
        Ask { home }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_amenbo"));
        cmd.env("AMENBO_HOME", &self.home)
            .env("AMENBO_UPDATE_CHECK", "0")
            .env("NO_COLOR", "1")
            .current_dir(&self.home)
            .args(with_actor(args));
        cmd
    }

    /// An ordinary run, with no terminal on stdin — what every other suite here drives. Used for the
    /// setting up and the reading back, so that what a question changed is read from a face that
    /// cannot be asked one.
    fn run(&self, args: &[&str]) -> (String, String, i32) {
        let out = self.command(args).output().expect("failed to run the binary");
        (
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
            out.status.code().unwrap_or(-1),
        )
    }

    /// The same run, on a terminal, with `answers` typed into it a line at a time.
    ///
    /// An **empty** `answers` is the other half of what this exists to walk: the terminal is closed
    /// with nothing said, which is the end-of-input a question meets when the reader walks away or a
    /// script closes their stdin. It is not a `no`, and this is the only way to put that to the
    /// binary at all.
    ///
    /// The terminal stays open while the run finishes, so a question is not answered by the stream
    /// disappearing under it; [`ANSWER_TIMEOUT`] is what stops a run that wanted more than it was
    /// given from hanging the suite.
    fn asked(&self, args: &[&str], answers: &[&str]) -> (String, String, i32) {
        use std::fs::File;
        use std::os::fd::FromRawFd;

        let (master, slave) = openpty();
        // The child takes the slave end as its stdin; this process's own copy of it goes with the
        // `Stdio` once the spawn is done, which is what leaves the terminal with one owner on each
        // side. stdout and stderr stay pipes: the question is on stderr, and it has to stay tellable
        // from what the command itself printed.
        let mut child = self
            .command(args)
            .stdin(unsafe { Stdio::from_raw_fd(slave) })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to run the binary");

        let mut terminal = Some(unsafe { File::from_raw_fd(master) });
        if answers.is_empty() {
            // Nothing to say: closing this end is the end of input the question meets.
            terminal = None;
        } else if let Some(tty) = terminal.as_mut() {
            for answer in answers {
                writeln!(tty, "{answer}").expect("could not type the answer");
            }
            tty.flush().ok();
        }

        let deadline = Instant::now() + ANSWER_TIMEOUT;
        loop {
            match child.try_wait().expect("could not wait for the binary") {
                Some(_) => break,
                None if Instant::now() >= deadline => {
                    // It is still waiting on a terminal this test did not undertake to keep feeding.
                    // Take it away rather than hang: the run ends, and the assertion below reports it.
                    terminal = None;
                    break;
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        let out = child.wait_with_output().expect("could not read the run back");
        drop(terminal);
        (
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
            out.status.code().unwrap_or(-1),
        )
    }

    /// A folder this reader's AI already works in, with nothing in it about amenbo — which is what
    /// makes the session-start question worth putting.
    fn traced_by_claude_code(&self) {
        std::fs::create_dir_all(self.home.join(".claude")).unwrap();
    }

    /// A git repository in the run's own folder, which is what gives the lint's question its slots.
    fn git_repo(&self) {
        let out = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&self.home)
            .output()
            .expect("could not run git");
        assert!(out.status.success(), "git init failed: {}", String::from_utf8_lossy(&out.stderr));
    }
}

/// The facet on the command line, as everywhere: a question is only ever put to a person, so the
/// facet these runs declare is the human one.
fn with_actor<'a>(args: &[&'a str]) -> Vec<&'a str> {
    let mut with = args.to_vec();
    if !args.contains(&"--actor") {
        with.extend_from_slice(&["--actor", "human"]);
    }
    with
}

/// A pty, as the two file descriptors either end of it. The C call is the whole of the unsafe here —
/// everything above takes the two ends as ordinary owned handles.
fn openpty() -> (std::os::fd::RawFd, std::os::fd::RawFd) {
    let mut master = -1;
    let mut slave = -1;
    let opened = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(opened, 0, "could not open a pty: {}", std::io::Error::last_os_error());
    // The child must not inherit this end. A fresh descriptor is inherited across a spawn unless it
    // says otherwise, and a copy of the writing end **inside** the child holds the terminal open from
    // there — so closing it here would never reach the read the child is sitting on, and the end of
    // input this suite exists to walk would be a hang instead.
    let closed_on_exec = unsafe { libc::fcntl(master, libc::F_SETFD, libc::FD_CLOEXEC) };
    assert_ne!(closed_on_exec, -1, "could not keep the pty from the child: {}", std::io::Error::last_os_error());
    (master, slave)
}

/// The words each question leads with, which is how a run says which one it put. They are matched on
/// a phrase rather than in full: what is under test is *that* the question was put and *which* one,
/// not the wording, which is the product's to change.
const LINT_QUESTION: &str = "Wire it up?";
const SESSION_QUESTION: &str = "Want the text?";

/// A `no` on the terminal is an answer: it is recorded, the question does not come back, and what
/// stood behind it goes quiet with it. It forbids nothing — the text is still handed over on demand
/// — so what is proved here is only that amenbo stops asking.
#[test]
fn a_no_is_recorded_and_the_question_does_not_come_back() {
    let ask = Ask::new();
    ask.run(&["init", "--name", "Alice"]);
    ask.traced_by_claude_code();

    let (_, err, code) = ask.asked(&["task", "list"], &["n"]);
    assert_eq!(code, 0, "the question is not a gate on the command: {err}");
    assert!(err.contains(SESSION_QUESTION), "the question was never put: {err}");
    assert!(err.contains("will not ask again"), "a no was taken without saying so: {err}");

    // Asked once for the project: a second terminal gets the command it ran, and nothing else.
    let (_, err, _) = ask.asked(&["task", "list"], &["n"]);
    assert!(!err.contains(SESSION_QUESTION), "the question came back after a no: {err}");
    // And the report the AI reads is silent too — a reader who said no has no setup pending.
    let (out, _, _) = ask.run(&["task", "list", "--json", "--actor", "ai"]);
    let doc: serde_json::Value = serde_json::from_str(&out).expect("--json is JSON");
    assert!(doc.get("setup_incomplete").is_none(), "a reader who said no is still being told: {doc}");
}

/// A `yes` is answered with what was asked for. amenbo writes no settings file, so the only thing a
/// yes can hand back is the text — and handing back nothing but a recorded consent would be taking
/// an answer and giving nothing for it.
#[test]
fn a_yes_hands_over_the_text_it_offered() {
    let ask = Ask::new();
    ask.run(&["init", "--name", "Alice"]);
    ask.traced_by_claude_code();

    let (_, err, code) = ask.asked(&["task", "list"], &["y"]);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains(SESSION_QUESTION), "the question was never put: {err}");
    assert!(err.contains(".claude/settings.json"), "a yes did not say where the text goes: {err}");
    assert!(err.contains("agent --json"), "a yes did not hand over the text: {err}");
}

/// **End of input is not a `no`.** A question the reader never saw — a closed stdin, a terminal they
/// walked away from — records nothing, and comes back next time. The opposite is the failure that
/// cannot be undone from the outside: an answer nobody gave, remembered for ever.
#[test]
fn an_end_of_input_is_not_an_answer() {
    let ask = Ask::new();
    ask.run(&["init", "--name", "Alice"]);
    ask.traced_by_claude_code();

    let (_, err, code) = ask.asked(&["task", "list"], &[]);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains(SESSION_QUESTION), "the question was never put: {err}");
    assert!(!err.contains("will not ask again"), "an unanswered question was taken as a no: {err}");

    // The question is still live, which is the whole of what "nothing was recorded" means here.
    let (_, err, _) = ask.asked(&["task", "list"], &["n"]);
    assert!(err.contains(SESSION_QUESTION), "the question did not come back: {err}");
}

/// **One question a run.** Two prompts in one command, over two different things, is how a reader
/// ends up answering neither on purpose. The lint's goes first — it is the older one, and its `no`
/// closes it for good — and the session-start one waits for the next run, having recorded nothing.
#[test]
fn only_one_question_is_put_in_a_run() {
    let ask = Ask::new();
    ask.run(&["init", "--name", "Alice"]);
    // Both questions have their occasion in this folder at once: a git repository with no lint hook
    // in it, and a tool of the reader's that does not start on amenbo.
    ask.git_repo();
    ask.traced_by_claude_code();

    let (_, err, code) = ask.asked(&["task", "list"], &["n"]);
    assert_eq!(code, 0, "{err}");
    assert!(err.contains(LINT_QUESTION), "the lint's question was not the one put first: {err}");
    assert!(!err.contains(SESSION_QUESTION), "both questions were put in one run: {err}");

    // The one held back is put on the next run, and is still the whole question — nothing about it
    // was recorded by having been skipped.
    let (_, err, _) = ask.asked(&["task", "list"], &["n"]);
    assert!(err.contains(SESSION_QUESTION), "the question held back never came: {err}");
    assert!(!err.contains(LINT_QUESTION), "the lint's question came back after a no: {err}");
}
