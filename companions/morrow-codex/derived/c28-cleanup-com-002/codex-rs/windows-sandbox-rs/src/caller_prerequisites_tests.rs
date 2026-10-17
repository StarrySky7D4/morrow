//! Scripted originals and byte buffers only; these methods never call Windows.
use super::*;
use std::cell::Cell;

struct Script {
    base: Instant,
    elapsed: Cell<Duration>,
    log: Vec<&'static str>,
    closes: Vec<HANDLE>,
    token_calls: Vec<(TOKEN_INFORMATION_CLASS, u32)>,
    opens: Vec<(bool, u32)>,
    thread_present: bool,
    thread_error: u32,
    thread_bad_output: bool,
    unavailable_export: bool,
    object_status: i32,
    object_length: u32,
    query_grant: u32,
    purpose_grant: u32,
    restriction_length: u32,
    stats_reads: u32,
    changed_stats: bool,
    fail_elevation: bool,
    panic_elevation: bool,
    late_query_open: bool,
    late_elevation_error: bool,
    fail_first_close: bool,
    privilege_attributes: [Option<u32>; 3],
}
impl Script {
    fn new() -> Self {
        Self { base: Instant::now(), elapsed: Cell::new(Duration::ZERO),
            log: Vec::new(), closes: Vec::new(), token_calls: Vec::new(), opens: Vec::new(),
            thread_present: false, thread_error: 1008, thread_bad_output: false,
            unavailable_export: false, object_status: 0, object_length: 56,
            query_grant: 8, purpose_grant: 6, restriction_length: 1, stats_reads: 0,
            changed_stats: false, fail_elevation: false, panic_elevation: false,
            late_query_open: false, late_elevation_error: false, fail_first_close: false,
            privilege_attributes: [Some(2); 3] }
    }
}
impl Backend for Script {
    fn now(&self) -> Instant { self.base + self.elapsed.get() }
    fn module(&mut self) -> Result<(), u32> { self.log.push("module"); Ok(()) }
    fn export(&mut self) -> Result<(), u32> {
        self.log.push("export"); if self.unavailable_export { Err(127) } else { Ok(()) }
    }
    fn pid(&mut self) -> u32 { self.log.push("pid"); 46 }
    fn creation(&mut self) -> Result<u64, u32> { self.log.push("creation"); Ok(134366721200866222) }
    fn open(&mut self, thread: bool, mask: u32, output: &mut HANDLE) -> WinReply {
        self.opens.push((thread, mask));
        self.log.push("open");
        if thread {
            if self.thread_present { *output = 303; return WinReply { ok: true, error: 0 }; }
            *output = if self.thread_bad_output { 33 } else { 0 };
            return WinReply { ok: false, error: self.thread_error };
        }
        *output = if mask == 8 { 101 } else { 202 };
        if self.late_query_open && mask == 8 { self.elapsed.set(Duration::from_secs(11)); }
        WinReply { ok: true, error: 0 }
    }
    fn object(&mut self, token: HANDLE, output: &mut [u32; 14]) -> ObjectReply {
        self.log.push("object");
        output[0] = 0; // Neither flags nor these counts are a granted-access substitute.
        output[1] = if token == 101 { self.query_grant } else { self.purpose_grant };
        output[2] = 0xffffffff; output[3] = 0xffffffff;
        ObjectReply { status: self.object_status, returned: self.object_length }
    }
    fn privilege(&mut self, index: usize) -> Result<u64, u32> {
        self.log.push("privilege"); Ok(index as u64 + 1)
    }
    fn token(&mut self, _token: HANDLE, class: TOKEN_INFORMATION_CLASS,
        output: *mut c_void, capacity: u32) -> TokenReply {
        self.token_calls.push((class, capacity));
        if class == TokenElevation {
            assert!(!self.panic_elevation, "scripted decoder/native-boundary unwind");
            if self.fail_elevation {
                if self.late_elevation_error { self.elapsed.set(Duration::from_secs(11)); }
                return TokenReply { ok: false, error: 24, returned: 4 };
            }
        }
        let length = match class {
            TokenStatistics => 56,
            TokenUser => 28,
            TokenIntegrityLevel => 28,
            TokenPrivileges => 4 + 12 * self.privilege_attributes.iter().filter(|v| v.is_some()).count() as u32,
            TokenHasRestrictions => self.restriction_length,
            _ => 4,
        };
        if capacity == 0 { return TokenReply { ok: false, error: 122, returned: length }; }
        assert!(capacity >= length);
        let mut bytes = vec![0u8; length as usize];
        match class {
            TokenStatistics => {
                self.stats_reads += 1;
                bytes[..8].copy_from_slice(&42u64.to_le_bytes());
                bytes[8..16].copy_from_slice(&44u64.to_le_bytes());
                bytes[24..28].copy_from_slice(&1u32.to_le_bytes());
                bytes[48..56].copy_from_slice(&(if self.changed_stats && self.stats_reads == 2 { 100u64 } else { 99u64 }).to_le_bytes());
            }
            TokenUser | TokenIntegrityLevel => {
                let start = 16;
                let pointer = output as usize + start;
                bytes[..mem::size_of::<usize>()].copy_from_slice(&pointer.to_le_bytes());
                bytes[start] = 1; bytes[start + 1] = 1;
                bytes[start + 7] = if class == TokenUser { 5 } else { 16 };
                bytes[start + 8..start + 12].copy_from_slice(&8192u32.to_le_bytes());
            }
            TokenPrivileges => {
                let count = self.privilege_attributes.iter().filter(|v| v.is_some()).count() as u32;
                bytes[..4].copy_from_slice(&count.to_le_bytes());
                let mut offset = 4;
                for (i, attributes) in self.privilege_attributes.iter().enumerate() {
                    if let Some(bits) = attributes {
                        bytes[offset..offset + 8].copy_from_slice(&(i as u64 + 1).to_le_bytes());
                        bytes[offset + 8..offset + 12].copy_from_slice(&bits.to_le_bytes());
                        offset += 12;
                    }
                }
            }
            TokenElevationType => bytes[..4].copy_from_slice(&1u32.to_le_bytes()),
            TokenHasRestrictions => bytes[0] = 1,
            _ => {}
        }
        // Only the test's caller-owned bounded storage is touched.
        unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), output.cast::<u8>(), bytes.len()); }
        TokenReply { ok: true, error: 0, returned: length }
    }
    fn close(&mut self, token: HANDLE) -> WinReply {
        self.closes.push(token);
        WinReply { ok: !(self.fail_first_close && self.closes.len() == 1), error: 5 }
    }
}
fn observe(script: &mut Script) -> Box<CallerPrerequisiteAttempt> {
    let mut attempt = CallerPrerequisiteAttempt::new();
    assert!(attempt.reserve());
    attempt.observe_with(script);
    attempt
}

