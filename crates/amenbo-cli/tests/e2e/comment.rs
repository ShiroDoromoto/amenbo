//! Comments and the attachments that hang off them: which of the two tables a comment id names,
//! editing and removing one, and the bytes an attachment brings into the store with it.

mod harness;

use serde_json::Value;

use harness::*;

/// The `attach` surface end-to-end — a file ingests as a `blob` (metadata recorded, bytes in
/// the content-addressed store), an external link attaches as `url`, both list/show, and `rm` deletes
/// them so the listing drops back to empty.
#[test]
fn attach_blob_and_url_lifecycle() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "添付PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "資料つきタスク", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "決めた理由", "--json"]);
    let did = id_str(&d["decision"]["id"]);

    // blob: ingest a file. The mime is guessed from the extension (.md → text/markdown), size is the byte length.
    let file = cli.home.join("report.md");
    std::fs::write(&file, "# title\nbody\n").unwrap();
    let add = cli.json(&["task", "attach", &tid, file.to_str().unwrap(), "--json"]);
    assert_eq!(add["action"], "attach.add");
    assert_eq!(add["attachment"]["kind"], "blob");
    assert_eq!(add["attachment"]["mime"], "text/markdown");
    assert_eq!(add["attachment"]["filename"], "report.md");
    let blob_id = id_str(&add["attachment"]["id"]);

    // url: hang an external link off a decision (--url); nothing is ingested.
    let url = cli.json(&["decision", "attach", &did, "https://example.com/spec", "--url", "--name", "spec", "--json"]);
    assert_eq!(url["attachment"]["kind"], "url");
    assert_eq!(url["attachment"]["url"], "https://example.com/spec");

    // Non-web schemes are turned away at the door — what is never stored can never reach the OS opener.
    let (_, code) = cli.run_err(&["task", "attach", &tid, "file:///etc/passwd", "--url", "--json"]);
    assert_ne!(code, 0, "a file: url attachment is not accepted");

    // ls: one blob on the task, one url on the decision. Ids are conversational numbers, so a bare `1` reads
    // as task `#1` or decision `D-1` alike: name the type (an ambiguous ref is rejected as `ambiguous_id`).
    let ls_task = cli.json(&["attach", "ls", &format!("T-{tid}"), "--json"]);
    assert_eq!(ls_task["count"], 1);
    assert_eq!(ls_task["attachments"][0]["kind"], "blob");
    let ls_dec = cli.json(&["attach", "ls", &format!("D-{did}"), "--json"]);
    assert_eq!(ls_dec["count"], 1);
    assert_eq!(ls_dec["attachments"][0]["kind"], "url");

    // show: fetch one attachment's metadata by id.
    let show = cli.json(&["attach", "show", &blob_id, "--json"]);
    assert_eq!(id_str(&show["id"]), blob_id);
    assert_eq!(show["size_bytes"], 13);

    // Without -y, rm is refused in a non-interactive context and the attachment survives.
    let (_, refused) = cli.run(&["attach", "rm", &blob_id, "--json"]);
    assert_eq!(refused, 1, "an rm that did not skip confirmation is rejected");
    assert_eq!(cli.json(&["attach", "ls", &format!("T-{tid}"), "--json"])["count"], 1, "a rejected rm deletes nothing");

    // rm removes it, a fresh ls no longer shows it, and a second rm is a no-op.
    let rm = cli.json(&["attach", "rm", &blob_id, "--yes", "--json"]);
    assert_eq!(rm["action"], "attach.rm");
    assert_eq!(rm["noop"], false);
    let ls_after = cli.json(&["attach", "ls", &format!("T-{tid}"), "--json"]);
    assert_eq!(ls_after["count"], 0);

    // A missing id is not_found (non-zero exit).
    let (_, code) = cli.run(&["attach", "show", "01NOPENOPENOPENOPENOPENOPE"]);
    assert_ne!(code, 0);
}

