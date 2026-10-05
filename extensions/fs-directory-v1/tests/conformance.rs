use morrow_fs_directory_v1::{
    DirectoryEntry, DirectoryState, EntryKind, FsDirectoryPage, NameEncoding, StateLimits,
};
use std::path::Path;
fn u32_at(b: &[u8], p: &mut usize) -> u32 {
    let v = u32::from_le_bytes(b[*p..*p + 4].try_into().unwrap());
    *p += 4;
    v
}
fn u64_at(b: &[u8], p: &mut usize) -> u64 {
    let v = u64::from_le_bytes(b[*p..*p + 8].try_into().unwrap());
    *p += 8;
    v
}
fn data<'a>(b: &'a [u8], p: &mut usize, n: usize) -> &'a [u8] {
    let x = &b[*p..*p + n];
    *p += n;
    x
}
fn corpus() -> Vec<(String, bool, Vec<u8>, FsDirectoryPage)> {
    let b = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors.bin")).unwrap();
    assert_eq!(&b[..8], b"DRCV0001");
    let mut p = 8;
    let count = u32_at(&b, &mut p);
    let mut out = Vec::new();
    for _ in 0..count {
        let nl = u32_at(&b, &mut p) as usize;
        let ok = u32_at(&b, &mut p) != 0;
        let wl = u32_at(&b, &mut p) as usize;
        let count = u32_at(&b, &mut p);
        let terminal = u32_at(&b, &mut p) != 0;
        assert_eq!(u32_at(&b, &mut p), 0);
        let page_sequence = u64_at(&b, &mut p);
        let name = String::from_utf8(data(&b, &mut p, nl).to_vec()).unwrap();
        let mut entries = Vec::new();
        for _ in 0..count {
            let entry_id = data(&b, &mut p, 32).try_into().unwrap();
            let n = u32_at(&b, &mut p) as usize;
            let enc = u32_at(&b, &mut p);
            let kind = u32_at(&b, &mut p);
            let has = u32_at(&b, &mut p) != 0;
            let len = u64_at(&b, &mut p);
            let name = data(&b, &mut p, n).to_vec();
            entries.push(DirectoryEntry {
                entry_id,
                name,
                encoding: if enc == 2 {
                    NameEncoding::Utf16Le
                } else {
                    NameEncoding::Utf8
                },
                kind: match kind {
                    2 => EntryKind::Directory,
                    3 => EntryKind::Other,
                    _ => EntryKind::File,
                },
                logical_length: has.then_some(len),
            });
        }
        let wire = data(&b, &mut p, wl).to_vec();
        out.push((
            name,
            ok,
            wire,
            FsDirectoryPage {
                selection_epoch: [1; 32],
                page_sequence,
                entries,
                terminal,
            },
        ));
    }
    assert_eq!(p, b.len());
    out
}
#[test]
fn independent_directory_schema_complete_fields_owned_bytes_and_far_compatibility() {
    for (name, ok, wire, want) in corpus() {
        let got = FsDirectoryPage::decode(&wire);
        assert_eq!(got.is_ok(), ok, "{name} {got:?}");
        if ok {
            let got = got.unwrap();
            assert_eq!(got, want, "semantic {name}");
            let encoded = got.encode().unwrap();
            assert_eq!(FsDirectoryPage::decode(&encoded).unwrap(), want);
            let mut overwritten = wire;
            overwritten.fill(0xaa);
            assert_eq!(got, want);
        }
    }
}
#[test]
fn state_malformed_terminal_admission_and_release_keep_paid_counters() {
    let mut state = DirectoryState::new([1; 32], StateLimits::default()).unwrap();
    let (_, _, wire, _) = corpus()
        .into_iter()
        .find(|r| r.0 == "file-no-length")
        .unwrap();
    state.admit(&wire).unwrap();
    let saved = state.snapshot();
    assert!(saved.terminal && saved.resident_ids == 1);
    assert!(state.admit(&[0; 8]).is_err());
    let failed = state.snapshot();
    assert_eq!(failed.accepted_pages, saved.accepted_pages);
    assert!(failed.admitted_wire_bytes > saved.admitted_wire_bytes);
    assert!(state.admit(&wire).is_err());
    let paid = state.snapshot().admitted_wire_bytes;
    state.release();
    assert_eq!(state.snapshot().resident_ids, 0);
    assert_eq!(state.snapshot().admitted_wire_bytes, paid);
    assert!(state.admit(&wire).is_err());
    assert_eq!(state.snapshot().admitted_wire_bytes, paid);
}
#[test]
fn state_real_wire_budget_refusal_keeps_all_snapshot_fields() {
    let (_, _, wire, _) = corpus()
        .into_iter()
        .find(|r| r.0 == "file-no-length")
        .unwrap();
    let mut state = DirectoryState::new(
        [1; 32],
        StateLimits {
            max_wire_bytes: wire.len() as u64 - 1,
            ..StateLimits::default()
        },
    )
    .unwrap();
    let saved = state.snapshot();
    assert!(state.admit(&wire).is_err());
    assert_eq!(state.snapshot(), saved);
}
