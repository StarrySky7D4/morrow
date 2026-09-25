//! Trusted local host transport. The UI owner supplies the selected device
//! identity; this boundary does not generate, persist, or upload private keys.
use crate::error;
use morrow_core::{audit::{SigningKey, TrustedLog, VerifyingKey}, plugin_package::Package};
use morrow_workbench_host::{Workbench, protocol};
use wasm_bindgen::prelude::*;
use zeroize::Zeroizing;

#[wasm_bindgen]
pub struct BrowserWorkbench { host: Option<Workbench> }

#[cfg(feature = "workbench-qualification")]
#[path = "../../workbench_host/examples/support/browser_workbench_probe.rs"]
mod probe;

#[wasm_bindgen]
impl BrowserWorkbench {
    pub fn import_device_file(&mut self, card: &str, name: &str, kind: &str, blob: JsValue) -> Result<crate::device_files::BrowserFileImport, JsValue> {
        let mut reader = crate::device_files::BlobReader::new(blob)?;
        let size = reader.size;
        let asset = self.host.as_mut().ok_or_else(|| error("workbench closed"))?
            .import(card, name, kind, &mut reader, size).map_err(error)?;
        Ok(crate::device_files::BrowserFileImport { id: asset.id, size: asset.bytes as u32 })
    }
    pub fn export_device_file(&self, card: &str, asset: &str) -> Result<crate::device_files::BrowserFileExport, JsValue> {
        let mut writer = crate::device_files::BlobWriter::new();
        self.host.as_ref().ok_or_else(|| error("workbench closed"))?
            .export(card, asset, &mut writer).map_err(error)?;
        writer.finish()
    }
    #[wasm_bindgen(constructor)]
    pub fn new(name: &str, create: bool, log_id: &str, public_key: &[u8], seed: &mut [u8], archive: &[u8]) -> Result<BrowserWorkbench, JsValue> {
        let secret = <[u8; 32]>::try_from(&*seed).map(Zeroizing::new);
        seed.fill(0);
        let secret = secret.map_err(error)?;
        let key = SigningKey::from_bytes(&secret);
        let public = <[u8; 32]>::try_from(public_key).map_err(error)?;
        let trust = TrustedLog {id: log_id.into(), key: VerifyingKey::from_bytes(&public).map_err(error)?};
        let package = Package::decode(archive).map_err(error)?;
        let host = Workbench::open_browser(name, create, trust, key, Some(package)).map_err(error)?;
        Ok(Self {host: Some(host)})
    }
    pub fn request(&mut self, bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
        protocol::respond(self.host.as_mut().ok_or_else(|| error("workbench closed"))?, bytes).map_err(error)
    }
    pub fn request_theme_package(&mut self, frame: &[u8], blob: JsValue) -> Result<Vec<u8>, JsValue> {
        use std::io::Read;
        let archive = (|| -> morrow_workbench_host::Result<Vec<u8>> {
            let mut reader = crate::device_files::BlobReader::new(blob)
                .map_err(|_| "Could not read the selected theme file")?;
            let limit = morrow_core::plugin_package::MAX_PACKAGE_BYTES;
            if reader.size == 0 || reader.size > limit as u64 {
                return Err("Theme package size exceeds the supported limit".into());
            }
            let mut bytes = Vec::with_capacity(reader.size as usize);
            reader.by_ref().take(limit as u64 + 1).read_to_end(&mut bytes)?;
            if bytes.len() != reader.size as usize { return Err("Theme file length changed".into()); }
            Ok(bytes)
        })();
        protocol::respond_theme_package(self.host.as_mut().ok_or_else(|| error("workbench closed"))?, frame, archive).map_err(error)
    }
    pub fn integrity_check(&self) -> Result<(), JsValue> {
        self.host.as_ref().ok_or_else(|| error("workbench closed"))?.browser_integrity_check().map_err(error)
    }
    pub fn finish(&mut self) -> Result<(), JsValue> {
        self.host.as_mut().ok_or_else(|| error("workbench closed"))?.finish().map_err(error)
    }
    /// Seal durable local state while keeping the admitted workbench live.
    pub fn checkpoint(&mut self) -> Result<(), JsValue> {
        self.host.as_mut().ok_or_else(|| error("workbench closed"))?.browser_checkpoint().map_err(error)
    }
    /// Flush before releasing ownership. A failed flush retains the owner so
    /// the caller can retry or deliberately dispose it after reporting failure.
    pub fn close(&mut self) -> Result<(), JsValue> {
        self.finish()?;
        self.host.take();
        Ok(())
    }
    #[cfg(feature = "workbench-qualification")]
    pub fn qualify(&mut self, restore: bool) -> Result<(), JsValue> {
        let host = self.host.as_mut().ok_or_else(|| error("workbench closed"))?;
        probe::run(&mut |bytes| protocol::respond(host, bytes), restore).map_err(error)
    }
}
