//! Real Windows broker, managed approval, original quotas, Store and native TxF.
#![cfg(all(windows, feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    file_effect::ReplaceResult,
    file_mutation::{Disposition, RequestRecord},
    io_intent::Phase,
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    store::Store,
};
use morrow_plugin_runtime::{
    Limits,
    file_target::{Error, SelectionScope, TargetBroker, TargetControl},
    io_binding::{Error as AdmissionError, IoBinding},
    manager::{ManagedInstance, Manager},
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::PathBuf};

const ID: &str = "org.example.conditional-replace";
const SUBJECT: &str = "conditional.replace-test";
struct Fixture {
    dir: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    instance: ManagedInstance,
    binding: IoBinding,
    broker: TargetBroker,
    path: PathBuf,
}
impl Fixture {
    fn new(resources: u32) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let wasm = wat::parse_str(
            r#"(module
            (import "morrow_task_v1" "read_input" (func (param i32 i32) (result i32)))
            (import "morrow_task_v1" "complete" (func (param i32 i32) (result i32)))
            (import "morrow_io_v1" "call" (func (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 4)
            (func (export "morrow_run") (result i32) i32.const 0))"#,
        )
        .unwrap();
        let caps = BTreeSet::from([IoCapability::FileReplace]);
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        let mut declaration = io::declaration(
            caps.iter().copied().collect(),
            vec!["file.replace-selected".into()],
        );
        let budget = declaration.budget.as_mut().unwrap();
        budget.max_resources = resources;
        budget.max_jobs = 4;
        budget.max_bytes = 4 * 1024 * 1024;
        budget.max_job_bytes = 1024 * 1024;
        budget.max_duration_ms = 30_000;
        manifest.io_declaration = Some(declaration);
        manifest.required_features.push(io::FEATURE.into());
        let package = Package::build(manifest, &wasm).unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(ID, package.digest(), caps.clone(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap())
                .unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
        let binding = manager
            .bind_io(
                &host,
                &instance,
                package.digest(),
                manager.revision(),
                &caps,
                30_000,
                1,
            )
            .unwrap();
        let parent = dir.path().join("selected-root");
        fs::create_dir(&parent).unwrap();
        let path = parent.join("selected.bin");
        fs::write(&path, b"original bytes").unwrap();
        Self {
            dir,
            manager,
            host,
            instance,
            binding,
            broker: TargetBroker::new([0x91; 32]).unwrap(),
            path,
        }
    }
    fn select(&mut self, bytes: &[u8]) -> Result<RequestRecord, Error> {
        let target = self.broker.select_existing(
            &self.manager,
            &self.host,
            &self.instance,
            &self.binding,
            &self.path,
            SelectionScope {
                subject: SUBJECT.into(),
                approval_sha256: [0x92; 32],
                disposition: Disposition::Replace,
            },
            || 2,
        )?;
        self.broker.build_request_controlled(
            &self.manager,
            &self.host,
            &self.instance,
            target.reference,
            "replace-original".into(),
            bytes.len() as u64,
            Some(Sha256::digest(bytes).into()),
            &mut Control {
                now: 3,
                cancelled: false,
            },
        )
    }
    fn prepare(&mut self, request: &RequestRecord, bytes: &[u8]) {
        self.broker
            .prepare_request(&self.manager, &mut self.host, &self.instance, request, || 4)
            .unwrap();
        self.broker
            .stage_content(
                &self.manager,
                &mut self.host,
                &self.instance,
                request,
                bytes,
                || 5,
            )
            .unwrap();
    }
    fn phase(&self) -> Phase {
        self.host
            .store_local()
            .lookup_io_intent(SUBJECT, "replace-original")
            .unwrap()
            .unwrap()
            .phase()
    }
}
struct Control {
    now: u64,
    cancelled: bool,
}
impl TargetControl for Control {
    fn with<T>(&mut self, action: impl FnOnce(u64, bool) -> T) -> T {
        action(self.now, self.cancelled)
    }
}

#[test]
fn original_selected_transaction_replaces_exact_bytes_once_and_records_durable_outcome() {
    for bytes in [b"".as_slice(), b"\0\xffnew\r\n  bytes\0".as_slice()] {
        let mut f = Fixture::new(8);
        let alias = f.dir.path().join("original-alias.bin");
        fs::hard_link(&f.path, &alias).unwrap();
        let plan = f.select(bytes).unwrap();
        assert!(f.binding.usage().resources > 3);
        assert_eq!(fs::read(&f.path).unwrap_err().raw_os_error(), Some(32));
        assert!(fs::write(&f.path, b"outside writer").is_err());
        assert!(fs::rename(f.path.parent().unwrap(), f.dir.path().join("moved")).is_err());
        f.prepare(&plan, bytes);
        assert_eq!(f.phase(), Phase::Prepared);
        let outcome = f
            .broker
            .replace(&f.manager, &mut f.host, &f.instance, &plan, || 6)
            .unwrap();
        assert_eq!(outcome.result(), ReplaceResult::Replaced);
        assert_eq!(fs::read(&f.path).unwrap(), bytes);
        assert_eq!(fs::read(&alias).unwrap(), b"original bytes");
        assert_eq!(f.phase(), Phase::Observed);
        assert_eq!(f.binding.usage().resources, 0);
        assert_eq!(f.binding.usage().jobs, 0);
        assert!(f.binding.usage().bytes > bytes.len() as u64);
        assert!(matches!(
            f.broker
                .replace(&f.manager, &mut f.host, &f.instance, &plan, || 7),
            Err(Error::AlreadyDispatched)
        ));
        assert_eq!(fs::read(&f.path).unwrap(), bytes);
        fs::rename(f.path.parent().unwrap(), f.dir.path().join("moved")).unwrap();
    }
}

#[test]
fn whole_native_group_must_fit_the_original_budget_before_any_open() {
    let mut f = Fixture::new(1);
    assert!(matches!(
        f.select(b"new"),
        Err(Error::Admission(AdmissionError::Limit))
    ));
    assert_eq!(f.broker.len(), 0);
    assert_eq!(f.binding.usage().resources, 0);
    assert_eq!(f.binding.usage().jobs, 0);
    assert_eq!(f.binding.usage().bytes, 0);
    fs::write(&f.path, b"still available").unwrap();
    fs::rename(f.path.parent().unwrap(), f.dir.path().join("moved")).unwrap();
}

#[test]
fn missing_staging_and_cancelled_dispatch_keep_prepared_original_and_return_capacity_on_release() {
    let mut f = Fixture::new(8);
    let plan = f.select(b"new").unwrap();
    f.broker
        .prepare_request(&f.manager, &mut f.host, &f.instance, &plan, || 4)
        .unwrap();
    assert!(matches!(
        f.broker
            .replace(&f.manager, &mut f.host, &f.instance, &plan, || 5),
        Err(Error::Persistence(morrow_core::Error::EvidenceUnavailable))
    ));
    assert_eq!(f.phase(), Phase::Prepared);
    assert_eq!(f.binding.usage().jobs, 0);
    f.broker
        .stage_content(&f.manager, &mut f.host, &f.instance, &plan, b"new", || 6)
        .unwrap();
    assert!(matches!(
        f.broker.replace_controlled(
            &f.manager,
            &mut f.host,
            &f.instance,
            &plan,
            &mut Control {
                now: 7,
                cancelled: true
            }
        ),
        Err(Error::CancelledBeforeDispatch)
    ));
    assert_eq!(f.phase(), Phase::Prepared);
    f.broker
        .release(
            &f.manager,
            &f.host,
            &f.instance,
            plan.request().target.reference,
            8,
        )
        .unwrap();
    assert_eq!(f.binding.usage().resources, 0);
    assert_eq!(fs::read(&f.path).unwrap(), b"original bytes");
}

#[test]
fn existing_temporary_is_never_adopted_and_os_rejection_is_observed_once() {
    let mut f = Fixture::new(8);
    let plan = f.select(b"new").unwrap();
    let name = format!(
        ".morrow-replace-{}.tmp",
        plan.command()
            .unwrap()
            .request_sha256
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let temporary = f.path.parent().unwrap().join(name);
    fs::write(&temporary, b"must survive").unwrap();
    f.prepare(&plan, b"new");
    let outcome = f
        .broker
        .replace(&f.manager, &mut f.host, &f.instance, &plan, || 6)
        .unwrap();
    assert!(matches!(outcome.result(),ReplaceResult::OsRejected{code} if code!=0));
    assert_eq!(f.phase(), Phase::Observed);
    assert_eq!(fs::read(&temporary).unwrap(), b"must survive");
    assert_eq!(fs::read(&f.path).unwrap(), b"original bytes");
    assert_eq!(f.binding.usage().resources, 0);
    assert!(matches!(
        f.broker
            .replace(&f.manager, &mut f.host, &f.instance, &plan, || 7),
        Err(Error::AlreadyDispatched)
    ));
}

#[test]
fn expiry_reaps_the_original_transaction_and_ancestry_without_file_effects() {
    let mut f = Fixture::new(8);
    let _ = f.select(b"new").unwrap();
    assert!(f.binding.usage().resources > 3);
    f.broker.reap(30_000);
    assert_eq!(f.broker.len(), 0);
    assert_eq!(f.binding.usage().resources, 0);
    assert_eq!(fs::read(&f.path).unwrap(), b"original bytes");
    fs::rename(f.path.parent().unwrap(), f.dir.path().join("moved")).unwrap();
}

#[test]
fn deep_ancestry_is_rejected_without_bypassing_the_original_eight_resource_ceiling() {
    let mut f = Fixture::new(8);
    let deep = f.path.parent().unwrap().join("one/two/three");
    fs::create_dir_all(&deep).unwrap();
    f.path = deep.join("selected.bin");
    fs::write(&f.path, b"deep original").unwrap();
    assert!(matches!(
        f.select(b"new"),
        Err(Error::Admission(AdmissionError::Limit))
    ));
    assert_eq!(f.broker.len(), 0);
    assert_eq!(f.binding.usage().resources, 0);
    assert_eq!(f.binding.usage().jobs, 0);
    assert_eq!(f.binding.usage().bytes, 0);
    assert_eq!(fs::read(&f.path).unwrap(), b"deep original");
    fs::rename(&deep, deep.with_file_name("moved")).unwrap();
}
