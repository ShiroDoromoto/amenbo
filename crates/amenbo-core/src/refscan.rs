//! Read the refs a body **says** — the ones a reader is meant to follow.
//!
//! Body text is Markdown ([`crate::agent`]'s `conventions.markdown`), and Markdown is what decides whether
//! `AMB-T-<n>` is a pointer or a specimen. Written as prose it is a pointer: the reader is told to go and
//! read that task. Written in a code span it is the *form* of a ref being shown — this repository's own
//! convention for talking about the spelling without pointing at anything — and a fenced block or an
//! existing link are the same story. So this module reads prose and nothing else.
//!
//! **Two readers, one answer.** The GUI turns the refs in a body into links (`remarkRefs`, over remark's
//! mdast) and skips exactly those three places. Whoever reports a ref as dead has to agree with it, or the
//! two faces disagree about what the body even said: a ref the GUI never linked would be called broken, and
//! the reader would go looking for a pointer that was never there. Both parsers are CommonMark, which is
//! what makes agreement something the grammar gives rather than something two hand-rolled scanners keep
//! promising each other.
//!
//! **The pattern is not restated here.** [`crate::lint::refs_in_line`] finds the refs on a line, driven by
//! [`RefKind::ALL`] — this module hands it prose and reads back what it found. The scanning is [`crate::lint`]'s,
//! the grammar is the parser's, and this module is only the seam.
//!
//! It reports; it never rewrites. Whether the number resolves is the caller's question
//! ([`crate::validate::doctor`]); this module answers only what the body points at.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::idref::{self, RefKind};

/// A ref a body points at: the space it names, the number in it, and how it was written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProseRef {
    /// Which number space — [`RefKind::Task`] or [`RefKind::Decision`].
    pub kind: RefKind,
    /// The number, as the conversational id.
    pub id: i64,
    /// The ref exactly as the body spells it, case included, so a report can quote it back.
    pub raw: String,
}

/// The refs `md`'s **prose** points at, in the order they appear, duplicates and all.
///
/// Only [`RefKind::Task`] and [`RefKind::Decision`] come back. They are the two spaces a body is written to
/// point at — the numbers a person types and an agent follows — and a ref this filters out is not a ref
/// this module missed: the other kinds ([`RefKind::Comment`], a dimension, an attachment) are ids amenbo
/// renders into its own output, not references a reader chases through a body.
pub fn refs_in_prose(md: &str) -> Vec<ProseRef> {
    let mut out = Vec::new();
    // `Event::Text` carries prose, but it is not *only* prose: an inline code span is its own event
    // (`Event::Code`, never read here), yet a code **block**'s content arrives as plain `Text` between
    // `Start(CodeBlock)` and `End(CodeBlock)` — and inside a link, `Text` is the link's own label. Both are
    // spans rather than events, so both are tracked; what is left over is prose.
    let mut skipping = 0usize;
    // GFM, matching what the GUI renders (`remark-gfm`): a table cell is prose, and a ref in one points
    // just as much as a ref in a paragraph.
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    for event in Parser::new_ext(md, options) {
        match event {
            Event::Start(Tag::Link { .. } | Tag::CodeBlock(_)) => skipping += 1,
            Event::End(TagEnd::Link | TagEnd::CodeBlock) => skipping = skipping.saturating_sub(1),
            Event::Text(text) if skipping == 0 => {
                out.extend(crate::lint::refs_in_line(&text).into_iter().filter_map(classify));
            }
            _ => {}
        }
    }
    out
}

