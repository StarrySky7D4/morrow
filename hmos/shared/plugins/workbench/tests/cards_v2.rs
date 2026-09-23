use morrow_workbench_plugin::{
    Asset, Idea,
    cards_v2::{self as cards, Command, Fields},
    cards_v2_codec as codec, persistence,
    tasks_v2::{self as tasks, Baseline},
};
fn source() -> Vec<u8> {
    let legacy = persistence::encode(
        &Idea {
            id: "card".into(),
            title: "Source".into(),
            category: "进行中".into(),
            stage: "计划中".into(),
            todos: vec!["one".into()],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    tasks::migrate(
        &Baseline::capture("card", 1, &legacy).unwrap(),
        "card",
        1,
        "Source",
        &legacy,
    )
    .unwrap()
}
fn field(tag: u8, value: &[u8]) -> Vec<u8> {
    let mut result = vec![(tag << 3) | 2, value.len() as u8];
    result.extend_from_slice(value);
    result
}
#[test]
fn common_edits_preserve_task_provenance_unknowns_and_retirement() {
    let mut p = source();
    p.extend([0xb8, 0x0c, 7]); // unknown top-level varint
    let mut asset = field(1, b"asset");
    asset.extend(field(2, b"old"));
    asset.extend(field(3, b"file"));
    asset.extend([0x20, 2]); // bytes=2
    asset.extend([0xa0, 0x06, 9]); // unknown nested varint
    p.extend(field(10, &asset));
    let before = tasks::decode("card", "Source", &p).unwrap();
    let edited = cards::apply(
        "card",
        "Source",
        &p,
        &Command::Edit(Fields {
            title: "Renamed".into(),
            description: "description".into(),
            hypothesis: "hypothesis".into(),
            conclusion: "conclusion".into(),
            icon: 2,
            color: 0xff00ee,
            assets: vec![Asset {
                id: "asset".into(),
                name: "new".into(),
                kind: "file".into(),
                bytes: 3,
            }],
        }),
    )
    .unwrap();
    assert_eq!(edited.title, "Renamed");
    let after = tasks::decode("card", "Renamed", &edited.properties).unwrap();
    assert_eq!(after.tasks, before.tasks);
    assert_eq!(after.origin, before.origin);
    assert_eq!(after.retired_task_ids, before.retired_task_ids);
    assert_eq!(after.assets[0].name, "new");
    assert!(edited.properties.windows(3).any(|v| v == [0xb8, 0x0c, 7]));
    assert!(edited.properties.windows(3).any(|v| v == [0xa0, 0x06, 9]));
}
#[test]
fn category_favorite_delete_restore_and_window() {
    let p = source();
    let favorite = cards::apply("card", "Source", &p, &Command::SetFavorite(true)).unwrap();
    assert!(
        tasks::decode("card", "Source", &favorite.properties)
            .unwrap()
            .favorite
    );
    let category = cards::apply(
        "card",
        "Source",
        &favorite.properties,
        &Command::SetCategory {
            category: "实验".into(),
            stage: "待验证".into(),
        },
    )
    .unwrap();
    assert_eq!(
        tasks::decode("card", "Source", &category.properties)
            .unwrap()
            .stage,
        "待验证"
    );
    assert!(
        cards::apply(
            "card",
            "Source",
            &p,
            &Command::SetCategory {
                category: "实验".into(),
                stage: "推进中".into()
            }
        )
        .is_err()
    );
    assert!(cards::apply("card", "Source", &p, &Command::Delete { now_ms: 0 }).is_err());
    let deleted = cards::apply(
        "card",
        "Source",
        &category.properties,
        &Command::Delete { now_ms: 100 },
    )
    .unwrap();
    assert!(
        cards::apply(
            "card",
            "Source",
            &deleted.properties,
            &Command::SetFavorite(false)
        )
        .is_err()
    );
    assert!(
        cards::apply(
            "card",
            "Source",
            &deleted.properties,
            &Command::Restore { now_ms: 8100 }
        )
        .is_err()
    );
    assert!(
        cards::apply(
            "card",
            "Source",
            &deleted.properties,
            &Command::Restore { now_ms: 99 }
        )
        .is_err()
    );
    let restored = cards::apply(
        "card",
        "Source",
        &deleted.properties,
        &Command::Restore { now_ms: 8099 },
    )
    .unwrap();
    let view = tasks::decode("card", "Source", &restored.properties).unwrap();
    assert!(!view.deleted);
    assert_eq!(view.deleted_at, 0);
}
#[test]
fn canonical_codec_round_trip_and_rejects_extraneous_parameters() {
    let p = source();
    let cmd = Command::SetFavorite(true);
    let req = codec::encode_request("card", "Source", &p, &cmd).unwrap();
    let decoded = codec::decode_request(&req).unwrap();
    assert_eq!(decoded.id, "card");
    assert_eq!(decoded.command, cmd);
    let response = codec::decode_response(&codec::process(&req).unwrap()).unwrap();
    assert_eq!(response, cards::apply("card", "Source", &p, &cmd).unwrap());
    let mut trailing = req.clone();
    trailing.extend([0, 0, 0, 0]);
    assert!(codec::decode_request(&trailing).is_err());
    assert!(codec::decode_request(&vec![0; 65537]).is_err());
}
#[test]
fn canonical_decoder_rejects_unrelated_action_data() {
    use capnp::{message::Builder, serialize};
    use morrow_workbench_plugin::cards_v2_capnp as wire;
    let p = source();
    let mut message = Builder::new_default();
    {
        let mut r = message.init_root::<wire::request::Builder>();
        r.set_version(2);
        r.set_digest(&codec::digest());
        r.set_action(wire::Action::SetFavorite);
        r.set_card_id("card");
        r.set_title("Source");
        r.set_properties(&p);
        r.set_favorite(true);
        r.set_category("实验");
    }
    let bytes = serialize::write_message_to_words(&message);
    assert!(codec::decode_request(&bytes).is_err());
}
#[test]
fn codec_covers_all_common_commands() {
    let p = source();
    for command in [
        Command::Edit(Fields {
            title: "Changed".into(),
            description: "D".into(),
            hypothesis: "H".into(),
            conclusion: "C".into(),
            icon: 1,
            color: 42,
            assets: vec![],
        }),
        Command::SetFavorite(true),
        Command::SetCategory {
            category: "实验".into(),
            stage: "待验证".into(),
        },
        Command::Delete { now_ms: 100 },
    ] {
        let bytes = codec::encode_request("card", "Source", &p, &command).unwrap();
        assert_eq!(codec::decode_request(&bytes).unwrap().command, command);
        assert_eq!(
            codec::decode_response(&codec::process(&bytes).unwrap()).unwrap(),
            cards::apply("card", "Source", &p, &command).unwrap()
        );
    }
    let deleted = cards::apply("card", "Source", &p, &Command::Delete { now_ms: 100 }).unwrap();
    let restore = Command::Restore { now_ms: 101 };
    let bytes = codec::encode_request("card", "Source", &deleted.properties, &restore).unwrap();
    assert_eq!(codec::decode_request(&bytes).unwrap().command, restore);
    assert_eq!(
        codec::decode_response(&codec::process(&bytes).unwrap()).unwrap(),
        cards::apply("card", "Source", &deleted.properties, &restore).unwrap()
    );
}
