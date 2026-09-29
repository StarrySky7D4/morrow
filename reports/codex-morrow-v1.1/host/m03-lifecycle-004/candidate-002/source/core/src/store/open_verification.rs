//! Bounded proofs for one read-only integrity pass in one transaction.
//! Never stored in Store, exported, persisted or reused after migration/writes.
use super::{evidence, read_archive, sql};
use crate::{Error, Result, read_journal::ReadObservation};
use rusqlite::Connection;
use std::collections::HashMap;

const MAX_ENTRIES: usize = 4096;
pub(super) struct OpenVerification<'a> {
    connection: &'a Connection,
    changes: u64,
    version: i64,
    evidence_costs: HashMap<[u8; 32], usize>,
    archive_digests: HashMap<String, [u8; 32]>,
}

impl<'a> OpenVerification<'a> {
    pub(super) fn new(connection: &'a Connection) -> Result<Self> {
        Self::build(connection, MAX_ENTRIES)
    }
    fn build(connection: &'a Connection, limit: usize) -> Result<Self> {
        if connection.is_autocommit() {
            return Err(Error::Integrity);
        }
        let mut result = Self {
            connection,
            changes: connection.total_changes(),
            version: sql(connection.query_row("PRAGMA user_version", [], |r| r.get(0)))?,
            evidence_costs: HashMap::new(),
            archive_digests: HashMap::new(),
        };
        read_archive::verify(connection, |id, digest| {
            if result.version == super::SCHEMA_VERSION && result.archive_digests.len() < limit {
                result.archive_digests.insert(id.to_owned(), digest);
            }
        })?;
        // Verify every physical object, even after the bounded proof map is full.
        if result.version == super::SCHEMA_VERSION {
            evidence::verify(connection, |digest, cost| {
                if result.evidence_costs.len() < limit {
                    result.evidence_costs.insert(digest, cost);
                }
            })?;
        }
        Ok(result)
    }
    pub(super) fn finish(self) -> Result<()> {
        self.unchanged()?;
        // Migration keeps the old verification order and version-error contract.
        if self.version != super::SCHEMA_VERSION {
            evidence::verify(self.connection, |_, _| {})?;
        }
        Ok(())
    }
    fn unchanged(&self) -> Result<()> {
        if self.connection.is_autocommit() || self.connection.total_changes() != self.changes {
            return Err(Error::Integrity);
        }
        Ok(())
    }
    pub(super) fn verify_event(&self, operation: &str, expected: &[Vec<u8>]) -> Result<()> {
        self.unchanged()?;
        if self.version != super::SCHEMA_VERSION {
            return evidence::verify_event(self.connection, operation, expected);
        }
        if expected.len() > evidence::MAX_COUNT {
            return Err(Error::Limit);
        }
        if self.version < 7 {
            return if expected.is_empty() {
                Ok(())
            } else {
                Err(Error::Integrity)
            };
        }
        let mut statement = sql(self.connection.prepare_cached(
            "SELECT ordinal,digest FROM operation_evidence WHERE operation_id=?1 ORDER BY ordinal",
        ))?;
        let mut rows = sql(statement.query([operation]))?;
        let mut count = 0;
        let mut total = 0usize;
        while let Some(row) = sql(rows.next())? {
            let ordinal: i64 = sql(row.get(0))?;
            let digest = sql(row.get_ref(1))?
                .as_blob()
                .map_err(|_| Error::Integrity)?;
            if count >= expected.len() || ordinal != count as i64 || digest != expected[count] {
                return Err(Error::Integrity);
            }
            let digest: [u8; 32] = digest.try_into().map_err(|_| Error::Integrity)?;
            let Some(cost) = self.evidence_costs.get(&digest) else {
                // Large libraries retain their existing correctness path.
                return evidence::verify_event(self.connection, operation, expected);
            };
            // Repeated positions are charged each time, despite decoded-object reuse.
            total = total.checked_add(*cost).ok_or(Error::Limit)?;
            if total > evidence::MAX_OPERATION_BYTES {
                return Err(Error::Limit);
            }
            count += 1;
        }
        if count != expected.len() {
            return Err(Error::Integrity);
        }
        Ok(())
    }
    pub(super) fn verify_observation(&self, observed: &ReadObservation) -> Result<()> {
        self.unchanged()?;
        let data = observed.data();
        if data.schema_version == 2
            && self.version >= 12
            && let Some(digest) = self.archive_digests.get(&data.operation_id)
        {
            let manifest = read_archive::load(self.connection, &data.subject, &data.operation_id)?
                .ok_or(Error::Integrity)?;
            if &manifest.digest() != digest {
                return Err(Error::Integrity);
            }
            // Physical parts and reverse association were verified in this same pass.
            return read_archive::match_observation(&manifest, observed);
        }
        read_archive::verify_observation(self.connection, observed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        read_archive::{Budget, Finish, Plan},
        store::Store,
        task_evidence,
    };

    fn fixture() -> (tempfile::TempDir, Store, Vec<Vec<u8>>, ReadObservation) {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("open.db"), Default::default()).unwrap();
        let (digest, bytes) = super::super::evidence_chunks::parallel_tests::fixture();
        let a = task_evidence::decode(&bytes, digest).unwrap();
        let mut data = a.data().clone();
        data.fuel_remaining -= 1;
        let b = task_evidence::encode(data).unwrap();
        let expected = vec![
            a.digest().to_vec(),
            b.digest().to_vec(),
            a.digest().to_vec(),
        ];
        store
            .begin_read_archive(&Plan {
                operation_id: "operation".into(),
                subject: "query".into(),
                request_type: "test.request".into(),
                request: vec![1],
                response_type: "test.response".into(),
                budget: Budget {
                    max_parts: 8,
                    max_bytes: 1024 * 1024,
                },
            })
            .unwrap();
        let status = store
            .append_read_archive("query", "operation", 0, "test.bytes", b"part")
            .unwrap();
        store
            .finish_read_archive_local_authorized(
                "query",
                "operation",
                &Finish {
                    response: vec![2],
                    part_count: status.count,
                    logical_bytes: status.logical_bytes,
                    chain_sha256: status.chain_sha256,
                    metadata_type: "test.metadata".into(),
                    metadata: vec![],
                },
                &[a.clone(), b, a],
                || Ok(()),
            )
            .unwrap();
        let observed = store.lookup_read("query", "operation").unwrap().unwrap();
        (dir, store, expected, observed)
    }

