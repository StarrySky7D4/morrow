//! SOURCE-only pure protocol/fault regressions. No WinAPI, sleeps, environment
//! selection or fabricated native witness. ROOT separately invokes the setup
//! --c28-station-broker-a0-self-fixture-v1 <exact-current-setup-sha256> entry for
//! real create/duplicate/restore/EOF/native join/checked-close qualification.
use super::*;

fn binding()->Binding {Binding {generation:7,parent_pid:101,parent_time:0x1234_0000_0001,
    helper_pid:102,helper_time:0x1234_0000_0002,deadline_ticks:100_000,nonce:[9;32]}}
fn offered()->Frame {Frame {kind:OFFER,binding:binding(),station:71,desktop:72,
    station_sd:[3;32],desktop_sd:[4;32],image:[5;32]}}
fn completed_model()->StationBrokerSnapshot {
    let mut h=StationBrokerSnapshot::new(7);h.phase=StationBrokerPhase::HelperRetired;
    h.helper_pid=Some(102);h.helper_creation_filetime_100ns=Some(0x1234_0000_0002);
    h.helper_raw_dword=Some(0);h.helper_objects_closed_receipt=true;
    h.original_eof=[true;2];h.original_reader_joined=[true;2];h.host_association_unchanged=true;h
}

#[test]
fn a0_unbound_pin_and_untrusted_namespace_rejected_before_native_admission() {
    assert!(matches!(StationBrokerA0::reserve_reviewed_setup(None),Err(e) if e.kind()==io::ErrorKind::PermissionDenied));
    assert!(parse_sha("").is_none());assert!(parse_sha(&"0".repeat(64)).is_none());
    assert!(parse_sha(&"A".repeat(64)).is_none());assert!(parse_sha(&"1".repeat(63)).is_none());
    assert_eq!(parse_sha(&"ab".repeat(32)),Some([0xab;32]));
    let name=endpoint(&[9;32]);assert!(valid_endpoint(&name));
    for bad in [r"\\other\pipe\C28StationBrokerA0-09",r"C:\arbitrary",r"\\.\pipe\other"] {
        assert!(!valid_endpoint(bad));
    }
    assert!(!valid_endpoint(&(name.clone()+"\\extra")));
    assert!(!valid_endpoint(&name.to_uppercase()));
}

#[test]
fn a0_frame_exact_length_kind_reserved_and_mac_are_all_required() {
    let mut key=[0u8;32];key[..20].fill(0x0b);
    assert_eq!(mac(&key,b"Hi There"),parse_sha("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7").unwrap());
    let bytes=encode(offered());assert!(decode(&bytes,OFFER,Some(binding())).is_ok());
    for size in [0,8,223,255] {assert!(decode(&bytes[..size],OFFER,Some(binding())).is_err());}
    let mut extra=bytes.to_vec();extra.push(0);assert!(decode(&extra,OFFER,Some(binding())).is_err());
    assert!(decode(&bytes,CLOSED,Some(binding())).is_err());
    for offset in [0,9,56,88,104,168,200,224,255] {
        let mut bad=bytes;bad[offset]^=1;assert!(decode(&bad,OFFER,Some(binding())).is_err(),"offset {offset}");
    }
    assert!(decode(&encode(message(0,binding(),[1;32])),0,Some(binding())).is_err());
}

#[test]
fn a0_generation_actual_pid_and_full_filetime_reject_other_attempts_even_with_valid_mac() {
    let expected=binding();
    for actual in [Binding {generation:8,..expected},Binding {parent_pid:103,..expected},
        Binding {helper_pid:104,..expected},Binding {parent_time:expected.parent_time+(1u64<<32),..expected},
        Binding {deadline_ticks:expected.deadline_ticks+1,..expected},
        Binding {helper_time:expected.helper_time+(1u64<<32),..expected},Binding {nonce:[8;32],..expected}] {
        assert!(decode(&encode(message(ACCEPT,actual,[1;32])),ACCEPT,Some(expected)).is_err());
    }
    for actual in [Binding {generation:0,..expected},Binding {helper_time:0,..expected},
        Binding {parent_pid:0,..expected},Binding {nonce:[0;32],..expected}] {
        assert!(decode(&encode(message(HELLO,actual,[1;32])),HELLO,None).is_err());
    }
}

