use morrow_plugin_sdk::{
    protocol::CodecError,
    ui::{Document, Event, EventKind, Kind, Node},
};
const FORM: &[u8] = include_bytes!("../../tests/ui_fixtures/document.capnp");
const EVENT: &[u8] = include_bytes!("../../tests/ui_fixtures/event.capnp");
#[test]
fn independent_host_form_and_event() {
    let d = Document::decode(FORM).unwrap();
    assert_eq!(d.nodes().len(), 5);
    assert_eq!(d.nodes()[2].text, "灵感🌈");
    assert_eq!(d.encode().unwrap(), FORM);
    let e = Event::decode(EVENT).unwrap();
    assert_eq!(e.generation, u64::MAX);
    assert_eq!((e.revision, e.serial), (1, 1));
    assert_eq!(e.kind, EventKind::EditText);
    assert_eq!(e.text, "从 Dart 编辑🌈");
}
#[test]
fn rejected_tree_and_field_mixtures() {
    let good = Document::decode(FORM).unwrap();
    let mut n = good.nodes().to_vec();
    n[2].parent = "title".into();
    assert!(Document::new(n).is_err());
    let mut n = good.nodes().to_vec();
    n[3].id = "title".into();
    assert!(Document::new(n).is_err());
    let mut n = good.nodes().to_vec();
    n[0].action = "forged".into();
    assert!(Document::new(n).is_err());
    let mut n = good.nodes().to_vec();
    n[2].text = "a\u{0}".into();
    assert!(Document::new(n).is_err());
    let mut n = good.nodes().to_vec();
    n[2].max_bytes = 6;
    n[2].text = "中文".into();
    assert!(Document::new(n.clone()).is_ok());
    n[2].text.push('a');
    assert!(Document::new(n).is_err());
}
#[test]
fn node_depth_and_expanded_text_budgets() {
    let mut n = vec![Node::new("root", "", Kind::Column)];
    for i in 1..8 {
        n.push(Node::new(
            &format!("n{i}"),
            if i == 1 { "root" } else { &n[i - 1].id },
            Kind::Column,
        ));
    }
    assert!(Document::new(n.clone()).is_ok());
    n.push(Node::new("too-deep", "n7", Kind::Column));
    assert_eq!(Document::new(n).unwrap_err(), CodecError::Limit);
    let mut n = vec![Node::new("root", "", Kind::Column)];
    for i in 0..127 {
        n.push(Node::new(&format!("n{i}"), "root", Kind::Text));
    }
    assert!(Document::new(n.clone()).is_ok());
    n.push(Node::new("overflow", "root", Kind::Text));
    assert!(Document::new(n).is_err());
    let mut n = vec![Node::new("root", "", Kind::Column)];
    for i in 0..8 {
        let mut t = Node::new(&format!("n{i}"), "root", Kind::Text);
        t.text = "a".repeat(4096);
        n.push(t);
    }
    assert_eq!(Document::new(n).unwrap_err(), CodecError::Limit);
}
#[test]
fn reject_truncation_trailing_and_contract_corruption() {
    for i in 0..FORM.len() {
        assert!(Document::decode(&FORM[..i]).is_err());
    }
    for i in 0..EVENT.len() {
        assert!(Event::decode(&EVENT[..i]).is_err());
    }
    let mut b = FORM.to_vec();
    b.extend_from_slice(&[0; 8]);
    assert!(Document::decode(&b).is_err());
    let mut b = EVENT.to_vec();
    b.extend_from_slice(&[0; 8]);
    assert!(Event::decode(&b).is_err());
    let r = capnp::serialize::read_message_from_flat_slice(&mut &EVENT[..], Default::default())
        .unwrap();
    let mut m = capnp::message::Builder::new_default();
    m.set_root(
        r.get_root::<morrow_plugin_sdk::ui_capnp::event::Reader>()
            .unwrap(),
    )
    .unwrap();
    m.get_root::<morrow_plugin_sdk::ui_capnp::event::Builder>()
        .unwrap()
        .set_schema_digest(&[0; 32]);
    assert_eq!(
        Event::decode(&capnp::serialize::write_message_to_words(&m)).unwrap_err(),
        CodecError::Contract
    );
}
