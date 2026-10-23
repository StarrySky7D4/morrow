use super::super::{
    tests::sample_clean_facts, SchemaDigest, SessionNonce, StartupAttemptBinding, StartupAttemptId,
    VerifiedChildProcessIdentity, NATIVE_CLOSE_PROTOCOL_SCHEMA,
};
use super::*;
use crate::ipc_framed::{
    encode_bytes, ControlHello, ControlHelloAck, RunnerControlCapabilities, StdinPayload,
};

fn binding() -> StartupAttemptBinding {
    StartupAttemptBinding::try_new(
        StartupAttemptId::try_new(u64::MAX).unwrap(),
        SessionNonce::try_new([0x73; 32]).unwrap(),
        SchemaDigest::v8_native_close(),
        NativeCloseCapabilityOptIn::ExplicitlyOptedInV8,
    )
    .unwrap()
}

fn facts() -> ReportedNativeCloseFacts {
    sample_clean_facts(
        u64::MAX,
        VerifiedChildProcessIdentity::try_new(u32::MAX, 17, u64::MAX).unwrap(),
        -1,
        7,
    )
}

fn packet() -> RuntimePacketWire {
    RuntimePacketWire {
        schema: NATIVE_CLOSE_PROTOCOL_SCHEMA,
        attempt: u64::MAX,
        sequence: u64::MAX,
        direction: RuntimeDirection::RunnerToHost,
        event: None,
        content: RuntimePacketContent::Empty,
        terminal_close_ack: None,
        tag: WireAuthTag::from_bytes([0x52; 32]),
    }
}

fn exit() -> Version8WireFrame {
    Version8WireFrame::new(Version8Message::BusinessExit(NativeCloseBusinessExitWire {
        exit_code: i32::MIN,
        timeout_disposition: TimeoutDisposition::TimedOut,
    }))
}

fn roundtrip(frame: &Version8WireFrame) -> Vec<u8> {
    let bytes = encode_v8_capnp_payload(frame).unwrap();
    assert!(bytes.len() <= MAX_NATIVE_CLOSE_FRAME_BYTES);
    assert_eq!(decode_v8_capnp_payload_exact(&bytes).unwrap(), *frame);
    let mut input = bytes.as_slice();
    let message =
        capnp::serialize::read_message_from_flat_slice(&mut input, Default::default()).unwrap();
    assert!(message.is_canonical().unwrap());
    bytes
}

#[test]
fn every_native_frame_and_runtime_content_roundtrips_as_canonical_binary() {
    let binding = binding();
    let child = VerifiedChildProcessIdentity::try_new(u32::MAX, 17, u64::MAX).unwrap();
    let events = [
        Version8Message::HandshakeHello(NativeCloseHandshakeHelloWire::new_signed(&binding)),
        Version8Message::HandshakeAck(NativeCloseHandshakeAckWire::new_signed(&binding)),
        Version8Message::SpawnReady(NativeCloseSpawnReadyWire::new_signed(&binding, child)),
        Version8Message::Output(NativeCloseOutputFrameWire {
            stream: NativeCloseOutputStream::Stderr,
            byte_len: u32::MAX,
        }),
        Version8Message::ControlResult(NativeCloseControlResultWire {
            request_id: u64::MAX,
            status: ControlStatus::Unknown,
        }),
        exit().message,
        Version8Message::NativeCloseReceipt(Box::new(NativeCloseReceiptWire::new_signed(
            &binding,
            facts(),
        ))),
    ];
    for event in events {
        let event = Version8WireFrame::new(event);
        roundtrip(&event);
        let mut p = packet();
        p.event = Some(Box::new(event));
        p.terminal_close_ack = Some(WireAuthTag::from_bytes([0xff; 32]));
        roundtrip(&Version8WireFrame::new(Version8Message::RuntimePacket(
            Box::new(p),
        )));
    }
    for content in [
        RuntimePacketContent::Empty,
        RuntimePacketContent::TerminalAck {
            child: child.to_wire(),
            raw_child_exit_dword: u32::MAX,
        },
        RuntimePacketContent::Message(Box::new(FramedMessage {
            version: 8,
            message: Message::Stdin {
                payload: StdinPayload {
                    data_b64: encode_bytes(&[0, 255, 128, 0]),
                },
            },
        })),
    ] {
        let mut p = packet();
        p.content = content;
        roundtrip(&Version8WireFrame::new(Version8Message::RuntimePacket(
            Box::new(p),
        )));
    }
    let bootstrap = RuntimeBootstrapWire {
        schema: NATIVE_CLOSE_PROTOCOL_SCHEMA,
        attempt: u64::MAX,
        runner: RuntimeRunnerIdentity {
            process_id: u32::MAX,
            thread_id: 17,
            creation_filetime_100ns: u64::MAX,
        },
        seed: [0x92; 32],
        hello: NativeCloseHandshakeHelloWire::new_signed(&binding),
        terminal_close_ack_required: true,
    };
    roundtrip(&Version8WireFrame::new(Version8Message::RuntimeBootstrap(
        Box::new(bootstrap),
    )));
}

