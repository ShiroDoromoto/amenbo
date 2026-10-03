//! **Run a script step's program once** (`AMB-D-1016`) — hand it its inputs, let it run, and take back
//! what it left, without deciding what any of it means.
//!
//! Each run gets a folder of its own under the OS's temporary folder:
//!
//! - `input.json` — `{"version":1,"ins":{…}}`, one entry per input, its path handed in `AMENBO_INPUT`;
//! - `in/<n>/<name>` — a file input, written out so its entry in `ins` can be the path to it;
//! - `out/output.json` — where the program writes, its path handed in `AMENBO_OUTPUT`. A file output is
//!   put beside it in `out/`.
//!
//! The program is started by its path with its arguments as they are written — no shell is in between,
//! so nothing in one is expanded or split. The two variables are set on that process alone, which its
//! children inherit, and it is started without [`crate::session::STEP_VAR`] so an Amenbo it calls is not
//! taken for a step it has no part in. It is started in a group of its own ([`crate::sys::ProcessGroup`]),
//! and one still running at its timeout, or when its caller says it is to stop, is killed with every
//! process it started.
//!
//! **Nothing is written to the store.** A program may run for hours ([`MAX_SCRIPT_TIMEOUT_MINUTES`]), so
//! this runs outside any transaction, and what came back is [`Ran`] for the caller to check against the
//! step's exits and ports and to record ([`crate::ops::automation_report::script_ran`]). Whatever
//! happened, the folder is gone by the time it returns: the files a program left are read into [`Ran`]
//! first.
//!
//! [`MAX_SCRIPT_TIMEOUT_MINUTES`]: crate::model::MAX_SCRIPT_TIMEOUT_MINUTES

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::model::StepScript;

/// The variable the path to `input.json` is handed in.
pub const INPUT_VAR: &str = "AMENBO_INPUT";

/// The variable the path to `output.json` is handed in.
pub const OUTPUT_VAR: &str = "AMENBO_OUTPUT";

/// How much of the end of what a program printed is kept, per stream, in bytes.
pub const TAIL_BYTES: usize = 4096;

/// The version of `input.json` written here.
const INPUT_VERSION: u64 = 1;

/// One input as it is handed to the program, under the name its port was declared with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Given {
    /// Text, written into `input.json` as it is — a value, or the `AMB-T-<n>` of a task.
    Text(String),
    /// A file, written out under `name` and handed as the path to it.
    File { name: String, bytes: Vec<u8> },
}

/// What one run of a program came to.
#[derive(Debug)]
pub struct Ran {
    pub ended: Ended,
    /// The end of what it printed to stdout, at most [`TAIL_BYTES`] of it.
    pub stdout_tail: String,
    /// The end of what it printed to stderr, at most [`TAIL_BYTES`] of it.
    pub stderr_tail: String,
}

/// How a run ended.
#[derive(Debug)]
pub enum Ended {
    /// It was never started: its folder or `input.json` could not be written, or the program could not
    /// be launched. Why, in the OS's words.
    NotStarted(String),
    /// It was still running at its timeout, and was killed.
    TimedOut,
    /// Its caller said it was to stop while it was still running, and it was killed.
    Stopped,
    /// It ended other than with exit code 0.
    Failed(ExitStatus),
    /// It ended with exit code 0 and wrote no `output.json`.
    NoOutput,
    /// It ended with exit code 0, and `output.json` is not JSON. Why, in the parser's words.
    NotJson(String),
    /// It ended with exit code 0 and wrote `output.json`.
    Wrote {
        output: serde_json::Value,
        /// The files it left beside `output.json`, by name, in name order.
        files: Vec<(String, Vec<u8>)>,
    },
}

/// **Run `script` once**, handing it `given`, and stop it at its timeout or as soon as `stop` answers
/// `true` — asked every [`STOP_EVERY`] while it runs.
pub fn run(script: &StepScript, given: &[(String, Given)], stop: impl FnMut() -> bool) -> Ran {
    let minutes = u64::try_from(script.timeout_minutes).unwrap_or(0);
    run_until(script, given, Duration::from_secs(minutes * 60), stop)
}

/// How often a running program's caller is asked whether it is to stop.
pub const STOP_EVERY: Duration = Duration::from_secs(1);

