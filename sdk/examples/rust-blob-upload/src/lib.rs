//! Bounded deterministic sample producer, using the real SDK and fixed imports.
//! Planning calls only task read/complete for bounded synthetic bytes.
//! Upload accepts the exact bound proposal (or the earlier Descriptor example).
//! The host must independently nominate/approve the fresh transfer_epoch name.
use morrow_blob_transfer_v1::{Action, Frame};
use morrow_blob_upload_proposal_v1::{BOUND_INPUT_TYPE, BoundInput, OUTPUT_TYPE, Proposal};
use morrow_blob_upload_request_v1::{
    ActionRef, Client, ClientLimits, Descriptor, Phase, ReplyRef, wasm,
};
use morrow_blob_upload_sample::{
    INPUT_TYPE, LABEL, Options, TEXT_INPUT_TYPE, TEXT_LABEL, text_sha256,
};
use sha2::{Digest, Sha256};

fn run() -> Result<(), ()> {
    let task = morrow_plugin_sdk::wasm::read_task().map_err(|_| ())?;
    let transform = task.transform().ok_or(())?;
    if transform.handler == "blob.plan.text" {
        if transform.input_type != TEXT_INPUT_TYPE || transform.output_type != OUTPUT_TYPE {
            return Err(());
        }
        let hash = text_sha256(&transform.input).map_err(|_| ())?;
        let proposal = Proposal::new(
            TEXT_LABEL,
            transform.input.len() as u64,
            hash,
            &transform.input,
        )
        .map_err(|_| ())?;
        return morrow_plugin_sdk::wasm::complete_output(&task, proposal.wire()).map_err(|_| ());
    }
    if transform.handler == "blob.plan" {
        if transform.input_type != INPUT_TYPE || transform.output_type != OUTPUT_TYPE {
            return Err(());
        }
        let options = Options::decode(&transform.input).map_err(|_| ())?;
        let proposal = Proposal::new(
            LABEL,
            options.byte_length(),
            options.sha256(),
            &transform.input,
        )
        .map_err(|_| ())?;
        return morrow_plugin_sdk::wasm::complete_output(&task, proposal.wire()).map_err(|_| ());
    }
    if !matches!(
        transform.input_type.as_str(),
        "upload.descriptor.v1" | BOUND_INPUT_TYPE
    ) || transform.output_type != "upload.result.v1"
    {
        return Err(());
    }
    let (release, text) = match transform.handler.as_str() {
        "blob.upload" => (false, false),
        "blob.upload.release" => (true, false),
        "blob.upload.text" => (false, true),
        "blob.upload.text.release" => (true, true),
        _ => return Err(()),
    };
    let (descriptor, descriptor_wire, fill, body) = if transform.input_type == BOUND_INPUT_TYPE {
        let bound = BoundInput::decode(&transform.input).map_err(|_| ())?;
        let proposal = bound.proposal();
        let (fill, body) = if text {
            let bytes = proposal.producer_input();
            if bytes.len() as u64 != proposal.byte_length()
                || text_sha256(bytes).map_err(|_| ())? != proposal.whole_sha256()
            {
                return Err(());
            }
            (0, Some(bytes.to_vec()))
        } else {
            let options = Options::decode(proposal.producer_input()).map_err(|_| ())?;
            if options.byte_length() != proposal.byte_length() {
                return Err(());
            }
            (options.fill(), None)
        };
        let frame = Frame {
            identity: bound.identity(),
            action: Action::Descriptor {
                total_length: proposal.byte_length(),
                whole_sha256: proposal.whole_sha256(),
            },
        };
        let bytes = frame.encode().map_err(|_| ())?;
        (frame, bytes, fill, body)
    } else {
        if text {
            return Err(());
        }
        (
            Frame::decode(&transform.input).map_err(|_| ())?,
            transform.input.clone(),
            42,
            None,
        )
    };
    let Action::Descriptor {
        total_length,
        whole_sha256,
    } = descriptor.action
    else {
        return Err(());
    };
    let mut client = Client::new(
        descriptor.identity.transfer_epoch,
        Descriptor {
            identity: descriptor.identity,
            byte_length: total_length,
            sha256: whole_sha256,
        },
        ClientLimits::default(),
    )
    .map_err(|_| ())?;
    let mut calls = 0_u64;
    let mut call = |action: ActionRef<'_>| {
        calls += 1;
        let mut nonce = [29; 32];
        nonce[..8].copy_from_slice(&calls.to_le_bytes());
        wasm::call_once(&mut client, nonce, action).map_err(|_| ())
    };
    let opened = call(ActionRef::Open)?;
    let ReplyRef::State(state) = opened.reply() else {
        return Err(());
    };
    if state.phase == Phase::Ready && !release {
        call(ActionRef::Push(&descriptor_wire))?;
        let mut offset = 0_u64;
        let mut sequence = 1_u64;
        while offset < total_length {
            let length = (total_length - offset).min(60 * 1024) as usize;
            let payload = match &body {
                Some(bytes) => bytes
                    .get(offset as usize..offset as usize + length)
                    .ok_or(())?
                    .to_vec(),
                None => vec![fill; length],
            };
            let chunk_sha256 = Sha256::digest(&payload).into();
            let frame = Frame {
                identity: descriptor.identity,
                action: Action::Chunk {
                    sequence,
                    offset,
                    payload,
                    chunk_sha256,
                },
            }
            .encode()
            .map_err(|_| ())?;
            call(ActionRef::Push(&frame))?;
            offset += length as u64;
            sequence += 1;
        }
        let end = Frame {
            identity: descriptor.identity,
            action: Action::End {
                total_length,
                whole_sha256,
            },
        }
        .encode()
        .map_err(|_| ())?;
        call(ActionRef::Push(&end))?;
    }
    let response = call(if release {
        ActionRef::Release
    } else {
        ActionRef::Inspect
    })?;
    let ReplyRef::State(state) = response.reply() else {
        return Err(());
    };
    if state.phase
        != if release {
            Phase::Released
        } else {
            Phase::Staged
        }
    {
        return Err(());
    }
    wasm::complete_response(&response).map_err(|_| ())
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    if run().is_ok() { 0 } else { 1 }
}
