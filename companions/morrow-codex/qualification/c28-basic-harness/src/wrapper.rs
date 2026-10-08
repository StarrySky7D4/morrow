//! Full wrapper construction and explicit original catalog steps.
use crate::catalog::CatalogCalls;
use anyhow::{Result, anyhow, ensure};
use morrow_agent_catalog_admin_v1::{Action, Approval, Body, Request, Revisions};
use morrow_agent_process_control_v1::Capabilities as ProcessCaps;
use morrow_agent_session_exec_v1_r2::authority::Capabilities as SessionCaps;
use morrow_agent_session_process_v1_host::{AgentProcessPackage, Declaration};
use morrow_core::plugin_package::Package;
use morrow_workbench_host::Workbench;
use std::{fs::OpenOptions, io::Write, path::PathBuf};

pub enum SelectionStep {
    Install,
    BaseSelect,
    BaseEnable,
    WrapperSelect,
    Approve,
    WrapperEnable,
}

pub struct ReviewedWrapper {
    pub id: String,
    pub path: PathBuf,
    pub full_sha256: [u8; 32],
    pub approval: Approval,
    pub base_sha256: [u8; 32],
    review: morrow_agent_catalog_admin_v1::Review,
    session_exec: bool,
    revisions: Revisions,
    next_step: u8,
    calls: CatalogCalls,
    next_id: u128,
}

impl ReviewedWrapper {
    /// Must be called only within the already owned synthetic root. Building a
    /// reviewed archive grants nothing; every following selection is explicit.
    pub fn controls(
        workbench: &mut Workbench,
        path: PathBuf,
        id: &str,
        bytes: &[u8],
        session: &str,
        domain: &str,
        process: ProcessCaps,
        request_seed: u128,
    ) -> Result<Self> {
        Self::build(
            workbench,
            path,
            id,
            bytes,
            crate::sealed::PROCESS_SHA,
            SessionCaps {
                session_read: true,
                ..Default::default()
            },
            process,
            vec![session.to_owned()],
            domain,
            request_seed,
            false,
        )
    }

