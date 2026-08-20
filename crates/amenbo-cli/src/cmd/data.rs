//! The commands that move the store's data as a whole — `export`, `sync`, `backup`, `restore` —
//! the migration every startup passes through, and the progress lines the long ones print.

use serde_json::json;

use amenbo_core::config::Paths;
use amenbo_core::{time, Store};

use crate::cli::*;
use crate::output::{confirm, human, print_json, CliError, CliErrorCode, Flags};

/// Where an export goes when the caller named no destination and the stream shape is not on offer: a fresh,
/// timestamped directory under the current one. The name carries the moment so a second export never lands
/// on the first — `export_bundle` refuses a destination that already exists, and quietly overwriting
/// someone's data is not Amenbo's to do.
fn default_export_dir() -> String {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    format!("amenbo-export-{stamp}")
}

/// `export` — one shape only: everything, as JSON. Export exists to hand the data to whatever the user moves
/// to next, and an excerpt or a human-readable table does not serve that, so neither exists. With `--out` it
/// writes the export directory: `export.json` plus `attachments/` holding every attachment's bytes under the
/// target it hangs on — the complete migration artifact, since export is one-way and metadata alone would
/// lose the files. With no `--out` it streams the same JSON to stdout so the dump can be piped; a stream has
/// nowhere to put the bytes, so that shape carries records only and says so. `--json` prints a one-line
/// completion summary (payload always JSON regardless). The stream shape is the one place where the whole
/// device's content lands in the caller's terminal — so a closed reach (an AI) does not get it. It is not
/// refused: taking the data out is the user's, and their AI's, right (no lock-in), and refusing would read
/// as "you cannot export". Instead the destination is chosen for it and the export goes to a file, which
/// returns only a path and a count. What the AI can then do with that file is raw file access, which Amenbo
/// does not stop.
///
/// **A plugin's window is refused the command outright** (`Reach::refuse_whole_device`, `AMB-D-406`). The
/// reasoning above turns on whose data is being taken out: an AI acts for the user whose device this is,
/// so narrowing its door is enough. A plugin moves to no other tool, and the window it reads through was
/// fixed by the runner that launched it — so the whole device is not a wider reading of what it was
/// launched to observe, it is the way around it.
pub(crate) fn export(store: &Store, flags: &Flags, out: Option<String>) -> Result<i32, CliError> {
    use amenbo_core::export;
    store.reach().refuse_whole_device("export").map_err(CliError::from)?;
    let out = match out {
        Some(path) => Some(path),
        None if store.reach().project().is_some() => Some(default_export_dir()),
        None => None,
    };
    match out {
        Some(path) => {
            let mut progress = progress_fn(flags);
            let report =
                export::export_bundle(std::path::Path::new(&path), &mut progress).map_err(|e| {
                    CliError {
                        code: "export_error",
                        message: e.to_string(),
                        hint: Some("Pick a destination that does not exist yet.".to_string()),
                        exit: 1,
                    }
                })?;
            human(flags, format!(
                "✓ Export written to {} ({} attachment(s))",
                report.path, report.attachments
            ));
            if report.missing > 0 {
                human(flags, format!(
                    "  ⚠ {} attachment(s) had no bytes left on disk — their export_path is null",
                    report.missing
                ));
            }
            if flags.json {
                print_json(&json!({
                    "ok": true, "action": "export", "noop": false, "out": report.path,
                    "bytes": report.bytes, "attachments": report.attachments, "missing": report.missing,
                }));
            }
        }
        None => {
            let stdout = std::io::stdout();
            let mut w = stdout.lock();
            let mut progress = progress_fn(flags);
            export::export_json(&mut w, &mut progress).map_err(|e| CliError {
                code: "export_error",
                message: e.to_string(),
                hint: None,
                exit: 1,
            })?;
            // stdout carries records only — never let that pass for the whole migration artifact. The
            // note goes to stderr: stdout is the dump itself and must stay pipeable.
            if !flags.json && !flags.quiet {
                eprintln!(
                    "note: attachment files are not in this stream — run `{} export --out <dir>` to take them with you",
                    Paths::command_name()
                );
            }
        }
    }
    Ok(0)
}

