//! A pure online text tool. No storage or host exchange; UI is not a saved card.
use morrow_plugin_sdk::ui::{Document, Event, EventKind, Kind, Node, Tone};
pub fn process(handler: &str, input_type: &str, input: &[u8]) -> Result<Vec<u8>, &'static str> {
    let value = match (handler, input_type) {
        ("ui.form", "text.utf8") if input.len() <= 32 => std::str::from_utf8(input)
            .map_err(|_| "请输入有效文字")?
            .to_owned(),
        ("ui.edit", "morrow.ui.event.v1") => {
            let event = Event::decode(input).map_err(|_| "表单输入无效")?;
            if event.kind != EventKind::EditText
                || event.node != "text"
                || event.action != "text.edit"
            {
                return Err("表单操作不受支持");
            }
            event.text
        }
        _ => return Err("表单输入类型不受支持"),
    };
    if value.len() > 1024 || value.chars().any(|c| c.is_control()) {
        return Err("文字过长或包含控制字符");
    }
    let mut nodes = vec![
        Node::new("root", "", Kind::Column),
        Node::new("heading", "root", Kind::Text),
        Node::new("text", "root", Kind::TextInput),
        Node::new("count", "root", Kind::Text),
        Node::new("preview", "root", Kind::Text),
    ];
    nodes[1].text = "文字小工具".into();
    nodes[1].tone = Tone::Emphasis;
    nodes[2].label = "输入文字".into();
    nodes[2].text = value.clone();
    nodes[2].action = "text.edit".into();
    nodes[2].max_bytes = 1024;
    nodes[3].text = format!(
        "{} 个字符 · 仅本次使用，不保存为卡片",
        value.chars().count()
    );
    nodes[3].tone = Tone::Muted;
    nodes[4].text = if value.is_empty() {
        "输入后查看大写转换".into()
    } else {
        value.to_uppercase()
    };
    Document::new(nodes)
        .and_then(|d| d.encode())
        .map_err(|_| "表单内容无法显示")
}
