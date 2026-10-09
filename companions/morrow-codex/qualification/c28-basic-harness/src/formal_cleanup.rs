//! Caller policy for the original public Workbench operations. The adapter is
//! deliberately private: scripted tests exercise call ordering, not protected
//! storage, actual Core retirement, native facts, or OS cleanup acceptance.
use anyhow::{Result, anyhow, ensure};
use morrow_workbench_host::{
    Workbench,
    agent_tasks::{AgentContext, AgentExitStatus, AgentSnapshot, AgentStart},
    io_tasks::{StoragePhase, TaskKey},
};
use std::time::{Duration, Instant};

pub(super) type WorkflowTracker = Tracker<morrow_core::dispatch::HostBinding>;

pub(super) struct WorkbenchAdapter<'a>(pub &'a mut Workbench);
impl Owner for WorkbenchAdapter<'_> {
    type Identity = morrow_core::dispatch::HostBinding;
    fn identity(&self) -> Result<Self::Identity> {
        Ok(self.0.agent_owner_binding())
    }
    fn status(&mut self) -> Result<Option<AgentSnapshot>> {
        self.0
            .agent_status()
            .map_err(|e| anyhow!("original cleanup status: {e}"))
    }
    fn start(
        &mut self,
        options: AgentStart,
        context: &mut Option<AgentContext>,
    ) -> Result<TaskKey> {
        self.0
            .start_agent(options, context)
            .map_err(|e| anyhow!("original worker start: {e}"))
    }
    fn stop(&mut self, key: TaskKey) -> Result<AgentSnapshot> {
        crate::product::stop_original(self.0, key)
    }
    fn recover(&mut self, key: TaskKey) -> Result<AgentSnapshot> {
        crate::product::recover_original(self.0, key)
    }
    fn poll(&mut self, key: TaskKey) -> Result<AgentSnapshot> {
        crate::product::poll_original(self.0, key)
    }
    fn acknowledge(&mut self, key: TaskKey) -> Result<()> {
        self.0
            .acknowledge_agent(key)
            .map_err(|e| anyhow!("original cleanup acknowledge: {e}"))
    }
}

pub(super) trait Owner {
    type Identity: Clone + PartialEq;
    fn identity(&self) -> Result<Self::Identity>;
    fn status(&mut self) -> Result<Option<AgentSnapshot>>;
    fn start(&mut self, options: AgentStart, context: &mut Option<AgentContext>)
    -> Result<TaskKey>;
    fn stop(&mut self, key: TaskKey) -> Result<AgentSnapshot>;
    fn recover(&mut self, key: TaskKey) -> Result<AgentSnapshot>;
    fn poll(&mut self, key: TaskKey) -> Result<AgentSnapshot>;
    fn acknowledge(&mut self, key: TaskKey) -> Result<()>;
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AcknowledgedCleanup {
    pub key: TaskKey,
    pub historical_exit: AgentExitStatus,
    pub explicit_recovery: bool,
}

pub(super) struct Tracker<I> {
    original_owner: Option<I>,
    start_attempted: bool,
    unknown_start: bool,
    acknowledged: Option<AcknowledgedCleanup>,
}
impl<I> Default for Tracker<I> {
    fn default() -> Self {
        Self {
            original_owner: None,
            start_attempted: false,
            unknown_start: false,
            acknowledged: None,
        }
    }
}
impl<I: Clone + PartialEq> Tracker<I> {
    /// The identity comes only from the same actual owner. It is neither a
    /// configuration value nor a substitute for original task/grant checks.
    pub fn bind<O: Owner<Identity = I>>(&mut self, owner: &O) -> Result<()> {
        let current = owner.identity()?;
        if let Some(original) = &self.original_owner {
            ensure!(*original == current, "foreign original cleanup owner");
        } else {
            self.original_owner = Some(current);
        }
        Ok(())
    }