#[test]
fn a0_offer_numbers_do_not_become_owned_slots_and_non_offer_has_no_handle_payload() {
    let decoded=decode(&encode(offered()),OFFER,Some(binding())).unwrap();
    assert_eq!(decoded.station,71);assert_eq!(decoded.desktop,72);
    let originals=Originals::new();assert_eq!(originals.station.state,StationBrokerHandle::NotAllocated);
    assert_eq!(originals.desktop.state,StationBrokerHandle::NotAllocated);
    for frame in [Frame {station:0,..offered()},Frame {desktop:71,..offered()},
        Frame {kind:ACCEPT,..offered()}] {
        assert!(decode(&encode(frame),frame.kind,Some(binding())).is_err());
    }
}

#[test]
fn a0_half_success_and_uncertain_native_outputs_are_retained_without_reissue() {
    let mut station=Slot::new(CloseKind::Station);let mut desktop=Slot::new(CloseKind::Desktop);
    station.begin().unwrap();desktop.begin().unwrap();
    station.captured(true,71,0).unwrap();assert!(desktop.captured(false,72,5).is_err());
    assert_eq!(station.state,StationBrokerHandle::Held);assert_eq!(station.raw,71);
    assert!(station.captured(true,999,0).is_err());assert_eq!(station.raw,71);
    assert_eq!(desktop.state,StationBrokerHandle::Uncertain);assert_eq!(desktop.raw,72);
    assert!(station.begin().is_err());assert!(desktop.begin().is_err());
    let mut not_created=Slot::new(CloseKind::Station);not_created.begin().unwrap();
    assert!(not_created.captured(false,0,183).is_err());
    assert_eq!(not_created.state,StationBrokerHandle::Rejected);assert!(not_created.begin().is_err());
    let mut false_success=Slot::new(CloseKind::Desktop);false_success.begin().unwrap();
    assert!(false_success.captured(true,0,0).is_err());assert!(!false_success.retired());
}

#[test]
fn a0_checked_close_failure_preserves_first_error_original_and_forbids_retry() {
    let native=StationBrokerFailure::from_io(StationBrokerDebt::Authentication,
        &io::Error::other(BrokerNtStatus(0xc0000022u32 as i32)));
    assert_eq!(native.native_ntstatus,Some(0xc0000022u32 as i32));
    assert_eq!(native.win32_error,None);assert_eq!(native.kind,StationBrokerErrorKind::NtStatus);
    let mut original=Slot::new(CloseKind::Desktop);original.begin().unwrap();original.captured(true,72,0).unwrap();
    let mut calls=0;
    let error=original.close_with(|raw|{calls+=1;assert_eq!(raw,72);Err(io::Error::from_raw_os_error(170))}).unwrap_err();
    assert_eq!(error.raw_os_error(),Some(170));assert_eq!(original.raw,72);
    assert_eq!(original.state,StationBrokerHandle::CloseFailed {error:170});
    assert!(original.close_with(|_|{calls+=1;Ok(())}).is_err());assert_eq!(calls,1);
    assert!(!original.retired());
}

#[test]
fn a0_close_unwind_keeps_inflight_original_and_cannot_invoke_close_again() {
    let mut original=Slot::new(CloseKind::Station);original.begin().unwrap();original.captured(true,71,0).unwrap();
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _=original.close_with(|_|->io::Result<()>{panic!("pure injected close unwind")});
    }));
    assert!(result.is_err());assert_eq!(original.raw,71);assert_eq!(original.state,StationBrokerHandle::InFlight);
    assert!(original.close_with(|_|panic!("must not retry")).is_err());assert!(!original.retired());
}

