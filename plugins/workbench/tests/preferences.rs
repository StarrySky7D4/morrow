use morrow_workbench_plugin::preferences::{
    self as p, Preferences, Source, Track, proto::Appearance,
};
fn config() -> Preferences {
    Preferences {
        version: 1,
        appearance: Some(Appearance {
            theme: "white".into(),
            glass: "frosted".into(),
            background: "transparent".into(),
            opacity: 0.76,
            corner_radius: 20.,
            window_radius: 20.,
            component_opacity: 0.76,
            component_blur: 22.,
            material_version: 1,
            ..Default::default()
        }),
        ..Default::default()
    }
}
fn track(i: usize) -> Track {
    Track {
        source: Some(Source {
            location: format!("file:///C:/test/{i}.mp3"),
            name: format!("{i}.mp3"),
            kind: "audio".into(),
            local: true,
        }),
        lyrics: "a".repeat(49152),
        ..Default::default()
    }
}
#[test]
fn near_limit_roundtrip_pages_and_aggregate_rejection() {
    let mut v = config();
    v.tracks = (0..80).map(track).collect();
    v.index = 79;
    let bytes = p::encode_wire(&v).unwrap();
    assert!(bytes.len() > 3 * 1024 * 1024);
    assert_eq!(p::decode_wire(&bytes).unwrap(), v);
    let pages = p::validation_pages(&v).unwrap();
    assert_eq!(pages.len(), 81);
    assert!(pages.iter().all(|b| b.len() <= p::GUEST_BYTES));
    for (i, bytes) in pages.iter().skip(1).enumerate() {
        assert_eq!(p::decode_wire(bytes).unwrap().tracks[0], v.tracks[i]);
    }
    let persistent = p::encode_persistent(&v, None).unwrap();
    assert_eq!(p::decode_persistent(&persistent).unwrap(), v);
    v.tracks.extend((80..90).map(track));
    assert!(p::encode_wire(&v).is_err());
    assert!(p::validation_pages(&v).is_err());
    assert!(p::decode_wire(&vec![0; p::MAX_BYTES + 1]).is_err());
}
#[test]
fn single_item_budget_and_cross_page_rules_remain_enforced() {
    let mut v = config();
    let mut oversized = track(0);
    oversized.title = "t".repeat(16384);
    v.tracks.push(oversized);
    assert!(p::encode_wire(&v).is_ok());
    assert!(p::validation_pages(&v).is_err());
    v.tracks = vec![track(0)];
    v.index = 1;
    assert!(p::validation_pages(&v).is_err());
    v.index = 0;
    v.completed = (0..128).map(|_| "c".repeat(2048)).collect();
    assert_eq!(p::validation_pages(&v).unwrap().len(), 10);
    v.completed.push("one-too-many".into());
    assert!(p::validation_pages(&v).is_err());
}

#[test]
fn independent_material_ids_and_legacy_defaults_roundtrip() {
    let mut v = config();
    v.components = vec![
        p::proto::ComponentMaterial {
            id: "card:a".into(),
            enabled: true,
            blur: 0.,
            opacity: 0.,
            color: 0xff33aa55,
            has_color: true,
        },
        p::proto::ComponentMaterial {
            id: "card:b".into(),
            enabled: false,
            blur: 40.,
            opacity: 1.,
            color: 0,
            has_color: false,
        },
    ];
    let bytes = p::encode_wire(&v).unwrap();
    assert_eq!(p::decode_wire(&bytes).unwrap(), v);
    let stored = p::encode_persistent(&v, None).unwrap();
    assert_eq!(p::decode_persistent(&stored).unwrap(), v);
    v.components[0].opacity = 0.3;
    let changed = p::encode_persistent(&v, Some(&stored)).unwrap();
    assert_eq!(
        p::decode_persistent(&changed).unwrap().components[1],
        v.components[1]
    );
    v.components[1].id = "card:a".into();
    assert!(p::encode_wire(&v).is_err());
    v.components[1].id = "card:b".into();
    v.components[1].blur = f64::NAN;
    assert!(p::validation_pages(&v).is_err());
    assert!(
        p::decode_persistent(&p::encode_persistent(&config(), None).unwrap())
            .unwrap()
            .components
            .is_empty()
    );
}
