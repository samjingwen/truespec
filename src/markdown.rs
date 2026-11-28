use std::{
    collections::HashSet,
    ops::Range,
    path::{Component, Path},
};

use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd, html};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Heading {
    pub id: String,
    pub title: String,
    pub level: u8,
    pub(crate) lineage: Vec<String>,
    pub(crate) projection_start: usize,
    pub(crate) body_start: usize,
    pub(crate) source: Range<usize>,
}

#[derive(Debug)]
pub struct Run {
    pub projection: Range<usize>,
    pub source: Range<usize>,
    pub exact: bool,
}

#[derive(Debug, Serialize)]
pub struct LinePart {
    pub run: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Serialize)]
pub struct SourceLine {
    pub number: usize,
    pub parts: Vec<LinePart>,
    #[serde(skip)]
    pub projection: Range<usize>,
    #[serde(skip)]
    pub source: Range<usize>,
}

#[derive(Debug)]
pub struct BlockSpan {
    pub projection: Range<usize>,
    pub source: Range<usize>,
}

pub struct Rendered {
    pub html: String,
    pub headings: Vec<Heading>,
    pub projection: String,
    pub runs: Vec<Run>,
    pub lines: Vec<SourceLine>,
    pub paragraphs: Vec<BlockSpan>,
    pub lists: Vec<BlockSpan>,
}

impl Rendered {
    pub fn line(&self, number: usize) -> Option<&SourceLine> {
        let index = self
            .lines
            .binary_search_by_key(&number, |line| line.number)
            .ok()?;
        self.lines.get(index)
    }
}

