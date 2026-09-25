//! Platform-selected browser blobs use the same host import/export rules as
//! native streams. FileReaderSync is confined to the device Worker.
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};
use wasm_bindgen::prelude::*;

pub const MAX_FILE_BYTES: u64 = 200 * 1024 * 1024;
#[wasm_bindgen(inline_js = "
export function deviceBlobSize(blob) {
  if (!(blob instanceof Blob)) throw new Error('Expected a selected device Blob');
  return blob.size;
}
export function deviceBlobRead(blob, offset, length) {
  return new Uint8Array(new FileReaderSync().readAsArrayBuffer(blob.slice(offset, offset + length)));
}
export function deviceBlobFinish(parts) { return new Blob(parts); }
")]
extern "C" {
    #[wasm_bindgen(catch, js_name = deviceBlobSize)]
    fn blob_size(blob: &JsValue) -> Result<f64, JsValue>;
    #[wasm_bindgen(catch, js_name = deviceBlobRead)]
    fn blob_read(blob: &JsValue, offset: u32, length: u32) -> Result<js_sys::Uint8Array, JsValue>;
    #[wasm_bindgen(catch, js_name = deviceBlobFinish)]
    fn blob_finish(parts: &js_sys::Array) -> Result<JsValue, JsValue>;
}

pub struct BlobReader {
    blob: JsValue,
    pub size: u64,
    offset: u64,
}
impl BlobReader {
    pub fn new(blob: JsValue) -> Result<Self, JsValue> {
        let size = blob_size(&blob)?;
        if !size.is_finite() || size < 0.0 || size.fract() != 0.0 || size > MAX_FILE_BYTES as f64 {
            return Err(crate::error("Selected attachment exceeds 200 MiB"));
        }
        Ok(Self {
            blob,
            size: size as u64,
            offset: 0,
        })
    }
}
impl Read for BlobReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = (self.size - self.offset)
            .min(buffer.len() as u64)
            .min(64 * 1024) as usize;
        if count == 0 {
            return Ok(0);
        }
        let part = blob_read(&self.blob, self.offset as u32, count as u32)
            .map_err(|_| io::Error::other("Could not read selected browser file"))?;
        if part.length() as usize != count {
            return Err(io::Error::other("Selected file length changed"));
        }
        part.copy_to(&mut buffer[..count]);
        self.offset += count as u64;
        Ok(count)
    }
}

pub struct BlobWriter {
    parts: js_sys::Array,
    size: u64,
    digest: Sha256,
}
impl BlobWriter {
    pub fn new() -> Self {
        Self {
            parts: js_sys::Array::new(),
            size: 0,
            digest: Sha256::new(),
        }
    }
    pub fn finish(self) -> Result<BrowserFileExport, JsValue> {
        Ok(BrowserFileExport {
            blob: blob_finish(&self.parts)?,
            size: self.size as u32,
            digest: self.digest.finalize().to_vec(),
        })
    }
}
impl Write for BlobWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = self
            .size
            .checked_add(bytes.len() as u64)
            .filter(|n| *n <= MAX_FILE_BYTES)
            .ok_or_else(|| io::Error::other("Attachment exceeds browser export budget"))?;
        for part in bytes.chunks(64 * 1024) {
            // Copy before returning to Rust; Wasm memory may be reused or grow.
            self.parts.push(&js_sys::Uint8Array::from(part));
        }
        self.digest.update(bytes);
        self.size = next;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[wasm_bindgen(getter_with_clone)]
pub struct BrowserFileExport {
    pub blob: JsValue,
    pub size: u32,
    pub digest: Vec<u8>,
}

#[wasm_bindgen(getter_with_clone)]
pub struct BrowserFileImport {
    pub id: String,
    pub size: u32,
}
