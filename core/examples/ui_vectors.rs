//! Independent Rust document producer and Dart event consumer; synthetic view, no Store.
use morrow_core::ui::{Document, Event, EventKind, Kind, Node, Session, Tone};
fn document() -> Document {
    let root = Node::new("root", "", Kind::Column);
    let mut title = Node::new("heading", "root", Kind::Text);
    title.text = "插件表单".into();
    title.tone = Tone::Emphasis;
    let mut input = Node::new("title", "root", Kind::TextInput);
    input.label = "标题".into();
    input.text = "灵感🌈".into();
    input.action = "title.edit".into();
    input.max_bytes = 32;
    let mut toggle = Node::new("pinned", "root", Kind::Toggle);
    toggle.label = "置顶".into();
    toggle.action = "pin.toggle".into();
    let mut button = Node::new("apply", "root", Kind::Button);
    button.label = "应用".into();
    button.action = "apply".into();
    Document::new(vec![root, title, input, toggle, button]).unwrap()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: ui_vectors generate|check directory".into());
    }
    let path = std::path::Path::new(&args[1]);
    match args[0].as_str() {
        "generate" => {
            std::fs::create_dir_all(path)?;
            std::fs::write(path.join("document.capnp"), document().encode()?)?;
            let e = Event {
                view: "view".into(),
                generation: u64::MAX,
                revision: 1,
                serial: 1,
                node: "title".into(),
                action: "title.edit".into(),
                kind: EventKind::EditText,
                text: "从 Dart 编辑🌈".into(),
                checked: false,
            };
            std::fs::write(path.join("expected-event.capnp"), e.encode()?)?;
            println!("PASS: generated independent Rust UI document and full-width event");
        }
        "check" | "check-web" => {
            let mut session = Session::new("view", u64::MAX)?;
            session.replace(0, document())?;
            let actual = std::fs::read(path.join(if args[0] == "check-web" {
                "browser-event.capnp"
            } else {
                "dart-event.capnp"
            }))?;
            let e = session.accept(&actual)?;
            assert_eq!(
                e,
                Event::decode(&std::fs::read(path.join("expected-event.capnp"))?)?
            );
            assert!(session.accept(&actual).is_err());
            session.close();
            assert!(session.accept(&actual).is_err());
            println!(
                "PASS: Rust accepted actual Dart event; full UInt64 generation, UTF-8 text, replay and closed-view checks"
            );
        }
        _ => return Err("unknown mode".into()),
    }
    Ok(())
}
