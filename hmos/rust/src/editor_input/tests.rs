use super::*;

fn value(text: &str) -> TextValue {
    TextValue {
        text: text.into(),
        selection_base: text.encode_utf16().count() as i32,
        selection_extent: text.encode_utf16().count() as i32,
        affinity: 1,
        directional: false,
        composing_start: -1,
        composing_end: -1,
    }
}
fn run(field: &str, old: &TextValue, new: &TextValue) -> Reply {
    request(&serde_json::json!({"field":field,"old_value":old,"new_value":new}).to_string())
}
fn row(limit: usize, old: &TextValue, new: &TextValue) -> Reply {
    request(
        &serde_json::json!({"field":"todos","mode":"todo_row","limit":limit,
        "old_value":old,"new_value":new})
        .to_string(),
    )
}

#[test]
fn enforced_keeps_complete_old_at_exact_limit_but_accepts_deletion_and_selection_replacement() {
    let mut old = value(&"x".repeat(60));
    old.composing_start = 10;
    old.composing_end = 50;
    old.affinity = 0;
    old.directional = true;
    let new = value(&"y".repeat(61));
    let reply = run("title", &old, &new);
    assert!(reply.ok);
    assert_eq!(reply.action, "retained");
    assert_eq!(reply.value, Some(old.clone()));
    assert_eq!(reply.old_value, Some(old.clone()));
    assert_eq!(reply.new_value, Some(new.clone()));
    old.selection_base = 50;
    old.selection_extent = 10;
    let reply = run("title", &old, &new);
    assert!(reply.ok);
    assert_eq!(reply.action, "truncated");
    assert_eq!(reply.value.unwrap().text, "y".repeat(60));
    let short = value("\0\u{fffd} e\u{301} ");
    let reply = run("title", &old, &short);
    assert_eq!(reply.action, "accepted");
    assert_eq!(reply.value, Some(short));
    let over = value(&"x".repeat(61));
    assert_eq!(run("title", &over, &new).action, "truncated");
}

#[test]
fn truncate_uses_unicode16_prefix_and_actual_utf16_selection_and_composition_rules() {
    for cluster in ["😀", "👨‍👩‍👧‍👦", "🇨🇳", "e\u{301}", "\u{915}\u{94d}\u{937}"]
    {
        let mut next = value(&cluster.repeat(61));
        let prefix_units = cluster.encode_utf16().count() as i32 * 60;
        next.selection_base = next.text.encode_utf16().count() as i32;
        next.selection_extent = 1;
        next.affinity = 0;
        next.directional = true;
        next.composing_start = 1;
        next.composing_end = next.selection_base;
        let reply = run("title", &value(""), &next);
        let result = reply.value.unwrap();
        assert_eq!(result.text, cluster.repeat(60));
        assert_eq!(
            (result.selection_base, result.selection_extent),
            (1, prefix_units)
        );
        assert_eq!((result.affinity, result.directional), (0, true));
        assert_eq!(
            (result.composing_start, result.composing_end),
            (1, prefix_units)
        );
        assert_eq!(
            (reply.grapheme_count, reply.utf16_length, reply.utf8_length),
            (60, prefix_units as i64, result.text.len() as i64)
        );
        next.composing_start = prefix_units;
        next.composing_end = prefix_units + 1;
        let result = run("title", &value(""), &next).value.unwrap();
        assert_eq!((result.composing_start, result.composing_end), (-1, -1));
        next.composing_start = -1;
        next.composing_end = 1;
        let result = run("title", &value(""), &next).value.unwrap();
        assert_eq!((result.composing_start, result.composing_end), (-1, 1));
    }
}

