//! Actual interpreter with synthetic WAT. No Store/Manager or operation approval.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_blob_upload_proposal_v1::{
    BOUND_INPUT_TYPE, MAX_BOUND_INPUT_BYTES, MAX_PROPOSAL_BYTES, OUTPUT_TYPE, Proposal,
};
use morrow_core::{
    plugin_package::{Package, blob_upload_proposal, proto},
    task::{FailureCode, Invocation, PluginFailure, Transform},
};
use morrow_plugin_runtime::{
    Cancellation, Fault, Limits,
    package::{PreparedPackage, ProposalReport},
};
use std::time::Instant;
fn invocation(name: &str) -> Invocation {
    Invocation::new_transform(
        name,
        Transform {
            handler: "blob.plan".into(),
            input_type: "sample.options.v1".into(),
            output_type: OUTPUT_TYPE.into(),
            input: vec![7],
        },
    )
    .unwrap()
}
fn proposal() -> Proposal {
    Proposal::new("Synthetic producer", 7, [9; 32], b"options").unwrap()
}
fn module(completion: &[u8], before: &str, after: &str) -> Vec<u8> {
    let data: String = completion.iter().map(|b| format!("\\{b:02x}")).collect();
    wat::parse_str(format!(
        r#"(module
      (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
      (import "morrow_task_v1" "complete" (func $complete (param i32 i32) (result i32)))
      (import "morrow_blob_upload_v1" "call" (func $upload (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 4)
      (data (i32.const 131072) "{data}")
      (func (export "morrow_run") (result i32)
        i32.const 0 i32.const 131072 call $read drop
        {before}
        i32.const 131072 i32.const {length} call $complete drop
        {after}
        i32.const 0))"#,
        length = completion.len()
    ))
    .unwrap()
}
fn manifest(wasm: &[u8]) -> proto::Manifest {
    Package::manifest_for_blob_upload_proposals(
        "org.example.plan",
        "1.0.0",
        wasm,
        vec![
            proto::TransformHandler {
                handler: "blob.plan".into(),
                input_type: "sample.options.v1".into(),
                output_type: OUTPUT_TYPE.into(),
                max_input_bytes: 1024,
                max_output_bytes: MAX_PROPOSAL_BYTES as u32,
            },
            proto::TransformHandler {
                handler: "blob.upload".into(),
                input_type: BOUND_INPUT_TYPE.into(),
                output_type: "upload.result.v1".into(),
                max_input_bytes: MAX_BOUND_INPUT_BYTES as u32,
                max_output_bytes: 4096,
            },
        ],
        vec!["blob.upload".into()],
        vec![proto::BlobUploadProposalHandler {
            handler: "blob.plan".into(),
            upload_handler: "blob.upload".into(),
        }],
    )
}
fn prepared(wasm: &[u8]) -> PreparedPackage {
    PreparedPackage::new(
        Package::build(manifest(wasm), wasm).unwrap(),
        Limits::default(),
    )
    .unwrap()
}
fn rejected(report: ProposalReport, expected: Fault) {
    assert_eq!(report.execution.outcome, Err(expected));
    assert!(report.proposal.is_none());
    assert!(report.failure.is_none());
}
#[test]
fn actual_pure_plan_returns_correlated_data_without_any_host_owner() {
    let input = invocation("plan-1");
    let p = proposal();
    let wasm = module(&input.output_completion(p.wire()).unwrap(), "", "");
    let report = prepared(&wasm).run_blob_upload_proposal(&input, Cancellation::default());
    assert_eq!(report.execution.outcome, Ok(0));
    assert_eq!(report.execution.host_calls, 0);
    assert_eq!(report.proposal, Some(p));
    assert!(report.failure.is_none());
}
#[test]
fn successful_completion_cannot_hide_an_upload_effect_import() {
    let input = invocation("plan-1");
    let completion = input.output_completion(proposal().wire()).unwrap();
    let call = "i32.const 0 i32.const 1 i32.const 65536 i32.const 4096 call $upload drop";
    for (before, after) in [(call, ""), ("", call)] {
        let report = prepared(&module(&completion, before, after))
            .run_blob_upload_proposal(&input, Cancellation::default());
        // The first variant reaches the suspended import; the second is denied
        // by the already-completed-task guard before it can create a pending call.
        assert_eq!(report.execution.host_calls, u32::from(!before.is_empty()));
        rejected(report, Fault::TaskProtocol);
    }
}
#[test]
fn pure_plan_cannot_accept_a_foreign_task_completion() {
    let wrong = invocation("foreign");
    let wasm = module(&wrong.output_completion(proposal().wire()).unwrap(), "", "");
    rejected(
        prepared(&wasm).run_blob_upload_proposal(&invocation("plan-1"), Cancellation::default()),
        Fault::TaskProtocol,
    );
}
#[test]
fn malformed_and_over_declared_object_outputs_fail() {
    let input = invocation("plan-1");
    let wasm = module(&input.output_completion(b"not a proposal").unwrap(), "", "");
    rejected(
        prepared(&wasm).run_blob_upload_proposal(&input, Cancellation::default()),
        Fault::TaskProtocol,
    );
    let wasm = module(&input.output_completion(proposal().wire()).unwrap(), "", "");
    let mut m = manifest(&wasm);
    m.blob_upload_declaration.as_mut().unwrap().max_object_bytes = 6;
    let p = PreparedPackage::new(Package::build(m, &wasm).unwrap(), Limits::default()).unwrap();
    rejected(
        p.run_blob_upload_proposal(&input, Cancellation::default()),
        Fault::TaskProtocol,
    );
}
#[test]
fn output_registration_ceiling_is_enforced_after_interpreter_returns() {
    let input = invocation("plan-1");
    let wasm = module(&input.output_completion(proposal().wire()).unwrap(), "", "");
    let mut m = manifest(&wasm);
    m.transform_handlers[0].max_output_bytes = 1;
    let p = PreparedPackage::new(Package::build(m, &wasm).unwrap(), Limits::default()).unwrap();
    rejected(
        p.run_blob_upload_proposal(&input, Cancellation::default()),
        Fault::TaskProtocol,
    );
}
#[test]
fn wrong_handler_or_input_type_never_enters_the_guest() {
    let input = invocation("plan-1");
    let wasm = module(
        &input.output_completion(proposal().wire()).unwrap(),
        "unreachable",
        "",
    );
    let p = prepared(&wasm);
    for field in 0..4 {
        let mut t = input.transform().unwrap().clone();
        match field {
            0 => t.handler = "blob.upload".into(),
            1 => t.input_type = "other".into(),
            2 => t.output_type = "other".into(),
            _ => t.input = vec![7; 1025],
        }
        let input = Invocation::new_transform("plan-1", t).unwrap();
        let report = p.run_blob_upload_proposal(&input, Cancellation::default());
        assert_eq!(report.execution.fuel_remaining, p.limits().fuel);
        rejected(report, Fault::TaskProtocol);
    }
}
#[test]
fn cancellation_and_deadline_refuse_without_guest_entry() {
    let input = invocation("plan-1");
    let wasm = module(
        &input.output_completion(proposal().wire()).unwrap(),
        "unreachable",
        "",
    );
    let p = prepared(&wasm);
    let cancel = Cancellation::default();
    cancel.cancel();
    rejected(p.run_blob_upload_proposal(&input, cancel), Fault::Cancelled);
    rejected(
        p.run_blob_upload_proposal(&input, Cancellation::until(Instant::now())),
        Fault::Deadline,
    );
}
#[test]
fn runaway_planner_exhausts_fixed_fuel_and_reports_no_proposal() {
    let input = invocation("plan-1");
    let wasm = module(
        &input.output_completion(proposal().wire()).unwrap(),
        "(loop $again br $again)",
        "",
    );
    let limits = Limits {
        fuel: 1000,
        ..Limits::default()
    };
    let p = PreparedPackage::new(Package::build(manifest(&wasm), &wasm).unwrap(), limits).unwrap();
    rejected(
        p.run_blob_upload_proposal(&input, Cancellation::default()),
        Fault::Limits,
    );
}
#[test]
fn structured_plugin_failure_is_untrusted_data_not_a_proposal() {
    let input = invocation("plan-1");
    let failure = PluginFailure {
        code: FailureCode::InvalidInput,
        message: "Choose a smaller sample".into(),
    };
    let wasm = module(&input.failure_completion(&failure).unwrap(), "", "");
    let report = prepared(&wasm).run_blob_upload_proposal(&input, Cancellation::default());
    assert_eq!(report.execution.outcome, Ok(0));
    assert!(report.proposal.is_none());
    assert_eq!(report.failure, Some(failure));
}
#[test]
fn package_contract_constants_match_shared_codec() {
    assert_eq!(
        blob_upload_proposal::schema_digest(),
        morrow_blob_upload_proposal_v1::SCHEMA_DIGEST
    );
    assert_eq!(blob_upload_proposal::OUTPUT_TYPE, OUTPUT_TYPE);
    assert_eq!(blob_upload_proposal::BOUND_INPUT_TYPE, BOUND_INPUT_TYPE);
    assert_eq!(
        blob_upload_proposal::MAX_PROPOSAL_BYTES as usize,
        MAX_PROPOSAL_BYTES
    );
    assert_eq!(
        blob_upload_proposal::MAX_BOUND_INPUT_BYTES as usize,
        MAX_BOUND_INPUT_BYTES
    );
}
