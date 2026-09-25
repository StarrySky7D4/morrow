//! Trusted owner API; never exported as guest imports or exposed to untrusted pages.
use crate::{BrowserStore, clock, error};
use morrow_core::plugin_package::{
    capability,
    io::IoCapability,
    registry::{Registry, sqlite::SqliteRegistryStorage},
};
use morrow_core::task::Invocation;
use morrow_plugin_runtime::{
    Limits,
    instance_pool::{Pool, Session},
    manager::Manager,
};
use std::path::Path;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct BrowserPluginRegistry {
    manager: Manager,
    pool: Option<Pool>,
}
#[wasm_bindgen]
pub struct BrowserPluginSession {
    session: Session,
}
fn digest(bytes: &[u8]) -> Result<[u8; 32], JsValue> {
    bytes
        .try_into()
        .map_err(|_| error("Invalid package digest"))
}
#[wasm_bindgen]
impl BrowserPluginRegistry {
    #[wasm_bindgen(constructor)]
    pub fn new(name: &str, create: bool) -> Result<Self, JsValue> {
        if name.is_empty()
            || name.len() > 64
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err(error("Invalid registry name"));
        }
        let path = format!("plugin-registry-{name}.sqlite3");
        let storage = SqliteRegistryStorage::open_opfs(Path::new(&path), create).map_err(error)?;
        Ok(Self {
            manager: Manager::new(
                Registry::from_storage(Box::new(storage)).map_err(error)?,
                Limits::default(),
            ),
            pool: None,
        })
    }
    pub fn revision(&self) -> u64 {
        self.manager.revision()
    }
    pub fn snapshot(&self) -> Result<Vec<u8>, JsValue> {
        Ok(self
            .manager
            .persisted_snapshot()
            .map_err(error)?
            .unwrap_or_default())
    }
    pub fn install(&self, archive: &[u8]) -> Result<Vec<u8>, JsValue> {
        self.manager
            .install_package(archive)
            .map(|digest| digest.to_vec())
            .map_err(error)
    }
    pub fn select(&mut self, selected: &[u8], revision: u64) -> Result<(), JsValue> {
        self.manager
            .select_digest(digest(selected)?, revision)
            .map_err(error)
    }
    pub fn approve(
        &mut self,
        id: &str,
        selected: &[u8],
        capabilities: &[i32],
        revision: u64,
    ) -> Result<(), JsValue> {
        if capabilities.len() > 7 {
            return Err(error("Too many content approvals"));
        }
        let approved = capabilities
            .iter()
            .copied()
            .map(capability)
            .collect::<morrow_core::Result<_>>()
            .map_err(error)?;
        self.manager
            .approve(id, digest(selected)?, approved, revision)
            .map_err(error)
    }
    pub fn approve_io(
        &mut self,
        id: &str,
        selected: &[u8],
        capabilities: &[i32],
        revision: u64,
    ) -> Result<(), JsValue> {
        if capabilities.len() > 10 {
            return Err(error("Too many IO approvals"));
        }
        let approved = capabilities
            .iter()
            .copied()
            .map(IoCapability::from_number)
            .collect::<morrow_core::Result<_>>()
            .map_err(error)?;
        self.manager
            .approve_io(id, digest(selected)?, approved, revision)
            .map_err(error)
    }
    pub fn set_enabled(
        &mut self,
        id: &str,
        selected: &[u8],
        enabled: bool,
        revision: u64,
    ) -> Result<(), JsValue> {
        self.manager
            .set_enabled(id, digest(selected)?, enabled, revision)
            .map_err(error)
    }
    pub fn remove(&mut self, id: &str, revision: u64) -> Result<(), JsValue> {
        self.manager.remove(id, revision).map_err(error)
    }
    pub fn approve_dependency(
        &mut self,
        caller: &str,
        caller_digest: &[u8],
        slot: &str,
        provider: &str,
        provider_digest: &[u8],
        revision: u64,
    ) -> Result<(), JsValue> {
        self.manager
            .approve_dependency(
                caller,
                digest(caller_digest)?,
                slot,
                provider,
                digest(provider_digest)?,
                revision,
            )
            .map_err(error)
    }
    pub fn remove_dependency(
        &mut self,
        caller: &str,
        slot: &str,
        revision: u64,
    ) -> Result<(), JsValue> {
        self.manager
            .remove_dependency(caller, slot, revision)
            .map_err(error)
    }
    /// One bounded call through the same manager/pool as native. No object grants
    /// are issued and a transient session is closed on success or failure.
    pub fn transform(
        &mut self,
        store: &mut BrowserStore,
        id: &str,
        invocation: &[u8],
    ) -> Result<Vec<u8>, JsValue> {
        let session = self.activate(store, id, self.manager.revision())?;
        let result = self.run_session(store, &session, invocation);
        self.close_session(store, &session)?;
        result
    }
    pub fn activate(
        &mut self,
        store: &mut BrowserStore,
        id: &str,
        revision: u64,
    ) -> Result<BrowserPluginSession, JsValue> {
        if self.pool.is_none() {
            self.pool = Some(Pool::new(&store.runtime, Default::default()).map_err(error)?);
        }
        let session = self
            .pool
            .as_mut()
            .expect("pool initialized")
            .start(&mut self.manager, &mut store.runtime, id, &[], revision)
            .map_err(error)?;
        Ok(BrowserPluginSession { session })
    }
    pub fn run_session(
        &mut self,
        store: &mut BrowserStore,
        session: &BrowserPluginSession,
        bytes: &[u8],
    ) -> Result<Vec<u8>, JsValue> {
        let input = Invocation::decode(bytes).map_err(error)?;
        if input.transform().is_none() {
            return Err(error("Pure transform adapter refuses content commands"));
        }
        let result = self
            .pool
            .as_mut()
            .ok_or_else(|| error("No active plugin pool"))?
            .run_task(
                &self.manager,
                &mut store.runtime,
                &session.session,
                &input,
                clock,
            )
            .map_err(error)?;
        crate::package::completion(&input, result)
    }
    pub fn close_session(
        &mut self,
        store: &mut BrowserStore,
        session: &BrowserPluginSession,
    ) -> Result<(), JsValue> {
        self.pool
            .as_mut()
            .ok_or_else(|| error("No active plugin pool"))?
            .close(&mut store.runtime, &session.session)
            .map_err(error)
    }
    /// Approved guest-requested dependency graph. Immutable input objects and
    /// output proofs stay local to this Worker and are released with the call.
    pub fn run_dependency_session(
        &mut self,
        store: &mut BrowserStore,
        session: &BrowserPluginSession,
        bytes: &[u8],
        max_calls: u32,
    ) -> Result<Vec<u8>, JsValue> {
        use morrow_plugin_runtime::{
            Cancellation,
            dynamic_dependencies::{Context, GraphLimits},
            monotonic::Instant,
            shared_objects::SharedObjects,
        };
        if !(1..=16).contains(&max_calls) {
            return Err(error("Invalid dependency budget"));
        }
        let input = Invocation::decode(bytes).map_err(error)?;
        if input.transform().is_none() {
            return Err(error("Dependency adapter refuses content commands"));
        }
        let expires = clock()
            .checked_add(30_000)
            .ok_or_else(|| error("Clock overflow"))?;
        let deadline = Instant::now()
            .checked_add(std::time::Duration::from_secs(30))
            .ok_or_else(|| error("Clock overflow"))?;
        let mut objects = SharedObjects::new(&store.runtime, Default::default()).map_err(error)?;
        let pool = self
            .pool
            .as_mut()
            .ok_or_else(|| error("No active plugin pool"))?;
        let output = pool
            .run(
                &self.manager,
                &mut store.runtime,
                &mut objects,
                &session.session,
                &input,
                Context {
                    scope: "browser-plugin-transform",
                    expires,
                    max_calls,
                },
                GraphLimits {
                    max_depth: 4,
                    max_total_calls: max_calls,
                },
                clock,
                Cancellation::until(deadline),
            )
            .map_err(error)?;
        output
            .validate(
                &store.runtime,
                pool.root(&session.session).map_err(error)?.connection(),
                clock(),
            )
            .map_err(error)?;
        let usage = objects.usage();
        if (
            usage.objects,
            usage.leases,
            usage.mappings,
            usage.charged_bytes,
        ) != (0, 0, 0, 0)
        {
            return Err(error("Dependency input resources not released"));
        }
        input.output_completion(output.bytes()).map_err(error)
    }
    /// Trusted owner closes before releasing the registry while the store is alive.
    pub fn close_all(&mut self, store: &mut BrowserStore) -> Result<(), JsValue> {
        if let Some(pool) = self.pool.as_mut() {
            pool.close_all(&mut store.runtime).map_err(error)?;
        }
        Ok(())
    }
}
