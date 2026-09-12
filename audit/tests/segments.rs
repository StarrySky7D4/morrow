use morrow_audit::*;
use morrow_core::{
    content::CardRecord,
    records::Record,
    store::{EventBudget, Store},
};
use prost::Message;
fn fixture() -> (SigningKey, TrustedLog, Vec<(i64, Vec<u8>)>) {
    let key = SigningKey::from_bytes(&[7; 32]); // deterministic test-only key
    let trusted = TrustedLog {
        id: "test-log".into(),
        key: key.verifying_key(),
    };
    let d = tempfile::tempdir().unwrap();
    let mut store = Store::open(&d.path().join("db"), EventBudget::default()).unwrap();
    store
        .create_local(
            "create-a",
            &CardRecord::new("a", "text", 1, "A", b"original".to_vec()).unwrap(),
        )
        .unwrap();
    store
        .create_record_local(
            "create-workspace",
            &Record::workspace("w", "Workspace").unwrap(),
        )
        .unwrap();
    store
        .create_local(
            "create-b",
            &CardRecord::new("b", "text", 1, "B", vec![]).unwrap(),
        )
        .unwrap();
    (key, trusted, store.pending(0, 10).unwrap())
}
fn frame(bytes: &[u8]) -> proto::SignedFrame {
    let len = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    let raw = lz4_flex::block::decompress(&bytes[18..], len).unwrap();
    proto::SignedFrame::decode(raw.as_slice()).unwrap()
}
fn container(frame: proto::SignedFrame) -> Vec<u8> {
    let raw = frame.encode_to_vec();
    let packed = lz4_flex::block::compress(&raw);
    [
        b"MORROWA1".as_slice(),
        &1u16.to_le_bytes(),
        &(raw.len() as u32).to_le_bytes(),
        &(packed.len() as u32).to_le_bytes(),
        &packed,
    ]
    .concat()
}
#[test]
fn original_bytes_unknown_fields_are_signed_not_reserialized() {
    let (key, trust, events) = fixture();
    let segment = from_pending(&trust, 1, [0; 32], &events).unwrap();
    let mut raw = segment.encode_to_vec();
    raw.extend_from_slice(&[0xa0, 0x06, 7]);
    let bytes = sign_raw(&raw, &trust, &key).unwrap();
    let checked = verify(&bytes, &trust).unwrap();
    assert_eq!(checked.raw(), raw);
    assert_eq!(checked.container(), bytes);
    assert_ne!(checked.raw(), checked.segment().encode_to_vec());
    assert_eq!(checked.segment().events[0].original_commit, events[0].1);
    let mut changed = frame(&bytes);
    *changed.raw_segment.last_mut().unwrap() = 8;
    assert!(matches!(
        verify(&container(changed), &trust),
        Err(AuditError::Signature)
    ));
    let mut normalized = frame(&bytes);
    normalized.raw_segment = segment.encode_to_vec();
    assert!(matches!(
        verify(&container(normalized), &trust),
        Err(AuditError::Signature)
    ));
    assert_eq!(sign_raw(&raw, &trust, &key).unwrap(), bytes);
}
#[test]
fn pinned_key_chain_and_checkpoint_reject_tamper_reorder_and_rollback() {
    let (key, trust, events) = fixture();
    let first = sign(
        &from_pending(&trust, 1, [0; 32], &events[..2]).unwrap(),
        &trust,
        &key,
    )
    .unwrap();
    let verified = verify(&first, &trust).unwrap();
    let second = sign(
        &from_pending(&trust, 2, verified.digest(), &events[2..]).unwrap(),
        &trust,
        &key,
    )
    .unwrap();
    let checkpoint = Checkpoint::from_verified(&verify(&second, &trust).unwrap());
    let wrong = TrustedLog {
        id: trust.id.clone(),
        key: SigningKey::from_bytes(&[8; 32]).verifying_key(),
    };
    assert!(verify(&first, &wrong).is_err());
    let mut chain = ChainVerifier::new(trust, Some(checkpoint)).unwrap();
    assert!(chain.accept(&second).is_err());
    chain.accept(&first).unwrap();
    assert!(chain.finish().is_err()); // truncated history behind independently pinned checkpoint
    assert!(chain.accept(&first).is_err());
    chain.accept(&second).unwrap();
    assert_eq!(chain.finish().unwrap(), (2, 3));
    let mut bad = frame(&second);
    bad.signature[10] ^= 1;
    let (_, trust, _) = fixture();
    assert!(verify(&container(bad), &trust).is_err());
}
#[test]
fn bounds_identity_sequences_and_foreign_commits_fail_closed() {
    let (key, trust, events) = fixture();
    let valid = from_pending(&trust, 1, [0; 32], &events).unwrap();
    for mode in 0..5 {
        let mut bad = valid.clone();
        match mode {
            0 => bad.events[1].sequence += 2,
            1 => bad.events[1].operation_id = bad.events[0].operation_id.clone(),
            2 => bad.events[0].original_commit[0] ^= 1,
            3 => bad.log_id = "foreign-log".into(),
            _ => bad.events = vec![bad.events[0].clone(); 129],
        }
        assert!(sign(&bad, &trust, &key).is_err());
    }
    let bytes = sign(&valid, &trust, &key).unwrap();
    let mut size = bytes.clone();
    size[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(verify(&size, &trust), Err(AuditError::Limit)));
    let mut trailing = bytes;
    trailing.push(0);
    assert!(verify(&trailing, &trust).is_err());
    let mut duplicate = valid.encode_to_vec();
    duplicate.extend_from_slice(&[0x08, 1]);
    assert!(sign_raw(&duplicate, &trust, &key).is_err());
}
