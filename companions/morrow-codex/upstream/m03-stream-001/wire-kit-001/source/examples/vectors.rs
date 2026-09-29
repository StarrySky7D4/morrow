#[path = "../tests/common/mod.rs"]
mod common;
use morrow_native_http_stream_wire::*;
fn main() {
    let dir = std::path::PathBuf::from(std::env::args_os().nth(1).expect("new output directory"));
    std::fs::create_dir(&dir).unwrap();
    for (name, frame) in common::cases() {
        let bytes = frame.encode().unwrap();
        assert_eq!(Frame::decode(&bytes).unwrap(), frame);
        std::fs::write(dir.join(format!("{name}.frame")), &bytes).unwrap();
        std::fs::write(dir.join(format!("{name}.capnp")), &bytes[4..]).unwrap();
        println!("{name} {} {}", bytes.len(), hex(&digest(&bytes)));
    }
    let mut invalid = common::initial().encode().unwrap();
    invalid.pop();
    std::fs::write(dir.join("truncated.invalid"), invalid).unwrap();
    std::fs::write(dir.join("oversize-prefix.invalid"), 32776u32.to_le_bytes()).unwrap();
}
