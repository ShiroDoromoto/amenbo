//! The word index: a **normalised copy** of every text face, and the two paths a term takes to it
//! (`AMB-D-450`).
//!
//! A term is matched by its length, because the index can only answer for one of the two:
//!
//! | term | path |
//! |---|---|
//! | 3 characters or more | the `tokenize='trigram'` FTS5 index |
//! | 2 characters or fewer | a substring scan of the normalised copy |
//!
//! FTS5's trigram tokenizer holds character *triples*, so a shorter term produces no token at all and
//! the index answers "no rows" for it — which is not the same as "no matches". The scan is the other
//! path, over the same copy, so which path a term took never changes what it means to match.
//!
//! **The copy is what is indexed, and the body is left alone.** Both the copy and the query term go
//! through [`normalize`], so a full-width `ＡＩ` finds `AI` and a word typed in hiragana finds the same
//! word written in katakana; the record's own text is never rewritten — display, diffs and quoted
//! comments return what was written.
//!
//! **The index holds no truth of its own.** Every row here is derived from a record's column, and
//! [`rebuild`] reconstructs the whole of it from those columns — which is what lets the version chain
//! fill it in for a store that predates it, and what makes a lost row a repairable fault rather than
//! lost data. It is not a [`Dataset`](super::schema::Dataset) for the same reason: `export` carries the
//! records, and a derived copy travelling beside them could only ever disagree with them.
//!
//! **Staying in step is one seam, not a habit.** [`FACES`] is the whole list of what is indexed, and
//! the engine's two write funnels ([`StoreEngine::set_field`](super::StoreEngine::set_field) and
//! [`delete_record`](super::StoreEngine::delete_record)) consult it — so a face is added by naming it
//! here, and no write path can quietly forget one.

use rusqlite::types::Value;
use rusqlite::Connection;
use unicode_normalization::UnicodeNormalization;

use super::schema::col;
use super::sql::{Expr, Pred};

/// The shortest term the trigram index can answer for. Below it there is no trigram to look up, so the
/// term takes the scan path instead ([`term_pred`]).
pub const TRIGRAM_MIN_CHARS: usize = 3;

/// The FTS5 index over [`DOC_TABLE`]'s normalised copy. Named here because it is a virtual table: it
/// carries no column declaration the registry could emit, so `col::` cannot hand it out.
pub(crate) const FTS_TABLE: &str = "search_fts";

/// The table holding the normalised copy — one row per indexed face. Declared in the registry
/// (`schema::PLAIN_TABLES`), so its columns are named through `col::search_doc`; only the FTS5 side
/// above is spelled as text.
pub(crate) const DOC_TABLE: &str = "search_doc";

/// The datasets whose text the index carries, named once. A doc row stamps one of these into
/// `owner_kind`, [`FACES`] declares its columns by it, and the read layer's subqueries seek by it — so
/// what a write stamps and what a read looks for cannot come apart.
pub const DATASET_TASK: &str = "task";
/// See [`DATASET_TASK`].
pub const DATASET_DECISION: &str = "decision";
/// See [`DATASET_TASK`].
pub const DATASET_TASK_COMMENT: &str = "task_comment";
/// See [`DATASET_TASK`].
pub const DATASET_DECISION_COMMENT: &str = "decision_comment";
/// See [`DATASET_TASK`]. The label half: an axis and the values on it are named by a person, and are
/// reached from a record rather than held on it.
pub const DATASET_DIMENSION: &str = "dimension";
/// See [`DATASET_DIMENSION`].
pub const DATASET_DIMENSION_VALUE: &str = "dimension_value";
/// See [`DATASET_TASK`]. An attachment names itself — by the file it came from, or by the address it
/// points at — and hangs off a record or off a comment on one.
pub const DATASET_ATTACHMENT: &str = "attachment";

/// One text face the index carries: the dataset it belongs to, and the column that holds the text.
/// The pair is the doc row's key, alongside the record's id.
pub struct Face {
    /// The dataset (`task`, `decision`, `attachment`, …) — the name
    /// [`StoreEngine::set_field`](super::StoreEngine::set_field) is called with.
    pub dataset: &'static str,
    /// The column on that dataset whose text is indexed.
    pub column: &'static str,
}

