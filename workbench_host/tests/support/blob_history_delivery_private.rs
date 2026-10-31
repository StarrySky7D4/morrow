use super::*;
use capnp::{
    message::{Builder, Reader, ReaderOptions},
    serialize,
};
use morrow_workbench_host::{host_capnp as wire, protocol};
use std::io::Cursor;

fn frame(action: wire::Action, fill: impl FnOnce(wire::request::Builder<'_>)) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut request = message.init_root::<wire::request::Builder>();
    request.set_version(1);
    request.set_digest(&protocol::digest());
    request.set_action(action);
    fill(request);
    serialize::write_message_to_words(&message)
}

fn exchange(f: &mut Fixture, bytes: &[u8]) -> Reader<serialize::OwnedSegments> {
    let bytes = protocol::respond(&mut f.w, bytes).unwrap();
    serialize::read_message(&mut Cursor::new(bytes), ReaderOptions::new()).unwrap()
}

fn fields(value: &DeliveryRequest, mut out: wire::blob_history_delivery_request::Builder<'_>) {
    out.set_request_id(&value.request_id);
    out.set_submission(&value.submission);
    out.set_package_id(value.package_id.as_str());
    out.set_package_digest(&value.package_digest);
    out.set_registry_revision(value.registry_revision);
    out.set_handler(value.handler.as_str());
    out.set_output_type(value.output_type.as_str());
    out.set_timeout_ms(value.lifetime.as_millis() as u32);
}

fn keyed(action: wire::Action, key: &[u8]) -> Vec<u8> {
    frame(action, |mut out| out.set_blob_history_delivery_key(key))
}

fn approve(p: &DeliveryPreview) -> Vec<u8> {
    frame(wire::Action::BlobHistoryDeliveryApprove, |mut out| {
        out.set_blob_history_delivery_key(p.key.as_bytes());
        let mut a = out.init_blob_history_delivery_approval();
        a.set_packet_sha256(&p.packet_sha256);
        a.set_package_digest(&p.request.package_digest);
        a.set_registry_revision(p.request.registry_revision);
    })
}

fn ok(response: &Reader<serialize::OwnedSegments>) -> wire::response::Reader<'_> {
    let row = response.get_root::<wire::response::Reader>().unwrap();
    assert_eq!(row.get_version(), 1);
    assert_eq!(row.get_digest().unwrap(), protocol::digest());
    assert!(row.get_error().unwrap().is_empty(), "{:?}", row.get_error());
    assert!(row.has_blob_history_delivery_preview() && row.has_blob_history_delivery_receipt());
    row
}