#[test]
fn seven_fact_domains_and_unknown_debt_states_survive_and_change_authentication_bytes() {
    let clean = facts();
    let expected = encode_reported_facts_canonical(&clean);
    macro_rules! cases {
        ($($field:ident).+, $values:expr) => {
            for value in $values {
                let mut changed = clean.clone(); changed.$($field).+ = value;
                let encoded = encode_reported_facts_canonical(&changed);
                assert_eq!(encoded == expected, changed == clean);
                let receipt = NativeCloseReceiptWire::new_signed(&binding(), changed);
                roundtrip(&Version8WireFrame::new(Version8Message::NativeCloseReceipt(Box::new(receipt))));
            }
        };
    }
    let identity = WireChildIdentity {
        pid: u32::MAX,
        thread_id: 17,
        creation_filetime_100ns: u64::MAX,
    };
    cases!(
        child_exit,
        [
            ChildNativeExitFact::NotObserved,
            ChildNativeExitFact::NotProven,
            ChildNativeExitFact::NotCreatedPreLaunch,
            ChildNativeExitFact::WaitTimedOutOrIncomplete { identity },
            ChildNativeExitFact::Witnessed {
                identity,
                raw_exit_code_dword: 259,
                business_exit_code_i32: 259,
                wait_on_original_handle: OriginalProcessWaitProof::QueryOnlyWithoutWaitSignal,
                timeout_disposition: TimeoutDisposition::TimedOut
            }
        ]
    );
    cases!(
        io_streams_and_workers.stdout.pipe_eof,
        [
            PipeEofFact::NotObserved,
            PipeEofFact::NotProven,
            PipeEofFact::CleanEofObserved,
            PipeEofFact::ReadFailedBeforeEof { os_error: None },
            PipeEofFact::ReadFailedBeforeEof {
                os_error: Some(i32::MIN)
            }
        ]
    );
    cases!(
        io_streams_and_workers.stdout.reader_join,
        [
            WorkerNativeJoinFact::NotObserved,
            WorkerNativeJoinFact::NotProven,
            WorkerNativeJoinFact::StillRunning,
            WorkerNativeJoinFact::JoinedTimelyClean,
            WorkerNativeJoinFact::JoinedWithWorkerError,
            WorkerNativeJoinFact::JoinedLateAfterBudget,
            WorkerNativeJoinFact::TimedOutStillRunning,
            WorkerNativeJoinFact::WaitFailed {
                os_error: Some(i32::MAX)
            },
            WorkerNativeJoinFact::WaitFailed { os_error: None },
            WorkerNativeJoinFact::Panicked
        ]
    );
    cases!(
        io_streams_and_workers.stdout.handle_close,
        [
            HandleCloseFact::NotObserved,
            HandleCloseFact::NotProven,
            HandleCloseFact::ClosedVerifiedOnce,
            HandleCloseFact::NotApplicableAbsent,
            HandleCloseFact::StillOpen,
            HandleCloseFact::CloseDebtRetained { os_error: None },
            HandleCloseFact::CloseDebtRetained { os_error: Some(-1) }
        ]
    );
    cases!(
        io_streams_and_workers.stderr,
        [
            StderrStreamWorkerFact::MergedConptyOrAbsent {
                handle_close: HandleCloseFact::StillOpen
            },
            StderrStreamWorkerFact::SeparateStream {
                stream: clean.io_streams_and_workers.stdout
            }
        ]
    );
    cases!(
        io_streams_and_workers.input.peer_eof,
        [
            InputPeerEofFact::NotObserved,
            InputPeerEofFact::NotProven,
            InputPeerEofFact::CleanPostChildExitFrameBoundaryEof,
            InputPeerEofFact::PrematureDisconnectBeforeChildExit,
            InputPeerEofFact::PartialFrameAtEof,
            InputPeerEofFact::FrameTooLarge,
            InputPeerEofFact::MalformedFrame,
            InputPeerEofFact::OperationAborted,
            InputPeerEofFact::IoError { os_error: Some(-1) },
            InputPeerEofFact::IoError { os_error: None }
        ]
    );
    cases!(
        handles_close.stdin_closure,
        [
            StdinHandleCloseFact::NotObserved,
            StdinHandleCloseFact::NotProven,
            StdinHandleCloseFact::InitiallyNotOpen,
            StdinHandleCloseFact::ClosedOnceVerified {
                write_disposition: StdinWriteDisposition::AllWritesSucceeded
            },
            StdinHandleCloseFact::ClosedOnceVerified {
                write_disposition: StdinWriteDisposition::WriteFailedBeforeClose
            },
            StdinHandleCloseFact::StillOpen,
            StdinHandleCloseFact::ClosedMultipleTimes {
                close_count: u32::MAX
            },
            StdinHandleCloseFact::CloseDebtRetained {
                close_count: 7,
                os_error: Some(i32::MIN)
            }
        ]
    );
    cases!(
        job_lifecycle.admission,
        [
            JobAdmissionFact::NotObserved,
            JobAdmissionFact::NotProven,
            JobAdmissionFact::JobMissing,
            JobAdmissionFact::AdmittedVerifiedOriginalJob {
                job_instance_id: u64::MAX,
                child_membership_verified: ObservationProof::NotProven
            }
        ]
    );
    cases!(
        job_lifecycle.active_zero_drain,
        [
            JobActiveZeroDrainFact::NotObserved,
            JobActiveZeroDrainFact::NotProven,
            JobActiveZeroDrainFact::DrainedToActiveZero {
                total_processes: u32::MAX,
                active_processes: 9,
                total_terminated_processes: 17,
                ordering_after_child_handle_close: StageOrderingFact::OrderViolated
            },
            JobActiveZeroDrainFact::DrainTimedOut {
                active_processes: u32::MAX
            },
            JobActiveZeroDrainFact::DrainFailed { os_error: Some(-1) }
        ]
    );
    cases!(
        writers.business_exit_writer,
        [
            BusinessExitWriterFact::NotObserved,
            BusinessExitWriterFact::NotProven,
            BusinessExitWriterFact::JoinedTimelyBeforeReceipt {
                exit_code_i32: i32::MIN
            },
            BusinessExitWriterFact::StillRunning,
            BusinessExitWriterFact::WriteOrJoinFailed,
            BusinessExitWriterFact::Panicked
        ]
    );
    cases!(
        writers.prior_control_and_frame_writers,
        [
            PriorControlWritersFact::NotObserved,
            PriorControlWritersFact::NotProven,
            PriorControlWritersFact::AllJoinedAndQuiesced {
                control_results_written: u16::MAX
            },
            PriorControlWritersFact::WriterStillRunning,
            PriorControlWritersFact::ControlWriteFailed,
            PriorControlWritersFact::PrecontextWriterRetained
        ]
    );
    cases!(
        leases.attempt_lease,
        [
            AttemptLeaseRetirementFact::NotObserved,
            AttemptLeaseRetirementFact::NotProven,
            AttemptLeaseRetirementFact::ActiveOrLoaned,
            AttemptLeaseRetirementFact::ParkedUnresolved,
            AttemptLeaseRetirementFact::ReadyToRetireOnFinalReceiptFlush {
                startup_attempt_id: 17
            }
        ]
    );
    cases!(
        leases.quota_lease,
        [
            LeaseRetirementFact::NotObserved,
            LeaseRetirementFact::NotProven,
            LeaseRetirementFact::ActiveOrLoaned,
            LeaseRetirementFact::ParkedUnresolved,
            LeaseRetirementFact::OutOfRunnerScopePendingHostOwner,
            LeaseRetirementFact::RetiredCleanly
        ]
    );
    cases!(
        debt_and_sticky_unknown.close_debt,
        [
            CloseDebtFact::NotObserved,
            CloseDebtFact::NotProven,
            CloseDebtFact::ZeroDebtProven,
            CloseDebtFact::DebtRetained {
                debt_count: u8::MAX,
                first_target: CloseDebtTarget::HostStartupPipeAlias,
                first_os_error: Some(i32::MIN)
            }
        ]
    );
    cases!(
        debt_and_sticky_unknown.first_error,
        [
            FirstErrorFact::NotObserved,
            FirstErrorFact::NoneObserved,
            FirstErrorFact::Recorded {
                kind: ProtocolStickyFailureKind::InputMalformedFrame,
                stage: ProtocolFailureStage::InputDrainAndJoin,
                os_error: Some(i32::MAX),
                replay_policy: ReplayDisposition::NoReplaySealed
            }
        ]
    );
    cases!(
        debt_and_sticky_unknown.poison_state,
        [
            PoisonStateFact::NotObserved,
            PoisonStateFact::NotProven,
            PoisonStateFact::NoPoisonObserved,
            PoisonStateFact::PoisonObserved {
                target: PoisonTarget::HostOwnerMutex
            }
        ]
    );
    cases!(
        debt_and_sticky_unknown.sticky_unknown_state,
        [
            StickyUnknownStateFact::NotObserved,
            StickyUnknownStateFact::NotProven,
            StickyUnknownStateFact::Clear,
            StickyUnknownStateFact::LatchedStickyUnknown {
                replay_policy: ReplayDisposition::NoReplaySealed
            }
        ]
    );
}

