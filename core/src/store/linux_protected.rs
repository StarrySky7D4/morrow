//! Crate-private, bounded real Store SQL slice over the owned Linux VFS.
//!
//! This distinct type has no Deref, raw Connection access or conversion to
//! Store. Only current-schema audited cards without attachments/evidence and
//! channel ACK history are supported. Unported Store methods cannot be called
//! on it. Its synthetic constructor is test-only. Public private factories,
//! readonly/snapshot/backup/recovery and product/audit owners remain disabled.
use super::{
    APPLICATION_ID, BASE_SCHEMA, CardRecord, ChannelAckReceipt, ChannelCheckpoint, ChannelCommit,
    EventBudget, Lookup, Receipt, RenameRequest, SCHEMA_VERSION, Store, StoreMutation,
    apply_card_in_transaction, binding, blobs, boundary, channel_journal, evidence,
    evidence_chunks, file_content, file_content_receipt, identity, io_evidence, io_intent,
    lookup_connection, lookup_for_card_connection, outbound_authority, pending_connection,
    pending_usage_connection, read_archive, read_archive_budget, read_archive_retention,
    read_capture, read_card, records, seals, service_authority, service_config, sql, tls_identity,
    transaction,
};
use crate::{
    Error, Result,
    audit::TrustedLog,
    linux_storage::{GuardedSqliteConnection, GuardedStoreOutcome, StoreObjectGuard},
};
use rusqlite::Connection;
#[cfg(test)]
use std::path::Path;

