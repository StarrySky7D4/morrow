//! New041 regressions. Synthetic allocation values below are used ONLY with
//! injected closures, never fed to any OS API. Only the first test owns a real
//! current-process QUERY/context/RM; its SDs are never assigned to an OS object.
use super::*;
use std::cell::Cell;

const WORLD:[u8;12]=[1,1,0,0,0,0,0,1,0,0,0,0];
const NOBODY:[u8;12]=[1,1,0,0,0,0,0,0,0,0,0,0];
fn memory_sd(ace:u8,mask:u32,sid:&[u8])->Buffer {
    let ace_size=8+sid.len();let len=52+ace_size;
    let mut b=Buffer::new(len,SECURITY_CAP).unwrap();let v=b.bytes_mut();
    v[0]=1;v[2..4].copy_from_slice(&0x8004u16.to_le_bytes());
    v[4..8].copy_from_slice(&20u32.to_le_bytes());
    v[8..12].copy_from_slice(&32u32.to_le_bytes());
    v[16..20].copy_from_slice(&44u32.to_le_bytes());
    v[20..32].copy_from_slice(&NOBODY);v[32..44].copy_from_slice(&NOBODY);
    v[44]=2;v[46..48].copy_from_slice(&((8+ace_size) as u16).to_le_bytes());
    v[48..50].copy_from_slice(&1u16.to_le_bytes());
    v[52]=ace;v[53]=0x10; // Keep an inherited flag unchanged through the projection.
    v[54..56].copy_from_slice(&(ace_size as u16).to_le_bytes());
    v[56..60].copy_from_slice(&mask.to_le_bytes());v[60..].copy_from_slice(sid);
    b
}
fn memory_object(sd:Buffer)->PreparedObject {
    // Labelled ordinary-memory fixture for the evaluator, NOT an observed desktop.
    PreparedObject {out:StartupObjectObservation {name_equals_creation_object:F::Observed(true),
        descriptor_read:F::Observed(()),..StartupObjectObservation::default()},
        descriptor:Ok(sd),visible:Ok(false)}
}
fn labelled_queries()->StartupQueries {
    let mut q=StartupQueries::default();q.primary.raw=101;
    q.authz.manager.0.allocate_with(|raw|{*raw=202;(1,0)}).unwrap();
    q.authz.context.0.allocate_with(|raw|{*raw=303;(1,0)}).unwrap();
    q
}
fn labelled_facts()->PrivateTokenFacts {
    PrivateTokenFacts {user:Ok(WORLD.to_vec()),logon:Ok((WORLD.to_vec(),LOGON_GROUP)),
        session:Ok(1),restricted:Ok(false),app_container:Ok(false)}
}

#[test]
fn own_query_authz_evaluates_two_unassigned_memory_dacls_and_checked_frees_originals() {
    // Ordinary self only, flags5 production helpers, never cross-account or VM proof.
    let mut q=StartupQueries::default();
    let (opened,query)=q.primary.open_for(unsafe{windows_sys::Win32::System::Threading::GetCurrentProcess()},
        TokenOpenPurpose::PrimaryQuery,TOKEN_QUERY);
    query.expect("own original QUERY8");
    let initialized=q.authz.initialize(q.primary.raw);
    if initialized.is_ok() {
        let allow=memory_object(memory_sd(0,1,&WORLD));
        let deny=memory_object(memory_sd(1,2,&WORLD));
        authz_object_once(q.authz.context.0.raw,&allow,false,&mut q.authz.station);
        authz_object_once(q.authz.context.0.raw,&deny,false,&mut q.authz.desktop);
    }
    // Retire the actual originals BEFORE checking results; never close the borrowed
    // GetCurrentProcess pseudo handle. A real Authz debt intentionally retains QUERY.
    let retired=q.close_primary_after_authz();let observation=q.authz_observation();
    assert_eq!(opened.requested_mask,8);assert_eq!(initialized,Ok(()));retired.unwrap();
    assert_eq!(observation.station().state,O::ReplyReturned);
    assert_eq!(observation.station().error,0);assert_eq!(observation.station().granted_mask&1,1);
    assert_eq!(observation.desktop().state,O::ReplyReturned);
    assert_eq!(observation.desktop().granted_mask&2,0);
    // MAXIMUM_ALLOWED may also report standard rights from the actual own token.
    // Denial of bit2 remains required; never presume the token's privilege set.
    assert_eq!(observation.desktop().error,if observation.desktop().granted_mask==0 {5}else{0});
    assert_eq!(observation.context().retirement,R::FreedVerified);
    assert_eq!(observation.resource_manager().retirement,R::FreedVerified);
    assert_eq!(q.all_states(),[QueryHandleClose::ClosedVerified,QueryHandleClose::NotAllocated,
        QueryHandleClose::NotAllocated]);assert!(!q.unresolved());
}

