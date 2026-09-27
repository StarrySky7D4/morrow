//! Operation-local cancellation at each guarded preparation and staging sample.
use super::*;
use morrow_core::{file_mutation::RequestRecord, file_path::RelativeFilePath, io_intent::Phase};
use morrow_plugin_runtime::file_target::{Error as TargetError, SelectionScope, TargetControl};
use morrow_plugin_runtime::io_binding::Error as AdmissionError;
use std::path::Path;

const BYTES: &[u8] = b"controlled preparation and staging bytes";

struct CancelAt {
    calls: usize,
    cancel_at: Option<usize>,
}
impl CancelAt {
    fn running() -> Self {
        Self {
            calls: 0,
            cancel_at: None,
        }
    }
    fn at(sample: usize) -> Self {
        Self {
            calls: 0,
            cancel_at: Some(sample),
        }
    }
}
impl TargetControl for CancelAt {
    fn with<T>(&mut self, action: impl FnOnce(u64, bool) -> T) -> T {
        self.calls += 1;
        action(5, self.cancel_at.is_some_and(|sample| self.calls >= sample))
    }
}

fn another_operation(original: &RequestRecord, operation: &str) -> RequestRecord {
    let mut value = original.request().clone();
    value.operation_id = operation.into();
    RequestRecord::new(value).unwrap()
}

fn has_plan(f: &Fixture, request: &RequestRecord) -> bool {
    f.host
        .store_local()
        .lookup_io_intent(&request.request().subject, &request.request().operation_id)
        .unwrap()
        .is_some()
}

fn staged(f: &Fixture, request: &RequestRecord) -> bool {
    let value = request.request();
    let content = f
        .host
        .store_local()
        .file_mutation_content_local_authorized(&value.subject, &value.operation_id, || Ok(()))
        .unwrap();
    let receipt = f
        .host
        .store_local()
        .file_mutation_content_receipt_local_authorized(&value.subject, &value.operation_id, || {
            Ok(())
        })
        .unwrap();
    assert_eq!(content.is_some(), receipt.is_some());
    if let (Some(content), Some(receipt)) = (content.as_ref(), receipt.as_ref()) {
        assert_eq!(content.content(), BYTES);
        assert_eq!(receipt.content_sha256(), content.content_sha256());
        assert_eq!(receipt.content_length(), BYTES.len() as u64);
    }
    content.is_some()
}