#[test]
fn a0_pending_io_disconnect_or_budget_expiry_never_authorizes_storage_retirement() {
    let mut channel=Channel::new();assert!(channel.can_close());
    channel.read_pending=true;assert!(!channel.can_close());assert!(!channel.retired());
    channel.write_pending=true;channel.read_pending=false;assert!(!channel.can_close());
    assert!(!channel.retired());
    assert_eq!(remaining(Instant::now()-Duration::from_secs(1)).unwrap_err().kind(),io::ErrorKind::TimedOut);
    assert!(shared_remaining(100,100,1000).is_err());assert!(shared_remaining(99,100,1000).is_err());
    assert!(shared_remaining(101,100,0).is_err());assert!(shared_remaining(40_000,100,1000).is_err());
    assert_eq!(shared_remaining(1100,100,1000).unwrap(),Duration::from_millis(999));
    let mut capture=Capture {len:0,eof:false,error:Some(109)};
    assert!(!reader_completed(1,capture));capture.error=None;assert!(!reader_completed(0,capture));
}

#[test]
fn a0_cap_limit_or_logical_zero_cannot_replace_original_eof_and_native_reader_join() {
    let mut c=Capture {len:0,eof:true,error:None};assert!(reader_completed(0,c));
    assert!(!reader_completed(259,c));c.len=STDIO_CAP;c.error=Some(122);assert!(!reader_completed(0,c));
    let good=completed_model();assert!(helper_retired_gate(&good));
    for i in 0..2 {
        let mut bad=good;bad.original_eof[i]=false;assert!(!helper_retired_gate(&bad));
        let mut bad=good;bad.original_reader_joined[i]=false;assert!(!helper_retired_gate(&bad));
    }
    for code in [None,Some(7),Some(259),Some(0xc0000142)] {
        let mut bad=good;bad.helper_raw_dword=code;assert!(!helper_retired_gate(&bad));
    }
}

#[test]
fn a0_local_closure_needs_object_receipt_unchanged_host_and_zero_real_debt() {
    let good=completed_model();assert!(helper_retired_gate(&good));
    for mut bad in [StationBrokerSnapshot {helper_objects_closed_receipt:false,..good},
        StationBrokerSnapshot {host_association_unchanged:false,..good},
        StationBrokerSnapshot {no_replay:true,..good},StationBrokerSnapshot {helper_pid:None,..good}] {
        assert!(!helper_retired_gate(&bad));bad.phase=StationBrokerPhase::Unknown;
        assert!(!helper_retired_gate(&bad));
    }
    let mut originals=Originals::new();assert!(originals.all_retired());
    originals.desktop.begin().unwrap();assert!(!originals.all_retired());
    originals.desktop.captured(true,72,0).unwrap();assert!(!originals.all_retired());
    originals.desktop.close_with(|_|Ok(())).unwrap();assert!(originals.all_retired());
    let mut local=good;local.phase=StationBrokerPhase::ResourceOnlyClosed;
    assert!(!helper_retired_gate(&local)); // A second transfer/retire is not allowed.
    assert_eq!(std::mem::size_of::<StationBrokerResourceOnlyClosed>(),8);
    assert!(std::mem::size_of::<StationBrokerSnapshot>()<=1024);
}

#[test]
fn a0_borrowed_original_rejects_busy_without_zero_snapshot_or_native_action() {
    let gate=std::sync::atomic::AtomicBool::new(false);claim_borrow(&gate).unwrap();
    assert_eq!(claim_borrow(&gate).unwrap_err().kind(),io::ErrorKind::WouldBlock);
    assert!(gate.load(std::sync::atomic::Ordering::Acquire));
    // Only the original borrow's release can admit a new read; a failed query
    // cannot clear it. No Core/state/registry mutex or native call is involved.
    gate.store(false,std::sync::atomic::Ordering::Release);assert!(claim_borrow(&gate).is_ok());
}

#[test]
fn a0_collision_names_are_finite_attempt_scoped_and_receipt_never_is_business_completion() {
    let original=binding();let (station,desktop)=names(original);
    assert!(!station.contains('\\'));assert!(!desktop.contains('\\'));
    assert!(station.len()<128 && desktop.len()<128);
    assert_ne!(names(Binding {generation:8,..original}).0,station);
    assert_ne!(names(Binding {nonce:[8;32],..original}).1,desktop);
    let mut h=StationBrokerSnapshot::new(7);h.helper_objects_closed_receipt=true;
    assert!(!helper_closed_facts(&h));assert_eq!(h.helper_raw_dword,None);
    assert_eq!(h.original_eof,[false;2]);assert_eq!(h.original_reader_joined,[false;2]);
    // CWF_CREATE_ONLY's actual 183 collision is never converted to Held.
    let mut collision=Slot::new(CloseKind::Station);collision.begin().unwrap();
    assert_eq!(collision.captured(false,0,183).unwrap_err().raw_os_error(),Some(183));
    assert_eq!(collision.state,StationBrokerHandle::Rejected);
}