/// `sync` — the road out for a plugin that carries this store's data somewhere else (`AMB-D-581`), in the
/// four faces a carrier actually uses: **ask the version**, take a **snapshot** only when it moved, from
/// the position that snapshot names read on through **changes**, and read what those changes named back
/// through **records**.
///
/// The split is the point, not a convenience. A carrier has to ask often and send rarely, so the asking
/// must not cost what the sending costs: `version` reads one row and never builds a snapshot
/// (`AMB-D-582`), and `changes` re-reads only what moved rather than the window entire — which `records`
/// is what makes possible, since the ledger carries no values and a carrier with nowhere to take the ids
/// it was handed would be back to taking the whole window. All of them answer
/// **through the reach this surface already holds** — a plugin's window (`AMB-D-406`), an AI's binding, or
/// the whole device for a human — so none needs a door of its own, and none can be widened by an argument.
///
/// **None is refused to a window, unlike `export` and `backup`.** Those act on the whole device and so
/// step past what a plugin was launched to observe; these are that window, answered. Nor is a facet
/// required on the plugin face: they read and record nothing, so there is no actor for one to name
/// (`stamps_facet`).
///
/// **`snapshot`'s stdout is the document, and `records`' is too.** Anything else Amenbo has to say goes to
/// stderr, so the stream stays pipeable — the same rule `export`'s stream shape follows. Neither consults
/// `--json`: the document is the answer either way, and there is no second shape to ask for.
pub(crate) fn sync_cmd(store: &Store, flags: &Flags, sub: SyncCmd) -> Result<i32, CliError> {
    match sub {
        SyncCmd::Version => {
            let version = store.sync_version().map_err(CliError::from)?;
            if flags.json {
                // The window is named beside the number: two carriers on one device hold numbers from
                // different windows, and only this says which one this is.
                print_json(&json!({ "version": version, "project_id": store.reach().project() }));
            } else {
                // The number *is* the answer, not a success message — so `--quiet` does not eat it.
                println!("{version}");
            }
        }
        SyncCmd::Changes { since } => return sync_changes(store, flags, since),
        SyncCmd::Records { dataset, ids } => {
            let stdout = std::io::stdout();
            let mut w = stdout.lock();
            // Refusals land before the first byte (`records_from`), so an error here never leaves half a
            // document on a carrier's stdout. `sync_error` is the road's own code, as the snapshot's is.
            amenbo_core::sync_snapshot::stream_records(store.reach(), &dataset, &ids, &mut w).map_err(
                |e| CliError {
                    code: CliErrorCode::SyncError.as_str(),
                    message: e.to_string(),
                    hint: None,
                    exit: 1,
                },
            )?;
        }
        SyncCmd::Snapshot => {
            let stdout = std::io::stdout();
            let mut w = stdout.lock();
            amenbo_core::sync_snapshot::stream(store.reach(), &mut w).map_err(|e| CliError {
                code: CliErrorCode::SyncError.as_str(),
                message: e.to_string(),
                hint: None,
                exit: 1,
            })?;
            // Never let the stream pass for everything the window holds. The note goes to stderr, and
            // `--json` silences it: under that flag stdout is being read by a program, which was told
            // this by the row it is holding.
            if !flags.json && !flags.quiet {
                eprintln!(
                    "note: attachment files are not in this snapshot — each row names the bytes it stands for, but the bytes stay here",
                );
            }
        }
    }
    Ok(0)
}

/// The payload version this road speaks (`AMB-D-349`): one integer, at the front, raised only when the
/// shape changes in a way a reader built on the old one cannot survive. A field added later does not
/// raise it — a carrier ignores what it does not know.
const SYNC_CHANGES_V: u32 = 1;

/// How many changes one call hands back. A carrier that has been away drains in pages, watching `more`
/// and coming back with the cursor it was given, so this bounds what either side has to hold rather than
/// what either side can learn. The ledger's own retention is thousands of rows, so a page this size is a
/// handful of calls even for a carrier that has been away a long time.
const SYNC_CHANGES_PAGE: i64 = 500;

