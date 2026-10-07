use super::*;
use sha2::{Digest, Sha256};

#[test]
fn pinned_tables_use_the_actual_flutter_characters_unicode_version() {
    assert_eq!(unicode_segmentation::UNICODE_VERSION, (16, 0, 0));
}

#[test]
fn complete_raw_field_counts_are_independent_of_utf16_offsets() {
    for (text, count, units) in [
        ("", 0, 0),
        (" A\r\nB ", 5, 6),
        ("中文", 2, 2),
        ("😀", 1, 2),
        ("👨‍👩‍👧‍👦", 1, 11),
        ("🇨🇳🇺🇸", 2, 8),
        ("e\u{301}", 1, 2),
        ("\u{915}\u{94d}\u{937}", 1, 3),
        ("\u{995}\u{9cd}\u{995}", 1, 3),
        ("\u{d15}\u{d4d}\u{d37}", 1, 3),
        ("\0\u{fffd}", 2, 2),
    ] {
        let reply = inspect("description", text);
        assert!(reply.ok, "{text:?}: {}", reply.error);
        assert_eq!(reply.grapheme_count, count, "{text:?}");
        assert_eq!(reply.utf16_length, units, "{text:?}");
        assert_eq!(reply.utf8_length, text.len() as i64);
    }
}

#[test]
fn every_field_has_its_real_limit_and_keeps_complete_overflow_counts() {
    for (field, limit) in [
        ("title", 60),
        ("todos", 1000),
        ("hypothesis", 5000),
        ("conclusion", 10000),
        ("description", 20000),
    ] {
        for sample in [
            "a",
            "中",
            "😀",
            "👨‍👩‍👧‍👦",
            "🇨🇳",
            "e\u{301}",
            "\u{915}\u{94d}\u{937}",
        ] {
            for extra in [-1_i64, 0, 1] {
                let n = (limit as i64 + extra) as usize;
                let text = sample.repeat(n);
                let reply = request(&serde_json::json!({"field":field,"text":text}).to_string());
                assert_eq!(reply.limit, limit);
                assert_eq!(reply.grapheme_count, n as i64, "{field} {sample:?}");
                assert_eq!(reply.utf16_length, text.encode_utf16().count() as i64);
                assert_eq!(reply.utf8_length, text.len() as i64);
                assert_eq!(reply.ok, extra <= 0);
                assert_eq!(
                    reply.error,
                    if extra <= 0 {
                        ""
                    } else {
                        "EditorFieldGraphemeLimit"
                    }
                );
            }
        }
    }
}

