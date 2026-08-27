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
//! **The excerpt is cut here too.** [`snippet`] belongs beside the folding rather than beside the face
//! that shows it: it has to find the term in text the fold brought together, and hand back the characters
//! the person actually wrote.
//!
//! **Staying in step is one seam, not a habit.** [`FACES`] is the whole list of what is indexed, and
//! the engine's two write funnels ([`StoreEngine::set_field`](super::StoreEngine::set_field) and
//! [`delete_record`](super::StoreEngine::delete_record)) consult it — so a face is added by naming it
//! here, and no write path can quietly forget one.

use rusqlite::types::Value;
use rusqlite::Connection;
use unicode_normalization::UnicodeNormalization;

use super::schema::col;
use super::sql::{Expr, Pred, Sql};

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

/// Where a hit landed, as the answer names it. The *kind* of face only — whose it is travels beside it,
/// because a title is a task's or a decision's and nothing about the face itself says which.
///
/// The order of the variants **is** the order hits are read in (`AMB-D-449`): a word in a name is a
/// stronger answer to "where is this written" than the same word in a paragraph, and that in turn than a
/// word in a remark on it. The two after them are the faces that are not on the record at all — a label
/// is reached through the placement, an attachment through what it hangs off — so they come after the
/// record's own words, however recent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HitFace {
    /// A task's `title` or a decision's `title`.
    Title,
    /// A task's `notes` or a decision's `body`.
    Body,
    /// The text of a comment on either.
    Comment,
    /// The name a person gave an axis, or a value on it.
    Label,
    /// What an attachment is called — its filename, or the address a link points at.
    Attachment,
}

impl HitFace {
    /// The rank this face sorts at, as the SQL that produces the rows carries it — 1-based, in variant
    /// order. It is a column rather than something the reader applies afterwards, because the order is
    /// what the page is cut from: the rows have to arrive already in it.
    pub fn tier(self) -> i64 {
        self as i64 + 1
    }

    /// The face a [`HitFace::tier`] names, for reading a row back. Total in both directions, so the rank
    /// travelling through SQL never has to be paired with a second column naming the face.
    pub fn from_tier(tier: i64) -> Option<Self> {
        [Self::Title, Self::Body, Self::Comment, Self::Label, Self::Attachment]
            .into_iter()
            .find(|f| f.tier() == tier)
    }
}

/// How many characters of the record's own text a snippet carries, and how many of them sit ahead of the
/// match. A snippet points at where something is written, and the reading itself is `show` and
/// `comment list`'s (`AMB-D-449`) — so this is a glance, deliberately narrow enough that a page of hits
/// stays readable.
pub const SNIPPET_CHARS: usize = 120;
/// See [`SNIPPET_CHARS`]. Enough of a run-up to see what the match sits in, without pushing it off the
/// end of a narrow terminal.
pub const SNIPPET_LEAD: usize = 30;

/// A run of the characters a term landed on, as a half-open range of **[`Snippet::text`]'s own
/// characters** — counted in characters, the unit the snippet is cut in, so a face reading it splits the
/// text the same way (`Array.from` on the GUI side, `chars()` on the CLI's).
///
/// The range is over the excerpt and not over the field it was cut from: the excerpt is the only text the
/// face was given, so a position into anything else is one it cannot use (`AMB-D-566`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct MatchRange {
    pub start: usize,
    pub end: usize,
}

/// An excerpt, and where in it the words landed.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Snippet {
    pub text: String,
    /// Sorted, and never overlapping: a face walks them in order and alternates plain and highlighted, so
    /// two terms landing on the same characters have to arrive as one run rather than as two to reconcile.
    /// Empty when this face carries none of the words, which is routine — see [`match_at`].
    pub matches: Vec<MatchRange>,
}