/// Read a raw ref token as one of the two spaces a body points at, or `None` for any other kind.
///
/// The number is taken by stripping the kind's own prefix rather than by cutting at the last `-`, so the
/// wrong space cannot be read as the right one — that scoping is [`idref::strip`]'s whole point, and it is
/// why a `AMB-DIM-3` can never come back here as decision 3.
fn classify(raw: &str) -> Option<ProseRef> {
    for kind in [RefKind::Task, RefKind::Decision] {
        let rest = idref::strip(kind, raw);
        if rest.len() == raw.len() {
            continue; // not this kind's spelling — `strip` hands back what it will not read
        }
        // `lint` matched digits, and it bounds them, so the tail it hands over parses — unless the number
        // overruns `i64`, which is a ref into nothing and can simply not be one.
        if let Ok(id) = rest.parse::<i64>() {
            return Some(ProseRef { kind, id, raw: raw.to_owned() });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(md: &str) -> Vec<String> {
        refs_in_prose(md).into_iter().map(|r| r.raw).collect()
    }

    #[test]
    fn prose_is_read() {
        let refs = refs_in_prose("read AMB-D-79 before AMB-T-12");
        assert_eq!(
            refs,
            vec![
                ProseRef { kind: RefKind::Decision, id: 79, raw: "AMB-D-79".to_owned() },
                ProseRef { kind: RefKind::Task, id: 12, raw: "AMB-T-12".to_owned() },
            ],
        );
    }

    /// The distinction the whole module exists for: a ref inside a code span is the spelling being shown,
    /// not a pointer being followed. This repository writes refs that way on purpose, so reading one as a
    /// pointer would report its own convention as breakage.
    #[test]
    fn a_specimen_is_not_a_pointer() {
        assert_eq!(found("the form is `AMB-T-12`"), Vec::<String>::new());
        assert_eq!(found("``a AMB-T-12 in a longer run``"), Vec::<String>::new());
        assert_eq!(found("```\nAMB-T-12\n```"), Vec::<String>::new());
        assert_eq!(found("~~~text\nAMB-T-12\n~~~"), Vec::<String>::new());
        assert_eq!(found("    AMB-T-12\n"), Vec::<String>::new(), "an indented code block");
    }

    /// A ref already written as a link is one the body has resolved for itself — both its text and its
    /// destination are the link's business, not a second reader's.
    #[test]
    fn an_existing_link_is_left_alone() {
        assert_eq!(found("[AMB-T-12](https://example.invalid/12)"), Vec::<String>::new());
        assert_eq!(found("[the task](ref:AMB-T-12)"), Vec::<String>::new());
        assert_eq!(found("<https://example.invalid/AMB-T-12>"), Vec::<String>::new());
        assert_eq!(found("after [x](y) AMB-T-12"), vec!["AMB-T-12"], "the link ends where it ends");
    }

    /// Emphasis, headings, list items, block quotes and table cells are all prose — the parser's job is to
    /// take the code and the links out, not to narrow prose down to paragraphs.
    #[test]
    fn every_prose_container_is_read() {
        assert_eq!(found("# AMB-T-1"), vec!["AMB-T-1"]);
        assert_eq!(found("- AMB-T-2"), vec!["AMB-T-2"]);
        assert_eq!(found("> AMB-T-3"), vec!["AMB-T-3"]);
        assert_eq!(found("**AMB-T-4**"), vec!["AMB-T-4"]);
        assert_eq!(found("| a | b |\n|---|---|\n| AMB-T-5 | x |"), vec!["AMB-T-5"]);
    }

    /// Only the two spaces a body points at. The other kinds amenbo can spell are ids it renders into its
    /// own output, and reading one here would invent a reference nobody wrote.
    #[test]
    fn only_the_task_and_decision_spaces_come_back() {
        assert_eq!(found("AMB-P-1 AMB-C-2 AMB-DIM-3 AMB-DIMV-4 AMB-ATT-5"), Vec::<String>::new());
        assert_eq!(
            refs_in_prose("AMB-DIM-3 and AMB-D-3").into_iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![3],
            "a longer code is never read as a shorter one with junk after it",
        );
    }

    /// Reading stays as loose as the renderer is (`idref::strip`), and as bounded as the lint is: the case
    /// a ref was typed in does not decide whether it points, and a ref has to stand on its own.
    #[test]
    fn the_bounds_and_the_case_are_the_lints() {
        assert_eq!(found("amb-t-12"), vec!["amb-t-12"]);
        assert_eq!(found("xAMB-T-12 AMB-T-12abc"), Vec::<String>::new());
    }

    /// A body says the same thing however often it says it: the caller counts, so nothing is folded here.
    #[test]
    fn a_repeated_ref_is_reported_each_time() {
        assert_eq!(found("AMB-T-1 and AMB-T-1"), vec!["AMB-T-1", "AMB-T-1"]);
    }
}
