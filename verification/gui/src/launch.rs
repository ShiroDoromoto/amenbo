//! Launching the GUI under test, and holding on to it for as long as the run lasts.
//!
//! The harness starts the app itself, and the pid it shoots is the one the launch handed back.
//! Nothing on the machine separates a shipped build started for a run from the same shipped build
//! the user keeps open: the executable name is one name, the bundle is one bundle, there is no badge
//! on screen, and nothing stops both from running at once. A run that goes looking for a process can
//! therefore shoot the wrong window, and the evidence it files is indistinguishable from the real
//! thing. Launching is what makes the answer certain — whatever else is running, this pid is ours.
//!
//! The executable inside the bundle is run directly rather than the bundle being `open`ed, because
//! the environment is what points the app at a throwaway store, and `open` hands the launch to
//! launchd with an environment of its own.
//!
//! A bundle carries two programs, and this module answers for the app. The CLI beside it — what a
//! run asks which build this is, and stands the scenario's world up with — is [`crate::shipped`]'s.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::scratch::Session;
use crate::{front, shoot};

/// How long a launched app is given to put a window on screen. Generous on purpose: a first launch
/// of a signed bundle is checked by the system before a line of the app runs, and a wait that ends
/// early reports a slow machine as a broken build.
const WINDOW_TIMEOUT: Duration = Duration::from_secs(60);

/// How long to wait between asks while the app is starting. Each ask runs the screen tool, which is
/// a swift invocation, so the wait between them is the smaller half of the cost.
const POLL: Duration = Duration::from_millis(500);

/// The app this run launched: the process it holds, and the store that process was pointed at.
/// Dropping it takes the app down, so a run that gives up anywhere leaves nothing running.
///
/// The store is borrowed, and the borrow is the guarantee: a store the app is writing into cannot
/// be swept while this is alive. Owning it belongs to the run rather than to the launch, since the
/// premise is stood up in that same store before there is an app to launch at all.
#[derive(Debug)]
pub struct Gui<'a> {
    /// The process id the launch answered with — the app the screen tool is named.
    pub pid: i64,
    child: Child,
    store: &'a Session,
}

/// Launch the app in `bundle` against `store`, and hand back the running process.
///
/// The app's stdout is dropped rather than inherited: a `--json` run answers with one machine
/// readable line, and an app writing to the same stream would be read as part of it. Its stderr is
/// left alone, which is where a build that dies on launch says why.
///
/// `AMENBO_HOME` carries a second meaning beside the store it names: a launch that names its own
/// store is the one the app leaves out of its single-instance guard. That is what lets this one come
/// up beside whatever the operator already has open, instead of being turned away into their window.
pub fn launch<'a>(bundle: &Path, store: &'a Session) -> Result<Gui<'a>, String> {
    let exe = executable(bundle)?;
    let child = Command::new(&exe)
        .env("AMENBO_HOME", &store.home)
        .current_dir(&store.cwd)
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| format!("could not launch {}: {e}", exe.display()))?;
    Ok(Gui { pid: i64::from(child.id()), child, store })
}

impl Gui<'_> {
    /// Hold the run until the app is up, in front, and can actually be shot.
    ///
    /// The proof asked for is a shot, because that is what every step of the walk asks for: an app
    /// the system has taken up is not yet an app with a window, and a run that started walking
    /// between the two would fail on step one with half an evidence directory behind it. The shot
    /// taken here is thrown away — it is asked for as a question, not kept as evidence — and
    /// fronting is asked for with it, since a window standing behind another Space is not one the
    /// tool will find. An app that exits on the way up is reported the moment it does, rather than
    /// after a timeout spent asking about a process that is already gone.
    pub fn wait_until_shootable(&mut self, screen: &Path) -> Result<(), String> {
        let probe = self.store.cwd.join("on-screen-probe.png");
        let deadline = Instant::now() + WINDOW_TIMEOUT;
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                return Err(format!(
                    "the app exited while starting ({status}) — no window was ever put on screen"
                ));
            }
            let last = match front(self.pid, screen).and_then(|()| shoot(self.pid, &probe, screen)) {
                Ok(()) => {
                    let _ = std::fs::remove_file(&probe);
                    return Ok(());
                }
                Err(e) => e,
            };
            if Instant::now() >= deadline {
                return Err(format!(
                    "the app put no window on screen within {}s: {last}",
                    WINDOW_TIMEOUT.as_secs()
                ));
            }
            std::thread::sleep(POLL);
        }
    }
}