/// No generic fallback or unported Store API exists on this internal type.
/// Operations borrow it mutably; moving ownership is allowed, sharing is not.
pub(super) struct ProtectedStoreSlice {
    database: GuardedSqliteConnection,
    budget: EventBudget,
    trust: TrustedLog,
    changes_identity: std::sync::Arc<()>,
    changes_liveness: std::sync::Arc<()>,
}
fn outcome<T>(value: StoreMutation<T>) -> GuardedStoreOutcome<T> {
    match value {
        StoreMutation::Commit(value) => GuardedStoreOutcome::Commit(value),
        StoreMutation::Unchanged(value) => GuardedStoreOutcome::Unchanged(value),
    }
}
fn mutation<T>(
    database: &mut GuardedSqliteConnection,
    after: &str,
    operation: impl for<'db> FnOnce(&'db Connection, StoreObjectGuard<'db>) -> Result<StoreMutation<T>>,
) -> Result<T> {
    let mut committed = false;
    let result = database.store_transaction(|c, guard| {
        let value = operation(c, guard)?;
        committed = matches!(&value, StoreMutation::Commit(_));
        Ok(outcome(value))
    });
    if committed && result.is_ok() {
        boundary(after);
    }
    result
}
fn authority(
    guard: &StoreObjectGuard<'_>,
    callback: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    let _exit = guard.verify_on_exit();
    guard.verify()?;
    let result = callback();
    // Cache-only SQL after an external callback must not access/return data
    // from an object the callback made unsafe, even if it returned an error.
    guard.verify()?;
    result
}
fn supported_card(card: &CardRecord) -> Result<()> {
    if !card.attachments().is_empty() {
        return Err(Error::Invalid(
            "protected Store slice attachments unavailable",
        ));
    }
    Ok(())
}
/// Reject persisted unported features on reopen before any mutation. The full
/// portable integrity verifier still verifies the schema/binding/event bytes.
fn supported_content(c: &Connection) -> Result<()> {
    for table in [
        "blobs",
        "card_blobs",
        "event_blobs",
        "retentions",
        "records",
        "sealed_segments",
        "task_evidence",
        "operation_evidence",
        "evidence_chunks",
        "task_evidence_chunks",
        "read_archives",
        "read_archive_parts",
        "operation_read_archives",
        "read_captures",
        "io_intents",
        "io_reservations",
        "io_evidence",
        "io_material_reservations",
        "service_configs",
        "service_authorities",
        "outbound_authorities",
        "tls_identities",
        "file_mutation_content",
        "file_content_receipts",
    ] {
        let count: i64 =
            sql(c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)))?;
        if count != 0 {
            return Err(Error::Invalid(
                "protected Store slice persisted feature unavailable",
            ));
        }
    }
    let kinds: i64 = sql(c.query_row(
        "SELECT count(*) FROM operations WHERE object_kind!=0",
        [],
        |r| r.get(0),
    ))?;
    if kinds != 0 {
        return Err(Error::Invalid(
            "protected Store slice persisted feature unavailable",
        ));
    }
    Ok(())
}
fn initialize_current(
    c: &Connection,
    trust: &TrustedLog,
    create: bool,
) -> Result<StoreMutation<()>> {
    let app: i64 = sql(c.query_row("PRAGMA application_id", [], |r| r.get(0)))?;
    let version: i64 = sql(c.query_row("PRAGMA user_version", [], |r| r.get(0)))?;
    if app == 0 && version == 0 && create {
        let tables: i64 = sql(c.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        ))?;
        if tables != 0 {
            return Err(Error::Invalid("unrelated database"));
        }
        // This is guarded initialization of a genuinely empty actual fd, not
        // generic fixture setup. Every current schema definition is shared with
        // the portable initializer; no historical payload migration is claimed.
        for schema in [
            BASE_SCHEMA,
            blobs::SCHEMA,
            records::SCHEMA,
            seals::SCHEMA,
            binding::SCHEMA,
            evidence::SCHEMA,
            evidence_chunks::SCHEMA,
            read_archive::SCHEMA,
            read_capture::SCHEMA,
            io_intent::SCHEMA,
            io_evidence::SCHEMA,
            service_config::SCHEMA,
            service_authority::SCHEMA,
            service_authority::IDENTITY_SCHEMA,
            outbound_authority::SCHEMA,
            tls_identity::SCHEMA,
            file_content::SCHEMA,
            file_content_receipt::SCHEMA,
        ] {
            sql(c.execute_batch(schema))?;
        }
        read_archive_retention::migrate(c)?;
        sql(c.execute_batch(&io_intent::reservation_schema()))?;
        service_authority::create_identity(c)?;
        channel_journal::create(c)?;
        sql(c.execute_batch(read_archive_budget::INDEX))?;
        sql(c.pragma_update(None, "user_version", SCHEMA_VERSION))?;
        binding::bind(c, trust)?;
        supported_content(c)?;
        Store::integrity_connection(c, Some(trust))?;
        Ok(StoreMutation::Commit(()))
    } else if app == APPLICATION_ID && version == SCHEMA_VERSION {
        // Existing current data must already be bound. Never silently bind a
        // current unaudited file or rewrite identity, indexes, schema or payloads.
        let existing = binding::read(c)?.ok_or(Error::Invalid(
            "protected Store slice audit binding required",
        ))?;
        if existing.log_id != trust.id || existing.public_key != *trust.key.as_bytes() {
            return Err(Error::Integrity);
        }
        supported_content(c)?;
        Store::integrity_connection(c, Some(trust))?;
        Ok(StoreMutation::Unchanged(()))
    } else {
        Err(Error::UnsupportedVersion)
    }
}
impl ProtectedStoreSlice {
    // No production admission route is exported. Synthetic tests provide an
    // explicit private temporary directory and fixed Core signing-key trust.
    #[cfg(test)]
    fn open_synthetic(
        path: &Path,
        budget: EventBudget,
        create: bool,
        trust: TrustedLog,
    ) -> Result<Self> {
        identity(&trust.id)?;
        if trust.key.is_weak() {
            return Err(Error::Invalid("weak audit key"));
        }
        let mut database = GuardedSqliteConnection::open_store(
            path,
            create,
            APPLICATION_ID as u32,
            SCHEMA_VERSION as u32,
        )?;
        database.configure_store()?;
        database.store_transaction(|c, _| initialize_current(c, &trust, create).map(outcome))?;
        let mut store = Self {
            database,
            budget,
            trust,
            changes_identity: std::sync::Arc::new(()),
            changes_liveness: std::sync::Arc::new(()),
        };
        store.integrity_check()?;
        Ok(store)
    }
    /// Synthetic guarded SQL support only; this is not a HostRuntime backend.
    pub(super) fn open_changes_window(&mut self, package: [u8;32], cards: &[String],
        start: super::ChangesStart, budget: super::ChangesBudget,
        mut authorize: impl FnMut()->Result<()>) -> Result<super::ChangesWindow> {
        let owner=self.changes_identity.clone();
        let liveness=std::sync::Arc::downgrade(&self.changes_liveness);
        self.database.store_read(|c,guard| {
            super::changes_metadata::open_connection(c,owner,liveness,package,cards,start,budget,
                &mut || authority(&guard,&mut authorize))
        })
    }
    pub(super) fn materialize_changes_window(&mut self, window: super::ChangesWindow,
        mut authorize: impl FnMut()->Result<()>) -> Result<super::ChangesBatch> {
        let owner=self.changes_identity.clone();
        self.database.store_read(|c,guard| {
            super::changes_metadata::materialize_connection(c,&owner,window,
                &mut || authority(&guard,&mut authorize))
        })
    }
    pub(super) fn card(&mut self, id: &str) -> Result<Option<CardRecord>> {
        self.database.store_read(|c, _| {
            identity(id)?;
            read_card(c, id)
        })
    }
    pub(super) fn lookup(&mut self, operation: &str) -> Result<Lookup> {
        self.database
            .store_read(|c, _| lookup_connection(c, operation))
    }
    pub(super) fn lookup_for_card(&mut self, card: &str, operation: &str) -> Result<Lookup> {
        self.database
            .store_read(|c, _| lookup_for_card_connection(c, card, operation))
    }
    pub(super) fn pending_usage(&mut self) -> Result<(u64, u64)> {
        self.database.store_read(|c, _| pending_usage_connection(c))
    }
    pub(super) fn pending(&mut self, after: i64, limit: u32) -> Result<Vec<(i64, Vec<u8>)>> {
        self.database
            .store_read(|c, _| pending_connection(c, after, limit))
    }
    pub(super) fn integrity_check(&mut self) -> Result<()> {
        self.database.store_read(|c, _| {
            supported_content(c)?;
            Store::integrity_connection(c, Some(&self.trust))
        })
    }
    pub(super) fn create_local(&mut self, operation: &str, card: &CardRecord) -> Result<Receipt> {
        self.create_authorized(operation, card, || Ok(()))
    }
    pub(super) fn create_authorized(
        &mut self,
        operation: &str,
        card: &CardRecord,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Receipt> {
        let budget = self.budget;
        mutation(&mut self.database, "after-commit", |c, guard| {
            supported_card(card)?;
            if card.summary().revision != 1 {
                return Err(Error::Invalid("creation revision"));
            }
            let command = transaction::create_command(operation, card)?;
            apply_card_in_transaction(
                c,
                budget,
                operation,
                &card.summary().id,
                command,
                &[],
                |_| Ok(card.clone()),
                || authority(&guard, &mut authorize),
                true,
            )
        })
    }
    pub(super) fn rename(
        &mut self,
        request: &RenameRequest,
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<Receipt> {
        let budget = self.budget;
        mutation(&mut self.database, "after-commit", |c, guard| {
            let command = transaction::rename_command(request)?;
            apply_card_in_transaction(
                c,
                budget,
                &request.operation_id,
                &request.card_id,
                command,
                &[],
                |current| {
                    let current = current.ok_or(Error::NotFound)?;
                    supported_card(current)?;
                    request.propose(current)
                },
                || authority(&guard, &mut authorize),
                false,
            )
        })
    }
    pub(super) fn channel_checkpoint(
        &mut self,
        subscription: &[u8; 32],
        epoch: &[u8; 32],
    ) -> Result<Option<ChannelCheckpoint>> {
        self.database
            .store_read(|c, _| channel_journal::checkpoint(c, subscription, epoch))
    }
    pub(super) fn channel_ack_receipt(
        &mut self,
        subscription: &[u8; 32],
        epoch: &[u8; 32],
        sequence: u64,
    ) -> Result<Option<ChannelAckReceipt>> {
        self.database
            .store_read(|c, _| channel_journal::receipt(c, subscription, epoch, sequence))
    }
    pub(super) fn commit_channel_ack_guarded(
        &mut self,
        subscription: &[u8; 32],
        expected: Option<&ChannelCheckpoint>,
        frame: &[u8],
        request: &[u8],
        response: &[u8],
        mut authorize: impl FnMut() -> Result<()>,
    ) -> Result<ChannelCommit> {
        let budget = self.budget;
        mutation(
            &mut self.database,
            "channel-checkpoint-after-commit",
            |c, guard| {
                let prepared =
                    channel_journal::prepare_ack(c, subscription, frame, request, response)?;
                channel_journal::commit_ack_in_transaction(c, budget, expected, prepared, || {
                    authority(&guard, &mut authorize)
                })
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn directory() -> tempfile::TempDir {
        tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap()
    }
    fn trust() -> TrustedLog {
        TrustedLog {
            id: "synthetic-linux-store".into(),
            key: crate::audit::SigningKey::from_bytes(&[51; 32]).verifying_key(),
        }
    }
    fn store(path: &Path) -> ProtectedStoreSlice {
        ProtectedStoreSlice::open_synthetic(path, Default::default(), true, trust()).unwrap()
    }
    fn card() -> CardRecord {
        CardRecord::new("card", "text", 1, "kept", vec![1, 2, 3]).unwrap()
    }
    fn rename(operation: &str, revision: u64) -> RenameRequest {
        RenameRequest {
            operation_id: operation.into(),
            card_id: "card".into(),
            expected_revision: revision,
            title: "renamed".into(),
        }
    }
    fn material(sequence: u64, call: u8) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        use crate::channel::{Action, Frame, Request, Response, Status};
        let frame = Frame {
            sequence,
            source_epoch: [3; 32],
            bytes: format!("event-{sequence}").into_bytes(),
            cursor: sequence.to_le_bytes().to_vec(),
        };
        let request = Request {
            call_id: [call; 32],
            reference: [2; 32],
            source_epoch: [3; 32],
            action: Action::Ack {
                sequence,
                frame_sha256: frame.digest().unwrap(),
                cursor: frame.cursor.clone(),
            },
        };
        let response = Response {
            call_id: request.call_id,
            request_sha256: request.digest().unwrap(),
            reference: request.reference,
            source_epoch: request.source_epoch,
            status: Status::Acked,
            frame: None,
            last_acked: sequence,
            accepted_sequence: 0,
            resource_reclaimed: false,
        };
        (
            frame.encode().unwrap(),
            request.encode().unwrap(),
            response.encode().unwrap(),
        )
    }
    #[test]
    fn changes_guarded_materialization_and_same_store_ack_have_atomic_authority() {
        use crate::channel::{Action,Frame,Request,Response,Status};
        let dir=directory();let path=dir.path().join("changes.db");let mut s=store(&path);
        let create=s.create_local("create",&card()).unwrap();
        s.rename(&rename("rename",1),||Ok(())).unwrap();
        let cards=vec!["card".into()];
        let w=s.open_changes_window([7;32],&cards,super::super::ChangesStart::Beginning,
            super::super::ChangesBudget{page_candidates:1,..Default::default()},||Ok(())).unwrap();
        let b=s.materialize_changes_window(w,||Ok(())).unwrap();assert_eq!(b.changes().len(),2);
        assert_eq!(b.changes()[0].card_sha256,create.content_sha256);
        let metadata=b.into_metadata([3;32]).unwrap().remove(0);
        let frame=Frame{sequence:1,source_epoch:[3;32],bytes:metadata.encode().unwrap(),cursor:metadata.cursor().unwrap().to_vec()};
        let request=Request{call_id:[1;32],reference:[2;32],source_epoch:[3;32],action:Action::Ack{sequence:1,frame_sha256:frame.digest().unwrap(),cursor:frame.cursor.clone()}};
        let response=Response{call_id:request.call_id,request_sha256:request.digest().unwrap(),reference:request.reference,source_epoch:request.source_epoch,status:Status::Acked,frame:None,last_acked:1,accepted_sequence:0,resource_reclaimed:false};
        let f=frame.encode().unwrap();let q=request.encode().unwrap();let r=response.encode().unwrap();
        let mut checks=0;
        assert_eq!(s.commit_channel_ack_guarded(&[9;32],None,&f,&q,&r,||{checks+=1;if checks==2{Err(Error::Integrity)}else{Ok(())}}),Err(Error::Integrity));
        assert_eq!(checks,2);assert!(s.channel_checkpoint(&[9;32],&[3;32]).unwrap().is_none());assert!(s.channel_ack_receipt(&[9;32],&[3;32],1).unwrap().is_none());
        assert!(matches!(s.commit_channel_ack_guarded(&[9;32],None,&f,&q,&r,||Ok(())).unwrap(),ChannelCommit::Committed(_)));
        assert!(matches!(s.commit_channel_ack_guarded(&[9;32],None,&f,&q,&r,||Ok(())).unwrap(),ChannelCommit::Duplicate(_)));
        drop(s);let mut s=ProtectedStoreSlice::open_synthetic(&path,Default::default(),false,trust()).unwrap();
        assert_eq!(s.channel_ack_receipt(&[9;32],&[3;32],1).unwrap().unwrap().frame_wire,f);
        let w=s.open_changes_window([7;32],&cards,super::super::ChangesStart::After(metadata),Default::default(),||Ok(())).unwrap();
        assert_eq!(s.materialize_changes_window(w,||Ok(())).unwrap().changes()[0].operation_id,"rename");
    }
    #[test]
    fn changes_guarded_foreign_window_and_read_authority_reject() {
        let dir=directory();let mut s=store(&dir.path().join("one.db"));let mut other=store(&dir.path().join("two.db"));s.create_local("create",&card()).unwrap();
        let cards=vec!["card".into()];let w=s.open_changes_window([7;32],&cards,super::super::ChangesStart::Beginning,Default::default(),||Ok(())).unwrap();
        assert!(other.materialize_changes_window(w,||Ok(())).is_err());
        let w=s.open_changes_window([7;32],&cards,super::super::ChangesStart::Beginning,Default::default(),||Ok(())).unwrap();
        let mut count=0;assert!(s.materialize_changes_window(w,||{count+=1;if count==4{Err(Error::Integrity)}else{Ok(())}}).is_err());
        s.integrity_check().unwrap();
    }
    #[test]
    fn changes_guarded_final_object_check_never_releases_unsafe_batch() {
        let dir=directory();let path=dir.path().join("changes.db");let mut s=store(&path);s.create_local("create",&card()).unwrap();
        let cards=vec!["card".into()];let w=s.open_changes_window([7;32],&cards,super::super::ChangesStart::Beginning,Default::default(),||Ok(())).unwrap();
        let target=path.clone();s.database.test_after_store_read(move||std::fs::set_permissions(target,std::fs::Permissions::from_mode(0o640)).unwrap());
        assert!(s.materialize_changes_window(w,||Ok(())).is_err());
        std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o600)).unwrap();assert!(s.card("card").is_err());
    }
    #[test]
    fn guarded_fresh_init_real_card_and_ack_match_portable_bytes_and_reopen() {
        let dir = directory();
        let path = dir.path().join("protected.db");
        let mut protected = store(&path);
        let generic_path = dir.path().join("portable.db");
        let mut generic =
            Store::open_audited(&generic_path, Default::default(), true, trust()).unwrap();
        let first = protected.create_local("create", &card()).unwrap();
        assert_eq!(first, generic.create_local("create", &card()).unwrap());
        assert_eq!(
            protected.card("card").unwrap().unwrap().encode(),
            generic.card("card").unwrap().unwrap().encode()
        );
        let request = rename("rename", 1);
        assert_eq!(
            protected.rename(&request, || Ok(())).unwrap(),
            generic.rename(&request, || Ok(())).unwrap()
        );
        assert_eq!(
            protected.lookup("rename").unwrap(),
            generic.lookup("rename").unwrap()
        );
        assert_eq!(
            protected.lookup_for_card("other", "rename").unwrap(),
            Lookup::Absent
        );
        assert_eq!(
            protected.lookup_for_card("card", "rename").unwrap(),
            generic.lookup_for_card("card", "rename").unwrap()
        );
        assert_eq!(
            protected.pending(0, 128).unwrap(),
            generic.pending(0, 128).unwrap()
        );
        assert_eq!(
            protected.pending_usage().unwrap(),
            generic.pending_usage().unwrap()
        );
        let (a, b, c) = material(1, 1);
        let committed = protected
            .commit_channel_ack_guarded(&[9; 32], None, &a, &b, &c, || Ok(()))
            .unwrap();
        assert_eq!(
            committed,
            generic
                .commit_channel_ack(&[9; 32], None, &a, &b, &c)
                .unwrap()
        );
        assert_eq!(
            protected
                .channel_ack_receipt(&[9; 32], &[3; 32], 1)
                .unwrap(),
            generic.channel_ack_receipt(&[9; 32], &[3; 32], 1).unwrap()
        );
        assert_eq!(
            protected.channel_checkpoint(&[9; 32], &[3; 32]).unwrap(),
            generic.channel_checkpoint(&[9; 32], &[3; 32]).unwrap()
        );
        protected.integrity_check().unwrap();
        generic.integrity_check().unwrap();
        drop(protected);
        let before = std::fs::read(&path).unwrap();
        let mut protected =
            ProtectedStoreSlice::open_synthetic(&path, Default::default(), false, trust()).unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "current reopen must not rewrite/rebind"
        );
        assert_eq!(
            protected.pending(0, 128).unwrap(),
            generic.pending(0, 128).unwrap()
        );
        assert_eq!(
            protected.channel_checkpoint(&[9; 32], &[3; 32]).unwrap(),
            generic.channel_checkpoint(&[9; 32], &[3; 32]).unwrap()
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!dir.path().join("protected.db-wal").exists());
        assert!(!dir.path().join("protected.db-shm").exists());
    }
    #[test]
    fn exact_card_retry_and_duplicate_ack_exit_without_commit_or_sync() {
        let dir = directory();
        let path = dir.path().join("retry.db");
        let mut store = store(&path);
        let created = store.create_local("create", &card()).unwrap();
        let (a, b, c) = material(1, 1);
        let first = store
            .commit_channel_ack_guarded(&[9; 32], None, &a, &b, &c, || Ok(()))
            .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let commits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = commits.clone();
        store.database.test_after_store_commit(move || {
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        store.database.test_fail_store_sync();
        assert_eq!(store.create_local("create", &card()).unwrap(), created);
        let (_, b2, c2) = material(1, 4);
        let duplicate = store
            .commit_channel_ack_guarded(&[9; 32], None, &a, &b2, &c2, || Ok(()))
            .unwrap();
        let ChannelCommit::Committed(checkpoint) = first else {
            panic!("commit required")
        };
        assert_eq!(duplicate, ChannelCommit::Duplicate(checkpoint));
        assert_eq!(commits.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(
            store.rename(&rename("later", 1), || Ok(())),
            Err(Error::CommitUnknown),
            "retained sync fault proves retries performed no sync/commit"
        );
        assert!(store.card("card").is_err());
    }
    #[test]
    fn cache_empty_error_and_all_read_results_are_withheld_after_metadata_fault() {
        // Eight actual read APIs, plus an empty read and an error path. Each
        // scope is warmed first; the hook runs after the shared SQL returned.
        for case in 0..10 {
            let dir = directory();
            let path = dir.path().join("cached.db");
            let mut store = store(&path);
            store.create_local("create", &card()).unwrap();
            let (a, b, c) = material(1, 1);
            store
                .commit_channel_ack_guarded(&[9; 32], None, &a, &b, &c, || Ok(()))
                .unwrap();
            fn read(store: &mut ProtectedStoreSlice, case: u8) -> Result<()> {
                match case {
                    0 => store.card("card").map(|_| ()),
                    1 => store.lookup("create").map(|_| ()),
                    2 => store.lookup_for_card("card", "create").map(|_| ()),
                    3 => store.pending_usage().map(|_| ()),
                    4 => store.pending(0, 128).map(|_| ()),
                    5 => store.integrity_check(),
                    6 => store.channel_checkpoint(&[9; 32], &[3; 32]).map(|_| ()),
                    7 => store.channel_ack_receipt(&[9; 32], &[3; 32], 1).map(|_| ()),
                    8 => store.card("absent").map(|_| ()),
                    _ => store.pending(0, 0).map(|_| ()),
                }
            }
            let _ = read(&mut store, case);
            let before = std::fs::read(&path).unwrap();
            let target = path.clone();
            store.database.test_after_store_read(move || {
                std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o640)).unwrap()
            });
            assert_eq!(
                read(&mut store, case),
                Err(Error::Invalid("unsafe owned Linux SQLite object")),
                "read seam {case}"
            );
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert!(
                store.card("card").is_err(),
                "poison survives mode restoration"
            );
            assert_eq!(
                std::fs::read(&path).unwrap(),
                before,
                "read scope performed no write"
            );
        }
    }
    #[test]
    fn pre_operation_and_post_authority_faults_block_cached_reads_and_mutations() {
        let dir = directory();
        let path = dir.path().join("pre.db");
        let mut store = store(&path);
        store.create_local("create", &card()).unwrap();
        store.card("card").unwrap();
        let bytes = std::fs::read(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(store.card("card").is_err());
        assert!(store.create_local("create", &card()).is_err());
        let (a, b, c) = material(1, 1);
        assert!(
            store
                .commit_channel_ack_guarded(&[9; 32], None, &a, &b, &c, || Ok(()))
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        for returning_error in [false, true] {
            let dir = directory();
            let path = dir.path().join("callback.db");
            let mut store = super::tests::store(&path);
            let bytes = std::fs::read(&path).unwrap();
            let result = store.create_authorized("new", &card(), || {
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
                if returning_error {
                    Err(Error::Integrity)
                } else {
                    Ok(())
                }
            });
            assert_eq!(
                result,
                Err(Error::Invalid("unsafe owned Linux SQLite object"))
            );
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
    }
    #[test]
    fn final_authority_metadata_fault_is_checked_before_card_or_ack_commit() {
        for ack in [false, true] {
            let dir = directory();
            let path = dir.path().join("final.db");
            let mut store = store(&path);
            let before = std::fs::read(&path).unwrap();
            let mut calls = 0;
            let mut authorize = || {
                calls += 1;
                if calls == 2 {
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))
                        .unwrap();
                }
                Ok(())
            };
            if ack {
                let (a, b, c) = material(1, 1);
                assert!(
                    store
                        .commit_channel_ack_guarded(&[9; 32], None, &a, &b, &c, &mut authorize)
                        .is_err()
                );
            } else {
                assert!(
                    store
                        .create_authorized("create", &card(), &mut authorize)
                        .is_err()
                );
            }
            assert_eq!(calls, 2);
            assert_eq!(std::fs::read(&path).unwrap(), before);
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert!(store.lookup("create").is_err());
            drop(store);
            // Unsafe rollback may have failed, so recovery is deliberately not
            // performed by this current-only Store slice. Retain journal bytes.
            let journal = dir.path().join("final.db-journal");
            assert!(journal.exists());
            let retained = std::fs::read(&journal).unwrap();
            assert!(
                ProtectedStoreSlice::open_synthetic(&path, Default::default(), false, trust())
                    .is_err()
            );
            assert_eq!(std::fs::read(&journal).unwrap(), retained);
        }
    }
    #[test]
    fn committed_then_postguard_fault_returns_unknown_and_requires_fresh_reconciliation() {
        let dir = directory();
        let path = dir.path().join("unknown.db");
        let mut store = store(&path);
        let target = path.clone();
        store.database.test_after_store_commit(move || {
            std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o640)).unwrap()
        });
        assert_eq!(
            store.create_local("create", &card()),
            Err(Error::CommitUnknown)
        );
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(store.lookup("create").is_err());
        assert!(store.pending_usage().is_err());
        drop(store);
        let mut reconciled =
            ProtectedStoreSlice::open_synthetic(&path, Default::default(), false, trust()).unwrap();
        assert!(matches!(
            reconciled.lookup("create").unwrap(),
            Lookup::Committed(_)
        ));
        assert_eq!(
            reconciled.card("card").unwrap().unwrap().encode(),
            card().encode()
        );
        assert_eq!(reconciled.pending_usage().unwrap().0, 1);
    }
    #[test]
    fn explicit_rollback_failure_and_sync_uncertainty_poison_even_valid_metadata() {
        let dir = directory();
        let path = dir.path().join("rollback.db");
        let mut store = store(&path);
        store.create_local("create", &card()).unwrap();
        let before = std::fs::read(&path).unwrap();
        store.database.test_fail_store_rollback();
        assert_eq!(store.create_local("create", &card()), Err(Error::Storage));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(store.card("card").is_err());
        drop(store);
        let mut store =
            ProtectedStoreSlice::open_synthetic(&path, Default::default(), false, trust()).unwrap();
        store.database.test_fail_store_sync();
        assert_eq!(
            store.rename(&rename("edit", 1), || Ok(())),
            Err(Error::CommitUnknown)
        );
        assert!(store.integrity_check().is_err());
        assert!(store.lookup("edit").is_err());
    }
    #[test]
    fn authority_rejection_and_panic_rollback_real_store_writes_without_result() {
        for panic in [false, true] {
            let dir = directory();
            let path = dir.path().join("rollback.db");
            let mut store = store(&path);
            let mut calls = 0;
            let mut authorize = || {
                calls += 1;
                if calls == 2 {
                    if panic {
                        panic!("final authority panic");
                    }
                    Err(Error::Integrity)
                } else {
                    Ok(())
                }
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                store.create_authorized("abort", &card(), &mut authorize)
            }));
            if panic {
                assert!(result.is_err());
            } else {
                assert_eq!(result.unwrap(), Err(Error::Integrity));
            }
            assert_eq!(calls, 2);
            assert_eq!(store.lookup("abort").unwrap(), Lookup::Absent);
            assert!(store.card("card").unwrap().is_none());
            assert_eq!(store.pending_usage().unwrap(), (0, 0));
            store.integrity_check().unwrap();
            store.create_local("okay", &card()).unwrap();
        }
    }
    #[test]
    fn borrowed_adapter_denies_control_cached_write_and_write_returning_in_reads() {
        let dir = directory();
        let path = dir.path().join("escape.db");
        let mut store = store(&path);
        for control in [
            "COMMIT",
            "END",
            "ROLLBACK",
            "BEGIN",
            "BEGIN IMMEDIATE",
            "SAVEPOINT sp_guarded",
            "RELEASE sp_guarded",
            "ROLLBACK TO sp_guarded",
        ] {
            assert!(
                store
                    .database
                    .store_read::<()>(|c, _| {
                        let error = c.execute_batch(control).unwrap_err();
                        assert_eq!(
                            error.sqlite_error_code(),
                            Some(rusqlite::ErrorCode::AuthorizationForStatementDenied),
                            "read authorizer {control}"
                        );
                        Err(Error::Storage)
                    })
                    .is_err(),
                "read {control}"
            );
            assert_eq!(
                store.database.store_transaction::<()>(|c, _| {
                    let error = c.execute_batch(control).unwrap_err();
                    assert_eq!(
                        error.sqlite_error_code(),
                        Some(rusqlite::ErrorCode::AuthorizationForStatementDenied),
                        "transaction authorizer {control}"
                    );
                    Err(Error::Integrity)
                }),
                Err(Error::Integrity)
            );
        }
        let text = "INSERT INTO cards(id,payload) VALUES('escape',X'00') RETURNING id";
        assert_eq!(
            store.database.store_transaction::<()>(|c, _| {
                let statement = sql(c.prepare_cached(text))?;
                drop(statement);
                Err(Error::Integrity)
            }),
            Err(Error::Integrity)
        );
        let called = std::cell::Cell::new(false);
        assert!(
            store
                .database
                .store_read(
                    |c, _| sql(sql(c.prepare_cached(text))?.query_row([], |row| {
                        called.set(true);
                        row.get::<_, String>(0)
                    }))
                )
                .is_err()
        );
        assert!(!called.get());
        assert!(store.card("escape").unwrap().is_none());
        assert!(
            store
                .database
                .store_read(|c, _| sql(c.execute_batch("PRAGMA application_id=1")))
                .is_err()
        );
        assert!(
            store
                .database
                .store_read(|c, _| sql(c.execute_batch("PRAGMA foreign_keys=OFF")))
                .is_err()
        );
        assert!(
            store
                .database
                .store_transaction::<()>(|c, _| {
                    sql(c.execute_batch("PRAGMA foreign_keys=OFF"))?;
                    Ok(GuardedStoreOutcome::Commit(()))
                })
                .is_err()
        );
        store.create_local("create", &card()).unwrap();
        store.integrity_check().unwrap();
    }
    #[test]
    fn actual_thread_handoff_use_return_and_owner_drop_preserve_exclusive_lifetime() {
        fn send<T: Send>() {}
        send::<ProtectedStoreSlice>();
        send::<GuardedSqliteConnection>();
        let dir = directory();
        let path = dir.path().join("move.db");
        let mut store = store(&path);
        store.create_local("create", &card()).unwrap();
        let mut store = std::thread::spawn(move || {
            let mut store = store;
            store.rename(&rename("edit", 1), || Ok(())).unwrap();
            let (a, b, c) = material(1, 1);
            store
                .commit_channel_ack_guarded(&[9; 32], None, &a, &b, &c, || Ok(()))
                .unwrap();
            store
        })
        .join()
        .unwrap();
        assert_eq!(store.card("card").unwrap().unwrap().summary().revision, 2);
        assert!(matches!(
            GuardedSqliteConnection::open(&path, false),
            Err(Error::StorageBusy)
        ));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (drop_tx, drop_rx) = std::sync::mpsc::channel();
        let owner = std::thread::spawn(move || {
            store.integrity_check().unwrap();
            ready_tx.send(()).unwrap();
            drop_rx.recv().unwrap();
            drop(store);
        });
        ready_rx.recv().unwrap();
        assert!(matches!(
            GuardedSqliteConnection::open(&path, false),
            Err(Error::StorageBusy)
        ));
        drop_tx.send(()).unwrap();
        owner.join().unwrap();
        let mut reopened =
            ProtectedStoreSlice::open_synthetic(&path, Default::default(), false, trust()).unwrap();
        assert_eq!(
            reopened.card("card").unwrap().unwrap().summary().revision,
            2
        );
    }
    #[test]
    fn store_profile_rejects_old_future_unrelated_partial_and_hot_states_without_changes() {
        for case in 0..6 {
            let dir = directory();
            let path = dir.path().join("unsupported.db");
            let original = store(&path);
            drop(original);
            let mut bytes = std::fs::read(&path).unwrap();
            match case {
                0 => bytes[60..64].copy_from_slice(&23u32.to_be_bytes()),
                1 => bytes[60..64].copy_from_slice(&25u32.to_be_bytes()),
                2 => bytes[68..72].copy_from_slice(&1u32.to_be_bytes()),
                3 => bytes.truncate(99),
                4 => bytes[..16].fill(0),
                _ => {
                    let journal = dir.path().join("unsupported.db-journal");
                    std::fs::write(&journal, b"retained unsupported journal").unwrap();
                    std::fs::set_permissions(&journal, std::fs::Permissions::from_mode(0o600))
                        .unwrap();
                }
            }
            std::fs::write(&path, &bytes).unwrap();
            let before: std::collections::BTreeMap<_, _> = std::fs::read_dir(dir.path())
                .unwrap()
                .map(|entry| {
                    let p = entry.unwrap().path();
                    (
                        p.file_name().unwrap().to_os_string(),
                        std::fs::read(p).unwrap(),
                    )
                })
                .collect();
            assert!(
                ProtectedStoreSlice::open_synthetic(&path, Default::default(), true, trust())
                    .is_err(),
                "header case {case}"
            );
            let after: std::collections::BTreeMap<_, _> = std::fs::read_dir(dir.path())
                .unwrap()
                .map(|entry| {
                    let p = entry.unwrap().path();
                    (
                        p.file_name().unwrap().to_os_string(),
                        std::fs::read(p).unwrap(),
                    )
                })
                .collect();
            assert_eq!(after, before, "refusal must precede SQLite IO/recovery");
        }
        let dir = directory();
        let path = dir.path().join("missing.db");
        let journal = dir.path().join("missing.db-journal");
        std::fs::write(&journal, b"held").unwrap();
        std::fs::set_permissions(&journal, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(
            ProtectedStoreSlice::open_synthetic(&path, Default::default(), true, trust()).is_err()
        );
        assert!(!path.exists());
        assert_eq!(std::fs::read(journal).unwrap(), b"held");
    }
    #[test]
    fn wrong_trust_unbound_current_and_unported_rows_refuse_without_rebinding() {
        let dir = directory();
        let path = dir.path().join("bound.db");
        drop(store(&path));
        let before = std::fs::read(&path).unwrap();
        let mut wrong = trust();
        wrong.key = crate::audit::SigningKey::from_bytes(&[52; 32]).verifying_key();
        assert!(
            ProtectedStoreSlice::open_synthetic(&path, Default::default(), false, wrong).is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let path = dir.path().join("unbound.db");
        drop(Store::open_exclusive(&path, Default::default(), true).unwrap());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(
            ProtectedStoreSlice::open_synthetic(&path, Default::default(), false, trust()).is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let path = dir.path().join("unported.db");
        drop(store(&path));
        let mut db = GuardedSqliteConnection::open(&path, false).unwrap();
        db.execute(
            "INSERT INTO records(kind,id,payload) VALUES(1,'not-qualified',X'00')",
            [],
        )
        .unwrap();
        drop(db);
        let before = std::fs::read(&path).unwrap();
        assert!(
            ProtectedStoreSlice::open_synthetic(&path, Default::default(), false, trust()).is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    fn contender(path: &Path) -> i32 {
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "store::linux_protected::tests::store_process_probe",
                "--nocapture",
            ])
            .env("MORROW_STORE_LOCK_PROBE", path)
            .status()
            .unwrap()
            .code()
            .unwrap()
    }
    #[test]
    fn store_process_probe() {
        let Some(path) = std::env::var_os("MORROW_STORE_LOCK_PROBE") else {
            return;
        };
        let c = rusqlite::Connection::open(path).unwrap();
        c.busy_timeout(std::time::Duration::ZERO).unwrap();
        std::process::exit(match c.execute_batch("BEGIN IMMEDIATE") {
            Ok(()) => 0,
            Err(error) if error.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseBusy) => {
                75
            }
            Err(_) => 79,
        });
    }
    #[test]
    fn typed_constructor_failure_keeps_existing_posix_lock_and_reaps_after_unlock() {
        let dir = directory();
        let path = dir.path().join("posix.db");
        drop(store(&path));
        let c = rusqlite::Connection::open(&path).unwrap();
        c.busy_timeout(std::time::Duration::ZERO).unwrap();
        c.execute_batch("BEGIN IMMEDIATE").unwrap();
        assert_eq!(contender(&path), 75);
        assert!(matches!(
            ProtectedStoreSlice::open_synthetic(&path, Default::default(), false, trust()),
            Err(Error::StorageBusy)
        ));
        assert_eq!(
            contender(&path),
            75,
            "a rejected owned fd must not close away the same-process POSIX lock"
        );
        c.execute_batch("ROLLBACK").unwrap();
        let mut admitted =
            ProtectedStoreSlice::open_synthetic(&path, Default::default(), false, trust()).unwrap();
        assert_eq!(contender(&path), 75);
        admitted.create_local("after-unlock", &card()).unwrap();
        drop(admitted);
        assert_eq!(contender(&path), 0);
    }
    #[test]
    fn store_process_crash_probe() {
        let Some(path) = std::env::var_os("MORROW_STORE_CRASH_PROBE") else {
            return;
        };
        let mut store = ProtectedStoreSlice::open_synthetic(
            Path::new(&path),
            Default::default(),
            false,
            trust(),
        )
        .unwrap();
        let _: Result<()> = store.database.store_transaction(|c, _| {
            // Force actual pager spill before real process exit. No fabricated
            // journal, Drop, rollback, committed malformed card, or fake ACK.
            for index in 0..64 {
                sql(c.execute(
                    "INSERT INTO cards(id,payload) VALUES(?1,zeroblob(131072))",
                    [format!("uncommitted-{index}")],
                ))?;
            }
            std::process::exit(86);
        });
        std::process::exit(87);
    }
    #[test]
    fn real_spilled_hot_journal_is_retained_and_refused_without_recovery() {
        let dir = directory();
        let path = dir.path().join("hot.db");
        drop(store(&path));
        let original = std::fs::read(&path).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "store::linux_protected::tests::store_process_crash_probe",
                "--nocapture",
            ])
            .env("MORROW_STORE_CRASH_PROBE", &path)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(86));
        let journal = dir.path().join("hot.db-journal");
        let main = std::fs::read(&path).unwrap();
        let held = std::fs::read(&journal).unwrap();
        assert_ne!(
            main, original,
            "real pager spill must modify the main before recovery"
        );
        assert!(held.len() > 512);
        assert!(
            held[..8].iter().any(|byte| *byte != 0),
            "real hot journal header"
        );
        assert!(
            ProtectedStoreSlice::open_synthetic(&path, Default::default(), false, trust()).is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), main);
        assert_eq!(std::fs::read(&journal).unwrap(), held);
        // Separately retain the existing stage14 general foundation recovery
        // behavior. This is not a Store slice recovery/admission capability.
        let mut recovery = GuardedSqliteConnection::open(&path, false).unwrap();
        assert_eq!(
            recovery
                .query_row("SELECT count(*) FROM cards", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(recovery);
        assert!(!journal.exists());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let mut reopened =
            ProtectedStoreSlice::open_synthetic(&path, Default::default(), false, trust()).unwrap();
        reopened.integrity_check().unwrap();
    }
    #[test]
    fn unsafe_metadata_panic_then_outer_restore_is_sticky_even_without_sqlite_io() {
        for case in 0..3 {
            let dir = directory();
            let path = dir.path().join("panic-guard.db");
            let mut store = store(&path);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut callback = || {
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))
                        .unwrap();
                    panic!("unsafe metadata callback");
                };
                let _: Result<()> = match case {
                    // Crate-private DEFERRED contract, no SQL and no pager lock.
                    0 => store.database.store_read(|_, _| callback()),
                    1 => store
                        .database
                        .store_transaction(|_, _| callback().map(GuardedStoreOutcome::Commit)),
                    _ => store
                        .create_authorized("abort", &card(), &mut callback)
                        .map(|_| ()),
                };
            }));
            assert!(result.is_err());
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            assert!(
                store.card("card").is_err(),
                "outer restoration must not revive case {case}"
            );
        }
    }
}
