//! Trusted first-party browser adapter. No third-party plugin is hosted here.
use morrow_core::{
    content::{Attachment, CardRecord},
    envelope,
    lifecycle::{Grant, HostPolicy, Instance},
    runtime::RenameRequest,
    store::{EventBudget, Store},
    transaction::Lookup,
};
use std::{cell::Cell, collections::BTreeMap, io::Cursor, path::Path};
thread_local! { static STORE_ACTIVE: Cell<bool> = const { Cell::new(false) }; }
use wasm_bindgen::prelude::*;
fn error(value: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&value.to_string())
}
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace=performance,js_name=now)]
    fn performance_now() -> f64;
}
fn clock() -> u64 {
    let value = performance_now();
    if value.is_finite() && value >= 0.0 {
        value as u64
    } else {
        u64::MAX
    }
}
#[wasm_bindgen]
pub async fn install_opfs() -> Result<(), JsValue> {
    use sqlite_wasm_vfs::sahpool::{OpfsSAHPoolCfgBuilder, install};
    let config = OpfsSAHPoolCfgBuilder::new()
        .vfs_name("morrow-opfs")
        .directory("morrow-test6")
        .initial_capacity(32)
        .clear_on_init(false)
        .build();
    install::<sqlite_wasm_rs::WasmOsCallback>(&config, false)
        .await
        .map_err(error)?;
    Ok(())
}
#[wasm_bindgen]
pub fn rename_encode(
    operation_id: &str,
    card_id: &str,
    revision: &js_sys::BigInt,
    title: &str,
) -> Result<Vec<u8>, JsValue> {
    let revision = revision
        .to_string(10)
        .map_err(JsValue::from)?
        .as_string()
        .ok_or_else(|| error("Invalid revision"))?
        .parse::<u64>()
        .map_err(error)?;
    RenameRequest {
        operation_id: operation_id.into(),
        card_id: card_id.into(),
        expected_revision: revision,
        title: title.into(),
    }
    .encode()
    .map_err(error)
}
#[wasm_bindgen]
pub struct DecodedRename {
    request: RenameRequest,
}
#[wasm_bindgen]
impl DecodedRename {
    #[wasm_bindgen(getter)]
    pub fn operation_id(&self) -> String {
        self.request.operation_id.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn card_id(&self) -> String {
        self.request.card_id.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn title(&self) -> String {
        self.request.title.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn revision(&self) -> u64 {
        self.request.expected_revision
    }
}
#[wasm_bindgen]
pub fn rename_decode(bytes: &[u8]) -> Result<DecodedRename, JsValue> {
    Ok(DecodedRename {
        request: RenameRequest::decode(bytes).map_err(error)?,
    })
}
#[wasm_bindgen]
pub struct BrowserStore {
    store: Store,
    host: HostPolicy,
    instance: Instance,
    grants: BTreeMap<String, Grant>,
}
#[wasm_bindgen]
impl BrowserStore {
    /// Path is a virtual name inside the dedicated OPFS pool; callers are trusted host code.
    #[wasm_bindgen(constructor)]
    pub fn new(name: &str, create: bool, event_capacity: u32) -> Result<BrowserStore, JsValue> {
        if name.is_empty()
            || name.len() > 64
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(error("Invalid database name"));
        }
        if STORE_ACTIVE.get() {
            return Err(error("OPFS store already open in this worker"));
        }
        let store = Store::open_opfs(
            Path::new(&format!("/{name}.db")),
            EventBudget {
                max_count: event_capacity,
                ..EventBudget::default()
            },
            create,
        )
        .map_err(error)?;
        let mut host = HostPolicy::new().map_err(error)?;
        let instance = host.activate().map_err(error)?;
        host.ready(instance).map_err(error)?;
        STORE_ACTIVE.set(true);
        Ok(Self {
            store,
            host,
            instance,
            grants: BTreeMap::new(),
        })
    }
    pub fn import_card(&mut self, operation: &str, container: &[u8]) -> Result<u64, JsValue> {
        let card = envelope::decode(container).map_err(error)?;
        Ok(self
            .store
            .create_local(operation, &card)
            .map_err(error)?
            .revision)
    }
    pub fn create_local(&mut self, operation: &str, id: &str, title: &str) -> Result<u64, JsValue> {
        let card = CardRecord::new(id, "morrow.text", 1, title, vec![]).map_err(error)?;
        Ok(self
            .store
            .create_local(operation, &card)
            .map_err(error)?
            .revision)
    }
    pub fn export_card(&self, id: &str) -> Result<Vec<u8>, JsValue> {
        envelope::encode(
            &self
                .store
                .card(id)
                .map_err(error)?
                .ok_or_else(|| error("NotFound"))?,
        )
        .map_err(error)
    }
    pub fn card_title(&self, id: &str) -> Result<String, JsValue> {
        Ok(self
            .store
            .card(id)
            .map_err(error)?
            .ok_or_else(|| error("NotFound"))?
            .summary()
            .title)
    }
    pub fn card_revision(&self, id: &str) -> Result<u64, JsValue> {
        Ok(self
            .store
            .card(id)
            .map_err(error)?
            .ok_or_else(|| error("NotFound"))?
            .summary()
            .revision)
    }
    pub fn lookup_revision(&self, id: &str) -> Result<Option<u64>, JsValue> {
        Ok(match self.store.lookup(id).map_err(error)? {
            Lookup::Committed(receipt) => Some(receipt.revision),
            Lookup::Absent => None,
        })
    }
    pub fn grant_rename(&mut self, id: &str, ttl: u32) -> Result<(), JsValue> {
        let now = clock();
        let expiry = now
            .checked_add(u64::from(ttl))
            .ok_or_else(|| error("Invalid expiry"))?;
        if let Some(old) = self.grants.remove(id) {
            self.host.revoke(old).map_err(error)?;
        }
        let grant = self
            .host
            .grant_rename(self.instance, id, expiry, now)
            .map_err(error)?;
        self.grants.insert(id.into(), grant);
        Ok(())
    }
    pub fn revoke_rename(&mut self, id: &str) -> Result<(), JsValue> {
        let grant = self.grants.remove(id).ok_or_else(|| error("No grant"))?;
        self.host.revoke(grant).map_err(error)
    }
    /// Both decoding and permission checking use the fixed binary request. No caller id on wire.
    pub fn rename(&mut self, bytes: &[u8]) -> Result<u64, JsValue> {
        let start = clock();
        let request = RenameRequest::decode(bytes).map_err(error)?;
        let grant = *self
            .grants
            .get(&request.card_id)
            .ok_or_else(|| error("No grant"))?;
        let permit = self
            .host
            .begin(self.instance, grant, &request, start)
            .map_err(error)?;
        Ok(self
            .host
            .commit_rename(permit, &mut self.store, clock)
            .map_err(error)?
            .revision)
    }
    pub fn stage(&mut self, bytes: &[u8], now: i64) -> Result<String, JsValue> {
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(error("Web transfer limit"));
        }
        Ok(self
            .store
            .stage_blob(&mut Cursor::new(bytes), bytes.len() as u64, None, now)
            .map_err(error)?
            .id)
    }
    pub fn create_attachment(
        &mut self,
        operation: &str,
        id: &str,
        blob: &str,
    ) -> Result<u64, JsValue> {
        let info = self.store.blob_info_local(blob).map_err(error)?;
        let attachment = Attachment {
            id: "file".into(),
            display_name: "原件.bin".into(),
            media_type: "application/octet-stream".into(),
            byte_length: info.byte_length,
            sha256: info.sha256,
        };
        let card = CardRecord::new_with_attachments(
            id,
            "morrow.file",
            1,
            "attachment",
            vec![],
            &[attachment],
        )
        .map_err(error)?;
        Ok(self
            .store
            .create_local(operation, &card)
            .map_err(error)?
            .revision)
    }
    pub fn clear_attachments(
        &mut self,
        operation: &str,
        id: &str,
        revision: u64,
    ) -> Result<u64, JsValue> {
        Ok(self
            .store
            .set_attachments_local(operation, id, revision, &[])
            .map_err(error)?
            .revision)
    }
    pub fn first_blob_page_count(&self) -> Result<u32, JsValue> {
        Ok(self.store.list_blobs_local("", 128).map_err(error)?.len() as u32)
    }
    pub fn export_blob(&self, id: &str) -> Result<Vec<u8>, JsValue> {
        if self.store.blob_info_local(id).map_err(error)?.byte_length > 4 * 1024 * 1024 {
            return Err(error("Web transfer limit"));
        }
        let mut bytes = vec![];
        self.store
            .export_blob_local(id, &mut bytes)
            .map_err(error)?;
        Ok(bytes)
    }
    pub fn retire(&mut self, id: &str, now: i64) -> Result<(), JsValue> {
        self.store.retire_blob_local(id, now).map_err(error)
    }
    pub fn collect(&mut self, now: i64) -> Result<u32, JsValue> {
        Ok(self
            .store
            .collect_retired_local(now, 60_000)
            .map_err(error)?
            .len() as u32)
    }
    pub fn check(&self) -> Result<(), JsValue> {
        self.store.integrity_check().map_err(error)
    }
}

impl Drop for BrowserStore {
    fn drop(&mut self) {
        STORE_ACTIVE.set(false);
    }
}
