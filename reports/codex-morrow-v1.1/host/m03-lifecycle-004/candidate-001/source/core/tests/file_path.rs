use morrow_core::{Error, file_path::RelativeFilePath};

fn invalid(value: &str) {
    assert!(
        matches!(RelativeFilePath::parse(value), Err(Error::Invalid(_))),
        "expected invalid syntax: {value:?}"
    );
}

fn limited(value: &str) {
    assert_eq!(RelativeFilePath::parse(value), Err(Error::Limit));
}

#[test]
fn preserves_valid_utf8_without_decoding_or_normalization() {
    for spelling in [
        "file.txt",
        "docs/report.v1.txt",
        "资料/😀/e\u{301}.txt",
        "é.txt",
        "%2e%2e/%2F.txt",
        "COM0.txt",
        "LPT0.txt",
        "conifer.txt",
        "clock.txt",
    ] {
        assert_eq!(
            RelativeFilePath::parse(spelling).unwrap().as_str(),
            spelling
        );
    }
    assert_ne!(
        RelativeFilePath::parse("e\u{301}.txt").unwrap(),
        RelativeFilePath::parse("é.txt").unwrap()
    );
}

#[test]
fn rejects_empty_absolute_segments_and_traversal() {
    for value in ["", "/", "/a", "a/", "a//b", ".", "..", "a/./b", "a/../b"] {
        invalid(value);
    }
}

#[test]
fn rejects_separators_controls_ads_punctuation_and_trailing_space_or_dot() {
    for value in [
        r"a\b",
        r"\server\share",
        "C:drive",
        "a:stream",
        "a/b:stream",
        "a\0b",
        "a\nb",
        "a\rb",
        "a\u{007f}b",
        "a\u{0085}b",
        "a ",
        "a.",
        "folder/name ",
        "folder/name.",
        "a<b",
        "a>b",
        "a\"b",
        "a|b",
        "a?b",
        "a*b",
    ] {
        invalid(value);
    }
}

#[test]
fn rejects_windows_devices_in_any_segment_even_with_extension() {
    for stem in [
        "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "CLOCK$", "COM1", "COM2", "COM3", "COM4",
        "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6",
        "LPT7", "LPT8", "LPT9", "COM¹", "COM²", "COM³", "LPT¹", "LPT²", "LPT³",
    ] {
        invalid(stem);
        invalid(&format!("folder/{stem}.txt"));
        invalid(&format!("folder/{}.TXT", stem.to_lowercase()));
        invalid(&format!("folder/{stem} .txt"));
    }
}

#[test]
fn enforces_total_utf8_segment_utf16_and_segment_count_limits() {
    assert!(RelativeFilePath::parse(&"a".repeat(255)).is_ok());
    limited(&"a".repeat(256));
    assert!(RelativeFilePath::parse(&("😀".repeat(127) + "a")).is_ok());
    limited(&"😀".repeat(128));

    let segments = vec!["a"; 128].join("/");
    assert!(RelativeFilePath::parse(&segments).is_ok());
    limited(&(segments + "/a"));

    let mut parts = vec!["a".repeat(31); 128];
    parts[0].push('a');
    let exact = parts.join("/");
    assert_eq!(exact.len(), 4096);
    assert!(RelativeFilePath::parse(&exact).is_ok());
    limited(&(exact + "a"));
}