#[test]
fn caller046_default_is_inert_and_delivery_failure_is_not_known_empty() {
    let mut attempt = CallerPrerequisiteAttempt::new();
    assert_eq!(attempt.snapshot().phase, Phase::Unattempted);
    assert_eq!(attempt.snapshot().call_counts, [0; 10]);
    assert!(!attempt.unresolved());
    assert!(!attempt.known_closed());
    attempt.retain_delivery_failure(None);
    assert!(attempt.unresolved());
    assert_eq!(attempt.snapshot().first_error.unwrap().kind, FailureKind::Delivery);
    assert!(!attempt.reserve());
}

#[test]
fn caller046_original_instance_and_reserved_state_reject_replay_before_effects() {
    let mut script = Script::new();
    let mut attempt = CallerPrerequisiteAttempt::new();
    assert!(attempt.reserve());
    attempt.anchor ^= 1; // A different/moved owner cannot inherit this reservation.
    attempt.observe_with(&mut script);
    assert!(script.log.is_empty());
    assert_eq!(attempt.snapshot().first_error.unwrap().kind, FailureKind::Admission);
    let mut original = observe(&mut Script::new());
    let counts = original.snapshot().call_counts;
    original.observe_with(&mut script);
    assert!(script.log.is_empty());
    assert_eq!(original.snapshot().call_counts, counts);
    assert!(original.unresolved()); // Closed originals never make replay acceptable.
}

