#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    plugin_package::{Package, blob_upload_proposal as p, proto},
    task::Transform,
};
use prost::Message;
const MODULE: &[u8] = b"\0asm\x01\0\0\0";
fn manifest() -> proto::Manifest {
    Package::manifest_for_blob_upload_proposals(
        "org.example.plan",
        "1.0.0",
        MODULE,
        vec![
            proto::TransformHandler {
                handler: "blob.plan".into(),
                input_type: "sample.options.v1".into(),
                output_type: p::OUTPUT_TYPE.into(),
                max_input_bytes: 1024,
                max_output_bytes: p::MAX_PROPOSAL_BYTES,
            },
            proto::TransformHandler {
                handler: "blob.upload".into(),
                input_type: p::BOUND_INPUT_TYPE.into(),
                output_type: "upload.result.v1".into(),
                max_input_bytes: p::MAX_BOUND_INPUT_BYTES,
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
fn input() -> Transform {
    Transform {
        handler: "blob.plan".into(),
        input_type: "sample.options.v1".into(),
        output_type: p::OUTPUT_TYPE.into(),
        input: vec![1],
    }
}
#[test]
fn proposal_mapping_round_trip_keeps_upload_and_pure_routes_separate() {
    let m = manifest();
    let package = Package::build(m.clone(), MODULE).unwrap();
    assert_eq!(Package::decode(package.archive()).unwrap().manifest(), &m);
    let (registration, mapping) = package.blob_upload_proposal_handler(&input()).unwrap();
    assert_eq!(registration, &m.transform_handlers[0]);
    assert_eq!(mapping.upload_handler, "blob.upload");
    assert!(package.transform_handler(&input()).is_err());
    assert!(package.blob_upload_handler(&input()).is_err());
    let stage = Transform {
        handler: "blob.upload".into(),
        input_type: p::BOUND_INPUT_TYPE.into(),
        output_type: "upload.result.v1".into(),
        input: vec![],
    };
    assert!(package.blob_upload_handler(&stage).is_ok());
    assert!(package.blob_upload_proposal_handler(&stage).is_err());
    assert!(package.capabilities().is_empty() && package.io_capabilities().is_empty());
}
#[test]
fn declaration_requires_upload_profile_feature_version_and_exact_schema() {
    for case in 0..7 {
        let mut m = manifest();
        match case {
            0 => m.required_features.retain(|f| f != p::FEATURE),
            1 => m.blob_upload_proposal_declaration = None,
            2 => m.blob_upload_declaration = None,
            3 => {
                m.blob_upload_proposal_declaration
                    .as_mut()
                    .unwrap()
                    .schema_version = 2
            }
            4 => {
                m.blob_upload_proposal_declaration
                    .as_mut()
                    .unwrap()
                    .schema_sha256[0] ^= 1
            }
            5 => m.required_features.push(p::FEATURE.into()),
            _ => m.required_features.push("dependencies-v1".into()),
        }
        assert!(Package::build(m, MODULE).is_err(), "case {case}");
    }
}
#[test]
fn cross_package_missing_duplicate_overlap_and_uncovered_handlers_refuse() {
    for case in 0..7 {
        let mut m = manifest();
        let declaration = m.blob_upload_proposal_declaration.as_mut().unwrap();
        match case {
            0 => declaration.handlers[0].upload_handler = "org.foreign.upload".into(),
            1 => declaration.handlers[0].handler = "absent".into(),
            2 => declaration.handlers.push(declaration.handlers[0].clone()),
            3 => declaration.handlers[0].handler = "blob.upload".into(),
            4 => declaration.handlers.clear(),
            5 => {
                let mut extra = m.transform_handlers[0].clone();
                extra.handler = "extra".into();
                m.transform_handlers.push(extra);
            }
            _ => declaration.handlers[0].handler = "invalid name".into(),
        }
        assert!(Package::build(m, MODULE).is_err(), "case {case}");
    }
}
#[test]
fn planning_and_bound_input_contract_types_and_budgets_are_exact() {
    for case in 0..7 {
        let mut m = manifest();
        match case {
            0 => m.transform_handlers[0].output_type = "ordinary.result".into(),
            1 => m.transform_handlers[1].input_type = "ordinary.input".into(),
            2 => m.transform_handlers[0].max_input_bytes = p::MAX_PLAN_INPUT_BYTES + 1,
            3 => m.transform_handlers[0].max_output_bytes = p::MAX_PROPOSAL_BYTES + 1,
            4 => m.transform_handlers[0].max_output_bytes = 0,
            5 => m.transform_handlers[1].max_input_bytes = p::MAX_BOUND_INPUT_BYTES + 1,
            _ => m.transform_handlers[1].max_input_bytes = 0,
        }
        assert!(Package::build(m, MODULE).is_err(), "case {case}");
    }
}
#[test]
fn invocation_rechecks_input_type_handler_and_bound() {
    let package = Package::build(manifest(), MODULE).unwrap();
    for case in 0..4 {
        let mut t = input();
        match case {
            0 => t.handler = "other".into(),
            1 => t.input_type = "other".into(),
            2 => t.output_type = "other".into(),
            _ => t.input = vec![1; 1025],
        }
        assert!(package.blob_upload_proposal_handler(&t).is_err());
    }
}
fn field23(out: &mut Vec<u8>, raw: &[u8]) {
    prost::encoding::encode_key(23, prost::encoding::WireType::LengthDelimited, out);
    prost::encoding::encode_varint(raw.len() as u64, out);
    out.extend_from_slice(raw);
}
#[test]
fn duplicate_unknown_and_noncanonical_persistent_declaration_refuse() {
    let m = manifest();
    let declaration = m
        .blob_upload_proposal_declaration
        .as_ref()
        .unwrap()
        .encode_to_vec();
    let mut duplicate = m.encode_to_vec();
    field23(&mut duplicate, &declaration);
    assert!(Package::from_parts(&duplicate, MODULE).is_err());
    for suffix in [[0x20, 1], [0x08, 1]] {
        let mut changed = declaration.clone();
        changed.extend_from_slice(&suffix);
        let mut m = m.clone();
        m.blob_upload_proposal_declaration = None;
        let mut raw = m.encode_to_vec();
        field23(&mut raw, &changed);
        assert!(Package::from_parts(&raw, MODULE).is_err());
    }
}
#[test]
fn several_distinct_planners_may_target_the_same_registered_uploader() {
    let mut m = manifest();
    let mut second = m.transform_handlers[0].clone();
    second.handler = "blob.plan.other".into();
    m.transform_handlers.push(second);
    m.blob_upload_proposal_declaration
        .as_mut()
        .unwrap()
        .handlers
        .push(proto::BlobUploadProposalHandler {
            handler: "blob.plan.other".into(),
            upload_handler: "blob.upload".into(),
        });
    let package = Package::build(m, MODULE).unwrap();
    assert!(package.blob_upload_proposal_handler(&input()).is_ok());
    let mut second = input();
    second.handler = "blob.plan.other".into();
    assert!(package.blob_upload_proposal_handler(&second).is_ok());
}
