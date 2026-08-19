//! `attach`: the files hung on tasks, decisions and comments — listing them, reading one back,
//! saving it, and handing a temp copy to another application.

use amenbo_core::config::Paths;
use amenbo_core::model::{Attachment, AttachmentTarget};
use amenbo_core::{ops, Store};

use crate::cli::*;
use crate::cmd::comment::{resolve_live_decision_comment, resolve_live_task_comment};
use crate::output::{confirm, human, print_json, write_envelope, CliError, Flags};

/// A label carrying the source file's suffix, unless it already ends in that same one.
///
/// `filename` is the one field an attachment has for what a file is called, and it is read as a name
/// again past the ingest: `attach save` writes under it, and `attach open` copies to a temp file
/// carrying its extension so the OS can pick an application. A label like `1 before the install` leaves
/// both with nothing to go on, so the file's own suffix goes back on the end of it.
///
/// The test is "does it already end in this suffix", and deliberately not "does it have an extension":
/// a label is a caption and may hold a dot anywhere — `v1.2 baseline` would answer the second question
/// yes, with `2 baseline` for an extension, and lose the `.csv` that made the name useful.
fn keep_the_suffix(label: &str, on_disk: Option<&str>) -> String {
    let Some(ext) = on_disk.map(std::path::Path::new).and_then(|p| p.extension()).and_then(|e| e.to_str())
    else {
        return label.to_string();
    };
    // Compared folded, and over whole strings: a label is free-form text, so slicing it by the suffix's
    // byte length could land inside a character.
    let suffix = format!(".{}", ext.to_lowercase());
    if label.to_lowercase().ends_with(&suffix) { label.to_string() } else { format!("{label}.{ext}") }
}

/// The attachment's display label (`att:<id>`).
fn attach_label(a: &Attachment) -> String {
    format!("att:{}", a.id)
}

/// The shared body of `task attach` / `decision attach`: ingest `source` as a blob (the default), or with
/// `--url` register it as an external link. A blob is checked against the per-file size limit before it is
/// ingested. The only creator recorded is the effective facet (`created_by_kind`). Invariant — ingest comes
/// last: the caller must resolve `target_id` before reaching here. Ingest ahead of resolving the target
/// would let a failed attach strand a pinned blob that nothing references. Blob reclamation happens on the
/// delete paths (`attach rm`, deleting a task or a decision), each collecting the orphans it made, so bytes
/// from an attach that never came to be pass through no delete at all: only `doctor --fix`'s full scan will
/// ever pick them up, and until then `blobs/` grows with every failure — and so does every archive, since
/// `backup` packs `blobs/` whole. For the same reason, reading the file's metadata and checking the size
/// limit both go before `ingest_path` (`failed_attach_ingests_nothing`).
pub(crate) fn attach_add(
    store: &mut Store,
    flags: &Flags,
    target_type: AttachmentTarget,
    target_id: i64,
    source: &str,
    url: bool,
    name: Option<String>,
) -> Result<i32, CliError> {
    let a = if url {
        store.attach_url(target_type, target_id, source, name.as_deref(), flags.facet()?)
            .map_err(CliError::from)?
    } else {
        let path = std::path::Path::new(source);
        let meta = std::fs::metadata(path).map_err(|e| CliError {
            code: "not_found",
            message: format!("cannot read file '{source}': {e}"),
            hint: Some("pass a readable file path, or use --url to attach an external link".to_string()),
            exit: 1,
        })?;
        if !meta.is_file() {
            return Err(CliError { code: "invalid_value", message: format!("'{source}' is not a regular file"), hint: None, exit: 2 });
        }
        // What the file *is* comes from the file, and `--name` only renames it: read the type off the
        // source's own name, never off the label, which carries no extension when it is a sentence.
        let on_disk = path.file_name().and_then(|n| n.to_str());
        let mime = on_disk.and_then(amenbo_core::blob::mime_from_filename);
        let filename = match name {
            Some(label) => keep_the_suffix(&label, on_disk),
            None => on_disk.unwrap_or("attachment").to_string(),
        };
        // Check the per-file limit (which varies by type) before ingesting — it is what stops a runaway.
        store.config.attachment_limits.check_per_file(mime, meta.len()).map_err(CliError::from)?;
        let blob = store.blobs().ingest_path(path).map_err(CliError::from)?;
        store.attach_blob(target_type, target_id, &blob.hash, &filename, mime, blob.size_bytes as i64, flags.facet()?)
            .map_err(CliError::from)?
    };
    let what = if url { "link" } else { "file" };
    write_envelope(flags, "attach.add", "attachment", serde_json::to_value(&a).unwrap(), None, false, format!("✓ Attached {what}: {}", attach_label(&a)));
    Ok(0)
}

