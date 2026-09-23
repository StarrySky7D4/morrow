//! Inert rich-content conversion. The host supplies a parsed tree and extracts
//! embedded file bytes. The guest never opens a URL or executes pasted content.
use crate::capture_capnp as wire;
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};
#[derive(Clone, Debug, Default)]
pub struct Node {
    pub parent: usize,
    pub tag: String,
    pub text: String,
    pub href: String,
    pub src: String,
    pub alt: String,
    pub style: String,
    pub columns: usize,
    pub rows: usize,
    pub index: usize,
    pub formula: String,
}
pub fn digest() -> [u8; 32] {
    Sha256::digest(
        include_str!("../schemas/capture.capnp")
            .replace("\r\n", "\n")
            .as_bytes(),
    )
    .into()
}
fn txt(v: capnp::Result<capnp::text::Reader<'_>>) -> Result<String, &'static str> {
    v.map_err(|_| "text")?
        .to_str()
        .map(str::to_owned)
        .map_err(|_| "utf8")
}
pub fn safe_link(value: &str) -> Option<String> {
    let value = value.trim();
    let u = url::Url::parse(value).ok()?;
    if !u.username().is_empty() || u.password().is_some() {
        return None;
    }
    match u.scheme() {
        "http" | "https" if u.host_str().is_some() => Some(value.into()),
        "mailto" | "attachment" if !u.path().is_empty() => Some(value.into()),
        _ => None,
    }
}
pub fn table(rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let columns = rows.iter().map(Vec::len).max().unwrap_or(1).clamp(1, 80);
    let cell = |v: &str| v.trim().replace('|', "\\|").replace(['\r', '\n'], " / ");
    let row = |values: &[String]| {
        format!(
            "| {} |",
            (0..columns)
                .map(|i| cell(values.get(i).map_or("", String::as_str)))
                .collect::<Vec<_>>()
                .join(" | ")
        )
    };
    let mut result = vec![
        row(&rows[0]),
        format!("| {} |", vec!["---"; columns].join(" | ")),
    ];
    result.extend(rows.iter().skip(1).take(499).map(|r| row(r)));
    result.join("\n")
}
pub fn plain(source: &str) -> String {
    let lines = source.trim().lines().collect::<Vec<_>>();
    if lines.len() >= 2 && lines.iter().filter(|l| l.contains('\t')).count() >= 2 {
        table(
            &lines
                .iter()
                .take(500)
                .map(|l| l.split('\t').take(80).map(str::to_owned).collect())
                .collect::<Vec<_>>(),
        )
    } else {
        source.into()
    }
}
fn children(nodes: &[Node]) -> Result<Vec<Vec<usize>>, &'static str> {
    if nodes.is_empty() || nodes.len() > 1024 || nodes[0].tag != "root" {
        return Err("capture tree");
    }
    let mut children = vec![Vec::new(); nodes.len()];
    for (i, n) in nodes.iter().enumerate().skip(1) {
        if n.parent >= i {
            return Err("capture tree parent");
        }
        children[n.parent].push(i);
    }
    Ok(children)
}
fn plain_node(i: usize, nodes: &[Node], children: &[Vec<usize>], depth: usize) -> String {
    if depth > 40 {
        return String::new();
    }
    let mut out = nodes[i].text.clone();
    for &c in &children[i] {
        out.push_str(&plain_node(c, nodes, children, depth + 1));
    }
    out
}
pub fn html(nodes: &[Node]) -> Result<(String, Vec<String>), &'static str> {
    let children = children(nodes)?;
    let mut warnings = std::collections::BTreeSet::new();
    fn visit(
        i: usize,
        n: &[Node],
        c: &[Vec<usize>],
        depth: usize,
        w: &mut std::collections::BTreeSet<String>,
    ) -> String {
        if depth > 40 {
            return String::new();
        }
        let node = &n[i];
        let tag = node.tag.as_str();
        if matches!(
            tag,
            "script" | "style" | "iframe" | "object" | "embed" | "head" | "meta" | "link"
        ) {
            return String::new();
        }
        if tag == "text" {
            let mut text = String::new();
            let mut space = false;
            for ch in node.text.chars() {
                if ch.is_whitespace() {
                    if !space {
                        text.push(' ');
                    }
                    space = true;
                } else {
                    text.push(ch);
                    space = false;
                }
            }
            return text;
        }
        if tag == "br" {
            return "\n".into();
        }
        if tag == "img" {
            let alt = if node.alt.is_empty() {
                "图片"
            } else {
                &node.alt
            }
            .replace(['[', ']'], "");
            if let Some(src) = safe_link(&node.src) {
                return format!("![{alt}](<{src}>)");
            }
            w.insert("本地链接图片没有自动读取，请粘贴图片或导入原文件。".into());
            return alt;
        }
        if tag == "table" {
            let mut rows = Vec::new();
            let mut occupied = [0usize; 80];
            for j in 0..n.len() {
                if n[j].tag != "tr" {
                    continue;
                }
                let mut ancestor = n[j].parent;
                while ancestor != 0 && n[ancestor].tag != "table" {
                    ancestor = n[ancestor].parent;
                }
                if ancestor != i {
                    continue;
                }
                if rows.len() >= 500 {
                    break;
                }
                let row_index = rows.len();
                let mut cells = Vec::new();
                for &cell in &c[j] {
                    if !matches!(n[cell].tag.as_str(), "td" | "th") {
                        continue;
                    }
                    while cells.len() < 80 && occupied[cells.len()] > row_index {
                        cells.push(String::new());
                    }
                    if cells.len() >= 80 {
                        break;
                    }
                    let column = cells.len();
                    let content = c[cell]
                        .iter()
                        .map(|&v| visit(v, n, c, depth + 1, w))
                        .collect::<String>();
                    cells.push(content.trim().into());
                    let span = n[cell].columns.clamp(1, 80);
                    let rowspan = n[cell].rows.clamp(1, 500);
                    for value in occupied
                        .iter_mut()
                        .take((column + span).min(80))
                        .skip(column)
                    {
                        *value = row_index + rowspan;
                    }
                    if span > 1 || rowspan > 1 {
                        w.insert("合并单元格已转为阅读表格；原始排版保留在 HTML 附件。".into());
                    }
                    cells.extend(vec![String::new(); (span - 1).min(80 - cells.len())]);
                }
                if !cells.is_empty() {
                    rows.push(cells);
                }
            }
            return format!("\n\n{}\n\n", table(&rows));
        }
        if tag == "pre" {
            let text = plain_node(i, n, c, 0);
            let mut fence = "```".to_owned();
            while text.contains(&fence) {
                fence.push('`');
            }
            return format!("\n\n{fence}\n{text}\n{fence}\n\n");
        }
        let text = c[i]
            .iter()
            .map(|&v| visit(v, n, c, depth + 1, w))
            .collect::<String>();
        match tag {
            "code" => format!("`{text}`"),
            "strong" | "b" => {
                if text.trim().is_empty() {
                    text
                } else {
                    format!("**{}**", text.trim())
                }
            }
            "em" | "i" => {
                if text.trim().is_empty() {
                    text
                } else {
                    format!("*{}*", text.trim())
                }
            }
            "del" | "s" => format!("~~{text}~~"),
            "a" => safe_link(&node.href)
                .map_or(text.clone(), |link| format!("[{}](<{link}>)", text.trim())),
            "li" => format!(
                "\n{} {}",
                if n[node.parent].tag == "ol" {
                    "1."
                } else {
                    "-"
                },
                text.trim()
            ),
            "blockquote" => format!("\n\n> {}\n\n", text.trim().replace('\n', "\n> ")),
            "p" | "div" | "section" | "article" | "ul" | "ol" => format!("\n\n{}\n\n", text.trim()),
            "span" => {
                if node
                    .style
                    .split(';')
                    .filter_map(|s| s.split_once(':'))
                    .any(|(k, v)| {
                        k.trim() == "font-weight"
                            && matches!(v.trim(), "bold" | "700" | "800" | "900")
                    })
                {
                    format!("**{}**", text.trim())
                } else if node.style.contains("italic") {
                    format!("*{}*", text.trim())
                } else {
                    text
                }
            }
            _ => {
                if tag.len() == 2
                    && tag.starts_with('h')
                    && matches!(tag.as_bytes()[1], b'1'..=b'6')
                {
                    format!(
                        "\n\n{} {}\n\n",
                        "#".repeat((tag.as_bytes()[1] - b'0') as usize),
                        text.trim()
                    )
                } else {
                    text
                }
            }
        }
    }
    let text = visit(0, nodes, &children, 0, &mut warnings);
    let clean = regex::Regex::new(r"[ \t]+\n")
        .unwrap()
        .replace_all(&text, "\n");
    let clean = regex::Regex::new(r"\n{3,}")
        .unwrap()
        .replace_all(&clean, "\n\n")
        .trim()
        .to_owned();
    Ok((clean, warnings.into_iter().collect()))
}
pub fn spreadsheet(nodes: &[Node]) -> Result<(String, Vec<String>), &'static str> {
    let c = children(nodes)?;
    let mut tables = Vec::new();
    for (i, node) in nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.tag == "Table")
        .take(8)
    {
        let _ = node;
        let mut rows = Vec::new();
        for &r in c[i].iter().filter(|&&v| nodes[v].tag == "Row").take(500) {
            let mut cells = Vec::new();
            for &cell in c[r].iter().filter(|&&v| nodes[v].tag == "Cell") {
                let n = &nodes[cell];
                let index = if n.index == 0 {
                    cells.len() + 1
                } else {
                    n.index
                }
                .clamp(1, 80);
                while cells.len() < index - 1 {
                    cells.push(String::new());
                }
                let value = c[cell]
                    .iter()
                    .find(|&&v| nodes[v].tag == "Data")
                    .map_or(String::new(), |&v| plain_node(v, nodes, &c, 0));
                cells.push(if n.formula.is_empty() {
                    value
                } else {
                    format!("{value} (公式: {})", n.formula)
                });
                if cells.len() >= 80 {
                    break;
                }
            }
            rows.push(cells);
        }
        tables.push(table(&rows));
    }
    Ok((
        tables
            .into_iter()
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"),
        vec!["表格显示值与公式已转为 Markdown，格式和合并信息保留在 XML 附件。".into()],
    ))
}
pub fn rtf(source: &str) -> Result<String, &'static str> {
    let input = source.encode_utf16().collect::<Vec<_>>();
    let (mut skip, mut uc, mut fallback) = (false, 1usize, 0usize);
    let mut stack = Vec::new();
    let mut out = Vec::new();
    let mut i = 0;
    let destinations = [
        "fonttbl",
        "colortbl",
        "stylesheet",
        "info",
        "pict",
        "object",
        "objdata",
        "listtable",
        "listoverridetable",
        "generator",
        "datastore",
        "themedata",
        "xmlopen",
        "xmlattrname",
    ];
    while i < input.len() {
        let value = input[i];
        i += 1;
        match value {
            123 => {
                if stack.len() >= 128 {
                    return Err("RTF depth");
                }
                stack.push((skip, uc));
            }
            125 => {
                if let Some(v) = stack.pop() {
                    (skip, uc) = v;
                }
            }
            92 => {
                if i >= input.len() {
                    break;
                }
                let next = input[i];
                i += 1;
                if next == 42 {
                    skip = true;
                    continue;
                }
                if next == 39 {
                    if i + 1 < input.len() {
                        let hex = String::from_utf16_lossy(&input[i..i + 2]);
                        if fallback > 0 {
                            fallback -= 1;
                        } else if !skip && let Ok(value) = u16::from_str_radix(&hex, 16) {
                            out.push(value);
                        }
                        i += 2;
                    }
                    continue;
                }
                if !(next <= 127 && (next as u8).is_ascii_alphabetic()) {
                    if fallback > 0 {
                        fallback -= 1;
                    } else if !skip {
                        if next == 126 {
                            out.push(32);
                        } else if [92, 123, 125].contains(&next) {
                            out.push(next);
                        }
                    }
                    continue;
                }
                let start = i - 1;
                while i < input.len() && input[i] <= 127 && (input[i] as u8).is_ascii_alphabetic() {
                    i += 1;
                }
                let word = String::from_utf16_lossy(&input[start..i]);
                let start = i;
                if i < input.len() && input[i] == 45 {
                    i += 1;
                }
                while i < input.len() && input[i] <= 127 && (input[i] as u8).is_ascii_digit() {
                    i += 1;
                }
                let number = String::from_utf16_lossy(&input[start..i])
                    .parse::<i32>()
                    .ok();
                if i < input.len() && input[i] == 32 {
                    i += 1;
                }
                if destinations.contains(&word.as_str()) {
                    skip = true;
                }
                if word == "uc"
                    && let Some(n) = number
                {
                    uc = n.clamp(0, 16) as usize;
                }
                if skip {
                    continue;
                }
                if word == "u"
                    && let Some(n) = number
                {
                    out.push(n as u16);
                    fallback = uc;
                }
                if matches!(word.as_str(), "par" | "line" | "row") {
                    out.push(10);
                }
                if matches!(word.as_str(), "tab" | "cell") {
                    out.push(9);
                }
            }
            _ => {
                if fallback > 0 {
                    fallback -= 1;
                } else if !skip && value != 13 && value != 10 {
                    out.push(value);
                }
            }
        }
    }
    Ok(String::from_utf16_lossy(&out).trim().into())
}
pub fn process(input: &[u8]) -> Result<Vec<u8>, &'static str> {
    if input.len() > 65536 {
        return Err("capture message budget");
    }
    let mut cursor = std::io::Cursor::new(input);
    let m = serialize::read_message(
        &mut cursor,
        ReaderOptions {
            traversal_limit_in_words: Some(8192),
            nesting_limit: 16,
        },
    )
    .map_err(|_| "capture frame")?;
    if cursor.position() != input.len() as u64 {
        return Err("capture trailing bytes");
    }
    let r = m
        .get_root::<wire::request::Reader>()
        .map_err(|_| "capture request")?;
    if r.get_version() != 1 || r.get_digest().map_err(|_| "digest")? != digest() {
        return Err("capture contract");
    }
    let nodes = r.get_nodes().map_err(|_| "nodes")?;
    if nodes.len() > 1024 {
        return Err("capture node budget");
    }
    let nodes = nodes
        .iter()
        .map(|n| {
            Ok(Node {
                parent: n.get_parent() as usize,
                tag: txt(n.get_tag())?,
                text: txt(n.get_text())?,
                href: txt(n.get_href())?,
                src: txt(n.get_src())?,
                alt: txt(n.get_alt())?,
                style: txt(n.get_style())?,
                columns: n.get_columns() as usize,
                rows: n.get_rows() as usize,
                index: n.get_index() as usize,
                formula: txt(n.get_formula())?,
            })
        })
        .collect::<Result<Vec<_>, &'static str>>()?;
    let (markdown, warnings) = match txt(r.get_format())?.as_str() {
        "html" => html(&nodes)?,
        "spreadsheet" => spreadsheet(&nodes)?,
        "rtf" => (rtf(&txt(r.get_source())?)?, vec![]),
        "plain" => (plain(&txt(r.get_source())?), vec![]),
        _ => return Err("capture format"),
    };
    let mut m = Builder::new_default();
    let mut b = m.init_root::<wire::response::Builder>();
    b.set_version(1);
    b.set_digest(&digest());
    b.set_markdown(markdown.as_str());
    let mut list = b.init_warnings(warnings.len() as u32);
    for (i, w) in warnings.iter().enumerate() {
        list.set(i as u32, w.as_str());
    }
    let bytes = serialize::write_message_to_words(&m);
    if bytes.len() > 65536 {
        return Err("capture output budget");
    }
    Ok(bytes)
}