/// `sync changes --since <cursor>`: **what moved in this window since the cursor**, and the cursor to come
/// back with (`AMB-D-582`). The last of the three a carrier walks — the version says whether to come, the
/// snapshot hands over the whole, and this hands over what has happened since.
///
/// What it names is which records moved and how, never what they now hold: the ledger carries no values
/// by construction (`AMB-D-367`), so a carrier reads a changed record back by name and gets the current
/// one. `delete` is the arm that makes the road work at all — there is nothing left to read back, and a
/// carrier that had to notice by re-reading everything it holds would be asking after the whole window on
/// every pass.
///
/// **The window is the reach's, and no argument widens it** — the same standing as `version` and
/// `snapshot` beside it. Nothing here names a project, so there is nothing to refuse: the answer is
/// simply that window's, or the device's for a human.
///
/// **A gap is not an empty page.** A cursor outside what the ledger can speak for — fallen behind the
/// window it keeps, or ahead of anything it has ever reached — has changes it will never be handed, and
/// answering nothing would be indistinguishable from nothing having happened, leaving the copy outside
/// stale and confident. So it is said in a code the caller can branch on (`sync_gap`) with a non-zero
/// exit, and the way on (a fresh snapshot, which names its own cursor) is in the hint: one operation
/// fixes it, whatever went wrong (`AMB-D-583`).
fn sync_changes(store: &Store, flags: &Flags, since: i64) -> Result<i32, CliError> {
    use amenbo_core::store::SyncChanges;

    let window = store.reach().project();
    let (rows, cursor, more) = match store.sync_changes(since, SYNC_CHANGES_PAGE).map_err(CliError::from)? {
        SyncChanges::Changes { rows, cursor, more } => (rows, cursor, more),
        SyncChanges::Gap => {
            return Err(CliError {
                code: CliErrorCode::SyncGap.as_str(),
                message: format!(
                    "the ledger cannot say what changed since {since} — that cursor is outside the \
                     stretch it still speaks for",
                ),
                hint: Some(
                    "Take the window again with `amenbo sync snapshot` and read on from the cursor its \
                     header names."
                        .to_string(),
                ),
                exit: 1,
            })
        }
    };

    if flags.json {
        // The window is named beside the answer, as `version`'s is: two carriers on one device hold
        // cursors from different windows, and only this says which one this page is of.
        print_json(&json!({
            "v": SYNC_CHANGES_V,
            "project_id": window,
            "cursor": cursor,
            "more": more,
            "changes": rows
                .iter()
                .map(|r| json!({ "dataset": r.dataset, "record_id": r.row_id, "op": r.op }))
                .collect::<Vec<_>>(),
        }));
    } else {
        // The changes *are* the answer, not a success message — so `--quiet` does not eat them, exactly
        // as it does not eat `version`'s number.
        for r in &rows {
            println!("{:<8} {:<16} {}", r.op, r.dataset, r.row_id);
        }
        println!("cursor: {cursor}{}", if more { " (more waiting)" } else { "" });
    }
    Ok(0)
}

/// Backup: stream a verified snapshot of this device's store into one `.amenbo-backup` archive at `path`.
/// A destination is required (the archive is a deliberate, self-placed disaster-recovery file), so an
/// omitted `path` is refused with a hint.
///
/// **A plugin's window is refused it** (`Reach::refuse_whole_device`, `AMB-D-406`): the archive holds every
/// project on the device, so writing one is the whole store leaving through a door that was opened to
/// observe a single project. `AMB-D-224` allowed it to the AI facet as the disaster recovery an agent is
/// there to run, which is not work a plugin was launched for.
pub(crate) fn run_backup(store: &Store, flags: &Flags, path: Option<String>) -> Result<i32, CliError> {
    use amenbo_core::archive;
    store.reach().refuse_whole_device("backup").map_err(CliError::from)?;
    let Some(path) = path else {
        return Err(CliError {
            code: "missing_required_flag",
            message: "backup needs a destination path".to_string(),
            hint: Some(format!("Run `{} backup <path>.{}`.", Paths::command_name(), archive::ARCHIVE_EXT)),
            exit: 2,
        });
    };
    let dest = std::path::Path::new(&path);
    let Some(source) = archive::enumerate_store() else {
        return Err(CliError {
            code: "backup_error",
            message: "found no store to back up on this device".to_string(),
            hint: Some(format!("Create or bind a store first (`{} init`).", Paths::command_name())),
            exit: 1,
        });
    };
    // Beyond the reach above, the opened store is not what is copied: backup reads the on-disk layout,
    // and the open is the exec guard.
    let mut progress = progress_fn(flags);
    let report = archive::backup_from(&source, dest, &mut progress).map_err(|e| CliError {
        code: "backup_error",
        message: e.to_string(),
        // A destination that is a directory is the one failure the generic hint misleads on: "pick one
        // that does not exist yet" reads as "delete that folder". Show the shape wanted instead. The
        // check holds after the fact too — a directory could only have been one before the run.
        hint: Some(if dest.is_dir() {
            format!(
                "Give a file path, not a folder — e.g. `{} backup {}/mystore.{}`.",
                Paths::command_name(),
                dest.display(),
                archive::ARCHIVE_EXT
            )
        } else {
            "Pick a destination that does not exist yet.".to_string()
        }),
        exit: 1,
    })?;
    if flags.json {
        print_json(&report);
    } else {
        human(flags, format!(
            "✓ Backup written to {} ({} bytes, {} attachment(s))",
            report.path, report.bytes, report.blobs
        ));
    }
    Ok(0)
}

