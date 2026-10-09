use super::*;
use serde_json::{Value, json};

fn engine(dir: &tempfile::TempDir) -> Engine { Engine::open(&dir.path().join("hmos-development.sqlite")).unwrap() }
fn call(e: &mut Engine, command: Command) -> Result<Value> {
    let req: Request = serde_json::from_value(json!({"action":"music","music":command})).unwrap();
    e.execute(req).and_then(|r| serde_json::to_value(r).map_err(err))
}
fn read(e: &mut Engine) -> Value { call(e, Command::Read { after:String::new(), limit:16 }).unwrap() }
fn request(id: &str, op: &str, revision: u64, data: &[u8]) -> ImportRequest {
    ImportRequest { schema_version:1, track_id:id.into(), operation_id:op.into(), expected_revision:revision.to_string(),
        name:format!("{id}.mp3"), byte_length:data.len().to_string(), sha256:hex(&Sha256::digest(data)) }
}
fn import(e: &mut Engine, r: &ImportRequest, mut data: &[u8]) -> Value {
    call(e, Command::ImportBegin { request:r.clone() }).unwrap();
    let req: Request = serde_json::from_value(json!({"action":"music_import","music":{"action":"import_file","request":r}})).unwrap();
    serde_json::to_value(e.import_from(req, &mut data).unwrap()).unwrap()
}
fn revision(v: &Value) -> String { v["music"]["library_revision"].as_str().unwrap().into() }

#[test]
fn durable_pending_ready_reopen_and_exact_verified_export() {
    let dir = tempfile::tempdir().unwrap(); let mut e = engine(&dir);
    assert_eq!(read(&mut e)["music"]["library_revision"], "0");
    let bytes = b"ID3\0actual synthetic audio fixture\xff\0";
    let r = request("track-one", "import-one", 0, bytes);
    let pending = call(&mut e, Command::ImportBegin { request:r.clone() }).unwrap();
    assert_eq!(pending["effect"], "committed"); assert_eq!(pending["music"]["tracks"][0]["phase"], "pending");
    assert_eq!(pending["music"]["tracks"][0]["bytes_retained"], false);
    drop(e); let mut e = engine(&dir);
    let ready = import(&mut e, &r, bytes);
    assert_eq!(ready["effect"], "committed"); assert_eq!(ready["music"]["tracks"][0]["phase"], "ready");
    assert_eq!(ready["music"]["library_revision"], "2");
    drop(e); let mut e = engine(&dir);
    let loaded = read(&mut e); assert_eq!(loaded["music"]["order"], json!(["track-one"]));
    let export = Command::Export { library_revision:revision(&loaded), track_id:r.track_id.clone(), import_operation:r.operation_id.clone(), byte_length:r.byte_length.clone(), sha256:r.sha256.clone() };
    let mut output = vec![]; let metadata = export_to(&e, &export, &mut output).unwrap();
    assert_eq!(output, bytes); assert_eq!(metadata.sha256, r.sha256);
    let literal = loaded["music"]["tracks"][0]["request_json"].as_str().unwrap();
    let restored: Request = serde_json::from_str(literal).unwrap();
    struct NoRead;
    impl Read for NoRead { fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> { panic!("exact owned retry must not read") } }
    let retry = e.import_from(restored, &mut NoRead).unwrap();
    assert_eq!(retry.music.unwrap().operation_revision, "2");
    assert!(e.execute(Request { action:"list".into(), ..Default::default() }).unwrap().cards.is_empty());
    assert!(e.execute(Request { action:"query".into(), section:"概览".into(), filter:"全部".into(), sort:"最近添加".into(), ..Default::default() }).unwrap().ids.is_empty());
    for action in ["edit", "favorite", "delete", "create", "task_add"] {
        assert!(e.execute(Request { action:action.into(), id:ID.into(), operation:"illegal-access".into(), ..Default::default() }).is_err());
    }
}