impl Drop for Gui<'_> {
    /// Take the app down with the run. It is killed rather than asked to quit: asking goes through
    /// the app's name, which is the one thing that cannot name a single instance, and the store it
    /// was writing into is thrown away in the same breath — so there is nothing a graceful close
    /// would preserve.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The executable inside a mac app bundle, as the bundle itself names it (`CFBundleExecutable`).
/// Asked rather than assumed: a dev build carries an executable name of its own, and a harness that
/// assumed the shipped one would launch nothing while reporting the bundle it was handed.
fn executable(bundle: &Path) -> Result<PathBuf, String> {
    let plist = bundle.join("Contents/Info.plist");
    let out = Command::new("plutil")
        .arg("-extract")
        .arg("CFBundleExecutable")
        .arg("raw")
        .arg(&plist)
        .output()
        .map_err(|e| format!("could not run plutil on {}: {e}", plist.display()))?;
    if !out.status.success() {
        return Err(format!(
            "`{}` is not an app bundle to launch: {} names no executable ({})",
            bundle.display(),
            plist.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let exe = bundle.join("Contents/MacOS").join(&name);
    if !exe.is_file() {
        return Err(format!(
            "`{}` names `{name}` as its executable, and there is nothing at {}",
            bundle.display(),
            exe.display()
        ));
    }
    Ok(exe)
}

// A bundle is a mac shape and `plutil` is a mac tool, so these run where the harness itself does.
// The workspace's own CI is Linux, which is why the rest of this crate's tests inject their side
// effects: this is the one part whose subject is the side effect.
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A bundle in the shape the launcher reads: a plist naming the executable, and a script
    /// standing in for the app at that name.
    fn bundle(dir: &Path, name: &str, body: &str) -> PathBuf {
        let app = dir.join("stand-in.app");
        let macos = app.join("Contents/MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        std::fs::write(
            app.join("Contents/Info.plist"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleExecutable</key><string>{name}</string></dict></plist>
"#
            ),
        )
        .unwrap();
        let exe = macos.join(name);
        std::fs::write(&exe, body).unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        app
    }

    /// The app is started against the store it was given — its `AMENBO_HOME` and the directory it
    /// runs in are both the run's own — and it goes down when the run lets go, the store following
    /// it out when the run lets go of that.
    #[test]
    fn the_app_runs_in_the_throwaway_store_and_goes_down_with_the_run() {
        let store = crate::scratch::session("selftest-launch", false).unwrap();
        let (home, cwd) = (store.home.clone(), store.cwd.clone());
        // Report the world it was handed, then stay up: `exec` puts the wait in this process, so the
        // pid the launcher holds is the one that has to be taken down.
        let app = bundle(
            &cwd,
            "stand-in-app",
            "#!/bin/sh\nprintf '%s\\n%s\\n' \"$AMENBO_HOME\" \"$PWD\" > \"$AMENBO_HOME/launched\"\nexec sleep 300\n",
        );

        let gui = launch(&app, &store).unwrap();
        let said = home.join("launched");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !said.is_file() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let said = std::fs::read_to_string(&said).expect("the app it launched said where it stands");
        let mut lines = said.lines();
        assert_eq!(lines.next(), Some(home.to_str().unwrap()), "pointed at the throwaway store");
        // The temp tree is reached through a symlink on a mac, and a shell answers `$PWD` with the
        // path it resolves to — the same directory, spelled the way the kernel spells it.
        let stood_in = lines.next().map(|l| std::fs::canonicalize(l).unwrap());
        assert_eq!(stood_in, Some(std::fs::canonicalize(&cwd).unwrap()), "run from the throwaway dir");

        let pid = gui.pid;
        assert!(alive(pid), "the app is up while the run holds it");
        drop(gui);
        assert!(!alive(pid), "and down when the run lets go");
        drop(store);
        assert!(!home.exists(), "the store goes with it");
    }

    /// A path that is not a bundle is refused at the door, naming what was looked for — a launcher
    /// that shrugged would leave the run waiting a minute for a window nothing was going to draw.
    #[test]
    fn a_path_that_is_no_bundle_is_refused_by_name() {
        let store = crate::scratch::session("selftest-nobundle", false).unwrap();
        let empty = store.cwd.join("not-an.app");
        std::fs::create_dir_all(&empty).unwrap();
        let err = launch(&empty, &store).unwrap_err();
        assert!(err.contains("not-an.app"), "the refusal names the path it was handed: {err}");
    }

    fn alive(pid: i64) -> bool {
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}