/// What a phase is called on the progress line. `Verifying` has no name because it has nothing to report:
/// it is a single statement, so its one tick would be a line that is already over by the time it is read.
fn progress_verb(phase: amenbo_core::progress::Phase) -> Option<&'static str> {
    use amenbo_core::progress::Phase;
    match phase {
        Phase::Snapshotting => Some("backing up"),
        Phase::Copying => Some("restoring"),
        Phase::Exporting => Some("exporting"),
        Phase::Migrating => Some("migrating"),
        Phase::Blobs => Some("attachments"),
        Phase::Unpacking => Some("unpacking"),
        Phase::Verifying => None,
    }
}

/// How many lines a phase may spend at most, when its total is known — the budget the throttle divides.
const PROGRESS_LINES_PER_PHASE: u64 = 10;

/// How many units an unbounded phase (a streaming export, which counts rows it has not pre-counted) covers
/// between lines. A row is small, so a line every few hundred is a pulse, not a flood.
const PROGRESS_UNBOUNDED_STEP: u64 = 500;

/// Decides which ticks earn a line — see [`progress_fn`]. Holds the last line's `(phase, done)` so the
/// throttle can tell a phase's first tick from its hundredth.
#[derive(Default)]
struct ProgressLines {
    last: Option<(amenbo_core::progress::Phase, u64)>,
}

impl ProgressLines {
    /// The line this tick is worth, or `None` to stay silent.
    fn line(&mut self, p: &amenbo_core::progress::Progress) -> Option<String> {
        let verb = progress_verb(p.phase)?;
        let entering = self.last.is_none_or(|(phase, _)| phase != p.phase);
        let due = match (p.total, self.last) {
            // Bounded: spend the budget evenly, and always report the last unit — a phase that stops at
            // `[120/131]` reads as one that gave up there.
            (Some(total), _) => {
                let step = (total / PROGRESS_LINES_PER_PHASE).max(1);
                p.done.is_multiple_of(step) || p.done + 1 == total
            }
            (None, Some((_, last))) => p.done >= last + PROGRESS_UNBOUNDED_STEP,
            (None, None) => true,
        };
        if !entering && !due {
            return None;
        }
        self.last = Some((p.phase, p.done));
        Some(match p.total {
            Some(total) => format!("  [{}/{}] {verb}", p.done + 1, total),
            None => format!("  [{}] {verb}", p.done + 1),
        })
    }
}

/// A progress sink for the bulk ops: a line to stderr, so `--json` output on stdout stays clean and
/// `--quiet` silences it. Never cancels (the CLI has no interactive interrupt here). The phase names the
/// line, not the command that started it — a command is not one phase (a migration takes a pre-migration
/// backup and then walks the chain), so a verb fixed by the caller would show one counter apparently
/// restarting mid-run. A line per tick is not an option, and neither is silence: the phases that carry the
/// bytes — [`amenbo_core::progress::Phase::Blobs`] and [`amenbo_core::progress::Phase::Unpacking`] — tick
/// once per attachment, so a line each drowns the run, while dropping them leaves the longest stretch of a
/// multi-GB restore with nothing on the terminal at all. So the ticks are thinned ([`ProgressLines`]) to a
/// handful of lines per phase: enough to see it move, few enough to read.
pub(crate) fn progress_fn(flags: &Flags) -> impl FnMut(&amenbo_core::progress::Progress) -> std::ops::ControlFlow<()> + '_ {
    use amenbo_core::progress::Progress;
    let mut lines = ProgressLines::default();
    move |p: &Progress| {
        if !flags.json && !flags.quiet {
            if let Some(line) = lines.line(p) {
                eprintln!("{line}");
            }
        }
        std::ops::ControlFlow::Continue(())
    }
}