fn run_until(script: &StepScript, given: &[(String, Given)], timeout: Duration, stop: impl FnMut() -> bool) -> Ran {
    let dir = match folder() {
        Ok(dir) => dir,
        Err(e) => return not_started(e.to_string()),
    };
    let ran = run_in(&dir, script, given, timeout, stop);
    let _ = std::fs::remove_dir_all(&dir);
    ran
}

fn not_started(why: String) -> Ran {
    Ran { ended: Ended::NotStarted(why), stdout_tail: String::new(), stderr_tail: String::new() }
}

/// A folder no other run has, under the OS's temporary folder.
fn folder() -> std::io::Result<PathBuf> {
    let mut raw = [0u8; 8];
    getrandom::fill(&mut raw).expect("failed to draw OS randomness");
    let name: String = raw.iter().map(|b| format!("{b:02x}")).collect();
    let dir = std::env::temp_dir().join(format!("amenbo-script-{name}"));
    std::fs::create_dir(&dir)?;
    Ok(dir)
}

fn run_in(
    dir: &Path,
    script: &StepScript,
    given: &[(String, Given)],
    timeout: Duration,
    mut stop: impl FnMut() -> bool,
) -> Ran {
    let input = match write_input(dir, given) {
        Ok(input) => input,
        Err(e) => return not_started(e.to_string()),
    };
    let out = dir.join("out");
    if let Err(e) = std::fs::create_dir(&out) {
        return not_started(e.to_string());
    }
    let output = out.join("output.json");
    let mut command = crate::sys::command(&script.program);
    command
        .args(&script.args)
        .env_remove(crate::session::STEP_VAR)
        .env(INPUT_VAR, &input)
        .env(OUTPUT_VAR, &output)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let (mut child, group) = match crate::sys::ProcessGroup::start(&mut command) {
        Ok(started) => started,
        Err(e) => return not_started(e.to_string()),
    };
    let stdout = Tail::drain(child.stdout.take());
    let stderr = Tail::drain(child.stderr.take());
    let deadline = Instant::now() + timeout;
    let mut asked = Instant::now();
    let status = loop {
        let cut = match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => {
                if asked.elapsed() >= STOP_EVERY {
                    asked = Instant::now();
                    if stop() {
                        Ended::Stopped
                    } else {
                        continue;
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(20));
                    continue;
                }
            }
            _ => Ended::TimedOut,
        };
        group.kill();
        let _ = child.kill();
        let _ = child.wait();
        break Err(cut);
    };
    // A child the program left running may still hold its pipes open, so the readers are waited for
    // only until the deadline, and what they have read by then is what is kept. One that was stopped
    // went with its group, and its pipes closed with it.
    let (stdout_tail, stderr_tail) = (stdout.until(deadline), stderr.until(deadline));
    let ended = match status {
        Err(cut) => cut,
        Ok(status) if !status.success() => Ended::Failed(status),
        Ok(_) => read_output(&out, &output),
    };
    Ran { ended, stdout_tail, stderr_tail }
}

/// Write `input.json`, and each file input beside it, and answer the path to `input.json`.
fn write_input(dir: &Path, given: &[(String, Given)]) -> std::io::Result<PathBuf> {
    let mut ins = serde_json::Map::new();
    for (n, (name, given)) in given.iter().enumerate() {
        let value = match given {
            Given::Text(text) => text.clone(),
            Given::File { name: file, bytes } => {
                let at = dir.join("in").join(n.to_string());
                std::fs::create_dir_all(&at)?;
                let file = Path::new(file).file_name().map(PathBuf::from).unwrap_or_else(|| "file".into());
                let path = at.join(file);
                std::fs::write(&path, bytes)?;
                path.to_string_lossy().into_owned()
            }
        };
        ins.insert(name.clone(), serde_json::Value::String(value));
    }
    let input = serde_json::json!({ "version": INPUT_VERSION, "ins": ins });
    let path = dir.join("input.json");
    std::fs::write(&path, serde_json::to_vec(&input).expect("a JSON value serializes"))?;
    Ok(path)
}

