//! A task's git commit SHAs (1 task : many commits). amenbo stores each SHA as an opaque string — it
//! never reads git, checks the commit exists, or knows which forge it lives on. The rows are the anchor
//! from history back to a task: a public commit carries no store-local reference, so the chain can only
//! be drawn on the task side.
//!
//! **The SHA is validated at the door, not at the point of use** — the same stance as the url
//! attachment's `is_web_url` ([`crate::ops::attachment`]): as long as a bad value can be *stored* at all,
//! every consumer has to defend against it forever. So `add` accepts only a full-length lower-case hex
//! SHA (40 = SHA-1, 64 = SHA-256) and normalises case before it lands. A short SHA is refused because
//! `abc1234` and the full 40 hex are the same commit in two spellings: admit both and the same commit can
//! land twice, and the `(task_id, sha)` UNIQUE index — which is what makes a re-record idempotent — stops
//! meaning anything (byte-equality is all it sees). The AI always has the full SHA, so nothing is lost.
//!
//! Writes ride the same seam as `task_dependency` ([`crate::ops::emit_create`]). An anchor belongs to its
//! task and goes when the task goes — deleted by [`crate::ops::task::delete_subtree`], which reads the
//! anchors' ids first, rather than by a constraint that would take them where no code could see it.

use crate::error::{Error, ErrorCode, Msg, Result};
use crate::model::{ActorKind, TaskCommit};
use crate::ops::emit_create;
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// Normalise a SHA for storage and lookup: trim surrounding whitespace and lower-case it. Case is
/// folded so two spellings of one commit cannot both land (the UNIQUE index sees bytes only). The
/// `commit:` filter ([`crate::query`]) normalises through this same function, so a SHA is looked up by
/// the very bytes the door stored it as.
pub(crate) fn normalize(sha: &str) -> String {
    sha.trim().to_ascii_lowercase()
}

/// The normalised SHA if it is a full-length hex commit id (40 = SHA-1, 64 = SHA-256), else an error.
/// This is the whole door: short forms, branch/tag names, URLs and revision expressions (`HEAD`, `@`)
/// all fail the length-and-charset test. No git is consulted — the shape is judged by shape alone.
fn validated_sha(sha: &str) -> Result<String> {
    let s = normalize(sha);
    if matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(s)
    } else {
        Err(Error::Invalid(
            Msg::new(
                "a commit sha must be full-length lower-case hex (40 for SHA-1, 64 for SHA-256) — not a short sha, branch, tag or revision",
            )
            .coded(ErrorCode::InvalidCommitSha),
        ))
    }
}

/// Record commit `sha` on `task_id`. The SHA is validated and normalised at the door. Idempotent: a SHA
/// already recorded on the task yields `created=false` (the `(task_id, sha)` UNIQUE index backs this).
pub fn add(
    tx: &WriteTx<'_>,
    task_id: i64,
    sha: &str,
    created_by_kind: Option<ActorKind>,
) -> Result<(TaskCommit, bool)> {
    let sha = validated_sha(sha)?;
    // The task must be a live, existing row — this is what keeps a commit from dangling off nothing.
    if read::task(tx.conn(), task_id)?.is_none() {
        return Err(crate::ops::task::NOUN.not_found(task_id));
    }
    if let Some(id) = read::task_commit_id(tx.conn(), task_id, &sha)? {
        let existing = read::task_commit(tx.conn(), id)?
            .expect("the row id was just read from the same transaction");
        return Ok((existing, false));
    }
    let now = Timestamp::now();
    let row = TaskCommit {
        // `next_id` takes the real table name; the `task_commit` dataset maps to table `task_commit`.
        id: read::next_id(tx.conn(), "task_commit")?,
        task_id,
        sha,
        created_by_kind,
        created_at: now,
        updated_at: now,
    };
    emit_create(tx, record::task_commit(&row))?;
    Ok((row, true))
}