#[test]
fn editable_gate_preserves_complete_overlimit_raw_selection_and_noncommit_ime_updates() {
    let mut old = value(&"x".repeat(65));
    old.composing_start = 2;
    old.composing_end = 4;
    let mut next = old.clone();
    next.selection_base = 4;
    next.selection_extent = 2;
    next.affinity = 0;
    next.directional = true;
    next.composing_start = 3;
    next.composing_end = 5;
    let reply = run("title", &old, &next);
    assert!(reply.ok);
    assert!(!reply.format_applied);
    assert_eq!(reply.value, Some(next.clone()));
    next.composing_start = -1;
    next.composing_end = -1;
    let reply = run("title", &old, &next);
    assert!(reply.format_applied);
    assert_eq!(reply.action, "truncated");
    assert_eq!(reply.value.unwrap().text, "x".repeat(60));
    // Collapsed composing already was collapsed: a sentinel change is no commit.
    old.composing_start = 2;
    old.composing_end = 2;
    assert!(!run("title", &old, &next).format_applied);
}

#[test]
fn todo_zero_guard_filter_and_positive_enforcement_have_the_actual_order_and_full_metadata() {
    let old = value("ab");
    let mut next = value("\r\n");
    next.selection_base = -1;
    next.selection_extent = 1;
    next.affinity = 0;
    next.directional = true;
    next.composing_start = 1;
    next.composing_end = 1;
    let reply = row(0, &old, &next);
    assert!(reply.ok);
    let result = reply.value.unwrap();
    assert_eq!(result.text, "  "); // one CRLF grapheme becomes two spaces only AFTER zero guard.
    assert_eq!(
        (
            result.selection_base,
            result.selection_extent,
            result.affinity,
            result.directional
        ),
        (-1, -1, 1, false)
    );
    assert_eq!((result.composing_start, result.composing_end), (-1, -1));
    assert_eq!(row(0, &old, &value("abc")).value, Some(old.clone()));
    assert_eq!(row(0, &old, &value("a")).action, "accepted");
    let mut old_collapsed = old.clone();
    old_collapsed.composing_start = 1;
    old_collapsed.composing_end = 1;
    let reply = row(0, &old_collapsed, &value("abc"));
    assert_eq!(reply.action, "retained");
    assert_eq!(reply.value.unwrap().composing_start, -1);
    let mut old_invalid = old.clone();
    old_invalid.selection_base = -1;
    old_invalid.selection_extent = 1;
    old_invalid.affinity = 0;
    old_invalid.directional = true;
    let result = row(0, &old_invalid, &value("abc")).value.unwrap();
    assert_eq!(
        (
            result.selection_base,
            result.selection_extent,
            result.affinity,
            result.directional
        ),
        (-1, -1, 1, false)
    );
    let reply = row(0, &value("a\r\nb"), &value("abcde"));
    assert_eq!(reply.action, "retained");
    assert_eq!(reply.value.unwrap().text, "a  b");
    let reply = row(1, &value(""), &value("\r\n"));
    assert_eq!(reply.action, "truncated");
    assert_eq!(reply.value.unwrap().text, " ");
    // Filtering finalize resets collapsed composing even without matched newline.
    let mut next = value("xyz");
    next.composing_start = 1;
    next.composing_end = 1;
    assert_eq!(
        row(10, &value(""), &next).value.unwrap().composing_start,
        -1
    );
}

#[test]
fn all_fixed_field_limits_accept_retain_and_truncate_without_business_authority() {
    for (field, limit) in [
        ("title", 60),
        ("todos", 1000),
        ("hypothesis", 5000),
        ("conclusion", 10000),
        ("description", 20000),
    ] {
        let full = value(&"😀".repeat(limit));
        let over = value(&"😀".repeat(limit + 1));
        let accepted = run(field, &value(""), &full);
        assert!(accepted.ok);
        assert_eq!(accepted.value, Some(full.clone()));
        assert_eq!(run(field, &full, &over).action, "retained");
        let truncated = run(field, &value(""), &over);
        assert_eq!(truncated.action, "truncated");
        assert_eq!(truncated.value, Some(full));
    }
}

