use crate::*;

fn headers(values: &[Header]) -> Result<()> {
    if values.len() > 32 {
        return Err("header count");
    }
    let mut sum = 0usize;
    for h in values {
        if h.name.is_empty() || h.name.len() > 128 || !h.name.is_ascii() || h.value.len() > 8192 {
            return Err("header size/name");
        }
        // HTTP grammar, denied names and approval allowlist are additionally checked by host.
        sum = sum
            .checked_add(h.name.len() + h.value.len() + 4)
            .ok_or("header sum")?;
        if sum > 8192 {
            return Err("header bytes");
        }
    }
    Ok(())
}
fn digest32(value: &[u8]) -> Result<()> {
    if value.len() != 32 {
        Err("digest/reference length")
    } else {
        Ok(())
    }
}
impl Frame {
    pub fn validate(&self) -> Result<()> {
        for bytes in [
            &self.nonce,
            &self.schema_sha256,
            &self.artifact_sha256,
            &self.execution_config_sha256,
        ] {
            digest32(bytes)?;
        }
        if self.schema_sha256 != schema_digest()
            || self.session == 0
            || self.instance_epoch == 0
            || self.revocation_generation == 0
            || self.child_pid == 0
            || self.capabilities != 3
            || self.operation_id.is_empty()
            || self.operation_id.len() > 256
            || self.attempt != 1
            || self.remaining_ms > 60000
            || self.request_budget > 128
        {
            return Err("identity/profile");
        }
        let compatible = match (&self.payload, self.kind) {
            (
                Payload::None,
                Kind::Challenge
                | Kind::Hello
                | Kind::Welcome
                | Kind::Query
                | Kind::Denied
                | Kind::Stop
                | Kind::Close
                | Kind::HttpCancel,
            ) => true,
            (Payload::Channel(_), Kind::DataOffer | Kind::DataBind | Kind::DataBound) => true,
            (Payload::Prepare(_), Kind::HttpPrepare) => true,
            (Payload::Chunk(_), Kind::RequestChunk | Kind::BodyChunk) => true,
            (Payload::Decision(_), Kind::HttpProposed | Kind::HttpApproved | Kind::HttpCommit) => {
                true
            }
            (Payload::Head(_), Kind::ResponseHead) => true,
            (Payload::Credit(_), Kind::HttpCredit) => true,
            (
                Payload::Progress(_),
                Kind::State
                | Kind::CreditState
                | Kind::CancelAccepted
                | Kind::HttpTerminal
                | Kind::RequestClosed,
            ) => true,
            _ => false,
        };
        if !compatible {
            return Err("kind/payload");
        }
        match &self.payload {
            Payload::None => {}
            Payload::Channel(c) => {
                digest32(&c.nonce)?;
                if c.locator.len() > 240
                    || !c.locator.starts_with(r"\\.\pipe\morrow-m03-")
                    || c.locator.chars().any(char::is_control)
                    || c.max_chunk_bytes == 0
                    || c.max_chunk_bytes > MAX_CHUNK as u32
                    || c.credit_limit != MAX_CREDIT
                {
                    return Err("channel bounds");
                }
            }
            Payload::Prepare(p) => {
                headers(&p.headers)?;
                digest32(&p.body_sha256)?;
                if p.method.is_empty()
                    || p.method.len() > 16
                    || !p.method.is_ascii()
                    || p.absolute_target.is_empty()
                    || p.absolute_target.len() > 2048
                    || (p.body_bytes as usize > MAX_BODY
                        || p.response_limit_bytes == 0
                        || p.response_limit_bytes as usize > MAX_RESPONSE)
                {
                    return Err("request bounds");
                }
            }
            Payload::Chunk(c) => {
                let limit = if self.kind == Kind::RequestChunk {
                    MAX_BODY
                } else {
                    MAX_RESPONSE
                };
                if c.bytes.is_empty()
                    || c.bytes.len() > MAX_CHUNK
                    || c.offset
                        .checked_add(c.bytes.len() as u64)
                        .is_none_or(|n| n > limit as u64)
                {
                    return Err("chunk bounds");
                }
            }
            Payload::Decision(d) => {
                for bytes in [&d.proposal_ref, &d.body_sha256, &d.request_sha256] {
                    digest32(bytes)?;
                }
                if (d.body_bytes as usize > MAX_BODY
                    || d.response_limit_bytes == 0
                    || d.response_limit_bytes as usize > MAX_RESPONSE)
                {
                    return Err("decision size");
                }
                if self.kind == Kind::HttpProposed {
                    if !d.http_grant_ref.is_empty()
                        || !d.endpoint_ref.is_empty()
                        || d.send_budget != 0
                    {
                        return Err("proposal grants nothing");
                    }
                } else {
                    digest32(&d.http_grant_ref)?;
                    digest32(&d.endpoint_ref)?;
                    if d.send_budget != 1 {
                        return Err("send budget");
                    }
                }
            }
            Payload::Head(h) => {
                headers(&h.headers)?;
                if !(100..=599).contains(&h.status)
                    || h.remote_address.is_empty()
                    || h.remote_address.len() > 128
                {
                    return Err("head bounds");
                }
            }
            Payload::Credit(c) => {
                let classified = c
                    .parser_yielded_bytes
                    .checked_add(c.drain_discarded_bytes)
                    .and_then(|n| n.checked_add(c.error_consumed_bytes))
                    .and_then(|v| v.checked_add(c.cancel_discarded_bytes))
                    .ok_or("credit sum")?;
                if classified != c.consumed_offset
                    || c.consumed_offset > MAX_RESPONSE as u64
                    || c.window_bytes > MAX_CREDIT
                    || c.max_chunk_bytes == 0
                    || c.max_chunk_bytes > MAX_CHUNK as u32
                {
                    return Err("credit bounds");
                }
            }
            Payload::Progress(p) => {
                if [
                    p.received_offset,
                    p.reserved_offset,
                    p.issued_offset,
                    p.os_completed_offset,
                    p.peer_consumed_offset,
                ]
                .iter()
                .any(|n| *n > MAX_RESPONSE as u64)
                    || p.os_completed_offset > p.issued_offset
                    || p.peer_consumed_offset > p.issued_offset
                    || p.issued_offset > p.reserved_offset
                    || p.reserved_offset > p.received_offset
                {
                    return Err("progress offsets");
                }
                if p.parser_yielded_bytes
                    .checked_add(p.drain_discarded_bytes)
                    .and_then(|n| n.checked_add(p.error_consumed_bytes))
                    .and_then(|n| n.checked_add(p.cancel_discarded_bytes))
                    != Some(p.peer_consumed_offset)
                {
                    return Err("progress classified");
                }
                if p.http_status != 0 && !(100..=599).contains(&p.http_status) {
                    return Err("progress status");
                }
                if p.intent == IntentPhase::Observed && (!p.http_eof || !p.response_material_stored)
                {
                    return Err("observed evidence");
                }
                if (self.kind == Kind::RequestClosed && !p.request_closed)
                    || (p.request_closed
                        && ((p.worker_started && !p.worker_joined)
                            || !p.connect_reaped
                            || !p.read_reaped
                            || !p.write_reaped
                            || !p.data_closed))
                {
                    return Err("request cleanup facts");
                }
                if p.worker_joined && !p.worker_started {
                    return Err("worker facts");
                }
                if p.owner_released
                    && ((p.worker_started && !p.worker_joined)
                        || !p.connect_reaped
                        || !p.read_reaped
                        || !p.write_reaped
                        || !p.data_closed
                        || !p.child_exited
                        || !p.stdout_eof
                        || !p.stderr_eof
                        || p.owner != OwnerPhase::Released)
                {
                    return Err("release facts");
                }
                if p.revoke_applied && !p.revoke_persisted {
                    return Err("revoke order");
                }
            }
        }
        Ok(())
    }
    /// Admission tuple only. Direction, channel, monotonic sequences, payload state and
    /// remaining/budget echo rules must separately be enforced by the session driver.
    pub fn same_admission(&self, initial: &Self) -> bool {
        self.session == initial.session
            && self.instance_epoch == initial.instance_epoch
            && self.child_pid == initial.child_pid
            && self.nonce == initial.nonce
            && self.schema_sha256 == initial.schema_sha256
            && self.artifact_sha256 == initial.artifact_sha256
            && self.execution_config_sha256 == initial.execution_config_sha256
            && self.operation_id == initial.operation_id
            && self.attempt == initial.attempt
            && self.capabilities == initial.capabilities
    }
}