/// `attach save` writes a blob's bytes back out to a file — the CLI counterpart of the GUI download.
/// A file path is written verbatim; a directory saves under the attachment's own filename. It refuses
/// to clobber an existing destination without `--force`, and refuses a URL attachment (no bytes to save).
#[test]
fn attach_save_writes_a_blob_to_a_file() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "保存PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "保存タスク", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "理由", "--json"]);
    let did = id_str(&d["decision"]["id"]);

    let body = "# title\nbody\n";
    let src = cli.home.join("report.md");
    std::fs::write(&src, body).unwrap();
    let blob_id = id_str(&cli.json(&["task", "attach", &tid, src.to_str().unwrap(), "--json"])["attachment"]["id"]);

    // Save to an explicit file path — the bytes round-trip exactly.
    let dst = cli.home.join("out").join("copy.md");
    std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
    let saved = cli.json(&["attach", "save", &blob_id, "--out", dst.to_str().unwrap(), "--json"]);
    assert_eq!(saved["action"], "attach.save");
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), body);

    // Save into a directory — the file lands under the attachment's own filename.
    let dir = cli.home.join("into");
    std::fs::create_dir_all(&dir).unwrap();
    cli.json(&["attach", "save", &blob_id, "--out", dir.to_str().unwrap(), "--json"]);
    assert_eq!(std::fs::read_to_string(dir.join("report.md")).unwrap(), body);

    // An existing destination is not clobbered without --force; --force overwrites it.
    let (_, refused) = cli.run(&["attach", "save", &blob_id, "--out", dst.to_str().unwrap(), "--json"]);
    assert_ne!(refused, 0, "saving over an existing file without --force is refused");
    let forced = cli.json(&["attach", "save", &blob_id, "--out", dst.to_str().unwrap(), "--force", "--json"]);
    assert_eq!(forced["action"], "attach.save");

    // A URL attachment has no bytes to save.
    let url_id = id_str(&cli.json(&["decision", "attach", &did, "https://example.com/spec", "--url", "--json"])["attachment"]["id"]);
    let (_, code) = cli.run(&["attach", "save", &url_id, "--out", cli.home.join("nope").to_str().unwrap(), "--json"]);
    assert_ne!(code, 0, "a url attachment cannot be saved");

    // A missing id is not_found.
    let (_, code) = cli.run(&["attach", "save", "01NOPENOPENOPENOPENOPENOPE", "--json"]);
    assert_ne!(code, 0);
}

/// How many blob files actually sit in a `blobs/` directory under `<home>`.
fn blob_count(home: &std::path::Path) -> usize {
    fn walk(dir: &std::path::Path, inside_blobs: bool, n: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, inside_blobs || p.file_name() == Some("blobs".as_ref()), n);
            } else if inside_blobs {
                *n += 1;
            }
        }
    }
    let mut n = 0;
    walk(home, false, &mut n);
    n
}

/// Ordering invariant: `attach` ingests the bytes only **after** the target resolves. The other order lets
/// a failed attach leave behind a pinned blob with zero references. Blobs are reclaimed on the delete paths
/// (`attach rm`, deleting a task or a decision), and each reclaims only what it orphaned — so an orphan
/// from an attach that never happened is on no delete path, and only a `doctor --fix` sweep picks it up.
/// Until then every failure fattens `blobs/`, and `backup` ships that directory whole.
#[test]
fn failed_attach_ingests_nothing() {
    let cli = Cli::new();
    let pid = cli.a_project();
    let tid = id_str(&cli.json(&["task", "add", "--title", "資料つきタスク", "--project", &pid, "--json"])["task"]["id"]);
    let file = cli.home.join("payload.txt");
    std::fs::write(&file, "payload\n").unwrap();
    let path = file.to_str().unwrap();

    // An unresolvable target exits non-zero and leaves no blob behind — tasks, decisions and comments alike.
    for target in ["#99999", "T-99999", "01NOPENOPENOPENOPENOPENOPE"] {
        let (_, code) = cli.run(&["task", "attach", target, path]);
        assert_ne!(code, 0, "an unresolvable attach target '{target}' should fail");
        let (_, code) = cli.run(&["decision", "attach", target, path]);
        assert_ne!(code, 0, "an unresolvable decision '{target}' should fail");
        let (_, code) = cli.run(&["comment", "attach", target, path]);
        assert_ne!(code, 0, "an unresolvable comment '{target}' should fail");
        assert_eq!(blob_count(&cli.home), 0, "a failed attach left a blob (target '{target}')");
    }

    // An unreadable file ingests nothing either: metadata and the per-file limit are checked before ingest.
    let (_, code) = cli.run(&["task", "attach", &tid, "no-such-file.txt"]);
    assert_ne!(code, 0, "attaching a missing file should fail");
    assert_eq!(blob_count(&cli.home), 0, "a failed attach left a blob (missing file)");

    // A successful attach does leave one blob, which proves the zeros above are not a miscount.
    cli.json(&["task", "attach", &tid, path, "--json"]);
    assert_eq!(blob_count(&cli.home), 1, "a successful attach leaves one blob");
}