#[test]
fn rejected_wrong_hash_short_and_trailing_source_preserve_original_pending() {
    let dir = tempfile::tempdir().unwrap(); let mut e = engine(&dir); let data = b"ID3 audio";
    let r = request("hash-track", "hash-import", 0, data);
    call(&mut e, Command::ImportBegin { request:r.clone() }).unwrap();
    for mut wrong in [b"not audio".as_slice(), b"ID3".as_slice(), b"ID3 audio trailing".as_slice()] {
        assert!(import_from(&mut e, &r, &mut wrong).is_err());
        let view = call(&mut e, Command::ImportInspect { request:r.clone() }).unwrap();
        assert_eq!(view["music"]["library_revision"], "1");
        assert_eq!(view["music"]["tracks"][0]["phase"], "pending");
        assert_eq!(view["music"]["tracks"][0]["bytes_retained"], false);
    }
    let unchanged = call(&mut e, Command::Reconcile { request:r.clone() }).unwrap();
    assert_eq!(unchanged["effect"], "not_committed");
    let mut changed = r.clone(); changed.name = "other.mp3".into();
    assert!(call(&mut e, Command::ImportInspect { request:changed }).is_err());
    let ready = import(&mut e, &r, data); assert_eq!(ready["music"]["tracks"][0]["phase"], "ready");
    let bad = Command::Export { library_revision:"2".into(), track_id:r.track_id.clone(), import_operation:r.operation_id.clone(), byte_length:r.byte_length.clone(), sha256:"00".repeat(32) };
    assert!(export_to(&e, &bad, &mut vec![]).is_err());
}

#[test]
fn retained_source_reconcile_after_reopen_never_reactivates_retired_track() {
    let dir = tempfile::tempdir().unwrap(); let mut e = engine(&dir); let data = b"ID3 retained";
    let r = request("retained-track", "retain-import", 0, data);
    call(&mut e, Command::ImportBegin { request:r.clone() }).unwrap();
    e.host.store_local_mut().stage_blob_retained(&mut data.as_slice(), data.len() as u64, hash(&r.sha256).unwrap(), &internal("owner", &r), crate::unix_millis().unwrap()).unwrap();
    drop(e); let mut e = engine(&dir);
    let pending = call(&mut e, Command::ImportInspect { request:r.clone() }).unwrap();
    assert_eq!(pending["music"]["tracks"][0]["phase"], "pending"); assert_eq!(pending["music"]["tracks"][0]["bytes_retained"], true);
    let ready = call(&mut e, Command::Reconcile { request:r.clone() }).unwrap();
    let removed = call(&mut e, Command::Remove { library_revision:revision(&ready), operation_id:"remove-retained".into(), track_id:r.track_id.clone() }).unwrap();
    assert_eq!(removed["music"]["tracks"][0]["phase"], "retired");
    let repeated = import(&mut e, &r, b"replacement source");
    assert_eq!(repeated["music"]["repeated"], true); assert_eq!(repeated["music"]["operation_revision"], "2");
    assert_eq!(repeated["music"]["library_revision"], "3"); assert_eq!(repeated["music"]["tracks"][0]["phase"], "retired");
    assert_eq!(read(&mut e)["music"]["order"], json!([]));
}