/// Forget commit `sha` on `task_id` (a hard delete). A SHA that is not recorded is a no-op (idempotent);
/// the return value is `changed`. The `sha` is normalised the same way `add` stored it, so a caller may
/// pass any case. Deleting the task is a separate path: [`crate::ops::task::delete_subtree`] sweeps the
/// anchors it finds, so nothing here is called for that.
pub fn remove(tx: &WriteTx<'_>, task_id: i64, sha: &str) -> Result<bool> {
    let sha = normalize(sha);
    let Some(id) = read::task_commit_id(tx.conn(), task_id, &sha)? else {
        return Ok(false);
    };
    tx.delete_record("task_commit", id)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{mk_task, with_tx};
    use crate::store_engine::read;

    const SHA1: &str = "0123456789abcdef0123456789abcdef01234567";
    const SHA256: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn add_is_idempotent_and_normalises_case() {
        with_tx(|tx| {
            let a = mk_task(tx, "a");
            let (row, created) = add(tx, a, SHA1, Some(ActorKind::Ai)).unwrap();
            assert!(created);
            assert_eq!(row.sha, SHA1, "stored lower-case");
            // The same commit in upper case with surrounding whitespace is the same row.
            let (again, created2) = add(tx, a, &format!("  {}  ", SHA1.to_uppercase()), None).unwrap();
            assert!(!created2, "re-recording the same commit does not create a second row");
            assert_eq!(again.id, row.id);
            assert_eq!(read::task_commits(tx.conn(), a).unwrap().len(), 1);
        });
    }

    #[test]
    fn add_accepts_sha256() {
        with_tx(|tx| {
            let a = mk_task(tx, "a");
            let (row, created) = add(tx, a, &SHA256.to_uppercase(), None).unwrap();
            assert!(created);
            assert_eq!(row.sha, SHA256);
        });
    }

    #[test]
    fn add_rejects_non_full_sha() {
        with_tx(|tx| {
            let a = mk_task(tx, "a");
            // Short sha, branch name, tag, url, revision, wrong length, non-hex — all refused at the door.
            for bad in ["abc1234", "main", "v1.0.0", "https://x/y", "HEAD", "HEAD~1", &SHA1[..39], &"g".repeat(40)] {
                assert!(add(tx, a, bad, None).is_err(), "must reject {bad:?}");
            }
            assert!(read::task_commits(tx.conn(), a).unwrap().is_empty(), "nothing landed");
        });
    }

    #[test]
    fn add_rejects_dangling_task() {
        with_tx(|tx| {
            assert!(add(tx, 9999, SHA1, None).is_err());
        });
    }

    #[test]
    fn remove_deletes_and_is_idempotent() {
        with_tx(|tx| {
            let a = mk_task(tx, "a");
            add(tx, a, SHA1, None).unwrap();
            // Any case removes it (normalised the same way it was stored).
            assert!(remove(tx, a, &SHA1.to_uppercase()).unwrap());
            assert!(!remove(tx, a, SHA1).unwrap(), "removing again is a no-op");
            // Once forgotten, the same commit can be recorded again.
            let (_row, created) = add(tx, a, SHA1, None).unwrap();
            assert!(created);
        });
    }

    #[test]
    fn same_sha_on_two_tasks_is_two_rows() {
        with_tx(|tx| {
            let a = mk_task(tx, "a");
            let b = mk_task(tx, "b");
            add(tx, a, SHA1, None).unwrap();
            let (_row, created) = add(tx, b, SHA1, None).unwrap();
            assert!(created, "the UNIQUE is over (task_id, sha) — a commit can touch many tasks");
        });
    }

    /// Delete a task and its commit anchors go with it (the `task_dependency` guarantee, mirrored).
    #[test]
    fn deleting_a_task_takes_its_commits() {
        with_tx(|tx| {
            let a = mk_task(tx, "a");
            add(tx, a, SHA1, None).unwrap();
            add(tx, a, SHA256, None).unwrap();
            crate::ops::task::delete(tx, a).unwrap();
            assert!(read::task_commits(tx.conn(), a).unwrap().is_empty(), "the anchors went with the task");
        });
    }
}
