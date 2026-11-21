use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::markdown::{BlockSpan, Heading, Rendered, Run};

const CONTEXT_CHARS: usize = 32;
const MAX_QUOTE_BYTES: usize = 32_768;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Point {
    pub run: usize,
    pub offset: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    General,
    Line { line: usize },
    Paragraph { paragraph: usize },
    List { list: usize },
    Section { heading: Option<String> },
    Selection { start: Point, end: Point },
}

impl Target {
    pub fn line_number(&self) -> Option<usize> {
        match self {
            Self::Line { line } => Some(*line),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Anchor {
    pub path: String,
    pub revision: String,
    pub heading: Vec<String>,
    pub quote: Option<String>,
    pub before: String,
    pub after: String,
    pub source_range: Option<Range<usize>>,
    original: Target,
    unique: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Attached,
    Relocated,
    NeedsReview,
    Missing,
}

#[derive(Debug, Serialize)]
pub struct Location {
    pub status: Status,
    pub target: Option<Target>,
    pub source_range: Option<Range<usize>>,
}

impl Location {
    pub fn missing() -> Self {
        Self {
            status: Status::Missing,
            target: None,
            source_range: None,
        }
    }
}

impl Anchor {
    pub fn general() -> Self {
        Self {
            path: String::new(),
            revision: String::new(),
            heading: Vec::new(),
            quote: None,
            before: String::new(),
            after: String::new(),
            source_range: None,
            original: Target::General,
            unique: true,
        }
    }

    pub fn is_general(&self) -> bool {
        matches!(self.original, Target::General)
    }

    pub fn capture(path: String, revision: String, doc: &Rendered, input: Target) -> Option<Self> {
        let (heading, quote, before, after, source_range) = match &input {
            Target::General => return None,
            Target::Section { heading } => {
                let found = match heading {
                    Some(id) => Some(doc.headings.iter().find(|h| &h.id == id)?),
                    None => None,
                };
                let lineage = found.map(|h| h.lineage.clone()).unwrap_or_default();
                (
                    lineage,
                    None,
                    String::new(),
                    section_context(doc, found).to_owned(),
                    found.map(|h| h.source.clone()),
                )
            }
            target => {
                let (span, source_range) = match target {
                    Target::Line { line } => {
                        let line = doc.line(*line)?;
                        (line.projection.clone(), Some(line.source.clone()))
                    }
                    Target::Paragraph { paragraph } => {
                        let paragraph = doc.paragraphs.get(paragraph.checked_sub(1)?)?;
                        (paragraph.projection.clone(), Some(paragraph.source.clone()))
                    }
                    Target::List { list } => {
                        let list = doc.lists.get(list.checked_sub(1)?)?;
                        (list.projection.clone(), Some(list.source.clone()))
                    }
                    Target::Selection { start, end } => (
                        point_byte(doc, start)?..point_byte(doc, end)?,
                        source_span(doc, start, end),
                    ),
                    Target::Section { .. } => unreachable!(),
                    Target::General => unreachable!(),
                };
                let quote = doc.projection.get(span.clone())?;
                if quote.trim().is_empty()
                    || (quote.len() > MAX_QUOTE_BYTES
                        && !matches!(target, Target::Paragraph { .. } | Target::List { .. }))
                {
                    return None;
                }
                let heading = heading_at(doc, span.start)
                    .map(|h| h.lineage.clone())
                    .unwrap_or_default();
                let (before, after) = quote_context(doc, &span);
                (
                    heading,
                    Some(quote.to_owned()),
                    before.to_owned(),
                    after.to_owned(),
                    source_range,
                )
            }
        };
        let mut anchor = Self {
            path,
            revision,
            heading,
            quote,
            before,
            after,
            source_range,
            original: input,
            unique: false,
        };
        anchor.unique = anchor.find(doc).is_some();
        Some(anchor)
    }

    pub fn locate(&self, revision: &str, doc: &Rendered) -> Location {
        if self.is_general() {
            return Location {
                status: Status::Attached,
                target: Some(Target::General),
                source_range: None,
            };
        }
        if self.revision == revision {
            return Location {
                status: Status::Attached,
                target: Some(self.original.clone()),
                source_range: self.source_range.clone(),
            };
        }
        if self.unique
            && let Some((target, source_range)) = self.find(doc)
        {
            return Location {
                status: Status::Relocated,
                target: Some(target),
                source_range,
            };
        }
        Location {
            status: Status::NeedsReview,
            target: None,
            source_range: None,
        }
    }

    fn find(&self, doc: &Rendered) -> Option<(Target, Option<Range<usize>>)> {
        if matches!(self.original, Target::Line { .. }) {
            let mut matches = doc
                .lines
                .iter()
                .filter(|line| self.matches_projection(doc, &line.projection));
            let line = matches.next()?;
            return matches.next().is_none().then(|| {
                (
                    Target::Line { line: line.number },
                    Some(line.source.clone()),
                )
            });
        }
        if matches!(self.original, Target::Paragraph { .. }) {
            let (paragraph, block) = self.find_block(doc, &doc.paragraphs)?;
            return Some((Target::Paragraph { paragraph }, Some(block.source.clone())));
        }
        if matches!(self.original, Target::List { .. }) {
            let (list, block) = self.find_block(doc, &doc.lists)?;
            return Some((Target::List { list }, Some(block.source.clone())));
        }
        match &self.quote {
            Some(quote) => {
                // Count overlapping matches too; otherwise a repeated substring can
                // look unique merely because a different occurrence was visited first.
                let advance = quote.chars().next()?.len_utf8();
                let mut index = 0;
                let mut matched = None;
                while let Some(offset) = doc.projection.get(index..)?.find(quote) {
                    let start = index + offset;
                    let span = start..start + quote.len();
                    index = start + advance;
                    if heading_at(doc, start)
                        .map(|h| h.lineage.as_slice())
                        .unwrap_or_default()
                        != self.heading
                        || quote_context(doc, &span) != (self.before.as_str(), self.after.as_str())
                    {
                        continue;
                    }
                    let start = point_at(doc, span.start, false)?;
                    let end = point_at(doc, span.end, true)?;
                    let source = source_span(doc, &start, &end);
                    if matched
                        .replace((Target::Selection { start, end }, source))
                        .is_some()
                    {
                        return None;
                    }
                }
                matched
            }
            None if self.heading.is_empty() => (section_context(doc, None) == self.after)
                .then_some((Target::Section { heading: None }, None)),
            None => {
                let mut matches = doc.headings.iter().filter(|heading| {
                    heading.lineage == self.heading
                        && section_context(doc, Some(heading)) == self.after
                });
                let heading = matches.next()?;
                matches.next().is_none().then(|| {
                    (
                        Target::Section {
                            heading: Some(heading.id.clone()),
                        },
                        Some(heading.source.clone()),
                    )
                })
            }
        }
    }

    fn find_block<'a>(
        &self,
        doc: &Rendered,
        blocks: &'a [BlockSpan],
    ) -> Option<(usize, &'a BlockSpan)> {
        let mut matches = blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| self.matches_projection(doc, &block.projection));
        let (index, block) = matches.next()?;
        matches.next().is_none().then_some((index + 1, block))
    }

    fn matches_projection(&self, doc: &Rendered, span: &Range<usize>) -> bool {
        doc.projection
            .get(span.clone())
            .is_some_and(|quote| Some(quote) == self.quote.as_deref())
            && heading_at(doc, span.start)
                .map(|h| h.lineage.as_slice())
                .unwrap_or_default()
                == self.heading
            && quote_context(doc, span) == (self.before.as_str(), self.after.as_str())
    }
}

fn heading_at(doc: &Rendered, start: usize) -> Option<&Heading> {
    doc.headings[..doc
        .headings
        .partition_point(|h| h.projection_start <= start)]
        .last()
}

fn section_context<'a>(doc: &'a Rendered, heading: Option<&Heading>) -> &'a str {
    let start = heading.map(|h| h.body_start).unwrap_or(0);
    let end = doc
        .headings
        .iter()
        .find(|h| heading.is_none_or(|current| h.source.start > current.source.start))
        .map(|h| h.projection_start)
        .unwrap_or(doc.projection.len());
    head(
        doc.projection.get(start..end).unwrap_or_default().trim(),
        CONTEXT_CHARS,
    )
}

