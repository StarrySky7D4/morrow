use super::*;

fn units(value: &str) -> Vec<u16> {
    let mut result = Vec::with_capacity(value.encode_utf16().count());
    result.extend(value.encode_utf16());
    result
}
fn path(component: Vec<u16>) -> Result<DirectoryRelativePath, DirectoryPathError> {
    DirectoryRelativePath::new(vec![component])
}

#[test]
fn forbidden_navigation_control_separator_ads_and_trailing_spellings_are_refused() {
    for value in [
        "",
        ".",
        "..",
        "a/b",
        "a\\b",
        "C:",
        "a:stream",
        "a\0b",
        "a\u{001f}b",
        "a\u{007f}b",
        "a\u{0085}b",
        "a.",
        "a ",
        "<",
        ">",
        "\"",
        "|",
        "?",
        "*",
    ] {
        assert_eq!(
            path(units(value)),
            Err(DirectoryPathError::InvalidComponent)
        );
    }
    assert_eq!(
        DirectoryRelativePath::new(Vec::new()),
        Err(DirectoryPathError::InvalidComponent)
    );
}

#[test]
fn semantic_unit_segment_and_aggregate_byte_limits_are_exact() {
    assert!(path(vec![97; 255]).is_ok());
    assert_eq!(path(vec![97; 256]), Err(DirectoryPathError::Limit));
    let mut accepted = Vec::with_capacity(17);
    for _ in 0..16 {
        accepted.push(vec![97; 255]);
    }
    accepted.push(vec![98; 16]);
    let value = DirectoryRelativePath::new(accepted).unwrap();
    assert_eq!(value.utf16_bytes(), 8192);
    assert_eq!(value.component_count(), 17);
    let mut too_many_bytes = Vec::with_capacity(17);
    for _ in 0..16 {
        too_many_bytes.push(vec![97; 255]);
    }
    too_many_bytes.push(vec![98; 17]);
    assert_eq!(
        DirectoryRelativePath::new(too_many_bytes),
        Err(DirectoryPathError::Limit)
    );
    let mut accepted = Vec::with_capacity(32);
    for _ in 0..32 {
        accepted.push(vec![97]);
    }
    assert_eq!(
        DirectoryRelativePath::new(accepted)
            .unwrap()
            .component_count(),
        32
    );
    let mut rejected = Vec::with_capacity(33);
    for _ in 0..33 {
        rejected.push(vec![97]);
    }
    assert_eq!(
        DirectoryRelativePath::new(rejected),
        Err(DirectoryPathError::Limit)
    );
}

#[test]
fn raw_isolated_surrogates_and_non_ascii_spelling_remain_exact() {
    for component in [
        vec![0xd800],
        vec![0xdc00],
        vec![97, 0xd800, 0x4e2d, 0xdc00],
        units("目录"),
        units("é"),
        units("e\u{0301}"),
    ] {
        let expected = component.clone();
        let value = path(component).unwrap();
        assert_eq!(value.as_components(), &[expected]);
    }
    assert_ne!(path(units("é")).unwrap(), path(units("e\u{0301}")).unwrap());
}

#[test]
fn device_stems_ascii_case_extensions_and_superscript_digits_are_refused() {
    for value in [
        "CON",
        "con.txt",
        "PRN",
        "AUX",
        "NUL",
        "CONIN$",
        "CONOUT$",
        "CLOCK$",
        "CoM1.txt",
        "LPT9",
        "COM¹",
        "com².log",
        "LPT³.bin",
        "CON .txt",
    ] {
        assert_eq!(
            path(units(value)),
            Err(DirectoryPathError::InvalidComponent)
        );
    }
    for value in ["console", "COM0", "COM10", "LPT0", "COM⁴", "NULx"] {
        assert!(path(units(value)).is_ok());
    }
}

#[test]
fn oversized_component_container_capacity_is_refused_even_with_one_short_name() {
    let mut components = Vec::with_capacity(33);
    components.push(vec![97]);
    assert_eq!(
        DirectoryRelativePath::new(components),
        Err(DirectoryPathError::Limit)
    );
}

#[test]
fn oversized_name_capacity_is_refused_even_with_one_unit() {
    let mut component = Vec::with_capacity(256);
    component.push(97);
    assert_eq!(path(component), Err(DirectoryPathError::Limit));
}

#[test]
fn aggregate_spare_capacity_is_bounded_and_actual_retained_bytes_are_accounted() {
    let mut too_large = Vec::with_capacity(17);
    for _ in 0..17 {
        let mut component = Vec::with_capacity(255);
        component.push(97);
        too_large.push(component);
    }
    assert_eq!(
        DirectoryRelativePath::new(too_large),
        Err(DirectoryPathError::Limit)
    );
    let mut accepted = Vec::with_capacity(2);
    let mut component = Vec::with_capacity(255);
    component.push(97);
    accepted.push(component);
    let value = DirectoryRelativePath::new(accepted).unwrap();
    assert_eq!(value.utf16_bytes(), 2);
    assert_eq!(
        value.retained_input_bytes(),
        2 * std::mem::size_of::<Vec<u16>>() + 510
    );
    let cloned = value.clone();
    let actual = cloned.components.capacity() * std::mem::size_of::<Vec<u16>>()
        + cloned
            .components
            .iter()
            .map(|component| component.capacity() * 2)
            .sum::<usize>();
    assert_eq!(cloned.retained_input_bytes(), actual);
    assert!(cloned.retained_input_bytes() <= value.retained_input_bytes());
}

#[test]
fn debug_output_redacts_exact_directory_spelling() {
    let value = path(units("sensitive-directory-name")).unwrap();
    let debug = format!("{value:?}");
    assert!(!debug.contains("sensitive"));
    assert!(debug.contains("component_count"));
    assert!(debug.contains("utf16_bytes"));
}
