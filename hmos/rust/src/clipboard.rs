//! Read-only, hash-bound clipboard normalization. No URI, network, executable
//! content, Store mutation or capture ticket enters this adapter.
use crate::{Result, err, file_stream, hex};
use base64::{
    Engine, alphabet,
    engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
};
use encoding_rs::{Encoding, WINDOWS_1252};
use morrow_workbench_plugin::capture::{self, Node};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    collections::HashMap,
    io::{Read, Write},
};

pub const MAX_SOURCE_BYTES: usize = 2 * 1024 * 1024;
const MAX_NODES: usize = 1024;
const MAX_DEPTH: usize = 40;
const MAX_IMAGES: usize = 10;
const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_OUTPUT_UTF16: usize = 20_000;
const SS_NS: &str = "urn:schemas-microsoft-com:office:spreadsheet";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConvertRequest {
    pub format: String,
    pub section: String,
    pub expected_sha256: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageRequest {
    pub format: String,
    pub section: String,
    pub expected_sha256: String,
    pub local_id: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct ImageMetadata {
    pub local_id: String,
    pub name: String,
    pub byte_length: String,
    pub sha256: String,
}
#[derive(Debug, Default, Serialize)]
pub struct ConvertReply {
    pub ok: bool,
    pub error: String,
    pub paste_text: String,
    pub warnings: Vec<String>,
    pub images: Vec<ImageMetadata>,
    pub source_sha256: String,
    pub source_byte_length: String,
}
impl ConvertReply {
    pub fn failure(error: String) -> Self {
        Self {
            error,
            ..Self::default()
        }
    }
}
struct Image {
    metadata: ImageMetadata,
    bytes: Vec<u8>,
}
struct Converted {
    reply: ConvertReply,
    images: Vec<Image>,
}

fn validate(format: &str, section: &str, hash: &str) -> Result<()> {
    if !matches!(format, "html" | "rtf" | "xml" | "plain") {
        return Err("ClipboardFormat".into());
    }
    if !matches!(
        section,
        "description" | "title" | "todos" | "hypothesis" | "conclusion"
    ) {
        return Err("InvalidPasteSection".into());
    }
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("ClipboardSourceHashRequired".into());
    }
    Ok(())
}
fn read_source(reader: &mut impl Read, expected: &str) -> Result<Vec<u8>> {
    let mut source = Vec::new();
    reader
        .take((MAX_SOURCE_BYTES + 1) as u64)
        .read_to_end(&mut source)
        .map_err(err)?;
    if source.len() > MAX_SOURCE_BYTES {
        return Err("ClipboardSourceByteLimit".into());
    }
    if hex(&Sha256::digest(&source)) != expected {
        return Err("ClipboardSourceHashMismatch".into());
    }
    Ok(source)
}
fn utf8(source: &[u8]) -> Result<&str> {
    let source = source.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(source);
    let text = std::str::from_utf8(source).map_err(|_| "ClipboardUtf8Required")?;
    if text.contains('\0') {
        return Err("ClipboardNulRejected".into());
    }
    Ok(text)
}
fn text_source(source: &[u8]) -> Result<Cow<'_, str>> {
    if source.starts_with(&[0xff, 0xfe]) || source.starts_with(&[0xfe, 0xff]) {
        if source.len() % 2 != 0 {
            return Err("ClipboardUtf16Required".into());
        }
        let little = source[0] == 0xff;
        let units = source[2..]
            .chunks_exact(2)
            .map(|v| {
                if little {
                    u16::from_le_bytes([v[0], v[1]])
                } else {
                    u16::from_be_bytes([v[0], v[1]])
                }
            })
            .collect::<Vec<_>>();
        let text = String::from_utf16(&units).map_err(|_| "ClipboardUtf16Required")?;
        if text.contains('\0') {
            return Err("ClipboardNulRejected".into());
        }
        Ok(Cow::Owned(text))
    } else {
        Ok(Cow::Borrowed(utf8(source)?))
    }
}
fn output_limit(text: &str) -> Result<()> {
    if text.encode_utf16().count() > MAX_OUTPUT_UTF16 {
        Err("ClipboardOutputLimit".into())
    } else {
        Ok(())
    }
}
fn add(nodes: &mut Vec<Node>, node: Node) -> Result<usize> {
    if nodes.len() >= MAX_NODES {
        return Err("ClipboardNodeLimit".into());
    }
    nodes.push(node);
    Ok(nodes.len() - 1)
}
fn span(value: Option<&str>, maximum: usize) -> Result<usize> {
    match value {
        None => Ok(1),
        Some(value) => value
            .parse::<usize>()
            .ok()
            .filter(|&v| v > 0 && v <= maximum)
            .ok_or_else(|| "ClipboardTableSpanLimit".into()),
    }
}
fn percent_bytes(value: &str) -> Result<Vec<u8>> {
    let input = value.as_bytes();
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        if input[i] == b'%' {
            if i + 2 >= input.len() {
                return Err("ClipboardDataImageEncoding".into());
            }
            let a = (input[i + 1] as char)
                .to_digit(16)
                .ok_or("ClipboardDataImageEncoding")?;
            let b = (input[i + 2] as char)
                .to_digit(16)
                .ok_or("ClipboardDataImageEncoding")?;
            out.push((a * 16 + b) as u8);
            i += 3;
        } else {
            out.push(input[i]);
            i += 1;
        }
    }
    Ok(out)
}
fn image(src: &str, hash: &str, images: &mut Vec<Image>) -> Result<String> {
    if images.len() >= MAX_IMAGES {
        return Err("ClipboardImageCountLimit".into());
    }
    let (header, body) = src.split_once(',').ok_or("ClipboardDataImageEncoding")?;
    let mut parts = header
        .strip_prefix("data:")
        .ok_or("ClipboardDataImageEncoding")?
        .split(';');
    let mime = parts.next().unwrap_or("").to_ascii_lowercase();
    let ext = match mime.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => return Err("ClipboardDataImageFormat".into()),
    };
    let mut base64 = false;
    for part in parts {
        if part.eq_ignore_ascii_case("base64") && !base64 {
            base64 = true;
        } else if !part.starts_with("charset=") {
            return Err("ClipboardDataImageEncoding".into());
        }
    }
    let mut bytes = percent_bytes(body)?;
    if base64 {
        for byte in &mut bytes {
            if *byte == b'-' {
                *byte = b'+';
            } else if *byte == b'_' {
                *byte = b'/';
            }
        }
        let decoder = GeneralPurpose::new(
            &alphabet::STANDARD,
            GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
        );
        bytes = decoder
            .decode(&bytes)
            .map_err(|_| "ClipboardDataImageEncoding")?;
    }
    let correct_header = match ext {
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" => bytes.starts_with(b"\xff\xd8\xff"),
        "gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "webp" => bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        _ => false,
    };
    if !correct_header {
        return Err("ClipboardDataImageSignature".into());
    }
    let total = images
        .iter()
        .try_fold(bytes.len(), |total, image| {
            total.checked_add(image.bytes.len())
        })
        .ok_or("ClipboardImageByteLimit")?;
    if total > MAX_IMAGE_BYTES {
        return Err("ClipboardImageByteLimit".into());
    }
    let local_id = format!("image-{}", images.len() + 1);
    let name = format!("clipboard-{hash}-{local_id}.{ext}");
    let metadata = ImageMetadata {
        local_id,
        name: name.clone(),
        byte_length: bytes.len().to_string(),
        sha256: hex(&Sha256::digest(&bytes)),
    };
    images.push(Image { metadata, bytes });
    Ok(format!("attachment:{name}"))
}
fn safe_href(value: &str) -> String {
    capture::safe_link(value)
        .filter(|v| {
            !v.split_once(':')
                .is_some_and(|(scheme, _)| scheme.eq_ignore_ascii_case("attachment"))
        })
        .unwrap_or_default()
}

