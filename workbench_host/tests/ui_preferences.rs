#![cfg(windows)]
use morrow_workbench_host::FontPreference;
use morrow_workbench_host::Workbench;
#[test]
fn font_preferences_are_independent_bounded_and_retryable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("font.db");
    let mut host = Workbench::open(&path, None).unwrap();
    assert_eq!(host.read_ui_font().unwrap(), (FontPreference::default(), 0));
    let font = FontPreference {
        family: "Microsoft YaHei".into(),
        ..Default::default()
    };
    assert_eq!(host.save_ui_font("font-create", 0, &font).unwrap(), 1);
    assert_eq!(host.save_ui_font("font-create", 0, &font).unwrap(), 1);
    assert!(
        host.save_ui_font("font-create", 0, &FontPreference::default())
            .is_err()
    );
    assert!(host.save_ui_font("font-stale", 0, &font).is_err());
    host.save_ui_locale("locale-create", 0, "de").unwrap();
    let imported = FontPreference {
        family: String::new(),
        asset: "a".repeat(64),
        name: "My Font.ttf".into(),
    };
    assert_eq!(host.save_ui_font("font-import", 1, &imported).unwrap(), 2);
    for invalid in [
        FontPreference {
            family: "x".repeat(129),
            ..Default::default()
        },
        FontPreference {
            asset: "../outside".into(),
            name: "bad.ttf".into(),
            ..Default::default()
        },
        FontPreference {
            family: "bad\nname".into(),
            ..Default::default()
        },
    ] {
        assert!(host.save_ui_font("font-invalid", 2, &invalid).is_err());
    }
    assert!(host.page("", 100).unwrap().0.is_empty());
    host.finish().unwrap();
    drop(host);
    let mut host = Workbench::open(&path, None).unwrap();
    assert_eq!(host.read_ui_font().unwrap(), (imported, 2));
    assert_eq!(host.read_ui_locale().unwrap().0, "de");
    assert_eq!(
        host.save_ui_font("font-reset", 2, &FontPreference::default())
            .unwrap(),
        3
    );
    host.finish().unwrap();
}
#[test]
fn locale_persists_without_a_plugin_and_retries_original_operation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let mut host = Workbench::open(&path, None).unwrap();
    assert_eq!(host.read_ui_locale().unwrap(), ("system".into(), 0));
    assert_eq!(host.save_ui_locale("locale-create", 0, "en").unwrap(), 1);
    assert_eq!(host.save_ui_locale("locale-create", 0, "en").unwrap(), 1);
    assert!(host.save_ui_locale("locale-create", 0, "zh").is_err());
    assert!(host.save_ui_locale("locale-stale", 0, "zh").is_err());
    assert_eq!(host.save_ui_locale("locale-edit", 1, "zh").unwrap(), 2);
    assert_eq!(host.save_ui_locale("locale-edit", 1, "zh").unwrap(), 2);
    assert!(host.save_ui_locale("locale-invalid", 2, "xx").is_err());
    assert!(host.page("", 100).unwrap().0.is_empty());
    host.finish().unwrap();
    drop(host);
    let mut host = Workbench::open(&path, None).unwrap();
    assert_eq!(host.read_ui_locale().unwrap(), ("zh".into(), 2));
    assert_eq!(host.save_ui_locale("locale-edit", 1, "zh").unwrap(), 2);
    assert_eq!(
        host.save_ui_locale("locale-system", 2, "system").unwrap(),
        3
    );
    host.finish().unwrap();
}

#[test]
fn every_added_locale_roundtrips_through_original_library() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    for (index, locale) in ["ru", "fr", "de", "es", "ja", "ko", "pt"]
        .into_iter()
        .enumerate()
    {
        let mut host = Workbench::open(&path, None).unwrap();
        let revision = host.read_ui_locale().unwrap().1;
        assert_eq!(revision, index as u64);
        let op = format!("locale-{locale}");
        assert_eq!(
            host.save_ui_locale(&op, revision, locale).unwrap(),
            revision + 1
        );
        assert_eq!(
            host.save_ui_locale(&op, revision, locale).unwrap(),
            revision + 1
        );
        host.finish().unwrap();
        drop(host);
        let mut reopened = Workbench::open(&path, None).unwrap();
        assert_eq!(
            reopened.read_ui_locale().unwrap(),
            (locale.into(), revision + 1)
        );
        reopened.finish().unwrap();
    }
}
