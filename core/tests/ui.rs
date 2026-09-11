use morrow_core::ui::{Document, Event, EventKind, Kind, Node, Session, Tone};
fn nodes() -> Vec<Node> {
    let root = Node::new("root", "", Kind::Column);
    let mut n = Node::new("input", "root", Kind::TextInput);
    n.label = "Title".into();
    n.action = "edit".into();
    n.max_bytes = 8;
    vec![root, n]
}
fn event() -> Event {
    Event {
        view: "view".into(),
        generation: u64::MAX,
        revision: 1,
        serial: 1,
        node: "input".into(),
        action: "edit".into(),
        kind: EventKind::EditText,
        text: "文字".into(),
        checked: false,
    }
}
#[test]
fn document_bounds_and_tree_shape_are_enforced() {
    let doc = Document::new(nodes()).unwrap();
    assert_eq!(Document::decode(&doc.encode().unwrap()).unwrap(), doc);
    for case in 0..10 {
        let mut n = nodes();
        match case {
            0 => n.clear(),
            1 => n[1].id = "root".into(),
            2 => n[1].parent = "missing".into(),
            3 => n[1].parent = "input".into(),
            4 => n[0].kind = Kind::Row,
            5 => n[1].max_bytes = 4097,
            6 => n[1].text = "x".repeat(9),
            7 => n[1].tone = Tone::Emphasis,
            8 => n[1].action.clear(),
            _ => n[1].checked = true,
        }
        assert!(Document::new(n).is_err(), "case {case}");
    }
    let mut n = nodes();
    let leaf = Node::new("leaf", "input", Kind::Text);
    n.push(leaf);
    assert!(Document::new(n).is_err());
    let mut n = vec![Node::new("root", "", Kind::Column)];
    for i in 1..=8 {
        let parent = if i == 1 {
            "root".to_owned()
        } else {
            format!("n{}", i - 1)
        };
        n.push(Node::new(&format!("n{i}"), &parent, Kind::Column));
        if i == 7 {
            assert!(Document::new(n.clone()).is_ok());
        }
    }
    assert!(Document::new(n).is_err());
    let mut n = vec![Node::new("root", "", Kind::Column)];
    for i in 1..128 {
        n.push(Node::new(&format!("n{i}"), "root", Kind::Text));
    }
    assert!(Document::new(n.clone()).is_ok());
    n.push(Node::new("overflow", "root", Kind::Text));
    assert!(Document::new(n).is_err());
    let mut n = nodes();
    n[1].max_bytes = 4096;
    n[1].text = "x".repeat(4096);
    for i in 0..20 {
        let mut t = Node::new(&format!("big{i}"), "root", Kind::Text);
        t.text = "x".repeat(4096);
        n.push(t);
    }
    assert!(Document::new(n).is_err());
}
#[test]
fn events_bind_view_generation_revision_serial_action_and_field_type() {
    let mut session = Session::new("view", u64::MAX).unwrap();
    assert!(session.accept(&event().encode().unwrap()).is_err());
    session.replace(0, Document::new(nodes()).unwrap()).unwrap();
    for case in 0..8 {
        let mut e = event();
        match case {
            0 => e.view = "other".into(),
            1 => e.generation -= 1,
            2 => e.revision += 1,
            3 => e.serial += 1,
            4 => e.node = "root".into(),
            5 => e.action = "other".into(),
            6 => {
                e.kind = EventKind::Activate;
                e.text.clear();
            }
            _ => e.text = "超长输入".into(),
        }
        assert!(session.accept(&e.encode().unwrap()).is_err());
    }
    let e = event().encode().unwrap();
    assert_eq!(session.accept(&e).unwrap(), event());
    assert!(session.accept(&e).is_err());
    assert!(session.replace(0, Document::new(nodes()).unwrap()).is_err());
    assert_eq!(session.revision(), 1);
    let mut n = nodes();
    n[1].enabled = false;
    session.replace(1, Document::new(n).unwrap()).unwrap();
    let mut e = event();
    e.revision = 2;
    e.serial = 2;
    assert!(session.accept(&e.encode().unwrap()).is_err());
    session.close();
    assert!(session.replace(2, Document::new(nodes()).unwrap()).is_err());
    assert!(session.accept(&e.encode().unwrap()).is_err());
}
#[test]
fn malformed_frames_and_payload_mixing_reject() {
    let bytes = Document::new(nodes()).unwrap().encode().unwrap();
    for i in 0..bytes.len() {
        assert!(Document::decode(&bytes[..i]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(Document::decode(&trailing).is_err());
    let mut corrupt = bytes;
    let digest = morrow_core::ui::schema_digest();
    let at = corrupt.windows(32).position(|b| b == digest).unwrap();
    corrupt[at] ^= 1;
    assert!(Document::decode(&corrupt).is_err());
    let mut e = event();
    e.checked = true;
    assert!(e.encode().is_err());
    e.checked = false;
    e.serial = 0;
    assert!(e.encode().is_err());
}

#[test]
fn expanded_text_budget_rejects_even_when_wire_frame_fits() {
    use morrow_core::ui_capnp as wire;
    let mut m = capnp::message::Builder::new_default();
    let mut r = m.init_root::<wire::document::Builder>();
    r.set_version(morrow_core::ui::VERSION);
    r.set_schema_digest(&morrow_core::ui::schema_digest());
    let mut nodes = r.init_nodes(10);
    {
        let mut root = nodes.reborrow().get(0);
        root.set_id("root");
        root.set_kind(Kind::Column);
    }
    for i in 1..10 {
        let mut n = nodes.reborrow().get(i);
        n.set_id(format!("n{i}"));
        n.set_parent("root");
        n.set_kind(Kind::Text);
        n.set_text("x".repeat(4096));
    }
    let bytes = capnp::serialize::write_message_to_words(&m);
    assert!(bytes.len() < 65536);
    assert!(Document::decode(&bytes).is_err());
}
