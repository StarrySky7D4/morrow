use super::*;
use morrow_fs_directory_v1::{DirectoryEntry, EntryKind, NameEncoding};
use std::{cell::Cell, mem::size_of, ptr};

fn opened() -> Opened {
    Opened { selection_epoch: [3;32], page_sequence: 1, after_entry_id: None, entries: 1, metadata_bytes: 64 }
}
fn ready() -> ClientState {
    let mut state=ClientState::new([1;32]).unwrap();
    let request=state.begin([2;32],Action::Open).unwrap();
    let response=Response::new(&request,Reply::Opened(opened())).unwrap();
    state.accept(response.wire()).unwrap();
    state
}
fn page(sequence:u64,terminal:bool,entries:Vec<DirectoryEntry>)->FsDirectoryPage {
    FsDirectoryPage { selection_epoch:[3;32],page_sequence:sequence,terminal,entries }
}
fn entry()->DirectoryEntry {
    DirectoryEntry { entry_id:[4;32],kind:EntryKind::File,encoding:NameEncoding::Utf16Le,name:vec![0x34,0xd8,0x1e,0xdd],logical_length:Some(0) }
}
fn request_wire(version:u16,reserved:u32,action:u16)->Vec<u8> {
    let mut message=Builder::new_default();
    {let mut root=message.init_root::<wire::request::Builder<'_>>();
    root.set_version(version);root.set_action(action);root.set_reserved(reserved);
    root.set_schema_sha256(&schema_digest());root.set_nomination_ref(&[1;32]);root.set_nonce(&[2;32]);}
    serialize::write_message_to_words(&message)
}

#[test]
fn request_actions_and_cursor_boundaries() {
    for action in [Action::Open,Action::Next{selection_epoch:[3;32],page_sequence:1,after_entry_id:None},
        Action::Next{selection_epoch:[3;32],page_sequence:2,after_entry_id:None},
        Action::Next{selection_epoch:[3;32],page_sequence:u64::MAX,after_entry_id:Some([4;32])},
        Action::Finish{selection_epoch:[3;32]},Action::Cancel{selection_epoch:[3;32]}] {
        let q=Request::new([1;32],[2;32],action).unwrap();
        assert_eq!(Request::decode(q.wire()).unwrap(),q);
        assert!(q.wire().len()<=MAX_REQUEST_BYTES);
    }
    for action in [Action::Next{selection_epoch:[0;32],page_sequence:1,after_entry_id:None},
        Action::Next{selection_epoch:[3;32],page_sequence:0,after_entry_id:None},
        Action::Next{selection_epoch:[3;32],page_sequence:1,after_entry_id:Some([4;32])},
        Action::Next{selection_epoch:[3;32],page_sequence:2,after_entry_id:Some([0;32])}] {
        assert_eq!(Request::new([1;32],[2;32],action),Err(Error::Invalid));
    }
    assert_eq!(Request::new([0;32],[2;32],Action::Open),Err(Error::Invalid));
    assert_eq!(Request::new([1;32],[0;32],Action::Open),Err(Error::Invalid));
}

#[test]
fn strict_outer_contract_and_trailing_rejection() {
    assert_eq!(Request::decode(&request_wire(2,0,0)),Err(Error::Contract));
    assert_eq!(Request::decode(&request_wire(1,1,0)),Err(Error::Invalid));
    assert_eq!(Request::decode(&request_wire(1,0,77)),Err(Error::Invalid));
    let mut trailing=request_wire(1,0,0);trailing.extend([0;8]);
    assert_eq!(Request::decode(&trailing),Err(Error::Invalid));
    assert_eq!(Request::decode(&vec![0;MAX_REQUEST_BYTES+1]),Err(Error::Limit));
    let mut expanded=request_wire(1,0,0);
    let pointer=u64::from_le_bytes(expanded[8..16].try_into().unwrap());
    let pointer=pointer+(1u64<<48);
    expanded[8..16].copy_from_slice(&pointer.to_le_bytes());
    assert!(Request::decode(&expanded).is_err());
}