/// Fixed hash preimage, independent of JSON key order or Capnp padding. Preserve the
/// original prepared body and ordered header pairs; guest hashes are never authority.
pub fn request_digest(
    session: u64,
    epoch: u64,
    operation: &[u8],
    attempt: u64,
    prepare: &Prepare,
    body: &[u8],
) -> Result<[u8; 32]> {
    headers(&prepare.headers)?;
    if body.len() > MAX_BODY
        || body.len() != prepare.body_bytes as usize
        || prepare.response_limit_bytes == 0
        || prepare.response_limit_bytes as usize > MAX_RESPONSE
        || digest(body).as_slice() != prepare.body_sha256
        || operation.is_empty()
        || operation.len() > 256
        || session == 0
        || epoch == 0
        || attempt != 1
        || prepare.method.len() > 16
        || prepare.absolute_target.len() > 2048
    {
        return Err("request digest input");
    }
    fn field(out: &mut Vec<u8>, bytes: &[u8]) {
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    let mut out = b"Morrow/native-http-proposal/v1\0".to_vec();
    out.extend_from_slice(&session.to_le_bytes());
    out.extend_from_slice(&epoch.to_le_bytes());
    field(&mut out, operation);
    out.extend_from_slice(&attempt.to_le_bytes());
    field(&mut out, prepare.method.as_bytes());
    field(&mut out, prepare.absolute_target.as_bytes());
    out.extend_from_slice(&(prepare.headers.len() as u32).to_le_bytes());
    for h in &prepare.headers {
        field(&mut out, h.name.as_bytes());
        field(&mut out, &h.value);
    }
    out.extend_from_slice(&prepare.response_limit_bytes.to_le_bytes());
    field(&mut out, body);
    Ok(digest(&out))
}
