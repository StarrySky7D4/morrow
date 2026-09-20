//! Bounded host-side identities for one service's owner-command lane. Submission
//! retries recover status only: a delivered or uncertain command is never replayed.
use super::{AccessError, Task, TaskKey};
use crate::{Result, Workbench};
use morrow_plugin_runtime::io_jobs::{
    MAX_OWNER_COMMAND_INPUT, OwnerCommandError, OwnerCommandHandle, OwnerCommandPoll,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

pub const MAX_LIVE_COMMANDS: usize = 8;
pub const MAX_COMMAND_HISTORY: usize = 512;

/// Opaque random identity scoped to the exact service task admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CommandKey([u8; 32]);
impl CommandKey {
    pub fn from_bytes(value: &[u8]) -> Result<Self> {
        Ok(Self(value.try_into().map_err(|_| AccessError::StaleTask)?))
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandSnapshot {
    pub key: CommandKey,
    pub submission: [u8; 32],
    pub delivery: OwnerCommandPoll,
    pub started: bool,
    pub terminal: Option<OwnerCommandError>,
}

struct Entry {
    submission: [u8; 32],
    digest: [u8; 32],
    handle: Option<OwnerCommandHandle>,
    started: bool,
    terminal: Option<OwnerCommandError>,
}
impl Entry {
    fn snapshot(&mut self, key: CommandKey) -> CommandSnapshot {
        let delivery = if let Some(handle) = &self.handle {
            // Ready is intentionally not read here. The native handle retains
            // its reservation and performs fresh authorization on explicit read.
            let delivery = handle.poll();
            self.started |= handle.is_started();
            delivery
        } else {
            OwnerCommandPoll::Consumed
        };
        CommandSnapshot {
            key,
            submission: self.submission,
            delivery,
            started: self.started,
            terminal: self.terminal,
        }
    }

    fn read(&mut self, key: CommandKey) -> (CommandSnapshot, Option<Zeroizing<Vec<u8>>>) {
        let Some(handle) = &mut self.handle else {
            return (self.snapshot(key), None);
        };
        let result = handle.read().map(|value| value.map(Zeroizing::new));
        self.started |= handle.is_started();
        match result {
            Ok(None) => (self.snapshot(key), None),
            Ok(Some(payload)) => {
                self.handle = None;
                (self.snapshot(key), Some(payload))
            }
            Err(error) => {
                self.terminal = Some(error);
                self.handle = None;
                (self.snapshot(key), None)
            }
        }
    }

    fn cancel(&mut self, key: CommandKey) -> CommandSnapshot {
        let Some(handle) = &mut self.handle else {
            return self.snapshot(key);
        };
        handle.cancel();
        // Cancellation classifies the original native result, including a
        // completed effect whose reply is now unavailable. Never return bytes.
        let result = handle.read().map(|value| value.map(Zeroizing::new));
        self.started |= handle.is_started();
        self.terminal = Some(match result {
            Err(error) => error,
            Ok(_) if self.started => OwnerCommandError::Unknown,
            Ok(_) => OwnerCommandError::Cancelled,
        });
        self.handle = None;
        self.snapshot(key)
    }
}

struct Admission {
    key: CommandKey,
    submission: [u8; 32],
    digest: [u8; 32],
}
enum Decision {
    Existing(CommandSnapshot),
    New(Admission),
}

#[derive(Default)]
pub(super) struct Registry {
    entries: BTreeMap<CommandKey, Entry>,
    submissions: BTreeMap<[u8; 32], CommandKey>,
}
impl Registry {
    fn prepare(&mut self, submission: [u8; 32], digest: [u8; 32]) -> Result<Decision> {
        if submission == [0; 32] {
            return Err("service command submission identity is required".into());
        }
        if let Some(key) = self.submissions.get(&submission).copied() {
            let entry = self.entries.get_mut(&key).ok_or(AccessError::StaleTask)?;
            if entry.digest != digest {
                return Err("service command submission conflicts with its original bytes".into());
            }
            return Ok(Decision::Existing(entry.snapshot(key)));
        }
        if self.entries.len() >= MAX_COMMAND_HISTORY {
            return Err("service command history capacity exhausted".into());
        }
        if self
            .entries
            .values()
            .filter(|entry| entry.handle.is_some())
            .count()
            >= MAX_LIVE_COMMANDS
        {
            return Err(OwnerCommandError::Busy.into());
        }
        // No entry or submission is reserved until the native queue accepts.
        // A failed entropy source or queue admission leaves all capacity intact.
        for _ in 0..4 {
            let mut key = [0; 32];
            getrandom::fill(&mut key)?;
            let key = CommandKey(key);
            if key.0 != [0; 32] && !self.entries.contains_key(&key) {
                return Ok(Decision::New(Admission {
                    key,
                    submission,
                    digest,
                }));
            }
        }
        Err("service command identity collision".into())
    }
    fn insert(&mut self, admission: Admission, handle: OwnerCommandHandle) -> CommandSnapshot {
        let mut entry = Entry {
            submission: admission.submission,
            digest: admission.digest,
            handle: Some(handle),
            started: false,
            terminal: None,
        };
        let snapshot = entry.snapshot(admission.key);
        self.submissions.insert(admission.submission, admission.key);
        self.entries.insert(admission.key, entry);
        snapshot
    }
    fn entry(&mut self, key: CommandKey) -> Result<&mut Entry> {
        self.entries
            .get_mut(&key)
            .ok_or_else(|| AccessError::StaleTask.into())
    }
}

impl Workbench {
    fn checked_command_task(&mut self, task: TaskKey) -> Result<&mut Task> {
        let task = self.state.checked_task(task)?;
        if task.service.is_none() {
            return Err(AccessError::StaleTask.into());
        }
        Ok(task)
    }

    /// Idempotent admission by caller submission identity and exact frame hash.
    /// A retry returns the original command status even after owner reclamation;
    /// neither a lost reply nor an unknown effect permits automatic resubmission.
    pub fn enqueue_service_command(
        &mut self,
        task: TaskKey,
        submission: [u8; 32],
        input: Vec<u8>,
    ) -> Result<CommandSnapshot> {
        let mut input = Zeroizing::new(input);
        self.checked_command_task(task)?;
        if input.len() > MAX_OWNER_COMMAND_INPUT {
            return Err(OwnerCommandError::Limit.into());
        }
        let digest = Sha256::digest(input.as_slice()).into();
        let admission = match self
            .checked_command_task(task)?
            .commands
            .prepare(submission, digest)?
        {
            Decision::Existing(snapshot) => return Ok(snapshot),
            Decision::New(admission) => admission,
        };
        let handle = self.submit_service_command(task, std::mem::take(&mut *input))?;
        // The exclusive Workbench borrow spans prepare, native admission and
        // insertion. No other caller can change this task or spend its history.
        Ok(self
            .checked_command_task(task)?
            .commands
            .insert(admission, handle))
    }

    /// Inspect only; does not consume Ready or copy any reply into a registry.
    /// A lost admission receipt can be reconciled without resending its body
    /// or executing a command that might never have been admitted.
    pub fn service_command_by_submission(
        &mut self,
        task: TaskKey,
        submission: [u8; 32],
    ) -> Result<CommandSnapshot> {
        let registry = &mut self.checked_command_task(task)?.commands;
        let key = registry
            .submissions
            .get(&submission)
            .copied()
            .ok_or(AccessError::StaleTask)?;
        Ok(registry.entry(key)?.snapshot(key))
    }

    /// Inspect only; does not consume Ready or copy any reply into a registry.
    pub fn service_command_status(
        &mut self,
        task: TaskKey,
        key: CommandKey,
    ) -> Result<CommandSnapshot> {
        Ok(self
            .checked_command_task(task)?
            .commands
            .entry(key)?
            .snapshot(key))
    }

    /// Successful bytes are returned once in a wiping owner. Consumed entries
    /// retain only identity, frame digest, start evidence and terminal category.
    pub fn read_service_command(
        &mut self,
        task: TaskKey,
        key: CommandKey,
    ) -> Result<(CommandSnapshot, Option<Zeroizing<Vec<u8>>>)> {
        Ok(self
            .checked_command_task(task)?
            .commands
            .entry(key)?
            .read(key))
    }

    pub fn cancel_service_command(
        &mut self,
        task: TaskKey,
        key: CommandKey,
    ) -> Result<CommandSnapshot> {
        Ok(self
            .checked_command_task(task)?
            .commands
            .entry(key)?
            .cancel(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tombstone(
        registry: &mut Registry,
        n: u16,
        terminal: Option<OwnerCommandError>,
    ) -> CommandKey {
        let mut identity = [0; 32];
        identity[..2].copy_from_slice(&n.to_le_bytes());
        let key = CommandKey(identity);
        registry.entries.insert(
            key,
            Entry {
                submission: identity,
                digest: [7; 32],
                handle: None,
                started: true,
                terminal,
            },
        );
        registry.submissions.insert(identity, key);
        key
    }

    #[test]
    fn consumed_submission_recovers_same_identity_and_terminal_without_replay() {
        for terminal in [
            None,
            Some(OwnerCommandError::Unknown),
            Some(OwnerCommandError::Cancelled),
        ] {
            let mut registry = Registry::default();
            let key = tombstone(&mut registry, 9, terminal);
            let expected = registry.entry(key).unwrap().snapshot(key);
            let Decision::Existing(actual) = registry.prepare(*key.as_bytes(), [7; 32]).unwrap()
            else {
                panic!("consumed command must never be admitted again");
            };
            assert_eq!(actual, expected);
            assert_eq!(actual.delivery, OwnerCommandPoll::Consumed);
            assert_eq!(registry.entry(key).unwrap().cancel(key), expected);
            let (read, payload) = registry.entry(key).unwrap().read(key);
            assert_eq!(read, expected);
            assert!(payload.is_none());
        }
    }

    #[test]
    fn submission_conflict_preserves_original_tombstone_and_history() {
        let mut registry = Registry::default();
        let key = tombstone(&mut registry, 1, Some(OwnerCommandError::Unknown));
        let original = registry.entry(key).unwrap().snapshot(key);
        assert!(registry.prepare(*key.as_bytes(), [8; 32]).is_err());
        assert_eq!(registry.entries.len(), 1);
        assert_eq!(registry.submissions.len(), 1);
        assert_eq!(registry.entry(key).unwrap().snapshot(key), original);
    }

    #[test]
    fn full_history_retains_deduplication_and_preparation_does_not_reserve() {
        let mut registry = Registry::default();
        assert!(registry.prepare([0; 32], [7; 32]).is_err());
        for _ in 0..4 {
            assert!(matches!(
                registry.prepare([99; 32], [7; 32]).unwrap(),
                Decision::New(_)
            ));
        }
        assert!(registry.entries.is_empty() && registry.submissions.is_empty());
        for n in 0..MAX_COMMAND_HISTORY {
            tombstone(&mut registry, n as u16, None);
        }
        assert!(registry.prepare([99; 32], [7; 32]).is_err());
        let mut last = [0; 32];
        last[..2].copy_from_slice(&((MAX_COMMAND_HISTORY - 1) as u16).to_le_bytes());
        assert!(matches!(
            registry.prepare(last, [7; 32]).unwrap(),
            Decision::Existing(_)
        ));
        assert_eq!(registry.entries.len(), MAX_COMMAND_HISTORY);
        assert_eq!(registry.submissions.len(), MAX_COMMAND_HISTORY);
    }

    #[test]
    fn command_key_requires_exact_identity_length() {
        for len in [0, 1, 31, 33, 64] {
            assert!(CommandKey::from_bytes(&vec![1; len]).is_err());
        }
        assert_eq!(
            CommandKey::from_bytes(&[3; 32]).unwrap().as_bytes(),
            &[3; 32]
        );
    }
}