#[test]
fn original_multisegment_request_digest_and_one_call() {
    let mut message=Builder::new(capnp::message::HeapAllocator::new().first_segment_words(0));
    {let mut root=message.init_root::<wire::request::Builder<'_>>();root.set_version(1);
    root.set_schema_sha256(&schema_digest());root.set_nomination_ref(&[1;32]);root.set_nonce(&[2;32]);}
    let raw=serialize::write_message_to_words(&message);
    let q=Request::decode(&raw).unwrap();
    let canonical=Request::new([1;32],[2;32],Action::Open).unwrap();
    assert!(q.wire()!=canonical.wire());
    assert_eq!(q.digest(),<[u8;32]>::from(Sha256::digest(&raw)));
    let calls=Cell::new(0);
    let mut state=ClientState::new([1;32]).unwrap();
    let response=state.call_request_once_with(q.clone(),|input,output|{
        calls.set(calls.get()+1);assert_eq!(input,raw);assert_eq!(output.len(),MAX_RESPONSE_BYTES);
        let q=Request::decode(input).unwrap();let r=Response::new(&q,Reply::Opened(opened())).unwrap();
        output[..r.wire().len()].copy_from_slice(r.wire());r.wire().len() as i32
    }).unwrap();
    assert_eq!(calls.get(),1);assert_eq!(response.request().digest(),q.digest());
    assert_eq!(Response::decode_for(response.wire(),&canonical),Err(Error::Correlation));
}

#[test]
fn empty_nonterminal_page_remains_encodable_next_cursor() {
    let mut state=ready();
    let q=state.begin([5;32],state.next_action().unwrap()).unwrap();
    let r=Response::new(&q,Reply::Page(page(1,false,vec![]))).unwrap();
    state.accept(r.wire()).unwrap();
    assert_eq!(state.next_action().unwrap(),Action::Next{selection_epoch:[3;32],page_sequence:2,after_entry_id:None});
    let q=state.begin([6;32],state.next_action().unwrap()).unwrap();
    assert_eq!(Request::decode(q.wire()).unwrap().action(),q.action());
    let r=Response::new(&q,Reply::Page(page(2,true,vec![entry()]))).unwrap();
    state.accept(r.wire()).unwrap();assert_eq!(state.snapshot().phase,Phase::Terminal);
    assert_eq!(state.next_action(),Err(Error::State));
}

#[test]
fn nonempty_page_exact_cursor_and_foreign_identity_rejection() {
    let mut state=ready();let q=state.begin([5;32],state.next_action().unwrap()).unwrap();
    state.accept(Response::new(&q,Reply::Page(page(1,false,vec![entry()]))).unwrap().wire()).unwrap();
    let before=state.snapshot();
    for action in [Action::Next{selection_epoch:[9;32],page_sequence:2,after_entry_id:Some([4;32])},
        Action::Next{selection_epoch:[3;32],page_sequence:3,after_entry_id:Some([4;32])},
        Action::Next{selection_epoch:[3;32],page_sequence:2,after_entry_id:None}] {
        assert_eq!(state.begin([6;32],action),Err(Error::State));assert_eq!(state.snapshot(),before);
    }
    assert_eq!(state.next_action().unwrap(),Action::Next{selection_epoch:[3;32],page_sequence:2,after_entry_id:Some([4;32])});
}