    #[test]
    fn bounded_proofs_and_fallback_preserve_order_and_reject_later_writes() {
        let (_dir, store, expected, observed) = fixture();
        assert!(OpenVerification::new(&store.connection).is_err());
        for limit in [0, 1, MAX_ENTRIES] {
            let tx = store.connection.unchecked_transaction().unwrap();
            let proof = OpenVerification::build(&tx, limit).unwrap();
            assert_eq!(proof.evidence_costs.len(), limit.min(2));
            assert_eq!(proof.archive_digests.len(), limit.min(1));
            proof.verify_event("operation", &expected).unwrap();
            proof.verify_observation(&observed).unwrap();
            let mut reordered = expected.clone();
            reordered.swap(0, 1);
            assert_eq!(
                proof.verify_event("operation", &reordered),
                Err(Error::Integrity)
            );
            assert_eq!(
                proof.verify_event("operation", &expected[..2]),
                Err(Error::Integrity)
            );
            assert_eq!(
                proof.verify_event("operation", &vec![expected[0].clone(); 17]),
                Err(Error::Limit)
            );
            tx.execute(
                "UPDATE operation_evidence SET ordinal=ordinal WHERE operation_id='operation'",
                [],
            )
            .unwrap();
            assert_eq!(
                proof.verify_event("operation", &expected),
                Err(Error::Integrity)
            );
            assert_eq!(proof.verify_observation(&observed), Err(Error::Integrity));
            drop(proof);
            tx.rollback().unwrap();
        }
        store.integrity_check().unwrap();
    }

    #[test]
    fn repeated_references_charge_every_position_in_verified_metadata() {
        let (_dir, store, expected, _) = fixture();
        let tx = store.connection.unchecked_transaction().unwrap();
        let mut proof = OpenVerification::new(&tx).unwrap();
        // Unit boundary for verified size metadata: two unique items, three positions.
        for cost in proof.evidence_costs.values_mut() {
            *cost = evidence::MAX_OPERATION_BYTES / 3 + 1;
        }
        assert_eq!(
            proof.verify_event("operation", &expected),
            Err(Error::Limit)
        );
    }

    #[test]
    fn no_proof_capacity_still_checks_all_physical_objects_and_associations() {
        for case in 0..3 {
            let (_dir, store, _, _) = fixture();
            let tx = store.connection.unchecked_transaction().unwrap();
            let damage = match case {
                0 => "UPDATE evidence_chunks SET payload=x'00'",
                1 => "UPDATE read_archive_parts SET payload=x'00'",
                _ => "DELETE FROM operation_read_archives",
            };
            tx.execute(damage, []).unwrap();
            assert!(OpenVerification::build(&tx, 0).is_err());
            assert!(OpenVerification::build(&tx, MAX_ENTRIES).is_err());
            tx.rollback().unwrap();
            store.integrity_check().unwrap();
        }
    }
}