fn html(source: &str, hash: &str) -> Result<(Vec<Node>, Vec<Image>)> {
    // html5ever implements real HTML5 parsing, including entity decoding,
    // implied table nodes and malformed-tag recovery. The DOM is inert.
    use html5ever::{QualName, local_name, ns, tendril::TendrilSink};
    fn dom_limits(doc: &scraper::Html) -> Result<()> {
        if doc.tree.nodes().count() > MAX_NODES {
            return Err("ClipboardNodeLimit".into());
        }
        for node in doc.tree.nodes() {
            if node.ancestors().take(MAX_DEPTH + 3).count() > MAX_DEPTH + 2 {
                return Err("ClipboardDepthLimit".into());
            }
        }
        Ok(())
    }
    let mut parser = html5ever::parse_fragment(
        scraper::HtmlTreeSink::new(scraper::Html::new_fragment()),
        Default::default(),
        QualName::new(None, ns!(html), local_name!("body")),
        vec![],
        false,
    );
    // Check the actual growing DOM between bounded UTF-8 chunks. A hostile
    // source cannot allocate an entire two-MiB tree before its node/depth limit.
    let mut start = 0;
    while start < source.len() {
        let mut end = (start + 256).min(source.len());
        while !source.is_char_boundary(end) {
            end -= 1;
        }
        parser.process(source[start..end].into());
        dom_limits(&parser.tokenizer.sink.sink.0.borrow())?;
        start = end;
    }
    let doc = parser.finish();
    dom_limits(&doc)?;
    let mut nodes = vec![Node {
        tag: "root".into(),
        ..Node::default()
    }];
    let mut images = Vec::new();
    let root = doc.root_element();
    let mut pending = root
        .children()
        .rev()
        .map(|n| (n, 0usize, 1usize))
        .collect::<Vec<_>>();
    while let Some((node, parent, depth)) = pending.pop() {
        if depth > MAX_DEPTH {
            return Err("ClipboardDepthLimit".into());
        }
        let current = match node.value() {
            scraper::Node::Text(text) => add(
                &mut nodes,
                Node {
                    parent,
                    tag: "text".into(),
                    text: text.to_string(),
                    ..Node::default()
                },
            )?,
            scraper::Node::Element(element) => {
                let tag = element.name();
                if matches!(
                    tag,
                    "script"
                        | "style"
                        | "iframe"
                        | "object"
                        | "embed"
                        | "head"
                        | "meta"
                        | "link"
                        | "template"
                ) {
                    continue;
                }
                let mut src = element.attr("src").unwrap_or("").to_owned();
                if tag == "img" && src.starts_with("data:") {
                    src = image(&src, hash, &mut images)?;
                } else if tag == "img" {
                    src = safe_href(&src);
                }
                add(
                    &mut nodes,
                    Node {
                        parent,
                        tag: tag.into(),
                        href: safe_href(element.attr("href").unwrap_or("")),
                        src,
                        alt: element.attr("alt").unwrap_or("").into(),
                        style: element.attr("style").unwrap_or("").into(),
                        columns: span(element.attr("colspan"), 80)?,
                        rows: span(element.attr("rowspan"), 500)?,
                        ..Node::default()
                    },
                )?
            }
            _ => continue,
        };
        pending.extend(node.children().rev().map(|n| (n, current, depth + 1)));
    }
    html_table_limits(&nodes)?;
    Ok((nodes, images))
}
fn html_table_limits(nodes: &[Node]) -> Result<()> {
    for (table, _) in nodes.iter().enumerate().filter(|(_, n)| n.tag == "table") {
        let mut rows = Vec::new();
        for (id, node) in nodes.iter().enumerate().filter(|(_, n)| n.tag == "tr") {
            let mut parent = node.parent;
            while parent != 0 && nodes[parent].tag != "table" {
                parent = nodes[parent].parent;
            }
            if parent == table {
                rows.push(id);
            }
        }
        if rows.len() > 500 {
            return Err("ClipboardTableRowLimit".into());
        }
        let mut occupied = [0usize; 80];
        for (r, row) in rows.iter().enumerate() {
            let mut column = 0;
            for cell in nodes
                .iter()
                .filter(|n| n.parent == *row && matches!(n.tag.as_str(), "th" | "td"))
            {
                while column < 80 && occupied[column] > r {
                    column += 1;
                }
                if column + cell.columns > 80 {
                    return Err("ClipboardTableColumnLimit".into());
                }
                for slot in &mut occupied[column..column + cell.columns] {
                    *slot = r + cell.rows;
                }
                column += cell.columns;
            }
        }
    }
    Ok(())
}
fn xml(source: &str) -> Result<Vec<Node>> {
    let doc = roxmltree::Document::parse_with_options(
        source,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_NODES as u32,
            ..Default::default()
        },
    )
    .map_err(|e| format!("ClipboardXml: {e}"))?;
    let mut nodes = vec![Node {
        tag: "root".into(),
        ..Node::default()
    }];
    let mut pending = doc
        .root()
        .children()
        .rev()
        .map(|n| (n, 0usize, 1usize))
        .collect::<Vec<_>>();
    while let Some((node, parent, depth)) = pending.pop() {
        if depth > MAX_DEPTH {
            return Err("ClipboardDepthLimit".into());
        }
        let current = if node.is_text() {
            add(
                &mut nodes,
                Node {
                    parent,
                    tag: "text".into(),
                    text: node.text().unwrap_or("").into(),
                    ..Node::default()
                },
            )?
        } else if node.is_element() {
            let tag = node.tag_name();
            if matches!(
                tag.name(),
                "Workbook" | "Worksheet" | "Table" | "Row" | "Cell" | "Data"
            ) && tag.namespace().is_some_and(|ns| ns != SS_NS)
            {
                return Err("ClipboardSpreadsheetNamespace".into());
            }
            let mut index = 0;
            let mut formula = String::new();
            for attr in node.attributes() {
                if matches!(attr.name(), "Index" | "Formula") {
                    if attr.namespace().is_some_and(|ns| ns != SS_NS) {
                        return Err("ClipboardSpreadsheetAttributeNamespace".into());
                    }
                    if attr.name() == "Index" {
                        index = span(Some(attr.value()), 80)?;
                    } else {
                        formula = attr.value().into();
                    }
                }
            }
            add(
                &mut nodes,
                Node {
                    parent,
                    tag: tag.name().into(),
                    index,
                    formula,
                    ..Node::default()
                },
            )?
        } else {
            continue;
        };
        pending.extend(node.children().rev().map(|n| (n, current, depth + 1)));
    }
    let tables = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.tag == "Table")
        .collect::<Vec<_>>();
    if tables.is_empty() {
        return Err("ClipboardSpreadsheetTableRequired".into());
    }
    if tables.len() > 8 {
        return Err("ClipboardSpreadsheetTableLimit".into());
    }
    for (table, _) in tables {
        let rows = nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.parent == table && n.tag == "Row")
            .collect::<Vec<_>>();
        if rows.len() > 500 {
            return Err("ClipboardTableRowLimit".into());
        }
        for (row, _) in rows {
            let mut column = 0;
            for cell in nodes.iter().filter(|n| n.parent == row && n.tag == "Cell") {
                let index = if cell.index == 0 {
                    column + 1
                } else {
                    cell.index
                };
                if index <= column || index > 80 {
                    return Err("ClipboardTableColumnLimit".into());
                }
                column = index;
            }
        }
    }
    Ok(nodes)
}
fn plain_nodes(nodes: &[Node]) -> String {
    let mut children = vec![Vec::new(); nodes.len()];
    for (i, node) in nodes.iter().enumerate().skip(1) {
        children[node.parent].push(i);
    }
    fn visit(i: usize, nodes: &[Node], children: &[Vec<usize>]) -> String {
        let node = &nodes[i];
        if node.tag == "text" {
            return node.text.clone();
        }
        if node.tag == "br" {
            return "\n".into();
        }
        if node.tag == "img" {
            return node.alt.clone();
        }
        if matches!(node.tag.as_str(), "tr" | "Row") {
            let cells = children[i]
                .iter()
                .copied()
                .filter(|&j| matches!(nodes[j].tag.as_str(), "td" | "th" | "Cell"))
                .map(|j| visit(j, nodes, children))
                .collect::<Vec<_>>();
            return format!("{}\n", cells.join("\t"));
        }
        let text = children[i]
            .iter()
            .map(|&j| visit(j, nodes, children))
            .collect::<Vec<_>>();
        let text = text.join("");
        if matches!(
            node.tag.as_str(),
            "p" | "div"
                | "section"
                | "article"
                | "li"
                | "blockquote"
                | "pre"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
        ) {
            format!("\n{}\n", text.trim())
        } else {
            text
        }
    }
    visit(0, nodes, &children).trim().into()
}