#[test]
fn response_wrong_correlation_is_unknown_and_never_refunds() {
    for wrong in [Request::new([9;32],[2;32],Action::Open).unwrap(),
        Request::new([1;32],[9;32],Action::Open).unwrap()] {
        let mut state=ClientState::new([1;32]).unwrap();state.begin([2;32],Action::Open).unwrap();
        let bytes=Response::new(&wrong,Reply::Opened(opened())).unwrap();
        assert_eq!(state.accept(bytes.wire()),Err(Error::Correlation));
        let snapshot=state.snapshot();assert_eq!(snapshot.phase,Phase::Unknown);
        assert_eq!(snapshot.calls,1);assert!(snapshot.request_wire_bytes>0);
        assert_eq!(snapshot.response_wire_bytes,bytes.wire().len() as u64);
        assert_eq!(snapshot.resident_nonces,0);
        assert_eq!(state.begin([2;32],Action::Open),Err(Error::State));
        assert_eq!(state.snapshot(),snapshot);
    }
    let mut state=ClientState::new([1;32]).unwrap();
    let wrong=Request::new([1;32],[9;32],Action::Open).unwrap();
    let wire=Response::new(&wrong,Reply::Opened(opened())).unwrap().encode().unwrap();
    let count=Cell::new(0);
    assert_eq!(state.call_once_with([2;32],Action::Open,|_,out|{count.set(count.get()+1);out[..wire.len()].copy_from_slice(&wire);wire.len() as i32}),Err(Error::Correlation));
    assert_eq!(count.get(),1);assert_eq!(state.snapshot().phase,Phase::Unknown);
    assert_eq!(state.snapshot().response_wire_bytes,wire.len() as u64);
}

#[test]
fn finish_cancel_and_wire_unknown_are_terminal_without_join_claim() {
    for cancel in [false,true] {
        let mut state=ready();let action=if cancel{state.cancel_action()}else{state.finish_action()}.unwrap();
        let q=state.begin([5;32],action).unwrap();
        let reply=if cancel{Reply::Cancelled}else{Reply::Finished};
        state.accept(Response::new(&q,reply).unwrap().wire()).unwrap();
        assert_eq!(state.snapshot().phase,Phase::Terminal);
        assert_eq!(state.begin([6;32],Action::Open),Err(Error::State));
        assert_eq!(state.snapshot().calls,2);
    }
    for status in [Status::Denied,Status::Closed,Status::Busy,Status::Invalid,Status::Limit,Status::Unsupported,Status::OutcomeUnknown] {
        let mut state=ClientState::new([1;32]).unwrap();let q=state.begin([2;32],Action::Open).unwrap();
        state.accept(Response::new(&q,Reply::Error(status)).unwrap().wire()).unwrap();
        assert_eq!(state.snapshot().phase,if status==Status::OutcomeUnknown{Phase::Unknown}else{Phase::Terminal});
    }
}

#[test]
fn transport_loss_is_one_call_and_replay_is_blocked() {
    for written in [0,-1,65537] {
        let mut state=ClientState::new([1;32]).unwrap();let count=Cell::new(0);
        assert_eq!(state.call_once_with([2;32],Action::Open,|_,_|{count.set(count.get()+1);written}),Err(Error::OutcomeUnknown));
        assert_eq!(count.get(),1);assert_eq!(state.snapshot().phase,Phase::Unknown);
        assert_eq!(state.call_once_with([3;32],Action::Open,|_,_|{count.set(count.get()+1);0}),Err(Error::State));
        assert_eq!(count.get(),1);
    }
}

#[test]
fn owned_page_and_debug_do_not_expose_raw_names_or_tokens() {
    let q=Request::new([1;32],[5;32],Action::Next{selection_epoch:[3;32],page_sequence:1,after_entry_id:None}).unwrap();
    let response=Response::new(&q,Reply::Page(page(1,true,vec![entry()]))).unwrap();
    let mut raw=response.encode().unwrap();let decoded=Response::decode_for(&raw,&q).unwrap();
    raw.fill(0);assert_eq!(decoded,response);
    let debug=format!("{q:?} {decoded:?} {:?}",ready());
    assert!(!debug.contains("name_bytes"));assert!(!debug.contains("nomination_ref"));
    assert!(!debug.contains("selection_epoch"));assert!(!debug.contains("[1, 1, 1"));
    assert!(!debug.contains("52, 216"));assert!(!debug.contains("request_digest"));
}

