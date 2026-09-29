use morrow_native_session_wire::{Frame, Kind, digest, hex, schema_digest};
fn main() {
    let out = std::path::PathBuf::from(std::env::args_os().nth(1).expect("fresh output directory"));
    std::fs::create_dir(&out).unwrap();
    let initial = Frame {
        kind: Kind::Challenge,
        sequence: 0,
        session: 1,
        epoch: 2,
        generation: 1,
        pid: 42,
        code: 0,
        remaining_ms: 1000,
        nonce: [7; 32],
        schema: schema_digest(),
        artifact: [8; 32],
        config: [9; 32],
        budget: 16,
        capabilities: 1,
    };
    for (name, kind, seq, code) in [
        ("challenge", Kind::Challenge, 0, 0),
        ("hello", Kind::Hello, 1, 0),
        ("welcome", Kind::Welcome, 1, 2),
        ("query", Kind::Query, 2, 0),
        ("state", Kind::State, 2, 2),
        ("denied-revoked", Kind::Denied, 3, 19),
        ("stop", Kind::Stop, 0, 25),
        ("close", Kind::Close, 3, 0),
    ] {
        let mut f = initial.request(kind, seq);
        f.code = code;
        if kind == Kind::Denied {
            f.generation = 2;
        }
        let bytes = f.encode();
        assert_eq!(Frame::decode(&bytes).unwrap(), f);
        std::fs::write(out.join(format!("{name}.frame")), &bytes).unwrap();
        std::fs::write(out.join(format!("{name}.capnp")), &bytes[4..]).unwrap();
        println!("{name} {} {}", bytes.len(), hex(&digest(&bytes)));
    }
    let bad = 4096u32.to_le_bytes();
    std::fs::write(out.join("oversize-prefix.invalid"), bad).unwrap();
    let mut bad = initial.encode();
    bad.truncate(bad.len() - 1);
    assert!(Frame::decode(&bad).is_err());
    std::fs::write(out.join("truncated.invalid"), bad).unwrap();
    let mut bad = initial.encode();
    bad[8..16].fill(255);
    assert!(Frame::decode(&bad).is_err());
    std::fs::write(out.join("bad-root.invalid"), bad).unwrap();
}
