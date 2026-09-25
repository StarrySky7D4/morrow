//! Trusted Worker-side execution of the same bounded, immutable packages as native.
//! This first adapter runs pure transforms only; it creates no content grants.
use crate::{BrowserStore, clock, error};
use morrow_core::{plugin_package::Package, task::Invocation};
use morrow_plugin_runtime::{Cancellation, Limits, package::PreparedPackage};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct BrowserTransformPackage {
    prepared: PreparedPackage,
}

#[wasm_bindgen]
impl BrowserTransformPackage {
    /// The trusted owner pins the selected package digest before preparing it.
    /// Package integrity is not approval to access documents or external IO.
    #[wasm_bindgen(constructor)]
    pub fn new(
        archive: &[u8],
        expected_digest: &[u8],
        fuel: u32,
        memory_bytes: u32,
    ) -> Result<BrowserTransformPackage, JsValue> {
        let package = Package::decode(archive).map_err(error)?;
        if expected_digest != package.digest() {
            return Err(error("Package digest does not match selected package"));
        }
        if package.manifest().guest_abi_version != 2
            || package.io_declaration().is_some()
            || package
                .manifest()
                .required_features
                .iter()
                .any(|feature| feature == morrow_core::plugin_package::DEPENDENCY_CALLS_FEATURE)
        {
            return Err(error("Package requires a managed IO/dependency adapter"));
        }
        let prepared = PreparedPackage::new(
            package,
            Limits {
                fuel: fuel.into(),
                memory_bytes: memory_bytes as usize,
                host_calls: 0,
            },
        )
        .map_err(|fault| error(format!("Package preparation: {fault:?}")))?;
        Ok(Self { prepared })
    }

    pub fn digest(&self) -> Vec<u8> {
        self.prepared.package().digest().to_vec()
    }

    /// Task identity, handler registration and output bounds are checked by the
    /// shared runtime. Each call retires its connection, including guest failure.
    pub fn transform(&self, store: &mut BrowserStore, bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
        let invocation = Invocation::decode(bytes).map_err(error)?;
        if invocation.transform().is_none() {
            return Err(error("Pure transform adapter refuses content commands"));
        }
        let connection = self
            .prepared
            .connect_approved(&mut store.runtime, &Default::default())
            .map_err(error)?;
        let result = self.prepared.run_task(
            &mut store.runtime,
            &connection,
            &invocation,
            clock,
            Cancellation::default(),
        );
        store.runtime.disconnect(&connection).map_err(error)?;
        completion(&invocation, result)
    }
}
pub(crate) fn completion(
    invocation: &Invocation,
    result: morrow_plugin_runtime::package::TaskReport,
) -> Result<Vec<u8>, JsValue> {
    if result.execution.outcome != Ok(0)
        || result.execution.host_calls != 0
        || result.response.is_some()
    {
        return Err(error(format!(
            "Package execution: {:?}",
            result.execution.outcome
        )));
    }
    match (result.output, result.failure) {
        (Some(output), None) => invocation.output_completion(&output.bytes).map_err(error),
        (None, Some(failure)) => invocation.failure_completion(&failure).map_err(error),
        _ => Err(error("Missing or ambiguous transform result")),
    }
}
