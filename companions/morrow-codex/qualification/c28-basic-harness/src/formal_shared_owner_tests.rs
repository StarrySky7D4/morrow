//! Real frozen session guest, fresh synthetic protected store and one owner.
//! Three newly reviewed full wrappers per attempt, with stable immutable bases.
//! No native execution factory, account setup, VM or repeated-refusal claim.
use super::*;
use std::sync::Arc;

struct RetainedSdkQualification {
    workbench: Option<Box<Workbench>>,
    contexts: Vec<PreparedSessionGuest>,
    wrappers: Vec<ReviewedWrapper>,
}

#[test]
fn seventeen_frozen_session_guests_keep_one_protected_owner_and_sdk_registry() {
    let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root=std::env::temp_dir().join(format!("morrow-shared-sdk-{}-{stamp}",std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let held=Box::leak(Box::new(RetainedSdkQualification {
        workbench:None,contexts:Vec::new(),wrappers:Vec::new(),
    }));
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        held.workbench=Some(Box::new(Workbench::open(&root.join("synthetic.sqlite"),None).unwrap()));
        let original_binding=held.workbench.as_ref().unwrap().agent_owner_binding();
        let original_address=held.workbench.as_ref().unwrap().as_ref() as *const Workbench as usize;
        for sequence in 0..17usize {
            let attempt=root.join(format!("attempt-{sequence:02}"));
            std::fs::create_dir(&attempt).unwrap();
            // Only the pure namespace helpers need an artifact digest. This
            // immutable guest hash is not represented as a native PE proof.
            let helper=Artifact{path:attempt.join("not-a-native-executable"),
                sha256:morrow_agent_session_exec_v1_r2::hash(SESSION_PUBLIC_R2_V1)};
            let mut workflow=Workflow::new(&attempt,&helper,false).unwrap();
            workflow.shared_parent_session=true;
            workflow.basic=true;
            let id=workflow.catalog_package_id("vm.public.session.r2.v1");
            let seed=workflow.catalog_request_seed(100).unwrap();
            let workbench=held.workbench.as_mut().unwrap();
            let wrapper=ReviewedWrapper::session_public_r2_v1(workbench,
                workflow.wrapper_path("session-public-r2-v1"),&id,SESSION_PUBLIC_R2_V1,&workflow.sid,seed).unwrap();
            held.wrappers.push(wrapper);
            let wrapper=held.wrappers.last_mut().unwrap();
            for step in selection_steps(sequence) {
                wrapper.step(workbench,step).unwrap();
            }
            let prepared=sealed_preparation::prepare_session_guest(workbench,&workflow.sid).unwrap();
            held.contexts.push(prepared);
            let original=&held.contexts[0];
            let actual=&held.contexts[sequence];
            assert!(Arc::ptr_eq(&original.host,&actual.host));
            assert!(Arc::ptr_eq(&original.resources,&actual.resources));
            assert!(actual.resources.owner_binding()==original_binding);
            let pair=ReviewedWrapper::current_pair(workbench,&[&*wrapper],workflow.catalog_request_seed(200).unwrap()).unwrap();
            let prepared=&mut held.contexts[sequence];
            assert!(workbench.finish_agent_attempt(&prepared.resources).is_err(),
                "an outstanding original context cannot be flushed as a completed attempt");
            let key=workbench.start_agent(AgentStart {
                package_id:wrapper.id.clone(),full_sha256:wrapper.full_sha256,revisions:pair,
                lifetime:Duration::from_secs(60),limits:AgentLimits::default(),
            },&mut prepared.context).unwrap();
            assert!(prepared.context.is_none());
            let nonce=crate::preflight::digest(&format!("{}{}",workflow.nonce,workflow.nonce)).unwrap();
            let receipt=crate::sealed::run_combined_session(workbench,key,prepared.generation,
                nonce[..16].try_into().unwrap(),&workflow.sid).unwrap();
            assert_eq!(receipt.generation,prepared.generation);
            join_original(workbench,key).unwrap();
            workbench.acknowledge_agent(key).unwrap();
            assert!(workbench.agent_status().unwrap().is_none());
            workbench.finish_agent_attempt(&prepared.resources).unwrap();
            assert!(workbench.maintenance_warning().is_none());
            assert!(prepared.resources.is_clean().unwrap());
            // Match the complete catalog footprint of the native qualification:
            // every attempt has a new session, controls and proposal wrapper.
            let domain=format!("synthetic-domain-{}",workflow.nonce);
            let controls=ReviewedWrapper::controls(workbench,workflow.wrapper_path("controls"),
                &workflow.catalog_package_id("vm.sealed.controls"),PROCESS,&workflow.sid,
                &domain,workflow.caps(),workflow.catalog_request_seed(300).unwrap()).unwrap();
            held.wrappers.push(controls);
            for step in selection_steps(sequence) {
                held.wrappers.last_mut().unwrap().step(workbench,step).unwrap();
            }
            let proposal=ReviewedWrapper::proposal(workbench,workflow.wrapper_path("proposal"),
                &workflow.catalog_package_id("vm.sealed.proposal"),PROPOSAL,&workflow.sid,
                &domain,workflow.catalog_request_seed(400).unwrap()).unwrap();
            held.wrappers.push(proposal);
            for step in selection_steps(sequence) {
                held.wrappers.last_mut().unwrap().step(workbench,step).unwrap();
            }
            let current=&held.wrappers[sequence*3..sequence*3+3];
            ReviewedWrapper::current_pair(workbench,&current.iter().collect::<Vec<_>>(),
                workflow.catalog_request_seed(700).unwrap()).unwrap();
            if sequence>0 {
                assert!(ReviewedWrapper::current_pair(workbench,&[&held.wrappers[0]],
                    workflow.catalog_request_seed(800).unwrap()).is_err(),
                    "an old full wrapper must not inherit the new attempt's selection/approval");
            }
            assert!(workbench.agent_owner_binding()==original_binding);
            assert_eq!(workbench.as_ref() as *const Workbench as usize,original_address);
            let scheduler=workbench.agent_scheduler_status();
            assert_eq!((scheduler.active_tokens,scheduler.retained_debts,scheduler.uncertain_debts),(0,0,0));
            eprintln!("SHARED_PROTECTED_SDK_SESSION_CLOSED sequence={} actual_frozen_guest=true same_owner=true same_registry=true",sequence+1);
        }
        assert_eq!(held.wrappers.len(),51);
        assert_eq!(held.wrappers.iter().map(|wrapper|wrapper.id.as_str()).collect::<std::collections::BTreeSet<_>>().len(),3);
        assert_eq!(held.wrappers.iter().map(|wrapper|wrapper.base_sha256).collect::<std::collections::BTreeSet<_>>().len(),3);
        assert_eq!(held.wrappers.iter().map(|wrapper|wrapper.full_sha256).collect::<std::collections::BTreeSet<_>>().len(),51);
        assert_eq!(held.contexts.len(),17);
        // 18 first-round mutations + 12 per later round = 210. Spend the exact
        // remaining 46 confirmations through the original administrative wire;
        // request 257 must still fail. No owner reset, eviction or raised bound.
        let workbench=held.workbench.as_mut().unwrap();
        let last=held.wrappers.last().unwrap();
        let mut pair=ReviewedWrapper::current_pair(workbench,&[last],900).unwrap();
        for index in 0..47u128 {
            use morrow_agent_catalog_admin_v1::{Action,Request,Reply,Status};
            let request=Request{id:(1000+index).to_le_bytes(),action:Action::BaseEnable {
                id:last.id.clone(),full_sha256:last.full_sha256,enabled:true,revisions:pair,
            }};
            let raw=morrow_workbench_host::protocol::respond(workbench,&request.encode().unwrap()).unwrap();
            let reply=Reply::decode_for(&request,&raw).unwrap();
            assert_eq!(reply.outcome.status,if index<46 {Status::Ok}else{Status::Limit});
            if index==46 {assert_eq!(reply.outcome.revisions,pair);}
            pair=reply.outcome.revisions;
        }
        eprintln!("SHARED_CATALOG_51_FULL_WRAPPERS_3_BASES_210_MUTATIONS_LIMIT_257_PRESERVED");
        held.workbench.as_mut().unwrap().finish().unwrap();
        held.contexts.clear();held.wrappers.clear();
        drop(held.workbench.take().unwrap());
    }));
    if result.is_err() {
        eprintln!("SHARED_PROTECTED_SDK_SESSION_UNKNOWN_ORIGINAL_OWNER_RETAINED_NO_RETRY");
        loop {std::thread::park();}
    }
}

fn selection_steps(sequence:usize)->[SelectionStep;6] {
    [SelectionStep::Install,
     if sequence==0 {SelectionStep::BaseSelect}else{SelectionStep::VerifyExistingBaseSelected},
     if sequence==0 {SelectionStep::BaseEnable}else{SelectionStep::VerifyExistingBaseEnabled},
     SelectionStep::WrapperSelect,SelectionStep::Approve,SelectionStep::WrapperEnable]
}