fn codepage(number: i32) -> Result<&'static Encoding> {
    let label = match number {
        1250 => "windows-1250",
        1251 => "windows-1251",
        1252 => "windows-1252",
        1253 => "windows-1253",
        1254 => "windows-1254",
        1255 => "windows-1255",
        1256 => "windows-1256",
        1257 => "windows-1257",
        1258 => "windows-1258",
        874 => "windows-874",
        932 => "shift_jis",
        936 => "gbk",
        949 => "euc-kr",
        950 => "big5",
        65001 => "utf-8",
        _ => return Err("ClipboardRtfCodePageUnsupported".into()),
    };
    Encoding::for_label_no_replacement(label.as_bytes())
        .ok_or_else(|| "ClipboardRtfCodePageUnsupported".into())
}
fn font_encoding(charset: i32, default: &'static Encoding) -> Result<&'static Encoding> {
    match charset {
        0 | 1 => Ok(default),
        128 => codepage(932),
        129 => codepage(949),
        134 => codepage(936),
        136 => codepage(950),
        161 => codepage(1253),
        162 => codepage(1254),
        163 => codepage(1258),
        177 => codepage(1255),
        178 => codepage(1256),
        186 => codepage(1257),
        204 => codepage(1251),
        222 => codepage(874),
        238 => codepage(1250),
        _ => Err("ClipboardRtfFontEncodingUnsupported".into()),
    }
}
#[derive(Clone, Copy)]
struct RtfState {
    encoding: &'static Encoding,
    font_table: bool,
    font: Option<i32>,
}
fn flush_rtf(bytes: &mut Vec<u8>, out: &mut String, encoding: &'static Encoding) -> Result<()> {
    if bytes.is_empty() {
        return Ok(());
    }
    let text = encoding
        .decode_without_bom_handling_and_without_replacement(bytes)
        .ok_or("ClipboardRtfEncoding")?;
    for ch in text.chars() {
        if matches!(ch, '\\' | '{' | '}') {
            out.push('\\');
        }
        out.push(ch);
    }
    bytes.clear();
    Ok(())
}
/// Normalize ANSI/hex bytes separately from the immutable RTF source. Keep
/// controls/destinations for the shared converter; never execute embedded data.
fn normalize_rtf(source: &[u8]) -> Result<String> {
    if !source.starts_with(b"{\\rtf1") || source.contains(&0) {
        return Err("ClipboardRtfSyntax".into());
    }
    let mut state = RtfState {
        encoding: WINDOWS_1252,
        font_table: false,
        font: None,
    };
    let mut default = WINDOWS_1252;
    let mut fonts = HashMap::new();
    let mut stack = Vec::new();
    let mut out = String::new();
    let mut bytes = Vec::new();
    let mut i = 0;
    while i < source.len() {
        let byte = source[i];
        if byte == b'\\' && source.get(i + 1) == Some(&b'\'') {
            if i + 3 >= source.len() {
                return Err("ClipboardRtfHex".into());
            }
            let a = (source[i + 2] as char)
                .to_digit(16)
                .ok_or("ClipboardRtfHex")?;
            let b = (source[i + 3] as char)
                .to_digit(16)
                .ok_or("ClipboardRtfHex")?;
            bytes.push((a * 16 + b) as u8);
            i += 4;
            if bytes.len() >= 256
                && state
                    .encoding
                    .decode_without_bom_handling_and_without_replacement(&bytes)
                    .is_some()
            {
                flush_rtf(&mut bytes, &mut out, state.encoding)?;
            }
            continue;
        }
        // In a DBCS run a raw trail byte may look like an RTF delimiter.
        // Byte runs are flushed in small complete chunks to avoid quadratic
        // decoding of a large paragraph. Escaped hex runs are handled above.
        if !bytes.is_empty()
            && !state.encoding.is_single_byte()
            && state
                .encoding
                .decode_without_bom_handling_and_without_replacement(&bytes)
                .is_none()
        {
            bytes.push(byte);
            i += 1;
            continue;
        }
        if !matches!(byte, b'{' | b'}' | b'\\') {
            bytes.push(byte);
            i += 1;
            if bytes.len() >= 256 {
                flush_rtf(&mut bytes, &mut out, state.encoding)?;
            }
            continue;
        }
        flush_rtf(&mut bytes, &mut out, state.encoding)?;
        if byte == b'{' {
            if stack.len() >= MAX_DEPTH {
                return Err("ClipboardDepthLimit".into());
            }
            stack.push(state);
            out.push('{');
            i += 1;
        } else if byte == b'}' {
            state = stack.pop().ok_or("ClipboardRtfSyntax")?;
            out.push('}');
            i += 1;
            if stack.is_empty() && source[i..].iter().any(|b| !b.is_ascii_whitespace()) {
                return Err("ClipboardRtfSyntax".into());
            }
        } else {
            let start = i;
            i += 1;
            if i >= source.len() {
                return Err("ClipboardRtfSyntax".into());
            }
            if !source[i].is_ascii_alphabetic() {
                out.push('\\');
                out.push(source[i] as char);
                i += 1;
                continue;
            }
            let word_start = i;
            while i < source.len() && source[i].is_ascii_alphabetic() {
                i += 1;
            }
            let word = std::str::from_utf8(&source[word_start..i]).unwrap();
            let number_start = i;
            if source.get(i) == Some(&b'-') {
                i += 1;
            }
            while i < source.len() && source[i].is_ascii_digit() {
                i += 1;
            }
            let number = if number_start == i {
                None
            } else {
                Some(
                    std::str::from_utf8(&source[number_start..i])
                        .unwrap()
                        .parse::<i32>()
                        .map_err(|_| "ClipboardRtfControl")?,
                )
            };
            if source.get(i) == Some(&b' ') {
                i += 1;
            }
            match word {
                "bin" => return Err("ClipboardRtfBinaryUnsupported".into()),
                "ansicpg" => {
                    default = codepage(number.ok_or("ClipboardRtfControl")?)?;
                    state.encoding = default;
                }
                "fonttbl" => state.font_table = true,
                "f" => {
                    let font = number.ok_or("ClipboardRtfControl")?;
                    state.font = Some(font);
                    if !state.font_table {
                        state.encoding = fonts.get(&font).copied().unwrap_or(default);
                    }
                }
                "fcharset" if state.font_table => {
                    let encoding = font_encoding(number.ok_or("ClipboardRtfControl")?, default)?;
                    fonts.insert(state.font.ok_or("ClipboardRtfFont")?, encoding);
                }
                "cpg" if state.font_table => {
                    fonts.insert(
                        state.font.ok_or("ClipboardRtfFont")?,
                        codepage(number.ok_or("ClipboardRtfControl")?)?,
                    );
                }
                "uc" if number.is_none_or(|n| !(0..=16).contains(&n)) => {
                    return Err("ClipboardRtfUnicode".into());
                }
                "u" if number.is_none_or(|n| !(-32768..=65535).contains(&n)) => {
                    return Err("ClipboardRtfUnicode".into());
                }
                _ => {}
            }
            out.push_str(std::str::from_utf8(&source[start..i]).unwrap());
        }
        if out.len() > MAX_SOURCE_BYTES * 4 {
            return Err("ClipboardRtfNormalizationLimit".into());
        }
    }
    flush_rtf(&mut bytes, &mut out, state.encoding)?;
    if !stack.is_empty() {
        return Err("ClipboardRtfSyntax".into());
    }
    Ok(out)
}
fn normalized(request: &ConvertRequest, source: &[u8]) -> Result<Converted> {
    validate(&request.format, &request.section, &request.expected_sha256)?;
    let description = request.section == "description";
    let (text, warnings, images) = match request.format.as_str() {
        "plain" => (
            crate::markdown::paste_plain(&text_source(source)?, &request.section)?,
            vec![],
            vec![],
        ),
        "html" => {
            let (nodes, images) = html(&text_source(source)?, &request.expected_sha256)?;
            let (markdown, warnings) = capture::html(&nodes).map_err(err)?;
            (
                if description {
                    markdown
                } else {
                    plain_nodes(&nodes)
                },
                warnings,
                images,
            )
        }
        "xml" => {
            let nodes = xml(&text_source(source)?)?;
            let (markdown, warnings) = capture::spreadsheet(&nodes).map_err(err)?;
            (
                if description {
                    markdown
                } else {
                    plain_nodes(&nodes)
                },
                warnings,
                vec![],
            )
        }
        "rtf" => {
            let text = capture::rtf(&normalize_rtf(source)?).map_err(err)?;
            if text.contains('\u{fffd}') {
                return Err("ClipboardRtfUnicode".into());
            }
            (
                crate::markdown::paste_plain(&text, &request.section)?,
                vec![],
                vec![],
            )
        }
        _ => unreachable!(),
    };
    output_limit(&text)?;
    let reply = ConvertReply {
        ok: true,
        error: String::new(),
        paste_text: text,
        warnings,
        images: images.iter().map(|i| i.metadata.clone()).collect(),
        source_sha256: request.expected_sha256.clone(),
        source_byte_length: source.len().to_string(),
    };
    Ok(Converted { reply, images })
}
pub fn convert(input: &str, reader: &mut impl Read) -> Result<ConvertReply> {
    if input.len() > crate::LIMIT {
        return Err("RequestTooLarge".into());
    }
    let request: ConvertRequest = serde_json::from_str(input).map_err(|_| "ClipboardRequest")?;
    validate(&request.format, &request.section, &request.expected_sha256)?;
    let source = read_source(reader, &request.expected_sha256)?;
    Ok(normalized(&request, &source)?.reply)
}
pub fn extract_bytes(input: &str, reader: &mut impl Read, maximum: u64) -> Result<Vec<u8>> {
    if input.len() > crate::LIMIT {
        return Err("RequestTooLarge".into());
    }
    if maximum == 0 || maximum > file_stream::MAX_IMPORT_BYTES {
        return Err("ImportByteLimit".into());
    }
    let request: ImageRequest = serde_json::from_str(input).map_err(|_| "ClipboardImageRequest")?;
    validate(&request.format, &request.section, &request.expected_sha256)?;
    if request.format != "html" {
        return Err("ClipboardImageFormat".into());
    }
    let source = read_source(reader, &request.expected_sha256)?;
    let converted = normalized(
        &ConvertRequest {
            format: request.format,
            section: request.section,
            expected_sha256: request.expected_sha256,
        },
        &source,
    )?;
    let image = converted
        .images
        .into_iter()
        .find(|image| image.metadata.local_id == request.local_id)
        .ok_or("ClipboardImageNotFound")?;
    if image.bytes.len() as u64 > maximum {
        return Err("ImportByteLimit".into());
    }
    Ok(image.bytes)
}
pub fn extract(
    input: &str,
    reader: &mut impl Read,
    writer: &mut impl Write,
    maximum: u64,
) -> Result<file_stream::FileMetadata> {
    let bytes = extract_bytes(input, reader, maximum)?;
    file_stream::prepare(&mut bytes.as_slice(), writer, maximum)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(format: &str, section: &str, source: &[u8]) -> String {
        serde_json::json!({"format":format,"section":section,"expected_sha256":hex(&Sha256::digest(source))}).to_string()
    }
    fn run(format: &str, source: &str) -> ConvertReply {
        convert(
            &request(format, "description", source.as_bytes()),
            &mut source.as_bytes(),
        )
        .unwrap()
    }
    fn fail(format: &str, source: &str, expected: &str) {
        let error = convert(
            &request(format, "description", source.as_bytes()),
            &mut source.as_bytes(),
        )
        .unwrap_err();
        assert!(
            error.contains(expected),
            "{error} does not contain {expected}"
        );
    }
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfixture";
    fn image_html(count: usize) -> String {
        let src = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(PNG)
        );
        format!(
            "<p>{}</p>",
            format!("<img alt='图' src='{src}'>").repeat(count)
        )
    }
    #[test]
    fn flutter_word_html_fixture_preserves_structure_without_active_content() {
        // Exact input from Flutter test/rich_capture_test.dart.
        let reply = run(
            "html",
            "<h2>研究</h2><p><b>重点</b> 与 <i>观察</i></p><ul><li>一步</li></ul><script>alert(1)</script><a href=\"javascript:alert(1)\">危险链接</a>",
        );
        assert_eq!(
            reply.paste_text,
            "## 研究\n\n**重点** 与 *观察*\n\n- 一步\n\n危险链接"
        );
        assert!(reply.ok && reply.images.is_empty() && !reply.paste_text.contains("alert"));
        assert!(!serde_json::to_string(&reply).unwrap().contains("ticket"));
    }
    #[test]
    fn actual_dom_recovers_tags_entities_tables_and_foreign_active_nodes() {
        let reply = run(
            "html",
            "<p>中 &amp; 文<b>bold</p><p>next</b><table><tr><td>A<td>B</table><iframe>hide</iframe><template>hidden</template>",
        );
        assert!(reply.paste_text.contains("中 & 文"));
        assert!(reply.paste_text.contains("| A | B |"));
        assert!(!reply.paste_text.contains("hide"));
        assert!(!reply.paste_text.contains("hidden"));
    }
    #[test]
    fn flutter_merged_html_table_retains_empty_occupied_cell_and_warning() {
        let reply = run(
            "html",
            "<table><tr><td rowspan=\"2\">跨行</td><td>A</td></tr><tr><td>B</td></tr></table>",
        );
        assert_eq!(reply.paste_text, "| 跨行 | A |\n| --- | --- |\n|  | B |");
        assert!(reply.warnings.iter().any(|v| v.contains("合并")));
    }
    #[test]
    fn unsafe_links_and_forged_attachment_aliases_are_inert() {
        let reply = run(
            "html",
            "<a href='https://u:pw@example.com'>auth</a><a href='file:///secret'>file</a><a href='attachment:asset'>forged</a><a href='ATTACHMENT:asset'>forged-case</a><a href='https://example.com'>safe</a><img src='file:///x' alt='local'>",
        );
        assert!(!reply.paste_text.contains("secret") && !reply.paste_text.contains("attachment:"));
        assert!(reply.paste_text.contains("[safe](<https://example.com>)"));
        assert!(reply.paste_text.ends_with("local"));
        assert!(!reply.warnings.is_empty());
    }
    #[test]
    fn non_body_fields_receive_plain_dom_text_without_markdown() {
        let source = "<h2>研究</h2><p><b>重点</b> 与 <i>观察</i></p>";
        let reply = convert(
            &request("html", "title", source.as_bytes()),
            &mut source.as_bytes(),
        )
        .unwrap();
        assert!(!reply.paste_text.contains("##") && !reply.paste_text.contains('*'));
        assert_eq!(reply.paste_text, "研究\n\n重点 与 观察");
    }
    #[test]
    fn flutter_excel_fixture_uses_namespace_formula_and_sparse_columns() {
        let source = "<Workbook xmlns:ss='urn:schemas-microsoft-com:office:spreadsheet'><Worksheet><Table><Row><Cell><Data>名称</Data></Cell><Cell><Data>结果</Data></Cell></Row><Row><Cell><Data>合计</Data></Cell><Cell ss:Formula='=SUM(R[-1]C:R[-1]C)'><Data>42</Data></Cell></Row></Table></Worksheet></Workbook>";
        let reply = run("xml", source);
        assert_eq!(
            reply.paste_text,
            "| 名称 | 结果 |\n| --- | --- |\n| 合计 | 42 (公式: =SUM(R[-1]C:R[-1]C)) |"
        );
        assert!(!reply.warnings.is_empty());
        let sparse = run(
            "xml",
            "<Workbook xmlns='urn:schemas-microsoft-com:office:spreadsheet' xmlns:ss='urn:schemas-microsoft-com:office:spreadsheet'><Worksheet><Table><Row><Cell ss:Index='3'><Data>x&amp;y</Data></Cell></Row></Table></Worksheet></Workbook>",
        );
        assert_eq!(sparse.paste_text, "|  |  | x&y |\n| --- | --- | --- |");
    }
    #[test]
    fn xml_rejects_entities_namespace_confusion_malformed_and_truncated_tables() {
        fail(
            "xml",
            "<!DOCTYPE Workbook [<!ENTITY x SYSTEM 'file:///secret'>]><Workbook><Table><Row><Cell><Data>&x;</Data></Cell></Row></Table></Workbook>",
            "ClipboardXml",
        );
        fail(
            "xml",
            "<x:Workbook xmlns:x='urn:fake'><x:Table/></x:Workbook>",
            "Namespace",
        );
        fail("xml", "<Workbook><Table></Workbook>", "ClipboardXml");
        fail(
            "xml",
            "<Workbook><Table><Row><Cell Index='80'/><Cell/></Row></Table></Workbook>",
            "ColumnLimit",
        );
        fail(
            "xml",
            "<Workbook><Table><Row><Cell Index='2'/><Cell Index='2'/></Row></Table></Workbook>",
            "ColumnLimit",
        );
        fail(
            "xml",
            &format!("<Workbook>{}</Workbook>", "<Table/>".repeat(9)),
            "TableLimit",
        );
    }
    #[test]
    fn flutter_rtf_fixture_and_utf16_surrogate_pair_match_shared_converter() {
        let reply = run(
            "rtf",
            r"{\rtf1\ansi\uc1 {\fonttbl secret}\u20013?\u25991?\par next{\*\objdata payload}}",
        );
        assert_eq!(reply.paste_text, "中文\nnext");
        assert_eq!(run("rtf", r"{\rtf1\uc1\u-10179?\u-8704?}").paste_text, "😀");
        fail("rtf", r"{\rtf1\u-10179?}", "Unicode");
    }
    #[test]
    fn rtf_ansi_hex_bytes_are_decoded_by_declared_codepage_without_changing_raw_hash() {
        let source = br"{\rtf1\ansi\ansicpg1252 caf\'e9 \u20013? \u8364?}";
        let reply = convert(
            &request("rtf", "description", source),
            &mut source.as_slice(),
        )
        .unwrap();
        assert_eq!(reply.paste_text, "café 中 €");
        assert_eq!(reply.source_sha256, hex(&Sha256::digest(source)));
        assert_eq!(
            run("rtf", r"{\rtf1\ansicpg936 \'d6\'d0\'ce\'c4}").paste_text,
            "中文"
        );
        assert_eq!(
            run("rtf", r"{\rtf1\ansicpg1252 \'80 \'5c \'7b \'7d}").paste_text,
            "€ \\ { }"
        );
    }
    #[test]
    fn rtf_raw_ansi_and_font_codepages_are_not_assumed_utf8() {
        let mut source = br"{\rtf1\ansicpg1252 caf".to_vec();
        source.extend([0xe9, b'}']);
        assert_eq!(
            convert(
                &request("rtf", "description", &source),
                &mut source.as_slice()
            )
            .unwrap()
            .paste_text,
            "café"
        );
        assert_eq!(
            run(
                "rtf",
                r"{\rtf1\ansicpg1252{\fonttbl{\f0\fcharset134 Arial;}}\f0 \'d6\'d0}"
            )
            .paste_text,
            "中"
        );
    }
    #[test]
    fn rtf_invalid_groups_binary_codepage_or_hex_are_explicit_errors() {
        for source in [r"{\rtf1 missing", r"{\rtf1 ok}}", r"{\rtf1 ok}trailing"] {
            fail("rtf", source, "Syntax");
        }
        fail("rtf", r"{\rtf1\bin4 abcd}", "BinaryUnsupported");
        fail("rtf", r"{\rtf1\ansicpg99999 x}", "CodePageUnsupported");
        fail("rtf", r"{\rtf1\'x0}", "Hex");
        fail("rtf", r"{\rtf1\uc99 x}", "Unicode");
    }
    #[test]
    fn data_images_have_complete_metadata_and_exact_hash_bound_fd_extraction() {
        let source = image_html(2);
        let reply = run("html", &source);
        assert_eq!(reply.images.len(), 2);
        for (i, metadata) in reply.images.iter().enumerate() {
            assert_eq!(metadata.local_id, format!("image-{}", i + 1));
            assert!(
                reply
                    .paste_text
                    .contains(&format!("attachment:{}", metadata.name))
            );
            assert_eq!(metadata.sha256, hex(&Sha256::digest(PNG)));
            let mut value: serde_json::Value =
                serde_json::from_str(&request("html", "description", source.as_bytes())).unwrap();
            value["local_id"] = serde_json::json!(metadata.local_id);
            let mut written = Vec::new();
            let file = extract(
                &value.to_string(),
                &mut source.as_bytes(),
                &mut written,
                PNG.len() as u64,
            )
            .unwrap();
            assert_eq!(written, PNG);
            assert_eq!(file.sha256, metadata.sha256);
            assert_eq!(file.byte_length, metadata.byte_length);
        }
        assert!(!serde_json::to_string(&reply).unwrap().contains("iVBOR"));
    }
    #[test]
    fn image_percent_encoding_and_blocked_subtrees_do_not_read_external_sources() {
        let reply = run(
            "html",
            "<img alt='图' src='data:image/png,%89PNG%0d%0a%1a%0afixture'><object><img src='data:image/png,broken'></object>",
        );
        assert_eq!(reply.images.len(), 1);
        assert_eq!(reply.images[0].sha256, hex(&Sha256::digest(PNG)));
    }
    #[test]
    fn image_overlimit_malformed_mime_or_magic_fail_whole_conversion() {
        fail("html", &image_html(11), "ImageCountLimit");
        fail(
            "html",
            "<img src='data:image/png;base64,AQID'>",
            "Signature",
        );
        fail(
            "html",
            "<img src='data:image/svg+xml;base64,AQID'>",
            "Format",
        );
        fail("html", "<img src='data:image/png;base64,!!!'>", "Encoding");
        fail("html", "<img src='data:image/png,%zz'>", "Encoding");
    }
    #[test]
    fn extract_refuses_changed_source_unknown_id_and_limit_before_writing() {
        let source = image_html(1);
        let mut value: serde_json::Value =
            serde_json::from_str(&request("html", "description", source.as_bytes())).unwrap();
        value["local_id"] = serde_json::json!("image-1");
        let mut written = Vec::new();
        assert!(
            extract(
                &value.to_string(),
                &mut b"changed".as_slice(),
                &mut written,
                100
            )
            .unwrap_err()
            .contains("HashMismatch")
        );
        assert!(written.is_empty());
        assert!(
            extract(
                &value.to_string(),
                &mut source.as_bytes(),
                &mut written,
                PNG.len() as u64 - 1
            )
            .unwrap_err()
            .contains("ByteLimit")
        );
        assert!(written.is_empty());
        value["local_id"] = serde_json::json!("image-2");
        assert!(
            extract(
                &value.to_string(),
                &mut source.as_bytes(),
                &mut written,
                100
            )
            .unwrap_err()
            .contains("NotFound")
        );
        assert!(written.is_empty());
    }
    #[test]
    fn source_sha_utf8_sections_schema_and_actual_read_limits_fail_closed() {
        let source = b"a";
        let mut value: serde_json::Value =
            serde_json::from_str(&request("plain", "description", source)).unwrap();
        value["expected_sha256"] = serde_json::json!("0".repeat(64));
        assert!(
            convert(&value.to_string(), &mut source.as_slice())
                .unwrap_err()
                .contains("HashMismatch")
        );
        value["expected_sha256"] = serde_json::json!("A".repeat(64));
        assert!(
            convert(&value.to_string(), &mut source.as_slice())
                .unwrap_err()
                .contains("HashRequired")
        );
        fail("plain", "abc\0", "Nul");
        let invalid = b"\xff";
        assert!(
            convert(
                &request("html", "description", invalid),
                &mut invalid.as_slice()
            )
            .unwrap_err()
            .contains("Utf8")
        );
        assert!(
            convert(&request("plain", "unknown", source), &mut source.as_slice())
                .unwrap_err()
                .contains("Section")
        );
        let large = vec![b'x'; MAX_SOURCE_BYTES + 1];
        assert!(
            convert(
                &request("plain", "description", &large),
                &mut large.as_slice()
            )
            .unwrap_err()
            .contains("SourceByteLimit")
        );
        value = serde_json::from_str(&request("plain", "description", source)).unwrap();
        value["ticket"] = serde_json::json!("forged");
        assert!(
            convert(&value.to_string(), &mut source.as_slice())
                .unwrap_err()
                .contains("Request")
        );
    }
    #[test]
    fn text_bom_encoding_is_strict_and_keeps_original_byte_hash() {
        for little in [true, false] {
            let mut bytes = if little {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for unit in "<h2>中文😀</h2>".encode_utf16() {
                bytes.extend(if little {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            let reply = convert(
                &request("html", "description", &bytes),
                &mut bytes.as_slice(),
            )
            .unwrap();
            assert_eq!(reply.paste_text, "## 中文😀");
            assert_eq!(reply.source_sha256, hex(&Sha256::digest(&bytes)));
        }
        let bad = [0xff, 0xfe, 0x00, 0xd8];
        assert!(
            convert(&request("plain", "description", &bad), &mut bad.as_slice())
                .unwrap_err()
                .contains("Utf16")
        );
        let odd = [0xff, 0xfe, 0x61];
        assert!(
            convert(&request("plain", "description", &odd), &mut odd.as_slice())
                .unwrap_err()
                .contains("Utf16")
        );
    }
    #[test]
    fn structural_output_and_table_budgets_never_silently_crop_content() {
        fail(
            "html",
            &format!("{}x{}", "<div>".repeat(41), "</div>".repeat(41)),
            "DepthLimit",
        );
        fail("html", &"<br>".repeat(1100), "NodeLimit");
        fail(
            "html",
            &format!("<p>{}</p>", "😀".repeat(10001)),
            "OutputLimit",
        );
        fail(
            "html",
            "<table><tr><td colspan='81'>x</td></tr></table>",
            "SpanLimit",
        );
        fail(
            "html",
            "<table><tr><td colspan='80'>x</td><td>y</td></tr></table>",
            "ColumnLimit",
        );
        fail("plain", &"a\tb\n".repeat(501), "PasteRows");
        fail(
            "plain",
            &format!("{}\n", vec!["x"; 81].join("\t")).repeat(2),
            "PasteColumns",
        );
    }
    #[test]
    fn interrupted_reads_short_writes_and_failed_writer_preserve_failure_semantics() {
        struct ShortRead {
            first: bool,
            source: Vec<u8>,
        }
        impl Read for ShortRead {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                if self.first {
                    self.first = false;
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                let count = self.source.len().min(out.len()).min(3);
                out[..count].copy_from_slice(&self.source[..count]);
                self.source.drain(..count);
                Ok(count)
            }
        }
        let source = image_html(1);
        let reply = convert(
            &request("html", "description", source.as_bytes()),
            &mut ShortRead {
                first: true,
                source: source.as_bytes().to_vec(),
            },
        )
        .unwrap();
        let mut value: serde_json::Value =
            serde_json::from_str(&request("html", "description", source.as_bytes())).unwrap();
        value["local_id"] = serde_json::json!(reply.images[0].local_id);
        struct ShortWrite(Vec<u8>);
        impl Write for ShortWrite {
            fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
                let n = input.len().min(2);
                self.0.extend(&input[..n]);
                Ok(n)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut writer = ShortWrite(vec![]);
        extract(&value.to_string(), &mut source.as_bytes(), &mut writer, 100).unwrap();
        assert_eq!(writer.0, PNG);
        struct Failed;
        impl Write for Failed {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::PermissionDenied.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        assert!(extract(&value.to_string(), &mut source.as_bytes(), &mut Failed, 100).is_err());
    }
    #[test]
    #[ignore = "requires a fresh actual Flutter capture in HMOS_CLIPBOARD_FLUTTER_REFERENCE"]
    fn actual_flutter_reference_outputs_match_five_complete_fixtures() {
        let path =
            std::env::var("HMOS_CLIPBOARD_FLUTTER_REFERENCE").expect("fresh Flutter fixture path");
        let values: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(values.len(), 5);
        for value in values {
            let format = value["format"].as_str().unwrap();
            let source = value["source"].as_str().unwrap();
            assert_eq!(
                run(format, source).paste_text,
                value["paste_text"].as_str().unwrap(),
                "actual Flutter {format}"
            );
        }
    }
}