#[test]
fn caller046_normal_own_scope_has_exact_budget_reverse_close_and_no_authority() {
    let mut script = Script::new();
    let mut attempt = observe(&mut script);
    assert!(attempt.known_closed());
    assert_eq!(attempt.snapshot().call_counts, LIMITS);
    assert_eq!(script.opens, [(true, 8), (false, 8), (false, 6)]);
    assert_eq!(script.closes, [202, 101]);
    assert_eq!(attempt.snapshot().native_pid, Some(46));
    assert_eq!(attempt.snapshot().creation_filetime_100ns, Some(134366721200866222));
    assert_eq!(attempt.snapshot().initial_statistics, attempt.snapshot().final_statistics);
    let mut storage = [0; CALLER_PREREQUISITE_OUTPUT_BYTES];
    let count = attempt.snapshot().write_json(&mut storage).unwrap();
    assert!(count < CALLER_PREREQUISITE_OUTPUT_BYTES);
    let value: serde_json::Value = serde_json::from_slice(&storage[..count]).unwrap();
    assert_eq!(value["scope"], "ORIGINAL_HARNESS_CALLER_PREREQUISITES_ONLY");
    assert_eq!(value["A1_target_token"], "NOT_PROVEN");
    assert_eq!(value["production_qualified"], false);
    assert_eq!(value["atomic_primary_snapshot_proven"], false);
    assert!(value.get("raw").is_none());
    assert!(attempt.snapshot().write_json(&mut [0; 1]).is_err());
    attempt.retain_delivery_failure(None);
    assert!(!attempt.known_closed());
    assert_eq!(script.closes, [202, 101]); // Delivery does not re-close a tombstone.
}

#[test]
fn caller046_requested_opened_and_actual_granted_are_distinct() {
    let mut script = Script::new();
    script.purpose_grant = 2;
    let attempt = observe(&mut script);
    assert_eq!(attempt.snapshot().purpose6.requested_mask, Some(6));
    assert_eq!(attempt.snapshot().purpose6.native_success, Some(true));
    assert_eq!(attempt.snapshot().purpose6.actual_granted_access, Some(2));
    assert_eq!(attempt.snapshot().phase, Phase::KnownIneligible);
    assert!(attempt.known_closed());
    assert_eq!(script.closes, [202, 101]);
    let mut script = Script::new();
    script.query_grant = 2;
    let attempt = observe(&mut script);
    assert_eq!(attempt.snapshot().query8.actual_granted_access, Some(2));
    assert!(attempt.snapshot().initial_statistics.is_none());
    assert_eq!(script.opens, [(true, 8), (false, 8)]);
    assert_eq!(script.closes, [101]);
    assert_eq!(attempt.snapshot().phase, Phase::KnownIneligible);
}

#[test]
fn caller046_export_ntstatus_and_layout_failures_never_fallback_or_close() {
    for mode in 0..3 {
        let mut script = Script::new();
        if mode == 0 { script.unavailable_export = true; }
        if mode == 1 { script.object_status = -1073741823; }
        if mode == 2 { script.object_length = 55; }
        let attempt = observe(&mut script);
        assert!(attempt.unresolved());
        assert!(script.closes.is_empty());
        assert!(attempt.snapshot().query8.actual_granted_access.is_none());
        let first = attempt.snapshot().first_error.unwrap();
        match mode {
            0 => { assert_eq!(first.kind, FailureKind::Unsupported); assert_eq!(first.win32, Some(127)); assert!(script.opens.is_empty()); }
            1 => { assert_eq!(first.ntstatus, Some(-1073741823)); assert_eq!(first.returned, Some(56)); }
            _ => { assert_eq!(first.kind, FailureKind::InvalidBuffer); assert_eq!(first.returned, Some(55)); }
        }
        assert!(script.opens.iter().all(|(thread, mask)| *thread || *mask == 8));
    }
}

