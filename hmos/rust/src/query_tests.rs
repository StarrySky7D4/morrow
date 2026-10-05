use super::*;
use serde_json::json;

fn fixture() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::open(&dir.path().join("hmos-development.sqlite")).unwrap();
    (dir, engine)
}
fn request(value: serde_json::Value) -> Request {
    serde_json::from_value(value).unwrap()
}
fn query(engine: &mut Engine, section: &str, filter: &str, text: &str, sort: &str) -> Reply {
    engine
        .execute(request(json!({"action":"query","section":section,"filter":filter,"text":text,"sort":sort})))
        .unwrap()
}
fn seed(engine: &mut Engine, id: &str, title: &str, properties: tasks_v2::Properties) {
    let body = properties.encode_to_vec();
    tasks_v2::decode(id, title, &body).unwrap();
    let card = CardRecord::new(id, "idea", 2, title, body).unwrap();
    // Fixture preparation only; assertions invoke the real Engine query path.
    engine
        .host
        .store_local_mut()
        .create_local(&format!("seed-{id}"), &card)
        .unwrap();
}
fn properties() -> tasks_v2::Properties {
    tasks_v2::Properties {
        version: 2,
        category: "灵感".into(),
        stage: "待整理".into(),
        ..Default::default()
    }
}
fn readpoint(engine: &Engine) -> morrow_core::store::ReadPoint {
    let snapshot = engine.host.store_local().open_card_snapshot().unwrap();
    let point = snapshot.readpoint().clone();
    snapshot.close().unwrap();
    point
}

#[test]
fn query_searches_complete_properties_without_mutation_or_card_projection() {
    let (_dir, mut engine) = fixture();
    let created = engine
        .execute(request(json!({"action":"create","id":"search-card","operation":"create-search","title":"查询记录","description":"普通正文","hypothesis":"ONLY Hypothesis Needle","conclusion":"ONLY Conclusion Needle","category":"灵感","stage":"待整理"})))
        .unwrap();
    assert!(created.ids.is_empty());
    let original = created.cards[0].source.clone();
    let before = readpoint(&engine);
    for text in ["hypothesis needle", "conclusion needle", "普通正文", "查询记录"] {
        let reply = query(&mut engine, "概览", "全部", text, "最近添加");
        assert_eq!(reply.ids, ["search-card"]);
        assert!(reply.cards.is_empty());
        assert!(reply.receipt_revision.is_empty());
        assert_eq!(reply.profile, "development-unsealed");
        assert_eq!(reply.effect, "not_committed");
    }
    assert!(query(&mut engine, "概览", "全部", "missing", "最近添加").ids.is_empty());
    assert_eq!(readpoint(&engine), before);
    assert_eq!(engine.cards().unwrap()[0].source, original);
}

#[test]
fn query_uses_original_asset_metadata_with_materialized_assets() {
    let (_dir, mut engine) = fixture();
    for (id, kind, name) in [
        ("image", "image", "AttachmentOnly Image.PNG"),
        ("gif", "gif", "AttachmentOnly Animation.GIF"),
        ("audio", "audio", "AttachmentOnly Recording.WAV"),
        ("video", "video", "AttachmentOnly Movie.MP4"),
        ("file", "file", "AttachmentOnly Document.PDF"),
    ] {
        let mut p = properties();
        p.favorite = true;
        p.assets.push(Default::default());
        let asset = p.assets.last_mut().unwrap();
        asset.id = format!("asset-{id}");
        asset.kind = kind.into();
        asset.name = name.into();
        asset.bytes = 17;
        // Fixture bytes are bound to the metadata, now also exposed in UI.
        // This still qualifies query semantics, not provider/URI import.
        let bytes = [0_u8; 17];
        let blob = engine.host.store_local_mut().stage_blob(&mut bytes.as_slice(), 17, None, 1).unwrap();
        let outer = morrow_core::content::Attachment { id: format!("asset-{id}"), display_name: name.into(), media_type: "application/octet-stream".into(), byte_length: 17, sha256: blob.sha256 };
        let card = CardRecord::new_with_attachments(id, "idea", 2, "记录", p.encode_to_vec(), &[outer]).unwrap();
        engine.host.store_local_mut().create_local(&format!("seed-{id}"), &card).unwrap();
    }
    let mut plain = properties();
    plain.favorite = true;
    seed(&mut engine, "plain", "纯文字记录", plain);
    let before = readpoint(&engine);
    let projected = engine.cards().unwrap();
    for card in &projected {
        assert!(!card.title.contains("AttachmentOnly"));
        assert!(!card.description.contains("AttachmentOnly"));
        assert_eq!(card.assets.len(), usize::from(card.id != "plain"));
        let original = CardRecord::decode(&unhex(&card.source).unwrap()).unwrap();
        let p = tasks_v2::decode(&card.id, &card.title, &original.body()).unwrap();
        assert_eq!(p.assets.len(), usize::from(card.id != "plain"));
    }
    assert_eq!(query(&mut engine, "概览", "全部", "attachmentonly", "最近添加").ids, ["video", "image", "gif", "file", "audio"]);
    assert_eq!(query(&mut engine, "概览", "全部", "animation.gif", "最近添加").ids, ["gif"]);
    for (filter, expected) in [
        ("图像", vec!["image", "gif"]),
        ("音视频", vec!["video", "audio"]),
        ("文件", vec!["file"]),
        ("文字", vec!["plain"]),
        ("含附件", vec!["video", "image", "gif", "file", "audio"]),
    ] {
        let reply = query(&mut engine, "已收藏", filter, "", "最近添加");
        assert_eq!(reply.ids, expected, "{filter}");
        assert!(reply.cards.is_empty());
        assert_eq!(reply.effect, "not_committed");
    }
    assert_eq!(readpoint(&engine), before);
}

