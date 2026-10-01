use morrow_core::{
    Error,
    channel::{self, Action, Budget, Directory, Endpoint, Frame, Kind, Request, Response, Status},
    plugin_package::{
        Package,
        proto::{Manifest, TransformHandler},
    },
    task::Transform,
};
use prost::Message;
const MODULE: &[u8] = b"\0asm\x01\0\0\0";
fn request(action: Action) -> Request {
    Request {
        call_id: [1; 32],
        reference: [2; 32],
        source_epoch: [3; 32],
        action,
    }
}
fn response(request: &Request, status: Status) -> Response {
    Response {
        call_id: request.call_id,
        request_sha256: request.digest().unwrap(),
        reference: request.reference,
        source_epoch: request.source_epoch,
        status,
        frame: None,
        last_acked: 0,
        accepted_sequence: 0,
        resource_reclaimed: false,
    }
}
fn manifest() -> Manifest {
    let mut value = Package::manifest_for_transform(
        "channel.fixture",
        "1.0.0",
        MODULE,
        vec![TransformHandler {
            handler: "channel.exercise".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            max_input_bytes: 65,
            max_output_bytes: 64,
        }],
    );
    value.required_features.push(channel::FEATURE.into());
    value.channel_declaration = Some(channel::declaration(
        vec!["channel.exercise".into()],
        vec![Kind::ByteStream, Kind::Events],
    ));
    value
}
#[test]
fn public_channel_actions_and_maximum_frames_roundtrip_in_one_segment() {
    for action in [
        Action::Receive {
            last_acked: 2,
            credit_bytes: 65536,
        },
        Action::Ack {
            sequence: 3,
            frame_sha256: [4; 32],
            cursor: vec![5; 256],
        },
        Action::Send {
            sequence: 1,
            bytes: vec![6; 65536],
        },
        Action::Close,
        Action::Query,
    ] {
        let value = request(action);
        let bytes = value.encode().unwrap();
        assert_eq!(&bytes[..4], &0u32.to_le_bytes());
        assert_eq!(Request::decode(&bytes).unwrap(), value);
    }
    let frame = Frame {
        sequence: 3,
        source_epoch: [3; 32],
        bytes: vec![7; 65536],
        cursor: vec![8; 256],
    };
    assert_eq!(Frame::decode(&frame.encode().unwrap()).unwrap(), frame);
    let receive = request(Action::Receive {
        last_acked: 2,
        credit_bytes: 65536,
    });
    let mut receipt = response(&receive, Status::Frame);
    receipt.last_acked = 2;
    receipt.frame = Some(frame);
    let raw = receipt.encode().unwrap();
    assert_eq!(&raw[..4], &0u32.to_le_bytes());
    assert_eq!(Response::decode(&raw).unwrap(), receipt);
    receipt.validate_for(&receive).unwrap();
    let directory = Directory {
        scope_sha256: [9; 32],
        channels: vec![Endpoint {
            reference: [2; 32],
            source_epoch: [3; 32],
            kind: Kind::Events,
            budget: Budget::default(),
        }],
    };
    assert_eq!(
        Directory::decode(&directory.encode().unwrap()).unwrap(),
        directory
    );
}
#[test]
fn malformed_unknown_noncanonical_and_trailing_channel_wire_rejects() {
    let raw = request(Action::Query).encode().unwrap();
    for length in 0..raw.len() {
        assert!(Request::decode(&raw[..length]).is_err());
    }
    let mut trailing = raw.clone();
    trailing.extend_from_slice(&[0; 8]);
    assert!(Request::decode(&trailing).is_err());
    let mut padding = raw.clone();
    let words = u32::from_le_bytes(padding[4..8].try_into().unwrap());
    padding[4..8].copy_from_slice(&(words + 1).to_le_bytes());
    padding.extend_from_slice(&[0; 8]);
    assert!(Request::decode(&padding).is_err());
    let mut extra = 1u32.to_le_bytes().to_vec();
    extra.extend_from_slice(&words.to_le_bytes());
    extra.extend_from_slice(&1u32.to_le_bytes());
    extra.extend_from_slice(&0u32.to_le_bytes());
    extra.extend_from_slice(&raw[8..]);
    extra.extend_from_slice(&[0; 8]);
    assert!(Request::decode(&extra).is_err());
    let mut version = raw.clone();
    version[16] = 2;
    assert!(Request::decode(&version).is_err());
    assert!(Request::decode(&vec![0; channel::MAX_WIRE_BYTES + 1]).is_err());
    for action in [
        Action::Receive {
            last_acked: 0,
            credit_bytes: 0,
        },
        Action::Receive {
            last_acked: 0,
            credit_bytes: 65537,
        },
        Action::Ack {
            sequence: 0,
            frame_sha256: [4; 32],
            cursor: vec![],
        },
        Action::Send {
            sequence: 1,
            bytes: vec![0; 65537],
        },
    ] {
        assert!(request(action).encode().is_err());
    }
}
#[test]
fn response_correlates_call_request_reference_epoch_action_and_credit() {
    let req = request(Action::Send {
        sequence: 5,
        bytes: b"hello".to_vec(),
    });
    let mut ack = response(&req, Status::Accepted);
    ack.accepted_sequence = 5;
    ack.validate_for(&req).unwrap();
    for case in 0..6 {
        let mut v = ack.clone();
        match case {
            0 => v.call_id = [10; 32],
            1 => v.request_sha256 = [10; 32],
            2 => v.reference = [10; 32],
            3 => v.source_epoch = [10; 32],
            4 => v.accepted_sequence = 6,
            _ => v.status = Status::Acked,
        };
        assert!(v.validate_for(&req).is_err());
    }
    let req = request(Action::Receive {
        last_acked: 0,
        credit_bytes: 2,
    });
    let mut v = response(&req, Status::Frame);
    v.frame = Some(Frame {
        sequence: 1,
        source_epoch: [3; 32],
        bytes: b"abc".to_vec(),
        cursor: vec![],
    });
    assert!(v.validate_for(&req).is_err());
    let req = request(Action::Close);
    let mut v = response(&req, Status::ClosingUnconfirmed);
    v.resource_reclaimed = true;
    assert!(v.encode().is_err());
    v.status = Status::Unknown;
    v.encode().unwrap();
    v.validate_for(&req).unwrap();
}
#[test]
fn channel_metadata_is_opt_in_strict_and_never_a_pure_transform_grant() {
    let package = Package::build(manifest(), MODULE).unwrap();
    assert!(package.channel_declaration().is_some());
    let input = Transform {
        handler: "channel.exercise".into(),
        input_type: "bytes".into(),
        output_type: "bytes".into(),
        input: vec![0; 65],
    };
    package.channel_handler(&input).unwrap();
    assert!(package.transform_handler(&input).is_err());
    let legacy = Package::build(
        Package::manifest_for_task("legacy.fixture", "1.0.0", MODULE, vec![]),
        MODULE,
    )
    .unwrap();
    assert!(legacy.channel_declaration().is_none());
    for case in 0..12 {
        let mut m = manifest();
        match case {
            0 => m.required_features.retain(|f| f != channel::FEATURE),
            1 => m.channel_declaration = None,
            2 => {
                m.channel_declaration
                    .as_mut()
                    .unwrap()
                    .channel_schema_sha256[0] ^= 1
            }
            3 => m.channel_declaration.as_mut().unwrap().channel_version = 2,
            4 => {
                m.channel_declaration
                    .as_mut()
                    .unwrap()
                    .budget
                    .as_mut()
                    .unwrap()
                    .max_requests = 0
            }
            5 => {
                m.channel_declaration
                    .as_mut()
                    .unwrap()
                    .budget
                    .as_mut()
                    .unwrap()
                    .max_frame_bytes = 65537
            }
            6 => m.channel_declaration.as_mut().unwrap().kinds.push(3),
            7 => m
                .channel_declaration
                .as_mut()
                .unwrap()
                .handlers
                .push("missing.metadata".into()),
            8 => m.required_features.push("dependencies-v1".into()),
            9 => {
                m.io_declaration = Some(morrow_core::plugin_package::io::declaration(
                    vec![morrow_core::plugin_package::io::IoCapability::FileRead],
                    vec!["file.read".into()],
                ))
            }
            10 => m.guest_abi_version = 1,
            _ => m.channel_declaration.as_mut().unwrap().budget = None,
        };
        assert!(Package::build(m, MODULE).is_err(), "case {case}");
    }
}
#[test]
fn duplicate_unknown_and_nonminimal_channel_declarations_reject() {
    let m = manifest();
    let declaration = m.channel_declaration.as_ref().unwrap().encode_to_vec();
    let mut duplicate = m.encode_to_vec();
    duplicate.extend_from_slice(&[0xaa, 0x01]);
    prost::encoding::encode_varint(declaration.len() as u64, &mut duplicate);
    duplicate.extend_from_slice(&declaration);
    assert!(Package::from_parts(&duplicate, MODULE).is_err());
    for case in 0..4 {
        let mut m = manifest();
        m.channel_declaration = None;
        let mut raw = m.encode_to_vec();
        let mut child = declaration.clone();
        if case == 0 {
            child.extend_from_slice(&[0xa0, 0x06, 1]);
        } else if case == 1 {
            assert_eq!(&child[..2], &[8, 1]);
            child.splice(1..2, [0x81, 0]);
        }
        raw.extend_from_slice(if case == 2 {
            &[0xaa, 0x81, 0]
        } else {
            &[0xaa, 0x01]
        });
        let mut length = Vec::new();
        prost::encoding::encode_varint(child.len() as u64, &mut length);
        if case == 3 {
            *length.last_mut().unwrap() |= 0x80;
            length.push(0);
        }
        raw.extend_from_slice(&length);
        raw.extend_from_slice(&child);
        assert!(Package::from_parts(&raw, MODULE).is_err());
    }
    let mut raw = manifest().encode_to_vec();
    raw.extend_from_slice(&[0xa0, 0x06, 7]);
    let p = Package::from_parts(&raw, MODULE).unwrap();
    assert_eq!(p.manifest_bytes(), raw);
    assert!(matches!(
        request(Action::Receive {
            last_acked: 0,
            credit_bytes: 0
        })
        .encode(),
        Err(Error::Limit)
    ));
}