// These fault models use only local metadata, never a real admission registry,
// native handle, reservation, or minted ResourceOnlyClosed capability.
#[test]
fn a0_commit_deadline_samples_retain_same_arc_until_timely_linearization() {
    let early=Instant::now();let deadline=early+Duration::from_secs(1);
    let late=deadline+Duration::from_secs(1);let original=Arc::new(17u8);
    let mut slot=Some(Arc::clone(&original));
    assert!(!budget_available(deadline,&mut CommitClock::scripted([late;8],None)));
    // Pause after entry/connect and before acquiring the registry: the original
    // charge remains. A post-take late sample restores it before publication.
    for times in [[late;8],[early,late,late,late,late,late,late,late]] {
        let count=Arc::strong_count(&original);
        assert_eq!(take_timely(&mut slot,deadline,&mut CommitClock::scripted(times,None)),Err(TimelyTakeError::Late));
        assert!(Arc::ptr_eq(slot.as_ref().unwrap(),&original));
        assert_eq!(Arc::strong_count(&original),count);
    }
    let removed=take_timely(&mut slot,deadline,&mut CommitClock::scripted([early;8],None)).unwrap();
    assert!(slot.is_none());assert!(Arc::ptr_eq(&removed,&original));
    // Return/observation can be late without recreating a committed charge.
    assert!(!budget_available(deadline,&mut CommitClock::scripted([late;8],None)));
    assert!(slot.is_none());assert_eq!(Arc::strong_count(&original),2);
}

#[test]
fn a0_commit_unwind_restores_original_before_poisoned_mutex_is_visible() {
    let early=Instant::now();let deadline=early+Duration::from_secs(1);
    let original=Arc::new(19u8);let slot=std::sync::Mutex::new(Some(Arc::clone(&original)));
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut held=slot.lock().unwrap();
        let _=take_timely(&mut *held,deadline,&mut CommitClock::scripted([early;8],Some(1)));
    }));
    assert!(result.is_err());assert!(slot.is_poisoned());
    let held=slot.lock().unwrap_err().into_inner();
    assert!(Arc::ptr_eq(held.as_ref().unwrap(),&original));
    assert_eq!(Arc::strong_count(&original),2);
}

#[test]
fn a0_partial_connect_and_late_registry_keep_charge_and_first_failure() {
    let early=Instant::now();let deadline=early+Duration::from_secs(1);
    let original=Arc::new(23u8);let mut charged=Some(Arc::clone(&original));
    for registry in [StationBrokerRegistryRelease::RetainedLate,StationBrokerRegistryRelease::RetainedBusy] {
        let mut history=StationBrokerSnapshot::new(7);
        history.connect_release=StationBrokerConnectRelease::ReleasedSameStartupId;
        history.registry_release=registry;
        let first=StationBrokerFailure::from_io(StationBrokerDebt::Admission,
            &io::Error::from(if registry==StationBrokerRegistryRelease::RetainedLate {
                io::ErrorKind::TimedOut
            } else {io::ErrorKind::WouldBlock}));
        latch_delivery(&mut history,ReportStage::Closed,
            StationBrokerOutput::Unavailable(StationBrokerUnavailable::TransportUnqualified),first);
        latch_delivery(&mut history,ReportStage::Closed,StationBrokerOutput::Unwound,
            StationBrokerFailure {target:StationBrokerDebt::Unwind,kind:StationBrokerErrorKind::Unwind,
                win32_error:None,native_ntstatus:None});
        assert_eq!(history.connect_release,StationBrokerConnectRelease::ReleasedSameStartupId);
        assert_eq!(history.registry_release,registry);assert_eq!(history.first_failure,Some(first));
        assert!(history.no_replay);assert_eq!(history.phase,StationBrokerPhase::Unknown);
        assert!(Arc::ptr_eq(charged.as_ref().unwrap(),&original));
    }
    let mut clock=CommitClock::scripted([deadline;8],None);
    assert_eq!(take_timely(&mut charged,deadline,&mut clock),Err(TimelyTakeError::Late));
    assert!(Arc::ptr_eq(charged.as_ref().unwrap(),&original));
}

