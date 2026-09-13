//! Real Rust pure-task evidence linked by an atomic content commit and a synthetic audit signature.
//! The fixed qualification key is not a production key. No default UI capture or host projection replay.
use morrow_core::{
    audit::{self, SigningKey, TrustedLog},
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::{
        Package,
        catalog::Catalog,
        proto::{Capability, TransformHandler},
        registry::Registry,
    },
    store::Store,
    task::{Invocation, Transform},
    task_evidence::{self, Evidence},
    transaction,
};
use morrow_plugin_runtime::{Limits, instance_pool::Pool, manager::Manager, replay};
const ID: &str = "org.test.committed-evidence.rust-transform";
const CARD: &str = "qualified-card";
const OP: &str = "qualified-create";
const EXPECTED: &[u8] = b"CBA";
fn verify_commit(raw: &[u8], evidence: &Evidence) -> Result<(), Box<dyn std::error::Error>> {
    let (commit, receipt) = transaction::decode_commit(raw)?;
    assert_eq!(commit.schema_version, 2);
    assert_eq!(receipt.operation_id, OP);
    assert_eq!(receipt.card_id, CARD);
    assert_eq!(receipt.revision, 1);
    // The field below is the ordered digest list in the schema-2 immutable commit.
    assert_eq!(
        commit.task_evidence_sha256,
        vec![evidence.digest().to_vec()]
    );
    Ok(())
}
fn verify_replay(evidence: &Evidence) -> Result<(), Box<dyn std::error::Error>> {
    let decoded = task_evidence::decode(evidence.container(), evidence.digest())?;
    let result = replay::replay(&decoded, Limits::default())?;
    assert!(result.matches);
    assert_eq!(result.report.execution.outcome, Ok(0));
    assert_eq!(result.report.execution.host_calls, 0);
    assert!(result.report.response.is_none() && result.report.failure.is_none());
    assert_eq!(result.report.output.unwrap().bytes, EXPECTED);
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 {
        return Err("usage: qualify_committed_evidence ACTUAL_RUST_TRANSFORM.wasm".into());
    }
    let directory = tempfile::tempdir()?;
    let snapshot_directory = tempfile::tempdir()?;
    let database = directory.path().join("synthetic.db");
    let snapshot = snapshot_directory.path().join("snapshot.db");
    // Publicly fixed test material; establishes only this synthetic qualification's pinned identity.
    let key = SigningKey::from_bytes(&[35; 32]);
    let trust = TrustedLog {
        id: "committed-evidence-qualification".into(),
        key: key.verifying_key(),
    };
    let (original_commit, signed, original_evidence) = {
        let module = std::fs::read(&args[0])?;
        let mut manifest = Package::manifest_for_transform(
            ID,
            "1.0.0",
            &module,
            vec![TransformHandler {
                handler: "bytes.reverse".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                max_input_bytes: 65536,
                max_output_bytes: 65536,
            }],
        );
        manifest.requested_capabilities = vec![Capability::CreateContent as i32];
        let package = Package::build(manifest, &module)?;
        let catalog = Catalog::open(&directory.path().join("packages"))?;
        catalog.install(&package)?;
        let mut manager = Manager::new(
            Registry::open(&directory.path().join("registry"), catalog)?,
            Limits::default(),
        );
        manager.select(&package, manager.revision())?;
        manager.approve(
            ID,
            package.digest(),
            [GrantKind::CreateContent].into(),
            manager.revision(),
        )?;
        manager.set_enabled(ID, package.digest(), true, manager.revision())?;
        let mut host = HostRuntime::new(Store::open_audited(
            &database,
            Default::default(),
            true,
            trust.clone(),
        )?)?;
        let mut pool = Pool::new(&host, Default::default())?;
        let revision = manager.revision();
        let session = pool.start(&mut manager, &mut host, ID, &[], revision)?;
        let input = Invocation::new_transform(
            "committed-reverse",
            Transform {
                handler: "bytes.reverse".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                input: b"ABC".to_vec(),
            },
        )?;
        let capture = pool.record_transform(&manager, &mut host, &session, &input)?;
        assert_eq!(capture.report().execution.outcome, Ok(0));
        assert_eq!(capture.report().execution.host_calls, 0);
        assert_eq!(capture.report().output.as_ref().unwrap().bytes, EXPECTED);
        let (_, evidence) = capture.into_parts();
        let card = CardRecord::new(
            CARD,
            "bytes",
            1,
            "Qualified pure transform",
            EXPECTED.to_vec(),
        )?;
        assert!(
            host.create_content_with_evidence(
                pool.root(&session)?.connection(),
                OP,
                &card,
                std::slice::from_ref(&evidence),
                || 1
            )
            .is_err(),
            "capture cannot grant content authority"
        );
        assert!(host.store_local().card(CARD)?.is_none());
        assert!(host.store_local().pending(0, 1)?.is_empty());
        assert!(matches!(
            host.store_local().operation_evidence(CARD, OP),
            Err(morrow_core::Error::NotFound)
        ));
        host.store_local().integrity_check()?;
        pool.grant_root(&mut host, &session, GrantKind::CreateContent, CARD, 100, 1)?;
        let receipt = host.create_content_with_evidence(
            pool.root(&session)?.connection(),
            OP,
            &card,
            std::slice::from_ref(&evidence),
            || 2,
        )?;
        assert_eq!(receipt.revision, 1);
        let retry = host.create_content_with_evidence(
            pool.root(&session)?.connection(),
            OP,
            &card,
            std::slice::from_ref(&evidence),
            || 3,
        )?;
        assert_eq!(
            retry, receipt,
            "same evidence retry returns the original receipt"
        );
        let pending = host.store_local().pending(0, 10)?;
        assert_eq!(pending.len(), 1);
        let original_commit = pending[0].1.clone();
        verify_commit(&original_commit, &evidence)?;
        let segment = audit::from_pending(&trust, 1, [0; 32], &pending)?;
        let signed = audit::sign(&segment, &trust, &key)?;
        let verified = audit::verify(&signed, &trust)?;
        assert_eq!(verified.segment().events.len(), 1);
        assert_eq!(
            verified.segment().events[0].original_commit,
            original_commit
        );
        assert!(host.store_local_mut().seal_pending(&signed)?);
        assert!(host.store_local().pending(0, 10)?.is_empty());
        assert_eq!(host.store_local().sealed_segment(1)?.unwrap(), signed);
        host.store_local().integrity_check()?;
        pool.close(&mut host, &session)?;
        assert_eq!((pool.usage().sessions, pool.usage().providers), (0, 0));
        (original_commit, signed, evidence)
    }; // No original Pool, Session, Manager, HostRuntime or prepared guest survives.
    let recovered = {
        let store = Store::open_audited(&database, Default::default(), false, trust.clone())?;
        store.integrity_check()?;
        assert_eq!(store.card(CARD)?.unwrap().body(), EXPECTED);
        let evidence = store.operation_evidence(CARD, OP)?;
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].digest(), original_evidence.digest());
        assert_eq!(evidence[0].container(), original_evidence.container());
        let verified = audit::verify(&store.sealed_segment(1)?.unwrap(), &trust)?;
        assert_eq!(
            verified.segment().events[0].original_commit,
            original_commit
        );
        verify_commit(&verified.segment().events[0].original_commit, &evidence[0])?;
        store.snapshot_to(&snapshot, 128 * 1024 * 1024)?;
        evidence
    };
    directory.close()?; // Removes the original DB and catalog, never the independent snapshot.
    verify_replay(&recovered[0])?;
    let snapshot_evidence = {
        let store = Store::open_read_only_audited(&snapshot, trust.clone())?;
        store.integrity_check()?;
        let evidence = store.operation_evidence(CARD, OP)?;
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].container(), original_evidence.container());
        assert_eq!(store.sealed_segment(1)?.unwrap(), signed);
        let verified = audit::verify(&signed, &trust)?;
        verify_commit(&verified.segment().events[0].original_commit, &evidence[0])?;
        evidence
    };
    verify_replay(&snapshot_evidence[0])?;
    println!(
        "PASS_SCOPED actual Rust reverse: grant required; one schema-2 content commit + exact evidence; idempotent receipt; synthetic pinned signature covers original commit refs; post-host/catalog removal and read-only snapshot evidence both replay exactly."
    );
    println!(
        "LIMITS: public synthetic signing key; no production-key qualification, default UI capture, or host projection reconstruction."
    );
    Ok(())
}