/// Read `output.json` and the files left beside it.
fn read_output(out: &Path, output: &Path) -> Ended {
    let bytes = match std::fs::read(output) {
        Ok(bytes) => bytes,
        Err(_) => return Ended::NoOutput,
    };
    let output = match serde_json::from_slice(&bytes) {
        Ok(output) => output,
        Err(e) => return Ended::NotJson(e.to_string()),
    };
    let mut files = Vec::new();
    for entry in std::fs::read_dir(out).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "output.json" || !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        if let Ok(bytes) = std::fs::read(entry.path()) {
            files.push((name, bytes));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ended::Wrote { output, files }
}

/// The end of one stream a program prints to, read on a thread of its own so a program that prints
/// more than a pipe holds is not stuck waiting for a reader.
struct Tail {
    kept: Arc<Mutex<Vec<u8>>>,
    reader: JoinHandle<()>,
}

impl Tail {
    fn drain(pipe: Option<impl Read + Send + 'static>) -> Tail {
        let kept = Arc::new(Mutex::new(Vec::new()));
        let into = Arc::clone(&kept);
        let reader = std::thread::spawn(move || {
            let Some(mut pipe) = pipe else { return };
            let mut chunk = [0u8; 8192];
            while let Ok(n @ 1..) = pipe.read(&mut chunk) {
                let mut kept = into.lock().unwrap_or_else(|e| e.into_inner());
                kept.extend_from_slice(&chunk[..n]);
                let over = kept.len().saturating_sub(TAIL_BYTES);
                kept.drain(..over);
            }
        });
        Tail { kept, reader }
    }

    /// What was read, once the stream has ended or `deadline` has passed. A character cut in two at the
    /// front is dropped.
    fn until(self, deadline: Instant) -> String {
        while !self.reader.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let kept = self.kept.lock().unwrap_or_else(|e| e.into_inner());
        let start = kept.iter().position(|b| (*b as i8) >= -0x40).unwrap_or(kept.len());
        String::from_utf8_lossy(&kept[start..]).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(program: &str, args: &[&str]) -> StepScript {
        StepScript {
            program: program.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
            timeout_minutes: crate::model::DEFAULT_SCRIPT_TIMEOUT_MINUTES,
        }
    }

    #[test]
    fn a_program_that_is_not_there_is_not_started() {
        let dir = amenbo_scratch::scratch("script-not-there");
        let missing = dir.join("no-such-program");
        let ran = run(&script(&missing.to_string_lossy(), &[]), &[], || false);
        assert!(matches!(ran.ended, Ended::NotStarted(_)), "{:?}", ran.ended);
    }

    #[test]
    fn the_end_of_a_stream_is_kept_whole_characters_only() {
        let text = "あ".repeat(TAIL_BYTES);
        let tail = Tail::drain(Some(std::io::Cursor::new(text.into_bytes())));
        let kept = tail.until(Instant::now() + Duration::from_secs(5));
        assert!(kept.len() <= TAIL_BYTES);
        assert!(kept.chars().all(|c| c == 'あ'), "{kept}");
    }

    #[cfg(unix)]
    mod unix {
        use super::*;

        fn sh(body: &str) -> StepScript {
            script("/bin/sh", &["-c", body])
        }

        /// The folder `input.json` was in, from its path as the program printed it.
        fn folder_of(printed: &str) -> PathBuf {
            PathBuf::from(printed.trim()).parent().expect("a folder").to_path_buf()
        }

        #[test]
        fn the_program_reads_its_inputs_and_what_it_wrote_comes_back() {
            let body = r#"echo "$AMENBO_INPUT" >&2
                cat "$AMENBO_INPUT"
                f=$(sed 's/.*"report":"\([^"]*\)".*/\1/' "$AMENBO_INPUT")
                cp "$f" "$(dirname "$AMENBO_OUTPUT")/copy.txt"
                echo '{"version":1,"exit":"done","outs":{"copy":"copy.txt"},"report":"ok"}' > "$AMENBO_OUTPUT""#;
            let given = [
                ("title".to_string(), Given::Text("hello".to_string())),
                ("report".to_string(), Given::File { name: "a.txt".to_string(), bytes: b"read me".to_vec() }),
            ];
            let ran = run(&sh(body), &given, || false);
            let Ended::Wrote { output, files } = &ran.ended else { panic!("{:?} / {}", ran.ended, ran.stderr_tail) };
            assert_eq!(output["exit"], "done");
            let input: serde_json::Value = serde_json::from_str(&ran.stdout_tail).expect("input.json is JSON");
            assert_eq!(input["version"], 1);
            assert_eq!(input["ins"]["title"], "hello");
            assert!(input["ins"]["report"].as_str().is_some_and(|path| path.ends_with("a.txt")), "{input}");
            assert_eq!(files, &[("copy.txt".to_string(), b"read me".to_vec())]);
            assert!(!folder_of(&ran.stderr_tail).exists(), "the folder is removed");
        }

        #[test]
        fn arguments_reach_the_program_without_a_shell() {
            let ran = run(&script("/bin/echo", &["$HOME", "a b"]), &[], || false);
            assert_eq!(ran.stdout_tail, "$HOME a b\n");
            assert!(matches!(ran.ended, Ended::NoOutput), "{:?}", ran.ended);
        }

        #[test]
        fn a_program_that_ends_other_than_with_zero_failed() {
            let ran = run(&sh("echo said >&2; echo '{}' > \"$AMENBO_OUTPUT\"; exit 4"), &[], || false);
            let Ended::Failed(status) = ran.ended else { panic!("{:?}", ran.ended) };
            assert_eq!(status.code(), Some(4));
            assert_eq!(ran.stderr_tail, "said\n");
        }

        #[test]
        fn output_that_is_not_json_is_told_apart() {
            let ran = run(&sh("echo 'not json' > \"$AMENBO_OUTPUT\""), &[], || false);
            assert!(matches!(ran.ended, Ended::NotJson(_)), "{:?}", ran.ended);
        }

        /// A program that prints where its `input.json` is, starts a child that would outlive it, prints
        /// the child's pid, and waits.
        const WITH_A_CHILD: &str = "echo \"$AMENBO_INPUT\"; sleep 30 & echo $!; wait";

        /// The folder and the child's pid, from what [`WITH_A_CHILD`] printed.
        fn printed(stdout: &str) -> (PathBuf, libc::pid_t) {
            let mut lines = stdout.lines();
            let folder = folder_of(lines.next().expect("the input path"));
            let pid = lines.next().expect("the child's pid").trim().parse().expect("a pid");
            (folder, pid)
        }

        /// Is the process `pid` gone — given a few seconds, since a killed orphan is collected by the OS.
        fn gone(pid: libc::pid_t) -> bool {
            let until = Instant::now() + Duration::from_secs(5);
            while Instant::now() < until {
                // SAFETY: signal 0 only asks whether the process is there.
                if unsafe { libc::kill(pid, 0) } != 0 {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            false
        }

        #[test]
        fn a_program_still_running_at_its_timeout_is_killed_with_its_children_and_its_folder_removed() {
            let started = Instant::now();
            let ran = run_until(&sh(WITH_A_CHILD), &[], Duration::from_millis(500), || false);
            assert!(matches!(ran.ended, Ended::TimedOut), "{:?}", ran.ended);
            assert!(started.elapsed() < Duration::from_secs(10));
            let (folder, child) = printed(&ran.stdout_tail);
            assert!(gone(child), "the child went with the program");
            assert!(!folder.exists(), "the folder is removed");
        }

        #[test]
        fn a_program_its_caller_stops_is_killed_with_its_children_and_its_folder_removed() {
            let started = Instant::now();
            let ran = run_until(&sh(WITH_A_CHILD), &[], Duration::from_secs(60), || true);
            assert!(matches!(ran.ended, Ended::Stopped), "{:?}", ran.ended);
            assert!(started.elapsed() < Duration::from_secs(10));
            let (folder, child) = printed(&ran.stdout_tail);
            assert!(gone(child), "the child went with the program");
            assert!(!folder.exists(), "the folder is removed");
        }

        #[test]
        fn only_the_end_of_what_it_printed_is_kept() {
            let ran = run(&sh("i=0; while [ $i -lt 2000 ]; do echo line $i; i=$((i+1)); done"), &[], || false);
            assert!(ran.stdout_tail.len() <= TAIL_BYTES);
            assert!(ran.stdout_tail.ends_with("line 1999\n"), "{}", ran.stdout_tail);
        }
    }
}