/// The CLI's half of the one execution site: carry this device's store forward before the command opens it,
/// whichever surface got here first. Everything about how is core's ([`amenbo_core::migrate::at_startup`] —
/// the lock the other surface waits on, the pre-migration backup, the rollback). What belongs here is only
/// what a terminal owes the human, on stderr so `--json` keeps stdout clean: before, what it will do and
/// what it will cost (`ensure_space` refuses a disk that cannot hold the backup with those same numbers in
/// the error, but a refusal is a bad first sight of them); after, where the backup went (the only way back —
/// there is no downgrade) and that older builds can no longer open this store. A store that is already
/// current is silent: it is the common case, and it has nothing to say.
pub(crate) fn migrate_at_startup(flags: &Flags) -> Result<(), CliError> {
    use amenbo_core::migrate::Pending;

    let mut announce = |p: &Pending| {
        if flags.quiet {
            return;
        }
        eprintln!(
            "Updating this device's store: format v{} → v{} ({} step(s)). Taking a pre-migration backup first (~{} MiB needed, ~{} MiB free).",
            p.from,
            p.to,
            p.steps,
            p.plan.required_bytes.div_ceil(1024 * 1024),
            p.plan.available_bytes.div_ceil(1024 * 1024),
        );
    };
    let mut progress = progress_fn(flags);
    let report = amenbo_core::migrate::at_startup(&mut announce, &mut progress).map_err(|e| CliError {
        code: "migrate_error",
        message: e.to_string(),
        hint: Some("The store was left as it was; nothing is half-migrated.".to_string()),
        exit: 1,
    })?;

    let Some(report) = report.filter(|r| r.migrated()) else { return Ok(()) };
    if !flags.quiet {
        eprintln!("✓ Store updated to format v{}.", report.run.to);
        if let Some(backup) = &report.backup {
            eprintln!("  The store as it was is kept at {} (the only way back — there is no downgrade).", backup.path);
        }
        // One rewind point, the newest. Say what went, so a deleted copy is never a silent one.
        if !report.superseded.is_empty() {
            eprintln!(
                "  Removed {} pre-migration backup(s) this one supersedes (nothing can go back past the newest).",
                report.superseded.len()
            );
        }
        eprintln!(
            "  Older Amenbo builds can no longer open this store — update them (`{} update`, or reinstall from the latest installer; GUI and CLI ship together).",
            Paths::command_name()
        );
    }
    Ok(())
}

