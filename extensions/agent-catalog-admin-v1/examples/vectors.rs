//! Emits canonical Rust frames for cross-language verification. No owner is called.
use morrow_agent_catalog_admin_v1::*;
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn main() {
    let revisions = Revisions {
        catalog: u64::MAX,
        manager: (1u64 << 53) + 19,
    };
    let approval = Approval {
        session_bits: 31,
        process_bits: 127,
        sessions: vec!["session:a".into(), "session:z".into()],
        domain: "local:test".into(),
    };
    let review = Review {
        id: "codex.fixture".into(),
        version: "0.1.0".into(),
        full_sha256: [2; 32],
        base_sha256: [3; 32],
        session_schema: [4; 32],
        process_schema: [5; 32],
        session_bits: 31,
        process_bits: 127,
        sessions: approval.sessions.clone(),
        domain: approval.domain.clone(),
    };
    let cases = vec![
        ("state", Action::State, Body::None),
        (
            "inspect_unicode",
            Action::Inspect {
                path: "C:/插件/完整包.mrowasp".into(),
            },
            Body::Review(review.clone()),
        ),
        (
            "install_max_u64",
            Action::Install {
                path: "C:/插件/完整包.mrowasp".into(),
                full_sha256: [2; 32],
                revisions,
            },
            Body::Review(review.clone()),
        ),
        (
            "approve",
            Action::Approve {
                id: "codex.fixture".into(),
                full_sha256: [2; 32],
                approval: approval.clone(),
                revisions,
            },
            Body::None,
        ),
        (
            "page",
            Action::Page {
                after: None,
                limit: 16,
                revisions,
            },
            Body::Page {
                entries: vec![Entry {
                    review,
                    selected: true,
                    enabled: true,
                    approval: Some(approval),
                    base_selected: true,
                    base_enabled: false,
                }],
                next: Some("02".repeat(32)),
            },
        ),
    ];
    for (name, action, body) in cases {
        let request = Request {
            id: [1; 16],
            action,
        };
        let bytes = request.encode().unwrap();
        assert_eq!(Request::decode(&bytes).unwrap(), request);
        println!("{name}.request.sha256={}", hex(&hash(&bytes)));
        println!("{name}.request.hex={}", hex(&bytes));
        let reply = Reply::new(&request, Outcome::ok(revisions, body)).unwrap();
        let bytes = reply.encode_for(&request).unwrap();
        assert_eq!(Reply::decode_for(&request, &bytes).unwrap(), reply);
        println!("{name}.reply.sha256={}", hex(&hash(&bytes)));
        println!("{name}.reply.hex={}", hex(&bytes));
    }
}