pub fn render(source: &str, link: impl Fn(&str) -> Option<String>) -> Rendered {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parsed: Vec<_> = Parser::new_ext(source, options)
        .into_offset_iter()
        .collect();
    let mut events = Vec::with_capacity(parsed.len());
    let mut output = Rendered {
        html: String::new(),
        headings: Vec::new(),
        projection: String::new(),
        runs: Vec::new(),
        lines: Vec::new(),
        paragraphs: Vec::new(),
        lists: Vec::new(),
    };
    let mut used = HashSet::new();
    let mut lineage: Vec<(u8, String)> = Vec::new();
    let mut links = Vec::new();
    let mut pending = "";
    let mut paragraph_first_run = 0;
    let mut list_depth = 0;
    let mut list_first_run = 0;

    for (index, (event, span)) in parsed.iter().enumerate() {
        match event {
            Event::Start(Tag::Paragraph) => {
                paragraph_first_run = output.runs.len();
                output.paragraphs.push(BlockSpan {
                    projection: 0..0,
                    source: span.clone(),
                });
                let number = output.paragraphs.len();
                events.push(Event::Html(
                    format!("<p data-paragraph=\"{number}\">").into(),
                ));
            }
            Event::End(TagEnd::Paragraph) => {
                if let Some(paragraph) = output.paragraphs.last_mut()
                    && let Some(first) = output.runs.get(paragraph_first_run)
                {
                    paragraph.projection = first.projection.start..output.projection.len();
                }
                pending = "\n\n";
                events.push(Event::Html("</p>\n".into()));
            }
            Event::Start(Tag::List(start)) => {
                if list_depth == 0 {
                    list_first_run = output.runs.len();
                    output.lists.push(BlockSpan {
                        projection: 0..0,
                        source: span.clone(),
                    });
                    let number = output.lists.len();
                    let tag = match start {
                        Some(1) => format!("<ol data-list=\"{number}\">\n"),
                        Some(start) => format!("<ol start=\"{start}\" data-list=\"{number}\">\n"),
                        None => format!("<ul data-list=\"{number}\">\n"),
                    };
                    events.push(Event::Html(tag.into()));
                } else {
                    // Nested lists are part of the outer target, not separate targets.
                    if pending.is_empty() {
                        pending = "\n";
                    }
                    events.push(event.clone());
                }
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                list_depth -= 1;
                if list_depth == 0
                    && let Some(list) = output.lists.last_mut()
                    && let Some(first) = output.runs.get(list_first_run)
                {
                    list.projection = first.projection.start..output.projection.len();
                }
                pending = "\n\n";
                events.push(event.clone());
            }
            Event::Start(Tag::Heading { level, .. }) => {
                flush(&mut output.projection, &mut pending);
                let title = heading_title(&parsed[index + 1..]);
                let level = level_number(*level);
                while lineage
                    .last()
                    .is_some_and(|(previous, _)| *previous >= level)
                {
                    lineage.pop();
                }
                lineage.push((level, title.clone()));
                let base = slug(&title);
                let mut id = base.clone();
                let mut increment = 1;
                while !used.insert(id.clone()) {
                    id = format!("{base}-{increment}");
                    increment += 1;
                }
                events.push(Event::Html(
                    format!("<h{level} id=\"{}\">", escape(&id)).into(),
                ));
                output.headings.push(Heading {
                    id,
                    title,
                    level,
                    lineage: lineage.iter().map(|(_, label)| label.clone()).collect(),
                    projection_start: output.projection.len(),
                    body_start: output.projection.len(),
                    source: span.start..source.len(),
                });
            }
            Event::End(TagEnd::Heading(level)) => {
                if let Some(heading) = output.headings.last_mut() {
                    heading.body_start = output.projection.len();
                }
                events.push(Event::Html(
                    format!("</h{}>\n", level_number(*level)).into(),
                ));
                pending = "\n\n";
            }
            Event::Text(text) | Event::Code(text) => {
                let code = matches!(event, Event::Code(_));
                let text = text
                    .replace("\r\n", "\n")
                    .replace('\r', "\n")
                    .replace('\0', "\u{fffd}");
                if !text.is_empty() {
                    flush(&mut output.projection, &mut pending);
                    let run = output.runs.len();
                    let start = output.projection.len();
                    output.projection.push_str(&text);
                    output.runs.push(Run {
                        projection: start..output.projection.len(),
                        source: span.clone(),
                        exact: source.get(span.clone()) == Some(text.as_str()),
                    });
                    let text = escape(&text);
                    let span = format!("<span data-run=\"{run}\">{text}</span>");
                    events.push(Event::InlineHtml(
                        if code {
                            format!("<code>{span}</code>")
                        } else {
                            span
                        }
                        .into(),
                    ));
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                output.projection.push('\n');
                events.push(event.clone());
            }
            Event::Start(Tag::Link {
                dest_url, title, ..
            }) => {
                let href = link(dest_url);
                links.push(href.is_some());
                let html = if let Some(href) = href {
                    format!(
                        "<a href=\"{}\" title=\"{}\" rel=\"noreferrer noopener\">",
                        escape(&href),
                        escape(title)
                    )
                } else {
                    "<span class=\"unavailable-link\">".into()
                };
                events.push(Event::InlineHtml(html.into()));
            }
            Event::End(TagEnd::Link) => {
                events.push(Event::InlineHtml(
                    if links.pop() == Some(true) {
                        "</a>"
                    } else {
                        "</span>"
                    }
                    .into(),
                ));
            }
            // Retain image alt text without fetching resources from reviewed Markdown.
            Event::Start(Tag::Image { .. }) => {
                events.push(Event::InlineHtml("<span class=\"image-alt\">".into()))
            }
            Event::End(TagEnd::Image) => events.push(Event::InlineHtml("</span>".into())),
            Event::Html(_) | Event::InlineHtml(_) => {}
            Event::End(TagEnd::CodeBlock | TagEnd::BlockQuote(_) | TagEnd::Table) | Event::Rule => {
                pending = "\n\n";
                events.push(event.clone());
            }
            Event::End(TagEnd::Item | TagEnd::TableRow | TagEnd::TableHead) => {
                pending = "\n";
                events.push(event.clone());
            }
            Event::End(TagEnd::TableCell) => {
                pending = "\t";
                events.push(event.clone());
            }
            _ => events.push(event.clone()),
        }
    }
    let mut section_end = [source.len(); 7];
    for heading in output.headings.iter_mut().rev() {
        heading.source.end = section_end[usize::from(heading.level)];
        section_end[usize::from(heading.level)..].fill(heading.source.start);
    }
    html::push_html(&mut output.html, events.into_iter());
    // GFM's renderer uses inline alignment styles; the local viewer forbids
    // all inline styles. The only raw attributes here are renderer-owned.
    for align in ["left", "center", "right"] {
        output.html = output.html.replace(
            &format!(" style=\"text-align: {align}\">"),
            &format!(" class=\"align-{align}\">"),
        );
    }
    output.lines = source_lines(source, &output);
    output
}

fn source_lines(source: &str, rendered: &Rendered) -> Vec<SourceLine> {
    let starts = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(index, _)| index + 1))
        .collect::<Vec<_>>();
    let mut lines = starts
        .iter()
        .enumerate()
        .map(|(index, &start)| {
            let end = starts.get(index + 1).copied().unwrap_or(source.len());
            let end = start + source[start..end].trim_end_matches(['\r', '\n']).len();
            SourceLine {
                number: index + 1,
                parts: Vec::new(),
                projection: 0..0,
                source: start..end,
            }
        })
        .collect::<Vec<_>>();
    let mut ambiguous = vec![false; lines.len()];
    let at = |byte| {
        starts
            .partition_point(|&start| start <= byte)
            .saturating_sub(1)
    };
    for (run_id, run) in rendered.runs.iter().enumerate() {
        let first = at(run.source.start);
        let last = at(run.source.end.saturating_sub(1));
        let text = &rendered.projection[run.projection.clone()];
        if first == last {
            add_line_part(&mut lines[first], run_id, text, run.projection.start, 0);
        } else if let Some(raw) = source.get(run.source.clone())
            && (run.exact || raw.replace("\r\n", "\n") == text)
            && text.bytes().filter(|&b| b == b'\n').count()
                == raw.bytes().filter(|&b| b == b'\n').count()
        {
            let mut byte = run.projection.start;
            let mut utf16 = 0;
            for (index, fragment) in text.split_inclusive('\n').enumerate() {
                add_line_part(&mut lines[first + index], run_id, fragment, byte, utf16);
                byte += fragment.len();
                utf16 += fragment.encode_utf16().count();
            }
        } else {
            // A transformed token can span several authored lines, for example
            // a multiline inline-code span. Do not guess which line owns it.
            ambiguous[first..=last].fill(true);
        }
    }
    lines
        .into_iter()
        .zip(ambiguous)
        .filter_map(|(line, ambiguous)| (!ambiguous && !line.parts.is_empty()).then_some(line))
        .collect()
}