/// Restore: destructively replace this device's store with the one the `.amenbo-backup` archive at `path`
/// carries, via core [`amenbo_core::archive::restore_into`] — all-or-nothing stage-and-swap, the archive's
/// store carried up the version chain in staging when it was taken by an older build, and the replaced truth
/// source set aside as `store.pre-restore-<stamp>.sqlite`. Destructive — confirms unless `--yes`.
///
/// Takes no [`Store`]: it writes the on-disk layout rather than the opened store, and it runs **ahead of**
/// the open so it still works on the store the open refuses — see the dispatch in [`run`](crate::run).
pub(crate) fn run_restore(
    flags: &Flags,
    path: Option<String>,
) -> Result<i32, CliError> {
    use amenbo_core::archive;
    let Some(path) = path else {
        return Err(CliError {
            code: "missing_required_flag",
            message: "restore needs the archive path".to_string(),
            hint: Some(format!("Run `{} restore <path>.{}`.", Paths::command_name(), archive::ARCHIVE_EXT)),
            exit: 2,
        });
    };
    let archive = std::path::Path::new(&path);
    // Ask the release-stamp gate up front (`AMB-D-378`). Core refuses this restore anyway — the gate lives
    // with the code that migrates — but asking here keeps its message intact: it names the three ways
    // through, and reaching it through the wrapper below would bury them under the too-new-archive hint.
    amenbo_core::build_stamp::ensure_may_migrate().map_err(CliError::from)?;
    // Read the manifest before the destructive prompt: it is cheap (no extraction) and it is where an
    // archive this build cannot read is refused, so the user is not asked to consent to a restore that
    // was never going to run.
    let manifest = archive::read_manifest(archive).map_err(|e| CliError {
        code: "restore_error",
        message: e.to_string(),
        hint: Some(format!("Pass a .amenbo-backup archive produced by `{} backup`.", Paths::command_name())),
        exit: 1,
    })?;
    if !confirm(
        flags,
        &format!(
            "destructively replace this device's store from {} (taken {}; the current truth source is set aside as a timestamped backup)",
            path, manifest.created_at
        ),
    )? {
        return Ok(1);
    }
    let stamp = time::Timestamp::now().0.format("%Y%m%dT%H%M%SZ").to_string();
    let mut progress = progress_fn(flags);
    let report = archive::restore_into(archive, &stamp, &archive::restore_dest(), &mut progress)
        .map_err(|e| CliError {
            code: "restore_error",
            message: e.to_string(),
            hint: Some(format!("On a too-new archive, update Amenbo first (`{} update`).", Paths::command_name())),
            exit: 1,
        })?;
    if flags.json {
        print_json(&report);
    } else {
        human(flags, format!("✓ Restore complete ({} attachment(s) written)", report.blobs));
        if let Some(prev) = &report.previous_saved_to {
            human(flags, format!("  Previous truth source set aside at {prev}"));
        }
        // The new aside is this store's rewind point, so the older ones were not kept.
        if !report.superseded.is_empty() {
            human(
                flags,
                format!(
                    "  Removed {} earlier set-aside store(s) this one supersedes",
                    report.superseded.len()
                ),
            );
        }
        // An archive taken by an older build is carried up the version chain on the way in. Say so: the
        // store the user gets back is not, byte for byte, the store they backed up.
        let m = &report.migration;
        if m.migrated() {
            human(
                flags,
                format!(
                    "  Archive brought forward from format v{} to v{} ({})",
                    m.from,
                    m.to,
                    m.applied.join(", ")
                ),
            );
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Thin the progress ticks: feed in every tick, get back only the ones that earned a line.
    fn thinned(ticks: &[amenbo_core::progress::Progress]) -> Vec<String> {
        let mut lines = ProgressLines::default();
        ticks.iter().filter_map(|p| lines.line(p)).collect()
    }

    fn tick(phase: amenbo_core::progress::Phase, done: u64, total: Option<u64>) -> amenbo_core::progress::Progress {
        amenbo_core::progress::Progress { phase, done, total }
    }

    /// A per-attachment tick is not a unit of output: unpacking 131 files must not print 131 lines, but a
    /// handful — first and last among them. Lines that scroll past take the shape of the run with them.
    #[test]
    fn a_per_attachment_phase_is_thinned_to_a_handful_of_lines() {
        use amenbo_core::progress::Phase;
        let ticks: Vec<_> = (0..131).map(|i| tick(Phase::Unpacking, i, Some(131))).collect();
        let lines = thinned(&ticks);
        assert!(
            lines.len() <= PROGRESS_LINES_PER_PHASE as usize + 2,
            "131 ticks turned into {} lines: {lines:?}",
            lines.len()
        );
        assert_eq!(lines.first().unwrap(), "  [1/131] unpacking", "it must speak on the first tick");
        assert_eq!(lines.last().unwrap(), "  [131/131] unpacking", "it must report through to the last unit");
    }

    /// A phase that does not know its total (a streaming export never pre-counts its rows) still speaks: it
    /// pulses at a fixed stride and prints the count alone, never inventing a denominator like `[5/0]`.
    #[test]
    fn an_unbounded_phase_pulses_by_count_without_inventing_a_total() {
        use amenbo_core::progress::Phase;
        let ticks: Vec<_> = (0..2000).step_by(256).map(|i| tick(Phase::Exporting, i, None)).collect();
        let lines = thinned(&ticks);
        assert_eq!(lines.first().unwrap(), "  [1] exporting");
        assert!(lines.len() >= 2 && lines.len() < ticks.len(), "it pulses, but not on every tick: {lines:?}");
        assert!(lines.iter().all(|l| !l.contains('/')), "no denominator when the total is unknown: {lines:?}");
    }

    /// Entering a phase always earns a line, mid-stride or not — what is being done next is needed before how
    /// far along it is.
    #[test]
    fn entering_a_phase_always_earns_a_line() {
        use amenbo_core::progress::Phase;
        let lines = thinned(&[
            tick(Phase::Unpacking, 0, Some(3)),
            tick(Phase::Unpacking, 1, Some(3)),
            tick(Phase::Copying, 0, Some(1)),
            tick(Phase::Blobs, 0, Some(2)),
        ]);
        assert!(lines.contains(&"  [1/1] restoring".to_string()), "{lines:?}");
        assert!(lines.contains(&"  [1/2] attachments".to_string()), "{lines:?}");
    }
}