#[test]
fn packet_authentication_covers_all_fields_except_its_own_tag() {
    let baseline = packet();
    let expected = encode_runtime_packet_auth_body(&baseline).unwrap();
    for index in 0..8 {
        let mut changed = baseline.clone();
        match index {
            0 => changed.schema[0] ^= 1,
            1 => changed.attempt -= 1,
            2 => changed.sequence -= 1,
            3 => changed.direction = RuntimeDirection::HostToRunner,
            4 => changed.event = Some(Box::new(exit())),
            5 => {
                changed.content = RuntimePacketContent::TerminalAck {
                    child: WireChildIdentity {
                        pid: 1,
                        thread_id: 2,
                        creation_filetime_100ns: 3,
                    },
                    raw_child_exit_dword: 259,
                }
            }
            6 => changed.terminal_close_ack = Some(WireAuthTag::from_bytes([3; 32])),
            _ => changed.tag = WireAuthTag::from_bytes([4; 32]),
        }
        assert_eq!(
            expected == encode_runtime_packet_auth_body(&changed).unwrap(),
            index == 7
        );
    }
}

#[test]
fn rejects_noncanonical_unknown_fields_truncation_and_invalid_pointers() {
    let bytes = roundtrip(&exit());
    for end in 0..bytes.len() {
        assert!(decode_v8_capnp_payload_exact(&bytes[..end]).is_err());
    }
    let mut tail = bytes.clone();
    tail.extend([0; 8]);
    assert_eq!(
        decode_v8_capnp_payload_exact(&tail),
        Err(NativeCloseCodecError::TrailingBytesInPayload)
    );
    // Insert a real future data field in the root struct. It remains canonical
    // Cap'n Proto, but the pinned schema cannot preserve it on exact re-encoding.
    let mut future = bytes.clone();
    let data_words = u16::from_le_bytes(future[12..14].try_into().unwrap());
    assert_eq!(data_words, 2);
    future[12..14].copy_from_slice(&(data_words + 1).to_le_bytes());
    future.splice(
        16 + usize::from(data_words) * 8..16 + usize::from(data_words) * 8,
        [0x53; 8],
    );
    let words = u32::from_le_bytes(future[4..8].try_into().unwrap()) + 1;
    future[4..8].copy_from_slice(&words.to_le_bytes());
    let mut input = future.as_slice();
    assert!(
        capnp::serialize::read_message_from_flat_slice(&mut input, Default::default())
            .unwrap()
            .is_canonical()
            .unwrap()
    );
    assert_eq!(
        decode_v8_capnp_payload_exact(&future),
        Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::NonCanonicalEncoding
        ))
    );
    let mut invalid = bytes.clone();
    invalid[8..16].fill(255);
    assert!(decode_v8_capnp_payload_exact(&invalid).is_err());
    let mut padding = bytes.clone();
    padding[30] = 1;
    assert_eq!(
        decode_v8_capnp_payload_exact(&padding),
        Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::NonCanonicalEncoding
        ))
    );
}