#[test]
fn lyrics_complete_budget_locator_and_playlist_reorder_keep_selected_identity() {
    let dir = tempfile::tempdir().unwrap(); let mut e = engine(&dir);
    let one = request("one", "import-a", 0, b"ID3a"); import(&mut e, &one, b"ID3a");
    let two = request("two", "import-b", 2, b"ID3b"); import(&mut e, &two, b"ID3b");
    let selected = call(&mut e, Command::Select { library_revision:"4".into(), operation_id:"select-two".into(), track_id:"two".into() }).unwrap();
    let reordered = call(&mut e, Command::Reorder { library_revision:revision(&selected), operation_id:"reorder-two".into(), order:vec!["two".into(),"one".into()] }).unwrap();
    assert_eq!(reordered["music"]["selected_track_id"], "two");
    let text = "[offset:500]\n[00:01.20][00:03.4]完整中文歌词\n[00:02.50]第二行\n";
    let set = Command::SetLyrics { library_revision:revision(&reordered), operation_id:"lyrics-two".into(), track_id:"two".into(), lyrics:text.into(), lyric_source:"manual LRC".into() };
    let stored = call(&mut e, set.clone()).unwrap();
    let lyrics = call(&mut e, Command::LyricsRead { library_revision:revision(&stored), track_id:"two".into(), position_ms:"2000".into() }).unwrap();
    assert_eq!(lyrics["music"]["lyrics"]["text"], text); assert_eq!(lyrics["music"]["lyrics"]["active_index"], 1);
    assert_eq!(lyrics["music"]["lyrics"]["lines"][0]["time_ms"], "700");
    assert_eq!(call(&mut e, set).unwrap()["music"]["repeated"], true);
    let revision = revision(&stored);
    assert!(call(&mut e, Command::SetLyrics { library_revision:revision.clone(), operation_id:"oversize-lyrics".into(), track_id:"two".into(), lyrics:"中".repeat(16385), lyric_source:"manual".into() }).is_err());
    assert_eq!(read(&mut e)["music"]["library_revision"], revision);
    let expansion = "[00:01.00]".repeat(512) + &"x".repeat(1024);
    assert!(call(&mut e, Command::SetLyrics { library_revision:revision.clone(), operation_id:"expanded-lyrics".into(), track_id:"two".into(), lyrics:expansion, lyric_source:"manual".into() }).is_err());
    assert_eq!(read(&mut e)["music"]["library_revision"], revision);
    assert!(call(&mut e, Command::Remove { library_revision:"6".into(), operation_id:"stale-remove".into(), track_id:"two".into() }).is_err());
    assert!(call(&mut e, Command::Read { after:String::new(), limit:17 }).is_err());
}

#[test]
fn pure_policy_match_and_library_page_bind_current_snapshot() {
    let dir = tempfile::tempdir().unwrap(); let mut e = engine(&dir);
    let a = request("a", "a-op", 0, b"ID3a"); import(&mut e, &a, b"ID3a");
    let b = request("b", "b-op", 2, b"ID3b"); import(&mut e, &b, b"ID3b");
    let first = call(&mut e, Command::Read { after:String::new(), limit:1 }).unwrap();
    assert_eq!(first["music"]["next_after"], "a");
    let last = call(&mut e, Command::Read { after:"a".into(), limit:1 }).unwrap();
    assert_eq!(last["music"]["tracks"][0]["track_id"], "b"); assert_eq!(last["music"]["next_after"], "");
    assert_eq!(last["music"]["order"], first["music"]["order"]);
    let policy = |action:&str, value:&str, flag:bool, blocked:bool| Command::Policy {
        library_revision:"4".into(), index:"0".into(), playing:true, blocked, position_ms:"50".into(), duration_ms:"100".into(),
        music_action:action.into(), value:value.into(), flag };
    let seek = call(&mut e, policy("seek", "999", false, false)).unwrap();
    assert_eq!(seek["music"]["playback"]["position_ms"], "100"); assert_eq!(seek["effect"], "not_committed");
    let next = call(&mut e, policy("next", "0", true, false)).unwrap(); assert_eq!(next["music"]["playback"]["track_id"], "b");
    let restored = call(&mut e, policy("restore", "0", false, false)).unwrap();
    assert_eq!(restored["music"]["playback"]["playing"], false); assert_eq!(restored["music"]["playback"]["transport_effect"], "stop");
    assert_eq!(call(&mut e, policy("toggle", "0", false, true)).unwrap()["music"]["playback"]["playing"], false);
    assert!(call(&mut e, policy("seek", "-1", false, false)).is_err());
    let matching = call(&mut e, Command::Match { title:"Song".into(), artist:String::new(), duration:10., candidates:vec![
        MatchCandidate { title:"Song".into(), artist:"A".into(), duration:10., synced:true, has_lyrics:true },
        MatchCandidate { title:"Song".into(), artist:"B".into(), duration:10., synced:true, has_lyrics:true }] }).unwrap();
    assert_eq!(matching["music"]["match_index"], -1);
    let mixed: Request = serde_json::from_value(json!({"action":"music","id":"business","music":{"action":"read","after":"","limit":16}})).unwrap();
    assert!(e.execute(mixed).is_err());
    let stale_export = Command::Export { library_revision:"3".into(), track_id:a.track_id, import_operation:a.operation_id, byte_length:a.byte_length, sha256:a.sha256 };
    assert!(export_to(&e, &stale_export, &mut Vec::new()).is_err());
}

