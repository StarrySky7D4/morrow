use morrow_core::{
    io::Header,
    service_resources::{Directory, Endpoint, HEADER, MAX_FRAME_BYTES},
};

fn sample(n: usize) -> Directory {
    Directory {
        scope_sha256: [7; 32],
        endpoints: (1..=n)
            .map(|i| Endpoint {
                reference: format!("{i:064x}"),
                credential: vec![b'a'; 64],
                methods: ["DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT"]
                    .map(str::to_owned)
                    .to_vec(),
                max_request_bytes: 64 * 1024 * 1024,
                max_response_bytes: 64 * 1024 * 1024,
                timeout_ms: 300_000,
                response_frame_limit: 128 * 1024,
            })
            .collect(),
    }
}
#[test]
fn positive_max_boundaries_inclusive_and_service_header_fits() {
    let d = sample(8);
    let bytes = d.encode().unwrap();
    assert!(bytes.len() <= MAX_FRAME_BYTES);
    assert_eq!(Directory::decode(&bytes).unwrap(), d);
    let h = d.to_header().unwrap();
    assert_eq!(
        Directory::from_headers(std::slice::from_ref(&h)).unwrap(),
        Some(d)
    );
    let invocation = morrow_core::service::Invocation {
        service: "svc".into(),
        handler: "api".into(),
        principal: "p".into(),
        method: "POST".into(),
        target: "/".into(),
        headers: vec![h],
        body: vec![],
    };
    let request = morrow_core::service::Request::encode(1, &invocation).unwrap();
    morrow_core::service::Request::decode(request.bytes()).unwrap();
}
#[test]
fn invalid_endpoint_sets_and_credentials() {
    assert!(sample(1).encode().is_ok());
    assert!(sample(0).encode().is_err());
    assert!(sample(9).encode().is_err());
    for case in 0..10 {
        let mut d = sample(2);
        match case {
            0 => d.endpoints[1] = d.endpoints[0].clone(),
            1 => d.endpoints.swap(0, 1),
            2 => d.endpoints[0].reference = "A".repeat(64),
            3 => d.endpoints[0].reference = "0".repeat(64),
            4 => d.endpoints[0].methods = vec!["GET".into(), "GET".into()],
            5 => d.endpoints[0].methods = vec!["POST".into(), "GET".into()],
            6 => d.endpoints[0].methods = vec!["BOGUS".into()],
            7 => d.endpoints[0].credential = vec![b'a'; 257],
            8 => d.endpoints[0].credential = vec![0],
            _ => d.scope_sha256 = [0; 32],
        }
        assert!(d.encode().is_err(), "case {case}");
    }
    let mut d = sample(1);
    d.endpoints[0].credential = vec![b'a'; 256];
    assert!(d.encode().is_ok());
    d.endpoints[0].credential.clear();
    assert!(d.encode().is_ok());
}
#[test]
fn invalid_numeric_limits() {
    for index in 0..4 {
        for v in [0, u64::MAX] {
            let mut d = sample(1);
            match index {
                0 => d.endpoints[0].max_request_bytes = v,
                1 => d.endpoints[0].max_response_bytes = v,
                2 => d.endpoints[0].timeout_ms = v,
                _ => d.endpoints[0].response_frame_limit = v,
            }
            assert!(d.encode().is_err());
        }
    }
}
#[test]
fn truncated_trailing_and_oversize_frames_reject() {
    let bytes = sample(2).encode().unwrap();
    for i in 0..bytes.len() {
        assert!(Directory::decode(&bytes[..i]).is_err(), "prefix {i}");
    }
    let mut trailing = bytes;
    trailing.extend_from_slice(&[0; 8]);
    assert!(Directory::decode(&trailing).is_err());
    assert!(Directory::decode(&vec![0; MAX_FRAME_BYTES + 1]).is_err());
}
#[test]
fn header_duplicates_and_noncanonical_hex_reject() {
    assert_eq!(Directory::from_headers(&[]).unwrap(), None);
    let h = sample(1).to_header().unwrap();
    let mut upper_name = h.clone();
    upper_name.name = HEADER.to_uppercase();
    assert!(Directory::from_headers(&[h.clone(), upper_name]).is_err());
    let mut upper_hex = h.value.clone();
    upper_hex.make_ascii_uppercase();
    assert_ne!(upper_hex, h.value);
    for value in [
        upper_hex,
        b"abc".to_vec(),
        vec![b'0'; 8194],
        vec![],
        b"xx".to_vec(),
    ] {
        assert!(
            Directory::from_headers(&[Header {
                name: HEADER.into(),
                value
            }])
            .is_err()
        );
    }
}
#[test]
fn unknown_version_and_digest_reject_even_in_canonical_frame() {
    use morrow_core::service_resources_capnp as wire;
    for version in [false, true] {
        let bytes = sample(1).encode().unwrap();
        let reader =
            capnp::serialize::read_message(&mut bytes.as_slice(), Default::default()).unwrap();
        let root = reader.get_root::<wire::directory::Reader<'_>>().unwrap();
        let mut message = capnp::message::Builder::new_default();
        message.set_root(root).unwrap();
        let mut root = message.get_root::<wire::directory::Builder<'_>>().unwrap();
        if version {
            root.set_version(2);
        } else {
            root.set_schema_sha256(&[8; 32]);
        }
        assert!(matches!(
            Directory::decode(&capnp::serialize::write_message_to_words(&message)),
            Err(morrow_core::Error::UnsupportedVersion)
        ));
    }
}