#[test]
fn rejects_wrong_envelope_identity_and_unknown_enum_or_union_tags() {
    let bytes = roundtrip(&exit());
    for index in 0..4 {
        let mut input = bytes.as_slice();
        let message =
            capnp::serialize::read_message_from_flat_slice(&mut input, Default::default()).unwrap();
        let mut builder = capnp::message::Builder::new_default();
        builder
            .set_root(
                message
                    .get_root::<wire::native_close_envelope::Reader<'_>>()
                    .unwrap(),
            )
            .unwrap();
        let mut root = builder
            .get_root::<wire::native_close_envelope::Builder<'_>>()
            .unwrap();
        match index {
            0 => root.set_magic(0),
            1 => root.set_wire_revision(2),
            2 => root.set_wire_format(wire::WireFormatKind::Unspecified),
            _ => root.set_envelope_kind(wire::EnvelopeKind::RuntimePacketAuthBody),
        }
        assert_eq!(
            decode_v8_capnp_payload_exact(&canonical_bytes(builder).unwrap()),
            Err(NativeCloseCodecError::UnknownWireIdentity)
        );
    }
    for offset in [26, 28] {
        let mut invalid = bytes.clone();
        invalid[offset..offset + 2].fill(255);
        assert_eq!(
            decode_v8_capnp_payload_exact(&invalid),
            Err(NativeCloseCodecError::UnknownSchemaVariant)
        );
    }
    let a = Version8WireFrame::new(Version8Message::Output(NativeCloseOutputFrameWire {
        stream: NativeCloseOutputStream::Stdout,
        byte_len: 0,
    }));
    let b = Version8WireFrame::new(Version8Message::ControlResult(
        NativeCloseControlResultWire {
            request_id: 0,
            status: ControlStatus::Applied,
        },
    ));
    let mut left = roundtrip(&a);
    let right = roundtrip(&b);
    let diff: Vec<_> = left
        .iter()
        .zip(&right)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(diff.len(), 1);
    let offset = diff[0];
    left[offset..offset + 2].fill(255);
    assert_eq!(
        decode_v8_capnp_payload_exact(&left),
        Err(NativeCloseCodecError::UnknownSchemaVariant)
    );
}

