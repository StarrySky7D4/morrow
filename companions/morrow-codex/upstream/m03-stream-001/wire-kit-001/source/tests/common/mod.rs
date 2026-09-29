use morrow_native_http_stream_wire::*;
pub fn progress() -> Progress {
    Progress {
        intent: IntentPhase::Unknown,
        network: NetworkPhase::Streaming,
        owner: OwnerPhase::Active,
        http_status: 200,
        error_code: 0,
        received_offset: 3,
        reserved_offset: 3,
        issued_offset: 3,
        os_completed_offset: 3,
        peer_consumed_offset: 0,
        parser_yielded_bytes: 0,
        drain_discarded_bytes: 0,
        cancel_discarded_bytes: 0,
        last_write_ordinal: 1,
        revoke_persisted: false,
        revoke_applied: false,
        http_eof: false,
        response_material_stored: false,
        worker_joined: false,
        connect_reaped: true,
        read_reaped: true,
        write_reaped: true,
        data_closed: false,
        child_exited: false,
        stdout_eof: false,
        stderr_eof: false,
        owner_released: false,
        worker_started: true,
        error_consumed_bytes: 0,
        request_closed: false,
    }
}
pub fn initial() -> Frame {
    Frame {
        kind: Kind::Challenge,
        sequence: 0,
        session: 1,
        instance_epoch: 2,
        revocation_generation: 1,
        child_pid: 42,
        code: 0,
        remaining_ms: 10000,
        nonce: vec![7; 32],
        schema_sha256: schema_digest().to_vec(),
        artifact_sha256: vec![8; 32],
        execution_config_sha256: vec![9; 32],
        request_budget: 128,
        capabilities: 3,
        operation_id: b"synthetic-operation-001".to_vec(),
        attempt: 1,
        payload: Payload::None,
    }
}
pub fn prepare() -> Prepare {
    Prepare {
        method: "POST".into(),
        absolute_target: "http://127.0.0.1:12345/responses".into(),
        headers: vec![Header {
            name: "content-type".into(),
            value: b"application/json".to_vec(),
        }],
        body_bytes: 2,
        body_sha256: digest(b"{}").to_vec(),
        response_limit_bytes: 65536,
    }
}
pub fn cases() -> Vec<(&'static str, Frame)> {
    let mut out = Vec::new();
    let original = initial();
    for (name, kind) in [
        ("challenge", Kind::Challenge),
        ("hello", Kind::Hello),
        ("welcome", Kind::Welcome),
        ("query", Kind::Query),
        ("denied", Kind::Denied),
        ("stop", Kind::Stop),
        ("close", Kind::Close),
        ("http-cancel", Kind::HttpCancel),
    ] {
        let mut f = original.clone();
        f.kind = kind;
        f.sequence = if matches!(kind, Kind::Challenge | Kind::Stop) {
            0
        } else {
            1
        };
        out.push((name, f));
    }
    for (name, kind) in [
        ("data-offer", Kind::DataOffer),
        ("data-bind", Kind::DataBind),
        ("data-bound", Kind::DataBound),
    ] {
        let mut f = original.clone();
        f.kind = kind;
        f.payload = Payload::Channel(Channel {
            locator: r"\\.\pipe\morrow-m03-synthetic-001".into(),
            nonce: vec![10; 32],
            max_chunk_bytes: 8192,
            credit_limit: 16384,
        });
        out.push((name, f));
    }
    let mut f = original.clone();
    f.kind = Kind::HttpPrepare;
    f.payload = Payload::Prepare(prepare());
    out.push(("http-prepare", f));
    for (name, kind) in [
        ("request-chunk", Kind::RequestChunk),
        ("body-chunk", Kind::BodyChunk),
    ] {
        let mut f = original.clone();
        f.kind = kind;
        f.payload = Payload::Chunk(Chunk {
            offset: 0,
            bytes: b"{}".to_vec(),
        });
        out.push((name, f));
    }
    for (name, kind) in [
        ("http-proposed", Kind::HttpProposed),
        ("http-approved", Kind::HttpApproved),
        ("http-commit", Kind::HttpCommit),
    ] {
        let mut f = original.clone();
        f.kind = kind;
        let proposed = kind == Kind::HttpProposed;
        f.payload = Payload::Decision(Decision {
            proposal_ref: vec![11; 32],
            body_sha256: digest(b"{}").to_vec(),
            response_limit_bytes: 65536,
            request_sha256: request_digest(1, 2, &f.operation_id, 1, &prepare(), b"{}")
                .unwrap()
                .to_vec(),
            http_grant_ref: if proposed { vec![] } else { vec![12; 32] },
            endpoint_ref: if proposed { vec![] } else { vec![13; 32] },
            body_bytes: 2,
            send_budget: if proposed { 0 } else { 1 },
        });
        out.push((name, f));
    }
    let mut f = original.clone();
    f.kind = Kind::ResponseHead;
    f.payload = Payload::Head(Head {
        status: 200,
        headers: vec![Header {
            name: "x-raw".into(),
            value: vec![255],
        }],
        remote_address: "127.0.0.1:12345".into(),
    });
    out.push(("response-head", f));
    let mut f = original.clone();
    f.kind = Kind::HttpCredit;
    f.payload = Payload::Credit(Credit {
        consumed_offset: 0,
        parser_yielded_bytes: 0,
        drain_discarded_bytes: 0,
        cancel_discarded_bytes: 0,
        error_consumed_bytes: 0,
        window_bytes: 16384,
        max_chunk_bytes: 8192,
    });
    out.push(("http-credit", f));
    for (name, kind) in [
        ("state", Kind::State),
        ("credit-state", Kind::CreditState),
        ("cancel-accepted", Kind::CancelAccepted),
        ("http-terminal", Kind::HttpTerminal),
        ("request-closed", Kind::RequestClosed),
    ] {
        let mut f = original.clone();
        f.kind = kind;
        let mut p = progress();
        if kind == Kind::CancelAccepted {
            p.revoke_persisted = true;
            p.revoke_applied = true;
            p.network = NetworkPhase::Cancelled;
        }
        if matches!(kind, Kind::HttpTerminal | Kind::RequestClosed) {
            p.intent = IntentPhase::Observed;
            p.network = NetworkPhase::Eof;
            p.http_eof = true;
            p.response_material_stored = true;
            p.worker_joined = true;
        }
        if kind == Kind::RequestClosed {
            p.request_closed = true;
            p.data_closed = true;
            p.peer_consumed_offset = 3;
            p.parser_yielded_bytes = 3;
        }
        f.payload = Payload::Progress(p);
        out.push((name, f));
    }
    // Standalone samples, not a single execution transcript.
    for (_, f) in &mut out {
        f.sequence = match f.kind {
            Kind::Hello | Kind::Welcome | Kind::DataBind | Kind::DataBound => 1,
            Kind::Query
            | Kind::Denied
            | Kind::HttpPrepare
            | Kind::RequestChunk
            | Kind::BodyChunk
            | Kind::State => 2,
            Kind::HttpCommit | Kind::Close => 3,
            Kind::HttpCredit | Kind::CreditState => 4,
            Kind::HttpCancel | Kind::CancelAccepted => 5,
            _ => 0,
        };
    }
    out
}
