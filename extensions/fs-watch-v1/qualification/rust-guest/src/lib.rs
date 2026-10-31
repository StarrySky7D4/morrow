//! Raw task observation consumer only. No directory, path or host IO import.
#![deny(unsafe_op_in_unsafe_fn)]
use morrow_fs_watch_v1::{
    Batch, End, Error, MAX_WIRE_BYTES,
    state::{ClientState, Limits, Phase},
    wire_digest,
};
#[link(wasm_import_module = "morrow_task_v1")]
unsafe extern "C" {
    fn read_input(output: *mut u8, capacity: u32) -> i32;
    fn complete(input: *const u8, length: u32) -> i32;
}
fn consume() -> Result<i32, Error> {
    let mut input = vec![0; 131072];
    let length = unsafe { read_input(input.as_mut_ptr(), input.len() as u32) };
    if length <= 0 || length as usize > MAX_WIRE_BYTES {
        return Err(Error::Limit);
    }
    input.truncate(length as usize);
    let batch = Batch::decode(&input)?;
    let encoded = batch.encode()?;
    if encoded != input {
        return Err(Error::Contract);
    }
    if batch.sequence == 1 {
        let mut state = ClientState::new(
            batch.watch_epoch,
            Limits {
                frames: 2,
                changes: 512,
                wire_bytes: 131072,
            },
        )?;
        let accepted = state.receive(&input)?;
        let digest = wire_digest(&input);
        if state.usage().frames != 1
            || state.usage().wire_bytes != input.len() as u64
            || state.phase() != Phase::Pending
            || state.receive(&input) != Err(Error::Pending)
            || state.acknowledge(2, digest) != Err(Error::Correlation)
        {
            return Err(Error::State);
        }
        state.acknowledge(1, digest)?;
        state.transport_lost();
        let (phase, code) = if batch.end == End::Continue {
            (Phase::Unknown, Error::OutcomeUnknown)
        } else {
            (Phase::Ended(batch.end), Error::State)
        };
        if state.phase() != phase
            || state.acknowledge(1, digest) != Err(code)
            || state.usage().frames != 1
            || accepted != batch
        {
            return Err(Error::State);
        }
    }
    Ok(unsafe { complete(encoded.as_ptr(), encoded.len() as u32) })
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    consume().unwrap_or(-1)
}
