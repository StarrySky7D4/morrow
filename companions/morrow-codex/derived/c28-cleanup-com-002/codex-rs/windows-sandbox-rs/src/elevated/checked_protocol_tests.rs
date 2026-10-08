use super::*;

#[test]
fn hello_requires_fixed_schema_and_nonzero_nonce() {
    assert!(
        !ControlHello {
            nonce: [0; 32],
            schema: CHECKED_CONTROL_SCHEMA
        }
        .valid()
    );
    assert!(
        !ControlHello {
            nonce: [1; 32],
            schema: [0; 32]
        }
        .valid()
    );
    assert!(
        ControlHello {
            nonce: [1; 32],
            schema: CHECKED_CONTROL_SCHEMA
        }
        .valid()
    );
}
#[test]
fn request_digest_and_arguments_are_checked() {
    let mut request = ControlRequest::new(1, [1; 32], ControlOperation::Resize, 25, 90);
    assert!(request.valid([1; 32]));
    request.cols = 91;
    assert!(!request.valid([1; 32]));
    assert!(!ControlRequest::new(1, [1; 32], ControlOperation::CloseStdin, 1, 0).valid([1; 32]));
    assert!(!ControlRequest::new(1, [1; 32], ControlOperation::Resize, 0, 90).valid([1; 32]));
}
#[test]
fn duplicate_unknown_never_reapplies_and_mismatch_is_zero_effect() {
    let mut ledger = ControlLedger::new([1; 32]);
    let request = ControlRequest::new(1, [1; 32], ControlOperation::CloseStdin, 0, 0);
    assert!(matches!(ledger.admit(&request), ControlAdmission::Apply));
    assert!(matches!(
        ledger.admit(&request),
        ControlAdmission::Observe(ControlResult {
            status: ControlStatus::Unknown,
            ..
        })
    ));
    let mismatch = ControlRequest::new(1, [1; 32], ControlOperation::Resize, 24, 80);
    assert!(matches!(
        ledger.admit(&mismatch),
        ControlAdmission::Observe(ControlResult {
            status: ControlStatus::Rejected,
            ..
        })
    ));
    assert!(ledger.finish(ControlResult::new(
        request.clone(),
        ControlStatus::Applied,
        None
    )));
    assert!(matches!(
        ledger.admit(&request),
        ControlAdmission::Observe(ControlResult {
            status: ControlStatus::Applied,
            ..
        })
    ));
}
#[test]
fn bound_never_evicts_unknown_or_accepts_more_ids() {
    let mut ledger = ControlLedger::new([1; 32]);
    for id in 1..=MAX_CONTROL_IDS as u64 {
        assert!(matches!(
            ledger.admit(&ControlRequest::new(
                id,
                [1; 32],
                ControlOperation::Resize,
                24,
                80
            )),
            ControlAdmission::Apply
        ));
    }
    assert!(matches!(
        ledger.admit(&ControlRequest::new(
            MAX_CONTROL_IDS as u64 + 1,
            [1; 32],
            ControlOperation::Resize,
            24,
            80
        )),
        ControlAdmission::Observe(ControlResult {
            status: ControlStatus::Rejected,
            ..
        })
    ));
    assert!(matches!(
        ledger.admit(&ControlRequest::new(
            1,
            [1; 32],
            ControlOperation::Resize,
            24,
            80
        )),
        ControlAdmission::Observe(ControlResult {
            status: ControlStatus::Unknown,
            ..
        })
    ));
}