fn report_model(closed:bool,deadline:Instant)->StationBrokerA0 {
    let mut h=StationBrokerSnapshot::new(7);
    h.phase=if closed {StationBrokerPhase::ResourceOnlyClosed}else{StationBrokerPhase::HelperRetired};
    h.station_alias=if closed {StationBrokerHandle::ClosedVerified}else{StationBrokerHandle::Held};
    h.desktop_alias=h.station_alias;
    if closed {h.registry_release=StationBrokerRegistryRelease::TimelyCommitted;}
    StationBrokerA0 {reservation:None,history:Some(h),deadline,
        delivery:StationBrokerDelivery::default(),_same_thread:PhantomData}
}

#[test]
fn a0_fixture_error_unwind_and_budget_pre_post_commit_never_reissue_output() {
    let early=Instant::now();let deadline=early+Duration::from_secs(1);
    for closed in [false,true] {
        let stage=if closed {ReportStage::Closed}else{ReportStage::Aliases};
        for unwind in [false,true] {
            let mut fixture=report_model(closed,deadline);let mut calls=0;
            let error=fixture.report_with(stage,|h| {
                calls+=1;
                let state=if closed {h.delivery.closed}else{h.delivery.aliases};
                assert_eq!(state,StationBrokerOutput::InFlight);
                if unwind {panic!("pure injected fixture delivery unwind");}
                Err(io::Error::from_raw_os_error(109))
            },&mut CommitClock::scripted([early;8],None)).unwrap_err();
            fixture.retain_fixture_failure(&error,false); // Outer catch must not erase Failed/Unwound.
            let h=fixture.snapshot().unwrap();let original=h.delivery.first_failure.unwrap();
            assert_eq!(if closed {h.delivery.closed}else{h.delivery.aliases},
                if unwind {StationBrokerOutput::Unwound}else{StationBrokerOutput::Failed});
            assert_eq!(original.kind,if unwind {StationBrokerErrorKind::Unwind}else{StationBrokerErrorKind::Win32});
            assert_eq!(original.win32_error,if unwind {None}else{Some(109)});
            assert!(h.no_replay && h.delivery.outer_unknown);
            assert_eq!(h.phase,if closed {StationBrokerPhase::ResourceOnlyClosed}else{StationBrokerPhase::Unknown});
            assert_eq!(h.registry_release,if closed {StationBrokerRegistryRelease::TimelyCommitted}else{StationBrokerRegistryRelease::Retained});
            assert_eq!(h.station_alias,if closed {StationBrokerHandle::ClosedVerified}else{StationBrokerHandle::Held});
            assert!(fixture.report_with(stage,|_|{calls+=1;Ok(())},&mut CommitClock::scripted([early;8],None)).is_err());
            assert_eq!(calls,1);assert_eq!(fixture.snapshot().unwrap().delivery.first_failure,Some(original));
        }
        let mut expired=report_model(closed,deadline);let mut calls=0;
        assert_eq!(expired.report_with(stage,|_|{calls+=1;Ok(())},
            &mut CommitClock::scripted([deadline;8],None)).unwrap_err().kind(),io::ErrorKind::TimedOut);
        assert_eq!(calls,0);assert!(expired.snapshot().unwrap().deadline_expired_observed);
        let mut late=report_model(closed,deadline);
        assert_eq!(late.report_with(stage,|_|{calls+=1;Ok(())},
            &mut CommitClock::scripted([early,deadline,deadline,deadline,deadline,deadline,deadline,deadline],None))
            .unwrap_err().kind(),io::ErrorKind::TimedOut);
        assert_eq!(calls,1);
        let h=late.snapshot().unwrap();
        assert_eq!(if closed {h.delivery.closed}else{h.delivery.aliases},StationBrokerOutput::WrittenLate);
        assert!(h.no_replay && h.deadline_expired_observed);
    }
}