/// An empty reference points at nothing: it fails to resolve, and it is not a wildcard. In a store with a
/// single live candidate an empty prefix would otherwise match it, `pick_id` would read that as a unique
/// hit, and `amenbo task done ""` would rewrite the one row it found without asking.
#[test]
fn an_empty_ref_resolves_to_nothing() {
    let cli = Cli::new();
    let pid = cli.a_project();
    let tid = id_str(&cli.json(&["task", "add", "--title", "唯一のタスク", "--project", &pid, "--json"])["task"]["id"]);
    cli.json(&["decision", "add", "--project", &pid, "--title", "唯一の決定", "--json"]);

    // Even with exactly one live task and one live decision, an empty ref does not resolve (non-zero exit).
    for empty in ["", " "] {
        let (_, code) = cli.run(&["task", "done", empty]);
        assert_ne!(code, 0, "an empty task ref {empty:?} resolved");
        let (_, code) = cli.run(&["task", "show", empty]);
        assert_ne!(code, 0, "an empty task ref {empty:?} resolved");
        let (_, code) = cli.run(&["decision", "show", empty]);
        assert_ne!(code, 0, "an empty decision ref {empty:?} resolved");
    }

    // The task was not touched — it did not quietly become done.
    assert_eq!(cli.json(&["task", "show", &tid, "--json"])["status"], "todo");
}

/// The two comment tables number independently, so the same decimal id can stand in both. **The command
/// says which table**: `comment attach` means a task comment, `decision comment attach` a decision comment,
/// and `attach ls` picks the table with a flag. Comment ids carry no type sigil, unlike `T-n` / `D-n`.
#[test]
fn a_comment_id_in_both_tables_is_disjoined_by_the_command() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "申請書作成", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "UTC で保存する", "--json"]);
    let did = id_str(&d["decision"]["id"]);

    // The tables number independently, so drive the decision side up until it collides with the task side's
    // id — that collision is exactly why the table has to be named.
    let tc = id_str(&cli.json(&["comment", "add", &tid, "--text", "タスクのコメント", "--json"])["comment"]["id"]);
    let dc = loop {
        let id = id_str(&cli.json(&["decision", "comment", "add", &did, "--text", "決定のコメント", "--json"])["comment"]["id"]);
        assert!(id.parse::<i64>().unwrap() <= tc.parse::<i64>().unwrap(), "the decision numbering overtook the task numbering");
        if id == tc {
            break id;
        }
    };

    cli.json(&["comment", "attach", &tc, "https://example.com/task", "--url", "--json"]);
    cli.json(&["decision", "comment", "attach", &dc, "https://example.com/decision", "--url", "--json"]);

    // The same id reaches the right table, because the command and its flag choose the table.
    let on_task = cli.json(&["attach", "ls", "--task-comment", &tc, "--json"]);
    assert_eq!(on_task["count"], 1);
    assert_eq!(on_task["attachments"][0]["url"], "https://example.com/task");
    let on_decision = cli.json(&["attach", "ls", "--decision-comment", &dc, "--json"]);
    assert_eq!(on_decision["count"], 1);
    assert_eq!(on_decision["attachments"][0]["url"], "https://example.com/decision");

    // There is no way to hand `attach ls` a bare comment id: it is read as a task/decision ref and fails.
    let (_, code) = cli.run(&["attach", "ls", "--task-comment", "9999", "--json"]);
    assert_eq!(code, 1, "a nonexistent comment is not_found");
}

