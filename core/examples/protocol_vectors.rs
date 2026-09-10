use capnp::{
    message::{AllocationStrategy, Builder, HeapAllocator},
    serialize,
};
use morrow_core::{
    runtime::{PROTOCOL_VERSION, RenameRequest, content_digest, runtime_digest},
    runtime_capnp,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("expected test output directory")?,
    );
    std::fs::create_dir_all(&dir)?;
    for (name, revision) in [
        ("one", 1),
        ("above-js", (1u64 << 53) + 1),
        ("max", u64::MAX),
    ] {
        let request = RenameRequest {
            operation_id: "vector-op".into(),
            card_id: "legacy-123".into(),
            expected_revision: revision,
            title: "消息 🪷".into(),
        };
        std::fs::write(dir.join(format!("{name}.capnp")), request.encode()?)?;
    }
    let allocator = HeapAllocator::new()
        .first_segment_words(1)
        .allocation_strategy(AllocationStrategy::FixedSize);
    let mut message = Builder::new(allocator);
    let mut root = message.init_root::<runtime_capnp::request::Builder>();
    root.set_protocol_version(PROTOCOL_VERSION);
    root.set_operation_id("vector-op");
    root.set_runtime_digest(&runtime_digest());
    root.set_content_digest(&content_digest());
    let mut rename = root.init_rename_card();
    rename.set_card_id("legacy-123");
    rename.set_expected_revision(u64::MAX);
    rename.set_title("消息 🪷");
    assert!(message.get_segments_for_output().len() > 1);
    std::fs::write(
        dir.join("multisegment.capnp"),
        serialize::write_message_to_words(&message),
    )?;
    println!("Wrote four Rust protocol test vectors.");
    Ok(())
}
