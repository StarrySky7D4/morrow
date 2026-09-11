//! Trusted first-party browser adapter. No third-party plugin is hosted here.
use morrow_core::{
    content::{Attachment, CardRecord},
    dispatch::{Connection, HostRuntime},
    envelope,
    lifecycle::GrantKind,
    response::{Outcome, Response},
    runtime::RenameRequest,
    store::{EventBudget, Store},
    transaction::Lookup,
};
use std::{cell::Cell, io::Cursor, path::Path};
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
        .directory("morrow-test7")
        .initial_capacity(64)
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
    runtime: HostRuntime,
    connection: Connection,
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
        let mut runtime = HostRuntime::new(store).map_err(error)?;
        let connection = runtime.connect().map_err(error)?;
        STORE_ACTIVE.set(true);
        Ok(Self {
            runtime,
            connection,
        })
    }

    pub fn import_card(&mut self, operation: &str, container: &[u8]) -> Result<u64, JsValue> {
        let card = envelope::decode(container).map_err(error)?;
        Ok(self
            .runtime
            .store_local_mut()
            .create_local(operation, &card)
            .map_err(error)?
            .revision)
    }
    pub fn create_local(&mut self, operation: &str, id: &str, title: &str) -> Result<u64, JsValue> {
        let card = CardRecord::new(id, "morrow.text", 1, title, vec![]).map_err(error)?;
        Ok(self
            .runtime
            .store_local_mut()
            .create_local(operation, &card)
            .map_err(error)?
            .revision)
    }
    pub fn export_card(&self, id: &str) -> Result<Vec<u8>, JsValue> {
        envelope::encode(
            &self
                .runtime
                .store_local()
                .card(id)
                .map_err(error)?
                .ok_or_else(|| error("NotFound"))?,
        )
        .map_err(error)
    }
    pub fn card_title(&self, id: &str) -> Result<String, JsValue> {
        Ok(self
            .runtime
            .store_local()
            .card(id)
            .map_err(error)?
            .ok_or_else(|| error("NotFound"))?
            .summary()
            .title)
    }
    pub fn card_revision(&self, id: &str) -> Result<u64, JsValue> {
        Ok(self
            .runtime
            .store_local()
            .card(id)
            .map_err(error)?
            .ok_or_else(|| error("NotFound"))?
            .summary()
            .revision)
    }
    pub fn lookup_revision(&self, id: &str) -> Result<Option<u64>, JsValue> {
        Ok(
            match self.runtime.store_local().lookup(id).map_err(error)? {
                Lookup::Committed(receipt) => Some(receipt.revision),
                Lookup::Absent => None,
            },
        )
    }
    pub fn grant_rename(&mut self, id: &str, ttl: u32) -> Result<(), JsValue> {
        let now = clock();
        self.runtime
            .grant(
                &mut self.connection,
                GrantKind::Rename,
                id,
                now.saturating_add(ttl as u64),
                now,
            )
            .map_err(error)
    }
    pub fn revoke_rename(&mut self, id: &str) -> Result<(), JsValue> {
        self.runtime
            .revoke(&mut self.connection, GrantKind::Rename, id)
            .map_err(error)
    }
    pub fn grant_read(&mut self, id: &str, ttl: u32) -> Result<(), JsValue> {
        let now = clock();
        self.runtime
            .grant(
                &mut self.connection,
                GrantKind::ReadSummary,
                id,
                now.saturating_add(ttl as u64),
                now,
            )
            .map_err(error)
    }
    pub fn revoke_read(&mut self, id: &str) -> Result<(), JsValue> {
        self.runtime
            .revoke(&mut self.connection, GrantKind::ReadSummary, id)
            .map_err(error)
    }
    pub fn dispatch(&mut self, bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
        self.runtime
            .dispatch(&self.connection, bytes, clock)
            .map_err(error)
    }
    /// Trusted local convenience; routes through the same binary dispatch path.
    pub fn rename(&mut self, bytes: &[u8]) -> Result<u64, JsValue> {
        let response = Response::decode(&self.dispatch(bytes)?).map_err(error)?;
        match response.outcome {
            Outcome::Renamed(value) => Ok(value.revision),
            other => Err(error(format!("{other:?}"))),
        }
    }
    pub fn attachment_count(&self, id: &str) -> Result<u32, JsValue> {
        Ok(self
            .runtime
            .store_local()
            .card(id)
            .map_err(error)?
            .ok_or_else(|| error("NotFound"))?
            .attachments()
            .len() as u32)
    }
    pub fn stage(&mut self, bytes: &[u8], now: i64) -> Result<String, JsValue> {
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(error("Web transfer limit"));
        }
        Ok(self
            .runtime
            .store_local_mut()
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
        let info = self
            .runtime
            .store_local()
            .blob_info_local(blob)
            .map_err(error)?;
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
            .runtime
            .store_local_mut()
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
            .runtime
            .store_local_mut()
            .set_attachments_local(operation, id, revision, &[])
            .map_err(error)?
            .revision)
    }
    pub fn first_blob_page_count(&self) -> Result<u32, JsValue> {
        Ok(self
            .runtime
            .store_local()
            .list_blobs_local("", 128)
            .map_err(error)?
            .len() as u32)
    }
    pub fn export_blob(&self, id: &str) -> Result<Vec<u8>, JsValue> {
        if self
            .runtime
            .store_local()
            .blob_info_local(id)
            .map_err(error)?
            .byte_length
            > 4 * 1024 * 1024
        {
            return Err(error("Web transfer limit"));
        }
        let mut bytes = vec![];
        self.runtime
            .store_local()
            .export_blob_local(id, &mut bytes)
            .map_err(error)?;
        Ok(bytes)
    }
    pub fn retire(&mut self, id: &str, now: i64) -> Result<(), JsValue> {
        self.runtime
            .store_local_mut()
            .retire_blob_local(id, now)
            .map_err(error)
    }
    pub fn collect(&mut self, now: i64) -> Result<u32, JsValue> {
        Ok(self
            .runtime
            .store_local_mut()
            .collect_retired_local(now, 60_000)
            .map_err(error)?
            .len() as u32)
    }
    pub fn check(&self) -> Result<(), JsValue> {
        self.runtime.store_local().integrity_check().map_err(error)
    }
}