/// A short excerpt of `text` around the first place one of `terms` lands in it, in **the characters the
/// person wrote** — the copy the index matched on is folded, and returning that would answer a search for
/// `ＡＩ` with a snippet nobody typed.
///
/// The text is flattened to one line first (a run of whitespace becomes one space): notes and a decision
/// body are paragraphs, and a hit list of paragraphs is not a list. A term never holds whitespace itself
/// ([`terms`] splits on it), so flattening can neither break a match nor invent one — it only ever leaves
/// one space where there were several.
///
/// Ellipses mark each end that was cut, so a snippet never reads as the whole of a field. They are the
/// snippet's own characters rather than the record's, so the ranges are shifted past a leading one and no
/// term is ever reported as landing on an ellipsis.
///
/// Two searches happen here, and they are not the same size: **where to cut** is asked of the whole field
/// ([`match_at`], which stops at the first answer), and **what to light up** only of the window that was
/// cut ([`match_ranges`], which finds every one). The second is the expensive question, and bounding it to
/// [`SNIPPET_CHARS`] is what keeps a long body from paying for it (`AMB-D-566`).
pub fn snippet(text: &str, terms: &[String]) -> Snippet {
    let chars: Vec<char> = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().collect();
    let at = match_at(&chars, terms).unwrap_or(0);
    let start = at.saturating_sub(SNIPPET_LEAD);
    let end = (start + SNIPPET_CHARS).min(chars.len());
    let window = &chars[start..end];
    let lead = usize::from(start > 0);
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(window);
    if end < chars.len() {
        out.push('…');
    }
    let matches = match_ranges(window, terms)
        .into_iter()
        .map(|r| MatchRange { start: r.start + lead, end: r.end + lead })
        .collect();
    Snippet { text: out, matches }
}

/// The earliest character of `chars` a term lands on, or `None` when none of them does — which is not a
/// fault: a record matches on all of its faces together, so a hit on one face is routinely shown for a
/// search whose other word is written somewhere else entirely.
///
/// Two passes, because the folding is not a character-for-character map. The first folds each character on
/// its own, which yields a position for every one of them and agrees with [`normalize`] everywhere the
/// folding does not **compose across a boundary** — half-width kana meeting its voicing mark being the one
/// place it does. Only when that finds nothing does the second pass run, normalising a window of the
/// original at each position: exact, and paid for only where the cheap map could not answer.
fn match_at(chars: &[char], terms: &[String]) -> Option<usize> {
    let (folded, source) = fold_map(chars);
    let cheap = terms
        .iter()
        .filter_map(|term| folded.find(term.as_str()))
        .map(|byte| source[folded[..byte].chars().count()])
        .min();
    cheap.or_else(|| terms.iter().filter_map(|term| scan_from(chars, term, 0)).map(|r| r.start).min())
}

/// Every run of `chars` a term lands on, merged into a set no two members of which overlap.
///
/// The same two passes as [`match_at`], but taken **per term**: there is no earliest answer to stop at, so
/// a term the cheap map cannot follow falls through to the scan on its own rather than dragging the others
/// with it. Each term is read for the places it is written and not for every offset it could be read at —
/// occurrences of one term do not overlap each other — but two *different* terms can land on the same
/// characters, and a face that received those runs separately would have to reconcile them to draw
/// anything, so they are merged here where the positions are already sorted.
fn match_ranges(chars: &[char], terms: &[String]) -> Vec<MatchRange> {
    let (folded, source) = fold_map(chars);
    let mut found: Vec<MatchRange> = Vec::new();
    for term in terms {
        let cheap: Vec<MatchRange> = folded
            .match_indices(term.as_str())
            .map(|(byte, hit)| {
                let from = folded[..byte].chars().count();
                let to = from + hit.chars().count();
                MatchRange { start: source[from], end: source[to - 1] + 1 }
            })
            .collect();
        if cheap.is_empty() {
            let mut at = 0;
            while let Some(range) = scan_from(chars, term, at) {
                at = range.end;
                found.push(range);
            }
        } else {
            found.extend(cheap);
        }
    }
    found.sort_by_key(|r| (r.start, r.end));
    found.into_iter().fold(Vec::new(), |mut runs: Vec<MatchRange>, r| {
        match runs.last_mut() {
            // Touching counts as overlapping: two runs that meet are one highlight, and saying so here
            // spares every face the same join.
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => runs.push(r),
        }
        runs
    })
}

