use morrow_workbench_plugin::{
    Idea, persistence,
    query_v2::{self as query, Candidate, Conditions, Request, Response, SortKey},
    query_v2_codec as codec,
    tasks_v2::{self, Baseline},
};

fn candidate(id: &str, title: &str, format_version: u32, favorite: bool) -> Candidate {
    let idea = Idea {
        id: id.into(),
        title: title.into(),
        category: "进行中".into(),
        stage: "计划中".into(),
        todos: vec!["same".into(), "same".into()],
        completed: vec!["same".into()],
        favorite,
        description: "needle".into(),
        ..Default::default()
    };
    let old = persistence::encode(&idea, None).unwrap();
    let properties = if format_version == 2 {
        tasks_v2::migrate(&Baseline::capture(id, 1, &old).unwrap(), id, 1, title, &old).unwrap()
    } else {
        old
    };
    Candidate {
        id: id.into(),
        title: title.into(),
        format_version,
        properties,
    }
}
fn conditions(filter: &str, sort: &str) -> Conditions {
    Conditions {
        section: "小项目".into(),
        filter: filter.into(),
        text: "needle".into(),
        sort: sort.into(),
    }
}
#[test]
fn v1_frozen_query_and_v2_explicit_stage_with_ambiguous_tasks() {
    let old = candidate("old", "Old", 1, false);
    let new = candidate("new", "New", 2, true);
    let v2 = tasks_v2::decode("new", "New", &new.properties).unwrap();
    assert_eq!(v2.stage, "计划中");
    assert_eq!(
        tasks_v2::project("new", "New", &new.properties)
            .unwrap()
            .ambiguous,
        2
    );
    let pending = query::execute(Request::Filter {
        conditions: conditions("有待办", "最近添加"),
        candidates: vec![old.clone(), new.clone()],
    })
    .unwrap();
    assert_eq!(pending.ids, vec!["new"]);
    let stage = query::execute(Request::Filter {
        conditions: conditions("已完成", "最近添加"),
        candidates: vec![old, new],
    })
    .unwrap();
    assert_eq!(stage.ids, vec!["old"]); // V1 projects completed text; V2 retains explicit stage.
}
#[test]
fn mixed_search_sections_deletion_and_stable_sorts() {
    let old = candidate("old", "Same", 1, false);
    let new = candidate("new", "Same", 2, true);
    let missing = query::execute(Request::Filter {
        conditions: Conditions {
            text: "absent".into(),
            ..conditions("全部", "最近添加")
        },
        candidates: vec![old.clone(), new.clone()],
    })
    .unwrap();
    assert!(missing.ids.is_empty());
    let favorite = query::execute(Request::Filter {
        conditions: conditions("全部", "收藏优先"),
        candidates: vec![old.clone(), new.clone()],
    })
    .unwrap();
    assert_eq!(favorite.ids, vec!["new", "old"]);
    let title = query::execute(Request::Filter {
        conditions: conditions("全部", "标题排序"),
        candidates: vec![old.clone(), new.clone()],
    })
    .unwrap();
    assert_eq!(title.ids, vec!["old", "new"]); // stable equal-title ordering
    let mut deleted = new.clone();
    let output = morrow_workbench_plugin::cards_v2::apply(
        "new",
        "Same",
        &deleted.properties,
        &morrow_workbench_plugin::cards_v2::Command::Delete { now_ms: 1 },
    )
    .unwrap();
    deleted.properties = output.properties;
    assert_eq!(
        query::execute(Request::Filter {
            conditions: conditions("全部", "最近添加"),
            candidates: vec![old.clone(), deleted],
        })
        .unwrap()
        .ids,
        vec!["old"]
    );
    let only_favorites = query::execute(Request::Filter {
        conditions: Conditions {
            section: "已收藏".into(),
            ..conditions("全部", "最近添加")
        },
        candidates: vec![old, new],
    })
    .unwrap();
    assert_eq!(only_favorites.ids, vec!["new"]);
}
#[test]
fn sort_phase_is_pure_keys_and_validates_identity() {
    let keys = vec![
        SortKey {
            id: "b".into(),
            title: "B".into(),
            favorite: false,
        },
        SortKey {
            id: "a".into(),
            title: "A".into(),
            favorite: true,
        },
        SortKey {
            id: "c".into(),
            title: "A".into(),
            favorite: true,
        },
    ];
    assert_eq!(
        query::execute(Request::Sort {
            sort: "最近添加".into(),
            keys: keys.clone(),
        })
        .unwrap()
        .ids,
        vec!["b", "a", "c"]
    );
    assert_eq!(
        query::execute(Request::Sort {
            sort: "标题排序".into(),
            keys: keys.clone(),
        })
        .unwrap()
        .ids,
        vec!["a", "c", "b"]
    );
    assert_eq!(
        query::execute(Request::Sort {
            sort: "收藏优先".into(),
            keys: keys.clone(),
        })
        .unwrap()
        .ids,
        vec!["a", "c", "b"]
    );
    let mut duplicate = keys;
    duplicate[1].id = "b".into();
    assert!(
        query::execute(Request::Sort {
            sort: "最近添加".into(),
            keys: duplicate
        })
        .is_err()
    );
    let bad = Candidate {
        format_version: 3,
        ..candidate("x", "X", 1, false)
    };
    assert!(query::sort_key(&bad).is_err());
    assert!(
        query::execute(Request::Filter {
            conditions: conditions("全部", "最近添加"),
            candidates: vec![candidate("x", "X", 1, false), candidate("x", "Y", 2, false)],
        })
        .is_err()
    );
}
#[test]
fn canonical_wire_round_trip_and_rejects_unrelated_data() {
    let requests = [
        Request::Filter {
            conditions: conditions("全部", "最近添加"),
            candidates: vec![
                candidate("old", "Old", 1, false),
                candidate("new", "New", 2, true),
            ],
        },
        Request::Sort {
            sort: "最近添加".into(),
            keys: vec![SortKey {
                id: "a".into(),
                title: "A".into(),
                favorite: true,
            }],
        },
    ];
    for request in requests {
        let bytes = codec::encode_request(&request).unwrap();
        assert_eq!(codec::decode_request(&bytes).unwrap(), request);
        let actual = codec::decode_response(&codec::process(&bytes).unwrap()).unwrap();
        assert_eq!(actual, query::execute(request).unwrap());
        assert_eq!(
            codec::decode_response(&codec::encode_response(&actual).unwrap()).unwrap(),
            actual
        );
        let mut trailing = bytes;
        trailing.extend([0; 8]);
        assert!(codec::decode_request(&trailing).is_err());
    }
    use capnp::{message::Builder, serialize};
    use morrow_workbench_plugin::query_v2_capnp as wire;
    let mut message = Builder::new_default();
    {
        let mut r = message.init_root::<wire::request::Builder>();
        r.set_version(2);
        r.set_digest(&codec::digest());
        r.set_action(wire::Action::Sort);
        r.set_sort("最近添加");
        r.init_conditions().set_section("概览"); // irrelevant to Sort
    }
    assert!(codec::decode_request(&serialize::write_message_to_words(&message)).is_err());
    assert!(codec::decode_request(&vec![0; 65537]).is_err());
    assert!(
        codec::encode_response(&Response {
            ids: vec!["same".into(), "same".into()]
        })
        .is_err()
    );
}
#[test]
fn aggregate_and_count_budgets_reject_before_guest_execution() {
    let key = SortKey {
        id: "a".into(),
        title: "A".into(),
        favorite: false,
    };
    assert!(
        query::execute(Request::Sort {
            sort: "最近添加".into(),
            keys: vec![key; 129],
        })
        .is_err()
    );
    let mut huge = candidate("big", "Big", 1, false);
    huge.properties.extend(vec![0; 65536]);
    let request = Request::Filter {
        conditions: conditions("全部", "最近添加"),
        candidates: vec![huge],
    };
    assert!(query::execute(request.clone()).is_err());
    assert!(codec::encode_request(&request).is_err());
}