    /// Pre-start observation and post-error capture share the adapter's single
    /// exclusive borrow. There is exactly one start call and no retry.
    pub fn start_once<O: Owner<Identity = I>>(
        &mut self,
        owner: &mut O,
        options: AgentStart,
        context: &mut Option<AgentContext>,
        known_key: &mut Option<TaskKey>,
    ) -> Result<TaskKey> {
        self.bind(owner)?;
        ensure!(
            !self.start_attempted && !self.unknown_start && self.acknowledged.is_none() && known_key.is_none(),
            "start identity retained; no replay"
        );
        ensure!(
            owner.status()?.is_none(),
            "an existing task cannot be adopted by start"
        );
        // A failed or unresolved call stays latched; absent status is not replay authority.
        self.start_attempted = true;
        match owner.start(options, context) {
            Ok(key) => {
                // A returned key fences this active task; original ACK may end this lane.
                self.start_attempted = false;
                *known_key = Some(key);
                Ok(key)
            }
            Err(error) => {
                match owner.status() {
                    Ok(Some(status)) => match status.task.key {
                        Some(key) => *known_key = Some(key),
                        None => self.unknown_start = true,
                    },
                    Ok(None) => {}
                    Err(_) => self.unknown_start = true,
                }
                Err(anyhow!(
                    "original worker start unavailable: {error}; retain identity/debt, no replay"
                ))
            }
        }
    }

    /// Called before disposing any external context or controlling a task.
    /// No current unknown task is ever promoted into this workflow's identity.
    pub fn precheck<O: Owner<Identity = I>>(
        &mut self,
        owner: &mut O,
        known_key: Option<TaskKey>,
    ) -> Result<Option<AgentSnapshot>> {
        self.bind(owner)?;
        ensure!(
            !self.unknown_start,
            "start identity Unknown; no blind task adoption"
        );
        let status = owner.status()?;
        if let Some(receipt) = &self.acknowledged {
            ensure!(
                known_key == Some(receipt.key) && status.is_none(),
                "acknowledged original task changed; keep remaining debt"
            );
        } else if let Some(status) = &status {
            let key =
                known_key.ok_or_else(|| anyhow!("current task is not owned by this workflow"))?;
            same_task(key, status)?;
        } else {
            ensure!(
                known_key.is_none(),
                "known task disappeared without original acknowledgement"
            );
        }
        Ok(status)
    }

    /// Current repair completion is separate from historical execution/cleanup
    /// outcomes. Only successful original ACK creates the private receipt.
    pub fn settle_task<O: Owner<Identity = I>>(
        &mut self,
        owner: &mut O,
        known_key: Option<TaskKey>,
        recovery: bool,
        wait: Duration,
    ) -> Result<Option<AcknowledgedCleanup>> {
        let status = self.precheck(owner, known_key)?;
        if let Some(receipt) = self.acknowledged {
            return Ok(Some(receipt));
        }
        let Some(mut status) = status else {
            return Ok(None);
        };
        let key = known_key.ok_or_else(|| anyhow!("known cleanup key absent"))?;
        let historical_failure = status.exit.as_ref().is_some_and(cleanup_failed);
        let action_needed =
            status.task.storage != StoragePhase::Reclaimed || (recovery && historical_failure);
        if action_needed {
            status = if recovery {
                owner.recover(key)?
            } else {
                owner.stop(key)?
            };
            same_task(key, &status)?;
        }
        let deadline = Instant::now() + wait;
        while status.task.storage != StoragePhase::Reclaimed {
            ensure!(
                !matches!(
                    status.task.storage,
                    StoragePhase::RecoveryRequired | StoragePhase::Unavailable
                ),
                "original owner cleanup retained; explicit repair required"
            );
            ensure!(
                Instant::now() < deadline,
                "original cleanup/join pending; debt remains charged"
            );
            std::thread::sleep(Duration::from_millis(5));
            status = owner.poll(key)?;
            same_task(key, &status)?;
        }
        let exit = status
            .exit
            .ok_or_else(|| anyhow!("original joined cleanup exit absent"))?;
        ensure!(
            !cleanup_failed(&exit) || (recovery && action_needed),
            "historical cleanup error requires explicit current original recovery"
        );
        self.bind(owner)?;
        owner.acknowledge(key)?;
        let receipt = AcknowledgedCleanup {
            key,
            historical_exit: exit,
            explicit_recovery: recovery && action_needed,
        };
        self.acknowledged = Some(receipt);
        Ok(Some(receipt))
    }

    /// The caller may clear only after preparation, native resources and facts
    /// have independently passed their original checks. Failures keep receipt.
    pub fn complete(&mut self) {
        self.acknowledged = None;
    }
}

fn same_task(key: TaskKey, status: &AgentSnapshot) -> Result<()> {
    ensure!(
        status.task.key == Some(key),
        "original cleanup task identity changed"
    );
    Ok(())
}
fn cleanup_failed(exit: &AgentExitStatus) -> bool {
    exit.disconnect.is_err() || exit.maintenance.is_err()
}

#[cfg(test)]
#[path = "formal_cleanup_tests.rs"]
mod tests;