#[test]
fn a0_observation_unavailable_and_pi_reports_never_create_verified_identity() {
    let unavailable=StationBrokerObservation::unavailable(StationBrokerUnavailable::ParentBorrowed);
    assert_eq!(unavailable.parent,StationBrokerParentObservation::Unavailable(StationBrokerUnavailable::ParentBorrowed));
    assert_eq!(unavailable.no_replay,None);
    let mut h=StationBrokerSnapshot::new(7);
    h.reported_creation=Some(StationBrokerReportedCreation {api_returned_success:true,process_id:101,thread_id:102});
    let observed=StationBrokerObservation::from_parent(h);
    assert_eq!(observed.helper_first_failure,StationBrokerUnavailable::IdentityNotCaptured);
    assert_eq!(h.helper_pid,None);assert_eq!(h.helper_creation_filetime_100ns,None);assert_eq!(h.helper_raw_dword,None);
    h.slots.read_pending=true;
    assert_eq!(StationBrokerObservation::from_parent(h).helper_first_failure,StationBrokerUnavailable::PendingOriginalIo);
    h.slots.read_pending=false;h.deadline_expired_observed=true;
    assert_eq!(StationBrokerObservation::from_parent(h).helper_first_failure,StationBrokerUnavailable::BudgetExpired);
    h.deadline_expired_observed=false;h.helper_pid=Some(101);h.helper_creation_filetime_100ns=Some(0x1234_0000_0001);
    assert_eq!(StationBrokerObservation::from_parent(h).helper_first_failure,StationBrokerUnavailable::HelperNoCommittedErrorRecord);
    let first=StationBrokerFailure::from_io(StationBrokerDebt::FixtureDelivery,&io::Error::from_raw_os_error(109));
    latch_delivery(&mut h,ReportStage::Aliases,StationBrokerOutput::Failed,first);
    let fixture=StationBrokerA0 {reservation:None,history:Some(h),deadline:Instant::now(),
        delivery:StationBrokerDelivery::default(),_same_thread:PhantomData};
    let actual=fixture.observation();assert!(actual.delivery.outer_unknown);
    assert_eq!(actual.delivery.first_failure,Some(first));assert_eq!(actual.no_replay,Some(true));
}

// 045 SOURCE/NOT_RUN pure reply injection through the production selector.
// No original/fake token HANDLE, OS call, helper, process or cleanup is used.
#[test]
fn a0_token045_elevation_uses_one_upfront_aligned_four_byte_fill_without_probe() {
    assert_eq!(mem::size_of::<TOKEN_ELEVATION>(),4);
    for value in [0u32,1,u32::MAX] {
        let mut calls=0;
        let (words,len)=token_buffer_with(TokenElevation,|buffer,capacity| {
            calls+=1;assert_eq!(calls,1);assert!(!buffer.is_null());assert_eq!(capacity,4);
            assert_eq!(buffer as usize%mem::align_of::<u64>(),0);
            unsafe{ptr::write(buffer.cast::<u32>(),value);}
            TokenQueryReply {ok:true,returned:4,error:0}
        }).unwrap();
        assert_eq!(calls,1);assert_eq!(len,4);assert_eq!(words.len(),1);
        assert_eq!(words[0] as u32,value);assert_eq!(words[0]>>32,0);
        // Zero is a valid observed elevation value, never substituted for error.
    }
}

#[test]
fn a0_token045_elevation_native_failure_keeps_raw_error_and_never_retries() {
    for error in [0u32,5,24,122] {
        let mut calls=0;
        let result=token_buffer_with(TokenElevation,|buffer,capacity| {
            calls+=1;assert_eq!(calls,1);assert!(!buffer.is_null());assert_eq!(capacity,4);
            TokenQueryReply {ok:false,returned:4,error}
        }).unwrap_err();
        assert_eq!(calls,1);assert_eq!(result.raw_os_error(),Some(error as i32));
        let fact=StationBrokerFailure::from_io(StationBrokerDebt::OwnToken,&result);
        assert_eq!(fact.kind,StationBrokerErrorKind::Win32);assert_eq!(fact.win32_error,Some(error));
        assert_eq!(fact.target,StationBrokerDebt::OwnToken);
    }
}