#[test]
fn strict_schema_unicode_offsets_and_complete_wire_budgets_never_return_a_partial_success() {
    let valid = serde_json::json!({"field":"title","old_value":value(""),"new_value":value("x")});
    for (key, replacement) in [
        ("field", serde_json::json!("unknown")),
        ("mode", serde_json::json!("todo_row")),
        ("limit", serde_json::json!(60)),
        ("extra", serde_json::json!(1)),
    ] {
        let mut altered = valid.clone();
        altered[key] = replacement;
        assert!(!request(&altered.to_string()).ok);
    }
    let malformed = valid
        .to_string()
        .replace("\"text\":\"x\"", r#""text":"\ud800""#);
    assert_eq!(request(&malformed).error, "EditorInputInvalidNewValue");
    for (key, n) in [
        ("affinity", 2),
        ("selection_base", 2),
        ("selection_extent", -2),
        ("composing_start", 2),
    ] {
        let mut altered = valid.clone();
        altered["new_value"][key] = n.into();
        assert!(!request(&altered.to_string()).ok);
    }
    let near = value(&format!("a{}", "\u{301}".repeat(150_000)));
    let reply = run("title", &value(""), &near);
    assert_eq!(reply.error, "EditorInputReplyBytesLimit");
    assert!(reply.old_value.is_none() && reply.new_value.is_none() && reply.value.is_none());
    assert_eq!(
        (reply.grapheme_count, reply.utf16_length, reply.utf8_length),
        (-1, -1, -1)
    );
    assert_eq!(
        request(&" ".repeat(MAX_REQUEST_BYTES + 1)).error,
        "EditorInputRequestBytesLimit"
    );
}

#[test]
fn receipt_hash_binds_exact_request_bytes_and_echoes_complete_non_normalized_values() {
    let old = value(" ");
    let mut next = value(" \u{fffd} e\u{301} ");
    next.selection_base = 1;
    next.selection_extent = 0;
    next.affinity = 0;
    next.directional = true;
    next.composing_start = 1;
    next.composing_end = 2;
    let input = serde_json::json!({"field":"title","old_value":old,"new_value":next}).to_string();
    let reply = request(&input);
    assert!(reply.ok);
    assert_eq!(reply.unicode_version, "16.0.0");
    assert_eq!(
        reply.request_sha256,
        crate::hex(&Sha256::digest(input.as_bytes()))
    );
    assert_eq!(reply.old_value, Some(old));
    assert_eq!(reply.new_value, Some(next.clone()));
    assert_eq!(reply.value, Some(next));
    assert_ne!(reply.request_sha256, request(&(input + " ")).request_sha256);
}

#[test]
fn input_ffi_returns_a_freeable_read_only_full_reply_and_rejects_invalid_c_input() {
    unsafe {
        let input = std::ffi::CString::new(
            serde_json::json!({"field":"title","old_value":value(""),"new_value":value("😀")})
                .to_string(),
        )
        .unwrap();
        let output = crate::morrow_hmos_editor_input(input.as_ptr());
        let reply: Reply =
            serde_json::from_str(std::ffi::CStr::from_ptr(output).to_str().unwrap()).unwrap();
        crate::morrow_hmos_free(output);
        assert!(reply.ok);
        assert_eq!(reply.value.unwrap().text, "😀");
        let output = crate::morrow_hmos_editor_input(std::ptr::null());
        let reply: Reply =
            serde_json::from_str(std::ffi::CStr::from_ptr(output).to_str().unwrap()).unwrap();
        crate::morrow_hmos_free(output);
        assert_eq!(reply.error, "EditorInputNullRequest");
        let bytes = [0xff_u8, 0];
        let output = crate::morrow_hmos_editor_input(bytes.as_ptr().cast());
        let reply: Reply =
            serde_json::from_str(std::ffi::CStr::from_ptr(output).to_str().unwrap()).unwrap();
        crate::morrow_hmos_free(output);
        assert_eq!(reply.error, "EditorInputInvalidUtf8");
    }
}

#[test]
#[ignore = "requires actual full Flutter editing-value capture in HMOS_EDITOR_INPUT_FLUTTER_REFERENCE"]
fn actual_flutter_complete_windows_edit_values_match() {
    let path =
        std::env::var("HMOS_EDITOR_INPUT_FLUTTER_REFERENCE").expect("actual Flutter fixture path");
    let records: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(records.len() >= 200);
    fn source(spec: &serde_json::Value) -> String {
        let cluster = spec["codepoints"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| char::from_u32(c.as_u64().unwrap() as u32).unwrap())
            .collect::<String>();
        let suffix = spec["suffix_codepoints"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| char::from_u32(c.as_u64().unwrap() as u32).unwrap())
            .collect::<String>();
        cluster.repeat(spec["repeats"].as_u64().unwrap() as usize) + &suffix
    }
    fn editing(v: &serde_json::Value) -> TextValue {
        let mut object = v.clone();
        object.as_object_mut().unwrap().remove("text_spec");
        object["text"] = source(&v["text_spec"]).into();
        serde_json::from_value(object).unwrap()
    }
    let mut compared = 0;
    let mut explicit_budget = 0;
    for record in records {
        let old = editing(&record["old_value"]);
        let new = editing(&record["new_value"]);
        for (input, key) in [(&old, "old"), (&new, "new")] {
            assert_eq!(
                crate::hex(&Sha256::digest(input.text.as_bytes())),
                record[format!("{key}_text_sha256")].as_str().unwrap()
            );
        }
        let mut input = serde_json::json!({"field":record["field"],"mode":record["mode"],"old_value":old,"new_value":new});
        if record["mode"] == "todo_row" {
            input["limit"] = record["limit"].clone();
        }
        let input = input.to_string();
        let reply = request(&input);
        assert_eq!(
            reply.request_sha256,
            crate::hex(&Sha256::digest(input.as_bytes()))
        );
        let expected_error = record["expected_native_error"].as_str().unwrap();
        if !expected_error.is_empty() {
            assert!(!reply.ok, "{}", record["name"]);
            assert_eq!(reply.error, expected_error, "{}", record["name"]);
            explicit_budget += 1;
            continue;
        }
        assert!(reply.ok, "{}: {}", record["name"], reply.error);
        let mut expected = record["value"].clone();
        let base = match record["result_from"].as_str().unwrap() {
            "old" => old.text.clone(),
            "new" => new.text.clone(),
            "row_old" => old.text.replace(['\r', '\n'], " "),
            "row_new" => new.text.replace(['\r', '\n'], " "),
            x => panic!("Unknown source {x}"),
        };
        let units = record["result_prefix_utf16"].as_u64().unwrap() as usize;
        let text =
            String::from_utf16(&base.encode_utf16().take(units).collect::<Vec<_>>()).unwrap();
        assert_eq!(
            crate::hex(&Sha256::digest(text.as_bytes())),
            record["value_text_sha256"].as_str().unwrap()
        );
        expected["text"] = text.into();
        let expected: TextValue = serde_json::from_value(expected).unwrap();
        assert_eq!(reply.value, Some(expected), "{}", record["name"]);
        assert_eq!(reply.old_value, Some(old));
        assert_eq!(reply.new_value, Some(new));
        assert_eq!(reply.action, record["action"].as_str().unwrap());
        assert_eq!(
            reply.format_applied,
            record["format_applied"].as_bool().unwrap()
        );
        for (actual, key) in [
            (reply.old_grapheme_count, "old_grapheme_count"),
            (reply.old_utf16_length, "old_utf16_length"),
            (reply.old_utf8_length, "old_utf8_length"),
            (reply.new_grapheme_count, "new_grapheme_count"),
            (reply.new_utf16_length, "new_utf16_length"),
            (reply.new_utf8_length, "new_utf8_length"),
            (reply.grapheme_count, "grapheme_count"),
            (reply.utf16_length, "utf16_length"),
            (reply.utf8_length, "utf8_length"),
        ] {
            assert_eq!(
                actual,
                record[key].as_i64().unwrap(),
                "{} {key}",
                record["name"]
            );
        }
        compared += 1;
    }
    println!(
        "Actual Flutter complete-value matches: {compared}; separately explicit native wire-budget differences: {explicit_budget}"
    );
}
