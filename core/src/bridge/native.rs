//! Trusted native control plane. Integer handles are not plugin capabilities.
//! Lock order is buffer arena then host registry. No caller callback runs under a lock.
use super::REGISTRY;
use crate::{
    dispatch::{Connection, HostRuntime},
    lifecycle::GrantKind,
    store::{EventBudget, Store},
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{LazyLock, Mutex},
    time::Instant,
};
struct Host {
    runtime: HostRuntime,
    connections: BTreeMap<u32, Connection>,
    started: Instant,
    path: PathBuf,
}
struct Hosts {
    next: u32,
    hosts: BTreeMap<u32, Host>,
}
static HOSTS: LazyLock<Mutex<Hosts>> = LazyLock::new(|| {
    Mutex::new(Hosts {
        next: 1,
        hosts: BTreeMap::new(),
    })
});
impl Hosts {
    fn id(&mut self) -> Option<u32> {
        let id = self.next;
        self.next = id.checked_add(1)?;
        Some(id)
    }
}
fn tick(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}
fn text(bytes: &[u8]) -> Result<&str, u32> {
    std::str::from_utf8(bytes).map_err(|_| 2)
}
fn code(error: crate::Error) -> u32 {
    match error {
        crate::Error::UnsupportedVersion => 1,
        crate::Error::Invalid(_) => 2,
        crate::Error::Limit => 3,
        crate::Error::StorageBusy => 5,
        _ => 4,
    }
}
fn with_input(id: u32, action: impl FnOnce(&[u8]) -> Result<u32, u32>) -> u32 {
    let Ok(mut arena) = REGISTRY.lock() else {
        return 0;
    };
    let Some(input) = arena.buffers.get(&id) else {
        return 0;
    };
    let bytes = input.bytes.to_vec();
    let (value, status) = match action(&bytes) {
        Ok(v) => (v, 0),
        Err(code) => (0, code),
    };
    if let Some(input) = arena.buffers.get_mut(&id) {
        input.status = status;
    }
    value
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_host_open(path_buffer: u32) -> u32 {
    with_input(path_buffer, |bytes| {
        if bytes.len() > 32768 || bytes.contains(&0) {
            return Err(2);
        }
        let path = PathBuf::from(text(bytes)?);
        if !path.is_absolute() {
            return Err(2);
        }
        let path = std::fs::canonicalize(path).map_err(|_| 4u32)?;
        let mut hosts = HOSTS.lock().map_err(|_| 4u32)?;
        if hosts.hosts.len() >= 8 {
            return Err(3);
        }
        if hosts.hosts.values().any(|host| host.path == path) {
            return Err(5);
        }
        let id = hosts.id().ok_or(3u32)?;
        let runtime =
            HostRuntime::new(Store::open_existing(&path, EventBudget::default()).map_err(code)?)
                .map_err(code)?;
        hosts.hosts.insert(
            id,
            Host {
                runtime,
                connections: BTreeMap::new(),
                started: Instant::now(),
                path,
            },
        );
        Ok(id)
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_host_close(host: u32) -> u32 {
    HOSTS.lock().map_or(0, |mut hosts| {
        u32::from(hosts.hosts.remove(&host).is_some())
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_host_connect(host: u32) -> u32 {
    let Ok(mut hosts) = HOSTS.lock() else {
        return 0;
    };
    let Some(id) = hosts.id() else {
        return 0;
    };
    let Some(host) = hosts.hosts.get_mut(&host) else {
        return 0;
    };
    let Ok(connection) = host.runtime.connect() else {
        return 0;
    };
    host.connections.insert(id, connection);
    id
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_host_disconnect(host: u32, connection: u32) -> u32 {
    let Ok(mut hosts) = HOSTS.lock() else {
        return 0;
    };
    let Some(host) = hosts.hosts.get_mut(&host) else {
        return 0;
    };
    let Some(connection) = host.connections.remove(&connection) else {
        return 0;
    };
    u32::from(host.runtime.disconnect(&connection).is_ok())
}
fn kind(value: u32) -> Result<GrantKind, u32> {
    match value {
        1 => Ok(GrantKind::Rename),
        2 => Ok(GrantKind::ReadSummary),
        3 => Ok(GrantKind::QueryOperation),
        _ => Err(2),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_host_grant(
    host: u32,
    connection: u32,
    capability: u32,
    card_buffer: u32,
    ttl_ms: u32,
) -> u32 {
    with_input(card_buffer, |bytes| {
        let card = text(bytes)?;
        let kind = kind(capability)?;
        let mut hosts = HOSTS.lock().map_err(|_| 4u32)?;
        let host = hosts.hosts.get_mut(&host).ok_or(255u32)?;
        let connection = host.connections.get_mut(&connection).ok_or(255u32)?;
        let now = tick(host.started);
        let expires = now.checked_add(ttl_ms as u64).ok_or(3u32)?;
        host.runtime
            .grant(connection, kind, card, expires, now)
            .map_err(code)?;
        Ok(1)
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_host_revoke(
    host: u32,
    connection: u32,
    capability: u32,
    card_buffer: u32,
) -> u32 {
    with_input(card_buffer, |bytes| {
        let card = text(bytes)?;
        let kind = kind(capability)?;
        let mut hosts = HOSTS.lock().map_err(|_| 4u32)?;
        let host = hosts.hosts.get_mut(&host).ok_or(255u32)?;
        let connection = host.connections.get_mut(&connection).ok_or(255u32)?;
        host.runtime.revoke(connection, kind, card).map_err(code)?;
        Ok(1)
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_host_dispatch(host: u32, connection: u32, input: u32) -> u32 {
    let Ok(mut arena) = REGISTRY.lock() else {
        return 0;
    };
    let Some(buffer) = arena.buffers.get(&input) else {
        return 0;
    };
    let bytes = buffer.bytes.to_vec();
    // Reserve the response slot BEFORE any command can commit, while the arena is exclusive.
    let output = arena.insert(vec![0; crate::runtime::MAX_MESSAGE_BYTES]);
    if output == 0 {
        if let Some(v) = arena.buffers.get_mut(&input) {
            v.status = 3;
        }
        return 0;
    }
    let result = (|| {
        let mut hosts = HOSTS.lock().map_err(|_| 4u32)?;
        let host = hosts.hosts.get_mut(&host).ok_or(255u32)?;
        let connection = host.connections.get(&connection).ok_or(255u32)?;
        let started = host.started;
        host.runtime
            .dispatch(connection, &bytes, || tick(started))
            .map_err(code)
    })();
    match result {
        Ok(bytes) => {
            // The reserved slot and input cannot disappear while the arena lock is held.
            if let Some(v) = arena.buffers.get_mut(&output) {
                v.bytes = bytes.into_boxed_slice();
            }
            if let Some(v) = arena.buffers.get_mut(&input) {
                v.status = 0;
            }
            output
        }
        Err(status) => {
            arena.buffers.remove(&output);
            if let Some(v) = arena.buffers.get_mut(&input) {
                v.status = status;
            }
            0
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn morrow_host_live() -> u32 {
    HOSTS
        .lock()
        .map_or(u32::MAX, |hosts| hosts.hosts.len() as u32)
}