/// Every face the index carries (`AMB-D-450`'s "what a word lands on"): a task's title and notes, a
/// decision's title and body, the body of a comment on either, the names a person gave an axis and its
/// values, and what an attachment is called — its filename, or the address a link points at. What is
/// deliberately absent is everything `--filter` already narrows exactly — `status`, `priority`, `due`,
/// `assignee`, a commit SHA — which would only blur the word face if a word could reach it.
///
/// The last three datasets are not *on* the record a search is about: a label is reached through the
/// task's assignment of it, and an attachment through what it hangs off. That join is the read layer's
/// ([`super::read`]) — this list says only what text exists, never whose it is.
///
/// This list is the seam: the write funnels ask it what to reindex, [`rebuild`] reads the store back
/// through it, and the read layer asks it for nothing at all (a doc row names its own face).
pub const FACES: &[Face] = &[
    Face { dataset: DATASET_TASK, column: "title" },
    Face { dataset: DATASET_TASK, column: "notes" },
    Face { dataset: DATASET_DECISION, column: "title" },
    Face { dataset: DATASET_DECISION, column: "body" },
    Face { dataset: DATASET_TASK_COMMENT, column: "text" },
    Face { dataset: DATASET_DECISION_COMMENT, column: "text" },
    Face { dataset: DATASET_DIMENSION, column: "name" },
    Face { dataset: DATASET_DIMENSION_VALUE, column: "name" },
    Face { dataset: DATASET_ATTACHMENT, column: "filename" },
    Face { dataset: DATASET_ATTACHMENT, column: "url" },
];

/// Is this `(dataset, column)` a face the index carries — the question
/// [`StoreEngine::set_field`](super::StoreEngine::set_field) asks of every field write.
pub(crate) fn indexes_field(dataset: &str, column: &str) -> bool {
    FACES.iter().any(|f| f.dataset == dataset && f.column == column)
}

/// Does this dataset carry any indexed face — the question
/// [`StoreEngine::delete_record`](super::StoreEngine::delete_record) asks before sweeping a deleted
/// record's doc rows.
pub(crate) fn indexes_dataset(dataset: &str) -> bool {
    FACES.iter().any(|f| f.dataset == dataset)
}

/// The normalised form of `s` — what the index stores and what a query term is compared as. Three
/// foldings, each one a difference a person does not intend to have typed:
///
/// | folded | what it brings together |
/// |---|---|
/// | width (NFKC) | full-width `ＡＩ` with `AI`, and half-width kana with the ordinary form |
/// | case | `Search` with `search` |
/// | kana | a word typed in hiragana with the same word in katakana |
///
/// NFKC runs first so that a half-width kana and its voicing mark have composed into one character
/// before the kana fold sees them. Nothing is stripped and no character is dropped: a form nobody
/// confuses is left exactly as it was.
pub fn normalize(s: &str) -> String {
    s.nfkc().flat_map(char::to_lowercase).map(fold_kana).collect()
}

/// Hiragana to katakana, the one script fold. The two kana are the same syllabary written twice, and
/// which one a word was typed in is a keyboard state, not a meaning — so the two spellings are one word
/// here. The blocks are contiguous and `0x60` apart (`U+3041`..`U+3096` onto `U+30A1`..`U+30F6`, and the
/// two iteration marks `U+309D`..`U+309E` onto `U+30FD`..`U+30FE`), so the fold is arithmetic rather
/// than a table. Katakana that hiragana has no counterpart for is left where it is, as are the
/// half-width forms NFKC has already widened.
fn fold_kana(c: char) -> char {
    match c {
        '\u{3041}'..='\u{3096}' | '\u{309D}'..='\u{309E}' => {
            char::from_u32(c as u32 + 0x60).unwrap_or(c)
        }
        _ => c,
    }
}