#[test]
fn a0_token045_elevation_success_requires_exact_returned_four_bytes() {
    for returned in [0u32,1,3,5,8,u32::MAX] {
        let mut calls=0;
        let error=token_buffer_with(TokenElevation,|buffer,capacity| {
            calls+=1;assert_eq!(calls,1);assert!(!buffer.is_null());assert_eq!(capacity,4);
            TokenQueryReply {ok:true,returned,error:24} // stale success last-error is not evidence
        }).unwrap_err();
        assert_eq!(calls,1);assert_eq!(error.kind(),io::ErrorKind::InvalidData);
        assert_eq!(error.raw_os_error(),None);
    }
}

#[test]
fn a0_token045_dynamic_probe_and_fill_remain_strict_bounded_and_error_preserving() {
    for class in [TokenUser,TokenHasRestrictions,TokenPrivileges] {
        for (ok,error,needed,raw) in [(false,24,4,Some(24)),(false,5,4,Some(5)),(false,0,4,Some(0)),
            (false,122,0,None),(false,122,65537,None),(true,122,4,None)] {
            let mut calls=0;
            let failure=token_buffer_with(class,|buffer,capacity| {
                calls+=1;assert_eq!(calls,1);assert!(buffer.is_null());assert_eq!(capacity,0);
                TokenQueryReply {ok,returned:needed,error}
            }).unwrap_err();
            assert_eq!(calls,1);assert_eq!(failure.raw_os_error(),raw);
            if raw.is_none(){assert_eq!(failure.kind(),io::ErrorKind::InvalidData);}
        }
    }
    // Both the existing one-byte restriction shape and exact64KiB upper bound
    // remain valid dynamic buffers. This is not a normalized native subject.
    for (needed,returned) in [(1u32,1u32),(4,1),(4,4),(65536,65536)] {
        let mut calls=0;
        let (words,len)=token_buffer_with(TokenHasRestrictions,|buffer,capacity| {
            calls+=1;
            if calls==1 {assert!(buffer.is_null());assert_eq!(capacity,0);
                TokenQueryReply {ok:false,returned:needed,error:122}}
            else {assert_eq!(calls,2);assert!(!buffer.is_null());assert_eq!(capacity,needed);
                assert_eq!(buffer as usize%mem::align_of::<u64>(),0);
                unsafe{ptr::write(buffer.cast::<u8>(),1);}
                TokenQueryReply {ok:true,returned,error:0}}
        }).unwrap();
        assert_eq!(calls,2);assert_eq!(len,returned as usize);assert_eq!(words[0]&255,1);
        assert_eq!(words.len(),(needed as usize).div_ceil(8));
    }
    for native_error in [5u32,24,122] {
        let mut calls=0;
        let error=token_buffer_with(TokenUser,|buffer,capacity| {
            calls+=1;
            if calls==1 {assert!(buffer.is_null());assert_eq!(capacity,0);
                TokenQueryReply {ok:false,returned:4,error:122}}
            else {assert_eq!(calls,2);assert!(!buffer.is_null());assert_eq!(capacity,4);
                TokenQueryReply {ok:false,returned:4,error:native_error}}
        }).unwrap_err();
        assert_eq!(calls,2);assert_eq!(error.raw_os_error(),Some(native_error as i32));
    }
    for returned in [0u32,5] {
        let mut calls=0;
        let error=token_buffer_with(TokenUser,|buffer,capacity| {
            calls+=1;
            if calls==1 {assert!(buffer.is_null());assert_eq!(capacity,0);
                TokenQueryReply {ok:false,returned:4,error:122}}
            else {assert_eq!(calls,2);assert!(!buffer.is_null());assert_eq!(capacity,4);
                TokenQueryReply {ok:true,returned,error:0}}
        }).unwrap_err();
        assert_eq!(calls,2);assert_eq!(error.kind(),io::ErrorKind::InvalidData);
        assert_eq!(error.raw_os_error(),None);
    }
}