#[test]
fn authz_allocation_results_distinguish_api_failure_uncertain_output_and_invalid_success() {
    for (ok,raw,allocation,retirement) in [(0,0,A::FailedNoOutput,R::NotAllocated),
        (0,202,A::FailedUncertainOutput,R::Uncertain),(1,0,A::InvalidSuccessOutput,R::Uncertain)] {
        let mut slot=AuthzSlot::default();
        assert!(slot.allocate_with(|out|{*out=raw;(ok,5)}).is_err());
        assert_eq!(slot.observation.allocation,allocation);
        assert_eq!(slot.observation.retirement,retirement);
        assert_eq!(slot.observation.initialization_error,if ok==0 {5}else{0});
        let calls=Cell::new(0);
        let result=slot.free_with(|_|{calls.set(calls.get()+1);Ok(())});
        assert_eq!(calls.get(),0);assert_eq!(result.is_err(),raw!=0 || ok!=0);
        assert!(slot.allocate_with(|_|panic!("cannot allocate a second original")).is_err());
    }
    let mut r=AuthzResources::default();
    r.manager.0.allocate_with(|out|{*out=202;(1,0)}).unwrap();
    assert_eq!(r.context.0.allocate_with(|_|(0,5)),Err(U::Win32{code:5}));
    let mut calls=Vec::new();
    assert!(r.close_with(|kind,_|{calls.push(kind);Err(6)}).is_err());
    assert_eq!(calls,[AuthzResourceKind::ResourceManager]);
    assert_eq!(r.observation().first_failure(),Some(("InitializeContextFromToken",Some(5))));
    assert_eq!(r.observation().resource_manager().retirement_error,6);
    assert_eq!(r.observation().context().initialization_error,5);
    assert_eq!(r.observation().resource_manager().initialization_error,0);
    assert_eq!(r.observation().context().retirement_error,0);
}

#[test]
fn context_free_debt_retains_manager_real_query_dependency_and_never_retries() {
    let mut q=labelled_queries();let query_calls=Cell::new(0);let mut frees=Vec::new();
    let first=q.close_primary_with(|kind,raw|{frees.push((kind,raw));Err(5)},
        |_|{query_calls.set(query_calls.get()+1);Ok(())});
    assert_eq!(first.unwrap_err().raw_os_error(),Some(5));
    assert_eq!(frees,[(AuthzResourceKind::Context,303)]);assert_eq!(query_calls.get(),0);
    assert_eq!(q.authz_observation().resource_manager().retirement,R::BlockedByContext);
    assert_eq!(q.authz_observation().context().retirement,R::Failed);
    assert_eq!(q.authz_observation().first_failure(),Some(("FreeContext",Some(5))));
    for _ in 0..2 {
        assert_eq!(q.close_primary_with(|_,_|panic!("no native free retry"),
            |_|panic!("QUERY must remain held")).unwrap_err().raw_os_error(),Some(5));
    }
    assert_eq!(q.primary.state(),QueryHandleClose::Held);assert_eq!(q.primary.raw,101);
    assert!(q.unresolved());assert!(!q.primary.close_attempted);
}

