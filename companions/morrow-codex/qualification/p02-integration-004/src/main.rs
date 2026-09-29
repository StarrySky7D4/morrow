mod deny;
mod exec;
mod integration;
mod network;
mod store;
use serde_json::json;

fn main() {
    let destination = std::env::args_os()
        .nth(1)
        .expect("fresh receipt destination required");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let cases = runtime.block_on(async {
        let mut cases = vec![integration::coexist().await];
        cases.extend(integration::guards().await);
        cases.extend(exec::qualify().await);
        cases.extend(store::qualify().await);
        cases.extend(network::qualify().await);
        cases
    });
    drop(runtime);
    let assertions: u64 = cases
        .iter()
        .map(|c| c["assertions"].as_u64().unwrap())
        .sum();
    let report = json!({"status":"passed_limited_integration_probe","P-02":"not_complete","G0":"not_claimed","case_count":cases.len(),"assertions":assertions,"cases":cases,"runtime_dropped_before_receipt":true,"qualification_only":true,"limits":["One source graph/executable; shared adapter lifetime is not production agent-loop integration","Restricted build guards are not runtime authority or OS isolation","No successful process, HTTP/WS data, durable commit or writer release","Default product build, direct LocalThreadStore methods, unrelated network/spawn paths, metadata contexts and async cancellation are unqualified"]});
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .unwrap();
    serde_json::to_writer_pretty(&mut output, &report).unwrap();
    println!(
        "{}",
        json!({"status":report["status"],"case_count":report["case_count"],"assertions":assertions})
    );
}
