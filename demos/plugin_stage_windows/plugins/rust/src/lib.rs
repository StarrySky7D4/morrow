//! Text workshop: all text operations run inside the Rust guest.
use morrow_plugin_sdk::{
    task::{FailureCode, Invocation},
    ui::{Document, Event, EventKind, Kind, Node, Tone},
    wasm,
};
fn text(d: &Document, id: &str) -> String {
    d.nodes()
        .iter()
        .find(|n| n.id == id)
        .map(|n| n.text.clone())
        .unwrap_or_default()
}
fn checked(d: &Document, id: &str) -> bool {
    d.nodes().iter().any(|n| n.id == id && n.checked)
}
fn tidy(s: &str, compact: bool) -> String {
    if compact {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        s.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
fn render(task: &Invocation) -> Result<Vec<u8>, &'static str> {
    let t = task.transform().ok_or("transform")?;
    if t.output_type != "morrow.ui.document.v1" {
        return Err("output");
    }
    let mut source = "  让灵感   有处安放  \n  用文字连接每一个想法  ".to_owned();
    let mut compact = false;
    if t.handler == "demo.update" && t.input_type == "morrow.demo.update.v1" {
        if t.input.len() < 8 {
            return Err("frame");
        }
        let n = u32::from_le_bytes(t.input[..4].try_into().unwrap()) as usize;
        if n > t.input.len() - 8 || t.input[4..8] != [0; 4] {
            return Err("frame");
        }
        let d = Document::decode(&t.input[8..8 + n]).map_err(|_| "document")?;
        let e = Event::decode(&t.input[8 + n..]).map_err(|_| "event")?;
        source = text(&d, "title");
        compact = checked(&d, "option");
        match (e.node.as_str(), e.action.as_str(), e.kind) {
            ("title", "edit", EventKind::EditText) => source = e.text,
            ("option", "compact", EventKind::SetToggle) => compact = e.checked,
            ("apply", "tidy", EventKind::Activate) => source = tidy(&source, compact),
            ("reset", "clear", EventKind::Activate) => source.clear(),
            _ => return Err("action"),
        }
    } else if t.handler != "demo.open" || t.input_type != "text.utf8" || !t.input.is_empty() {
        return Err("handler");
    }
    let result = tidy(&source, compact);
    let chars = source.chars().filter(|c| !c.is_whitespace()).count();
    let lines = source.lines().filter(|l| !l.trim().is_empty()).count();
    let mut nodes = vec![Node::new("root", "", Kind::Column)];
    let mut input = Node::new("title", "root", Kind::TextInput);
    input.label = "写下或粘贴文字".into();
    input.text = source.clone();
    input.max_bytes = 2048;
    input.action = "edit".into();
    nodes.push(input);
    let mut option = Node::new("option", "root", Kind::Toggle);
    option.label = "合并为一段".into();
    option.checked = compact;
    option.action = "compact".into();
    nodes.push(option);
    nodes.push(Node::new("actions", "root", Kind::Row));
    for (id, action, label) in [("apply", "tidy", "整理文字"), ("reset", "clear", "清空")] {
        let mut n = Node::new(id, "actions", Kind::Button);
        n.action = action.into();
        n.label = label.into();
        nodes.push(n);
    }
    for (id, value, tone) in [
        ("result", result, Tone::Normal),
        (
            "detail",
            format!(
                "{} 个非空白字符 · {} 行 · {} UTF-8 字节",
                chars,
                lines,
                source.len()
            ),
            Tone::Muted,
        ),
        (
            "caption",
            "去除行首尾空白、移除空行；也可把所有空白合并为单个空格。字符按 Unicode 标量统计。"
                .into(),
            Tone::Muted,
        ),
    ] {
        let mut n = Node::new(id, "root", Kind::Text);
        n.text = value;
        n.tone = tone;
        nodes.push(n);
    }
    Document::new(nodes)
        .and_then(|d| d.encode())
        .map_err(|_| "bounds")
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    let Ok(task) = wasm::read_task() else {
        return -1;
    };
    let r = match render(&task) {
        Ok(bytes) => wasm::complete_output(&task, &bytes),
        Err(_) => wasm::complete_failure(
            &task,
            FailureCode::InvalidInput,
            "Invalid text workshop input",
        ),
    };
    if r.is_ok() { 0 } else { -1 }
}