#[test]
fn pending_count_and_total_quotas_reject_before_retaining_bytes() {
    let dir = tempfile::tempdir().unwrap(); let mut e = engine(&dir);
    let mut r = request("big-a", "big-a-op", 0, b"a"); r.byte_length = MAX_AUDIO_BYTES.to_string();
    for i in 0..3 {
        r.track_id = format!("big-{i}"); r.operation_id = format!("big-op-{i}"); r.expected_revision = i.to_string();
        call(&mut e, Command::ImportBegin { request:r.clone() }).unwrap();
    }
    r.track_id = "quota-overflow".into(); r.operation_id = "quota-overflow-op".into(); r.expected_revision = "3".into();
    assert!(call(&mut e, Command::ImportBegin { request:r.clone() }).is_err());
    assert_eq!(read(&mut e)["music"]["library_revision"], "3");
    r.byte_length = (MAX_AUDIO_BYTES + 1).to_string(); assert!(validate_import(&r).is_err());
    r.byte_length = "1".into(); r.name = "encrypted.ncm".into(); assert!(validate_import(&r).is_err());
    let mut l = defaults().unwrap(); l.generation = 600; l.last_operation="valid-mutation".into(); l.last_request_json="x".into();
    for n in 0..512 {
        let r = request(&format!("id-{n}"), &format!("op-{n}"), n, b"a");
        l.entries.push(Entry { request_json:import_literal(&r).unwrap(), request:r, phase:"pending".into(), blob_id:String::new() });
    }
    validate(&l).unwrap();
    let r = request("id-extra", "op-extra", 599, b"a");
    l.entries.push(Entry { request_json:import_literal(&r).unwrap(), request:r, phase:"pending".into(), blob_id:String::new() });
    assert!(validate(&l).is_err());
}

#[test]
fn unknown_preferences_and_retained_track_fields_survive_real_store_edits() {
    use prost::{Message, encoding::{encode_key, encode_varint, WireType}};
    let dir = tempfile::tempdir().unwrap(); let mut e = engine(&dir);
    let r = request("opaque-track", "opaque-import", 0, b"ID3opaque"); import(&mut e, &r, b"ID3opaque");
    let mut l = current(&e).unwrap(); let (_, mut p) = prefs(&l).unwrap();
    let tracks = std::mem::take(&mut p.tracks); let mut raw = p.encode_to_vec();
    for t in tracks {
        let mut track = t.encode_to_vec();
        let marker = b"future-track-field-retained";
        encode_key(50011, WireType::LengthDelimited, &mut track); encode_varint(marker.len() as u64, &mut track); track.extend(marker);
        encode_key(4, WireType::LengthDelimited, &mut raw); encode_varint(track.len() as u64, &mut raw); raw.extend(track);
    }
    let marker = b"future-library-preference-retained";
    encode_key(50012, WireType::LengthDelimited, &mut raw); encode_varint(marker.len() as u64, &mut raw); raw.extend(marker);
    l.preferences = STANDARD.encode(raw); l.generation = 3; l.last_operation = "fixture-opaque-seed".into(); l.last_request_json="test-only future preference seed".into();
    write(&mut e, &l, 2).unwrap();
    call(&mut e, Command::SetLyrics { library_revision:"3".into(), operation_id:"opaque-lyrics".into(), track_id:r.track_id.clone(), lyrics:"plain完整歌词".into(), lyric_source:"manual".into() }).unwrap();
    drop(e); let e = engine(&dir); let (saved, p) = prefs(&current(&e).unwrap()).unwrap();
    for marker in [b"future-track-field-retained".as_slice(), b"future-library-preference-retained".as_slice()] {
        assert!(saved.windows(marker.len()).any(|s| s == marker));
    }
    assert_eq!(p.tracks[0].lyrics, "plain完整歌词"); assert_eq!(p.appearance.unwrap().theme, "white");
}