/// The fold of `chars`, character by character, beside which character of the original each character of
/// the fold came from — the cheap half of both [`match_at`] and [`match_ranges`], and the reason a position
/// in the fold can be answered in the characters the person wrote.
fn fold_map(chars: &[char]) -> (String, Vec<usize>) {
    let mut folded = String::with_capacity(chars.len());
    let mut source: Vec<usize> = Vec::with_capacity(chars.len());
    for (i, c) in chars.iter().enumerate() {
        for f in normalize(c.encode_utf8(&mut [0u8; 4])).chars() {
            folded.push(f);
            source.push(i);
        }
    }
    (folded, source)
}

/// The first place at or after `from` that `term` lands on, found by normalising a window of the original
/// at each position — the exact fallback of [`match_at`] and [`match_ranges`], and their slow half.
///
/// The window is wider than the term because the folding can shorten what it reads (two characters
/// composing into one), never by more than a third of what went in — a Hangul syllable, at three jamo, is
/// the deepest composition there is. That width is what the walk tests, one normalisation per position;
/// only where it answers is the run narrowed to the fewest characters that still carry the term, so a
/// highlight covers what was matched rather than the whole window it was found in.
fn scan_from(chars: &[char], term: &str, from: usize) -> Option<MatchRange> {
    let width = term.chars().count() * 3 + 4;
    let folds = |run: &[char]| normalize(&run.iter().collect::<String>()).starts_with(term);
    (from..chars.len()).find_map(|i| {
        let end = (i + width).min(chars.len());
        folds(&chars[i..end]).then(|| {
            let taken = (1..=end - i).find(|&k| folds(&chars[i..i + k])).unwrap_or(end - i);
            MatchRange { start: i, end: i + taken }
        })
    })
}

/// How one term is matched, as a predicate over a [`DOC_TABLE`] row the caller has reached. The split
/// is [`TRIGRAM_MIN_CHARS`], counted in **characters** —
/// a trigram is three characters, not three bytes, and a term of two kanji is short by that measure
/// however many bytes it takes.
///
/// The long path leaves the substring test to FTS5: a quoted phrase over the trigram tokenizer matches
/// where the trigrams sit at consecutive positions, which is an exact substring — a term whose triples
/// all appear out of order does not match. The short path is the plain scan the store did before there
/// was an index, run against the same normalised copy.
///
/// **Both paths are the row's membership of a set the term is looked up once for** (`AMB-D-507`). A
/// scan written in place as `sd.norm LIKE …` depends on nothing around it, and SQLite is free to — and
/// does — re-run it for every row the enclosing subquery walks, going over the whole copy each time.
/// As its own uncorrelated `IN` the scan happens once for the term and the walk only tests membership;
/// the long path has that shape by construction, since `MATCH` is already a subquery of its own. What a
/// term matches is untouched: the same scan, over the same copy, of the same normalised substring.
///
/// That is the term's own cost, and it bounds nothing above it: what the callers do with this predicate
/// is theirs to keep off the candidate rows (`AMB-D-511`). How many times a statement *asks* the lookup
/// is theirs as well — [`Term`] is where that is said.
pub(crate) fn term_pred(sd: col::search_doc::Cols, term: &str) -> Pred {
    let lookup = term_lookup(term);
    Pred::raw(format!("{} IN ({})", sd.id.to_sql(), lookup.text()), lookup.params().to_vec())
}

/// The lookup itself, as a statement of its own: the ids of the copy's rows carrying `term`, by whichever
/// path its length calls for. Written once here and reached two ways — in place ([`term_pred`]) or under
/// a name the statement made at its head ([`terms_head`]) — so the two forms cannot come to match
/// different rows: they are the same statement, one of them named.
fn term_lookup(term: &str) -> Sql {
    if term.chars().count() >= TRIGRAM_MIN_CHARS {
        let mut sql = Sql::new(format!("SELECT rowid FROM {FTS_TABLE} WHERE {FTS_TABLE} MATCH "));
        sql.bind(Value::Text(fts_phrase(term)));
        sql
    } else {
        // The copy under its own name, so the scan inside the subquery is of the whole table rather
        // than of the row the caller correlated.
        const DOC: col::search_doc::Cols = col::search_doc::ALL;
        let scan = Pred::like(DOC.norm, format!("%{}%", escape_like(term)));
        let mut sql = Sql::new(format!("SELECT {} FROM {} WHERE ", DOC.id.to_sql(), DOC.table.to_sql()));
        sql.push_pred(&scan);
        sql
    }
}

