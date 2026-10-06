//! Dedicated R2 frame route; old native handshakes and capability bits remain separate.
use crate::ApprovedSession;
use morrow_agent_session_exec_v1_r2::{
    Error, MAX_FRAME_BYTES, Reply, Request, Result, authority::SessionExecHost,
};
use morrow_core::dispatch::HostRuntime;
pub fn exchange(
    runtime: &mut HostRuntime,
    host: &SessionExecHost,
    approved: &ApprovedSession,
    bytes: &[u8],
    clock: impl FnMut() -> u64,
) -> Result<Vec<u8>> {
    let request = Request::decode(bytes)?;
    let reply = host.dispatch(
        runtime,
        &approved.connection,
        &approved.admission,
        bytes,
        clock,
    )?;
    Reply::decode_for(&request, &reply)?;
    Ok(reply)
}
pub fn exchange_frame(
    runtime: &mut HostRuntime,
    host: &SessionExecHost,
    approved: &ApprovedSession,
    frame: &[u8],
    clock: impl FnMut() -> u64,
) -> Result<Vec<u8>> {
    if frame.len() < 4 || frame.len() > MAX_FRAME_BYTES + 4 {
        return Err(Error::Limit);
    }
    let len = u32::from_le_bytes(frame[..4].try_into().unwrap()) as usize;
    if len == 0 || len > MAX_FRAME_BYTES || len != frame.len() - 4 {
        return Err(Error::Invalid);
    }
    let reply = exchange(runtime, host, approved, &frame[4..], clock)?;
    let mut output = Vec::with_capacity(reply.len() + 4);
    output.extend_from_slice(&(reply.len() as u32).to_le_bytes());
    output.extend_from_slice(&reply);
    Ok(output)
}