/// The `attach` group (ls/show/open/rm). Adding lives on `task attach` / `decision attach`.
pub(crate) fn attach(store: &mut Store, flags: &Flags, sub: AttachCmd) -> Result<i32, CliError> {
    match sub {
        AttachCmd::Ls { target, task_comment, decision_comment } => {
            let (target_type, target_id) = resolve_attach_ls_target(store, target.as_deref(), task_comment.as_deref(), decision_comment.as_deref())?;
            let list = store.attachments_for_target(target_type, target_id)?;
            if flags.json {
                print_json(&serde_json::json!({ "count": list.len(), "attachments": list }));
            } else {
                human(flags, format!("{} attachment(s)", list.len()));
                for a in &list {
                    human(flags, format!("  {}", attach_line(a)));
                }
            }
        }
        AttachCmd::Show { id } => {
            let a = resolve_attachment(store, &id)?;
            if flags.json {
                print_json(&a);
            } else {
                human(flags, attach_line(&a));
            }
        }
        AttachCmd::Open { id } => {
            use amenbo_core::model::AttachmentKind;
            let a = resolve_attachment(store, &id)?;
            let target = match (a.kind, a.url.as_deref(), a.blob_hash.as_deref()) {
                // The way in (`ops::attachment::add_url`) allows only web schemes, but rows written before
                // that check existed can still hold anything. os_open interprets whatever it is handed, so
                // check again right before opening: no `file:` reaching a local file, no leading `-` being
                // taken for a command option.
                (AttachmentKind::Url, Some(u), _) if amenbo_core::ops::attachment::is_web_url(u) => u.to_string(),
                (AttachmentKind::Url, Some(u), _) => {
                    return Err(CliError {
                        code: "invalid_value",
                        message: format!("refusing to open '{u}' (only http, https and mailto)"),
                        hint: None,
                        exit: 2,
                    })
                }
                (AttachmentKind::Blob, _, Some(h)) => materialize_blob_temp(store, h, a.filename.as_deref())?,
                _ => {
                    return Err(CliError { code: "invalid_value", message: "attachment has neither a url nor a local blob".to_string(), hint: None, exit: 2 })
                }
            };
            os_open(&target)?;
            write_envelope(flags, "attach.open", "attachment", serde_json::to_value(&a).unwrap(), None, false, format!("✓ Opened {}", attach_label(&a)));
        }
        AttachCmd::Save { id, out, force } => {
            use amenbo_core::model::AttachmentKind;
            let a = resolve_attachment(store, &id)?;
            // Only a blob has bytes to save. A URL attachment records a link, not a file — open it.
            let hash = match (a.kind, a.blob_hash.as_deref()) {
                (AttachmentKind::Blob, Some(h)) => h,
                (AttachmentKind::Url, _) => {
                    return Err(CliError {
                        code: "invalid_value",
                        message: "this attachment is an external link, not a stored file — open it with `attach open`".to_string(),
                        hint: None,
                        exit: 2,
                    })
                }
                _ => {
                    return Err(CliError { code: "invalid_value", message: "attachment has no local blob to save".to_string(), hint: None, exit: 2 })
                }
            };
            if !store.blobs().has(hash) {
                return Err(CliError { code: "not_found", message: format!("blob {hash} is not stored locally"), hint: None, exit: 1 });
            }
            let filename = a.filename.clone().unwrap_or_else(|| "attachment".to_string());
            // `--out` is a file path, unless it names an existing directory — then save under the
            // attachment's own filename inside it. With no `--out`, that filename in the CWD.
            let dest = match out.as_deref() {
                None => std::path::PathBuf::from(&filename),
                Some(p) => {
                    let p = std::path::Path::new(p);
                    if p.is_dir() { p.join(&filename) } else { p.to_path_buf() }
                }
            };
            if dest.exists() && !force {
                return Err(CliError {
                    code: "file_exists",
                    message: format!("{} already exists", dest.display()),
                    hint: Some("pass --force to overwrite".to_string()),
                    exit: 1,
                });
            }
            let bytes = store.blobs().read(hash).map_err(CliError::from)?;
            std::fs::write(&dest, &bytes).map_err(|e: std::io::Error| CliError {
                code: "io_error",
                message: format!("cannot write {}: {e}", dest.display()),
                hint: None,
                exit: 1,
            })?;
            write_envelope(flags, "attach.save", "attachment", serde_json::to_value(&a).unwrap(), None, false, format!("✓ Saved {} → {}", attach_label(&a), dest.display()));
        }
        AttachCmd::Rm { id } => {
            let a = resolve_attachment(store, &id)?;
            if !confirm(flags, "remove attachment")? {
                return Ok(0);
            }
            let changed = store.remove_attachment(a.id).map_err(CliError::from)?;
            write_envelope(flags, "attach.rm", "attachment", serde_json::to_value(&a).unwrap(), None, !changed, format!("✓ Removed {}", attach_label(&a)));
        }
    }
    Ok(0)
}

