//! Bounded, inert reading projection. Parsing does not access a Store, URL or file.
//! Flutter reference: lib/content/idea_markdown.dart and rich_content.dart.
use morrow_workbench_plugin::capture;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use serde::Serialize;

pub const MAX_GRAPHEMES: usize = 20_000;
const MAX_INPUT_BYTES: usize = 512 * 1024;
const MAX_DEPTH: usize = 32;
const MAX_EVENTS: usize = 8_192;
const MAX_BLOCKS: usize = 2_048;
const MAX_RUNS: usize = 4_096;
const MAX_CELLS: usize = 2_048;
const MAX_ROWS: usize = 500;
const MAX_COLUMNS: usize = 80;
const MAX_STRING_BYTES: usize = 256 * 1024;
// Stricter than the parser's own reference-expansion fuel, so exhaustion is an
// explicit error rather than a partial switch from links to literal markup.
const MAX_LINK_BYTES: usize = 64 * 1024;
const MAX_JSON_BYTES: usize = 512 * 1024;
type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Default, Serialize)]
pub struct MarkdownDoc {
    pub blocks: Vec<MarkdownBlock>,
}
#[derive(Debug, Default, Serialize)]
pub struct MarkdownBlock {
    pub kind: String,
    pub runs: Vec<MarkdownRun>,
    pub level: u8,
    /// Zero outside lists; one for the outermost list, two for its child list.
    pub indent: usize,
    pub quote: usize,
    pub marker: String,
    pub language: String,
    pub rows: Vec<MarkdownRow>,
    pub alignments: Vec<&'static str>,
}
#[derive(Debug, Default, Serialize)]
pub struct MarkdownRow {
    pub header: bool,
    pub cells: Vec<MarkdownCell>,
}
#[derive(Debug, Default, Serialize)]
pub struct MarkdownCell {
    pub runs: Vec<MarkdownRun>,
}
#[derive(Debug, Default, Serialize)]
pub struct MarkdownRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub href: String,
    pub image: bool,
}
impl MarkdownRun {
    fn same_style(&self, other: &Self) -> bool {
        !self.image
            && !other.image
            && self.bold == other.bold
            && self.italic == other.italic
            && self.strike == other.strike
            && self.code == other.code
            && self.href == other.href
    }
}

fn limit(ok: bool, name: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(format!("MarkdownLimit:{name}"))
    }
}
fn text_limit(input: &str) -> Result<()> {
    limit(input.len() <= MAX_INPUT_BYTES, "InputBytes")?;
    limit(crate::editor_field::grapheme_count(input) <= MAX_GRAPHEMES, "Grapheme")
}

/// Reuse the original protocol's URL policy, then reject control-character
/// normalization and empty userinfo that URL parsers can otherwise erase.
fn safe_href(value: &str, image: bool) -> String {
    if value.chars().any(char::is_control) {
        return String::new();
    }
    let mut decoded = Vec::with_capacity(value.len());
    let raw = value.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' {
            let digit = |v: u8| match v {
                b'0'..=b'9' => Some(v - b'0'),
                b'a'..=b'f' => Some(v - b'a' + 10),
                b'A'..=b'F' => Some(v - b'A' + 10),
                _ => None,
            };
            let Some(a) = raw.get(i + 1).and_then(|v| digit(*v)) else {
                return String::new();
            };
            let Some(b) = raw.get(i + 2).and_then(|v| digit(*v)) else {
                return String::new();
            };
            decoded.push(a * 16 + b);
            i += 3;
        } else {
            decoded.push(raw[i]);
            i += 1;
        }
    }
    let Ok(decoded) = std::str::from_utf8(&decoded) else {
        return String::new();
    };
    if decoded.chars().any(char::is_control) || decoded.contains('\\') {
        return String::new();
    }
    let trimmed = value.trim();
    if let Some((scheme, rest)) = trimmed.split_once(':') {
        // Url::parse repairs some malformed network authorities; Flutter's Uri
        // policy requires a real host. Do not expose repaired destinations.
        if (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
            && (!rest.starts_with("//")
                || rest[2..]
                    .split(['/', '?', '#'])
                    .next()
                    .unwrap_or("")
                    .is_empty())
        {
            return String::new();
        }
    }
    if let Some((_, rest)) = decoded.split_once("://") {
        if rest
            .split(['/', '?', '#'])
            .next()
            .unwrap_or("")
            .contains('@')
        {
            return String::new();
        }
    }
    let Some(safe) = capture::safe_link(value) else {
        return String::new();
    };
    if image
        && safe
            .split(':')
            .next()
            .is_some_and(|s| s.eq_ignore_ascii_case("mailto"))
    {
        return String::new();
    }
    safe
}

