//! Scoped one-shot native SDK adapters. Callback pointers never cross IPC.
use crate::Result;
use morrow_linux_supervisor_foundation::{CapabilityTransport, FrameKind, FrameRead};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    time::{Duration, Instant},
};
pub const CHANNEL: u8 = 0;
pub const RUNTIME: u8 = 1;
pub const TRANSFORM: u8 = 2;
pub const PAUSE: u8 = 3;
struct Session {
    peer: CapabilityTransport,
    start: Instant,
    last_pulse: Duration,
    submissions: u32,
}
pub struct Adapter {
    session: RefCell<Session>,
    context: u32,
    in_callback: Cell<bool>,
    local: PhantomData<*mut ()>,
}
struct CallbackGuard<'a>(&'a Cell<bool>);
impl Drop for CallbackGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
impl Adapter {
    pub fn new(peer: CapabilityTransport) -> Self {
        Self {
            session: RefCell::new(Session {
                peer,
                start: Instant::now(),
                last_pulse: Duration::ZERO,
                submissions: 0,
            }),
            context: std::process::id(),
            in_callback: Cell::new(false),
            local: PhantomData,
        }
    }
    pub fn initial_directory(&self) -> Result<Vec<u8>> {
        let mut session = self.session.try_borrow_mut()?;
        let Session {
            peer,
            start,
            last_pulse,
            ..
        } = &mut *session;
        while start.elapsed() < Duration::from_secs(4) {
            let now = start.elapsed();
            peer.poll_write(now)?;
            match peer.poll_read(now)? {
                FrameRead::Frame(frame) if frame.kind() == FrameKind::Status => {
                    peer.queue(FrameKind::Heartbeat, &[], now)?;
                    peer.poll_write(now)?;
                    *last_pulse = now;
                    return Ok(frame.payload().to_vec());
                }
                FrameRead::Pending => (),
                _ => return Err("native bootstrap failed".into()),
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Err("bounded SDK bootstrap deadline".into())
    }
    fn exchange(&self, tag: u8, input: &[u8], capacity: usize) -> Result<Vec<u8>> {
        if self.context != std::process::id() || self.in_callback.replace(true) {
            return Err("changed/reentrant SDK adapter".into());
        }
        let _guard = CallbackGuard(&self.in_callback);
        if input.is_empty() || input.len() >= 128 * 1024 {
            return Err("input bounds".into());
        }
        let mut session = self.session.try_borrow_mut()?;
        let Session {
            peer,
            start,
            last_pulse,
            submissions,
        } = &mut *session;
        let mut body = Vec::with_capacity(input.len() + 1);
        body.push(tag);
        body.extend_from_slice(input);
        let submitted = start.elapsed();
        peer.queue(FrameKind::Data, &body, submitted)?;
        *submissions = submissions.checked_add(1).ok_or("submission counter")?;
        while start.elapsed().saturating_sub(submitted) < Duration::from_secs(3) {
            let now = start.elapsed();
            peer.poll_write(now)?;
            // Global session pulse time is preserved across calls; short continuous calls
            // cannot keep resetting a per-call heartbeat timer until the lease expires.
            if now.saturating_sub(*last_pulse) >= Duration::from_millis(80) {
                peer.queue(FrameKind::Heartbeat, &[], now)?;
                *last_pulse = now;
            }
            match peer.poll_read(now)? {
                FrameRead::Frame(frame) if frame.kind() == FrameKind::Reply => {
                    if frame.payload().is_empty() || frame.payload().len() > capacity {
                        peer.retire();
                        return Err("native output bounds".into());
                    }
                    return Ok(frame.payload().to_vec());
                }
                FrameRead::Frame(frame)
                    if frame.kind() == FrameKind::Status && frame.payload() == b"worker-held" =>
                {
                    println!(
                        "actual-worker-held=true;native-driver-still-decoding-heartbeats=true"
                    );
                }
                FrameRead::Frame(_) | FrameRead::Eof => {
                    peer.retire();
                    return Err("native one-shot call failed; outcome Unknown".into());
                }
                FrameRead::Pending => (),
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        peer.retire();
        Err("native call deadline; outcome Unknown; no automatic replay".into())
    }
    /// Standalone negative fixture: authenticated Data with an invalid SDK body.
    pub fn malformed_call_fixture(&self) -> Result<()> {
        let mut session = self.session.try_borrow_mut()?;
        let now = session.start.elapsed();
        session.peer.queue(FrameKind::Data, &[CHANNEL], now)?;
        session.peer.poll_write(now)?;
        Ok(())
    }
    pub fn submissions(&self) -> u32 {
        self.session.borrow().submissions
    }
    pub fn heartbeat_until_collected(&self) -> Result<()> {
        let mut session = self.session.try_borrow_mut()?;
        let Session {
            peer,
            start,
            last_pulse,
            ..
        } = &mut *session;
        let begin = start.elapsed();
        while start.elapsed().saturating_sub(begin) < Duration::from_secs(4) {
            let now = start.elapsed();
            if now.saturating_sub(*last_pulse) >= Duration::from_millis(80) {
                peer.queue(FrameKind::Heartbeat, &[], now)?;
                *last_pulse = now;
            }
            peer.poll_write(now)?;
            std::thread::sleep(Duration::from_millis(1));
        }
        Err("original C not collected by G".into())
    }
}
fn disjoint(a: usize, n: usize, b: usize, m: usize) -> bool {
    a.checked_add(n)
        .zip(b.checked_add(m))
        .is_some_and(|(end_a, end_b)| end_a <= b || end_b <= a)
}
unsafe fn callback(
    context: *mut c_void,
    input: *const u8,
    length: u32,
    output: *mut u8,
    capacity: u32,
    written: *mut u32,
    tag: u8,
    max: usize,
) -> u32 {
    // Only the live, fixed Box constructed by the scoped caller is accepted under
    // the SDK's unsafe lifetime contract. These range checks do not validate an
    // arbitrary native address and promise no malicious-native-code isolation.
    if context.is_null()
        || (context as usize) % std::mem::align_of::<Adapter>() != 0
        || input.is_null()
        || output.is_null()
        || written.is_null()
        || length == 0
        || length as usize > max
        || capacity as usize != max
        || (written as usize) % std::mem::align_of::<u32>() != 0
        || !disjoint(
            input as usize,
            length as usize,
            output as usize,
            capacity as usize,
        )
        || !disjoint(input as usize, length as usize, written as usize, 4)
        || !disjoint(output as usize, capacity as usize, written as usize, 4)
        || !disjoint(
            context as usize,
            size_of::<Adapter>(),
            input as usize,
            length as usize,
        )
        || !disjoint(
            context as usize,
            size_of::<Adapter>(),
            output as usize,
            capacity as usize,
        )
        || !disjoint(context as usize, size_of::<Adapter>(), written as usize, 4)
    {
        return 1;
    }
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        // Shared reference plus private RefCell/Cell avoids creating a second &mut
        // on reentry; the guard refuses before a second session borrow or I/O.
        let adapter = unsafe { &*context.cast::<Adapter>() };
        let request = unsafe { std::slice::from_raw_parts(input, length as usize) };
        let reply = adapter.exchange(tag, request, capacity as usize)?;
        unsafe {
            std::ptr::copy_nonoverlapping(reply.as_ptr(), output, reply.len());
            std::ptr::write(written, reply.len() as u32);
        }
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    }));
    if matches!(outcome, Ok(Ok(()))) { 0 } else { 1 }
}
pub unsafe extern "C" fn channel_callback(
    c: *mut c_void,
    i: *const u8,
    n: u32,
    o: *mut u8,
    z: u32,
    w: *mut u32,
) -> u32 {
    unsafe {
        callback(
            c,
            i,
            n,
            o,
            z,
            w,
            CHANNEL,
            morrow_plugin_sdk::channel::MAX_WIRE_BYTES,
        )
    }
}
pub unsafe extern "C" fn runtime_callback(
    c: *mut c_void,
    i: *const u8,
    n: u32,
    o: *mut u8,
    z: u32,
    w: *mut u32,
) -> u32 {
    unsafe {
        callback(
            c,
            i,
            n,
            o,
            z,
            w,
            RUNTIME,
            morrow_plugin_sdk::MAX_MESSAGE_BYTES,
        )
    }
}
pub unsafe extern "C" fn transform_callback(
    c: *mut c_void,
    i: *const u8,
    n: u32,
    o: *mut u8,
    z: u32,
    w: *mut u32,
) -> u32 {
    unsafe {
        callback(
            c,
            i,
            n,
            o,
            z,
            w,
            TRANSFORM,
            morrow_plugin_sdk::MAX_MESSAGE_BYTES,
        )
    }
}
pub unsafe extern "C" fn pause_callback(
    c: *mut c_void,
    i: *const u8,
    n: u32,
    o: *mut u8,
    z: u32,
    w: *mut u32,
) -> u32 {
    unsafe {
        callback(
            c,
            i,
            n,
            o,
            z,
            w,
            PAUSE,
            morrow_plugin_sdk::MAX_MESSAGE_BYTES,
        )
    }
}