#[test]
fn manager_free_debt_follows_context_then_blocks_query_without_replaying_context() {
    let mut q=labelled_queries();let mut order=Vec::new();
    let result=q.close_primary_with(|kind,_|{order.push(kind);
        if kind==AuthzResourceKind::Context {Ok(())}else{Err(6)}},
        |_|panic!("manager debt blocks QUERY close"));
    assert_eq!(result.unwrap_err().raw_os_error(),Some(6));
    assert_eq!(order,[AuthzResourceKind::Context,AuthzResourceKind::ResourceManager]);
    assert_eq!(q.authz.context.0.raw,0);assert_eq!(q.authz.manager.0.raw,202);
    assert_eq!(q.authz_observation().context().retirement,R::FreedVerified);
    assert_eq!(q.authz_observation().first_failure(),Some(("FreeResourceManager",Some(6))));
    assert_eq!(q.close_primary_with(|_,_|panic!("free already attempted"),
        |_|panic!("QUERY close not authorized")).unwrap_err().raw_os_error(),Some(6));
    assert_eq!(q.primary.state(),QueryHandleClose::Held);assert!(q.unresolved());
}

#[test]
fn allocation_and_free_unwind_keep_inflight_slots_even_with_zero_numeric_output() {
    use std::panic::{catch_unwind,AssertUnwindSafe};
    let mut slot=AuthzSlot::default();
    assert!(catch_unwind(AssertUnwindSafe(||slot.allocate_with(|_|panic!("labelled unwind")))).is_err());
    assert_eq!(slot.raw,0);assert!(slot.unresolved());
    assert_eq!(slot.observation.allocation,A::InFlight);
    assert!(slot.free_with(|_|panic!("undefined output is not free authority")).is_err());
    let mut q=labelled_queries();
    assert!(catch_unwind(AssertUnwindSafe(||q.close_primary_with(
        |_,_|panic!("labelled free unwind"),|_|panic!("no query close")))).is_err());
    assert_eq!(q.authz_observation().context().retirement,R::InFlight);
    assert!(q.close_primary_with(|_,_|panic!("cannot retry uncertain free"),
        |_|panic!("no query close")).is_err());
    assert_eq!(q.primary.state(),QueryHandleClose::Held);assert!(q.unresolved());
    let mut evaluating=labelled_queries();let memory=memory_object(memory_sd(0,1,&WORLD));
    assert!(catch_unwind(AssertUnwindSafe(||authz_object_with(&memory,false,
        &mut evaluating.authz.station,|_|panic!("labelled evaluator unwind")))).is_err());
    assert!(evaluating.close_primary_with(|_,_|panic!("unfinished evaluator cannot free dependencies"),
        |_|panic!("unfinished evaluator cannot close QUERY")).is_err());
    assert_eq!(evaluating.authz_observation().station().state,O::InFlight);
    assert_eq!(evaluating.primary.state(),QueryHandleClose::Held);assert!(evaluating.unresolved());
}

#[test]
fn successful_retirement_orders_context_manager_query_and_never_recloses() {
    let mut q=labelled_queries();let order=std::cell::RefCell::new(Vec::new());
    q.close_primary_with(|kind,raw|{order.borrow_mut().push((kind==AuthzResourceKind::Context,raw));Ok(())},
        |raw|{order.borrow_mut().push((false,raw));Ok(())}).unwrap();
    assert_eq!(*order.borrow(),[(true,303),(false,202),(false,101)]);
    assert!(!q.unresolved());assert_eq!(q.primary.state(),QueryHandleClose::ClosedVerified);
    q.close_primary_with(|_,_|panic!("no second Authz free"),
        |_|panic!("no second QUERY close")).unwrap();
}

