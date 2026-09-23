use morrow_workbench_plugin::{Action, Asset, Idea, Request, codec, execute, persistence};
fn idea(id: &str) -> Idea {
    Idea {
        id: id.into(),
        title: format!("记录 {id}"),
        description: "**原始 Markdown**\n\n| A | B |".into(),
        category: "灵感".into(),
        stage: "待整理".into(),
        ..Default::default()
    }
}
fn req(action: Action, current: Idea) -> Request {
    Request {
        action,
        current,
        proposed: Idea::default(),
        text: String::new(),
        flag: false,
        now_ms: 1,
        ideas: vec![],
        section: "概览".into(),
        filter: "全部".into(),
        sort: "最近添加".into(),
    }
}
#[test]
fn sections_search_sort_and_attachment_only_parity() {
    let mut a = idea("a");
    a.favorite = true;
    a.todos = vec!["未完成".into()];
    let mut b = idea("b");
    b.category = "进行中".into();
    b.stage = "计划中".into();
    b.hypothesis = "唯一假设".into();
    let mut c = idea("c");
    c.title.clear();
    c.assets = vec![Asset {
        id: "asset".into(),
        name: "特殊表格.xlsx".into(),
        kind: "file".into(),
        bytes: 200 * 1024 * 1024,
    }];
    let mut d = idea("d");
    d.category = "实验".into();
    d.stage = "待验证".into();
    d.conclusion = "唯一结论".into();
    for (section, filter, text, expected) in [
        ("概览", "全部", "", vec!["a", "b", "c", "d"]),
        ("灵感收件箱", "全部", "", vec!["a", "c"]),
        ("小项目", "计划中", "", vec!["b"]),
        ("实验室", "全部", "结论", vec!["d"]),
        ("已收藏", "有待办", "", vec!["a"]),
        ("概览", "全部", "假设", vec!["b"]),
        ("概览", "文件", "表格", vec!["c"]),
    ] {
        let mut r = req(Action::Query, Idea::default());
        r.ideas = vec![a.clone(), b.clone(), c.clone(), d.clone()];
        r.section = section.into();
        r.filter = filter.into();
        r.text = text.into();
        assert_eq!(execute(r).unwrap().ids, expected);
    }
    let mut r = req(Action::Query, Idea::default());
    r.ideas = vec![d, c, b, a];
    r.sort = "收藏优先".into();
    assert_eq!(execute(r).unwrap().ids, vec!["a", "d", "c", "b"]);
}
#[test]
fn editing_category_checklist_and_delete_deadline() {
    let mut a = idea("a");
    a.favorite = true;
    a.todos = vec!["保留".into(), "删除".into()];
    a.completed = a.todos.clone();
    let mut r = req(Action::Edit, a.clone());
    r.proposed = a;
    r.proposed.todos = vec!["保留".into(), "新增".into()];
    r.proposed.completed.clear();
    r.proposed.favorite = false;
    r.proposed.category = "实验".into();
    let updated = execute(r).unwrap().idea;
    assert!(updated.favorite);
    assert_eq!(updated.completed, vec!["保留"]);
    assert_eq!(updated.stage, "待验证");
    let mut r = req(Action::Delete, updated);
    r.now_ms = 123;
    let deleted = execute(r).unwrap().idea;
    for (now, ok) in [(122, false), (123, true), (8122, true), (8123, false)] {
        let mut r = req(Action::Restore, deleted.clone());
        r.now_ms = now;
        assert_eq!(execute(r).is_ok(), ok);
    }
    assert!(execute(req(Action::Favorite, deleted)).is_err());
}
#[test]
fn binary_contract_is_unaligned_safe_and_rejects_mutation() {
    let mut r = req(Action::Create, Idea::default());
    r.proposed = idea("a");
    let bytes = codec::encode_request(&r).unwrap();
    let mut unaligned = vec![0];
    unaligned.extend(&bytes);
    assert_eq!(
        execute(codec::decode_request(&unaligned[1..]).unwrap())
            .unwrap()
            .idea,
        r.proposed
    );
    for len in 0..bytes.len() {
        assert!(codec::decode_request(&bytes[..len]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.extend([0; 8]);
    assert!(codec::decode_request(&trailing).is_err());
    let m = capnp::serialize::read_message(std::io::Cursor::new(bytes), Default::default())
        .unwrap()
        .into_typed::<morrow_workbench_plugin::workbench_capnp::request::Owned>();
    let mut builder = capnp::message::Builder::new_default();
    builder.set_root(m.get().unwrap()).unwrap();
    builder
        .get_root::<morrow_workbench_plugin::workbench_capnp::request::Builder>()
        .unwrap()
        .set_version(99);
    assert!(codec::decode_request(&capnp::serialize::write_message_to_words(&builder)).is_err());
}
#[test]
fn persisted_markdown_and_future_fields_survive_edits() {
    use prost::Message;
    use prost_reflect::{DescriptorPool, DynamicMessage, Value};
    let pool = DescriptorPool::decode(
        include_bytes!(concat!(env!("OUT_DIR"), "/properties.descriptor.bin")).as_slice(),
    )
    .unwrap();
    let desc = pool
        .get_message_by_name("morrow.workbench.v1.Properties")
        .unwrap();
    let mut a = idea("a");
    a.assets = vec![Asset {
        id: "attachment".into(),
        name: "原件.docx".into(),
        kind: "file".into(),
        bytes: 123,
    }];
    let mut raw = persistence::encode(&a, None).unwrap();
    raw.extend([0xa0, 0x06, 0x07]);
    let mut dynamic = DynamicMessage::decode(desc.clone(), raw.as_slice()).unwrap();
    let mut assets = dynamic
        .get_field_by_name("assets")
        .unwrap()
        .as_list()
        .unwrap()
        .to_vec();
    let asset = assets[0].as_message().unwrap();
    use prost_reflect::ReflectMessage;
    let mut payload = asset.encode_to_vec();
    payload.extend([0xa0, 0x06, 0x09]);
    assets[0] =
        Value::Message(DynamicMessage::decode(asset.descriptor(), payload.as_slice()).unwrap());
    dynamic.set_field_by_name("assets", Value::List(assets));
    raw = dynamic.encode_to_vec();
    a.favorite = true;
    let saved = persistence::encode(&a, Some(&raw)).unwrap();
    assert_eq!(persistence::decode("a", &a.title, &saved).unwrap(), a);
    let out = DynamicMessage::decode(desc, saved.as_slice()).unwrap();
    assert_eq!(out.unknown_fields().count(), 1);
    assert_eq!(
        out.get_field_by_name("assets").unwrap().as_list().unwrap()[0]
            .as_message()
            .unwrap()
            .unknown_fields()
            .count(),
        1
    );
}
