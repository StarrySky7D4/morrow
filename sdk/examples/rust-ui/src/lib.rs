//! Declarative UI generation from current invocation data; no host calls.
use morrow_plugin_sdk::{
    task::{FailureCode, Invocation},
    ui::{Document, Event, EventKind, Kind, Node, Tone},
    wasm,
};
fn form(title: &str) -> Result<Document, morrow_plugin_sdk::protocol::CodecError> {
    let mut nodes = vec![
        Node::new("root", "", Kind::Column),
        Node::new("heading", "root", Kind::Text),
        Node::new("title", "root", Kind::TextInput),
        Node::new("pinned", "root", Kind::Toggle),
        Node::new("apply", "root", Kind::Button),
    ];
    nodes[1].text = "插件表单".into();
    nodes[1].tone = Tone::Emphasis;
    nodes[2].label = "标题".into();
    nodes[2].text = title.into();
    nodes[2].action = "title.edit".into();
    nodes[2].max_bytes = 32;
    nodes[3].label = "置顶".into();
    nodes[3].action = "pin.toggle".into();
    nodes[4].label = "应用".into();
    nodes[4].action = "apply".into();
    Document::new(nodes)
}
fn fail(task: &Invocation) -> i32 {
    if wasm::complete_failure(task, FailureCode::InvalidInput, "Invalid form input").is_ok() {
        0
    } else {
        -1
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    let Ok(task) = wasm::read_task() else {
        return -1;
    };
    let Some(t) = task.transform() else {
        return -1;
    };
    if t.output_type != "morrow.ui.document.v1" {
        return -1;
    }
    let title = match (t.handler.as_str(), t.input_type.as_str()) {
        ("ui.form", "text.utf8") => match std::str::from_utf8(&t.input) {
            Ok(v) => v.to_owned(),
            Err(_) => return fail(&task),
        },
        ("ui.edit", "morrow.ui.event.v1") => {
            let Ok(e) = Event::decode(&t.input) else {
                return fail(&task);
            };
            if e.kind != EventKind::EditText || e.node != "title" || e.action != "title.edit" {
                return fail(&task);
            }
            e.text
        }
        _ => return -1,
    };
    let Ok(document) = form(&title).and_then(|v| v.encode()) else {
        return fail(&task);
    };
    if wasm::complete_output(&task, &document).is_ok() {
        0
    } else {
        -1
    }
}
