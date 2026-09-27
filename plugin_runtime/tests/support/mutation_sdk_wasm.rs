//! Explicit qualification of freshly compiled Rust/C/C++ SDK guests. These
//! tests run the original managed owner in a temporary directory, not a mock
//! callback that can invent effects. Build with build_plugin_mutation_wasm.ps1.
use super::mutation_guest_budget::exercise_maximum_module;
use super::mutation_guest_frame::exercise_module;

fn module(name: &str) -> Vec<u8> {
    let directory = std::env::var_os("MORROW_MUTATION_SDK_WASM_DIR")
        .expect("set MORROW_MUTATION_SDK_WASM_DIR to freshly compiled mutation guests");
    let path = std::path::PathBuf::from(directory).join(name);
    let wasm = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    assert!(
        wasm.starts_with(b"\0asm"),
        "{} is not WebAssembly",
        path.display()
    );
    assert!(wasm.len() <= morrow_core::plugin_package::MAX_MODULE_BYTES);
    wasm
}

#[test]
#[ignore = "requires freshly compiled mutation SDK guests"]
fn rust_mutation_sdk_uses_original_approved_owner() {
    exercise_module(module("rust_mutation.wasm"));
}

#[test]
#[ignore = "requires freshly compiled mutation SDK guests"]
fn c_mutation_sdk_uses_original_approved_owner() {
    exercise_module(module("c_mutation.wasm"));
}

#[test]
#[ignore = "requires freshly compiled mutation SDK guests"]
fn cpp_mutation_sdk_uses_original_approved_owner() {
    exercise_module(module("cpp_mutation.wasm"));
}

#[test]
#[ignore = "requires freshly compiled mutation SDK guests"]
fn rust_mutation_sdk_creates_maximum_content() {
    exercise_maximum_module(module("rust_mutation.wasm"));
}

#[test]
#[ignore = "requires freshly compiled mutation SDK guests"]
fn c_mutation_sdk_creates_maximum_content() {
    exercise_maximum_module(module("c_mutation.wasm"));
}

#[test]
#[ignore = "requires freshly compiled mutation SDK guests"]
fn cpp_mutation_sdk_creates_maximum_content() {
    exercise_maximum_module(module("cpp_mutation.wasm"));
}