fn add_line_part(line: &mut SourceLine, run: usize, text: &str, byte: usize, utf16: usize) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    let leading = text.len() - text.trim_start().len();
    let end_byte = byte + leading + trimmed.len();
    let start = utf16 + text[..leading].encode_utf16().count();
    if line.parts.is_empty() {
        line.projection.start = byte + leading;
    }
    line.projection.end = end_byte;
    line.parts.push(LinePart {
        run,
        start,
        end: start + trimmed.encode_utf16().count(),
    });
}

fn flush(out: &mut String, pending: &mut &str) {
    if !out.is_empty() {
        out.push_str(pending);
    }
    *pending = "";
}

fn heading_title(events: &[(Event<'_>, Range<usize>)]) -> String {
    let mut title = String::new();
    for (event, _) in events {
        match event {
            Event::End(TagEnd::Heading(_)) => break,
            Event::Text(text) | Event::Code(text) => title.push_str(text),
            Event::SoftBreak | Event::HardBreak => title.push(' '),
            _ => {}
        }
    }
    title.trim().to_owned()
}

fn level_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() || matches!(c, '-' | '_') {
            out.push(c);
        } else if c.is_whitespace() {
            out.push('-');
        }
    }
    if out.is_empty() {
        "section".into()
    } else {
        out
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Resolve only known in-workspace documents; HTML cannot supply its own links.
pub fn safe_link(
    current: &str,
    href: &str,
    id: impl FnOnce(&str) -> Option<String>,
) -> Option<String> {
    if href.chars().any(char::is_control) || href.contains('\\') {
        return None;
    }
    let lower = href.to_ascii_lowercase();
    if ["https://", "http://", "mailto:"]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return Some(href.to_owned());
    }
    let (path, fragment) = href.split_once('#').unwrap_or((href, ""));
    let fragment = percent_decode_str(fragment).decode_utf8().ok()?;
    let fragment = if fragment.is_empty() {
        String::new()
    } else {
        format!("#{}", utf8_percent_encode(&fragment, NON_ALPHANUMERIC))
    };
    if path.is_empty() {
        return (!fragment.is_empty()).then_some(fragment);
    }
    let path = percent_decode_str(path).decode_utf8().ok()?;
    if path.contains(['\\', ':', '?']) || path.chars().any(char::is_control) {
        return None;
    }
    let mut parts = current.split('/').collect::<Vec<_>>();
    parts.pop();
    for component in Path::new(path.as_ref()).components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            _ => return None,
        }
    }
    let id = id(&parts.join("/"))?;
    Some(format!(
        "?doc={}{fragment}",
        utf8_percent_encode(&id, NON_ALPHANUMERIC)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_safe_gfm_and_keep_unique_heading_ids() {
        let r = render(
            "# Hello\n\n## A `thing`\n\n## A thing\n\n| a | b |\n|-|-|\n| c | d |\n\n```rust\nlet angle = '<';\n```\n\n<script>alert(1)</script>\n\n[bad](javascript:alert%281%29) ![alt](https://example.org/track.png)",
            |link| safe_link("docs/current.md", link, |_| None),
        );
        assert!(r.html.contains("<table>"));
        assert!(r.html.contains("<pre><code class=\"language-rust\">"));
        assert!(r.html.contains("&lt;"));
        for blocked in ["<script>", "javascript:", "<img", "track.png"] {
            assert!(!r.html.contains(blocked), "{blocked} in {}", r.html);
        }
        assert_eq!(
            r.headings.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
            ["hello", "a-thing", "a-thing-1"]
        );
        assert_eq!(r.headings[1].lineage, ["Hello", "A thing"]);
    }

    #[test]
    fn resolve_discovered_document_link_and_heading() {
        let target = |p: &str| (p == "docs/next page.md").then(|| "42".to_owned());
        assert_eq!(
            safe_link("docs/sub/current.md", "../next%20page.md#next-step", target).as_deref(),
            Some("?doc=42#next%2Dstep")
        );
        assert!(safe_link("docs/current.md", "../../secret.md", |_| Some("1".into())).is_none());
        assert!(safe_link("docs/current.md", "/root.md", |_| Some("1".into())).is_none());
    }
}
