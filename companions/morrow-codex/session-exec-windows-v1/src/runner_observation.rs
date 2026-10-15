//! Same-reviewed-invocation data link; contains no native owner, executor,
//! backend, admission, process, Job or lease. No map or global latest lookup.
use codex_exec_server::{RunnerObservationReader,RunnerObservation,RunnerObservationAvailability};
use morrow_agent_session_exec_v1_r2::safe_exec::ToolIdentity;
use morrow_core::dispatch::HostBinding;
use sha2::{Digest,Sha256};
use std::sync::{Arc,OnceLock,atomic::{AtomicBool,Ordering}};

fn identity(v:&ToolIdentity)->Option<[u8;32]> {
    // Match the existing protocol text bound without validating/changing its
    // execution state. Even an arbitrary read argument has bounded hash work.
    if [v.operation_id.len(),v.execution_domain.len(),v.session_id.len()].iter()
        .any(|n|*n>morrow_agent_session_exec_v1_r2::MAX_TEXT_BYTES) { return None; }
    let mut h=Sha256::new();h.update(b"C28_ORIGINAL_REVIEWED_RUNNER_HISTORY_V1");
    for p in [v.operation_id.as_bytes(),v.proposal_sha256.as_slice(),v.intent_sha256.as_slice(),
              v.execution_domain.as_bytes(),v.session_id.as_bytes(),v.original_issuer_nonce.as_slice()] {
        h.update((p.len() as u64).to_le_bytes());h.update(p);
    }
    h.update(v.session_epoch.to_le_bytes());h.update(v.generation.to_le_bytes());
    Some(h.finalize().into())
}
struct Bound {
    identity:[u8;32],
    reader:RunnerObservationReader,
    key:Option<codex_exec_server::RunnerObservationKey>,
}
impl Bound {
    fn read_for(&self,v:&ToolIdentity)->RunnerObservation {
        if identity(v)!=Some(self.identity) {
            return RunnerObservation{availability:RunnerObservationAvailability::WrongInstance,
                key:None,revision:None,snapshot:None};
        }
        self.reader.read_for(self.key)
    }
}
struct Link {
    owner:HostBinding,
    once:OnceLock<Bound>,
    invalid:AtomicBool,
    binding_claimed:AtomicBool,
}
/// Historical data endpoint created with the original reviewed invocation.
/// Copying it grants no control and does not retain any native resource.
#[derive(Clone)]
pub struct ReviewedRunnerObservation { link:Arc<Link> }
impl ReviewedRunnerObservation {
    pub(crate) fn new(owner:HostBinding)->Self {
        Self { link:Arc::new(Link{owner,once:OnceLock::new(),invalid:AtomicBool::new(false),binding_claimed:AtomicBool::new(false)}) }
    }
    pub(crate) fn bind_actual(&self,v:&ToolIdentity,reader:RunnerObservationReader) {
        // At most one initializer can enter OnceLock::set; duplicate binds never
        // wait on it or replace the original, and become unavailable history.
        if self.link.binding_claimed.compare_exchange(false,true,Ordering::SeqCst,Ordering::SeqCst).is_err() {
            self.link.invalid.store(true,Ordering::SeqCst);
            return;
        }
        let Some(identity)=identity(v) else {
            self.link.invalid.store(true,Ordering::SeqCst);
            return;
        };
        let key=reader.key();
        if self.link.once.set(Bound{identity,reader,key}).is_err() {
            self.link.invalid.store(true,Ordering::SeqCst);
        }
    }
    pub(crate) fn read_for(&self,owner:HostBinding,v:&ToolIdentity)->RunnerObservation {
        let unavailable=|availability|RunnerObservation{availability,key:None,revision:None,snapshot:None};
        if self.link.owner!=owner || self.link.invalid.load(Ordering::SeqCst) {
            return unavailable(RunnerObservationAvailability::WrongInstance);
        }
        let Some(bound)=self.link.once.get() else {
            return unavailable(RunnerObservationAvailability::NotObserved);
        };
        bound.read_for(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generation_session_and_issuer_changes_are_not_same_reviewed_identity() {
        let original=ToolIdentity{operation_id:"pure".into(),proposal_sha256:[1;32],
            intent_sha256:[2;32],execution_domain:"ordinary-test".into(),session_id:"same".into(),
            session_epoch:3,generation:4,original_issuer_nonce:[5;32]};
        let bound=Bound{identity:identity(&original).unwrap(),
            reader:RunnerObservationReader::unsupported(),key:None};
        assert_eq!(bound.read_for(&original).availability,RunnerObservationAvailability::Unsupported);
        let mut changed=original.clone();changed.generation+=1;
        assert_eq!(bound.read_for(&changed).availability,RunnerObservationAvailability::WrongInstance);
        assert!(bound.read_for(&changed).snapshot.is_none());
        assert_ne!(identity(&original),identity(&changed));
        changed=original.clone();changed.session_epoch+=1;
        assert_ne!(identity(&original),identity(&changed));
        changed=original.clone();changed.original_issuer_nonce[0]^=1;
        assert_ne!(identity(&original),identity(&changed));
        changed=original.clone();changed.session_id="different-session".into();
        assert_eq!(bound.read_for(&changed).availability,RunnerObservationAvailability::WrongInstance);
        changed=original.clone();
        changed.operation_id="x".repeat(morrow_agent_session_exec_v1_r2::MAX_TEXT_BYTES+1);
        assert!(identity(&changed).is_none());
        assert_eq!(bound.read_for(&changed).availability,RunnerObservationAvailability::WrongInstance);
    }
}
