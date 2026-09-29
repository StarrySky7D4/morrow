use morrow_native_http_stream_wire::{Frame, Kind, Payload, Progress};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

pub type Result<T> = std::result::Result<T, &'static str>;

/// Exactly one admitted session and one writer per lane. The shared session
/// supervisor owns this value; a disposable read future never owns sequences.
pub struct Admission {
    initial: Frame,
    deadline: Instant,
    first_read_started: Instant,
    control_next: u64,
    data_next: u64,
    host_data_next: u64,
    host_generation: u64,
    host_remaining_ms: u64,
    pending: BTreeMap<u64, Kind>,
    welcome: bool,
    cancelled: bool,
}
impl Admission {
    pub fn new(
        challenge: Frame,
        first_read_started: Instant,
        actual_pid: u32,
        actual_artifact: &[u8; 32],
    ) -> Result<Self> {
        challenge.validate()?;
        if challenge.kind != Kind::Challenge
            || challenge.sequence != 0
            || challenge.code != 0
            || challenge.revocation_generation != 1
            || challenge.child_pid != actual_pid
            || challenge.artifact_sha256 != actual_artifact
            || challenge.remaining_ms == 0
            || challenge.request_budget < 8
        {
            return Err("initial Challenge identity/profile");
        }
        let deadline = first_read_started
            .checked_add(Duration::from_millis(challenge.remaining_ms))
            .ok_or("deadline overflow")?;
        if deadline <= Instant::now() {
            return Err("expired initial Challenge");
        }
        Ok(Self {
            host_remaining_ms: challenge.remaining_ms,
            initial: challenge,
            deadline,
            first_read_started,
            control_next: 1,
            data_next: 1,
            host_data_next: 1,
            host_generation: 1,
            pending: BTreeMap::new(),
            welcome: false,
            cancelled: false,
        })
    }
    pub fn initial(&self) -> &Frame {
        &self.initial
    }
    pub fn first_read_started(&self) -> Instant {
        self.first_read_started
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn cancelled(&self) -> bool {
        self.cancelled
    }
    pub fn welcomed(&self) -> bool {
        self.welcome
    }
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    fn original(&self, kind: Kind, sequence: u64, payload: Payload) -> Result<Frame> {
        let mut frame = self.initial.clone();
        frame.kind = kind;
        frame.sequence = sequence;
        frame.payload = payload;
        // Native guest control/data echoes original remaining/budget/generation;
        // those values never refresh the host or this local deadline.
        frame.validate()?;
        Ok(frame)
    }
    pub fn control(&mut self, kind: Kind, payload: Payload) -> Result<Frame> {
        let cleanup = matches!(kind, Kind::Query | Kind::Close | Kind::HttpCancel)
            || matches!(&payload, Payload::Credit(c) if kind == Kind::HttpCredit && c.window_bytes == 0);
        if (self.cancelled || Instant::now() >= self.deadline) && !cleanup {
            return Err("no new work after cancel/expiry");
        }
        if !self.welcome && !(kind == Kind::Hello && self.control_next == 1) {
            return Err("Hello/Welcome first");
        }
        if kind == Kind::Hello && self.control_next != 1 {
            return Err("duplicate Hello");
        }
        let expected = match kind {
            Kind::Hello => Kind::Welcome,
            Kind::Query | Kind::Close | Kind::HttpPrepare | Kind::HttpCommit => Kind::State,
            Kind::HttpCredit => Kind::CreditState,
            Kind::HttpCancel => Kind::CancelAccepted,
            _ => return Err("guest control direction"),
        };
        if self.control_next > self.initial.request_budget.min(128) {
            return Err("control request budget");
        }
        // Keep one reserved ordinal for cancellation and one for Close. ACKs
        // are coalesced by the session writer before entering this method.
        if !cleanup && self.control_next > self.initial.request_budget.min(128).saturating_sub(2) {
            return Err("reserved cleanup request budget");
        }
        let frame = self.original(kind, self.control_next, payload)?;
        self.pending.insert(self.control_next, expected);
        self.control_next += 1;
        Ok(frame)
    }
    pub fn data(&mut self, kind: Kind, payload: Payload) -> Result<Frame> {
        if !self.welcome || self.cancelled || Instant::now() >= self.deadline {
            return Err("data admission expired/cancelled");
        }
        if !matches!((self.data_next, kind), (1, Kind::DataBind))
            && !(self.data_next > 1 && kind == Kind::RequestChunk)
        {
            return Err("guest data direction/sequence");
        }
        let frame = self.original(kind, self.data_next, payload)?;
        self.data_next = self
            .data_next
            .checked_add(1)
            .ok_or("data sequence overflow")?;
        Ok(frame)
    }
    fn stable(&self, frame: &Frame) -> Result<()> {
        frame.validate()?;
        if !frame.same_admission(&self.initial)
            || frame.request_budget != self.initial.request_budget
            || frame.remaining_ms > self.initial.remaining_ms
        {
            return Err("host stable admission/budget/remaining");
        }
        Ok(())
    }
    pub fn host_control(&mut self, frame: &Frame) -> Result<()> {
        self.stable(frame)?;
        if frame.revocation_generation < self.host_generation
            || frame.revocation_generation > 2
            || frame.remaining_ms > self.host_remaining_ms
        {
            return Err("host control generation/time regression");
        }
        if frame.revocation_generation == 2 && self.host_generation == 1 {
            let applied = matches!(&frame.payload, Payload::Progress(p) if p.revoke_applied && p.revoke_persisted);
            if !applied && !matches!(frame.kind, Kind::Stop | Kind::Denied) {
                return Err("unexplained generation change");
            }
            self.cancelled = true;
        }
        if frame.sequence == 0 {
            if !matches!(
                frame.kind,
                Kind::DataOffer
                    | Kind::HttpProposed
                    | Kind::HttpApproved
                    | Kind::ResponseHead
                    | Kind::HttpTerminal
                    | Kind::RequestClosed
                    | Kind::Stop
                    | Kind::Denied
            ) {
                return Err("host notification direction");
            }
        } else {
            let expected = self
                .pending
                .get(&frame.sequence)
                .ok_or("unsolicited/replayed response sequence")?;
            if frame.kind != *expected && frame.kind != Kind::Denied {
                return Err("host direct reply kind");
            }
            self.pending.remove(&frame.sequence);
        }
        if frame.code != 0
            && !matches!(
                frame.kind,
                Kind::Denied
                    | Kind::Stop
                    | Kind::CancelAccepted
                    | Kind::HttpTerminal
                    | Kind::RequestClosed
                    | Kind::State
            )
        {
            return Err("nonzero success response code");
        }
        if frame.kind == Kind::Welcome {
            if self.welcome || frame.sequence != 1 || frame.code != 0 {
                return Err("duplicate/invalid Welcome");
            }
            self.welcome = true;
        } else if !self.welcome && !matches!(frame.kind, Kind::Stop | Kind::Denied) {
            return Err("host message before Welcome");
        }
        if matches!(frame.kind, Kind::Stop | Kind::Denied | Kind::CancelAccepted) {
            self.cancelled = true;
        }
        self.host_remaining_ms = frame.remaining_ms;
        self.host_generation = frame.revocation_generation;
        Ok(())
    }
    /// Data may be older than a control CancelAccepted. Authenticate it, keep
    /// strict lane order, then return false so the owner records a late discard.
    pub fn host_data(&mut self, frame: &Frame) -> Result<bool> {
        self.stable(frame)?;
        if !self.welcome
            || frame.code != 0
            || frame.sequence != self.host_data_next
            || (self.host_data_next == 1 && self.data_next != 2)
            || frame.revocation_generation == 0
            || frame.revocation_generation > self.host_generation
        {
            return Err("host data admission/sequence");
        }
        if !matches!((self.host_data_next, frame.kind), (1, Kind::DataBound))
            && !(self.host_data_next > 1 && frame.kind == Kind::BodyChunk)
        {
            return Err("host data direction");
        }
        self.host_data_next = self
            .host_data_next
            .checked_add(1)
            .ok_or("host data sequence overflow")?;
        Ok(!self.cancelled
            && Instant::now() < self.deadline
            && frame.revocation_generation == self.host_generation)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn admitted() -> Admission {
        let initial = Frame {
            kind: Kind::Challenge,
            sequence: 0,
            session: 1,
            instance_epoch: 2,
            revocation_generation: 1,
            child_pid: std::process::id(),
            code: 0,
            remaining_ms: 30000,
            nonce: vec![7; 32],
            schema_sha256: morrow_native_http_stream_wire::schema_digest().to_vec(),
            artifact_sha256: vec![8; 32],
            execution_config_sha256: vec![9; 32],
            request_budget: 128,
            capabilities: 3,
            operation_id: b"local-state-test".to_vec(),
            attempt: 1,
            payload: Payload::None,
        };
        let mut value = Admission::new(
            initial.clone(),
            Instant::now(),
            std::process::id(),
            &[8; 32],
        )
        .unwrap();
        value.control(Kind::Hello, Payload::None).unwrap();
        let mut welcome = initial;
        welcome.kind = Kind::Welcome;
        welcome.sequence = 1;
        value.host_control(&welcome).unwrap();
        value
    }
    #[test]
    fn original_generation_cleanup_is_allowed_without_renewing_work() {
        let mut session = admitted();
        session.cancel();
        let credit = morrow_native_http_stream_wire::Credit {
            consumed_offset: 0,
            parser_yielded_bytes: 0,
            drain_discarded_bytes: 0,
            error_consumed_bytes: 0,
            cancel_discarded_bytes: 0,
            window_bytes: 0,
            max_chunk_bytes: 1024,
        };
        let frame = session
            .control(Kind::HttpCredit, Payload::Credit(credit.clone()))
            .unwrap();
        assert_eq!(frame.revocation_generation, 1);
        assert_eq!(frame.remaining_ms, 30000);
        let mut positive = credit;
        positive.window_bytes = 1;
        assert!(
            session
                .control(Kind::HttpCredit, Payload::Credit(positive))
                .is_err()
        );
        assert!(session.control(Kind::HttpCommit, Payload::None).is_err());
        assert!(session.control(Kind::Query, Payload::None).is_ok());
        assert!(session.control(Kind::HttpCancel, Payload::None).is_ok());
        assert!(session.control(Kind::Close, Payload::None).is_ok());
    }
    #[test]
    fn stable_identity_does_not_allow_replayed_or_wrong_direction_control() {
        let mut session = admitted();
        let request = session.control(Kind::Query, Payload::None).unwrap();
        let mut reply = session.initial().clone();
        reply.kind = Kind::Welcome;
        reply.sequence = request.sequence;
        assert!(session.host_control(&reply).is_err());
        reply.sequence = 1;
        assert!(session.host_control(&reply).is_err());
    }
}

/// Cross-message evidence can accumulate but cannot rewrite durable history.
pub fn progress_follows(old: &Progress, new: &Progress) -> Result<()> {
    use morrow_native_http_stream_wire::IntentPhase;
    if old.intent == IntentPhase::Observed && new.intent != IntentPhase::Observed {
        return Err("Observed regression");
    }
    let offsets = [
        (old.received_offset, new.received_offset),
        (old.reserved_offset, new.reserved_offset),
        (old.issued_offset, new.issued_offset),
        (old.os_completed_offset, new.os_completed_offset),
        (old.peer_consumed_offset, new.peer_consumed_offset),
        (old.parser_yielded_bytes, new.parser_yielded_bytes),
        (old.drain_discarded_bytes, new.drain_discarded_bytes),
        (old.error_consumed_bytes, new.error_consumed_bytes),
        (old.cancel_discarded_bytes, new.cancel_discarded_bytes),
        (old.last_write_ordinal, new.last_write_ordinal),
    ];
    if offsets.into_iter().any(|(a, b)| b < a) {
        return Err("host progress counter regression");
    }
    let facts = [
        (old.http_eof, new.http_eof),
        (old.response_material_stored, new.response_material_stored),
        (old.revoke_persisted, new.revoke_persisted),
        (old.revoke_applied, new.revoke_applied),
        (old.worker_started, new.worker_started),
        (old.worker_joined, new.worker_joined),
        (old.data_closed, new.data_closed),
        (old.request_closed, new.request_closed),
        (old.owner_released, new.owner_released),
    ];
    if facts.into_iter().any(|(a, b)| a && !b) {
        return Err("host durable/completion fact regression");
    }
    // reaped can become false again when a fresh authorized I/O begins. A true
    // requestClosed already fixes all reaped facts through the sealed codec.
    Ok(())
}