#[test]
fn rejects_unredacted_embedded_nonce_on_both_encoding_and_decoding() {
    let mut p = packet();
    p.content = RuntimePacketContent::Message(Box::new(FramedMessage {
        version: 8,
        message: Message::HelloAck {
            payload: ControlHelloAck {
                hello: ControlHello {
                    nonce: [0; 32],
                    schema: crate::ipc_framed::CHECKED_CONTROL_SCHEMA,
                },
                controls: RunnerControlCapabilities::default(),
            },
        },
    }));
    let frame = Version8WireFrame::new(Version8Message::RuntimePacket(Box::new(p.clone())));
    let bytes = roundtrip(&frame);
    let RuntimePacketContent::Message(framed) = &mut p.content else {
        unreachable!()
    };
    let Message::HelloAck { payload } = &mut framed.message else {
        unreachable!()
    };
    payload.hello.nonce = [0x91; 32];
    assert!(matches!(
        encode_runtime_packet_auth_body(&p),
        Err(NativeCloseCodecError::EncodeFailed(
            EncodeErrorKind::UnredactedWireNonce
        ))
    ));
    let RuntimePacketContent::Message(framed) = &p.content else {
        unreachable!()
    };
    let hostile_inner = crate::runner_wire::encode_frame_payload(framed).unwrap();
    let mut input = bytes.as_slice();
    let message =
        capnp::serialize::read_message_from_flat_slice(&mut input, Default::default()).unwrap();
    let mut builder = capnp::message::Builder::new_default();
    builder
        .set_root(
            message
                .get_root::<wire::native_close_envelope::Reader<'_>>()
                .unwrap(),
        )
        .unwrap();
    let root = builder
        .get_root::<wire::native_close_envelope::Builder<'_>>()
        .unwrap();
    let wire::version_eight_wire_frame::RuntimePacket(packet) =
        root.get_frame().unwrap().which().unwrap()
    else {
        unreachable!()
    };
    packet
        .unwrap()
        .get_content()
        .unwrap()
        .set_runner_message(&hostile_inner);
    assert_eq!(
        decode_v8_capnp_payload_exact(&canonical_bytes(builder).unwrap()),
        Err(NativeCloseCodecError::MalformedCapnp(
            MalformedCapnpKind::UnredactedWireNonce
        ))
    );
}

