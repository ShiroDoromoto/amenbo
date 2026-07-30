//! The CLI as a process: the exit code each kind of failure leaves, which stream an answer is
//! written to, the input a `-` reads off stdin, and what a reader walking away does to a run.

mod harness;

use std::process::Command;

use harness::*;

#[test]
fn errors_and_exit_codes() {
    let cli = Cli::new();
    // Unknown command → exit 2.
    let (_, code) = cli.run(&["tsak"]);
    assert_eq!(code, 2);
    // Missing id → exit 1.
    let (_, code) = cli.run(&["task", "show", "01ZZZZZZZZZZZZZZZZZZZZZZZZZ"]);
    assert_eq!(code, 1);
    // A destructive op with --json and no --yes → confirmation_required (exit 1).
    let p = cli.json(&["project", "add", "--name", "x", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let (out, code) = cli.run(&["project", "delete", &pid, "--json"]);
    assert_eq!(code, 1);
    assert!(out.is_empty(), "the confirmation error is expected on stderr");
}

/// A reader that walks away ends the run quietly, rather than crashing it.
///
/// The Rust runtime ignores SIGPIPE on startup, and with it ignored every printing macro panics on a
/// write it cannot make — so `amenbo task list | head` used to end in a panic message and exit 101.
/// `main` hands the signal back, which is what this pins.
///
/// `agent --full` is the fixture because it is the one command that overflows a pipe buffer off a
/// bare `init`: the writer is still writing when the reader goes, which is the only way to reach the
/// failing write at all. A smaller output lands whole in the buffer and the write always succeeds.
#[cfg(unix)]
#[test]
fn a_closed_pipe_ends_the_run_without_a_panic() {
    use std::os::unix::process::ExitStatusExt;
    use std::process::Stdio;

    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);

    let mut child = Command::new(env!("CARGO_BIN_EXE_amenbo"))
        .env("AMENBO_HOME", &cli.home)
        .env("AMENBO_UPDATE_CHECK", "0")
        .current_dir(&cli.home)
        .args(["agent", "--full", "--actor", "human"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run the binary");

    // What `head` does after its last line: the read end goes, and the next write finds no reader.
    drop(child.stdout.take().expect("stdout was piped"));

    let out = child.wait_with_output().expect("failed to wait for the binary");
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert_eq!(
        out.status.signal(),
        Some(libc::SIGPIPE),
        "expected the kernel's SIGPIPE to end the run, got {:?} (stderr: {stderr})",
        out.status
    );
    assert!(!stderr.contains("panicked"), "the run should say nothing on its way out (stderr: {stderr})");
}

/// Every body option takes `-` as "the body comes in on stdin". Bodies here are Markdown thick with
/// code spans, and a shell eats those out of a quoted argument by command substitution — silently,
/// taking the word with it — so the text has to be able to arrive without word expansion at all.
/// The value is passed through byte for byte, and a value that is not `-` is still the text itself.
#[test]
fn a_body_option_reads_stdin_on_dash() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "本文PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    // A body of the kind actually written here: code spans, which a shell would eat.
    const BODY: &str = "`--text` に直書きすると消える\n\n- `a` と `b`\n";

    let t = cli.json_stdin(&["task", "add", "--title", "本文", "--project", &pid, "--notes", "-", "--json"], BODY);
    let tid = id_str(&t["task"]["id"]);
    assert_eq!(t["task"]["notes"], BODY, "task add --notes - takes the body off stdin");

    let e = cli.json_stdin(&["task", "update", &tid, "--notes", "-", "--json"], "書き換え `x`");
    assert_eq!(e["task"]["notes"], "書き換え `x`", "task update --notes - too");

    let c = cli.json_stdin(&["comment", "add", &tid, "--text", "-", "--json"], BODY);
    assert_eq!(c["comment"]["text"], BODY, "comment add --text - too");

    let d = cli.json_stdin(&["decision", "add", "--title", "決定", "--project", &pid, "--body", "-", "--json"], BODY);
    let did = id_str(&d["decision"]["id"]);
    assert_eq!(d["decision"]["body"], BODY, "decision add --body - too");

    let dc = cli.json_stdin(&["decision", "comment", "add", &did, "--text", "-", "--json"], BODY);
    assert_eq!(dc["comment"]["text"], BODY, "decision comment add --text - too");

    // The ordinary path is untouched: a value that is not `-` is the body, stdin unread.
    let plain = cli.json(&["comment", "add", &tid, "--text", "そのまま", "--json"]);
    assert_eq!(plain["comment"]["text"], "そのまま", "a non-dash value is still the text");
}