#[test]
fn private_capnp_prepares_previews_delivers_once_and_recovers_original_receipt() {
    use wire::Action::*;
    let mut f = Fixture::new("private-capnp", false);
    let r = f.request(1);
    let bytes = frame(BlobHistoryDeliveryPrepare, |out| {
        fields(&r, out.init_blob_history_delivery_request())
    });
    let reply = exchange(&mut f, &bytes);
    let preview = ok(&reply).get_blob_history_delivery_preview().unwrap();
    assert!(preview.get_present());
    assert!(
        !ok(&reply)
            .get_blob_history_delivery_receipt()
            .unwrap()
            .get_found()
    );
    let key = DeliveryKey::from_bytes(preview.get_key().unwrap()).unwrap();
    let p = f.w.preview_blob_history_delivery(key).unwrap();
    assert_eq!(preview.get_packet().unwrap(), p.packet);
    assert_eq!(preview.get_task_id().unwrap().to_str().unwrap(), p.task_id);
    assert_eq!(preview.get_packet_sha256().unwrap(), p.packet_sha256);
    let again = exchange(&mut f, &keyed(BlobHistoryDeliveryPreview, key.as_bytes()));
    assert_eq!(
        ok(&again)
            .get_blob_history_delivery_preview()
            .unwrap()
            .get_packet()
            .unwrap(),
        p.packet
    );

    // Losing this response cannot justify resubmission. The next action reads
    // this same retained original task and its actual close result.
    let _lost_reply = exchange(&mut f, &approve(&p));
    let read = exchange(&mut f, &keyed(BlobHistoryDeliveryRead, key.as_bytes()));
    let receipt = ok(&read).get_blob_history_delivery_receipt().unwrap();
    assert!(
        receipt.get_found()
            && receipt.get_consumed()
            && receipt.get_dispatched()
            && receipt.get_finished()
    );
    assert_eq!(receipt.get_close(), 2);
    assert_eq!(receipt.get_close_failure_kind(), 0);
    assert_eq!(receipt.get_error(), 0);
    assert_eq!(receipt.get_delivery_error(), 0);
    assert!(receipt.get_has_execution() && receipt.get_has_output());
    assert_eq!(receipt.get_execution_fault(), 0);
    assert_eq!(receipt.get_host_calls(), 0);
    assert_eq!(
        receipt.get_output_type().unwrap().to_str().unwrap(),
        INPUT_TYPE
    );
    assert_eq!(receipt.get_output().unwrap(), p.packet);
    let duplicate = exchange(&mut f, &approve(&p));
    assert!(
        !duplicate
            .get_root::<wire::response::Reader>()
            .unwrap()
            .get_error()
            .unwrap()
            .is_empty()
    );
    let discover = frame(BlobHistoryDeliveryDiscover, |mut out| {
        out.set_blob_history_delivery_request_id(&r.request_id)
    });
    let original = exchange(&mut f, &discover);
    assert_eq!(
        ok(&original)
            .get_blob_history_delivery_receipt()
            .unwrap()
            .get_output()
            .unwrap(),
        p.packet
    );
    let ack = exchange(
        &mut f,
        &keyed(BlobHistoryDeliveryAcknowledge, key.as_bytes()),
    );
    assert!(
        ok(&ack)
            .get_blob_history_delivery_receipt()
            .unwrap()
            .get_acknowledged()
    );
    let next = f.preview(2);
    let cancelled = exchange(
        &mut f,
        &keyed(BlobHistoryDeliveryCancel, next.key.as_bytes()),
    );
    let receipt = ok(&cancelled).get_blob_history_delivery_receipt().unwrap();
    assert_eq!(receipt.get_error(), 1);
    assert_eq!(receipt.get_close(), 0);
    assert!(!receipt.get_dispatched());
    assert!(!receipt.get_has_execution() && !receipt.get_has_output());
    let absent = frame(BlobHistoryDeliveryDiscover, |mut out| {
        out.set_blob_history_delivery_request_id(&[9; 32])
    });
    let absent = exchange(&mut f, &absent);
    assert!(
        !ok(&absent)
            .get_blob_history_delivery_receipt()
            .unwrap()
            .get_found()
    );
    f.finish();
}

#[test]
fn private_capnp_refuses_mixed_fields_bad_ids_missing_envelopes_and_wrong_contract() {
    use wire::Action::*;
    let mut f = Fixture::new("private-rejections", false);
    let r = f.request(1);
    let valid = || {
        frame(BlobHistoryDeliveryPrepare, |out| {
            fields(&r, out.init_blob_history_delivery_request())
        })
    };
    let mut bad = Vec::new();
    for action in [
        BlobHistoryDeliveryPrepare,
        BlobHistoryDeliveryPreview,
        BlobHistoryDeliveryApprove,
        BlobHistoryDeliveryRead,
        BlobHistoryDeliveryCancel,
        BlobHistoryDeliveryAcknowledge,
        BlobHistoryDeliveryDiscover,
    ] {
        bad.push(frame(action, |_| {}));
    }
    for mutation in 0..8 {
        bad.push(frame(BlobHistoryDeliveryPrepare, |mut out| {
            fields(&r, out.reborrow().init_blob_history_delivery_request());
            match mutation {
                0 => out.set_id("unrelated"),
                1 => out.set_payload(&[]), // Even an explicitly present empty pointer is unrelated.
                2 => out.set_revision(99),
                3 => out.set_blob_history_delivery_key(&[1; 32]),
                4 => {
                    out.init_blob_history_delivery_approval();
                }
                5 => out.set_digest(&[7; 32]),
                6 => out.set_version(2),
                _ => out
                    .get_blob_history_delivery_request()
                    .unwrap()
                    .set_timeout_ms(120_001),
            }
        }));
    }
    for id in [vec![], vec![1; 31], vec![1; 33], vec![0; 32]] {
        bad.push(keyed(BlobHistoryDeliveryRead, &id));
        bad.push(frame(BlobHistoryDeliveryPrepare, |mut out| {
            fields(&r, out.reborrow().init_blob_history_delivery_request());
            out.get_blob_history_delivery_request()
                .unwrap()
                .set_request_id(&id);
        }));
    }
    bad.push(frame(Read, |out| {
        fields(&r, out.init_blob_history_delivery_request())
    }));
    let mut trailing = valid();
    trailing.extend_from_slice(&[0; 8]);
    bad.push(trailing);
    let mut truncated = valid();
    truncated.truncate(truncated.len() - 8);
    bad.push(truncated);
    for bytes in bad {
        let reply = exchange(&mut f, &bytes);
        let row = reply.get_root::<wire::response::Reader>().unwrap();
        assert!(!row.get_error().unwrap().is_empty());
        assert!(
            !row.has_blob_history_delivery_preview() && !row.has_blob_history_delivery_receipt()
        );
        assert!(
            f.w.discover_blob_history_delivery(&r.request_id)
                .unwrap()
                .is_none()
        );
        f.unchanged();
    }
    // Rejected frames did not consume the valid original request identity.
    let accepted = exchange(&mut f, &valid());
    let key = ok(&accepted)
        .get_blob_history_delivery_preview()
        .unwrap()
        .get_key()
        .unwrap()
        .to_vec();
    let cancelled = exchange(&mut f, &keyed(BlobHistoryDeliveryCancel, &key));
    assert_eq!(
        ok(&cancelled)
            .get_blob_history_delivery_receipt()
            .unwrap()
            .get_close(),
        0
    );
    f.finish();
}