#[test]
fn actual_file_reader_export_short_writer_and_owner_damage_fail_closed() {
    use std::io::Seek;
    let dir = tempfile::tempdir().unwrap(); let mut e = engine(&dir); let data = b"ID3file\0binary\xff";
    let input_path = dir.path().join("actual-music-spool"); std::fs::write(&input_path, data).unwrap();
    let r = request("file-track", "file-import", 0, data);
    call(&mut e, Command::ImportBegin { request:r.clone() }).unwrap();
    let mut file = std::fs::File::open(&input_path).unwrap();
    let req: Request = serde_json::from_str(&import_literal(&r).unwrap()).unwrap();
    let ready = e.import_from(req, &mut file).unwrap(); assert_eq!(ready.music.unwrap().library_revision, "2");
    file.rewind().unwrap();
    struct Short(Vec<u8>); impl Write for Short {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> { let n=data.len().min(3); self.0.extend_from_slice(&data[..n]); Ok(n) }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }
    let export = Command::Export { library_revision:"2".into(), track_id:r.track_id.clone(), import_operation:r.operation_id.clone(), byte_length:r.byte_length.clone(), sha256:r.sha256.clone() };
    let mut output = Short(vec![]); export_to(&e, &export, &mut output).unwrap(); assert_eq!(output.0, data);
    struct Failed; impl Write for Failed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> { Err(std::io::ErrorKind::PermissionDenied.into()) }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }
    assert!(export_to(&e, &export, &mut Failed).is_err());
    let l = current(&e).unwrap(); let entry=&l.entries[0]; let blob=retained(&e, entry).unwrap().unwrap();
    e.host.store_local_mut().release_retention_local(&blob.id, &internal("owner", &r), morrow_core::attachment::RetentionKind::Snapshot, crate::unix_millis().unwrap()).unwrap();
    assert!(export_to(&e, &export, &mut vec![]).is_err());
    assert!(call(&mut e, Command::TrackRead { library_revision:"2".into(), track_id:r.track_id }).is_err());
}

#[test]
#[ignore = "subprocess only; parent owns crash setup"]
fn music_crash_child() {
    let directory = std::env::var_os("MORROW_HMOS_MUSIC_CRASH_DIR").expect("child directory");
    let mut e = Engine::open(&std::path::PathBuf::from(directory).join("hmos-development.sqlite")).unwrap();
    let data = b"ID3 crash fixture"; let r = request("crash-track", "crash-import", 0, data);
    let req = serde_json::from_str(&import_literal(&r).unwrap()).unwrap();
    e.import_from(req, &mut data.as_slice()).unwrap();
    panic!("fault boundary was not enabled");
}