#[test]
fn caller046_privilege_attributes_absence_duplicate_and_count_are_not_success() {
    assert!(!privilege_enabled(1)); // ENABLED_BY_DEFAULT alone.
    assert!(!privilege_enabled(6)); // ENABLED plus REMOVED.
    assert!(privilege_enabled(2));
    let mut bytes = vec![0u8; 16];
    bytes[..4].copy_from_slice(&1u32.to_le_bytes());
    bytes[4..12].copy_from_slice(&1u64.to_le_bytes());
    bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
    assert_eq!(parse_privileges(&bytes, [1, 2, 3]), Some([Some(1), None, None]));
    bytes.resize(28, 0);
    bytes[..4].copy_from_slice(&2u32.to_le_bytes());
    bytes[16..24].copy_from_slice(&1u64.to_le_bytes());
    assert!(parse_privileges(&bytes, [1, 2, 3]).is_none());
    bytes[..4].copy_from_slice(&257u32.to_le_bytes());
    assert!(parse_privileges(&bytes, [1, 2, 3]).is_none());
    assert!(parse_privileges(&bytes[..3], [1, 2, 3]).is_none());
    for attributes in [None, Some(0), Some(1), Some(6)] {
        let mut script = Script::new();
        script.privilege_attributes[0] = attributes;
        let attempt = observe(&mut script);
        assert!(attempt.known_closed());
        assert_eq!(attempt.snapshot().phase, Phase::KnownIneligible);
        assert_eq!(attempt.snapshot().privileges[0].present, Some(attributes.is_some()));
        assert_eq!(attempt.snapshot().privileges[0].attributes, attributes);
        assert_eq!(attempt.snapshot().privileges[0].enabled, Some(false));
        assert_eq!(script.closes, [202, 101]);
    }
}

#[test]
fn caller046_sid_bounds_and_changed_statistics_do_not_prove_an_atomic_token() {
    let mut bytes = [0u8; 32];
    let address = bytes.as_ptr() as usize + 16;
    bytes[..mem::size_of::<usize>()].copy_from_slice(&address.to_le_bytes());
    bytes[16] = 1; bytes[17] = 1;
    assert_eq!(token_sid(&bytes, 16).unwrap().len(), 12);
    bytes[17] = 16;
    assert!(token_sid(&bytes, 16).is_none());
    bytes[..mem::size_of::<usize>()].copy_from_slice(&usize::MAX.to_le_bytes());
    assert!(token_sid(&bytes, 16).is_none());
    assert!(parse_statistics(&[0; 55]).is_none());
    let mut script = Script::new();
    script.changed_stats = true;
    let attempt = observe(&mut script);
    assert_eq!(attempt.snapshot().first_error.unwrap().kind, FailureKind::Inconsistent);
    assert_ne!(attempt.snapshot().initial_statistics, attempt.snapshot().final_statistics);
    assert_eq!(attempt.slots[QUERY].raw, 101);
    assert_eq!(attempt.slots[PURPOSE].raw, 202);
    assert!(script.closes.is_empty());
}

#[test]
fn caller046_thread_no_token_presence_denial_and_uncertain_output_stay_distinct() {
    let mut script = Script::new();
    script.thread_present = true;
    let attempt = observe(&mut script);
    assert!(attempt.known_closed());
    assert_eq!(attempt.snapshot().phase, Phase::KnownIneligible);
    assert_eq!(attempt.snapshot().no_thread_token, Some(false));
    assert_eq!(script.closes, [303]);
    assert_eq!(script.opens, [(true, 8)]);
    for (error, bad) in [(5, false), (1008, true)] {
        let mut script = Script::new();
        script.thread_error = error; script.thread_bad_output = bad;
        let attempt = observe(&mut script);
        assert!(attempt.unresolved());
        assert!(attempt.snapshot().no_thread_token.is_none());
        assert_eq!(attempt.snapshot().thread.open_error, Some(error));
        assert_eq!(script.opens, [(true, 8)]);
        assert!(script.closes.is_empty());
        if bad { assert_eq!(attempt.slots[THREAD].raw, 33); assert_eq!(attempt.snapshot().thread.closure, Closure::Uncertain); }
    }
}

