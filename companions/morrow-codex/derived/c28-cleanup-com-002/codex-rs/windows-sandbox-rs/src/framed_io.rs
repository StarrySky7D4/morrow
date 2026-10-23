//! Bounded byte transport. Serialization belongs to the typed protocol module.

use anyhow::Result;
use std::fs::File;
use std::io;
use std::io::Read;
use std::io::Write;
use std::os::windows::io::AsRawHandle;
use std::ptr;
use std::time::Duration;
use std::time::Instant;
use windows_sys::Win32::System::Pipes::PeekNamedPipe;

/// Bound the memory used by an individual untrusted IPC frame.
const MAX_FRAME_LEN: usize = 8 * 1024 * 1024;
const FRAME_POLL_INTERVAL: Duration = Duration::from_millis(5);

pub(crate) fn write_payload<W: Write>(mut writer: W, payload: &[u8], cap: usize) -> Result<()> {
    anyhow::ensure!(!payload.is_empty() && payload.len() <= cap && payload.len() <= u32::MAX as usize, "IPC frame size rejected");
    let len = payload.len() as u32;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(payload)?;
    writer.flush()?;
    Ok(())
}

pub(crate) fn read_payload<R: Read>(mut reader: R, cap: usize) -> Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    // Only an empty stream is EOF. Even one consumed prefix byte is a failed
    // frame, never physical completion evidence.
    loop {
        match reader.read(&mut len_buf[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err.into()),
        }
    }
    reader.read_exact(&mut len_buf[1..])?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > cap {
        anyhow::bail!("frame too large: {len}");
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload)?;
    Ok(Some(payload))
}

pub(crate) fn wait_for_complete_frame(pipe: &File, deadline: Instant) -> io::Result<()> {
    wait_for_complete_frame_with_cap(pipe,deadline,MAX_FRAME_LEN)
}

pub(crate) fn wait_for_complete_frame_with_cap(pipe:&File,deadline:Instant,max_frame_len:usize)->io::Result<()> {
    let mut len_buf = [0_u8; 4];

    loop {
        let mut bytes_read = 0_u32;
        let mut total_available = 0_u32;
        let ok = unsafe {
            PeekNamedPipe(
                pipe.as_raw_handle() as _,
                len_buf.as_mut_ptr().cast(),
                len_buf.len() as u32,
                &mut bytes_read,
                &mut total_available,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }

        if bytes_read == len_buf.len() as u32 {
            let frame_len = u32::from_le_bytes(len_buf) as usize;
            if frame_len > max_frame_len {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("frame too large: {frame_len}"),
                ));
            }
            if total_available as usize >= len_buf.len() + frame_len {
                return Ok(());
            }
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "timed out waiting for a complete IPC frame",
            ));
        }
        std::thread::sleep(remaining.min(FRAME_POLL_INTERVAL));
    }
}