/// The terms a query text carries: split on whitespace, each one normalised. They are combined with
/// AND by whoever matches them — a record matches when *every* term does, on any of its faces
/// (`AMB-D-450`).
///
/// A text of nothing but whitespace yields no terms, which reads as no constraint rather than as a
/// constraint nothing meets: an empty search box is not a search for the empty string.
pub fn terms(query: &str) -> Vec<String> {
    query.split_whitespace().map(normalize).filter(|t| !t.is_empty()).collect()
}

/// How one term is matched, as a predicate over a [`DOC_TABLE`] row the caller has already correlated
/// to the record it is asking about. The split is [`TRIGRAM_MIN_CHARS`], counted in **characters** —
/// a trigram is three characters, not three bytes, and a term of two kanji is short by that measure
/// however many bytes it takes.
///
/// The long path leaves the substring test to FTS5: a quoted phrase over the trigram tokenizer matches
/// where the trigrams sit at consecutive positions, which is an exact substring — a term whose triples
/// all appear out of order does not match. The short path is the plain scan the store did before there
/// was an index, run against the same normalised copy.
pub(crate) fn term_pred(sd: col::search_doc::Cols, term: &str) -> Pred {
    if term.chars().count() >= TRIGRAM_MIN_CHARS {
        Pred::raw(
            format!("{} IN (SELECT rowid FROM {FTS_TABLE} WHERE {FTS_TABLE} MATCH ?)", sd.id.to_sql()),
            vec![Value::Text(fts_phrase(term))],
        )
    } else {
        Pred::like(sd.norm, format!("%{}%", escape_like(term)))
    }
}

