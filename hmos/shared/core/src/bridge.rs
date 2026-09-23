//! Trusted-client ABI shared by native FFI and a browser-owned Wasm instance.
//! No OS pointer or buffer ID is a plugin authorization capability.
//! Client must own each handle exclusively, never keep a pointer after free,
//! and never write concurrently with process/free. No callbacks occur here.
use crate::runtime::{MAX_MESSAGE_BYTES, RenameRequest};
use std::{
    collections::BTreeMap,
    sync::{LazyLock, Mutex},
};
struct Buffer {
    bytes: Box<[u8]>,
    status: u32,
}
struct Registry {
    next: u32,
    buffers: BTreeMap<u32, Buffer>,
}
static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| {
    Mutex::new(Registry {
        next: 1,
        buffers: BTreeMap::new(),
    })
});
impl Registry {
    fn insert(&mut self, bytes: Vec<u8>) -> u32 {
        if self.buffers.len() >= 16 || bytes.is_empty() || bytes.len() > MAX_MESSAGE_BYTES {
            return 0;
        }
        let Some(next) = self.next.checked_add(1) else {
            return 0;
        };
        let id = self.next;
        self.next = next;
        self.buffers.insert(
            id,
            Buffer {
                bytes: bytes.into_boxed_slice(),
                status: 0,
            },
        );
        id
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_buffer_new(len: u32) -> u32 {
    if len == 0 || len as usize > MAX_MESSAGE_BYTES {
        return 0;
    }
    let Ok(mut registry) = REGISTRY.lock() else {
        return 0;
    };
    registry.insert(vec![0; len as usize])
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_buffer_ptr(id: u32) -> *mut u8 {
    let Ok(mut registry) = REGISTRY.lock() else {
        return std::ptr::null_mut();
    };
    registry
        .buffers
        .get_mut(&id)
        .map_or(std::ptr::null_mut(), |b| b.bytes.as_mut_ptr())
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_buffer_len(id: u32) -> u32 {
    let Ok(registry) = REGISTRY.lock() else {
        return 0;
    };
    registry
        .buffers
        .get(&id)
        .map_or(0, |b| b.bytes.len() as u32)
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_buffer_status(id: u32) -> u32 {
    let Ok(registry) = REGISTRY.lock() else {
        return 255;
    };
    registry.buffers.get(&id).map_or(255, |b| b.status)
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_buffer_process(id: u32) -> u32 {
    let Ok(mut registry) = REGISTRY.lock() else {
        return 0;
    };
    let Some(input) = registry.buffers.get(&id) else {
        return 0;
    };
    // Fixed owned copy is used for both schema checks and decoding.
    let fixed = input.bytes.to_vec();
    let encoded = RenameRequest::decode(&fixed).and_then(|request| request.encode());
    let (output, status) = match encoded {
        Ok(bytes) => {
            let output = registry.insert(bytes);
            (output, if output == 0 { 3 } else { 0 })
        }
        Err(crate::Error::UnsupportedVersion) => (0, 1),
        Err(crate::Error::Limit) => (0, 3),
        Err(_) => (0, 2),
    };
    registry.buffers.get_mut(&id).unwrap().status = status;
    output
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_buffer_free(id: u32) -> u32 {
    let Ok(mut registry) = REGISTRY.lock() else {
        return 0;
    };
    u32::from(registry.buffers.remove(&id).is_some())
}

#[unsafe(no_mangle)]
pub extern "C" fn morrow_buffer_live() -> u32 {
    REGISTRY
        .lock()
        .map_or(255, |registry| registry.buffers.len() as u32)
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native;