/// Resolve what `attach ls` lists. A task or a decision is named by its reference in its number space
/// (`AMB-T-n` / `AMB-D-n`); a comment is named by a flag that says which table it is in (`--task-comment` /
/// `--decision-comment`). The two comment tables are numbered independently, so a bare `5` could equally be
/// task comment 5 or decision comment 5. Rather than stamp a kind onto the id, the command says which table
/// it means — the same shape as `comment attach` / `decision comment attach` splitting the tables by
/// namespace, except that here a flag makes the choice.
fn resolve_attach_ls_target(
    store: &Store,
    target: Option<&str>,
    task_comment: Option<&str>,
    decision_comment: Option<&str>,
) -> Result<(AttachmentTarget, i64), CliError> {
    if let Some(id) = task_comment {
        return Ok((AttachmentTarget::TaskComment, resolve_live_task_comment(store, id)?));
    }
    if let Some(id) = decision_comment {
        return Ok((AttachmentTarget::DecisionComment, resolve_live_decision_comment(store, id)?));
    }
    let Some(target) = target else {
        return Err(CliError {
            code: "invalid_args",
            message: "no target given".to_string(),
            hint: Some("pass a task/decision ref (#n / T-n / D-n), or --task-comment <id> / --decision-comment <id>".to_string()),
            exit: 2,
        });
    };
    match store.resolve_any_ref(target)? {
        ops::Ref::Task(id) => Ok((AttachmentTarget::Task, id)),
        ops::Ref::Decision(id) => Ok((AttachmentTarget::Decision, id)),
    }
}

/// Look an attachment up by id, matched exactly. Only live attachments exist to be found — a removed one has
/// no row. The id is the key of a single table, so it matches either nothing or one thing, and can never be
/// ambiguous.
fn resolve_attachment(store: &Store, id: &str) -> Result<Attachment, CliError> {
    let not_found = || CliError {
        code: "not_found",
        message: format!("attachment '{id}' not found"),
        hint: Some(format!("list ids with `{} attach ls <target>`", Paths::command_name())),
        exit: 1,
    };
    let hit = store.resolve_attachment(id)?.first().copied().ok_or_else(not_found)?;
    store.attachment(hit)?.ok_or_else(not_found)
}

/// One attachment, summarized as a line for a human.
fn attach_line(a: &Attachment) -> String {
    use amenbo_core::model::AttachmentKind;
    let label = a.filename.clone().or_else(|| a.url.clone()).unwrap_or_else(|| "(no name)".to_string());
    match a.kind {
        AttachmentKind::Blob => {
            let size = a.size_bytes.unwrap_or(0);
            let mime = a.mime.as_deref().unwrap_or("application/octet-stream");
            format!("{}  blob  {label}  {mime}  {size}B", attach_label(a))
        }
        AttachmentKind::Url => {
            let u = a.url.as_deref().unwrap_or("");
            format!("{}  url   {label}  {u}", attach_label(a))
        }
    }
}

/// The one directory `attach open` puts its temp copies in. Everything it leaves behind lives here, which
/// is what makes the copies sweepable as a set (and identifiable, by anyone wondering what wrote them).
/// On unix it is created 0700: the system temp dir is shared, and an attachment's bytes are the user's.
fn open_temp_dir() -> Result<std::path::PathBuf, std::io::Error> {
    let dir = std::env::temp_dir().join("amenbo-open");
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(&dir)?;
    Ok(dir)
}

