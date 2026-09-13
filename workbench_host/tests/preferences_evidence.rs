#![cfg(target_os = "windows")]
mod common;
use morrow_core::{
    plugin_package::{Package, proto::TransformHandler},
    task::Invocation,
    task_evidence::Evidence,
};
use morrow_plugin_runtime::{Limits, replay};
use morrow_workbench_host::Workbench;
use morrow_workbench_plugin::preferences::{
    self as p, Preferences, Source, Track,
    proto::{Appearance, ComponentMaterial},
};
const CARD: &str = "morrow-studio-preferences";
fn package() -> Package {
    let original = common::package();
    let mut manifest = original.manifest().clone();
    manifest.transform_handlers.push(TransformHandler {
        handler: "studio.preferences".into(),
        input_type: "morrow.studio.preferences.v1".into(),
        output_type: "morrow.studio.preferences.v1".into(),
        max_input_bytes: 65536,
        max_output_bytes: 65536,
    });
    Package::build(manifest, original.module()).unwrap()
}
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
            location: format!("file:///C:/synthetic/{i}.mp3"),
            name: format!("{i}.mp3"),
            kind: "audio".into(),
            local: true,
        }),
        title: format!("synthetic track {i}"),
        ..Default::default()
    }
}
fn original(h: &Workbench, op: &str) -> Evidence {
    let mut e = h.operation_evidence(CARD, op).unwrap();
    assert_eq!(e.len(), 1);
    e.remove(0)
}
fn configure(h: &mut Workbench, enabled: bool) {
    let s = h.plugin_status();
    h.configure_plugin(s.revision, &s.digest, enabled).unwrap();
}
fn assert_pages(e: &Evidence, prefs: &Preferences) {
    assert_eq!(e.data().schema_version, 2);
    let b = e.data().batch.as_ref().unwrap();
    assert_eq!(b.intent_type, "morrow.studio.preferences-save.v1");
    assert_eq!(p::decode_wire(&b.intent).unwrap(), *prefs);
    let pages = p::validation_pages(prefs).unwrap();
    assert_eq!(b.observations.len(), pages.len());
    for (o, page) in b.observations.iter().zip(pages) {
        let i = Invocation::decode(&o.invocation).unwrap();
        let t = i.transform().unwrap();
        assert_eq!(t.handler, "studio.preferences");
        assert_eq!(t.input_type, "morrow.studio.preferences.v1");
        assert_eq!(t.output_type, "morrow.studio.preferences.v1");
        assert_eq!(t.input, page);
        assert_eq!(
            p::decode_wire(&i.verify_output(&o.completion).unwrap().bytes).unwrap(),
            p::decode_wire(&page).unwrap()
        );
        assert_eq!(o.fault, 0);
        assert_eq!(o.exit_code, Some(0));
    }
}
#[test]
fn actual_sdk_complete_649_pages_form_one_atomic_self_contained_batch() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    let mut prefs = config();
    prefs.tracks = (0..512).map(track).collect();
    prefs.index = 511;
    prefs.completed = (0..128).map(|i| format!("done-{i}")).collect();
    prefs.components = (0..4096)
        .map(|i| ComponentMaterial {
            id: format!("card:{i}"),
            enabled: i % 2 == 0,
            blur: (i % 41) as f64,
            opacity: 0.5,
            color: 0xff123456,
            has_color: true,
        })
        .collect();
    assert_eq!(p::validation_pages(&prefs).unwrap().len(), 649);
    let input = p::encode_wire(&prefs).unwrap();
    assert_eq!(
        h.save_preferences("all-pages", input.clone()).unwrap(),
        input
    );
    let e = original(&h, "all-pages");
    assert_pages(&e, &prefs);
    assert!(
        replay::replay_batch(&e, Limits::default(), 1_000_000_000)
            .unwrap()
            .matches
    );
    h.finish().unwrap();
    drop(h);
    let h = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    assert_eq!(h.read_preferences().unwrap(), Some(input));
    let stored = original(&h, "all-pages");
    assert_eq!(stored.container(), e.container());
    assert_eq!(stored.digest(), e.digest());
}
#[test]
fn historical_retries_return_old_intent_without_overwriting_later_settings() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    let a = p::encode_wire(&config()).unwrap();
    h.save_preferences("first", a.clone()).unwrap();
    let first = original(&h, "first");
    let mut changed = config();
    changed.show_lyrics = true;
    changed.tracks.push(track(0));
    let b = p::encode_wire(&changed).unwrap();
    h.save_preferences("second", b.clone()).unwrap();
    let second = original(&h, "second");
    assert_pages(&second, &changed);
    assert_eq!(h.save_preferences("first", a.clone()).unwrap(), a);
    assert_eq!(h.read_preferences().unwrap(), Some(b.clone()));
    assert!(h.save_preferences("first", b.clone()).is_err());
    assert!(h.save_preferences("second", a.clone()).is_err());
    h.finish().unwrap();
    drop(h);
    let mut h = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    assert_eq!(h.save_preferences("first", a.clone()).unwrap(), a);
    assert_eq!(h.read_preferences().unwrap(), Some(b));
    assert_eq!(original(&h, "first").container(), first.container());
    assert_eq!(original(&h, "second").container(), second.container());
}
#[test]
fn current_permission_is_required_even_for_previously_committed_settings() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    let input = p::encode_wire(&config()).unwrap();
    h.save_preferences("first", input.clone()).unwrap();
    let e = original(&h, "first");
    configure(&mut h, false);
    assert!(h.save_preferences("first", input.clone()).is_err());
    assert_eq!(h.read_preferences().unwrap(), Some(input.clone()));
    assert_eq!(original(&h, "first").digest(), e.digest());
    h.finish().unwrap();
    drop(h);
    let mut h = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    assert!(h.save_preferences("first", input.clone()).is_err());
    configure(&mut h, true);
    assert_eq!(h.save_preferences("first", input.clone()).unwrap(), input);
    h.finish().unwrap();
    assert!(h.save_preferences("first", input).is_err());
}
#[test]
fn cross_card_operation_collision_and_invalid_later_page_never_change_settings() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    h.create("occupied", common::idea("card")).unwrap();
    let input = p::encode_wire(&config()).unwrap();
    assert!(h.save_preferences("occupied", input.clone()).is_err());
    assert!(h.read_preferences().unwrap().is_none());
    h.save_preferences("seed", input.clone()).unwrap();
    let mut oversized = config();
    let mut t = track(0);
    t.lyrics = "l".repeat(49152);
    t.title = "t".repeat(16384);
    oversized.tracks.push(t);
    let wire = p::encode_wire(&oversized).unwrap();
    assert!(p::validation_pages(&oversized).is_err());
    assert!(h.save_preferences("oversized", wire).is_err());
    assert!(h.operation_evidence(CARD, "oversized").is_err());
    assert_eq!(h.read_preferences().unwrap(), Some(input));
    assert!(h.writable());
}
fn trap_package() -> Package {
    let mut module = vec![
        0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 127, 3, 2, 1, 0, 5, 3, 1, 0, 1, 7, 23, 2,
        10, b'm', b'o', b'r', b'r', b'o', b'w', b'_', b'r', b'u', b'n', 0, 0, 6, b'm', b'e', b'm',
        b'o', b'r', b'y', 2, 0,
    ];
    module.extend([10, 5, 1, 3, 0, 0, 11]);
    let good = package();
    let mut manifest = Package::manifest_for_transform(
        "org.morrow.workbench",
        "0.1.9-test.99",
        &module,
        good.manifest().transform_handlers.clone(),
    );
    manifest.requested_capabilities = good.manifest().requested_capabilities.clone();
    Package::build(manifest, &module).unwrap()
}
#[test]
fn explicitly_enabled_trapping_upgrade_can_retry_history_without_executing_guest() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    let input = p::encode_wire(&config()).unwrap();
    h.save_preferences("first", input.clone()).unwrap();
    let e = original(&h, "first");
    h.finish().unwrap();
    drop(h);
    let mut h = Workbench::open_managed(dir.path(), Some(trap_package())).unwrap();
    assert!(!h.writable());
    configure(&mut h, true);
    assert!(h.writable());
    assert_eq!(h.save_preferences("first", input.clone()).unwrap(), input);
    assert!(h.writable(), "retry must not run trapping guest");
    assert_eq!(original(&h, "first").digest(), e.digest());
    let mut changed = config();
    changed.show_lyrics = true;
    let err = h
        .save_preferences("new", p::encode_wire(&changed).unwrap())
        .unwrap_err();
    assert!(!err.to_string().is_empty());
    assert!(h.operation_evidence(CARD, "new").is_err());
    assert_eq!(h.read_preferences().unwrap(), Some(input));
    assert!(!h.writable());
}
#[test]
fn unchanged_settings_leave_no_operation_but_do_not_hide_existing_id_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    let input = p::encode_wire(&config()).unwrap();
    h.save_preferences("seed", input.clone()).unwrap();
    assert_eq!(h.save_preferences("noop", input.clone()).unwrap(), input);
    assert!(h.operation_evidence(CARD, "noop").is_err());
    let mut changed = config();
    changed.show_lyrics = true;
    let changed = p::encode_wire(&changed).unwrap();
    h.save_preferences("noop", changed.clone()).unwrap();
    assert!(h.save_preferences("seed", changed).is_err());
}

#[test]
fn near_four_mib_actual_lyrics_batch_fits_total_fuel_and_keeps_full_intent() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    let mut prefs = config();
    prefs.tracks = (0..80)
        .map(|i| {
            let mut t = track(i);
            t.lyrics = "a".repeat(49152);
            t
        })
        .collect();
    prefs.index = 79;
    let input = p::encode_wire(&prefs).unwrap();
    assert!(input.len() > 3 * 1024 * 1024);
    assert_eq!(h.save_preferences("large", input.clone()).unwrap(), input);
    let e = original(&h, "large");
    assert_pages(&e, &prefs);
    let batch = e.data().batch.as_ref().unwrap();
    assert_eq!(batch.observations.len(), 81);
    let used: u64 = batch
        .observations
        .iter()
        .map(|o| o.budget.as_ref().unwrap().fuel - o.fuel_remaining)
        .sum();
    assert!(used <= batch.total_fuel);
    println!(
        "81-page near-4MiB actual fuel={used}, raw={} container={}",
        e.raw().len(),
        e.container().len()
    );
    assert!(
        replay::replay_batch(&e, Limits::default(), 1_000_000_000)
            .unwrap()
            .matches
    );
}
