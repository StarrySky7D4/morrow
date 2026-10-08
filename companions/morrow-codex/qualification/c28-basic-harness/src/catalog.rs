//! One explicit original catalog operation per ID. Unknown is never retried.
use anyhow::{Result, anyhow, ensure};
use morrow_agent_catalog_admin_v1::{Reply, Request, Status};
use morrow_workbench_host::Workbench;
use std::collections::BTreeSet;

#[derive(Default)]
pub struct CatalogCalls {
    mutations: BTreeSet<[u8; 16]>,
    uncertain: bool,
}
impl CatalogCalls {
    pub fn call(&mut self, workbench: &mut Workbench, request: &Request) -> Result<Reply> {
        let raw = request.encode()?;
        if request.is_mutation() {
            ensure!(
                !self.uncertain,
                "catalog delivery is unresolved; explicit recovery is required"
            );
            ensure!(
                self.mutations.len() < 64,
                "qualification catalog mutation budget exhausted"
            );
            ensure!(
                self.mutations.insert(request.id),
                "mutation ID already attempted; no replay"
            );
            self.uncertain = true;
        }
        let raw_reply = morrow_workbench_host::protocol::respond(workbench, &raw)
            .map_err(|error| anyhow!("original catalog delivery unavailable: {error}"))?;
        let reply = Reply::decode_for(request, &raw_reply)?;
        if request.is_mutation() && reply.outcome.status != Status::Unknown {
            self.uncertain = false;
        }
        ensure!(
            reply.outcome.status == Status::Ok,
            "original catalog returned {:?}; no automatic retry",
            reply.outcome.status
        );
        Ok(reply)
    }
}