#[test]
fn exact_frame_limit_and_recursive_runtime_event_are_rejected_without_fallback() {
    let mut accepted_boundary = false;
    let mut rejected_boundary = false;
    for len in (MAX_NATIVE_CLOSE_FRAME_BYTES - 1024..=MAX_NATIVE_CLOSE_FRAME_BYTES).step_by(8) {
        let mut p = packet();
        p.content = RuntimePacketContent::Message(Box::new(FramedMessage {
            version: 8,
            message: Message::Stdin {
                payload: StdinPayload {
                    data_b64: encode_bytes(&vec![0x45; len]),
                },
            },
        }));
        let frame = Version8WireFrame::new(Version8Message::RuntimePacket(Box::new(p)));
        match encode_v8_capnp_payload(&frame) {
            Ok(bytes) => {
                if bytes.len() == MAX_NATIVE_CLOSE_FRAME_BYTES {
                    roundtrip(&frame);
                    accepted_boundary = true;
                }
            }
            Err(_) => rejected_boundary = true,
        }
    }
    assert!(accepted_boundary && rejected_boundary);
    let mut p = packet();
    p.event = Some(Box::new(Version8WireFrame::new(
        Version8Message::RuntimePacket(Box::new(packet())),
    )));
    assert!(matches!(
        encode_runtime_packet_auth_body(&p),
        Err(NativeCloseCodecError::EncodeFailed(
            EncodeErrorKind::RecursiveRuntimePacket
        ))
    ));
}

#[test]
fn hostile_deep_pointer_chain_and_all_legacy_json_are_bounded_and_redacted() {
    // A canonical chain of pointer-only structs, deeper than the reader limit.
    let depth = MAX_NATIVE_CLOSE_CAPNP_DEPTH + 8;
    let mut words = Vec::new();
    for _ in 0..depth {
        words.push(1u64 << 48);
    }
    words.push(0xffff_fffcu64); // canonical empty struct
    let mut deep = vec![0; 4];
    deep.extend_from_slice(&(words.len() as u32).to_le_bytes());
    for word in words {
        deep.extend_from_slice(&word.to_le_bytes());
    }
    assert!(matches!(
        decode_v8_capnp_payload_exact(&deep),
        Err(NativeCloseCodecError::MaxCapnpDepthExceeded { .. })
    ));
    for raw in [
        r#"{"version":8,"PRIVATE_WIRE_MARKER":"SECRET_NONCE"}"#,
        r#" ["SECRET_NONCE"]"#,
        r#""SECRET_NONCE""#,
    ] {
        let error = decode_v8_capnp_payload_exact(raw.as_bytes()).unwrap_err();
        assert_eq!(
            error,
            NativeCloseCodecError::MalformedCapnp(MalformedCapnpKind::NonCapnpOrJsonInput)
        );
        assert!(!format!("{error:?} {error}").contains("SECRET_NONCE"));
    }
}
