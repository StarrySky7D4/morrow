//! Host clocks and entropy; browser application data never depends on a server clock.
pub use morrow_plugin_runtime::monotonic::Instant;
#[cfg(not(target_arch = "wasm32"))]
pub fn random(bytes: &mut [u8]) -> crate::Result<()> {
    getrandom::fill(bytes)?;
    Ok(())
}
#[cfg(target_arch = "wasm32")]
pub fn random(bytes: &mut [u8]) -> crate::Result<()> {
    #[wasm_bindgen::prelude::wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(catch, js_namespace = crypto, js_name = getRandomValues)]
        fn fill(bytes: &mut [u8]) -> std::result::Result<(), wasm_bindgen::JsValue>;
    }
    fill(bytes).map_err(|_| "browser entropy unavailable".into())
}
#[cfg(not(target_arch = "wasm32"))]
pub fn unix_millis() -> crate::Result<i64> {
    Ok(i64::try_from(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis())?)
}
#[cfg(target_arch = "wasm32")]
pub fn unix_millis() -> crate::Result<i64> {
    let value = js_sys::Date::now();
    if !value.is_finite() || value < 0.0 || value > 9_007_199_254_740_991.0 { return Err("invalid browser clock".into()); }
    Ok(value as i64)
}
