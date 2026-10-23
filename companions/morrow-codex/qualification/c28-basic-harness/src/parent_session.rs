//! Fixed, explicit guest qualification for seventeen closed native refusals.
//! A single imported P/C lifetime, protected Workbench and SDK registry survive
//! every attempt. Any error or unwind retains the current and historical owners;
//! no subsequent attempt, cleanup, retry or ordinary-success fallback follows it.
use crate::live::{LiveConfig, Session};
use anyhow::{Result, ensure};
use codex_windows_sandbox::{AsUserParentAttempt, AsUserParentOrigin, AsUserParentSession};
use std::{io::BufRead, sync::Arc};

pub const PHASE: &str = "live-guest-refusal-session-interactive";
pub const ATTEMPTS: usize = 17;

struct OriginalLifetime {
    imported: Option<AsUserParentOrigin>,
    parent: Option<AsUserParentSession>,
    attempts: Vec<Arc<AsUserParentAttempt>>,
    sessions: Vec<Box<Session>>,
}

impl OriginalLifetime {
    fn run(&mut self, config: LiveConfig, reader: &mut impl BufRead) -> Result<()> {
        self.imported.as_ref().unwrap().validate_before_setup()?;
        let original = self.imported.take().unwrap();
        self.parent = Some(match original.into_parent_session() {
            Ok(parent) => parent,
            Err((original, error)) => {
                self.imported = Some(original);
                return Err(error.into());
            }
        });
        let common_root = config.files.synthetic_root.clone();
        let parent = common_root.parent().ok_or_else(|| anyhow::anyhow!("fresh guest root parent absent"))?;
        let stem = common_root.file_name().and_then(|value| value.to_str())
            .ok_or_else(|| anyhow::anyhow!("fresh guest root name invalid"))?;
        for sequence in 0..ATTEMPTS {
            let mut next = config.clone();
            next.files.synthetic_root = parent.join(format!("{stem}-attempt-{:02}", sequence + 1));
            // Install the genuine original capability before any attempt-specific
            // caller observation, inventory, setup or protected-owner effect.
            let attempt = self.parent.as_mut().unwrap().begin_attempt()?;
            self.attempts.push(attempt.clone());
            let session = Session::begin_parent_attempt(next, reader, attempt)?;
            self.sessions.push(session);
            if sequence > 0 {
                let (previous, current) = self.sessions.split_at_mut(sequence);
                current[0].attach_completed_owner(&mut previous[sequence - 1])?;
            }
            let current = &mut self.sessions[sequence];
            current.interact(reader)?;
            current.require_closed_parent_refusal(sequence)?;
        }
        ensure!(self.sessions.len() == ATTEMPTS && self.attempts.len() == ATTEMPTS,
            "fixed original-parent attempt count incomplete");
        self.sessions.last_mut().unwrap().finish_common_owner_lifetime(ATTEMPTS)?;
        self.parent.as_mut().unwrap().finish()?;
        // Only the genuine final close permits normal destruction of the same
        // original protected owner, completed workflows and checked native pins.
        self.sessions.clear();
        self.attempts.clear();
        self.parent.take();
        Ok(())
    }
}

/// This entry consumes the imported capability before constructing any session.
/// The explicit fixed phase is selected by the independently pinned parent PE.
pub fn run(original: AsUserParentOrigin, config: LiveConfig, reader: &mut impl BufRead) -> Result<()> {
    let held = Box::leak(Box::new(OriginalLifetime {
        imported: Some(original), parent: None,
        attempts: Vec::with_capacity(ATTEMPTS), sessions: Vec::with_capacity(ATTEMPTS),
    }));
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| held.run(config, reader))) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) | Err(_) => {
            // Deliberately do not serialize capabilities or perform diagnostic
            // reads against a failed original owner. Keep the entire lifetime.
            eprintln!("ORIGINAL_PARENT_SESSION_UNKNOWN_RETAINED_NO_NEXT_ATTEMPT");
            loop { std::thread::park(); }
        }
    }
}
