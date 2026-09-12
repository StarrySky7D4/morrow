//! Trusted single-offer reader. Never runs native guest code or forwards received handles.
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use morrow_core::shared_transfer::{MAX_FRAME_BYTES, Offer, Reply};
    use std::io::{Read, Write};
    let mut input = std::io::stdin().lock();
    let mut header = [0; 4];
    input.read_exact(&mut header)?;
    let size = u32::from_le_bytes(header) as usize;
    if size == 0 || size > MAX_FRAME_BYTES {
        return Err("offer size".into());
    }
    let mut bytes = vec![0; size];
    input.read_exact(&mut bytes)?;
    let offer = Offer::decode(&bytes)?;
    let len = usize::try_from(offer.descriptor.length)?;
    let (payload, write_rejected) =
        morrow_plugin_runtime::shared_memory::copy_received_readonly(offer.remote_handle, len)?;
    let reply = Reply {
        transfer: offer.transfer,
        descriptor: offer.descriptor,
        payload,
        write_rejected,
    }
    .encode()?;
    let mut output = std::io::stdout().lock();
    output.write_all(&(reply.len() as u32).to_le_bytes())?;
    output.write_all(&reply)?;
    output.flush()?;
    // A reply is not release. Keep the original duplicated handle alive until the parent closes input.
    let mut trailing = [0];
    if input.read(&mut trailing)? != 0 {
        return Err("unexpected second request".into());
    }
    Ok(())
}
#[cfg(not(windows))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("Windows shared reader backend unavailable".into())
}