/// How long a temp copy is kept before a later `attach open` reclaims it. `attach open` cannot delete the
/// file it just wrote — it hands the path to another application, which is still reading it — so nothing
/// can clean up after a given open except a *later* one. The window is generous because the cost of being
/// wrong runs one way: sweeping a file that is still open breaks something the user is looking at, while
/// keeping one too long costs a few bytes of temp until the next open.
const OPEN_TEMP_TTL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// Delete temp copies left by earlier opens that nothing can still plausibly be reading
/// ([`OPEN_TEMP_TTL`]). Best-effort by nature: this is reclaiming garbage, and failing to reclaim it is
/// not worth failing the open the user actually asked for.
fn sweep_open_temp(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .and_then(|t| t.elapsed().map_err(std::io::Error::other))
            .map(|age| age > OPEN_TEMP_TTL)
            .unwrap_or(false);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Materialize a stored blob into a temp file and return its path — what `attach open` needs first. A blob
/// is stored under its content address and therefore has no extension, which leaves the OS unable to pick a
/// default application; copying it to a temp file that carries the attachment's extension is done for that
/// reason alone.
///
/// The copy is named after the blob, so opening the same attachment twice rewrites one file instead of
/// growing a pile, and it lands in [`open_temp_dir`] — where the next open sweeps whatever is old enough to
/// have been abandoned ([`sweep_open_temp`]). Taking an attachment *out* is `export --out <dir>`; this
/// path only exists to let the OS choose an application, so what it writes is scratch, not a copy anyone
/// is meant to keep.
fn materialize_blob_temp(store: &Store, hash: &str, filename: Option<&str>) -> Result<String, CliError> {
    if !store.blobs().has(hash) {
        return Err(CliError {
            code: "not_found",
            message: format!("blob {hash} is not stored locally"),
            hint: None,
            exit: 1,
        });
    }
    let bytes = store.blobs().read(hash).map_err(CliError::from)?;
    let io_err = |e: std::io::Error| CliError {
        code: "io_error",
        message: format!("cannot write temp file: {e}"),
        hint: None,
        exit: 1,
    };
    let dir = open_temp_dir().map_err(io_err)?;
    sweep_open_temp(&dir);

    let short = &hash[..hash.len().min(16)];
    let name = match filename.and_then(|f| std::path::Path::new(f).extension()).and_then(|e| e.to_str()) {
        Some(ext) => format!("amenbo-{short}.{ext}"),
        None => format!("amenbo-{short}"),
    };
    let tmp = dir.join(name);
    std::fs::write(&tmp, &bytes).map_err(io_err)?;
    Ok(tmp.to_string_lossy().into_owned())
}

/// Open a path or URL in the OS's default application (macOS `open`, Windows `cmd /C start`, otherwise
/// `xdg-open`).
pub(crate) fn os_open(target: &str) -> Result<(), CliError> {
    let mkerr = |e: std::io::Error| CliError { code: "io_error", message: format!("could not open '{target}': {e}"), hint: None, exit: 1 };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = amenbo_core::sys::command("open");
        c.arg(target);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = amenbo_core::sys::command("cmd");
        c.args(["/C", "start", "", target]);
        c
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut cmd = {
        let mut c = amenbo_core::sys::command("xdg-open");
        c.arg(target);
        c
    };
    cmd.status().map_err(mkerr)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `attach open` hands its temp copy to another application and returns, so it can never delete what
    /// it wrote — only a later open can. The sweep is that later open: it reclaims what has aged past
    /// [`OPEN_TEMP_TTL`] and leaves anything recent, since a fresh copy may be the very file an
    /// application still has in front of the user.
    #[test]
    fn a_later_open_reclaims_the_temp_copies_the_earlier_ones_could_not() {
        let dir = amenbo_scratch::scratch("sweep-test");

        let stale = dir.join("amenbo-oldhash.pdf");
        let fresh = dir.join("amenbo-newhash.png");
        std::fs::write(&stale, b"opened days ago").unwrap();
        std::fs::write(&fresh, b"still on screen").unwrap();
        // Age the one file past the window; `elapsed()` reads mtime, so this is the whole of "old".
        let aged = std::time::SystemTime::now() - (OPEN_TEMP_TTL + std::time::Duration::from_secs(60));
        std::fs::File::options().write(true).open(&stale).unwrap().set_modified(aged).unwrap();

        sweep_open_temp(&dir);

        assert!(!stale.exists(), "a copy nothing can still be reading is reclaimed");
        assert!(fresh.exists(), "a fresh copy is left alone — an application may still be reading it");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