impl Drop for BrowserStore {
    fn drop(&mut self) {
        STORE_ACTIVE.set(false);
    }
}

#[wasm_bindgen]
pub fn read_encode(request_id: &str, card_id: &str) -> Result<Vec<u8>, JsValue> {
    morrow_core::runtime::Command::ReadSummary {
        request_id: request_id.into(),
        card_id: card_id.into(),
    }
    .encode()
    .map_err(error)
}
#[wasm_bindgen]
pub struct DecodedResponse {
    response: Response,
}
#[wasm_bindgen]
impl DecodedResponse {
    #[wasm_bindgen(getter)]
    pub fn request_id(&self) -> String {
        self.response.request_id.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        match &self.response.outcome {
            Outcome::Renamed(_) => "renamed",
            Outcome::Summary(_) => "summary",
            Outcome::Rejected(_) => "rejected",
        }
        .into()
    }
    #[wasm_bindgen(getter)]
    pub fn revision(&self) -> Option<u64> {
        match &self.response.outcome {
            Outcome::Renamed(v) => Some(v.revision),
            Outcome::Summary(v) => Some(v.revision),
            _ => None,
        }
    }
    #[wasm_bindgen(getter)]
    pub fn title(&self) -> Option<String> {
        match &self.response.outcome {
            Outcome::Summary(v) => Some(v.title.clone()),
            _ => None,
        }
    }
    #[wasm_bindgen(getter)]
    pub fn failure(&self) -> Option<String> {
        match &self.response.outcome {
            Outcome::Rejected(v) => Some(format!("{v:?}")),
            _ => None,
        }
    }
}
#[wasm_bindgen]
pub fn response_decode(bytes: &[u8]) -> Result<DecodedResponse, JsValue> {
    Ok(DecodedResponse {
        response: Response::decode(bytes).map_err(error)?,
    })
}