/// The two comment tables number apart, so their refs are spelled apart (`AMB-D-377`): a task comment is
/// `AMB-TC-<n>`, a decision comment `AMB-DC-<n>`. The same id names a row in each, which is exactly why
/// neither door may accept the other's spelling — the ref has to say which table it came from.
#[test]
fn task_and_decision_comment_refs_are_spelled_apart() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "コメント綴りPJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "作業", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "UTC で保存する", "--json"]);
    let did = id_str(&d["decision"]["id"]);

    let tc = cli.json(&["comment", "add", &tid, "--text", "タスク側", "--json"]);
    let tcid = id_str(&tc["comment"]["id"]);
    let dc = cli.json(&["decision", "comment", "add", &did, "--text", "決定側", "--json"]);
    let dcid = id_str(&dc["comment"]["id"]);

    // The listings are where a person reads a ref off the screen, so that is where the spelling has to be
    // right (`--json` carries the id, the human line carries the ref).
    let (task_list, _, _) = cli.run_both(&["comment", "list", &tid]);
    assert!(task_list.contains(&format!("AMB-TC-{tcid}")), "a task comment reads as AMB-TC: {task_list}");
    let (decision_list, _, _) = cli.run_both(&["decision", "comment", "list", &did]);
    assert!(decision_list.contains(&format!("AMB-DC-{dcid}")), "a decision comment reads as AMB-DC: {decision_list}");

    // Each door takes its own spelling…
    cli.json(&["comment", "edit", &format!("AMB-TC-{tcid}"), "--text", "タスク側（改）", "--json"]);
    cli.json(&["decision", "comment", "edit", &format!("AMB-DC-{dcid}"), "--text", "決定側（改）", "--json"]);
    // …and not the other's, whatever number it carries.
    let (_, wrong_task) = cli.run(&["comment", "edit", &format!("AMB-DC-{dcid}"), "--text", "x", "--json"]);
    assert_eq!(wrong_task, 1, "a decision comment's ref is not a task comment's");
    let (_, wrong_decision) = cli.run(&["decision", "comment", "edit", &format!("AMB-TC-{tcid}"), "--text", "x", "--json"]);
    assert_eq!(wrong_decision, 1, "a task comment's ref is not a decision comment's");
    // The retired spelling is not a third accepted form.
    let (_, retired) = cli.run(&["comment", "edit", &format!("AMB-C-{tcid}"), "--text", "x", "--json"]);
    assert_eq!(retired, 1, "AMB-C- is not accepted");
}

/// A misposted comment is taken back with `comment rm` — a hard delete, attachments and all. Decision comments mirror it.
#[test]
fn comment_rm_deletes_the_comment_and_its_attachment() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "申請書作成", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);

    let c = cli.json(&["comment", "add", &tid, "--text", "誤投稿", "--json"]);
    let cid = id_str(&c["comment"]["id"]);
    cli.json(&["comment", "add", &tid, "--text", "残すコメント", "--json"]);
    cli.json(&["comment", "attach", &cid, "https://example.com/", "--url", "--json"]);

    let removed = cli.json(&["comment", "rm", &cid, "--yes", "--json"]);
    assert_eq!(removed["comment"]["deleted"], true);

    let listed = cli.json(&["comment", "list", &tid, "--json"]);
    assert_eq!(listed["count"], 1, "only the deleted comment drops out");
    assert_eq!(listed["comments"][0]["text"], "残すコメント");
    // The attachments hanging off the comment go with it, and with their target gone `attach ls` can no
    // longer resolve that id.
    let (_, code) = cli.run(&["attach", "ls", "--task-comment", &cid, "--json"]);
    assert_eq!(code, 1, "a deleted comment cannot resolve as an attach target");
    // A second rm is not_found: the row is gone.
    let (_, again) = cli.run(&["comment", "rm", &cid, "--yes", "--json"]);
    assert_eq!(again, 1);

    // Decision comments delete the same way.
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "UTC で保存する", "--json"]);
    let did = id_str(&d["decision"]["id"]);
    let dc = cli.json(&["decision", "comment", "add", &did, "--text", "誤投稿", "--json"]);
    let dcid = id_str(&dc["comment"]["id"]);
    cli.json(&["decision", "comment", "rm", &dcid, "--yes", "--json"]);
    assert_eq!(cli.json(&["decision", "comment", "list", &did, "--json"])["count"], 0);
}

