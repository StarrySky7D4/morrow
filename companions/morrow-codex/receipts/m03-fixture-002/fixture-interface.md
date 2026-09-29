# fixture002 interface additions

CLI, strict fixture-spec.json, identity, event IDs, barriers and release-consumer.json remain as fixture001. Host MUST pin the new manifest and executable.

`host_revoke_received.detail.cancel_transition` is null if no signal was bound; otherwise it contains:
- `cancel_gate_before`, `cancel_gate_after`: booleans for the actual cancellation latch, distinct from pause.
- `first_cancel_reason_before`, `first_cancel_reason_after`: nullable CancelReason strings.
- `delivery_paused_before`: boolean.
- `host_control_closed_gate`: true only when this cancel invocation acquired a previously uncancelled gate and set HostCancelled as the first reason.

The fields are observed within the same gate lock as cancellation. Marker emission remains outside native/gate locks and is not an OS timestamp. A must require a nonnull transition, before=false, after=true, before reason=null, after reason=HostCancelled, host_control_closed_gate=true, plus task.audit.first_cancel_reason=HostCancelled. delivery_paused_before may be either boolean. Keep the existing exact event identity/hash and barrier suppression checks.

Result `data_end_observation` fields:
- `pause_observed`, `control_explained`: booleans.
- `frames_index_at_pause`, `io_index_at_pause`: nullable zero-based array indices, captured as array lengths in the pause State critical section. Entries at/after the indices were recorded after pause.
- `fixed_wait_budget_ns`: nullable integer duration, <=500000000 and capped by original authority remaining time; no new grant.
- `wait_cap_ms`: 500, `authority_deadline_renewed`: false, `delivery_resumed`: false.

Every result.frames item adds `delivery_paused` and nullable `credit_window_bytes` (null for non-Credit). `guest_control_write_admitted` is the final logical admission marker. When pause_observed=true, subsequent admitted frames must exclude HttpCommit and positive HttpCredit; subsequent IO observations must exclude os_issue. Check both overflow flags. Control-first legal ordering need not produce a pause.

For A: transport aggregate/close/final may retain the existing Unknown outcome from host reason19. No Protocol waiver. Request cleanup must be Ok with local_data_thread_joined=true; independent control clean/sticky-null and all actual join requirements remain. Gate closure markers alone cannot establish host causality. This candidate has no actual A/B runtime evidence.

Final settlement: matching replies use the frozen Kind::CreditState (not State). Request cleanup waits within its unchanged 2000ms budget for any required final zero-credit acknowledgement. Early RequestClosed cleanup facts are retained; only the peer offset is updated from later strictly validated matching cumulative facts. Close cannot overtake required settlement. Pre-head/all-zero/no-credit early cancellation needs no credit acknowledgement.
