use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use crate::{Error, Result};

use super::ServiceAuthorityResource;

pub(super) const MAX_LIVE_EPOCHS: usize = 2048;

#[derive(Default)]
pub(super) struct Epochs {
    pub(super) global: Arc<AtomicBool>,
    pub(super) writes: Arc<AtomicBool>,
    map: BTreeMap<ServiceAuthorityResource, Weak<AtomicBool>>,
}

impl Epochs {
    pub(super) fn bind(
        &mut self,
        keys: &[ServiceAuthorityResource],
    ) -> Result<Vec<Arc<AtomicBool>>> {
        // Drop dead entries so live counts are accurate.
        self.map.retain(|_, weak| weak.strong_count() > 0);

        // Count unique keys with no live entry, before mutating.
        let mut missing: BTreeSet<&ServiceAuthorityResource> = BTreeSet::new();
        let mut seen: BTreeSet<&ServiceAuthorityResource> = BTreeSet::new();
        for key in keys {
            if !seen.insert(key) {
                continue;
            }
            let live = self
                .map
                .get(key)
                .map(|weak| weak.strong_count() > 0)
                .unwrap_or(false);
            if !live {
                missing.insert(key);
            }
        }

        if self.map.len() + missing.len() > MAX_LIVE_EPOCHS {
            return Err(Error::Limit);
        }

        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            let flag = match self.map.get(key).and_then(Weak::upgrade) {
                Some(existing) => existing,
                None => {
                    let fresh = Arc::new(AtomicBool::new(false));
                    self.map.insert(key.clone(), Arc::downgrade(&fresh));
                    fresh
                }
            };
            out.push(flag);
        }
        Ok(out)
    }

    pub(super) fn revoke(&mut self, key: &ServiceAuthorityResource) {
        // Invalidate legacy all-writes guards and this exact resource only.
        self.writes.store(true, Ordering::Release);
        self.writes = Arc::new(AtomicBool::new(false));

        if let Some(flag) = self.map.remove(key).and_then(|weak| weak.upgrade()) {
            flag.store(true, Ordering::Release);
        }
    }

    pub(super) fn revoke_all(&mut self) {
        self.global.store(true, Ordering::Release);
        self.writes.store(true, Ordering::Release);
        for weak in self.map.values() {
            if let Some(flag) = weak.upgrade() {
                flag.store(true, Ordering::Release);
            }
        }
        self.global = Arc::new(AtomicBool::new(false));
        self.writes = Arc::new(AtomicBool::new(false));
        self.map.clear();
    }
}