/// A post you only want to reword is rewritten in place by `comment edit`: id, position in the thread and
/// attachments all survive, unlike delete-and-repost. Decision comments mirror it, even under an accepted decision.
#[test]
fn comment_edit_rewrites_the_body_and_keeps_the_id_and_its_attachment() {
    let cli = Cli::new();
    let p = cli.json(&["project", "add", "--name", "PJ", "--json"]);
    let pid = id_str(&p["project"]["id"]);
    let t = cli.json(&["task", "add", "--title", "申請書作成", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);

    let c = cli.json(&["comment", "add", &tid, "--text", "誤字のある投稿", "--json"]);
    let cid = id_str(&c["comment"]["id"]);
    cli.json(&["comment", "add", &tid, "--text", "後の投稿", "--json"]);
    cli.json(&["comment", "attach", &cid, "https://example.com/", "--url", "--json"]);

    let edited = cli.json(&["comment", "edit", &cid, "--text", "直した投稿", "--json"]);
    assert_eq!(id_str(&edited["comment"]["id"]), cid, "the id does not change (not a new post)");

    let listed = cli.json(&["comment", "list", &tid, "--json"]);
    assert_eq!(listed["count"], 2, "the count neither grows nor shrinks");
    assert_eq!(listed["comments"][0]["text"], "直した投稿", "still first in oldest-first order = its position does not move");
    assert_eq!(listed["comments"][1]["text"], "後の投稿");
    assert_eq!(cli.json(&["attach", "ls", "--task-comment", &cid, "--json"])["count"], 1, "the attachment that was there remains");

    // An empty body is refused, and so is editing a comment that is not there — unlike delete, it is no no-op.
    let (_, empty) = cli.run(&["comment", "edit", &cid, "--text", "  ", "--json"]);
    assert_eq!(empty, 1);
    let (_, gone) = cli.run(&["comment", "edit", "9999", "--text", "x", "--json"]);
    assert_eq!(gone, 1);

    // Decision comments take the same shape, and stay editable under an accepted decision: what freezes is the decision's body.
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "UTC で保存する", "--json"]);
    let did = id_str(&d["decision"]["id"]);
    let dc = cli.json(&["decision", "comment", "add", &did, "--text", "誤字のある投稿", "--json"]);
    let dcid = id_str(&dc["comment"]["id"]);
    cli.json(&["decision", "accept", &did, "--json"]);
    cli.json(&["decision", "comment", "edit", &dcid, "--text", "直した投稿", "--json"]);
    let dlisted = cli.json(&["decision", "comment", "list", &did, "--json"]);
    assert_eq!(dlisted["count"], 1);
    assert_eq!(dlisted["comments"][0]["text"], "直した投稿");
}