#[test]
fn query_sections_stage_tasks_favorites_and_deleted_follow_original_v2_semantics() {
    let (_dir, mut engine) = fixture();
    let mut project = properties();
    project.category = "进行中".into();
    project.stage = "计划中".into();
    project.tasks = vec![tasks_v2::Task {
        id: "todo-id".into(),
        text: "待完成".into(),
        completion: 0,
        ..Default::default()
    }];
    seed(&mut engine, "project", "小项目记录", project);
    let mut idea = properties();
    idea.favorite = true;
    seed(&mut engine, "idea", "收藏灵感", idea);
    let mut lab = properties();
    lab.category = "实验".into();
    lab.stage = "验证中".into();
    seed(&mut engine, "lab", "实验记录", lab);
    assert_eq!(query(&mut engine, "小项目", "计划中", "", "最近添加").ids, ["project"]);
    assert_eq!(query(&mut engine, "概览", "有待办", "", "最近添加").ids, ["project"]);
    assert_eq!(query(&mut engine, "实验室", "验证中", "", "最近添加").ids, ["lab"]);
    assert_eq!(query(&mut engine, "已收藏", "文字", "", "最近添加").ids, ["idea"]);
    assert_eq!(query(&mut engine, "灵感收件箱", "仅收藏", "", "最近添加").ids, ["idea"]);
    assert!(query(&mut engine, "概览", "含附件", "", "最近添加").ids.is_empty());
    let card = engine.cards().unwrap().into_iter().find(|c| c.id == "project").unwrap();
    engine.execute(request(json!({"action":"task_complete_all","operation":"complete","id":"project","source":card.source,"stage":"已完成"}))).unwrap();
    assert!(query(&mut engine, "概览", "有待办", "", "最近添加").ids.is_empty());
    assert_eq!(query(&mut engine, "小项目", "已完成", "", "最近添加").ids, ["project"]);
    let card = engine.cards().unwrap().into_iter().find(|c| c.id == "project").unwrap();
    engine.execute(request(json!({"action":"delete","operation":"delete-project","id":"project","source":card.source,"now_ms":"1000"}))).unwrap();
    assert!(query(&mut engine, "小项目", "已完成", "", "最近添加").ids.is_empty());
    assert_eq!(query(&mut engine, "概览", "全部", "", "最近添加").ids, ["lab", "idea"]);
    assert_eq!(engine.cards().unwrap().len(), 3); // Discovery still includes recycle-bin records.
}

#[test]
fn query_utf16_sort_and_equal_titles_preserve_reverse_id_protocol_order() {
    let (_dir, mut engine) = fixture();
    for (id, title, favorite) in [
        ("a", "same", true),
        ("b", "same", false),
        ("c", "\u{10000}", false),
        ("d", "\u{e000}", true),
        ("e", "😀", false),
        ("f", "中文", false),
    ] {
        let mut p = properties();
        p.favorite = favorite;
        seed(&mut engine, id, title, p);
    }
    assert_eq!(query(&mut engine, "概览", "全部", "", "最近添加").ids, ["f", "e", "d", "c", "b", "a"]);
    assert_eq!(query(&mut engine, "概览", "全部", "", "标题排序").ids, ["b", "a", "f", "c", "e", "d"]);
    assert_eq!(query(&mut engine, "概览", "全部", "", "收藏优先").ids, ["d", "a", "f", "e", "c", "b"]);
}

