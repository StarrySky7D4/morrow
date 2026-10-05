use morrow_blob_transfer_v1::{
    Acceptance, Action, ActionRef, Frame, FrameRef, Limits, Phase, ReceiptStatus, Receiver,
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
fn corpus() -> Vec<(String, bool, Vec<u8>, Frame)> {
    let b = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vectors.bin")).unwrap();
    assert_eq!(&b[..8], b"BLCV0001");
    let mut p = 8;
    let count = u32_at(&b, &mut p);
    let mut out = Vec::new();
    for _ in 0..count {
        let nl = u32_at(&b, &mut p) as usize;
        let ok = u32_at(&b, &mut p) != 0;
        let wl = u32_at(&b, &mut p) as usize;
        let kind = u32_at(&b, &mut p);
        let pl = u32_at(&b, &mut p) as usize;
        let status = u32_at(&b, &mut p);
        let sequence = u64_at(&b, &mut p);
        let offset = u64_at(&b, &mut p);
        let total_length = u64_at(&b, &mut p);
        assert_eq!(u64_at(&b, &mut p), 0);
        let hash = data(&b, &mut p, 32).try_into().unwrap();
        let name = String::from_utf8(data(&b, &mut p, nl).to_vec()).unwrap();
        let payload = data(&b, &mut p, pl).to_vec();
        let wire = data(&b, &mut p, wl).to_vec();
        let action = match kind {
            0 => Action::Descriptor {
                total_length,
                whole_sha256: hash,
            },
            1 => Action::Chunk {
                sequence,
                offset,
                payload,
                chunk_sha256: hash,
            },
            2 => Action::Receipt {
                request_digest: [9; 32],
                sequence,
                offset,
                length: total_length,
                chunk_sha256: hash,
                status: if status == 0 {
                    ReceiptStatus::Accepted
                } else {
                    ReceiptStatus::Existing
                },
            },
            3 => Action::End {
                total_length,
                whole_sha256: hash,
            },
            _ => unreachable!(),
        };
        out.push((
            name,
            ok,
            wire,
            Frame {
                identity: morrow_blob_transfer_v1::Identity {
                    transfer_epoch: [1; 32],
                    object_ref: [2; 32],
                    operation_id: [3; 32],
                },
                action,
            },
        ));
    }
    assert_eq!(p, b.len());
    out
}
fn named(name: &str) -> Vec<u8> {
    corpus().into_iter().find(|r| r.0 == name).unwrap().2
}
#[test]
fn independent_schema_corpus_complete_semantics_ownership_and_unaligned_borrow() {
    let rows = corpus();
    for (name, ok, wire, want) in rows {
        let result = Frame::decode(&wire);
        assert_eq!(result.is_ok(), ok, "{name} {result:?}");
        let mut unaligned = vec![0xff];
        unaligned.extend_from_slice(&wire);
        assert_eq!(
            FrameRef::decode(&unaligned[1..]).is_ok(),
            ok,
            "unaligned {name}"
        );
        if ok {
            let got = result.unwrap();
            assert_eq!(got, want, "semantic {name}");
            let borrowed = FrameRef::decode(&wire).unwrap();
            if let ActionRef::Chunk { payload, .. } = borrowed.action {
                if !payload.is_empty() {
                    let s = payload.as_ptr() as usize;
                    let lo = wire.as_ptr() as usize;
                    assert!(s >= lo && s + payload.len() <= lo + wire.len());
                }
            }
            let canonical = got.encode().unwrap();
            assert_eq!(Frame::decode(&canonical).unwrap(), want);
            let mut overwritten = wire;
            overwritten.fill(0xaa);
            assert_eq!(got, want);
        }
    }
}
#[test]
fn receiver_actual_bytes_exact_wire_duplicate_and_noncanonical_retry() {
    let d = named("action-0");
    let c = named("action-1");
    let end = named("action-3");
    let mut r = Receiver::new(Limits::default()).unwrap();
    assert_eq!(r.accept(&d).unwrap(), Acceptance::DescriptorAccepted);
    let first = r.accept(&c).unwrap();
    let Acceptance::Receipt(first) = first else {
        panic!()
    };
    let Action::Receipt {
        status,
        request_digest,
        ..
    } = first.action
    else {
        panic!()
    };
    assert_eq!(status, ReceiptStatus::Accepted);
    assert_ne!(request_digest, [0; 32]);
    let before = r.snapshot();
    let duplicate = r.accept(&c).unwrap();
    let Acceptance::Receipt(duplicate) = duplicate else {
        panic!()
    };
    let Action::Receipt { status, .. } = duplicate.action else {
        panic!()
    };
    assert_eq!(status, ReceiptStatus::Existing);
    let after = r.snapshot();
    assert_eq!(after.received_bytes, 3);
    assert_eq!(after.next_sequence, 2);
    assert!(
        after.request_bytes > before.request_bytes && after.response_bytes > before.response_bytes
    );
    assert_eq!(&c[..4], &[0; 4]);
    let mut differently_encoded = 1u32.to_le_bytes().to_vec();
    differently_encoded.extend_from_slice(&c[4..8]);
    differently_encoded.extend_from_slice(&[0; 8]);
    differently_encoded.extend_from_slice(&c[8..]);
    assert_eq!(
        Frame::decode(&differently_encoded).unwrap(),
        Frame::decode(&c).unwrap()
    );
    assert!(r.accept(&differently_encoded).is_err());
    assert_eq!(r.snapshot().received_bytes, 3);
    assert!(matches!(
        r.accept(&end).unwrap(),
        Acceptance::VerifiedBytes {
            total_length: 3,
            ..
        }
    ));
    assert_eq!(r.snapshot().phase, Phase::VerifiedBytes);
    assert_eq!(r.snapshot().retained_recent_requests, 0);
    assert!(r.accept(&c).is_err());
}
#[test]
fn decoder_failure_charges_wire_without_advancing_or_refunding() {
    let mut r = Receiver::new(Limits::default()).unwrap();
    let d = named("action-0");
    r.accept(&d).unwrap();
    let before = r.snapshot();
    assert!(r.accept(&named("wrong-chunk-sha")).is_err());
    let after = r.snapshot();
    assert_eq!(after.received_bytes, before.received_bytes);
    assert_eq!(after.next_sequence, before.next_sequence);
    assert!(after.request_bytes > before.request_bytes);
    r.cancel();
    let cancelled = r.snapshot();
    assert_eq!(cancelled.phase, Phase::Cancelled);
    assert_eq!(cancelled.request_bytes, after.request_bytes);
    assert!(r.accept(&named("action-1")).is_err());
    assert_eq!(r.snapshot(), cancelled);
}
#[test]
fn response_budget_refuses_payload_commit_and_retained_receipt() {
    let mut r = Receiver::new(Limits {
        max_response_bytes: 0,
        ..Limits::default()
    })
    .unwrap();
    r.accept(&named("action-0")).unwrap();
    assert!(r.accept(&named("action-1")).is_err());
    let s = r.snapshot();
    assert_eq!(s.received_bytes, 0);
    assert_eq!(s.next_sequence, 1);
    assert_eq!(s.receipts, 0);
    assert_eq!(s.retained_recent_requests, 0);
    assert!(s.request_bytes > 0);
}