    pub fn session(
        workbench: &mut Workbench,
        path: PathBuf,
        id: &str,
        bytes: &[u8],
        session: &str,
        request_seed: u128,
    ) -> Result<Self> {
        let mut sessions = vec![session.to_owned(), format!("{session}-child")];
        sessions.sort();
        Self::build(
            workbench,
            path,
            id,
            bytes,
            crate::sealed::SESSION_SHA,
            SessionCaps {
                session_read: true,
                session_write: true,
                ..Default::default()
            },
            ProcessCaps::default(),
            sessions,
            "session-only",
            request_seed,
            false,
        )
    }
    pub fn proposal(
        workbench: &mut Workbench,
        path: PathBuf,
        id: &str,
        bytes: &[u8],
        session: &str,
        domain: &str,
        request_seed: u128,
    ) -> Result<Self> {
        Self::build(
            workbench,
            path,
            id,
            bytes,
            crate::sealed::PROPOSAL_SHA,
            SessionCaps {
                session_read: true,
                propose: true,
                ..Default::default()
            },
            ProcessCaps::default(),
            vec![session.to_owned()],
            domain,
            request_seed,
            // Exact9552 bytes use the combined import even though this wrapper
            // declares read/propose only and zero process-control capabilities.
            // Inspection selects that profile explicitly, with no R2 fallback.
            false,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn build(
        workbench: &mut Workbench,
        path: PathBuf,
        id: &str,
        bytes: &[u8],
        module_sha: &str,
        session: SessionCaps,
        process: ProcessCaps,
        sessions: Vec<String>,
        domain: &str,
        request_seed: u128,
        session_exec: bool,
    ) -> Result<Self> {
        crate::sealed::check_guest(bytes, module_sha).map_err(anyhow::Error::msg)?;
        ensure!(request_seed != 0, "catalog request seed cannot be zero");
        let base = Package::build(
            Package::manifest_for_task(id, "1.0.0", bytes, vec![]),
            bytes,
        )
        .map_err(|e| anyhow!("original base package rejected: {e:?}"))?;
        let full = AgentProcessPackage::build(
            base,
            Declaration {
                session,
                process,
                sessions: sessions.clone(),
                execution_domain: domain.to_owned(),
            },
        )?;
        let full_sha256 = full.review_sha256();
        let base_sha256 = full.base_sha256();
        let approval = Approval {
            session_bits: u16::from(session.session_read)
                | u16::from(session.session_write) << 1
                | u16::from(session.propose) << 2
                | u16::from(session.execute) << 3
                | u16::from(session.retire) << 4,
            process_bits: u16::from(process.read)
                | u16::from(process.events) << 1
                | u16::from(process.write) << 2
                | u16::from(process.close_input) << 3
                | u16::from(process.interrupt) << 4
                | u16::from(process.terminate) << 5
                | u16::from(process.resize_pty) << 6,
            sessions: sessions.clone(),
            domain: domain.to_owned(),
        };
        let expected_review = morrow_agent_catalog_admin_v1::Review {
            id: id.into(),
            version: "1.0.0".into(),
            full_sha256,
            base_sha256,
            session_schema: morrow_agent_session_exec_v1_r2::schema_digest(),
            process_schema: morrow_agent_process_control_v1::schema_digest(),
            session_bits: approval.session_bits,
            process_bits: approval.process_bits,
            sessions,
            domain: domain.into(),
        };
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(full.archive())?;
        file.sync_all()?;
        drop(file);
        let mut calls = CatalogCalls::default();
        let state = calls.call(
            workbench,
            &Request {
                id: request_seed.to_le_bytes(),
                action: Action::State,
            },
        )?;
        let mut result = Self {
            id: id.to_owned(),
            path,
            full_sha256,
            approval,
            base_sha256,
            review: expected_review,
            session_exec,
            revisions: state.outcome.revisions,
            next_step: 0,
            calls,
            next_id: request_seed
                .checked_add(1)
                .ok_or_else(|| anyhow!("request ID overflow"))?,
        };
        if session_exec {
            let (review, revisions) = workbench
                .inspect_agent_session_exec(full.archive())
                .map_err(|e| anyhow!("original fixed R2 profile review: {e}"))?;
            result.verify_native_review(&review)?;
            result.revisions = revisions;
            return Ok(result);
        }
        let request = result.request(Action::Inspect {
            path: result
                .path
                .to_str()
                .ok_or_else(|| anyhow!("wrapper path Unicode"))?
                .to_owned(),
        })?;
        let reply = result.calls.call(workbench, &request)?;
        let Body::Review(review) = reply.outcome.body else {
            return Err(anyhow!("original full wrapper review absent"));
        };
        ensure!(
            review == result.review,
            "original review does not match the complete fixed wrapper"
        );
        result.revisions = reply.outcome.revisions;
        Ok(result)
    }

    fn request(&mut self, action: Action) -> Result<Request> {
        let id = self.next_id.to_le_bytes();
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| anyhow!("request ID overflow"))?;
        Ok(Request { id, action })
    }

    fn verify_native_review(
        &self,
        review: &morrow_agent_session_process_v1_host::catalog::Review,
    ) -> Result<()> {
        let declaration = &review.declaration;
        let expected = &self.review;
        let s = declaration.session;
        let p = declaration.process;
        ensure!(
            review.id == expected.id
                && review.version == expected.version
                && review.wrapper_sha256 == expected.full_sha256
                && review.base_sha256 == expected.base_sha256
                && review.session_schema == expected.session_schema
                && review.process_schema == expected.process_schema
                && (u16::from(s.session_read)
                    | u16::from(s.session_write) << 1
                    | u16::from(s.propose) << 2
                    | u16::from(s.execute) << 3
                    | u16::from(s.retire) << 4)
                    == expected.session_bits
                && (u16::from(p.read)
                    | u16::from(p.events) << 1
                    | u16::from(p.write) << 2
                    | u16::from(p.close_input) << 3
                    | u16::from(p.interrupt) << 4
                    | u16::from(p.terminate) << 5
                    | u16::from(p.resize_pty) << 6)
                    == expected.process_bits
                && declaration.sessions == expected.sessions
                && declaration.execution_domain == expected.domain,
            "full fixed R2 review/ceiling differs from this exact wrapper"
        );
        Ok(())
    }

    /// Caller explicitly selects exactly one next approval/selection step.
    /// This method never proceeds automatically after an Unknown or error.
    pub fn step(&mut self, workbench: &mut Workbench, step: SelectionStep) -> Result<()> {
        let ordinal = match step {
            SelectionStep::Install => 0,
            SelectionStep::BaseSelect => 1,
            SelectionStep::BaseEnable => 2,
            SelectionStep::WrapperSelect => 3,
            SelectionStep::Approve => 4,
            SelectionStep::WrapperEnable => 5,
        };
        ensure!(
            ordinal == self.next_step,
            "explicit wrapper/base approval step out of order"
        );
        let revisions = self.revisions;
        let full_sha256 = self.full_sha256;
        let action = match step {
            SelectionStep::Install => Action::Install {
                path: self
                    .path
                    .to_str()
                    .ok_or_else(|| anyhow!("wrapper path Unicode"))?
                    .to_owned(),
                full_sha256,
                revisions,
            },
            SelectionStep::BaseSelect => Action::BaseSelect {
                full_sha256,
                revisions,
            },
            SelectionStep::BaseEnable => Action::BaseEnable {
                id: self.id.clone(),
                full_sha256,
                enabled: true,
                revisions,
            },
            SelectionStep::WrapperSelect => Action::WrapperSelect {
                full_sha256,
                revisions,
            },
            SelectionStep::Approve => Action::Approve {
                id: self.id.clone(),
                full_sha256,
                approval: self.approval.clone(),
                revisions,
            },
            SelectionStep::WrapperEnable => Action::WrapperEnable {
                id: self.id.clone(),
                full_sha256,
                enabled: true,
                revisions,
            },
        };
        let request = self.request(action)?;
        // Mark the local stage unavailable before dispatch; even a known rejection
        // requires new explicit review, rather than fabricating the old revisions.
        self.next_step = u8::MAX;
        if self.session_exec && ordinal == 0 {
            let raw = std::fs::read(&self.path)?;
            ensure!(
                morrow_agent_session_exec_v1_r2::hash(&raw) == full_sha256,
                "fixed R2 wrapper changed before explicit install"
            );
            let (review, revisions) = workbench
                .install_agent_session_exec(request.id, &raw, full_sha256, revisions)
                .map_err(|e| anyhow!("fixed R2 install outcome unavailable; no replay: {e}"))?;
            self.verify_native_review(&review)?;
            self.revisions = revisions;
            self.next_step = 1;
            return Ok(());
        }
        let reply = self.calls.call(workbench, &request)?;
        self.revisions = reply.outcome.revisions;
        self.next_step = ordinal + 1;
        Ok(())
    }
    pub fn approved_revisions(&self) -> Result<Revisions> {
        ensure!(
            self.next_step == 6,
            "full wrapper is not explicitly approved and enabled"
        );
        Ok(self.revisions)
    }

    /// Pure caller fence before creating the independent proposer lease. The
    /// full immutable module is checked again by the original owned SDK lease.
    pub(crate) fn require_sealed_proposal_review(&self, sid: &str, domain: &str) -> Result<()> {
        self.approved_revisions()?;
        ensure!(
            !self.session_exec
                && self.review.id == self.id
                && self.review.session_bits == 5
                && self.review.process_bits == 0
                && self.review.sessions == [sid.to_owned()]
                && self.review.domain == domain
                && self.review.full_sha256 == self.full_sha256
                && self.review.base_sha256 == self.base_sha256
                && self.approval.session_bits == 5
                && self.approval.process_bits == 0
                && self.approval.sessions == self.review.sessions
                && self.approval.domain == self.review.domain,
            "sealed proposal requires the explicitly reviewed combined profile and read/propose-only ceiling"
        );
        Ok(())
    }

    /// A changed catalog pair is not new approval. This bounded read proves all
    /// exact full/base identities, declarations and existing approval flags at
    /// one current pair; no selection, enable or permission changes are made.
    pub fn current_pair(
        workbench: &mut Workbench,
        wrappers: &[&Self],
        request_seed: u128,
    ) -> Result<Revisions> {
        ensure!(
            !wrappers.is_empty() && wrappers.len() <= 3,
            "exact wrapper set bound"
        );
        for wrapper in wrappers {
            wrapper.approved_revisions()?;
        }
        let mut calls = CatalogCalls::default();
        let state = calls.call(
            workbench,
            &Request {
                id: request_seed.to_le_bytes(),
                action: Action::State,
            },
        )?;
        let pair = state.outcome.revisions;
        let mut after = None;
        let mut found = std::collections::BTreeSet::new();
        for page in 0..16u128 {
            let request = Request {
                id: request_seed
                    .checked_add(page + 1)
                    .ok_or_else(|| anyhow!("page id overflow"))?
                    .to_le_bytes(),
                action: Action::Page {
                    after: after.clone(),
                    limit: 16,
                    revisions: pair,
                },
            };
            let reply = calls.call(workbench, &request)?;
            ensure!(
                reply.outcome.revisions == pair,
                "read pair changed; explicit fresh review required"
            );
            let Body::Page { entries, next } = reply.outcome.body else {
                return Err(anyhow!("canonical catalog page missing"));
            };
            for entry in entries {
                if let Some(wrapper) = wrappers.iter().find(|w| w.id == entry.review.id) {
                    ensure!(
                        entry.review == wrapper.review
                            && entry.selected
                            && entry.enabled
                            && entry.base_selected
                            && entry.base_enabled
                            && entry.approval.as_ref() == Some(&wrapper.approval)
                            && found.insert(wrapper.id.clone()),
                        "exact current full wrapper/base approval unavailable"
                    );
                }
            }
            match next {
                None => {
                    ensure!(
                        found.len() == wrappers.len(),
                        "explicit wrappers missing from original catalog"
                    );
                    return Ok(pair);
                }
                Some(next) => {
                    ensure!(
                        after.as_ref().is_none_or(|old| old < &next),
                        "page cursor did not advance"
                    );
                    after = Some(next);
                }
            }
        }
        Err(anyhow!(
            "catalog page budget exhausted; no permission inferred"
        ))
    }
}