#[test]
fn inner_page_limits_propagate_and_error_body_cannot_be_success() {
    let q=Request::new([1;32],[5;32],Action::Next{selection_epoch:[3;32],page_sequence:1,after_entry_id:None}).unwrap();
    let mut oversized=entry();oversized.name=vec![0;16386];
    assert_eq!(Response::new(&q,Reply::Page(page(1,true,vec![oversized]))),Err(Error::Limit));
    assert_eq!(Response::new(&q,Reply::Finished),Err(Error::Correlation));
    let open=Request::new([1;32],[2;32],Action::Open).unwrap();
    let response=Response::new(&open,Reply::Error(Status::Denied)).unwrap();
    assert!(matches!(Response::decode_for(response.wire(),&open).unwrap().reply(),Reply::Error(Status::Denied)));
    let mut trailing=response.encode().unwrap();trailing.extend([0;8]);
    assert_eq!(Response::decode_for(&trailing,&open),Err(Error::Invalid));
}

#[test]
fn nonce_reuse_and_pending_are_rejected_without_new_charge() {
    let mut state=ready();let before=state.snapshot();
    assert_eq!(state.begin([2;32],state.next_action().unwrap()),Err(Error::State));
    assert_eq!(state.snapshot(),before);
    let q=state.begin([5;32],state.next_action().unwrap()).unwrap();let pending=state.snapshot();
    assert_eq!(state.begin([6;32],q.action()),Err(Error::State));assert_eq!(state.snapshot(),pending);
    state.transport_unknown().unwrap();assert_eq!(state.snapshot().calls,2);
    assert_eq!(state.snapshot().request_wire_bytes,pending.request_wire_bytes);
    let mut bounded=ready();
    for sequence in 1..crate::state::MAX_CALLS {
        let mut nonce=[0x7a;32];nonce[..8].copy_from_slice(&sequence.to_le_bytes());
        let request=bounded.begin(nonce,bounded.next_action().unwrap()).unwrap();
        let reply=Response::new(&request,Reply::Page(page(sequence,false,vec![]))).unwrap();
        bounded.accept(reply.wire()).unwrap();
    }
    let paid=bounded.snapshot();assert_eq!(paid.calls,crate::state::MAX_CALLS);
    assert_eq!(paid.resident_nonces,crate::state::MAX_CALLS as usize);
    assert_eq!(bounded.begin([0x5f;32],bounded.next_action().unwrap()),Err(Error::Budget));
    assert_eq!(bounded.snapshot(),paid);
}

#[test]
fn c_opaque_outputs_preserve_slots_prefix_lengths_and_owned_wire() {
    use crate::ffi::*;
    let q=Request::new([1;32],[2;32],Action::Open).unwrap();
    let mut wire=q.encode().unwrap();let mut handle=ptr::null_mut();
    // SAFETY: all inputs/outputs below are live aligned bounded local objects.
    unsafe {
        assert_eq!(mdr_request_decode(wire.as_ptr(),wire.len() as u32,&mut handle),0);
        wire.fill(0);let mut output=[0xa5;512];let mut length=77;
        assert_eq!(mdr_request_encode(handle,output.as_mut_ptr(),1,&mut length),Error::Buffer as u32);
        assert_eq!(output,[0xa5;512]);assert_eq!(length,77);
        assert_eq!(mdr_request_encode(handle,output.as_mut_ptr(),512,&mut length),0);
        assert_eq!(&output[..length as usize],q.wire());
        assert!(output[length as usize..].iter().all(|v|*v==0xa5));
        let original=handle;
        assert_eq!(mdr_request_decode(ptr::null(),1,&mut handle),Error::Buffer as u32);
        assert_eq!(handle,original);
        let mut view=std::mem::MaybeUninit::<RequestView>::uninit();
        assert_eq!(mdr_request_get_view(handle,view.as_mut_ptr(),size_of::<RequestView>() as u32-1),Error::Buffer as u32);
        assert_eq!(mdr_request_get_view(handle,view.as_mut_ptr(),size_of::<RequestView>() as u32),0);
        assert_eq!(view.assume_init().nomination_ref,[1;32]);mdr_request_free(handle);
    }
}