#[test]
fn query_more_than_one_run_filters_and_stably_merges_all_candidates() {
    let (_dir, mut engine) = fixture();
    let mut expected = Vec::new();
    for index in 0..150 {
        let id = format!("card-{index:03}");
        let title = if index % 2 == 0 { "A" } else { "B" };
        let mut p = properties();
        p.favorite = index % 3 == 0;
        p.description = if index % 7 == 0 { "needle" } else { "body" }.into();
        seed(&mut engine, &id, title, p);
        expected.push((id, title, index % 3 == 0, index % 7 == 0));
    }
    expected.reverse();
    let reverse: Vec<_> = expected.iter().map(|v| v.0.clone()).collect();
    assert_eq!(query(&mut engine, "概览", "全部", "", "最近添加").ids, reverse);
    let mut by_title = expected.clone();
    by_title.sort_by_key(|v| v.1);
    assert_eq!(query(&mut engine, "概览", "全部", "", "标题排序").ids, by_title.into_iter().map(|v| v.0).collect::<Vec<_>>());
    let mut by_favorite = expected.clone();
    by_favorite.sort_by_key(|v| !v.2);
    assert_eq!(query(&mut engine, "概览", "全部", "", "收藏优先").ids, by_favorite.into_iter().map(|v| v.0).collect::<Vec<_>>());
    assert_eq!(query(&mut engine, "概览", "全部", "NEEDLE", "最近添加").ids, expected.into_iter().filter(|v| v.3).map(|v| v.0).collect::<Vec<_>>());
    engine.host.store_local().integrity_check().unwrap();
}

#[test]
fn query_byte_batches_and_single_card_frame_failure_do_not_fall_back_or_write() {
    let (_dir, mut engine) = fixture();
    for index in 0..6 {
        let mut p = properties();
        p.description = "x".repeat(20_000);
        seed(&mut engine, &format!("batch-{index}"), "同名", p);
    }
    assert_eq!(query(&mut engine, "概览", "全部", "", "标题排序").ids, ["batch-5", "batch-4", "batch-3", "batch-2", "batch-1", "batch-0"]);
    let (_large_dir, mut large) = fixture();
    let title = "t".repeat(1_000);
    let created = large.execute(request(json!({"action":"create","id":"large","operation":"create-large","title":title,"description":"x".repeat(65_000),"category":"灵感","stage":"待整理"}))).unwrap();
    let before = readpoint(&large);
    assert_eq!(large.execute(request(json!({"action":"query","section":"概览","filter":"全部","text":"","sort":"最近添加"}))).unwrap_err(), "card exceeds query request budget");
    assert_eq!(large.effect, "not_committed");
    assert_eq!(readpoint(&large), before);
    assert_eq!(large.cards().unwrap()[0].source, created.cards[0].source);
}

#[test]
fn query_empty_invalid_and_capacity_rejection_preserve_development_boundary() {
    let (_dir, mut engine) = fixture();
    assert!(query(&mut engine, "概览", "全部", "", "最近添加").ids.is_empty());
    for invalid in [
        json!({"action":"query","section":"unknown","filter":"全部","sort":"最近添加"}),
        json!({"action":"query","section":"概览","filter":"全部","sort":"时间排序"}),
        json!({"action":"query","section":"概览","filter":"全部","sort":"最近添加","text":"x".repeat(16_385)}),
    ] {
        assert!(engine.execute(request(invalid)).is_err());
        assert_eq!(engine.effect, "not_committed");
    }
    for index in 0..256 {
        seed(&mut engine, &format!("card-{index:03}"), "记录", properties());
    }
    assert_eq!(query(&mut engine, "概览", "全部", "", "最近添加").ids.len(), 256);
    assert_eq!(engine.execute(request(json!({"action":"create","id":"card-256","operation":"new-over-capacity","title":"拒绝","category":"灵感","stage":"待整理"}))).unwrap_err(), "DevelopmentCardLimit");
    assert_eq!(engine.effect, "not_committed");
    // An externally overfilled development fixture also fails closed on query.
    seed(&mut engine, "card-256", "记录", properties());
    assert_eq!(engine.execute(request(json!({"action":"query","section":"概览","filter":"全部","sort":"最近添加"}))).unwrap_err(), "DevelopmentCardLimit");
    assert!(Engine::open(Path::new("production.sqlite")).is_err());
}