/// How a statement asks for one term's lookup — the two are the same question, and differ only in how
/// many times the statement can end up running it.
///
/// A read that names a term **once** writes the lookup where it asks it. A read that names the same term
/// in many places — the word search puts every term to twelve arms, twice over (the count and the page)
/// — makes the lookup once at its head instead and asks the arms for membership of that: written into
/// each arm, the copy is walked once per arm, and no order the tables are named in changes that
/// (`AMB-D-511`). The wording is the caller's because only the caller knows how many arms it has.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Term<'a> {
    /// The lookup written where it is asked.
    Inline(&'a str),
    /// Membership of the lookup this statement already made at its head, named by the term's position in
    /// what [`terms_head`] was given — so the two sides agree by construction, not by spelling.
    Named(usize),
}

impl Term<'_> {
    /// This term as a predicate over a [`DOC_TABLE`] row the caller has reached — [`term_pred`], or the
    /// same membership asked of the head's name.
    pub(crate) fn pred(self, sd: col::search_doc::Cols) -> Pred {
        match self {
            Term::Inline(term) => term_pred(sd, term),
            Term::Named(i) => {
                Pred::plain(format!("{} IN (SELECT id FROM {})", sd.id.to_sql(), term_cte(i)))
            }
        }
    }
}

/// The name one term's hoisted lookup goes under, by its position. Namespaced so it cannot collide with a
/// table: a `WITH` name shadows a real table of the same name, silently and for the whole statement.
fn term_cte(i: usize) -> String {
    format!("search_term_{i}")
}

