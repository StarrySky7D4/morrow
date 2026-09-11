//! Pure binary transforms. All input comes from the current invocation.
use morrow_plugin_sdk::wasm;
#[unsafe(no_mangle)]
pub extern "C" fn morrow_run() -> i32 {
    let Ok(task) = wasm::read_task() else {
        return -1;
    };
    let Some(t) = task.transform() else {
        return -1;
    };
    if t.input_type != "bytes" || t.output_type != "bytes" {
        return -1;
    }
    let mut output = t.input.clone();
    match t.handler.as_str() {
        "bytes.reverse" => output.reverse(),
        "bytes.ascii-uppercase" => output.make_ascii_uppercase(),
        "bytes.require-ascii" => {
            if !output.is_ascii() {
                return if wasm::complete_failure(
                    &task,
                    morrow_plugin_sdk::task::FailureCode::UnsupportedInput,
                    "Input contains non-ASCII bytes",
                )
                .is_ok()
                {
                    0
                } else {
                    -1
                };
            }
        }
        _ => return -1,
    };
    if wasm::complete_output(&task, &output).is_ok() {
        0
    } else {
        -1
    }
}