fn quote_context<'a>(doc: &'a Rendered, span: &Range<usize>) -> (&'a str, &'a str) {
    let before = heading_at(doc, span.start)
        .map(|h| h.projection_start)
        .unwrap_or(0);
    let after = doc
        .headings
        .get(
            doc.headings
                .partition_point(|h| h.projection_start < span.end),
        )
        .map(|h| h.projection_start)
        .unwrap_or(doc.projection.len());
    (
        tail(
            doc.projection[before..span.start].trim_start(),
            CONTEXT_CHARS,
        ),
        head(doc.projection[span.end..after].trim_end(), CONTEXT_CHARS),
    )
}

fn head(text: &str, chars: usize) -> &str {
    &text[..text
        .char_indices()
        .nth(chars)
        .map_or(text.len(), |(i, _)| i)]
}

fn tail(text: &str, chars: usize) -> &str {
    &text[text
        .char_indices()
        .rev()
        .nth(chars - 1)
        .map_or(0, |(i, _)| i)..]
}

fn point_byte(doc: &Rendered, point: &Point) -> Option<usize> {
    let run = doc.runs.get(point.run)?;
    let text = doc.projection.get(run.projection.clone())?;
    let mut offset = 0;
    for (index, ch) in text.char_indices() {
        if offset == point.offset {
            return Some(run.projection.start + index);
        }
        offset += ch.len_utf16();
    }
    (offset == point.offset).then_some(run.projection.end)
}

