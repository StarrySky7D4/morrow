//! Narrow adaptation of pinned harness wrapper/catalog policy. Same Workbench only.
use super::qualification_session_bootstrap as bootstrap;
use morrow_workbench_host::{Result, Workbench};
use morrow_agent_catalog_admin_v1::{Action, Approval, Body, Request, Revisions, Review, Status};
use morrow_agent_session_exec_v1_r2::authority::Capabilities;
use morrow_agent_session_process_v1_host::{AgentProcessPackage, Declaration};
use morrow_core::plugin_package::Package;
use std::{fs::{File, OpenOptions}, io::Write};

pub struct Reviewed {
    pub full_sha: [u8; 32], pub revisions: Revisions,
    _wrapper_pin: File,
}
struct Calls { next: u128, mutations: u8, unresolved: bool }
impl Calls {
    fn call(&mut self, host: &mut Workbench, action: Action) -> Result<morrow_agent_catalog_admin_v1::Reply> {
        if self.unresolved || self.mutations >= 8 { return Err("qualification catalog fenced/bound".into()); }
        let q = Request { id: self.next.to_le_bytes(), action };
        self.next = self.next.checked_add(1).ok_or("qualification catalog ID overflow")?;
        let mutation = q.is_mutation();
        if mutation { self.mutations += 1; self.unresolved = true; }
        let raw = q.encode()?;
        let response = morrow_workbench_host::protocol::respond(host, &raw)?;
        let reply = morrow_agent_catalog_admin_v1::Reply::decode_for(&q, &response)?;
        if mutation && reply.outcome.status != Status::Unknown { self.unresolved = false; }
        if reply.outcome.status != Status::Ok { return Err("qualification catalog non-OK; no replay".into()); }
        Ok(reply)
    }
}
/// Caller permanently reserves Start before entering. Every stage is attempted once.
pub fn review_once(host: &mut Workbench, admission: &bootstrap::Admission) -> Result<Reviewed> {
    let p = &admission.plan;
    if admission.fixture.len() != 425912 || bootstrap::sha(&admission.fixture) != p.fixture.sha {
        return Err("qualification public immutable guest pin".into());
    }
    let id = p.package_id();
    let sid = p.sid();
    let mut scope = vec![sid.clone(), format!("{sid}-child")]; scope.sort();
    let session = Capabilities { session_read: true, session_write: true, ..Default::default() };
    let base = Package::build(Package::manifest_for_task(&id, "1.0.0", &admission.fixture, vec![]), &admission.fixture)?;
    let full = AgentProcessPackage::build(base, Declaration {
        session, process: Default::default(), sessions: scope.clone(), execution_domain: "session-only".into(),
    })?;
    let full_sha = full.review_sha256();
    let expected = Review {
        id: id.clone(), version: "1.0.0".into(), full_sha256: full_sha,
        base_sha256: full.base_sha256(), session_schema: morrow_agent_session_exec_v1_r2::schema_digest(),
        process_schema: morrow_agent_process_control_v1::schema_digest(), session_bits: 3,
        process_bits: 0, sessions: scope.clone(), domain: "session-only".into(),
    };
    let wrapper = p.root.join("c28-session.wrapper");
    bootstrap::plain_ancestors(&p.root)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(&wrapper)?;
    file.write_all(full.archive())?; file.sync_all()?; drop(file);
    let (wrapper_pin, _) = bootstrap::pinned(&wrapper, full.archive().len() as u64, full_sha, 33554432)?;
    let seed = (u128::from_le_bytes(p.nonce) & (u128::MAX >> 1)).checked_add(1).ok_or("qualification seed overflow")?;
    let mut calls = Calls { next: seed, mutations: 0, unresolved: false };
    let state = calls.call(host, Action::State)?;
    let review = calls.call(host, Action::Inspect { path: wrapper.to_str().ok_or("qualification wrapper Unicode")?.into() })?;
    if !matches!(&review.outcome.body, Body::Review(actual) if *actual == expected) {
        return Err("qualification full original review mismatch".into());
    }
    // State is still queried through the original owner, never invented revisions.
    if state.outcome.revisions != review.outcome.revisions { return Err("qualification catalog changed during review".into()); }
    let mut pair = review.outcome.revisions;
    for stage in 0..6 {
        let action = match stage {
            0 => Action::Install { path: wrapper.to_str().ok_or("qualification wrapper Unicode")?.into(), full_sha256: full_sha, revisions: pair },
            1 => Action::BaseSelect { full_sha256: full_sha, revisions: pair },
            2 => Action::BaseEnable { id: id.clone(), full_sha256: full_sha, enabled: true, revisions: pair },
            3 => Action::WrapperSelect { full_sha256: full_sha, revisions: pair },
            4 => Action::Approve { id: id.clone(), full_sha256: full_sha, approval: Approval {
                session_bits: 3, process_bits: 0, sessions: scope.clone(), domain: "session-only".into(),
            }, revisions: pair },
            5 => Action::WrapperEnable { id: id.clone(), full_sha256: full_sha, enabled: true, revisions: pair },
            _ => unreachable!(),
        };
        pair = calls.call(host, action)?.outcome.revisions;
    }
    Ok(Reviewed { full_sha, revisions: pair, _wrapper_pin: wrapper_pin })
}
