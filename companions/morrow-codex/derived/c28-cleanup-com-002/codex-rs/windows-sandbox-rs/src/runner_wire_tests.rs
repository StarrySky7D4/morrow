use super::*;
use std::io::Cursor;

fn spawn(profile: PermissionProfile) -> FramedMessage {
    FramedMessage {
        version: 8,
        message: Message::SpawnRequest {
            payload: Box::new(SpawnRequest {
                command: vec!["cmd.exe".into(), "/c".into(), "echo 中文".into()],
                cwd: PathBuf::from(r"D:\工作 空间"),
                env: HashMap::from([("Z".into(), "last".into()), ("A".into(), "first".into())]),
                permission_profile: profile,
                workspace_roots: vec![
                    AbsolutePathBuf::from_absolute_path(r"D:\工作 空间").unwrap(),
                ],
                codex_home: PathBuf::from(r"D:\codex"),
                real_codex_home: PathBuf::from(r"D:\real"),
                cap_sids: vec!["S-1-15-3-1024-1".into()],
                network_proxy_restricting_sid: Some("S-1-5-21-1".into()),
                timeout_ms: Some(u64::MAX),
                tty: false,
                stdin_open: true,
                private_desktop_name: Some("".into()),
            }),
        },
    }
}
fn roundtrip(frame: &FramedMessage) {
    let bytes = encode_frame_payload(frame).unwrap();
    assert_eq!(decode_frame_payload(&bytes).unwrap(), *frame);
    let mut framed = Vec::new();
    write_frame(&mut framed, frame).unwrap();
    let mut cursor = Cursor::new(&framed);
    assert_eq!(read_frame(&mut cursor).unwrap(), Some(frame.clone()));
    assert!(read_frame(&mut cursor).unwrap().is_none());
    assert_eq!(&framed[4..], bytes);
}
#[test]
fn every_runner_message_uses_pinned_binary_transport() {
    let hello = ControlHello {
        nonce: [3; 32],
        schema: CHECKED_CONTROL_SCHEMA,
    };
    let request = ControlRequest::new(u64::MAX, [3; 32], ControlOperation::Resize, 25, 80);
    let messages = vec![
        Message::Hello {
            payload: hello.clone(),
        },
        Message::HelloAck {
            payload: ControlHelloAck {
                hello,
                controls: RunnerControlCapabilities {
                    close_stdin: true,
                    resize: true,
                },
            },
        },
        Message::ControlRequest {
            payload: request.clone(),
        },
        Message::ControlResult {
            payload: ControlResult::new(request, ControlStatus::Unknown, Some(i32::MIN)),
        },
        Message::SpawnReady {
            payload: SpawnReady {
                process_id: u32::MAX,
                controls: RunnerControlCapabilities::default(),
            },
        },
        Message::Output {
            payload: OutputPayload {
                data_b64: encode_bytes(&[0, 255, 1, 128, 0]),
                stream: OutputStream::Stderr,
            },
        },
        Message::Stdin {
            payload: StdinPayload {
                data_b64: encode_bytes(&[0, 255, 2, 0]),
            },
        },
        Message::CloseStdin {
            payload: EmptyPayload {},
        },
        Message::Resize {
            payload: ResizePayload {
                rows: u16::MAX,
                cols: 0,
            },
        },
        Message::Exit {
            payload: ExitPayload {
                exit_code: i32::MIN,
                timed_out: true,
            },
        },
        Message::Error {
            payload: ErrorPayload {
                message: "有限错误".into(),
                stage: ErrorStage::TerminalWitness,
                windows_error_code: Some(u32::MAX),
            },
        },
        Message::Terminate {
            payload: EmptyPayload {},
        },
    ];
    for version in [6, 7, 8] {
        for message in &messages {
            roundtrip(&FramedMessage {
                version,
                message: message.clone(),
            });
        }
    }
}
#[test]
fn permission_variants_and_path_semantics_roundtrip_without_json() {
    let paths = vec![
        FileSystemPath::from(AbsolutePathBuf::from_absolute_path(r"D:\a b\中").unwrap()),
        FileSystemPath::GlobPattern {
            pattern: "**/private/**".into(),
        },
        FileSystemPath::Special {
            value: FileSystemSpecialPath::Root,
        },
        FileSystemPath::Special {
            value: FileSystemSpecialPath::Minimal,
        },
        FileSystemPath::Special {
            value: FileSystemSpecialPath::Tmpdir,
        },
        FileSystemPath::Special {
            value: FileSystemSpecialPath::SlashTmp,
        },
        FileSystemPath::Special {
            value: FileSystemSpecialPath::ProjectRoots { subpath: None },
        },
        FileSystemPath::Special {
            value: FileSystemSpecialPath::ProjectRoots {
                subpath: Some("src".into()),
            },
        },
        FileSystemPath::Special {
            value: FileSystemSpecialPath::Unknown {
                path: ":future".into(),
                subpath: Some("".into()),
            },
        },
    ];
    let entries = paths
        .into_iter()
        .enumerate()
        .map(|(i, path)| FileSystemSandboxEntry {
            path,
            access: [
                FileSystemAccessMode::Read,
                FileSystemAccessMode::Write,
                FileSystemAccessMode::Deny,
            ][i % 3],
            missing_path_behavior: (i % 2 == 0)
                .then_some(FileSystemSandboxEntryMissingPathBehavior::Skip),
        })
        .collect();
    let profiles = [
        PermissionProfile::Disabled,
        PermissionProfile::External {
            network: NetworkSandboxPolicy::Enabled,
        },
        PermissionProfile::Managed {
            file_system: ManagedFileSystemPermissions::Unrestricted,
            network: NetworkSandboxPolicy::Restricted,
        },
        PermissionProfile::Managed {
            file_system: ManagedFileSystemPermissions::Restricted {
                entries,
                glob_scan_max_depth: NonZeroUsize::new(17),
            },
            network: NetworkSandboxPolicy::Enabled,
        },
        PermissionProfile::read_only(),
    ];
    for p in profiles {
        roundtrip(&spawn(p));
    }
}
#[test]
fn environment_order_does_not_change_authenticated_payload() {
    let a = spawn(PermissionProfile::read_only());
    let mut b = a.clone();
    let Message::SpawnRequest { payload } = &mut b.message else {
        unreachable!()
    };
    payload.env = HashMap::from([("A".into(), "first".into()), ("Z".into(), "last".into())]);
    assert_eq!(
        encode_frame_payload(&a).unwrap(),
        encode_frame_payload(&b).unwrap()
    );
}
#[test]
fn json_trailing_truncated_and_oversized_input_is_never_eof_or_dispatch() {
    let old = br#"{"version":8,"type":"terminate","payload":{}}"#;
    assert!(decode_frame_payload(old).is_err());
    let mut framed = (old.len() as u32).to_le_bytes().to_vec();
    framed.extend_from_slice(old);
    assert!(read_frame(framed.as_slice()).is_err());
    let frame = spawn(PermissionProfile::read_only());
    let bytes = encode_frame_payload(&frame).unwrap();
    let mut complete = Vec::new();
    write_frame(&mut complete, &frame).unwrap();
    assert!(read_frame(&[][..]).unwrap().is_none());
    for n in 1..complete.len() {
        assert!(read_frame(&complete[..n]).is_err(), "partial length {n}");
    }
    for extra in [vec![0], bytes.clone(), vec![0; 8]] {
        let mut tail = bytes.clone();
        tail.extend(extra);
        assert!(decode_frame_payload(&tail).is_err());
    }
    assert!(read_frame((MAX_FRAME_BYTES as u32 + 1).to_le_bytes().as_slice()).is_err());
    assert!(read_frame([0u8; 4].as_slice()).is_err());
    assert!(decode_frame_payload(&[0xff; 64]).is_err());
}
#[test]
fn schema_version_magic_and_unknown_union_cannot_be_accepted() {
    let frame = spawn(PermissionProfile::read_only());
    let bytes = encode_frame_payload(&frame).unwrap();
    for action in 0..3 {
        let mut input = bytes.as_slice();
        let message =
            capnp::serialize::read_message_from_flat_slice(&mut input, Default::default()).unwrap();
        let r = message.get_root::<wire::envelope::Reader<'_>>().unwrap();
        let mut builder = capnp::message::Builder::new_default();
        builder.set_root(r).unwrap();
        let mut b = builder.get_root::<wire::envelope::Builder<'_>>().unwrap();
        match action {
            0 => b.set_magic(0),
            1 => b.set_wire_revision(2),
            _ => b.set_schema(&[9; 32]),
        }
        assert!(decode_frame_payload(&capnp::serialize::write_message_to_words(&builder)).is_err());
    }
    // The schema's union occupies the 16-bit slot after its three scalar fields.
    // Derive its location from a controlled discriminant difference, then inject
    // an unknown tag instead of relying on a generated Rust enum constructor.
    let a = FramedMessage {
        version: 8,
        message: Message::CloseStdin {
            payload: EmptyPayload {},
        },
    };
    let b = FramedMessage {
        version: 8,
        message: Message::Terminate {
            payload: EmptyPayload {},
        },
    };
    let mut a_bytes = encode_frame_payload(&a).unwrap();
    let b_bytes = encode_frame_payload(&b).unwrap();
    let diff: Vec<_> = a_bytes
        .iter()
        .zip(&b_bytes)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(diff.len(), 1);
    let offset = diff[0];
    a_bytes[offset] = 255;
    a_bytes[offset + 1] = 255;
    assert!(decode_frame_payload(&a_bytes).is_err());
}
#[test]
fn diagnostic_never_contains_untrusted_text() {
    let marker = "PRIVATE_RUNNER_FRAME_MARKER_2387";
    let input = format!(r#"{{"secret":"{marker}"}}"#);
    let error = decode_frame_payload(input.as_bytes()).unwrap_err();
    assert!(!format!("{error:?} {error:#}").contains(marker));
}
#[test]
fn encoder_enforces_text_list_and_aggregate_bounds() {
    let mut frame = spawn(PermissionProfile::read_only());
    let Message::SpawnRequest { payload } = &mut frame.message else {
        unreachable!()
    };
    payload.command = vec!["x".repeat(MAX_TEXT_BYTES + 1)];
    assert!(encode_frame_payload(&frame).is_err());
    let Message::SpawnRequest { payload } = &mut frame.message else {
        unreachable!()
    };
    payload.command = vec!["x".into(); MAX_ITEMS as usize + 1];
    assert!(encode_frame_payload(&frame).is_err());
    let Message::SpawnRequest { payload } = &mut frame.message else {
        unreachable!()
    };
    payload.command = vec!["x".repeat(MAX_TEXT_BYTES); 65];
    assert!(encode_frame_payload(&frame).is_err());
}