/// A term as an FTS5 query: one quoted phrase, so every character in it is taken literally rather than
/// read as the query language's own syntax (`*`, `OR`, `NEAR`, a bare `-`). The one character a phrase
/// cannot hold plainly is the quote, which is doubled — the same escape SQL string literals use.
fn fts_phrase(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

/// Escape LIKE metacharacters so a term matches literally (paired with `ESCAPE '\'`): a `%` someone
/// typed is a percent sign they are looking for, not a wildcard.
pub(crate) fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Write one face's normalised copy — the upsert behind every indexed field write. A face whose text
/// normalises to nothing carries no row at all: an unwritten column (`''` until its first write) and a
/// notes field someone emptied are the same absence, and a row of empty text is one the scan path would
/// walk for every short term and never match.
pub(crate) fn put_doc(
    conn: &Connection,
    dataset: &str,
    row: i64,
    column: &str,
    text: &str,
) -> rusqlite::Result<()> {
    let norm = normalize(text);
    if norm.is_empty() {
        return drop_doc(conn, dataset, row, column);
    }
    const SD: col::search_doc::Cols = col::search_doc::ALL;
    conn.execute(
        &format!(
            "INSERT INTO {DOC_TABLE}({kind}, {id}, {field}, {norm}) VALUES (?1, ?2, ?3, ?4) \
               ON CONFLICT({kind}, {id}, {field}) DO UPDATE SET {norm} = excluded.{norm}",
            kind = SD.owner_kind.name(),
            id = SD.owner_id.name(),
            field = SD.field.name(),
            norm = SD.norm.name(),
        ),
        rusqlite::params![dataset, row, column, norm],
    )?;
    Ok(())
}

/// Forget one face — the other half of [`put_doc`], for text that has gone.
fn drop_doc(conn: &Connection, dataset: &str, row: i64, column: &str) -> rusqlite::Result<()> {
    const SD: col::search_doc::Cols = col::search_doc::ALL;
    conn.execute(
        &format!(
            "DELETE FROM {DOC_TABLE} WHERE {kind} = ?1 AND {id} = ?2 AND {field} = ?3",
            kind = SD.owner_kind.name(),
            id = SD.owner_id.name(),
            field = SD.field.name(),
        ),
        rusqlite::params![dataset, row, column],
    )?;
    Ok(())
}

/// Forget every face of one record — what a deleted record owes the index. The doc rows carry no
/// foreign key to their record (the owner is polymorphic, as `attachment`'s target is), so this sweep is
/// what stands in for the constraint.
pub(crate) fn drop_record(conn: &Connection, dataset: &str, row: i64) -> rusqlite::Result<()> {
    const SD: col::search_doc::Cols = col::search_doc::ALL;
    conn.execute(
        &format!(
            "DELETE FROM {DOC_TABLE} WHERE {kind} = ?1 AND {id} = ?2",
            kind = SD.owner_kind.name(),
            id = SD.owner_id.name(),
        ),
        rusqlite::params![dataset, row],
    )?;
    Ok(())
}

/// Rebuild the whole index from the records it is derived from — the operation that makes "the index
/// holds no truth of its own" a fact rather than a claim, since every row it would have can be produced
/// again from the columns alone. The version chain runs it, to fill in a store whose records predate the
/// index.
///
/// The normalisation happens here, in Rust, as it does on the write path — SQLite has no NFKC of its
/// own, and a rebuild that folded differently from the writes would produce an index that disagrees
/// with itself depending on which rows were written when.
pub fn rebuild(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(&format!("DELETE FROM {DOC_TABLE}"), [])?;
    for face in FACES {
        // The dataset's table comes from the registry, not from the face's own name, so a table renamed
        // there is renamed here too.
        let table = super::schema::dataset(face.dataset)
            .expect("every indexed face names a registry dataset")
            .table;
        // Read as optional: a face may sit on a nullable column (an attachment carries a filename or a
        // url, never both), and a NULL is the same absence as empty text — no row.
        let mut stmt = conn.prepare(&format!("SELECT id, {} FROM {table}", face.column))?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, text) in rows {
            put_doc(conn, face.dataset, id, face.column, text.as_deref().unwrap_or(""))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_engine::StoreEngine;

    /// A store with one task, whose title and notes are what the tests search.
    fn store_with_task(title: &str, notes: &str) -> StoreEngine {
        let e = StoreEngine::open_in_memory().expect("in-memory engine");
        e.put_record(
            DATASET_TASK,
            1,
            &[("title", Value::Text(title.into())), ("notes", Value::Text(notes.into()))],
        )
        .expect("write the task");
        e
    }

    /// The ids one term reaches, straight off the copy — the two paths under test without a filter
    /// around them.
    fn hits(e: &StoreEngine, term: &str) -> Vec<i64> {
        const SD: col::search_doc::Cols = col::search_doc::of("sd");
        let term = normalize(term);
        let pred = term_pred(SD, &term);
        let sql = format!(
            "SELECT DISTINCT {id} FROM {DOC_TABLE} sd WHERE {} ORDER BY {id}",
            pred.sql(),
            id = SD.owner_id.to_sql(),
        );
        let mut stmt = e.conn().prepare(&sql).expect("prepare");
        stmt.query_map(rusqlite::params_from_iter(pred.params()), |r| r.get::<_, i64>(0))
            .expect("query")
            .collect::<rusqlite::Result<Vec<i64>>>()
            .expect("rows")
    }

    /// A term of three characters or more goes through FTS5's trigram index and comes back with the
    /// record it was written on — the index exists, the triggers filled it, and the tokenizer is there.
    #[test]
    fn a_long_term_is_found_through_the_trigram_index() {
        let e = store_with_task("全文検索の索引", "");
        assert!(term_pred(col::search_doc::of("sd"), "全文検索").sql().contains("MATCH"));
        assert_eq!(hits(&e, "全文検索"), vec![1]);
        assert_eq!(hits(&e, "の索引"), vec![1], "a word boundary is not required");
        assert!(hits(&e, "全文一致").is_empty());
    }

    /// The index matches an exact substring, not merely the same triples in some other order — which is
    /// what lets the two paths mean one thing (`AMB-D-450`).
    #[test]
    fn the_index_does_not_match_reordered_triples() {
        let e = store_with_task("ABCDEF", "");
        assert_eq!(hits(&e, "cdef"), vec![1]);
        assert!(hits(&e, "cdeabc").is_empty());
    }

    /// A term of two characters has no trigram to look up, so the scan path answers for it — and
    /// answers the same way.
    #[test]
    fn a_short_term_is_found_by_the_scan() {
        let e = store_with_task("全文検索の索引", "AI が引く");
        assert_eq!(hits(&e, "検索"), vec![1]);
        assert_eq!(hits(&e, "ai"), vec![1]);
        assert!(hits(&e, "索出").is_empty());
    }

    /// The folding reaches both sides: the copy is normalised on the way in, the term on the way out,
    /// so a width, a case or a kana difference is not a miss. Both paths, since they read one copy.
    #[test]
    fn the_folding_brings_the_spellings_together() {
        let e = store_with_task("ＡＩ の Search", "さーばの設定");
        assert_eq!(hits(&e, "ai"), vec![1], "width and case, on the scan path");
        assert_eq!(hits(&e, "SEARCH"), vec![1], "case, on the index path");
        assert_eq!(hits(&e, "サーバ"), vec![1], "kana, on the index path");
        assert_eq!(hits(&e, "ｻｰﾊﾞ"), vec![1], "half-width kana too");
    }

    /// A word that reached the copy is a word the record still holds: rewriting the column rewrites the
    /// copy, and deleting the record takes it away entirely.
    #[test]
    fn the_copy_follows_the_record() {
        let e = store_with_task("全文検索の索引", "");
        e.set_field(DATASET_TASK, 1, "title", Value::Text("番号で引く".into())).expect("rewrite");
        assert!(hits(&e, "全文検索").is_empty(), "the old title is gone from the index");
        assert_eq!(hits(&e, "番号で引く"), vec![1]);

        e.delete_record(DATASET_TASK, 1).expect("delete");
        assert!(hits(&e, "番号で引く").is_empty(), "a deleted record leaves no copy behind");
    }

    /// The index holds no truth of its own: emptied and rebuilt from the records alone, it comes back
    /// the same — which is what the version chain leans on for a store that predates it.
    #[test]
    fn the_index_rebuilds_from_the_records_alone() {
        let e = store_with_task("全文検索の索引", "二文字は走査で引く");
        let before = (hits(&e, "全文検索"), hits(&e, "走査"));

        e.conn().execute(&format!("DELETE FROM {DOC_TABLE}"), []).expect("empty the copy");
        assert!(hits(&e, "全文検索").is_empty(), "the index really was emptied");

        rebuild(e.conn()).expect("rebuild");
        assert_eq!((hits(&e, "全文検索"), hits(&e, "走査")), before);
    }

    /// Text that normalises to nothing carries no row — an unwritten column and an emptied one are the
    /// same absence.
    #[test]
    fn empty_text_leaves_no_row() {
        let e = store_with_task("題", "");
        let rows: i64 = e
            .conn()
            .query_row(&format!("SELECT COUNT(*) FROM {DOC_TABLE}"), [], |r| r.get(0))
            .expect("count");
        assert_eq!(rows, 1, "only the title is a face with text in it");
    }

    /// The three foldings of `AMB-D-450`, each stated as the pair it is meant to bring together.
    #[test]
    fn normalize_folds_width_case_and_kana() {
        assert_eq!(normalize("ＡＩ"), normalize("ai"));
        assert_eq!(normalize("Search"), normalize("search"));
        assert_eq!(normalize("さーば"), normalize("サーバ"));
        // Half-width kana composes its voicing mark before the kana fold sees it.
        assert_eq!(normalize("ｻｰﾊﾞ"), normalize("サーバ"));
    }

    /// What nobody confuses is left alone: the fold brings spellings together, it does not erase
    /// characters.
    #[test]
    fn normalize_keeps_what_it_does_not_fold() {
        assert_eq!(normalize("検索 the index"), "検索 the index");
    }

    /// Terms are the whitespace-separated words, normalised. Whitespace alone is no constraint, not a
    /// constraint nothing meets.
    #[test]
    fn terms_split_on_whitespace_and_normalize() {
        assert_eq!(terms(" 検索\u{3000}Ｉndex "), vec!["検索".to_string(), "index".to_string()]);
        assert!(terms("   ").is_empty());
    }

    /// The path a term takes is decided in characters: two kanji are short however many bytes they are.
    #[test]
    fn the_path_is_chosen_by_character_count() {
        const SD: col::search_doc::Cols = col::search_doc::of("sd");
        assert!(term_pred(SD, "検索").sql().contains("LIKE"), "two characters take the scan");
        assert!(term_pred(SD, "全文検索").sql().contains("MATCH"), "four take the index");
        assert!(term_pred(SD, "ai").sql().contains("LIKE"), "so do two ASCII characters");
    }

    /// A term is one quoted phrase, so the FTS5 query language cannot read a user's punctuation as its
    /// own syntax.
    #[test]
    fn a_term_reaches_fts5_as_a_literal_phrase() {
        assert_eq!(fts_phrase("a OR b"), "\"a OR b\"");
        assert_eq!(fts_phrase("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    /// An attachment is the one record swept by the polymorphic delete rather than by `delete_record`
    /// ([`StoreEngine::delete_records_for_target`](super::StoreEngine::delete_records_for_target)), so
    /// that path owes the index the same sweep — otherwise a word would go on pointing at a file the
    /// store no longer holds.
    #[test]
    fn the_polymorphic_sweep_takes_the_attachments_copies_with_it() {
        let e = StoreEngine::open_in_memory().expect("in-memory engine");
        let tx = e.transaction().expect("transaction");
        e.put_record("project", 1, &[("name", Value::Text("PJ".into()))]).unwrap();
        e.put_record("task", 1, &[("project_id", Value::Integer(1)), ("title", Value::Text("T".into()))])
            .unwrap();
        let attach = |id: i64, target: i64, filename: &str| {
            e.put_record(
                DATASET_ATTACHMENT,
                id,
                &[
                    ("target_type", Value::Text(DATASET_TASK.into())),
                    ("target_id", Value::Integer(target)),
                    ("kind", Value::Text("blob".into())),
                    ("filename", Value::Text(filename.into())),
                ],
            )
            .unwrap();
        };
        e.put_record("task", 2, &[("project_id", Value::Integer(1)), ("title", Value::Text("U".into()))])
            .unwrap();
        attach(1, 1, "計測ログ.md");
        attach(2, 2, "別のログ.md");
        tx.commit().unwrap();
        assert_eq!(hits(&e, "計測ログ"), vec![1]);

        assert_eq!(e.delete_records_for_target(DATASET_TASK, 1).unwrap(), 1);
        assert!(hits(&e, "計測ログ").is_empty(), "the swept attachment left no words behind");
        assert_eq!(hits(&e, "別のログ"), vec![2], "the one hanging off another task is untouched");
    }

    /// A face on a nullable column is written and cleared like any other: an attachment carries a
    /// filename or a url, never both, so the absent one must simply hold no row.
    #[test]
    fn a_face_on_a_nullable_column_holds_no_row_when_it_is_null() {
        let e = StoreEngine::open_in_memory().expect("in-memory engine");
        let tx = e.transaction().expect("transaction");
        e.put_record("project", 1, &[("name", Value::Text("PJ".into()))]).unwrap();
        e.put_record(
            DATASET_ATTACHMENT,
            1,
            &[
                ("target_type", Value::Text(DATASET_TASK.into())),
                ("target_id", Value::Integer(1)),
                ("kind", Value::Text("url".into())),
                ("url", Value::Text("https://example.com/profile-run".into())),
                ("filename", Value::Null),
            ],
        )
        .unwrap();
        tx.commit().unwrap();

        let rows: i64 = e
            .conn()
            .query_row(&format!("SELECT COUNT(*) FROM {DOC_TABLE}"), [], |r| r.get(0))
            .expect("count");
        assert_eq!(rows, 1, "the url is a face with text in it; the null filename is not");
        assert_eq!(hits(&e, "profile-run"), vec![1]);

        // Rebuilding from the columns reads the same nullable column back, and reaches the same place.
        rebuild(e.conn()).expect("rebuild");
        assert_eq!(hits(&e, "profile-run"), vec![1]);
    }
}