#[test]
fn prepare_controlled_cancel_at_each_sample_has_one_commit_boundary() {
    let (mut baseline, instance, _binding, broker, original, leaf) =
        file_create::setup(BYTES, "prepare-baseline.bin", 3);
    let new_request = another_operation(&original, "controlled-prepare-baseline");
    let mut control = CancelAt::running();
    let record = broker
        .prepare_request_controlled(
            &baseline.manager,
            &mut baseline.host,
            &instance,
            &new_request,
            &mut control,
        )
        .unwrap();
    assert_eq!(record.phase(), Phase::Prepared);
    assert!(control.calls >= 5);
    let samples = control.calls;
    assert!(has_plan(&baseline, &new_request));
    assert!(!leaf.exists());
    baseline.host.store_local().integrity_check().unwrap();
    instance.close(&mut baseline.host).unwrap();

    for cancel_at in 1..=samples {
        let (mut f, instance, binding, broker, original, leaf) =
            file_create::setup(BYTES, "prepare-cancel.bin", 3);
        let request = another_operation(&original, &format!("controlled-prepare-{cancel_at}"));
        let pending = f.host.store_local().pending(0, 10).unwrap();
        let usage = binding.usage();
        let mut control = CancelAt::at(cancel_at);
        let result = broker.prepare_request_controlled(
            &f.manager,
            &mut f.host,
            &instance,
            &request,
            &mut control,
        );
        assert_eq!(control.calls, cancel_at);
        if cancel_at == samples {
            assert!(matches!(
                result,
                Err(TargetError::CommittedButDeliveryCancelled)
            ));
            assert!(has_plan(&f, &request));
            assert_eq!(
                f.host.store_local().pending(0, 10).unwrap().len(),
                pending.len() + 1
            );
            // The committed plan remains usable even though delivery was cancelled.
            // The shared fixture's stage helper samples 4; this operation has
            // already advanced the same binding clock to 5.
            let mut next = CancelAt::running();
            broker
                .stage_content_controlled(
                    &f.manager,
                    &mut f.host,
                    &instance,
                    &request,
                    BYTES,
                    &mut next,
                )
                .unwrap();
            assert!(staged(&f, &request));
        } else {
            assert!(
                matches!(result, Err(TargetError::CancelledBeforeDispatch)),
                "{cancel_at}"
            );
            assert!(!has_plan(&f, &request));
            assert_eq!(f.host.store_local().pending(0, 10).unwrap(), pending);
            assert_eq!(binding.usage().resources, usage.resources);
        }
        assert!(!leaf.exists());
        f.host.store_local().integrity_check().unwrap();
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn stage_controlled_cancel_at_each_sample_rolls_back_or_delivers_committed_bytes() {
    let (mut baseline, instance, _binding, broker, request, leaf) =
        file_create::setup(BYTES, "stage-baseline.bin", 3);
    let mut control = CancelAt::running();
    broker
        .stage_content_controlled(
            &baseline.manager,
            &mut baseline.host,
            &instance,
            &request,
            BYTES,
            &mut control,
        )
        .unwrap();
    assert!(control.calls >= 5);
    let samples = control.calls;
    assert!(staged(&baseline, &request));
    assert!(!leaf.exists());
    baseline.host.store_local().integrity_check().unwrap();
    instance.close(&mut baseline.host).unwrap();

    for cancel_at in 1..=samples {
        let (mut f, instance, binding, broker, request, leaf) =
            file_create::setup(BYTES, "stage-cancel.bin", 3);
        let pending = f.host.store_local().pending(0, 10).unwrap();
        let usage = binding.usage();
        let mut control = CancelAt::at(cancel_at);
        let result = broker.stage_content_controlled(
            &f.manager,
            &mut f.host,
            &instance,
            &request,
            BYTES,
            &mut control,
        );
        assert_eq!(control.calls, cancel_at);
        assert_eq!(file_create::phase(&f), Phase::Prepared);
        if cancel_at == samples {
            assert!(matches!(
                result,
                Err(TargetError::CommittedButDeliveryCancelled)
            ));
            assert!(staged(&f, &request));
            assert_eq!(
                f.host.store_local().pending(0, 10).unwrap().len(),
                pending.len() + 1
            );
        } else {
            assert!(
                matches!(result, Err(TargetError::CancelledBeforeDispatch)),
                "{cancel_at}"
            );
            assert!(!staged(&f, &request));
            assert_eq!(f.host.store_local().pending(0, 10).unwrap(), pending);
            assert_eq!(binding.usage().resources, usage.resources);
        }
        assert!(!leaf.exists());
        f.host.store_local().integrity_check().unwrap();
        instance.close(&mut f.host).unwrap();
    }
}

fn select_again(
    broker: &mut morrow_plugin_runtime::file_target::TargetBroker,
    f: &Fixture,
    instance: &ManagedInstance,
    binding: &IoBinding,
    request: &RequestRecord,
    root: &Path,
    control: &mut impl TargetControl,
) -> std::result::Result<morrow_plugin_runtime::file_target::SelectedCreateTarget, TargetError> {
    let relative = RelativeFilePath::parse("a/b/second-selection.bin").unwrap();
    broker.select_create_controlled(
        &f.manager,
        &f.host,
        instance,
        binding,
        root,
        &relative,
        SelectionScope {
            subject: request.request().subject.clone(),
            approval_sha256: request.request().approval_sha256,
            disposition: request.request().disposition,
        },
        control,
    )
}

#[test]
fn create_selection_cancellation_releases_every_partially_admitted_resource() {
    let (mut baseline, instance, binding, mut broker, request, leaf) =
        file_create::setup(BYTES, "selection-baseline.bin", 4);
    let root = leaf.parent().unwrap();
    std::fs::create_dir_all(root.join("a").join("b")).unwrap();
    let mut control = CancelAt::running();
    select_again(
        &mut broker,
        &baseline,
        &instance,
        &binding,
        &request,
        root,
        &mut control,
    )
    .unwrap();
    let samples = control.calls;
    assert!(samples >= 10);
    assert_eq!(broker.len(), 2);
    assert_eq!(binding.usage().resources, 4);
    instance.close(&mut baseline.host).unwrap();

    for cancel_at in 1..=samples {
        let (mut f, instance, binding, mut broker, request, leaf) =
            file_create::setup(BYTES, "selection-cancel.bin", 4);
        let root = leaf.parent().unwrap();
        std::fs::create_dir_all(root.join("a").join("b")).unwrap();
        let before = binding.usage().resources;
        let mut control = CancelAt::at(cancel_at);
        let result = select_again(
            &mut broker,
            &f,
            &instance,
            &binding,
            &request,
            root,
            &mut control,
        );
        assert!(
            matches!(result, Err(TargetError::CancelledBeforeDispatch)),
            "{cancel_at}"
        );
        assert_eq!(control.calls, cancel_at);
        assert_eq!(broker.len(), 1);
        assert_eq!(binding.usage().resources, before);
        assert!(!leaf.exists());
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn cancelled_foreign_owner_cannot_skip_create_selection_or_validation_preflight() {
    let (mut f, owner, binding, mut broker, request, leaf) =
        file_create::setup(BYTES, "foreign-owner.bin", 4);
    let foreign = f.connect();
    let root = leaf.parent().unwrap();
    std::fs::create_dir_all(root.join("a").join("b")).unwrap();
    let before = binding.usage().resources;

    let mut selection_control = CancelAt::at(1);
    assert_eq!(
        select_again(
            &mut broker,
            &f,
            &foreign,
            &binding,
            &request,
            root,
            &mut selection_control,
        ),
        Err(TargetError::Admission(AdmissionError::Denied))
    );
    assert_eq!(selection_control.calls, 1);
    assert_eq!(broker.len(), 1);
    assert_eq!(binding.usage().resources, before);

    let mut validation_control = CancelAt::at(1);
    assert_eq!(
        broker.validate_request_controlled(
            &f.manager,
            &f.host,
            &foreign,
            &request,
            &mut validation_control,
        ),
        Err(TargetError::Admission(AdmissionError::Denied))
    );
    assert_eq!(validation_control.calls, 1);
    assert_eq!(broker.len(), 1);
    assert_eq!(binding.usage().resources, before);
    assert!(!leaf.exists());

    let mut owner_control = CancelAt::running();
    broker
        .validate_request_controlled(&f.manager, &f.host, &owner, &request, &mut owner_control)
        .unwrap();
    foreign.close(&mut f.host).unwrap();
    owner.close(&mut f.host).unwrap();
}
