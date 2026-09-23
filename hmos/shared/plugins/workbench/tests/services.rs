use morrow_workbench_plugin::services::*;
#[test]
fn all_glass_canvas_combinations_and_ranges() {
    for theme in ["white", "custom", "dark"] {
        for glass in ["frosted", "clear", "liquid"] {
            for background in ["ambient", "solid", "texture", "transparent"] {
                for liquid_canvas in [false, true] {
                    let a = Appearance {
                        theme: theme.into(),
                        glass: glass.into(),
                        background: background.into(),
                        liquid_canvas,
                        ..Default::default()
                    };
                    a.validate().unwrap();
                }
            }
        }
    }
    let mut a = Appearance {
        opacity: f64::NAN,
        ..Default::default()
    };
    assert!(a.validate().is_err());
    a.opacity = 0.19;
    assert!(a.validate().is_err());
    a.opacity = 0.2;
    a.window_radius = 32.;
    a.validate().unwrap();
    a.window_radius = 32.1;
    assert!(a.validate().is_err());
}
#[test]
fn lyrics_offsets_multiple_stamps_fraction_sort_and_match() {
    let parsed = parse_lyrics("[offset:+100]\n[01:02.3][00:01:045] 中文\n[00:00.01]开头").unwrap();
    assert_eq!(
        parsed.iter().map(|l| l.time_ms).collect::<Vec<_>>(),
        [0, 945, 62200]
    );
    assert_eq!(parsed[1].text, "中文");
    let candidates = vec![
        Candidate {
            title: " A B ".into(),
            artist: "One".into(),
            duration: 100.,
            synced: false,
            lyrics: "plain".into(),
        },
        Candidate {
            title: "ab".into(),
            artist: "One".into(),
            duration: 101.,
            synced: true,
            lyrics: "sync".into(),
        },
    ];
    assert_eq!(
        lyric_match(&candidates, "ab", "one", 100.).unwrap(),
        Some(1)
    );
    let mut ambiguous = candidates;
    ambiguous[1].artist = "Two".into();
    assert_eq!(lyric_match(&ambiguous, "ab", "", 100.).unwrap(), None);
}
#[test]
fn playlist_wrap_remove_restore_and_audio_exclusivity() {
    let p = Playback {
        ids: vec!["a".into(), "b".into(), "c".into()],
        playing: true,
        ..Default::default()
    };
    let (p, _) = p.apply(MusicAction::Previous, 0, true).unwrap();
    assert_eq!(p.index, 2);
    let (p, e) = p.apply(MusicAction::Remove, 0, false).unwrap();
    assert_eq!(p.index, 1);
    assert_eq!(e, "none");
    let (p, e) = p.apply(MusicAction::Block, 0, true).unwrap();
    assert!(!p.playing);
    assert_eq!(e, "pause");
    let (p, _) = p.apply(MusicAction::Toggle, 0, true).unwrap();
    assert!(!p.playing);
    let (p, _) = p.apply(MusicAction::Restore, 0, false).unwrap();
    assert!(!p.playing);
    assert_eq!(p.position_ms, 0);
}
#[test]
fn media_caps_and_urls() {
    import_policy("file", 200 * 1024 * 1024, "", true).unwrap();
    assert!(import_policy("file", 200 * 1024 * 1024 + 1, "", true).is_err());
    import_policy(
        "video",
        150 * 1024 * 1024,
        "https://example.org/a.mp4",
        false,
    )
    .unwrap();
    assert!(import_policy("image", 25 * 1024 * 1024 + 1, "", false).is_err());
    for url in [
        "file:///private",
        "https://user:pass@example.org/a",
        "javascript:alert(1)",
    ] {
        assert!(import_policy("image", 1, url, false).is_err());
    }
}

#[test]
fn material_parameters_validate_independent_opacity_and_blur() {
    use morrow_workbench_plugin::services::Appearance;
    let valid = Appearance {
        canvas_blur: 40.,
        canvas_opacity: 0.,
        component_custom: true,
        component_opacity: 0.,
        component_blur: 0.,
        ..Appearance::default()
    };
    valid.validate().unwrap();
    for value in [f64::NAN, f64::INFINITY, -0.1, 40.1] {
        assert!(
            Appearance {
                canvas_blur: value,
                ..valid.clone()
            }
            .validate()
            .is_err()
        );
    }
    for value in [f64::NAN, -0.1, 1.1] {
        assert!(
            Appearance {
                canvas_opacity: value,
                ..valid.clone()
            }
            .validate()
            .is_err()
        );
        assert!(
            Appearance {
                component_opacity: value,
                ..valid.clone()
            }
            .validate()
            .is_err()
        );
    }
}