#[test]
fn caller046_close_failure_retains_both_originals_and_never_closes_later_slot() {
    let mut script = Script::new();
    script.fail_first_close = true;
    let mut attempt = observe(&mut script);
    assert_eq!(script.closes, [202]);
    assert_eq!(attempt.slots[PURPOSE].raw, 202);
    assert_eq!(attempt.slots[QUERY].raw, 101);
    assert_eq!(attempt.snapshot().purpose6.closure, Closure::CloseFailed);
    assert_eq!(attempt.snapshot().query8.closure, Closure::Held);
    assert_eq!(attempt.snapshot().first_error.unwrap().win32, Some(5));
    attempt.observe_with(&mut script);
    assert_eq!(script.closes, [202]);
    assert!(attempt.unresolved());
}

#[test]
fn caller046_unwind_and_later_delivery_preserve_first_error_and_original_anchor() {
    let mut script = Script::new();
    script.panic_elevation = true;
    let mut attempt = observe(&mut script);
    let anchor = attempt.anchor;
    let first = attempt.snapshot().first_error.unwrap();
    assert_eq!(first.stage, Stage::Elevation);
    assert_eq!(first.kind, FailureKind::Unwind);
    assert_eq!(attempt.slots[QUERY].raw, 101);
    assert!(script.closes.is_empty());
    attempt.retain_delivery_failure(None);
    assert_eq!(attempt.anchor, anchor);
    assert_eq!(attempt.snapshot().first_error, Some(first));
    assert!(attempt.unresolved());
}

#[test]
fn caller046_fixed_elevation_and_restrictions_one_or_four_never_probe_or_retry() {
    for length in [1, 4] {
        let mut script = Script::new();
        script.restriction_length = length;
        let attempt = observe(&mut script);
        assert!(attempt.known_closed());
        assert_eq!(attempt.snapshot().has_restrictions, Some(true));
        assert_eq!(script.token_calls.iter().filter(|(class, _)| *class == TokenElevation).copied().collect::<Vec<_>>(), [(TokenElevation, 4)]);
        assert_eq!(script.token_calls.iter().filter(|(class, _)| *class == TokenHasRestrictions).copied().collect::<Vec<_>>(), [(TokenHasRestrictions, 4)]);
    }
    assert_eq!(restrictions(&[1, 0, 0, 0]), Some(true));
    assert_eq!(restrictions(&[0]), Some(false));
    assert_eq!(restrictions(&[0, 0, 0]), None);
    assert_eq!(restrictions(&[1, 1, 0, 0]), None);
    assert_eq!(restrictions(&[2]), None);
    let mut script = Script::new();
    script.fail_elevation = true;
    let attempt = observe(&mut script);
    assert_eq!(attempt.snapshot().first_error.unwrap().win32, Some(24));
    assert_eq!(script.token_calls.iter().filter(|(class, _)| *class == TokenElevation).count(), 1);
    assert!(script.closes.is_empty());
}

#[test]
fn caller046_after_call_deadline_keeps_captured_handle_and_native_first_error() {
    let expired = Script::new();
    let mut pending = CallerPrerequisiteAttempt::new();
    assert!(pending.reserve());
    pending.deadline = Some(expired.base);
    assert_eq!(pending.before(&expired, 0, Stage::Module).unwrap_err().kind, FailureKind::Deadline);
    assert_eq!(pending.snapshot().call_counts, [0; 10]);
    assert!(expired.log.is_empty());
    let mut script = Script::new();
    script.late_query_open = true;
    let attempt = observe(&mut script);
    assert_eq!(attempt.slots[QUERY].raw, 101);
    assert_eq!(attempt.snapshot().query8.native_success, Some(true));
    assert_eq!(attempt.snapshot().query8.closure, Closure::Held);
    assert_eq!(attempt.snapshot().first_error.unwrap().kind, FailureKind::Deadline);
    assert_eq!(attempt.snapshot().call_counts[6], 0);
    assert!(script.closes.is_empty());
    let mut script = Script::new();
    script.fail_elevation = true; script.late_elevation_error = true;
    let attempt = observe(&mut script);
    assert_eq!(attempt.snapshot().first_error.unwrap().kind, FailureKind::Win32);
    assert_eq!(attempt.snapshot().first_error.unwrap().win32, Some(24));
    assert!(script.closes.is_empty());
}