/// Only the description uses the original TSV conversion. Preflight all rows
/// and columns so capture::plain's legacy take() calls cannot omit content.
pub fn paste_plain(input: &str, section: &str) -> Result<String> {
    text_limit(input)?;
    match section {
        "title" | "hypothesis" | "conclusion" | "todos" => Ok(input.into()),
        "description" => {
            let lines = input.trim().lines().collect::<Vec<_>>();
            if lines.len() >= 2 && lines.iter().filter(|l| l.contains('\t')).count() >= 2 {
                limit(lines.len() <= MAX_ROWS, "PasteRows")?;
                limit(
                    lines
                        .iter()
                        .all(|line| line.split('\t').count() <= MAX_COLUMNS),
                    "PasteColumns",
                )?;
            }
            let out = capture::plain(input);
            text_limit(&out)?;
            Ok(out)
        }
        _ => Err("InvalidPasteSection".into()),
    }
}

struct Item {
    marker: String,
    used: bool,
}
struct Image {
    run: MarkdownRun,
}
#[derive(Default)]
struct Projector {
    doc: MarkdownDoc,
    active: Option<MarkdownBlock>,
    stack: Vec<TagEnd>,
    lists: Vec<Option<u64>>,
    items: Vec<Item>,
    quote: usize,
    strong: usize,
    emphasis: usize,
    strike: usize,
    links: Vec<String>,
    images: Vec<Image>,
    runs: usize,
    cells: usize,
    string_bytes: usize,
    link_bytes: usize,
    in_cell: bool,
}
impl Projector {
    fn account(&mut self, bytes: usize) -> Result<()> {
        self.string_bytes = self
            .string_bytes
            .checked_add(bytes)
            .ok_or("MarkdownLimit:Strings")?;
        limit(self.string_bytes <= MAX_STRING_BYTES, "Strings")
    }
    fn link_budget(&mut self, destination: &str, title: &str) -> Result<()> {
        self.link_bytes += destination.len() + title.len();
        limit(self.link_bytes <= MAX_LINK_BYTES, "Links")
    }
    fn flush(&mut self) -> Result<()> {
        if let Some(block) = self.active.take() {
            limit(self.doc.blocks.len() < MAX_BLOCKS, "Blocks")?;
            self.doc.blocks.push(block);
        }
        Ok(())
    }
    fn begin(&mut self, kind: &str) -> Result<()> {
        self.flush()?;
        let marker = self
            .items
            .last_mut()
            .filter(|i| !i.used)
            .map(|i| {
                i.used = true;
                i.marker.clone()
            })
            .unwrap_or_default();
        self.active = Some(MarkdownBlock {
            kind: kind.into(),
            indent: self.lists.len(),
            quote: self.quote,
            marker,
            ..Default::default()
        });
        Ok(())
    }
    fn style(&self, code: bool) -> MarkdownRun {
        MarkdownRun {
            bold: self.strong > 0,
            italic: self.emphasis > 0,
            strike: self.strike > 0,
            code,
            href: self.links.last().cloned().unwrap_or_default(),
            ..Default::default()
        }
    }
    fn push_run(&mut self, run: MarkdownRun) -> Result<()> {
        // Empty images must remain visible as placeholders; ordinary empty text
        // has no reading content, except the deliberately retained empty blocks.
        if run.text.is_empty() && !run.image {
            return Ok(());
        }
        if self.active.is_none() {
            self.begin("paragraph")?;
        }
        self.account(run.text.len() + run.href.len())?;
        let block = self.active.as_mut().ok_or("MarkdownStructure")?;
        let target = if block.kind == "table" {
            if !self.in_cell {
                return Err("MarkdownStructure:Cell".into());
            }
            &mut block
                .rows
                .last_mut()
                .ok_or("MarkdownStructure:Row")?
                .cells
                .last_mut()
                .ok_or("MarkdownStructure:Cell")?
                .runs
        } else {
            &mut block.runs
        };
        if let Some(last) = target.last_mut().filter(|last| last.same_style(&run)) {
            last.text.push_str(&run.text);
        } else {
            self.runs += 1;
            limit(self.runs <= MAX_RUNS, "Runs")?;
            target.push(run);
        }
        Ok(())
    }
    fn text(&mut self, text: &str, code: bool) -> Result<()> {
        if let Some(image) = self.images.last_mut() {
            image.run.text.push_str(text);
            return Ok(());
        }
        let code = code || self.active.as_ref().is_some_and(|b| b.kind == "code");
        let mut run = self.style(code);
        run.text = text.into();
        self.push_run(run)
    }
    fn start(&mut self, tag: Tag<'_>) -> Result<()> {
        limit(self.stack.len() < MAX_DEPTH, "Depth")?;
        self.stack.push(tag.to_end());
        match tag {
            Tag::Paragraph => self.begin("paragraph")?,
            Tag::Heading { level, .. } => {
                self.begin("heading")?;
                self.active.as_mut().unwrap().level = level as u8;
            }
            Tag::CodeBlock(kind) => {
                self.begin("code")?;
                if let CodeBlockKind::Fenced(info) = kind {
                    let language = info.split_whitespace().next().unwrap_or("").to_owned();
                    self.account(language.len())?;
                    self.active.as_mut().unwrap().language = language;
                }
            }
            Tag::HtmlBlock => self.begin("paragraph")?,
            Tag::BlockQuote(_) => {
                self.flush()?;
                self.quote += 1;
            }
            Tag::List(first) => {
                self.flush()?;
                // Retain a parent item whose only child is another list.
                if self.items.last().is_some_and(|i| !i.used) {
                    self.begin("paragraph")?;
                    self.flush()?;
                }
                self.lists.push(first);
            }
            Tag::Item => {
                self.flush()?;
                let next = self.lists.last_mut().ok_or("MarkdownStructure:List")?;
                let marker = match next {
                    Some(n) => {
                        let marker = format!("{n}.");
                        *n = n.saturating_add(1);
                        marker
                    }
                    None => "•".into(),
                };
                self.items.push(Item {
                    marker,
                    used: false,
                });
            }
            Tag::Table(alignments) => {
                limit(alignments.len() <= MAX_COLUMNS, "Columns")?;
                self.begin("table")?;
                self.active.as_mut().unwrap().alignments = alignments
                    .into_iter()
                    .map(|a| match a {
                        Alignment::None => "none",
                        Alignment::Left => "left",
                        Alignment::Center => "center",
                        Alignment::Right => "right",
                    })
                    .collect();
            }
            Tag::TableHead | Tag::TableRow => {
                let header = matches!(tag, Tag::TableHead);
                let block = self.active.as_mut().ok_or("MarkdownStructure:Table")?;
                limit(block.rows.len() < MAX_ROWS, "Rows")?;
                block.rows.push(MarkdownRow {
                    header,
                    cells: vec![],
                });
            }
            Tag::TableCell => {
                self.cells += 1;
                limit(self.cells <= MAX_CELLS, "Cells")?;
                let row = self
                    .active
                    .as_mut()
                    .ok_or("MarkdownStructure:Table")?
                    .rows
                    .last_mut()
                    .ok_or("MarkdownStructure:Row")?;
                limit(row.cells.len() < MAX_COLUMNS, "Columns")?;
                row.cells.push(MarkdownCell::default());
                self.in_cell = true;
            }
            Tag::Strong => self.strong += 1,
            Tag::Emphasis => self.emphasis += 1,
            Tag::Strikethrough => self.strike += 1,
            Tag::Link {
                dest_url, title, ..
            } => {
                self.link_budget(&dest_url, &title)?;
                self.links.push(safe_href(&dest_url, false));
            }
            Tag::Image {
                dest_url, title, ..
            } => {
                self.link_budget(&dest_url, &title)?;
                let mut run = self.style(false);
                run.image = true;
                run.href = safe_href(&dest_url, true);
                self.images.push(Image { run });
            }
            _ => return Err("UnsupportedMarkdownNode".into()),
        }
        Ok(())
    }
    fn end(&mut self, end: TagEnd) -> Result<()> {
        if self.stack.pop() != Some(end) {
            return Err("MarkdownStructure:Balance".into());
        }
        match end {
            TagEnd::Paragraph
            | TagEnd::Heading(_)
            | TagEnd::CodeBlock
            | TagEnd::HtmlBlock
            | TagEnd::Table => self.flush()?,
            TagEnd::BlockQuote(_) => {
                self.flush()?;
                self.quote -= 1;
            }
            TagEnd::List(_) => {
                self.flush()?;
                self.lists.pop().ok_or("MarkdownStructure:List")?;
            }
            TagEnd::Item => {
                if self.items.last().is_some_and(|i| !i.used) {
                    self.begin("paragraph")?;
                }
                self.flush()?;
                self.items.pop().ok_or("MarkdownStructure:Item")?;
            }
            TagEnd::TableCell => self.in_cell = false,
            TagEnd::TableHead | TagEnd::TableRow => {}
            TagEnd::Strong => self.strong -= 1,
            TagEnd::Emphasis => self.emphasis -= 1,
            TagEnd::Strikethrough => self.strike -= 1,
            TagEnd::Link => {
                self.links.pop().ok_or("MarkdownStructure:Link")?;
            }
            TagEnd::Image => {
                let image = self.images.pop().ok_or("MarkdownStructure:Image")?;
                if let Some(parent) = self.images.last_mut() {
                    parent.run.text.push_str(&image.run.text);
                } else {
                    self.push_run(image.run)?;
                }
            }
            _ => return Err("UnsupportedMarkdownNode".into()),
        }
        Ok(())
    }
    fn task(&mut self, checked: bool) -> Result<()> {
        if self.active.is_none() {
            self.begin("paragraph")?;
        }
        let marker = if checked { "☑" } else { "☐" };
        self.active.as_mut().ok_or("MarkdownStructure:Task")?.marker = marker.into();
        Ok(())
    }
}