fn point_at(doc: &Rendered, byte: usize, end: bool) -> Option<Point> {
    let index = doc.runs.partition_point(|r| {
        if end {
            r.projection.end < byte
        } else {
            r.projection.end <= byte
        }
    });
    let run = doc.runs.get(index)?;
    if byte < run.projection.start || byte > run.projection.end {
        return None;
    }
    let offset = doc
        .projection
        .get(run.projection.start..byte)?
        .encode_utf16()
        .count();
    Some(Point { run: index, offset })
}

fn source_span(doc: &Rendered, start: &Point, end: &Point) -> Option<Range<usize>> {
    fn endpoint(run: &Run, projection: usize, end: bool) -> Option<usize> {
        if run.exact {
            Some(run.source.start + projection - run.projection.start)
        } else if !end && projection == run.projection.start {
            Some(run.source.start)
        } else if end && projection == run.projection.end {
            Some(run.source.end)
        } else {
            None
        }
    }
    let a = endpoint(doc.runs.get(start.run)?, point_byte(doc, start)?, false)?;
    let b = endpoint(doc.runs.get(end.run)?, point_byte(doc, end)?, true)?;
    (a < b).then_some(a..b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown;

    fn render(text: &str) -> Rendered {
        markdown::render(text, |_| None)
    }

    fn select(rendered: &Rendered, quote: &str) -> Target {
        let start = rendered.projection.find(quote).unwrap();
        Target::Selection {
            start: point_at(rendered, start, false).unwrap(),
            end: point_at(rendered, start + quote.len(), true).unwrap(),
        }
    }

    #[test]
    fn maps_utf16_and_gfm_inline_to_source_across_blocks() {
        let source = "# Notes\n\nA 😀 **bold** &amp; `code`.\n\nNext paragraph.\n";
        let doc = render(source);
        let quote = "😀 bold & code.\n\nNext";
        let evidence =
            Anchor::capture("notes.md".into(), "rev".into(), &doc, select(&doc, quote)).unwrap();
        assert_eq!(evidence.quote.as_deref(), Some(quote));
        assert_eq!(
            &source[evidence.source_range.unwrap()],
            "😀 **bold** &amp; `code`.\n\nNext"
        );
        let Target::Selection { mut start, end } = select(&doc, quote) else {
            unreachable!()
        };
        start.offset += 1;
        assert!(
            Anchor::capture(
                "notes.md".into(),
                "rev".into(),
                &doc,
                Target::Selection { start, end }
            )
            .is_none()
        );
    }

    #[test]
    fn shifts_unique_evidence_and_does_not_guess_at_duplicates() {
        let text = "# Intro\n\nStandalone.\n\n## Target\n\nExpected phrase follows.\n";
        let original = render(text);
        let anchor = Anchor::capture(
            "notes.md".into(),
            "rev".into(),
            &original,
            select(&original, "Expected phrase"),
        )
        .unwrap();
        let shifted = render(&format!("Preamble.\n\n{text}"));
        assert_eq!(anchor.locate("shifted", &shifted).status, Status::Relocated);
        let ambiguous = render(&format!("{text}\n## Target\n\nExpected phrase follows.\n"));
        assert_eq!(
            anchor.locate("duplicate", &ambiguous).status,
            Status::NeedsReview
        );
        let deleted = render("# Intro\n\n## Target\n\nDifferent statement.\n");
        assert_eq!(
            anchor.locate("deleted", &deleted).status,
            Status::NeedsReview
        );
        assert_eq!(anchor.quote.as_deref(), Some("Expected phrase"));
    }

    #[test]
    fn identifies_section_by_content_not_slug_occurrence() {
        let original =
            render("# Root\n\n## Repeated\n\nFirst content.\n\n## Repeated\n\nSecond content.\n");
        let anchor = Anchor::capture(
            "notes.md".into(),
            "old".into(),
            &original,
            Target::Section {
                heading: Some("repeated-1".into()),
            },
        )
        .unwrap();
        let same_body = render("# Root\n\n## Repeated\n\nSecond content.\n");
        assert_eq!(
            anchor.locate("new", &same_body).target,
            Some(Target::Section {
                heading: Some("repeated".into())
            })
        );
    }

    #[test]
    fn tracks_authored_lines_without_changing_legacy_targets() {
        let source = "# Notes\r\n\r\nFirst **bold** 😀 &amp; `code` line.\r\nSecond authored line.\r\n\r\n```text\r\none\r\ntwo 🐈\r\n```\r\n";
        let rendered = render(source);
        let capture = |line| {
            Anchor::capture(
                "notes.md".into(),
                "old".into(),
                &rendered,
                Target::Line { line },
            )
            .unwrap()
        };
        let line = capture(3);
        assert_eq!(line.quote.as_deref(), Some("First bold 😀 & code line."));
        assert_eq!(
            &source[line.source_range.clone().unwrap()],
            "First **bold** 😀 &amp; `code` line."
        );
        assert_eq!(capture(4).quote.as_deref(), Some("Second authored line."));
        assert_eq!(capture(7).quote.as_deref(), Some("one"));
        assert_eq!(capture(8).quote.as_deref(), Some("two 🐈"));
        assert!(rendered.line(2).is_none());
        assert!(rendered.line(6).is_none());
        let shifted = render(&format!("New introduction.\n\n{source}"));
        let relocated = line.locate("new", &shifted);
        assert_eq!(relocated.status, Status::Relocated);
        assert_eq!(relocated.target, Some(Target::Line { line: 5 }));
        let duplicated = render(&format!("{source}\n{source}"));
        assert_eq!(
            line.locate("duplicate", &duplicated).status,
            Status::NeedsReview
        );

        let legacy: Anchor = serde_json::from_value(serde_json::json!({
            "path": "notes.md", "revision": "old", "heading": ["Notes"],
            "quote": null, "before": "", "after": "", "source_range": null,
            "original": {"kind": "section", "heading": "notes"}, "unique": true,
        }))
        .unwrap();
        assert_eq!(
            legacy.locate("old", &rendered).target,
            Some(Target::Section {
                heading: Some("notes".into())
            })
        );
    }

    #[test]
    fn matches_whole_paragraphs_and_keeps_nested_headings_inside_a_section() {
        let source = "# Workspace\n\nOverview.\n\n## Reader\n\nA **whole** paragraph\nhas two lines.\n\n### Rendering\n\nNested details.\n\n## Handoff\n\nOutside the section.\n";
        let doc = render(source);
        let capture =
            |target| Anchor::capture("guide.md".into(), "old".into(), &doc, target).unwrap();
        let paragraph = capture(Target::Paragraph { paragraph: 2 });
        assert_eq!(
            paragraph.quote.as_deref(),
            Some("A whole paragraph\nhas two lines.")
        );
        assert_eq!(
            &source[paragraph.source_range.clone().unwrap()].trim_end(),
            &"A **whole** paragraph\nhas two lines."
        );
        let shifted = render(&format!("Another paragraph.\n\n{source}"));
        assert_eq!(
            paragraph.locate("new", &shifted).target,
            Some(Target::Paragraph { paragraph: 3 })
        );
        let duplicated = render(&format!("{source}\n{source}"));
        assert_eq!(
            paragraph.locate("duplicate", &duplicated).status,
            Status::NeedsReview
        );

        let section = capture(Target::Section {
            heading: Some("reader".into()),
        });
        let expected = source.find("## Reader").unwrap()..source.find("## Handoff").unwrap();
        assert_eq!(section.source_range, Some(expected));
        assert_eq!(
            section.locate("new", &shifted).target,
            Some(Target::Section {
                heading: Some("reader".into())
            })
        );
    }

    #[test]
    fn anchors_outer_lists_without_including_the_label_or_adjacent_blocks() {
        let source = "## Plan\n\n**Goals:**\n\n- First **goal**.\n  - Nested goal.\n- Last goal.\n\nStop here.\n\n3. Ordered one.\n4. Ordered two.\n\nAfterward.\n";
        let doc = render(source);
        assert_eq!(doc.lists.len(), 2);
        assert!(doc.html.contains("<ul data-list=\"1\">"));
        assert!(doc.html.contains("<ol start=\"3\" data-list=\"2\">"));
        let capture = |list| {
            Anchor::capture(
                "plan.md".into(),
                "original".into(),
                &doc,
                Target::List { list },
            )
            .unwrap()
        };
        let unordered = capture(1);
        assert_eq!(
            unordered.quote.as_deref(),
            Some("First goal.\nNested goal.\nLast goal.")
        );
        assert_eq!(
            source[unordered.source_range.clone().unwrap()].trim_end(),
            "- First **goal**.\n  - Nested goal.\n- Last goal."
        );
        let ordered = capture(2);
        assert_eq!(ordered.quote.as_deref(), Some("Ordered one.\nOrdered two."));
        assert_eq!(
            source[ordered.source_range.unwrap()].trim_end(),
            "3. Ordered one.\n4. Ordered two."
        );

        let shifted = render(&format!("- Earlier list.\n\n{source}"));
        let relocated = unordered.locate("changed", &shifted);
        assert_eq!(relocated.status, Status::Relocated);
        assert_eq!(relocated.target, Some(Target::List { list: 2 }));
        let duplicate = render(&format!("{source}\n{source}"));
        assert_eq!(
            unordered.locate("duplicate", &duplicate).status,
            Status::NeedsReview
        );
    }

    #[test]
    fn general_feedback_has_no_document_anchor() {
        let anchor = Anchor::general();
        assert!(anchor.is_general());
        assert!(anchor.path.is_empty());
        assert!(anchor.source_range.is_none());
        assert_eq!(
            serde_json::to_value(&anchor).unwrap()["original"]["kind"],
            "general"
        );
    }
}
