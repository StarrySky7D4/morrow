"""Prepare bounded multi-round evidence collection without weakening old gates."""
from pathlib import Path
import re
from derive_p2_source_policy_controller_039 import change
from derive_parent_session_controller_042 import patch_controller as derive,SYMBOL,FLAG,STATUS
from derive_p2_source_policy_capture_039 import patch_capture as single_capture,QA as SINGLE_QA

BASE=Path(__file__).parent
CASE_STATUS='P2_SEVENTEEN_SHARED_REFUSALS_P_NATIVE_IO_CLOSED_ROOT_REVIEW_PENDING'
SCOPE='ONE_FIXED_SEVENTEEN_SHARED_REFUSAL_SESSION_AND_OWNED_POLICY_RESTORE_CLOSED_ROOT_REVIEW_PENDING'

def patch_controller(src,root):
    derive(src,root)
    return 60469

def patch_capture(raw):
    raw=single_capture(raw)
    return raw.replace(b'P2_SOURCE_POLICY_REFUSAL_QUALIFICATION',SYMBOL.encode()).replace(b'--run-p2-source-policy-refusal',FLAG.encode())
QA=SINGLE_QA.replace('--run-p2-source-policy-refusal',FLAG)

def patch_guest(raw,wire):
    assert wire==60469
    source=raw.decode('utf-8-sig')
    start=source.index('function CheckTerminal([byte[]]$raw){')
    end=source.index("$s.DiagnosticStage='ORIGINAL_STATE_LOOKUP'",start)
    old=source[start:end]
    lines=old.splitlines()
    commands=next(x for x in lines if ' $commands=@(' in x)
    commands=commands.replace("'sealed-native-start','sealed-basic-observe','sealed-native-join'", "'sealed-qualify-source-policy-refusal','sealed-native-start','sealed-refusal-join'")
    ordered=next(x for x in lines if ' $prior=-1;' in x).replace('$t.steps','$round.steps')
    frame=next(x for x in lines if ' $bytes=0;for' in x).replace('$n.frames','$round.frames').replace('$f.ordinal-eq $i+1','$f.ordinal-eq (($ordinal-1)*92+$i+1)').replace('3523','3557')
    frame=frame.replace('$f.native_returned-eq $true',"$f.state-ceq 'Delivered'-and $f.native_returned-eq $true")
    frame=frame.replace('};N ($bytes-eq 3557)',"};N ($round.input_bytes-eq 3557-and $bytes-eq 3557)")
    round_function='''function CheckRound042($round,[int]$ordinal,$helper){
 N ($ordinal-ge 1-and $ordinal-le 17-and $round.schema-ceq 'ORIGINAL_SHARED_PARENT_ROUND042_V1'-and $round.attempt-eq $ordinal-and @($round.steps).Count-eq 47-and @($round.frames).Count-eq 92) 'EXACT_FIXED_ROUND_SHAPE'
 N ($round.first_line-eq (($ordinal-1)*92+1)-and $round.last_line-eq ($ordinal*92)-and $round.same_original_parent_pid-eq $helper.pid-and $round.same_original_parent_creation-eq $helper.process_creation_filetime-and -not $round.parent_lifetime_closed-and -not $round.source_accepted-and -not $round.release_eligible) 'SAME_ORIGINAL_PARENT_PER_ROUND'
'''+commands+'\n'+ordered+'\n'+frame+'''
 N ($round.original_attempt_closed_sequence-gt $prior-and $round.original_attempt_closed_sequence-lt 256) 'ACTUAL_CLOSED_ATTEMPT_AFTER_EXIT_RETURN'
 if($ordinal-eq 17){N ($round.original_common_owner_closed_sequence-gt $round.original_attempt_closed_sequence-and $round.original_common_owner_closed_sequence-lt 256) 'ORIGINAL_FINAL_COMMON_OWNER_CLOSED'}else{N ($round.original_common_owner_closed_sequence-eq -1) 'COMMON_OWNER_RETAINED_BETWEEN_ATTEMPTS'}
 N ($round.prompt_native_write_baseline-ge 0-and $round.prompt_native_write_baseline-le 2097152-and $round.journal_records-ge 1-and $round.journal_records-le 256) 'BOUNDED_ORIGINAL_PROMPT_AND_JOURNAL'
}
'''
    replacement=old
    header=next(x for x in lines if " 'ACTUAL_ROOT47_TERMINAL'" in x)
    replacement=replacement.replace(header," N ($t.status-ceq '"+STATUS+"'-and $t.attempts-eq 17-and $t.steps_total-eq 799-and $t.delivered-eq 1564-and $t.input_bytes-eq 60469-and @($t.rounds).Count-eq 17-and $t.self_close_witness-ceq 'NOT_IN_THIS_PRE_CLOSE_FILE_ROOT_MUST_VERIFY_ORIGINAL_CONTROLLER_EXIT0') 'ACTUAL_FIXED_SHARED_PARENT_TERMINAL'")
    replacement=replacement.replace(next(x for x in lines if ' $commands=@(' in x)+'\n','')
    replacement=replacement.replace(next(x for x in lines if ' $prior=-1;' in x)+'\n','')
    closure=next(x for x in lines if " 'ACTUAL_P_IO_INPUT_CLOSURE'" in x)
    replacement=replacement.replace(closure,closure.replace('$n.delivered-eq 92-and $n.native_returns-eq 92-and $n.delivered_bytes-eq 3523-and @($n.frames).Count-eq 92',
        '$n.delivered-eq 1564-and $n.reserved-eq 1564-and $n.write_attempts-eq 1564-and $n.native_returns-eq 1564-and $n.delivered_bytes-eq 60469-and $n.requested_bytes-eq 60469-and $n.reported_successful_bytes-eq 60469-and @($n.round_reports).Count-eq 17'))
    replacement=replacement.replace(next(x for x in lines if ' $bytes=0;for' in x)+'\n','')
    bound=next(x for x in lines if " 'ORIGINAL_BOUNDED_COMPLETE_STREAM_AND_JOURNAL'" in x)
    replacement=replacement.replace(bound," N ($t.stdout_bytes-ge 0-and $t.stdout_bytes-le 2097152-and $t.stderr_bytes-ge 0-and $t.stderr_bytes-le 2097152) 'ORIGINAL_BOUNDED_COMPLETE_STREAMS'")
    replacement=replacement.replace(' return $t\n',r''' for($ordinal=1;$ordinal-le 17;$ordinal++){
  $summary=$t.rounds[$ordinal-1];$inputSummary=$n.round_reports[$ordinal-1];$label=$ordinal.ToString('D2')
  $reportPath='C:\Morrow-C28-Root47-1725-run-'+$Tag+'\attempt-'+$label+'.json'
  $journalPath='C:\Morrow-C28-A1-047-'+$Tag+'\case-attempt-'+$label+'.evidence\journal.jsonl'
  N ($summary.attempt-eq $ordinal-and $summary.path-ceq $reportPath-and $summary.bytes-ge 1-and $summary.bytes-le 131072-and $summary.sha256-cmatch '^[0-9a-f]{64}$'-and $summary.journal_path-ceq $journalPath-and $summary.journal_bytes-ge 1-and $summary.journal_bytes-le 33554432-and $summary.journal_sha256-cmatch '^[0-9a-f]{64}$') 'EXACT_ROUND_REPORT_AND_JOURNAL_PATHS'
  N ($inputSummary.attempt-eq $ordinal-and $inputSummary.path-ceq $summary.path-and $inputSummary.bytes-eq $summary.bytes-and $inputSummary.sha256-ceq $summary.sha256-and $inputSummary.first_line-eq (($ordinal-1)*92+1)-and $inputSummary.last_line-eq ($ordinal*92)) 'ORIGINAL_INPUT_HISTORY_BOUND_TO_ROUND_REPORT'
  $roundRaw=FileBytes $reportPath 131072
  N ($roundRaw.Length-eq $summary.bytes-and (ShaGuest1737 $roundRaw)-ceq $summary.sha256) 'ROUND_ORIGINAL_BYTES_HASH'
  $round=J $roundRaw;CheckRound042 $round $ordinal $h
  N ($round.journal_path-ceq $journalPath-and $round.journal_bytes-eq $summary.journal_bytes-and $round.journal_sha256-ceq $summary.journal_sha256-and $round.journal_records-eq $summary.journal_records-and $round.original_attempt_closed_sequence-eq $summary.original_attempt_closed_sequence-and $round.original_common_owner_closed_sequence-eq $summary.original_common_owner_closed_sequence) 'ROUND_ORIGINAL_JOURNAL_BINDING'
  [void](Hold $journalPath $summary.journal_bytes $summary.journal_sha256)
 }
 return $t
''')
    source=source[:start]+round_function+replacement+source[end:]
    source=source.replace("'--run-1725 '","'"+FLAG+" '")
    source=source.replace("   $journalPath='C:\\Morrow-C28-A1-047-'+$Tag+'\\case.evidence\\journal.jsonl'\n   [void](Hold $journalPath $t.journal_bytes $t.journal_sha256)\n",'')
    source=source.replace('journal_path=$journalPath;journal_bytes=$t.journal_bytes;journal_sha256=$t.journal_sha256;', 'rounds=$t.rounds;')
    source=source.replace('ORIGINAL_CLI0_47_92_3523_P_NATIVE_IO_CLOSED_NINE_GATES_PENDING_ROOT',CASE_STATUS)
    source=source.replace("full_nine_gate_qualification='ROOT_REVIEW_REQUIRED'","full_nine_gate_qualification='NOT_THIS_FIXED_SHARED_PARENT_PROFILE'")
    assert 'case.evidence' not in source and '3523' not in source and '$t.steps' not in source.replace('$t.steps_total','')
    return source.encode('utf-8')

def patch_invoke(raw):
    raw=change(raw,'ORIGINAL_CLI0_47_92_3523_P_NATIVE_IO_CLOSED_NINE_GATES_PENDING_ROOT',CASE_STATUS)
    raw=change(raw,'ONE_ORIGINAL47_CASE_AND_OWNED_POLICY_RESTORE_CLOSED_ROOT_NINE_GATE_REVIEW_PENDING',SCOPE)
    return change(raw,"full_nine_gate_qualification='ROOT_REVIEW_REQUIRED'","full_nine_gate_qualification='NOT_THIS_FIXED_SHARED_PARENT_PROFILE'")