pub fn project(input: &str) -> Result<MarkdownDoc> {
    text_limit(input)?;
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut out = Projector::default();
    for (i, event) in Parser::new_ext(input, options).enumerate() {
        limit(i < MAX_EVENTS, "Events")?;
        match event {
            Event::Start(tag) => out.start(tag)?,
            Event::End(end) => out.end(end)?,
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                out.text(&text, false)?
            }
            Event::Code(text) => out.text(&text, true)?,
            Event::SoftBreak | Event::HardBreak => out.text("\n", false)?,
            Event::Rule => {
                out.begin("rule")?;
                out.flush()?;
            }
            Event::TaskListMarker(checked) => out.task(checked)?,
            _ => return Err("UnsupportedMarkdownNode".into()),
        }
    }
    out.flush()?;
    if !out.stack.is_empty() {
        return Err("MarkdownStructure:Balance".into());
    }
    limit(
        serde_json::to_vec(&out.doc)
            .map_err(|_| "MarkdownSerialization")?
            .len()
            <= MAX_JSON_BYTES,
        "JSON",
    )?;
    Ok(out.doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text(b: &MarkdownBlock) -> String {
        b.runs.iter().map(|r| r.text.as_str()).collect()
    }
    #[test]
    fn headings_inline_styles_soft_breaks_and_code_are_preserved() {
        let doc = project("# 标题😀\n\nplain **bold *nested*** ~~gone~~ `a<b>`\nnext\n\n```rust extra\nlet x = 1;\n\nlet y = 2;\n```\n\n---").unwrap();
        assert_eq!(doc.blocks.len(), 4);
        assert_eq!(
            (
                doc.blocks[0].kind.as_str(),
                doc.blocks[0].level,
                text(&doc.blocks[0])
            ),
            ("heading", 1, "标题😀".into())
        );
        assert!(
            doc.blocks[1]
                .runs
                .iter()
                .any(|r| r.text == "nested" && r.bold && r.italic)
        );
        assert!(
            doc.blocks[1]
                .runs
                .iter()
                .any(|r| r.text == "gone" && r.strike)
        );
        assert!(
            doc.blocks[1]
                .runs
                .iter()
                .any(|r| r.text == "a<b>" && r.code)
        );
        assert!(text(&doc.blocks[1]).ends_with("\nnext"));
        assert_eq!(doc.blocks[2].language, "rust");
        assert_eq!(text(&doc.blocks[2]), "let x = 1;\n\nlet y = 2;\n");
        assert!(doc.blocks[2].runs.iter().all(|r| r.code));
        assert_eq!(doc.blocks[3].kind, "rule");
    }
    #[test]
    fn nested_loose_lists_tasks_quotes_and_continuations_keep_order() {
        let doc = project("3. outer\n\n   second paragraph\n\n   - [x] inner\n   - [ ] pending\n\n4. final\n\n> quoted\n>\n> > nested\n\nend").unwrap();
        let summary = doc
            .blocks
            .iter()
            .map(|b| (text(b), b.indent, b.quote, b.marker.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            vec![
                ("outer".into(), 1, 0, "3."),
                ("second paragraph".into(), 1, 0, ""),
                ("inner".into(), 2, 0, "☑"),
                ("pending".into(), 2, 0, "☐"),
                ("final".into(), 1, 0, "4."),
                ("quoted".into(), 0, 1, ""),
                ("nested".into(), 0, 2, ""),
                ("end".into(), 0, 0, "")
            ]
        );
    }
    #[test]
    fn table_rows_alignment_and_inline_cells_are_structured() {
        let doc = project("| L | C | R |\n| :--- | :---: | ---: |\n| **粗** | `a\\b` | [link](https://example.com) |\n\nafter").unwrap();
        let table = &doc.blocks[0];
        assert_eq!(table.kind, "table");
        assert_eq!(table.alignments, ["left", "center", "right"]);
        assert_eq!(table.rows.len(), 2);
        assert!(table.rows[0].header);
        assert!(!table.rows[1].header);
        assert!(table.rows[1].cells[0].runs[0].bold);
        assert!(table.rows[1].cells[1].runs[0].code);
        assert_eq!(table.rows[1].cells[2].runs[0].href, "https://example.com");
        assert_eq!(text(&doc.blocks[1]), "after");
    }
    #[test]
    fn reference_links_images_and_html_never_execute_or_read_files() {
        let doc = project("[safe][ref] [bad](javascript:alert) ![**alt**](attachment:image.png) ![](file:///tmp/a) ![mail](mailto:a@b)\n\n[ref]: https://example.com\n\n<script>danger()</script>\n\ninline <b>tag</b>").unwrap();
        let runs = &doc.blocks[0].runs;
        assert!(
            runs.iter()
                .any(|r| r.text == "safe" && r.href == "https://example.com")
        );
        assert!(
            runs.iter()
                .any(|r| r.text.contains("bad") && r.href.is_empty())
        );
        assert!(
            runs.iter()
                .any(|r| r.image && r.text == "alt" && r.href == "attachment:image.png")
        );
        assert!(
            runs.iter()
                .any(|r| r.image && r.text.is_empty() && r.href.is_empty())
        );
        assert!(
            runs.iter()
                .any(|r| r.image && r.text == "mail" && r.href.is_empty())
        );
        assert_eq!(text(&doc.blocks[1]), "<script>danger()</script>\n");
        assert_eq!(text(&doc.blocks[2]), "inline <b>tag</b>");
    }
    #[test]
    fn href_policy_rejects_userinfo_controls_local_and_active_urls() {
        for bad in [
            "http://@example.com",
            "https://user:pass@example.com",
            "https:\\\\@example.com",
            "https:example.com",
            "https:////@example.com",
            "https://example.com/\nnext",
            "https://example.com/%0a",
            "https://example.com/%C2%85",
            "javascript:alert(1)",
            "data:image/png,x",
            "file:///tmp/a",
            "/relative",
            "//example.com",
            "https://example.com/%zz",
        ] {
            assert!(safe_href(bad, false).is_empty(), "{bad:?}");
        }
        for safe in [
            "https://example.com/a",
            "http://example.com",
            "mailto:a@example.com",
            "attachment:原图.png",
            "https://example.com/%E4%B8%AD",
        ] {
            assert_eq!(safe_href(safe, false), safe);
        }
        assert!(safe_href("mailto:a@example.com", true).is_empty());
    }
    #[test]
    fn grapheme_depth_and_node_budgets_fail_instead_of_truncating() {
        assert_eq!(
            text(&project(&"😀".repeat(20_000)).unwrap().blocks[0]),
            "😀".repeat(20_000)
        );
        assert_eq!(
            project(&"😀".repeat(20_001)).unwrap_err(),
            "MarkdownLimit:Grapheme"
        );
        assert_eq!(
            project(&format!("{}x", "> ".repeat(33))).unwrap_err(),
            "MarkdownLimit:Depth"
        );
        let paragraphs = "x\n\n".repeat(MAX_BLOCKS + 1);
        assert_eq!(project(&paragraphs).unwrap_err(), "MarkdownLimit:Blocks");
        let events = "*x* ".repeat(2_800);
        assert_eq!(project(&events).unwrap_err(), "MarkdownLimit:Events");
        let many_cells = format!("|a|b|c|d|e|\n|-|-|-|-|-|\n{}", "|a|b|c|d|e|\n".repeat(410));
        assert_eq!(project(&many_cells).unwrap_err(), "MarkdownLimit:Cells");
    }
    #[test]
    fn repeated_reference_destinations_have_an_output_budget() {
        let input = format!(
            "{}\n\n[x]: https://example.com/{}",
            "[a][x] ".repeat(100),
            "a".repeat(4_000)
        );
        assert!(matches!(project(&input), Err(error) if error == "MarkdownLimit:Links"));
    }
    #[test]
    fn plain_paste_preserves_raw_text_and_converts_complete_tsv() {
        let raw = "  A😀\tB\r\nC\tD  ";
        for field in ["title", "hypothesis", "conclusion", "todos"] {
            assert_eq!(paste_plain(raw, field).unwrap(), raw);
        }
        assert_eq!(
            paste_plain(raw, "description").unwrap(),
            "| A😀 | B |\n| --- | --- |\n| C | D |"
        );
        let table = paste_plain("a|b\tc\nx\ty", "description").unwrap();
        assert!(table.contains("a\\|b"));
        assert_eq!(project(&table).unwrap().blocks[0].rows.len(), 2);
        assert_eq!(paste_plain("x\ty", "description").unwrap(), "x\ty");
        assert_eq!(
            paste_plain("x", "unknown").unwrap_err(),
            "InvalidPasteSection"
        );
    }
    #[test]
    fn plain_paste_rejects_every_legacy_truncation_or_output_overflow() {
        assert_eq!(
            paste_plain(&"a\tb\n".repeat(501), "description").unwrap_err(),
            "MarkdownLimit:PasteRows"
        );
        let too_wide = format!("{}\n{}", vec!["x"; 81].join("\t"), "a\tb");
        assert_eq!(
            paste_plain(&too_wide, "description").unwrap_err(),
            "MarkdownLimit:PasteColumns"
        );
        assert_eq!(
            paste_plain(&"x".repeat(20_001), "todos").unwrap_err(),
            "MarkdownLimit:Grapheme"
        );
        // A legal 500-row input can expand beyond the body budget; reject the
        // complete result rather than returning a partial table to the editor.
        let expansion = "x\tx\tx\tx\tx\tx\tx\tx\tx\tx\n".repeat(500);
        assert_eq!(
            paste_plain(&expansion, "description").unwrap_err(),
            "MarkdownLimit:Grapheme"
        );
    }
}
