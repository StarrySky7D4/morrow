//! Cleanup-only continuation after expiry. Never changes the business gate or deadline.
use crate::wire as w;
use std::time::Instant;
pub(super) struct ExpiryTeardown { deadline: Instant }
impl ExpiryTeardown {
    pub(super) fn new(deadline: Instant) -> Self { Self { deadline } }
    pub(super) fn deadline(&self) -> Instant { self.deadline }
    pub(super) fn permits(&self, now: Instant, kind: w::Kind, payload: &w::Payload) -> bool {
        now < self.deadline && (
            matches!((kind,payload),(w::Kind::Query | w::Kind::HttpCancel | w::Kind::Close,w::Payload::None))
            || matches!((kind,payload),(w::Kind::HttpCredit,w::Payload::Credit(c)) if c.window_bytes==0)
        )
    }
}
pub(super) fn request_reaped(p: &w::Progress) -> bool {
    p.request_closed && p.revoke_persisted && p.revoke_applied && p.data_closed
        && p.connect_reaped && p.read_reaped && p.write_reaped
        && (!p.worker_started || p.worker_joined)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn fixed_cleanup_deadline_never_renews_and_all_business_kinds_are_denied() {
        let start=Instant::now();let deadline=start+Duration::from_millis(50);
        let t=ExpiryTeardown::new(deadline);
        for kind in [w::Kind::Query,w::Kind::Close,w::Kind::HttpCancel] {
            assert!(t.permits(start,kind,&w::Payload::None));
            assert!(!t.permits(deadline,kind,&w::Payload::None));
        }
        for kind in [w::Kind::Hello,w::Kind::HttpPrepare,w::Kind::HttpCommit,w::Kind::DataBind,w::Kind::RequestChunk] {
            assert!(!t.permits(start,kind,&w::Payload::None));
        }
        assert_eq!(t.deadline(),deadline);
    }
    #[test]
    fn cancel_and_partial_resource_recovery_cannot_authorize_terminal_ack() {
        let mut p=super::super::progress();
        p.revoke_persisted=true;p.revoke_applied=true;
        assert!(!request_reaped(&p));
        p.request_closed=true;p.data_closed=true;p.connect_reaped=true;p.read_reaped=true;p.write_reaped=true;
        assert!(request_reaped(&p));
        p.worker_started=true;assert!(!request_reaped(&p));
        p.worker_joined=true;assert!(request_reaped(&p));
        p.read_reaped=false;assert!(!request_reaped(&p));
    }
}
