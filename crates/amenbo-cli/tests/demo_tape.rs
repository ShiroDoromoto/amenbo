//! The README's CLI demo, held to the CLI it films.
//!
//! `assets/cli-demo.gif` is the first thing the README shows, and it is a picture: once the wording
//! of a line the film prints changes, the picture keeps showing the old one and nothing anywhere
//! notices. The bytes of the recording are no help either — a re-recording differs from the last one
//! whether or not the CLI moved — so what is compared here is the **text**.
//!
//! The film's script (`assets/cli-demo.tape`) is the input: every `amenbo …` line it types is run,
//! in order, against a throwaway store, and the transcript is compared with the one recorded beside
//! it. A mismatch means the film shows something this build no longer prints, and the answer is to
//! re-record it (`cargo build -p amenbo-cli && vhs assets/cli-demo.tape`) and refresh the transcript
//! with it. That the demo's commands still *succeed* comes along for free: one that started failing
//! is a film of a broken tour.
//!
//! The snapshot is only ever as honest as the recording it was taken beside — writing it from a
//! build the film was not shot with would green-light a stale picture. Refreshing it in the same
//! breath as the `vhs` run is what keeps the two the same age.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The repository root — this crate sits two levels under it.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Every `amenbo …` command the tape types, in the order it types them. The setup and teardown the
/// film hides (`export`, `rm -rf`, `cd`) invoke no amenbo, so naming the binary is enough to pick out
/// exactly the lines the viewer sees run.
fn demo_commands(tape: &str) -> Vec<String> {
    tape.lines()
        .filter_map(|line| line.strip_prefix(r#"Type "amenbo "#))
        .filter_map(|rest| rest.split_once('"').map(|(cmd, _)| format!("amenbo {cmd}")))
        .collect()
}

/// Split a command line into argv, honouring the single quotes the tape uses to hold a title or a
/// filter together. There is no shell here on purpose: the tape's lines have to run the same way on
/// every machine the suite runs on, and a `sh -c` would hand that answer to whatever shell is there.
fn argv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in line.chars() {
        match c {
            '\'' => {
                quoted = !quoted;
                started = true;
            }
            ' ' if !quoted => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            _ => {
                cur.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(cur);
    }
    out
}

/// The one thing in the transcript that moves on its own: `--due +3d` prints the day it lands on.
/// Blanking every date keeps the snapshot readable tomorrow without blunting anything else — the
/// wording around it is what this gate is watching.
fn normalise(text: &str) -> String {
    let bytes: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        let is_date = i + 10 <= bytes.len()
            && bytes[i..i + 4].iter().all(char::is_ascii_digit)
            && bytes[i + 4] == '-'
            && bytes[i + 5..i + 7].iter().all(char::is_ascii_digit)
            && bytes[i + 7] == '-'
            && bytes[i + 8..i + 10].iter().all(char::is_ascii_digit);
        if is_date {
            out.push_str("<date>");
            i += 10;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

/// Run the tape's commands in a store of their own and render what the terminal would show: the
/// prompt line the viewer reads, then what the command answered.
fn transcript(commands: &[String]) -> String {
    let home = amenbo_scratch::scratch("demo-home");
    // The film's working directory is a folder with no `.amenbo` above it — the same isolation the
    // tape sets up, and what makes `init` the first act rather than a no-op.
    let cwd = amenbo_scratch::scratch("demo-cwd").join("website");
    std::fs::create_dir_all(&cwd).expect("create the demo working directory");

    let mut out = String::new();
    for line in commands {
        let args = argv(line);
        let done = Command::new(env!("CARGO_BIN_EXE_amenbo"))
            .args(&args[1..])
            .current_dir(&cwd)
            .env("AMENBO_HOME", &home)
            // No update check: the film is shot offline, and an advisory line would land in the
            // transcript on whichever day upstream published something.
            .env("AMENBO_UPDATE_CHECK", "0")
            .env("NO_COLOR", "1")
            .output()
            .unwrap_or_else(|e| panic!("could not run `{line}`: {e}"));
        assert!(
            done.status.success(),
            "the demo types `{line}`, and this build refuses it:\n{}",
            String::from_utf8_lossy(&done.stderr)
        );
        out.push_str(&format!("$ {line}\n"));
        out.push_str(&String::from_utf8_lossy(&done.stdout));
        out.push_str(&String::from_utf8_lossy(&done.stderr));
        out.push('\n');
    }
    normalise(&out)
}

/// The film and the CLI, side by side.
///
/// Refreshing the snapshot is *deleting* it: a missing one is written from this build, and a present
/// one is only ever compared with. The rewrite is therefore an act with a diff of its own — a file
/// that comes back changed, beside the recording it was taken with — rather than a flag someone
/// reaches for the moment a test goes red.
#[test]
fn the_readme_demo_still_shows_what_this_build_prints() {
    let root = repo_root();
    let tape = std::fs::read_to_string(root.join("assets/cli-demo.tape")).expect("read the tape");
    let commands = demo_commands(&tape);
    assert!(
        commands.len() >= 5,
        "the tape types {} amenbo commands — the parser has lost the film",
        commands.len()
    );

    let actual = transcript(&commands);
    let snapshot = root.join("assets/cli-demo.transcript.txt");
    let Ok(expected) = std::fs::read_to_string(&snapshot) else {
        std::fs::write(&snapshot, &actual).expect("write the transcript");
        return;
    };
    assert_eq!(
        expected, actual,
        "the CLI no longer prints what the README's demo shows. Re-record the film and refresh this \
         transcript together:\n\n    cargo build -p amenbo-cli && vhs assets/cli-demo.tape\n    \
         rm assets/cli-demo.transcript.txt && cargo test -p amenbo-cli --features e2e --test demo_tape\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_lines_the_viewer_sees_run_are_picked_up() {
        let tape = "Hide\nType \"export PATH=x\" Enter\nShow\n\
                    Type \"# a comment the film shows\" Sleep 400ms Enter\n\
                    Type \"amenbo task list --actor human\" Sleep 400ms Enter\n";
        assert_eq!(demo_commands(tape), vec!["amenbo task list --actor human"]);
    }

    #[test]
    fn a_quoted_argument_stays_one_word() {
        assert_eq!(
            argv("amenbo task add --title 'Draft the launch note' --actor human"),
            vec!["amenbo", "task", "add", "--title", "Draft the launch note", "--actor", "human"]
        );
    }

    #[test]
    fn a_date_is_blanked_and_the_wording_around_it_is_not() {
        assert_eq!(normalise("  [ ] AMB-T-1  Draft it due:2026-07-29"), "  [ ] AMB-T-1  Draft it due:<date>");
        assert_eq!(normalise("no date here"), "no date here");
    }
}