#[test]
#[ignore = "explicit morrow-core/fault-injection subprocess matrix"]
fn actual_process_crash_reopen_reconcile_uses_original_pending_and_owner() {
    for (point, generation, owner_present) in [
        ("stage-after-allocation", 1, false), ("stage-after-payload", 1, false),
        ("stage-after-chunk", 1, false), ("stage-retained-before-commit", 1, false),
        ("stage-before-commit", 1, false), ("stage-after-commit", 1, true),
        ("after-begin", 1, true), ("after-card", 1, true), ("after-operation", 1, true),
        ("after-blob-references", 1, true), ("after-event", 1, true), ("before-commit", 1, true), ("after-commit", 2, true),
    ] {
        let dir=tempfile::tempdir().unwrap(); let data=b"ID3 crash fixture"; let r=request("crash-track", "crash-import", 0, data);
        let mut e=engine(&dir); call(&mut e, Command::ImportBegin { request:r.clone() }).unwrap(); drop(e);
        let status=std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "music_bridge::tests::music_crash_child", "--ignored", "--nocapture"])
            .env("MORROW_HMOS_MUSIC_CRASH_DIR", dir.path()).env("MORROW_TEST_CRASH_AT", point).status().unwrap();
        assert_eq!(status.code(), Some(86), "{point}");
        let mut e=engine(&dir); let observed=call(&mut e, Command::ImportInspect { request:r.clone() }).unwrap();
        assert_eq!(observed["music"]["library_revision"], generation.to_string(), "{point}");
        assert_eq!(observed["music"]["tracks"][0]["bytes_retained"], owner_present, "{point}");
        let recovered=call(&mut e, Command::Reconcile { request:r.clone() }).unwrap();
        if owner_present { assert_eq!(recovered["music"]["tracks"][0]["phase"], "ready", "{point}"); }
        else { assert_eq!(recovered["music"]["tracks"][0]["phase"], "pending", "{point}"); assert_eq!(recovered["effect"], "not_committed"); }
        eprintln!("music crash boundary={point} exit=86 revision={generation} owner={owner_present} reconcile_phase={}", recovered["music"]["tracks"][0]["phase"]);
    }
}