/// An edited post says so. No revision history is kept, so `edited_at` is the only clue a reader gets that
/// the body is no longer the one they read — which counts for most when the writer is an AI. An untouched
/// post stays quiet: the mark appears only where there is a fact to report.
#[test]
fn an_edited_comment_says_so_and_an_untouched_one_stays_quiet() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();
    let t = cli.json(&["task", "add", "--title", "申請書作成", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);

    let c = cli.json(&["comment", "add", &tid, "--text", "誤字のある投稿", "--json"]);
    let cid = id_str(&c["comment"]["id"]);
    cli.json(&["comment", "add", &tid, "--text", "触らない投稿", "--json"]);

    // Before any edit nothing is marked: updated_at equals created_at on insert, which is not "edited".
    let before = cli.json(&["comment", "list", &tid, "--json"]);
    assert!(
        before["comments"].as_array().unwrap().iter().all(|c| c["edited_at"].is_null()),
        "a merely-posted comment shows no edited mark: {before}"
    );

    cli.json(&["comment", "edit", &cid, "--text", "直した投稿", "--json"]);
    let after = cli.json(&["comment", "list", &tid, "--json"]);
    assert!(after["comments"][0]["edited_at"].is_string(), "the edited post has an edited-at time: {after}");
    assert!(after["comments"][1]["edited_at"].is_null(), "an untouched post stays quiet: {after}");

    // The human output says the same thing, and only on the line that was edited.
    let (human, code) = cli.run(&["comment", "list", &tid]);
    assert_eq!(code, 0);
    let edited_line = human.lines().find(|l| l.contains("直した投稿")).expect("the edited row exists");
    let quiet_line = human.lines().find(|l| l.contains("触らない投稿")).expect("the untouched row exists");
    assert!(edited_line.contains("edited"), "the edited row says edited: {edited_line}");
    assert!(!quiet_line.contains("edited"), "the untouched row does not: {quiet_line}");

    // Decision comments mirror it.
    let d = cli.json(&["decision", "add", "--project", &pid, "--title", "UTC で保存する", "--json"]);
    let did = id_str(&d["decision"]["id"]);
    let dc = cli.json(&["decision", "comment", "add", &did, "--text", "誤字のある投稿", "--json"]);
    let dcid = id_str(&dc["comment"]["id"]);
    assert!(cli.json(&["decision", "comment", "list", &did, "--json"])["comments"][0]["edited_at"].is_null());
    cli.json(&["decision", "comment", "edit", &dcid, "--text", "直した投稿", "--json"]);
    let dlisted = cli.json(&["decision", "comment", "list", &did, "--json"]);
    assert!(dlisted["comments"][0]["edited_at"].is_string(), "a decision comment also says edited: {dlisted}");
}

/// The edited mark shows on `activity` too, the main way a timeline is read. It exists so a human notices
/// that an AI rewrote its own post — staying quiet on the surface both of them use most would defeat it.
#[test]
fn the_timeline_says_a_comment_was_edited_too() {
    let cli = Cli::new();
    cli.run(&["init", "--name", "tester"]);
    let pid = cli.a_project();
    let t = cli.json(&["task", "add", "--title", "申請書作成", "--project", &pid, "--json"]);
    let tid = id_str(&t["task"]["id"]);
    let c = cli.json(&["comment", "add", &tid, "--text", "誤字のある投稿", "--json"]);
    let cid = id_str(&c["comment"]["id"]);
    cli.json(&["comment", "add", &tid, "--text", "触らない投稿", "--json"]);

    let comment_rows = |v: &Value| -> Vec<Value> {
        v["items"].as_array().unwrap().iter().filter(|i| i["type"] == "comment").cloned().collect()
    };
    let before = cli.json(&["activity", "--task", &tid, "--json"]);
    assert!(
        comment_rows(&before).iter().all(|i| i["edited_at"].is_null()),
        "a merely-posted row shows no edited mark: {before}"
    );

    cli.json(&["comment", "edit", &cid, "--text", "直した投稿", "--json"]);
    let after = cli.json(&["activity", "--task", &tid, "--json"]);
    let rows = comment_rows(&after);
    let edited = rows.iter().find(|i| i["text"] == "直した投稿").expect("the edited row exists");
    let quiet = rows.iter().find(|i| i["text"] == "触らない投稿").expect("the untouched row exists");
    assert!(edited["edited_at"].is_string(), "the edited row has an edited-at time: {after}");
    assert!(quiet["edited_at"].is_null(), "the untouched row stays quiet: {after}");

    // The human timeline line says the same thing.
    let (human, code) = cli.run(&["activity", "--task", &tid]);
    assert_eq!(code, 0);
    let edited_line = human.lines().find(|l| l.contains("直した投稿")).expect("the edited row exists");
    let quiet_line = human.lines().find(|l| l.contains("触らない投稿")).expect("the untouched row exists");
    assert!(edited_line.contains("edited"), "the edited row says edited: {edited_line}");
    assert!(!quiet_line.contains("edited"), "the untouched row does not: {quiet_line}");
}