/// The `WITH …` clause a statement puts at its head to look each term up once ([`Sql::with_head`]),
/// against which [`Term::Named`] then asks membership. `None` when there are no terms — no words is no
/// search, and an empty clause is not valid SQL.
///
/// `MATERIALIZED` is the point of the clause, not decoration: left to itself SQLite may inline a CTE into
/// each place that names it, which is the very repetition being taken out.
pub(crate) fn terms_head(terms: &[String]) -> Option<Sql> {
    if terms.is_empty() {
        return None;
    }
    let mut sql = Sql::default();
    for (i, term) in terms.iter().enumerate() {
        sql.push(if i == 0 { "WITH " } else { ", " })
            .push(format!("{}(id) AS MATERIALIZED (", term_cte(i)))
            .push_sql(&term_lookup(term))
            .push(")");
    }
    sql.push(" ");
    Some(sql)
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
    use crate::model::AttachmentTarget;
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

    /// Neither path tests the row the caller correlated: both look the term up in a subquery that names
    /// the copy itself, so what a candidate record costs is one membership test rather than a walk
    /// (`AMB-D-507`). Written as a shape assertion because the cost is the plan's, and a correlated
    /// predicate would still return exactly the right rows.
    #[test]
    fn neither_path_is_evaluated_against_the_row_it_is_asked_of() {
        const SD: col::search_doc::Cols = col::search_doc::of("sd");
        for term in ["検索", "全文検索"] {
            let sql = term_pred(SD, term).sql().to_owned();
            assert!(sql.starts_with(&format!("{} IN (SELECT ", SD.id.to_sql())), "{term}: {sql}");
            assert!(!sql.contains(SD.norm.to_sql().as_str()), "{term} reads the row it is asked of: {sql}");
        }
    }

    /// The two wordings of one term ask the **same** lookup: what a statement hoists to its head is
    /// character for character what the inline form puts where it asks, values and all. The point of the
    /// split is how many times a statement runs the lookup, never what it finds — so this pins the one
    /// way the split could go wrong, on both paths.
    #[test]
    fn hoisting_a_term_does_not_change_what_it_looks_up() {
        const SD: col::search_doc::Cols = col::search_doc::of("sd");
        for term in ["検索", "全文検索"] {
            let inline = term_pred(SD, term);
            let head = terms_head(std::slice::from_ref(&term.to_string())).expect("one term, one head");
            let lookup = term_lookup(term);
            assert!(inline.sql().contains(lookup.text()), "{term} inline: {}", inline.sql());
            assert!(head.text().contains(lookup.text()), "{term} hoisted: {}", head.text());
            assert_eq!(inline.params(), lookup.params(), "{term}: the inline form binds the lookup's values");
            assert_eq!(head.params(), lookup.params(), "{term}: and so does the head");
            assert!(head.text().contains("MATERIALIZED"), "{term}: the head is computed once, not inlined per use");
            // And the arms reach it by the name the head made, rather than by looking anything up again.
            let named = Term::Named(0).pred(SD);
            assert_eq!(named.sql(), format!("{} IN (SELECT id FROM {})", SD.id.to_sql(), term_cte(0)));
            assert!(named.params().is_empty(), "a name binds nothing");
        }
    }

    /// A term is one quoted phrase, so the FTS5 query language cannot read a user's punctuation as its
    /// own syntax.
    #[test]
    fn a_term_reaches_fts5_as_a_literal_phrase() {
        assert_eq!(fts_phrase("a OR b"), "\"a OR b\"");
        assert_eq!(fts_phrase("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    /// The snippet is cut around the match, and says at each end that it was cut — so it never reads as
    /// the whole of a field.
    #[test]
    fn a_snippet_is_cut_around_the_match_and_marks_each_cut_end() {
        let text = format!("{}索引{}", "あ".repeat(200), "い".repeat(200));
        let s = snippet(&text, &[normalize("索引")]).text;
        assert!(s.starts_with('…') && s.ends_with('…'), "both ends were cut: {s}");
        assert!(s.contains("索引"));
        assert_eq!(s.chars().count(), SNIPPET_CHARS + 2, "the window, plus one ellipsis at each end");
        assert_eq!(
            s.chars().skip(1).take_while(|c| *c == 'あ').count(),
            SNIPPET_LEAD,
            "the run-up ahead of the match, past the leading ellipsis"
        );

        let short = snippet("索引の話", &[normalize("索引")]).text;
        assert_eq!(short, "索引の話", "a field that fits is not cut, and says so by having no ellipsis");
    }

    /// The whole point of the ranges: they are read against the snippet that came back, so slicing the
    /// snippet by one gives the characters that were matched — including past a leading ellipsis, which is
    /// the snippet's own character and not the record's.
    #[test]
    fn the_ranges_are_read_against_the_snippet_that_came_back() {
        let text = format!("{}索引{}", "あ".repeat(200), "い".repeat(200));
        let s = snippet(&text, &[normalize("索引")]);
        let chars: Vec<char> = s.text.chars().collect();
        assert_eq!(s.matches.len(), 1, "one place, matched once: {:?}", s.matches);
        let m = s.matches[0];
        assert_eq!(chars[m.start..m.end].iter().collect::<String>(), "索引");
        assert_eq!(m.start, 1 + SNIPPET_LEAD, "past the leading ellipsis, after the run-up");
    }

    /// A snippet holds as many matches as land in it, and every term's — one range per place, not one per
    /// snippet, because a reader scanning a page is looking for whichever word they typed.
    #[test]
    fn every_place_a_word_lands_in_the_window_is_returned() {
        let s = snippet("索引と走査、索引の話", &[normalize("索引"), normalize("走査")]);
        let chars: Vec<char> = s.text.chars().collect();
        let lit: Vec<String> =
            s.matches.iter().map(|m| chars[m.start..m.end].iter().collect()).collect();
        assert_eq!(lit, ["索引", "走査", "索引"], "in reading order, whichever term found them");
    }

    /// Ranges never overlap: two terms landing on the same characters arrive as one run, so a face can
    /// walk them in order without reconciling anything.
    #[test]
    fn overlapping_matches_arrive_merged() {
        let s = snippet("abcd", &[normalize("abc"), normalize("bcd")]);
        assert_eq!(s.matches, [MatchRange { start: 0, end: 4 }], "one run, not two that overlap");

        // One term is read for the places it is written, not for every offset it could be read at: the
        // second `aa` of `aaa` starts inside the first, so the run stays one place and not two.
        let same = snippet("aaa", &[normalize("aa")]);
        assert_eq!(same.matches, [MatchRange { start: 0, end: 2 }]);
    }

    /// The ranges point at the characters the person wrote, not at the fold — so a search for `ai`
    /// highlights the full-width spelling that is on the page, and a search in full-width kana the
    /// half-width one the cheap map cannot follow (the exact scan narrows that to the characters it took,
    /// voicing mark included).
    #[test]
    fn a_range_covers_the_characters_that_were_written() {
        let wide = snippet("ＡＩ が引く", &[normalize("ai")]);
        let chars: Vec<char> = wide.text.chars().collect();
        assert_eq!(chars[wide.matches[0].start..wide.matches[0].end].iter().collect::<String>(), "ＡＩ");

        let composed = snippet("ｻｰﾊﾞの設定", &[normalize("サーバ")]);
        let chars: Vec<char> = composed.text.chars().collect();
        assert_eq!(
            chars[composed.matches[0].start..composed.matches[0].end].iter().collect::<String>(),
            "ｻｰﾊﾞ",
            "narrowed to what carries the term, and no further"
        );
    }

    /// A face carrying none of the words has nothing to light up — the same routine case as the snippet
    /// showing its opening, and not a fault.
    #[test]
    fn a_face_that_carries_no_term_has_no_ranges() {
        assert!(snippet("索引の話", &[normalize("走査")]).matches.is_empty());
    }

    /// Only the window is asked what to light up: a word written past the excerpt is not in the text the
    /// face was given, so a range pointing at it would point outside the string.
    #[test]
    fn a_word_past_the_end_of_the_window_is_not_ranged() {
        let text = format!("索引{}索引", "あ".repeat(SNIPPET_CHARS));
        let s = snippet(&text, &[normalize("索引")]);
        assert_eq!(s.matches, [MatchRange { start: 0, end: 2 }], "the one that is in the excerpt");
        assert!(s.text.chars().count() >= s.matches[0].end, "the range lands inside the text");
    }

    /// The excerpt is in the characters the person wrote: the fold is the index's copy, and answering a
    /// search for `ai` with a snippet nobody typed would be answering about the copy.
    #[test]
    fn a_snippet_comes_back_in_the_characters_that_were_written() {
        assert_eq!(snippet("ＡＩ が引く", &[normalize("ai")]).text, "ＡＩ が引く");
        // The folding composes here (half-width kana with its voicing mark), which the cheap map cannot
        // follow — the exact scan behind it still points at the word.
        let text = format!("{}ｻｰﾊﾞの設定", "あ".repeat(100));
        let s = snippet(&text, &[normalize("サーバ")]).text;
        assert!(s.contains("ｻｰﾊﾞの設定"), "the scan found the composed spelling: {s}");
    }

    /// A snippet is one line: notes and a decision body are paragraphs, and a list of paragraphs is not a
    /// list.
    #[test]
    fn a_snippet_is_flattened_to_one_line() {
        assert_eq!(snippet("## 見出し\n\n索引を  引く\n", &[normalize("索引")]).text, "## 見出し 索引を 引く");
    }

    /// A face routinely carries only some of the words — every term has to land on the *record*, not on
    /// one face — so a face carrying none of them is not a fault, and shows its opening.
    #[test]
    fn a_face_that_carries_no_term_shows_its_opening() {
        let text = format!("{}索引", "あ".repeat(200));
        let s = snippet(&text, &[normalize("走査")]).text;
        assert!(!s.starts_with('…'), "cut from the front, not around a match that is not there: {s}");
        assert!(s.ends_with('…'));
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

        assert_eq!(e.delete_records_for_target(AttachmentTarget::Task, 1).unwrap(), 1);
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