#[test]
fn generic_mapping_changes_only_mask_bytes_for_both_stations_and_desktop() {
    for (station,visible,expected) in [(true,false,[0x20103,0x2000c,0x20060,0xf016f]),
        (true,true,[0x20303,0x2001c,0x20060,0xf037f]),
        (false,false,[0x20041,0x200be,0x20100,0xf01ff])] {
        for (generic,specific) in [0x80000000u32,0x40000000,0x20000000,0x10000000]
            .into_iter().zip(expected) {
            let original=memory_sd(0,generic|8,&WORLD);let before=original.bytes().to_vec();
            let mapped=mapped_dacl(&original,station,visible).unwrap();
            assert_eq!(u32_at(mapped.bytes(),56).unwrap(),specific|8);
            assert_eq!(&mapped.bytes()[..56],&before[..56]);
            assert_eq!(&mapped.bytes()[60..],&before[60..]);
            assert_eq!(original.bytes(),before); // Original for legacy AccessCheck stays unchanged.
        }
    }
}

#[test]
fn projection_rejects_complex_principal_self_null_absent_overlap_and_truncated_layouts() {
    for ace in [4,5,9,11,17,255] {
        assert_eq!(mapped_dacl(&memory_sd(ace,1,&WORLD),false,false).err(),Some(O::UnsupportedAce));
    }
    let principal_self=[1,1,0,0,0,0,0,5,10,0,0,0];
    assert_eq!(mapped_dacl(&memory_sd(0,1,&principal_self),false,false).err(),Some(O::PrincipalSelf));
    let mut null=memory_sd(0,1,&WORLD);null.bytes_mut()[16..20].fill(0);
    assert_eq!(mapped_dacl(&null,false,false).err(),Some(O::NullOrAbsentDacl));
    let mut absent=memory_sd(0,1,&WORLD);absent.bytes_mut()[2]&=!4;
    assert_eq!(mapped_dacl(&absent,false,false).err(),Some(O::NullOrAbsentDacl));
    let mut alias=memory_sd(0,1,&WORLD);alias.bytes_mut()[8..12].copy_from_slice(&20u32.to_le_bytes());
    assert_eq!(mapped_dacl(&alias,false,false).err(),Some(O::OverlappingLayout));
    let mut short=memory_sd(0,1,&WORLD);short.len-=1;
    assert_eq!(mapped_dacl(&short,false,false).err(),Some(O::InvalidBuffer));
}

#[test]
fn per_object_call_count_and_api_reply_errors_stay_separate_from_legacy_access() {
    let memory=memory_object(memory_sd(0,1,&WORLD));let calls=Cell::new(0);
    let mut api=AuthzObjectObservation::default();
    authz_object_with(&memory,false,&mut api,|_|{calls.set(calls.get()+1);(0,5,u32::MAX,0)});
    assert_eq!(calls.get(),1);assert_eq!(api.state,O::ApiFailed);assert_eq!(api.error,5);
    assert_eq!(api.granted_mask,0);
    authz_object_with(&memory,false,&mut api,|_|panic!("per-object API is one-shot"));
    let mut denied=AuthzObjectObservation::default();
    authz_object_with(&memory,false,&mut denied,|_|(1,0,0,5));
    assert_eq!(denied.state,O::ReplyReturned);assert_eq!(denied.error,5);
    assert_ne!(api,denied);
    let mut invalid_reply=AuthzObjectObservation::default();
    authz_object_with(&memory,false,&mut invalid_reply,|_|(1,0,0,0));
    let projection=AuthzDaclProjection::from_observations(AuthzResourceObservation::default(),AuthzResourceObservation::default(),invalid_reply,AuthzObjectObservation::default());
    assert_eq!(projection.first_failure(),Some(("StationAuthzAccessCheck",None)));
    assert!(!memory.out.access_check_invoked);
    assert_eq!(memory.out.access,F::NotObserved(U::NotAttempted));
    let mut wrong=memory_object(memory_sd(0,1,&WORLD));wrong.out.name_equals_creation_object=F::Observed(false);
    let mut uncalled=AuthzObjectObservation::default();
    authz_object_with(&wrong,false,&mut uncalled,|_|panic!("no same-object identity"));
    assert_eq!(uncalled.state,O::ObjectIdentityUnavailable);
}