#[test]
#[ignore = "explicit actual Store DTO export requires output path"]
fn actual_store_fixture_export() {
    let path = std::env::var_os("MORROW_HMOS_MUSIC_FIXTURE").expect("explicit fixture output path");
    let dir = tempfile::tempdir().unwrap(); let mut e = engine(&dir);
    let empty = read(&mut e); let data = b"ID3 v29 public synthetic fixture\0\xff";
    let request = request("fixture-track", "fixture-import", 0, data);
    let pending = call(&mut e, Command::ImportBegin { request:request.clone() }).unwrap();
    let ready = import(&mut e, &request, data);
    let set_command = Command::SetLyrics { library_revision:"2".into(), operation_id:"fixture-lyrics".into(), track_id:request.track_id.clone(), lyrics:"[00:01.00]第一行\n[00:02.50]第二行\n".into(), lyric_source:"fixture local LRC".into() };
    let set = call(&mut e, set_command).unwrap();
    let loaded = read(&mut e);
    let lyrics = call(&mut e, Command::LyricsRead { library_revision:"3".into(), track_id:request.track_id.clone(), position_ms:"1600".into() }).unwrap();
    let policy = call(&mut e, Command::Policy { library_revision:"3".into(), index:"0".into(), playing:false, blocked:false, position_ms:"0".into(), duration_ms:"3000".into(), music_action:"toggle".into(), value:"0".into(), flag:false }).unwrap();
    let second = self::request("fixture-second", "fixture-import-second", 3, data);
    let ready_second = import(&mut e, &second, data);
    let selected = call(&mut e, Command::Select { library_revision:revision(&ready_second), operation_id:"fixture-select-second".into(), track_id:second.track_id.clone() }).unwrap();
    let reorder = call(&mut e, Command::Reorder { library_revision:revision(&selected), operation_id:"fixture-reorder".into(), order:vec![second.track_id.clone(),request.track_id.clone()] }).unwrap();
    let untimed_set = call(&mut e, Command::SetLyrics { library_revision:revision(&reorder), operation_id:"fixture-untimed".into(), track_id:second.track_id.clone(), lyrics:"完整无时间轴歌词\n第二行\n".into(), lyric_source:"fixture local text".into() }).unwrap();
    let untimed = call(&mut e, Command::LyricsRead { library_revision:revision(&untimed_set), track_id:second.track_id.clone(), position_ms:"1600".into() }).unwrap();
    let retained_request = self::request("fixture-retained", "fixture-retain-import", revision(&untimed_set).parse().unwrap(), data);
    let retained_begin = call(&mut e, Command::ImportBegin { request:retained_request.clone() }).unwrap();
    e.host.store_local_mut().stage_blob_retained(&mut data.as_slice(), data.len() as u64, hash(&retained_request.sha256).unwrap(), &internal("owner", &retained_request), crate::unix_millis().unwrap()).unwrap();
    drop(e); let mut e = engine(&dir);
    let retained_pending = call(&mut e, Command::ImportInspect { request:retained_request.clone() }).unwrap();
    let reconciled = call(&mut e, Command::Reconcile { request:retained_request.clone() }).unwrap();
    let removed = call(&mut e, Command::Remove { library_revision:revision(&reconciled), operation_id:"fixture-remove-retained".into(), track_id:retained_request.track_id.clone() }).unwrap();
    let mut next_revision: u64 = revision(&removed).parse().unwrap();
    for index in 0..15 {
        let r = self::request(&format!("page-{index:02}"), &format!("fixture-page-{index:02}"), next_revision, data);
        let pending = call(&mut e, Command::ImportBegin { request:r }).unwrap(); next_revision = revision(&pending).parse().unwrap();
    }
    let first_page = read(&mut e);
    let last_page = call(&mut e, Command::Read { after:first_page["music"]["next_after"].as_str().unwrap().into(), limit:16 }).unwrap();
    assert_eq!(first_page["music"]["tracks"].as_array().unwrap().len(),16);
    assert_eq!(last_page["music"]["tracks"].as_array().unwrap().len(),2);
    let current_track = call(&mut e, Command::TrackRead { library_revision:next_revision.to_string(), track_id:second.track_id.clone() }).unwrap();
    let make_policy = |action: &str, value: &str| Command::Policy { library_revision:next_revision.to_string(), index:"0".into(), playing:true, blocked:false, position_ms:"1000".into(), duration_ms:"3000".into(), music_action:action.into(), value:value.into(), flag:false };
    let seek = call(&mut e, make_policy("seek","5000")).unwrap();
    let next = call(&mut e, make_policy("next","0")).unwrap();
    let restore = call(&mut e, make_policy("restore","0")).unwrap();
    let error_message = call(&mut e, Command::Select { library_revision:"0".into(), operation_id:"fixture-stale-selection".into(), track_id:second.track_id.clone() }).unwrap_err();
    let mut stale_error = Reply::failure(error_message); stale_error.effect=e.effect;
    let error_message = call(&mut e, Command::ImportInspect { request:ImportRequest { name:"changed.mp3".into(), ..request.clone() } }).unwrap_err();
    let mut changed_error = Reply::failure(error_message); changed_error.effect=e.effect;
    let export_command = Command::Export { library_revision:next_revision.to_string(), track_id:second.track_id.clone(), import_operation:second.operation_id.clone(), byte_length:second.byte_length.clone(), sha256:second.sha256.clone() };
    let mut exported = vec![]; let metadata=export_to(&e, &export_command, &mut exported).unwrap(); assert_eq!(exported,data);
    let export_reply=crate::file_stream::FileReply::from_result(Ok(metadata));
    let fixture = json!({"schema_version":1,"scope":"Actual isolated development Store synthetic bytes; not decoded/audible/device/protected proof", "request":request,"empty_reply":empty,"pending_reply":pending,"ready_reply":ready,"set_lyrics_reply":set,"read_reply":loaded,"lyrics_reply":lyrics,"policy_reply":policy,
        "second_request":second,"second_ready_reply":ready_second,"selected_reply":selected,"reorder_reply":reorder,"untimed_set_reply":untimed_set,"untimed_lyrics_reply":untimed,
        "retained_request":retained_request,"retained_begin_reply":retained_begin,"retained_pending_reply":retained_pending,"reconcile_reply":reconciled,"remove_reply":removed,
        "read_pages":[first_page,last_page],"current_track_reply":current_track,"seek_reply":seek,"next_reply":next,"restore_reply":restore,"stale_revision_error_reply":stale_error,"changed_request_error_reply":changed_error,"export_request":export_command,"export_reply":export_reply});
    std::fs::write(path, serde_json::to_vec_pretty(&fixture).unwrap()).unwrap();
}