#[test]
fn escaped_unpaired_utf16_fails_without_replacement_or_losing_known_field() {
    for text in [
        r#""\ud800""#,
        r#""\udc00""#,
        r#""\ud800a""#,
        r#""\ud800\ud800""#,
        r#""a\udc00""#,
    ] {
        let reply = request(&format!(r#"{{"field":"title","text":{text}}}"#));
        assert_eq!(reply, Reply::failure("title", "EditorFieldInvalidUnicode"));
    }
    let pair = request(r#"{"field":"title","text":"\ud83d\ude00"}"#);
    assert!(pair.ok);
    assert_eq!(pair.grapheme_count, 1);
    assert_eq!(pair.utf16_length, 2);
    assert!(request(r#"{"field":"title","text":"\ufffd"}"#).ok);
}

#[test]
fn schema_and_transport_limits_are_distinct_from_character_count() {
    for input in [
        "{}",
        r#"{"field":"title"}"#,
        r#"{"field":"title","text":"x","extra":1}"#,
        r#"{"field":"title","field":"todos","text":"x"}"#,
    ] {
        assert_eq!(
            request(input),
            Reply::failure("", "EditorFieldInvalidRequest")
        );
    }
    assert_eq!(
        request(r#"{"field":"other","text":"x"}"#),
        Reply::failure("other", "EditorFieldName")
    );
    assert_eq!(
        request(r#"{"field":"title","text":[]}"#),
        Reply::failure("title", "EditorFieldTextRequired")
    );
    let one_cluster = format!("a{}", "\u{301}".repeat(MAX_TEXT_BYTES / 2));
    assert_eq!(grapheme_count(&one_cluster), 1);
    assert_eq!(
        inspect("title", &one_cluster),
        Reply::failure("title", "EditorFieldTextBytesLimit")
    );
    assert_eq!(
        request(&" ".repeat(MAX_REQUEST_BYTES + 1)),
        Reply::failure("", "EditorFieldRequestBytesLimit")
    );
}

#[test]
fn async_ffi_contract_is_pure_and_always_owns_a_freeable_reply() {
    unsafe {
        let input = std::ffi::CString::new(r#"{"field":"title","text":"e\u0301"}"#).unwrap();
        let output = crate::morrow_hmos_editor_field(input.as_ptr());
        let reply: Reply =
            serde_json::from_str(std::ffi::CStr::from_ptr(output).to_str().unwrap()).unwrap();
        crate::morrow_hmos_free(output);
        assert!(reply.ok);
        assert_eq!(reply.grapheme_count, 1);
        assert_eq!(reply.utf16_length, 2);
        let output = crate::morrow_hmos_editor_field(std::ptr::null());
        let reply: Reply =
            serde_json::from_str(std::ffi::CStr::from_ptr(output).to_str().unwrap()).unwrap();
        crate::morrow_hmos_free(output);
        assert_eq!(reply.error, "EditorFieldNullRequest");
        let bytes = [0xff_u8, 0];
        let output = crate::morrow_hmos_editor_field(bytes.as_ptr().cast());
        let reply: Reply =
            serde_json::from_str(std::ffi::CStr::from_ptr(output).to_str().unwrap()).unwrap();
        crate::morrow_hmos_free(output);
        assert_eq!(reply.error, "EditorFieldInvalidUtf8");
    }
}

#[test]
fn markdown_and_rich_outputs_accept_full_graphemes_above_the_old_utf16_proxy() {
    let text = "e\u{301}".repeat(20_000);
    assert_eq!(text.encode_utf16().count(), 40_000);
    assert_eq!(
        crate::markdown::paste_plain(&text, "description").unwrap(),
        text
    );
    assert!(crate::markdown::project(&text).is_ok());
    for (format, source) in [
        ("plain", text.clone()),
        ("html", format!("<p>{text}</p>")),
        ("rtf", format!("{{\\rtf1\\ansi\\ansicpg65001 {text}}}")),
    ] {
        let expected_sha256 = crate::hex(&Sha256::digest(source.as_bytes()));
        let request=serde_json::json!({"format":format,"section":"description","expected_sha256":expected_sha256}).to_string();
        let reply = crate::clipboard::convert(&request, &mut source.as_bytes()).unwrap();
        assert_eq!(reply.paste_text, text);
        assert_eq!(reply.paste_grapheme_count, 20_000);
        assert_eq!(reply.paste_utf16_length, 40_000);
        assert_eq!(reply.paste_utf8_length, text.len());
        assert_eq!(reply.unicode_version, "16.0.0");
    }
    assert!(
        crate::markdown::paste_plain(&(text + "x"), "description")
            .unwrap_err()
            .contains("Grapheme")
    );
}

#[test]
fn render_and_converter_bytes_remain_independent_of_one_huge_grapheme() {
    let text = format!("a{}", "\u{301}".repeat(270_000));
    assert_eq!(grapheme_count(&text), 1);
    assert_eq!(
        crate::markdown::paste_plain(&text, "description").unwrap_err(),
        "MarkdownLimit:InputBytes"
    );
    let html = format!("<p>{text}</p>");
    let request=serde_json::json!({"format":"html","section":"description","expected_sha256":crate::hex(&Sha256::digest(html.as_bytes()))}).to_string();
    assert_eq!(
        crate::clipboard::convert(&request, &mut html.as_bytes()).unwrap_err(),
        "ClipboardOutputBytesLimit"
    );
    // The reading projection's existing total-string budget remains stricter
    // than the plain field/native request budget.
    let within = format!("a{}", "\u{301}".repeat(140_000));
    assert!(inspect("title", &within).ok);
    assert_eq!(
        crate::markdown::project(&within).unwrap_err(),
        "MarkdownLimit:Strings"
    );
    let near_wire_limit = format!("a{}", "\u{301}".repeat(262_142));
    assert!(near_wire_limit.len() < MAX_TEXT_BYTES);
    assert!(inspect("title", &near_wire_limit).ok);
    let request=serde_json::json!({"format":"plain","section":"description","expected_sha256":crate::hex(&Sha256::digest(near_wire_limit.as_bytes()))}).to_string();
    assert_eq!(
        crate::clipboard::convert(&request, &mut near_wire_limit.as_bytes()).unwrap_err(),
        "ClipboardReplyBytesLimit"
    );
    let failure = crate::clipboard::ConvertReply::failure("test-error".into());
    assert_eq!(failure.paste_text, "");
    assert_eq!(
        (
            failure.paste_grapheme_count,
            failure.paste_utf16_length,
            failure.paste_utf8_length
        ),
        (0, 0, 0)
    );
    assert_eq!(failure.unicode_version, "16.0.0");
}

#[test]
#[ignore = "requires fresh actual Flutter Characters reference in HMOS_EDITOR_FIELD_FLUTTER_REFERENCE"]
fn actual_flutter_characters_corpus_and_complete_field_boundaries_match() {
    let path = std::env::var("HMOS_EDITOR_FIELD_FLUTTER_REFERENCE")
        .expect("actual Flutter field capture path");
    let values: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(
        values.len() > 1000,
        "complete actual Characters corpus plus field boundaries"
    );
    for value in values {
        let sample: String = value["codepoints"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| char::from_u32(v.as_u64().unwrap() as u32).unwrap())
            .collect();
        let text = sample.repeat(value["repeats"].as_u64().unwrap() as usize);
        assert_eq!(text.len(), value["utf8_length"].as_u64().unwrap() as usize);
        assert_eq!(
            crate::hex(&Sha256::digest(text.as_bytes())),
            value["sha256"].as_str().unwrap()
        );
        let reply = inspect(value["field"].as_str().unwrap(), &text);
        assert_eq!(
            reply.grapheme_count,
            value["grapheme_count"].as_i64().unwrap(),
            "{value}"
        );
        assert_eq!(reply.utf16_length, value["utf16_length"].as_i64().unwrap());
        assert_eq!(reply.ok, value["ok"].as_bool().unwrap(), "{value}");
    }
}