#[test]
fn authz_five_reads_restrictions_and_appcontainer_are_real_prerequisites() {
    assert_eq!(authz_prerequisite(Err(U::Win32{code:5}),&labelled_facts()),Err(O::PrimaryUnavailable));
    for index in 0..5 {
        let mut facts=labelled_facts();
        match index {0=>facts.user=Err(U::InvalidBuffer),1=>facts.logon=Err(U::InvalidBuffer),
            2=>facts.session=Err(U::InvalidBuffer),3=>facts.restricted=Err(U::InvalidBuffer),
            _=>facts.app_container=Err(U::InvalidBuffer)}
        assert_eq!(authz_prerequisite(Ok(()),&facts),Err(O::PrimaryUnavailable));
    }
    let mut facts=labelled_facts();facts.restricted=Ok(true);
    assert_eq!(authz_prerequisite(Ok(()),&facts),Err(O::RestrictedToken));
    facts.restricted=Ok(false);facts.app_container=Ok(true);
    assert_eq!(authz_prerequisite(Ok(()),&facts),Err(O::AppContainer));
}

#[test]
fn copy_history_keeps_authz_as_data_not_native_completion_and_latches_original_identity() {
    use crate::{WindowsStartDiagnostic,WindowsStartRunnerObservation,WindowsStartRunnerState,
        WindowsStartSnapshot,WindowsStartCompletion};
    fn require_copy<T:Copy>() {}require_copy::<WindowsStartSnapshot>();
    assert!(mem::size_of::<WindowsStartSnapshot>()<=2048);
    assert_eq!(mem::size_of::<AuthzDaclProjection>(),32);
    assert_eq!(AuthzDaclProjection::default().station().state,O::NotAttempted);
    let original=WindowsStartRunnerObservation {startup_id:41,runner_pid:123,
        runner_creation_filetime_100ns:134367020212369864,state:WindowsStartRunnerState::RunningAtObservation};
    let history=WindowsStartDiagnostic::default();history.observe_original_runner(original);
    let mut context=WindowsStartupContext {startup_id:41,original_runner_identity:Some((123,original.runner_creation_filetime_100ns)),
        query_status:F::Observed(()),..WindowsStartupContext::default()};
    let mut allowed=AuthzObjectObservation {state:O::ReplyReturned,error:0,granted_mask:1,
        mapping:M::Desktop};
    context.authz_dacl=AuthzDaclProjection::from_observations(AuthzResourceObservation::default(),AuthzResourceObservation::default(),allowed,AuthzObjectObservation::default());
    history.observe_startup_context(context);
    let first=history.snapshot().unwrap();assert_eq!(first.current.original_runner,Some(original));
    assert_eq!(first.completion,WindowsStartCompletion::NotObserved);
    assert_eq!(first.current.startup_context.unwrap().station.access_check_invoked,false);
    context.original_runner_identity=Some((999,1));allowed.granted_mask=u32::MAX;
    context.authz_dacl=AuthzDaclProjection::from_observations(AuthzResourceObservation::default(),AuthzResourceObservation::default(),allowed,AuthzObjectObservation::default());
    history.observe_startup_context(context);assert_eq!(history.snapshot().unwrap(),first);
    let text=format!("{:?}",first.current.startup_context.unwrap().authz_dacl);
    assert!(text.contains("MIC_SACL_CAP_NATIVE_NOT_PROVEN"));
    assert!(!text.contains("303"));assert!(!text.contains("202"));
}