#[test]
fn private_capnp_gate_veto_keeps_execution_but_withholds_guest_output() {
    let mut f = Fixture::new("private-veto", false);
    let p = f.preview(1);
    let original = exchange(&mut f, &approve(&p));
    assert!(
        ok(&original)
            .get_blob_history_delivery_receipt()
            .unwrap()
            .get_has_output()
    );
    f.w.product_gate().revoke();
    let after = exchange(
        &mut f,
        &keyed(wire::Action::BlobHistoryDeliveryRead, p.key.as_bytes()),
    );
    let receipt = ok(&after).get_blob_history_delivery_receipt().unwrap();
    assert_eq!(receipt.get_delivery_error(), 6);
    assert_eq!(receipt.get_error(), 0);
    assert_eq!(receipt.get_close(), 2);
    assert!(receipt.get_has_execution());
    assert!(!receipt.get_has_output() && !receipt.get_has_plugin_failure());
    assert!(receipt.get_output().unwrap().is_empty());
    f.w.finish().unwrap();
}

pub(super) fn retained_close_reply(f: &mut Fixture, key: DeliveryKey, trap: bool) {
    let reply = exchange(
        f,
        &keyed(wire::Action::BlobHistoryDeliveryRead, key.as_bytes()),
    );
    let row = ok(&reply).get_blob_history_delivery_receipt().unwrap();
    assert_eq!(row.get_close(), 3);
    assert_eq!(row.get_close_failure_kind(), 1);
    assert_eq!(row.get_close_core_error(), 2);
    assert_eq!(
        row.get_close_detail().unwrap().to_str().unwrap(),
        "stale instance"
    );
    assert_eq!(row.get_error(), if trap { 8 } else { 12 });
    assert_eq!(row.get_execution_fault(), if trap { 9 } else { 0 });
    assert!(row.get_has_execution() && !row.get_has_output());
}

pub(super) fn retained_unwind_reply(f: &mut Fixture, key: DeliveryKey, trap: bool) {
    let reply = exchange(
        f,
        &keyed(wire::Action::BlobHistoryDeliveryRead, key.as_bytes()),
    );
    let row = ok(&reply).get_blob_history_delivery_receipt().unwrap();
    assert_eq!(row.get_close(), 4);
    assert_eq!(row.get_close_failure_kind(), 2);
    assert_eq!(row.get_close_core_error(), 0);
    assert!(row.get_close_detail().unwrap().is_empty());
    assert_eq!(row.get_error(), if trap { 8 } else { 12 });
    assert_eq!(row.get_execution_fault(), if trap { 9 } else { 0 });
    assert!(row.get_has_execution() && !row.get_has_output());
}
