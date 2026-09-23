use super::*;
use prost::Message;

fn slot_with_asset(asset_bytes: u64, evidence_bytes: u64) -> proto::Slot {
    proto::Slot {
        schema_version: 1,
        parent_link: None,
        retirement: None,
        predecessor_evidence_bytes: evidence_bytes,
        assets: vec![proto::StoredAsset {
            selection: None,
            pin_id: "draft-asset-0".into(),
            display_name: "selected".into(),
            media_type: "application/octet-stream".into(),
            byte_length: asset_bytes,
            sha256: vec![7; 32],
        }],
        ..Default::default()
    }
}

#[test]
fn charge_includes_encoded_metadata_evidence_and_selected_asset_lengths() {
    // Only lengths are represented; no blob payload is allocated.
    let mut slot = slot_with_asset(23 * 1024 * 1024, 11 * 1024 * 1024);
    slot.active_bytes = 17; // A caller-supplied accounting field is ignored.
    let actual = charge(&slot).expect("within active budget");
    slot.active_bytes = actual;
    assert_eq!(
        actual,
        slot.encoded_len() as u64 + slot.predecessor_evidence_bytes + slot.assets[0].byte_length
    );
}

#[test]
fn active_budget_accepts_exact_boundary_and_rejects_next_byte() {
    let mut slot = slot_with_asset(1, 0);
    // At this magnitude, moving evidence by the small metadata overhead does
    // not change its varint width. Include the final accounting field's width.
    slot.active_bytes = MAX_ACTIVE_BYTES;
    slot.predecessor_evidence_bytes = MAX_ACTIVE_BYTES;
    slot.predecessor_evidence_bytes = MAX_ACTIVE_BYTES - slot.encoded_len() as u64 - 1;
    assert_eq!(
        slot.encoded_len() as u64 + slot.predecessor_evidence_bytes + 1,
        MAX_ACTIVE_BYTES
    );
    slot.active_bytes = 0;
    assert_eq!(charge(&slot).unwrap(), MAX_ACTIVE_BYTES);

    slot.predecessor_evidence_bytes += 1;
    assert!(charge(&slot).is_err());
}

#[test]
fn charge_rejects_u64_overflow() {
    let slot = slot_with_asset(1, u64::MAX);
    assert!(charge(&slot).is_err());

    let slot = slot_with_asset(u64::MAX, 1);
    assert!(charge(&slot).is_err());
}
